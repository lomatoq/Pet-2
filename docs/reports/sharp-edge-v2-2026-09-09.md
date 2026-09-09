# Sharp Edge V2 — implementation and acceptance record

Status: implementation candidate; perceptual acceptance is not signed off. This report distinguishes code, automated evidence and outstanding observation. Source baseline: `e9bf1ec5ad7a7b247e453e00f247609476d417d2`, branch `codex/pet2-nervous-system-r12-impl`. The supplied DOCX is the specification; its older `fbff91f` reference was not used as the checkout.

## Changes

- Extended the existing expression contract with per-eye lid geometry, independent brow endpoints/curve/thickness and procedural mouth width/corners/compression. Eight canonical poses share the production CPU/WGSL path. Full held blink masks iris; silent emotional mouth shapes survive audio composition. Physiological blink and aperture are separate. Tired yawn is a bounded transition.
- Removed repeated scalar smoothing and independent facial oscillation. Phenotype selection has candidate/hold hysteresis; ExpressionRuntime owns attack/release. Safety masks inviting mouth geometry. Curious pose requires a perceived target. Lab exposes canonical face overrides, motor combinations and autonomous return, plus desired/mixed/smoothed/rendered channel diagnostics and CPU geometry latency.
- Added Reach, Present, Guard, Settle and Recoil forces inside the existing PBF solver. Fields preserve mass and remove mass-weighted translation, share a force bound and keep actual contact/hit testing. Affine target uses reciprocal scales; nonlinear forces do not claim exact measured area preservation. Settle selection requires actual support. Commanded deformation participates in existing somatic energy and efference feedback; measured strain/pain are retained.
- EpisodeDirector owns petting continuation, orb turn-taking and obstructed-goal help. Petting starts after actual pleasant contact release, keeps the side, follows a body-diameter-bounded target and waits. Orb acceptance requires the same orb, fresh user interaction and actual displacement. Help requires repeated contact-based failure, low progress and an obstacle. Acceptance, timeout, missing target, safety and explicit refusal have separate paths. Motor no longer supplies a second play/contact reward or autonomous post-timeout task selector.
- Successful contact side counts persist through EcologyState. Existing GestureConventionLibrary and affect remain authoritative; no replacement classifier, brain or reward layer was added. Focus/absence reduce availability. Human-held objects cannot be moved/released by ecology commands.
- Profile schema 22 migrates older schemas, rejects unknown top-level/PBF fields and adds bounded posture_gain (0 disables new shape fields). One face scale increase (1.15 on the known authored scale) and separate rim/halo calibration. Saved custom scale values are retained. Last-applied acknowledgement now includes the canonical loaded-profile hash.

## Baseline and configuration

The pre-change saved user profile had SHA-256 `1B79D4458333411F86B4D1FB0F92B321A3198D81E50DDE8359661AB318390599`; its last recorded acknowledgement was revision 575/schema 21. That acknowledgement is not evidence that the supplied video ran that profile. User persistent data was not overwritten by QA. Captures use isolated seed 42 and the checked-in authored profile, revision 510/schema 22. The release manifest records its file hash and the exact executable hashes. The runtime acknowledgement hash is a stable hash of canonical serialized applied settings, distinct from file SHA-256.

Rollback: posture_gain=0 disables added PBF postures. Restore the previous package and a saved previous profile for full code/profile rollback; do not ask the old schema loader to interpret a schema-22 profile. New geometry defaults are neutral when absent. Scene/controller state is transient; successful side preferences remain persistent.

## Automated and visual evidence

The full workspace test run passed 575 tests, zero failed, five ignored before the final small hardening changes. Subsequent targeted release tests cover the changed ecology/body paths, held blink and the complete motor/body catalog. Clippy with `-D warnings` passes for changed application/crate targets. WGSL validates with Naga; Rust uniform size is checked against the actual WGSL layout. Fixed 120-Hz simulation with 30/60/120-Hz presentation is tested for all eight faces; the catalog test exercises all 64 programs, including variable presentation timing. This is structural/physical regression evidence, not visual approval of all 64 programs.

`PetLab.exe --face-captures OUTPUT` produces 72 production-renderer stills (8 poses × 3 backgrounds × 3 pixel scales), six isolated silhouette stills and channel/physical diagnostics. This does not manipulate the user's persistent pet. Pixel-scale captures are not a claim of native Windows DPI testing. The release evidence folder contains contact sheets and PNG originals.

## Outstanding acceptance and limits

- Five blinded human observers and the proposed 80% recognition threshold have not been run. Distinguishing curiosity/confusion, affection/tiredness and boundary/fear still requires that observation.
- Nine complete autonomous desktop recordings (three scenes × acceptance/timeout/cancellation, two seconds before and three–five after) and same-input baseline/face-only/face+shape/full video ablations are not supplied. Deterministic branch tests and Lab captures do not substitute for those recordings.
- Familiar/ambiguous gesture readability, measured afterglow durations, semantic-phase audio timing, prolonged calm and live safety recovery need longitudinal desktop observation. Existing gesture and affect paths were preserved, not newly certified against every perceptual criterion.
- CPU channel latency is available; it does not measure end-to-end stimulus-to-photon latency. A single headless comparison does not establish rendered frame-time/CPU regressions. Native Windows 100/150/200% DPI, live hot-reload target continuity and production live-fixture learning isolation need explicit acceptance sessions.
- Canonical endpoints are intentionally strong. Autonomous intensity modulation and secondary-expression blending have not been fully calibrated against the proposed 20–30% envelope. Shape silhouette recognition is not established by finite/mass tests; Recoil/Present in particular require observer evaluation.

No universal readability, complete perceptual acceptance or absence of every runtime regression is claimed. This package is reviewable implementation with documented remaining gates.

## Final capture and smoke observations

Production GPU capture completed with 78 stills. All eight endpoints differ across white/black/checker backgrounds. Affection has lower-lid lift while tiredness uses upper-lid closure; boundary has inward-sloping brows and a compressed flat mouth. Small mouth details remain lower contrast on the busy background. Reach and Settle are visibly elongated; Recoil leans away. Present and Guard remain close to neutral at small size, so the five-way silhouette recognition gate remains open.

The same 60-second, seed-42, silent headless scenario returned exit 1 in both V1 and V2: bounded=true, travels=true, pursues_goals=true, plays_with_orb=false. This inherited behavioral acceptance failure is unresolved, not presented as a passing smoke test. Single-run wall times were 1.856 s (V1) and 2.139 s (V2), while compilation/other validation was active; this is not an isolated performance benchmark and cannot establish a frame-time regression or improvement.

Final targeted release results: ecology 65 passed / 0 failed; body library 175 passed / 0 failed / 3 ignored; face pipeline 4 passed / 0 failed; all-64 motor/body catalog 1 passed / 0 failed (17.26 s). Formatting and diff whitespace checks passed.
