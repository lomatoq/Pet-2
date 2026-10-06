"""The trainer must reject missing evidence without changing active weights."""
from pathlib import Path
import json,subprocess,sys,datetime
R=Path(__file__).resolve().parents[1];D=R/'reports/v69'
profile=D/('empty-teaching-'+datetime.datetime.now().strftime('%Y%m%d-%H%M%S'));profile.mkdir()
package=Path(json.loads((D/'preparation.json').read_text())['previous_release'])
p=subprocess.run([sys.executable,str(R/'scripts/train_personal_adapter.py'),'--profile',str(profile),'--model',str(R/'models/training-base'),'--package',str(package)],capture_output=True,text=True,encoding='utf-8',errors='replace')
status=json.loads((profile/'vision-training-status.json').read_text())
passed=p.returncode!=0 and status['state']=='not_promoted' and status.get('active_model_changed') is False and not (profile/'vision-adapter.json').exists()
record={'passed':passed,'trainer_exit':p.returncode,'status':status,'scope':'new isolated empty teaching profile; no real user data or weights altered'}
(D/'no-feedback-validation.json').write_text(json.dumps(record,indent=2),encoding='utf-8')
print(json.dumps(record),flush=True)
if not passed:raise SystemExit(1)
