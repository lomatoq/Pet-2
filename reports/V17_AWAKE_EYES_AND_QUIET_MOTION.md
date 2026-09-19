# V17: awake eyes and quiet head motion

2026-09-14. Windows delivery; macOS installation is not available from this host.

## Causes and changes

The existing uncommitted follow-up to v16 corrected the phenotype's permanent
squint, the neutral shader lid height, awake selection of WakeUp, entry into the
REM program on waking, startle cooldown, and landing hysteresis. Those changes
were present in source but were not in the published v16 executable. They are
retained in this package.

Additional causal defects found in this pass:

- The companion expression owner still assigned neutral aperture 0.90/squint
  0.04, and Rest always assigned aperture 0.62 even without fatigue. Neutral now
  uses 0.98/0.0; Rest goes from 0.96 to 0.62 as fatigue rises from 0.35 to 0.85.
  Sleep, painful contact, and affectionate squint keep their distinct meanings.
- A sustained startle could replay its head motion whenever cooldown expired.
  An event latch now rearms below 0.25 startle or on a new recognized sharp flick.
  Cooldown still applies; an ongoing bout continues. Ineligible startle no longer
  masks a new pain/integrity response in the selector.
- The quiet inspection/check-back deduplication key included action and intent
  labels. Label churn renewed the same performance at the same target. The key
  now uses target and salience, also covering quiet curiosity arcs and mutual
  gaze. A target displacement of 0.08 or salience increase over 0.20 permits a
  new bout. Object goals and active approach/play retain their own controllers.

This changes event eligibility, not random animation timing. Eye disks retain
their existing size. No save format or learned state is reset.

## Regression evidence

- 200-second sustained-alarm test: one startle bout; a recovered/new alarm works.
- 200-second quiet action/intent chatter: one inspection/check-back bout; a new
  target works.
- Pain interrupts after a latched startle.
- Awake rest has aperture at least 0.95 and no squint; high fatigue reduces it
  by over 0.30; actual sleep retains zero aperture.
- Motor suite: 65 unit tests and 12 integration tests pass.
- App suite: 145 pass, one pre-existing ignored test.
- Expression director: five tests pass. Full face geometry: 11 pass.
- Ablation: disabling only the startle latch reproduces multiple bouts in the
  same 200-second fixture despite retaining the cooldown.
- LifeCore: 120 tests pass; body library: 219 pass, three pre-existing ignored.
  All 64 physical motor programs pass finite/jerk checks at 30/60/120/variable Hz;
  orb mass contact and four supported-settle tests also pass.
- Formatting, diff whitespace checks and all-target Clippy pass for app, motor,
  body, LifeCore and Body Lab.
- Release app and Dev Console built successfully. GPU captures are in
  `target/face-v17`; the production contact sheet was visually reviewed.
- A 12-second, 240-tick smoke with a copy of the saved state exits zero and
  remains bounded. This short smoke is not full autonomous-play acceptance;
  its optional orb-play check is false.
- Packaged app SHA-256:
  `c53b3830fc87986e88eeed7e5df86aef6bc2935c3b7d642f907e032c44f7c0f4`.
  Console SHA-256:
  `2f565a701d28b9770e7c9e7ae812c331f54fa683db34ac7bf02bb2aad4548495`.
  Source/package binary hashes match. The original state was backed up under
  `target/v17-original-state-backup` before launching the v17 Windows package.

Static captures and numerical tests cannot prove naturalness over all live
sessions. The pre-existing v16 autonomous orb-travel acceptance failure is a
separate issue and is not claimed fixed here.
