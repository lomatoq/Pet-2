"""Disposable v21 voice-action and native microphone control verification."""
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
  assert len(rows[-1]['hearing']['cues'])==21
  old=W.POINT();u.GetCursorPos(ctypes.byref(old));w=u.GetSystemMetrics(78);height=u.GetSystemMetrics(79)
  x,y=rows[-1]['world_position'];u.SetCursorPos(int((.80 if x<.5 else .20)*w),int(.4*height));sample(.3)
  begin=len(rows);send(dict(type='hearing',action={'perform':{'cue':'dash'}}));sample(5)
  dash=rows[begin:];peak=max((r['screen_velocity_px'][0]**2+r['screen_velocity_px'][1]**2)**.5 for r in dash)
  assert peak>450, f'dash too slow: {peak}'
  send(dict(type='renew_session',lease_seconds=10));begin=len(rows)
  send(dict(type='hearing',action={'perform':{'cue':'circle'}}));sample(7.5)
  circle=rows[begin:];xs=[r['screen_body_center_px'][0] for r in circle];ys=[r['screen_body_center_px'][1] for r in circle]
  assert max(xs)-min(xs)>60 and max(ys)-min(ys)>60,'circle did not travel'
  send(dict(type='renew_session',lease_seconds=10));begin=len(rows)
  send(dict(type='hearing',action={'perform':{'cue':'jump'}}));sample(3)
  jump=rows[begin:];ys=[r['screen_body_center_px'][1] for r in jump];assert max(ys)-min(ys)>40,'no jump'
  send(dict(type='hearing',action={'perform':{'cue':'sit'}}));sample(5)
  send(dict(type='renew_session',lease_seconds=10))
  send(dict(type='hearing',action={'train_command':{'cue':'name'}}));sample(1)
  assert rows[-1]['hearing']['training'] is not None,'training did not start'
  send(dict(type='hearing',action='cancel_training'));sample(.8)
  assert rows[-1]['hearing']['training'] is None,'training did not cancel'
  u.SetCursorPos(old.x,old.y)
  report={'dash_peak_px_s':peak,'circle_span_px':[max(xs)-min(xs),max(r['screen_body_center_px'][1] for r in circle)-min(r['screen_body_center_px'][1] for r in circle)],'jump_span_px':max(ys)-min(ys),'catalog_count':len(rows[-1]['hearing']['cues']),'training_start_cancel':True,'hearing':rows[-1]['hearing']['status'],'fps_mean':sum(r['fps'] for r in rows)/len(rows)}
  (data/'trace.json').write_text(json.dumps(rows));(data/'verification.json').write_text(json.dumps(report,indent=2));print(json.dumps(report),flush=True)

 finally:
  if process.poll() is None:
   send(dict(type='shutdown_for_promotion'))
   try:process.wait(10)
   except subprocess.TimeoutExpired:process.terminate()
