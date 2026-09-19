"""Bounded native smoke against an isolated copy, using the console protocol."""
import json
from pathlib import Path
import subprocess
import sys
import time
import uuid

exe, data = (Path(value).resolve() for value in sys.argv[1:3])
data.relative_to(Path(__file__).resolve().parents[1] / "target")
token = uuid.uuid4().hex
started = time.time()


def latest():
    path = data / "telemetry.jsonl"
    if not path.exists() or path.stat().st_mtime < started:
        return None
    for line in reversed(path.read_bytes().splitlines()[-3:]):
        try:
            record = json.loads(line)
            if "hearing" in record.get("details", {}):
                return record["details"]
        except ValueError:
            pass
    return None


def send(command):
    stamp = int(time.time() * 1000)
    payload = dict(schema_version=4, command_id=stamp, issued_unix_ms=stamp,
                   expires_after_ms=5000, session_token=token, command=command)
    (data / "lab-control.json").write_text(json.dumps(payload), encoding="utf-8")
    return stamp


def wait_for(predicate, seconds=10):
    until = time.monotonic() + seconds
    while time.monotonic() < until:
        if process.poll() is not None:
            raise RuntimeError(f"Native Pet exited: {process.returncode}")
        record = latest()
        if record is not None and predicate(record):
            return record
        time.sleep(0.2)
    raise RuntimeError("Native verification condition timed out")


def control(command):
    stamp = send(command)
    return wait_for(lambda row: row["lab_interventions"]["last_command_id"] == stamp
                    and row["lab_interventions"]["last_command_status"] == "applied")


startup = subprocess.STARTUPINFO()
startup.dwFlags |= subprocess.STARTF_USESHOWWINDOW
startup.wShowWindow = subprocess.SW_HIDE
with (data / "native-python.stdout.log").open("w") as stdout, (data / "native-python.stderr.log").open("w") as stderr:
    process = subprocess.Popen([str(exe), "--data-dir", str(data), "--listen", "--debug-log"],
                               cwd=exe.parent, stdout=stdout, stderr=stderr, startupinfo=startup)
    try:
        first = wait_for(lambda row: row["hearing"]["status"]["state"] == "listening", 60)
        control(dict(type="open_session", protocol_version=2, lease_seconds=10))
        quiet = control(dict(type="hearing", action="quiet_now"))
        assert quiet["hearing"]["master_gain"] == 0.25
        assert quiet["hearing"]["quiet_seconds"] > 0
        control(dict(type="renew_session", lease_seconds=10))
        restored = control(dict(type="hearing", action="restore_volume"))
        assert restored["hearing"]["master_gain"] == 1.0
        saved = json.loads((data / "hearing-state.json").read_text())
        assert saved["master_gain"] == 1.0 and saved["enabled"]
        result = dict(native_startup=True, microphone_listening=True,
                      device=first["hearing"]["device"], quiet_command=True,
                      restore_command=True, hearing_saved=True, pid=process.pid,
                      fps=restored.get("fps"), organic=restored.get("organic"))
        (data / "verification.json").write_text(json.dumps(result, indent=2), encoding="utf-8")
        print(json.dumps(result))
    finally:
        if process.poll() is None:
            send(dict(type="shutdown_for_promotion"))
            process.wait(timeout=20)
