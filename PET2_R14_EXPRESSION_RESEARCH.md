# PET 2 R14 — Expression / Companion Research Basis

This document defines the research basis for the R14 living-companion implementation. It is not a bibliography dump: every cited idea below is translated into a concrete constraint for Pet 2.

## 0. Product thesis

Pet 2 should create attachment through legible intentionality, reciprocity, recognition and shared routines. The target is not a constantly cute face. The target is a creature whose gaze, face, liquid body, motion, touch response and sparse vocalization consistently reveal what it noticed, what it wants, how certain it is and how the interaction changed it.

The implementation must therefore optimize for:

1. causal readability;
2. multimodal coherence;
3. social timing;
4. quiet presence;
5. individual preference learning;
6. bounded surprise and imperfect understanding;
7. visible continuity across sessions.

---

# 1. Human/animal bonding findings that matter for Pet 2

## 1.1 Gaze is social, but should be contextual rather than constant

Dog-human research consistently treats gaze toward people as a socially meaningful behavior. Dogs show stronger human-directed sociability and face-gazing than wolves in comparable socialized conditions. Owner-directed gaze can also function as contact maintenance or social referencing, especially when direct proximity is unavailable.

Implementation consequence:

- gaze is a first-class communication channel, not decorative eye motion;
- every gaze must have a target and reason;
- when uncertain, the pet may alternate target -> user proxy -> target, rather than staring at center screen;
- when a problem cannot be solved, a check-back glance can be used as a help/social-reference behavior;
- avoid permanent eye contact because it destroys meaning and can feel uncanny.

Sources:
- Bentosela et al. 2016, Sociability and gazing toward humans in dogs and wolves. DOI: 10.1002/jeab.191
- Wanser & Udell 2019, attachment and handler gaze in animal-assisted activity. DOI: 10.1016/j.applanim.2018.09.005
- Karl et al. 2020, dog-human relationship combining fMRI, eye tracking and behavior. Scientific Reports 10:22273
- Boada et al. 2026, behavioural manifestations of human-directed social motivation in dogs. Scientific Reports 16:4649

## 1.2 Bonding is strongly associated with perceived two-way communication

A PLOS One study on behaviors owners perceive as important to human-dog bonding found recurring themes around shared communication, including eye gaze, object-directed look-backs and being able to understand what the dog wants.

Implementation consequence:

The pet must visibly close social loops:

```
notice -> signal/act -> wait -> inspect user response -> update -> resolve
```

A one-way animation is not enough. `CHECK_RESULT` is mandatory for social episodes.

Source:
- Barber et al. 2022, Exploring behaviours perceived as important for human—Dog bonding and their translation to a robotic platform. PLOS One 17(9):e0274353.

## 1.3 Petting can be intrinsically rewarding; vocal praise is not a substitute

Feuerbacher & Wynne found dogs in their experiments preferred petting to vocal praise under several conditions. This does not mean Pet 2 should imitate a dog literally, but it supports making safe tactile interaction the strongest affiliative channel rather than making the creature chatter constantly.

Implementation consequence:

- direct safe touch may have a much larger social-reward weight than a vocal response;
- pleasant contact should often be rewarded with body behavior, not sound;
- silence during touch is a valid and often preferable response.

Source:
- Feuerbacher & Wynne 2015, Shut up and pet me! Behavioural Processes 110:47–59. DOI: 10.1016/j.beproc.2014.08.019

## 1.4 Play needs recognizable meta-signals and pauses

Research on canine play bows indicates that play signals are strategically timed, commonly appearing around pauses and helping initiate/reinitiate play. Attention-getting signaling also adapts to whether the partner appears attentive.

Implementation consequence:

- `InvitePlay` should be a distinct pre-play signal, not immediate chase;
- after a short play pause the creature may re-invite before resuming;
- if the user is not attending, the bid may become slightly more legible once, then withdraw;
- repeated ignored bids must not escalate into guilt or spam.

Sources:
- Byosiere, Espinosa & Smuts 2016, Investigating the function of play bows in adult pet dogs. Behavioural Processes 125:106–113. DOI: 10.1016/j.beproc.2016.02.007
- Mitchell 2025, Play bows by dogs in dog-human play. Interaction Studies 25(2):146–166. DOI: 10.1075/is.24016.mit

## 1.5 Human affection gestures can be ambiguous to animals

Research on human-dog communication warns that human gestures such as hugging, restraining and petting are not automatically positive or correctly interpreted. This strongly supports Pet 2's existing body-based classification approach instead of equating `mouse_down` with affection.

Implementation consequence:

- the same input can be pleasant, neutral or aversive depending on speed, strain, pressure, duration and prediction error;
- preference learning is contextual;
- hard holds and over-stretch must never be relabeled as affection merely because attachment is high.

Source:
- Walsh et al. 2024, Human-dog communication: How body language and non-verbal cues are key to clarity in dog directed play, petting and hugging behaviour by humans. Applied Animal Behaviour Science 272:106206.

---

# 2. Social-motivation model for Pet 2

Recent dog research separates human-directed social motivation into at least three useful functional categories:

1. **social orienting** — prioritizing socially relevant stimuli;
2. **social reward** — finding interaction intrinsically rewarding;
3. **social maintaining** — sustaining proximity/contact when useful.

Pet 2 should explicitly represent these separately.

Do not collapse them into one `social` float.

Recommended signals:

```rust
pub struct SocialMotivationState {
    pub orienting: f32,
    pub reward_expectancy: f32,
    pub maintaining: f32,
    pub reunion_interest: f32,
    pub bid_persistence: f32,
    pub contact_satiation: f32,
}
```

This solves several current pathologies:

- a sociable pet can notice the user without always approaching;
- enjoying a stroke does not imply infinite desire for continued contact;
- after interaction satiation, quiet companionship can remain high while petting solicitation drops.

Source:
- Boada et al. 2026, Scientific Reports 16:4649.

---

# 3. Appraisal rather than direct emotion triggers

Marsella & Gratch's EMA work is useful here because it frames emotion as appraisal over an interpreted relationship between agent and environment, with fast and slower dynamics emerging from inference and changing interpretations.

Pet 2 translation:

Raw input must never directly drive an expression.

Wrong:

```
fast cursor -> fear face
```

Correct:

```
fast cursor
-> source + trajectory + directness + familiarity + predicted contact
-> appraisal(threat=.12, play=.70, novelty=.33, controllability=.88)
-> Invite/Chase intent
-> expression
```

The same cursor velocity can therefore mean play, irrelevant background work, or a threat depending on context.

Source:
- Marsella & Gratch 2009, EMA: A process model of appraisal dynamics. Cognitive Systems Research 10(1):70–90. DOI: 10.1016/j.cogsys.2008.03.005

---

# 4. Facial expression representation: borrow FACS concepts, not human caricature

FACS decomposes human facial behavior into action units rather than treating each emotion as a fixed mask. OpenFACS demonstrates a real-time implementation style where combinations of action units drive a 3D face.

Pet 2 does not have human facial anatomy, so it should use a **Pet Action Unit** model: orthogonal facial controls that can combine continuously.

Recommended PAUs:

```text
PAU01 inner/whole brow lift
PAU02 brow lower/tension
PAU03 brow asymmetry
PAU04 upper-lid raise / aperture
PAU05 lid compression / squint
PAU06 slow closure
PAU07 pupil dilation
PAU08 pupil focus/convergence proxy
PAU09 mouth corner/curve
PAU10 mouth compression
PAU11 jaw/aperture
PAU12 mouth asymmetry
PAU13 cheek/core warmth
PAU14 face-forward attention alignment
```

Expressions become targets in PAU space, not enum masks.

Example:

```text
curious =
  PAU01 +0.22
  PAU03 +0.08 toward target
  PAU04 +0.10
  PAU07 +0.12 novelty-weighted
  PAU09 +0.03
  head/body lean +0.12
```

This creates many expressions from a small set of interpretable channels.

Research / implementation references:
- Ekman & Friesen, Facial Action Coding System (foundational framework)
- Cuculo & D'Amelio 2019, openFACS: an open source FACS-based 3D face animation system
- https://github.com/phuselab/openFACS
- Rawal & Stock-Homburg 2022, Facial Emotion Expressions in Human–Robot Interaction: A Survey. International Journal of Social Robotics 14:1583–1604.

---

# 5. Never map one scalar emotion directly to one face

The survey literature on facial expression in HRI emphasizes that facial expression is a communication channel for emotion/intent, but perceived naturalness depends on the whole system, embodiment and multimodal consistency.

Pet 2 rule:

Visible expression is composed from:

```text
intent template
+ appraisal modifiers
+ measured physical effort
+ social target
+ physiological state
+ identity bias
+ authored temporal event
```

with strict ownership priorities.

`valence -> smile` is prohibited as the primary expression model.

---

# 6. Animation principles directly applicable to procedural behavior

Thomas & Johnston's classic animation principles remain highly relevant even for procedural agents. The most important for Pet 2 are:

## Anticipation

Before any major action, provide a small readable preparatory change.

Examples:
- intercept -> eye lead + slight compression;
- nuzzle -> gaze + body lean before contact;
- jump/dash -> mass loads opposite direction;
- reject contact -> local stiffening before withdrawal.

## Staging

Only one idea should dominate a moment.

Pet 2 translation: `PrimaryIntent` is staging. Secondary motion cannot compete with it.

## Follow-through / overlap

Liquid body makes this especially important. After locomotion stops, internal flow and local deformation may settle later than the face/gaze. Conversely, gaze often arrives before the body.

## Slow in / slow out

Use asymmetric temporal filters. Startle is fast-on/slow-off; affection is slower-on/slower-off; gaze acquisition is fast but disengagement is softer.

## Arcs

Cursor approach, nuzzle, chase and retreat should avoid mathematically perfect straight-line interpolation unless urgency demands it.

## Secondary action

One supporting detail is enough: a little glance, internal pulse or tail-like droplet. Do not add five unrelated micro-motions.

## Timing

Meaning is often timing rather than amplitude. A 120 ms freeze can read as surprise; a 700 ms freeze reads as fear or bug.

## Exaggeration

Because Pet 2's face is small, some expression targets need controlled exaggeration, especially brows and eye aperture, but only after silhouette readability tests.

References:
- Thomas, Frank & Johnston, Ollie. The Illusion of Life: Disney Animation (1981).
- Standard summaries of the 12 principles are also available through animation teaching resources from WKU / D'Source.

---

# 7. Bidirectional physical interaction is a major opportunity

Recent HRI work on physical social expressiveness (e.g. RoboBunting, 2026) argues that continuous bidirectional modulation of movement/impedance can convey personality, intentionality and aliveness more effectively than simple trigger-response motion.

Pet 2 already has the core technical advantage for this: deformable liquid contact.

Translation:

During touch, do not run a preset animation beside physics. Continuously modulate:

- local compliance;
- tangential following;
- center-of-mass bias;
- local viscosity;
- rebound;
- gaze to contact;
- contact-area persistence.

The creature should physically co-produce the interaction.

Source:
- Guta & MacLean 2026, RoboBunting: Building Social, Affective Expressiveness through Bilateral Force Exchange. ACM Transactions on Human-Robot Interaction. DOI: 10.1145/3807949.

---

# 8. Expression should communicate intent before emotion category

A core HRI finding across expressive robots is that coordinated eyes, head/body movement, gestures and facial animation are preferred over static channels. For Pet 2, the practical implication is that expression should first answer:

1. What is it attending to?
2. What is it about to do?
3. Does it want continuation, pause or distance?
4. Was the outcome expected?

Only then should the user infer a named emotion.

Reference:
- Non-verbal behavior of the robot companion: a contribution to the likeability. Procedia Computer Science 169 (2020):800–806.

---

# 9. R14 temporal expression model

Each expressive channel requires a different time constant and optional onset/offset asymmetry.

Recommended defaults (must remain tunable):

```text
Gaze acquire                45–90 ms
Gaze release                120–240 ms
Startle eye-open            25–55 ms
Normal eye aperture         90–160 ms
Brow raise                  110–180 ms
Brow tension                130–230 ms
Mouth audio aperture        20–45 ms
Mouth affect curve          160–300 ms
Pupil                       220–450 ms
Body lean                   180–420 ms
Local contact compliance    80–180 ms
Glow/core warmth            300–700 ms
Post-social afterglow       0.45–1.8 s
Stress recovery             0.8–4.0 s depending on cause
```

Use critically damped or exponential smoothing for ordinary channels; authored gesture pulses use bounded envelopes.

---

# 10. Gaze controller specification

Gaze is not a single target setter. Implement a small controller with:

```rust
GazePlan {
  primary_target,
  secondary_target,
  mode,
  acquire_tau,
  dwell_min,
  dwell_max,
  checkback_probability,
  saccade_scale,
  lead_seconds,
  refractory,
}
```

Modes:

```text
Track
Inspect
SocialReference
MutualGaze
ContactMonitor
PredictiveIntercept
AvoidantCheck
Drowsy
Sleep
```

Rules:

- `PredictiveIntercept`: target future cursor/orb position 80–180 ms ahead, clamped by confidence.
- `Inspect`: 0.3–1.2 s dwell, optional one re-fixation.
- `SocialReference`: object -> user proxy -> object.
- `ContactMonitor`: contact point dominates, then brief glance to cursor/user proxy after pleasant confirmation.
- `MutualGaze`: short and soft, not endless staring.
- `Sleep`: gaze generator disabled.

Micro-saccades exist only during stable fixation and are subpixel/very small at presentation scale.

---

# 11. Blink controller specification

Blink has separate causes:

```text
physiological_wetness
social_slow_blink
after_startle_reset
fatigue_long_closure
sleep_transition
protective_reflex
```

They must never all fire independently.

Arbiter priority:

```text
sleep/protective > authored social > fatigue > physiological
```

Add refractory windows and phase-aware suppression. A social slow blink suppresses physiological blink initiation for ~1.5–2.5 s.

Do not derive blinking directly from random noise every frame.

---

# 12. Prediction error / confusion as a visible intelligence cue

Agents feel intelligent when their uncertainty is readable.

If a user gesture partially matches a learned ritual:

```text
orient
-> slight brow raise/asymmetry
-> pause 120–350 ms
-> inspect/check-back
-> tentative first phase of likely response
-> evaluate result
```

Do not trigger a generic sad or error face.

This creates a crucial difference between “random” and “not sure”.

---

# 13. Expression atlas in semantic space

R14 should not ship eight fixed emotions. It should ship dozens of **intent/appraisal prototypes** that blend continuously.

Minimum prototypes:

```text
calm_content
quiet_companionship
notice_user
notice_object
curious_inspect
uncertain_inspect
recognition
reunion_soft
greeting_excited
ask_attention
ask_contact
accept_contact
lean_into_stroke
contact_satiated
nuzzle
play_invite
play_ready
chase_focus
intercept_anticipation
catch_success
miss_surprise
retry_determined
object_offer
waiting_response
proud_mastery
ritual_recognition
confused_near_match
startle
cautious_recovery
safe_relief
pain_guard
contact_reject
frustration_low
frustration_retry
bored_low
self_play
fatigue
sleepy
sleep_transition
sleep
wake_orientation
nest_content
food_inspect
food_accept
food_reject
trapped_help
freed_relief
window_ride
novel_visual
habituated_glance
```

These prototypes define channel targets, not animations.

---

# 14. Expression readability constraints

At normal desktop viewing size:

- eyes and brows must remain readable on both bright and dark backgrounds;
- internal caustics cannot cross the eye/brow region strongly enough to resemble expression changes;
- glow must not blow out eyelid silhouette;
- mouth must remain subordinate to gaze except during real vocalization/effort;
- pose changes must remain visible at 100% UI scaling and common desktop distances;
- expression must survive grayscale and reduced contrast tests.

Add screenshot acceptance passes on white, black, textured and moving-window backgrounds.

---

# 15. Anti-addiction / attachment ethics constraint

The goal is strong voluntary attachment, not compulsive coercion.

Never implement:

- punishment for absence;
- fake sickness to force return;
- streak loss;
- guilt face because the user worked instead of interacting;
- escalating audio bids;
- artificial scarcity of affection;
- monetized withdrawal of learned relationship behaviors.

Use positive continuity: recognition, learned rituals, object history, subtle reunion, and individual preferences.

---

# 16. Required validation studies inside the project

For each candidate build, run blinded clips or in-app tests measuring whether observers can identify:

1. target of attention;
2. likely next action;
3. whether contact is wanted;
4. whether play should continue;
5. whether the pet understood the preceding event;
6. whether a response was caused by user vs environment;
7. whether the pet is calm, playful, uncertain, uncomfortable or tired.

Do not ask only “is it cute?”.

Acceptance target for top-level intents: >=80% correct interpretation across first-time observers before adding more behaviors.

---

# 17. External references / repos to keep in R14 notes

- openFACS — https://github.com/phuselab/openFACS
- Rawal & Stock-Homburg 2022 — Facial Emotion Expressions in Human–Robot Interaction: A Survey
- Marsella & Gratch 2009 — EMA appraisal dynamics
- Barber et al. 2022 — behaviors important to human-dog bonding / robotic translation
- Feuerbacher & Wynne 2015 — preference for petting over vocal praise
- Byosiere et al. 2016 — play bows as play-reinitiation signals
- Walsh et al. 2024 — ambiguity in human-dog nonverbal interaction
- Boada et al. 2026 — social orienting / reward / maintaining model
- Guta & MacLean 2026 — RoboBunting / bilateral physical expressiveness
- Thomas & Johnston 1981 — The Illusion of Life

The implementation may borrow architectural ideas and terminology. Do not copy third-party code unless license compatibility is verified.