param(
    [Parameter(Mandatory=$true)][string]$Package,
    [Parameter(Mandatory=$true)][string]$Previous,
    [Parameter(Mandatory=$true)][string]$Data,
    [Parameter(Mandatory=$true)][string]$BackupRoot
)
$ErrorActionPreference='Stop'
[Console]::OutputEncoding=[Text.UTF8Encoding]::new()
$manifest=Get-Content -LiteralPath (Join-Path $Package 'release-manifest.json') -Raw -Encoding UTF8 | ConvertFrom-Json
foreach ($item in $manifest.files.PSObject.Properties) {
    $file=Join-Path $Package $item.Name
    if (!(Test-Path -LiteralPath $file)) { throw "Missing package file: $($item.Name)" }
    if ((Get-Item -LiteralPath $file).Length -ne $item.Value.size) { throw "Package size mismatch: $($item.Name)" }
    if ((Get-FileHash -LiteralPath $file -Algorithm SHA256).Hash.ToLowerInvariant() -ne $item.Value.sha256) {
        throw "Package checksum mismatch: $($item.Name)"
    }
}
$stamp=Get-Date -Format 'yyyyMMdd-HHmmss'
$backup=Join-Path $BackupRoot ('before-install-'+$stamp)
New-Item -ItemType Directory -Force -Path $backup | Out-Null
Copy-Item -LiteralPath $Data -Destination (Join-Path $backup 'data-before-close') -Recurse
$before=Get-CimInstance Win32_Process | Where-Object {
    $_.Name -ieq 'Pet2.exe' -and $_.ExecutablePath -ieq (Join-Path $Previous 'Pet2.exe')
}
$before | Select-Object ProcessId,ExecutablePath,CommandLine | ConvertTo-Json | Set-Content (Join-Path $backup 'previous-process.json') -Encoding UTF8
foreach ($entry in $before) {
    $process=Get-Process -Id $entry.ProcessId -ErrorAction SilentlyContinue
    if ($null -eq $process) { continue }
    $requested=$process.CloseMainWindow()
    if (!$requested -or !$process.WaitForExit(15000)) {
        throw "Pet2 PID $($entry.ProcessId) did not close gracefully; old build and backup retained."
    }
}
# Menu helpers have no autonomous pet state; close only helpers belonging to the
# same project's versioned builds, after their old owner has already exited.
$buildRoot=Split-Path -Parent $Previous
$helpers=Get-CimInstance Win32_Process | Where-Object {
    $_.Name -ieq 'Pet2 Dev Console.exe' -and $_.ExecutablePath -and $_.CommandLine -and
    $_.ExecutablePath.StartsWith($buildRoot,[StringComparison]::OrdinalIgnoreCase) -and
    $_.CommandLine -match '--pet-menu-idle' -and
    $_.CommandLine.IndexOf($Data,[StringComparison]::OrdinalIgnoreCase) -ge 0
}
foreach ($entry in $helpers) {
    $process=Get-Process -Id $entry.ProcessId -ErrorAction SilentlyContinue
    if ($null -eq $process) { continue }
    if ($process.CloseMainWindow()) { $null=$process.WaitForExit(3000) }
    if (!$process.HasExited) { Stop-Process -Id $entry.ProcessId }
}
Copy-Item -LiteralPath $Data -Destination (Join-Path $backup 'data-after-graceful-close') -Recurse
$exe=Join-Path $Package 'Pet2.exe'
$arguments='--data-dir "'+$Data+'" --local-vision'
$new=Start-Process -FilePath $exe -ArgumentList $arguments -WorkingDirectory $Package -PassThru -RedirectStandardOutput (Join-Path $backup 'v68-stdout.log') -RedirectStandardError (Join-Path $backup 'v68-stderr.log')
Start-Sleep -Seconds 3
if ($new.HasExited) {
    Start-Process -FilePath (Join-Path $Previous 'Pet2.exe') -ArgumentList ('--data-dir "'+$Data+'"') -WorkingDirectory $Previous
    throw "New Pet2 exited with code $($new.ExitCode); previous build relaunched."
}
$result=@{version='V68';package=$Package;source_commit=$manifest.source_commit;pid=$new.Id;backup=$backup;files_verified=@($manifest.files.PSObject.Properties).Count;launched_local_vision=$true;state_reset=$false}
$result | ConvertTo-Json | Set-Content (Join-Path $backup 'deployment.json') -Encoding UTF8
$result | ConvertTo-Json
