param([string]$Target = 'x86_64-pc-windows-msvc')
$ErrorActionPreference = 'Stop'
$workspace = [IO.Path]::GetFullPath((Split-Path -Parent $PSScriptRoot))
$distRoot = [IO.Path]::GetFullPath((Join-Path $workspace 'dist'))
$dist = [IO.Path]::GetFullPath((Join-Path $distRoot 'Pet2-windows-x64'))
$archive = Join-Path $workspace 'dist/Pet2-windows-x64.zip'
$expectedPrefix = $distRoot.TrimEnd([IO.Path]::DirectorySeparatorChar) + [IO.Path]::DirectorySeparatorChar
if (-not $dist.StartsWith($expectedPrefix, [StringComparison]::OrdinalIgnoreCase)) {
    throw "refusing to clean package path outside $distRoot"
}
cargo build --manifest-path (Join-Path $workspace 'Cargo.toml') --release --target $Target -p pet2 -p body_lab -p habitat_lab
if ($LASTEXITCODE -ne 0) { throw "cargo build failed with exit code $LASTEXITCODE" }
if (Test-Path -LiteralPath $dist) {
    Remove-Item -LiteralPath $dist -Recurse -Force
}
New-Item -ItemType Directory -Force -Path $dist | Out-Null
New-Item -ItemType Directory -Force -Path (Join-Path $dist 'config') | Out-Null
New-Item -ItemType Directory -Force -Path (Join-Path $dist 'licenses') | Out-Null
New-Item -ItemType Directory -Force -Path (Join-Path $dist 'tools') | Out-Null
Copy-Item -Force (Join-Path $workspace "target/$Target/release/pet2.exe") (Join-Path $dist 'Pet2.exe')
Copy-Item -Force (Join-Path $workspace "target/$Target/release/body_lab.exe") (Join-Path $dist 'Pet2 Dev Console.exe')
Copy-Item -Force (Join-Path $workspace "target/$Target/release/habitat_lab.exe") (Join-Path $dist 'tools/HabitatLab.exe')
Copy-Item -Force (Join-Path $workspace 'README.md') (Join-Path $dist 'README.txt')
Copy-Item -Force (Join-Path $workspace 'LICENSE') (Join-Path $dist 'licenses/LICENSE')
Copy-Item -Force (Join-Path $workspace 'config/default.json') (Join-Path $dist 'config/default.json')
Set-Content -Encoding ascii -Path (Join-Path $dist 'Pet2-Dev.cmd') -Value @('@echo off', 'start "" "%~dp0Pet2.exe" --dev-mode', 'start "" "%~dp0Pet2 Dev Console.exe" --live-pet')
Compress-Archive -Force -Path (Join-Path $dist '*') -DestinationPath $archive
Write-Output $archive
