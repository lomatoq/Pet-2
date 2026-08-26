# PET-2 Living Desktop Habitat — acceptance report

Date: 2026-08-26

Status: `READY_FOR_INDEPENDENT_HABITAT_REVIEW`

Base: `3504403462d37ce6e0199c12f83ee32ab6ba0a62`

Branch: `codex/pet2-living-desktop-habitat`

Implementation checkpoint used for the packaged candidate: `59ae70b`

Review range: `3504403462d37ce6e0199c12f83ee32ab6ba0a62..HEAD`

## ARCHITECTURE

- `pet_ecology` owns the portable habitat state: one canonical orb, a den with three slots, up to eight objects, food/taste state, bounded skill signatures, fixed-capacity contacts, deterministic object physics, and the sole multi-step `EpisodeDirector`.
- `app/src/ecology_runtime.rs` is the integration boundary. It combines LifeCore intent with ecology episodes, routes object commands, owns pointer capture independently from PET dragging, advances physics/metabolism, and exposes privacy-safe diagnostics.
- `pet_perception` owns reduced window affordances and the 16×9 spatial visual grid. It does not expose titles, text, raw pixels, native handles, or typed content to the organism.
- `pet_body` owns the procedural orb/den/morsel pass, local external-contact response, transient chromatic/camouflage material effects, and the existing liquid body. Morph topology remains exactly 526 neurons, 17,475 synapses, and 57 populations.
- `desktop_host` owns native overlay, coordinate, sensor, and crash-safe persistence adapters. Ecology persists separately as schema-v1 `ecology-state.json` with an atomic previous snapshot.
- `HabitatLab.exe` is the deterministic, headless review surface for 18 named scenarios, JSON traces, and 16×9 SVG evidence.
- Refusal and focus behavior remain hard constraints: explicit refusal applies a 45-second non-learned bid cooldown, and focus mode preempts play/help bids and routes PET home.

Two release-found defects have dedicated regressions:

1. PET↔orb contact could undo radius-aware desktop bounds and pin the orb center to an edge. The contact solver now chooses the nearest deterministic feasible separation direction.
2. Commands that removed a stored object from the den could leave a stale slot reference. All exit/store commands now share one den-ownership invariant; the exact 24-hour save failure no longer reproduces.

## TESTS RUN

- `cargo fmt --all -- --check` → pass.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` → pass.
- `cargo test --workspace` → 305 passed, 3 intentionally ignored production acceptance replays, 0 failed.
- `cargo test -p pet_body --release production_idle_acceptance_10_seeds_120_seconds -- --ignored --nocapture` → pass for 10 seeds × 120 simulated seconds.
- `cargo test -p pet_body --release production_pointer_stress_60_seconds_circles_and_8hz_zigzag -- --ignored --nocapture` → pass; peak density 1.0001, one component, minimum main mass 96/96, peak extent 0.3569, rigid-fit p95 0.5512, longest rigid run 0.
- `cargo test -p pet_body --release production_fixed_120_replay_is_independent_of_30_60_144hz_presentation -- --ignored --nocapture` → pass; all pair distances 0 and topology 96/96/96.
- `cargo test -p pet2 object_commands_cannot_leave_stale_or_duplicate_den_slots -- --nocapture` → pass.
- `cargo test -p pet2 fixed_update_releases_orb_pinned_between_pet_and_top_edge -- --nocapture` → pass, including 240 repeated contact ticks.
- `cargo test -p pet_ecology one_hundred_explicit_refusals_never_repeat_a_bid_inside_cooldown -- --nocapture` → pass.
- Habitat Lab `every_named_scenario_is_deterministic_finite_and_bounded` → all 18 scenarios pass.
- Packaged `Pet2.exe --headless-smoke 10 --seed 42 --reset-pet --no-audio --data-dir target\packaged-smoke-final` → pass; schema 1, one canonical object, ecology hash `8123284623903296375`.
- Packaged `HabitatLab.exe --scenario habitat_story_v1 --ticks 1600 ...` → pass; 29 outcomes, 53 transitions, save/reload complete.
- Release `Pet2.exe --simulate-hours 24 --seed 42 --reset-pet --no-audio --data-dir target\memory-smoke-fixed` → pass; 86,400 seconds, 345,600 ticks, exit code 0, empty stderr, valid ecology save hash `12888705924936098925`.
- Forbidden-field scan over 11 generated JSON artifacts → pass, 0 matches.
- `cargo build --workspace --release --target aarch64-apple-darwin` → blocked on this Windows host because `coreaudio-sys` bindgen cannot load `libclang.dll`; this is not recorded as a macOS pass.

## MEASUREMENTS

- Flagship EpisodeDirector: p50 0.1 µs, p95 0.1 µs, max 20.0 µs; target p95 <0.15 ms → pass.
- Flagship object physics: p50 0.2 µs, p95 0.4 µs, max 5.6 µs; target p95 <0.20 ms → pass.
- 24-hour release process memory: conservative Windows peak working set 18.43 MiB; target <150 MB → pass with 87.7% headroom.
- Final packaged Morph smoke, three clean sequential runs: p95 368.2/359.0/377.7 µs; median 368.2 µs versus 382.0 µs baseline, −3.61%; allowed regression +10% → pass.
- Final candidate `Pet2.exe`: 6,582,784 bytes, +220,672 bytes / +3.4685% versus published current.
- Final candidate `BodyLab.exe`: 7,226,880 bytes, +7,168 bytes / +0.0993%.
- New `HabitatLab.exe`: 583,680 bytes.
- Candidate ZIP: 6,291,497 bytes.
- Renderer instrumentation reports total-frame render percentiles in debug JSONL, but the added ecology-pass p95 delta was not isolated against the published binary. The `<0.60 ms` delta budget remains an independent-review measurement rather than a claimed pass.

Candidate SHA-256:

- `Pet2.exe`: `07020247338812511FC5A8ECB2885D0D6BD3D75B7F1705CF70943A7D4BBA5BBD`
- `BodyLab.exe`: `1C643C5A1460E2240C1939BC83A75A08F82BD678C2EB502A54C1B6636F3F3455`
- `HabitatLab.exe`: `523AD983A874E2B3DE92665BD01323FED1411DDD1E535C235348BA64129EBD5E`
- `Pet2-windows-x64.zip`: `61FC03A159DFB1BA53BBFA418F37280EFFA1280383D49E25F72026F9A6168816`

## PERSISTENCE/PRIVACY

- Ecology state is separate from the portable organism state and validates canonical identity, finite bounds, den-slot uniqueness/lifecycle agreement, fixed object limits, and bounded learning state before save or restore.
- Primary/previous-backup recovery is tested for organism, Morph, liquid tuning, and ecology state.
- Restart repairs transient grabbed/carried/sleeping lifecycles without losing the canonical orb or learned identity.
- The final 24-hour release run persisted `state.json`, `morph-brain.json`, and `ecology-state.json` successfully after the den-slot invariant fix.
- Eleven scenario traces contain no matches for raw pixel bytes, window titles, typed characters, key codes, clipboard data, absolute paths, native window handles, screen text, or microphone sample buffers.
- Persisted spatial data is normalized geometry and reduced semantics only. Runtime remains offline and adds no network, model, sidecar, IPC, global keyboard hook, microphone, or camera dependency.

## LAUNCH

Candidate package:

```powershell
.\dist\Pet2-windows-x64\Pet2.exe
.\dist\Pet2-windows-x64\BodyLab.exe
.\dist\Pet2-windows-x64\HabitatLab.exe --scenario habitat_story_v1 --ticks 1600 --trace target\habitat.json --screenshot target\habitat.svg
```

Packaged convenience launchers:

```text
Pet2-Fusion.cmd
Pet2-Morph-Shadow.cmd
Pet2-Morph-Fusion.cmd
HabitatLab-Flagship.cmd
```

The existing `builds/current` executables were intentionally not replaced. The implementation plan requires an independent regression review before promotion; this report is the reviewer dispatch, not that independent verdict.

## KNOWN RESIDUAL RISKS

- A live Windows overlay capture before the edge fix verified the transparent, always-on-top host and procedural material, and exposed the orb pinned at the top edge. The exact saved state and repeated-contact regression pass after the fix, but the post-fix live GUI recapture was stopped by the user's Escape and remains unobserved.
- The added ecology renderer p95 delta is not isolated from total render time. The reviewer must compare published and candidate binaries at production 1× before promotion.
- White, black, and busy-background captures were inspected through deterministic SVG evidence, not a complete post-fix live desktop matrix.
- Apple Silicon overlay, Metal, CoreAudio, and packaging remain hands-on platform gates. Cross-building from Windows is blocked by the unavailable macOS/libclang toolchain.
- Subjective voice character, shared-attention target readability, and long-term story recall still benefit from independent human observation even though deterministic semantic triggers and traces pass.
- `dist` is the release candidate. `builds/current` remains the previously independently accepted Morph build until the residual review attacks pass.

## REVIEWER ATTACKS

1. Launch candidate `Pet2.exe`, throw the orb repeatedly into all four desktop edges while PET contacts it, and verify the whole orb remains visible and separates without jitter or pinning. Repeat across monitor seams and 100/125/150/200% DPI.
2. Reproduce `pet_contact_cannot_pin_orb_center_to_desktop_edge` and `fixed_update_releases_orb_pinned_between_pet_and_top_edge`; then replay the user's former saved state with orb position `[0.36860466, 0.0]`.
3. Run `object_commands_cannot_leave_stale_or_duplicate_den_slots`, then a fresh 24-hour `--simulate-hours 24` save. Corrupt the primary ecology file and verify recovery from `backups/ecology-state.previous.json`.
4. Compare production 1× published versus candidate render timings on the same Windows desktop/GPU; require ecology-pass p95 delta <0.60 ms and no FPS/cadence regression.
5. Run the three ignored liquid production replays in release mode and attack long slow pulls, maximum-speed throws, stationary pointer, 8 Hz zigzag, split/remerge, and 30/60/144 Hz presentation.
6. Exercise 100 explicit refusals, focus-mode interruption, audio unavailable/recovery, visual capability unavailable, window spam, den full, morsel refusal, and trapped-orb help. Confirm no coercive retry loop or identity loss.
7. Inspect `habitat_story_v1`, `orb_window_bounce`, `orb_trapped_help`, `den_store_restart_retrieve`, `morsel_accept`, `saliency_shared_attention`, and `skill_transfer_new_region` JSON/SVG pairs and rerun them with the same seed for byte-stable semantic hashes.
8. Scan all persisted/exported files for raw pixels, titles, text, key codes, native handles, absolute paths, microphone samples, or unbounded histories.
9. On Apple Silicon, build/package natively and validate overlay click-through, object hit testing, monitor/DPI remap, Metal rendering, CoreAudio loss/recovery, and a real desktop visual capture before promotion.
