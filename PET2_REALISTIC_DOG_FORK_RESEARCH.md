# Pet 2: рэалістычная 3D-сабака з ригам, навучаннем і realtime-поўсцю

> **Update 2026-08-24:** PIDI быў адхілены па art direction. Яго рэкамендацыя ў гэтым дакуменце больш не актуальная. Новы вердыкт — constrained procedural `Canine Forge`; гл. [PET2_PROCEDURAL_CANINE_DEEP_RESEARCH.md](./PET2_PROCEDURAL_CANINE_DEEP_RESEARCH.md).

Дата праверкі: 2026-08-24

## Кароткі вердыкт

Так, патрэбныя блокі існуюць. Але гатовага камерцыйна бяспечнага блока «любая сабака + эвалюцыя цела + сама вучыцца ўсім рухам + танная поўсць» цяпер няма.

Самы хуткі рэалістычны шлях для асобнага proof-of-concept:

**Unity URP на D3D11 + PIDI German Shepherd + звычайны Animator/Blend Tree + IK чатырох лап + UniWindowController + спачатку PBR-fuzz, пасля 8–16 shell-слаёў XFur + існуючы LifeCore/VITA як адзіны мозг.**

Гэта дазваляе хутка праверыць, ці працуе сама ідэя «рэальная сабака Pet 2 на працоўным стале». Калі выгляд і паводзіны атрымліваюцца, наступны крок — перанесці body-backend у native Rust/wgpu, калі важныя цяперашнія ліміты памеру і RAM.

Не варта ў першай версіі:

- вучыць суставы RL/SGD непасрэдна на камп'ютары карыстальніка;
- будаваць поўсць з сапраўдных асобных валасоў;
- рабіць бесперапынны morph паміж усімі пародамі;
- спрабаваць проста падмяніць цяперашні liquid mesh: цяперашні рэндэр Pet 2 не з'яўляецца skinned-3D-рэндэрам.

## Што ўжо ёсць у Pet 2

Захоўваюцца амаль без змен:

- `LifeCore`: патрэбы, эмоцыі, памяць, звычкі, reward learning і дэтэрмінаваны microbrain;
- `VITA`: сэнсавыя рашэнні, позірк, поза, мэта руху і экспрэсія;
- desktop-сенсары, курсор, idle, topology экранаў, захаванне стану;
- `BodyIntent` як каманда целу і `BodyFeedback` як адказ мозгу;
- genome, lineage і падзея metamorphosis.

Трэба замяніць або дадаць паралельна:

- skinned mesh сабакі і сапраўдны skeleton;
- animation graph, paw contacts, IK і foot locking;
- canine morph profile;
- 3D materials/fur;
- сілуэт для click-through і dog-specific hit testing.

Цяперашні `ProceduralBody` — добрае канцэптуальнае месца для новага backend, але код пакуль шчыльна звязаны з liquid simulation і рэндэрам. Унутраны `BodyGraph` не з'яўляецца поўным сабачым ригам, а renderer не загружае skinning matrices. Таму «паставіць FBX замест цяперашняга mesh» недастаткова.

Рэкамендаваная мяжа:

```text
LifeCore + VITA (адзіны аўтарытэтны мозг)
        |
        v
BehaviorV1 / PetIntent
{ action, target velocity, facing, gaze, affect, pose, TTL }
        |
        v
Dog Presentation Director
  - clip/blend-tree locomotion
  - phase + paw contacts
  - foot IK + pelvis correction
  - head/eye aim
  - face blendshapes
  - ears/tail secondary motion
        |
        v
Skinned dog + materials/fur
        |
        v
BodyFeedback
{ grounded, speed, contacts, slip, failed transition }
        |
        +---------------------------> LifeCore + VITA
```

Мозг кажа **што сабака хоча зрабіць**. Dog backend вырашае **як менавіта рухаць косткамі**. Гэта захоўвае асобу Pet 2 і не дае эксперыментальнай анімацыі сапсаваць памяць або паводзіны.

## Найлепшыя гатовыя блокі

### 1. Базавая сабака: PIDI German Shepherd

[Афіцыйны Unity Asset Store](https://assetstore.unity.com/packages/3d/characters/animals/pidi-realistic-3d-german-shepherd-dog-322044) на момант праверкі паказвае версію 3.1.0 ад 29 мая 2026, сумяшчальнасць з Unity 2022.3.62 і Unity 6000.0.70 ва ўсіх трох render pipelines, Single Entity license і source-package 100.7 MB. [Індэкс тэхнічных метаданых пакета](https://www.gameassetdeals.com/asset/322044/pidi-realistic-3d-german-shepherd-dog) заяўляе 3 LOD, 43 bones, 58 animations, 4K maps, тры афарбоўкі, facial blendshapes і гатовыя XFur profiles; гэтыя пункты трэба яшчэ раз звярыць у Package Content перад купляй.

Чаму гэта лепшы першы блок:

- адна кансістэнтная мадэль, rig і вялікі набор dog-specific clips;
- ёсць LOD і гатовая сувязь з shell-fur;
- дастаткова выразаў твару для перакладу affect/VITA;
- невялікая геаметрыя ў параўнанні з film asset.

Абмежаванне: гэта адна парода. Для першага slice гэта плюс, а не мінус: спачатку трэба даказаць рух, альфа-акно і характар.

### 2. Desktop window: UniWindowController

[UniWindowController](https://github.com/kirurobo/UniWindowController) — MIT-бібліятэка для borderless/transparent/topmost Windows і macOS акна, пазіцыі, памеру і click-through.

Для Windows ёсць жорсткія ўмовы:

- Direct3D 12 transparency не працуе — патрэбны D3D11;
- трэба выключыць DXGI flip-model swapchain для D3D11;
- у URP трэба выключыць HDR і ўключыць Alpha Processing;
- правяраць transparency трэба ў build, не толькі ў Editor;
- opacity hit-test цяжкі, таму патрэбны collider/raycast hit-test.

Менавіта transparent compositor, а не dog rig, з'яўляецца галоўнай тэхнічнай рызыкай хуткага Unity-форка.

### 3. Поўсць: XFur Studio 4 або ўласныя shells

[Афіцыйная дакументацыя XFur Studio 4](https://irreverent-software.com/docs/xfur-studio-4/user-manual/xfur-studio-main-components/) пацвярджае, што гэта **shell-based fur**, а не сапраўдныя strands. Яна працуе са Skinned Mesh Renderer, пераносіць skinning на GPU, не патрабуе compute або geometry shader і ў URP мае anisotropic-lighting рэжым. Базавая тэхніка апісана ў класічнай працы [Real-Time Fur over Arbitrary Surfaces](https://people.csail.mit.edu/ericchan/bib/pdf/p227-lengyel.pdf): shells даюць аб'ём, а fins могуць рамантаваць сілуэт на вострых кутах.

Shell fur — гэта некалькі амаль аднолькавых паверхняў над скурай. Кожны наступны слой паказвае іншую «вышыню» з texture/noise mask. На малым сабаку мозг счытвае іх як шчыльную поўсць.

Рэкамендаваныя tiers:

| Рэжым | Візуальны метад | Мэта |
|---|---|---|
| Low / далёка / батарэя | opaque PBR + detail normal + лёгкі rim/fuzz | самы стабільны альфа-сілуэт |
| Normal | 8–12 shells | асноўны desktop-рэжым |
| Close / active | 16 shells | лепшы аб'ём пры блізкім памеры |
| Hero, толькі калі трэба | некалькі cutout cards на хвасце, грудзях і вушах | доўгія пучкі без shells па ўсім целе |

Не колькасць triangles, а overdraw і desktop-alpha будуць галоўным коштам. Вочы, нос і кіпцюры павінны застацца асобнымі opaque materials. TAA, bloom, SSAO, motion blur і HDR спачатку выключыць.

### 4. Навучаная dog locomotion: яна сапраўды існуе

[AI4AnimationPy Quadruped](https://github.com/facebookresearch/ai4animationpy/tree/main/Demos/Locomotion/Quadruped) ужо мае `Dog.glb`, pretrained networks, walk/pace/trot/canter, sit/lie/stand guidance, прагназаванне paw contacts і FABRIK IK. Гэта самы блізкі да запыту гатовы даследчы demo-блок.

Але яго [ліцэнзія CC BY-NC 4.0](https://github.com/facebookresearch/ai4animationpy/blob/main/LICENSE) дазваляе толькі некамерцыйнае выкарыстанне. Таму ён добры для ізаляванага A/B-доказу, але не як залежнасць shipping-версіі.

Іншыя пацверджаныя шляхі:

- [DeepMimic](https://github.com/xbpeng/DeepMimic) мае physics-based `dog3d`, але гэта стары research/training stack, а не game component;
- [MimicKit](https://github.com/xbpeng/MimicKit) — актуальнейшая Apache-2.0 база для offline motion imitation, але гатовай рэалістычнай сабакі і камерцыйна чыстых dog motions у ёй няма;
- [Unity AI Inference/Sentis](https://docs.unity3d.com/Packages/com.unity.ai.inference@2.4/manual/index.html) можа пазней запускаць frozen ONNX-мадэль лакальна на CPU/GPU;
- Unreal Motion Matching добра выбірае позы з вялікай бібліятэкі, але сам не вучыцца і патрабуе шмат легальных clips.

Вынік: neural locomotion тэхнічна рэальная, але **baseline clips + contact-aware IK** дасць больш надзейную сабаку за два дні. Neural controller трэба пакінуць заменнай крыніцай позы з імгненным fallback на clips.

## Што павінна «вучыцца ўсё лепей»

### Бяспечна ўжо цяпер

LifeCore/VITA вучацца:

- калі ісці да чалавека, гуляць, адпачываць або не перашкаджаць;
- якую дыстанцыю і хуткасць чалавек лепш успрымае;
- якія idles, позы і рэакцыі падабаюцца ў гэтым кантэксце;
- інтэнсіўнасць позірку, хваста, вушэй і экспрэсіі;
- перавагі канкрэтнага карыстальніка без змены rig.

Можна дадаць невялікі `MotionAdaptationProfile`, які абмежавана карэктуе:

- stride/speed ratio;
- transition duration;
- root height і paw offsets для канкрэтнага morph;
- approach distance;
- tail/gaze intensity.

Кожны параметр мае межы, version, checkpoint і rollback. Новая версія праходзіць deterministic shadow replay перад актывацыяй.

### Пазней, offline

Калі clip+IK baseline ужо добры і ёсць **уласныя або выразна дазволеныя** motion data:

1. збіраць толькі патрэбныя метрыкі: intent, clip/model id, paw slip, IK correction, latency і fallback;
2. трэніраваць motor model у асобным simulator/editor працэсе;
3. правяраць joint limits, morph extremes, suspend/resume і доўгі replay;
4. пастаўляць frozen versioned model;
5. пры NaN, затрымцы або дзіўнай позе за адзін frame вяртацца да clips.

Не трэба рабіць live continual learning костак: няма надзейнага reward-сігналу, магчымыя catastrophic forgetting, нагрэў GPU і бачныя зламаныя позы.

## Morph / «эвалюцыя» цела

Сапраўдныя parametric dog models існуюць: [BARC](https://openaccess.thecvf.com/content/CVPR2022/html/Ruegg_BARC_Learning_To_Regress_3D_Dog_Shape_From_Images_by_CVPR_2022_paper.html), D-SMAL і [BITE](https://openaccess.thecvf.com/content/CVPR2023/html/Ruegg_BITE_Beyond_Priors_for_Improved_Three-D_Dog_Pose_Estimation_CVPR_2023_paper.html) мадэлююць dog-specific shape, pose і bone proportions. Але асноўныя models/data маюць non-commercial research terms, таму гэта не хуткі production-блок.

Для Pet 2 лепш захаваць genome і metamorphosis, але перакласці іх у `DogMorphProfile`:

- агульны scale;
- даўжыня ног і вышыня корпуса ў вузкім бяспечным дыяпазоне;
- chest і muzzle blendshape;
- памер вушэй і хваста;
- 2–4 facial blendshapes;
- fur length, density, roughness і coat color;
- stride scale, paw radius/offset і root height для анімацыі.

Першы дыяпазон: прыблізна ±10–15%, адна парода, адзін canonical skeleton. Morph адбываецца ў бяспечнай stand-позе за 2–5 секунд. Захоўваюцца параметры morph, а не выпадковыя bone transforms.

Вельмі розныя пароды лепш пазней рабіць як некалькі сумяшчальных rigs/profiles, а не адзін магічны slider. Інакш спатрэбяцца corrective blendshapes, retargeting і асобная праверка кожнай анімацыі.

## Як перавесці цяперашнія намеры Pet 2 у сабаку

| Цяперашні intent | Рэалістычная сабачая падача |
|---|---|
| `Seek / Arrive / Wander / Flee` | walk, trot або run па speed + turn |
| `Sleep` | lie-down → sleep loop |
| `SurfaceApproach / Landing` | step/jump/land толькі там, дзе гэта анатамічна магчыма |
| gaze / interest | head + eye aim, пасля мяккі body turn |
| positive affect | мяккі твар, ears, tail wag, play bow |
| fear / low energy | ears back, lower posture, slower gait |
| `Hover / Orbit / EdgeCling / Cocoon` | stand-and-look, circle/sniff, peek або curl-up; не прымушаць «рэальную» сабаку лётаць |

Для першага desktop slice сабака ходзіць па ўнутранай ground line / верхнім краі taskbar. Рух акна або экранная пазіцыя застаюцца аўтарытэтнымі; анімацыя пераважна in-place, а paw locking хавае невялікае разыходжанне.

## Варыянты рэалізацыі

| Варыянт | Час да бачнага proof | Плюсы | Мінусы | Рашэнне |
|---|---:|---|---|---|
| **A. Асобны Unity fork**: PIDI + URP + UniWindow + clips/IK + XFur | 1–2 дні | усе складаныя 3D-блокі ўжо ёсць; самы хуткі візуальны адказ | Unity player амаль напэўна парушыць цяперашнія `<50 MB` package і, верагодна, `<150 MB RAM`; альфа-акно рызыкоўнае | **Рэкамендаваны proof** |
| **B. Native Rust/wgpu dog backend**: glTF skin/animation + свой animation graph + shells | прыблізна 1–3 тыдні | захоўвае цяперашняе акно, памяць, мозг і кантроль над alpha | трэба напісаць skinning, clip blending, IK, materials, importer і fur | **Рэкамендаваны product path пасля proof** |
| C. Bevy fork | некалькі дзён–тыдняў | glTF skin, morph targets і animation graph ужо ёсць | усё роўна міграцыя desktop overlay/renderer, большы build; няясная перавага над A або B | Не першы выбар |
| D. Unreal + Groom/physics RL | тыдні–месяцы | найвышэйшая hero-якасць | занадта цяжка для пастаяннага desktop pet; groom і transparent compositor дарагія | Адхіліць для v1 |

### Як хутка падключыць мозг у Unity proof

Не пераносіць cognition у C#. Зрабіць маленькі headless Rust host, які валодае LifeCore/VITA і persistence, а Unity валодае толькі dog window/presentation.

На першым этапе:

```text
Rust brain host <-> localhost/named-pipe BehaviorV1 + BodyFeedback <-> Unity dog
```

Пратакол павінен быць versioned, bounded і мець TTL. Калі сувязь знікае, сабака за ≤100 ms пераходзіць у бяспечны neutral idle. Пасля proof можна выбраць C ABI/DLL або native wgpu backend без змены сэнсавай мяжы.

## 48-гадзінны spike

### Baseline і representative scene

- Windows 11, Unity URP, D3D11, transparent topmost build памерам 512–768 px;
- PIDI dog без fur як вядомы baseline;
- 8 неабходных станаў: idle, look, walk/turn, sit, lie/sleep, bark, play/petting;
- brain trace: idle → курсор → approach → sit → petting → sleep → bounded metamorph;
- фоны: чорны, белы, шэры, checkerboard, рэальны desktop; статычны і рухомы;
- collider-based click-through.

### Паслядоўнасць

1. **0–4 h:** import dog, праверыць license/package, bone map і clips.
2. **4–12 h:** Animator/blend tree, head aim, 4-paw contacts/IK; празрыстае акно без fur.
3. **12–20 h:** `BehaviorV1` fixture, пасля жывы headless LifeCore/VITA bridge.
4. **20–32 h:** A/B: PBR-fuzz, 8, 12 і 16 shells; LOD і fallbacks.
5. **32–40 h:** два вузкія morph profiles, stride/root/paw calibration.
6. **40–48 h:** 10-хвілінны unattended replay, alt-tab/suspend/resume, multi-DPI/background matrix і справаздача.

### Pass gates

- 60 fps на мэтавым discrete GPU; 30 fps iGPU fallback;
- p95 frame ≤16.7 ms для 60-fps tier;
- дадатковы fur GPU cost ≤2 ms на discrete GPU або ≤6 ms на iGPU пры 512 px і не больш за 25% frame budget;
- не больш за 1 px dark/color halo на чорным, белым і checkerboard;
- няма temporal trail даўжэй за 2 frames;
- 100/100 клікаў па празрыстым месцы праходзяць наскрозь, 100/100 па collider сабакі рэгіструюцца;
- planted paw slip <2% даўжыні корпуса за крок;
- 20 прымусовых LOD transitions без відавочнага pop;
- 10 хвілін без NaN, broken pose або страты persistent brain state;
- brain timeout → neutral idle ≤100 ms.

### Kill criteria і fallback

| Калі | Рашэнне |
|---|---|
| Alpha не выпраўляецца за 4 h | выключыць post effects, мінімальны URP/D3D11 path; калі ўсё яшчэ дрэнна — kill Unity overlay |
| Shells займаюць >25% budget або даюць fringe/shimmer | ship PBR-fuzz + sparse cutout cards, без whole-body shells |
| Click-through блакуе desktop | толькі collider/raycast або вяртанне да native desktop host |
| Unity RAM/package непрымальныя | выкарыстоўваць proof толькі як visual spec і перанесці ў native wgpu |
| Morph ламае IK/skin | адзін dog profile; discrete breeds пазней |
| License на motion/ML няясная | не выкарыстоўваць asset у training; learning толькі на abstract actions |

## Ліцэнзійная мяжа

Гэта не юрыдычная кансультацыя, але тут ёсць важная практычная пастка.

[Unity Asset Store EULA](https://unity.com/legal/as-terms) дазваляе ўбудоўваць і мадыфікаваць звычайны asset у substantial product, але забараняе выкарыстоўваць Asset Store assets як training data, model inputs або частку AI/ML creation process без express consent provider/Unity — нават некамерцыйна.

Таму бяспечная схема:

- камерцыйны dog mesh/rig/clips выкарыстоўваюцца толькі для runtime presentation;
- LifeCore вучыцца на abstract state/action/reward, не на vertices, rendered pixels або animation frames;
- motor-model трэніруецца толькі на ўласных, CC0/дазволеных motions;
- калі clips купленай сабакі павінны стаць training input, спачатку атрымаць пісьмовы дазвол publisher.

AI4AnimationPy, DigiDogs, 3DDogs, BARC/D-SMAL — карысныя research references або закрыты некамерцыйны experiment, не production dependency.

## Канчатковае рашэнне

**GO на асобны 48-гадзінны Unity proof, але не на поўны пераезд праекта.**

Першы slice павінен даказаць толькі адно: ці адчуваецца рэалістычная сабака з цяперашнім мозгам Pet 2 жывой і прыгожай на празрыстым desktop. Яго frontier-рызыка — shell fur у alpha-window. Neural joint controller, physics RL і multi-breed continuous morph не ўваходзяць у гэты slice.

Калі proof праходзіць pass gates:

1. замарозіць `BehaviorV1` / `BodyFeedback`;
2. пакінуць LifeCore/VITA адзіным уладальнікам асобы і навучання;
3. выбраць: ship Unity з новымі budget limits або перанесці зацверджаны выгляд у native `DogBodyBackend` на wgpu;
4. толькі пасля стабільнага clip+IK baseline зрабіць асобны noncommercial AI4AnimationPy A/B;
5. neural motor model дадаваць толькі як replaceable frozen pose source з clip fallback.

Так атрымаецца не проста «3D-мадэль сабакі», а сапраўды той жа Pet 2 у новым рэалістычным целе — з памяццю, характарам, навучаннем і кантраляванай эвалюцыяй выгляду.
