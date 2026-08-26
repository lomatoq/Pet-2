# ADR 0002 — One portable ecology and episode layer for the living desktop habitat

## Status

Accepted for implementation.

## Date and owners

- Date: 2026-08-26
- Decision owner: Pet 2 project

## Context

PET-2 already has a portable LifeCore, VITA, bounded Morph fusion, a procedural liquid body, procedural audio, privacy-reduced desktop perception, atomic saves, and a transparent cross-platform host. Its visible behavior still ends mostly at a short semantic `BodyIntent`. It lacks persistent objects and places, multi-phase goals, physical failure/retry/recovery, ecology-specific procedural skills, and material continuity across sessions.

The living-habitat slice must add those capabilities without creating another brain, changing the 24-action neural interface, duplicating homeostasis, breaking liquid topology, or persisting private desktop content.

## Decision

Create one portable workspace crate, `pet_ecology`, as the sole writer of:

- persistent world objects and canonical orb identity;
- den state and object slots;
- bounded metabolic enrichment and taste memory;
- deterministic object physics and window/object affordances;
- one transient `ActivityEpisode` and reason-coded `EpisodeDirector`;
- mimesis signatures, competence, uncertainty, and skill persistence;
- ecology outcomes, diagnostics, and schema-v1 save state.

Insert `EpisodeDirector` after the existing LifeCore/VITA/Morph resolution and before body/object motor execution. It may refine a resolved semantic intent into a multi-phase ecology action, but it does not own LifeCore drives, VITA affect, Morph state, body particles, renderer state, native sensors, or audio waveform generation.

Use a separate `ecology-state.json` with its own previous-valid backup. Do not change `PortablePetState v1` during foundation work. Active episodes are transient and end as interrupted on shutdown.

## Alternatives considered

### Add habitat state directly to LifeCore and expand `ACTION_COUNT`

- Benefits: fewer crates and apparently direct action access.
- Costs: couples physical world state to identity/homeostasis, changes the stable neural interface, enlarges save migration risk, and introduces competing action ownership.
- Failure modes: Morph parity regression, duplicate homeostasis, object state leaking into brain snapshots, difficult rollback.
- Why not selected: it violates current state ownership and the explicit habitat constraint to preserve 24 actions.

### Implement each feature independently in the app/body renderer

- Benefits: fast isolated demos.
- Costs: duplicated object/window histories, animation-owned state, non-portable saves, no coherent causal episodes, and giant runtime coupling.
- Failure modes: random-animation ecology, renderer or input becoming gameplay authority, object loss, untestable cross-feature behavior.
- Why not selected: it cannot produce the required causal story or deterministic Habitat Lab.

### One portable ecology crate behind a narrow runtime seam

- Benefits: explicit state ownership, pure deterministic tests, separate persistence/rollback, one object/episode vocabulary, reusable Lab, and bounded integration with all five brain modes.
- Costs: one new crate and translation boundary; renderer/body/input still need measured adapters.
- Failure modes: the crate could become an over-general framework or a second brain if its scope is not enforced.
- Why selected: it is the smallest architecture that can produce persistent world continuity and multi-step behavior while preserving the current organism.

## Scientific and engineering basis

- **Established engineering practice:** one authoritative writer per important state, versioned persistence, atomic backup, deterministic scenarios, and simulation/presentation separation reduce corruption and regression risk.
- **Supported transfer:** affordance-first action generation, commitment/hysteresis, bounded retries, outcome-linked memory, and shared attention produce more interpretable embodied behavior than random idle selection.
- **Engineering hypothesis:** a persistent orb, den, moving-window pressure, and bounded learned skill library will increase next-day causal story recall compared with the current intent/pose baseline.
- **Speculative claim explicitly rejected:** the mimesis layer is an interpretable observation-execution coupling, not literal biological mirror neurons or evidence of consciousness.

## Expected consequences

- Behavior: one final ecology-aware semantic intent can sustain orient -> approach -> manipulate -> evaluate -> retry/help -> recover/complete sequences.
- Learning: orb outcomes, taste, and mimesis competence update from explicit causal results with bounds and rollback.
- Embodiment: object/window contacts constrain action and locally affect the liquid body through a fixed transient contact frame.
- Communication: gaze, posture, object relations, and optional LifeCore-owned motifs communicate goals without a text UI.
- Performance: fixed capacities and rates keep hot paths bounded; inactive/sleeping ecology can reduce work.
- Privacy: only transient reduced spatial features and window geometry enter ecology; no raw pixels, titles, text, handles, or paths persist.
- Cross-platform: portable state/physics/episodes compile everywhere; Windows receives full capability first and macOS keeps explicit graceful fallbacks.

## Experiment and acceptance gate

- Baseline: seed-42 current app/headless traces and the current body/brain regression suite.
- Primary proof: deterministic `habitat_story_v1` uses production scoring, episodes, contacts, outcomes, save/reload, and retained state rather than a hardcoded animation sequence.
- Ablation: inactive ecology must preserve the resolved brain intent field-for-field; scenario variants can disable persistence, episode memory, or learning independently.
- Longitudinal horizon: 24-hour accelerated run, seven-day routine shift, 14-day absence, and restart/corruption fixtures.
- Hard invariants: focus/refusal, no absence crisis, no raw private content, one canonical orb, bounded objects/contacts/windows, no Morph topology or action-count change.
- Performance thresholds: object physics p95 below 0.20 ms at 120 Hz; episode p95 below 0.15 ms at 20 Hz; renderer delta below 0.60 ms; Morph p95 regression below 10%; process memory below 150 MB.
- Promotion: only after workspace format/Clippy/tests/release builds, platform scripts, scenarios, privacy scan, captures, and independent review are green.

## Migration and rollback

- Missing ecology state creates a deterministic schema-v1 default from the identity seed.
- Invalid primary loads the last valid backup; invalid candidate state is rejected or repaired to a traced safe state.
- Existing `PortablePetState v1`, Morph state, liquid tuning, and their backups remain independently loadable.
- Removing or disabling ecology returns the exact existing brain/body path; the crate does not own the organism identity or current save envelope.
- `builds/current` remains untouched until the final gate.

## Reversal condition

Revisit this decision if the crate requires a second homeostasis/action authority, inactive pass-through changes existing traces, performance budgets cannot be met with fixed capacities, or cross-feature composition cannot outperform isolated scripted behavior in causal story recall.

## References

- `.solo-studio/HABITAT_IMPLEMENTATION_INVENTORY.md`
- `.solo-studio/BRAIN_SUBSYSTEMS_FUSION_PLAN.md`
- `.solo-studio/VITA_ITERATION_2.md`
- `PROJECT_REFERENCE.md`
- `PET2_LIVING_DESKTOP_HABITAT_CODEX_PLAN.md` (external supplied plan)
