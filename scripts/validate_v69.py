"""Run real checks with immutable command/result logs, never reset the live profile."""
from pathlib import Path
import os,sys,subprocess,json,time,hashlib
R=Path(__file__).resolve().parents[1];D=R/'reports/v69';D.mkdir(parents=True,exist_ok=True)
E=os.environ.copy();E.update(CARGO_TARGET_DIR=str(R.parent/'Pet2-V69-cache'),CARGO_BUILD_JOBS='4',PYTHONIOENCODING='utf-8')
P=Path(json.loads((D/'preparation.json').read_text())['previous_release'])
E.update(PET2_VISION_MODEL_DIR=str(P/'models/lfm2.5-vl-450m'),PET2_ONNX_RUNTIME_PATH=str(P/'runtime/onnxruntime.dll'),PET2_VISION_TEST_IMAGE=str(D/'vision-curriculum/document_test_20.png'))
commands={
 'ecology':['cargo','test','--locked','-p','pet_ecology'],
 'check':['cargo','check','--locked','-p','pet2','-p','body_lab'],
 'workspace':['cargo','test','--locked','--release','--workspace','--no-fail-fast'],
 'build':['cargo','build','--locked','--release','-p','pet2','-p','body_lab','-p','pet_vision','--bins'],
 'gpu-build':['cargo','build','--locked','--release','-p','pet_body','--example','living_review'],
 'native-cancel':['cargo','test','--locked','--release','-p','pet_vision','--test','native_cancellation','--','--ignored','--nocapture'],
}
stage=sys.argv[1];cmd=commands[stage];start=time.monotonic()
print('START',stage,flush=True)
with (D/(stage+'.log')).open('w',encoding='utf-8') as f:
 f.write('COMMAND '+repr(cmd)+'\n');f.flush();p=subprocess.run(cmd,cwd=R,env=E,stdout=f,stderr=subprocess.STDOUT)
r={'stage':stage,'command':cmd,'exit':p.returncode,'seconds':round(time.monotonic()-start,3)}
(D/(stage+'-result.json')).write_text(json.dumps(r,indent=2),encoding='utf-8');print(json.dumps(r),flush=True)
if p.returncode:print('\n'.join((D/(stage+'.log')).read_text(encoding='utf-8',errors='replace').splitlines()[-90:]),flush=True)
sys.exit(p.returncode)
