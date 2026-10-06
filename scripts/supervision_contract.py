"""Fail closed on duplicate content across training/evaluation splits."""
from __future__ import annotations
import hashlib,json,math,struct
from pathlib import Path
REVISION='95c283d4497a56477a83177079fa6b7121abb1b1'

def feature_digest(payload:dict)->str:
    n=payload.get('image_tokens'); values=payload.get('features')
    if payload.get('model_revision')!=REVISION or type(n) is not int or not 1<=n<=256:
        raise ValueError('Incompatible feature revision or token count')
    if not isinstance(values,list) or len(values)!=n*1024:
        raise ValueError('Invalid visual feature shape')
    if any(type(x) not in (int,float) or not math.isfinite(x) or abs(x)>1000 for x in values):
        raise ValueError('Invalid visual feature values')
    return hashlib.sha256(struct.pack('<I',n)+struct.pack('<%df'%len(values),*values)).hexdigest()

def validate_supervision(rows:list[dict])->dict:
    if not 16<=len(rows)<=512: raise ValueError('Supervision requires 16 through 512 examples')
    groups={};content={};counts={'train':0,'validation':0,'test':0};manifest=[]
    for row in rows:
        label=row.get('label');split=row.get('split');group=row.get('group')
        if row.get('confirmed') is not True or not isinstance(label,str) or len(label)!=1 or label not in 'ABCDEFGHI':
            raise ValueError('Training requires explicit confirmed labels')
        if split not in counts or not isinstance(group,str) or not group or len(group)>256:
            raise ValueError('Invalid split or group')
        if group in groups and groups[group]!=split:raise ValueError('Observation group leaks across splits')
        groups[group]=split
        p=Path(row['features'])
        if not p.is_file() or p.stat().st_size>4_000_000:raise ValueError('Missing or oversized feature example')
        digest=feature_digest(json.loads(p.read_text(encoding='utf-8')))
        if digest in content:
            old_split,old_label=content[digest]
            if old_split!=split:raise ValueError('Identical visual content occurs in different splits')
            if old_label!=label:raise ValueError('Contradictory duplicate labels')
            raise ValueError('Duplicate visual example; deduplicate before training')
        content[digest]=(split,label);counts[split]+=1
        manifest.append({'sha256':digest,'split':split,'group':group,'label':label})
    if counts['train']<8 or counts['validation']<8:raise ValueError('Need 8 training and 8 disjoint validation examples')
    return {'counts':counts,'unique_features':len(content),'groups':len(groups),'cross_split_duplicates':0,'content_manifest':manifest}
