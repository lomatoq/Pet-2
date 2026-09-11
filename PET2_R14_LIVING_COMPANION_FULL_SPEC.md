# PET 2 R14 — Living Companion Full Integration Spec

## Product target

Pet 2 is not a chatbot with a face and not a Tamagotchi obligation loop. It is an affectionate, playful, curious desktop animal that develops recognizable habits with one person, understands a large vocabulary of grounded desktop events, and expresses one coherent intention through gaze, face, liquid body, locomotion, touch response, voice, and object interaction.

The desired emotional result is strong attachment through recognition, reciprocity, shared history, and physical play. Do not use guilt, loss threats, punishment for absence, fake emergencies, escalating notifications, or artificial scarcity as retention mechanics.

The creature must feel as if it has one nervous system, not seven animation systems competing for the face.

---

# 1. Core invariant: one organism, one current story

Every rendered frame must be explainable by a single causal chain:

`perception -> appraisal -> current concern -> intent -> motor episode -> body feedback -> outcome -> memory`

The visual/audio output is not allowed to invent an emotion that is absent from that chain.

Examples:

- Cursor slowly strokes the body -> safe social touch -> accept contact -> lean into contact -> softer eyes + contact-oriented gaze + lower body stiffness -> quiet exhale or no sound -> user releases -> short afterglow -> memory reinforcement.
- Window suddenly moves into the pet -> external spatial disturbance -> orient/startle -> avoid or ride surface -> recover -> no reduction in attachment to the user unless the interaction was clearly direct and repeated.
- User ignores an invitation -> invitation expires -> neutral withdrawal -> self-play / rest / exploration. Never sadness punishment, guilt face, whining loop, or attachment loss from normal absence.

At any moment exactly one `PrimaryIntent` owns the readable performance. Secondary physiology may modulate it but may not contradict it.

---

# 2. New high-level state contract

Introduce one canonical social/behavior frame produced after LifeCore/VITA/Morph appraisal and before motor rendering.

```rust
pub struct CompanionIntentFrame {
    pub episode_id: u64,
    pub primary: PrimaryIntent,
    pub target: IntentTarget,
    pub confidence: f32,
    pub urgency: f32,
    pub valence: f32,
    pub arousal: f32,
    pub social_safety: f32,
    pub trust: f32,
    pub attachment: f32,
    pub play_readiness: f32,
    pub curiosity: f32,
    pub fatigue: f32,
    pub discomfort: f32,
    pub surprise: f32,
    pub frustration: f32,
    pub anticipation: f32,
    pub contact_pleasantness: f32,
    pub agency_match: f32,
    pub expected_outcome: ExpectedOutcome,
    pub social_mode: SocialMode,
    pub persistence: f32,
}
```

`PrimaryIntent` minimum set:

```text
IdleContent
Rest
Sleep
Wake
Orient
Inspect
Approach
Follow
InviteContact
AcceptContact
Nuzzle
GroomSelf
InvitePlay
Chase
Intercept
Catch
Carry
OfferObject
SearchObject
Explore
Hide
Peek
Mimic
Celebrate
RecoverFromMiss
Avoid
StartleFreeze
GuardPain
EscapePressure
RejectContact
SettleAfterStress
ReturnHome
Nest
EatInspect
EatAccept
EatReject
SocialCheckIn
QuietCompanionship
```

This enum is semantic, not an animation list. Motor programs remain reusable performances.

---

# 3. Expression ownership hierarchy

Use strict ownership. Highest level wins.

1. **Integrity / pain / physical danger**
2. **Current intentional motor episode**
3. **Contact semantics**
4. **Affect / temperament modulation**
5. **Autonomic physiology**
6. **Micro-variation / personality noise**

No lower level may override a higher level with opposite meaning.

Examples:

- During pain guard, positive valence cannot force a smile.
- During affectionate nuzzle, recurrent-network brow noise cannot produce angry eyebrows.
- During sleep, spontaneous gaze and blink generators are disabled.
- During a deliberate slow blink, procedural blink is suppressed for ~1.5–2.0 s.

---

# 4. Expression channels

All visible body channels must be driven from the same `CompanionIntentFrame` plus measured body feedback.

## 4.1 Eyes

Channels:

- gaze target
- gaze lead
- pupil size
- pupil focus
- eye aperture
- squint
- blink left/right
- micro-saccade amplitude
- convergence / near-target focus if rendering permits

Rules:

- **Gaze always has an object**, even if it is an abstract screen region.
- Avoid random scanning while a high-confidence target exists.
- Cursor proximity is not touch.
- Real touch target > object target > salient window > idle wander.
- During play, gaze anticipates trajectory slightly ahead of the target.
- During affection, gaze alternates between contact point and cursor/person proxy rather than staring dead-center.
- During uncertain inspection, use short look -> pause -> relook, not constant jitter.
- During fear/startle, eyes open rapidly and fix on source before locomotion.
- During relaxed affiliation, allow partial eyelid closure and rare slow blink.
- During fatigue, aperture narrows slowly; do not use continuous frequent blinking as a fatigue substitute.

Suggested blink rates are state-conditioned, not fixed:

```text
calm idle:      1 blink / 3.5–7.0 s
focused inspect:1 / 5–10 s
play:           1 / 4–8 s, avoid during high-speed intercept
stress:         burst possible after event, then refractory
sleepy:         longer closures, not higher random frequency
social slow blink: authored event, 0.35–0.75 s closure, rare
```

## 4.2 Brows

Brows communicate appraisal, not generic animation noise.

Primary mappings:

- curiosity: mild raise + small asymmetry toward target
- affection: low tension, slight inner raise or neutral-soft
- play: mild raise, low tension, asymmetric anticipation
- uncertainty: raise + mild medial tension
- effort: tension proportional to measured motor error / strain
- startle: fast raise then settle
- pain: tension + asymmetry toward affected side
- frustration: moderate tension, compressed mouth, no random smile
- sleep: near neutral / relaxed

Never modulate brow tension directly from raw recurrent neuron activity every cognition tick. Raw neural output can bias a target; the visible brow follows a low-pass semantic owner.

## 4.3 Mouth

Channels:

- curve
- open
- compression
- tension
- asymmetry

Rules:

- Mouth opening follows actual audio envelope / breathing / physical exertion.
- Positive valence influences resting curve only weakly.
- Play uses open anticipatory mouth only during relevant moments, not as a constant grin.
- Miss/failure: brief compression/asymmetry, then retry orientation.
- Affection: very subtle positive curve or relaxed neutral.
- Sniffing: repeated tiny aperture/nasal gesture synchronized to sound, no broad mouth opening.
- Startle: brief aperture spike.
- Pain: compressed or strained, not cartoon frown unless strong.

## 4.4 Pupils

- arousal/novelty drives dilation.
- near focused inspection can narrow slightly.
- no valence = pupil-size shortcut.
- smooth with tau ~0.20–0.40 s.

## 4.5 Whole liquid body

The liquid body is a major emotional communication surface.

Control dimensions:

- compactness
- elongation toward target
- center-of-mass lean
- vertical buoyancy
- viscosity
- surface tension
- local flow speed
- internal pulse
- settling time
- contact yield
- rebound
- asymmetric deformation toward stimulus

Mappings:

- affection: slightly softer, more contact yield, small lean into contact, slow recovery
- curiosity: local protrusion toward target, body remains coherent
- play: higher elastic energy, faster recovery, anticipatory compression before movement
- contentment: broad stable silhouette, slow breathing pulse
- fatigue: lower buoyancy, slower recovery, higher settle
- threat: compact and stiff
- pain: protect local area, shift mass away, reduce cooperation
- surprise: brief global compression then orient
- rejection: local repel from contact, then move away; do not overdo anger

Do not continuously deform the silhouette merely to prove that the brain is active.

## 4.6 Glow/material

Glow is an autonomic/supporting cue, not a mood lamp.

- base identity pigment remains stable
- internal glow amplitude can follow arousal + affiliative warmth within a narrow range
- threat may reduce internal softness / increase sharp pulse, not turn the body into a different color identity
- pain may create a very subtle local response if technically feasible
- no rainbow affect coding
- no constant animated caustics at high intensity; visual noise must not compete with face readability

---

# 5. Temporal grammar for every behavior

All meaningful behaviors use phases, never single-frame reactions.

Canonical grammar:

```text
NOTICE
ORIENT
APPRAISE / HESITATE
COMMIT
ACT
CHECK RESULT
RESOLVE / AFTERGLOW / WITHDRAW
```

Not every action needs all phases, but every social action must contain at least:

`orient -> commit -> result check -> resolution`

This provides readable causality.

Phase transitions use measured evidence, not only timers.

Examples:

### Petting

```text
notice contact
-> orient eyes/contact field
-> yield locally
-> if pleasant for >250 ms: lean in
-> if movement continues: follow stroke
-> if user pauses: remain in contact
-> release edge: afterglow 0.4–1.2 s
-> optional slow blink/exhale
-> return to previous activity
```

### Tickle

```text
contact classifier high confidence
-> local recoil 80–150 ms
-> playful re-approach if safe
-> wriggle / pursuit
-> check whether user continues
-> if repeated and pleasant: play episode
-> if excessive: calm boundary / retreat
```

### Cursor chase

```text
play invitation detected
-> play bow / readiness compression
-> gaze lead
-> short feint
-> chase in bounded bout
-> fake miss only if actual interception error supports it
-> success/failure reaction
-> pause for user response
```

### Window movement

```text
window motion detected
-> orient
-> if collision threat: evade / ride surface
-> if harmless repeated familiar movement: brief glance only
-> never classify window motion as petting/play invitation by itself
```

---

# 6. Appraisal: turning raw events into meaning

Create `CompanionAppraisal` between perception and action selection.

Every perceived event gets:

```rust
pub struct AppraisedEvent {
    pub source: EventSource,
    pub kind: EventKind,
    pub target: EventTarget,
    pub confidence: f32,
    pub novelty: f32,
    pub controllability: f32,
    pub directness: f32,
    pub social_likelihood: f32,
    pub threat_likelihood: f32,
    pub play_likelihood: f32,
    pub contact_quality: f32,
    pub prediction_error: f32,
    pub timestamp: f64,
}
```

Critical distinction:

- direct user contact
- indirect desktop motion
- pet self-motion
- environment motion
- application/UI event

Never reduce attachment from ambiguous environmental events.

---

# 7. Event vocabulary: hundreds of computer actions without hundreds of hardcoded animations

Do not implement 300 independent if-statements. Implement a compositional event ontology.

## 7.1 Pointer family

Raw features:

- cursor position
- velocity
- acceleration
- jerk
- radial speed relative to pet
- path curvature
- dwell
- click/down/up edges
- drag state
- repeated click rhythm
- contact geometry
- pressure proxy
- deformation
- strain
- release impulse

Derived events:

```text
approach_pet
withdraw_from_pet
hover_near_pet
soft_touch
stroke
hold
slow_pull
pull_release
tickle
circle/stir
flick
rapid_pass
rhythmic_tap
play_invitation
object_drag_near_pet
object_throw
pet_drag
contact_release
attempt_fragment
help_fragment
```

## 7.2 User activity family

Privacy-safe system signals:

```text
user_active
user_idle_short
user_idle_long
user_returned
rapid_pointer_work
slow_pointer_work
continuous_typing_activity (activity only, never keys/text)
repetitive_click_activity
work_burst_started
work_burst_ended
```

These are context signals, not commands.

## 7.3 Window family

```text
window_appeared
window_disappeared
window_moved
window_resized
window_maximized
window_minimized
window_became_foreground
window_lost_foreground
surface_passed_near_pet
surface_collided_with_pet
pet_occluded
free_space_changed
pet_became_trapped
pet_freed
```

## 7.4 Optional UI Automation family

Explicit opt-in provider. Store semantic categories only, never text contents.

Examples:

```text
button_invoked
selection_changed
menu_opened
menu_closed
dialog_opened
dialog_closed
progress_started
progress_completed
focus_changed
scroll_started
scroll_ended
app_busy
app_idle
```

Do not read password fields, clipboard, document text, chat content, filenames, or arbitrary labels into persistent memory.

## 7.5 Application adapters

Adapters are optional and separately permissioned.

Possible semantic adapters:

```text
browser_video_started / paused
creative_export_started / completed
build_started / completed
render_started / completed
meeting_started / ended
game_session_started / ended
music_started / stopped
```

Each adapter emits generic semantic events; the pet does not receive the private payload unless an explicit feature later requires it.

---

# 8. Reaction policy to desktop events

Most events should NOT trigger a full performance.

Use response tiers:

```text
Tier 0 — ignore
Tier 1 — eye glance only
Tier 2 — eyes + small head/body orientation
Tier 3 — short motor response
Tier 4 — full episode
```

Examples:

- ordinary window resize far away -> Tier 0
- nearby new window -> Tier 1
- moving edge approaching pet -> Tier 2/3
- collision/trapping -> Tier 4
- export complete while pet is calm and user active -> optional Tier 1 check-in, not celebration spam
- user returns after long idle -> Tier 2 greeting; only occasionally Tier 3

Habituation lowers response tier to repeated predictable events.

Novelty increases it temporarily.

---

# 9. Social attachment model

Attachment is long-term familiarity, not moment-to-moment happiness.

Separate:

```text
trust
familiarity
attachment
social_safety
play_history
contact_preference
ritual_familiarity
recent_social_satisfaction
```

Attachment changes slowly.

Positive evidence:

- repeated safe contact
- successful shared play
- predictable user response to bids
- help/recovery
- familiar ritual recognition

Negative evidence only from clear repeated direct causes:

- harmful strain after clear boundaries
- repeated aggressive direct gestures

Do NOT reduce attachment because:

- user is absent
- user ignores one invitation
- application moves a window
- computer sleeps
- audio is disabled
- user is working

---

# 10. Individual preference learning

The animal should learn preferences, not just global reward.

Persist compact preference records:

```rust
PreferenceKey {
    interaction_kind,
    region,
    speed_bin,
    rhythm_bin,
    context,
}

PreferenceValue {
    expected_pleasantness,
    confidence,
    exposure_count,
    recency,
}
```

Examples:

- likes slow side strokes
- tolerates brief head taps
- loves chase after a circular cursor invitation
- dislikes sustained stretching
- often brings the orb after user returns from idle

Preference updates must be bounded and require repeated evidence.

---

# 11. Habit / ritual learning

This is where long-term attachment becomes visible.

Implement compact sequential prototypes rather than text memory.

A ritual is:

```text
context signature
+ user action sequence
+ pet action sequence
+ outcome
+ time-of-day optional weak cue
```

Examples:

- user circles cursor twice -> pet interprets as chase invite
- after long work session user taps near den -> pet settles beside den
- repeated gentle stroke before sleep -> pet anticipates relaxation

Recognition must have confidence and near-match tolerance.

A learned ritual may bias intent selection; it must not bypass safety or physical evidence.

---

# 12. Sound system: animal, sparse, causal

Silence is default.

Three sound classes:

## A. Non-phonated bodily sounds

Generated from breath/noise/resonance, not vocal motifs:

```text
sniff_in
sniff_pair
soft_huff
content_exhale
startle_inhale
effort_exhale
sleep_breath
shake_off_breath
```

## B. Semi-voiced animal sounds

```text
warm_chuff
small_whine
play_yip
low_grumble
purr/trill
relief_murmur
```

## C. Social call

Rare. Used for explicit invitation, help, reunion, or strong outcome.

Admission budget applies globally across all producers.

Initial production tuning:

```text
normal vocal minimum gap: 8–14 s
ambient call: <= 1 / 45–90 s
play burst: max 2 short events then 8+ s quiet
safety/startle may override normal gap once
no queued backlog
```

Sniffs and breath sounds can have a separate low-energy budget because they are much less intrusive, but still cannot become a continuous loop.

Sound must align with body:

- sniff -> nose/contact orientation + small body stillness
- purr -> relaxed contact state
- effort exhale -> measured acceleration/strain
- startle inhale -> actual sudden event

Never emit a vocalization solely because an internal timer expired.

---

# 13. Mouse interaction rules

The real cursor remains owned by the user.

The pet may:

- approach it
- nuzzle around it
- chase after explicit invitation
- avoid it
- present a body side for touch
- deform around contact
- carry its own orb toward it

The pet must never in normal mode:

- move the real system cursor
- click application controls
- drag files
- steal focus
- block important clicks with a giant hitbox

Hit-testing must remain silhouette/object-local and click-through elsewhere.

---

# 14. Interface interaction rules

The pet can interact with **geometry**, not arbitrary private content.

Allowed default interactions:

- sit on window edge
- ride moving surface
- peek around edge
- avoid a window covering it
- use free desktop region
- hide behind a window if the geometry supports it
- watch a progress completion semantic event if adapter enabled

Any action that affects another app must require a separate explicit automation/assistant feature; do not mix it into the animal loop.

---

# 15. Detailed state-expression bible

This section defines defaults. Temperament modifies amplitudes/timing by <= 20–25%, not semantic polarity.

## Calm content

- eyes: aperture 0.82–0.95, soft focus
- brows: neutral/low tension
- mouth: neutral to +0.10 curve
- body: broad, settled, slow pulse
- motion: low-frequency small shifts
- sound: usually none, rare soft exhale

## Curious

- eyes: target locked, aperture 0.95–1.0, pupils slightly larger
- brows: +raise, mild asymmetry
- mouth: neutral/slightly open only at peak inspection
- body: elongate 3–8% toward target
- locomotion: arc approach, pause/check
- sound: sniff pair if close to novel target

## Affectionate

- eyes: aperture 0.70–0.90, occasional slow blink
- brows: low tension
- mouth: +0.05–0.18 curve max
- body: soft, yields/leans toward contact
- glow: slight warm amplitude increase only
- sound: usually silence; occasional exhale/purr/chuff

## Playful

- eyes: wide focused gaze, target prediction
- brows: raised/asymmetric, low tension
- mouth: +curve/open only during invite/intercept
- body: elastic, compressed preparation then fast release
- movement: feints, arcs, pauses, overshoot
- sound: one short yip/chuff at invite or success, not every phase

## Excited reunion

- trigger requires meaningful absence + user return + adequate social drive
- eyes target cursor/user proxy immediately
- body performs one approach burst, then brakes
- optional one short call
- no repeated calls if user does not interact

## Sleepy

- aperture lower, slower gaze
- body lower and heavier
- movement reduced
- yawn allowed rarely if actually transitioning to rest
- no fake sadness

## Sleeping

- motor owner = sleep
- eyes closed
- spontaneous gaze disabled
- very low breathing motion
- no social calls
- only safety event wakes it

## Startled

- rapid eye opening/orient
- brief body compression/freeze
- maybe one short inhale/pulse
- recover quickly if no continuing threat
- do not convert every cursor acceleration into startle

## Threatened

- compact, stiff
- gaze to threat
- mouth/brow tension
- no cute smile/purr
- retreat or brace

## Pain/discomfort

- local body guard
- gaze may check affected region
- brow tension / mouth compression proportional to measured evidence
- no repeated whining unless ongoing severe event
- attachment changes only when direct user cause is high-confidence and repeated

## Frustrated

- caused by repeated failed intentional action, not random timing
- small tension, pause/reappraise
- retry if persistence high
- withdraw if effort budget exhausted

## Sad / low valence

Rare, low-amplitude state.

- lower buoyancy
- reduced play initiation
- no theatrical crying
- user absence alone cannot create it

## Bored

- self-directed behavior first: inspect, groom, play with orb, explore
- attention bid is a later bounded option
- never spam user

## Confused / prediction error

- orient -> pause -> slight brow raise/asymmetry -> retry/check
- this is critical to appearing intelligent

## Proud / success after effort

Not a permanent emotion scalar. A short outcome episode:

- stop
- check result
- look toward user
- small expansion/glow pulse
- optional chuff
- then settle

---

# 16. Facial action system implementation

Do not expose raw neuron outputs directly as final facial values.

Use semantic targets:

```rust
struct ExpressionTarget {
    eye_aperture: f32,
    pupil_size: f32,
    pupil_focus: f32,
    brow_raise: f32,
    brow_tension: f32,
    brow_asymmetry: f32,
    mouth_open: f32,
    mouth_curve: f32,
    mouth_tension: f32,
    mouth_compression: f32,
    mouth_asymmetry: f32,
    glow: f32,
}
```

Pipeline:

```text
intent template
+ measured physical modulation
+ affect modulation
+ temperament bounded variation
-> semantic target
-> per-channel critically damped temporal filter
-> authored blink/audio overlays
-> renderer
```

Use different temporal constants per channel. Avoid one global smoothing constant.

Approximate targets:

```text
gaze 50–100 ms
mouth audio 25–60 ms
startle eyes 30–60 ms
brows 120–220 ms
mouth curve 150–250 ms
pupils 200–400 ms
glow 250–500 ms
```

---

# 17. Motor program selection integration

LifeCore owns motives. Motor selection owns performance variant.

Selection should combine:

```text
current ActionId
+ CompanionIntentFrame
+ contact evidence
+ target affordance
+ current active readable phase
+ cooldown
+ recent program signatures
+ learned preference bias
```

No selector branch should infer semantic object category from spatial presence alone.

Examples:

- orb present != food present
- cursor near != touch
- moving window != user aggression
- high salience != play invitation

Add typed affordances to world objects:

```text
Toy
Food
Home
Surface
UserProxy
Cursor
UnknownVisual
```

Programs may only select affordance-compatible actions.

---

# 18. Attention system

Attention is a scarce resource.

Maintain top candidates with:

```text
salience
novelty
social relevance
threat
current-goal relevance
habituation
distance
recently attended penalty
```

The winning target controls gaze. Locomotion only follows if intent selection authorizes it.

This prevents the pet from chasing every bright moving thing on screen.

---

# 19. Inhibition and refractory periods

Living behavior needs suppression as much as actions.

Add refractory windows:

- slow blink: 4–10 s
- social bid: 20–60 s after ignored bid
- greeting: once per meaningful return episode
- startle: short refractory unless threat continues
- vocal call: global budget
- repeated same motor program: anti-repeat penalty

Also add `do_nothing` / maintain-current-state as a first-class outcome.

---

# 20. Memory and visible continuity

Persist only compact semantic state.

Minimum user-visible continuity:

- preferred touch signatures
- learned rituals
- object familiarity
- favorite orb usage
- den familiarity
- recent successful game type
- social trust/attachment
- learned gesture prototypes

On startup do not play a canned "I remember you" animation. Let memory become visible when relevant context reoccurs.

---

# 21. Learning rule

Do not reward salience itself.

Reward terms:

```text
+ social reciprocity
+ safe contact pleasantness
+ successful shared play
+ prediction improvement
+ goal completion
+ user positive explicit feedback if provided
- pain
- boundary violation
- repeated failed action
- prolonged restraint
```

Never positively reinforce an event simply because it generated a strong body reaction.

Learning should update:

- preference priors
- action utility in context
- gesture/ritual recognition
- timing conventions

It should not freely mutate face semantics into unreadable configurations.

---

# 22. User teaching mode

Teaching should be explicit enough to learn quickly but still feel animal-like.

Flow:

1. user starts teaching gesture/path
2. pet enters attentive pose
3. capture compact trajectory/rhythm features
4. pet attempts imitation/action
5. user repeats or gives positive/negative feedback
6. prototype merges near duplicates
7. confidence rises
8. learned convention becomes available in normal life

The user should see the learning curve in behavior, not through a debug percentage.

---

# 23. Personality

Temperament changes thresholds and style, not fundamental comprehension.

Traits already present can bias:

- sociability -> social bid frequency, approach distance
- curiosity -> inspect duration / novelty threshold
- boldness -> hesitation duration
- playfulness -> play opportunity utility
- patience -> waiting duration before withdrawal
- persistence -> retry count
- autonomy -> self-directed activity probability
- vocality -> probability *inside global sound budget*
- adaptability -> learning rate within safety bounds
- attachment speed -> long-term attachment rate

No personality seed may produce an unusably noisy, hostile, or permanently anxious pet by default.

---

# 24. Visual simplification target

The current material/halo may remain as identity but readability wins.

Required presentation tests:

- white background
- black background
- busy file explorer
- browser/video
- 100%, 125%, 150%, 200% DPI
- small on-screen size

The eyes, brows, mouth, silhouette lean, and target direction must remain readable.

Reduce any shader motion or halo intensity that competes with those signals.

---

# 25. Privacy and permissions

Default mode:

- no text collection
- no clipboard
- no filenames
- no microphone
- no persisted raw screenshots
- no key content

Optional semantic adapters must be explicit and individually disableable.

Telemetry stores semantic event categories and numeric features only.

---

# 26. Required code ownership changes

Keep crate boundaries:

```text
pet_perception
    raw gesture/window/activity classification

lifecore
    needs, affect, social memory, preferences, attachment, semantic intent

morph_brain
    bounded somatic contribution, never direct facial ownership

pet_motor
    phase grammar, target lock, performance variant, expression intent overlay

pet_body
    physics + final filtered physical expression/rendering

pet_audio
    actual sound synthesis + global admission budget

app
    orchestration + optional desktop/app semantic providers
```

Add modules conceptually:

```text
lifecore/src/companion_appraisal.rs
lifecore/src/social_memory.rs
lifecore/src/ritual_memory.rs
pet_perception/src/desktop_events.rs
pet_motor/src/expression_owner.rs
pet_audio/src/animal_nonphonated.rs
app/src/desktop_semantics_runtime.rs
```

Do not create a second independent brain owner.

---

# 27. Diagnostics needed before tuning

Pet Lab must expose one causal line, not dozens of raw values:

```text
INPUT:
  soft_stroke confidence=.88 target=body_left

APPRAISAL:
  safe=.94 pleasant=.76 social=.82 prediction_error=.12

INTENT:
  AcceptContact target=cursor attachment=.61

PROGRAM:
  touch.lean_into_stroke phase=maintain_contact

EXPRESSION OWNER:
  affiliation

AUDIO:
  silence (budget / no semantic need)

OUTCOME:
  contact ended safely -> preference +0.012
```

If a weird face appears, developer must be able to answer who owned brow, mouth, blink, gaze, and body deformation that frame.

---

# 28. Acceptance scenarios

Automated deterministic scenarios plus real human observation.

Minimum scenarios:

1. cursor passes nearby rapidly: glance or ignore; no petting reaction
2. cursor stops near face: inspect; no physical-contact reaction
3. soft touch: yield -> lean -> afterglow
4. long hold: pleasant initially, then neutral/brace based on real load
5. tickle: playful response only when safe
6. sharp flick: startle/guard, no smile
7. touch released: one resolution response, no repeated loop
8. ignored social bid: graceful withdrawal, no whining
9. user returns after idle: one greeting, no repeated greeting
10. orb visible: toy behavior, never food behavior
11. orb throw: track/intercept/play outcome
12. window approaches: orient/avoid
13. window traps orb: grounded help episode
14. window moves repeatedly: habituation
15. user works for 10 minutes: mostly quiet companionship
16. sleep: no spontaneous social expressions
17. direct harmful strain: guard/reject and recover
18. recovery after stress: shake/exhale then neutral
19. repeated learned gesture: confidence and reaction improve
20. restart: learned preference/ritual remains visible in matching context

---

# 29. Quantitative readability gates

These are engineering gates, not claims about human attachment.

- no more than one primary intent transition per 250 ms unless safety interrupt
- no repeated authored social blink within 4 s
- ordinary vocal requests <= 6/min global hard ceiling; target behavior much lower
- ambient social call <= ~1/min maximum, normally lower
- ignored bid cooldown >= 20 s
- no user-absence attachment penalty
- >95% of non-safety motor episodes must expose a target/cause in telemetry
- no object semantic action without compatible typed affordance
- no raw neural channel directly setting final brow/mouth/blink after motor ownership stage
- NaN/inf at any expression input may not propagate to renderer

---

# 30. Implementation order

## Phase A — coherence first

1. expression ownership hierarchy
2. blink consolidation
3. typed object affordances (fix orb/food confusion)
4. global audio admission
5. afterglow/resolution phases
6. Pet Lab causal owner display

## Phase B — affectionate core

1. soft touch / stroke / release
2. nuzzle / present side
3. quiet companionship
4. greeting after real absence
5. play invitation/chase/orb loop
6. sparse sniff/chuff/exhale synthesis

## Phase C — desktop understanding

1. pointer activity features
2. window events + habituation
3. user active/idle/return semantics
4. optional UI Automation provider
5. optional app adapters

## Phase D — visible learning

1. preference memory
2. ritual prototypes
3. better imitation/teaching
4. cross-session continuity

Do not start Phase C by adding hundreds of reactions before Phase A/B look alive.

---

# 31. Definition of done

R14 is successful only when a new observer, without debug UI, can correctly describe most episodes in ordinary language:

- "he noticed the cursor"
- "he wants me to pet him"
- "he liked that"
- "he got startled"
- "he tried to catch it and missed"
- "he is waiting for me"
- "he gave up and went to do his own thing"
- "he remembered that gesture"

If observers instead say "his face changed randomly", "he keeps blinking", "he makes noises constantly", or "I don't know what he wants", adding more intelligence is not the next step. Fix causal readability first.
