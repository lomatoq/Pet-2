"""Attach a successful post-package desktop check without changing any executable."""
from pathlib import Path
import hashlib,json,os,shutil
R=Path(__file__).resolve().parents[1];D=R/'reports/v69'
record=json.loads((D/'native-practice-result.json').read_text(encoding='utf-8'))
if record['exit']!=0 or not record['confirmed_plan_success']:raise RuntimeError('Native exercise did not pass')
package=Path(json.loads((D/'package.json').read_text())['package'])
manifest_path=package/'release-manifest.json'
manifest=json.loads(manifest_path.read_text(encoding='utf-8'))
def digest(p):
 h=hashlib.sha256()
 with p.open('rb') as f:
  for chunk in iter(lambda:f.read(1048576),b''):h.update(chunk)
 return {'sha256':h.hexdigest(),'size':p.stat().st_size}
for name,expected in manifest['files'].items():
 if digest(package/name)!=expected:raise RuntimeError('Package changed before post-package verification: '+name)
name='validation/native-practice-result.json'
shutil.copy2(D/'native-practice-result.json',package/name)
manifest['files'][name]=digest(package/name)
manifest['native_desktop_practice_verified']=True
manifest['native_practice_scope']='new isolated diagnostic pet; no user state reset'
tmp=manifest_path.with_suffix('.tmp');tmp.write_text(json.dumps(manifest,indent=2,ensure_ascii=False),encoding='utf-8');os.replace(tmp,manifest_path)
print('Native desktop verification attached; executable hashes unchanged')
