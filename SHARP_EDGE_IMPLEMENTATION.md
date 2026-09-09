# Sharp Edge living tuning — R13 working candidate

Base: `fbff91f`, branch `codex/pet2-nervous-system-r12-impl`, the latest local implementation worktree. The top-level checkout at `1413ed1` is older. Changes remain in this worktree; no branch reset, merge, or replacement of `builds/current` was performed.

## Implementation coverage

| Brief | Implementation |
|---|---|
| 1. Real social waiting | `SocialBid` records bout ID, target, expected response, start timestamp/frame and response edge. Social holds run without motor tempo, normally 3.5 seconds. Minimum readability does not complete them. Fresh matching touch advances immediately; focus/boundary and missing toy terminate appropriately. The selector preserves the hold. Physics no longer invents social credit from contact area. |
| 2. Acknowledgement | Motor readout emits a 100–180 ms recognition/contact/salience/object acknowledgement before movement: gaze lead, focused pupil, small pulse and face roll, short stillness. This is a reflex overlay, not a new action or emotion owner. |
| 3. Gaze | Nervous perception carries target ID, position, kind and confidence. Contact points are used for actual contact danger, never as the universal salient target. Final motor gaze preserves a detected danger source. |
| 4. Outcome truth | Invitation detection is not play success. Novelty remains classifier prediction error; it no longer becomes repeated intentional failure or motor success. Completed ecology outcomes provide success impulses; measured repeated failed object attempts provide a deduplicated failure impulse. |
| 5. Objects | Context distinguishes toy, edible, home, obstacle, support and unknown affordances. Food selector/target paths consume actual morsel positions. Digestion requires an actual morsel-consumed event. Food catalog fixtures now provide an edible rather than a proxy orb. |
| 6–7. Face | Existing felt-state/emotion scores choose one dominant geometry with a bounded secondary modifier. Curious, playful, affiliative, tired, confused, threatened and protest geometry have distinct overrides. Protection masks smile, warm cheek glow, social approach, play, purr and trill after composition. The face no longer adds unconditional attachment glow downstream. Existing body regimes retain softness, fatigue weight, flow and breathing cues. Boundary performance averts gaze and commands a short withdrawal. |
| 8. Dynamics | Expression filtering treats motion away from neutral as attack, including downward tired aperture and negative mouth curvature. Return is slower. Blink keeps its own fast channel. |
| 9. Meaningful stillness | Waiting, acknowledgement, signalling, inspection and boundaries reduce microsaccades to 0.55, decorative droplets to 0.6 and idle lean to 0.5. Breathing, physical slosh and contact deformation remain active. |
| 10. Pet me more | Measured pleasant contact followed by release can start the existing solicitation when social motivation outweighs autonomy. Approach is capped at one 0.03 step; contact side is retained. Fresh acceptance produces relief then the existing rub/nuzzle program. Ignoring produces withdrawal and a need-driven independent activity; related solicitations share a 12–35 second cooldown. |
| 11. Your turn | Existing ecology offer owns object movement and motor presentation. Manipulation moves the real orb; waiting stops issuing object movement commands, alternates orb/user gaze and waits 3.5 seconds. Acceptance must follow the offer timestamp. An accepted offer gets a cooldown; timeout follows the existing carry-home continuation. |
| 12. Help me | Existing trapped-object episode uses two actual impulses and checks failed displacement against the same object before asking. Its wait lasts 3.5 seconds independently of the attempt budget. Resolving the obstruction produces orientation/relief and resumes the original retrieval goal. |
| 13. Learned gestures | Existing convention library remains the owner. Recognition precedes the learned response. High confidence commits; medium confidence holds for repetition, and a fresh matching episode within 3.5 seconds resolves ambiguity. Low-confidence/unmatched gestures retain novel-interaction handling. |
| 14. Afterglow | Pleasant contact leaves an 8-second output residual in softness/viscosity/relief. Actual completed play leaves a 10-second flow residual. Existing LifeCore felt-state integration retains outcome consequences; ignored bids do not inject sadness. Protective state suppresses incompatible positive residuals. |
| 15. Autonomy/focus | Solicitation depends on the balance of social and autonomy drives. Ignored bids can transition to self-play, inspection, grooming or support/rest using existing programs. Focus changes availability and cancels pending credit without reward, habit training or attachment/valence damage in the shared feedback path. |

Stock nervous expression/motion gains are 1.40/1.30 in code and the active JSON profile. The exact old stock pair 1.28/1.20 migrates on load; custom pairs remain authored. Shape, breathing, material, pain/threat sensitivity and the black body identity are preserved.

## Automated evidence

- `cargo test --workspace --quiet`: passed, including the long body integration and 64-program multi-cadence physical test. Five pre-existing tests were ignored by the suite.
- Final core/semantic checks: LifeCore, ecology, motor and their integration tests passed.
- New regression tests: real-time wait, old versus fresh touch, focus withdrawal, toy/edible separation, selector preservation of social hold, protective composition, neutral focus feedback and semantic gaze plumbing.
- Added ecology regression verifies that actual help resumes the original retrieval goal.
- `cargo test -p pet2`: 84 passed, one pre-existing ignored test.
- `cargo test --release -p pet_body --test motor_body_catalog`: passed (all 64 programs at 30/60/120 and variable Hz).
- Clippy with `-D warnings` on the changed runtime crates and application: passed.
- `cargo fmt --all` and `git diff --check`: passed.
- Final release executable: headless simulation (200 ticks), state export and re-import passed. Normal desktop overlay startup was observed with debug labels off and audio disabled, using isolated test data. This is a startup/visual smoke check, not a 12-scenario perceptual evaluation.

Detailed command logs are under `target/sharp-edge-*.log`. The full workspace run predates the final small refinements; affected application/core tests and the physical catalog were rerun as described above.

## Human acceptance gate

**Not signed off by a human observer.** Automated or Lab results are not evidence that the requested perceptual gate passed.

Evaluate the normal autonomous desktop Pet at actual size, with debug labels off. Record 2 seconds before each stimulus and 3–5 seconds after its outcome. Test: (1) soft touch continued, (2) touch stopped, (3) accepted petting bid, (4) ignored petting bid, (5) accepted orb offer, (6) ignored orb offer, (7) familiar gesture, (8) ambiguous gesture/repetition, (9) two failed object attempts and user help, (10) startle/recovery, (11) protest/boundary, (12) Focus Mode.

For each recording independently judge what was noticed, intended target/action, whether Pet is waiting, whether the user's response changed the outcome, and the rough affective state. Exact visual response latency, real displacement and cross-channel readability still require these recordings; authored time constants and numeric tests do not prove them.
