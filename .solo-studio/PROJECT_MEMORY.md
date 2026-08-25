# Project Memory — Pet 2

Updated: 2026-08-25
Current gate: first-class colleague Morph brain + bounded Morph Fusion under short release QA
Current branch: `codex/pet2-vita-embodied-iteration`

## Product truth

- Player promise: the same persistent procedural creature on Windows and macOS.
- The organism stays offline-first, deterministic, portable, and bounded.
- VITA invariants are autonomy, emergence, continuity, and individuality.
- The visual body must expose internal state; green tests alone are not a visual gate.

## Current golden path

- Launch: `cargo run -p pet2`.
- Smart mode (default): `--brain-mode morphic`; local hybrid: `fusion`; real colleague comparison: `morph-shadow`; all-brain mode: `morph-fusion`; rollback: `classic`. `Win+Alt+B` cycles all five.
- Packaged no-rebuild launchers: `Pet 2 - Fusion.cmd`, `Pet 2 - Morph Shadow.cmd`, and `Pet 2 - Morph Fusion.cmd` beside the same canonical executable.
- Body Lab: `cargo run -p body_lab`.
- Canonical gated executables: `C:\Users\nirrt\OneDrive\Документы\ChatGPT\Pet 2\builds\current\Pet 2.exe` and `C:\Users\nirrt\OneDrive\Документы\ChatGPT\Pet 2\builds\current\Body Lab.exe`; no differently named copied builds are authoritative.
- Body Lab presets: first choice is the active-at-open `Authored current`; built-in `Moonlit Glass` supplies a visibly different but solver-safe alternative. User JSON presets live at `%LOCALAPPDATA%\lomatoq\Pet 2\data\liquid-presets\` and can be saved/loaded/renamed/deleted without a rebuild or touching active `liquid-tuning.json`.
- `Apply this liquid to running Pet` hot-reloads an existing process. `Apply + launch desktop Pet` saves and verifies first, then resolves the sibling canonical executable with `builds/current`, `target/release`, and `target/debug` development fallbacks.
- Headless: `cargo run -p pet2 -- --headless-smoke 10 --seed 42 --reset-pet --no-audio --data-dir target/smoke-state`.
- Cross-platform state import/export remains schema-v1 compatible during the visual-body slice.
- The legacy procedural mesh remains available for deterministic geometry and hit testing.

## VITA embodiment work now on the branch

- Replaced the visible primitive assembly with a continuous implicit morphic renderer.
- Added soft-body squash/stretch, head and tail lag, anticipation, breathing, and compression.
- Added target-readable gaze, saccades, fixation, direct-viewer gaze, vergence, microsaccades, and side-eye modes.
- Added physical eyelid apertures, spontaneous blink, saccade blink, slow social blink, startle blink, and wink.
- Added procedural orbital brows, mouth crease/open mouth/tongue rendering, cheek glow, and emotional articulation.
- Added lock-free procedural-audio feedback for mouth envelope, syllable openness, pitch, noise, and purr response.
- Added synthetic occlusion modes so the body can peek from window edges without fragile foreign-window z-order tricks.
- Added privacy-preserving pointer, typing-rhythm, click, scroll, moving-window and visual-feature perception contracts.
- Added attention, appraisal, emotion episodes, predictive self-model, agency, favorite places and bounded influence learning.
- Kept the portable LifeCore, Windows/macOS host boundary, saved identity, memories, habits, voice motifs, and genome intact.

## Current architecture truth

- `lifecore` owns needs, affect, learning, action arbitration, memory, development, VITA mind, and portable identity.
- `pet_body` owns the morphic visual field, embodied face/soft-body runtime, locomotion, hit testing, and rendering.
- `pet_audio` owns procedural sound and publishes only lock-free derived visual feedback; no audio samples are persisted. A dedicated process-owned thread owns device enumeration, stream lifetime, retry, enqueue, polling, and teardown, so cold audio startup never blocks the visible overlay or physics loop.
- `pet_perception` owns transient derived input rhythm, gesture, window ecology and salience; it never receives typed content.
- `desktop_host` owns platform APIs, coordinates, overlay behavior, visual sampling, and persistence paths.
- `morph_brain` is the same-process Rust port of the colleague's pinned `Thandorcat/morph` neural core: exact `526`-neuron/`17,475`-synapse/`57`-population topology, LIF/delay/STD dynamics, readout, KC→MBON and operant learning. It consumes the same normalized Pet signals and cannot address physics/render/audio internals.
- `wgpu` continues to use WGSL on D3D12/Metal with no platform-specific shader fork.
- Liquid schema `16` defines one authoritative `120 Hz`/one-substep pipeline for every state: external fields → predicted positions → `6` XPBD density/containment iterations → commit corrected positions/reconstruct velocities → pair-symmetric numerical XSPH velocity filter → diagnostics/presentation.
- The permanent bounded elliptical character field and compact filtered pointer potential act particle-wise. Component classification is diagnostics/presentation only; it cannot gate forces, damping, solver choice, or merge behavior.
- Pointer input has no captured set, bond, velocity target, COM compensator, rigid chunk, pinch event, spray, or recoil. Physical idle budding, shape recovery, whole-component upright rotation, and dynamic bonds are outside the production solver.
- Artistic viscosity is independent from the `0.01` numerical XSPH floor. Sparse anisotropic splats fall back to round shapes instead of velocity-aligned capsules.
- Face presentation follows one immutable face-weight carrier through split/remerge; low confidence holds/eases the previous frame rather than reparenting to the largest component.
- Neutral face X follows the permanent carrier-component symmetry center and semantic roll is exactly zero. Strong VITA attention adds one bounded whole-face offset/roll with hysteresis and per-fixation seeded variation; authored origin X remains an explicit bias.
- Flight velocity now adds a deterministic whole-face translation/small roll presentation cue, including vertical travel, and returns to exact horizontal target after stop. It does not touch particles or create a controller switch.
- Face tuning now has backward-compatible custom iris HSV override controls. The mouth cavity uses dark semi-transparent body jelly instead of a separate tongue/plastic color.
- All five brow points preserve an eye-top-plus-stroke clearance in WGSL, including extreme tension/asymmetry.
- Flight contributes continuous comoving-frame inertia and a `1.0…1.34` area-preserving reshape of the same permanent field; the same density/field gathers lagging mass after stop without a controller switch. Body Lab exposes this as `Flight plasticity`.
- The full-silhouette shadow now uses an area-prefiltered `R16` mask plus normalized separable Gaussian passes, with preallocated half/quarter targets. Production `1×` resolves natively; Body Lab retains its true `2×` resolve. The old repeated-silhouette gather is gone.
- LifeCore owns touch voice selection, anti-repeat, context learning, one-shot credit, performance variation, and monoculture repair. A unique performance ID joins LifeCore, the owner queue, callback feedback, acknowledgement, and rejection; nothing enters use/recency/credit state before it is audibly rendered, and restart clears process-owned windows only.
- Organic voice DSP now uses one band-limited voice oscillator, a 190 Hz low cut, filtered breath, wider damped formants, DC blocking, a transparent soft knee, shorter reflections, and independently varied loudness/duration. The detuned pair and resonator output during gaps were removed as the electronic-hum sources. Audible calls are calibrated to cat/dog call ranges; purr cadence is amplitude modulation only.
- Duplicate same-size startup resize events are no-ops, preventing a second allocation of the full virtual-desktop HDR/shadow targets.
- Pointer physics remains `120 Hz`, while pointer/audio-only presentation stays `60 Hz`; only real flight enters `120 Hz` through `12/8 px/s` hysteresis. Predictive Win32 hit-test retention avoids press-time full-overlay style/cadence churn.
- The active `liquid-tuning.json` and the authoring preset library have separate ownership. Preset deletion is restricted to safe filenames inside `data/liquid-presets`; applying advances from the current active revision to prevent hot-reload rollback.

## Validation state

- Published canonical build passed the full workspace suite, all three optimized liquid acceptance replays, strict Clippy, release builds, and focused queued/heard/rejected voice, mono, naturalness, separable-shadow, native `1×`/Lab `2×`, texture-limit, halo-coverage, and resize-allocation tests. Independent audio and renderer verdicts are clean.
- The complete pre-sampler VITA slice passed strict Clippy, all workspace tests, headless simulation, release build and packaging on Windows x64 and Apple Silicon macOS.
- Procedural-mesh continuity is validated semantically across current Rust toolchains rather than by brittle raw floating-point bits.
- Windows visual perception is materialized as a 5 Hz bounded pixel-grid sampler for luminance, local luminance, contrast, colorfulness, warmth, dominant hue, motion, edge density and sudden change.
- Visual features feed the VITA salience/appraisal loop and the embodied pupil response.
- Pixel samples are reduced immediately to scalar features and are never saved, logged, exported or sent to LifeCore as an image.
- macOS and fallback hosts retain capability-based `None` until a permission-aware native sampler is implemented.
- This checkpoint triggers strict cross-platform validation of the materialized sampler and the macOS no-op path.
- The real Morph checkout at commit `3c6e27e3b55e4aff1d6c2c254713cdbd79099715` passes upstream acceptance `13/13`, desktop-adapter `15/15`, and health `25/25`. A JS golden neural trace matches the Rust engine for rates, voltages, adaptation, and short-term depression within `8e-4`.
- Final post-review optimized 10-second Morph Fusion smoke measured p50 `0.344 ms`, p95 `0.382 ms`, max `0.960 ms` per 50 ms decision tick (about `0.7%` of one core averaged over real time); it produced four distinct commands and five switches. There is no Node, IPC, network, thread, model, or first-click initialization in the shipped path.
- Independent regression re-review is clean: it reproduced corrupt-primary Morph recovery against the canonical exe, verified actual `≤0.30` continuous blending plus zero protected-state authority, and matched packaged/release SHA-256 `95ED3AF9BBAFA57327F84C075561809AE3CA6EADEA57882D8473A3918323CCE0`.
- The previous v15 flight/pointer measurements (`11/96` lagging, `10/96` edge tear) are retained only as historical baselines; they do not pass the v16 rescue gate because long/high-frequency pulls, idle stability, face continuity, and capsule rendering were not covered.
- v16 must pass: `10×120 s` idle stability; stationary, slow, long and `8 Hz` pointer replays; natural `8–16` particle tear/remerge; flight stop/recovery; face hitch/split continuity; sparse anisotropy fallback; and equivalent topology at `30/60/144 Hz` presentation.
- No executable may replace `builds/current` until workspace tests, Clippy, release build, Body Lab + real desktop visual capture, and an independent regression review pass.

## Remaining implementation sequence

1. Collect user visual/listening feedback from the canonical Pet and Body Lab; tune authored shadow and voice character without changing the stable solver or delivery lifecycle.
2. Validate the same attention, overlay, material, and CoreAudio behavior on real Apple Silicon hardware.
3. Add best-effort UI/control geometry without collecting labels or text.
4. Add opt-in microphone-derived RMS/voice-activity/prosody features without retaining audio.
5. Add optional camera/semantic providers without making LifeCore dependent on them.

## Brain infrastructure fusion direction

- The accepted direction is now implemented as a first-class same-process component:
  local `LifeCore + VITA`, real colleague `MorphBrain`, shadow/fusion modes, one final
  behavioral authority, and a local embodied safety kernel.
- `pet_body`, the presentation mapping, liquid/material renderer, procedural audio,
  desktop host, privacy reduction, and portable organism envelope remain ours.
- Morph emits only semantic command rates, attention, affect, confidence and turn;
  it may not control particles, face geometry, shader/material parameters, native
  handles, raw private input, or realtime audio samples.
- Full map, gaps, state ownership, gates, and the first executable slice are recorded
  in `BRAIN_SUBSYSTEMS_FUSION_PLAN.md`.
- The comparison has five lanes: `Classic`, `Morphic`, `Fusion`, `Morph Shadow` (real Morph runs at zero authority), and `Morph Fusion`. Morph continuous attention/appraisal/gaze/target influence is actually blended by authority capped at `0.30`; it cannot directly replace discrete pose/locomotion, and the local protected kernel zeros Morph first.
- All five use the same 20 Hz boundary, body, audio, save and renderer. Morph learning persists atomically in `morph-brain.json` and falls back to `backups/morph-brain.previous.json` if the primary is missing or corrupt; user feedback is applied to LifeCore, VITA and Morph together. Shared LifeCore drives remain the sole homeostasis owner.

## Active risks

- Windows visual QA is complete for neutral centering, extreme brow clearance, whole-face attention, click/release continuity, flight plasticity, and a continuous shadow penumbra in Body Lab. Cold audio ownership and repertoire acknowledgement have focused automated coverage; subjective sound character remains a user-listening gate in the canonical build.
- The implicit shader must stay readable and performant on low-power integrated GPUs.
- Interactive macOS overlay/Metal/CoreAudio behavior still requires Apple Silicon runtime validation.
- Global input rhythm and visual-feature providers require explicit capability/permission handling and must never persist raw content.
- Learned influence must remain playful and inspectable rather than deceptive, coercive, or disruptive.
