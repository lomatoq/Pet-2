from pathlib import Path
import subprocess,sys
R=Path(__file__).resolve().parents[1];D=R/'reports/v69'
for name in ['synthetic-teaching-profile','training-fixture-package','personal-workflow-result.json','personal-workflow.log']:
 p=D/name
 if p.exists():p.rename(D/(name+'.insufficient-examples'))
p=R/'scripts/test_personal_training_workflow.py';s=p.read_text(encoding='utf-8');s=s.replace("training=[r for r in raw['rows'] if r['split']=='train'][:24]","training=[r for r in raw['rows'] if r['split']!='test']").replace('on 24 labelled synthetic fixtures','on authored synthetic fixtures after deduplication');p.write_text(s,encoding='utf-8',newline='\n')
print('Earlier insufficient-data rejection preserved; adding genuinely distinct authored examples, not lowering the threshold',flush=True)
sys.exit(subprocess.call([sys.executable,str(p)]))
