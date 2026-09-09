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
if [[ -z "$codesign_identity" || "$codesign_identity" == "-" ]]; then
  if [[ "${PET2_ALLOW_ADHOC_SIGNING:-0}" != "1" ]]; then
    echo "error: no usable signing identity; refusing an ad-hoc application update" >&2
    echo "Use a session with Keychain access and a Developer ID Application identity," >&2
    echo "or set PET2_CODESIGN_IDENTITY explicitly. For disposable CI builds only," >&2
    echo "set PET2_ALLOW_ADHOC_SIGNING=1 (screen-recording grants will not survive updates)." >&2
    exit 1
  fi
  codesign_identity="-"
fi

dist_root="$workspace/dist"
payload="$package_root/$package_name"
pet_app="$payload/Pet2.app"
console_app="$payload/Pet2 Dev Console.app"

cargo build \
  --manifest-path "$workspace/Cargo.toml" \
  --release \
  --target "$target" \
  -p pet2 -p body_lab -p habitat_lab -p voice_lab

mkdir -p \
  "$pet_app/Contents/MacOS" "$pet_app/Contents/Resources" \
  "$console_app/Contents/MacOS" "$console_app/Contents/Resources" \
  "$payload/config" "$payload/licenses" "$payload/tools" \
  "$dist_root"

cp "$workspace/target/$target/release/pet2" "$pet_app/Contents/MacOS/Pet2"
cp "$workspace/assets/macos/Info.plist" "$pet_app/Contents/Info.plist"
cp "$workspace/config/default.json" "$pet_app/Contents/Resources/default.json"

cp "$workspace/assets/macos/DevConsoleLauncher" "$console_app/Contents/MacOS/Pet2DevConsole"
cp "$workspace/target/$target/release/body_lab" "$console_app/Contents/Resources/DevConsole"
cp "$workspace/assets/macos/DevConsole-Info.plist" "$console_app/Contents/Info.plist"

if [[ -f "$workspace/assets/macos/AppIcon.icns" ]]; then
  cp "$workspace/assets/macos/AppIcon.icns" "$pet_app/Contents/Resources/AppIcon.icns"
  cp "$workspace/assets/macos/AppIcon.icns" "$console_app/Contents/Resources/AppIcon.icns"
fi

cp -R "$workspace/config/evolution" "$payload/config/evolution"
cp -R "$workspace/config/embodiment" "$payload/config/embodiment"
cp "$workspace/target/$target/release/voice_lab" "$payload/tools/VoiceLab"
cp "$workspace/PET2_ORGANIC_VOICE_IMPLEMENTATION.md" "$payload/ORGANIC_VOICE.md"
cp "$workspace/target/$target/release/habitat_lab" "$payload/tools/HabitatLab"
cp "$workspace/assets/macos/Pet2-Dev.command" "$payload/Pet2-Dev.command"
cp "$workspace/README.md" "$payload/README.txt"
cp "$workspace/LICENSE" "$payload/licenses/LICENSE"
cp "$workspace/config/default.json" "$payload/config/default.json"
chmod +x \
  "$pet_app/Contents/MacOS/Pet2" \
  "$console_app/Contents/MacOS/Pet2DevConsole" \
  "$console_app/Contents/Resources/DevConsole" \
  "$payload/tools/HabitatLab" "$payload/tools/VoiceLab" \
  "$payload/Pet2-Dev.command"

if [[ "$codesign_identity" == "-" ]]; then
  echo "warning: ad-hoc signing does not preserve macOS privacy identity across changed builds" >&2
fi
codesign --force --deep --sign "$codesign_identity" "$pet_app"
codesign --force --deep --sign "$codesign_identity" "$console_app"

# Sign first so the manifest hashes the final executable bytes. Updating the
# resource seal below does not re-sign the nested DevConsole executable.
python3 - "$pet_app" "$console_app" "$workspace/Cargo.toml" <<'PY'
import hashlib, json, pathlib, re, sys
pet, console, cargo = map(pathlib.Path, sys.argv[1:])
def entry(path):
    data = path.read_bytes()
    return {"sha256": hashlib.sha256(data).hexdigest(), "size": len(data)}
version = re.search(r'^version\s*=\s*"([^"]+)"', cargo.read_text(), re.M).group(1)
manifest = {"schema": "pet2.release_manifest.v1", "release_version": version,
    "lab_control_protocol": 2, "compatible_pair": True,
    "files": {"pet": entry(pet / "Contents/MacOS/Pet2"),
              "lab": entry(console / "Contents/Resources/DevConsole")}}
(console / "Contents/Resources/release-manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
PY
codesign --force --sign "$codesign_identity" "$console_app"
codesign --verify --deep --strict "$pet_app"
codesign --verify --deep --strict "$console_app"

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
