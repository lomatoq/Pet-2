# Eye/face rig evidence and implemented scope — V13

## Observed missing mechanism

The rendered lid aperture used authored lid shape, blink, squint and eye aperture, but not gaze. `EmbodiedRuntime` already exposes the final smoothed pupil direction. A near-floor fixation therefore moved pupils without any lid progression. The patch composes lid progression from that **presented** direction, not from a newly selected target, after each presentation update.

Upper inner/outer controls receive `gaze_y * [0.30, 0.26]`; the lower control receives `gaze_y * 0.08`. Each eye independently attenuates the additive effect by `(1 - blink)^2`. Values are recomposed from cached authored controls, never accumulated, and sanitized to existing ranges. Lower-lid range does not allow negative retraction, so downward gaze only relaxes an already-raised lower lid; this is a limitation of the existing ABI. No pupil filter, blink schedule, oscillator or shader layout changed.

A separate bounded smile synergy adds lower-lid cheek raise: `positive_smile * 0.22 * (1-compression) * (1-0.5*mouth_tension)`. This prevents a broad playful mouth from being paired with entirely uninvolved eyes. It is a stylized art-directable corrective, not a physiological muscle simulation.

## Primary evidence

- [Pinskiy and Miller, Realistic Eye Motion Using Procedural Geometric Methods](https://media.disneyanimation.com/uploads/production/publication_asset/66/asset/realisticEyeMotion.pdf), section 4: gaze-driven lid progression, neutral blending and suppression during blinking; also local gaze-dependent shape changes. Our tiny procedural controls implement only the progression/suppression principle, not their spherical skinning, wrinkles or cornea collision model.
- [Trutoiu et al., Modeling and Animating Eye Blinks](https://la.disneyresearch.com/wp-content/uploads/Modeling-and-Animating-Eye-Blinks-Paper.pdf): recorded blinks exhibit spatial and temporal asymmetry; their perceptual study favors recorded-data-derived dynamics with complete closure over textbook alternatives. This does **not** justify random independent winks, perpetual blinking, or inventing biological timings. Existing managed blink ownership and timings remain unchanged.
- [Disney, A Deformer-Based Approach to Facial Rigging](https://media.disneyanimation.com/uploads/production/publication_asset/97/asset/facial.pdf): localized curves plus pose-space correctives support controlled non-rigid facial shapes. This motivates the separately controlled brow sections and lip contours, without claiming equivalence to a production film rig.

## Validation and limits

The actual companion director now has three explicitly causal contrasts: inspection uses curiosity/confidence for a small questioning opening and raised brow; play uses confidence/play readiness for smile, opening and coherent squint; recovery from a miss uses frustration for lip compression, downward mouth curvature and reduced aperture. These feed existing runtime targets before the protective override, with no gaze/blink writes. Four director regressions pass, including protective ownership and asymmetric-transition continuity. This establishes reachable target generation for those families, but not that later scene arbitration preserves every channel in all live contexts.

Added final-RenderParameters regression for upward/downward fixation, independent blink suppression, complete closed-eye preservation and unchanged mouth at 30/60/120 presentation rates. Existing gaze controls remain opt-in/managed as before. Separate six-expression fixtures exercise playful crook, skepticism, concern, effort, startle and open jaw.

**A CPU fixture proves representability and propagation, not autonomous reachability.** It cannot show that the running companion selects these intensities, keeps them long enough, or combines them attractively. In particular, `FacePose::Startled` has mouth opening 0.65, whereas the autonomous director prototype starts at 0.10; later scene/motor ownership must be traced to establish the actual live endpoint. Production-render contact sheets and live autonomous telemetry are separate acceptance gates. Numerical distinctions are not evidence of emotional appeal, naturalness, or a completed 200-behavior repertoire.
