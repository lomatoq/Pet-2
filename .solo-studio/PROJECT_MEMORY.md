# Project Memory — Pet 2

Updated: 2026-08-21
Current gate: VITA Iteration 2 — embodied social organism
Current branch: `codex/pet2-vita-embodied-iteration`

## Product truth

- Player promise: the same persistent procedural creature on Windows and macOS.
- The organism stays offline-first, deterministic, portable, and bounded.
- VITA invariants are autonomy, emergence, continuity, and individuality.
- The visual body must expose internal state; green tests alone are not a visual gate.

## Current golden path

- Launch: `cargo run -p pet2`.
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
- Kept the portable LifeCore, Windows/macOS host boundary, saved identity, memories, habits, voice motifs, and genome intact.

## Current architecture truth

- `lifecore` owns needs, affect, learning, action arbitration, memory, development, and portable identity.
- `pet_body` owns the morphic visual field, embodied face/soft-body runtime, locomotion, hit testing, and rendering.
- `pet_audio` owns procedural sound and publishes only lock-free derived visual feedback; no audio samples are persisted.
- `desktop_host` owns platform APIs, coordinates, overlay behavior, and persistence paths.
- `wgpu` continues to use WGSL on D3D12/Metal with no platform-specific shader fork.

## Active implementation sequence

1. Make the morphic body, face, and audio feedback compile and pass Windows/macOS CI.
2. Add perception features for pointer gestures, typing/click/scroll rhythm, window motion, luminance, color, and visual salience.
3. Add attention/appraisal/emotion composition, predictive self-model and agency estimation.
4. Add bounded personalized influence strategies with focus-mode and privacy constraints.
5. Add optional microphone/camera/semantic providers without making LifeCore dependent on them.

## Active risks

- The implicit shader must stay readable and performant on low-power integrated GPUs.
- Interactive macOS overlay/Metal/CoreAudio behavior still requires Apple Silicon runtime validation.
- Global typing rhythm and visual-feature providers require explicit capability/permission handling and must never persist raw content.
- Learned influence must remain playful and inspectable rather than deceptive, coercive, or disruptive.
