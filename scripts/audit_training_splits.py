"""Inspect supervision content before any training or release approval."""
from pathlib import Path
import collections,json
from supervision_contract import feature_digest
R=Path(__file__).resolve().parents[1]
D=R/'reports/v69'
rows=json.loads((D/'vision-curriculum/dataset.json').read_text(encoding='utf-8'))['rows']
content=collections.defaultdict(list)
for row in rows:
    f=json.loads(Path(row['features']).read_text(encoding='utf-8'))
    content[feature_digest(f)].append(row)
leaks=[{'sha256':key,'splits':sorted({r['split'] for r in group}),'examples':len(group)} for key,group in content.items() if len({r['split'] for r in group})>1]
report={'original_rows':len(rows),'unique_features':len(content),'cross_split_duplicates':len(leaks),'leaks':leaks,'prior_accuracy_is_not_independent_holdout':bool(leaks)}
(D/'supervision-audit.json').write_text(json.dumps(report,indent=2),encoding='utf-8')
print(json.dumps(report),flush=True)

# Create a new metadata-only split; original images, features and reports stay unchanged.
from supervision_contract import validate_supervision
by_kind=collections.defaultdict(list)
for key,group in content.items():
    if len({r['label'] for r in group})!=1:raise ValueError('Conflicting labels for identical content')
    by_kind[group[0]['kind']].append((key,group[0]))
unique_rows=[]
for kind,group in sorted(by_kind.items()):
    group.sort();n=len(group)
    if n<3:raise ValueError('Not enough distinct contents in '+kind)
    train=n//2;validation=max(1,(n-train)//2)
    for i,(key,row) in enumerate(group):
        split='train' if i<train else 'validation' if i<train+validation else 'test'
        unique_rows.append({**row,'split':split,'group':key})
check=validate_supervision(unique_rows)
output=D/'unique-dataset.json'
if output.exists():raise FileExistsError('Preserve previous split before rerunning')
output.write_text(json.dumps({'rows':unique_rows,'audit':check,'scope':'content-disjoint authored four-class fixtures; shared templates, not real applications'},indent=2),encoding='utf-8')
print(json.dumps({'new_split':check['counts'],'unique_features':check['unique_features']}),flush=True)
