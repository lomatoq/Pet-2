"""Package one validated standalone release without replacing a previous version."""
from __future__ import annotations
import argparse, datetime, hashlib, json, os, pathlib, shutil, subprocess
from release_provenance import ARTIFACTS, verify_artifacts
ROOT=pathlib.Path(__file__).resolve().parents[1]
REPORT=pathlib.Path(os.environ.get('PET2_VALIDATION_DIR',str(ROOT/'reports/v681')))
def digest(p):
 h=hashlib.sha256()
 with pathlib.Path(p).open('rb') as f:
  while b:=f.read(1024*1024): h.update(b)
 return {'sha256':h.hexdigest(),'size':pathlib.Path(p).stat().st_size}
def main():
 ap=argparse.ArgumentParser(__doc__)
 ap.add_argument('--previous',type=pathlib.Path,required=True)
 ap.add_argument('--destination',type=pathlib.Path,required=True)
 ap.add_argument('--target',type=pathlib.Path,default=pathlib.Path(os.environ.get('CARGO_TARGET_DIR',str(ROOT/'target'))))
 args=ap.parse_args(); dest=args.destination.resolve()
 provenance=verify_artifacts()
 if dest.exists() or dest==ROOT or dest==args.previous.resolve(): raise RuntimeError('Use a new release directory')
 # A saved pet may legitimately sleep. Check play on a fresh reference fixture,
 # and check identity/load/save separately on the unchanged saved profile.
 for name in ['workspace','build','gpu-build','clippy','geometry','care-native','contact','native','native-cancel','gpu','fresh-behavior','persistence']:
  if (REPORT/(name+'-exit.txt')).read_text(encoding='utf-8-sig').strip()!='0':
   raise RuntimeError('Required validation failed or missing: '+name)
 git=['git','-c','safe.directory='+ROOT.as_posix(),'-C',str(ROOT)]
 if subprocess.check_output(git+['status','--porcelain'],text=True).strip():
  raise RuntimeError('Commit reviewed source before release provenance')
 commit=subprocess.check_output(git+['rev-parse','HEAD'],text=True).strip()
 paths=sorted(p for parent in ['app','crates','tools','config'] for p in (ROOT/parent).rglob('*')
              if p.is_file() and p.suffix in ['.rs','.toml','.wgsl','.json'])
 tested=hashlib.sha256()
 for p in paths:
  tested.update(p.relative_to(ROOT).as_posix().encode()); tested.update(p.read_bytes())
 source_digest=tested.hexdigest()
 if source_digest!=json.loads((REPORT/'source-digest.json').read_text(encoding='utf-8'))['sha256']:
  raise RuntimeError('Production source changed after validation')
 persistence=json.loads((REPORT/'persistence-result.json').read_text(encoding='utf-8'))
 if not persistence.get('genome_preserved') or not persistence.get('export_matches_saved_state'):
  raise RuntimeError('Saved identity or state round-trip was not verified')
 model=ROOT/'models/lfm2.5-vl-450m'
 spec=json.loads((ROOT/'config/vision-model-manifest.json').read_text(encoding='utf-8'))
 for name,expected in spec['files'].items():
  p=(model/name).resolve()
  if not p.is_relative_to(model.resolve()) or digest(p)!=expected: raise RuntimeError('Invalid model: '+name)
 dest.parent.mkdir(parents=True,exist_ok=True)
 stage=dest.with_name(dest.name+'.staging')
 if stage.exists(): raise RuntimeError('Review previous staging directory before retrying: '+str(stage))
 stage.mkdir()
 try:
  for directory in ['assets','config','licenses']:
   shutil.copytree(args.previous/directory,stage/directory)
  shutil.copytree(ROOT/'config',stage/'config',dirs_exist_ok=True)
  shutil.copy2(ROOT/'LICENSE',stage/'licenses/Pet2-MIT.txt')
  shutil.copy2(args.previous/'app.ico',stage/'app.ico')
  for old,new in [('pet2.exe','Pet2.exe'),('body_lab.exe','Pet2 Dev Console.exe'),('vision_probe.exe','vision_probe.exe')]:
   shutil.copy2(ARTIFACTS/old,stage/new)
  (stage/'runtime').mkdir()
  for name in ['onnxruntime.dll','onnxruntime_providers_shared.dll']:
   shutil.copy2(ROOT/'.runtime-python/onnxruntime/capi'/name,stage/'runtime'/name)
  for name in ['LICENSE','ThirdPartyNotices.txt']:
   shutil.copy2(ROOT/'.runtime-python/onnxruntime'/name,stage/'licenses'/('ONNX-Runtime-'+name))
  target_model=stage/'models/lfm2.5-vl-450m'
  for name in list(spec['files'])+['LICENSE','model-manifest.json']:
   target=(target_model/name); target.parent.mkdir(parents=True,exist_ok=True)
   shutil.copy2(model/name,target)
  shutil.copy2(ROOT/'V68_1_LIVING_AGENCY.md',stage/'ПРОЧИТАЙ.md')
  shutil.copy2(ROOT/'V68_1_1_DESKTOP_RECOVERY.md',stage/'V68.1.1-Изменения.md')
  control=r'''param([ValidateSet('Enable','Disable','Status')][string]$Action='Status')
$ErrorActionPreference='Stop'
$root=Join-Path $env:LOCALAPPDATA 'lomatoq\Pet 2\data'
$settings=Join-Path $root 'vision-settings.json'
if ($Action -ne 'Status') {
 if (!(Test-Path -LiteralPath $root)) { throw 'Start Pet2 first.' }
 $json=@{enabled=($Action -eq 'Enable');interval_seconds=12} | ConvertTo-Json
 $tmp=$settings+'.control.tmp'
 [IO.File]::WriteAllText($tmp,$json,[Text.UTF8Encoding]::new($false))
 Move-Item -LiteralPath $tmp -Destination $settings -Force
}
if (Test-Path -LiteralPath $settings) { Get-Content -LiteralPath $settings }
$status=Join-Path $root 'vision-status.json'
if (Test-Path -LiteralPath $status) { Get-Content -LiteralPath $status }
'''
  (stage/'Vision-Control.ps1').write_text(control,encoding='utf-8-sig')
  for filename,action in [('Enable Local Vision.cmd','Enable'),('Disable Local Vision.cmd','Disable'),('Vision Status.cmd','Status')]:
   (stage/filename).write_text('@echo off\r\npowershell.exe -NoProfile -File "%~dp0Vision-Control.ps1" -Action '+action+'\r\npause\r\n',encoding='ascii')
  for filename,args_ in [('Start Pet2.cmd',''),('Start with Local Vision.cmd',' --local-vision')]:
   (stage/filename).write_text('@echo off\r\nstart "" "%~dp0Pet2.exe"'+args_+'\r\n',encoding='ascii')
  validation=stage/'validation'; validation.mkdir()
  for name in ['workspace-result.json','build-result.json','gpu-build-result.json','clippy-result.json','geometry-result.json','geometry.log','care-native-result.json','build-provenance.json','contact-result.json','native-result.json','native-cancel-result.json','gpu-result.json','fresh-behavior-result.json','persistence-result.json','test-summary.json','VALIDATION.md','workspace.log','native.log','native-cancel.log','contact.log','gpu.log','fresh-behavior.log','persistence.log','source-digest.json']:
   shutil.copy2(REPORT/name,validation/name)
  for name in ['material-review.png','pixel-delta.json','physics.json']:
   shutil.copy2(REPORT/'gpu-review'/name,validation/name)
  manifest={'schema':'pet2.release_manifest.v1','release_label':'Pet2 V68.1.1 Living Agency + Local Vision + Desktop Canvas Recovery',
   'built_utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'source_commit':commit,'source_worktree':str(ROOT),
   'production_source_sha256':source_digest,
   'cargo_sha256':provenance['cargo_sha256'],'lock_sha256':provenance['lock_sha256'],
   'compatible_pair':True,'lab_control_protocol':2,'lab_control_schema':4,'model_revision':spec['revision'],
   'files':{p.relative_to(stage).as_posix():digest(p) for p in sorted(stage.rglob('*')) if p.is_file()}}
  (stage/'release-manifest.json').write_text(json.dumps(manifest,indent=2,ensure_ascii=False),encoding='utf-8')
  for name,expected in manifest['files'].items():
   if digest(stage/name)!=expected: raise RuntimeError('Package verification failed: '+name)
  stage.rename(dest)
  record={'release':str(dest),'source_commit':commit,'files_verified':len(manifest['files']),'bytes':sum(x['size'] for x in manifest['files'].values())}
  (REPORT/'package.json').write_text(json.dumps(record,indent=2),encoding='utf-8')
  print(json.dumps(record),flush=True)
 except Exception:
  # Keep incomplete staging for diagnosis; never overwrite a working release.
  raise
if __name__=='__main__': main()
