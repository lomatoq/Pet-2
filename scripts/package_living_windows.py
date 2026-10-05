"""Package a validated local Windows build; never touches saved pet state.
Usage: python scripts/package_living_windows.py --previous OLD_RELEASE --destination NEW_RELEASE --target TARGET_DIR
Model and native runtime files must already be provisioned locally.
"""
from __future__ import annotations
import argparse, datetime, hashlib, json, pathlib, shutil, subprocess
ROOT = pathlib.Path(__file__).resolve().parents[1]
def digest(path: pathlib.Path) -> dict:
    h = hashlib.sha256()
    with path.open('rb') as f:
        while chunk := f.read(1024 * 1024): h.update(chunk)
    return {'sha256': h.hexdigest(), 'size': path.stat().st_size}
def main() -> None:
    parser = argparse.ArgumentParser(__doc__)
    parser.add_argument('--previous', type=pathlib.Path, required=True)
    parser.add_argument('--destination', type=pathlib.Path, required=True)
    parser.add_argument('--target', type=pathlib.Path, required=True)
    args = parser.parse_args()
    dest = args.destination.resolve()
    if dest == args.previous.resolve() or dest == ROOT: raise RuntimeError('Destination must be a new release directory')
    report = ROOT / 'reports/v68'
    for name in ['final-release-tests-exit.txt', 'final-release-build-exit.txt', 'native-vision-exit.txt', 'gpu-review-exit.txt']:
        if (report / name).read_text(encoding='utf-8-sig').strip() != '0':
            raise RuntimeError('Required validation did not succeed: ' + name)
    if dest.exists(): raise RuntimeError('Refusing to overwrite an existing release: ' + str(dest))
    dest.mkdir(parents=True)
    for directory in ['assets', 'config', 'licenses']:
        shutil.copytree(args.previous / directory, dest / directory)
    shutil.copy2(args.previous / 'app.ico', dest / 'app.ico')
    for source, name in [('pet2.exe', 'Pet2.exe'), ('body_lab.exe', 'Pet2 Dev Console.exe'), ('vision_probe.exe', 'vision_probe.exe')]:
        shutil.copy2(args.target / 'release' / source, dest / name)
    runtime = dest / 'runtime'; runtime.mkdir()
    for name in ['onnxruntime.dll', 'onnxruntime_providers_shared.dll']:
        shutil.copy2(ROOT / '.runtime-python/onnxruntime/capi' / name, runtime / name)
    for name in ['LICENSE', 'ThirdPartyNotices.txt']:
        shutil.copy2(ROOT / '.runtime-python/onnxruntime' / name, dest / 'licenses' / ('ONNX-Runtime-' + name))
    model = ROOT / 'models/lfm2.5-vl-450m'
    manifest = json.loads((model / 'model-manifest.json').read_text(encoding='utf-8'))
    for relative, expected in manifest['files'].items():
        source = (model / relative).resolve()
        if not source.is_relative_to(model.resolve()) or digest(source) != expected:
            raise RuntimeError('Invalid model file: ' + relative)
    shutil.copytree(model, dest / 'models/lfm2.5-vl-450m')
    shutil.copy2(ROOT / 'V68_LIVING_AGENCY.md', dest / 'ПРОЧИТАЙ.md')
    controls = r"""param([ValidateSet('Enable','Disable','Status')][string]$Action='Status')
$ErrorActionPreference='Stop'
$root=Join-Path $env:LOCALAPPDATA 'lomatoq\Pet 2\data'
$settings=Join-Path $root 'vision-settings.json'
if ($Action -ne 'Status') {
    if (!(Test-Path -LiteralPath $root)) { throw 'Pet2 data directory not found. Start Pet2 first.' }
    $enabled=($Action -eq 'Enable')
    $json=@{enabled=$enabled;interval_seconds=12} | ConvertTo-Json
    $tmp=$settings+'.control.tmp'
    [IO.File]::WriteAllText($tmp,$json,[Text.UTF8Encoding]::new($false))
    Move-Item -LiteralPath $tmp -Destination $settings -Force
}
Write-Host 'Local vision settings:'
if (Test-Path -LiteralPath $settings) { Get-Content -LiteralPath $settings }
Write-Host 'Runtime status:'
$status=Join-Path $root 'vision-status.json'
if (Test-Path -LiteralPath $status) { Get-Content -LiteralPath $status }
"""
    (dest / 'Vision-Control.ps1').write_text(controls, encoding='utf-8-sig')
    for filename, action in [('Enable Local Vision.cmd','Enable'),('Disable Local Vision.cmd','Disable'),('Vision Status.cmd','Status')]:
        (dest / filename).write_text('@echo off\r\npowershell.exe -NoProfile -File "%~dp0Vision-Control.ps1" -Action ' + action + '\r\npause\r\n', encoding='ascii')
    (dest / 'Start Pet2.cmd').write_text('@echo off\r\nstart "" "%~dp0Pet2.exe"\r\n', encoding='ascii')
    (dest / 'Start with Local Vision.cmd').write_text('@echo off\r\nstart "" "%~dp0Pet2.exe" --local-vision\r\n', encoding='ascii')
    validation = dest / 'validation'; validation.mkdir()
    for rel in ['gpu-review/pixel-delta.json', 'gpu-review/physics.json', 'gpu-review/material-review.png', 'native-vision-validation.log']:
        shutil.copy2(report / rel, validation / pathlib.Path(rel).name)
    git = ['git', '-c', 'safe.directory=' + str(ROOT), '-C', str(ROOT)]
    commit = subprocess.check_output(git + ['rev-parse','HEAD'], text=True).strip()
    changes = subprocess.check_output(git + ['status','--porcelain'], text=True).strip()
    if changes: raise RuntimeError('Source tree must be committed before generating release provenance')
    manifest = {'schema':'pet2.release_manifest.v1','release_label':'Pet2 V68 Living Agency + Local Vision',
        'built_utc':datetime.datetime.now(datetime.timezone.utc).isoformat(), 'source_commit':commit,
        'source_worktree':str(ROOT), 'compatible_pair':True,'lab_control_protocol':2,'lab_control_schema':4,
        'model_revision':'95c283d4497a56477a83177079fa6b7121abb1b1',
        'files':{p.relative_to(dest).as_posix():digest(p) for p in sorted(dest.rglob('*')) if p.is_file()}}
    (dest/'release-manifest.json').write_text(json.dumps(manifest,indent=2,ensure_ascii=False),encoding='utf-8')
    for relative, expected in manifest['files'].items():
        if digest(dest/relative) != expected: raise RuntimeError('Package verification failed: '+relative)
    print(json.dumps({'release':str(dest),'source_commit':commit,'files_verified':len(manifest['files']),
        'bytes':sum(x['size'] for x in manifest['files'].values())},ensure_ascii=False),flush=True)
if __name__ == '__main__': main()
