# Structural phrase correction — 2026-09-13

The preceding rendition composer varied timing and pitch but retained the stored
syllable count, order and articulation. Numerical uniqueness was therefore not
evidence that a listener would hear a different phrase. The inspected persisted
SoftContact motif `16125364461231072457` has five syllables; persisted motifs are
not restricted by the current initial-motif generator's shorter defaults.

Producer-side composition now chooses 1–5 segments, reorders source gestures and
samples correlated oral-frontness, constriction and nasality trajectories. Phrase
duration is normalized to a bounded budget. Existing affect controls influence
tempo, pauses and contour; the anatomical voice genome and source-credit ID are
unchanged. Purr is one sustained segment; frustration is 1–2 short segments.
Startle and explicit RhythmMimic preserve their authored structure. Emission
frequency is unchanged. No work was added to the audio callback beyond publishing
the actual performed `syllable_count` in its existing atomic state word.

## Reproducible A/B

Run `cargo run -p pet_audio --example phrase_ab -- <state.json> <output-dir>`.
The example reads state without modifying it. Both branches use the production
SynthVoice sample renderer and the saved voice/motif. Requests are controlled
SocialContact examples, not a recording of the live request's unknown mood.

At `artifacts/audio-structural-ab`:

- `A-stored-five-segments.wav`: eight uncomposed five-segment phrases, 19.87 s.
- `B-production-structural-phrases.wav`: counts 4, 4, 3, 3, 3, 2, 5, 1; 14.75 s.

The waveforms are finite and bounded. Listening validation remains necessary;
no subjective quality claim follows solely from counts or fingerprints.

## Checks

`cargo test -p pet_audio --lib`: 47 passed, one explicit Windows endpoint smoke
test ignored. `cargo clippy -p pet_audio --all-targets -- -D warnings`: passed.
Tests cover 1,000 ordinary renditions with all five segment counts and more than
20 source orders, 1,000 requests each for sustained Purr and short frustration,
bounded durations/energy, deterministic identity and mixed-affect controls.
The ordinary 1,000-rendition debug preparation measurement was about 34 ms on
this machine; this is not an audio-callback benchmark or a perceptual score.
