$ErrorActionPreference = 'Stop'
$workspace = Split-Path -Parent $PSScriptRoot
$data = Join-Path $workspace 'target/ci-state'
$export = Join-Path $workspace 'target/ci-state-export.json'
cargo run --manifest-path (Join-Path $workspace 'Cargo.toml') -p pet2 -- --headless-smoke 10 --seed 42 --reset-pet --no-audio --data-dir $data --export-state $export
if ($LASTEXITCODE -ne 0) { throw "headless export failed with exit code $LASTEXITCODE" }
cargo run --manifest-path (Join-Path $workspace 'Cargo.toml') -p pet2 -- --headless-smoke 1 --import-state $export --no-audio --data-dir $data
if ($LASTEXITCODE -ne 0) { throw "headless import failed with exit code $LASTEXITCODE" }
