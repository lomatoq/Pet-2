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

- `Pet2.exe`: `CBF8B78168919C0EC426AC5464920A58D8593554E043ECE432FF136FED41D337`
- `BodyLab.exe`: `3316EB79C9D94E5A49D1E355C3C9799AABFB8F6C7585C116BA3E5972FFFD7C25`
- `HabitatLab.exe`: `A999F1EA0B7EAE852674C36F0ABB6852EEC3429424791F6A37AE00F8510439EC`
- `Pet2-windows-x64.zip`: `51694E930468721744F0B330FB81CA6D503F8AB8B94D1EB66ECC42AFA33BF5F7`

Promotion requires the independent attacks in `.solo-studio/HABITAT_ACCEPTANCE_REPORT.md`, especially post-fix edge visual capture, production 1× renderer-delta measurement, and native Apple Silicon validation.
