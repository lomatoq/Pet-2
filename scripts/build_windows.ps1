param([string]$Target = 'x86_64-pc-windows-msvc')
$ErrorActionPreference = 'Stop'
$workspace = Split-Path -Parent $PSScriptRoot
cargo fmt --manifest-path (Join-Path $workspace 'Cargo.toml') --all --check
if ($LASTEXITCODE -ne 0) { throw "cargo fmt failed with exit code $LASTEXITCODE" }
cargo clippy --manifest-path (Join-Path $workspace 'Cargo.toml') --workspace --all-targets -- -D warnings
if ($LASTEXITCODE -ne 0) { throw "cargo clippy failed with exit code $LASTEXITCODE" }
cargo test --manifest-path (Join-Path $workspace 'Cargo.toml') --workspace
if ($LASTEXITCODE -ne 0) { throw "cargo test failed with exit code $LASTEXITCODE" }
cargo build --manifest-path (Join-Path $workspace 'Cargo.toml') -p pet2 --release --target $Target
if ($LASTEXITCODE -ne 0) { throw "cargo build failed with exit code $LASTEXITCODE" }
