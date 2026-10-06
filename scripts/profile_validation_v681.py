"""Separate a deterministic fresh behavior fixture from real-profile persistence."""
from pathlib import Path
import json, os, subprocess, time, sys
ROOT=Path(__file__).resolve().parents[1]; D=ROOT/'reports/v681'
EXE=Path(os.environ.get('CARGO_TARGET_DIR',str(ROOT/'target')))/'release/pet2.exe'
name='fresh-behavior'
state=D/'fresh-behavior-profile'
if state.exists(): raise RuntimeError('Fresh fixture directory already exists; preserve the previous evidence before retry')
cmd=[str(EXE),'--headless-smoke','60','--seed','42','--reset-pet','--no-audio','--data-dir',str(state)]
started=time.monotonic()
p=subprocess.run(cmd,cwd=ROOT,capture_output=True,text=True,encoding='utf-8',errors='replace')
(D/(name+'.log')).write_text('COMMAND: '+str(cmd)+'\n'+p.stdout+'\n'+p.stderr,encoding='utf-8')
try: result=json.loads(p.stdout)
except ValueError: result={'raw_stdout':p.stdout[-2000:]}
record={'stage':name,'command':cmd,'exit':p.returncode,'seconds':round(time.monotonic()-started,3),
        'scope':'new isolated seed-42 reference fixture, not the user pet','result':result}
(D/(name+'-result.json')).write_text(json.dumps(record,indent=2),encoding='utf-8')
(D/(name+'-exit.txt')).write_text(str(p.returncode),encoding='utf-8')
print(json.dumps({'exit':p.returncode,'seconds':record['seconds'],'acceptance':result.get('behavior_acceptance'),
                  'actions':result.get('action_counts'),'ecology':result.get('ecology')}),flush=True)
sys.exit(p.returncode)
