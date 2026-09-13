# macOS parity + R14 integration (2026-09-13)

Base: `7fb14ef` (`codex/pet2-macos-parity`).
Merged source: `5d34273` (`origin/codex/affectionate-companion-r14`), the newest remote branch at integration time.

## Resolution

- Keep macOS host, bundle identifiers, stable signing, screen permission handling,
  console connection and packaging unchanged from the macOS parent.
- Retain adaptive learning, persistent gesture IDs, focus cancellation, edible
  object handling, shared live/replay motor context and Sharp Edge face geometry.
- Add R14 social memory, semantic event appraisal, habituation, intent selection,
  gaze/blink control and audio throttling.
- Feed R14 intent/confidence through the shared motor context, including replays.
- Apply the motor composition after the R14 expression layer, preserving defensive
  ownership and physiological closure. The existing protective-face regression
  exposed and verified this integration correction.
- Render the caller's final expression rather than overwriting it with an older
  phenotype. Preserve lab/manual geometry and semantic scenes.
- Combine geometry convergence diagnostics with R14 channel smoothing and blink
  suppression. Keep direct semantic face values in embodiment to avoid restoring
  ambient affect offsets removed by the macOS/Sharp Edge work.
- Make the R14-specific Clippy allowance compatible with older Clippy versions.

## Validation

- Windows workspace compilation and all-target strict Clippy: passed.
- Formatting and diff whitespace checks: passed.
- Application tests after integration corrections: 93 passed, 1 ignored.
- macOS ARM64 check of lifecore, pet_motor, pet_perception and pet_ecology: passed.
- All workspace test targets passed across the full run and corrected application
  rerun (636 passed, 5 ignored). The full run recorded one failure in the newly
  added test fixture, which assumed a nonzero intent on the first idle tick.
  The fixture now supplies measured pain and checks its explicit protective intent;
  the complete application target then passed (93 tests). No failing target remains.
- The 64-program motor/body catalog passed at 30/60/120 Hz and variable cadence;
  all four Sharp Edge face geometry integration tests passed.
- Headless export/import of saved state: passed (10-second run, then restoration).

## Native macOS delivery limitation

The full macOS cross-check stops in the CoreAudio dependency's bindgen build
because this Windows environment has no usable libclang/Apple SDK toolchain.
This is not a successful native build or runtime verification. No installed Mac
applications were updated. The repository AGENTS.md requires native packaging,
validation and installation of both apps for application delivery; that portion
still requires a Mac with the existing signing identity and installed apps.

The existing cross-platform workflow retains its macOS build, tests, package and
signature checks. Source integration does not establish that screen permission
persists across a signed installed update; that must still be verified on Mac.
