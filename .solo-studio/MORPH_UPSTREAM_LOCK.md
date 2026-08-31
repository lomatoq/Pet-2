# Morph upstream lock

- Repository: https://github.com/Thandorcat/morph
- Local checkout: `_external/morph` (pinned source mirror from the colleague)
- Commit: `6aa4e7c871c11ff2fa1619942611d4fb50457e49`
- Commit date: 2026-08-25T17:37:37+02:00
- Verified on: 2026-08-31
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
- `tools/behavior-cycles.mjs`: 16/16
- `tools/simulation-speed.mjs`: 11/11
- exported topology remains 526 neurons / 17,475 synapses / 57 populations;
  the canonical network and neural golden trace are byte-identical to the prior
  lock after ignoring only the provenance commit marker

## Refinement integration disposition

The `6aa4e7c` behavioral refinements are integrated at Pet 2's existing ownership
boundaries rather than copied in as a second companion runtime:

- upstream `ActivitySystem` fields map to the authoritative LifeCore action
  timing plus Ecology `ActivityEpisode` commitment and candidate scores, exposed
  together under causal telemetry `/details/activity`;
- upstream outcome visibility maps to Pet 2's existing short-term/episodic
  outcomes and Ecology expected/results telemetry; learning ownership remains in
  LifeCore, Ecology, and Morph plasticity instead of a duplicate JS store;
- Morph population rates, reward trace, spike count, and bounded classical and
  operant weight envelopes come from a read-only Rust diagnostics accessor;
- sensor timing, stable object identity, and discrete touch semantics remain at
  the existing desktop-host/perception/ecology boundaries, so no raw screen data
  or parallel sensor clock is introduced;
- the live Body Lab monitor presents those layers together and can replay their
  recorded evolution without mutating the organism.
