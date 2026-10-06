"""Package a source-bound, validated Windows release beside the last known build."""
from pathlib import Path
import datetime,hashlib,json,os,re,shutil,subprocess,sys
R=Path(__file__).resolve().parents[1];D=R/'reports/v69';T=D/'validated-release'
prep=json.loads((D/'preparation.json').read_text());previous=Path(prep['previous_release'])
def digest(p):
 h=hashlib.sha256()
 with p.open('rb') as f:
  for b in iter(lambda:f.read(1024*1024),b''):h.update(b)
 return {'sha256':h.hexdigest(),'size':p.stat().st_size}
def main():
 for name in ['workspace','build','runtime-review','native-cancel','personal-workflow']:
  report=json.loads((D/(name+'-result.json')).read_text());assert report['exit']==0,(name,report)
 assert json.loads((D/'no-feedback-validation.json').read_text())['passed']
 validation=json.loads((D/'native-adapter-validation.json').read_text());assert validation['approved_for_packaging']
 from review_v69 import source
 provenance=json.loads((D/'source-provenance.json').read_text())
 if source()!=provenance['source_sha256']:raise RuntimeError('Source changed after native validation')
 for name,expected in provenance['binaries'].items():
  if digest(T/name)!=expected:raise RuntimeError('A verified executable changed: '+name)
 commit=subprocess.check_output(['git','-c','safe.directory='+R.as_posix(),'-C',str(R),'rev-parse','HEAD'],text=True).strip()
 status=subprocess.check_output(['git','-c','safe.directory='+R.as_posix(),'-C',str(R),'status','--porcelain'],text=True).strip()
 if status:raise RuntimeError('Commit the tested source before release')
 destination=previous.parent/'Pet2-V69-Grounded-Learning-2026-10-06';stage=destination.with_name(destination.name+'.staging')
 if destination.exists() or stage.exists():raise RuntimeError('A release/staging directory already exists; preserve it before retry')
 stage.mkdir()
 for directory in ['assets','config','licenses','runtime','models']:
  shutil.copytree(previous/directory,stage/directory)
 shutil.copytree(R/'config',stage/'config',dirs_exist_ok=True)
 shutil.copy2(previous/'app.ico',stage/'app.ico')
 files=[('pet2.exe','Pet2.exe'),('body_lab.exe','Pet2 Dev Console.exe'),('vision_probe.exe','vision_probe.exe'),('vision_feature_probe.exe','vision_feature_probe.exe')]
 for source,target in files:shutil.copy2(T/source,stage/target)
 adapter=D/'vision-candidate-02/adapter.json';assert digest(adapter)['sha256']==validation['adapter_sha256']
 shutil.copy2(adapter,stage/'models/vision-adapter.json')
 scripts=stage/'scripts';scripts.mkdir()
 for name in ['train_personal_adapter.py','train_vision_connector.py','supervision_contract.py']:shutil.copy2(R/'scripts'/name,scripts/name)
 (stage/'config/learning-runtime.json').write_text(json.dumps({'training_python':'C:\\Python312\\python.exe','training_model':str(R/'models/training-base'),'inference_requires_python':False},indent=2),encoding='utf-8')
 retention=stage/'training/retention';retention.mkdir(parents=True)
 dataset=json.loads((D/'unique-dataset.json').read_text());rows=[]
 for r in dataset['rows']:
  if r['split']!='test':continue
  f=Path(r['features']);shutil.copy2(f,retention/f.name);rows.append({'features':f.name,'kind':r['kind'],'label':r['label'],'group':r['group'],'scope':'authored retention fixture'})
 (retention/'dataset.json').write_text(json.dumps({'rows':rows,'scope':'synthetic retention only'},indent=2),encoding='utf-8')
 shutil.copy2(R/'V69_GROUNDED_LEARNING.md',stage/'ПРОЧИТАЙ.md')
 for name in ['Start Pet2.cmd','Start with Local Vision.cmd','Enable Local Vision.cmd','Disable Local Vision.cmd','Vision Status.cmd','Vision-Control.ps1']:
  if (previous/name).exists():shutil.copy2(previous/name,stage/name)
 validation_dir=stage/'validation';validation_dir.mkdir()
 for name in ['workspace-result.json','build-result.json','runtime-review-result.json','native-adapter-validation.json','no-feedback-validation.json','source-provenance.json','supervision-audit.json','personal-workflow-result.json','native-cancel-result.json','test-counts.json','frozen-artifacts.json']:
  shutil.copy2(D/name,validation_dir/name)
 for name in ['workspace.log','runtime-review.log']:shutil.copy2(D/name,validation_dir/name)
 shutil.copy2(D/'vision-candidate-02/training-report.json',validation_dir/'adapter-training.json')
 if (D/'gpu-review/material-review.png').exists():shutil.copy2(D/'gpu-review/material-review.png',validation_dir/'material-review.png')
 manifest={'schema':'pet2.release_manifest.v1','release_label':'Pet2 V69 Grounded Learning','built_utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'source_commit':commit,'base_commit':prep['base_commit'],'source_worktree':str(R),'compatible_pair':True,'lab_control_protocol':2,'lab_control_schema':4,'files':{p.relative_to(stage).as_posix():digest(p) for p in sorted(stage.rglob('*')) if p.is_file()}}
 (stage/'release-manifest.json').write_text(json.dumps(manifest,indent=2,ensure_ascii=False),encoding='utf-8')
 for path,spec in manifest['files'].items():assert digest(stage/path)==spec,path
 stage.rename(destination)
 record={'package':str(destination),'source_commit':commit,'files_verified':len(manifest['files']),'bytes':sum(s['size'] for s in manifest['files'].values()),'previous_unchanged':str(previous)}
 (D/'package.json').write_text(json.dumps(record,indent=2),encoding='utf-8');print(json.dumps(record),flush=True)
if __name__=='__main__':main()
