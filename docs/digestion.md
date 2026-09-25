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
