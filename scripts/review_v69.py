"""Validate the built native release, isolated state migration and production GPU output."""
from pathlib import Path
import hashlib,importlib.util,json,os,shutil,subprocess,sys,time
R=Path(__file__).resolve().parents[1];D=R/'reports/v69';T=D/'validated-release'
prep=json.loads((D/'preparation.json').read_text());previous=Path(prep['previous_release'])
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def source():
 h=hashlib.sha256()
 paths=[R/'Cargo.toml',R/'Cargo.lock']+sorted(p for name in ['app','crates','tools','config'] for p in (R/name).rglob('*') if p.is_file() and p.suffix in ['.rs','.toml','.wgsl','.json'])
 for p in paths:h.update(p.relative_to(R).as_posix().encode());h.update(p.read_bytes())
 return h.hexdigest()
def run(name,cmd,timeout=240):
 print('REVIEW',name,flush=True);start=time.monotonic();p=subprocess.run([str(x) for x in cmd],cwd=R,capture_output=True,text=True,encoding='utf-8',errors='replace',timeout=timeout)
 (D/(name+'.log')).write_text('COMMAND '+repr(cmd)+'\n'+p.stdout+'\n'+p.stderr,encoding='utf-8')
 row={'stage':name,'command':[str(x) for x in cmd],'exit':p.returncode,'seconds':time.monotonic()-start}
 if p.returncode:raise RuntimeError(f'{name}: '+(p.stdout+'\n'+p.stderr)[-5000:])
 return p,row

def main():
 start_source=source();checks=[]
 p,r=run('fresh-behavior',[T/'pet2.exe','--headless-smoke','60','--seed','42','--reset-pet','--no-audio','--data-dir',D/'fresh-profile'])
 result=json.loads(p.stdout);r['acceptance']=result['behavior_acceptance'];r['ecology']=result.get('ecology');checks.append(r)
 profile=D/'preserved-profile'
 if profile.exists():raise RuntimeError('Preserve old review state before rerunning')
 shutil.copytree(Path(prep['backup'])/'data',profile)
 before=json.loads((profile/'state.json').read_text(encoding='utf-8-sig'))
 export=D/'roundtrip-state.json'
 p,r=run('preserved-state',[T/'pet2.exe','--headless','--no-audio','--data-dir',profile,'--export-state',export])
 after=json.loads((profile/'state.json').read_text(encoding='utf-8-sig'));assert before['life']['state']['genome']==after['life']['state']['genome'];assert after==json.loads(export.read_text(encoding='utf-8-sig'))
 ecology=json.loads((profile/'ecology-state.json').read_text(encoding='utf-8-sig'))
 assert 'grounded' in ecology and ecology['grounded']['schema_version']==1
 r.update(genome_preserved=True,reset_pet=False,live_profile_modified=False,grounded_stats=ecology['grounded']['stats']);checks.append(r)
 images=[D/'vision-curriculum/document_test_20.png',D/'vision-curriculum/chart_test_20.png']
 p,r=run('native-adapted',[T/'vision_probe.exe',previous/'models/lfm2.5-vl-450m',previous/'runtime/onnxruntime.dll','--adapter',D/'vision-candidate-02/adapter.json',*images])
 native=[json.loads(x) for x in p.stdout.splitlines() if x.startswith('{')];assert all(x['accepted'] for x in native)
 r['predictions']=native;checks.append(r)
 features=[p.with_suffix('.features.json') for p in images]
 p,r=run('feature-decoder',[T/'vision_feature_probe.exe',previous/'models/lfm2.5-vl-450m',previous/'runtime/onnxruntime.dll','--adapter',D/'vision-candidate-02/adapter.json',*features])
 raw=[json.loads(x) for x in p.stdout.splitlines() if x.startswith('{')]
 assert len(raw)==len(native) and all(a['prediction']['kind']==b['prediction']['kind'] and abs(a['prediction']['support']-b['prediction']['support'])<0.0001 for a,b in zip(native,raw))
 r['same_decoder_verified']=True;checks.append(r)
 shutil.copy2(Path(prep['backup'])/'data/liquid-tuning.json',D/'review-profile.json')
 spec=importlib.util.spec_from_file_location('gpu_review',R/'scripts/review_v681_windows.py');review=importlib.util.module_from_spec(spec);spec.loader.exec_module(review)
 review.ROOT=R;review.REPORT=D;review.TARGET=T
 code=review.gpu();assert code==0;checks.append(json.loads((D/'gpu-result.json').read_text()))
 assert source()==start_source,'Source changed during native validation'
 binaries={str(p.relative_to(T)):{'sha256':sha(p),'size':p.stat().st_size} for p in [T/'pet2.exe',T/'body_lab.exe',T/'vision_probe.exe',T/'vision_feature_probe.exe',T/'examples/living_review.exe']}
 (D/'source-provenance.json').write_text(json.dumps({'source_sha256':start_source,'binaries':binaries,'tested_base_commit':prep['base_commit']},indent=2),encoding='utf-8')
 record={'exit':0,'checks':checks,'source_sha256':start_source,'user_profile_reset':False}
 (D/'runtime-review-result.json').write_text(json.dumps(record,indent=2),encoding='utf-8')
 (D/'runtime-review.log').write_text('\n\n'.join((D/(c['stage']+'.log')).read_text(encoding='utf-8',errors='replace') for c in checks if (D/(c['stage']+'.log')).exists()),encoding='utf-8')
 print('ALL NATIVE RELEASE CHECKS PASSED',flush=True)
if __name__=='__main__':
 try:main()
 except Exception as e:(D/'runtime-review-result.json').write_text(json.dumps({'exit':1,'error':str(e)},indent=2),encoding='utf-8');raise
