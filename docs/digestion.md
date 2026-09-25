# Digestion and persistent traces

Food is counted only when the existing ingestion loop consumes a portion. The
saved `metabolism.tract` tracks esophagus, stomach, intestine, bowel, absorbed
mass and expelled mass. Existing saves start with an empty tract; nothing is
invented from historical feeding. Time away does not generate a backlog of mess.

The current game-time stomach and intestine time constants are 32–87 seconds and
48–113 seconds. These are interaction tuning values, not biological claims.
Food warmth/cohesion determine hydration and residue; stimulation/novelty
influence fermentation. Hunger affects swallowed air. Upper air causes a brief
burp, intestinal gas causes lower-side bubbles. Neither happens without input.

Bowel amount, hydration, residence time and pressure determine readiness,
required effort, segment count, thickness, stiffness and extrusion speed. The
pet chooses nearby ground away from the nest and existing traces, approaches
through ordinary locomotion, then applies a small gather field and an effort
expression. Dragging or another physical interruption stops extrusion and returns
unemitted mass to the bowel. Published fragments remain in world coordinates.

Traces use constrained capsule joints, gravity, damping, bending and nonadjacent
joint separation. Their floor is recorded at deposition, so walking to a different
monitor does not move old traces. Settled chains sleep. There is no age expiry.
The persistent node budget is 2048; when full, further extrusion waits for cleanup
rather than evicting existing traces. Gas bubbles are temporary visual particles.

Right-click the nest → **Clean up** → **Start cleaning**. Hovering selects only
the touched chain. Its joints accelerate toward the cursor with slight curl,
while scale and opacity ease out over 0.42 seconds. Esc or right-click exits;
interrupting a suction returns the remaining trace to ordinary physics. Cleanup
mode is transient and is never restored after relaunch.

Regression checks cover conservation, no-food inactivity, food-dependent transit,
persistent traces, targeted cleanup, native WGSL validation and real-liquid orb
pickup in cursor chase. The Dev Console diagnostic
`--digestion-captures <directory>` renders isolated GPU fixtures without reading
or modifying the pet's save. `waste-fixture.json` records the simulated geometry.

## State-dependent play and nutritional body condition (V49)

The existing affect, felt nervous-system state and genome temperament now feed
orb grip-versus-bat utility. Learned affinity, relative toy velocity, fatigue,
reserve and stomach fullness contribute too. Bout drive/fatigue follow live state
smoothly. Carrying ends on destination arrival or accumulated effort relative to
current motivation; a 30-second watchdog is only a failure bound. Homeward return
competes with continued play. Grip ownership survives brief silhouette-contact
loss; new pickup still requires real contact and respects user ownership.
`orb_motivation` telemetry exposes the competing utilities and effort rate.

Saved `body_condition` changes from newly assimilated food minus active-time
expenditure. It cannot jump on ingestion. Its bounded size factor multiplies age
growth and the actual body projection used by contact. Relative volume/mass also
modulates locomotion acceleration, speed and carrying effort. Offline absence
preserves body condition. Old saves start at their current neutral size.

Chewing owns mouth opening/compression after generic facial arbitration and keeps
rendering authority after the idle-mouth fade. Amplitude depends on bolus size
and cohesion; tempo still depends on appetite and processing progress. Gas exits
from the measured lower-side body support, with varying radii and soft separation.

Waste is drawn once per chain as distance to a continuous midpoint spline; a
single coverage/normal evaluation replaces overlapping individually shaded links.
Firm joints retain signed rest curvature with centre-of-mass-conserving angular
constraints; wet chains yield to gravity. Rest curvature defaults to zero for old
saves. These remain game physiology parameters, not biological claims.


## V50 compact, interactive soft traces

Pieces use shorter centreline lengths and smaller radii. The rest shape stores a
separate angle at each joint, derived from bounded, irregular headings; uniform
turns can no longer accumulate into a large circular arch. Each piece keeps its
profile across saves. Existing V49 traces migrate once to a compact scale without
removing traces or changing expelled mass.

Bending uses compliant XPBD constraints. Floor contact includes damped restitution
and friction; a left-click applies a local impulse and wakes the touched chain.
Holding the button does not repeatedly inject energy. Picking a trace does not
capture the orb or drag the pet, and normal clicks never activate cleanup.
Explicit Cleanup retains its existing hover-to-vacuum behavior.

Validation covers click-to-wake at 60/120 Hz, local bending and lift followed by
settling, isolation of unhit pieces, bounded generated lengths, irregular rest
angles, old-save migration and one impulse per mouse press. Native GPU fixtures
include an after-poke frame using the same physics and continuous shading.

References used for the visual/physical model:
- Purina shape reference: https://www.purina.com.au/dog-poop-health-indicators.html
- Position-based constraints: https://learn.physics-simulation.org/examples/pbd.html
