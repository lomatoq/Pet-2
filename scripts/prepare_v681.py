"""Preserve the running profile and reuse the verified local inference assets."""
from pathlib import Path
import os, shutil, json, datetime, hashlib
ROOT = Path(__file__).resolve().parents[1]
OLD = ROOT.parent / 'Pet2-V68-LivingAgency'
PROFILE = Path(os.environ['LOCALAPPDATA']) / 'lomatoq' / 'Pet 2' / 'data'
PROJECT = Path(r'C:\Users\nirrt\OneDrive\Документы\ChatGPT\Pet 2')
REPORT = ROOT / 'reports/v681'; REPORT.mkdir(parents=True,exist_ok=True)
STAMP=datetime.datetime.now().strftime('%Y%m%d-%H%M%S')
BACKUP=ROOT.parent / 'Pet2-Backups' / ('before-v68.1-'+STAMP)
BACKUP.mkdir(parents=True)
# Prior backups/curriculum runs are independent archives, not live profile data.
shutil.copytree(PROFILE,BACKUP/'running-profile',ignore=shutil.ignore_patterns('backups','evolution-runs'))
shutil.copy2(PROFILE/'liquid-tuning.json',REPORT/'review-profile.json')
model=ROOT/'models/lfm2.5-vl-450m'
shutil.copytree(OLD/'models/lfm2.5-vl-450m',model,dirs_exist_ok=True)
manifest=json.loads((ROOT/'config/vision-model-manifest.json').read_text(encoding='utf-8'))
for relative,spec in manifest['files'].items():
    p=(model/relative).resolve()
    assert p.is_relative_to(model.resolve())
    assert p.stat().st_size==spec['size'],relative
    h=hashlib.sha256()
    with p.open('rb') as f:
        while b:=f.read(1024*1024): h.update(b)
    assert h.hexdigest()==spec['sha256'],relative
runtime=ROOT/'.runtime-python/onnxruntime'
(runtime/'capi').mkdir(parents=True,exist_ok=True)
for rel in ['capi/onnxruntime.dll','capi/onnxruntime_providers_shared.dll','LICENSE','ThirdPartyNotices.txt']:
    shutil.copy2(OLD/'.runtime-python/onnxruntime'/rel,runtime/rel)
shutil.copytree(OLD/'reports/v68/fixtures',REPORT/'fixtures',dirs_exist_ok=True)
record={'backup':str(BACKUP),'profile':str(PROFILE),'model_revision':manifest['revision'],'model_files_verified':len(manifest['files']),
 'profile_files':len(list((BACKUP/'running-profile').rglob('*'))),'excluded_archive_directories':['backups','evolution-runs'],
 'source_worktree':str(ROOT),'previous_worktree_unchanged':str(OLD)}
(REPORT/'preparation.json').write_text(json.dumps(record,indent=2,ensure_ascii=False),encoding='utf-8')
print(json.dumps(record,ensure_ascii=True),flush=True)
