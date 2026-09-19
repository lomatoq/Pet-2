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
