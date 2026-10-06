"""Freeze the exact tested native binaries; never package from a mutable shared cache."""
from pathlib import Path
import hashlib,importlib.util,json,re,shutil,subprocess,sys,time
R=Path(__file__).resolve().parents[1];D=R/'reports/v69';T=R.parent/'Pet2-V69-cache/release'
def run(script,*args):
    cmd=[sys.executable,str(R/'scripts'/script),*args]
    print('FINAL',script,*args,flush=True)
    p=subprocess.run(cmd,cwd=R)
    if p.returncode:raise SystemExit(p.returncode)
def source():
    h=hashlib.sha256()
    paths=[R/'Cargo.toml',R/'Cargo.lock']+sorted(p for name in ['app','crates','tools','config'] for p in (R/name).rglob('*') if p.is_file() and p.suffix in ['.rs','.toml','.wgsl','.json'])
    for p in paths:h.update(p.relative_to(R).as_posix().encode());h.update(p.read_bytes())
    return h.hexdigest()
start=source();run('validate_v69.py','workspace')
for stage in ['build','gpu-build','native-cancel']:run('validate_v69.py',stage)
if source()!=start:raise RuntimeError('Source changed during final checks')
frozen=D/'validated-release'
if frozen.exists():raise FileExistsError('Preserve previous release evidence before reusing this directory')
frozen.mkdir();artifacts={}
for name in ['pet2.exe','body_lab.exe','vision_probe.exe','vision_feature_probe.exe','examples/living_review.exe']:
    src=T/name;dst=frozen/name;dst.parent.mkdir(parents=True,exist_ok=True)
    h=hashlib.sha256(src.read_bytes()).hexdigest();shutil.copy2(src,dst)
    if h!=hashlib.sha256(src.read_bytes()).hexdigest() or h!=hashlib.sha256(dst.read_bytes()).hexdigest():raise RuntimeError('Build artifact changed while copying')
    artifacts[name]={'sha256':h,'size':dst.stat().st_size}
(D/'frozen-artifacts.json').write_text(json.dumps({'source_sha256':start,'binaries':artifacts},indent=2),encoding='utf-8')
run('review_v69.py');run('validate_v69_adapter.py');run('test_personal_training_workflow.py')
if source()!=start:raise RuntimeError('Source changed during runtime evaluation')
rows=re.findall(r'test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored',(D/'workspace.log').read_text(encoding='utf-8'))
counts={k:sum(int(row[i]) for row in rows) for i,k in enumerate(['passed','failed','ignored'])}
if not rows or counts['failed'] or counts['passed']<1100:raise RuntimeError('Invalid workspace evidence')
(D/'test-counts.json').write_text(json.dumps(counts,indent=2),encoding='utf-8')
print('FINAL_VALIDATED',json.dumps(counts),flush=True)
