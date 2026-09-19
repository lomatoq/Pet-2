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

See packaged verification logs and `verification/native.json` for final results. Native validation uses disposable state, with no microphone activation. Windows is the available host; no macOS package or installation is claimed.
