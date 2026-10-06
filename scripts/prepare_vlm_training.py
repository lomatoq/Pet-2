"""Download only public model weights; no user data is transmitted."""
from pathlib import Path
import json, os
from huggingface_hub import HfApi, snapshot_download
R=Path(__file__).resolve().parents[1]
D=R/'reports/v69'; D.mkdir(parents=True,exist_ok=True)
repo='LiquidAI/LFM2.5-VL-450M'
info=HfApi().model_info(repo)
print('PINNED_TRAINING_BASE',info.sha,flush=True)
local=R/'models/training-base'
snapshot_download(repo,revision=info.sha,local_dir=local,allow_patterns=['*.json','*.safetensors','LICENSE','README.md'],max_workers=2)
(D/'training-base.json').write_text(json.dumps({'model':repo,'revision':info.sha,'path':str(local),'uploaded_user_data':False},indent=2),encoding='utf-8')
import torch, transformers
from transformers import AutoModelForImageTextToText
m=AutoModelForImageTextToText.from_pretrained(local,dtype=torch.bfloat16,device_map='cpu',local_files_only=True,attn_implementation='eager')
print('MODULES',[name for name,_ in m.named_children()],flush=True)
print('INNER',[name for name,_ in m.model.named_children()],flush=True)
print('PROJECTOR',m.model.multi_modal_projector,flush=True)
print('READY',torch.__version__,transformers.__version__,flush=True)
