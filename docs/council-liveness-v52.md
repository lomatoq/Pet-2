# V52: council review of coherent, persistent companion behavior

Date: 2026-09-27. Scope: a bounded improvement to causal behavior and continuity across sessions, not a new claim of consciousness, biological fidelity, or guaranteed long-term engagement.

## What the review found and changed

| Evidence in the previous code | Change | Observable consequence |
| --- | --- | --- |
| `EpisodeDirector` reconstructed learned game and placement evidence from defaults at launch | Validated, additive `EpisodeMemory` in ecology persistence; runtime snapshots export the live director and restore it on load | Actual repeated acceptance, refusal, and successful placement remain relevant after restarting. New contradictory evidence can reverse bounded preferences. |
| The same global six-second formula changed wandering targets regardless of arrival | Identity-specific spatial memory uses observed occupancy, familiarity, recency, current curiosity, fatigue-related needs, and stress; a route continues while making progress | The creature can finish approaching and inspecting a place. Repeated obstruction changes the next destination without permanently banning it. |
| A no-contact `OfferOrb::Approach` never relinquished control, including in a seven-session replay | Existing approach commitment now bounds failed acquisition; a retry guard follows timeout | An inaccessible toy cannot monopolize behavior forever. Timeout is not recorded as user rejection. |
| Holding the orb unconditionally nominated a chase | Current play interest is compared with recovery costs, with hysteresis for an ongoing chase; a tired animal can inspect quietly and later accept | A toy offer is an invitation, not a command. Disengagement does not damage learned preference or attachment. |
| Interception misses increased prediction confidence | A failed attempt reduces confidence before retry | Repeated failure no longer reports rising confidence. This alone does not prove higher catch success or calibrated motion prediction. |
| Final gaze could borrow the pet's own velocity as target motion; social referencing alternated indefinitely | Motion is estimated from consecutive positions of the same orb with reset guards; one referential glance returns to the object and requires a meaningful change to rearm | A stationary orb remains a stationary prediction target. An unchanged invitation does not create perpetual eye ping-pong. |

Primary implementation locations: `crates/lifecore/src/exploration.rs`, `crates/lifecore/src/lib.rs`, `crates/lifecore/src/persistence.rs`, `crates/pet_ecology/src/episode.rs`, `crates/pet_ecology/src/state.rs`, `app/src/ecology_runtime.rs`, `app/src/nervous_system_runtime.rs`, and `crates/pet_body/src/gaze_controller.rs`.

Readiness weights and the quiet inspection pose are an engineering hypothesis, not a simulated animal hormone system. Sleep, focus, protection, and active food episodes retain priority.

No new vocal solicitation loop is part of this change. Quiet behavior and accepting refusal remain valid outcomes; more motion or more sounds are not success metrics.

## Research basis and epistemic limits

**Supported, with limited transfer:** animal social referencing can involve alternating attention between an object and a social partner, and responses depend on context. This motivates a recipient-related checkback, not a perpetual animation. The desktop cursor is an interface convention, not a biological social partner. [Merola et al., Dogs’ Social Referencing towards Owners and Strangers (2012)](https://pmc.ncbi.nlm.nih.gov/articles/PMC3469536/).

**Research model, not biological fact:** computational intrinsic motivation can favor learning progress rather than either random novelty or endlessly repeating fully predictable activity. V52 implements a small familiarity/recency heuristic; it does not implement the paper's learning-progress architecture or dopamine hypotheses. [Kaplan and Oudeyer, In search of the neural circuits of intrinsic motivation (2007)](https://www.frontiersin.org/journals/neuroscience/articles/10.3389/neuro.01.1.1.017.2007/full).

**Design precedent:** believable-agent work argues for integration of goals, emotion, reactive behavior, and social behavior rather than isolated expressive effects. This is a useful architecture precedent, not evidence that this version will delight every user. [Reilly and Bates, Emotion as part of a Broad Agent Architecture (1993)](https://www.cs.cmu.edu/afs/cs/user/wsr/Web/research/waume93.html).

**Engineering hypotheses:** twelve spatial regions, evidence weights, dwell intervals, retry bounds, and the single-checkback policy are authored design choices. Their numerical values are not derived from animal physiology. They require observation and tuning against actual use.

## What tests establish

`crates/pet_ecology/tests/longitudinal.rs` separates active simulation from elapsed calendar time:

- Seven daily ten-minute sessions integrate the director and metabolism at 20 Hz (84,000 active ticks total), with offline gaps handled explicitly. Two identical replays must match. The fixture deliberately supplies no fictitious contact; it exposed the unbounded approach failure.
- Thirty minutes of focus must not initiate toy offers, solo-play episodes, shared-attention solicitations, or ecology vocal triggers.
- Thirty daily save/restore cycles preserve identity and interaction evidence, reject duplicate reinforcement after restart, and allow subsequent refusals to change the actual next behavior. Offline time must not invent learned outcomes.
- Twenty-four hours of director/metabolism ticks remain finite and bounded. This is not a renderer, audio device, or full liquid-physics simulation.
- Fourteen-day offline recovery preserves the orb and does not manufacture a starvation crisis.

The runtime test `interaction_learning_survives_real_store_reload_without_motor_phase` verifies the actual state-store path, legacy-save compatibility, deduplication after restart, and that active motor phases are not resumed as if uninterrupted.

Exploration tests use three identities, measured kinematic occupancy over ten active minutes, healthy-progress continuity, obstruction recovery, personality/state contrasts, and thirty days of repeated observation plus explicit offline recovery. These are policy simulations, not native desktop collision or subjective user studies.

Gaze tests distinguish an unchanged invitation, tiny cursor noise, meaningful partner movement, stationary-object motion, object replacement, and teleport resets. Rendered perception of the gesture still needs real-device observation.

## Remaining gaps, deliberately not hidden

- The orb's 24 approach/contact variants still include authored sequencing. A repertoire count is not evidence of adaptive skill discovery.
- Persisted `ObjectMemory.prediction_error_ema` has no observed production learning loop in this audit; it must not be advertised as an active world model.
- Touch-side preference still uses lifetime counts and can become entrenched. Bounded reversible recent evidence is a future improvement.
- Spatial regions use normalized desktop coordinates. Native multi-monitor projection and moving-window obstructions require separate runtime scenarios.
- Lower interception confidence is semantically consistent, but signed prediction-error learning and catch-rate comparisons remain unimplemented.
- Long-lived familiarity can saturate. Months of adaptive competence, changing preferences, meaningful new affordances, and perceived novelty require further work and observation.
- Accelerated days and repeated reloads demonstrate invariants and persistence. They cannot establish that the companion remains interesting after ten minutes, several days, or months.

## Practical follow-up evaluation

Use the same identity across several ordinary sessions, preserve state, and record actual episode outcomes rather than counting nominated actions. Include ten minutes of no input, work/focus, a playful toy offer, a fatigued or satiated offer, a reachable and obstructed toy, a restart, and a changed preference. Check for coherent acceptance or disengagement, arrival without timer-driven reversals, quiet intervals, recovery from failure, and successful state restoration.

For a longitudinal user study, ask whether the user can correctly infer the current intention and why behavior changed; record repeated-action fatigue and unwanted interruptions. Do not optimize for attention capture or treat the user's absence as rejection.
