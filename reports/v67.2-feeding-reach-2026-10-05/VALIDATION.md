# V67.2 Feeding Reach — Windows validation

## Behavior changes

Food in the den now retains its lateral mouth target. The contained whole face tilts toward reachable food, and contact helpers use the same frame as the rendered eyes and lips. The food position owns den membership; the entry waypoint owns locomotion until actual admission, preventing premature meal support from flattening the creature outside the entrance. Consumption still requires awake, open-mouth, physical contact and visible ingestion. An unsuccessful reach attempt temporarily yields to other food, without changing food preferences or learning a refusal.

Right-click cancels feeding/cleanup before routing another menu request. An additional distinct right-click may reopen the menu; an already visible menu dismisses with its existing fade. Native secondary-button handling also works before egui has a pointer position. Initial hidden-window focus events no longer immediately dismiss a newly shown menu, and late focus-loss messages cannot reverse a completed dismissal.

The earlier soft pink waste material remains the base. The shader adds a faint continuous rim, six-percent maximum edge darkening and a bounded exterior glow. It keeps the original geometry, normals, floor mask and cleanup opacity; no extra pass or texture is introduced.

## Scope of verification

All checks use owned test processes and fresh isolated data directories. The live pet and its saved state are not modified. A copied user snapshot was not a recording of the reported failure; reach evidence comes from explicit synthetic fixtures using production body/ecology code.

The den fixture covers resting crumbs, crumbs falling from above and entrance from outside, with two lateral crumbs, a stored orb, production seed 5784121873664838231, scale 2 and a 3440x1440 viewport. Each bite requires actual admission, supported open-mouth contact, visible ingress and renderer/contact agreement below 0.001 px. Eye centers remain in liquid, failsafe/recovery counts remain zero and the unchanged maximum face-step gate is 3.5 px. The measured maxima are 1.806, 2.432 and 1.925 px respectively. This is not a claim that every seed and morphology can reach every possible crumb; unreachable food must yield through the bounded cooldown.

Native menu replay posts secondary/focus events only to the fixture's own Win32 windows. Feed activation uses the existing diagnostic RawInput fixture against the actual egui widget, followed by real IPC/session/ack handling; it is not a test of OS cursor injection. The sequence checks dismissal without cursor movement, early outside-focus dismissal, late-message fade ordering, feed activation, right-click placement cancellation and a subsequent distinct reopen.

Waste review compares 186 native premultiplied RGBA GPU captures at two scales and three shapes against the earlier shader. It checks unchanged opaque core coverage, no removed alpha coverage, the physical floor mask and frame continuity. Review composites premultiplied output in linear color space on light and dark backgrounds.

## Release-sensitive baseline failures

The complete release test command is **not fully green**. Two unchanged physics tests fail identically in an isolated archive of previous commit 5547514f3ccab2659b8775e67e81e55b999694a9:

- `supported_rest_forms_a_flat_patch_without_moving_its_reference_frame`: current and baseline release bottom jitter 0.07096863 px exceeds the unchanged 0.05 px assertion. Current debug passes at 0.015274048 px. All reported sample coordinates match the baseline exactly.
- `voluntary_separation_uses_real_production_particles_under_the_hard_guard`: current and baseline release diagnostics match exactly: detached 0, components 1, minimum fragment 96, rejects 13, strain 1.7065312, radius 0.6846811, density error 0.08242585, max speed 0.5005877, failsafe/recovery 0, finite true. Current debug passes.

Liquid solver files and both assertions remain unchanged. Matching the baseline isolates these from this patch; the underlying release/debug numerical mechanism remains unresolved and is not claimed fixed. Baseline logs and manifests are in the outer local report's `quiet-rest-baseline` directory; copied private state is excluded from Git.

## Final delivery gates

Strict workspace Clippy with `-D warnings`, the current-source release build, native replay against the final release pair and all 186 final-binary GPU capture checks pass. The reviewed Pet SHA-256 is `f56726d89840e1123a146b3942e61cf83ba25d1963b4923cba53637631c12dfb`; the console SHA-256 is `46e2b62dd6a3ba32ccd200eefae2c028f73d026ae3e11156a897544dbb611d28`. Both shipping binaries were rebuilt from the current source after the baseline comparison.

Full debug test totals and actual packaged startup/replay results are recorded in the outer workspace report before delivery. Packaging preserves V67.1's manifest and all its files, verifies the reviewed binary hashes and records the source commit. No macOS package was produced on this Windows host.
