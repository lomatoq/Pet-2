# PET 2 Living Language Implementation Report

Date: 2026-09-01
Base SHA: `644a900a5de0acdc76b20ab2a3a45ccd2779a89a`
Final SHA: `HEAD` — the third commit containing this report; resolve with `git rev-parse HEAD`
Branch: `codex/pet2-embodied-pointer-language`

## Outcome

The implementation is complete as three logical commits:

1. `b0ee35780628bbe7f747ff70f85b082ddc3d5e42` — accelerated-learning execution, live progress, offline audio receipts, and promotion handshake.
2. `98a2a23cafc4ff38e9a0b112dad3b0a50fab39e9` — coordinated transient expression language, one vocal arbiter, social intents, and mammalian source-filter voice gestures.
3. `HEAD` — bounded vocal lexicon, minimal world model, world-aware gaze, production renderer capture, and the final evidence set.

The active Pet acknowledged the promoted learned snapshot:

- status: `running`
- PID at acceptance: `45620`
- loaded Life hash: `75bf0ec73025e44c`
- loaded genome hash: `65f1c4cea68d6359`
- executable: `target/debug/Pet2.exe`, version `0.1.0`
- recovery backup was created before the atomic promotion

## Why the previous build looked unchanged

The “zero difference” report was valid. Four separate gaps masked the work:

- Body Lab could resolve a stale `builds/current` executable instead of the packaged sibling or matching workspace target, so accelerated learning could appear not to start.
- The UI had no durable progress contract, executable identity, PID, hash, or failure tail; a real run and a failed launch looked alike.
- Offline evolution counted requested sound events without exporting real PCM/WAV evidence, while the live path still used direct touch-triggered voice and the legacy bright oscillator range.
- Physical expression held for about `0.25 s`, gaze had no procedural-object identity, and the old learning rule changed only a tiny rendition band, so body, flight, and learned voice differences were hard to perceive.

## What changed

### Accelerated learning and promotion

- Body Lab now prefers a packaged sibling, then matching `target/debug` or `target/release` executables; stale `builds/current` cannot override them.
- `EvolutionProgress` is atomically written during execution and exposes stage, replicate, episode, simulated time, audio render count, learning update count, ETA, and the last bounded error.
- The UI displays executable path, version, SHA-256, PID, progress, dry-run status, and error-log tail.
- The evolution runner exports deterministic real WAV files and audio measurements. `--no-audio-output` disables device playback without disabling offline synthesis.
- `--promote-report` validates report schema/status/invariants/fork bounds, gracefully stops the live Pet, waits for the old Windows PID to release its singleton, saves a recovery bundle, promotes atomically, restarts, and requires an exact loaded-hash acknowledgement. Failure restores the backup.
- The Life hash is storage-canonical: it includes one serde round trip, eliminating the RAM-vs-loaded JSON hash mismatch found during end-to-end validation.

### Coordinated social expression

- `ExpressionDirector` converts one interaction reason plus affect, temperament, physical load, audience, and world state into one coordinated phrase.
- `VocalArbiter` is the single gate. Quiet/focus suppression wins, direct touch enqueue is removed, and legacy behavior remains behind `legacy-expression-fallback`.
- Social intent classes are `contact`, `effort`, `alarm`, `boundary`, `relief`, `invite`, `query`, `notice`, and `quiet`.
- Gaze, eyes/mouth, body pulse/lean/recoil/resistance/cooperation, timing, and voice gesture share one causal plan.
- Expression onset is bounded below `200 ms`; authored hold is `0.8–2.0 s`.

### Mammalian voice and living lexicon

- Living voice uses bounded glottal source/filter synthesis, formants, aspiration, nonlinear events, and short room response.
- Living F0 is constrained to `65–620 Hz`; the old fallback remains separately available and is not silently mixed into the default path.
- Gestures include warm chuff, low rumble, mew/whine, clipped pulse, and relief exhale.
- Semantic unit pacing is `2.05–2.90 units/s`; production evidence measured `2.232–2.737 units/s`.
- The persistent lexicon has nine fixed semantic entries. Learning changes bounded timing/rhythm confidence, not arbitrary timbre identity; the old ±6% interaction-variant selection is retained only for migration compatibility.

### World-aware gaze and embodiment

- `WorldModelFrame` is a fixed-capacity, allocation-free observation set for cursor, viewer, contact point, body components, visible surfaces, and procedural ecology objects.
- Objects carry stable id, kind, bbox/position/velocity, confidence, novelty, familiarity, salience, affordances, and last-seen time.
- Query/notice phrases can point gaze at an actual observed `WorldEntity`; no fake target is invented.
- Physical load, topology use, detached fraction, strain, deformation, speed, acceleration, contact, and airborne state shape expression and body actuation.
- Bold and shy fixtures use the same motif identity but differ in onset, hold, gaze, lean, and amplitude.

## Migration and compatibility

- `LIFE_SNAPSHOT_SCHEMA_VERSION`: `2 → 3`.
- Readers accept schemas `1`, `2`, and `3`; missing `vocal_lexicon` data receives bounded defaults.
- Portable state remains schema `1`; evolution report remains schema `2`.
- Process-owned pending vocal delivery/credit is cleared on restore and never fabricated as heard user feedback.
- `cargo check -p pet2 --no-default-features --features legacy-expression-fallback` passes.

## Production perceptual evidence

Artifact root: `artifacts/living-language-smoke-definitive/`

- active body profile: `Black copy r11`
- renderer: exact `pet_body::Renderer` production GPU pipeline
- body/physics: actual `ProceduralBody` and current liquid tuning
- world objects: actual `EcologyRenderer` fixtures
- fixtures: `13/13`
- WAV: `26/26`, all SHA-256 hashes unique
- MP4: `26/26`, all SHA-256 hashes unique
- traces: `13/13`
- every MP4: H.264, `512×288`, `yuv420p`, `24 fps`, `1.500 s`
- recoveries: `0`

| Fixture | Intent | Voice gesture | Units/s | Onset s | Hold s | Gaze |
|---|---|---:|---:|---:|---:|---|
| soft touch | contact | warm chuff | 2.243 | 0.074 | 1.235 | viewer |
| calm stroke | contact | warm chuff | 2.232 | 0.089 | 1.001 | contact point |
| slow pull | effort | low rumble | 2.372 | 0.082 | 1.183 | contact point |
| sharp flick | alarm | clipped pulse | 2.737 | 0.089 | 1.178 | cursor |
| prolonged hold | boundary | low rumble | 2.293 | 0.085 | 1.340 | away |
| fragment separation | alarm | clipped pulse | 2.414 | 0.087 | 1.294 | cursor |
| remerge | relief | relief exhale | 2.320 | 0.080 | 1.139 | viewer |
| playful flight | invite | mew/whine | 2.604 | 0.074 | 1.233 | viewer |
| hard braking | effort | low rumble | 2.471 | 0.083 | 1.100 | cursor |
| new moving object | query | warm chuff | 2.262 | 0.089 | 0.958 | world entity `186566400` |
| object-oriented query | query | mew/whine | 2.373 | 0.081 | 1.086 | world entity `186566400` |
| ignored social bid | invite | mew/whine | 2.351 | 0.089 | 0.967 | cursor |
| successful user response | relief | relief exhale | 2.444 | 0.074 | 1.229 | viewer |

Aggregate before/after measurements:

| Metric | Legacy baseline | Living language |
|---|---:|---:|
| semantic unit rate mean | 4.974/s | 2.393/s |
| zero-crossing rate mean | 0.017649 | 0.011882 |
| spectral centroid mean | 690.44 Hz | 675.00 Hz |
| roughness index mean | 0.133829 | 0.103752 |
| expression hold mean | 0.250 s | 1.149 s |
| expression amplitude mean | 0.460 | 0.884 |
| maximum living onset | — | 0.089 s |
| living hold range | — | 0.958–1.340 s |

Bold/shy identity check:

- same motif id: `15053844046573283599`
- bold: viewer gaze, lean `+0.016245`, amplitude `0.8563704`, onset `0.07424 s`, hold `1.23506 s`
- shy: contact gaze, lean `-0.0039600004`, amplitude `0.798484`, onset `0.08892799 s`, hold `1.00144 s`

Evidence stills:

- `artifacts/living-language-smoke-definitive/evidence/soft-touch-baseline-vs-living.png`
- `artifacts/living-language-smoke-definitive/evidence/physical-and-world-language-grid.png`

The stills were visually inspected and show the current round black/glass body, not the rejected temporary CPU-raster body. The physical grid covers fragment deformation, playful flight, a moving ecology object, and object-oriented query gaze.

## Verification commands

Passed:

```text
cargo fmt --all -- --check
cargo check --workspace --all-targets --all-features
cargo test --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo check -p pet2 --no-default-features --features legacy-expression-fallback
target/debug/pet2.exe --evolution-config config/evolution/accelerated-smoke.json ... --evolution-persist fork --no-audio-output
target/debug/body_lab.exe --data-dir artifacts/promotion-e2e3/live --promote-report artifacts/promotion-e2e3/report.json
target/debug/living_language_smoke.exe --output artifacts/living-language-smoke-definitive
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/package_windows.ps1
dist/Pet2-windows-x64/Pet2.exe --help
dist/Pet2-windows-x64/BodyLab.exe --help
```

Workspace test result: `443 passed, 0 failed, 4 ignored` (the ignored cases are explicit long acceptance/fixture-regeneration tests).
Package: `dist/Pet2-windows-x64.zip`
Package SHA-256: `D9A59C972D078588C87700BA4FEA4DC61702EA848177E6BCA0CEDDB6C080FFDB`

Accelerated smoke receipt:

- status `completed`
- 2/2 deterministic episodes
- 3 real WAV exports
- 1 bounded lexicon update
- 180 telemetry samples
- 0 invariant failures
- 0 emergency recoveries
- final/persisted Life hash `75bf0ec73025e44c`

## Changed files

```text
.gitignore
Cargo.lock
app/Cargo.toml
app/src/evolution_runner.rs
app/src/main.rs
app/src/vita_runtime.rs
config/evolution/accelerated-smoke.json
crates/desktop_host/src/evolution_control.rs
crates/desktop_host/src/lab_control.rs
crates/desktop_host/src/storage.rs
crates/lifecore/src/actions.rs
crates/lifecore/src/interaction.rs
crates/lifecore/src/language.rs
crates/lifecore/src/lib.rs
crates/lifecore/src/persistence.rs
crates/pet_audio/Cargo.toml
crates/pet_audio/src/engine.rs
crates/pet_audio/src/motif.rs
crates/pet_audio/src/mutation.rs
crates/pet_body/src/lib.rs
crates/pet_body/src/renderer.rs
reports/PET2_LIVING_LANGUAGE_IMPLEMENTATION_REPORT.md
reports/PET2_VISIBLE_LEARNING_GAP_DIAGNOSIS.md
tools/body_lab/Cargo.toml
tools/body_lab/src/bin/living_language_smoke.rs
tools/body_lab/src/main.rs
```

## Honest limitations

- No independent blinded human panel was run, so no claim is made that five of six users can identify every intent class.
- The evidence audio is deterministic offline synthesis, not a microphone loopback recording from a particular speaker/device. The device playback path is covered by existing ownership and callback tests.
- The minimal world model uses existing bounded desktop/ecology observations. A slow VLM semantic-label path is not implemented in this change.
- The 13 videos are deterministic 1.5-second acceptance captures, not a longitudinal usability study.
