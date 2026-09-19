"""Launch an isolated native v20 pet and exercise right-click without touching saved pet state."""
import ctypes, json, os, subprocess, sys, time
from ctypes import wintypes as W
from pathlib import Path
exe=Path(sys.argv[1]).resolve(); data=Path(sys.argv[2]).resolve()
data.relative_to(Path(__file__).resolve().parents[1]/'target')
data.mkdir(parents=True, exist_ok=False)
u=ctypes.WinDLL('user32',use_last_error=True)
u.PostMessageW.argtypes=[W.HWND,W.UINT,W.WPARAM,W.LPARAM]
callback=ctypes.WINFUNCTYPE(W.BOOL,W.HWND,W.LPARAM)
def windows(pid):
 result=[]
 @callback
 def collect(hwnd,unused):
  owner=W.DWORD(); u.GetWindowThreadProcessId(hwnd,ctypes.byref(owner))
  if owner.value==pid and u.IsWindowVisible(hwnd): result.append(hwnd)
  return True
 u.EnumWindows(collect,0); return result
startup=subprocess.STARTUPINFO();startup.dwFlags|=subprocess.STARTF_USESHOWWINDOW;startup.wShowWindow=subprocess.SW_HIDE
env=os.environ.copy();env['PET2_MENU_CAPTURE']=str(data/'menu.ppm')
out=(data/'stdout.log').open('w');err=(data/'stderr.log').open('w')
p=subprocess.Popen([str(exe),'--data-dir',str(data),'--listen','--no-audio-output','--dev-mode'],cwd=exe.parent,env=env,startupinfo=startup,stdout=out,stderr=err)
(data/'native.json').write_text(json.dumps({'pid':p.pid,'data':str(data)}))
for _ in range(100):
 time.sleep(.5)
 if p.poll() is not None: raise RuntimeError(f'Pet exit {p.returncode}')
 if windows(p.pid) and (data/'telemetry.jsonl').exists(): break
hwnd=windows(p.pid)[0]
u.PostMessageW(hwnd,0x204,2,0);u.PostMessageW(hwnd,0x205,0,0)
for _ in range(90):
 time.sleep(.5)
 if (data/'menu.ppm').exists(): break
assert (data/'menu.ppm').exists(),'right-click menu did not connect/capture'
print(json.dumps({'pid':p.pid,'hwnd':hwnd,'menu':str(data/'menu.ppm')}),flush=True)
