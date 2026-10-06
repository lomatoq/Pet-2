"""Exercise the real desktop app on a NEW isolated diagnostic pet, not user data."""
from pathlib import Path
import ctypes,datetime,json,os,subprocess,time
from ctypes import wintypes
R=Path(__file__).resolve().parents[1];D=R/'reports/v69'
package=Path(json.loads((D/'package.json').read_text(encoding='utf-8'))['package'])
profile=D/'native-practice-profile'
if profile.exists():raise RuntimeError('Preserve previous native fixture before repeating')
# The desktop singleton is global. Never close a pet not started by this probe.
existing=subprocess.check_output(['powershell.exe','-NoProfile','-Command',"@(Get-Process Pet2 -ErrorAction SilentlyContinue).Count"],text=True).strip()
if existing!='0':raise RuntimeError('Another pet is running; the isolated native probe will not replace it')
exe=package/'Pet2.exe';profile.mkdir()
subprocess.run([str(exe),'--headless-smoke','1','--seed','42','--reset-pet','--no-audio','--data-dir',str(profile)],check=True,capture_output=True)
state=json.loads((profile/'state.json').read_text(encoding='utf-8'))
# Controlled fixture initial conditions; never applied to the real profile.
state['life']['state']['drives']['sleep']=0.10
state['life']['state']['drives']['play']=0.70
state['life']['state']['current_action']='idle_hover'
(profile/'state.json').write_text(json.dumps(state),encoding='utf-8')
(profile/'birth-42.txt').write_text(str(int(time.time())-86400),encoding='ascii')
(profile/'vision-settings.json').write_text(json.dumps({'enabled':False,'interval_seconds':12}),encoding='utf-8')
user32=ctypes.WinDLL('user32',use_last_error=True)
CALLBACK=ctypes.WINFUNCTYPE(wintypes.BOOL,wintypes.HWND,wintypes.LPARAM)
user32.EnumWindows.argtypes=[CALLBACK,wintypes.LPARAM]
user32.GetWindowThreadProcessId.argtypes=[wintypes.HWND,ctypes.POINTER(wintypes.DWORD)]
user32.PostMessageW.argtypes=[wintypes.HWND,wintypes.UINT,wintypes.WPARAM,wintypes.LPARAM]
def close_owned_process(p):
    if p.poll() is not None:return
    @CALLBACK
    def visit(window,_):
        pid=wintypes.DWORD();user32.GetWindowThreadProcessId(window,ctypes.byref(pid))
        if pid.value==p.pid:user32.PostMessageW(window,0x0010,0,0)
        return True
    user32.EnumWindows(visit,0)
    try:p.wait(timeout=20)
    except subprocess.TimeoutExpired:p.terminate();p.wait(timeout=10)
def read(name):
    try:return json.loads((profile/name).read_text(encoding='utf-8-sig'))
    except (OSError,ValueError):return {}

rows=[];sent=False;success=False;initial=0;started=time.monotonic();started_unix_ms=int(time.time()*1000);request_id=0;observed_requested_family=False
stdout=(D/'native-practice-stdout.log').open('w',encoding='utf-8')
stderr=(D/'native-practice-stderr.log').open('w',encoding='utf-8')
p=subprocess.Popen([str(exe),'--data-dir',str(profile),'--no-audio','--debug-log'],cwd=package,stdout=stdout,stderr=stderr,creationflags=subprocess.CREATE_NO_WINDOW)
try:
    while time.monotonic()-started<90:
        if p.poll() is not None:raise RuntimeError('Diagnostic desktop pet exited')
        status=read('grounded-status.json');stats=status.get('stats',{})
        own_runtime=read('runtime-load-ack.json').get('pid')==p.pid
        if own_runtime and status.get('instance',0)>=started_unix_ms and not sent:
            initial=stats.get('plan_successes',0)
            request={'id':int(time.time()*1000),'instance':status['instance'],'issued_unix_ms':int(time.time()*1000),'action':'practice','family':'roll','level':0}
            request_id=request['id']
            tmp=profile/'grounded-control.tmp';tmp.write_text(json.dumps(request),encoding='utf-8');os.replace(tmp,profile/'grounded-control.json');sent=True
        if status:
            rows.append({'seconds':round(time.monotonic()-started,2),'stats':stats,'exercise':status.get('exercise'),'suspended':status.get('suspended'),'message':status.get('message')})
            ack=read('grounded-control-ack.json')
            x=status.get('exercise') or {}
            observed_requested_family |= sent and x.get('family')=='roll'
            if ack.get('id')==request_id and ack.get('applied') is True and observed_requested_family and stats.get('plan_successes',0)>initial:
                success=True;break
        time.sleep(1)
finally:
    close_owned_process(p);stdout.close();stderr.close()
    report={'exit':0 if success else 1,'scope':'real native desktop application with a NEW controlled awake seed42 fixture; user profile untouched','pid':p.pid,'profile':str(profile),'seconds':time.monotonic()-started,'requested':sent,'confirmed_plan_success':success,'ack':read('grounded-control-ack.json'),'rows':rows}
    (D/'native-practice-result.json').write_text(json.dumps(report,indent=2),encoding='utf-8')
    print(json.dumps({k:v for k,v in report.items() if k!='rows'}),flush=True)
if not success:raise SystemExit(1)
