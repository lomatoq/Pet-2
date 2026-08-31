# Project Memory — Pet 2

Updated: 2026-08-31

Current gate: living-desktop habitat candidate ready for independent regression review

Current branch: `codex/pet2-living-desktop-habitat`

Base: `3504403462d37ce6e0199c12f83ee32ab6ba0a62`

Packaged implementation checkpoint: `d6e6435`

## Product truth

- Pet 2 is one persistent, offline-first, deterministic procedural creature for Windows and macOS.
- The body, voice, behavior, habitat, memory, and learning remain generated locally; runtime has no network, model, sidecar, or recorded-asset dependency.
- VITA invariants remain autonomy, emergence, continuity, individuality, inspectability, and non-coercive attention.
- The habitat is a persistent world for causal behavior, not a collection of scripted animations.
- Green tests are not a substitute for live visual, listening, cross-platform, or independent regression review.

## Current golden path

- Development PET: `cargo run -p pet2`.
- Headless smoke: `cargo run -p pet2 -- --headless-smoke 10 --seed 42 --reset-pet --no-audio --data-dir target/smoke-state`.
- Habitat Lab: `cargo run -p habitat_lab -- --scenario habitat_story_v1 --ticks 1600 --trace target/habitat.json --screenshot target/habitat.svg`.
- Candidate Windows package: `dist/Pet2-windows-x64/` and `dist/Pet2-windows-x64.zip`.
- Published canonical binaries remain in `builds/current`. They were not replaced because the habitat plan requires an independent regression review first.
- Brain modes remain `morphic` by default, plus `fusion`, `morph-shadow`, `morph-fusion`, and `classic`; `Win+Alt+B` cycles all five.
- Habitat controls: `Win+Alt+F` food, `P` focus, `L` shared attention, `T` teach, `M` metamorphosis, `B` brain mode, `R/N` debug feedback, and `D` debug snapshot, all with `Win+Alt`.

## Habitat implementation now on the branch

- One canonical persistent orb with deterministic throw, swept boundary/window collision, local PET contact, bounded restitution/drag, familiarity, preference, novelty, wear, and normalized saved position.
- A persistent den with edge-relative anchor, three validated slots, visits, familiarity, comfort, storage/retrieval, focus retreat, sleep return, and monitor/DPI remapping.
- Edible-light morsels with bounded count, nutritional/light profiles, inspection, accept/refuse/store choice, taste memory, temporary metabolic effects, and non-coercive absence behavior.
- Reduced window ecology with geometry, velocity, pressure, shared contact normals, local liquid response, escape/recovery, window riding, and trapped-object help.
- A 16×9 reduced visual frame with salience, habituation, explicit shared-attention cues, chromatic echo, and bounded camouflage without text or raw-pixel persistence.
- Bounded mimesis signatures for pointer paths and rhythms, near-duplicate merge, motor-error learning, figure-eight spatial transfer, and persistent skill competence without Morph topology changes.
- A single `EpisodeDirector` composing multi-step Offer, Chase, Intercept, Solo Play, Carry Home, Retrieve, Return/Sleep, Escape/Recover, Window, Food, Shared Attention, Chromatic, Camouflage, Skill, and Rhythm episodes.
- LifeCore still owns repertoire selection, anti-repeat, outcome credit, and audible acknowledgement. Ecology contributes only grounded semantic vocal triggers and reduced rhythm intervals.
- Habitat Lab ships 18 deterministic scenarios with pause/step/speed controls, JSON traces, 16×9 SVG evidence, candidate scores, transitions, contacts, normals, paths, outcomes, timings, and privacy capabilities.

## Architecture truth

- `lifecore` owns needs, affect, action arbitration, memory, habits, development, VITA mind, identity, and procedural voice repertoire choice.
- `morph_brain` remains the same-process pinned 526-neuron/17,475-synapse/57-population LIF network. Habitat work did not add neurons, synapses, populations, actions, Node, IPC, or a second homeostasis owner.
- `pet_ecology` owns portable objects, den, food/taste, mimesis signatures, deterministic physics, episode selection, and ecology debug semantics.
- `pet_body` owns liquid physics, procedural body/face/material, ecology object rendering, local external-contact response, hit-shape projection, and presentation.
- `pet_audio` owns procedural synthesis and device-independent callbacks. No microphone or audio samples are collected or persisted.
- `pet_perception` owns reduced pointer, rhythm, window, and spatial-visual affordances; it never receives typed content or window titles.
- `desktop_host` owns platform APIs, virtual-desktop coordinates, transparent overlay behavior, capability fallbacks, and crash-safe storage.
- `app/src/ecology_runtime.rs` is the sole runtime seam combining brain intent with an episode and applying bounded object commands.
- All ecology hot paths use fixed capacities: 8 objects, 8 external contacts, 32 windows, 144 visual cells, 16 learned skills, 32 path samples, and 16 speed samples.

## Persistence and privacy truth

- Portable organism state, Morph learning, liquid tuning, and ecology state have separate ownership and atomic previous snapshots.
- Ecology schema 1 validates one canonical orb, unique object IDs, finite normalized values, maximum counts, den-slot uniqueness/lifecycle agreement, bounded taste/skill memories, and serialized RNG state.
- Shutdown interrupts active episodes; restart repairs transient grabbed/carried/sleeping states without duplicating or losing the orb.
- Persisted habitat data contains normalized geometry and reduced semantics only. It excludes native handles, process IDs, window titles, typed characters, key codes, clipboard content, raw pixels, screenshots, screen text, microphone buffers, device handles, and absolute paths.
- Eleven final scenario artifacts pass the forbidden-field scan with zero matches.

## Validation state

- Formatting passes.
- Strict workspace/all-target/all-feature Clippy passes with warnings denied.
- Full workspace/all-target suite: 311 passed, 3 intentionally ignored long production replays, 0 failed.
- All three ignored optimized liquid production replays pass independently: 10×120-second idle, 60-second adversarial pointer, and presentation-independent fixed-120 replay at 30/60/144 Hz.
- All 18 Habitat Lab scenarios are deterministic, finite, bounded, and pass.
- Final packaged 10-second headless smoke passes with one canonical object and ecology hash `8123284623903296375`.
- Final packaged flagship passes with state hash `4913997946884220323`, transition hash `5335229718525845320`, 29 outcomes, and 53 transitions.
- A fresh release 24-hour/345,600-tick simulation exits 0, writes all three state files, and validates ecology hash `12888705924936098925`.
- The 24-hour run found and then verified the fix for stale den-slot ownership. Object exit/store commands now share one slot-clearing invariant.
- The user-observed top-edge orb pin was reproduced from saved position `[0.36860466, 0.0]` and fixed with radius-aware feasible contact separation. Unit and 240-tick runtime replays pass.
- A later live replay exposed the separate multi-monitor failure at `[0.52105993, 0.9730903]`: a stationary maximized window occupying one monitor expelled an already embedded orb toward the virtual-desktop bottom. Static frame-start embedding is now pass-through, fresh swept crossings and moving windows remain physical, and a zero-velocity radius-boundary state receives one deterministic inward recovery impulse.
- A simple pointer press no longer snaps or cancels motion. Drag requires 4 physical pixels, preserves the contact offset, follows with bounded spring response, clamps the center by radius, and releases bounded velocity.
- The semantic face now follows the permanent carrier center plus its authored upright offset; rotating face-weighted material affects support/confidence but cannot orbit the face.
- Flagship EpisodeDirector p95 is 0.1 µs; object physics p95 is 0.4 µs.
- Conservative peak working set during the 24-hour release run is 18.43 MiB against the <150 MB target.
- Final three-run Morph p95 median is 404.5 µs versus the 382.0 µs published baseline, a 5.89% regression and within the +10% no-regression budget.
- Candidate Pet size is 6,583,296 bytes, +3.4766%; Body Lab is +0.0993%; new Habitat Lab is 583,680 bytes; candidate ZIP is 6,435,560 bytes.
- Windows release/package/smoke gates pass. A macOS cross-build from Windows is blocked by unavailable `libclang.dll` for `coreaudio-sys`; no macOS runtime claim is made.

## Visual evidence

- Deterministic SVG/JSON pairs exist for the flagship story, orb offer/play, den store/restart/retrieve, window squeeze/escape, morsel acceptance, shared attention, skill transfer, window bounce, and trapped help.
- The flagship and window-bounce SVGs were rasterized and inspected for safe gaps, labels, paths, contact normals, candidate panels, timeline readability, clipping, and privacy disclosure.
- A live Windows capture verified the transparent always-on-top overlay and procedural PET/material, and exposed the top-edge orb pin.
- The intermediate live replay exposed the one-monitor-maximized-window residual. The final `d6e6435` deterministic replays pass, but the final live capture was stopped by the user's Escape before observation. Do not claim final live visual confirmation until an independent reviewer repeats it.

## Candidate package hashes

- `Pet2.exe`: `CBF8B78168919C0EC426AC5464920A58D8593554E043ECE432FF136FED41D337`
- `BodyLab.exe`: `3316EB79C9D94E5A49D1E355C3C9799AABFB8F6C7585C116BA3E5972FFFD7C25`
- `HabitatLab.exe`: `A999F1EA0B7EAE852674C36F0ABB6852EEC3429424791F6A37AE00F8510439EC`
- `Pet2-windows-x64.zip`: `51694E930468721744F0B330FB81CA6D503F8AB8B94D1EB66ECC42AFA33BF5F7`

## Remaining review sequence

1. Independently reproduce the fixed edge-pinning and den-slot attacks on the candidate.
2. Measure candidate-versus-published ecology renderer p95 delta at production 1× and require <0.60 ms.
3. Repeat white/black/busy-background and multi-monitor/DPI live captures after the edge fix.
4. Run the full reviewer attack matrix in `.solo-studio/HABITAT_ACCEPTANCE_REPORT.md`.
5. Build and validate overlay, Metal, CoreAudio, packaging, and persistence on real Apple Silicon hardware.
6. Only after those independent gates pass, promote the candidate binaries and launchers into `builds/current` and update its published hashes.

## Active risks

- The ecology render delta is instrumented but not isolated against the published binary.
- Post-fix live Windows visual confirmation is pending because the user stopped the recapture with Escape.
- Interactive macOS behavior and packaging still require Apple Silicon validation.
- Subjective voice character and causal-story readability remain human-observation gates.
- New perception providers must preserve immediate reduction and never persist raw private content.
