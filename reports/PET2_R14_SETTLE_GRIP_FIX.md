# R14 blink, supported rest and orb handoff fixes

## September 13: v12 integration (verification/delivery recorded below)

**Delivered Windows v12:** `builds/Pet2-R14-Embodied-v12-2026-09-13`
and matching ZIP in the main workspace. Pet SHA256
`c588ac1eb4e75a1237a71233b10fda8bd18b3a740e5370ba517d97dfcb64554f`;
paired console SHA256
`ab1fd53beb9928c00103f20c3ad939240294d79f7464b94d6363ee2aa79249fd`.
Package manifest hashes verified; VoiceLab 15/15 passed. Fresh copied-current-state
12s headless smoke exited0, bounded; it did not exercise orb play (optional
behavior acceptance was false, required=false). Original state was not reset.

Native PID31100 started from this package with `--dev-mode`. At t59.03s:
rest_sit_settle/rest_hold, measured support=true, gap0px, load.4, aspect2.39976,
96 finite particles, zero screen/presentation jump events. Earlier startle
interrupted one settling bout; this is not evidence of uninterrupted rest.
Audio Ready/no error; existing aggregate typing rate9Hz reached telemetry.
Screenshots confirmed rendering and a broader seated base; neither a screenshot
nor these checks prove all emotion/voice variants aesthetically successful.
Native macOS build/install was not performed on this Windows host.

- Voice: replaced the unchanged phrase-anchor grammar with bounded continuous
  correlated phrasing, conditioned on authoritative valence/arousal/stress/
  fatigue/confidence/attachment. Voice identity and output cadence unchanged;
  exact rhythm imitation and protective calls keep their authored timing.
- Mouth: compression now reaches lip geometry, jaw opening/rounding/width and
  unequal corner pull are causally coupled through existing smoothing.
- Body accents preserve signed gather/yield and roll; no recipe may steal grip,
  root or support ownership. Measured moving carry and unload produce one release.
- Orb: finite pet/orb effective mass, compliant low-restitution contact,
  reciprocal force and carried weight replace the mass-agnostic bounce. Body
  distributes real load locally; sensed load is separate from touch/pain.
- Quiet near attention chooses actual objects or a real floor during rest;
  semantic near-gaze magnification preserves true target coordinates and the
  existing pupil filter. Activity-triggered viewer glance is a separate cause.
- Fictional finite interoception and throat recovery use virtual object doses
  and actual synthesizer effort, not microphone/text/illness inference. No cough
  sound has been added. These are stylized simulations, not biological organs.

The app/core producer union is **153/200**. The other 47 remain explicitly
dormant; adding executable names does not make them physical skills. See the
coverage report for the current list and limitations. Native v12 delivery is
not asserted until the package and fresh launch checks complete.

Final source checks before packaging: 124 app tests passed (1 existing ignored),
116 LifeCore, 53 motor, 45 audio (1 existing ignored), 55 ecology. Strict Clippy
for all six affected packages/all targets passed; formatting and diff checks
passed. The previous comprehensive run also passed the 64-program body catalog
at 30/60/120/variable cadence; mass changes were then separately checked against
all four supported-settle regressions and measured orb force/unload fixtures.
Near-gaze projection and activity glance have explicit 30/60/120 tests.

Activity gaze uses existing timestamp-only DeviceEvent::Key/activity aggregates;
the perception layer also has idle-reset inference, so a typing label is not
proof of a particular physical key. No character content or new capture hook is
used. Hold is 1.3s with 22s refractory, not a recurring timer-driven gesture.
Global input delivery and natural-sounding emotional variation still require
live observation/hearing, beyond deterministic source-level tests.

## September 13: v11 broad contact patch (supersedes oval-only acceptance)

The user correctly distinguished global squashing from spreading against a
surface. The v9 ellipse only changed the width within 3 px of the floor from
32.6% to 33.6% of body width. Production had no particle-level floor constraint;
the desktop host instead aligned the whole rendered body to its lowest point.
The old support force attracted particle centers toward the visible wall and
weighted the center more strongly than the sides, preserving a rounded toe.

The new supported path calibrates one common particle-center clearance on actual
bottom-edge contact, retains it for that contact, and projects a unilateral wall
inside the density iterations and again after topology corrections. Adhesion is
localized to a lower band with a uniform central footprint, while already seated
points do not receive an unbounded downward attraction. A small supported load
acts through the mass. High authored cohesion (2.5 in this saved profile) relaxes
toward 0.62 with the continuous support state, then returns on release; flight
tuning and saved settings remain unchanged. These are stylized material controls,
not a calibrated biological tissue model or a claim of full SPH solid coupling.

This follows the collision-inside-projection structure in
[Macklin and Mueller, Position Based Fluids](https://mmacklin.com/pbf_sig_preprint.pdf).
The distinction between cohesion and solid adhesion is motivated by
[Akinci et al., Versatile Surface Tension and Adhesion for SPH Fluids](https://cg.informatik.uni-freiburg.de/publications/2013_SIGGRAPHASIA_surfaceTensionAdhesion.pdf).
The artistic coefficients and the rendered-kernel clearance calibration are our
engineering adaptation, not values established by those papers.

Acceptance now measures the requested contact geometry: footprint fraction >=45%,
absolute near-floor width >=1.5 times the neutral control, overall width >=95% of
neutral, and height <=90%. The previous requirement of globally widening the oval
by 12% is superseded by this user-requested local footprint criterion, not silently
weakened. Particle mass, finite state, frame continuity and release recovery remain
mandatory. Initial 60 Hz result: footprint 67.4% (about 72 px) versus 32.2% (35 px),
overall 107.45 x 96.71 versus 108.48 x 111.34. At 120 Hz the footprint is
66.0%, about 1.99 times the neutral absolute width; release dimensions recover
within 0.7%. All four settle/release tests, 210 body unit tests (3 ignored),
111 app tests (1 ignored), workspace Clippy and the 15 Voice Lab scenarios pass.

Native v11 was launched from `builds/Pet2-R14-Wetting-v11-2026-09-13/Pet2.exe`,
PID 32868, with the existing identity 5784121873664838231. Package/Lab hashes
match their release manifest; a copied saved-state headless smoke exits zero.
The old diagnostic console was closed because it relaunched the old v9 pet.
At native time 56.22 seconds, RestSitSettle/rest_hold has actual load 0.4,
aspect 2.399994, gap 0, jump events 0, all 96 particles and finite state.
The native screenshot shows a visibly broad, nearly flat floor-facing base,
though the upper body remains rounded; this is not a claim of a fully flat puddle.
An early defense-overpressure bout interrupted the first landing; subsequent
supported hold is observed, but long-run behavioral/visual perfection is unclaimed.

Also corrected rest acquisition: the accelerated authored approach phase timed
out after 0.55 seconds, repeatedly restarting orientation before physical arrival.
It now holds the same approach through ambient proposals until measured arrival
or a bounded 8-second lease. Safety/user/object interrupts retain priority.

## September 13: v9 supported-material correction

Native v8 did reach the visible floor after the apparent-scale projection fix,
but remained nearly round. A saved-profile regression reproduced the reason:
`return_strength = 0.05` made the permanent well 6.8 times weaker than the default
0.34, while surface tension was 2.5 rather than 0.62. At 120 Hz the old supported
shape was only 1.04% wider and 1.06% shorter; 60 Hz behaved similarly. This was
not a frame-rate bug, and the earlier default-profile 20% result did not describe
the user's running material.

Measured contact now requests a stronger, area-preserving physical well aspect
(2.4; airborne requests still cap at 1.34). The continuously changing excess
aspect also raises weak well acceleration toward 3.0. This separates loaded
posture authority from the authored free-flight/fragment-return softness without
changing saved tuning. It changes forces, not rendered particle positions. Loss
of contact relaxes the same state instead of abruptly switching the force.

Actual-profile six-second settle/release regressions: 120 Hz loaded
124.49 x 95.31 versus neutral 107.86 x 110.40 (+15.4%, -13.7%); 60 Hz
126.43 x 95.19 versus 108.48 x 111.34 (+16.6%, -14.5%). Both recover within 1%
after release, retain all 96 particles and have no failsafe/recovery events or
per-frame contour extent changes above 3 px. Native v9 validation is pending.

v9 checks: 207 pet_body unit tests pass (3 ignored), 111 app tests pass
(1 ignored), all four supported-profile integration tests pass, and workspace
all-target Clippy with warnings denied passes. The production-profile acceptance
threshold now requires at least 12% widening and 10% height reduction, and an
airborne control must retain zero measured load and aspect at most 1.34.
Object event coverage is now 132/200 (four additional measured ecology events);
68 catalog entries still have no genuine automatic trigger, as detailed in the
coverage report. This is not a claim of 200 implemented physical skills.

Persistence note: an old pre-v6 backup contains invalid v5 body particle positions
and correctly fails semantic loading. The actual healthy v6 save was copied and
passed a v8 headless roundtrip; the live pet's genome/life history was not reset.

## Reproduced failures

- A sustained fatigue input generated 139 blink peaks per minute.
- Reissuing social blink requests reset the active envelope every frame.
- Sleep closure remained active after waking.
- A confident R14 social/inspection intent displaced selected sleep and home rest.
- Carry commands changed object position directly at 20 Hz and reconstructed
  velocity in normalized coordinates, rather than physical desktop-height units.
- A released den handoff could fight the body's separation solver.
- Pickup and storage required the carrier's centre to overlap the den, even
  when the visible body already touched the object at an edge-mounted home.

## Changes

- Live expression has one blink owner; standalone previews retain their fallback.
  Fatigue changes duration, not an unbounded retrigger rate. All blinks share a
  recovery interval; social requests wait 7.5 seconds. Waking releases closure.
- Sleep/landing and committed object interactions precede ambient R14 intentions.
  Quiet home behavior acquires measured support and holds its settled bout;
  direct contact and defensive interrupts remain available.
- A compliant, acceleration-limited grip moves carried objects at physics cadence.
  New commands preserve current position and momentum, and the grip follows body
  displacement between cognition ticks. Grip strength controls response speed.
- Den handoff preserves bounded momentum and temporarily yields body separation.
  Fast outward releases escape den attraction. Existing throw impulses remain
  bounded by the object speed limit.
- Correct the swept orb contact conversion from height-space velocity to
  normalized horizontal coordinates on ultrawide desktops.
- Retrieval uses measured object contact; storage uses actual object arrival at
  the latch. Neither requires the pet's centre to enter the den.

## Regression coverage

Blink frequency and social completion over one minute; wake release; managed
open-eye rendering; sleep priority; continuous awake supported rest; defensive
interruption; carry priority; continuous pickup with bounded velocity changes;
fast release; den handoff while overlapping the carrier; pickup and handoff with
the carrier's centre outside the den.

Validation: full Windows workspace run passed 645 tests (5 ignored). The
subsequent contact-threshold change passed the complete pet_ecology test suite.
Release headless smoke passed on the first package; final face/grip package is
validated separately below.

## Live validation exposed additional failures (second correction)

The first SoftRest/SoftGrip package was **not accepted**: the user reported eye
jitter, repeated vertical face motion and teleports. A live diagnostic session
confirmed repeated rest interruptions and actual root jumps (not window moves).
At 238.024 s a landing sample had center Y 1305.96 and gap 0.35 px; at 238.231 s
home travel had Y 1245.44 and gap 73 px with the jump counter incremented.

Corrections after that observation:

- The lower collision extent always follows the actual lower silhouette. It no
  longer switches to the cached symmetric upper-lobe radius when leaving rest.
- Boundary projection due to body shape is no longer interpreted as motor
  velocity/acceleration. A stationary shape correction cannot create a collision
  startle. Loaded bottom support follows silhouette recovery only with measured
  support and an actual `screen:bottom_edge` attachment.
- Rest acquires a bounded commitment lease (4 s approach; 8 s on first support),
  preserving real danger/user/object interruptions. Loaded flattening remains
  active during rest hold instead of disappearing after the spread phase.
- The managed gaze path bypasses legacy mood-based scan/side-eye substitution,
  a second microsaccade generator, and flight-derived face sway while supported.
  Brief attention flicker is filtered; absent contact no longer supplies a stale
  gaze point. See `FACE_BIOMECHANICS_RESEARCH.md` for details and regressions.

The first unstable package was closed gracefully. Its files are retained as a
recoverable historical build, not presented as the final correction. The
anatomical MaleCNS research and 200-behavior repertoire are design artifacts;
no whole-brain replacement or complete new gesture library is claimed.

These tests establish code/physics properties, not a subjective guarantee of
animation quality. Native macOS package signing and installation are not exercised
by this Windows delivery.

## Physical loaded-body correction (third package)

The second-correction workspace run completed: 653 passed, 5 ignored; workspace
Clippy and the portable macOS core check passed. That was not sufficient to prove
physical spreading: the loaded-hull experiment initially measured a narrower,
taller body than its neutral control.

The permanent liquid shape field was restoring the resting body toward round,
overriding the local flatten command. In addition, center-only contact detection
missed a floor touching the physical splat boundary. The correction measures a
multi-particle boundary patch at the requested support plane, then smoothly
reshapes the existing area-preserving physical field along the support tangent.
It does not scale the rendered image or directly teleport particles. A support
command without a nearby physical patch cannot activate the deformation.

The deterministic 6-second loaded/neutral comparison now measures approximately
121.71 x 118.38 px loaded versus 114.18 x 126.44 px neutral (6.6% wider and 6.4%
shorter), with mass 96 in both and zero failsafe recoveries. Release continuity,
final package validation and live observation are tracked separately; these
measurements alone do not establish a fully accepted live build.

The release regression also passed: after six seconds unloaded, the formerly
loaded hull measured 112.32 x 125.30 px against a concurrent neutral control of
113.62 x 125.83 px (less than 1.2% relative difference). Every sampled loaded and
release hull change stayed below 3 px per fixed frame. All seven physical-field
unit tests and all-targets Clippy passed.

The v3 Windows package passed Voice Lab 15/15, both executable SHA-256 comparisons
and a 12-second headless smoke using a separate copy of the preserved JSON state.
The original saved pet was then launched from `builds/Pet2-R14-Stability-v3-2026-09-13`
after verifying no other Pet2 process existed. The previous builds remain intact.
The portable `lifecore`, `pet_motor`, and `pet_ecology` macOS cross-check passed;
checking `pet_body` for macOS on this Windows host was blocked by missing libclang,
so neither a native macOS body build nor installation is claimed.

## v3 live rejection and downstream ownership audit

The user rejected v3 for remaining pupil jitter and lack of spreading. The first
30-second live trace had zero root/presented jump events, maximum fixed root step
2.625 px, and a continuous eight-second rest bout. This proved that one teleport
cause was removed, not that the visible eyes or physical seating were correct.

Two additional causal mismatches were then confirmed:

- Native `liquid_visual_bounds_pixels.main_*` included rim/bloom padding. The
  screen host declared contact while real material was still above the support
  plane. The contact bounds now exclude cosmetics while retaining every real
  particle; overall compositor bounds still contain the glow/shadow. The liquid
  support patch now uses the same rendered centers and conservative anisotropic
  splat radii as that material footprint. Diagnostics expose actual field load
  and field aspect. Halo-width invariance is covered by a regression.
- Managed physiological blinks were overwritten by legacy intent preservation
  and scene/VITA projection. They are restored at the final expression boundary;
  a 30-second pipeline regression verifies visible bilateral closures and release.

The user clarified that the remaining eye fault is pupils inside the eyes. An
awake v3 trace at 508–516 s captured the same rest bout repeatedly alternating
between cursor-local gaze approximately (-0.88,+0.35) and self-local (0,-0.09).
Presented pupil X oscillated approximately -0.76,-0.35,-0.03,-0.45,-0.24,-0.52,
-0.30,-0.76,-0.19,-0.82. Motor/VITA gaze writes bypassed the earlier director
filter. Final gaze ownership and independent expressive asymmetry are therefore
still under correction; the user-visible build is not marked accepted.
# Visible contact and procedural repertoire — 2026-09-13, integration in progress

Further native rejection at t272s: supported/gap0/load.4/aspect1.65, yet a
synchronized screenshot still showed ~16px visual gap. Cause now identified:
GPU presentation divisor included `fast.apparent_scale`, CPU contact/offset
conversion did not. Live multiplier1.02302897 shifts the apparent root toward
the fullscreen host center by tens of pixels. v8 source shares the exact bounded
product, preserves pixel offset across scale changes, and updates world/body
conversion consistently. This supersedes any claim that raw iso alone solved
native seating. Independent shader-projection tests and native v8 are required.

Native v6 update: packaged release + Voice Lab15/15, SHA256 pair verified, smoke
using copied saved state exits0. Launched exactly one new Pet2, PID32020, from
`builds/Pet2-R14-Repertoire-v6-2026-09-13`. Prior save JSONs backed up in
`target/pet-state-before-v6`; old build remains recoverable. Source-only additions
after v6 (expanded producer coverage) require another package.

New live session `16221637402174964000`: t2–23s supported RestSitSettle, gap0,
load .4, well aspect settles1.65; no screen tick jumps. At t24 it intentionally
leaves for home/movement and support/load clear. A later screenshot captured
flight, not resting contact, so visual seating acceptance remains pending.
Targeted checks: pet_body lib205pass/3ignored; pet2 app103pass/1ignored; Clippy
workspace clean. Portable macOS lifecore/motor/ecology check passes; native macOS
app installation cannot be verified from this Windows host.

Stronger measured-only support candidate: visible profile119.78×93.52 versus
neutral99.87×113.44 (~+20%width/-18%height), release99.51×112.46 versus
neutral99.92×112.74. Mass96, no failsafes/recovery, continuity<3px/frame. Flight
targets remain<=1.34; supported deformation can reach1.65 with smooth release.

The native v5 still used the compact density kernel's invisible support skirt.
Shader density is `density * (1 - q²)^3`; for one unit-density kernel at iso .34,
the visible radius is only `sqrt(1 - cbrt(.34)) = .54955` of the support radius.
Halo removal alone could not repair that mismatch. New contact reconstruction
uses all real particles and the same raw iso, with conservative fallback and a
geometry-keyed cache. Raw contour excludes raster/AA/filter uncertainty; native
visual acceptance is still required. SupportedSleep now shares the same surface
attachment as SupportedRest (previously omitted).

The visible-contour loaded profile regression passes: 107.23×104.96 px versus
neutral 99.87×113.44 px; mass remains 96, release returns within 1% and no failsafe
or recovery occurs. This is evidence of deformation, not yet native acceptance.

200 typed expressive recipes, 20 families, now have a bounded runtime and app
bridge. Initial adapter has 43 actual event producers; other recipes need
appropriate events. These are expressive accompaniments, not 200 newly solved
physical tasks or an implemented neural fly brain. The base motor remains owner
of navigation, support and grip; existing blink scheduler owns eyelid closure.

## Sleep research and transfer

Supported: [de Melo, Kenny & Gratch, Real-Time Expression of Affect through
Respiration](https://people.ict.usc.edu/~gratch/papers/demelo_jcavw10.pdf) models
rate, depth and cycle shape separately and evaluated expression in 41 observers.
Transfer here is an engineering hypothesis: sleep phases regulate smooth breath
parameters; transitions preserve accumulated phase rather than multiplying total
elapsed time by a changing frequency (which causes late-session phase jumps).

Supported in neonatal rats, not a biological claim about this pet:
[Blumberg et al., Spatiotemporal Structure of REM Sleep Twitching Reveals
Developmental Origins of Motor Synergies](https://blumberg.lab.uiowa.edu/sites/blumberg.lab.uiowa.edu/files/2022-01/Blumberg_Current_Biology_2013.pdf)
describes structured local twitching. Transfer: rare, bounded local responses
within real sleep phases/contact, not frame-random face jitter or forced waking.
