# Pet 2 embodied pointer language — implementation report

Date: 2026-09-01
Base commit: `6010c77447c9cb68fbe732e8ee8223626741575c`
Feature branch: `codex/pet2-embodied-pointer-language`

## Outcome

This branch implements an end-to-end, physically caused body-communication path: normalized pointer input drives the existing production PBF body; bounded body-local readback drives a deterministic gesture classifier; LifeCore appraises and learns; VITA enforces one response per physical episode; expression, body actuation and learned gesture conventions remain causally coherent. It also adds bounded body/replay persistence, Body Lab controls and a typed multi-rate evolution runner.

The implementation keeps one production PBF path and one persistent behavior owner. Synthetic curricula cannot write user-specific gesture conventions, a failed run cannot promote state, and all physical hard limits remain outside learning.

## Repository audit

The starting branch already had a deterministic LifeCore, VITA arbitration, Morph fusion, procedural body/PBF simulation, ecology persistence, bounded causal telemetry, a typed Lab Control channel, headless smoke tests and Windows packaging. The missing vertical path was the connection from real material readback through semantic classification and appraisal to bounded response and learning. PBF did not persist its component lifecycle, the liquid profile had no body-communication controls, replay did not carry raw normalized pointer samples plus a body snapshot, and accelerated evolution used no typed acceptance report.

The baseline was clean and exactly at the required commit. Baseline `fmt` and `clippy` passed; workspace tests reported 371 passed, 0 failed and 3 ignored. The baseline headless process was blocked by the already-running user Pet singleton (PID 41768); no user process was terminated. New headless replay/evolution paths bypass the singleton, and the final normal headless smoke runs on an isolated data directory.

## Architecture and decisions

Data flows in one causal direction:

`normalized pointer sample → production PBF → body-local contact/material/component frame → deterministic physical-feature classifier → LifeCore appraisal/variant selection → VITA one-turn gate → body/expression/voice actuation → explicit outcome credit`.

Key decisions:

- The classifier is a deterministic, fixed-memory feature model rather than a neural model. Eleven classes do not justify opaque training, and deterministic replay plus limited personal data favor explainable fixed thresholds.
- Split/remerge operates on the existing 96 production particles. No visual-only fragments, deletion, mass respawn or topology prediction substitutes for simulation.
- Topology is guarded before a bond break is accepted. The hard production limits are at most three detached components, at most four tracked components including main, detached mass clamped by a 0.05–0.25 tuning range (production default 0.18), and minimum fragment size enforced before break.
- Voluntary budding is an explicit, bounded creature actuation. It temporarily lowers surface tension and support radius, while the topology guard remains authoritative. Cursor motion alone does not grant this permission.
- LifeCore owns persistent appraisal, response variants and credit. VITA only manages transient episode/turn state and requires fresh input after one response.
- Interaction learning uses bounded updates (maximum absolute variant update 0.04) and cannot change topology, pressure, strain, quiet/mute, sound, permission or recovery safety limits.
- Strong/long input maps to a calm boundary and disengagement. Sleep suppresses interaction voice at source and caps body/expression amplitude.
- User conventions have a 12-item library, eight rollback checkpoints, deterministic recognition/replacement, delete/reset/rollback, and update only from explicit live or user-authorized input. Synthetic curriculum preserves the initial convention state unchanged.
- Rejected alternatives were a second behavior brain in the host, cursor-only gesture inference, direct classifier mutation in fixtures, cosmetic particle sprites, automatic emergency teleport during normal recovery, giant accelerated `dt`, unbounded personal path logging and unconditional metamorphosis.

## Physical model and hard bounds

For each viscoelastic bond, strain is `distance / max(rest_length, 1e-5) - 1`. Above yield strain, the rest length relaxes by `1 - exp(-dt / max(relaxation_time, 0.05))`, scaled by 0.72. A non-face bond must exceed break strain for three consecutive substeps before it breaks. Visual neck thickness is `sqrt(1 - clamp(positive_strain / break_strain, 0, 1))`; strength falls only to the bounded `1 - 0.65 * normalized_strain` until an accepted break.

The XPBD bond solve uses accumulated lambda with compliance scaled by `dt²`; inverse-mass corrections are applied symmetrically. Face-lock bonds use stricter material behavior and cannot break. Component summaries are computed from real particle membership; accepted frames require `mass_conservation_error <= 1e-5`.

Production defaults/ranges relevant to communication:

- detached components: 1–3, default 3;
- detached mass fraction: 0.05–0.25, default 0.18;
- interaction boundary strain: 0.45–1.20, default 0.72;
- offscreen recovery delay: 0.25–5.0 s, default 1.25 s;
- recovery-field boost: 1.0–2.0, default 1.60;
- bond compliance: `1e-7..5e-3`, default `4e-4`;
- bond yield/break strain: default 0.14/0.82;
- bond iterations: 1–10, default 3;
- tracked observations: main plus at most three detached summaries;
- pointer replay: at most 30 s and 3,600 strictly ordered normalized samples;
- causal telemetry: 5 Hz, at most 8 MiB current log plus one rotated predecessor.

Normal return is physical. Separation fixtures get a bounded 30-second PBF settle window; they do not delete, teleport or reconstruct mass. Offscreen recovery is deterministic, observable and reserved for an actual lost component.

## Persistence and schema migrations

- Life snapshot 1 → 2: adds default-empty persistent interaction/variant state; restores pending credit as interrupted instead of replaying a response.
- Liquid tuning 16 → 17: preserves prior PBF/material/face values and adds sanitized `InteractionTuning` defaults; structural tuning is deferred while mass is detached.
- Ecology 1 → 2: adds an empty bounded gesture-convention library; no conventions are fabricated from generic mimesis.
- Lab Control 1 → 2: adds closed semantic interaction fixture/convention commands; arbitrary trajectories remain invalid.
- Causal telemetry 2 → 3: adds contact/material/component/appraisal/turn/convention summaries without raw paths, app content or native identifiers.
- Body snapshot 1: persists particles, bonds, component lifecycle, tuning identity/revision and mass checksum with atomic backup/validation.
- Pointer replay 1: persists only fixed-step body-local pointer position/down/hover input plus initial body snapshot and deterministic expectations. Velocity, acceleration, contact, classification and response are recomputed.
- Evolution config/report 1: typed closed presets, clocks, persistence policy, eligibility, per-replicate hashes, invariants and performance.

The legacy portable v1 fixture still restores and round-trips after normalizing the Life snapshot schema to v2; genome, motifs, memories, habits and generated body identity are unchanged.

## Privacy-safe Live Brain example

Representative schema-3 body payload shape:

```json
{
  "episode_id": 17,
  "phase": "awaiting_fresh_input",
  "gesture": {
    "kind": "slow_stretch",
    "confidence": 0.81,
    "causes": ["rising_strain", "sustained_contact"]
  },
  "contact": {
    "point_local": [0.21, 0.03],
    "effective_pressure": 0.34,
    "contact_seconds": 0.72
  },
  "material": {
    "maximum_strain": 0.67,
    "neck_thickness": 0.58,
    "mass_conservation_error": 0.0
  },
  "components": {
    "count": 1,
    "detached_mass_fraction": 0.0,
    "selected": []
  },
  "response": {
    "reason": "calm_boundary",
    "response_emitted": true
  }
}
```

There is no HWND, device/native ID, screen-pixel path, active application, text/content payload or particle array. `point_world` and raw pointer trajectories are not logged.

## Verification record

Baseline:

- `cargo fmt --all -- --check` — pass.
- `cargo clippy --workspace --all-targets -- -D warnings` — pass.
- `cargo test --workspace --all-targets` — 371 passed, 0 failed, 3 ignored.
- baseline normal smoke — blocked only by the pre-existing live Pet singleton; no process was killed.

Final code gates:

- `cargo test --workspace --all-targets --quiet` — 411 passed, 0 failed, 4 ignored; 152-test `pet_body` target completed 149/0/3.
- `cargo clippy --workspace --all-targets -- -D warnings` — pass.
- `cargo fmt --all -- --check` — pass.
- `cargo run -p pet2 -- --headless-smoke 60 --seed 42 --reset-pet --no-audio --data-dir target/final-smoke` — pass; 1,200 Life ticks / 60.0 s, behavior acceptance passed, genome hash `8216674915911011881`, Life hash `5124423106707041543`, mesh hash `15275378911829371554`, ecology hash `17545723728721338657`.
- deterministic pointer replay, seed 42 — pass on repeated clean data directories; semantic input hash `15326742926118267672`, final state hash `8343081490374799617`, mass checksum `14112960558033500581`, `slow_stretch → calm_boundary`, one response, no expectation failures.
- production split/remerge smoke (180 s, 13 episodes, one replicate) — pass; exact 120/60/20/5 Hz, 0 invariant failures, detached mass peak 0.14583333, 10 safe-boundary episodes, six variant updates, zero convention updates/recoveries, final state hash `15741287331066650284`.
- one-day diagnostic first full run — correctly rejected after 2,585.30 s because replicate seed `16240640731987609054` retained one detached component after an 11 s settle; all other invariants were zero. This was not waived.
- exact failing-seed reproduction after the 30 s physical-settle fix, 48 episodes — pass in 46.75 s with 0 unremerged/recovery/mass/topology/non-finite failures and detached mass peak 0.16666667.
- a subsequent full run exposed an absolute-clock phase defect: quiet telemetry could miss 5 Hz boundaries after an odd-ending physical episode. The odd-phase regression now proves perception/Life/telemetry fire on absolute multiples 2/6/24; a 900 s exact reproduction produced exactly 4,500 telemetry samples.
- final `OneDayDiagnostic` — pass, `status=completed`; 4×24 h, 192 total episodes, 10,368,000 base ticks per replicate, exact 120/60/20/5 Hz, exactly 432,000 telemetry samples per replicate, five sleep consolidations, no persistence, no convention updates, no genome mutation. All mass/non-finite/recovery/topology/unremerged/duplicate-response counters are zero; peak detached mass is 0.17708333 and every interaction is closed. Primary final state hash `3197832673906884474`; replicate hashes `3197832673906884474`, `3087012315111376271`, `9430295008322787047`, `5064628352912095633`. The preset is correctly not metamorphosis-eligible because 48 episodes are below the 120-episode eligibility threshold; policy is off, so this is a completed diagnostic rather than a promotion.
- `powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\package_windows.ps1 -Configuration Release` — pass; produced `dist/Pet2-windows-x64.zip`.
- packaged `Pet2.exe --headless-smoke 60 ...` on a clean data directory — pass; 1,200 ticks, behavior acceptance true, genome hash `8216674915911011881`, mesh hash `15275378911829371554`, Life hash `18263068548396109636`, Morph p50/p95 344.9/380.9 µs.
- final `git diff --check` — pass.

## Performance

- 180-second real-split release smoke: 9,737 active body steps, p50 892.8 µs, p95 1,134.6 µs, max 6,481.2 µs, 1.247 episodes/s.
- First exact 4×24 h one-day run (failed only on final remerge): wall 2,585.30 s, 137,288 active body steps, p50 710.5 µs, p95 1,049.9 µs, max 17,233.0 µs, 0.0743 episodes/s. Process sampling near the end showed 2,139 CPU seconds for 2,152 wall seconds, ~17.4 MiB working set and stable handles.
- Final absolute-phase exact 4×24 h run: wall 2,681.18 s, 228,488 active body steps, p50 743.0 µs, p95 1,084.0 µs, max 27,089.6 µs, 0.07161 episodes/s. Telemetry scheduling produced exactly 432,000 samples per 24-hour replicate (5 Hz), proving that active/quiet phase transitions do not drop clock boundaries.
- Final Windows archive: 6,886,715 bytes, SHA-256 `63271ADC311CDDCAE9475D54B7DECDEC35DECB7B88F8663042905B161146EA67`; packaged Pet executable 7,443,968 bytes and Body Lab 8,502,272 bytes.
- Exact quiet mode is intentionally expensive because LifeCore/VITA/Morph retain their real 20 Hz clocks. CalendarOnly is explicitly approximate and performs no interaction learning.
- Fixed-size PBF/classifier rings and arrays avoid per-tick growth; allocator-zero behavior was verified by code structure and bounded-state tests, not by a platform allocation profiler. This distinction is intentional in the report.

## Files changed

The complete source/config/fixture/report list is:

```text
Cargo.lock
README.md
app/Cargo.toml
app/src/ecology_runtime.rs
app/src/evolution_runner.rs
app/src/main.rs
app/src/pointer_replay_runner.rs
app/src/replay/mod.rs
app/src/replay/pointer_interaction.rs
app/src/vita_runtime.rs
config/evolution/boundary-safety.json
config/evolution/one-day-diagnostic.json
config/evolution/seven-day-socialization.json
crates/desktop_host/src/evolution_control.rs
crates/desktop_host/src/lab_control.rs
crates/desktop_host/src/lib.rs
crates/desktop_host/src/sensors.rs
crates/desktop_host/src/storage.rs
crates/desktop_host/tests/portable_fixture.rs
crates/lifecore/src/actions.rs
crates/lifecore/src/interaction.rs
crates/lifecore/src/lib.rs
crates/lifecore/src/persistence.rs
crates/pet_body/src/embodiment.rs
crates/pet_body/src/expression.rs
crates/pet_body/src/lib.rs
crates/pet_body/src/liquid/body_snapshot.rs
crates/pet_body/src/liquid/collisions.rs
crates/pet_body/src/liquid/component_lifecycle.rs
crates/pet_body/src/liquid/interaction.rs
crates/pet_body/src/liquid/mod.rs
crates/pet_body/src/liquid/rescue_tests.rs
crates/pet_body/src/liquid/topology_guard.rs
crates/pet_body/src/liquid/viscoelastic_bonds.rs
crates/pet_body/src/tuning.rs
crates/pet_ecology/src/gesture_conventions.rs
crates/pet_ecology/src/lib.rs
crates/pet_ecology/src/persistence.rs
crates/pet_ecology/src/state.rs
crates/pet_ecology/tests/persistence.rs
crates/pet_perception/src/embodied_gesture.rs
crates/pet_perception/src/lib.rs
reports/PET2_BODY_COMMUNICATION_IMPLEMENTATION_REPORT.md
tests/fixtures/pointer/pull_split_remerge_v1.json
tools/body_lab/Cargo.toml
tools/body_lab/README.md
tools/body_lab/src/main.rs
```

## Known limitations and safe next step

- The checked-in replay fixture name retains the historical `pull_split_remerge` label, but its pointer-only production replay does not grant creature-side voluntary budding and therefore remains single-component. Real voluntary separation/remerge is covered by production-particle PBF tests and the evolution curriculum, where bounded creature actuation is explicit.
- `OneDayDiagnostic` exact mode is a release regression workload measured in tens of minutes on this machine. Body Lab should present progress and recommend the short deterministic smoke for iteration; exact mode should remain the promotion gate.
- The “include saved gesture replays” field is typed and surfaced, but the current evolution curriculum does not yet ingest capture files; it remains false in all shipped presets. It must not be enabled as a convention-learning source until explicit capture selection and meaning authorization are wired end to end.

The safe next step is a user-observed Body Lab session using the semantic fixtures and Live Brain body-local telemetry, followed by an explicitly selected captured-replay import design. Do not promote a saved-replay learning path by merely toggling the existing config field.

## Final metadata

The final commit SHA is reported in the task handoff after the report itself is committed. Generated `target/` evolution data and `dist/` binaries are intentionally ignored build artifacts; the reproducible source, presets, fixture and this report are committed.
