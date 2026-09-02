@echo off
setlocal
tasklist /FI "IMAGENAME eq Pet2.exe" 2>NUL | find /I "Pet2.exe" >NUL
if not errorlevel 1 (
  echo Close the running Pet2.exe before starting Pet plus Lab.
  pause
  exit /b 1
)
start "PET-2 live den" "%~dp0Pet2.exe" --dev-mode
timeout /t 2 /nobreak >NUL
start "PET-2 unified lab" "%~dp0PetLab.exe"
endlocal
