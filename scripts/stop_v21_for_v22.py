"""Graceful update of the known V21 runtime; no saved state is replaced."""
import json,time,uuid,shutil,hashlib,ctypes
from pathlib import Path
# Workspace is .worktrees/macos-parity-r14, so project root is two parents above it.
root=Path(__file__).resolve().parents[1].parent.parent
data=Path.home()/'AppData/Local/lomatoq/Pet 2/data'
token=uuid.uuid4().hex
last=json.loads((data/'telemetry.jsonl').read_bytes().splitlines()[-1])['details']['lab_interventions']['last_command_id']
for command in [dict(type='open_session',protocol_version=2,lease_seconds=10),dict(type='shutdown_for_promotion')]:
 stamp=max(int(time.time()*1000),last+1);last=stamp
 payload=dict(schema_version=4,command_id=stamp,issued_unix_ms=int(time.time()*1000),expires_after_ms=5000,session_token=token,command=command)
 tmp=data/'v22-control.tmp';tmp.write_text(json.dumps(payload));tmp.replace(data/'lab-control.json');time.sleep(1.0)
for _ in range(100):
 try:ack=json.loads((data/'runtime-load-ack.json').read_text())
 except (OSError,ValueError):time.sleep(.1);continue
 if ack.get('status')=='stopped':break
 time.sleep(.1)
assert ack.get('status')=='stopped',ack
backup=root/'backups'/'pre-v22-2026-09-19';backup.mkdir(parents=True,exist_ok=False)
for name in ['state.json','body-state.json','ecology-state.json','hearing-state.json','liquid-tuning.json','organic_regulation.json','morph-brain.json']:
 if (data/name).exists():shutil.copy2(data/name,backup/name)
h=json.loads((data/'hearing-state.json').read_text());m=h['model'];print(json.dumps({'backup':str(backup),'name_examples':len((m.get('name') or {}).get('examples',[])),'other_examples':len(m.get('other_examples',[])),'ack':ack},ensure_ascii=False))
