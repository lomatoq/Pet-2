# PET 2 R14 — File-by-file integration map

This is the implementation map for converting the current R13 stack into the R14 living companion. It complements `PET2_R14_LIVING_COMPANION_FULL_SPEC.md` and `PET2_R14_EXPRESSION_RESEARCH.md`.

## A. lifecore — semantic owner

### Add `crates/lifecore/src/companion.rs`
Owns only semantic state:

- `PrimaryIntent`
- `IntentTarget`
- `SocialMode`
- `CompanionIntentFrame`
- `AppraisedEvent`
- `SocialMotivationState`
- `PreferenceKey/Value`
- `RitualPrototype`
- `CompanionSocialMemory`
- deterministic bounded update rules

Do not put rendering, PBF values or OS APIs here.

### Modify `crates/lifecore/src/lib.rs`
Export companion contracts.

### Modify `crates/lifecore/src/interaction.rs`
Feed safe, grounded social outcomes to the new preference/ritual learner. Preserve existing gesture classifier as evidence, not final meaning.

### Modify `crates/lifecore/src/actions.rs`
Keep 24 macro actions for compatibility. Add mapping `ActionId -> candidate PrimaryIntent`, but do not expand ActionId to hundreds of desktop events.

### Modify persistence
Persist only compact preference/ritual/social-memory state. Version schema. Never persist raw UI text, screenshots or event payloads.

---

## B. pet_perception — event semantics

### Add `crates/pet_perception/src/companion_events.rs`
Convert available sensor evidence to generic semantic events:

- pointer approach / withdraw / hover
- touch / stroke / hold / pull / release / tickle / flick
- repeated rhythm / circle / learned-ritual candidate
- window appear / disappear / move / resize / near pass / collision / trap / release
- user active / idle / return
- generic visual novelty

Events include source, confidence, directness, novelty, expectedness and duration.

### Add `crates/pet_perception/src/habituation.rs`
Small deterministic habituation table keyed by semantic event class + coarse target, with decay and novelty recovery.

### Existing `embodied_gesture.rs`
Keep as the physical touch classifier. Add no social reward here. Its job is measurement/classification only.

---

## C. app — integration and optional providers

### Add `app/src/companion_runtime.rs`
Single orchestration seam:

```text
perception events
 -> appraisal
 -> ritual recognizer
 -> social-memory modifiers
 -> PrimaryIntent
 -> motor mapping
 -> expression target
 -> outcome recording
```

No second brain. It coordinates existing LifeCore/VITA/Morph outputs.

### Add `app/src/desktop_semantics.rs`
Privacy-safe aggregation of host activity and window events.

### Optional Windows provider
`app/src/platform_semantics/windows_ui_automation.rs`

Opt-in only. Emit categories, not content:

- focus changed
- button invoked
- selection changed
- menu/dialog opened/closed
- progress began/completed
- scroll began/ended

Never retain labels, text, password values, clipboard, document contents or file contents.

### Optional adapters
`app/src/adapters/*`

Adapters emit generic categories only. Initial useful adapters:

- creative task started/completed
- build started/completed
- render started/completed
- meeting started/ended
- media started/stopped

They may be absent in R14 MVP without breaking companion behavior.

---

## D. pet_motor — readable intention

### Existing `companion_selection.rs`
Extend into a semantic mapping layer from `PrimaryIntent` to one of the existing 64 motor programs.

Critical rule: motor selection does not infer food from the orb, user love from cursor proximity, or play from window motion.

### Existing program families
Do not create one bespoke program per emotion. Reuse programs with intent-specific style/profile targets.

Every social program requires:

- orient
- preparation/anticipation
- commit
- physical action
- result check
- resolution/afterglow/withdraw

### Add `crates/pet_motor/src/intent_mapping.rs`
Explicit, testable table:

```text
InviteContact -> SocialPettingSolicitation / SocialPresentTouchSide
AcceptContact -> TouchSoftTouchYield / TouchLeanIntoStroke
Nuzzle -> SocialRubNuzzleCursor
InvitePlay -> PlayPlayBowAnalog / PlayChaseInviteFeint
Chase -> PlayCursorChaseBout
Intercept -> PlayCursorChaseBout + predictive gaze
Catch -> PlayOrbCatchEnvelop
OfferObject -> PlayOrbCarryOffer
QuietCompanionship -> SocialQuietCompanionship
StartleFreeze -> DefenseStartleOrientFreeze
GuardPain -> DefenseLocalPainGuard
RejectContact -> DefenseStrainBraceAndRelease + withdraw
```

No raw expression values in selector.

---

## E. pet_body — top-quality expression owner

### Replace the current loose expression blending with two layers

1. `companion_expression_director.rs` — semantic target composition
2. `expression.rs` — temporal actuator/filter only

### Add `crates/pet_body/src/companion_expression_director.rs`
Inputs:

- `CompanionIntentFrame`
- motor phase
- contact location
- measured body state
- actual audio feedback
- loaded `r14_expression_profile.json`

Outputs:

```rust
CompanionExpressionTarget {
  gaze_plan,
  face,
  body_style,
  material_style,
  blink_request,
  expression_owner,
}
```

It must implement strict priority:

```text
pain/integrity
> motor episode intent
> contact semantics
> appraisal modifiers
> physiology
> identity microvariation
```

### Add `crates/pet_body/src/gaze_controller.rs`
Modes:

- Track
- Inspect
- SocialReference
- MutualGaze
- ContactMonitor
- PredictiveIntercept
- AvoidantCheck
- Drowsy
- Sleep

Must predict cursor/orb trajectory for intercept; use check-back behavior for uncertainty; suppress random scanning under high-confidence attention.

### Add `crates/pet_body/src/blink_controller.rs`
Owners:

- protective
- sleep
- authored social
- fatigue
- physiological

Only one source wins. Includes refractory windows.

### Add `crates/pet_body/src/expression_profile.rs`
Loads/validates `config/r14_expression_profile.json`, clamps values and provides deterministic fallback defaults if the config is absent/invalid.

### Existing `expression.rs`
Becomes a pure actuator with per-channel time constants and finite-value safety. No semantic decisions.

### Existing liquid/PBF path
Map body-style targets to bounded PBF modulation:

- compactness
- lean
- local compliance
- viscosity
- surface tension
- buoyancy
- recovery
- pulse

Do not use random topology changes to show emotion.

### Rendering readability
Keep face visually above internal caustics/glow. Add minimum contrast protection for pupils/brows/eyelids. Validate on bright/dark/busy backgrounds.

---

## F. pet_audio — sparse animal acoustics

### Existing companion audio admission gate
All vocal producers pass through one global budget.

### Add `crates/pet_audio/src/nonphonated.rs`
Procedural bodily sounds from airflow/noise/body resonance:

- `sniff_single`
- `sniff_pair`
- `soft_huff`
- `content_exhale`
- `startle_inhale`
- `effort_exhale`
- `sleep_breath`
- `shake_off_breath`

These are not motifs and do not need pitch melody.

### Add semantic sound policy
Default response = silence.

Examples:

- inspect unknown object -> 70–85% silence, otherwise sniff
- pleasant stroke -> mostly silence; occasional exhale/purr
- startle -> inhale only when intensity threshold crossed
- play invitation -> rare soft yip/chuff
- repeated normal UI events -> silence

Audio must follow the action, never announce an emotion the body does not show.

---

## G. social memory / attachment

### Persistent variables

```text
trust
familiarity
attachment
social_safety
social_orienting
social_reward_expectancy
social_maintaining
contact_satiation
reunion_interest
```

Attachment changes slowly. Momentary affect changes quickly.

### Preference records
Bounded table keyed by interaction type + body region + speed/rhythm + context.

### Ritual records
Bounded prototype sequences. Store semantic/coarse temporal signatures only.

Must support:

- learning that two cursor circles mean chase invitation;
- recognizing a repeated sleep/den ritual;
- preferring a specific touch style;
- expectation and visible recognition on near-match.

---

## H. desktop interaction policy

R14 default is observational and physically local.

Allowed without additional permission:

- react to pointer/body contact;
- react to window geometry already exposed by desktop_host;
- use reduced visual grid already in the project;
- move itself and its own objects.

Not allowed by default:

- move the real user cursor;
- click third-party UI;
- type;
- drag files;
- inspect arbitrary text;
- record private screen content.

Any future active UI control needs a separate explicit capability and user-visible indication.

---

# I. expression invariants to encode as tests

1. `pain_guard` can never show positive mouth curve from ambient valence.
2. `accept_contact` cannot retain high angry brow tension from raw neural readout.
3. `sleep` suppresses gaze and physiological blinking.
4. social slow blink suppresses automatic blink for a refractory period.
5. pleasant touch can occur with no sound.
6. cursor proximity without contact cannot trigger touch reward.
7. window movement cannot reduce attachment.
8. ignored play invitation resolves to neutral self-directed activity.
9. high-confidence target suppresses idle random gaze.
10. gaze precedes locomotion for approach/intercept.
11. startle eye opening precedes escape/defense locomotion.
12. contact rejection reduces local compliance and moves mass away.
13. learned ritual near-match produces uncertainty, not a full confident behavior.
14. repeated harmless UI event habituates to Tier 0/1.
15. new or dangerous event can dishabituate.
16. actual audio envelope owns mouth aperture while phonating.
17. no expression channel accepts NaN/Inf.
18. face remains legible when body glow is maximal inside allowed bounds.
19. same semantic intent with different temperament remains recognizable.
20. same affect with different intent remains visually distinguishable.

---

# J. Pet Lab changes

Add a `Companion` live tab showing, in one causal row:

```text
RAW EVENT
SEMANTIC EVENT
APPRAISAL
RITUAL MATCH
PRIMARY INTENT
MOTOR PROGRAM / PHASE
GAZE OWNER
FACE OWNER
BODY OWNER
AUDIO DECISION
OUTCOME
MEMORY UPDATE
```

Also display why a candidate was rejected, e.g.:

```text
sound: suppressed (global cooldown)
play: rejected (user not attending)
contact: rejected (strain boundary)
ritual: tentative match 0.61 < 0.78 commit threshold
blink: physiological suppressed by social-blink refractory
```

This is mandatory for tuning: if a weird face occurs, we need to identify its owner in seconds.

---

# K. Acceptance sequence

### Stage 1 — expression isolation
Run every prototype in Body Lab against neutral physics. Human reviewer must identify the intended concern/intent.

### Stage 2 — transition tests
Test pairs:

- calm -> notice -> curiosity
- curiosity -> recognition
- contact request -> accepted stroke -> satiation
- play invite -> chase -> miss -> retry -> success
- calm -> startle -> cautious recovery -> relief
- touch -> overstrain -> rejection -> recovery
- awake -> fatigue -> sleep transition -> sleep -> wake

No single-frame facial pops or contradictory channels.

### Stage 3 — live desktop causality
Use scripted pointer/window traces and verify event-to-intent attribution.

### Stage 4 — repeated exposure
30–60 minute scenario to verify habituation, sparse sound and non-annoying bids.

### Stage 5 — persistence
Restart and verify learned preferences and ritual recognition without retaining prohibited raw content.

### Stage 6 — user study
First-time users should correctly infer top-level intent in >=80% of selected clips/situations before the repertoire is expanded.

---

# L. Do not ship until these subjective gates pass

- face no longer appears randomly angry/sad during affection;
- blink frequency no longer reads as nervous tic;
- gaze target is obvious;
- touch feels bidirectional rather than a cursor spring;
- quiet idle remains interesting for at least several minutes;
- play invitation is recognizable without UI text;
- failure looks like a failed attempt, not a bug;
- uncertainty looks like uncertainty, not random behavior;
- sound is surprising/meaningful rather than constant;
- one remembered ritual is visibly recognizable after restart;
- pet can spend a work session nearby without becoming irritating.