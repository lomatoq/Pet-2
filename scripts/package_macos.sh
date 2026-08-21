#!/usr/bin/env bash
set -euo pipefail
workspace="$(cd "$(dirname "$0")/.." && pwd)"
target="aarch64-apple-darwin"
package_root="$(mktemp -d "${TMPDIR:-/tmp}/pet2-package.XXXXXX")"
trap 'case "$package_root" in *pet2-package.*) rm -rf "$package_root" ;; esac' EXIT
app="$package_root/Pet2.app"
dist_app="$workspace/dist/Pet2.app"
cargo build --manifest-path "$workspace/Cargo.toml" -p pet2 --release --target "$target"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources" "$workspace/dist"
cp "$workspace/target/$target/release/pet2" "$app/Contents/MacOS/Pet2"
cp "$workspace/assets/macos/Info.plist" "$app/Contents/Info.plist"
cp "$workspace/README.md" "$app/Contents/Resources/README.txt"
cp "$workspace/LICENSE" "$app/Contents/Resources/LICENSE"
cp "$workspace/config/default.json" "$app/Contents/Resources/default.json"
if [[ -f "$workspace/assets/macos/AppIcon.icns" ]]; then
  cp "$workspace/assets/macos/AppIcon.icns" "$app/Contents/Resources/AppIcon.icns"
fi
chmod +x "$app/Contents/MacOS/Pet2"
ditto "$app" "$dist_app"
ditto -c -k --sequesterRsrc --keepParent "$app" "$workspace/dist/Pet2-macos-arm64.zip"
printf '%s\n' "$workspace/dist/Pet2-macos-arm64.zip"
