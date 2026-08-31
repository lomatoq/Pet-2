# Pet 2 — карта мозга, падсістэм і план fusion

Updated: 2026-08-31
Status: Morph gates 1–4 implemented; bounded same-process fusion under live QA
Scope: тое, што ўжо ёсць у рэпазіторыі, тое, што было запланавана, і бяспечная інтэграцыя з інфраструктурай калегі без страты нашага візуалу.

## Вердыкт

Morph — другая частка праекта ад калегі, а не знешні сэрвіс. Яго brain-код трэба зліць у прадукт, але renderer, shader-параметры, audio callback і particle physics яму ўсё роўна не належаць.

Рэкамендаваны шлях — **hybrid brain behind a versioned port**:

1. наш desktop host, privacy reduction, цела, face/motion director, матэрыял, renderer, procedural voice і portable save-envelope застаюцца аўтарытэтнымі;
2. цяперашнія `LifeCore + VitaMind` спачатку абгортваюцца ў `NativeBrain` без змены паводзін;
3. калегаў Morph падключаецца як другі first-class brain component праз вузкі `ObservationFrame -> BehaviorFrame` кантракт;
4. спачатку яна працуе ў shadow mode, потым у suggestion mode, і толькі пасля параўнальных праверак можа стаць аўтарытэтнай для high-level behavior;
5. local embodied kernel заўсёды захоўвае safety, focus mode, deadline handling і offline fallback.

Так мы можам замяняць або камбінаваць мозг, не перамалёўваючы істоту і не прывязваючы яе характар да чужой інфраструктуры.

## Рэалізаваны Morph slice (2026-08-25)

- Падцягнуты рэальны [Thandorcat/morph](https://github.com/Thandorcat/morph), commit `6aa4e7c871c11ff2fa1619942611d4fb50457e49`.
- Дакладная тапалогія (`526` нейронаў, `17,475` сінапсаў, `57` папуляцый), delay-каналы, Tsodyks–Markram depression, LIF dynamics, readout і plasticity-індэксы перанесены ў workspace crate `morph_brain`.
- JS golden trace параўноўвае rates, voltage, adaptation і STD з Rust-портам; максімальны дапуск `8e-4`.
- `Morph Shadow` дае нулявы ўплыў; `Morph Fusion` мае bounded authority `≤0.30` у тым самым `FusionArbiter`.
- Shared `LifeCore::Drives` — адзін writer для homeostasis. Morph не дублюе патрэбы; ён вучыць KC→MBON і operant KC→command вагі і захоўвае іх у `morph-brain.json`.
- Final post-review release measurement: p50 `0.344 ms`, p95 `0.382 ms`, max `0.960 ms` на адзін 50 ms brain tick; runtime не мае Node/IPC/network.

## Што нельга зламаць

- Адна і тая ж істота захоўвае genome, цела, голас, памяць і вывучаны стан паміж Windows і macOS.
- Візуальны вынік застаецца нашым: сілуэт, вочы, твар, squash/stretch, liquid dynamics, droplets, material, lighting, compose і overlay behavior.
- Runtime застаецца offline-first. Калі калегава інфраструктура патрабуе абавязковы remote service, гэта ўжо змена product promise, а не тэхнічная дэталь.
- Raw pixels, screenshots, typed text, key codes, microphone recordings, accessibility labels, clipboard, native handles і absolute paths не ўваходзяць у мозг або save.
- Адзін важны стан мае аднаго writer-а. Нельга пакінуць два незалежныя action arbiters, якія па чарзе перапісваюць адзін intent.
- Experimental provider заўсёды мае назіральны failure mode, timeout і лакальны fallback.

## Фактычная runtime-карта

```text
Windows / macOS / fallback host
  ├─ cursor, idle, app category, window geometry, pointer events
  ├─ desktop background capture
  └─ optional visual feature sampler
            │
            ▼
SensorNormalizer @ 60 Hz ───────────────► SensorFrame
            │                                  │
            │                                  ├──────────────┐
            ▼                                  ▼              ▼
PerceptionRuntime                         LifeCore @ 20 Hz   BodyFeedback
  ├─ pointer gestures                       ├─ 8 drives         ▲
  ├─ typing/click/scroll rhythm             ├─ affect           │
  ├─ window ecology                         ├─ 64-neuron CTRNN   │
  └─ scalar visual features                 ├─ action scoring    │
            │                               ├─ habits/memory     │
            ▼                               ├─ voice motifs      │
     VitaPerceptFrame                       └─ development       │
            │                                      │            │
            └──────────────► VitaMind @ 20 Hz ◄────┘            │
                              ├─ attention                      │
                              ├─ appraisal                      │
                              ├─ emotion + mood                 │
                              ├─ predictive self-model          │
                              ├─ favorite places                │
                              └─ influence policy               │
                                      │                         │
                         base intent + VITA overrides           │
                                      ▼                         │
                              Presentation mapping              │
                    BodyIntent + VisualMindInput + Voice request│
                            │                     │              │
                            ▼                     ▼              │
              ProceduralBody / liquid       pet_audio           │
              physics @ 30–120 Hz           callback            │
                    │       ▲                   │                │
                    │       └───────────────────┘                │
                    │        lock-free mouth/purr feedback       │
                    └────────────────────────────────────────────┘
                                      │
                                      ▼
                     RenderParameters -> wgpu/WGSL -> overlay

Persistence:
PortablePetState v1 = LifeSnapshot + optional VitaState + normalized position
Liquid tuning = separate versioned profile; transient body particles are regenerated.
```

### Вядомы разрыў у гэтай карце

`WindowsBackend::poll_visual_features()` і `PerceptionRuntime::set_visual_features()` існуюць, але app runtime іх не злучае. Таму scalar visual perception цяпер фактычна dormant, хоць project memory апісвае яго як materialized. Background capture для renderer працуе асобна і не выпраўляе гэты разрыў.

## Інвентар падсістэм

| Падсістэма | Фактычны ўладальнік | Стан | Роля ў fusion |
|---|---|---|---|
| Persistent identity і genome | `lifecore::Genome` | Ёсць | Вынесці ў агульны organism envelope; не аддаваць provider-у як яго прыватны дубль |
| Homeostasis | `Drives`: sleep, social, play, curiosity, comfort, safety, autonomy, novelty | Ёсць, bounded і tested | Альбо цалкам `NativeBrain`, альбо адзін абраны provider; не два writer-ы |
| Базавы affect | valence, arousal, stress, confidence, attachment, frustration | Ёсць | Частка semantic mind-state, якую presentation чытае, але не піша |
| Microbrain | deterministic sparse 64-neuron CTRNN, 32 inputs, reward plasticity | Ёсць | Native fallback і baseline для параўнання з калегавай сістэмай |
| Action arbitration | 24 actions, conditions, cooldowns, softmax, attention cost | Ёсць | Цяпер аўтарытэтны ў `LifeCore`; перад fusion патрэбны адзін канчатковы arbiter |
| Habits і reward learning | contextual bandit + attention feedback | Ёсць | Provider-owned state behind snapshot port |
| Memory | short-term, episodic, habits, user model | Ёсць | Provider-owned; міграваць versioned, не перакладаць lossily ў UI state |
| Voice vocabulary | genome-derived motifs, reward, mutation during sleep | Ёсць | Выбар сэнсу/акта можа ісці з мозга; waveform, timbre і mouth sync застаюцца нашымі |
| Development | lifetime stats і bounded metamorphosis | Ёсць | Genome/identity transaction; патрабуе atomic coordination з body regeneration |
| VITA attention | attention target, commitment, habituation | Ёсць | Кандыдат на аб'яднанне з калегавай attention model |
| VITA appraisal/emotions/mood | novelty, threat, agency, emotion episodes, slow mood | Ёсць | Semantic output у presentation; не shader controls |
| Predictive self-model | action forward models, agency, uncertainty, calibration urge | Ёсць | Павінен атрымліваць рэальны `BodyFeedback`; карысны local fast kernel нават пры external cognition |
| Influence policy | 12 social strategies, cooldown, annoyance risk, learning | Ёсць | Абавязкова адзін writer разам з attention/action policy |
| Favorite places | app category + normalized position + comfort/play/safety | Ёсць | Portable semantic memory; transient window IDs не захоўваць |
| Perception | gestures, input rhythm, window ecology, salience | Часткова злучана | Захаваць наш privacy firewall; калега атрымлівае толькі normalized summary |
| Visual feature sampling | Windows scalar sampler | Рэалізаваны backend, не злучаны з VITA | Спачатку wired async + bounded; macOS застаецца capability-based `None` |
| Locomotion/body feedback | `BodySimulation` | Ёсць | Наш fast embodied layer; provider дае target/intent, не піша position |
| Embodiment | gaze, lids, brows, mouth, lag, breathing, compression | Ёсць | **Захаваць цалкам** як visual behavior director |
| Liquid body | PBF/XPBD, bonds, components, droplets, re-merge, face frame | WIP, 90 body tests праходзяць | **Захаваць цалкам**; brain бачыць толькі semantic diagnostics/body feedback |
| Material/renderer | analytic fallback + particle path, current-safe + cinematic, HDR/compose/debug views | WIP, вялікі uncommitted visual slice | **Захаваць цалкам** і зафіксаваць reference captures перад fusion |
| Procedural audio | cpal synth + lock-free visual feedback | Ёсць | **Захаваць**; provider не генеруе samples у realtime path |
| Desktop host | overlay, coordinates, capabilities, storage path, background capture | Windows працуе; macOS патрабуе hands-on QA | **Захаваць** як platform boundary |
| Persistence | atomic JSON + backup + v1 fixture | Ёсць | Пашырыць да provider envelope з міграцыяй v1; не серыялізаваць runtime body |
| Body Lab | tuning, scenarios, debug views, hot reload/ack | Ёсць | Галоўны visual acceptance harness |
| Config/profile | JSON і `UserProfile` API | Ёсць як schema, але app іх не ўжывае | Падключыць пасля brain seam; не дубліраваць у калегавай config system |
| Diagnostics | debug JSONL, headless smoke, unit/cross-platform tests | Ёсць | Дадаць brain decision trace, deadline, fallback і provider health |

## Дзе цяпер неадназначнае валоданне

Сёння `LifeCore` выбірае action і стварае `BodyIntent`, пасля чаго `VitaMind` можа перапісаць pose, locomotion, target і interaction. Гэта працавальны vertical-slice bridge, але дрэнная fusion-мяжа: два policy layers лічаць сябе апошнім словам.

Перад падключэннем калегі трэба атрымаць адзін pipeline:

```text
perception/state
  -> one behavioral decision
  -> local safety/focus validator
  -> presentation director
  -> body/audio/render
```

`VitaMind` можа застацца часткай `NativeBrain`, але яго output павінен быць скампанаваны ў адзін `BehaviorFrame` унутры backend-а, а не перапісваць runtime intent пасля іншага backend-а.

## Што ўжо планавалася

### VITA / perception

1. Давесці Windows visual sampler да app/VITA праз async worker.
2. Дадаць best-effort UI/control geometry без labels або text.
3. Дадаць opt-in microphone-derived RMS, voice activity і prosody без raw audio retention.
4. Праверыць overlay, Metal/CoreAudio і visual behavior на рэальных Windows і Apple Silicon.
5. Дадаць optional camera/semantic providers, не робячы `LifeCore` залежным ад іх.

### Visual body

Research-план прадугледжваў filtered surface normals, calibrated transmittance/wet coat, material-space internal lights, source-specific bloom, brain-to-material smoothing, external bubbles і production transparency fallback. Значная частка гэтага ўжо materialized у бягучым working tree: liquid passes, cinematic material controls, internal orbs/soul glow, caustics, physiology mapping, droplets і debug views існуюць. Але гэты slice яшчэ нельга лічыць locked, пакуль няма:

- representative target-size captures на black/white/gray/checker/real desktop;
- параўнання Current Safe vs Cinematic;
- GPU capture на мэтавым integrated GPU;
- real Windows і Apple Silicon runtime QA;
- зафіксаванага approved tuning profile/revision.

### Product / ship

Яшчэ не закрыты first-session onboarding, accessibility/reduced motion, settings wiring, target performance/memory capture, install/update/uninstall, macOS hands-on QA, licenses/provenance pass і release operations.

## Варыянты fusion

| Варыянт | Плюс | Крытычны мінус | Вердыкт |
|---|---|---|---|
| Зліць рэпазіторыі і даць калегавай сістэме кіраваць усім | Хутка выглядае як “адна сістэма” | Ламае state ownership, visual boundary, rollback і privacy audit | Адхіліць |
| Знешні brain service адразу як адзіны аўтарытэт | Можна хутка выкарыстаць моцную infra | Latency/offline/determinism і data migration становяцца release blockers | Толькі пасля shadow gate |
| Versioned brain port + Native/Colleague adapters + local embodied kernel | Захоўвае visual identity, дае A/B, fallback і паступовую міграцыю | Патрэбны невялікі protocol/adapter слой | **Рэкамендавана** |

## Мэтавая архітэктура

```text
                     ┌──────────────────────────────┐
normalized signals ─►│ pet_protocol::ObservationV1 │
                     └──────────────┬───────────────┘
                                    ▼
                         ┌─────────────────────┐
                         │    BrainRouter      │
                         │ native / shadow /   │
                         │ assist / colleague  │
                         └───────┬───────┬─────┘
                                 │       └── timeout/invalid -> NativeBrain
                                 ▼
                     pet_protocol::BehaviorV1
                                 │
                       safety + focus validator
                                 │
                                 ▼
                        PresentationDirector
             ┌───────────────────┼────────────────────┐
             ▼                   ▼                    ▼
         BodyIntent        VisualMindInput       VoiceAct
             │                   │                    │
             └──────────── our body/material/audio/render ──────► frame
```

### `BrainBackend` port

Мінімальны provider contract:

```rust
trait BrainBackend {
    fn provider_id(&self) -> &'static str;
    fn step(&mut self, observation: &ObservationV1) -> BrainStepResult<BehaviorV1>;
    fn feedback(&mut self, event: &FeedbackV1);
    fn snapshot(&self) -> BrainStateEnvelope;
    fn reset_learning(&mut self, identity: &OrganismIdentity);
    fn note_metamorphosis(&mut self, identity: &OrganismIdentity);
}
```

Гэта канцэптуальны кантракт, не патрабаванне да мовы калегі. In-process Rust adapter, IPC або local service павінны выглядаць аднолькава для app runtime.

### `ObservationV1`

Павінен утрымліваць толькі:

- monotonic tick/time і capability mask;
- normalized `SensorFrame`-падобныя сігналы;
- gesture/rhythm/window/visual scalar percepts;
- semantic `BodyFeedback`;
- user feedback events;
- focus/privacy/influence mode;
- bounded recent context, калі ён сапраўды патрэбны provider-у.

Не павінны пераходзіць мяжу:

- raw pixels/screenshots і background texture;
- typed content/key identity;
- raw microphone/camera frames;
- window titles/accessibility text;
- native handles, process IDs, device IDs і paths;
- PBF particles, shader buffers або renderer internals.

### `BehaviorV1`

Provider можа задаваць:

- semantic action і яго TTL/confidence;
- locomotion mode, normalized target, facing і interaction target;
- gaze target/direct-viewer intent;
- semantic pose/emotion;
- affect vector: valence, arousal, stress, confidence, attachment, frustration;
- high-level voice act: silent/chirp/purr/rhythm, intensity і timing intent;
- explanation/debug reason codes.

Provider не можа задаваць:

- particle positions, bonds, viscosity або solver iterations;
- eye geometry, blink curve, squash/stretch curve або microsaccade noise;
- material absorption, rim, caustics, bloom, color grading або exposure;
- WGSL uniforms/passes;
- overlay composition і hit-test silhouette.

Гэтыя рэчы належаць нашаму `PresentationDirector + pet_body + pet_audio + Renderer`. Менавіта тут жыве наш visual signature.

## Уладальнікі стану пасля fusion

| Стан | Адзіны writer |
|---|---|
| Organism identity, body/voice genome, lineage | portable organism envelope |
| Drives, affect, memory, attention, policy | актыўны `BrainBackend` |
| High-level behavior decision | актыўны `BrainBackend`; `BrainRouter` толькі валідуюць/fallback |
| Focus/safety/privacy constraints | local runtime validator |
| Face, motion grammar і brain-to-visual mapping | наш `PresentationDirector` |
| Body position/velocity/contact/liquid state | `pet_body` |
| Material, lighting, renderer і tuning profile | `pet_body::Renderer` + Body Lab profile |
| Audio samples і mouth envelope | `pet_audio` |
| Window/topology/capabilities | `desktop_host` |
| Atomic save/backup/migration | `StateStore` |

## Persistence v2

Не трэба прымушаць калегаву infra серыялізавацца ў наш стары `LifeSnapshot`.

Рэкамендаваны envelope:

```text
PortablePetStateV2
  identity
  position
  selected_brain_provider
  brain_state:
    provider_id
    protocol_version
    provider_schema_version
    payload
    checksum
  visual_profile_ref/revision
```

Правілы:

- v1 fixture працягвае загружацца праз migration у `NativeBrain`;
- provider payload opaque для renderer і host, але мае size limit, checksum і validation;
- body liquid particles і audio callback state не захоўваюцца;
- пераключэнне provider-а не павінна ціха губляць identity. Калі поўная semantic migration немагчымая, захоўваем абодва provider snapshots і яўна выбіраем актыўны;
- save адбываецца atomic + backup, як цяпер.

## Runtime і latency model

- Body physics, cursor contact, blink continuity, audio callback і visual smoothing заўсёды local.
- Local embodied loop працягвае працаваць на сваіх 30–120/60 Hz нават калі provider думае павольней.
- Brain contract застаецца tick-based. Для slow/remote infra `BrainRouter` трымае апошні валідны intent толькі да яго TTL.
- Timeout, invalid values, schema mismatch або disconnect не замарожваюць pet: runtime пераходзіць у bounded neutral behavior або `NativeBrain`.
- Калі provider nondeterministic, replay захоўвае/прайграе `BehaviorV1` trace; ён не спрабуе паўторна выклікаць provider і выдаваць гэта за deterministic replay.

## Паэтапная інтэграцыя

### Gate 0 — зафіксаваць baseline

Захаваць:

- v1 portable fixture і seed-42 10-second headless trace;
- action/affect/body hashes;
- approved Body Lab tuning profile і яго revision;
- target-size visual captures на review backgrounds;
- CPU/GPU timings і launch/suspend/save smoke.

Done: ёсць baseline, з якім можна параўнаць fusion, а не толькі меркаванне “выглядае падобна”.

### Gate 1 — protocol без змены behavior

Дадаць versioned `pet_protocol`, `BrainBackend` і `NativeBrain` adapter. App размаўляе з native brain толькі праз port. Downstream пакуль можа атрымліваць цяперашнія `BodyIntent`/`VisualMindInput` праз adapter.

Done:

- native baseline trace і snapshot round-trip не змяніліся;
- `cargo test --workspace` і headless smoke праходзяць;
- provider contract не імпартуе `wgpu`, `winit`, `cpal` або native APIs.

### Gate 2 — прыбраць double arbitration

Скампанаваць `LifeCore + VitaMind` у адзін native `BehaviorV1`. `PresentationDirector` становіцца адзіным месцам, дзе semantic behavior перакладаецца ў body/face/audio intent.

Done: адзін writer на action/pose/target; VITA больш не перапісвае чужы intent у app loop.

### Gate 3 — colleague shadow mode

Адпраўляць абодвум backend-ам адзін `ObservationV1`, але на pet ужываць толькі `NativeBrain`. Захоўваць bounded decision diff: action, target, emotion, gaze, voice act, latency, invalid/timeout.

Done:

- `Morph Shadow` запускае сапраўдную сетку ў тым жа працэсе і мае нулявы ўплыў на visual/body output;
- payload — толькі existing normalized `SensorFrame`, semantic `BodyFeedback` і shared `LifeState`; raw pixels/text/audio не ўваходзяць;
- headless summary захоўвае command counts/switches, rates, attention, affect і p50/p95/max tick time.

### Gate 4 — suggestion mode

Калегава infra прапануе high-level goal, attention або memory recall. Local native kernel захоўвае safety, focus, annoyance budget, body self-model і final validation.

Implemented candidate: `Morph Fusion` перакладае каманду/attention у VITA enrichment,
пасля чаго існуючы адзін `FusionArbiter` робіць final validation. Morph authority
абмежаваны `0.30` не толькі ў diagnostics: continuous attention/appraisal/gaze/target
рэальна lerp-аюцца гэтым weight, а discrete pose/locomotion Morph наўпрост не
перапісвае. Local sleep/focus/drag/retreat/metamorphosis kernel абнуляе Morph
раней за ўсе override-ы.
Player-visible blind A/B застаецца жывым acceptance gate перад тым, як рабіць гэты
рэжым default.

### Gate 5 — optional authority

Толькі пасля папярэдніх gates калегаў backend можа выбіраць high-level behavior. Presentation і embodiment усё роўна нашы.

Done:

- offline/fallback path правераны;
- save migration і provider switching не губляюць identity;
- target performance, macOS/Windows behavior і long-running boundedness праходзяць;
- адзін provider не можа запісаць стан іншага без яўнай migration.

## Visual acceptance gates

Fusion лічыцца бяспечным толькі калі:

1. `NativeBrain` дае той жа headless/action/body baseline пасля protocol extraction.
2. Адзін і той жа `BehaviorV1` trace дае адзін і той жа presentation/body trace незалежна ад provider-а.
3. Калегаў payload не змяшчае renderer/material/particle controls.
4. Current Safe і approved Cinematic profile застаюцца імгненным rollback/A-B lane.
5. Gaze, lids, mouth/audio sync, squash/stretch, liquid continuity і silhouette праходзяць target-size review.
6. Disconnect/timeout не выклікае pose pop, teleport, stuck vocalization або visual reset.
7. Metamorphosis atomic: identity/genome абнаўляюцца разам, body бяспечна рэгенеруецца, self-model атрымлівае calibration event.
8. Visual QA праводзіцца на рэальным Windows desktop і Apple Silicon, не толькі ў unit tests.

## Бягучы integration debt, які варта закрыць да colleague authority

1. Злучыць visual sampler з perception або выдаліць ілжывае сцвярджэнне, што ён active.
2. Зрабіць sampling asynchronous/bounded; цяпер backend API сінхронны, а app яго не выклікае.
3. Падключыць runtime config або перастаць пакоўваць яго як актыўную канфігурацыю.
4. Узгадніць sleep FPS: README/config кажуць 15, фактычная канстанта app дае 30.
5. Падключыць `UserProfile` да runtime або пакінуць schema як explicit future work.
6. Зафіксаваць approved visual profile/revision і reference captures для вялікага uncommitted renderer/body slice.
7. Праверыць real macOS overlay/Metal/CoreAudio і capability fallbacks.

## Што трэба атрымаць ад калегі

1. Мова і форма runtime: library, process, local service або remote service.
2. Якія input/output structs і rates ужо ёсць.
3. Хто ў іх валодае identity, memory, affect, policy і persistence.
4. Latency distribution, timeout і reconnect behavior.
5. Offline path і ці ёсць network dependency.
6. Determinism/seed/replay guarantees.
7. Windows x64 і macOS arm64 support.
8. Якія raw data яны чакаюць і ці могуць працаваць толькі з нашымі scalar/semantic observations.
9. State schema/versioning/migration, payload size і corruption behavior.
10. License, provenance, privacy і магчымасць запусціць shadow mode без уплыву на pet.

Гэтыя адказы не змяняюць асноўную мяжу; яны вызначаюць толькі канкрэтны adapter і тое, на якім Gate integration спыніцца.

## Першы выканальны slice

Без чакання калегавага кода можна зрабіць адзін bounded slice:

1. зафіксаваць `ObservationV1`, `BehaviorV1`, `FeedbackV1` і `BrainStateEnvelope` fixtures;
2. стварыць `BrainBackend`, `NativeBrain` і `BrainRouter` у native-only mode;
3. перанесці цяперашнюю `LifeCore -> VitaMind -> intent` кампазіцыю за adapter;
4. дадаць deterministic brain-decision trace;
5. даказаць, што seed-42 smoke, v1 save fixture і ўсе workspace tests не змяніліся.

Гэта дае калегу стабільны порт для інтэграцыі, але не закранае renderer, liquid simulation або наш art direction.
