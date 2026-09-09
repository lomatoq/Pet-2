# Pet2 Dev Console

Run the unified console with:

```powershell
cargo run -p body_lab
```

Start directly on live perception with:

```powershell
cargo run -p pet2 -- --dev-mode
cargo run -p body_lab -- --live-pet
```

`F1`–`F4` switch Character, Perception, Behavior, and Diagnostics without
discarding panel state. In Diagnostics, `Space` plays or pauses the captured
timeline, the left/right arrows step frames, and the slider and speed buttons
scrub bounded history. Replay changes only what the console shows; it never
rolls back the running organism.

The four top tabs edit one versioned profile. The Den/Home tab renders the exact
production shader over a reference desktop background, Voice/Sounds uses the
production audio path, and Nervous -> Body exposes perceptual gain calibration
for shape, PBF material, expression, motion, and voice. These gains amplify only
bounded runtime phenotype outputs: solver counts, topology limits, identity
pigment, learning rates, seeds, and the genome loudness ceiling remain locked.


Dev Console reads `telemetry.jsonl` and its single rotated previous file, falling
back to legacy `events.jsonl` when no bounded stream exists. Its typed Lab
controls can issue expiring attention cues and drive pulses, clear pulses, set
focus mode, apply an explicit learning reward, or run any of the 64 indexed
motor programs through the production particle-body path. The motor panel shows
the numeric 64/64 acceptance result, phase grammar, live phase, and override
state; `Cancel` immediately restores autonomous selection. Controls are enabled
only while following a fresh incremental live frame.

The separate Evolution rewind is also read-only. It exposes the exact bounded
before/after genome checkpoints and their lifetime snapshots from validated
`state.json` (at most 64 records), gated by lineage and truncated to the selected
telemetry frame's mutation count/generation so replay never leaks a future
mutation into the past.

The `Accelerated Learning` panel is a separate, explicit workflow from that
read-only rewind. It writes a typed schema-1 config, launches `pet2` as a
separate headless process, and reports the exact 120/60/20/5 Hz clock mode,
episode/gesture distribution, bounded learning updates, consolidations,
eligibility, invariant failures, performance, and state hashes. The default is
`Dry run`; `Fork` keeps accepted state under `evolution-runs`, and promotion is
enabled only for a parsed report whose invariants passed. Promotion first makes
a complete recovery backup, and `Rollback` restores that backup. Synthetic
curricula never modify the user gesture-convention library.

New telemetry groups contain normalized or
aggregate state, not pixels, typed text, raw audio, or native window IDs.

The single preview is the conserved soft-field particle liquid from the research
specification. The analytic fallback is intentionally not shown: every visible
control edits this exact preview, including while playback is paused.

Character exposes:

- fixed-step soft-field physics, viscosity, cohesion, zero-G drag, and reconstruction;
- component separation/re-merge, inertia, return behavior, breathing, bounded idle lean,
  post-drag upright recovery, and budding bubbles with adaptive density-field necks,
  local pinch bounce, and varied normal-cone spray;
- absorption, SSS, transmission blur, refraction, dual rim, rounded highlights,
  RGB caustics, adjustable radial spread for internal inclusions, soul glow,
  emission, chromatic rim saturation, and color;
- face anchoring, neutral HDR sclera, shared jelly mouth/brow relief, physiological
  pupil response, event-based microsaccades, focus lock, scale, eye spacing, and stabilization;
- a macOS-style full-silhouette drop shadow with live X/Y offset, feather, opacity,
  and color controls, plus exposure and real 1x/2x render scale;
- real wall-clock FPS and rolling p95 frame time;
- material, field/density, alpha, face-coverage, component, and split base/coat/fill
  studio-lobe debug views;
- deterministic calm, velocity, stop, reversal, impulse, detach/re-merge, and
  window-pressure scenarios.

`Play`, `Pause`, `Step`, and `Reset same seed` control deterministic playback.
`Space` toggles playback, `R` resets, and `Esc` exits.
Drag the liquid directly in the preview with the left mouse button; releasing it
exposes inertia, slosh, particle-wise field return, and surface recovery. Dragging
temporarily advances the solver even while ordinary playback is paused.
The mouse is one softened gravity field acting on the real particles. There is no
separate face target, captured rigid chunk, hinge, or velocity spring; a slow edge
pull can naturally separate material and the permanent character field gathers it
after release.

`Save JSON` and `Load JSON` work with an arbitrary profile path. `Apply selected
mode to running Pet` validates and atomically writes the shared
`liquid-tuning.json`; a running Pet hot-reloads the last valid profile within 250 ms.
The Den / home panel additionally has a `LIVE` toggle. While enabled, changed den
sliders are debounced to 10 writes per second and applied to the same running Pet
without rebuilding; unchanged body, voice, physics, and storage settings are kept.
Each apply increments the profile revision, rereads the atomic save, and reports
`Saved`, `Waiting for Pet`, or `Applied by Pet` from the Pet acknowledgement file.
Startup and hot reload share the same migrate/sanitize/apply path. With den LIVE disabled, the lab does not
change production until that Apply button is pressed. Invalid or partial writes do
not replace the last valid runtime profile.

R12 integration note: Character keeps the four calibration subsections inside
F1; F4 Diagnostics adds the live nervous-system readouts and accelerated runner.
The exact curriculum and pointer fixtures share the live R12 interoception and
final actuation boundaries, with body feedback published after every physical
substep. Calendar-only remains approximate. Synthetic runs do not validate native
screen/audio capture. Diagnostics also shows the adaptive body predictor and
confirmed counts for quiet/acknowledgement/invitation responses; rest rehearsal
never adds user approvals.
