#!/usr/bin/env bash
set -euo pipefail
root="$(cd "$(dirname "$0")" && pwd)"
open "$root/Pet2.app" --args --dev-mode
sleep 1
open "$root/Body Lab.app" --args --live-pet
