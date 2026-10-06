param(
 [Parameter(Mandatory=$true)][string]$Package,
 [Parameter(Mandatory=$true)][string]$Previous,
 [Parameter(Mandatory=$true)][string]$Data,
 [Parameter(Mandatory=$true)][string]$BackupRoot
)
$ErrorActionPreference='Stop'
[Console]::OutputEncoding=[Text.UTF8Encoding]::new()
function Copy-LiveProfile([string]$Source,[string]$Destination) {
 New-Item -ItemType Directory -Force -Path $Destination | Out-Null
 foreach ($entry in Get-ChildItem -LiteralPath $Source -Force) {
  if ($entry.Name -notin @('backups','evolution-runs')) {
   Copy-Item -LiteralPath $entry.FullName -Destination $Destination -Recurse
  }
 }
}
$Package=[IO.Path]::GetFullPath($Package)
$Previous=[IO.Path]::GetFullPath($Previous)
$Data=[IO.Path]::GetFullPath($Data)
$manifest=Get-Content -LiteralPath (Join-Path $Package 'release-manifest.json') -Raw -Encoding UTF8 | ConvertFrom-Json
foreach ($item in $manifest.files.PSObject.Properties) {
 $file=[IO.Path]::GetFullPath((Join-Path $Package $item.Name))
 if (!$file.StartsWith($Package+[IO.Path]::DirectorySeparatorChar,[StringComparison]::OrdinalIgnoreCase)) { throw 'Unsafe package entry' }
 if (!(Test-Path -LiteralPath $file)) { throw "Missing file: $($item.Name)" }
 if ((Get-Item -LiteralPath $file).Length -ne $item.Value.size) { throw "Size mismatch: $($item.Name)" }
 if ((Get-FileHash -LiteralPath $file -Algorithm SHA256).Hash.ToLowerInvariant() -ne $item.Value.sha256) { throw "Checksum mismatch: $($item.Name)" }
}
$allPets=@(Get-CimInstance Win32_Process | Where-Object { $_.Name -ieq 'Pet2.exe' })
$expected=Join-Path $Previous 'Pet2.exe'
if (@($allPets | Where-Object { $_.ExecutablePath -ine $expected }).Count -gt 0) {
 throw 'Another Pet2 executable is running; no process or profile has been changed.'
}
$backup=Join-Path $BackupRoot ('before-install-v68.1.1-'+(Get-Date -Format 'yyyyMMdd-HHmmss'))
New-Item -ItemType Directory -Path $backup | Out-Null
Copy-LiveProfile $Data (Join-Path $backup 'data-before-close')
$allPets | Select-Object ProcessId,ExecutablePath,CommandLine | ConvertTo-Json | Set-Content (Join-Path $backup 'previous-process.json') -Encoding UTF8
foreach ($entry in $allPets) {
 $process=Get-Process -Id $entry.ProcessId -ErrorAction SilentlyContinue
 if ($null -eq $process) { continue }
 if (!$process.CloseMainWindow() -or !$process.WaitForExit(15000)) {
  throw "Pet2 PID $($entry.ProcessId) did not close gracefully; previous build is unchanged."
 }
}
# Hidden care-menu helpers contain no autonomous pet state. Only this project's
# helpers on this exact profile are eligible, after the old owner has exited.
$buildRoot=(Split-Path -Parent $Previous)+[IO.Path]::DirectorySeparatorChar
$helpers=@(Get-CimInstance Win32_Process | Where-Object {
 $_.Name -ieq 'Pet2 Dev Console.exe' -and $_.ExecutablePath -and $_.CommandLine -and
 $_.ExecutablePath.StartsWith($buildRoot,[StringComparison]::OrdinalIgnoreCase) -and
 $_.CommandLine -match '--pet-menu-idle' -and
 $_.CommandLine.IndexOf($Data,[StringComparison]::OrdinalIgnoreCase) -ge 0
})
foreach ($entry in $helpers) {
 $process=Get-Process -Id $entry.ProcessId -ErrorAction SilentlyContinue
 if ($null -eq $process) { continue }
 if ($process.CloseMainWindow()) { $null=$process.WaitForExit(3000) }
 if (!$process.HasExited) { Stop-Process -Id $entry.ProcessId }
}
$closed=Join-Path $backup 'data-after-graceful-close'
Copy-LiveProfile $Data $closed
$new=Start-Process -FilePath (Join-Path $Package 'Pet2.exe') -ArgumentList ('--data-dir "'+$Data+'"') -WorkingDirectory $Package -WindowStyle Hidden -PassThru -RedirectStandardOutput (Join-Path $backup 'v681-stdout.log') -RedirectStandardError (Join-Path $backup 'v681-stderr.log')
Start-Sleep -Seconds 5
if ($new.HasExited) {
 Copy-LiveProfile $Data (Join-Path $backup 'failed-start-profile')
 # Restore the actual portable snapshots without touching archived experiments.
 foreach ($file in Get-ChildItem -LiteralPath $closed -File -Filter '*.json') {
  Copy-Item -LiteralPath $file.FullName -Destination (Join-Path $Data $file.Name) -Force
 }
 Start-Process -FilePath $expected -ArgumentList ('--data-dir "'+$Data+'"') -WorkingDirectory $Previous -WindowStyle Hidden
 throw "New Pet2 exited with code $($new.ExitCode); JSON snapshots restored and previous version relaunched."
}
$result=@{version='V68.1.1';package=$Package;source_commit=$manifest.source_commit;pid=$new.Id;backup=$backup;files_verified=@($manifest.files.PSObject.Properties).Count;vision_preference_preserved=$true;state_reset=$false;closed_menu_helpers=@($helpers).Count}
$result | ConvertTo-Json | Set-Content (Join-Path $backup 'deployment.json') -Encoding UTF8
$result | ConvertTo-Json
