# VITA Iteration 2 — Embodied Social Organism

Updated: 2026-08-21

## Goal

Replace the v0.1 primitive presentation with a readable procedural organism while
preserving the portable LifeCore, deterministic identity, offline operation, and
Windows/macOS host boundary.

VITA invariants:

- **Autonomy** — the pet chooses attention, action, and influence strategies.
- **Emergence** — behavior is composed from perception, appraisal, needs, body, and
  learned consequences rather than animation scripts.
- **Continuity** — identity, memory, voice motifs, and learned relationships survive
  restart, migration, metamorphosis, and OS transfer.
- **Individuality** — morphology, motor style, gaze, vocal phrasing, favorite places,
  and social strategies diverge through use.

## Executable slice

1. Morphic implicit body renderer with soft-body pose parameters and a legacy-mesh
   fallback.
2. Anthropomorphic eyes with saccades, fixation, direct-viewer gaze, vergence,
   physical eyelids, slow blink, and wink.
3. Procedural brows and mouth, including lock-free audio-to-mouth feedback.
4. Perception layer for pointer gestures, typing/click/scroll rhythm, moving-window
   ecology, luminance/color features, and salience events.
5. Attention, appraisal, short emotion episodes, a functional predictive self-model,
   and bounded personalized influence strategies.
6. Synthetic window occlusion so the pet can hide and peek without relying on fragile
   foreign-window z-order tricks.
7. Versioned migration from snapshot schema v1 to v2 without losing identity,
   attachment, memories, habits, or vocal motifs.

## Materialized implementation

The branch now contains the morphic renderer, embodied face and soft-body runtime,
lock-free audio articulation, privacy-preserving perception runtime, VITA attention,
appraisal, emotion episodes, predictive self-model, agency estimation, favorite-place
learning, bounded influence policy, desktop-loop integration, headless simulation, and
portable optional VITA state. The remaining gate is validated cross-platform build,
tests, packaging, and hands-on visual tuning.

## Privacy boundary

No key codes, typed characters, screenshots, camera frames, microphone recordings,
clipboard contents, document text, or raw accessibility names may enter persistence.
Only bounded derived features and abstract event summaries are retained.

## Visual gate

The iteration is not complete merely because tests pass. The body must visibly show:

- one continuous soft silhouette;
- a target-readable gaze and direct eye contact;
- eyelid geometry rather than eye alpha fading;
- a readable mouth synchronized to procedural sound;
- anticipation, squash/stretch, follow-through, and volume preservation;
- coherent emotional changes across face, body, glow, voice, and action tendency.
