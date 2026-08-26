# PET-2 current independently accepted local build

Published: 2026-08-25

This directory intentionally remains the last independently reviewed Morph build. The living-desktop habitat candidate is ready for independent review but has not been promoted here, because the habitat implementation plan forbids replacing `builds/current` before that verdict.

Current published executables:

- `Pet 2.exe` — default Morphic desktop PET.
- `Pet 2 - Fusion.cmd` — LifeCore/VITA fusion.
- `Pet 2 - Morph Shadow.cmd` — real Morph network with zero visible authority.
- `Pet 2 - Morph Fusion.cmd` — bounded LifeCore/VITA/Morph arbiter.
- `Body Lab.exe` — procedural body/material/physics authoring lab.

Published SHA-256:

- `Pet 2.exe`: `95ED3AF9BBAFA57327F84C075561809AE3CA6EADEA57882D8473A3918323CCE0`
- `Body Lab.exe`: `A87853A223D287F80F7D537C38E3672A2798E9E0B60A660EE52EF13D94CCBF9C`

## Living-habitat candidate

Candidate package:

```text
..\..\dist\Pet2-windows-x64\
..\..\dist\Pet2-windows-x64.zip
```

Candidate launch commands from the repository root:

```powershell
.\dist\Pet2-windows-x64\Pet2.exe
.\dist\Pet2-windows-x64\BodyLab.exe
.\dist\Pet2-windows-x64\HabitatLab.exe --scenario habitat_story_v1 --ticks 1600 --trace target\habitat.json --screenshot target\habitat.svg
```

Candidate SHA-256:

- `Pet2.exe`: `07020247338812511FC5A8ECB2885D0D6BD3D75B7F1705CF70943A7D4BBA5BBD`
- `BodyLab.exe`: `1C643C5A1460E2240C1939BC83A75A08F82BD678C2EB502A54C1B6636F3F3455`
- `HabitatLab.exe`: `523AD983A874E2B3DE92665BD01323FED1411DDD1E535C235348BA64129EBD5E`
- `Pet2-windows-x64.zip`: `61FC03A159DFB1BA53BBFA418F37280EFFA1280383D49E25F72026F9A6168816`

Promotion requires the independent attacks in `.solo-studio/HABITAT_ACCEPTANCE_REPORT.md`, especially post-fix edge visual capture, production 1× renderer-delta measurement, and native Apple Silicon validation.
