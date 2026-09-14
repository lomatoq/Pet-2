# MaleCNS training lessons and a testable Pet 2 curriculum

2026-09-13. Design only: no training run, superiority result, model installation or production brain replacement is claimed. Companion architecture/learning/research skill references and the local Morph/MicroBrain interfaces were inspected. See `FLY_BRAIN_TRANSFER.md` for dataset distinctions and licensing.

## Recommendation

To do better than the viral demos, first demonstrate **useful sensory discrimination, correctly timed credit and held-out physical skill improvement**. More neurons, changing weights or an attractive video do not establish any of these. Keep the current physics and motor authority; train a small bounded action/readiness readout offline. An actual MaleCNS circuit can later compete in precisely the same harness.

## What was trained, and what failed

The original Doom learning candidate added dopamine-gated presynaptic eligibility on existing KC→MBON connections. Its visual assays left T4/T5 and Kenyon cells silent, while direct KC stimulation produced nonselective changes. Its initial survival pilot changed no memory weights. A reward-timing shuffle initially delivered unequal exposure because death truncated the scheduled pulse sequence; the corrected dose control still did not establish learning. [Author first-candidate review](https://github.com/nftechie/doomfly/blob/main/docs/doom-learning-review.md).

The subsequent author iteration log describes:

| Candidate | Intervention | Failure/lesson |
| --- | --- | --- |
| v2 | Separate neuromodulator channel; R8 input and target-specific sign assumptions | Visual recruitment appeared but selective conditioning failed. |
| v3 | Lower KC resting potential; exploratory learning-rate calibration | Direct stimulation improved under selected parameters, not held-out visual validation. |
| v4 | KC spike adaptation | Less activity did not produce cue-selective memory. |
| v5 | Calibrate baseline firing; centered bidirectional rule | No-imposed-punishment control mimicked the apparent conditioned response. |
| v6 | Combine adaptation and centered memory | Post-cue activity failed to recover; learned memory harmed one tested start. |

These are author-reported negative results, not independent replications. The source explicitly separates engineering checks from scientific gates. [Iteration log](https://github.com/nftechie/doomfly/blob/main/docs/doom-learning-iteration-log.md).

The saved v6 pilot used one training replicate and two held-out starts; it cannot support population-level comparisons. Its raw summary records learned/frozen/shuffled arms and retains failure status. Use its control structure as inspiration, not its sample size as a sufficient evaluation campaign. [Machine-readable evidence](https://github.com/nftechie/doomfly/blob/main/doom-ui/public/learning-iterations.json).

### The actual v5/v6 update mechanism

Code inspection, not merely the README: low-pass KC and DAN rates produce midpoint traces. The plasticity drive is proportional to instantaneous KC rate × filtered DAN signal minus instantaneous DAN signal × filtered KC rate, mixed by compartment gains. A slow memory state decays over 1,800 s and a second state follows it with a 50 ms filter. Efficacy stays within 0.1–2 times its original positive strength. Rate bins are at most 10 ms. This is a modeled timing-dependent, centered rule, not backpropagation or PPO. [Actual rule implementation](https://github.com/nftechie/doomfly/blob/main/doom_learning_v6/rule.py).

The live version schedules 200 ms artificial PPL101 stimulation after nonfatal damage, not before the causative game interval; plasticity affects only 4,184 existing KC→MBON11 edges. The code's actual spikes, not score, drive the rule. Reset cancels pending reinforcement to prevent crediting new imagery for an old round. [Live protocol](https://github.com/nftechie/doomfly/blob/main/docs/doom-live-training.md).

The neural adapter additionally retains explicit KC rest/adaptation assumptions and separate baseline rate parameters; checkpoints contain traces, membrane state, delays and efficacy state, not just final weights. This matters for valid replay and retention comparisons. [Actual brain adapter](https://github.com/nftechie/doomfly/blob/main/doom_learning_v6/brain.py).

### Contrast with genuinely task-trained FlyGM

FlyGM first imitates an existing expert policy, then fine-tunes using PPO. Its topology comes from a connectome, but learned encoders/readouts and message passing solve specified locomotor tasks. Walking and flight use different observation/action configurations; flight retains a wing-pattern generator. Thus the transferable training lesson is **stable expert initialization → bounded learning → physical feedback**, not expecting raw anatomy to discover our animation controls. This is preprint evidence, not validated transfer to Pet 2. [FlyGM v3 methods](https://arxiv.org/html/2602.17997v3).

## Lowest-cost implementable benchmark

Proposed new Rust-only headless test target, not existing commands:

```text
tools/association_lab
  fixture.rs       deterministic physical contexts/events
  evaluator.rs     objective outcome labels and reward vector
  predictor.rs     tiny readout, no new ML framework
  experiment.rs    seeds, training/evaluation split, manifests
  report.rs        CSV/JSON metrics and failure traces
```

Reuse `lifecore`, `morph_brain`, `pet_motor` and `pet_ecology`; if app-only runtime prevents direct reuse, expose its simulation adapter rather than building a second approximate physics engine. Keep renderer/native windows/audio out of training. The first test may call existing physics functions with fixtures and predict grasp readiness only; it need not instantiate the whole desktop.

Adapter input: the 16 normalized physical/context features defined in `FLY_BRAIN_TRANSFER.md`, plus optional existing Morph population rates. Initial predictor: four logistic heads for inspect/sniff/touch/grasp success, 68 parameters for the 16-feature version including biases. Train only executed action; do not add another recurrent network until this baseline is beaten.

Sampling: physical integration keeps the production fixed step; decision/readout at 10 Hz; learner updates once per completed bout. Outcome timestamps and body contact are authoritative. A raw neural request never counts as executed success. Reuse `acknowledge_execution`; tag each record with action ID, object ID, committed phase and context version.

## Curriculum with promotion gates

| Stage | Scenes | Required evidence before next stage |
| --- | --- | --- |
| 0: sensory/clock validity | Object left/right/near/far, moving/resting, absent, contact/noncontact; 30/60/120 Hz render schedules | Finite consistent physical features; missing target cannot report contact; changing render schedule does not change fixed-step labels. |
| 1: body competence | Static object; approach, slow pickup, hold, gentle placement, explicit quick throw | Existing deterministic controller completes each valid skill; resting body stays supported; no teleport/release recapture loop. No learner needed yet. |
| 2: readiness prediction | Vary relative speed, grasp distance, object radius/mass and den geometry within valid bounds | Held-out prediction calibration beats constant-frequency and per-action running-average predictors; labels depend on real completion. |
| 3: contextual choice | Same object cues predict different valid physical opportunities; reverse the association halfway | Learner improves valid choices, then reverses without corrupting old contexts; frozen and shuffled-outcome controls do not show equivalent gain. |
| 4: composition | Inspect→touch→grasp→carry→place; distractors and interruption | Fewer abandoned tasks, stable commitment, bounded switching; danger/refusal/rest win when required. |
| 5: embodied style | Gentle vs quick manipulation; wall contact/grooming only with valid support | Task success retained while jerk/contact impulse declines; expressive diversity measured separately, not rewarded indiscriminately. |

A stage fails if its upstream assumptions fail. Do not compensate for bad contact labels by stronger reward, more training or larger neural gains.

## Objective vector and formulas

All formulas below are engineering choices, not biological measurements. Keep components separately logged. Let H be screen-reference height, T bout duration, m object mass, v relative velocity in H/s. Let S be successful completion sustained for 0.5 s, D an unplanned drop, C an invalid body penetration event, U an unnecessary action switch before commitment ends. Only a new externally assigned trial goal creates a success reward; self-starting an identical micro-bout cannot farm it.

```text
q_success = S                              # binary, once per trial
q_softness = -min(1, sum(|contact_impulse|)/(m * 0.25 H/s))
q_jerk = -min(1, integral(|da/dt| dt)/(20 H/s²))
q_cost = -min(1, T / T_limit)
q_failure = -min(1, D + C)
q_chatter = -min(1, U / 3)

R = [q_success, q_softness, q_jerk, q_cost, q_failure, q_chatter]
r_train = q_success + 0.15*q_softness + 0.10*q_jerk
          + 0.05*q_cost + 0.50*q_failure + 0.20*q_chatter
```

Never use the weighted score to waive hard constraints: safety, quiet/refusal, valid contact, acceleration bounds and supported rest are admission gates. A failed gate rejects the episode/candidate regardless of score. Fast throw trials have a different target velocity and do not incur a “gentle contact” penalty for the authorized release impulse; limit penalties to unintended collision impulse. No reward for blinking more, eliciting clicks, appearing distressed or repeated user bids.

For prediction-only stage use binary cross-entropy/Brier score on S, not the multi-objective reward. For later discrete safe-variant ranking, use `Q_a += 0.01*(r_train-Q_a)` within context or a bounded linear contextual bandit. These simple baselines are cheaper to validate than end-to-end RL.

Reward timing: attach success/failure to the completed committed action and exact target. Cancel pending credit on target substitution or episode reset. An interrupted action is censored unless an explicit mechanical failure occurred. For delayed effects, a bounded trace `e <- exp(-dt/0.5 s)*e` can retain the executed action for at most 2 s; do not spread one outcome across all recent commands. Log scheduled vs actually delivered feedback separately.

## Fair controls and budget

Start with 5 independent training seeds, 100 episodes each, maximum 20 simulated seconds per episode. Freeze candidate weights; evaluate on 100 new seeds, one fixed trial from every curriculum scene per seed. Keep 20 validation seeds for tuning, separate from the 100 final seeds. Publish the seed lists before running. Calibrate runtime with ten episodes first; initial CPU budget 15 minutes for a smoke experiment, then a capped two-hour comparison only if it passes. These are ceilings, not runtime estimates.

Baselines: (A) existing deterministic/Morph behavior frozen, (B) constant per-action success, (C) running-average predictor, (D) learned readout. Controls for D: outcomes shuffled within equal-duration context blocks, learning frozen with otherwise identical state evolution, memory-erased after training, and matched sensor removal. Compare paired scenes and bootstrap across training seeds, not simulation ticks. Memory-erasure must restore baseline behavior; ordinary reset must not accidentally erase memory in the retained arm.

Target gate: at least 10% lower held-out Brier score than the strongest simple predictor; for deployed action bias, a positive paired task-completion difference with no increase in invalid contacts, chatter or support failures. Report uncertainty and null results. No “better than other projects” claim without a shared task, inputs, output authority and compute budget.

Later actual-MaleCNS experiment: add the pinned circuit extractor and solver as another feature generator, matching observed inputs and semantic outputs. Compare same-sized degree-preserving rewiring, existing Morph rates and a cheap random-feature baseline; do not let the connectome arm receive richer sensors or a stronger decoder. Run independently from Pet 2; no full-graph download is needed for the first Rust benchmark. Reject if model/source licensing, bounds or performance are unresolved.

## Promotion and rollback

Store candidate readout weights/normalization/version/seed in a separate experimental artifact. No mutation of the user's save. Initially telemetry-only; enable influence only after offline gates, with ±0.08 maximum affordance bias and no motor-coordinate authority. One flag restores the frozen baseline. Divergence, corrupted checkpoints, exceeded CPU budget or new regression automatically disables the candidate. Follow successful offline tests with real-time blind A/B clips and several days of use: smoother physics alone does not prove more legible emotion or a better companion.
