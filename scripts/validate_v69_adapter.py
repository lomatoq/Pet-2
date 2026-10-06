"""Check the candidate against the actual quantized native runtime, no live capture."""
from pathlib import Path
import json,subprocess,sys,time,hashlib
R=Path(__file__).resolve().parents[1];D=R/'reports/v69';p=json.loads((D/'preparation.json').read_text())
package=Path(p['previous_release']);exe=D/'validated-release/vision_probe.exe'
model=package/'models/lfm2.5-vl-450m';dll=package/'runtime/onnxruntime.dll'
candidate=D/'vision-candidate-02/adapter.json'
dataset=json.loads((D/'unique-dataset.json').read_text());rows=[r for r in dataset['rows'] if r['split']!='train']
from supervision_contract import validate_supervision
split_audit=validate_supervision(dataset['rows'])
results={}
for name,options in [('base',[]),('candidate',['--adapter',str(candidate)])]:
 cmd=[str(exe),str(model),str(dll),*options,*[r['image'] for r in rows]];start=time.monotonic()
 process=subprocess.run(cmd,capture_output=True,text=True,encoding='utf-8',errors='replace',timeout=180)
 (D/f'native-{name}.log').write_text(process.stdout+'\n'+process.stderr,encoding='utf-8')
 if process.returncode:raise RuntimeError(f'{name} native failed: {process.stderr}')
 predictions=[json.loads(x) for x in process.stdout.splitlines() if x.startswith('{')];assert len(predictions)==len(rows)
 by_image={x['image']:x for x in predictions};metrics={}
 for split in ['validation','test']:
  subset=[r for r in rows if r['split']==split]
  correct=sum(by_image[r['image']]['prediction']['kind']==r['kind'] for r in subset)
  accepted=sum(by_image[r['image']]['accepted'] for r in subset)
  false_accept=sum(by_image[r['image']]['accepted'] and by_image[r['image']]['prediction']['kind']!=r['kind'] for r in subset)
  metrics[split]={'examples':len(subset),'argmax_correct':correct,'accepted':accepted,'wrong_accepted':false_accept}
 results[name]={'metrics':metrics,'seconds':time.monotonic()-start,'predictions':predictions}
 print(name,metrics,flush=True)
# Content-disjoint split; shared authored templates still limit generalization.
approved=all(results['candidate']['metrics'][s]['argmax_correct']>=results['base']['metrics'][s]['argmax_correct'] and results['candidate']['metrics'][s]['wrong_accepted']<=results['base']['metrics'][s]['wrong_accepted'] for s in ['validation','test'])
results.update(approved_for_packaging=approved,adapter_sha256=hashlib.sha256(candidate.read_bytes()).hexdigest(),scope='10 validation and 11 test authored fixtures, zero identical tensors across splits; shared templates, not a real-desktop benchmark')
results['split_audit']=split_audit
(D/'native-adapter-validation.json').write_text(json.dumps(results,indent=2),encoding='utf-8')
print('NATIVE_ADAPTER_APPROVED',approved,flush=True)
sys.exit(0 if approved else 1)
