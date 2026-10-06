"""Explicit local training from confirmed features, with separate native checks.
Creates a candidate only. Promotion is a separate deliberate Console action.
"""
from pathlib import Path
import argparse,hashlib,json,os,subprocess,sys,time,datetime,shutil
LABEL_KIND=dict(zip('ABCDEFGHI',['code','document','media','artwork','game','chart','desktop','web','unknown']))
def atomic(path,value):
 temp=path.with_suffix('.tmp');temp.write_text(json.dumps(value,indent=2),encoding='utf-8');os.replace(temp,path)
def dataset(profile,out):
 rows=[];seen={};directory=profile/'vision-teaching'
 if not directory.is_dir():raise ValueError('No confirmed teaching examples yet')
 samples=[]
 for p in sorted(directory.glob('sample-*.json')):
  if p.stat().st_size>4_000_000:raise ValueError('Oversized teaching example')
  x=json.loads(p.read_text(encoding='utf-8'));label=x.get('label');f=x.get('features',{})
  if x.get('confirmed') is not True or label not in LABEL_KIND:continue
  fingerprint=hashlib.sha256(json.dumps(f,sort_keys=True).encode()).hexdigest()
  if fingerprint in seen:
   samples[seen[fingerprint]]=(x,p)  # Latest explicit correction supersedes the old label.
   continue
  seen[fingerprint]=len(samples);samples.append((x,p))
 groups=sorted({x['group'] for x,_ in samples},key=lambda s:hashlib.sha256(s.encode()).hexdigest())
 if len(groups)<3:raise ValueError('Need confirmed examples from at least three independent observation contexts')
 validation=set(groups[::3]);features=out/'features';features.mkdir()
 for i,(x,source) in enumerate(samples[:96]):
  p=features/f'example-{i:03}.json';p.write_text(json.dumps(x['features']),encoding='utf-8')
  rows.append({'features':str(p),'label':x['label'],'kind':LABEL_KIND[x['label']],'split':'validation' if x['group'] in validation else 'train','group':x['group'],'confirmed':True,'source':'explicitly retained confirmed example'})
 if sum(x['split']=='train' for x in rows)<8 or sum(x['split']=='validation' for x in rows)<8:raise ValueError('Need at least 8 training and 8 held-out examples, split by observation context rather than neighboring frames')
 p=out/'dataset.json';atomic(p,{'schema_version':1,'rows':rows,'scope':'explicit private teaching features; no screenshot or automatic model labels'});return p,rows

def native(probe,model,dll,rows,adapter=None):
 cmd=[str(probe),str(model),str(dll)]+(['--adapter',str(adapter)] if adapter else [])+[r['features'] for r in rows]
 p=subprocess.run(cmd,capture_output=True,text=True,encoding='utf-8',errors='replace',timeout=max(60,len(rows)*12))
 if p.returncode:raise RuntimeError('Native candidate check failed: '+p.stderr[-2000:])
 predictions=[json.loads(s) for s in p.stdout.splitlines() if s.startswith('{')]
 if len(predictions)!=len(rows):raise RuntimeError('Missing native validation outcomes')
 return {'examples':len(rows),'correct':sum(x['prediction']['kind']==r['kind'] for x,r in zip(predictions,rows)),
         'accepted_correct':sum(x['accepted'] and x['prediction']['kind']==r['kind'] for x,r in zip(predictions,rows)),
         'wrong_accepted':sum(x['accepted'] and x['prediction']['kind']!=r['kind'] for x,r in zip(predictions,rows))}

def main():
 ap=argparse.ArgumentParser(__doc__);ap.add_argument('--profile',type=Path,required=True);ap.add_argument('--model',type=Path,required=True);ap.add_argument('--package',type=Path,required=True)
 a=ap.parse_args();root=a.profile.resolve();status=root/'vision-training-status.json';stage=root/'vision-training';stage.mkdir(exist_ok=True)
 lock=stage/'training.lock'
 try:fd=os.open(lock,os.O_CREAT|os.O_EXCL|os.O_WRONLY)
 except FileExistsError:raise SystemExit('A training lock already exists; review the current job before starting another')
 os.write(fd,str(os.getpid()).encode());os.close(fd)
 out=stage/datetime.datetime.now().strftime('%Y%m%d-%H%M%S');out.mkdir()
 try:
  atomic(status,{'state':'checking_examples','pid':os.getpid(),'message':'Preparing context-disjoint confirmed examples'})
  manifest,rows=dataset(root,out)
  if not (a.model/'config.json').exists():raise ValueError('The local training checkpoint is missing; inference base has not been changed')
  atomic(status,{'state':'training','pid':os.getpid(),'examples':len(rows),'message':'Training bounded visual connector; immutable base unchanged'})
  trainer=Path(__file__).with_name('train_vision_connector.py');candidate=out/'candidate'
  active=root/'vision-adapter.json'
  if not active.exists():active=a.package/'models/vision-adapter.json'
  initial=['--initial-adapter',str(active)] if active.exists() else []
  env=os.environ.copy();env.update(HF_HUB_OFFLINE='1',TRANSFORMERS_OFFLINE='1',HF_HUB_DISABLE_TELEMETRY='1')
  with (out/'training.log').open('w',encoding='utf-8') as log:
   p=subprocess.run([sys.executable,str(trainer),'--model',str(a.model),'--dataset',str(manifest),'--output',str(candidate),'--steps','64','--rank','8',*initial],env=env,stdout=log,stderr=subprocess.STDOUT)
  if p.returncode:raise RuntimeError('Candidate did not pass training validation; see '+str(out/'training.log'))
  atomic(status,{'state':'validating_native','pid':os.getpid(),'message':'Checking actual quantized runtime and retention examples'})
  probe=a.package/'vision_feature_probe.exe';model=a.package/'models/lfm2.5-vl-450m';dll=a.package/'runtime/onnxruntime.dll'
  held=[r for r in rows if r['split']=='validation'];adapter=candidate/'adapter.json'
  baseline=native(probe,model,dll,held,active if active.exists() else None);trained=native(probe,model,dll,held,adapter)
  retention_manifest=a.package/'training/retention/dataset.json'
  if not retention_manifest.exists():raise ValueError('Retention evaluation set is missing; candidate cannot be promoted')
  retention=json.loads(retention_manifest.read_text(encoding='utf-8'))['rows']
  for r in retention:r['features']=str(retention_manifest.parent/r['features'])
  base_ret=native(probe,model,dll,retention,active if active.exists() else None);new_ret=native(probe,model,dll,retention,adapter)
  accepted=trained['accepted_correct']>=baseline['accepted_correct'] and new_ret['accepted_correct']>=base_ret['accepted_correct'] and trained['correct']>=baseline['correct'] and trained['wrong_accepted']<=baseline['wrong_accepted'] and new_ret['correct']>=base_ret['correct'] and new_ret['wrong_accepted']<=base_ret['wrong_accepted']
  report={'approved':accepted,'baseline':baseline,'candidate':trained,'retention_base':base_ret,'retention_candidate':new_ret,'candidate_path':str(adapter.relative_to(stage)),'sha256':hashlib.sha256(adapter.read_bytes()).hexdigest(),'automatic_promotion':False,'created_unix_ms':int(time.time()*1000)}
  atomic(out/'native-validation.json',report)
  if not accepted:raise RuntimeError('Native or retention results degraded; candidate is rejected, active model unchanged')
  atomic(stage/'candidate.json',report)
  atomic(status,{'state':'ready_for_promotion','pid':None,'message':'Validated candidate ready. Promote explicitly in the Console; nothing has overwritten the active weights.','report':report})
 except Exception as e:
  atomic(status,{'state':'not_promoted','pid':None,'message':str(e),'active_model_changed':False});raise
 finally:
  lock.unlink(missing_ok=True)
if __name__=='__main__':main()
