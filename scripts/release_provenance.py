"""Bind release artifacts to reviewed source; isolate them from a shared Cargo cache."""
from __future__ import annotations
from pathlib import Path
import hashlib, json, os, shutil, time

ROOT = Path(__file__).resolve().parents[1]
REPORT = Path(os.environ.get('PET2_VALIDATION_DIR', str(ROOT / 'reports/v681')))
ARTIFACTS = REPORT / 'validated-release'

def digest(path: Path) -> str:
    h = hashlib.sha256()
    with path.open('rb') as f:
        while chunk := f.read(1024 * 1024):
            h.update(chunk)
    return h.hexdigest()

def source_identity() -> dict:
    paths = sorted(p for parent in ['app', 'crates', 'tools', 'config']
                   for p in (ROOT / parent).rglob('*')
                   if p.is_file() and p.suffix in ['.rs', '.toml', '.wgsl', '.json'])
    h = hashlib.sha256()
    for p in paths:
        h.update(p.relative_to(ROOT).as_posix().encode())
        h.update(p.read_bytes())
    return {'source_sha256': h.hexdigest(), 'cargo_sha256': digest(ROOT / 'Cargo.toml'),
            'lock_sha256': digest(ROOT / 'Cargo.lock')}

def snapshot_binaries(target: Path, names: list[str], finished_ns: int) -> None:
    ARTIFACTS.mkdir(parents=True, exist_ok=True)
    receipt_path = REPORT / 'build-provenance.json'
    identity = source_identity()
    receipt = json.loads(receipt_path.read_text(encoding='utf-8')) if receipt_path.exists() else {'files': {}}
    if any(receipt.get(k, v) != v for k, v in identity.items()):
        raise RuntimeError('Source changed between artifact builds; rerun the release checks')
    receipt.update(identity)
    for name in names:
        source = target / name
        before = source.stat()
        if before.st_mtime_ns > finished_ns:
            raise RuntimeError('Another build already replaced the requested binary: ' + name)
        destination = ARTIFACTS / name
        destination.parent.mkdir(parents=True, exist_ok=True)
        temporary = destination.with_suffix(destination.suffix + '.copying')
        shutil.copy2(source, temporary)
        expected = digest(source)
        after = source.stat()
        if (before.st_size, before.st_mtime_ns) != (after.st_size, after.st_mtime_ns) or digest(temporary) != expected:
            raise RuntimeError('Concurrent artifact change detected: ' + name)
        temporary.replace(destination)
        receipt['files'][name] = {'sha256': expected, 'size': after.st_size}
    receipt['captured_unix_ns'] = time.time_ns()
    temp = receipt_path.with_suffix('.tmp')
    temp.write_text(json.dumps(receipt, indent=2), encoding='utf-8')
    temp.replace(receipt_path)

def verify_artifacts() -> dict:
    receipt = json.loads((REPORT / 'build-provenance.json').read_text(encoding='utf-8'))
    if any(receipt.get(k) != v for k, v in source_identity().items()):
        raise RuntimeError('Release source no longer matches the compiled artifacts')
    for name in ['pet2.exe', 'body_lab.exe', 'vision_probe.exe']:
        spec = receipt['files'][name]
        path = ARTIFACTS / name
        if path.stat().st_size != spec['size'] or digest(path) != spec['sha256']:
            raise RuntimeError('Validated release binary was changed: ' + name)
    return receipt
