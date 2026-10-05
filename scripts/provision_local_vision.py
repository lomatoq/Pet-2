"""Download only pinned model data; optionally install a private CPU runtime.
No remote Python code is imported or executed. Never changes the global Python.
"""
from __future__ import annotations
import argparse, hashlib, json, pathlib, subprocess, sys, urllib.request
ROOT = pathlib.Path(__file__).resolve().parents[1]
def sha256(path: pathlib.Path) -> str:
    h=hashlib.sha256()
    with path.open('rb') as stream:
        while chunk:=stream.read(1024*1024): h.update(chunk)
    return h.hexdigest()
def main() -> None:
    parser=argparse.ArgumentParser(__doc__)
    parser.add_argument('--install-runtime',action='store_true')
    args=parser.parse_args()
    manifest=json.loads((ROOT/'config/vision-model-manifest.json').read_text(encoding='utf-8'))
    if manifest['model']!='LiquidAI/LFM2.5-VL-450M-ONNX': raise RuntimeError('Unrecognized model')
    model=ROOT/'models/lfm2.5-vl-450m'; model.mkdir(parents=True,exist_ok=True)
    for name,spec in manifest['files'].items():
        path=(model/name).resolve()
        if not path.is_relative_to(model.resolve()): raise RuntimeError('Invalid model path')
        path.parent.mkdir(parents=True,exist_ok=True)
        if path.is_file() and path.stat().st_size==spec['size'] and sha256(path)==spec['sha256']: continue
        url=f"https://huggingface.co/{manifest['model']}/resolve/{manifest['revision']}/{name}"
        partial=path.with_name(path.name+'.partial')
        with urllib.request.urlopen(url,timeout=120) as response,partial.open('wb') as output:
            while chunk:=response.read(1024*1024): output.write(chunk)
        if partial.stat().st_size!=spec['size'] or sha256(partial)!=spec['sha256']:
            raise RuntimeError('Checksum mismatch: '+name)
        partial.replace(path); print('Verified',name,flush=True)
    (model/'model-manifest.json').write_text(json.dumps(manifest,indent=2),encoding='utf-8')
    source=ROOT/'config/vision-model-LICENSE.txt'
    (model/'LICENSE').write_bytes(source.read_bytes())
    if args.install_runtime:
        subprocess.run([sys.executable,'-m','pip','install','--target',str(ROOT/'.runtime-python'),
            '--no-deps','--only-binary=:all:','onnxruntime==1.30.0'],check=True)
    print('Pinned model data is ready. Rust loads the native library, not Python.')
if __name__=='__main__': main()
