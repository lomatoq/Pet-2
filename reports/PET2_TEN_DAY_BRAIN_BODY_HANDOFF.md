# PET 2: 10 дзён за гадзіну і сувязь мозг → цела

Дата праверкі: 2026-09-01.

## Кароткі вынік

Для хуткага запуску створаны `config/evolution/ten-day-one-hour.json`: 240 simulated hours, 360 сапраўдных fixed-step interaction episodes, адзін replicate, sleep consolidation, persistent learning і максімум адна eligible metamorphosis. Пустыя прамежкі ідуць праз `calendar_only`, таму не пракручваюцца мільёны пустых neural ticks.

Па ўжо вымераных production-smoke выніках runner робіць 0.157–0.188 episode/s. 360 эпізодаў займаюць прыкладна 32–38 хвілін; з запасам на дыск і фіналізацыю трэба чакаць каля 45 хвілін. Гэта host-specific ацэнка, не hard deadline.

Гэта не падмена навучання: кожны з 360 interaction episodes усё роўна выкарыстоўвае PBF 120 Hz, perception 60 Hz і Life/Morph 20 Hz. Аднак мозг не «думае» на кожным пустым такце паміж эпізодамі. `calendar_only` аналітычна рухае каляндар/homeostasis, але не генеруе фальшывыя user responses або hidden learning.

## Як запусціць

Спачатку перазбудаваць release binary, затым з кораня repository:

```powershell
.\target\release\pet2.exe `
  --evolution-config .\config\evolution\ten-day-one-hour.json `
  --evolution-report .\artifacts\ten-day-life\report.json `
  --evolution-progress .\artifacts\ten-day-life\progress.json `
  --no-audio-output
```

Бяспечны default — `persistence: fork`. Ён не перапісвае жывога PET: accepted state будзе ў падкаталогу `evolution-runs`, а дакладны шлях з'явіцца ў `report.json -> persisted_state`.

Каб зрабіць вынік жывым PET, спачатку праверыць у report:

- `status == "completed"`;
- `invariants.passed == true`;
- `eligibility.eligible == true`;
- `performance.wall_seconds <= 3600`;
- `final_generation <= initial_generation + 1`;
- `persisted_state` існуе і загружаецца.

Толькі пасля гэтага выкарыстоўваць штатны Body Lab promotion flow, які спыняе live process, прасоўвае bundle, перазапускае PET і чакае loaded-state ACK. Не запускаць ручны `--evolution-persist save-final` паралельна з жывым PET.

## Чаму `exact` не падыходзіць для гадзіны

`advance_quiet(Exact)` робіць perception tick на 60 Hz і Life/Morph tick на 20 Hz ва ўсіх пустых інтэрвалах. Для 240 гадзін гэта 17.28 мільёна Life/Morph ticks. Кароткі 36-second exact smoke ўжо займаў каля 10–13 секунд; лінейная экстрапаляцыя да 10 дзён дае шмат дзясяткаў гадзін.

`calendar_only` выкарыстоўваецца толькі для пустых прамежкаў. Фізічныя curricula не спрошчаныя.

## Што цяпер сапраўды залежыць ад мозгу

Частка сувязі ўжо ёсць. `VisualMindInput` атрымлівае affect/mood/attention і змяняе:

- pulse і pulse amplitude;
- shell opacity, inner density і translucency;
- flow strength і flow speed;
- core glow і halo;
- iris activity і eye wetness;
- decorative droplet energy, spread і cohesion.

`ExpressionState` ужо звязвае affect з pupil, brows, mouth, cheek/body glow. `InteractionBodyActuation` ужо даходзіць да PBF і bounded-мяняе compliance, cohesion, local pulse, lean, recoil, resistance і cooperation.

## Чаму гэта амаль не відаць

1. Актыўны profile `Black copy r11` мае `material.override_genome_colors=true`. Таму ўсе `BodyGenome.primary_color_hsv`, `secondary_color_hsv` і `glow_color_hsv` ігнаруюцца shader-ам.
2. Authored palette амаль чорная, `bloom_strength=0.05`; невялікія змены emission/physiology маюць слабы кантраст.
3. `body_length`, `body_width`, `body_roundness`, `softness`, `visual_mass` і `inertia` не змяняюцца падчас звычайнага навучання.
4. `viscosity`, `surface_tension` і базавы `density_compliance` фіксаваныя ў `LiquidTuningProfile`. Зараз ёсць толькі малыя transient interaction deltas.
5. Metamorphosis цяпер змяняе абмежаваны набор traits. Яна не муціруе асноўныя памеры корпуса, roundness, softness або PBF liquid constants.
6. `BrainGenome.learning_rate`, `plastic_fraction` і падобныя — механіка навучання, а не live neural state. Іх прамая прывязка да колеру была б дэкаратыўнай і некаузальнай.

## JSON для рэалізацыі

- `config/embodiment/active-liquid-profile-r11.json` — дакладная копія ўсіх 209 leaf-параметраў актыўнага liquid profile.
- `config/embodiment/brain-body-parameter-catalog.json` — усе cognitive/state/body/voice targets і апісанні ўсіх liquid fields. Аўтаматычная праверка паказвае поўнае пакрыццё: profile 5/5, analytic 8/8, PBF 71/71, interaction 27/27, droplets 16/16, material 58/58, face 22/22, compositor 7/7.
- `config/embodiment/brain-body-coupling-proposal.json` — 13 fast causal mappings, формулы, clamps, rise/fall smoothing, slow-development evidence, per-generation caps, забароненыя runtime channels, файлы і acceptance tests.

Галоўная рэкамендацыя: зрабіць асобны `BodyPhenotypeDirector`. Ён атрымлівае immutable `EmbodimentSourceFrame` з Life/VITA/Morph і выдае `FastPhenotypeActuation`. `pet_body` прымяняе толькі bounded runtime multipliers да authored base. Persistent Genome змяняецца асобна пасля sleep/evolution gate.

Не даваць renderer доступ да mutable cognition і не пісаць fast emotion values назад у Genome.

## Абавязковая двуххуткасная логіка

### Fast, 0.1–3 секунды

Аrousal, stress, fatigue, confidence, attention, physical load і neural population readouts мяняюць толькі:

- breathing shape у межах каля ±5%;
- compliance/viscosity/surface-tension multipliers у межах прыблізна 0.75–1.30;
- flow, glow, opacity, translucency;
- hue не больш за 8° ад identity base;
- gaze, face, lean, recoil і asymmetric follow-through.

Усе каналы маюць асобны rise/fall smoothing і вяртаюцца да identity base.

### Slow, дні і metamorphosis

Persistent morphology выкарыстоўвае назапашаныя evidence channels: flight mastery, social security, exploration mastery, rest adaptation, physical resilience і communication mastery. Адна падзея нічога не муціруе. Змена адбываецца толькі пасля eligibility, запісвае поўны before/after Genome і праходзіць invariants.

Прапанаваныя per-generation caps ужо знаходзяцца ў coupling JSON: памеры да 2–3%, softness/inertia да 0.04, hue 2–3°, saturation 0.03, bioluminescence 0.04.

## Што ніколі не мяняць ад эмоцыі

Не мапіць live brain state ў:

- PBF frequency, particle count, substeps або solver iterations;
- numerical XSPH і topology budgets;
- maximum speed і maximum detached mass;
- render scale/exposure;
- voice maximum loudness;
- identity/pattern seeds;
- learning rate/plasticity caps.

Гэтыя параметры ўплываюць на стабільнасць, бяспеку або identity і павінны заставацца authoritative.

## Fix адарванай кроплі

Прычына была ў coordinate ownership. Рэальныя detached PBF particles захоўвалі local coordinates, але renderer дадаваў ім body-root translation. У выніку кропля атрымлівала амаль увесь рух асноўнага цела і выглядала жорстка прывязанай.

Fix counter-translates толькі `component_id != main_component` па `presentation_displacement` і адначасова рухае ўсе authoritative position buffers і recovery proxy. Main mass працягвае рухацца разам з root. Да fix regression бачыў world step detached mass `(0.11998, -0.035)` пры root delta `(0.12, -0.035)`; пасля fix detached world step застаецца каля нуля.

Рызыка: component membership ідзе з папярэдняга fixed tick, таму ў момант самога split магчыма адна затрымка да 1/120 секунды.

## Definition of done для мадэлі-рэалізатара

1. У Body Lab ёсць live табліца `source -> raw target -> filtered -> clamped -> effective`.
2. Arousal-only intervention павялічвае pulse/flow/emission, але не stress cohesion.
3. Stress-only intervention павялічвае cohesion/compression і памяншае saturation; hue не выходзіць за 8°.
4. Fatigue-only intervention павялічвае damping/viscosity і памяншае flow/apparent size.
5. Genome palette застаецца пазнавальнай нават у cinematic authored material.
6. Адзін high-stress event не змяняе persistent body.
7. Два аднолькавыя 10-day runs даюць адзін final Genome hash і phenotype trace.
8. Ніводны fast channel не змяняе structural/safety параметры.
9. Detached real component захоўвае desktop inertia пры руху body root.
10. Усе existing body, evolution, persistence, fmt і clippy checks праходзяць.
