"""Train a small visual-connector residual; base weights stay frozen.
Training is explicit, local, group-split and staged. Inference remains native Rust.
"""
from __future__ import annotations
import argparse, hashlib, json, math, os, random, time
from pathlib import Path
from supervision_contract import validate_supervision
import torch
from torch import nn
from transformers import AutoModelForImageTextToText, AutoTokenizer
QUESTION='What is visible? Reply with exactly one letter: A code editor, B text document, C photograph or video, D artwork or drawing canvas, E video game, F spreadsheet or chart, G empty desktop, H web page, I other or uncertain. Answer:'
REVISION='95c283d4497a56477a83177079fa6b7121abb1b1'
class Residual(nn.Module):
    def __init__(self,rank:int=8,limit:float=.05):
        super().__init__();self.down=nn.Linear(1024,rank,bias=False);self.up=nn.Linear(rank,1024,bias=False);self.limit=limit
        nn.init.normal_(self.down.weight,std=.01);nn.init.zeros_(self.up.weight)
    def forward(self,x):
        x=x.float();delta=self.up(self.down(x));scale=(self.limit*x.norm(dim=-1,keepdim=True)/(delta.norm(dim=-1,keepdim=True)+1e-8)).clamp(max=1.)
        return x+delta*scale

def main():
    ap=argparse.ArgumentParser(__doc__);ap.add_argument('--model',type=Path,required=True);ap.add_argument('--dataset',type=Path,required=True);ap.add_argument('--output',type=Path,required=True);ap.add_argument('--steps',type=int,default=96);ap.add_argument('--rank',type=int,default=8);ap.add_argument('--initial-adapter',type=Path)
    a=ap.parse_args();assert 8<=a.steps<=512 and 1<=a.rank<=16
    if a.output.exists():raise RuntimeError('Use a new candidate output directory; never overwrite the active adapter')
    a.output.mkdir(parents=True);random.seed(69);torch.manual_seed(69);torch.set_num_threads(2)
    if not torch.cuda.is_available():raise RuntimeError('CUDA training device is not available; base inference remains unchanged')
    device='cuda';dtype=torch.bfloat16
    raw=json.loads(a.dataset.read_text(encoding='utf-8'));rows=raw['rows'];groups={s:set() for s in ['train','validation','test']}
    supervision_audit=validate_supervision(rows)
    data=[]
    for row in rows:
        if row.get('confirmed') is not True or row.get('label') not in 'ABCDEFGHI' or len(row.get('label',''))!=1:raise ValueError('Only explicit verified labels are training evidence')
        split=row['split'];group=row['group'];assert split in groups;groups[split].add(group)
        f=json.loads(Path(row['features']).read_text(encoding='utf-8'));n=f['image_tokens']
        if f['model_revision']!=REVISION or not 1<=n<=256 or len(f['features'])!=n*1024:raise ValueError('Wrong feature revision or shape')
        x=torch.tensor(f['features'],dtype=torch.float32).view(n,1024)
        if not torch.isfinite(x).all() or x.abs().max()>1000:raise ValueError('Invalid feature tensor')
        data.append({**row,'x':x,'n':n})
    assert groups['train'].isdisjoint(groups['validation']) and groups['train'].isdisjoint(groups['test']) and groups['validation'].isdisjoint(groups['test']), 'Group leakage'
    train=[r for r in data if r['split']=='train'];val=[r for r in data if r['split']=='validation'];test=[r for r in data if r['split']=='test']
    if len(train)<8 or len(val)<8:raise ValueError('Need at least eight training and eight separately confirmed validation examples')
    print('SUPERVISION',len(train),len(val),len(test),flush=True)
    m=AutoModelForImageTextToText.from_pretrained(a.model,dtype=dtype,local_files_only=True,attn_implementation='eager')
    for p in m.parameters():p.requires_grad_(False)
    m.eval();lm=m.model.language_model.to(device);head=m.lm_head.to(device)
    tokenizer=AutoTokenizer.from_pretrained(a.model,local_files_only=True)
    label_ids=torch.tensor([tokenizer.encode(c,add_special_tokens=False)[0] for c in 'ABCDEFGHI'],device=device)
    adapter=Residual(a.rank).to(device)
    if a.initial_adapter:
        initial=json.loads(a.initial_adapter.read_text(encoding='utf-8'))
        if initial['model_revision']!=REVISION or initial['rank']!=a.rank:raise ValueError('Initial adapter is incompatible')
        with torch.no_grad():
            adapter.down.weight.copy_(torch.tensor(initial['down'],device=device).reshape(a.rank,1024))
            adapter.up.weight.copy_(torch.tensor(initial['up'],device=device).reshape(1024,a.rank))
        if not all(torch.isfinite(p).all() and p.abs().max()<=2 for p in adapter.parameters()):raise ValueError('Invalid initial adapter weights')
    prepared={}
    def forward(row,adapt):
        n=row['n'];key=n
        if key not in prepared:
            prompt='<|startoftext|><|im_start|>user\n<|image_start|>'+'<image>'*n+'<|image_end|>'+QUESTION+'<|im_end|>\n<|im_start|>assistant\n'
            ids=torch.tensor([tokenizer.encode(prompt,add_special_tokens=False)],device=device)
            assert ids.shape[1]<=512 and (ids==396).sum()==n
            with torch.no_grad():base=lm.embed_tokens(ids).detach()
            prepared[key]=(ids,base)
        ids,base=prepared[key];x=row['x'].to(device)
        if adapt:x=adapter(x)
        inputs=base.clone();inputs[:,ids[0]==396,:]=x.to(dtype)
        h=lm(inputs_embeds=inputs,attention_mask=torch.ones_like(ids),use_cache=False,return_dict=True).last_hidden_state
        return head(h[:,-1,:]).float()
    @torch.no_grad()
    def evaluate(samples,adapt):
        result=[];loss=0.;correct=0
        for row in samples:
            logits=forward(row,adapt);target=tokenizer.encode(row['label'],add_special_tokens=False)[0]
            loss+=float(nn.functional.cross_entropy(logits,torch.tensor([target],device=device)))
            pred=int(logits[0,label_ids].argmax());correct+=int('ABCDEFGHI'[pred]==row['label'])
            result.append({'group':row['group'],'label':row['label'],'predicted':'ABCDEFGHI'[pred]})
        return {'nll':loss/max(1,len(samples)),'accuracy':correct/max(1,len(samples)),'rows':result}
    baseline=evaluate(val,bool(a.initial_adapter));baseline_test=evaluate(test,bool(a.initial_adapter)) if test else None
    optimizer=torch.optim.AdamW(adapter.parameters(),lr=.003,weight_decay=.02)
    losses=[];started=time.monotonic()
    for step in range(a.steps):
        optimizer.zero_grad(set_to_none=True);total=0.
        for offset in range(4):
            row=train[(step*4+offset)%len(train)];logits=forward(row,True)
            target=tokenizer.encode(row['label'],add_special_tokens=False)[0]
            loss=nn.functional.cross_entropy(logits,torch.tensor([target],device=device))/4
            loss.backward();total+=float(loss.detach())
        torch.nn.utils.clip_grad_norm_(adapter.parameters(),1.0);optimizer.step()
        with torch.no_grad():
            for p in adapter.parameters():p.clamp_(-2.,2.)
        losses.append(total)
        if step%16==0:print('STEP',step,'LOSS',round(total,5),flush=True)
    candidate=evaluate(val,True);candidate_test=evaluate(test,True) if test else None
    approved=candidate['nll']<=baseline['nll'] and candidate['accuracy']>=baseline['accuracy']
    report={'training_examples':len(train),'validation_examples':len(val),'test_examples':len(test),'baseline':baseline,'candidate':candidate,'baseline_test':baseline_test,'candidate_test':candidate_test,'approved_pytorch_validation':approved,'native_validation_required':True,'steps':a.steps,'rank':a.rank,'trainable_parameters':sum(p.numel() for p in adapter.parameters()),'peak_cuda_memory_mb':torch.cuda.max_memory_allocated()/2**20,'seconds':time.monotonic()-started,'loss_first':losses[0],'loss_last':losses[-1],'source_scope':raw.get('scope','explicit user feedback')}
    validation={'training_examples':len(train),'validation_examples':len(val),'baseline_nll':baseline['nll'],'candidate_nll':candidate['nll'],'baseline_accuracy':baseline['accuracy'],'candidate_accuracy':candidate['accuracy'],'approved':approved}
    payload={'schema_version':1,'model_revision':REVISION,'rank':a.rank,'down':adapter.down.weight.detach().cpu().flatten().tolist(),'up':adapter.up.weight.detach().cpu().flatten().tolist(),'max_relative_change':adapter.limit,'validation':validation}
    (a.output/'adapter.json').write_text(json.dumps(payload),encoding='utf-8')
    report['supervision_audit']=supervision_audit
    report['adapter_sha256']=hashlib.sha256((a.output/'adapter.json').read_bytes()).hexdigest()
    (a.output/'training-report.json').write_text(json.dumps(report,indent=2),encoding='utf-8');print(json.dumps({k:v for k,v in report.items() if k not in ['baseline','candidate','baseline_test','candidate_test']}),flush=True)
    # Nothing is copied into the live pet's profile by this trainer.
    if not approved:raise SystemExit('Candidate failed held-out validation; base model is unchanged')
if __name__=='__main__':main()
