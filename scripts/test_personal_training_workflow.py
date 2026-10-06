"""Exercise the real personal-training pipeline using authored synthetic fixtures.
Never reads or writes the live pet profile and never claims human supervision.
"""
from pathlib import Path
import json,shutil,subprocess,time
R=Path(__file__).resolve().parents[1];D=R/'reports/v69';base=Path(json.loads((D/'preparation.json').read_text())['previous_release'])
root=D/'synthetic-teaching-profile';package=D/'training-fixture-package'
if root.exists() or package.exists():raise RuntimeError('Keep prior workflow evidence before retry')
(root/'vision-teaching').mkdir(parents=True);package.mkdir()
shutil.copytree(base/'runtime',package/'runtime');shutil.copytree(base/'models',package/'models')
shutil.copy2(D/'validated-release/vision_feature_probe.exe',package/'vision_feature_probe.exe')
shutil.copy2(D/'vision-candidate-02/adapter.json',package/'models/vision-adapter.json')
retention=package/'training/retention';retention.mkdir(parents=True)
raw=json.loads((D/'unique-dataset.json').read_text());rows=[]
for row in raw['rows']:
 if row['split']=='test':
  f=Path(row['features']);shutil.copy2(f,retention/f.name);rows.append({'features':f.name,'kind':row['kind'],'label':row['label']})
(retention/'dataset.json').write_text(json.dumps({'rows':rows}),encoding='utf-8')
training=[r for r in raw['rows'] if r['split']!='test']
for i,row in enumerate(training):
 sample={'schema_version':1,'request_id':i+1,'observation_id':i+1,'label':row['label'],'group':f'synthetic-context-{i%3}',
         'confirmed':True,'source':'authored synthetic integration fixture; NOT a human interaction','features':json.loads(Path(row['features']).read_text())}
 (root/'vision-teaching'/f'sample-{i+1:04}.json').write_text(json.dumps(sample),encoding='utf-8')
start=time.monotonic()
p=subprocess.run(['C:/Python312/python.exe',str(R/'scripts/train_personal_adapter.py'),'--profile',str(root),'--model',str(R/'models/training-base'),'--package',str(package)],capture_output=True,text=True,encoding='utf-8',errors='replace',timeout=600)
(D/'personal-workflow.log').write_text(p.stdout+'\n'+p.stderr,encoding='utf-8')
status=json.loads((root/'vision-training-status.json').read_text())
record={'exit':p.returncode,'seconds':time.monotonic()-start,'status':status,'scope':'end-to-end trainer test on authored synthetic fixtures after deduplication; live user data untouched','active_profile_modified':False,'automatically_promoted':(root/'vision-adapter.json').exists()}
(D/'personal-workflow-result.json').write_text(json.dumps(record,indent=2),encoding='utf-8')
assert not record['automatically_promoted']
print(json.dumps(record),flush=True)
if p.returncode:raise SystemExit(p.returncode)
assert status['state']=='ready_for_promotion'
