#!/usr/bin/env bash
set -euo pipefail
workspace="$(cd "$(dirname "$0")/.." && pwd)"
data="$workspace/target/ci-state"
exported="$workspace/target/ci-state-export.json"
cargo run --manifest-path "$workspace/Cargo.toml" -p pet2 -- --headless-smoke 10 --seed 42 --reset-pet --no-audio --data-dir "$data" --export-state "$exported"
cargo run --manifest-path "$workspace/Cargo.toml" -p pet2 -- --headless-smoke 1 --import-state "$exported" --no-audio --data-dir "$data"
