"""Explicitly authored synthetic supervision, never pseudo-labels or user captures."""
from pathlib import Path
import sys
sys.path.insert(0,str(Path(__file__).resolve().parents[1]/'.training-runtime'))
from PIL import Image, ImageDraw, ImageFont
import json, random, hashlib, numpy as np, onnxruntime as ort, time
R=Path(__file__).resolve().parents[1];D=R/'reports/v69/vision-curriculum';D.mkdir(parents=True,exist_ok=True)
prep=json.loads((R/'reports/v69/preparation.json').read_text(encoding='utf-8'))
M=Path(prep['previous_release'])/'models/lfm2.5-vl-450m'
manifest=json.loads((M/'model-manifest.json').read_text(encoding='utf-8'))
for name in ['onnx/vision_encoder_q8.onnx','onnx/vision_encoder_q8.onnx_data']:
 p=M/name;assert hashlib.sha256(p.read_bytes()).hexdigest()==manifest['files'][name]['sha256']
options=ort.SessionOptions();options.intra_op_num_threads=2;options.inter_op_num_threads=1
session=ort.InferenceSession(str(M/'onnx/vision_encoder_q8.onnx'),options,providers=['CPUExecutionProvider'])
font=ImageFont.truetype('C:/Windows/Fonts/consola.ttf',9)
small=ImageFont.truetype('C:/Windows/Fonts/arial.ttf',8)
rows=[]
for label,kind in enumerate(['code','document','chart','desktop']):
 letter={'code':'A','document':'B','chart':'F','desktop':'G'}[kind]
 for split,indices in [('train',range(8)),('validation',range(10,14)),('test',range(20,24))]:
  for i in indices:
   rng=random.Random(137*label+i);dark=i%2==0
   bg=(24,29,37) if dark else (230,233,239);fg=(207,212,226) if dark else (31,39,51)
   im=Image.new('RGB',(256,256),bg);d=ImageDraw.Draw(im)
   d.rectangle((0,0,255,19),fill=(47,53,67) if dark else (199,206,219))
   d.text((9,5),{'code':'source.rs - Code','document':'Document - Writer','chart':'Workbook - Sheet 1','desktop':'Desktop'}[kind],font=small,fill=fg)
   if kind=='code':
    sidebar=35+(i%3)*10;d.rectangle((0,20,sidebar,255),fill=(35,39,46) if dark else (214,219,226))
    for j,t in enumerate(['src/','app.rs','model.rs','test.rs']):d.text((3,28+j*14),t,font=small,fill=fg)
    code=['fn main() {','  let target = 42;','  for i in 0..8 {','    update(i);','    value += 1;','  }','}','// new observation','fn predict(x: f32) {','  x * 0.85 + 0.1','}']
    for j,t in enumerate(code):
     d.text((sidebar+4,25+j*17),str(j+1),font=small,fill=(120,126,140));d.text((sidebar+17,25+j*17),t,font=font,fill=[fg,(134,191,132),(91,154,226),(211,144,110)][j%4])
   elif kind=='document':
    d.rectangle((27,27,231,246),fill=(251,249,244));ink=(44,43,39)
    d.text((39,39),'Project notes '+str(i),font=font,fill=ink)
    d.line((39,57,210,57),fill=(98,106,122),width=2)
    for j in range(15):
     y=72+j*10;w=rng.randint(80,167);d.line((39,y,39+w,y),fill=(100,101,105),width=1)
     if j in [4,10]:d.text((39,y+2),'Section '+str(j),font=small,fill=ink)
   elif kind=='chart':
    for j in range(9):d.line((18,26+j*24,242,26+j*24),fill=(72,80,94) if dark else (185,192,202))
    for j in range(8):d.line((18+j*30,26,18+j*30,236),fill=(72,80,94) if dark else (185,192,202))
    for j in range(6):
     h=rng.randint(30,145);x=35+j*31;d.rectangle((x,225-h,x+18,225),fill=[(77,139,214),(103,179,144),(202,142,90)][j%3])
     d.text((x,234),str(j+1),font=small,fill=fg)
    d.text((36,28),'Monthly results',font=font,fill=fg)
   else:
    for y in range(20,238):
     c=int((y-20)/218*55);d.line((0,y,255,y),fill=(30+c,57+c,104+c))
    for j in range(3+i%4):
     x=15+(j//4)*54;y=32+(j%4)*46;d.rounded_rectangle((x,y,x+18,y+19),radius=3,fill=(184,201,225));d.text((x-1,y+22),'Folder',font=small,fill=(236,240,249))
    d.rectangle((0,239,255,255),fill=(31,35,42))
    for x in [83,106,129,152]:d.rectangle((x,243,x+12,252),fill=(118,139,165))
   name=f'{kind}_{split}_{i:02}';image=D/(name+'.png');im.save(image)
   pixels=np.asarray(im,dtype=np.float32)/127.5-1.0
   patches=pixels.reshape(16,16,16,16,3).transpose(0,2,1,3,4).reshape(1,256,768)
   features=session.run(['image_features'],{'pixel_values':patches,'pixel_attention_mask':np.ones((1,256),np.int64),'spatial_shapes':np.array([[16,16]],np.int64)})[0]
   assert features.shape==(64,1024) and np.isfinite(features).all()
   feature=D/(name+'.features.json');feature.write_text(json.dumps({'schema_version':1,'model_revision':manifest['revision'],'image_tokens':64,'features':features.reshape(-1).tolist()}),encoding='utf-8')
   rows.append({'image':str(image),'features':str(feature),'label':letter,'kind':kind,'split':split,'group':name,'confirmed':True,'source':'authored synthetic fixture, not model-generated label'})
   print(name,flush=True)
(D/'dataset.json').write_text(json.dumps({'schema_version':1,'rows':rows,'scope':'64 authored images across four known UI classes; separate layouts, not a real-desktop benchmark'},indent=2),encoding='utf-8')
print('CURRICULUM_READY',len(rows),flush=True)
