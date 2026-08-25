# Pet 2: працэдурная сабака замест чарговага stock-асэта

> **Update 2026-08-24:** карыстальнік удакладніў, што патрэбна neural active-ragdoll сабака з працэдурнай фізікай, а Poly Art яму падыходзіць. Clip-first рэкамендацыя ў секцыі руху больш не фінальная; гл. [PET2_NEURAL_PHYSICS_DOG_ARCHITECTURE.md](./PET2_NEURAL_PHYSICS_DOG_ARCHITECTURE.md).

Дата праверкі: 2026-08-24
Статус: decision memo + art/tech research
Гэты дакумент **замяняе папярэднюю рэкамендацыю PIDI** з `PET2_REALISTIC_DOG_FORK_RESEARCH.md`.

## Вердыкт у двух сказах

**Не трэба шукаць яшчэ адну “гатовую сабаку”. Трэба хутка сабраць маленькі `Canine Forge`: адзін прыгожы кананічны mesh са стабільнай тапалогіяй і шкілетам, 10–16 анатамічных марф-восей, абмежаванні ад вырадкаў, працэдурны coat-генератар і звычайная надзейная анімацыя з IK.**

Для 48–72-гадзіннага visual/motion proof лепшая bootstrap-база — **Malbers Skye Poly Art**, не рэалістычная Skye: яна ўжо мае выразны tiny-screen сілуэт, adult/puppy, 32 morphs і амаль 300 рухаў. Для прадукту база павінна стаць уласнай або замоўленай: DOGWALK-inspired canonical dog з той жа чытаемай мовай формы. **Daz Dog 8 + Phenotypes** застаецца карысным anatomy/morph lab, але ён даражэйшы, цяжэйшы і менш шармовы на desktop-памеры.

Research-мадэлі SMAL/BARC/BITE/RGBT-Dog і свежыя dog-avatar/3DGS-праекты пацвярджаюць правільную структуру — shared topology + semantic skeleton + асобныя shape/pose/appearance layers, — але цяпер гэта пераважна **research-only, non-commercial, offline або без публічнага кода**. Іх трэба выкарыстоўваць як навуковы чарцёж, не як dependency.

---

## 1. Чаму PIDI выглядае дрэнна менавіта для Pet 2

Праблема не толькі ў якасці мадэлі.

1. **Гэта чужая завершаная сабака, а не наша сістэма формы.** Калі да stock-mesh дадаць scale, колер і некалькі blendshape, ён усё адно чытаецца як адзін і той жа асэт.
2. **Няма выразнай art thesis.** “Рэалістычная нямецкая аўчарка” — апісанне тавару, а не персанажа Pet 2.
3. **Эвалюцыя не закладзена ў прадстаўленне.** Выпадковыя slider-ы хутка даюць вузкую галаву, зламаныя лапы, вочы ў шчоках і футра, якое адрываецца ад цела.
4. **На desktop-памеры працуе не micro-detail.** Пры 128–512 px мозг бачыць сілуэт, прапорцыі галавы, вушы, хвост, вочы/нос і рытм руху. 4K-тэкстура не ратуе generic silhouette.
5. **Фоторэал без production-рэсурсу трапляе ў uncanny valley.** Лепш добрая сабачая анатомія + выразна аўтарская поўсць, чым пасрэдны “AAA dog”.

### Новая art thesis

> **Жывая палявая сабака:** праўдзівая сабачая анатомія і хада, але яе поўсць, плямы і дробныя прапорцыі павольна запамінаюць гісторыю жыцця.

Кантраляванае супрацьпастаўленне:

- цела і кантакты лап — праўдзівыя;
- формы — крыху выразнейшыя за натуру, каб працаваць у малым памеры;
- coat — немагчыма “жывы”: узоры, даўжыня, сівізна і акцэнты могуць эвалюцыянаваць;
- эмоцыі — не чалавечыя бровы, а вушы, хвост, шея, позірк, дыханне і тэмп.

Гэта “рыл сабака”, але не фотакопія і не Marketplace-персанаж.

---

## 2. Якія варыянты рэальна ёсць

| Варыянт | Што атрымліваем | Хуткасць | Унікальнасць | Рызыка | Рашэнне |
|---|---|---:|---:|---:|---|
| Malbers Skye **Poly Art** | Adult + puppy, 298+ рухаў, 32 morphs, 63 skins; выразная форма | вельмі хутка | сярэдняя | bounded stock-база, NoAI | **Лепшы proof chassis** |
| Malbers Skye realistic | Прыгожая сабака, багатыя анімацыі, morphs | вельмі хутка | нізкая–сярэдняя | stock-look, fur alpha, цяжкі пакет | **Толькі realism baseline** |
| Daz Dog 8 + Phenotypes → Blender | 63 construction morphs, рэальныя і fantasy phenotypes | хутка–сярэдне | сярэдняя–высокая пасля art-pass | cleanup, асобныя interactive licenses, no-AI clause | **Anatomy/morph lab** |
| Свой canonical dog + cage/RBF morphs | Поўны кантроль, стабільная тапалогія, уласная мова формы | сярэдне | высокая | патрэбны моцны character artist/tech art | **Фінальны production-path** |
| Skeleton + SDF/metaballs → mesh | Сапраўды працэдурная, вельмі ўнікальная форма | сярэдне для demo, доўга для якасці | вельмі высокая | blobby-анатомія, твар, weights, тапалогія | **Адзін bounded frontier experiment** |
| SMAL/BITE/3DGS/photo-to-dog | Сабака з фота, pose transfer, neural appearance | павольна/offline | вельмі высокая | research-only ліцэнзіі, няма кода, GPU/memory | **Будучая функцыя, не v1** |
| DOGWALK/papercraft | Моцны стыль, танны і стабільны render | хутка | высокая | менш “рэальная сабака” | **Стылёвы fallback** |

### Моцная рэкамендацыя

**Двухкрокавы маршрут: Skye Poly Art як хуткі proof → уласны Blender Canine Forge як product identity.** У proof пакінуць толькі 8–12 бяспечных morph parameters, palette-driven coat і агульны animation stack. Калі сабака праходзіць tiny-screen/taste gate, паўтарыць ужо правераную граматыку сілуэту на ўласным canonical mesh з трыма phenotype anchors. Daz выкарыстоўваць толькі калі трэба хутка stress-test больш шырокую анатамічную прастору.

---

## 3. Правераныя гатовыя блокі

### 3.0 Malbers Skye Poly Art: найлепшае хуткае шасі

[Skye — Border Collie Poly Art](https://www.fab.com/listings/a7cd32af-d039-4386-b8fc-db9b96fefc27) заяўляе 298+ in-place/root-motion animations, 32 morph targets + scalable bones, adult і puppy, 63 skin sets і palette shader. На момант праверкі Fab паказваў `$25.99 Personal / $50.99 Professional` да мясцовага падатку; правы Standard License аднолькавыя, але патрэбны правільны revenue tier на момант пакупкі. Нічога не купляць без асобнага рашэння.

Чаму ён мацнейшы за realistic dog для desktop:

- faceted fur clumps, вялікая галава/лапы, вочы, blaze/muzzle/chest/paw markings захоўваюцца пры 150–250 px;
- morph extremes мяняюць head/chest/legs/ears і таму бачныя нават як grayscale silhouette;
- adult/puppy адразу дае першы праверачны metamorph;
- амаль 300 рухаў закрываюць motion baseline раней, чым мы пабудуем уласную бібліятэку.

Гэта ўсё яшчэ bounded authored asset, не сапраўдны breed generator. Listing стаіць `Allows usage with AI: No`, а аўтар папярэджвае пра anatomy на крайніх morphs. Таму выкарыстоўваць для deterministic morph/blend/IK і ordinary rendering; не трэніраваць на ім neural shape/motion model. Калі proof паспяховы, яго stock-мову трэба замяніць уласнай, а не размножваць выпадковымі sliders.

### 3.1 Daz Dog 8 + Phenotypes: самая шырокая гатовая parametric-база

[Daz Dog 8](https://www.daz3d.com/daz-dog-8) — rigged dog base. [Phenotypes for Dog 8](https://www.daz3d.com/phenotypes-for-dog-8) дадае **63 construction morphs**: агульныя Bigger/Smaller/Heavy/Thin/Muscular/Loose Skin і частковыя кантролі галавы, морды, вушэй, шыі, грудзей, тулава, ног, лап, хваста, ruff/fluff ды іншае. Гэта значна бліжэй да “генератара сабакі”, чым любы адзін breed asset.

Афіцыйны [Daz to Blender bridge](https://github.com/daz3d/DazToBlender/blob/master/README.md) перадае skeletal mesh, animation і выбраныя morphs як shape keys. Для proof гэтага дастаткова, але Dog 8 трэба асобна праверыць на joint corrective morphs, weights і export extremes.

На момант праверкі старонкі паказвалі толькі базавы morph stack:

- Dog 8: `$25.95` + Interactive License `$50`;
- Phenotypes: `$19.99` + Interactive License `$35`;
- разам каля `$130.94` да падаткаў, без animation/fur add-ons. Нават мінімальны пакет з асобна ліцэнзаванымі [basic animation cycles](https://www.daz3d.com/dog-8-animation-cycles--base-and-great-dane) выходзіць прыкладна ў `$216.89` да breed/coat/fur add-ons.

**Ліцэнзійны стоп-сігнал:** паводле [Daz EULA / Interactive License](https://www.daz3d.com/eula), кожны выкарыстаны прадукт патрабуе адпаведнага interactive add-on, кантэнт у праграме павінен быць абаронены ад вымання, а Daz content нельга выкарыстоўваць як input для генератыўнай/аналізуючай AI-сістэмы. Таму:

- у гульні/desktop app можна рэндэрыць ліцэнзаваны derivative пры выкананні ўмоў;
- **нельга** трэніраваць shape/motion/generative model на Daz-мешах, морфах або кадрах;
- перад пакупкай трэба зафіксаваць дакладны спіс прадуктаў і ліцэнзій;
- доўгатэрмінова лепш замовіць уласную canonical mesh, узяўшы логіку марфаў, але не капіруючы кантэнт.

### 3.2 Malbers realistic Skye: добры realism baseline, але ўсё яшчэ stock

[Skye — Border Collie](https://www.fab.com/listings/fa25a2b9-0178-4805-97d9-5caeb5e2e82f) цяпер заяўляе 40 skin sets, 298+ animations, 32 morph targets і scalable bones для Unity/Unreal. Гэта на парадак лепшы presentation baseline за PIDI: міміка, хада, станы і дастатковая колькасць марфаў для тэсту.

Але аўтар сам папярэджвае, што anatomy не будзе дакладнай на ўсіх крайніх morphs і сапраўднаму puppy патрэбна асобная мадэль. На listing таксама стаіць `Allows usage with AI: No`. Значыць:

- падыходзіць для традыцыйнага realtime-рэндэру і анімацыі паводле абранай Fab-ліцэнзіі;
- не выкарыстоўваць кадры/mesh/motions як training data;
- купляць толькі пасля ліцэнзійнай праверкі;
- выкарыстоўваць як A/B baseline або часовы motion source, а не як твар Pet 2.

### 3.3 Blender: authoring/generation, не runtime dependency

[Rigify](https://docs.blender.org/manual/en/latest/addons/rigging/rigify/basics.html) мае quadruped/wolf-падобныя meta-rigs; у runtime трэба экспартаваць толькі чысты deform skeleton. [Geometry Nodes](https://docs.blender.org/manual/en/latest/editors/geometry_node.html) і [Generate Hair Curves](https://docs.blender.org/manual/en/4.4/modeling/geometry_nodes/hair/generation/generate_hair_curves.html) добра падыходзяць для offline-генерацыі fur guides, cards, masks і LOD-аў.

Не трэба спрабаваць запускаць Blender Geometry Nodes у Pet 2. Blender — гэта `Canine Forge`, які выпякае:

- base mesh + LODs;
- semantic shape keys і corrective keys;
- deform skeleton + clips;
- UV/part masks/fur-flow map;
- coat palettes, cards і shell masks;
- glTF/FBX/runtime bundle.

### 3.4 Blender Studio DOGWALK: не база рэальнай сабакі, але моцны art/rig donor

[DOGWALK](https://studio.blender.org/projects/dogwalk/) — выразная stop-motion/papercraft сабака з адкрытымі [CC BY assets/code](https://studio.blender.org/blog/thank-you-for-playing-dogwalk/). Гэта не патрэбны нам “real dog”, але карысная крыніца рашэнняў: чытаемы сілуэт, baked clips, простая deform-bone hierarchy, выразная матэрыяльнасць без фоторэалу.

Калі semi-real Canine Forge за 72 гадзіны не дае шарму, DOGWALK-падобная мова — лепшы fallback, чым яшчэ адзін Marketplace animal.

---

## 4. Што даюць навуковыя dog-models — і чаму іх пакуль нельга проста ўставіць

### 4.1 SMAL → BARC/D-SMAL/BITE

[BARC](https://openaccess.thecvf.com/content/CVPR2022/html/Ruegg_BARC_Learning_To_Regress_3D_Dog_Shape_From_Images_by_CVPR_2022_paper.html) і [BITE](https://openaccess.thecvf.com/content/CVPR2023/html/Ruegg_BITE_Beyond_Priors_for_Improved_Three-D_Dog_Pose_Estimation_CVPR_2023_paper.html) будуюць сабаку як parametric body model з агульнай тапалогіяй, shape parameters, pose і skeleton. Гэта менавіта правільны канцэпт для morph-ready Pet 2.

Але public releases, уключаючы [D-SMAL](https://github.com/shenbao233/D-SMAL), абмежаваныя non-commercial scientific research; камерцыйнае выкарыстанне патрабуе асобных дамоўленасцей з rights holders. BITE/BARC таксама не з'яўляюцца лёгкім realtime renderer або гатовай animation system.

**Што пазычаем як ідэю:**

- common topology;
- асобныя shape і pose spaces;
- semantic joints/landmarks;
- low-dimensional bounded shape basis;
- pose correctives;
- regression/fit як offline tooling, не як сэрца runtime.

### 4.2 RGBT-Dog

[RGBT-Dog (WACV 2024)](https://openaccess.thecvf.com/content/WACV2024/html/Deane_RGBT-Dog_A_Parametric_Model_and_Pose_Prior_for_Canine_Body_WACV_2024_paper.html) амаль ідэальна паказвае патрэбны компактны target: fixed 2,426-vertex topology, 43 joints, 10-D body-shape basis, 11-D texture basis, LBS і canine pose prior/VAE. Гэта моцнае пацвярджэнне, што сабачая форма, тэкстура і рух павінны быць асобнымі мадэлямі.

Публічны [RGBD-Dog repository](https://github.com/CAMERA-Bath/RGBD-Dog) пазначае academic/non-commercial intent і не выглядае production-maintained. Для кампаній прапануецца звязвацца з аўтарамі. Таму зараз — reference, не dependency.

### 4.3 Animal Avatar, DogRecon, SMAL-pets, CORGI

- [Animal Avatar (ECCV 2024)](https://remysabathier.github.io/animalavatar.github.io/index.html) рэканструюе animatable animal з casual video; [код](https://github.com/facebookresearch/AnimalAvatar) — CC BY-NC і залежыць ад SMAL/BITE. Offline research tool.
- [DogRecon (IJCV 2025)](https://vision3d-lab.github.io/dogrecon/) ідзе ад аднаго фота да animatable 3D Gaussian dog праз dog priors і generated views; на project page няма production-ready code path.
- [SMAL-pets (2026)](https://piotr310100.github.io/SMAL-pets/) аб'ядноўвае common-topology SMAL mesh і Gaussian splats, паказвае pose/text/image-driven animation, але код пазначаны як coming soon.
- [CORGI (2026)](https://dzzzby.github.io/CORGI/) выкарыстоўвае D-SMAL як анатамічны anchor і deformable 3DGS; публічнага production release на project page няма.

Гэта вельмі цікавы будучы рэжым: **“дай фота свайго сабакі → атрымай Pet 2”**. Для v1 ён дрэнны: ліцэнзійны ланцужок, offline optimization, нестабільны код, alpha/compositing, GPU/memory і цяжкая morph-editability.

**Technology radar:** `HOLD`, сачыць; не будаваць roadmap-залежнасць.

### 4.4 Дакладная production-матрыца свежых рэлізаў

| Сістэма | Што ўжо даказана | Рэальная даступнасць на 2026-08-24 | Значэнне для Pet 2 |
|---|---|---|---|
| Animal Avatars, ECCV 2024 | casual video → fixed-topology D-SMAL mesh + reposable textured avatar; motion transfer | initial code ёсць, але CC BY-NC; патрэбныя cameras/masks і BITE/D-SMAL | моцны reference для shared topology і pose transfer; не шыпіць |
| DogRecon, IJCV 2025 | single RGB → animatable D-SMAL-anchored Gaussian dog; reconstruction reported каля 6 хвілін на RTX 4090 | афіцыйнага code/weights repo няма; D-SMAL/BITE NC | будучы photo-to-pet; не dependency |
| [4D-Animal, WACV 2026](https://github.com/zhongshsh/4D-Animal) | monocular dog video → D-SMAL shape/pose + duplex appearance | wrapper code Apache-2.0, але dog bundle/pipeline усё адно ўтрымлівае BITE/D-SMAL і third-party data constraints | open wrapper не чысціць downstream ліцэнзіі |
| SMAL-pets, 2026 | joint SMAL mesh + bound/unbound Gaussian avatar; pose і appearance editing | [афіцыйны repo](https://github.com/piotr310100/SMAL-pets) пакуль README/assets; checklist кажа, што code не published; няма software license | цікавая мэтавая UX, але пакуль paper demo |
| CORGI, 2026 | D-SMAL-anchored deformable 3DGS, arbitrary pose driving, fur-like detail | няма repo/code/weights/software license; D-SMAL NC | лепшы research-visual, але найгоршая production-залежнасць |
| [GART, CVPR 2024](https://github.com/JiahuiLei/GART) | MIT-licensed canonical Gaussian/forward-skinning core, reposable avatars | core адкрыты, але dog path uses D-SMAL/BITE; headline speed з human benchmark нельга пераносіць на сабаку | карысны GS engineering reference, не commercial dog solution |
| [WildAni4D, CVPR 2026](https://vision3d-lab.github.io/wildani4d/) | world-space dog motion і SMAL+ shape/pose з monocular video | “Code Coming Soon”, няма вагаў/ліцэнзіі; не photoreal appearance avatar | будучы motion-capture tool, не renderer |
| [AniGauss, CVPRW 2026 draft](https://vision3d-lab.github.io/anigauss/) | feed-forward SMAL-topology Gaussian quadruped і pose-conditioned reposing | paper/code “Coming Soon”, няма repo/weights/license | watchlist, не план |

Крытычная праверка: [афіцыйная BITE/D-SMAL license](https://raw.githubusercontent.com/runa91/bite_release/master/LICENSE) дае single-user права для non-commercial research/education/art і забараняе commercial product/service і redistribution без асобнай ліцэнзіі. Таму нават MIT/Apache wrapper не робіць dog pipeline камерцыйна чыстым, калі ўнутры застаецца D-SMAL/BITE model/data.

Для параўнання, Animal Avatars працуе з вельмі кампактным D-SMAL mesh — каля 3,889 vertices / 7,774 faces. Гэта паказвае, што parametric dog **можа** быць лёгкім; цяжкімі свежыя сістэмы робіць не анатомія, а reconstruction/radiance/Gaussian appearance stack. Для Pet 2 лагічна ўзнавіць лёгкую структуру ўласным mesh, не цягнуць neural reconstruction у runtime.

### 4.5 Што з AI/ML усё ж можна ўзяць як offline authoring aid

Ёсць некалькі permissive open-source інструментаў, якія не даюць гатовую dog-genome model, але могуць сэканоміць ручную працу на **ўласным** mesh. Іх output усё адно патрабуе topology/weights/art QA і provenance audit.

| Інструмент | Рэальна карысны блок | Мяжа |
|---|---|---|
| [SMALify](https://github.com/benjiebob/SMALify) | MIT fitter/registration algorithm можа падганяць common topology да калекцыі ўласных artist meshes і дапамагчы пабудаваць свой PCA/corrective basis | не выкарыстоўваць restricted SMAL models/data; толькі algorithm + owned inputs |
| [AniGen, SIGGRAPH 2026](https://github.com/VAST-AI-Research/AniGen) | image → rigged GLB з mesh/skeleton/skin; можа хутка даць кандыдатаў для retopo і art exploration | offline GPU; generated asset і training provenance трэба аўдытаваць; не runtime genome |
| [UniRig, SIGGRAPH 2025](https://github.com/VAST-AI-Research/UniRig) | owned mesh → draft skeleton hierarchy + weights; MIT code/weights | quadruped result не прымаць без ручной anatomy/weights праверкі |
| [Puppeteer, NeurIPS 2025](https://github.com/Seed3D/Puppeteer) | Apache-2.0 rig/weights bootstrap і motion-from-reference-video | аўдыт правоў на reference video; не запускаць у desktop runtime |
| [3D-Fauna / 3DAnimals](https://github.com/3DAnimals/3DAnimals) | evidence для category-level articulated mesh і motion latent space | code MIT, але pretrained-data provenance не дае чыстага commercial dog asset; толькі research/evaluation або retrain на owned data |

Найлепшы “AI” use-case тут — не навучаць сабаку хадзіць у карыстальніка, а адзін раз дапамагчы artist-у з rig proposal, mesh registration і corrective discovery. Фінальны bundle павінен быць звычайным deterministic mesh/rig/morph/clip кантэнтам.

---

## 5. Рэкамендаваная архітэктура: Canine Forge

```text
CanineGenome + Life history
          │
          ▼
 bounded phenotype weights ──► constraint projection
          │
          ▼
 canonical mesh ──► shape morphs ──► pose correctives ──► skinning
          │                                                   │
          ├─► semantic coat fields ─► palette/masks/fur tier  │
          │                                                   ▼
          └──────── stable UV / fur flow ─────────────── realtime dog

LifeCore/VITA ─► semantic intent ─► clips + gait phase + IK + additives
                  ▲                                      │
                  └────────────── BodyFeedback ◄─────────┘
```

Галоўнае правіла: **эвалюцыя змяняе параметры стабільнага арганізма, а не перабудоўвае mesh выпадкова.**

### 5.1 Canonical mesh і skeleton

Пачатковая production-мэта:

- адзін watertight dog mesh са стабільнымі vertex IDs/UV;
- 3 phenotype anchors: `sighthound`, `spitz/collie`, `mastiff/terrier`; пасля можна дадаць puppy;
- 35–43 deform bones: pelvis/spine/chest/neck/head/jaw, limbs/paws/toes, tail chain, ears, eyes; control rig застаецца ў Blender;
- 12–20 runtime shape targets, а не 63 сырыя sliders;
- LOD0 мэтава 8–15k vertices / 16–30k triangles, меншыя LOD для tiny desktop size;
- не больш за 4 skin influences на vertex;
- асобныя eyes, cornea/eye card, teeth/tongue і nose, але з invariant anchors;
- glTF 2.0 як нейтральны runtime contract, калі exporter захоўвае патрэбныя morphs/skins/animations.

Лічбы — пачатковыя бюджэты для spike, не догма. Іх трэба скараціць пасля GPU capture на рэальным target hardware.

### 5.2 CanineGenome: не sliders, а семантычная праграма формы

Прапанаваныя genes:

- `body_size`, `leg_length`, `torso_length`, `torso_depth`;
- `chest_width`, `pelvis_width`, `neck_length/thickness`;
- `skull_width/dome`, `muzzle_length/depth`;
- `ear_archetype`, `ear_size`, `ear_drop`;
- `tail_length`, `tail_curl`, `tail_plume`;
- `paw_scale`, `fat`, `muscle`, `age/puppy`;
- `fur_length`, `fur_density`, `ruff`, `feathering`;
- palette, marking grammar і seed.

У genome няма прамога “vertex 1842 += noise”. Ён ператвараецца ў phenotype-anchor weights і невялікія residual morphs.

Адзін варыянт:

```text
shape(g) = neutral
         + Σ softmax(anchor_logits(g))[i] · anchor_delta[i]
         + Σ bounded_axis(g)[j] · semantic_delta[j]
         + Σ corrective(i,j) · weight[i] · weight[j]
```

Пасля — constraint projection. Гэта аналаг constrained cage/RBF deformation: дастаткова працэдурны, каб расці, але значна больш кіраваны за noise або runtime remesh.

Важны парадак: спачатку genome будуе **rest joints і bone lengths**, потым anatomy/cage field дэфармуе surface вакол новага skeleton, пасля дадаюцца малыя owned corrective bases. Інакш mesh выцягнецца, а плечы, hips і IK anchors застануцца ў старой сабацы.

```text
J_rest(g) = semantic_skeleton(g)
V_rest(g) = V0 + D_anatomy(V0, J_rest(g), g) + Σ corrective_weight(g)[i] · B[i]
V_pose    = LBS_or_DQS(V_rest, J_rest, pose) + pose_correctives
```

### 5.3 Invariants: аўтаматычная абарона ад “уродлівай сабакі”

Кожная генерацыя/метамарфоза павінна праверыць:

- вочы, зрэнкі, nose і teeth застаюцца ў сваіх sockets;
- павекі і рот закрываюцца;
- локці/калені/скакальныя суставы маюць дапушчальны range і clearance;
- лапы дасягаюць зямлі, toe direction не пераварочваецца;
- chest/pelvis не перасякаюць legs пры стандартных позах;
- tail base і ears не адрываюцца;
- аб'ём тулава не калапсуе;
- skin stretch/compression і corrective residual не перавышаюць ліміт;
- UV seams і fur-flow не ламаюцца;
- усе float-ы finite, normals/tangents валідныя.

Няўдалы genome не “спрабуе шчасце” ў renderer. Ён праецыруецца ў бліжэйшую валідную вобласць або вяртаецца да бацькоўскай формы.

### 5.4 Як рабіць эвалюцыю бачнай і прыгожай

Не мяняць 20 параметраў кожную хвіліну. Эвалюцыя павінна мець рытм:

1. LifeCore назапашвае павольныя traits/history.
2. У момант metamorph генеруецца target genome і праходзіць validity tests.
3. Пераход пачынаецца толькі ў бяспечнай позе: stand/sleep, лапы зафіксаваныя.
4. За 2–5 секунд мяняюцца 1–3 групы: напрыклад ears + coat marking + ruff.
5. Morph uses ease curve; corrective targets і coat masks ідуць сінхронна.
6. Новы genome захоўваецца як lineage event; адкат магчымы.

Так карыстальнік разумее, **што** вырасла і **чаму**, а не бачыць shader glitch.

---

## 6. Працэдурная поўсць без мільёнаў валасоў

Поўсць павінна быць трыма незалежнымі пластамі.

### Layer A: добрая скура/кароткі ворс

- opaque PBR base;
- anisotropic/fuzz response па fur-flow map;
- low-frequency normal breakup;
- rim толькі як фізічны fuzz, не neon outline;
- самая танная і заўсёды ўключаная якасць.

### Layer B: selective shell fur

- 6–10 shells толькі там, дзе патрэбны аб'ём, а не на ўсім LOD0;
- mask для ruff/chest/back/thighs;
- dither/alpha-to-coverage, калі target backend гэта дазваляе;
- depth-aware fade на малым экранным памеры;
- адключэнне shells ніжэй зададзенага pixel footprint.

### Layer C: silhouette cards

- рэдкія cards на вушах, грудзях, жываце, задніх лапах і хвасце;
- размяшчэнне і кірунак генеруюцца ў Blender з guide curves;
- некалькі atlas-варыянтаў і seeded density;
- cards важней за fibers, бо менавіта яны мяняюць сілуэт.

Свежы research падтрымлівае менавіта падзел body/fur: [NeuralFur (3DV 2026)](https://github.com/Vanessik/NeuralFur) аднаўляе defurred surface і surface-rooted neural guide strands, а [AnimalLift dataset/research (2026)](https://huggingface.co/datasets/Chunyi99/AnimalLift) выкарыстоўвае shared topology/UV і UV-local compressed fur maps. Абодва варыянты не production dependency: NeuralFur — CC BY-NC-SA/offline, AnimalLift — каля 318 GB з `license: other`. Але іх прадстаўленне пацвярджае наш таннейшы дызайн: стабільная скура + UV length/direction/density + cards/shells.

### Coat grammar: хітры layering, які сапраўды варты працы

Не генерыраваць RGB-noise. Генерыраваць **анатамічныя палі** ў canonical/body space:

- dorsal ↔ ventral gradient;
- muzzle/eye/ear masks;
- socks па geodesic distance ад лап;
- blaze па bilateral head coordinates;
- saddle па spine coordinate;
- patches з нізкачастотных warped fields;
- tips/salt-and-pepper, звязаныя з age і fur length;
- 2–4 колеры з абмежаванай palette harmony.

Парадак:

```text
base palette
→ dorsal/ventral structure
→ breed-like markings
→ seeded patches
→ age/history accents
→ fur-direction microbreakup
```

Гэта дае тысячы сабак, якія выглядаюць намерна, а не як noise demo. Low-frequency палі можна лічыць у runtime; final masks/cards лепш bake/cache пры metamorph.

### Alpha/compositing для desktop overlay

Правяраць сабаку мінімум на чорным, белым, checkerboard і рэальным desktop. Патрэбны premultiplied-alpha contract, правільнае edge dilation для atlas і адсутнасць светлага/чорнага halo. Fur tier павінен gracefully спадаць да opaque base, калі overlay/backend не трымае празрыстасць.

---

## 7. Рух: навучаць характар, а не калені

Поўнае online RL цела на камп'ютары карыстальніка тут не трэба. Яно будзе менш стабільным, цяжэй тэсціцца і часцей выглядаць як баг.

[Spore animation research](https://chrishecker.com/How_To_Animate_a_Character_You%27ve_Never_Seen_Before) паказаў патрэбную ідэю: motion захоўваецца morphology-independent і адаптуецца да target structure праз procedural rules і IK. Нам прасцей за Spore, бо topology і quadruped skeleton фіксаваныя.

[Google Research: Toward Believable Acting for Autonomous Animated Characters](https://research.google/pubs/toward-believable-acting-for-autonomous-animated-characters/) асабліва добра адпавядае Pet 2: “мозг” выдае action/emotion/attention, а procedural animation system ператварае гэта ў выразны рух quadruped character.

Прапанаваны stack:

1. authored clips: idle, walk, trot, run, sit, lie, sleep, sniff, scratch, shake, turn;
2. morphology-aware stride/hip/spine scaling;
3. planted-paw IK + floor/contact solver;
4. phase-matched transitions;
5. additive breathing, head aim, ears, tail, gaze;
6. emotion/personality modifiers на speed, amplitude, latency, posture;
7. procedural step/turn толькі пасля стабільнага clip baseline.

LifeCore/VITA працягвае вучыцца, **калі, куды і з якім характарам** рухацца. Яно не павінна спрабаваць самастойна адкрыць біямеханіку кожнага сустава.

[Wobbledogs postmortem](https://www.gamedeveloper.com/design/behind-the-ai-and-physics-of-i-wobbledogs-i-procedurally-goofy-wobbledogs) — карысны контрпрыклад: складаныя скрытыя learned/physics-механізмы лёгка чытаюцца як выпадковасць або паломка, а full-physics locomotion моцна ўскладняе прадказальнае AI. У Pet 2 працэдурнасць лепш укласці ў форму, coat і expressive layers.

---

## 8. Як гэта падключаецца да цяперашняга Pet 2

Існуючы `LifeCore/VITA`, sensor stack, persistence, desktop topology, `BodyIntent` і `BodyFeedback` трэба захаваць. Новая сабака — іншы presentation/body backend.

Гэта не asset swap у цяперашні body:

- [`pet_body/src/graph.rs`](./crates/pet_body/src/graph.rs) апісвае невялікі node graph з torso/head/eyes/wings/forelimbs/tail, а не canine skeleton з чатырма поўнымі limb chains;
- [`pet_body/src/mesh.rs`](./crates/pet_body/src/mesh.rs) цяпер мае position/normal/color/part, але не UV, joint indices і skin weights;
- цяперашняя animation — node/spring dynamics, а morph — liquid/polar deformation, не anatomy space.

Таму чыстая мяжа — асобны `DogBodyBackend`, пакуль цяперашні body застаецца fallback. Гэта ізалюе найбольш рызыкоўную частку і не ламае LifeCore.

```text
LifeCore/VITA
  └─ BodyIntent
       locomotion, target, attention, arousal, valence,
       curiosity, sleepiness, morph_event
            └─ CanineController
                 clip state + IK + additive expression
                      └─ CanineRenderer
                           skin + morph + coat + fur + alpha
                                └─ BodyFeedback
                                     reached, blocked, grounded,
                                     contact, animation_event, stress
```

### Два этапы замест вялікага rewrite

**A. 72-гадзінны art/tech spike ў Blender + простым realtime viewer.** Мэта — выбраць форму і representation, а не перарабіць увесь app.

**B. Толькі пасля PASS — `canine` backend у Rust/wgpu** побач з цяперашнім liquid body. Ён чытае baked bundle і рэалізуе skin/morph/material/animation. Brain API не змяняецца.

Unity можа быць хуткім viewer для Malbers/Daz baseline, але не павінен аўтаматычна стаць новай архітэктурай усяго прадукту. Калі Blender preview ужо забівае look, нават viewer не патрэбен.

---

## 9. Адзін frontier-варыянт: implicit/SDF canine

Гэта адзіны радыкальны experiment, які лагічна працягвае цяперашнюю liquid/implicit спадчыну Pet 2:

```text
semantic skeleton
→ capsules / ellipsoids / muscle volumes
→ smooth-union SDF
→ constrained surface extraction
→ primitive-influence skin weights
→ triplanar/anatomy-space coat
```

Плюсы:

- рэальна працэдурная будова;
- вельмі шырокая прастора формаў;
- deterministic genome;
- няма залежнасці ад чужога breed mesh;
- можа даць непаўторны “spirit dog” выгляд.

Мінусы:

- натуральны canine face і paws вельмі цяжкія;
- smooth union робіць anatomy blobby;
- runtime remesh пагражае topology, UV, animation і fur cards;
- добрыя deformation weights і correctives могуць з'есці ўвесь выйгрыш;
- патрэбны separate facial parts і моцны sculptural constraint system.

**Bounded test:** 8 гадзін на standing silhouette з 12–16 primitives і двума крайнімі genomes. Калі абедзве формы не чытаюцца як прыгожыя сабакі на 256 px — спыніць. SDF можна пакінуць для aura/undercoat/metamorph VFX, нават калі ён не стане целам.

---

## 10. 72-гадзінны discriminating spike

Перад ім ёсць яшчэ таннейшы **48-гадзінны asset gate**: чатыры Poly Art variants (adult, puppy, два silhouette extremes), 12 clips, 128/192/256 px, чорны/белы/busy desktop. Pass: твар чытаецца каля 160 px, усе варыянты адрозныя ў grayscale silhouette, лапы не слізгаюць, edges чыстыя, trimmed mesh/atlas/animations арыенціровачна ўкладваюцца ў 20–30 MB. Fail не азначае “купіць іншую stock dog”; ён адразу пасылае нас у custom DOGWALK-inspired sculpt.

### Пытанне, якое ён павінен закрыць

> Ці можа адзін canonical dog з адным rig даць тры адразу розныя, прыгожыя і анатамічна праўдзівыя phenotype на 256–512 px, з працэдурнымі coat-амі, адной хадой без слізгання лап і без alpha-артафактаў?

### Baselines

- PIDI — rejected negative baseline;
- Malbers Skye Poly Art adult/puppy/morph — tiny-screen character baseline, калі ёсць доступ;
- Malbers realistic Skye — realism baseline, калі ён наогул патрэбны;
- Daz Dog 8 neutral + Phenotypes або бясплатны placeholder — generator baseline.

Нічога не купляць да асобнага рашэння карыстальніка.

### 0–12 гадзін: taste lock

- адзін moodboard і art thesis;
- тры silhouette anchors;
- target screenshots на 128/256/512 px;
- праверка ліцэнзій і export path;
- brutally delete generic options.

### 12–30 гадзін: body

- canonical mesh/retopo;
- 10–16 curated semantic axes;
- тры phenotype anchors;
- simple quadruped deform rig;
- invariants для eyes/nose/mouth/paws і 10 стандартных poses.

### 30–44 гадзіны: coat/fur

- anatomy-space coat grammar;
- тры palettes і мінімум шэсць marking seeds;
- quality tiers: opaque fuzz / selective shells / cards;
- black-white-checker-desktop alpha test.

### 44–58 гадзін: motion

- stand, idle, walk, trot, sit, lie, sniff, turn;
- foot contacts і basic IK;
- morph extremes на тым жа clip set;
- ears/tail/gaze additives.

### 58–72 гадзіны: brutal comparison

- 3 phenotype × 3 coat × 3 sizes;
- 100–1000 seeded genomes праз automated validity checks;
- 30-second loop на desktop;
- blind preference test супраць PIDI/stock baseline;
- GPU capture і memory snapshot;
- рашэнне `PASS / REDUCE / KILL`.

### Метрыкі

**Taste/readability**

- 8/10 людзей выбіраюць Canine Forge над PIDI на 256 px;
- 3/3 silhouettes адрозніваюцца без тэкстуры;
- мінімум 90% правільна чытаюць “сабака” на 64–96 px silhouette;
- вочы/нос/вушы застаюцца галоўным focal cluster.

**Morph validity**

- 1000 seeds: 0 NaN, inverted transforms, detached eyes/ears/tail;
- 0 відавочных self-intersections у 10 canonical poses для прынятых seeds;
- invalid genome дэтэрмінавана clamp/project/reject;
- адзін rig і адзін clip set для ўсіх трох anchors.

**Motion**

- paw slip менш за 2% body length за planted phase;
- няма hyperextension і floor penetration на target desktop terrain;
- пры morph transition contact paws не рухаюцца больш за 2 px на 512 px output.

**Render**

- halo не шырэй за 1 px на чорным/белым/checker;
- fur tier укладаецца ў 25% агульнага frame budget;
- пры маленькім pixel footprint shells/cards адключаюцца без бачнага pop;
- target framerate трымаецца на выбраным low-end iGPU, не толькі на dev GPU.

### Kill criteria і fallback

- **Daz/Fab license або export не падыходзіць:** не абыходзіць; commissioned/custom mesh, DOGWALK placeholder або іншы чысты source.
- **Extreme morph патрабуе асобнага rig/weights:** скараціць прастору да трох anchors + невялікіх residual axes.
- **Адна topology дае больш за 5% anatomy failures:** перайсці да трох topology-compatible rest bases (`long`, `medium`, `compact`) з аднолькавымі bone names, UV semantics і clip library; не прымушаць адну сетку быць і dachshund, і great dane.
- **Coat выглядае noise:** пакінуць authored anatomy masks, seed выкарыстоўваць толькі для другаснага breakup.
- **Shell fur дае alpha/overdraw:** opaque fuzz + cards; shells толькі high tier або выдаліць.
- **SDF dog blobby праз 8 гадзін:** забіць як body, пакінуць толькі metamorph/aura VFX.
- **Рэалізм generic/uncanny пасля taste lock:** перайсці ў stylized-real/DOGWALK material language, не купляць трэцюю stock dog.
- **Brain integration пачынае кіраваць joints:** вярнуць мяжу `semantic intent → controller`; learning застаецца на ўзроўні паводзін.

### Production decision пасля research

**ADOPT** representation: shared topology + constrained morph basis + procedural coat + layered animation.
**TRIAL** Skye Poly Art як 48-гадзіннае proof chassis, не фінальны moat.
**ASSESS** Daz Dog 8 толькі як anatomy/morph lab і custom DOGWALK-inspired mesh як production identity.
**HOLD** SMAL/BITE/3DGS photo-to-dog.
**EXPERIMENT** SDF dog толькі 8 гадзін.
**REJECT** PIDI як art direction і full online learned locomotion для v1.

---

## 11. Мінімальны спіс блокаў

### Authoring

- Blender;
- canonical dog mesh: спачатку Daz/ліцэнзаваны або clean custom;
- Rigify/custom quadruped control rig → export deform rig;
- shape-key naming/schema;
- Geometry Nodes для hair guides/cards/masks;
- automated Blender validation script;
- glTF export profile.

### Runtime

- skinned mesh;
- 12–20 morph targets + pose correctives;
- animation graph/crossfade/phase sync;
- paw IK/contact;
- anatomy-space coat shader або cached masks;
- opaque fuzz + optional shells/cards;
- premultiplied-alpha desktop compositor;
- genome/preset serializer;
- `BodyIntent`/`BodyFeedback` adapter.

### Кантэнт для first pass

- 1 neutral mesh;
- 3 phenotype anchors;
- 8–12 clips;
- 3 palettes × 6 marking seeds;
- 3 fur profiles: short, medium, feathered;
- 3 LOD/quality tiers;
- 10 validation poses;
- screenshot/performance harness.

---

## 12. Што я б зрабіў заўтра

1. Не купляў бы PIDI і не пачынаў runtime rewrite.
2. Узяў бы тры reference silhouettes: sighthound, collie/spitz, compact mastiff/terrier.
3. Для вельмі хуткага proof праверыў бы Skye Poly Art; паралельна папрасіў бы character artist зрабіць адзін прыгожы DOGWALK-inspired neutral canine і тры anchor sculpts. Daz браў бы толькі для anatomy-range experiment.
4. Звёў бы ўсе магчымыя sliders да 12–16 семантычных axes.
5. За адзін дзень зрабіў бы 3×3 still/render comparison на 128/256/512 px.
6. Толькі калі ўсе тры формы ўжо прыгожыя без motion і fur — дадаў бы rig, coat і animation.
7. Пасля PASS пачаў бы `canine` backend у цяперашнім Rust/wgpu Pet 2.

Гэта самы кароткі шлях не да “тэхнічна існуючай сабакі”, а да сабакі, якую хочацца пакінуць на працоўным стале.

---

## Асноўныя крыніцы

### Production assets / tools / licenses

- [Daz Dog 8](https://www.daz3d.com/daz-dog-8)
- [Phenotypes for Dog 8](https://www.daz3d.com/phenotypes-for-dog-8)
- [Skye Poly Art — current Fab listing](https://www.fab.com/listings/a7cd32af-d039-4386-b8fc-db9b96fefc27)
- [Daz to Blender README](https://github.com/daz3d/DazToBlender/blob/master/README.md)
- [Daz Interactive License / EULA](https://www.daz3d.com/eula)
- [Malbers Skye — current Fab listing](https://www.fab.com/listings/fa25a2b9-0178-4805-97d9-5caeb5e2e82f)
- [Fab Standard License](https://www.fab.com/eula)
- [Blender Rigify manual](https://docs.blender.org/manual/en/latest/addons/rigging/rigify/basics.html)
- [Blender Geometry Nodes](https://docs.blender.org/manual/en/latest/editors/geometry_node.html)
- [Blender Generate Hair Curves](https://docs.blender.org/manual/en/4.4/modeling/geometry_nodes/hair/generation/generate_hair_curves.html)
- [Blender Studio DOGWALK](https://studio.blender.org/projects/dogwalk/)

### Parametric dogs / reconstruction

- [BARC, CVPR 2022](https://openaccess.thecvf.com/content/CVPR2022/html/Ruegg_BARC_Learning_To_Regress_3D_Dog_Shape_From_Images_by_CVPR_2022_paper.html)
- [BITE, CVPR 2023](https://openaccess.thecvf.com/content/CVPR2023/html/Ruegg_BITE_Beyond_Priors_for_Improved_Three-D_Dog_Pose_Estimation_CVPR_2023_paper.html)
- [RGBT-Dog, WACV 2024](https://openaccess.thecvf.com/content/WACV2024/html/Deane_RGBT-Dog_A_Parametric_Model_and_Pose_Prior_for_Canine_Body_WACV_2024_paper.html)
- [Animal Avatar, ECCV 2024](https://remysabathier.github.io/animalavatar.github.io/index.html)
- [DogRecon, IJCV 2025](https://vision3d-lab.github.io/dogrecon/)
- [SMAL-pets, 2026](https://piotr310100.github.io/SMAL-pets/)
- [CORGI, 2026](https://dzzzby.github.io/CORGI/)
- [4D-Animal, WACV 2026 code](https://github.com/zhongshsh/4D-Animal)
- [GART, CVPR 2024](https://github.com/JiahuiLei/GART)
- [WildAni4D, CVPR 2026](https://vision3d-lab.github.io/wildani4d/)
- [AniGauss, CVPRW 2026 draft](https://vision3d-lab.github.io/anigauss/)
- [BITE/D-SMAL license](https://raw.githubusercontent.com/runa91/bite_release/master/LICENSE)
- [SMALify](https://github.com/benjiebob/SMALify)
- [AniGen, SIGGRAPH 2026](https://github.com/VAST-AI-Research/AniGen)
- [UniRig, SIGGRAPH 2025](https://github.com/VAST-AI-Research/UniRig)
- [Puppeteer, NeurIPS 2025](https://github.com/Seed3D/Puppeteer)
- [NeuralFur, 3DV 2026](https://github.com/Vanessik/NeuralFur)
- [AnimalLift dataset/research, 2026](https://huggingface.co/datasets/Chunyi99/AnimalLift)

### Procedural character lessons

- [Spore: How to Animate a Character You've Never Seen Before](https://chrishecker.com/How_To_Animate_a_Character_You%27ve_Never_Seen_Before)
- [Spore procedural texturing course material](https://www.cs.cmu.edu/~ajw/s2007/)
- [Google Research: believable acting for autonomous animated characters](https://research.google/pubs/toward-believable-acting-for-autonomous-animated-characters/)
- [Wobbledogs AI/physics postmortem](https://www.gamedeveloper.com/design/behind-the-ai-and-physics-of-i-wobbledogs-i-procedurally-goofy-wobbledogs)
