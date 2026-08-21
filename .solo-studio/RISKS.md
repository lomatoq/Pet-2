# Risk Register — Pet 2

Updated: 2026-08-21

## Active

### R-001 — macOS native behavior cannot be exercised on Windows

- Category: technical
- Probability: high
- Impact: high
- Mitigation: use maintained objc2 bindings, successful Apple Silicon cross-checks,
  native macOS CI, and capability-based failure.
- Fallback: keep the same trait and run without optional active-window geometry.

### R-002 — Transparent surface behavior varies by compositor/adapter

- Category: technical
- Probability: medium
- Impact: medium
- Mitigation: select only reported surface formats/alpha/present modes and render the
  first frame before showing the window.
- Fallback: surface reconfiguration and a reduced rectangular overlay.

### R-003 — Long-running learned behavior needs product playtesting

- Category: design / quality
- Probability: medium
- Impact: high
- Mitigation: deterministic 24-hour simulation, bounded state, fixed seeds, debug
  snapshots, and explicit attention/focus constraints.
- Fallback: tune data coefficients without changing portable schema or platform seams.
