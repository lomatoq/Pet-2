# V53: organism-linked small actions and readable expression

Date: 2026-09-27. This is the follow-up council review to V52. Implementation and verification status are recorded below as the shared changes settle.

## Design constraints

The new actions should have a cause, a target, and a stopping condition. A gesture that runs on a periodic timer regardless of hunger, contact, uncertainty, fatigue, or recent outcome is an animation loop, not additional agency. Gesture variation may perturb timing and amplitude within a cause; it must not invent the cause.

The liquid character is fictional. It has no canine nose, paws, biological olfactory receptors, or real hormone measurements. Sniffing represents closer information sampling of a virtual source. Licking and scratching are body-language conventions driven by internal state and measured events. They must not be described as diagnoses or literal animal physiology.

A visible action requires an end-to-end path: observed event or body state -> bounded persistent/transient state -> selected gesture -> final face/body actuation -> renderer. Selection counters alone do not establish visible expression. Eating owns the jaw during a bite; sleep and protective reactions preempt optional gestures; digestion strain and burps must not fight a second mouth writer. Focus mode remains quiet.

## Primary research consulted

- **Supported but context-dependent:** dogs' mouth licking occurred more frequently with negative than positive facial stimuli in one controlled study. This does not establish that every lick means stress, nor that a slime's lick should inherit that meaning. [Albuquerque et al., Mouth-licking by dogs as a response to emotional stimuli (2018)](https://pubmed.ncbi.nlm.nih.gov/29129727/).
- **Supported, not a one-to-one dictionary:** audience conditions affected dogs' behavioral displays and facial movements; the authors discuss conflicting interpretations of licking across studies. Read gestures jointly with approach/avoidance, posture, and context. [Intra and interspecific audience effect on domestic dogs' behavioural displays and facial expressions (2024)](https://www.nature.com/articles/s41598-024-58757-6).
- **Supported within the study:** investigation and looking-back behavior depended on detection task context. It motivates information-seeking linked to uncertainty, not a universal two-sniff sequence. [DeChant et al., Effect of Handler Knowledge of the Detection Task on Canine Search Behavior and Performance (2020)](https://www.frontiersin.org/journals/veterinary-science/articles/10.3389/fvets.2020.00250/full).
- **Limited transfer:** a study of dogs exposed to threatening and neutral human approaches evaluates putative displacement/appeasement behaviors. Its caution about behavioral interpretation argues against coding scratching as an unambiguous emotion label. [Appeasement function of displacement behaviours? (2023)](https://pmc.ncbi.nlm.nih.gov/articles/PMC10066101/).

All utility weights, thresholds, envelopes, sampling rates, and liquid-body translations in this project are **engineering hypotheses or fictional conventions**, not experimentally measured biological constants. No research source establishes that these changes guarantee engagement over months.

## Existing implementation risks found

- Existing motor repertoire recipes already include sniffing and nuzzling. Adding another scheduler without output arbitration would double gestures and create competing face/body writers.
- Opportunistic food selection preceded sleep, and large crumbs bypassed a satiation gate. Mouth-contact protection alone did not protect idle sleeping selection.
- Skill choice preferred highest competence; selection diversity should consider learning opportunity and elapsed use without making failure count as success.
- Touch preference used lifetime counts: early experience could dominate indefinitely. Keep diagnostic counts separate from bounded, reversible behavioral evidence.
- Orb tactic names were indexed by episode number. Context-dependent utility must choose the executed tactic, not only the diagnostic label.
- Reduced confidence after an interception miss is not signed error correction. Prediction adaptation needs consistent coordinates, measured motion, bounded evidence, and stationary/teleport guards.
- Waste originated under the body center. A lower-side outlet must be based on actual silhouette support and available ground; shifting only the visual sprite would leave physics and appearance inconsistent.

## Verification contract

Tests should cover absent stimulus (no fabricated sniff target), repeated unchanged stimulus (bounded habituation), eating/sleep/focus/protection ownership, renewed interest after relevant change, identity-preserving persistence, and no evidence created during offline time. Repeated contact should both earn and satisfy comfort needs rather than escalate an endless request.

For every microgesture, inspect the final rendered or renderer-bound values, not only the selected enum. For waste, verify particles originate outside the central body silhouette on the selected lower side and retain physical behavior afterward.

Active simulated time and offline calendar time must remain separate. Multi-day serialization tests establish persistence and bounds; they do not establish subjective believability, audio comfort, native collision quality, or month-long enjoyment.

## Implementation and test results

Implemented and directly verified in the critic-owned scope:

- Sleep selection now precedes opportunistic food. A hungry or satiated sleeping creature keeps the offered crumb available for waking instead of inspecting it immediately.
- Skill selection combines competence, time since use, uncertainty, curiosity, and positive social value. The selected ID remains the executed ID. Selecting a movement does not fabricate an attempt or success.
- `EcologyState.touch_preference` stores bounded positive-contact evidence separately from lifetime counters. Each new pleasant completed contact discounts old evidence before adding its observed side. New evidence can reverse preference even after thousands of old contacts. Offline absence does not change it; legacy saves start without invented recent evidence.
- `app/src/motor_context.rs` reads the new preference rather than the sign of lifetime counts.

`cargo test --offline -p pet_ecology --test council_v53` passed all eight independent integration tests. They exercise sleep/food priority, actual selected skill ID and no fabricated attempt, measured failed-practice accounting and cooldown eligibility, touch reversal, serialization, legacy compatibility, absence semantics, contextual tactic choice and within-bout stability, rejection of passive user-held contact as motor training, signed free-flight correction with pickup reset, and thirty daily save/reload sessions of new touch and tactic evidence. These tests do not claim full rendered gesture verification.

Review of the additional shared changes found and corrected a presentation boundary issue: supported sleep can use `PoseIntent::Compact` with `LocomotionMode::Sleep`, so the latter also needs explicit tongue/mouth suppression. Presentation now clears oral gesture fields immediately when the source becomes inactive, rather than leaving a smoothed tongue visible during a higher-priority action.

Additional shared implementation reviewed:

- Orb tactics are selected once per bout from current context plus bounded measured contact evidence and recent use. Transition into play also selects a tactic. Passive contact with a user-held ball does not train the tactic; safety interruption is not recorded as a failed motor outcome.
- A free-flight observer measures signed displacement error in desktop-height coordinates, persists bounded correction and prediction-error evidence, and resets across pickup/large discontinuities. Interception converts the correction back to normalized desktop coordinates. This now gives the previously decorative object prediction-error field a production writer and behavioral reader.
- Failed skill execution records an actual unsuccessful attempt and its timestamp. It does not remain eternally eligible as an untried, old skill.
- A lower-side waste outlet uses measured main-body bounds and wall clearance. This is a physical origin change, not merely a sprite offset.
- The self-care source maintains depletable body/contact reservoirs; presentation adds a bounded field only to an unused slot of the base motor packet, preserves support, and reconstructs the packet each update to avoid accumulation. Higher-priority food, voice, sleep, protective, and digestion ownership suppresses optional oral/body gestures.

The thirty-day fixture advances exactly 360,000 active 20 Hz steps (thirty ten-minute sessions) and thirty separate 85,800-second offline gaps. Scripted completed-contact and motor-outcome observations occur only during active sessions. Opposite evidence in the second half reverses both preferences, evidence remains bounded, legacy saves default safely, and no offline outcomes are fabricated. It is a learning/persistence replay with explicit inputs, not thirty days of native animal behavior.

Further final review confirmed that emerging waste's conservative body exclusion applies only while a chain is attached and settled; completed soft traces retain their existing independent physics. General expression prototypes were made more distinguishable: celebration recruits a stronger smile, surprise raises rather than knits brows, and contact refusal retains a frown/tension. Managed blinking and gaze remain separate owners. The integrating agent must verify the final render and whole-path visibility rather than rely on prototype values alone.

Final Windows verification: 137 lifecore, 93 ecology, 71 motor, 269 body, 191 application unit tests, eight independent council tests and six prior longitudinal regressions passed (775 total; four existing ignored). Four self-care ownership tests were repeated after the final field-release guard and passed. Clippy for the application, body lab and habitat lab passed with warnings denied. Thirty-six production-renderer captures on white, black and busy backgrounds were reviewed, including feeding priority and six general expression fixtures. These captures verify supplied states, not their spontaneous lifetime frequency. Remaining limits include virtual rather than physical smell, authored microgesture envelopes, finite learned-tactic choices, lack of a controlled human believability study, and no demonstrated month-long subjective engagement.

Native contact correction: pointer capture also represents gentle held contact. Self-care distinguishes this from actual pulling using contact, strain and displacement; a manipulation latch stays active until release. Tests exercise the real LifeCore tick plus captured input, and distinguish nuzzling, saturation-driven decline, fast pulling and slow pickup. Sleep/stress alone cannot trigger immediate refusal before actual touch accumulates.
