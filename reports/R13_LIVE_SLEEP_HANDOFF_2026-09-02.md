# Pet 2 R13 — sleep handoff superseded (2026-09-02)

This document previously described a failed live sleep attempt and is retained
only as a historical pointer. Its implementation warnings and continuation
steps are superseded by `reports/R13_MOTOR_IMPLEMENTATION.md`.

The identified root cause was fixed: sleep support now comes from the measured
physical pixel gap between the real particle silhouette and desktop bottom,
not from the support constraint commanded by the motor itself.

Current gates:

- approach: gap > 8 px;
- landing: gap <= 8 px;
- contact: absolute gap <= 4 px and normal speed <= 36 px/s;
- support: 300 ms continuous contact dwell;
- revocation: gap > 6 px;
- NREM: only after measured support and loaded settle.

The lower collision bound now uses the real lower metaball extent. NREM closes
the eyes, deforms the lower contact side, and uses slow breathing. All other
programs use family defaults and automated numeric activation probes.

Acceptance artifacts:

- `reports/r13_catalog_acceptance.json`: 64/64 passed with selector
  reachability and exact threshold evidence;
- `reports/r13_deterministic_traces.jsonl`: deterministic sleep chain included;
- optimized build: `target/release/pet2.exe`.

Current manual review path (added 2026-09-03): launch Pet 2 with `--dev-mode`,
open `target/release/body_lab.exe`, press `F12`, then use the open `Controlled
intervention` panel. Its indexed Motor catalog contains all 64 programs and runs
each through the live production particle body; it also shows the numeric 64/64
acceptance result and live program/phase telemetry.

The den capture regression is also superseded. Background displacement now uses
a 60 Hz, maximum-512-pixel ROI around the den, with crop-aware shader UVs and an
ROI-sized DXGI staging resource. It no longer uploads the full overlay at 12 Hz
or copies the full monitor at the restored cadence.

No per-program visual calibration is required. Computer Use is optional only
for subjective appearance review.
