# Fly-brain research → Pet 2: a bounded transfer experiment

Research checked 2026-09-13. This is an architecture recommendation, not an implemented brain replacement. Primary papers, author repositories and author technical reports were inspected. No external code/data installed; no animal/consciousness claim.

## User clarification: MaleCNS specifically

Yes: **MaleCNS**, not merely the older female FlyWire brain. The official project describes an entire male central nervous system: central brain, optic lobes and ventral nerve cord connected through an intact neck. That wider sensor-to-motor coverage is materially relevant to future embodied experiments. It remains an anatomical dataset, not a pretrained universal behavior engine. The official timeline says **v1.0 released 2026-06-08; paper published 2026-09-03**. Do not call September 3 the first dataset release. Dataset license is CC BY 4.0. [Official project](https://male-cns.janelia.org/), [release notes](https://male-cns.janelia.org/release/), [anatomical scope](https://www.janelia.org/project-team/flyem/male-cns-connectome).

### What the current game demos actually demonstrate

**Doomfly — primary code available.** The author's README says the current v6 candidate **failed visual, conditioning and survival validation gates**; changing weights is not demonstrated learned survival. It retains 166,700 neurons and 25,582,938 directed graph connections, drives modeled visual inputs with game frames and uses an engineered fixed button mapping. Original project code is MIT; third-party materials have separate terms. Python 3.11/C++ and several GB RAM are required; the browser is only a viewer. [Author repository](https://github.com/nftechie/doomfly).

The detailed live protocol identifies 124,177,617 anatomical contacts represented in that graph and plasticity on only 4,184 existing KC→MBON11 connections. Nonfatal game damage triggers a modeled 200 ms PPL101 input; an adapted dopamine-gated rule updates those connections. This is deliberately experimental conditioning, not proof that the model understands Doom. Neural dynamics step at 0.1 ms; plasticity at at most 10 ms bins. These are simulator design choices, not measured dynamics of this specimen. [Author training protocol, 2026-09-05](https://github.com/nftechie/doomfly/blob/main/docs/doom-live-training.md).

**Mario/Fly64 — primary code available.** The author explicitly says there is **no training, reward or star-collection goal**. Visual input runs at 10 Hz, simplified neural dynamics at 50 Hz; selected descending groups map to movement, steering and jumping through hand-written smoothing/gates. It can remain stuck at walls. The author tested one M2 MacBook with 16 GB RAM; about 1.1 GB disk is needed for brain data. Code is described as unreviewed hobby work. No reusable code license was verified, so inspect ideas without copying implementation. [Author repository](https://github.com/ornata/fly).

**Beat Saber:** the viral claim was located, but public primary implementation/training artifacts were not verified in this search. Do not assert autonomous visual play, general learning, measured skill or transferable weights from the clip. It is not an engineering dependency for Pet 2.

### A concrete MaleCNS option, not a generic dismissal

If we test the actual dataset next, build an **offline MaleCNS circuit probe** beside Pet 2, not a replacement for the running pet. Pin v1.0 download hashes from the official connectivity/annotation tables; retain original neuron IDs, anatomical contacts and graph-edge counts separately; document inferred transmitter signs and every manually assigned input/output. Start with a bilateral visual-orientation or touch/grooming circuit plus its relevant VNC path, exporting a reproducible subgraph and explicitly reporting its boundary (not calling it “the whole brain”).

Feed only simulated object bearing/motion and contact events. Read out two orientation drives and one inspect/groom drive into the same bounded semantic interface as Morph. First run in shadow mode. Compare real wiring against degree-preserving rewiring and the existing Morph network under matched input gain, dynamics, initialization and compute budget. The decisive test is whether removing the relevant input or pathway selectively changes the expected behavior, and whether the true graph improves contact-compatible orientation on held-out scenes. If it cannot, keep the simpler controller. This plan does not assume a fly's learned body coordinates transfer to our creature.

Only after reproducible improvement should it become an optional runtime adapter or distilled readout. Do not import a full dataset during this animation repair or erase existing neural memory. Doomfly's published negative controls are especially useful design guidance: test sensory validity before celebrating changing weights.

## Decision

Keep Pet 2's existing neural/homeostatic architecture and physically constrained motor controller. First test a small, reversible associative predictor/readout for **which nearby object to inspect and whether to sniff, touch or grasp it**. Do not put a 140,000-neuron fly simulation in the frame loop to fix blinking, support contacts or facial asymmetry. Those failures need actuator ownership, continuous motor trajectories and contact feedback, regardless of brain size.

This is an engineering hypothesis: sparse recurrent state plus bounded outcome learning may improve contextual continuity. It is not evidence that a biological fly circuit will improve a stylized desktop organism.

## What actually exists

| Work | What it does / learns | Important limit |
| --- | --- | --- |
| Shiu et al., Nature, 2024-10-02 | Whole-brain connectome-based leaky-integrate-and-fire model; sensory stimulation predicts feeding and antennal-grooming circuit responses. | A functional hypothesis built on anatomy and inferred transmitter signs, not a measured copy of every dynamic parameter, memory or complete embodied repertoire. |
| Lappalainen et al., Nature, 2024-09-11; flyvis | Connectome-constrained visual-motion network: 64 cell types, 45,669 modeled neurons, 734 free biophysical parameters. Trained on optic-flow estimation; compared with neural measurements across 26 studies. | A visual circuit trained for motion estimation, not a whole-brain pet policy. |
| Vaxenburg et al., Nature 2025; flybody | Anatomically detailed MuJoCo body; walking, flight and vision-guided flight tasks, reinforcement-learning tools. | Body model and learned controllers do not constitute a reconstructed fly brain. |
| Eon technical report, 2026-03-10 | Combines connectome LIF brain, visual model, NeuroMechFly body, low-dimensional descending readouts and existing imitation-trained body controllers; demo includes foraging, feeding and grooming. | Authors state plasticity/learning/internal state are largely missing, brain-body mappings are hand-chosen, visual activations currently have little behavioral influence, embodied escape is not yet implemented, and internal dynamics are not biologically validated. |
| Jin et al., FlyGM preprint, 2026-02-20, v3 2026-06-14 | Uses fly connectivity as graph architecture; expert-policy imitation followed by PPO trains gait initiation, walking, turning and flight control. | Task-trained graph policy, not simply activating the connectome and obtaining all fly behavior. Separate walking/flight input/output configurations; transfer to our morphology untested. |

Sources for the rows: [Shiu paper](https://www.nature.com/articles/s41586-024-07763-9), [Lappalainen paper](https://www.nature.com/articles/s41586-024-07939-3), [flybody author repository](https://github.com/TuragaLab/flybody), [Eon technical disclosure](https://eon.systems/updates/embodied-brain-emulation), [FlyGM v3](https://arxiv.org/html/2602.17997v3).

Structural connectome ≠ complete functional model ≠ trained policy ≠ transferred personality. Anatomical synapse counts also differ from aggregated weighted neuron-to-neuron graph edges: do not compare repository edge counts to anatomical synapse counts as if they used identical units.

There is counterevidence to a blanket “biological topology wins” claim: Dhiman's 2026-04-05 preprint finds apparent flyvis topology advantages can disappear with matched initialization and degree-preserving graph controls. This is a limited computational study, not a refutation of every connectome model; it makes fair baselines essential. [Primary preprint](https://arxiv.org/abs/2604.04033).

## Engineering and distribution constraints

- `flybody` advertises Apache-2.0. Its basic Python/MuJoCo install is separated from optional TensorFlow/Acme and Ray training dependencies. This is useful offline tooling, not a drop-in Rust dependency. [Repository](https://github.com/TuragaLab/flybody).
- Eon's brain repository advertises GPL-2.0 and Brian2 CPU plus multiple CUDA backends. Its NEST GPU instructions need a custom source build and NVIDIA CUDA; Windows instructions include WSL2. These are repository facts, not a local performance result. Do not copy/link code into Pet 2 without a license review; a paper's open-access license does not automatically license implementation or datasets. [Repository and setup](https://github.com/eonsystemspbc/fly-brain).
- FlyGM's reported comparison allocates a single NVIDIA A100 80 GB GPU for graph baselines; this is a training experimental setup, not a demonstrated minimum inference requirement. No laptop/Apple Silicon frame-time claim follows from it. Model/checkpoint redistribution terms were not verified. [Appendix H](https://arxiv.org/html/2602.17997v3#A8).
- No external model was benchmarked on this user's computer. Whole-brain runtime RAM, battery cost, deterministic parity and macOS support remain unknown. Do not make them a dependency of this repair.

## Existing local integration points (read-only inspection)

- `crates/morph_brain/src/lib.rs`: same-process Rust port pinned to Thandorcat/morph commit `6aa4e7c871c11ff2fa1619942611d4fb50457e49`. Already has a spiking network, habituation, reward-linked plastic pathways, object attention and commands including Sniff, Grasp and Release.
- `MorphBrain::tick_with_world` transduces sensors/body/life/world, applies modulators, steps neural dynamics and updates plasticity every 100 ms. `acknowledge_execution` explicitly credits physically executed commands; `apply_feedback` clamps reward; `snapshot` preserves neural state.
- `MorphWorldInput` provides six optional object slots, selected slot and eight affordance action biases. Biases are suggestions to neural competition, not a second motor authority.
- `crates/lifecore/src/microbrain.rs`: existing 64-unit recurrent MicroBrain, 32 inputs, ten populations, bounded eligibility plasticity, normalization and divergence recovery. Therefore “add a tiny neural brain” duplicates machinery already present.
- `app/src/nervous_system_runtime.rs` is the brain/body adapter; `crates/pet_motor` owns motor commitments and `app/src/ecology_runtime.rs` owns actual object physics. Keep those boundaries.

## Options

**Minimal — recommended first:** reuse existing Morph activity as context, add a tiny per-action success predictor in shadow mode. Learn only from completed physical outcomes, e.g. did soft grasp maintain attachment for 0.5 seconds, did placement settle without reacquisition? Predict success before execution; do not alter actual grip force or world coordinates. Expected cost is small, but measure it.

**Balanced:** add a sparse fixed 64-unit context expansion/readout only if existing Morph/MicroBrain features fail the baseline. Train a bounded output layer for inspect/sniff/touch/grasp variants; retain explicit motor programs. This is insect-inspired associative engineering, not a literal mushroom-body reconstruction. Do not call random recurrent activity “emergence.”

**Frontier/offline:** run a licensed connectome-derived small circuit or FlyGM-style teacher in an isolated research harness, compare with equally sized random/degree-preserving networks, then distill any measurable benefit into a tiny Rust model. Whole-brain experiments should stay out of the released pet until they justify their integration and cross-platform cost.

## Exact first experiment: soft-object attention continuity

Hypothesis: outcome-conditioned neural readout reduces abandoned object interactions and improves grasp readiness without increasing behavioral switching or unsolicited bids.

1. Add experimental `AssociationPredictor` behind a default-off flag in the nervous-system adapter. No change to existing saved-brain schema. In shadow mode, predictions and proposed biases are telemetry only.
2. At 10 Hz read 16 bounded features: normalized target distance, relative speed, target motion, familiarity, affordance confidence, contact, carried flag, support flag, novelty, fatigue, arousal, stress, current commitment progress, previous success, time since interaction, and explicit touch input. Use already-authorized local simulation state, not camera/screen contents.
3. Four outputs predict success of inspect, sniff, touch and grasp within the next second: `p_a = sigmoid(w_a · x + b_a)`. Train only the actually completed, eligible action using `w += eta * (y-p) * x`, eta=0.002, each weight clamped [-0.5,0.5]. Binary y is an objective completion criterion; sniff/inspect require a valid target and completed bout, not merely firing a command. Never reward raw user attention or self-generated triggers.
4. Credit only outcomes with a matching action ID and target ID. Interrupted, ambiguous, expired or changed-target episodes receive no update. Maximum one update per completed bout. Keep a 128-event bounded replay buffer of abstract features/outcomes; no raw user content.
5. After calibration, optionally turn predictions into a confidence-weighted bias limited to ±0.08 in `MorphWorldInput.action_biases`. Biases cannot override supported rest, danger, refusal, quiet mode, active carry, invalid affordances or the motor minimum commitment. Physical controllers remain unchanged.
6. Log model version, seed, features, predictions, target/action IDs, objective outcomes, candidate bias and actual selected command. Record baseline and treatment against identical event streams.
7. Predefine 50 seeds × 20-minute headless scenarios: moving/stationary orb, den-edge contact, fatigue, cursor distractors, missed contact, interruption, frame-rate variation. Compare frozen predictor, online predictor, and simple exponentially averaged per-action success baseline. If later testing topology, add degree-preserving rewiring from the same initialization.
8. Promotion gates (engineering thresholds, not achieved results): no safety/quiet/support regression; fewer abandoned valid object bouts; no increased switches per minute; prediction Brier score at least 10% below constant-frequency predictor on held-out seeds; p99 predictor time under 0.1 ms at 10 Hz; repeated identical replay produces identical output. Subjective quality still requires real-time A/B viewing and multi-day use.
9. Rollback: flag off returns existing behavior immediately; keep candidate weights in a separate versioned experimental file, with a last-good checksum. On non-finite state, runaway update count, performance regression or failed gates, discard candidate only. Never reset the user's pet identity, state or existing Morph snapshot.

## What to do in this repair

Deliver contact/rest/blink fixes and organic face motor control first. The useful lesson from current embodied-fly work is a closed sensor→brain→bounded motor→sensor loop with meaningful feedback, not neuron count. Start the predictor only after those feedback signals are trustworthy; otherwise it learns the physics bugs. No implementation of this experiment is claimed by this report.
