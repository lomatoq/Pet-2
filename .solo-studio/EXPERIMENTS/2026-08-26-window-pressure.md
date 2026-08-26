# Experiment — Window pressure and recovery

ID: HAB-WINDOW-2026-08-26
Date: 2026-08-26
Owner: Pet 2
Status: adopt

## Decision

Adopt reduced moving-window geometry as a shared external affordance for both the
orb solver and PET's local liquid-body pressure response.

## Question

Can visible window motion produce bounded contact, escape, retry, help, and
recovery without collecting window identity/content or destabilizing the body?

## Hypothesis

Geometry, velocity, overlap pressure, and nearest-edge normals are sufficient for
legible physical causality; labels, process IDs, titles, and pixels are unnecessary.

## Baseline

Ignore foreign windows and treat the virtual desktop as one empty rectangle.

## Representative setup

- Engine/library versions: Rust 1.88; fixed object/body steps at 120 Hz.
- Hardware/OS: Windows 11 live overlay plus platform-neutral deterministic fixtures.
- Build settings: workspace tests, strict Clippy, optimized liquid acceptance.
- Scene/content: one orb, PET, one to four moving AABBs, desktop edges, den.
- Resolution: normalized virtual desktop with mixed-DPI/layout remap tests.
- Seed/camera path: fixed seeds 9–13 and seed 42 flagship.
- Warmup: none.
- Duration/repetitions: 60-second 8 Hz adversarial stress and 720/1600-tick stories.

## Variables

- Independent: window velocity, overlap, opposing contacts, body/orb position.
- Controlled: maximum four reduced windows, bounded impulse, fixed timestep.
- Outputs: contact normal/intensity, pressure, escape direction, trapped/help state.

## Metrics

- Primary: finite body/object state; no tunnelling; bounded external impulse.
- Secondary: pressure preemption, escape completion, bounded help retry.
- Qualitative: pressure trail and reason-code readability.
- Capture tools: Habitat Lab SVG/JSON and optimized liquid replay output.
- Raw artifact path: `target/habitat-acceptance/pet_window_squeeze_escape.*`.

## Success threshold

- 8 Hz adversarial window motion remains finite and contact bounded.
- Pressure preempts play and enters recovery.
- Trapped orb asks for help without inventing another object.
- No native window identity or content crosses the perception boundary.

## Kill criteria

- Teleportation, unstable repeated impulses, unbounded retries, or persistence of
  native window metadata.

## Integration boundary

`desktop_host` reduces native observations; `pet_perception` owns affordances;
`pet_ecology` owns object contact/episodes; `pet_body` consumes local contacts.

## Fallback/removal path

Return `WindowAffordanceFrame::default()`; habitat and LifeCore remain functional.

## Risks

- License/provenance: no third-party content retained.
- Privacy/safety: geometry/velocity/pressure only.
- Platform: macOS currently returns capability `None` for unavailable providers.
- Maintenance: order of desktop, window, and body contacts is a coupled invariant.
- Failure visibility: decision traces expose pressure, escape confidence, and reasons.

## Result

- Raw numbers: optimized pointer stress retained one main component, peak density ratio `1.0001`, peak extent `0.3569`, rigid p95 `0.5512`, and zero long rigid runs.
- Visual/player evidence: flagship trace contains trapped-object retry/help and later recovery; dedicated pressure SVG remains within frame bounds.
- Variance/worst case: maximum-speed throws and thin-window sweeps pass; repeated 8 Hz motion stays finite.
- Quality concessions: windows are reduced AABBs, not arbitrary native contours.
- Unexpected findings: coupled PET contact could undo desktop-radius clamping; fixed by deterministic feasible separation at the boundary.

## Decision

Adopt.

Reduced window affordances create readable pressure and recovery without expanding
the privacy boundary.

## Follow-through

- ADR/update: `.solo-studio/DECISIONS/0002-living-desktop-habitat.md`.
- Production boundary: window affordance frame and external contact arrays.
- Regression guard: window spam, thin-window sweep, edge-pin, focus suppression.
- Next action: validate the no-permission capability path on Apple Silicon hardware.
