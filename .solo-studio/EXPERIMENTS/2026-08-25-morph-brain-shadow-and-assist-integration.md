# Experiment — Morph brain shadow and assist integration

ID: morph-shadow-assist-2026-08-25
Date: 2026-08-25
Owner: Pet 2
Status: narrow

## Decision

Whether the real Thandorcat/morph neural system is safe and useful as a
first-class second brain component behind Pet 2's existing behavior boundary.

## Question

The real pinned Morph topology and learning rules can run in-process as Rust,
consume Pet 2 observations at 20 Hz, and improve visible attention diversity at
bounded authority without changing physics ownership.

## Hypothesis

Morph's 526-neuron LIF network should fit comfortably beside the existing 20 Hz
LifeCore/VITA decision because its own measured full `sim.step(16.67)` cost is far
below one millisecond on the target machine. A direct Rust port removes Node,
process startup and IPC from the shipped runtime.

## Baseline

Keep the in-process LifeCore/VITA Fusion mode and use Morph only as a behavioral
reference. This is also the immediate mode-level rollback.

## Representative setup

- Engine/library versions: Rust 1.88 workspace; Node 20; Morph commit `3c6e27e3b55e4aff1d6c2c254713cdbd79099715`
- Hardware/OS: current Windows development machine
- Build settings: Pet release with same-process `morph_brain`; Node only runs the original golden reference
- Scene/content: deterministic normalized desktop observation trace
- Resolution: 900 × 560 Morph sensor stage; Pet coordinates remain normalized
- Seed/camera path: shared deterministic identity/trace seeds
- Warmup: embedded topology parses during Pet initialization, never on first click
- Duration/repetitions: short 3–10 s integration probes plus upstream acceptance suites

## Variables

- Independent: Classic/Morphic/Fusion/Morph Shadow/Morph Assist mode
- Controlled: Pet body, renderer, audio, seed, observation trace and 20 Hz decision cadence
- Outputs: Morph winner/rates/attention/affect, latency, switches, agreement and fallback counts

## Metrics

- Primary: p95 same-process tick; valid finite output fraction
- Secondary: winner chatter, local-action agreement, state size and assist authority
- Qualitative: attention/pose variation without unsafe locomotion or face snapping
- Capture tools: headless JSON summary, debug event log and live desktop observation
- Raw artifact path: `.solo-studio/EXPERIMENTS/results/morph-shadow-assist/`

## Success threshold

- 100% finite outputs; p95 processing below 2 ms.
- Selecting Classic/Morphic/Fusion removes Morph authority immediately.
- Morph contribution is capped at 0.30 and cannot own particles, audio or native input.

## Kill criteria

- Any UI-thread wait, unbounded queue, direct particle command, invalid output above 0.1%,
  or material regression in Pet frame pacing rejects the live assist path.

## Integration boundary

Only the semantic `ObservationV1 -> MorphBehaviorV1` adapter and the existing single
Fusion arbiter may depend on it. Production body/audio/render code may not.

## Fallback/removal path

- Select Classic/Morphic/Fusion for zero Morph authority. Removing the `morph_brain`
  input from the one arbiter restores the previous executable architecture.

## Risks

- License/provenance: colleague-owned project component, but upstream should add explicit repository license text before public redistribution.
- Privacy/safety: only normalized numeric features enter the module; no text or pixels.
- Platform: same-process Rust core keeps the production executable standalone.
- Maintenance: upstream is pinned by URL and commit; protocol rejects incompatible schemas.
- Failure visibility: headless/debug output reports every semantic readout and bounded authority.

## Result

- Raw numbers: upstream acceptance 13/13, desktop adapter 15/15, system health 25/25;
  original `net.step()` 16.50 µs and full `sim.step(16.67)` 0.363 ms. Optimized
  initial in-process Rust port p50/p95/max = `0.338 / 0.392 / 0.672 ms` per 50 ms tick;
  final post-review 10-second Morph Fusion build after command habituation and
  authority hardening = `0.344 / 0.382 / 0.960 ms` and produced four distinct
  commands with five switches.
- Visual/player evidence: separate launchable Shadow and Fusion modes; live subjective A/B pending.
- Variance/worst case: deterministic 15 s trace produced five commands and nine switches;
  no invalid output, physics mutation, fallback or save failure.
- Quality concessions: Morph object/episode/world-model modules are not duplicated; Pet's
  existing perception, homeostasis and memory remain the single shared owners. Neural
  topology, dynamics, readout and plastic pathways are the merged colleague component.
- Unexpected findings: the original JS checkout lacks ESM package metadata and repository
  license text. Neither issue affects the same-process executable, but the upstream repo
  should declare its project license before public distribution.

## Decision

Narrow: adopt the same-process neural core and persisted learning; keep Morph Fusion
opt-in until the user completes live visual/behavior A/B. Reject the sidecar design.

Why: exact golden parity passes within `8e-4`, performance is comfortably below the
2 ms gate, and one final bounded arbiter preserves body/visual ownership.

## Follow-through

- ADR/update: `BRAIN_SUBSYSTEMS_FUSION_PLAN.md`, `PROJECT_REFERENCE.md`, project memory.
- Production boundary: `morph_brain` may emit semantic suggestions only.
- Regression guard: embedded topology shape, deterministic trace, plastic state roundtrip,
  JS golden rates/voltage/adaptation/STD parity, strict Clippy and headless summaries.
- Next action: run packaged Morph Shadow and Morph Fusion on the real desktop and compare
  attention/pose diversity before considering a default-mode change.
