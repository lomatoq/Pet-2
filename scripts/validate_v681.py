"""Run validation on this worktree; preserve full command, exit status and logs."""
import os, sys, json, subprocess, time
from pathlib import Path
ROOT = Path(__file__).resolve().parents[1]
REPORT = ROOT / 'reports' / 'v681'
REPORT.mkdir(parents=True, exist_ok=True)
ENV = os.environ.copy()
ENV['CARGO_TARGET_DIR'] = os.environ.get('CARGO_TARGET_DIR',str(ROOT/'target'))
ENV['CARGO_BUILD_JOBS'] = '4'
ENV['PYTHONIOENCODING'] = 'utf-8'
ENV['PET2_VISION_MODEL_DIR'] = str(ROOT/'models/lfm2.5-vl-450m')
ENV['PET2_ONNX_RUNTIME_PATH'] = str(ROOT/'.runtime-python/onnxruntime/capi/onnxruntime.dll')
ENV['PET2_VISION_TEST_IMAGE'] = str(REPORT/'fixtures/document.png')
STAGES = {
 'contact': ['cargo','test','--locked','--release','--workspace','--bin','pet2','tests::supported_rest_forms_a_flat_patch_without_moving_its_reference_frame','--','--exact','--nocapture'],
 'flow': ['cargo','test','--locked','--release','--workspace','--lib','living_flow::','--','--nocapture'],
 'native-cancel': ['cargo','test','--locked','--release','--workspace','--test','native_cancellation','--','--ignored','--nocapture'],
 'app-tests': ['cargo','test','--locked','--release','--workspace','--bin','pet2'],
 'physics': ['cargo','test','--locked','--release','--workspace','--lib','liquid::'],
 'workspace': ['cargo','test','--locked','--release','--workspace','--no-fail-fast'],
 'build': ['cargo','build','--locked','--release','--workspace','--bin','pet2','--bin','body_lab','--bin','vision_probe'],
 'gpu-build': ['cargo','build','--locked','--release','--workspace','--example','living_review'],
}
name = sys.argv[1]
cmd = STAGES[name]
start = time.time()
print('START',name,' '.join(cmd),flush=True)
with (REPORT / (name+'.log')).open('w',encoding='utf-8') as f:
    f.write('COMMAND: '+' '.join(cmd)+'\n'); f.flush()
    result=subprocess.run(cmd,cwd=ROOT,env=ENV,stdout=f,stderr=subprocess.STDOUT)
record={'stage':name,'command':cmd,'exit':result.returncode,'seconds':round(time.time()-start,2)}
(REPORT/(name+'-exit.txt')).write_text(str(result.returncode),encoding='utf-8')
(REPORT/(name+'-result.json')).write_text(json.dumps(record,indent=2),encoding='utf-8')
print(json.dumps(record),flush=True)
if result.returncode:
    print('\n'.join((REPORT/(name+'.log')).read_text(encoding='utf-8',errors='replace').splitlines()[-35:]),flush=True)
sys.exit(result.returncode)
