# Pet 2

Pet 2 is an offline, deterministic desktop lifeform built as one portable Rust
simulation with thin Windows and macOS host adapters. The pet body and voice are
generated from its genome at runtime; no model, texture, or recorded-audio assets
are required.

> Status: v0.1 living-desktop habitat slice. The repository implements the portable
> LifeCore, procedural body and voice, a persistent object ecology, multi-step
> behavior episodes, native overlay/sensor adapters, versioned persistence,
> deterministic Habitat Lab, packaging, and Windows/macOS CI. Windows and macOS
> share the same visual feature extraction, simulation, persistence, and tools;
> only native capture, overlay, input, audio, and packaging remain platform-specific.

## Supported targets

- Windows 10/11 x86_64 (`x86_64-pc-windows-msvc`): D3D12 through `wgpu`, WASAPI
  through `cpal`.
- macOS 13+ Apple Silicon (`aarch64-apple-darwin`): Metal through `wgpu`, CoreAudio
  through `cpal`.
- Other desktop targets compile through a reduced-capability portable backend; they
  are not advertised as fully supported.

## Quick start

```bash
cargo run -p pet2 -- --headless-smoke 60 --seed 42 --reset-pet --no-audio --data-dir target/smoke-state
cargo test --workspace
cargo run -p pet2
cargo run -p habitat_lab -- --scenario habitat_story_v1 --ticks 1600 --trace target/habitat.json --screenshot target/habitat.svg
```

The desktop runtime creates a small transparent always-on-top window, initializes
the renderer before showing it, and dynamically switches cursor hit-testing over
the procedural body. On a host without the required desktop capabilities, the
simulation and persistence continue to work.

## Portable state

```bash
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
tools/body_lab       liquid tuning plus the live brain, telemetry, and evolution monitor
tools/habitat_lab    developer-only deterministic scenario runner and SVG/JSON evidence
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

The macOS archive contains `Pet2.app`, `Body Lab.app`, `Habitat Lab.app`, the
deterministic `tools/HabitatLab` runner, and `Pet2-Dev.command`. Double-click the
latter to start Pet 2 with live telemetry and open Body Lab on its Live Brain
view. `Habitat Lab.app` is the macOS equivalent of Windows `Pet2-Dev.cmd`: it
restarts the installed Pet in bounded 5 Hz telemetry mode and keeps the Live
Brain panel open. That panel shows realtime desktop position, movement, final
gaze, visual-saliency target, cursor/orb context, decisions, drives, and visible
body response. Its attention map also renders the same privacy-safe `8x5`
brightness/color/motion approximation consumed by the organism, plus anonymous
window rectangles. It never exposes window titles, typed text, native window
identifiers, screenshots, or pixel buffers. The deterministic cross-platform
scenario runner remains in `tools/HabitatLab`.

Packaging leaves only the ZIP archive in `dist`; the installed applications are
therefore not duplicated in Spotlight or Launchpad by an unpacked build tree.
For privacy permission continuity across releases, package with an installed
Developer ID Application identity. The packaging script selects one
automatically when available, or accepts an explicit
`PET2_CODESIGN_IDENTITY="Developer ID Application: …"` override. Ad-hoc signing
remains the CI/local fallback but cannot promise stable TCC identity after
executable contents change.

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

The orb uses a `4 px` drag threshold: clicking does not snap or stop it, while
dragging preserves the grab offset, follows with bounded spring response, and
releases physical velocity. It falls under deterministic screen-height gravity,
bounces with authored restitution, loses tangential energy on floor contact, and
sleeps only after physically settling. Stationary windows only collide on a fresh
swept crossing, so a maximized window cannot expel an already embedded orb to a
screen edge; moving windows still transfer bounded impulse. Window stacking order
is preserved through perception and physics, so a covered lower-window edge cannot
become an invisible wall. Sustained opposing contacts—not mere inclusion in a
window bounding box—are required before the brain treats the orb as trapped.

The episode director can start solo play from endogenous play, curiosity, and
autonomy instead of waiting for a coincidental high-level action. It seeks a
distant orb, enters a contact-sized orbit, and authors alternating physical taps;
the interaction hull is large enough to reach an orb resting on the screen floor.
Navigation targets are projected onto the actual union of monitor rectangles, so
an L-shaped desktop cannot leave the pet pursuing an unreachable point in a gap.

Windows and macOS visual sensing perform one reduced `64x36` desktop capture and
derive the same privacy-safe `16x9` grid with `4x4` color/edge samples per cell.
The pet's own body
footprint is masked before temporal saliency, preventing self-attention lock.
Static saturated colors and structured monochrome shapes can claim attention on
their own, causing gaze/travel plus bounded hue, glow, flow, and cohesion changes.
A smaller visual reflex remains visible while safety motion owns locomotion.

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
- `Cmd/Win + Alt + D`: toggle bounded causal telemetry logging.

## Body Lab and live brain monitor

Both packages keep the same companion tool: `BodyLab.exe` on Windows and
`Body Lab.app` on macOS. Run `Pet2-Dev.cmd` or `Pet2-Dev.command` to start Pet 2
with 5 Hz causal telemetry and open Body Lab directly on its Live Brain view.
If a normal Pet 2 instance is already running, close it first because the
desktop organism is single-instance on both platforms.

Inside Body Lab, `F12` switches the same window between Liquid Body Lab and Live
Brain. In Live Brain, `Space`, the arrow keys, the timeline slider, and the
playback-rate buttons replay the captured state. This rewinds the observation,
not the running organism. The live view follows attention, decisions, drives,
affect, body and object motion, Morph/VITA/Fusion state, learning, identity, and
exact bounded mutation checkpoints.

Telemetry is written to an 8 MiB `telemetry.jsonl` plus one rotated previous
file. The older semantic `events.jsonl` is not truncated or rotated and no
longer receives high-rate debug frames; ordinary semantic events still append
there. Lab interventions use a closed, expiring command set. Attention cues and drive pulses are
temporary, while reward intentionally changes learned state. No screen captures
or pixel buffers, typed text, raw audio, or native window identifiers are added
to the new telemetry groups.

Use `cargo run -p pet2 -- --help` for deterministic seed, accelerated simulation,
portable import/export, reset, focus, audio, and isolated data-directory options.

## Platform notes

- Windows uses Win32 cursor, idle, foreground process, visible-window geometry,
  tool-window/no-activate styles, and a Per-Monitor-V2 manifest.
- macOS uses AppKit/Quartz cursor, idle and frontmost-application APIs, a floating
  all-spaces `NSWindow`, and a regular Dock presence so Pet 2 can be quit or
  relaunched like any other application. It requests the standard Screen
  Recording consent once for reduced visual sensing; denial keeps the organism
  running with that capability disabled. No Accessibility permission is needed.
  Retina and mixed-scale Quartz coordinates are converted at the platform edge.
- Audio-device loss is non-fatal. LifeCore, body motion, persistence, and silent
  operation continue while the output stream is recreated.

## Design constraints

- No network dependency at runtime.
- No Unity, Blender, FBX, glTF, HLSL, MSL, or recorded voice assets.
- All randomness flows through a serialized `ChaCha8Rng` state.
- `lifecore`, `pet_body`, and `pet_audio` contain no OS-specific APIs or `cfg` gates.
- Platform APIs live only under `crates/desktop_host/src/platform/`.

See `.solo-studio/PROJECT_MEMORY.md` for current evidence and remaining risks.
