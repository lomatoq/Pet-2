# V20 validation

Windows delivery on 2026-09-19. Builds on the v19 integration of local v17/v18 work and origin/main v16; no remote publication or macOS installation was performed from this Windows host.

- Full workspace baseline after v20 feature integration: 862 passed, 5 ignored.
- Subsequent physics checks: pet_body library 232 passed / 3 ignored; pet2 application 156 passed / 1 ignored; ecology 65 passed; grip boundary regression group 16 passed.
- Whole-face containment regression passed. Face geometry integration: 11 passed.
- Workspace/all-target Clippy with warnings denied passed before the final largest-component face refinement; final check recorded in delivery logs.
- Native right-click opened the compact care menu. A real click on Feed enabled feeding. GPU-rendered menu inspected in target/v20-native/menu.png.
- Native microphone: Headset (Nothing Headphone (a)), listening, 16 kHz mono, no dropped chunks and no device error. User-specific name training still requires the user's examples.
- Native food scenario after physical-contact and food-selection corrections: consumed=true, satiation 0.3193→0.5983, one crumb removed, startup component count=1, mean 98.96 FPS. This used disposable state, not the user's memory. Final packaged-file repetition is saved with the release.

Fixes found through native verification: fixed 176 px navigation margin made taskbar food unreachable; center/bottom-only proximity missed real gel contact; ordinary food utility routed tiny offered crumbs to storage; minimum serialized radius rejected crumbs below 4 px. Each was corrected before delivery. Native test failures are retained in target for reproducibility, not described as passes.

Previous user state backed up in backups/pre-v20-2026-09-19. Cold-start geometry is gathered without replaying stale velocities; per-particle mass/pigment and the companion's cognitive state are preserved.

Final face-smoothing correction: containment is sampled in presentation_update, with persistent position and a critically damped spring (9/s, capped 0.65 local units/s). Render reads do not advance motion. Inner target margin and outer safety margin prevent repeated corrections at the same contour; boundary search is refined rather than quantized to 1/16 increments. Face tests: 39 passed including continuous target jumps at 30/60/120 Hz. Final workspace/all-target Clippy with warnings denied passed.

Compositional motor extension: bounded pursuit braking hesitation, decaying impact caution, effort-dependent reach plus lateral correction, and a mass-neutral wall compression impulse. Physical screen-domain projection remains authoritative. Feeding, sleep and precision interactions do not enable exuberant pursuit. Tuning regression now checks the authored face target before the presentation containment layer; containment has independent footprint tests.

Final compositional-physics checks: pet_body 235 passed / 3 ignored, screen-domain 6 passed, dedicated wall impulse regression passed (compression with unchanged positions/mass and zero net momentum), workspace/all-target Clippy with warnings denied passed.
