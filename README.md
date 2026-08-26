# Pet 2

Pet 2 is an offline, deterministic desktop lifeform built as one portable Rust
simulation with thin Windows and macOS host adapters. The pet body and voice are
generated from its genome at runtime; no model, texture, or recorded-audio assets
are required.

> Status: v0.1 living-desktop habitat slice. The repository implements the portable
> LifeCore, procedural body and voice, a persistent object ecology, multi-step
> behavior episodes, native overlay/sensor adapters, versioned persistence,
> deterministic Habitat Lab, packaging, and Windows/macOS CI. Interactive macOS
> behavior still needs hands-on validation on Apple Silicon hardware.

## Supported targets

- Windows 10/11 x86_64 (`x86_64-pc-windows-msvc`): D3D12 through `wgpu`, WASAPI
  through `cpal`.
- macOS 13+ Apple Silicon (`aarch64-apple-darwin`): Metal through `wgpu`, CoreAudio
  through `cpal`.
- Other desktop targets compile through a reduced-capability portable backend; they
  are not advertised as fully supported.

## Quick start

```powershell
cargo run -p pet2 -- --headless-smoke 10 --seed 42 --reset-pet --no-audio --data-dir target/smoke-state
cargo test --workspace
cargo run -p pet2
cargo run -p habitat_lab -- --scenario habitat_story_v1 --ticks 1600 --trace target/habitat.json --screenshot target/habitat.svg
```

The desktop runtime creates a small transparent always-on-top window, initializes
the renderer before showing it, and dynamically switches cursor hit-testing over
the procedural body. On a host without the required desktop capabilities, the
simulation and persistence continue to work.

## Portable state

```powershell
cargo run -p pet2 -- --export-state pet2-state.json --seed 42 --reset-pet
cargo run -p pet2 -- --import-state pet2-state.json
```

The canonical runtime state is stored with the OS directory abstraction:

- Windows: Local Application Data / `Pet2`
- macOS: Application Support / `Pet2`

Snapshots contain normalized monitor position and portable domain data only. Native
window handles, process identifiers, executable paths, device handles, graphics
adapter identifiers, and absolute data paths are never serialized.

## Workspace

```text
crates/lifecore      deterministic state and snapshot contract
crates/pet_body      procedural mesh, hit shape, and wgpu renderer
crates/pet_audio     procedural voice and device-independent offline synthesis
crates/pet_ecology   portable objects, den, metabolism, skills, physics, and episodes
crates/pet_perception reduced window, gesture, rhythm, and 16x9 visual affordances
crates/desktop_host  platform contract, sensors, coordinates, overlay, storage
app                  desktop runtime and state import/export CLI
tools/habitat_lab    deterministic ecology scenario runner and SVG/JSON evidence
```

Build and package entry points live in `scripts/`. CI runs formatting, Clippy,
tests, release builds, state export/import smoke simulation, and packaging on
Windows x64 and macOS Apple Silicon.

```powershell
./scripts/build_windows.ps1
./scripts/package_windows.ps1
```

```bash
bash ./scripts/build_macos.sh
bash ./scripts/package_macos.sh
```

The checked-in ICO and ICNS packaging assets are reproducible with
`scripts/generate_icons.ps1`; they are not loaded by the organism at runtime.

## Living habitat and interaction

LifeCore runs at 20 Hz and combines eight homeostatic drives, continuous affect,
a deterministic 64-neuron sparse CTRNN, bounded reward-modulated plasticity,
contextual action arbitration over 24 actions, episodic memory, attention budget,
sleep consolidation, habits, development, and bounded metamorphosis. Body physics
runs at 120 Hz; rendering runs at 60 FPS while active and 15 FPS while sleeping.

The habitat adds one canonical persistent orb, a den with storage, temporary
edible light morsels, reduced spatial attention, window pressure/contact, and a
bounded mimesis library. It persists separately in `ecology-state.json` with an
atomic previous snapshot. It stores no pixels, typed text, audio, window titles,
native handles, or OS object identifiers.

The overlay is click-through outside the projected procedural silhouette and
interactive habitat objects. These in-window controls never install global
keyboard hooks:

- `Cmd/Win + Alt + F`: spawn one edible light morsel at the cursor.
- `Cmd/Win + Alt + P`: toggle focus mode and route PET home.
- `Cmd/Win + Alt + L`: cue shared attention at the cursor.
- `Cmd/Win + Alt + T`: start/finish teaching a bounded pointer trajectory.
- `Cmd/Win + Alt + M`: trigger bounded metamorphosis.
- `Cmd/Win + Alt + B`: cycle Morphic, Fusion, Morph Shadow, Morph Fusion, and Classic brain modes.
- `Cmd/Win + Alt + R`: apply positive debug reward.
- `Cmd/Win + Alt + N`: apply negative debug reward.
- `Cmd/Win + Alt + D`: append a debug snapshot to `events.jsonl`.

Use `cargo run -p pet2 -- --help` for deterministic seed, accelerated simulation,
portable import/export, reset, focus, audio, and isolated data-directory options.

## Platform notes

- Windows uses Win32 cursor, idle, foreground process, visible-window geometry,
  tool-window/no-activate styles, and a Per-Monitor-V2 manifest.
- macOS uses AppKit/Quartz cursor, idle and frontmost-application APIs, a floating
  all-spaces `NSWindow`, and `LSUIElement=true`. It requests no Accessibility or
  Screen Recording entitlement; window-geometry sensing is capability-disabled
  when it cannot be obtained without permissions.
- Audio-device loss is non-fatal. LifeCore, body motion, persistence, and silent
  operation continue while the output stream is recreated.

## Design constraints

- No network dependency at runtime.
- No Unity, Blender, FBX, glTF, HLSL, MSL, or recorded voice assets.
- All randomness flows through a serialized `ChaCha8Rng` state.
- `lifecore`, `pet_body`, and `pet_audio` contain no OS-specific APIs or `cfg` gates.
- Platform APIs live only under `crates/desktop_host/src/platform/`.

See `.solo-studio/PROJECT_MEMORY.md` for current evidence and remaining risks.
