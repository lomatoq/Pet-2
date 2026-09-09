# Dev Console startup and macOS screen access

## Confirmed failures

The installed console opened Perception with `--live-pet`, but that flag only
selected a panel. Its monitor remained disconnected and displayed historical
telemetry with OFFLINE / STALE. The ordinary launch never requested a Lab lease.

Testing automatic startup also exposed a separate existing launch defect: Pet
was spawned directly as a child executable. TCC logs attributed its screen
recording request to `io.lomatoq.pet2.dev-console`, so the screen permission
already granted to Pet did not apply. The console could become LIVE while the
visual channel remained unavailable.

## Changes

- Live startup now initiates the existing bounded connection protocol. Character
  preview startup remains offline, and the Disconnect action remains explicit.
- Packaged macOS Pet launches use LaunchServices through `/usr/bin/open -a`.
  The selected bundle path and optional data directory are separate arguments.
  Development binaries and other platforms retain their direct launch path.
- Promotion displays the PID from the actual loaded-state acknowledgement, not
  from the launcher process. Existing state/hash acknowledgement checks remain.
- Packaging explicitly signs the console executable under Resources before
  generating final executable hashes. Signing the outer app with `--deep` had
  previously left that executable linker-signed. Both bundles and this executable
  are now individually verified with the same Developer ID identity.

## Validation and installed result

- All 30 Body Lab tests passed; strict Clippy, formatting, and whitespace checks
  passed. Added coverage exercises lease request, matching-session telemetry,
  renewal, release, offline preview, and macOS bundle launch arguments.
- Final release packaging passed. Installed file contents/modes match the ZIP;
  both app signatures and the nested console executable were verified, including
  Developer ID authority/team and final release-manifest hashes.
- Saved and stopped Pet before each installation. Both old applications and
  saved data were backed up; top-level state JSON files remained byte-identical
  throughout final installation. Details are in the adjacent receipt.
- Cold start opened only the installed console. It launched Pet through macOS,
  connected automatically, and received 55 new telemetry frames over 11 seconds.
  Screen capture and the coarse visual scene were available throughout; visual
  samples were approximately 10 ms old. Pet and console were separate processes
  owned by launchd, rather than a parent/child capture responsibility chain.
- Reopening the console connected with a new lease to the same running Pet PID.
  Another 55 frames arrived over 11 seconds, with screen capture still available.
  Both applications were left running.
- The native Quit path did not emit an immediate CloseSession in the observed
  check; the existing 10-second lease expiry remains the fallback for that path.
  Unit tests cover explicit release on monitor drop. No claim is made that every
  native termination runs Rust destructors.
- An intermediate screenshot verified Connected/LIVE and revealed the visual
  permission defect. Final window screenshots could not be captured by the
  window-capture tool; final visual availability was verified using fresh runtime
  telemetry rather than a screenshot.
