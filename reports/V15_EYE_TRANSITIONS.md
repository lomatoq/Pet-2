# V15 — eye contours and temporal control

User-reported v14 failure: eye sizes pump mechanically, neutral eyes bulge, pose changes look abrupt and uniform.

Source causes:
- V14 whole-eye target multiplied brow raise by mouth opening and inherited speech eye_scale. Speech changed the entire eye.
- The categorical Startled tag bypassed continuous expression channels.
- Iris, pupil, gaze displacement and reflections used the scaled eye coordinate system.
- Neutral lids revealed a near-complete circular disk; socket relief retained the circle even behind closed lids.
- All authored lid/brow geometry used the same 80 ms first-order interpolation.

Changes:
- EyeShapeController has no mouth/voice/eye_scale dependence. A 75 ms confirmed alarm holds through 450 ms selector dropouts; critically damped recruitment/release retains velocity rather than restarting scale tweens. No random oscillator.
- Iris/pupil/reflection coordinates use neutral radii. Expression changes the opening around them, not their magnification.
- Independent upper/lower arcs meet at shared corners. The lower arc follows cheek recruitment while the upper arc has unequal inner/outer controls. Neutral upper lid covers the upper iris. Hidden socket rings are suppressed.
- Authored eyelid, brow and lip controls have separate directional recruitment/relaxation rates and continuous velocity.

Validation: two controller tests cover 30/60/120 Hz speech/pose chatter and alarm hold/relaxation; 10 final CPU face geometry tests pass. Shader layout/Naga and Clippy pass. Added a 120-frame production-render timeline (neutral speech, alarm, recovery), separate from static endpoint captures. Final GPU/native delivery results below when available.

This is an engineering animation correction, not a claim of biological realism. The 200-behavior repertoire and physical surface rubbing are not completed by these changes. macOS installation cannot be validated from this Windows host; preserve incumbent Windows v14 and saved state.

## User's repertoire follow-up (read-only diagnosis)

Coverage regression confirms 153/200 declared expressive event producers, not 200 physical skills. Missing wall-interaction IDs include 92–98. Virtual care 87/88/90 requires existing support and only applies local fields; it does not select/navigate to a wall. SocialRubNuzzleCursor targets the cursor, not a surface.

Latest live sample examined by the behavior agent had focus_mode=true, orb StoredInDen, ReturnHome/FocusModeRetreat. Focus explicitly suppresses autonomous play and was not changed. Independently, saved orb novelty=0 blocks SoloOrbPlay thresholds >0.04 / SelfPlay >0.12 in pet_ecology/src/episode.rs. Future correction should separate familiar-object play motivation/satiation from novelty, preserving focus; v15 does not claim that correction.

Interest/anger/sleepiness/excitement/oddness/goofiness are examples of a continuous expressive space, not a new six-state catalogue. Mood fixtures added to the capture harness test representational endpoints only and are not counted as newly autonomous behaviors.

## Final package checks

- Release + Voice Lab15/15, fmt, Clippy, Naga/layout PASS.
- Final GPU captures: target/face-v15-final; endpoint contact sheet inspected, sleepy circular ghost rims gone. 120-frame temporal fixture exported as eye-transition.gif and sampled timeline sheet. Den stale texture oracle PASS262144pixels.
- Saved-state copy smoke:240ticks/12s exit0, bounded/travels/pursues_goals=true; optional orb-play criterion false. An earlier PowerShell stdout-capture attempt closed its pipe; the direct smoke invocation passed. Original saved state was not used by smoke.
- Separate Windows v15 package hashes verified: Pet E43728DBB304D563DA2B14CB1D1CAA4FAE9A692EE95510558B4EFD68AD6AF46E, Console927254C8D69DD4E13C512021D538067BEA7398AD5376B2F0B4DAB96E2A690309. v14 retained.
- Launched ordinary desktop v15 PID10656; matching native window observed. Perceptual quality remains subject to user evaluation rather than a claim that tests prove organic animation.
