# Experiment — Living orb continuity and play

ID: HAB-ORB-2026-08-26
Date: 2026-08-26
Owner: Pet 2
Status: adopt

## Decision

Adopt one canonical portable orb and causal offer, chase, intercept, retrieve,
store, and help episodes as the first persistent habitat object.

## Question

Can one orb preserve identity through play, collisions, storage, restart, monitor
changes, and long deterministic runs without tunnelling, duplication, loss, or
edge pinning?

## Hypothesis

A fixed-step height-normalized circle solver plus explicit object lifecycles and
episode commands will create legible play while keeping persistence and rendering
independent of native monitor/window identity.

## Baseline

A transient visual prop owned by one animation, with no portable identity or
post-restart continuity.

## Representative setup

- Engine/library versions: Rust 1.88 workspace; `pet_ecology` schema 1.
- Hardware/OS: Windows 11 x86_64; deterministic tests are platform neutral.
- Build settings: debug regression plus optimized release acceptance.
- Scene/content: one canonical orb, PET body, den, desktop bounds, moving windows.
- Resolution: normalized desktop; 16:9 and ultrawide aspect tests.
- Seed/camera path: seed 42 flagship; fixed seeds in physics and persistence tests.
- Warmup: none.
- Duration/repetitions: 720/1600-tick scenarios, 24-hour and 7/14-day accelerated runs.

## Variables

- Independent: drag/throw velocity, PET contact, window motion, restart, layout/DPI change.
- Controlled: fixed 120 Hz object step, serialized RNG, one canonical orb ID.
- Outputs: lifecycle, position, velocity, contacts, episode transitions, state hash.

## Metrics

- Primary: one orb after restart; finite bounded state; deterministic replay hash.
- Secondary: no tunnelling; bounded speed; completed offer/intercept/retrieve/store paths.
- Qualitative: orb, trail, den, gaze, and outcome readability in Habitat Lab SVG.
- Capture tools: Habitat Lab JSON/SVG and live Windows overlay capture.
- Raw artifact path: `target/habitat-acceptance/orb_offer_user_plays.*`, `target/habitat-acceptance/habitat_story_v1.*`.

## Success threshold

- Exact identity survives JSON round-trip and layout remap.
- Maximum-speed throws do not cross desktop or thin-window boundaries.
- A PET contact at a desktop edge cannot leave the orb center at `0` or `1`.
- Flagship scenario verdict is `pass` with finite bounded state.

## Kill criteria

- Duplicate/lost orb, non-finite state, topology-dependent behavior, or persistent edge pinning.

## Integration boundary

`pet_ecology` owns portable state/physics/episodes; `pet_body` only renders the
snapshot; `desktop_host` only maps normalized coordinates and persists JSON.

## Fallback/removal path

Disable habitat candidates and retain exact LifeCore intent pass-through.

## Risks

- License/provenance: repository-native MIT code only.
- Privacy/safety: no native handles, titles, pixels, text, or audio in orb state.
- Platform: interactive macOS behavior still needs Apple Silicon validation.
- Maintenance: object/desktop/body contacts must remain one coupled invariant.
- Failure visibility: Habitat Lab trace includes contacts, lifecycles, and hashes.

## Result

- Raw numbers: flagship ecology state hash `4913997946884220323`; 29 causal outcomes; object physics p95 `0.3 us` in the final deterministic capture.
- Visual/player evidence: live Windows overlay rendered the habitat without an opaque host; Habitat Lab shows independent PET/orb trails and den state.
- Variance/worst case: 30/60/144 Hz presentation produced identical object state; 8 Hz moving-window spam stayed finite and contact bounded.
- Quality concessions: collision geometry is circle/AABB rather than a pixel contour.
- Unexpected findings: live QA exposed PET↔orb contact clamping the center to `y=0`; the radius-aware feasible escape solver and two regressions now prevent recurrence.

## Decision

Adopt.

The persistent orb creates durable causal history and now survives the observed
desktop-edge failure without special-casing a monitor or saved identity.

## Follow-through

- ADR/update: `.solo-studio/DECISIONS/0002-living-desktop-habitat.md`.
- Production boundary: `crates/pet_ecology`, app ecology integration, ecology renderer.
- Regression guard: edge pin, tunnelling, determinism, persistence, layout remap, longitudinal suites.
- Next action: subjective play-feel tuning without weakening fixed-step or persistence invariants.
