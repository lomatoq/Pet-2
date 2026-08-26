# Experiment — Bounded mimesis training and transfer

ID: HAB-MIMESIS-2026-08-26
Date: 2026-08-26
Owner: Pet 2
Status: adopt

## Decision

Adopt explicit teaching of reduced trajectory and rhythm signatures with bounded
competence updates and spatial transfer.

## Question

Can PET learn the structure of a gesture/rhythm, reduce motor error, and replay it
elsewhere without storing raw input history or changing Morph topology?

## Hypothesis

Arc-length-resampled normalized signatures preserve the relevant structure while
removing absolute screen path and raw timing history.

## Baseline

One-shot animation playback tied to the original coordinates and event density.

## Representative setup

- Engine/library versions: Rust 1.88; LifeCore action count 24; Morph topology 526/17,475/57.
- Hardware/OS: platform-neutral deterministic tests and Windows Habitat Lab build.
- Build settings: workspace tests, strict Clippy, optimized release.
- Scene/content: figure-eight trajectory, new target region, click rhythm.
- Resolution: normalized desktop coordinates.
- Seed/camera path: fixed training fixtures and seed 42 scenario.
- Warmup: one explicit `Cmd/Win+Alt+T` teaching capture.
- Duration/repetitions: ten safe training attempts plus transfer/rhythm replays.

## Variables

- Independent: event density, target region, rhythm intervals, attempt count.
- Controlled: fixed signature capacity, normalized coordinates, bounded learning rate.
- Outputs: motor error, competence, uncertainty, rhythm correlation, execution path.

## Metrics

- Primary: motor error reduction after ten attempts; successful new-region transfer.
- Secondary: interval correlation and deterministic signature merge.
- Qualitative: figure-eight path remains recognizable and continuous.
- Capture tools: Habitat Lab JSON/SVG and unit-test diagnostics.
- Raw artifact path: `target/habitat-acceptance/skill_transfer_new_region.*`.

## Success threshold

- At least 25% motor-error reduction on the fixed fixture.
- Rhythm interval correlation at least `0.85` after repertoire-owned synthesis shaping.
- Event-density changes produce the same arc-length signature.
- No raw pointer/audio history and no Morph topology change.

## Kill criteria

- Absolute-coordinate memorization, unbounded skill growth, raw-history persistence,
  or learning that bypasses the existing action/audio owners.

## Integration boundary

`pet_ecology` stores only bounded signatures/competence; LifeCore retains action and
voice ownership; `pet_audio` reshapes repertoire-owned syllables/gaps only.

## Fallback/removal path

Disable teaching and skill candidates; all existing brain modes remain exact
semantic pass-through in an empty habitat.

## Risks

- License/provenance: repository-native algorithms and fixtures.
- Privacy/safety: normalized signatures only; no text, pixels, or audio samples.
- Platform: global input availability remains capability-dependent.
- Maintenance: signature schema must remain bounded and portable.
- Failure visibility: competence, uncertainty, and motor error are exposed in diagnostics.

## Result

- Raw numbers: ten attempts reduce the fixed-fixture motor error by at least 25%; grounded rhythm correlation passes `0.85`.
- Visual/player evidence: learned figure-eight transfers continuously into a new region in Habitat Lab.
- Variance/worst case: near-duplicate demonstrations merge instead of growing the library; event density does not change the signature.
- Quality concessions: mimesis represents motion/rhythm structure, not semantic human intent.
- Unexpected findings: none requiring architecture change.

## Decision

Adopt.

The reduced signature is learnable, portable, inspectable, and does not create a
second behavior or audio authority.

## Follow-through

- ADR/update: `.solo-studio/DECISIONS/0002-living-desktop-habitat.md`.
- Production boundary: mimesis library, teach recorder, LifeCore/audio adapters.
- Regression guard: resampling, merge, error reduction, transfer, rhythm correlation.
- Next action: collect subjective recognizability feedback from independent review.
