param([string]$Target = 'x86_64-pc-windows-msvc')
$ErrorActionPreference = 'Stop'
$workspace = Split-Path -Parent $PSScriptRoot
$dist = Join-Path $workspace 'dist/Pet2-windows-x64'
$archive = Join-Path $workspace 'dist/Pet2-windows-x64.zip'
cargo build --manifest-path (Join-Path $workspace 'Cargo.toml') -p pet2 --release --target $Target
if ($LASTEXITCODE -ne 0) { throw "cargo build failed with exit code $LASTEXITCODE" }
New-Item -ItemType Directory -Force -Path $dist | Out-Null
New-Item -ItemType Directory -Force -Path (Join-Path $dist 'config') | Out-Null
New-Item -ItemType Directory -Force -Path (Join-Path $dist 'licenses') | Out-Null
Copy-Item -Force (Join-Path $workspace "target/$Target/release/pet2.exe") (Join-Path $dist 'Pet2.exe')
Copy-Item -Force (Join-Path $workspace 'README.md') (Join-Path $dist 'README.txt')
Copy-Item -Force (Join-Path $workspace 'LICENSE') (Join-Path $dist 'licenses/LICENSE')
Copy-Item -Force (Join-Path $workspace 'config/default.json') (Join-Path $dist 'config/default.json')
Compress-Archive -Force -Path (Join-Path $dist '*') -DestinationPath $archive
Write-Output $archive
