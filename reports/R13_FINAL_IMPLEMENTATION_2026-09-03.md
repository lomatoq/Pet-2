# R13 final implementation — 2026-09-03

## Outcome

The movement/audio/orb/Pet Lab plan is implemented in the ordinary release.
Manual visual calibration is no longer required to make any of the 64 motor
programs reachable or safe. Pet Lab remains an optional visual review surface.

## Motion

- Body action timing and final travel speed use the global 2x tempo. Face,
  gaze, blink, eye and mouth presentation continue on unscaled real time.
- Controller response and base acceleration are not multiplied by the tempo.
- Screen-space jerk budgets are 32 reference spans for ordinary movement, 48
  for seek/flee/orbit and 18 for landing/sleep.
- Jerk-aware arrival braking prevents one-frame reversal and overshoot.
- Modal deformation uses a frame-invariant 14 Hz acceleration filter and a
  soft knee. True collision impulses remain a separate channel.
- The last two body-motion signatures are excluded deterministically from the
  next repeated bout. Facial presentation is not part of the signature.
- Telemetry exposes screen jerk, acceleration delta, controller saturation and
  the true collision-impulse count.
- Sleep keeps the fast pre-sleep/edge approach, then lands with the 18-span
  budget, deforms the contact side, confirms measured support, closes the eyes
  and enters slow NREM breathing.

The calibrated physical speed tests remain:

- seek: 540–620 px/s;
- flee: 580–670 px/s;
- orbit: 470–570 px/s.

## Windows audio

- Pet follows the Windows default `eRender/eMultimedia` Core Audio endpoint,
  not the communications endpoint.
- An `IMMNotificationClient` receives exact default-device changes. A 500 ms
  endpoint-ID poll is the recovery path.
- The stream is created and replaced only on `pet2-audio-owner`, never from the
  render/audio callback.
- A started request may be cut during a switch but is never replayed on the new
  endpoint. Unheard requests are retained and re-enqueued; two streams are not
  left active together.
- Telemetry includes endpoint ID/name, generation, switch reason and recovery
  errors.

The explicit Windows host smoke resolved:

`Headphones (Nothing Headphone (a))`

with Core Audio endpoint ID:

`{0.0.0.00000000}.{aaa4de1a-8723-4c0b-b10d-b93e0c3a4f16}`

The fake watcher transition test verifies that only a new multimedia endpoint
increments the stream generation. The test did not change the user's selected
Windows output.

## Physical orb

- `PhysicalInteractionHull` owns the 31 px physical core. Glow is render-only.
- Pet-body geometry comes from the main liquid particles before rim, bloom,
  bubbles, shadow or compositor padding.
- Grab uses swept circle/main-liquid overlap, not center distance.
- The carry socket is on the measured contact side and embeds 35% of the orb's
  physical radius.
- The 5 px tolerance remains only in pointer hit testing.
- A 0.00–1.00 glow sweep keeps hull, contact frame, socket and carry anchor
  bit-identical.

## Pet Lab Connect

- `Connect to Pet` attaches to a running normal release or starts the packaged
  sibling `Pet2.exe`; it then becomes `Disconnect`.
- LabControl session protocol v2 uses a 128-bit lowercase hexadecimal token, an
  exact 10 second lease and a 3 second renewal cadence.
- During the lease, the normal release publishes 5 Hz telemetry and accepts the
  closed Lab command set, including all 64 motor programs. No permanent
  `--dev-mode` is enabled.
- Live state is correlated by runtime session ID, Lab session ID and increasing
  telemetry sequence; stale is 2 seconds.
- UI states are `Disconnected`, `Connecting`, `Connected`, `Pet not running`
  and `Version mismatch`.
- Disconnect clears Lab fixtures immediately. A crashed/closed Lab is cleared
  automatically when the lease expires.
- `release-manifest.json` binds Pet2/PetLab version, protocol, size and SHA-256.
  Pet Lab refuses a stale or replaced sibling as `Version mismatch` and does not
  stop an already-running incompatible Pet.

## Numeric verification

- Catalog acceptance: 64/64 programs passed, zero unreachable programs.
- Full motor → body → liquid/PBF acceptance: all 64 programs at 30, 60 and 120
  Hz plus variable frame time; 256 combinations passed in 120.26 seconds.
- Every combination remained finite, stayed inside its mode jerk budget, had no
  one-frame teleport/reversal and triggered no PBF failsafe or whole-solver
  recovery.
- Sixteen repeated body bouts have no immediate or last-two signature repeat;
  same-seed replay is identical.
- `pet_motor`: 25 passed.
- `pet_body` library: 174 passed, 3 explicit long production stress tests
  ignored; the new 64x4 acceptance test passed separately.
- `pet_ecology`: 63 passed across unit, determinism, scenarios, longitudinal
  and persistence suites.
- `pet_audio`: 37 passed; the explicit ignored Windows endpoint smoke was run
  separately and passed.
- `desktop_host`: 32 unit plus 2 integration tests passed.
- `pet2`: 84 passed, 1 fixture-regeneration test ignored.
- `body_lab`: 22 passed.
- Strict Clippy with `-D warnings` passed across the changed runtime, body,
  motor, ecology, audio, host and Lab targets.
- Voice Lab packaging gate: 15/15 scenarios passed.

The packaged release IPC smoke used the normal `Pet2.exe` without dev mode and
passed `OpenSession -> LIVE -> run move.orient_reflex -> CloseSession -> bounded
shutdown`. It observed increasing telemetry sequences and telemetry stopped
after disconnect.

## Release artifacts

Package:

`dist/Pet2-windows-x64.zip`

SHA-256:

- `Pet2.exe`: `211F67F4D69DE29682D7645C48A272387C1F1AC47ECE65A0A9CA47B1C6A20068`
- `PetLab.exe`: `B6B0CCB5659078363939F5FD0FA0C72CE458684C924F55C1F24727202A595BA4`
- `Pet2-windows-x64.zip`: `9FAE9FB9D9770EBF45BD11608230D255A8309E9C81DF23EA885E6F9698CD8013`

The manifest contains the same executable hashes and was verified after copy,
before archive creation.

## How to use

Run `PetLab.exe`, press `F12`, then press `Connect to Pet`. Select any of the 64
motor programs and use `Run selected`. Pet Lab starts the sibling normal release
when necessary; no manual `--dev-mode` or per-action calibration is needed.
