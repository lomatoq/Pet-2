# PET-2 — compact project reference

Updated: 2026-08-25 · stage: real colleague Morph brain + five runtime modes · tuning schema: `16`

## Product truth

- Offline deterministic desktop lifeform: one genome, body, voice, memories, and learned state across Windows and macOS.
- Quiet always-on-top transparent companion, not a full-screen app or asset-heavy sprite.
- Runtime-generated body and audio; no model, texture, recorded-audio, Unity, or network dependency.
- Golden path: seeded pet → simulate → atomically save/load → reproduce identity/body/voice → launch transparent overlay.

## Visual invariant

The character shadow is a **macOS-style drop shadow behind the full organism silhouette**. The compositor offsets and feathers the body alpha; it must never flatten, squash, or project the shadow into a ground ellipse below the character.

The body alpha is area-prefiltered into a compact `R16` mask, then blurred by normalized horizontal and vertical Gaussian passes before one final composite lookup. Feather up to `48 px` uses a half-resolution mask and larger values use quarter-resolution; both pairs are preallocated. The result must remain one continuous penumbra with no rings, stacked silhouettes, or velocity-shaped layers at either production `1×` or Lab `2×` rendering.

Shadow controls in Body Lab:

| Control | Range | Previous/default look | `macOS soft preset` |
|---|---:|---:|---:|
| X offset | `-96…96 px` | `0` | `0` |
| Y offset | `-96…96 px` | `9` | `10` |
| Feather | `2…128 px` | `22` | `42` |
| Opacity | `0…0.50` | `0.05` | `0.12` |
| Color (linear RGB) | `0…1` | `[0.008, 0.012, 0.020]` | same |

`Reset previous look` restores the incumbent values. Shadow changes stay in the Lab preview until explicitly applied.

## Run and verify

Canonical local executables use only these exact paths after the release gate:

- `C:\Users\nirrt\OneDrive\Документы\ChatGPT\Pet 2\builds\current\Pet 2.exe`
- `C:\Users\nirrt\OneDrive\Документы\ChatGPT\Pet 2\builds\current\Body Lab.exe`

Do not use older `target/debug/*_field.exe`, `*_updated.exe`, or differently named copied builds.

```powershell
cargo run -p pet2
cargo run -p pet2 -- --brain-mode morphic
cargo run -p pet2 -- --brain-mode fusion
cargo run -p pet2 -- --brain-mode morph-shadow
cargo run -p pet2 -- --brain-mode morph-fusion
cargo run -p pet2 -- --brain-mode classic
cargo run -p body_lab
cargo run -p pet2 -- --headless-smoke 10 --seed 42 --reset-pet --no-audio --data-dir target/smoke-state
cargo test --workspace
./scripts/build_windows.ps1
./scripts/package_windows.ps1
```

Body Lab: `Space` play/pause · `R` deterministic reset · `Esc` quit · left-drag the liquid · `Play/Pause/Step/Reset same seed` in UI.

Desktop Pet: `Win+Alt+B` cycles `Morphic → Fusion → Morph Shadow → Morph Fusion → Classic`. `Morphic` remains the default; `Classic` is the immediate LifeCore-only rollback. The canonical folder contains direct no-rebuild launchers for Fusion, Morph Shadow, and Morph Fusion.

### Body Lab design presets

- `1 · Authored current` is always the profile that was active when the Lab opened. It remains the first choice and is not overwritten when another preset is previewed.
- `2 · Moonlit Glass` is a deliberately different cool translucent material/face/shadow treatment with conservative schema-16 liquid dynamics. Selecting any preset updates only the Lab preview.
- User presets can be saved, loaded, renamed, and deleted without rebuilding. Windows stores them as individual JSON files under `%LOCALAPPDATA%\lomatoq\Pet 2\data\liquid-presets\`; these operations never rename or delete the active `%LOCALAPPDATA%\lomatoq\Pet 2\data\liquid-tuning.json`.
- `Apply this liquid to running Pet` preserves the existing hot-reload flow. `Apply + launch desktop Pet` first writes and verifies the active profile, then launches the sibling canonical `Pet 2.exe`; development fallback checks `builds/current`, `target/release/pet2.exe`, then `target/debug/pet2.exe`.

## Runtime defaults

`config/default.json` retains the portable product defaults. The production
virtual-desktop overlay currently overrides presentation cadence as noted below;
simulation cadence never follows presentation cadence.

| Setting | Value |
|---|---:|
| Presentation | `120 Hz` only during real screen flight; `60 Hz` for calm, pointer hold, and audio |
| Overlay | one fixed transparent virtual-desktop host; apparent Pet scale is height-calibrated |
| LifeCore / body physics / sensors | `20 / 120 / 120 Hz` |
| Save interval | `30 s` |
| Audio | enabled; process-owned worker starts/retries off the UI thread; device loss is non-fatal |

## Body Lab defaults worth remembering

- Solver: fixed `120 Hz`, `96` particles, one substep in every state, `6` XPBD density iterations, compliance `8e-6`, numerical XSPH `0.01` plus artistic viscosity `0.080`.
- Fields: character radii `0.35 × 0.43` at radius scale/strength `1.0 / 0.34`; pointer support/strength/response `1.75h / 240 / 18 Hz`. `Flight plasticity` authors the field's continuous area-preserving aspect (`0` disables it; default `0.85`).
- Fluid presentation: cohesion `0.62`; anisotropy cap `2.15`. Physical idle fragments, bud pull, pinch bounce, shape recovery, and upright stabilization default to `0`.
- Droplets: `8` available, at most `2` mobile, elasticity `9.0`, drag `4.6`, cohesion `0.68`.
- Face: origin `[0, 0.13]`, scale `[1, 1]`, eye highlight `1.45`, socket `0.18`, relief `0.55`, pupil responses `1`, smoothing `16 / 10 / 7`. `Use custom iris color` enables the adjacent HSV controls; disabled preserves the inherited palette.
- Compositor: real `2x` render scale, exposure `1.0`, full-silhouette drop shadow described above.

## Drag/topology invariant

- There is one authoritative liquid pipeline in idle, pointer pull, flight, impact, split, and merge: external fields → predict → XPBD density + containment → commit corrected positions/reconstruct velocity → pair-symmetric XSPH velocity filter → diagnostics/presentation. There is no drag-only or impact-only solver lane.
- The permanent bounded elliptical character field acts on every particle every tick. Component IDs are diagnostics/presentation only and never gate return, damping, or ownership.
- The mouse adds one compact Wendland-C2 potential. Its centre is simulation-filtered at `18 Hz`; field support is `1.75h`. It has no captured set, spring/bond, velocity target, COM compensator, rigid chunk, pinch event, spray, or recoil.
- Releasing the mouse removes only the pointer potential. The same permanent character field gathers detached real particles; merge cannot switch forces or inject an impulse.
- Density projection is always on. `numerical_xsph` is a small stability floor independent from artistic `viscosity`, which may still be `0`.
- Face ownership follows its immutable material carrier, not the largest component. Presentation may smooth/freeze low-confidence tracking but cannot move simulation particles.
- Render anisotropy is presentation-only: sparse neighborhoods fall back to round splats; particle velocity cannot turn a blob into one hard capsule.
- Rejected architecture remains rejected: press-time cages, dynamic bonds, shape matching, synthetic fragments, component-driven return, whole-component rigid rotation, and physical autonomous budding.

## Flight-field invariant

- Desktop acceleration contributes one continuous inertial load in the character field's comoving frame. Constant velocity contributes none.
- Flight presentation does not add a second controller: speed/acceleration continuously reshape that same field by an area-preserving aspect of `1.0…1.34`. The unoriented major axis is perpendicular to travel, so reversal cannot cause an axis flip; the field relaxes continuously to the neutral ellipse after stop.
- Shell particles may lag or detach; the always-on density law and character field remain unchanged throughout acceleration, cruise, stop, and remerge.
- No COM servo, mean-velocity matching, torque projection, component switch, or authored flight fragment participates. `flight_inertia` and `flight_damping` remain internal parameters of this one continuous field; `flight_stretch` is now the visible `Flight plasticity` control. `flight_max_lag` remains inactive legacy data.

The Body Lab preview is centered in egui's real `CentralPanel`, not the full window hidden under the controls. Pointer mapping and containment use the same canvas center and presentation offset.

## Character look defaults

- Material lane: `CinematicJelly`; genome colors overridden in the Lab.
- HSV: primary `[0.77231336, 0.7349049, 0.8532155]`; secondary `[0.913958, 0.3259855, 0.9573257]`; glow `[0.28437626, 0.428986, 0.8177877]`.
- Optical core: absorption `1.0`, scattering `0.72`, thickness `1.0`, translucency `0.72`, refraction `1.0`, transmission blur `1.15`, opacity `0.92`.
- Surface/light: studio `1.45`, gel/coat roughness `0.32 / 0.09`, rim `1.0 / 0.62`, edge width `18 px`, bloom `0.55`.
- Internal life: `8` glow spheres at intensity `0.92`; `4` soul lobes at strength `0.20`; living flow `0.18`.
- The saved `liquid-tuning.json` may override all defaults; Body Lab shows the loaded values live.

## Active authored profile

Current authored profile after the rescue QA: schema `16`, revision `8`, name `Black`, material `cinematic_jelly`. Body Lab saved and reread this revision after the centering and flight-plasticity pass. Schema `16` keeps legacy JSON readable while physical budding, spray, bonds, grab-follow, and shape-recovery remain outside the production force path.

Windows profile path: `%LOCALAPPDATA%\lomatoq\Pet 2\data\liquid-tuning.json`; acknowledgement: `liquid-tuning-applied.json` in the same directory.

- Active liquid values: viscosity `0.075`, cohesion `1.20`, numerical XSPH `0.014`, pointer support/strength/response `2.25h / 320 / 32 Hz`, anisotropy `2.00`, character-field radius/strength `1.10 / 0.30`, and flight plasticity `0.85`.
- Face origin `[0.00, -0.03]`; scale `[0.96, 1.02]`; eye size `0.55`; spacing `1.00`; pupil `1.50`; highlight `2.15`; socket `0.34`.
- Wet relief: strength `0.60`, darkness `0.79`, coat `1.60`.
- Pupil response: light `1.65`, emotion `1.42`, focus `1.65`; microsaccade amount/rate `1.65 / 1.35`.
- Face smoothing: translation `16`, rotation `10`, scale `13`; authored maximum roll `0.13 rad`.
- Compositor: `2x`, shadow `[x 30, y 8, feather 64, opacity 0.31, white]`, exposure `1.0`.

## Emotion readability contract

- VITA remains authoritative and blends one of 11 coordinated emotion targets into the neural/affective face. The renderer only presents the result.
- Strong semantic attention moves the complete facial mask toward the selected fixation (maximum offset `0.085`) and adds a bounded head-turn roll (maximum `0.15 rad`). Authority is attention confidence/commitment plus curiosity, novelty, and social focus; raw cursor velocity cannot turn the head.
- Real body flight contributes a second continuous presentation cue from actual screen velocity: up/left/right travel visibly shifts and slightly rolls the whole face, then the same analytic tracker returns the target to exact `0°` after arrival. This cue never applies a particle force.
- Engagement uses hysteresis. Bounded seeded variation is sampled only when a meaningful fixation changes, so a held focus stays calm and separate attention episodes do not replay one obvious sine loop.
- Neutral semantic roll is exactly `0°`. The neutral origin follows the center of the permanent face-carrier component plus the authored upright face offset; rotating face-weighted material controls support/confidence but cannot make the semantic face orbit. Authored `Face origin X/Y` remain explicit additive biases, and Body Lab provides `Center face horizontally` to set X bias to zero.
- The closed mouth uses six smooth spans; each brow uses four. Positive mouth curve is a smile, negative is a frown.
- Every brow control point is clamped above the full eye boundary plus its soft stroke, so extreme tension/raise/asymmetry cannot paint a brow over an eye.
- Mouth curvature leads valence; inner/outer brow shape and eyelid tension disambiguate fear, frustration, shyness, boredom, and surprise; cheek glow reinforces delight/affection.
- Pupil size encodes light, focus distance, and emotional arousal/intensity—not positive versus negative valence. Strong pleasant and unpleasant episodes can both dilate it.
- Blinks remain procedural. A visible open mouth remains owned by confirmed audio playback; silent emotion changes its curve/tension without miming failed speech.
- Body Lab exposes `Face emotion` plus `Emotion intensity` for deterministic readability review.
- The open mouth cavity reuses the body jelly palette at lower radiance and `0.76` coverage. There is no separate plastic tongue material.

## Brain modes and embodiment boundary

- `Morphic` is the new default behavior mode. One 20 Hz decision boundary lets LifeCore choose the base action and VITA enrich it once with attention, appraisal, emotion, gaze, locomotion/target, pose, interaction, and expression.
- `Fusion` is the separate local hybrid proposed in `BRAIN_SUBSYSTEMS_FUSION_PLAN.md`. LifeCore needs/affect/learning and VITA attention/appraisal enter one confidence-weighted arbiter. It blends target and gaze analytically, holds discrete pose changes briefly, and keeps a local protection kernel authoritative during sleep, focus mode, pointer drag, retreat and metamorphosis.
- `Morph Shadow` runs the real colleague Morph neural core on the same observations but gives it exactly zero visible authority. Its command, rates, attention, affect and timing remain available in headless/debug traces.
- `Morph Fusion` sends LifeCore, VITA and Morph through the same final arbiter. Morph may blend attention, appraisal, gaze and a validated high-level target by its actual authority, capped at `0.30`; it cannot directly replace discrete pose or locomotion. Sleep/focus/drag/retreat/metamorphosis protection zeros every Morph override before arbitration.
- `Classic` returns LifeCore's intent byte-for-byte and keeps the same body/audio/runtime. It is a rollback mode, not a second physics implementation.
- There is one final `BodyIntent` writer. Locomotion drives desktop motion; actual velocity/acceleration drives liquid lag, flight flattening and head turn; affect/appraisal drives physiology, eyes and material presentation; confirmed callback audio alone drives mouth opening. Brain code cannot address particles, components, solver iterations, shader knobs, or native handles.
- `morph_brain` is compiled into `Pet 2.exe`; it is not a sidecar. The generated topology is pinned to colleague commit `6aa4e7c871c11ff2fa1619942611d4fb50457e49` and contains `526` neurons, `17,475` synapses and `57` populations. No Node, IPC or network call exists in the desktop runtime.
- LifeCore remains the sole owner of shared needs/homeostasis. Morph owns only its spiking state and plastic KC→MBON/operant weights, persisted atomically as `%LOCALAPPDATA%\lomatoq\Pet 2\data\morph-brain.json`; a corrupt/missing primary recovers from `backups\morph-brain.previous.json`. The original JS checkout is used only for provenance and golden parity tests.

## Voice learning contract

- LifeCore is the sole owner of touch and autonomous vocal selection. The desktop host no longer chooses the loudest motif.
- The same exact motif is excluded for the next three **heard** utterances, and one lineage cannot play twice in a row while another family is available. A stable motif gets an ephemeral performance ID, pitch drift up to `±4%`, gain variation `0.58…1.04×`, and duration variation `0.72…1.45×`; Touch, Chirp, Purr, and Mimic retain distinct acoustic targets.
- Queueing commits no learning state. Use count, novelty, anti-repeat history, and the `3…9 s` feedback window begin only when the callback publishes that exact performance ID. Rejection is therefore a lossless discard, and starting a new audio process clears only queued/credit windows—not learned repertoire history.
- Synthesis uses one band-limited voiced source, softened harmonic balance, a `190 Hz` output low cut, high-passed/low-passed breath, broad damped formants, DC blocking, a transparent soft-knee limiter, and short damped early reflections. The former near-detuned oscillator pair and formant ringing through silent gaps are removed because they produced an electronic beat/drone. Mono output is a symmetric fold-down and meters read the sample actually sent to the device.
- Vocal calibration follows measured animal ranges: ordinary cat calls commonly use roughly `300–1000 Hz` fundamentals with strongest energy around `1–2 kHz`; dog whines span low fundamentals into high multi-kilohertz bands; purr cadence stays near `24–32 Hz` only as amplitude texture, never as an audible pure bass oscillator.
- Restore/consolidation repairs a collapsed repertoire by restoring at least eight canonical families and caps any lineage at four motifs without resetting the Pet's other learned state. Snapshot schema remains compatible through `serde(default)` fields.

## Press-latency contract

- The transparent window is shown before audio-device discovery. The UI thread only spawns a bounded audio-owner channel; device enumeration, stream creation, polling, retry, and teardown stay on that owner thread.
- A duplicate same-size startup `Resized` event is a strict no-op, so it cannot recreate the full virtual-desktop HDR targets after initial allocation.
- Pointer physics remains fixed at `120 Hz`; press adds only one O(`96`) field pass and never changes XPBD iterations or solver lane.
- Pointer/audio-only presentation stays at `60 Hz`, avoiding the previous press-triggered doubling of a full-virtual-desktop swapchain. Only real screen flight enters `120 Hz`, with `12 px/s` enter and `8 px/s` exit hysteresis.
- Win32 cursor hit testing arms before contact using a bounded velocity-aware margin and retains state across the edge. A captured drag always wins; the first far released sample restores click-through. Native window styles therefore do not chatter at the silhouette or switch for the first time on mouse-down.
- Orb capture has a `4 px` physical drag threshold. A click preserves position, lifecycle, and velocity; an active drag preserves the initial grab offset, follows with a bounded spring response, clamps the center by the rendered radius, and releases only bounded physical velocity.

## Tuning and persistence contract

- `Save JSON` / `Load JSON` operate on the path shown in Body Lab.
- Design-preset files are a separate authoring library under the Pet local-data `liquid-presets` directory. Their revision is reset on save; applying a preset advances from the larger of its revision and the current active revision, so selecting a built-in or older user preset cannot roll hot reload backwards.
- Ordinary Lab edits are preview-only. `Apply this liquid to running Pet` sanitizes, increments `profile_revision`, atomically writes `liquid-tuning.json`, rereads it, and waits for acknowledgement.
- Running Pet polls tuning every `250 ms`, applies the last valid profile, and writes `liquid-tuning-applied.json`; invalid/partial writes do not replace the valid runtime profile.
- Schema `16` keeps legacy PBF/grab/bond/idle fields for serde compatibility while authoring only the single solver lane. v15 migration forces `120 Hz`, `1` substep, `6` density iterations, numerical XSPH `0.01`, field scales `1.0 / 1.75h / 18 Hz`, and physical idle/recovery forces off.
- The old JSON key `shadow_blur_radius` remains accepted as `shadow_feather`; old profiles preserve their authored blur value.
- StateStore also owns `state.json`, `profile.json`, `events.jsonl`, and backups in the OS application-data directory. Portable state contains no native handles, paths, device IDs, or raw private input.

## Physics rescue acceptance gates

- Idle: `10` seeds × `120 s` with no split, reset, NaN, speed-cap hit, or kinetic-energy growth; p95 compression ≤`10%`, maximum ≤`25%`.
- Pointer: stationary hold settles; slow pull moves near mass at least `2×` far mass; `60 s` circles plus `8 Hz` zig-zag stay finite and non-rigid with density ≤`1.3ρ₀`.
- Topology: a deliberate `8–16` particle tear needs no authored event; release returns ≥`95%` mass within `3 s`, one component within `5 s`, and merge Δv <`0.08`.
- Presentation: face carrier never changes on split/merge; per-frame face/gaze limits hold through a `50 ms` hitch; `<6` anisotropy neighbors always render aspect `1`.
- Determinism: equivalent pointer replays at `30/60/144 Hz` presentation end within `2%` body radius with identical topology.
- Attention: weak/neutral focus yields zero semantic offset/roll; strong left/right fixation moves the full face consistently, holds one seeded pose during the fixation, and replays deterministically for the same seed.
- Shadow: normalized separable blur stays continuous at the half/quarter boundary; production `1×` resolves one centered texel while Lab `2×` keeps its supersample resolve; no old silhouette survives a moving scissor.
- Voice: at least four successive heard touch requests have no exact repeat; adjacent families differ when alternatives exist; gain/duration differences are measurable; PCM stays bounded, declicked, DC-safe, and free of excessive high-frequency noise; queued/rejected performances teach nothing.
- Startup/press latency: audio initialization cannot block visibility or the frame loop; an identical resize allocates nothing; pointer/audio alone never select the `120 Hz` full-overlay cadence; flight cadence has `12/8 px/s` hysteresis and cursor hit-test retention never breaks captured drag/release.
- Release gate: tests, Clippy, release builds, Body Lab + desktop visual capture, and an independent regression review must all pass before replacing `builds/current`.

## Architecture map

| Path | Owns |
|---|---|
| `crates/lifecore` | deterministic needs, affect, brain, learning, actions, memory, development, portable identity |
| `crates/pet_body` | liquid simulation, face, locomotion, hit testing, WGSL renderer, material/compositor tuning |
| `crates/pet_audio` | procedural voice and lock-free visual audio feedback |
| `crates/pet_perception` | privacy-reduced rhythm, gesture, window ecology, salience |
| `crates/desktop_host` | Windows/macOS APIs, overlay, coordinates, sensors, atomic storage |
| `app` | desktop runtime, CLI, hot reload, import/export |
| `tools/body_lab` | deterministic authoring/diagnostic Lab; never changes production without Apply |

State ownership rule: simulation owns truth; presentation reads it. Shaders, animation, VFX, audio, and UI do not decide gameplay state.

## Technical envelope and risks

- Rust `1.88`, edition `2024`; `wgpu 27.0.1`, `winit 0.30.12`, `egui 0.33.3`, `cpal 0.15.3`.
- Windows 10/11 x64: D3D12 + WASAPI. macOS 13+ Apple Silicon: Metal + CoreAudio. Other desktop backends are reduced-capability only.
- Targets: `<150 MB` memory and `<50 MB` release package; active target `60 FPS`.
- Main open risks: real Apple Silicon overlay/audio validation, integrated-GPU material cost, permission-aware perception providers, and keeping learned influence inspectable/non-disruptive.

Longer history and decisions remain in `.solo-studio/PROJECT_MEMORY.md`, `.solo-studio/PROJECT_CHARTER.md`, `.solo-studio/RISKS.md`, and `.solo-studio/DECISIONS/`.
