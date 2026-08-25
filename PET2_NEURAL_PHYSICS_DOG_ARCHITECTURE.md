# Pet 2: нейрасабака з працэдурным фізічным целам

Дата праверкі: 2026-08-24
Статус: architecture decision + falsifiable research spike
Удакладняе і замяняе clip-first motion recommendation у `PET2_PROCEDURAL_CANINE_DEEP_RESEARCH.md`.

## Кароткі вердыкт

**Так: Skye Poly Art павінна быць толькі візуальнай скурай. Пад ёй будуем уласную працэдурную active-ragdoll сабаку. Бяспечны gait/IK controller дае базавую позу, а маленькая neural motor policy вучыцца дадаваць bounded residual joint targets праз PD-“мышцы”. Яна не кіруе vertices і не прайграе animation clips.**

Фізіка ідзе на 120 Hz, policy — на 30 Hz, render — з інтэрпаляцыяй. `LifeCore/VITA` застаецца вышэйшым мозгам і кажа “хачу ісці сюды / нюхаць / гуляць / баяцца”; `Motor Cortex` сам вырашае, як перастаўляць лапы, трымаць баланс і аднаўляцца пасля штуршка.

Важная мяжа: [Skye Poly Art](https://www.fab.com/listings/a7cd32af-d039-4386-b8fc-db9b96fefc27) мае `Allows usage with AI: No`. Яе можна выкарыстоўваць як runtime mesh/rig/material і дэтэрмінавана рэтаргеціць у яе pose, але не карміць яе mesh/анімацыі/кадры ў training pipeline. Physical dog і training corpus павінны быць нашымі або rights-clean.

---

## 1. Што менавіта значыць “нейрасабака”

Не LLM унутры сабакі і не generator, які кожны кадр выдумляе анімацыю.

```text
LifeCore / VITA                    “Навошта і куды”
        │
        ▼
Motor Command                     velocity, heading, stance, attention,
        │                          affect/style, interaction target
        ▼
Procedural gait + Neural Motor Cortex · 30 Hz
        │                          14 bounded joint targets
        ▼
PD muscles · 120 Hz               torque = Kp·pose_error − Kd·joint_velocity
        │
        ▼
Procedural Active Body · 120 Hz   mass, inertia, joints, limits, contacts
        │                   │
        │                   └──► proprioception + contact + stability
        ▼                                      │
Poly Art skin + secondary motion               ▼
                                      Motor Memory / adaptation
```

Гэта **neural physics-based character controller**. У адрозненне ад kinematic clip:

- лапа сапраўды нясе вагу;
- цела рэагуе на штуршок і слізкую паверхню;
- змененыя прапорцыі мяняюць stride, баланс і энергію;
- сабака можа спатыкнуцца, пераставіць лапу і аднавіцца;
- адзін policy можа кіраваць сям'ёй падобных morphology, калі бачыць genome;
- вушы, хвост, ruff і мяккае цела фізічна дагульваюць motion.

## Signature moment

Сабака засынае і праходзіць metamorph: лапы становяцца даўжэйшыя, грудзі цяжэйшыя, хвост пышнейшы. Пасля абуджэння яна робіць некалькі асцярожных крокаў, ловіць новы цэнтр масы, а затым яе хада становіцца ўпэўненай. Эвалюцыя бачная не толькі ў mesh — **мозг навучыўся жыць у новым целе**.

---

## 2. Пяць выразна асобных уладальнікаў стану

| Слой | Аўтарытэтны стан | Што не мае права рабіць |
|---|---|---|
| `LifeCore/VITA` | патрэбы, мэты, эмоцыі, памяць, выбар дзеяння | не піша joint angles і torques |
| `Motor Cortex` | policy state/history і bounded joint-target action | не выбірае жыццёвыя мэты і не рухае render bones наўпрост |
| `Motor Memory` | morphology/environment context, safe adapter, gait calibration | не перапісвае base policy у live-сеансе |
| `Active Body` | rigid bodies, joints, contacts, COM, velocity, physical truth | не захоўвае эмоцыі і не прыдумляе анімацыю |
| `Poly Skin` | visual rig, skinning, corrective shapes, coat/fur | не валодае collision, locomotion success або gameplay state |

Гэта добра кладзецца на існуючы кантракт Pet 2: `BodyIntent` ужо з'яўляецца семантычнай камандай, а `BodyFeedback` — адказам цела. Новы motor layer устаўляецца паміж імі, не замяняючы LifeCore.

---

## 3. Працэдурнае фізічнае цела

### 3.1 Два skeleton-ы, не адзін

1. **Physical skeleton** — маленькі, стабільны, training/runtime-owned: colliders, mass, constraints, motors.
2. **Visual skeleton** — Skye Poly Art або будучы custom dog: больш костак для deformation, ears, tail, face і fur.

Physical pose пераносіцца ў visual rig праз rest-space mapping:

```text
visual_world(bone) = physics_world(link)
                   · inverse(physics_rest(link))
                   · visual_rest(bone)
                   · authored_twist_or_corrective
```

Так policy ніколі не залежыць ад proprietary vertex data або назваў 298 animations.

### 3.2 Першы physical body budget

**Spike:** 11 anatomical/collidable links — pelvis, chest, head і upper/lower segment × 4 legs. Foot — contact geometry на lower segment, а не асобнае collidable link. Першыя 12 гадзін працуюць толькі 12 leg DOFs: на кожнай лапе hip/shoulder abduction, hip/shoulder flexion і elbow/stifle flexion.

**V1 пасля стабільнага spike:** 11–15 anatomical/collidable links і **14 controlled DOFs** — 12 leg DOFs + spine pitch/yaw. Фактычная колькасць solver bodies можа быць большай: MJCF/Rapier importer стварае intermediate bodies, калі адзін anatomical link мае некалькі hinge axes. Асобныя paws, neck link і 1–3 passive tail links дадаюцца толькі калі не ламаюць budget і transfer.

Агульныя правілы:

- tree articulation без loop constraints;
- capsules/spheres/boxes, не triangle-mesh colliders;
- 4 foot contact sensors;
- head, tail і ears не ўваходзяць у першы neural action space;
- ears, cheeks, ruff і belly — spring/XPBD presentation-only proxies.

Не трэба фізічна сімуляваць усе 35–43 visual bones. Neural policy кіруе толькі тымі degree-of-freedom, якія нясуць вагу і ствараюць сілуэт руху.

### 3.3 Genome → body builder

`DogPhysicsGenome` генеруе:

- bone/rest lengths;
- shoulder/hip width і position;
- collider radii і body volume;
- segment density → mass;
- mass distribution і COM;
- inertia tensor з collider geometry;
- joint axes, limits і soft-limit margins;
- motor strength, stiffness і damping;
- paw area/friction/compliance;
- spine/tail/ear spring profiles.

Парадак абавязковы:

```text
semantic genome
→ valid rest skeleton
→ colliders and volumes
→ mass + COM + inertia
→ joint limits
→ motor strength/gains
→ visual morph targets
→ policy morphology observation
```

Нельга проста зрабіць лапу ў 1.5× даўжэйшай і пакінуць старую масу, torque limit ды center of mass.

### 3.4 Фізічныя scaling rules

Пачатковыя, пасля калібруюцца:

- mass прыкладна маштабуецца з аб'ёмам;
- inertia пералічваецца з рэальнай collider shape;
- даступны torque расце павольней за mass, каб вялікае цела адчувалася цяжэйшым;
- foot area і friction bounded асобна, каб вялікая лапа не стала магнітам;
- joint speed limit памяншаецца для цяжкіх/даўгіх сегментаў;
- PD gains нармалізуюцца па inertia і fixed timestep;
- extreme morphology праходзіць validity projection або пераключаецца на асобны expert/base.

### 3.5 Invariants

- усе lengths/masses/inertias finite і positive;
- joint axes normalized, rest pose унутры soft limits;
- colliders не перасякаюцца ў neutral stand;
- paw colliders ніжэй за limb colliders і дасягаюць ground;
- COM projection у neutral stand унутры support polygon;
- torque/impulse/energy маюць hard clamps;
- solver не атрымлівае topology changes падчас active contact;
- physical і visual joint maps versioned і complete;
- адзін seed дае адзін і той жа body hash.

---

## 4. Neural Motor Cortex

### 4.1 Action space: target pose, не raw torque

Procedural gait/IK выдае стабільны reference, а policy — толькі bounded residual. Для spike выкарыстоўваем stock Gaussian PPO з linear mean і **адным** explicit clamp, не double-`tanh`:

```text
u        = policy(observation)              # sample train / mean runtime
a        = clamp(u, -1, +1)
q_target = q_reference(command, phase, morphology) + action_scale · a

motor ≈ Kp · shortest_angle(q_target − q) − Kd · q_velocity
        with explicit force/torque limit
```

Нулявы neural output дакладна вяртае працэдурны controller. Гэта і аварыйны fallback, і baseline для доказу, што сетка сапраўды нешта палепшыла.

Чаму не raw torque ў v1:

- цяжэй трэніраваць і пераносіць паміж engines;
- адна памылка policy імгненна ўводзіць энергію ў solver;
- складаней гарантаваць joint limits і recoverability;
- target-angle policy + PD ужо дае рэальную physical response, але значна лепшую art control.

Гэта падыход, які выкарыстоўвае [DeepMimic](https://xbpeng.github.io/projects/DeepMimic/index.html): neural controller задае target joint orientations, а PD ператварае іх у фізічнае ўздзеянне.

### 4.2 Частоты

- асобны dog constant: **`DOG_DT = 1/120 s`**; ён не залежыць ад liquid tuning, які цяпер можа быць 30–120 Hz;
- PD motors і contacts: кожны physics tick;
- neural policy: **30 Hz** — адзін action на 4 physics ticks;
- baseline толькі запісвае proprioception history; feed-forward actor яго не чытае;
- LifeCore/VITA: яго цяперашні павольны semantic cadence;
- render: інтэрпаляцыя паміж двума physics snapshots.

Spike выкарыстоўвае **zero-order hold**: action трымаецца роўна 4 physics ticks і ў mjlab, і ў Rapier. Інтэрпаляцыя — толькі пазнейшы paired A/B. Нельга запускаць inference на render cadence: 60/75/120 Hz monitor не павінен мяняць характар хады.

### 4.3 Observation vector

Паколькі action — residual адносна dynamic reference, actor мусіць бачыць і reference, і gait phase. Першы actor мае **86 FP32 scalars**:

| Блок | Памер |
|---|---:|
| root linear velocity у body space | 3 |
| root angular velocity | 3 |
| projected gravity | 3 |
| command: surface-local `v_u`, `v_v`, `yaw_rate` | 3 |
| joint position relative to rest | 14 |
| joint velocity | 14 |
| previous action | 14 |
| foot contacts | 4 |
| morphology vector | 12 |
| `q_reference − q_rest` | 14 |
| `sin(2π phase)`, `cos(2π phase)` | 2 |

12 morphology values: trunk length/height, shoulder/hip width, fore upper/lower length, hind upper/lower length, foot radius, total mass, normalized longitudinal COM і joint-range scale. Пачынаем з трох coherent anchors (`long/light`, `medium`, `compact/heavy`) і bounded interpolation каля ±15%, а не незалежна рандомізуем кожную костку.

Asymmetric critic падчас training можа бачыць яшчэ 19 privileged values: friction/restitution, COM jitter, mass/gain/torque randomization, external wrench і foot normal forces. Разам critic мае 105 scalars; у runtime privileged values няма.

Policy не атрымлівае desktop pixels або terrain scan у першым spike. Style/affect і head gaze спачатку мяняюць procedural reference/gait preset; пасля іх можна дадаць у actor толькі праз асобны A/B test.

### 4.4 Маленькая сетка

Першы actor: **86 → 256 → 256 → 14**, ELU і linear output: **91,662 parameters / 366,648 bytes FP32**. Адзіны clamp апісаны вышэй. Action scale: прыкладна 0.35 rad для ab/ad, 0.65 для hip flex/ext, 0.90 для elbow/stifle і 0.25 для spine. Дакладныя ліміты і gains належаць versioned body manifest.

Artifact: ONNX opset 18 + JSON manifest з joint order/axes/limits/rest pose/action scales/PD gains, observation order, rates, morphology bounds, training versions і SHA-256. `policy.onnx` прымае **raw ordered observations і сам валодае normalization**; manifest дублюе stats толькі для audit/hash, Rust не normalizes другі раз. Разам з artifact — 10,000 frozen raw observation/action golden pairs, якія правяраюць і raw model output, і clamp/reference/scale mapping.

RMA-style history encoder — **асобны phase-2 model**, не plugin да ўжо навучанага actor: base actor трэба нанова offline train-іць як 94-D input (86 + 8D hidden-dynamics latent), а encoder вучыць 20 actor frames ≈0.67 s → гэты latent. Ён не дублюе вядомую 12D morphology. У 72h spike RMA няма.

---

## 5. Як вучыць, каб рух быў сабачы, а не робат

### 5.1 Curriculum

1. **Stand:** баланс, neutral pose, small pushes.
2. **Recover:** бок/спіна/нізкі crouch → бяспечнае ўставанне.
3. **Velocity tracking:** наперад, паварот, stop.
4. **Contact grammar:** walk/trot/run phase relationships.
5. **Morphology randomization:** спачатку ±5–10%, потым phenotype envelope.
6. **Dynamics randomization:** mass, friction, PD gains, latency, small impulses.
7. **Interaction:** cursor push, moving ground proxy, screen edge/step.
8. **Style:** confidence/caution/playfulness і owned motion prior.

Не пачынаць ад “усе пароды + поўсць + desktop”. Спачатку debug-capsule dog павінна прыгожа стаяць, хадзіць і аднаўляцца.

### 5.2 Reward stack

```text
+ commanded velocity / heading tracking
+ upright and target body height
+ valid support / contact schedule
+ clean swing-foot clearance
+ recovery progress
+ symmetry where gait requires it
+ canine spine/head rhythm

− foot slip
− self-collision / ground penetration
− joint-limit pressure
− torque and power
− action jerk / pose jitter
− head shake / COM oscillation
− falling or solver explosion
```

Толькі velocity reward звычайна вырошчвае эфектыўнага чатырохногага робата, не сабаку. Таму патрэбны contact grammar і невялікі owned style prior.

[RMLL](https://ojs.aaai.org/index.php/ICAPS/article/view/31470) паказвае карысную альтэрнатыву вялікаму motion corpus: gaits можна задаць некалькімі лагічнымі правіламі над foot contacts і дазволіць policy знайсці фізічную рэалізацыю. Гэта асабліва падыходзіць нам, бо Skye animations забаронена выкарыстоўваць для AI training.

### 5.3 Motion priors — толькі rights-clean

Калі пазней з'явяцца ўласныя/замоўленыя dog clips, можна дадаць:

- DeepMimic-style pose/velocity imitation;
- [AMP](https://arxiv.org/abs/2104.02180) style discriminator на неструктураваных owned clips;
- асобныя priors для walk/trot/run/sit/recover.

Але першую фізічную хаду трэба ўмець навучыць без proprietary motion data: procedural gait phase + task rewards + ручныя pose anchors.

### 5.4 Morphology-conditioned policy

Адзін policy атрымлівае morphology vector і трэніруецца на curriculum of bodies. Гэта не спекуляцыя як клас: [McARL](https://arxiv.org/abs/2505.18418) і новыя universal morphology controllers паказваюць, што explicit morphology conditioning паляпшае transfer паміж quadruped embodiments.

Для Pet 2 envelope значна меншы за “мільёны робатаў”:

- 3 archetypes: `long/light`, `medium`, `compact/heavy`;
- bounded residual proportions;
- адзін skeleton schema і action semantics;
- калі extreme fails — mixture of 3 policies/experts з агульным API, а не бясконцыя correctives.

---

## 6. “Вучыцца ўсё лепей” без самазнішчэння

### 6.1 Тры ўзроўні learning

| Узровень | Што змяняецца | Дзе | Рызыка | Рашэнне |
|---|---|---|---|---|
| `L0 Fast Adaptation` | infer-нуты hidden dynamics context; weights не змяняюцца | live, кожны момант | нізкая | **TRIAL у phase 2** |
| `L1 Motor Calibration` | 8–24 bounded gait/PD/context parameters | live + persistence | сярэдняя | **TRIAL** |
| `L2 Neural Residual Adapter` | маленькая residual network/head | толькі headless dream sandbox | высокая | **ASSESS** |
| Base policy retraining | асноўныя weights | ніколі ў live app | катастрафічная | **REJECT для v1** |

[Rapid Motor Adaptation](https://ashish-kmr.github.io/rma-legged-robots/) дае моцную структуру для `L0`: base policy спецыяльна offline трэніруецца спажываць dynamics latent, а adaptation module выводзіць яго з кароткай гісторыі руху. Гэта live system identification/inference, **не persistent learning** і не gradient update. Latent трэба clamp-іць у training envelope, згладжваць EMA/rate limit, скідаць пры spawn/fall/teleport і адключаць watchdog-ам пры regression.

### 6.2 Motor Memory

У runtime захоўваюцца:

- body/genome hash;
- engine/policy/schema versions;
- estimated friction і damping context;
- stride length/frequency, foot clearance, posture bias;
- per-gait energy/stability statistics;
- асобны persistent L1 calibration vector; RMA L0 latent не захоўваецца;
- last known-good checkpoint;
- frozen validation scores.

Гэта сапраўднае навучанне: адна і тая ж сабака запамінае, якія рухі лепш працуюць у яе целе і асяроддзі. Але пошук абмежаваны маленькай правяральнай прасторай.

### 6.3 Dream training

Калі сабака спіць:

1. клонуецца яе physics genome і recent environment conditions;
2. headless simulator запускае шмат кароткіх rollout-аў;
3. small adapter/context vector мутуе або навучаецца;
4. кандыдат павінен выйграць на frozen validation suite, не толькі на адным эпізодзе;
5. правяраюцца falls, energy, slip, tracking, NaN і joint-limit violations;
6. толькі лепшы bounded adapter атамарна становіцца active;
7. пры любым regression — rollback.

Гэта прыгожая частка fiction і бяспечная engineering boundary: сабака “сніць”, але не эксперыментуе небяспечна ў visible desktop-сеансе.

### 6.4 Што можна вучыць ад карыстальніка

- preferred walking speed і distance;
- асцярожнасць каля cursor/windows;
- якой gait/style карыстальнік часцей узнагароджвае;
- recovery aggressiveness;
- timing stand/sit/approach;
- bounded motor adapter на канкрэтны genome.

Нельга выкарыстоўваць карыстальніцкі desktop image як training input motor policy. Для руху дастаткова абстрактных contacts/heightfields/events; гэта захоўвае privacy і спрашчае determinism.

---

## 7. Training і shipping stack

### Рэкамендаваны падзел

```text
OFFLINE TRAINING TOOL
  Python + GPU parallel simulator + PPO
  DogPhysicsGenome → canonical DogBodyDesc → generated MJCF
  domain/morphology randomization
  export policy + normalization + IO schema

SHIPPING PET 2
  Rust fixed-step physics
  small CPU neural inference
  visual retarget + wgpu skinning
  no Python, CUDA, simulator editor or training framework
```

**Рашэнне для першага spike:** [mjlab 1.6.0](https://github.com/mujocolab/mjlab/releases/tag/v1.6.0) + MuJoCo/MuJoCo Warp **3.11** + RSL-RL **5.4.2**, training на Ubuntu. [mjlab metadata](https://github.com/mujocolab/mjlab/blob/v1.6.0/pyproject.toml) мае Apache-2.0 і self-declared Production/Stable classifier; RSL-RL — BSD-3-Clause. Tagged [velocity task](https://github.com/mujocolab/mjlab/blob/v1.6.0/src/mjlab/tasks/velocity/velocity_env_cfg.py) ужо мае actor/critic plumbing, pushes, friction/COM randomization і locomotion rewards, а tagged [runner](https://github.com/mujocolab/mjlab/blob/v1.6.0/src/mjlab/rl/runner.py) экспартуе ONNX opset 18.

`~=3.11`, `torch>=2.7` і іншыя ranges самі па сабе нічога не pin-яць. Для run commit-ім `uv.lock`/container digest і exact Python, Torch build, CUDA/driver, GPU і repo commit. MuJoCo 3.12 выйшаў 2026-08-20, а яго [versioning policy](https://github.com/google-deepmind/mujoco/blob/main/VERSIONING.md) не абяцае numerical reproducibility паміж версіямі; upgrade — толькі праз новы smoke/parity run.

Альтэрнатывы, не default:

1. [Isaac Lab](https://github.com/isaac-sim/IsaacLab) — калі важней гатовы PhysX/Windows/NVIDIA workflow; цяпер ён цяжэйшы, beta-залежны і патрабуе вялікі Isaac Sim footprint.
2. [MuJoCo Playground](https://github.com/google-deepmind/mujoco_playground) — калі патрэбны JAX/Brax multi-backend research; JAX→ONNX shipping path менш прамы.

**Canonical source of truth — `DogPhysicsGenome → versioned DogBodyDesc`, не static MJCF.** З аднаго `DogBodyDesc` дэтэрмінавана генеруем MJCF для training і Rapier bodies/joints для runtime. `dog_v0.mjcf` — generated/reference artifact. Так эвалюцыйныя lengths/masses/inertias не канфліктуюць з “адным файлам”, а topology, axes, limits і rest pose маюць адзін semantic owner.

Stock mjlab `JointPositionActionCfg` робіць offset ад static default pose, не ад dynamic procedural gait. Таму spike патрабуе custom residual action term з **тым самым** `q_reference`/phase/clamp mapping, што Rust, і explicit `timestep=1/120`, `decimation=4` (120/30 Hz). Да запуску freeze-ім exact reward formulas/weights, command distribution, terminations і curriculum; пры mjlab `scale_rewards_by_dt=True` weights задаюцца як per-second rates і множацца на 1/30 s control step.

### Runtime physics

[Rapier 0.35.2](https://docs.rs/rapier3d/latest/rapier3d/) натуральна адпавядае цяперашняму Rust app: spherical/revolute/generic joints, `MultibodyJointSet`, rigid-body contacts і joint motors. [Joint motors](https://rapier.rs/docs/user_guides/templates/joints/) рэалізуюць position/velocity targets і maximum impulse. Rapier 0.33/0.34 таксама дадаў pure-Rust `rapier3d-mjcf`, per-DoF armature і MuJoCo-style springs ([changelog](https://github.com/dimforge/rapier/blob/master/CHANGELOG.md)).

Але importer — не semantic equivalence з MuJoCo: яго [official feature matrix](https://github.com/dimforge/rapier/blob/master/crates/rapier3d-mjcf/README.md) апісвае няпоўны `solref/solimp`, approximation для `frictionloss`, асаблівасці joint `ref` і reset generalized coordinates. Таму exact-pin `rapier3d = "=0.35.2"` і `rapier3d-mjcf = "=0.35.2"`, прымаць вузкі audited MJCF subset, bake-іць rest offsets і parity-test compiled `DogBodyDesc`. Main runtime лепш будаваць напрамую з `DogBodyDesc`; importer — oracle/tooling path.

| Runtime solver | Што добра | Чаму не default |
|---|---|---|
| **Rapier 0.35.2** | Pure Rust, current, multibody/motors, MJCF import | **ADOPT**; transfer усё роўна трэба мераць |
| [Jolt + `rolt`](https://github.com/SecondHalfGames/jolt-rust) | Моцны C++ engine | Rust wrapper сам называе сябе early/incomplete; native toolchain |
| [`physx-rs`](https://github.com/EmbarkStudios/physx-rs) | Mature underlying PhysX | Repo archived/read-only 2026-05-21, high-level API няпоўны, вялікі C++ build |
| Custom XPBD | Поўны control, карысна для tail/ears/soft coat | Не будаваць свой articulated contact solver для skeleton-а |

Рэкамендаваны baseline:

- Rapier `MultibodyJointSet` для tree dog як baseline, бо reduced coordinates не дазваляюць joint drift; `ImpulseJointSet` — толькі measured fallback;
- policy action → Rapier motor target/limit;
- explicit `MotorModel::ForceBased` у body manifest, каб mass/torque scaling і generated MJCF actuator semantics былі правяральнымі;
- асобны fixed `DOG_DT = 1/120`, не liquid timestep;
- per-step controller/solver/inference/total timers; catch-up count асобна;
- previous/current transforms + render alpha для сапраўднай snapshot interpolation;
- deterministic seeds/replay metadata, fixed insertion order і sorted contacts.

Multibody joints не даюць тых жа retrievable reaction forces, што impulse joints. Таму observability таксама ўваходзіць у іх A/B: motor energy ацэньваецца з вядомага commanded effort × joint velocity, а ўсе знешнія/anchor forces прымяняюцца і лагіруюцца explicit-ам. Exact replay азначае той жа target/build/features/initial values/insertion order; для cross-platform патрэбны `enhanced-determinism`, без `parallel`/explicit SIMD ([Rapier determinism](https://rapier.rs/docs/user_guides/templates/determinism)).

Галоўная рызыка — **sim-to-sim gap** паміж training physics і Rapier. Таму той жа policy абавязкова праганяецца на frozen cases у абодвух engines. Калі success падае больш за gate, ёсць два fallback:

1. моцней domain-randomize dynamics/contact;
2. асобны spike на batched Rapier training bridge; гэта не “хуткі fallback”.

### Runtime inference

Для 91,662-param dense MLP першы shipping path — **generated fixed-size Rust dense evaluator**. Ён не цягне backend, thread pool або native runtime. [tract 0.23.x](https://github.com/sonos/tract) застаецца ONNX import/parity reference і optional backend; яго default features могуць уключаць target-specific accelerators, таму “CPU single-thread/minimal” трэба даказваць exact pin + disabled defaults + package/RSS/thread audit. `ort` 2.x RC/native runtime — не default.

Golden parity gate параўноўвае PyTorch/ONNX, fixed Rust MLP і optional tract. Zero-residual procedural controller заўсёды застаецца безмадэльным fallback.

---

## 8. Інтэграцыя ў цяперашні Pet 2

### Што ўжо гатова

- [`app/src/main.rs`](./app/src/main.rs) мае body accumulator і cap backlog, але яго liquid timestep tune-іцца 30–120 Hz, а timer мерае ўвесь catch-up loop; dog патрабуе асобны `DOG_DT` і per-step metrics;
- [`lifecore/actions.rs`](./crates/lifecore/src/actions.rs) ужо падзяляе `BodyIntent` і `BodyFeedback`;
- [`lifecore/genome.rs`](./crates/lifecore/src/genome.rs) мае deterministic genome, identity seed, mutation і developmental plasticity;
- metamorphosis, persistence, desktop topology і sensor abstraction ужо існуюць;
- visual/render cadence ўжо аддзелены ад authoritative physics.

### Новыя межы, не канчатковыя імёны

```text
crates/pet_motor/
  policy.rs           model + normalization + inference
  observation.rs      versioned observation schema
  command.rs          BodyIntent → MotorCommand
  memory.rs           adapter/context/checkpoint
  safety.rs           watchdog + rollback

crates/pet_body/src/canine/
  genome.rs           DogPhysicsGenome projection
  backend.rs          solver-neutral DogPhysicsBackend
  articulation.rs     bodies/colliders/joints/motors
  rapier.rs           адзіны модуль з Rapier types
  contacts.rs         feet/support/slip
  retarget.rs         physical → visual bones
  secondary.rs        tail/ears/belly
  feedback.rs         physics → BodyFeedback

tools/canine_lab/
  training env/export/validation assets
```

Не рэалізоўваць гэтыя модулі да capsule-dog spike; назвы і dependency boundary павінны быць правераныя найменшым runnable path.

`DogPhysicsBackend` павінен мець толькі semantic/plain-math boundary: `rebuild`, `set_environment`, `teleport_root`, `step`, `snapshot`, `drain_contacts`. Rapier types не выходзяць з `rapier.rs`; public structs выкарыстоўваюць project `glam 0.30`/plain arrays, каб версія math dependency solver-а не заразіла ўвесь repo.

Над ім патрэбны outer avatar/body-runtime adapter, бо app цяпер наўпрост чытае liquid `ProceduralBody`: `fixed_step`, `feedback`, `projected_visual_bounds`, previous/current render pose, `presentation_update(alpha, dt)` і render payload. Screen geometry ператвараецца ў solver colliders/targets **да** step; стары post-step clamp не мае права перапісваць physics feedback.

Для neural locomotion **physical pelvis authoritative**. LifeCore 2D position ператвараецца ў desired trajectory/velocity, а сімуляваны pelvis вяртаецца праз `BodyFeedback`. У proof horizontal/vertical root propulsion выключаны: можна constraint-іць толькі screen depth і непажаданыя rotations. Калі для ранняга product bridge часова пакідаецца XY-attractor, mode называецца `hybrid`, ён не падтрымлівае вагу па vertical axis, яго explicit force/work лагіруецца і cap-іцца адносна leg-actuator work; ён не ўдзельнічае ў locomotion gates.

Адна conversion boundary фіксуе адзінкі: solver `X` = screen horizontal, `Y` = up/gravity, `Z` = depth; усе physics у m/kg/s, screen projection мае versioned metres-per-pixel, а desktop surfaces — local tangent/normal. `desired_velocity_xy` унутры motor layer пераймяноўваецца ў solver-aligned planar velocity, каб “y” не азначала адначасова screen-down і gravity-up.

Long headless evolution цяпер робіць вялікія крокі каля 0.25 s, таму ён не можа непасрэдна step-іць articulated Rapier. Для яго застаецца `KinematicDogPhysicsBackend`/цяперашняя semantic simulation; поўныя 120 Hz rollouts запускаюцца толькі асобным dream/training job.

### Internal MotorCommand

`BodyIntent` можна пакінуць стабільным. Adapter ператварае яго ў:

```text
desired_surface_velocity_uv
desired_yaw_rate
desired_stance_height
gait_hint
confidence / caution / playfulness
head_attention_hint
recovery_allowed
```

### Пашыраны feedback

LifeCore не патрэбны ўсе joints. Яму патрэбныя:

- grounded/current surface;
- COM velocity/acceleration;
- stable / stumbling / fallen / recovering;
- energy/effort і fatigue proxy;
- foot slip/contact confidence;
- target progress/completion;
- touch/collision intensity;
- policy confidence/OOD flag;
- motor-learning event: candidate accepted/rejected/rollback.

---

## 9. Да 72 гадзін: паслядоўныя neural-body kill gates

### Falsifiable question

> Ці паляпшае маленькі neural residual controller працэдурную physical dog па tracking/recovery/style, не ламаючы яе на трох morphology anchors, і ці захоўвае ён паводзіны пасля export у shipping physics engine?

### Non-goals

- няма Poly Art mesh, fur, face, coat і final rig;
- няма ўсіх desktop windows і obstacle perception;
- няма puppy→great-dane extremes;
- няма arbitrary side/back get-up; толькі recovery пасля non-fall push;
- няма live base-policy training;
- няма RMA/history adapter;
- няма “разумных” трукаў, мовы або generative AI.

Кожны gate правярае адну новую рызыку. Калі ён падае, наступны не запускаецца; 72 гадзіны — cap, не абяцанне, што ўсе пяць гіпотэз абавязкова змесцяцца.

### 0–12 гадзін: Rapier oracle без RL/MJCF/morphology

- 11 collidable links; 12 leg DOFs, 2 spine DOFs спачатку locked;
- procedural stand і commanded walk + ForceBased PD motors;
- multibody/impulse A/B па stability, cost **і observability**;
- small non-fall push recovery, deterministic reset/body hash і debug draw;
- locomotion proof без XY/root anchor;
- калі oracle не стаіць і не ходзіць, RL не запускаецца.

### 12–28 гадзін: residual policy ў адным training engine

- unlock 14 outputs і train stand + velocity tracking + push recovery;
- custom dynamic-reference residual action, 120/30 Hz zero-order hold;
- upright/pose/energy/slip/action-rate rewards;
- адзін neutral body; frozen procedural-only vs neural-residual A/B.

### 28–40 гадзін: export/runtime на тым жа neutral body

- ONNX opset 18 + manifest + hash;
- generate fixed Rust dense weights/evaluator;
- 10,000 PyTorch/ONNX/Rust golden pairs; optional tract parity;
- exact 30/120 Hz observation/action/reference/PD bridge;
- release-mode timing, package, RSS і cold-start audit.

### 40–56 гадзін: cross-engine transfer

- generated MJCF і direct Rapier body з аднаго `DogBodyDesc`;
- тыя ж neutral-body commands/push seeds у mjlab і Rapier;
- спачатку fix units/timestep/reference/gains/contact mismatch, потым domain randomization;
- калі transfer усё яшчэ падае — neural v1 narrowing/reject, не хаваць гэта прыгожым training video.

### 56–72 гадзіны: morphology causal A/B, толькі калі папярэднія PASS

- 3 coherent anchors + interpolation да ±15%;
- дзве policy з аднолькавым rollout budget/PPO config/seeds/bodies: 12D morphology vs тыя ж 12 slots, але masked to zero;
- paired held-out-body evaluation у абодвух engines;
- асобны hybrid-anchor stress пасля locomotion score, ніколі падчас яго;
- decision `PROMOTE TO POLY-ART VISUAL PROOF / NARROW / REJECT`.

### Hard pass gates

**Physics oracle**

- 72,000 steps without NaN, escaped joint або unbounded energy;
- p95 planted-paw stance displacement <2% body length;
- calibrated non-fall push: impulse `J = body_mass × 0.5 m/s` у preregistered ground-plane directions і application point; вяртанне ў upright/height/velocity tolerance на 0.5 s — <2 s;
- checksum identical у 3 runs на тым жа CPU/OS/Rust/Rapier build, feature set і insertion order;
- solver + controller p95 <0.5 ms, p99 <1.0 ms at 120 Hz, release build, warmed, debug draw off; per-step timers, не catch-up-loop timer.

**Neural locomotion**

- 20 paired 60 s episodes per anchor per engine з тымі ж command/push seeds: 60/engine пасля morphology stage;
- ≥95% no-fall success у mjlab і ≥90% у Rapier;
- velocity RMSE ≤0.20 m/s і yaw-rate RMSE ≤0.35 rad/s;
- p95 planted-paw stance displacement <2% body length;
- clamped scalar components / `(14 × policy frames)` <0.5%; illegal-contact physics frames / all physics frames <1%;
- >10 Hz joint-angle/action spectral energy ≤1.25× procedural baseline; blind tiny-screen video review застаецца qualitative gate.

**Morphology**

- no more than 10 percentage-point success drop from neutral body;
- morphology policy дае ≥25% лепшы command RMSE **або** ≥50% менш падзенняў на extreme bodies супраць equal-budget masked-input policy; калі baseline fall rate <5%, fall branch не выкарыстоўваецца;
- OOD morphology is rejected/routed to procedural controller, not allowed to explode.

**Runtime і export**

- PyTorch/ONNX↔fixed Rust MLP max action error ≤1e-5 на 10,000 raw golden inputs; optional tract праходзіць той жа gate;
- batch=1 warmed inference p99 <0.25 ms у release/LTO build на **названых перад run** Windows target і Apple Silicon machine;
- model <1 MB, total packaged app <50 MB, total process working set <150 MB; neural deltas справаздаюцца асобна;
- neural numeric parity правяраецца tolerance, не bitwise.

**Transfer**

- success-rate drop training engine → Rapier ≤10 percentage points;
- no systematic gait-phase inversion або foot-contact order change;
- observation/action order, units, normalizers, rest pose, gains і timestep manifest супадаюць;
- locomotion gate праходзіць з root anchor выключаным; у асобным hybrid test anchor work мае preregistered cap адносна leg-actuator work і не дае vertical support.

Лічбы — preregistered spike gates; яны могуць быць зменены толькі перад run пасля рэальнага target-hardware profile, не пасля таго, як вынік не спадабаўся.

### Kill/fallback

- **Procedural oracle не гатовы за 12 гадзін:** спыніць RL; body/solver/PD contract яшчэ няправільны.
- **Policy не паляпшае zero-residual baseline да 28-й гадзіны:** shipping fallback — procedural gait + IK, neural residual адкладваецца.
- **Хада robotic:** дадаць contact grammar і 4–8 owned pose anchors; не браць NoAI Skye clips.
- **Morphology conditioning не праходзіць causal A/B:** выдаліць morphology vector і звузіць envelope або train 3 experts.
- **Sim-to-sim gap >10 pp:** праверыць contract + domain randomization; exact-dynamics Rapier training патрабуе асобнага batched-environment spike.
- **Raw physical pose уродлівая:** neural body застаецца control proxy, visual retarget атрымлівае bounded corrective layer; physics truth не скідаецца.
- **Policy inference/runtime цяжкі:** hand-loaded fixed MLP або zero-residual procedural gait fallback.

---

## 10. Што не будаваць цяпер

- full torque policy;
- neural soft-body skin;
- online PPO асноўнай сеткі на desktop карыстальніка;
- vision model, які глядзіць на desktop pixels;
- 3D Gaussian dog;
- імпарт Skye animation у motion-training dataset;
- адначасова neural locomotion + full breed generator + final fur;
- physics joints для кожнай visual tail/ear/fur bone;
- адна policy для dachshund→great dane extremes у першым run;
- engine rewrite да sim-to-sim proof.

Адна высокая рызыка ў першым slice — **neural active-ragdoll locomotion**. Poly Art, coat і brain integration дадаюцца толькі пасля таго, як debug-capsule dog ужо жывая.

---

## Production decision

**PROMOTE як прадуктовую архітэктуру:** `LifeCore → procedural gait + bounded neural residual → PD active body → Poly Art skin`.

**KEEP EXPERIMENTAL да 72h gate:** поўны neural controller, morphology conditioning і cross-engine deployment.

**TRIAL асобнымі phase-2 spike-амі:** RMA-style transient dynamics inference і persistent bounded L1 Motor Calibration; гэта розныя механізмы.

**HOLD:** dream-trained residual network; дазволіць толькі ў headless sandbox з frozen validation і rollback.

**REJECT для v1:** live retraining base policy, raw torque control і навучанне на NoAI Poly Art content.

Гэта ўжо не “сабака з animation pack”. Гэта фізічная істота, у якой выгляд, маса, суставы, навучанне і характар звязаныя адным genome, але кожны слой мае сваю бяспечную мяжу.

---

## Асноўныя крыніцы

- [Skye Poly Art — Fab listing](https://www.fab.com/listings/a7cd32af-d039-4386-b8fc-db9b96fefc27)
- [DeepMimic](https://xbpeng.github.io/projects/DeepMimic/index.html)
- [AMP](https://arxiv.org/abs/2104.02180)
- [Rapid Motor Adaptation](https://ashish-kmr.github.io/rma-legged-robots/)
- [RMLL quadruped gait rules, ICAPS 2024](https://ojs.aaai.org/index.php/ICAPS/article/view/31470)
- [McARL morphology-conditioned quadruped policy, 2025](https://arxiv.org/abs/2505.18418)
- [mjlab 1.6.0 release](https://github.com/mujocolab/mjlab/releases/tag/v1.6.0)
- [mjlab 1.6.0 pinned metadata](https://github.com/mujocolab/mjlab/blob/v1.6.0/pyproject.toml)
- [mjlab v1.6.0 velocity-task configuration](https://github.com/mujocolab/mjlab/blob/v1.6.0/src/mjlab/tasks/velocity/velocity_env_cfg.py)
- [mjlab v1.6.0 ONNX exporter](https://github.com/mujocolab/mjlab/blob/v1.6.0/src/mjlab/rl/runner.py)
- [mjlab v1.6.0 RSL-RL training documentation](https://github.com/mujocolab/mjlab/blob/v1.6.0/docs/source/training/rsl_rl.rst)
- [RSL-RL model/export source](https://github.com/leggedrobotics/rsl_rl/blob/v5.4.2/rsl_rl/models/mlp_model.py)
- [RSL-RL license/version metadata](https://github.com/leggedrobotics/rsl_rl/blob/v5.4.2/pyproject.toml)
- [MuJoCo Playground](https://github.com/google-deepmind/mujoco_playground)
- [MuJoCo](https://github.com/google-deepmind/mujoco)
- [MuJoCo versioning and reproducibility policy](https://github.com/google-deepmind/mujoco/blob/main/VERSIONING.md)
- [Isaac Lab](https://github.com/isaac-sim/IsaacLab)
- [Rapier 0.35.2 Rust docs](https://docs.rs/rapier3d/latest/rapier3d/)
- [Rapier changelog: MJCF/armature/springs](https://github.com/dimforge/rapier/blob/master/CHANGELOG.md)
- [Rapier MJCF feature matrix and limitations](https://github.com/dimforge/rapier/blob/master/crates/rapier3d-mjcf/README.md)
- [Rapier determinism requirements](https://rapier.rs/docs/user_guides/templates/determinism)
- [Jolt Rust bindings (`jolt-rust`/`rolt`)](https://github.com/SecondHalfGames/jolt-rust)
- [`physx-rs`](https://github.com/EmbarkStudios/physx-rs)
- [Rapier joints](https://rapier.rs/docs/user_guides/rust/joints)
- [Rapier joint constraints](https://rapier.rs/docs/user_guides/rust/joint_constraints)
- [Rapier joint motors](https://rapier.rs/docs/user_guides/templates/joints/)
- [tract](https://github.com/sonos/tract)
