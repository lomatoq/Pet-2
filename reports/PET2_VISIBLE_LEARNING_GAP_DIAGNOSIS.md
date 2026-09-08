# PET 2 — чаму паскоранае навучанне, гук і эмоцыі не даюць бачнай розніцы

Дата дыягностыкі: 2026-09-01
Правераны commit: `644a900a5de0acdc76b20ab2a3a45ccd2779a89a`
Branch: `codex/pet2-embodied-pointer-language`

## Кароткая выснова

Назіранне карыстальніка «пакуль 0 розніцы» абгрунтавана. Большая частка новай працы існуе як causal backend, telemetry, headless validation і safety-механізмы, але карыстальніцкі end-to-end шлях не замкнуты.

Ёсць чатыры асобныя прычыны:

1. **Accelerated Learning фактычна не запускаўся.** Body Lab выбіраў састарэлы `builds/current/Pet 2.exe`, які адмаўляўся стартаваць, пакуль жывое акно Pet ужо працуе.
2. **Default-прэсет і не павінен змяняць жывога Pet.** `One Day Diagnostic` — `Dry run`, evolution `Off`, `maximum_generations = 0`; ён толькі правярае сістэму.
3. **Паскораны runner не навучае новаму гучанню.** Ён запускаецца з `--no-audio`, усе vocal requests у headless адмяняюцца, а interaction bandit змяняе толькі амплітуду целавага адказу прыкладна на ±6%, не voice/timbre/motif style.
4. **Фізіка і палёт амаль не маюць асобнага бачнага emotional presentation contract.** Physical appraisal захоўваецца ў LifeCore/telemetry і каротка ўплывае на твар/pose; палёт дае стрыманы tilt/face offset, але не перадае strain/slosh/flight state у выразную эмоцыю з гарантаванай perceptual salience.

Гэта не адна памылка ў shader або audio device. Гэта разрыў ланцужка:

`Start → правільны runner → progress → learned artifact → safe promotion → live reload → выразны body/voice delta`.

## 1. P0: чаму кнопка Start не запускае навучанне

### Пацверджаныя runtime-факты

У `%LOCALAPPDATA%\lomatoq\Pet 2\data\evolution-runs` знойдзена:

- 16 каталогаў `body-lab-*` за 25 секунд;
- 16/16 `runner.log` маюць толькі `Pet 2 is already running; refusing to create a second desktop organism`;
- 0/16 маюць `report.json`.

Такім чынам, гэта не «доўга лічыць». Працэс амаль адразу завяршаўся без запуску `EvolutionRunner`.

### Root cause

- `tools/body_lab/src/main.rs:5952-5999` шукае canonical-файл `Pet 2.exe` і ставіць `builds/current/Pet 2.exe` вышэй за актуальны release/dev executable.
- `scripts/package_windows.ps1:19-20` пакуе іншыя імёны: `Pet2.exe` і `BodyLab.exe`.
- У гэтым workspace ёсць стары `builds/current/Pet 2.exe`, таму ён перамагае ў resolution нават пры запуску свежага Body Lab.
- Актуальны source у `app/src/main.rs:117-130` накіроўвае evolution mode да single-instance guard. Значыць адмова па single-instance прыходзіць ад не таго, састарэлага binary.

### Чаму UI не тлумачыць праблему

`AcceleratedLearningState::poll` у `tools/body_lab/src/main.rs:1792-1826` чытае толькі фінальны `report.json`. Калі яго няма, UI прапануе ўручную адкрыць `runner.log`, але не паказвае яго error-тэкст у панэлі. Пасля хуткага exit кнопка `Start` зноў становіцца актыўнай, таму карыстальнік натуральна націскае яе паўторна.

## 2. Нават пасля launch fix default не дасць бачнай змены

`One Day Diagnostic` па змаўчанні задае ў `tools/body_lab/src/main.rs:1633-1643`:

- 24 simulated hours;
- 48 episodes × 4 replicates;
- `EvolutionPolicy::Off`;
- `maximum_generations = 0`;
- `EvolutionPersistence::DryRun`.

Наступствы:

- `DryRun` нічога не захоўвае (`app/src/evolution_runner.rs:1105-1113`).
- Metamorphosis запускаецца толькі пры eligibility, policy `EligibleMaxOne` і `maximum_generations > 0` (`app/src/evolution_runner.rs:552-568`).
- Eligibility патрабуе мінімум 120 interaction episodes (`app/src/evolution_runner.rs:1001-1034`), а default мае 48.
- Нават паспяховы default-run мае намер быць regression/diagnostic workload, а не змяняць асобу, genome або жывое акно.

На гэтай машыне раней правераны поўны `One Day Diagnostic` заняў каля **44,7 хвіліны**. UI паказвае толькі wall-time і тэкст, што report будзе ў канцы (`tools/body_lab/src/main.rs:2451-2458`). Сам runner запісвае report адзін раз пасля ўсіх replicates (`app/src/evolution_runner.rs:258-344`). Episode/replicate progress IPC або прамежкавага `progress.json` няма, хоць зыходная спецыфікацыя патрабуе simulated time і episode progress.

## 3. Promotion зараз небяспечны для бачнага выніку

Жывы Pet загружае state пры startup (`app/src/main.rs:814-850`) і не мае hot-reload/IPC для прыняцця прасунутага fork. Пры гэтым ён захоўвае свой стары in-memory state кожныя 30 секунд і пры exit (`app/src/main.rs:2734-2745`, `3281-3311`).

Таму нават пасля паспяховага `Promote validated fork`:

- адкрыты Pet не пачне выкарыстоўваць новы state;
- наступны periodic save жывога працэсу можа перапісаць прасунуты state старой in-memory копіяй;
- `Save final` у той жа live data-dir мае такую ж гонку.

UI паведамляе, што файлы прасунуты, але няма `reload acknowledged`, параўнання active state hash або блакіроўкі live writer. Гэта яшчэ адна прычына, чаму карыстальнік можа ўбачыць нуль змен.

## 4. Чаму гукі «ўвогуле не памяняліся»

### Што ўжо ёсць

У live path ёсць новыя `VocalTrigger`: `SoftTouch`, `PhysicalStartle`, `RhythmEcho`, `CalmBoundary`, detach/remerge і іншыя. Physics ужо можа модуляваць pitch, tempo і stress у `crates/lifecore/src/lib.rs:959-1046`.

### Чаго няма ў accelerated learning

- Body Lab заўсёды дадае runner-у `--no-audio` (`tools/body_lab/src/main.rs:1758-1771`).
- Headless runner стварае vocal request, але адразу выклікае `cancel_vocal_request` (`app/src/evolution_runner.rs:733-773`).
- Пасля cancel не адбываецца `confirm_vocal_request_heard`, таму vocal delivery/history learning не каміціцца (`crates/lifecore/src/lib.rs:640-701`).
- Interaction bandit вывучае толькі адзін `variant` і ў выніку маштабуе expression/body response на `0.94`, `1.00` або `1.06` (`crates/lifecore/src/interaction.rs:1230-1238`). Ён не выбірае voice family, timbre, rhythm signature або выразна іншую prosody.
- `One Day Diagnostic` не мяняе VoiceGenome, бо evolution выключана.
- Live input дадаткова пакінуў legacy `VocalTrigger::Touch` адразу пры `petting_started` (`app/src/main.rs:1931-1937`, `4380-4410`). Ён можа заняць першы vocal slot да больш спецыфічнага фізічнага trigger і суб'ектыўна гучыць як стары touch-response.

Такім чынам, код дадаў новыя ўмовы выбару і невялікую фізічную модуляцыю існуючага procedural repertoire, але не стварыў навучаемы і добра адрозны «новы гук пасля навучання».

## 5. Чаму не бачныя эмоцыі пра фізічныя ўласцівасці і палёт

### Пацверджана ў causal core

- `InteractionAppraisal` улічвае strain, deformation, pressure, detached mass і recovery confidence (`crates/lifecore/src/interaction.rs:951-1033`).
- Ён змяняе persistent affect вельмі асцярожна: valence на 5% appraisal, stress да 8%, arousal праз bounded max (`crates/lifecore/src/lib.rs:427-438`).
- Interaction plan сапраўды задае eye/brow/mouth/pose/body actuation (`crates/lifecore/src/interaction.rs:1037-1240`), а VITA прымяняе яго толькі на кароткай фазе `Responding` (`app/src/vita_runtime.rs:521-620`).
- Палёт у body runtime дае стрыманы face offset і roll ад velocity (`crates/pet_body/src/embodiment.rs:576-670`), а PBF мае flight stretch/inertia.

### Прабел presentation layer

- `VisualMindInput` у `app/src/main.rs:4517-4555` перадае агульныя affect/attention values, але не перадае `maximum_strain`, `slosh_energy`, topology state, flight acceleration або braking як выразны emotional context.
- Flight cue — гэта фізічны нахіл/зрух, не асобная эмоцыя і не multimodal phrase.
- Навучаны variant змяняе толькі амплітуду на ±6%; форма выразу, hold-time і sound style не навучаюцца. Такая розніца лёгка ніжэй за perceptual threshold.
- Няма acceptance-тэсту «чалавек без telemetry адрознівае soft touch / effort / boundary / playful flight па руху і гуку» і няма Before/After A/B у Body Lab.

Апошняя live telemetry пацвярджае, што backend нешта распазнаў: быў committed `fragment_separation_attempt`, створаны appraisal і `calm_boundary` response plan. Гэта доказ працы perception/appraisal, але не доказ бачнай рэакцыі. Менавіта тут backend-success памылкова быў прыняты за product-success.

## 6. Мінімальны хуткі план рэалізацыі

### P0 — зрабіць запуск праўдзівым і назіраемым

1. Выправіць executable resolution:
   - спачатку exact sibling `Pet2.exe`/`pet2.exe`;
   - затым matching target binary;
   - `builds/current` толькі як яўны fallback;
   - у UI паказваць поўны selected executable path, build SHA/version і PID.
2. Дадаць unit/integration test для рэальнага package layout `BodyLab.exe + Pet2.exe` і асобна dev layout.
3. На exit без report аўтаматычна паказваць tail `runner.log` у чырвоным error box. Не пакідаць толькі «inspect runner.log».
4. Дадаць atomic `progress.json` або bounded stdout protocol: stage, replicate, episode, simulated time, learning update count, ETA, last invariant failure. Абнаўляць мінімум пасля кожнага episode.

### P0 — бяспечнае прыняцце навучанага стану

1. Не дазваляць `Save final`/`Promote` пры жывым Pet без handshake.
2. Рэалізаваць адзін з двух простых кантрактаў:
   - `pause saves → import promoted bundle → validate hash → hot reload → ACK → resume`; або
   - закрыць Pet, promote, затым перазапусціць яго і атрымаць `loaded_state_hash` ACK.
3. Паказваць у UI `before hash → fork hash → active loaded hash`.
4. Вялікім тэкстам пазначыць `Dry run: жывога Pet не зменіць`.

### P1 — зрабіць вынік хутка бачным, не выдаючы demo за доўгатэрміновае навучанне

1. Дадаць `Visible Interaction Smoke` на 1–3 хвіліны: 13 fixtures, адзін seed, no evolution, але live preview адказаў і A/B baseline. Ён правярае presentation, а не імітуе сем дзён жыцця.
2. Пакінуць `One Day Diagnostic` доўгім release gate і не выбіраць яго default для ручной праверкі.
3. Рэалізаваць або выключыць checkbox `Include explicit captures`: цяпер field існуе ў config/UI, але runner яго нідзе не чытае.

### P1 — выразны physics-to-expression/audio contract

1. Увесці typed `PhysicalExpressionContext` з strain, slosh, topology/remerge, flight speed, acceleration/braking і controllability.
2. Зрабіць 4 добра адрозныя, але bounded phrase-family: `gentle`, `effort`, `boundary`, `play/flight`.
3. Для цела/твару задаць мінімальную salience і hold 1–2 секунды: silhouette/lean, eye aperture/scale, brow/mouth tension, gaze target. Не толькі ±6% amplitude.
4. Для гуку навучаць bounded style choice/timing вакол identity VoiceGenome; не loudness ceiling. Каардынаваць legacy `Touch`, каб ён не забіраў vocal slot у gesture-specific response.
5. Дадаць offline A/B render: аднолькавы motif да/пасля, з вымярэннем pitch contour, tempo, spectral/noise mix і envelope. Mute/quiet заўсёды маюць прыярытэт.

## 7. Acceptance criteria

- З пакетнага `BodyLab.exe` Start запускае суседні актуальны `Pet2.exe`, нават калі desktop Pet адкрыты; progress з'яўляецца не пазней чым праз 1 секунду.
- У выпадку launch failure канкрэтны stderr бачны ў панэлі без адкрыцця файла.
- `Dry run` не мяняе active hash і прама так падпісаны.
- Promotion не можа быць перапісаны старым live process; пасля restart/reload `active loaded hash == promoted hash`.
- Visible smoke паказвае кожную gesture family целам; tester без telemetry правільна адрознівае хаця б `gentle`, `effort`, `boundary`, `play/flight`.
- Audio A/B мае вымяральную розніцу ў contour/tempo/timbre, але захоўвае identity і loudness limit.
- `Include explicit captures` альбо мае сапраўдны ingestion path з consent/provenance, альбо недаступны ў UI.
- End-to-end test пакрывае: `Start → progress → report → fork → promote → reload ACK → visible + audible delta`.

## 8. Задача для планавальніка/наступнага выканаўцы

Падрыхтаваць кароткі implementation plan без перапісвання архітэктуры. Спачатку закрыць P0 launch/progress/promotion race, затым адзін vertical slice `SharpFlick + CalmBoundary + playful flight` з выразным body і audio A/B. Для кожнага кроку назваць файлы/функцыі, тэсты, migration/rollback і крытэрый бачнай гатоўнасці. Не лічыць telemetry або green unit tests доказам UX-гатоўнасці без end-to-end perceptual check.
