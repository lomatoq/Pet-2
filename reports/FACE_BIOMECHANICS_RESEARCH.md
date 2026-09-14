# Procedural face biomechanics: evidence and implementation

Date: 2026-09-13. Scope: current macOS-parity/R14 worktree; source inspection, primary-paper research, and a small bounded gaze/brow patch. This is not a claim of biological realism or a completed full face redesign.

## Decision

Keep the existing low-dimensional procedural face. Add causal, time-dependent, bilateral muscle-like controls around it, not a large neural mesh model. Independent control means each side can respond differently within a coherent action; it does not mean independent random motion on every frame. Contact/rest fixes remain prerequisite: expressive eyebrows cannot repair a floating root or unstable grip.

## What the evidence supports

1. **Blink shape, not merely blink rate.** Trutoiu et al. measured high-speed recordings and compared animated blink variants on realistic and cartoon faces. Their data-based, fully closing blinks were rated more natural than textbook alternatives. Fast closing and slower reopening matter, as does lower-lid motion. This supports unequal closing/opening phases, not continual asynchronous blinking. [Modeling and Animating Eye Blinks, 2011](https://la.disneyresearch.com/wp-content/uploads/Modeling-and-Animating-Eye-Blinks-Paper.pdf).

2. **Two eyes normally share a blink event.** Stava et al. measured strong bilateral coupling of spontaneous blink timing, with negligible onset differences; occasional amplitude differences did not remove duration coupling. Reserve conspicuous one-eye delay for a deliberate wink or tired/comic gesture. [Conjugacy of spontaneous blinks in man, 1994](https://pubmed.ncbi.nlm.nih.gov/7928197/).

3. **Lids follow gaze and have different mechanisms.** Evinger et al. found upper-lid elevation related to upward gaze and active-plus-passive closure during blinks. Map upward/downward gaze into lid shape, fading that contribution during full closure. [Eyelid movements: mechanisms and normal data, 1991](https://pubmed.ncbi.nlm.nih.gov/1993591/).

4. **Some large gaze changes recruit blinking.** Evinger et al. found blink-muscle activity strongly associated with large human gaze shifts. A desktop controller can request one blink on a large attention transition, but must still obey the single blink owner and refractory interval; a continuous tracking update must never re-request it. Human angular thresholds cannot be copied directly into normalized screen coordinates. [Not looking while leaping, 1994](https://pubmed.ncbi.nlm.nih.gov/7813670/).

5. **Organic gaze is an attention/motor system.** Pan et al. describe an animatronic architecture with attention habituation, layered behaviors, saccades, and different actuator bandwidths. Transfer the hierarchy and event ownership, not cameras or human-identification requirements: the pet already knows cursor, ball and den positions. [Realistic and Interactive Robot Gaze, 2020](https://la.disneyresearch.com/publication/realistic-and-interactive-robot-gaze/).

6. **Procedural local geometry can be sufficient.** Pinskiy and Miller use curved lid patches, gaze-dependent deformation and local eye primitives. For this 2D creature the transferable principle is continuous shape deformation constrained to an eye boundary, not their complete 3D skin algorithm. [Realistic Eye Motion Using Procedural Geometric Methods](https://media.disneyanimation.com/uploads/production/publication_asset/66/asset/realisticEyeMotion.pdf).

7. **Muscle-like actuation is distinct from geometry.** Lee, Terzopoulos and Waters demonstrate a skin/muscle/jaw model driven by contractions. We should retain an action-control space separate from the visible curves; a real human skull or expensive tissue simulation is unnecessary for a liquid creature. [Realistic Modeling for Facial Animation, SIGGRAPH 1995](https://web.cs.ucla.edu/~dt/papers/siggraph95b/siggraph95b.pdf).

8. **More asymmetry is not automatically more natural.** Schmidt et al. analyzed 64 participants and found deliberate smiles larger/faster and more asymmetric in aspects of onset/offset than spontaneous smiles. Other studies report side-specific differences in particular contexts. Use bounded, context-dependent asymmetry and variable onset/offset; do not impose a permanent exaggerated left/right skew and call it human biology. [Movement Differences between Deliberate and Spontaneous Facial Expressions](https://pmc.ncbi.nlm.nih.gov/articles/PMC2668537/); [Differences in Facial Expressions between Spontaneous and Posed Smiles, 2020](https://pubmed.ncbi.nlm.nih.gov/32098261/).

9. **Style is a separate axis.** Zoss et al.'s learned implicit physical face model separates expression and performance style, with training and identity constraints. The useful immediate mapping is separate pet identity coefficients from expression intensity; importing their neural physics is not a same-day drop-in. [An Implicit Physical Face Model Driven by Expression and Style, 2023](https://studios.disneyresearch.com/2023/11/29/an-implicit-physical-face-model-driven-by-expression-and-style/).

10. **Laughing has respiratory structure.** Kret et al. studied developmental differences in inhaled/exhaled laughter and perceived positivity. This supports coupling laughter to a breath phrase rather than arbitrary mouth flapping, not a universal face waveform. [The ontogeny of human laughter, 2021](https://pubmed.ncbi.nlm.nih.gov/34464539/).

All numerical tuning below is an **engineering hypothesis**, not a measured biological constant. Whole-eye size expansion and temporarily divergent eyes are **fictional conventions** for expressiveness.

## Existing implementation and exact integration points

| File | Present capability / limitation | Next use |
|---|---|---|
| `crates/lifecore/src/face_geometry.rs` | Per-side lids `[inner, outer, lower raise, curvature]`, brows `[inner, outer, arc, thickness]`; separate mouth corner heights already exist | Reuse, do not duplicate a second face schema |
| `crates/pet_body/src/companion_expression_director.rs` | `FaceTarget` has shared brow/mouth parameters and asymmetry; actual audio owns mouth aperture while phonating | Derive a bilateral face actuation target once from evidence + gesture phase |
| `crates/pet_body/src/expression.rs` | Smooths expression fields including geometry, eye scale, brow and mouth asymmetry | One smoothing stage with event-dependent time constants; avoid repeated filtering of fast startle |
| `crates/pet_body/src/gaze_controller.rs` | One gaze target, social checkback, prediction; sinusoidal micro-offset was fed into persistent tracked position | Fixed below; future target arbitration + fixation/saccade phases |
| `crates/pet_body/src/embodiment.rs` | Managed blink ownership; shared eye scale clamped 0.88–1.18 and smoothed; geometry passed into pose | Keep one blink owner; audit all eye-scale clamps before adding a startle overshoot |
| `crates/pet_body/src/liquid_surface.wgsl` | Eight-span brow curve, constant width; mouth arc has independent corner offsets; eyelid shader | Slim tapered brows now; future upper/lower lip contours with collision clamp |
| `app/src/nervous_system_runtime.rs` | Expression/face actuation projection and gaze assignment | Final reconciliation boundary: physiology > reflex > intentional gesture > idle style |

## Small patch implemented in this task

### Gaze stability

The old recurrence was effectively `current = smooth(current, target) + offset(t)` every tick. Its steady offset grows approximately like `offset / (1-exp(-dt/tau))`; changing FPS therefore changes eye drift. Now the persistent fixation receives only tracking, and a bounded offset is added to the returned presentation target. A deterministic 12-second test compares 30/60/120 Hz, asserts per-axis offset <= 0.01001, and verifies the tracked state stays at the target.

This fixes a control bug; the remaining sinusoid is still a stylized micro-motion, not a biological microsaccade model. Future work should replace it with sparse small fixation events, not layer another noise source on top.

### Slim tapered eyebrows

The eight existing curve segments and independent left/right shape controls remain. Radius now varies with closest position along the curve:

`radius(t) = old_radius * 0.68 * [0.48 + 0.52 * (4*t*(1-t))^0.55]`.

This retains 68% of previous thickness centrally and roughly 33% at tips. Existing opacity/contrast and user thickness multiplier remain unchanged. The nearest-point projection makes taper follow a tilted eyebrow. Actual desktop-size visual validation is still required; passing shader validation alone does not demonstrate attractive appearance.

## Next expressive layer: implementable contract

### 1. Bilateral actuation with stable identity

For each region, keep `a_L`, `a_R` in [-1,1] (or [0,1] for contraction). Shared cause `u`, identity bias `b`, and transient gesture asymmetry `g` produce:

`target_L = clamp(u + b + g); target_R = clamp(u - b - g)`.

Use |b| <= 0.04 normally; |g| <= 0.15 for ordinary expression and <= 0.4 only in a short deliberate grimace. Store identity bias once, never redraw it each frame. Temporal asymmetry should be a bounded phase offset within one event, not two unrelated event generators. Baseline phase delay <= 25 ms; a 60–120 ms lazy-eye gesture is explicitly cartoon behavior and must recover.

For a responsive stable scalar actuator use exact critically damped integration for constant target over dt. With `e=x-target`, `j=v+omega*e`, `r=exp(-omega*dt)`: `x=target+(e+j*dt)*r`, `v=(v-omega*j*dt)*r`. Start at omega 18–30 /s for brow/lip transitions; tune by visual replay. No stochastic force injection. Blinks retain their dedicated phase curve instead.

### 2. Brows

Drive inner raise, outer raise and arc independently on each side. Skepticism: one inner/outer pair rises, opposite remains near neutral. Concern: inner raises with outer lowering. Play: uneven onset then converging smile-brow release. Thinner stroke is a rendering decision, not a muscle command; keep it separate from expression intensity. Avoid huge vertical offset that detaches brows visually from eye sockets.

### 3. Gaze, convergence and local attention

Select a concrete target from the current action first: carried orb, upcoming contact, den entrance, recent cursor event; quiet social checkbacks second. Give a target 0.6–2 s dwell and habituate repeated unchanged stimuli. When no valid target exists, relax toward local neutral over 0.3–0.8 s instead of holding a stale far target indefinitely (the current no-target path retains `current`). No screen-content access is needed.

For future bilateral pupils derive both directions from one point: `dir_i = normalize(target3D-eyeCenter_i)`. The 2D desktop has no real target depth; set a documented virtual depth from interaction class, not pretend it is measured. Clamp inward convergence and pupil travel to the visible lid boundary. During a contact task, both eyes must keep the object readable. Brief eye-lag gestures may relax coupling only outside precision contact or threat.

Saccade trajectory can use `p(s)=10s^3-15s^4+6s^5` between fixation endpoints, with duration 60–140 ms as an art starting point. Then hold fixation. Eye movement precedes a slower face orienting motion, but neither may displace the physics root.

### 4. Startle and blink

Startle needs a rising-edge event and minimum refractory interval, not a permanent high-surprise input. Quick whole-eye scale 1 -> 1.16–1.18 over ~80 ms, short hold, ~350 ms recovery is already within the current scale cap. Pair with upper-lid opening and raised brows, not a blink that erases the cue. A genuinely protective closure outranks the art exaggeration. Larger scale needs coordinated changes to every cap, eye/brow spacing and mask bounds, so is not included blindly.

Keep ordinary blink event shared across eyes; closing ~60 ms and reopening ~140 ms are starting values, not research-fit parameters. Ensure full closure, asymmetric down/up velocity and no restart on sustained fatigue. Do not change the current corrected rate without a minute-long replay. Drowsiness can change resting aperture independently from complete blink events.

### 5. Mouth, laugh, grimace, sniff, sneeze

Upgrade the existing mouth to two curves sharing corners: `upper(s)` and `lower(s)` as cubic Beziers. Control corner L/R elevation, width, upper center, lower center/jaw, compression and lateral skew. Enforce `lower <= upper` in chosen coordinate convention and keep corners coincident. This prevents inverted apertures when one corner rises. Preserve current audio aperture ownership; smiles may move corners during phonation but not independently override its opening.

Laugh: pleasant play outcome -> inhale anticipation -> 2–4 exhale pulses -> breath recovery. Couple corner lift and lower-lid cheek raise to phrase strength, jaw aperture to real sound envelope. Silent chuckle gets a smaller explicit gesture envelope. Mouth asymmetry varies over the phrase and returns to identity baseline. Do not equate a grin with genuine internal emotion; it is a communicative act.

Sniff: inspect a new ecology object -> approach/orient -> two small inhalation-like mouth/body pulses -> pause -> update familiarity. Since there is no smell sensor, label this a simulated inspection affordance, not detection of real odors. Sneeze: only a rare fictional irritant event in ecology -> anticipation/squint -> one closure + compressed recoil -> recovery. No constant random sneezing, fake illness or real-health inference. Both require cooldown and prohibit new root offsets while supported against a wall or carrying an orb.

## Verification and release gates

- Deterministic 30/60/120 Hz face replay, finite parameters, no per-frame random walks.
- One blink owner; repeated level inputs cannot restart closure; wake releases eyes; protect outranks a wink.
- Target-loss relaxation; stationary target does not drift; no constant crossed eyes; gaze remains on the held object during handoff.
- Face geometry extremes: every pose, both asymmetry signs, max mouth compression, closed lids, tiny/large user scales. Shader parse/validate and image contact sheets at actual desktop pet size, not only enlarged portraits.
- A/B replay: old face vs bounded asymmetry vs excessive asymmetry; ask which action/emotion is readable, and separately which is annoying. Numerical stability cannot establish appeal.
- Keep changes transient/save-compatible. Budget target under 0.1 ms CPU per face update; measure rather than assume. No neural assets or new sensors required.
- Scope boundary: this report does not claim to implement the complete mouth/eye gesture system or to verify macOS GPU appearance.

## Live-feedback follow-up: downstream ownership defect

The user reported severe eye jitter and repeated whole-face up/down motion after the first build. The output-only micro-offset fix was insufficient: `embodiment.rs` still chose legacy gaze modes from mood/pose after the director had chosen a target. Those modes substitute a large sinusoidal scan, an abrupt side glance, or direct-viewer gaze. The same layer also generated a second set of microsaccades. This provided a concrete route for a calm semantic target to become changing rendered eye motion.

For the managed companion path, the presentation now follows the supplied semantic target (or relaxes to neutral when absent), without legacy mood-based mode replacement or a second micro-motion generator. A constant critically damped response replaces arousal-like frequency jumps. The legacy standalone/Lab path remains available. The existing `managed_blink` ownership flag currently identifies this managed path; a future rename to explicit managed-face ownership would clarify its wider role.

The director now ignores a cached contact point when there is no measured contact pressure or relevant contact intent; otherwise it could repeatedly steal gaze from an object. Target loss defaults to the body position, giving neutral local direction. Ordinary far-away targets must remain consistent for 120 ms before acceptance; interception and avoidance retain immediate acquisition. High uncertainty no longer overrides the Sleep gaze mode. Supported bodies no longer derive a flight face offset from residual contact velocity. This does not hide upstream physical motion bugs; those are fixed separately in the desktop constraint/motor pipeline.

New tests exercise alternating distant targets at 30/60/120 Hz, actual `pose.gaze` settling despite flickering legacy pose hints, neutral target-loss recovery, and zero face-attention bob under alternating supported-contact velocities. Render telemetry `blink = 0.14` can also mean 14% aperture closure from a semantic expression, not a BlinkController blink event; diagnosis must inspect both channels before changing blink timing again.
