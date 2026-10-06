"""Validate real-profile persistence without requiring a sleep-deprived pet to play."""
from pathlib import Path
import json, os, shutil, subprocess, time, sys
ROOT=Path(__file__).resolve().parents[1]; D=ROOT/'reports/v681'
prep=json.loads((D/'preparation.json').read_text(encoding='utf-8'))
source=Path(prep['backup'])/'running-profile'; state=D/'persistence-profile'
if state.exists(): raise RuntimeError('Preserve earlier evidence before repeating this profile round-trip')
shutil.copytree(source,state)
before=json.loads((state/'state.json').read_text(encoding='utf-8-sig'))
export=D/'roundtrip-state.json'
cmd=[str(Path(os.environ.get('CARGO_TARGET_DIR',str(ROOT/'target')))/'release/pet2.exe'),
     '--headless','--no-audio','--data-dir',str(state),'--export-state',str(export)]
started=time.monotonic()
p=subprocess.run(cmd,cwd=ROOT,capture_output=True,text=True,encoding='utf-8',errors='replace')
(D/'persistence.log').write_text('COMMAND: '+str(cmd)+'\n'+p.stdout+'\n'+p.stderr,encoding='utf-8')
record={'stage':'persistence','exit':p.returncode,'command':cmd,'seconds':round(time.monotonic()-started,3),
        'scope':'60-second real-profile load/save/export, not a playful-behavior acceptance test',
        'reset_pet':False,'live_profile_modified':False,'isolated_profile':str(state)}
if p.returncode==0:
    after=json.loads((state/'state.json').read_text(encoding='utf-8-sig'))
    exported=json.loads(export.read_text(encoding='utf-8-sig'))
    record['genome_preserved']=before['life']['state']['genome']==after['life']['state']['genome']
    record['export_matches_saved_state']=after==exported
    record['source_sleep_need']=before['life']['state']['drives']['sleep']
    record['result_sleep_need']=after['life']['state']['drives']['sleep']
    record['result_action']=after['life']['state']['current_action']
    if not record['genome_preserved'] or not record['export_matches_saved_state']:record['exit']=1
(D/'persistence-result.json').write_text(json.dumps(record,indent=2),encoding='utf-8')
(D/'persistence-exit.txt').write_text(str(record['exit']),encoding='utf-8')
print(json.dumps(record),flush=True)
sys.exit(record['exit'])
