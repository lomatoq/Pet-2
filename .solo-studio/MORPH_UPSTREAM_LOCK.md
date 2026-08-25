# Morph upstream lock

- Repository: https://github.com/Thandorcat/morph
- Local checkout: `_external/morph` (pinned source mirror from the colleague)
- Commit: `3c6e27e3b55e4aff1d6c2c254713cdbd79099715`
- Commit date: 2026-08-25T00:58:15+02:00
- Verified on: 2026-08-25
- Runtime boundary: first-class same-process `morph_brain` Rust component; the JS checkout is the behavioral reference
- License status: colleague-owned project component; **no repository-level LICENSE or COPYING file found yet**

The user identifies Morph as the second part of this project produced by their
colleague. Integration therefore ports the brain into the shared product rather
than treating it as a third-party service. Before public redistribution, the
upstream repository should still receive an explicit project license so this
permission is machine-readable and unambiguous.

## Verified upstream gates

Node 20 needs `--experimental-default-type=module` because the checkout has no
package manifest declaring ESM.

- `tools/acceptance.mjs`: 13/13
- `tools/desktop-adapter.mjs`: 15/15
- `tools/system-health.mjs`: 25/25 across five seeds
- local full-sim benchmark: 0.363 ms per 60 Hz frame, 19.7 MB Node heap
