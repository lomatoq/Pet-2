# Current organic-behavior integration audit

Scope: preserve the existing v17 changes and saved organism. Implement after the
research synthesis. Production sources are this worktree, not the root checkout.

## Confirmed implementation boundaries

- Microphone: no input stream exists. `desktop_host` capabilities report false
  on Windows/macOS. Workspace already pins CPAL 0.15.3 for output. Input requires
  real capture, bounded feature processing, observable device/error status and
  optional local enrollment; loudness is not keyword recognition.
- Blinking: `BlinkController` uses 0.14-second sine-squared physiological blinks,
  currently advanced by the 20 Hz companion expression tick. ExpressionRuntime
  then smooths each eyelid. A steady QuietCompanionship label requests social
  blinks every tick with a 7.5-second refractory interval. Evaluate final lids,
  not just abstract envelopes, at 30/60/120 Hz.
- Gaze: `GazeController` adds sin/cos offsets in normalized desktop units.
  Companion gaze, final nervous gaze, and Embodiment gaze all participate.
  Need one fixation owner and small bounded presentation offsets; avoid treating
  a micro-offset as a new world target or a head-turn trigger.
- Material: `apply_companion_expression` multiplies phenotype viscosity and
  tension by semantic body style. `liquid/mod.rs` consumes runtime multipliers;
  supported softness also derives from flight aspect. Preserve conservation,
  contact, grip, and load-bearing support while changing visible compliance.
- Surface context: app `build_motor_context` unconditionally adds only the
  desktop bottom edge. Window candidates already have four-edge ranking.
  Sleeping chooses horizontal support; side attachment must remain a different
  supported physical behavior with explicit normal and bounded duration.
- Continuity: LifeCore owns action selection, VITA companion intent, ecology
  owns object episodes, motor runtime owns readable phases. New organic urges
  should nominate or enrich these owners rather than independently move the root.

## Root integration ownership

Root reserves `app/src/main.rs`, `app/src/nervous_system_runtime.rs`,
`tools/body_lab/src/main.rs`, `crates/desktop_host/src/lab_control.rs` and shared
module registrations as needed. Specialists should report additive APIs and
integration hooks before editing shared files. Agents must not run overlapping
Cargo builds/tests or launch native apps; root coordinates compiler and delivery.

## User requirements

Research precedes implementation. Implementation agents use Sol, not Astra.
Desired experience: readable causal state sequences, reciprocal contact and
turn-taking, subtle gaze/microgestures without jitter, natural blinks, fluid
body with meaningful shape, endogenous needs and stable preferences, supported
sitting/clinging on different surfaces, real microphone reaction and learned
name/quiet cues with a clear teach/test procedure.

No unsupported claim of literal biochemistry, consciousness, general speech
understanding, or guaranteed recognition of an untrained voice. Windows is the
available validation/delivery host; do not claim macOS installation.
