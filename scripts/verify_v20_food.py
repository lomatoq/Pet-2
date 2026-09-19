"""Disposable end-to-end food test. Writes only under target; uses native pet input."""
import ctypes,json,os,subprocess,time,uuid,sys
from pathlib import Path
from ctypes import wintypes as W
exe=Path(sys.argv[1]).resolve();data=Path(sys.argv[2]).resolve();data.relative_to(Path(__file__).resolve().parents[1]/'target');data.mkdir(parents=True,exist_ok=False)
u=ctypes.WinDLL('user32');u.PostMessageW.argtypes=[W.HWND,W.UINT,W.WPARAM,W.LPARAM]
token=uuid.uuid4().hex;stamp=0;rows=[]
def send(command):
 global stamp
 stamp=max(stamp+1,int(time.time()*1000));p=dict(schema_version=4,command_id=stamp,issued_unix_ms=int(time.time()*1000),expires_after_ms=5000,session_token=token,command=command)
 t=data/'control.tmp';t.write_text(json.dumps(p));t.replace(data/'lab-control.json')
def sample(seconds):
 until=time.monotonic()+seconds
 while time.monotonic()<until:
  time.sleep(.15)
  if process.poll() is not None: raise RuntimeError('pet exited')
  p=data/'telemetry.jsonl'
  if p.exists():
   for line in p.read_bytes().splitlines()[-2:]:
    try:
     r=json.loads(line)['details']
     if not rows or r['sequence']>rows[-1]['sequence']:rows.append(r)
    except (KeyError,ValueError):pass
startup=subprocess.STARTUPINFO();startup.dwFlags|=subprocess.STARTF_USESHOWWINDOW;startup.wShowWindow=subprocess.SW_HIDE
with (data/'stdout.log').open('w') as out,(data/'stderr.log').open('w') as err:
 process=subprocess.Popen([str(exe),'--data-dir',str(data),'--listen','--no-audio-output','--dev-mode'],cwd=exe.parent,startupinfo=startup,stdout=out,stderr=err)
 try:
  for _ in range(40):
   sample(1)
   if rows:break
  assert rows
  found=[]
  @ctypes.WINFUNCTYPE(W.BOOL,W.HWND,W.LPARAM)
  def cb(h,l):
   pid=W.DWORD();u.GetWindowThreadProcessId(h,ctypes.byref(pid))
   if pid.value==process.pid and u.IsWindowVisible(h):found.append(h)
   return True
  u.EnumWindows(cb,0);h=found[0]
  send(dict(type='open_session',protocol_version=2,lease_seconds=10));sample(.7)
  send(dict(type='feeding',enabled=True));sample(.7)
  assert rows[-1]['feeding']['enabled']
  # Drop above the pet, then restore the user's pointer.
  old=W.POINT();u.GetCursorPos(ctypes.byref(old));w=u.GetSystemMetrics(78);height=u.GetSystemMetrics(79)
  x,y=rows[-1]['world_position'];u.SetCursorPos(int(x*w),int(max(.15,y-.18)*height));sample(.3)
  u.PostMessageW(h,0x201,1,0);time.sleep(.1);u.PostMessageW(h,0x202,0,0);sample(.5);u.SetCursorPos(old.x,old.y)
  assert any(r['ecology'].get('food') for r in rows),'no food spawned'
  initial=next(r for r in reversed(rows) if r['ecology'].get('food'))['ecology']['satiation']
  for _ in range(8):
   sample(5);send(dict(type='renew_session',lease_seconds=10))
   if rows[-1]['ecology']['satiation']>initial+.01:break
  report={'consumed':rows[-1]['ecology']['satiation']>initial+.01,'initial_satiation':initial,'final_satiation':rows[-1]['ecology']['satiation'],'food_left':rows[-1]['ecology']['food'],'goals':sorted({r['ecology']['active_goal'] or '' for r in rows}),'hearing':rows[-1]['hearing']['status'],'fps_mean':sum(r['fps'] for r in rows)/len(rows),'startup_components':rows[0]['liquid']['components']}
  (data/'trace.json').write_text(json.dumps(rows));(data/'verification.json').write_text(json.dumps(report,indent=2));print(json.dumps(report),flush=True)
  assert report['consumed'],'food was not consumed'
 finally:
  if process.poll() is None:
   send(dict(type='shutdown_for_promotion'))
   try:process.wait(10)
   except subprocess.TimeoutExpired:process.terminate()
