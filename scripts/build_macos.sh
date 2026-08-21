#!/usr/bin/env bash
set -euo pipefail
workspace="$(cd "$(dirname "$0")/.." && pwd)"
target="aarch64-apple-darwin"
export MACOSX_DEPLOYMENT_TARGET=13.0
cargo fmt --manifest-path "$workspace/Cargo.toml" --all --check
cargo clippy --manifest-path "$workspace/Cargo.toml" --workspace --all-targets -- -D warnings
cargo test --manifest-path "$workspace/Cargo.toml" --workspace
cargo build --manifest-path "$workspace/Cargo.toml" -p pet2 --release --target "$target"
