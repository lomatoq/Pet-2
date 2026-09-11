# PET 2 R14 — Expression Acceptance Matrix

R14 is not accepted because every module compiles. It is accepted when a first-time observer can read the creature's target, intent and interaction boundary from behavior without labels.

## 1. Golden expression scenes

Each scene must be recordable deterministically from a seed and event trace.

### A. Quiet companionship
- Pet is calm while user works for 3 minutes.
- No more than short gaze checks; no unsolicited full-body bids during focus periods.
- Face remains soft/neutral, not blank and not smiling continuously.
- No vocalization is required.

### B. User returns
- User absent/idle long enough for a return event.
- Pet orients before locomotion.
- Greeting intensity is modulated by familiarity/attachment, but never guilt.
- Most returns are silent or contain at most one small bodily/vocal cue.

### C. Gentle stroke
- Contact point becomes the primary gaze target briefly.
- Local body yields before whole-body lean.
- Brow tension decreases, eyelids soften.
- No mandatory sound.
- On release: 0.4–1.2 s visible afterglow, then return to previous concern.

### D. Contact satiation
- Repeated pleasant stroke does not increase solicitation forever.
- Contact-maintaining drive eventually decreases while social safety stays high.
- Pet may remain nearby without continuing to ask for touch.

### E. Ambiguous hold
- Stationary hold begins neutral.
- If pressure/strain remains safe, pet relaxes or waits.
- If strain rises, expression changes to boundary/rejection before retreat.
- Attachment is not damaged by one ambiguous event.

### F. Overstrain rejection
- Local stiffness/guard appears first.
- Brow/mouth tension supports the physical boundary.
- Body moves away or reduces cooperation.
- No positive smile, purr or playful response can leak through.

### G. Play invitation
- Pet produces anticipation/play signal before chase.
- User has a clear response window.
- Ignored invitation resolves neutrally; no whining loop.

### H. Chase
- Eyes lead cursor/orb motion by a bounded predictive interval.
- High-speed intercept suppresses physiological blinking.
- Body anticipates direction change with compression/lean.

### I. Miss and retry
- Miss is driven by real motor error/contact evidence.
- Brief surprise/uncertainty, then reorientation.
- Retry is readable as determination, not anger.

### J. Catch success
- Gaze verifies object/contact first.
- Celebration follows the verified result, not the prediction.
- Afterglow is longer than the transient success pulse.

### K. Near-match learned ritual
- Pet recognizes partial similarity but does not instantly execute a full learned response.
- Readable sequence: orient -> asymmetric curiosity/confusion -> checkback -> tentative action.

### L. Exact learned ritual
- Recognition becomes visibly faster across successful repetitions.
- Ritual bias never bypasses safety boundaries.

### M. Startle
- Eye opening/orient occurs before defensive locomotion.
- Freeze target 80–160 ms depending on severity.
- Recovery follows if no continuing threat.

### N. Window near-pass
- Pet glances/orients; no false user blame.
- Repeated harmless pass habituates to gaze-only or ignore.

### O. Window collision/trap
- Real collision/pressure escalates to escape/help episode.
- Freed event produces relief/recovery.
- Attachment does not change from environmental cause.

### P. Fatigue -> sleep
- Eye aperture and buoyancy fall gradually.
- Long closures replace frequent nervous blinking.
- Sleep disables random gaze and ordinary social bids.

### Q. Wake
- Eyes/gaze reacquire environment before high-energy movement.
- No instant cheerful display unless an actual social event follows.

### R. Inspection/sniff
- Unknown object: orient -> inspect -> optional small sniff.
- Sniff is rare, non-melodic and uses tiny mouth motion.
- No automatic food behavior from orb presence.

### S. Conflicting upstream signals
- Positive valence + pain: pain owns face.
- High attachment + rejection: rejection owns immediate behavior.
- Play drive + sleep: sleep commitment owns until valid wake condition.

### T. Audio silence
- Run 10 min of ordinary desktop activity.
- Common window/pointer/UI events should mostly produce silence.
- No audio queue backlog.

---

## 2. Readability scoring

For every recorded clip, a blind reviewer answers:

1. What is the pet looking at?
2. What does it appear about to do?
3. Does it want interaction to continue, stop or remain unchanged?
4. Was the last result expected, surprising or unclear?
5. Did the user cause the response, did the environment cause it, or is it self-directed?
6. Is the pet calm, playful, uncertain, uncomfortable, fatigued, or protective?

Targets before repertoire expansion:

- target-of-attention: >= 90% correct
- continue/stop interaction boundary: >= 90% correct
- broad primary intent: >= 80% correct
- causal source user/environment/self: >= 85% correct
- no contradictory-channel report in > 5% of clips

These thresholds are product acceptance targets, not claims from the literature.

---

## 3. Desktop readability tests

Record every primary scene on:

- pure white background
- pure black background
- dark UI
- light UI
- textured/photo background
- moving windows behind pet
- 100%, 125%, 150%, 200% display scaling where applicable

Reject if:

- pupils disappear into material;
- brows become unreadable;
- body glow reads as eyelid/brow motion;
- mouth dominates the face at idle;
- caustics create false expression flicker;
- silhouette intent is unreadable at ordinary desktop size.

---

## 4. Temporal quality gates

Measure onset ordering, not only final values.

Required ordering examples:

```text
approach: gaze -> body orientation -> travel
startle: eye/open/orient -> freeze -> defensive motion
petting: contact recognition -> local yield -> whole-body lean -> afterglow
catch: object contact -> verification -> success expression
reject: strain/pressure -> local guard -> face boundary -> withdrawal
sleep: fatigue -> drowsy posture -> longer closure -> gaze off -> sleep
```

No two unrelated major expression changes may begin on the same frame unless caused by the same high-priority event.

---

## 5. Anti-randomness gates

During a 10-minute deterministic idle/focus run:

- no unexplained anger/sadness expression;
- no repeated brow flipping unrelated to semantic state;
- no repeated rapid blinking;
- no arbitrary mouth opening without audio/breath/effort;
- no gaze teleport while a high-confidence target exists;
- no full-body action solely from low-confidence novelty;
- no unsolicited attention loop repeating without user response.

---

## 6. Individuality gates

Temperament may vary amplitude, latency, persistence and preference, but may not make intent unreadable.

Test at least:

- high sociability / low sociability
- high playfulness / low playfulness
- high boldness / cautious
- high patience / low patience
- high attachment / low attachment

A blind reviewer must still identify the same semantic intent across variants.

---

## 7. Memory gates

After restart:

- preference records remain bounded and valid;
- one learned touch preference changes response measurably but not cartoonishly;
- one learned ritual is recognized;
- no raw screen/UI content is persisted;
- absence does not reduce attachment;
- old false interaction credits are not replayed.

---

## 8. Performance gates

New expression/appraisal systems must remain cheap enough for desktop residence.

Targets:

- semantic event reduction p95 < 0.10 ms
- appraisal/intent p95 < 0.10 ms
- expression target composition p95 < 0.10 ms
- gaze/blink controllers p95 < 0.03 ms combined
- no heap allocation in per-frame gaze/blink path
- bounded tables for habituation, preferences and rituals

Measure on Windows production build; do not infer from debug.

---

## 9. Ship blocker checklist

R14 cannot be called “living companion” until all are true:

- affectionate touch has readable start, continuation and release;
- rejection/boundary is readable without appearing hateful;
- gaze explains attention;
- blink no longer reads as a nervous tic;
- mouth only opens for grounded reasons;
- quiet idle is visually alive but not noisy;
- play has invitation, turns and outcome checks;
- confusion looks intentional;
- failure looks causal;
- sound is sparse and animal-like;
- learned preference is visible;
- learned ritual survives restart;
- ordinary desktop actions habituate;
- environment events are not blamed on user;
- no coercive/guilt retention behavior.