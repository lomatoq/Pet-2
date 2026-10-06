"""Run the final source validation in order, stopping at the first failed stage."""
from pathlib import Path
import subprocess,sys,hashlib,json,time
ROOT=Path(__file__).resolve().parents[1]
REPORT=ROOT/'reports/v681'
paths=sorted(p for parent in ['app','crates','tools','config'] for p in (ROOT/parent).rglob('*')
             if p.is_file() and p.suffix in ['.rs','.toml','.wgsl','.json'])
h=hashlib.sha256()
for p in paths:
    h.update(p.relative_to(ROOT).as_posix().encode());h.update(p.read_bytes())
source=h.hexdigest()
(REPORT/'source-digest.json').write_text(json.dumps({'sha256':source,'files':len(paths)},indent=2),encoding='utf-8')
stages=[('validate_v681.py','workspace'),('validate_v681.py','build'),('validate_v681.py','gpu-build'),
        ('validate_v681.py','contact'),('validate_v681.py','native-cancel'),
        ('review_v681_windows.py','native'),('review_v681_windows.py','gpu'),
        ('profile_validation_v681.py','fresh-behavior'),('persistence_validation_v681.py','persistence')]
results=[]
for script,stage in stages:
    print('FINAL STAGE',stage,flush=True)
    started=time.monotonic()
    p=subprocess.run([sys.executable,str(ROOT/'scripts'/script)]+([stage] if stage else []),cwd=ROOT)
    results.append({'stage':stage,'exit':p.returncode,'seconds':round(time.monotonic()-started,2)})
    (REPORT/'final-sequence.json').write_text(json.dumps({'source_sha256':source,'stages':results},indent=2),encoding='utf-8')
    if p.returncode: sys.exit(p.returncode)
print('All final validation stages finished successfully',flush=True)
