"""Verify the packaged Windows runtime on disposable state; never touch the user's pet."""
import json, math, subprocess, sys, time, uuid
from pathlib import Path
exe = Path(sys.argv[1]).resolve()
data = Path(sys.argv[2]).resolve()
data.relative_to(Path(__file__).resolve().parents[1] / 'target')
data.mkdir(parents=True, exist_ok=False)
rows = []
token = uuid.uuid4().hex
stamp = 0

def send(command):
    global stamp
    stamp = max(stamp + 1, int(time.time() * 1000))
    payload = dict(schema_version=4, command_id=stamp, issued_unix_ms=stamp,
                   expires_after_ms=5000, session_token=token, command=command)
    temp = data / 'control.tmp'
    temp.write_text(json.dumps(payload), encoding='utf-8')
    temp.replace(data / 'lab-control.json')
    return stamp

def sample(seconds):
    end = time.monotonic() + seconds
    while time.monotonic() < end:
        if process.poll() is not None:
            raise RuntimeError(f'Pet exited {process.returncode}')
        time.sleep(.1)
        path = data / 'telemetry.jsonl'
        if path.exists():
            for line in path.read_bytes().splitlines()[-2:]:
                try:
                    row = json.loads(line)['details']
                    if not rows or row.get('sequence', 0) > rows[-1].get('sequence', 0):
                        rows.append(row)
                except (ValueError, KeyError):
                    pass

startup = subprocess.STARTUPINFO()
startup.dwFlags |= subprocess.STARTF_USESHOWWINDOW
startup.wShowWindow = subprocess.SW_HIDE
with (data/'stdout.log').open('w') as out, (data/'stderr.log').open('w') as err:
    process = subprocess.Popen([str(exe), '--data-dir', str(data), '--no-audio-output', '--dev-mode'],
                               cwd=exe.parent, startupinfo=startup, stdout=out, stderr=err)
    try:
        deadline = time.monotonic() + 40
        while not rows and time.monotonic() < deadline:
            sample(1)
        assert rows, 'No rendered telemetry'
        send(dict(type='open_session', protocol_version=2, lease_seconds=10))
        sample(.5)
        for _ in range(3):
            send(dict(type='run_motor_program', program='defense_startle_orient_freeze'))
            sample(2.6)
            send(dict(type='renew_session', lease_seconds=10))
            sample(.3)
        send(dict(type='cancel_motor_program'))
        sample(2)
        phases = {r.get('motor',{}).get('packet',{}).get('phase_name') for r in rows}
        launch = [r for r in rows if r.get('motor',{}).get('packet',{}).get('phase_name') == 'startle_launch']
        assert launch, f'No launch frames: {phases}'
        assert 'startle_brake' in phases and 'startle_recover' in phases, phases
        for r in rows:
            for key in ('world_position','velocity'):
                assert all(math.isfinite(v) for v in r[key]), (key,r[key])
        assert all(r.get('activity',{}).get('orb_play_variant_count') == 24 for r in rows)
        fps = [r['fps'] for r in rows if r.get('fps',0)>0]
        speeds = [math.hypot(*r['velocity']) for r in rows]
        report = dict(passed=True, frames=len(rows), fps_min=min(fps), fps_mean=sum(fps)/len(fps),
                      max_normalized_speed=max(speeds), launch_frames=len(launch),
                      phases=sorted(p for p in phases if p), finite_motion=True,
                      orb_play_variants=24, isolated_state=str(data))
        (data/'verification.json').write_text(json.dumps(report,indent=2),encoding='utf-8')
        print(json.dumps(report))
    finally:
        if process.poll() is None:
            send(dict(type='shutdown_for_promotion'))
            try: process.wait(timeout=15)
            except subprocess.TimeoutExpired:
                process.terminate()
                process.wait(timeout=5)
