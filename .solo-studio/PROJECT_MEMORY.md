# Project Memory — Pet 2

Updated: 2026-08-21
Current gate: Vertical slice
Current build/commit: `main` at initial repository import; implementation uncommitted.

## Product truth

- Player promise: the same persistent procedural creature on Windows and macOS.
- Explicit cut line: stay offline, procedural, and bounded; no service or asset pipeline.

## Current golden path

- Launch: `cargo run -p pet2 -- --headless-smoke 10 --seed 42 --reset-pet --no-audio --data-dir target/smoke-state`.
- Scenario: deterministic state advances, procedural body/voice hashes are produced,
  then snapshot serialization and restoration are verified.
- Desktop: `cargo run -p pet2` creates a small transparent procedural overlay.
- Reset: launch with explicit `--reset-pet` or `--reset-learning`; state replacement
  retains a previous backup.

## Current decisive uncertainty

- The real AppKit/Quartz adapter cross-compiles for Apple Silicon, but interactive
  window, Metal, and CoreAudio behavior still requires a macOS runner or physical Mac.

## Accepted decisions

- Platform capability and coordinate contracts are owned by `desktop_host`.
- Portable domain state owns normalized position only; storage owns physical paths.
- `wgpu::Instance::default()` chooses D3D12/Metal without production env variables.
- Application time may schedule frames, but cannot seed simulation randomness.

## Current architecture truth

- Authoritative state: `lifecore::LifeCore` and its versioned snapshot DTO.
- Presentation: procedural mesh and voice definitions derived from the genome.
- Save: `desktop_host::storage::PortablePetState`, JSON fixture, atomic persistence.
- Platform: one `PlatformBackend` factory and capability-based degradation.
- Critical tests: deterministic snapshot replay, 24-hour simulation, coordinate remap,
  mesh/voice hashes, audio sample-rate/channel/format matrix, save/import roundtrip.
- Windows evidence: optimized binary launches a non-activating overlay; a live debug
  run remained responsive and persisted schema-v1 state on shutdown. Runtime testing
  found and fixed an invalid WGSL swizzle and added shader parse/validation to CI.
- Packaging evidence: PerMonitorV2/asInvoker manifest is embedded; Windows portable
  ZIP, reproducible ICO/ICNS assets, and macOS `.app` assembly scripts are present.

## Next executable slice

Outcome: validate and tune the complete v0.1 organism during all-day interactive use
on Windows and Apple Silicon without changing portable boundaries.

Definition of done: CI remains green, native overlay/audio behavior is exercised on
both platforms, and observed attention/sleep/development behavior meets the product
constraints without schema-invalidating changes.
