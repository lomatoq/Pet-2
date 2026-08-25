# PET-2 current local build

Published: `2026-08-25T12:23:38+02:00` after integrating the colleague's real
Morph brain as a first-class, same-process part of Pet 2.

Source baseline commit: `e1d06e0f6197e4563a6ba6f657a19a5f9863bad7` plus the
reviewed working-tree implementation. Morph source is pinned to
`3c6e27e3b55e4aff1d6c2c254713cdbd79099715` from
`https://github.com/Thandorcat/morph`. The repository was intentionally not
auto-committed because it contains user-owned work in progress.

- `Pet 2.exe` — launches in the stable `Morphic` behavior mode by default.
- `Pet 2 - Fusion.cmd` — launches the local LifeCore/VITA fusion.
- `Pet 2 - Morph Shadow.cmd` — runs the real Morph network with zero visible authority.
- `Pet 2 - Morph Fusion.cmd` — runs LifeCore, VITA and Morph through one bounded arbiter.
- `Body Lab.exe` — tunes iris HSV, material, face, dynamics, shadow and presets.

Runtime behavior mode: `Win+Alt+B` cycles
`Morphic → Fusion → Morph Shadow → Morph Fusion → Classic`. Morph Fusion caps
the colleague brain's continuous influence at an actual blend weight of `0.30`;
it cannot directly replace discrete pose/locomotion, and local sleep, focus,
drag, retreat and metamorphosis protections zero Morph overrides. The build contains no Node,
sidecar, IPC or network dependency.

Morph topology: `526` neurons, `17,475` synapses, `57` populations. Its learned
state is saved separately and atomically as
`%LOCALAPPDATA%\lomatoq\Pet 2\data\morph-brain.json`, with automatic recovery
from `backups\morph-brain.previous.json`.

Release validation: workspace formatting, `239` full-workspace tests plus `3`
new post-review regressions passed (`3` deliberately ignored long production
replays), and strict all-target/changed-crate Clippy passed. The
optimized 10-second Morph Fusion smoke produced four distinct neural commands
and five command switches; final Morph tick p50 was `0.344 ms`, p95 `0.382 ms`,
and max `0.960 ms` per 50 ms decision tick. Morph Shadow separately confirmed zero
Morph authority. The independent final regression re-review returned `CLEAN`,
including a canonical-exe corrupt-primary/valid-backup recovery reproduction.

Build SHA-256:

- `Pet 2.exe`: `95ED3AF9BBAFA57327F84C075561809AE3CA6EADEA57882D8473A3918323CCE0`
- `Body Lab.exe`: `A87853A223D287F80F7D537C38E3672A2798E9E0B60A660EE52EF13D94CCBF9C`
