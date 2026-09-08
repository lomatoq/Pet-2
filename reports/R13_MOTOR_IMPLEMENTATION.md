# Pet 2 R13 motor implementation evidence

Status: automated implementation, live Lab exposure, and acceptance complete on
2026-09-03.

The earlier 20-program/P0-only status is superseded. The executable catalog now
contains all 64 programs from the R13 catalog: eight families with eight
programs each. Every program has a deterministic selector path, a bounded phase
grammar, physical/non-cosmetic actuation, and a causal trace. The 64 programs
share eight family controllers; they do not require 64 independent visual
calibrations.

## Global timing

- Body action phase clock: 2.0x.
- Physical navigation/flight target speed: 2.0x for all locomotion modes.
- Target response and bounded acceleration scale with the same body-tempo lane.
- Face presentation remains at real-time 1.0x: gaze, saccades, eyelids, pupil,
  mouth, and brow smoothing do not consume the accelerated body clock.
- Sleep approach/landing/onset remains a special 4.0x pre-sleep chain; NREM
  holding and slow breathing remain physiological real time.

Measured purposeful 1920x1080 speed bands after the change:

- seek: 540-620 px/s;
- flee: 580-670 px/s;
- orbit: 470-570 px/s.

Tests retain bounded acceleration, braking, monitor-independent physical pixel
speed, and prevention of a one-tick reversal or teleport.

## Sleep contract

Bottom-edge sleep is a measured physical sequence:

1. Approach while the particle-only silhouette gap is greater than 8 px.
2. Enter soft landing at 8 px or less.
3. Accept contact only while absolute gap is at most 4 px and normal velocity is
   at most 36 px/s.
4. Require 300 ms continuous measured dwell; a commanded support constraint is
   not evidence.
5. Revoke support if the gap exceeds 6 px.
6. Load and flatten the lower contact side, close the eyes, and enter NREM with
   `breath_speed_multiplier=0.44` only after measured support.

The desktop collision bound uses the real lower metaball extent rather than a
symmetric half-extent, removing the former 150-300 px visible gap.

## Automated 64/64 acceptance

`reports/r13_catalog_acceptance.json` uses schema
`pet2.motor_catalog_acceptance.v2` and records, for every program:

- the concrete just-over-threshold activation probe;
- selector reachability and activation cause;
- bounded phase completion;
- non-cosmetic actuation;
- causal trace presence;
- local-field and surface-attachment budgets.

Result: 64/64 passed, zero unreachable programs, maximum two local fields and
one surface attachment. SHA-256:
`b41837c812710d8f1ab3950e2e808a2c224d30c36738ae71a7df1ca800b4a087`.

The deterministic motion/surface/sleep/touch/physiology/den replay is
`reports/r13_deterministic_traces.jsonl`:

- records: 161;
- internal stable digest: `8b758fd25553dffb`;
- SHA-256:
  `7ba1731c8c3753cd0addee23e76746b928a0a0f67b76779bf1a8605232dc137d`.

The sleep replay includes all four required stages: roost search, soft landing,
measured settle, and NREM.

## Live manual review of all 64 programs

Pet Lab now exposes the complete motor catalog in `F12` -> `Live Nervous
System` -> `Controlled intervention`. The panel is open by default and provides:

- one indexed selector containing all 64 programs, grouped by family;
- the selected program's priority, cooldown, and complete phase list;
- `Run selected`, which starts one bounded bout through the production
  `pet_motor -> SomaticActuationBus -> particle PBF` path;
- `Cancel`, which returns control to the autonomous selector immediately;
- live program, phase, and Lab-override telemetry;
- the independently recomputed deterministic acceptance count and resource
  budgets.

The brain keeps advancing while a Lab bout runs, but it cannot replace that
bout. Completion still comes from the production phase maxima/physical evidence;
the Lab does not inject a cosmetic animation layer.

## Den background capture performance repair

The den displacement slowdown came from a previous change that reduced a
full-overlay desktop capture to 12 Hz. It is replaced by a den-local square ROI:

- 60 Hz request cadence;
- 440 reference pixels across at 1152 px desktop height, scaled with den size
  and displacement support;
- maximum uploaded texture dimension 512 px;
- DXGI copies only the ROI into its staging texture rather than copying/mapping
  the complete monitor;
- a cross-monitor ROI uses the local GDI stretch path instead of returning a
  partial capture;
- the shader receives the ROI's normalized screen rectangle and transforms
  screen-global refraction UVs into crop-local UVs.

This restores responsive displacement without reintroducing full-desktop GPU
readback and render-thread uploads.

## Verification

- `cargo test -p lifecore`: 88 passed, 0 failed.
- `cargo test -p desktop_host`: 31 passed, 0 failed, plus 2 integration tests.
- `cargo test -p pet_motor`: 24 passed, 0 failed, including the 64-program Lab
  execution sweep.
- `cargo test -p pet_body`: 174 passed, 0 failed, 3 explicitly ignored
  long production-acceptance stress fixtures.
- `cargo test -p pet2`: 80 passed, 0 failed, 1 explicitly ignored fixture
  regeneration test.
- `cargo test -p body_lab`: 21 passed, 0 failed.
- strict Clippy across `pet_motor`, `desktop_host`, `pet2`, and `body_lab`, all
  targets with `-D warnings`: passed.
- `cargo build --release -p pet2 -p body_lab`: passed; optimized executables
  generated at `target/release/pet2.exe` and `target/release/body_lab.exe`.
- the optimized 60-second isolated headless smoke passed its bounded behavior
  acceptance gate (`travels`, `pursues_goals`, and `plays_with_orb` all true).

Current release SHA-256:

- `pet2.exe`: `5D6ED33A7D63D0C321B915E6DFA721C72B58CE7AAC60CB1D1E48902DF91F7EA1`;
- `body_lab.exe`: `7012C9C886898FC9295C74F14ED9CEB298720F0A74B4A206B2407C6965E2E260`.

These checks replace per-action Computer Use calibration for activation,
threshold, timing, causality, and safety. A live visual review remains useful
only for subjective aesthetics; it is not required to make a program reachable
or functionally valid.

## 2026-09-03 completion update

The follow-up movement smoothing, Windows default-output audio watcher, 31 px
physical orb hull, authenticated normal-release Pet Lab connection and fresh
hashed Windows package are complete. The current hashes and full numeric
verification supersede the older release values above and are recorded in
`reports/R13_FINAL_IMPLEMENTATION_2026-09-03.md`.
