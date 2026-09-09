# macOS screen-recording permission repair — 2026-09-09

## Confirmed cause

The installed Pet2 used an ad-hoc signature. macOS TCC logs explicitly reported
`Failed to match existing code requirement for subject io.lomatoq.pet2 and service kTCCServiceScreenCapture`.
The granted requirement was the old executable's `cdhash
407eed9e1f538b05a16e8cc58eb89ba081c4c0be`; the installed executable required
`f4c3873b6a646421f8d58b20b8fcdb8e89dc4697`. The Settings toggle therefore represented
a grant that did not match the updated application. The supplied screenshot was
the initial Screen Recording request, not a periodic recording reminder.

The normal sandbox reported no signing identities. The same read with Keychain
access found a valid Developer ID Application identity for team `KKM3M9K749`.
The previous packaging fallback silently turned this visibility problem into an
ad-hoc release. Merely adding certificate autodetection had not fixed delivery.

## Repair

- Packaging now refuses missing identities and explicit `-` unless the caller
  deliberately opts into disposable ad-hoc builds with `PET2_ALLOW_ADHOC_SIGNING=1`.
- Unsigned CI packaging explicitly opts in; local application updates do not.
- Both installed applications were replaced with bundles signed by the existing
  Developer ID certificate. No keys, trust settings, bundle identifiers, privacy
  database records, or saved-state formats were changed.
- The new designated requirement binds the bundle identifier to Apple's Developer
  ID certificate chain and team `KKM3M9K749`, rather than executable content hashes.

Apple describes designated requirements and privacy-resource identity in
[TN3127](https://developer.apple.com/documentation/technotes/tn3127-inside-code-signing-requirements).

## Validation and delivery

- Shell syntax and whitespace checks passed.
- Missing Keychain identity and explicit ad-hoc identity were both rejected before
  compilation; the explicit CI opt-in reached compilation.
- Release packaging passed. Both extracted and installed bundles passed strict
  signature verification with system certificate access.
- Developer ID authority, team, and certificate-based requirements were verified
  on the actual installed bundles. Two different compiled test executables signed
  with the same identity both satisfied the installed Pet2 requirement.
- The release manifest matches final signed executable hashes and sizes. The
  installed bundles match all packaged file contents and modes.
- Pet acknowledged graceful shutdown before installation. Previous apps and pet
  data were archived; all top-level saved-state JSON hashes were preserved during
  installation. See `PET2_MACOS_PERMISSION_INSTALL_RECEIPT.json` for backup location
  and archive hash.
- LaunchServices launched the new Pet and its runtime acknowledged the preserved
  state. After reauthorization, a further graceful restart also succeeded.

## Verified consent recovery

The first launch still encountered the old TCC requirement, now compared against
the correct Developer ID requirement. The user was asked to renew the Pet2 entry
in Screen & System Audio Recording. A later user-started instance reported
`screen_capture=true` and a fresh visual grid (approximately 9 ms old).

We then gracefully saved, stopped, and relaunched the installed Pet. Fresh bounded
Lab diagnostics again confirmed `screen_capture=true`, `coarse_scene.available=true`,
and a visual grid age of approximately 9 ms. The TCC log contained no identity
mismatch after that restart. The diagnostic lease was explicitly closed; Pet
remains running normally. No permission was silently granted or reset for other
applications. The saved genome hash stayed unchanged across the checks.
