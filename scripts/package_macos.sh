#!/usr/bin/env bash
set -euo pipefail

workspace="$(cd "$(dirname "$0")/.." && pwd)"
target="${PET2_MACOS_TARGET:-aarch64-apple-darwin}"
case "${target%%-*}" in
  aarch64)
    package_name="Pet2-macos-arm64"
    ;;
  x86_64)
    package_name="Pet2-macos-x64"
    ;;
  *) echo "unsupported macOS target: $target" >&2; exit 1 ;;
esac
package_root="$(mktemp -d "${TMPDIR:-/tmp}/pet2-package.XXXXXX")"
trap 'case "$package_root" in *pet2-package.*) rm -rf "$package_root" ;; esac' EXIT
codesign_identity="${PET2_CODESIGN_IDENTITY:-}"
if [[ -z "$codesign_identity" ]]; then
  codesign_identity="$(
    security find-identity -v -p codesigning 2>/dev/null \
      | sed -n 's/^[[:space:]]*[0-9]*) \([0-9A-F]\{40\}\) "Developer ID Application:.*$/\1/p' \
      | head -n 1
  )"
fi
codesign_identity="${codesign_identity:--}"

dist_root="$workspace/dist"
payload="$package_root/$package_name"
pet_app="$payload/Pet2.app"
lab_app="$payload/Body Lab.app"
habitat_app="$payload/Habitat Lab.app"

cargo build \
  --manifest-path "$workspace/Cargo.toml" \
  --release \
  --target "$target" \
  -p pet2 -p body_lab -p habitat_lab

mkdir -p \
  "$pet_app/Contents/MacOS" "$pet_app/Contents/Resources" \
  "$lab_app/Contents/MacOS" "$lab_app/Contents/Resources" \
  "$habitat_app/Contents/MacOS" "$habitat_app/Contents/Resources" \
  "$payload/config" "$payload/licenses" "$payload/tools" \
  "$dist_root"

cp "$workspace/target/$target/release/pet2" "$pet_app/Contents/MacOS/Pet2"
cp "$workspace/assets/macos/Info.plist" "$pet_app/Contents/Info.plist"
cp "$workspace/config/default.json" "$pet_app/Contents/Resources/default.json"

cp "$workspace/target/$target/release/body_lab" "$lab_app/Contents/MacOS/BodyLab"
cp "$workspace/assets/macos/BodyLab-Info.plist" "$lab_app/Contents/Info.plist"

cp "$workspace/assets/macos/HabitatLabLauncher" "$habitat_app/Contents/MacOS/HabitatLab"
cp "$workspace/target/$target/release/body_lab" "$habitat_app/Contents/Resources/BodyLabMonitor"
cp "$workspace/assets/macos/HabitatLab-Info.plist" "$habitat_app/Contents/Info.plist"

if [[ -f "$workspace/assets/macos/AppIcon.icns" ]]; then
  cp "$workspace/assets/macos/AppIcon.icns" "$pet_app/Contents/Resources/AppIcon.icns"
  cp "$workspace/assets/macos/AppIcon.icns" "$lab_app/Contents/Resources/AppIcon.icns"
  cp "$workspace/assets/macos/AppIcon.icns" "$habitat_app/Contents/Resources/AppIcon.icns"
fi

cp "$workspace/target/$target/release/habitat_lab" "$payload/tools/HabitatLab"
cp "$workspace/assets/macos/Pet2-Dev.command" "$payload/Pet2-Dev.command"
cp "$workspace/README.md" "$payload/README.txt"
cp "$workspace/LICENSE" "$payload/licenses/LICENSE"
cp "$workspace/config/default.json" "$payload/config/default.json"
chmod +x \
  "$pet_app/Contents/MacOS/Pet2" \
  "$lab_app/Contents/MacOS/BodyLab" \
  "$habitat_app/Contents/MacOS/HabitatLab" \
  "$habitat_app/Contents/Resources/BodyLabMonitor" \
  "$payload/tools/HabitatLab" \
  "$payload/Pet2-Dev.command"

if [[ "$codesign_identity" == "-" ]]; then
  echo "warning: ad-hoc signing does not preserve macOS privacy identity across changed builds" >&2
fi
codesign --force --deep --sign "$codesign_identity" "$pet_app"
codesign --force --deep --sign "$codesign_identity" "$lab_app"
codesign --force --deep --sign "$codesign_identity" "$habitat_app"

dist_payload="$dist_root/$package_name"
archive="$dist_root/$package_name.zip"
if [[ -e "$dist_payload" ]]; then
  case "$dist_payload" in
    "$dist_root"/Pet2-macos-*) rm -rf "$dist_payload" ;;
    *) echo "refusing to replace unexpected path: $dist_payload" >&2; exit 1 ;;
  esac
fi
ditto -c -k --sequesterRsrc --keepParent "$payload" "$archive"
printf '%s\n' "$archive"
