# V19 integration and behavior changes

Source branch: `codex/pet2-integrated-v19` in `.worktrees/macos-parity-r14`.

## Integration

- Fetched GitHub on 2026-09-19. `origin/main` was `0754018` (v16).
- Preserved all local v17/v18 production edits in `ef02561` before merging.
- Merged the remaining R12 branch in `d83c238`; all local branches are now ancestors.
- Retained newer v18 rendering, semantic face ownership and notification-based audio endpoint recovery where old R12 changes conflicted. Retained compatible breath continuity and audio callback heartbeat telemetry.
- Root checkout's loose `phenotype.rs` is an older incomplete contract; preserved in place, not substituted for the current contract.

## Behavior

- Startle: 35-65 ms orient/freeze, 120-220 ms launch, bounded brake, recheck, recovery. Actual phase sampling follows cognition frequency. A sleeping zero-speed intent can launch; drag ownership, integrity priorities and startle rearm/cooldown remain authoritative.
- Startle cannot be replaced mid-bout by a lower-priority idle/sleep program or a stale surface-care/social-regulation packet.
- 24 deterministic orb trajectory/contact combinations: stalk, left/right hook, feint, intercept and retreat approaches, each combined with pounce, side bat, lob or roll. This is a compositional repertoire, not 24 new independent brains.
- Each bout keeps its identity. Fatigue reduces speed/impulse and lengthens pauses; contact, user ownership, wall/floor clearances and existing throw limits remain physical gates.
- Orb navigation survives later phenotype/motor presentation while yielding to defense, dragging, focus and surface care. Quiet mode still reduces effort.
- Added variant name/count and audio callback heartbeat to telemetry.

## Verification

Full workspace: 858 passed, 0 failed, 5 intentionally ignored. Final focused
checks after the last navigation refinements: 285 passed, 0 failed, 1 ignored.
`cargo clippy --workspace --all-targets -- -D warnings` and formatting passed.
Voice Lab: 15/15. Release package hashes verified after copying.
Native Windows desktop: 63 distinct telemetry frames, average 101.92 FPS,
minimum 58.46 FPS, observed launch/brake/recheck/recovery and finite motion.
The first sandbox smoke could not capture the desktop and exceeded its startup
window; the ordinary desktop run passed with a bounded startup wait.
See packaged verification logs and `verification/native.json` for final results. Native validation uses disposable state, with no microphone activation. Windows is the available host; no macOS package or installation is claimed.
