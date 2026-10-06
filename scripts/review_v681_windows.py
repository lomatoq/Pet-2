"""Real native/GPU/state checks; no access to the running Pet's mutable state."""
from __future__ import annotations
import json, os, re, shutil, statistics, subprocess, sys, time
from pathlib import Path
ROOT=Path(__file__).resolve().parents[1]
REPORT=ROOT/'reports/v681'
TARGET=Path(os.environ.get('CARGO_TARGET_DIR',str(ROOT/'target')))/'release'
def run(name,cmd):
    start=time.perf_counter()
    p=subprocess.run([str(x) for x in cmd],cwd=ROOT,capture_output=True,text=True,encoding='utf-8',errors='replace')
    (REPORT/(name+'.log')).write_text('COMMAND: '+str(cmd)+'\n'+p.stdout+'\n'+p.stderr,encoding='utf-8')
    return p,{'stage':name,'command':[str(x) for x in cmd],'exit':p.returncode,'seconds':round(time.perf_counter()-start,3)}
def finish(name,record):
    (REPORT/(name+'-result.json')).write_text(json.dumps(record,indent=2,ensure_ascii=False),encoding='utf-8')
    (REPORT/(name+'-exit.txt')).write_text(str(record['exit']),encoding='utf-8')
    print(json.dumps(record,ensure_ascii=True),flush=True)
    return record['exit']
def native():
    fixtures=[REPORT/'fixtures'/f'{x}.png' for x in ['document','chart','code','desktop']]*3
    p,record=run('native',[TARGET/'vision_probe.exe',ROOT/'models/lfm2.5-vl-450m',ROOT/'.runtime-python/onnxruntime/capi/onnxruntime.dll',*fixtures])
    rows=[json.loads(line) for line in p.stdout.splitlines() if line.strip().startswith('{')]
    expected={'document':'document','chart':'chart','code':'code','desktop':'desktop'}
    if p.returncode==0:
        valid=len(rows)==12 and all(not r['accepted'] or r['prediction']['kind']==expected[Path(r['image']).stem] for r in rows)
        valid=valid and all(r['accepted'] for r in rows if Path(r['image']).stem in ['document','chart'])
        record['exit']=0 if valid else 1
    times=sorted(r['prediction']['elapsed_ms'] for r in rows)
    record.update({'scope':'four synthetic fixtures repeated three times, NOT a real-desktop accuracy benchmark',
        'observations':len(rows),'accepted':sum(r['accepted'] for r in rows),'rows':rows,
        'median_ms':statistics.median(times) if times else None,
        'p95_ms':times[min(len(times)-1,int(.95*len(times)))] if times else None})
    return finish('native',record)
def gpu():
    from PIL import Image, ImageDraw, ImageFont
    import numpy as np
    out=REPORT/'gpu-review'
    if out.exists(): raise RuntimeError('Preserve previous GPU evidence before reusing this output path')
    p,record=run('gpu',[TARGET/'examples/living_review.exe',REPORT/'review-profile.json',out])
    if p.returncode: return finish('gpu',record)
    frames={}
    for path in sorted(out.glob('*.rgba')):
        raw=path.read_bytes(); w=int.from_bytes(raw[:4],'little'); h=int.from_bytes(raw[4:8],'little')
        assert len(raw)==8+w*h*4 and w==768 and h==512
        image=Image.frombytes('RGBA',(w,h),raw[8:]); image.save(path.with_suffix('.png')); frames[path.stem]=image
    deltas=[]
    for bg in ['black','white','busy']:
        first=np.asarray(frames['frozen-'+bg+'-0']).astype(np.float32)
        last=np.asarray(frames['frozen-'+bg+'-720']).astype(np.float32)
        diff=np.abs(first[:,:,:3]-last[:,:,:3])
        row={'background':bg,'mean_abs_rgb_8bit':float(diff.mean()),'pixels_with_channel_delta_gt_3':int((diff.max(axis=2)>3).sum())}
        assert row['pixels_with_channel_delta_gt_3']>30 and row['mean_abs_rgb_8bit']>0.005, row
        deltas.append(row)
    physics=json.loads((out/'physics.json').read_text(encoding='utf-8'))
    assert all(x['finite'] and x['failsafe']==0 and x['components']==1 for x in physics),physics
    # Contact sheet contains unmodified real renderer pixels, only resized for layout.
    names=[f'frozen-{bg}-{t}' for bg in ['black','white','busy'] for t in [0,120,360,720]]
    sheet=Image.new('RGB',(4*384,3*284),(36,36,40)); draw=ImageDraw.Draw(sheet)
    for i,name in enumerate(names):
        x=(i%4)*384;y=(i//4)*284
        sheet.paste(frames[name].convert('RGB').resize((384,256)),(x,y+28))
        draw.text((x+8,y+7),name,fill=(240,240,245))
    sheet.save(out/'material-review.png')
    (out/'pixel-delta.json').write_text(json.dumps(deltas,indent=2),encoding='utf-8')
    record.update({'captured_frames':len(frames),'frozen_geometry_deltas':deltas,'physics':physics,
                   'scope':'isolated production renderer and authored profile; not a recording of the user desktop'})
    return finish('gpu',record)
def seeds(node,prefix=''):
    found={}
    if isinstance(node,dict):
        for k,v in node.items():
            if k=='identity_seed': found[prefix+k]=v
            else: found.update(seeds(v,prefix+k+'.'))
    elif isinstance(node,list):
        for i,v in enumerate(node):found.update(seeds(v,prefix+str(i)+'.'))
    return found
def smoke():
    prep=json.loads((REPORT/'preparation.json').read_text(encoding='utf-8'))
    source=Path(prep['backup'])/'running-profile'
    state=REPORT/'smoke-profile'
    if state.exists(): raise RuntimeError('Preserve previous isolated state run before repeating')
    shutil.copytree(source,state)
    before=json.loads((state/'state.json').read_text(encoding='utf-8'))
    p,record=run('smoke',[TARGET/'pet2.exe','--headless-smoke','60','--no-audio','--data-dir',state])
    after=json.loads((state/'state.json').read_text(encoding='utf-8'))
    preserved=bool(seeds(before)) and seeds(before)==seeds(after)
    if not preserved: record['exit']=1
    record.update({'identity_preserved':preserved,'isolated_profile':str(state),'reset_pet':False,'live_profile_modified':False})
    return finish('smoke',record)
if __name__=='__main__':
    try: sys.exit({'native':native,'gpu':gpu,'smoke':smoke}[sys.argv[1]]())
    except Exception as e:
        name=sys.argv[1]; finish(name,{'stage':name,'exit':1,'error':repr(e)});raise
