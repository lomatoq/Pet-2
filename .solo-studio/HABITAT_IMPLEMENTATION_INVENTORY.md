# PET-2 Living Desktop Habitat — implementation inventory

Date: 2026-08-26
Status: pre-production baseline
Plan: `PET2_LIVING_DESKTOP_HABITAT_CODEX_PLAN.md` supplied outside the repository

## Repository truth

- Repository: `lomatoq/Pet-2`.
- Actual branch before habitat work: `codex/pet2-vita-embodied-iteration`.
- Actual HEAD: `3504403462d37ce6e0199c12f83ee32ab6ba0a62` (`feat: ship liquid pet lab and Morph brain fusion`).
- Requested habitat branch: `codex/pet2-living-desktop-habitat`.
- Target branch did not exist locally or on `origin` at inventory time.
- `git status --porcelain=v1 --untracked-files=all`: empty.
- Modified/untracked files before this inventory: none.
- Unknown local changes to preserve: none.
- Repository operations deliberately not used: reset, clean, rebase, checkout-overwrite, or deletion.

The custom `artificial-life-companion-architect` skill supplied with the plan was not initially installed. The user supplied `artificial-life-companion-architect.zip`; it validated with `0 errors, 0 warnings` and was installed outside the repository. Zip SHA-256: `20406A8DD6164F1EE5D3B389CEC67E86808A0AE2740A70437F11D9E81555827A`.

## Baseline commands and results

| Command | Result |
|---|---|
| `cargo fmt --all -- --check` | PASS; only the existing `could not canonicalize path C:\Users\nirrt` warning |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | PASS |
| `cargo test --workspace` | PASS: 242 passed, 0 failed, 3 ignored production acceptance tests |
| `cargo build --workspace --release` | PASS in 55.57 s |
| `powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\test_headless.ps1` | PASS: 10 s seed-42 morphic smoke, schema-v1 export, 1 s import/resume smoke |

The ignored tests are the existing long liquid acceptance replays:

- `production_idle_acceptance_10_seeds_120_seconds`;
- `production_pointer_stress_60_seconds_circles_and_8hz_zigzag`;
- `production_fixed_120_replay_is_independent_of_30_60_144hz_presentation`.

Not yet run at inventory time: optimized liquid acceptance replays, macOS cross-build/package, Windows package, real desktop capture, and canonical artifact replacement. They remain release gates, not baseline claims.

## Current runtime call graph

```text
StateStore::discover/at
  -> prepare_state
       -> import/load PortablePetState v1
       -> LifeCore::restore/new
       -> VitaRuntime::new(VitaState)
       -> StateStore::load_morph_brain
       -> MorphBrain::new

ApplicationHandler::resumed
  -> create transparent virtual-desktop overlay
  -> PlatformBackend::initialize/apply_overlay_policy
  -> ProceduralBody::generate + liquid tuning restore
  -> Renderer::new_with_render_scale
  -> AudioManager::new
  -> PetRuntime

PetApplication::update_runtime
  sensor cadence
    -> PlatformBackend::poll_desktop
    -> body-only predictive/exact hit tests
    -> SensorNormalizer::normalize -> SensorFrame
    -> PlatformBackend::poll_visual_features at 5 Hz
    -> scalar luminance copied to SensorFrame
    -> VitaRuntime::observe
         -> PerceptionRuntime::update -> VitaPerceptFrame

  120 Hz body cadence
    -> visual_mind_input
    -> ProceduralBody::fixed_update
         -> BodySimulation::fixed_update -> BodyFeedback
    -> apply_screen_domain
    -> ProceduralBody::embodied_update
         -> animation/expression/liquid/physiology presentation
    -> update_desktop_presentation

  20 Hz brain cadence
    -> MorphBrain::tick
    -> LifeCore::tick -> base BodyIntent
    -> VitaRuntime::resolve_intent_with_morph
         -> Classic/Morphic/Fusion/Morph Shadow/Morph Fusion
         -> one resolved BodyIntent
    -> preserve_navigation_during_material_drag
    -> PetRuntime.intent (current final behavior writer)

  presentation cadence
    -> ProceduralBody::presentation_update
    -> ProceduralBody::render_parameters
    -> Renderer::render

  30 s / sleep / exit
    -> persist_runtime
         -> StateStore::save_state(PortablePetState v1)
         -> StateStore::save_morph_brain
```

## Current perception truth and seam

- `desktop_host::SensorNormalizer` is the native-to-portable boundary for cursor, buttons, idle time, app category, window rectangles, and user availability.
- `pet_perception::PerceptionRuntime` owns transient cursor/rhythm/window history and produces `VitaPerceptFrame`.
- Window history is currently a private `BTreeMap<String, SurfaceHistory>` inside `pet_perception`; it calculates scalar motion, pressure, popup salience, and the nearest edge.
- This private window history is the extraction seam for one reusable `WindowAffordance` representation. A second ecology-only history must not be added.
- `PlatformBackend::poll_visual_features` currently returns one scalar `DesktopVisualSample` at 5 Hz. The app copies only mean/local luminance into `SensorFrame`.
- `PerceptionRuntime::set_visual_features` exists, but the app does not call it. Therefore motion/color/contrast feature events inside `PerceptionRuntime` are not currently fed by the native sampler. Habitat work must close this seam or explicitly replace it with the spatial grid path.
- macOS/fallback capability behavior remains `None`; habitat must preserve this valid path.

## Current behavior/brain seam

- `LifeCore` owns drives, affect, action selection, learning, memories, development, and voice repertoire.
- `VitaRuntime` owns the persistent `VitaMind`, transient `PerceptionRuntime`, and the final LifeCore/VITA/Morph semantic resolution.
- `MorphBrain` owns only its pinned neural state and bounded learning; Morph topology is `526` neurons, `17,475` synapses, `57` populations.
- `ACTION_COUNT` remains 24 and is not a habitat extension point.
- The habitat `EpisodeDirector` should be inserted after `resolve_intent_with_morph` and before `preserve_navigation_during_material_drag`, becoming the only final ecology-aware `BodyIntent` writer.
- With no eligible/active ecology episode, pass-through must preserve the resolved intent field-for-field.
- Focus, drag, sleep/retreat/metamorphosis, refusal, and capability constraints remain hard filters outside learners.

## Current pointer and hit-test flow

```text
PlatformBackend::poll_desktop -> DesktopSnapshot.cursor/button
  -> hit_test_desktop_cursor
       -> desktop point to overlay-normalized point
       -> ProceduralBody::projected_hit_test_with_margin
            -> same projection scale/aspect/presentation offset as body render
            -> liquid main-component hit or legacy projected hit shape
  -> update_pointer_state
  -> CursorHitTestLatch::resolve(predictive, hysteresis, captured drag)
  -> PlatformBackend::set_cursor_hittest
  -> SensorNormalizer::normalize
```

Current limitations:

- only the PET body participates in hit testing;
- left-button capture is PET/material-specific;
- there is no object/den capture owner;
- right/middle buttons are not explicitly captured by habitat code;
- hit-test state must be expanded to `pet_hit || orb_hit || den_handle_hit || active_capture` without breaking predictive body hysteresis or desktop click-through.

## Current renderer extension points

- `ProceduralBody::render_parameters` is the CPU presentation snapshot boundary.
- `Renderer` owns the wgpu surface, body/liquid passes, HDR offscreen target, shadow masks, and final compose.
- Current body render graph: liquid density/material flow -> optional macro filter -> body material into HDR offscreen -> shadow downsample/blur -> final compose.
- `Renderer::render_with_overlay` exposes a callback after final compose and before command submission. It is a useful debug/Lab overlay seam but is too late for full shared HDR/material integration by itself.
- Production currently calls `Renderer::render`, the no-overlay wrapper.
- `Renderer::replace_mesh`, `resize`, review backgrounds, studio backdrop, background UV/freshness, and scissor helpers are available extension points.
- Habitat objects need a dedicated bounded instance buffer/pass. The implementation should first preserve body output, then integrate ecology objects at the narrowest measured pass location; no shader or render-graph rewrite is justified.

## Current body feedback and collision seams

- `lifecore::BodyFeedback` currently contains one optional global `CollisionEvent { normal, intensity }`, plus position, velocity, acceleration, contact flags, pose error, and locomotion completion.
- `pet_body::liquid::collisions::apply_interaction_forces` consumes that one collision and applies a particle-wise exposure force based on the normal. This is the current local-response seam, but it lacks point, penetration, source, relative velocity, and multiple contacts.
- `ProceduralBody::fixed_update` advances locomotion only; `ProceduralBody::embodied_update` advances liquid/face/material state after screen-domain correction.
- `EmbodiedEnvironmentFrame` should be a new transient body input with a fixed `[ExternalContact; 8]`. It must not be serialized and must not replace `BodyFeedback` ownership.
- `BodyFeedback` can receive semantic ecology outcome summaries later, but solver contacts remain owned by `pet_body`/`pet_ecology` and must not become a second LifeCore homeostasis writer.

## Current storage and backup policy

Windows runtime root is discovered through `ProjectDirs::from("io", "lomatoq", "Pet 2").data_local_dir()`, currently documented as `%LOCALAPPDATA%\lomatoq\Pet 2\data\`.

Existing files:

- `state.json` -> `backups/state.previous.json`;
- `profile.json` -> `backups/profile.previous.json`;
- `liquid-tuning.json` -> `backups/liquid-tuning.previous.json`;
- `liquid-tuning-applied.json` -> its own backup;
- `morph-brain.json` -> `backups/morph-brain.previous.json`;
- `events.jsonl` append-only debug log.

`atomic_json` writes a same-directory temporary file, flushes and `sync_all`s it, rotates the current primary to the explicit backup, then renames the temporary file into place. If final rename fails, it attempts to restore the backup. Loaders for portable state and Morph state recover a corrupt primary from the previous valid backup.

Habitat storage seam:

- new primary: `ecology-state.json`;
- new backup: `backups/ecology-state.previous.json`;
- separate schema/version/validation and atomic load/save methods;
- no change to `PortablePetState v1` in foundation work;
- active episodes, native handles, raw pixels, absolute paths, and transient window keys are never serialized.

## Known constraints to preserve

- Five brain modes and one final semantic intent.
- `ACTION_COUNT == 24` and pinned Morph topology.
- Liquid schema 16, fixed 120 Hz production solver, face-carrier continuity, current shadow/voice behavior.
- Offline-first, deterministic seeds, no network/runtime model dependency.
- No raw private content in brain, save, logs, traces, or scenario fixtures.
- One canonical orb, maximum eight ecology objects, eight external contacts, 32 windows, and a 16x9 transient visual grid.
- Canonical `builds/current` artifacts are not replaced before every release gate and independent review pass.

## First production edit boundary

After this inventory and ADR are created, create `codex/pet2-living-desktop-habitat` from the actual current HEAD. Wave 0 may then add only:

1. the portable `pet_ecology` crate and deterministic validated state;
2. separate atomic ecology persistence;
3. a field-for-field pass-through `EpisodeDirector`;
4. `app/src/ecology_runtime.rs` as the integration seam;
5. a minimal deterministic Habitat Lab and headless smoke;
6. regression tests proving inactive ecology changes no current behavior.
