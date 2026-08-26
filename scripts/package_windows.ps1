param([string]$Target = 'x86_64-pc-windows-msvc')
$ErrorActionPreference = 'Stop'
$workspace = Split-Path -Parent $PSScriptRoot
$dist = Join-Path $workspace 'dist/Pet2-windows-x64'
$archive = Join-Path $workspace 'dist/Pet2-windows-x64.zip'
cargo build --manifest-path (Join-Path $workspace 'Cargo.toml') --workspace --release --target $Target
if ($LASTEXITCODE -ne 0) { throw "cargo build failed with exit code $LASTEXITCODE" }
New-Item -ItemType Directory -Force -Path $dist | Out-Null
New-Item -ItemType Directory -Force -Path (Join-Path $dist 'config') | Out-Null
New-Item -ItemType Directory -Force -Path (Join-Path $dist 'licenses') | Out-Null
Copy-Item -Force (Join-Path $workspace "target/$Target/release/pet2.exe") (Join-Path $dist 'Pet2.exe')
Copy-Item -Force (Join-Path $workspace "target/$Target/release/body_lab.exe") (Join-Path $dist 'BodyLab.exe')
Copy-Item -Force (Join-Path $workspace "target/$Target/release/habitat_lab.exe") (Join-Path $dist 'HabitatLab.exe')
Copy-Item -Force (Join-Path $workspace 'README.md') (Join-Path $dist 'README.txt')
Copy-Item -Force (Join-Path $workspace 'LICENSE') (Join-Path $dist 'licenses/LICENSE')
Copy-Item -Force (Join-Path $workspace 'config/default.json') (Join-Path $dist 'config/default.json')
Set-Content -Encoding ascii -Path (Join-Path $dist 'Pet2-Fusion.cmd') -Value @('@echo off', 'start "" "%~dp0Pet2.exe" --brain-mode fusion')
Set-Content -Encoding ascii -Path (Join-Path $dist 'Pet2-Morph-Shadow.cmd') -Value @('@echo off', 'start "" "%~dp0Pet2.exe" --brain-mode morph-shadow')
Set-Content -Encoding ascii -Path (Join-Path $dist 'Pet2-Morph-Fusion.cmd') -Value @('@echo off', 'start "" "%~dp0Pet2.exe" --brain-mode morph-fusion')
Set-Content -Encoding ascii -Path (Join-Path $dist 'HabitatLab-Flagship.cmd') -Value @('@echo off', '"%~dp0HabitatLab.exe" --scenario habitat_story_v1 --ticks 1600 --trace habitat_story_v1.json --screenshot habitat_story_v1.svg')
Compress-Archive -Force -Path (Join-Path $dist '*') -DestinationPath $archive
Write-Output $archive
