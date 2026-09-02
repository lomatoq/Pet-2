@echo off
setlocal
tasklist /FI "IMAGENAME eq Pet2.exe" 2>NUL | find /I "Pet2.exe" >NUL
if not errorlevel 1 (
  echo Close every packaged Pet2.exe before starting the live den experiment.
  echo Another transparent overlay would contaminate the desktop capture.
  pause
  exit /b 1
)
start "PET-2 live den" "%~dp0target\debug\pet2.exe" --dev-mode
timeout /t 2 /nobreak >NUL
start "PET-2 den controls" "%~dp0target\debug\body_lab.exe"
endlocal
