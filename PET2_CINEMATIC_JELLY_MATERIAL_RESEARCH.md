# Pet 2 — cinematic jelly material research

Дата: 2026-08-22
Статус: research-only. У гэтай фазе shader/material код не змяняўся.

## Кароткі вердыкт

Для Pet 2 не патрэбны поўны volumetric raymarch. Найлепшы баланс выгляду, стабільнасці і кошту — **гібрыдны 2.5D material**:

1. існуючы particle density field дае сілуэт, тоўшчу і фактычную дэфармацыю;
2. адфільтраваная height/thickness field дае гладкую аб'ёмную normal;
3. Beer–Lambert transmission + мяккае wrapped scattering ствараюць масу жэле;
4. асобны wet-coat lobe дае глос, але не ператварае цела ў пластык;
5. унутраныя светлавыя сферы жывуць у material space і рэндэрацца ў асобны half-resolution HDR field;
6. іх яркасць лічыцца праз chord length, optical depth і двухмаштабную diffusion-функцыю, а не праз звычайны radial `smoothstep`;
7. emission складаецца ў linear HDR, слабое bloom дадаецца да tone mapping;
8. фінал выходзіць як premultiplied source-over, а не `screen`/`overlay`.

Галоўная мастацкая думка: **жэле павінна чытацца нават без бурбалак**. Бурбалкі — гэта ўнутранае жыццё і эмоцыя, не спосаб схаваць плоскі base material.

## Што ўжо ёсць у Pet 2

Бягучы `liquid_surface.wgsl` ужо мае карысную аснову:

- density iso-surface і analytical antialiasing;
- thickness proxy;
- Beer–Lambert-style RGB absorption;
- wrapped scattering;
- refracted background;
- два specular lobes, Fresnel і rim;
- HDR intermediate і premultiplied compose.

Гэта значыць, перапісваць renderer з нуля не трэба. Але ёсць чатыры структурныя абмежаванні:

1. `field.b` — адзін scalar emission, змяшаны з матэрыялам. Ён можа паказваць flow, але не можа апісаць асобныя каляровыя сферы з уласнай глыбінёй.
2. Normal бярэцца амаль непасрэдна з reconstructed field. Дробныя ваганні часцінак трапляюць у gloss і робяць паверхню крупчастай/піксельнай.
3. Унутраная emission зараз не ведае, колькі жэле знаходзіцца перад крыніцай святла. Таму яна выглядае намаляванай на паверхні.
4. У compose няма асобнага emissive bloom chain. Простае павышэнне emission толькі выбельвае core пасля tone mapping.

## Што робяць моцныя tech-art падыходы

### 1. Раздзяляюць аптычныя задачы на слаі

Daniil Spivak у сваім Rippley-inspired slime material пакідае матэрыял opaque/default-lit, а fake translucency, Fresnel, highlights і bubbles будуе асобнымі light-reactive emissive сігналамі. Гэта не фізічна дакладна, але дае кантроль, стабільную сарціроўку і добрую форму ў realtime: [Slime Shader — Fake Translucency](https://spacecentipede.artstation.com/projects/P6Wkk4).

Ayman-Yereem Kone для water ball наогул разбівае эфект на тры часткі: refractive core, metaball layer і splash/droplets. Урок для Pet 2 — не прымушаць адзін scalar field адначасова быць паверхняй, аб'ёмам, унутранымі аб'ектамі і знешнімі кроплямі: [UE Waterball VFX](https://aymanportfolio.com/projects/ue-waterball-vfx).

### 2. Фэйкаюць глыбіню, але прывязваюць яе да формы і camera/material space

Blake Johnson выкарыстоўвае bubble textures для fake innards, рухае іх па-рознаму адносна view direction і трымае refraction вельмі стрыманай, бо празмернае distortion пачынае чытацца як вада, а не jello: [Slimer breakdown](https://unblakeable.artstation.com/projects/bl31md).

Charlene Altamirano выкарыстоўвае refraction vector і bump offset, каб ніжні слой сапраўды адчуваўся пад паверхняй, а не проста быў другой тэкстурай зверху: [Frozen Fuzzy River breakdown](https://charlenesville.artstation.com/blog).

Sarah Lenker паказвае яшчэ адзін важны патэрн: унутранае святло лепш задаваць 3D/world-space sphere mask, а не camera-aligned 2D texture, і random phase трэба рассінхранізаваць на ўзроўні асобнага аб'екта: [Paper Lantern shader breakdown](https://sarahlenker.artstation.com/blog).

Для Pet 2 гэта перакладаецца ў material-space orb state з `position.xy`, pseudo-depth `z`, radius, age, phase і linear RGB emission. Free-running screen-space noise для такіх аб'ектаў непрымальны.

### 3. Будуюць «мокрую плёнку» асобна ад аб'ёму

Disney Principled BRDF вылучае clearcoat у асобны specular lobe, бо тонкая гладкая плёнка і асноўны матэрыял маюць розную highlight response: [Physically-Based Shading at Disney](https://disneyanimation.com/publications/physically-based-shading-at-disney/).

Epic Substrate таксама разглядае матэрыял як layered matter: Simple Volume сумяшчае Beer–Lambert transmission з fitted scattering, а roughness і thickness верхняга слоя кіруюць blur таго, што знаходзіцца ніжэй: [Substrate Materials](https://dev.epicgames.com/documentation/unreal-engine/overview-of-substrate-materials-in-unreal-engine?lang=en-US).

Такім чынам, body pigment не павінен фарбаваць wet highlight. Для water-rich gel surface specular амаль нейтральны, вузкі і адносна слабы па энергіі, але кантрасны па форме.

### 4. Не размываюць сілуэт разам з паверхняй

NVIDIA screen-space fluid pipeline спачатку атрымлівае depth/thickness, потым bilateral-filtered depth, потым normal; bilateral filter захоўвае межы, у той час як naive blur псуе перакрыцці і сілуэт: [Fluid Rendering in Alice](https://developer.download.nvidia.com/tools/docs/Fluid_Rendering_Alice.pdf).

Сучасная anisotropic screen-space reconstruction таксама спалучае anisotropic particle splats з curvature/narrow-range filtering, каб прыбраць jagged і uneven surface без страты high-frequency формы: [Xu et al., 2023](https://eprints.bournemouth.ac.uk/37974/).

Для Pet 2 raw density павінна застацца крыніцай coverage, а filtered height — толькі крыніцай normal/refraction. Тады паверхня стане гладкай, але blob не атрымае прастакутную blur-маску і не страціць кроплі.

## Параўнанне магчымых архітэктур

| Метад | Якасць | Кошт | Асноўная праблема | Рашэнне |
|---|---:|---:|---|---|
| Поўны 3D raymarch, 12–32 steps | высокая | вельмі высокі | overkill для маленькага desktop pet, цяжкі bloom/transparency | не браць у асноўны tier |
| Адзін звычайны translucent shader | сярэдняя | нізкі/сярэдні | sorting, плоскія innards, слабы gloss, OS alpha абмежаванні | адхіліць |
| Цалкам opaque fake translucency | добрая, stylized | нізкі | няма сапраўднага desktop transmission | прыдатны fallback |
| 2.5D density + thickness + captured background + emissive field | вельмі добрая | сярэдні | патрабуе 2–3 акуратныя passes | **рэкамендаваны** |

## Рэкамендаваны render graph

### Pass A — density/thickness accumulation

Пакінуць цяперашнія anisotropic particle і bond splats. Кантракт:

- `R`: raw density `D`;
- `G`: weighted optical thickness;
- `B`: advected low-frequency living-flow scalar;
- `A`: pigment/material variation.

Distinct internal spheres не павінны больш запісвацца ў `B`.

### Pass B — surface conditioning

Стварыць filtered height `H_f` ад raw field:

- coverage лічыць толькі ад `D_raw`;
- фільтраваць толькі ўнутры liquid neighborhood;
- range weight павінен падаць пры вялікай розніцы density/height;
- normal:

```text
N = normalize(vec3(-s * dH_f/dx, -s * dH_f/dy, 1))
```

Мінімальны варыянт — 5/9-tap narrow-range gather у surface shader. Больш стабільны — асобны half/full-resolution bilateral pass. Не выкарыстоўваць isotropic Gaussian па alpha.

### Pass C — internal emissive spheres

Рэкамендаваны implementation: 8–12 instanced quads у half-resolution `RGBA16F` target.

- RGB: linear emissive radiance;
- A: optional orb/core occupancy для debug і controlled diffusion;
- кожны instance семпліць body thickness/density, таму можа быць clipped і attenuated ўнутры жэле;
- draw additive, але ў linear HDR;
- пасля accumulation — адзін вельмі малы separable blur або dual-scale gather толькі для halo.

Гэта лепш за loop па ўсіх сферах у кожным full-resolution body pixel: кошт расце з фактычнай плошчай quad-аў, а не з плошчай усяго blob × orb count.

### Pass D — jelly surface

Парадак святла ў pixel павінен быць аптычным, а не Photoshop-style:

```text
L_volume = T * L_background + L_scatter + sum(L_orb_i)
L_body   = (1 - F) * L_volume + F * L_reflection + L_direct_specular
```

Потым асобна накладваюцца face features.

### Pass E — emissive bloom

З HDR body target вылучыць толькі emissive contribution, зрабіць quarter/half-resolution separable Gaussian і дадаць перад tone mapping. Gaussian separable, таму 2D convolution можна зрабіць двума 1D passes: [GPU Gems 3, Gaussian](https://developer.nvidia.com/gpugems/gpugems3/part-vi-gpu-computing/chapter-40-incremental-computation-gaussian).

Bloom павінен быць слабым і source-specific. Не blur-ыць увесь body color, інакш знікне wet highlight і вернецца «мыла».

### Pass F — compose

Усе absorption, scattering, emission і bloom аперацыі — у linear HDR. Gamma/tone mapping толькі ў канцы; NVIDIA асобна папярэджвае не рабіць далейшую матэматыку над ужо gamma-corrected image: [The Importance of Being Linear](https://developer.nvidia.com/gpugems/gpugems3/part-iv-image-effects/chapter-24-importance-being-linear).

Фінальны transparent output — premultiplied source-over:

```text
C_out = C_src_premul + C_dst * (1 - alpha_src)
```

Гэта стандартная source-over форма для premultiplied color: [W3C Compositing and Blending](https://www.w3.org/TR/compositing-1/).

`Screen`, `overlay` і звычайны additive blend не павінны выкарыстоўвацца для base jelly/transmission. Additive дапушчальны толькі для фізічна дадатковай emission/bloom у HDR.

## Матэматыка базавага жэле

### Таўшчыня і transmission

Няхай `h(x)` — artist-scaled optical thickness. Задаём жаданы RGB колер transmission `T_ref` пры рэферэнснай таўшчыні `h_ref`:

```text
sigma_a = -ln(max(T_ref, eps)) / h_ref
T(x)    = exp(-sigma_a * h(x))
```

Гэта дае стабільнае кіраванне колерам праз «які колер праходзіць праз 1 адзінку жэле», а не праз незразумелы arbitrary absorption multiplier. Beer transmittance і accumulation emission/scattering уздоўж ray вынікаюць з equation of transfer: [PBRT — The Equation of Transfer](https://www.pbr-book.org/4ed/Light_Transport_II_Volume_Rendering/The_Equation_of_Transfer).

### Scattering

Для realtime 2.5D дастаткова art-directable approximation:

```text
S = pigment * (1 - exp(-sigma_s * h))
    * (ambient_scatter + direct_scatter * wrapped_NdotL)
```

`sigma_s` і `sigma_a` павінны быць асобнымі. Калі адным slider-ам адначасова павышаць opacity і scattering, атрымаецца шэры wax.

### Wet surface

Schlick Fresnel:

```text
F = F0 + (1 - F0) * (1 - NdotV)^5
F0 = ((eta_gel - eta_air) / (eta_gel + eta_air))^2
```

Для water-rich hydrogels measured normal-incidence reflectance складае прыкладна 2.2–2.7%, што адпавядае `F0 ≈ 0.02–0.03`: [hydrogel optical measurements](https://link.springer.com/article/10.1007/s11340-020-00626-0). Таму цяперашні default `F0 = 0.055` знаходзіцца бліжэй да сухога шкла/пластыку і можа рабіць blob пластыкавым.

Рэкамендацыя:

- base lobe: roughness прыкладна `0.20–0.32`, вельмі мяккі;
- coat lobe: roughness `0.06–0.14`, вузкі wet sparkle;
- фізічны `F0 = 0.022–0.030`;
- artist boost рабіць праз light intensity/coat weight, а не праз нерэальны `F0`;
- specular амаль белы; колер жыве ў transmission/scattering.

### Rim

Rim не павінен быць проста emissive `pow(1-NdotV,p)`. Лепш звязаць яго з thinness і backlight:

```text
thin      = 1 - saturate(h / h_core)
backlight = saturate((-NdotL_back + wrap) / (1 + wrap))
rim       = pow(1 - NdotV, p) * (0.25 + 0.75 * backlight) * (0.35 + 0.65 * thin)
```

Так край будзе больш светлым там, дзе святло сапраўды можа прайсці праз тонкі слой, а не па ўсім контуры аднолькава.

## Не-default gradient для ўнутранай светлавой сферы

### Чаму radial `1-r` або адзін `smoothstep` выглядае танна

Звычайны 2D gradient не ўлічвае:

- даўжыню праходу праз сферу;
- saturation emission у тоўстым core;
- глыбіню сферы ўнутры жэле;
- blur/scatter на шляху да паверхні;
- normal і маленькі highlight самой сферы;
- energy-preserving compositing.

Таму ён чытаецца як UI-spot, не як святло ў матэрыі.

### 1. Elliptical coordinate і antialiasing

Для сферы/эліпсоіда `i`:

```text
d  = x - center_i_refracted
q2 = dot(d, A_i * d) / radius_i^2
aa = fwidth(q2)
mask = 1 - smoothstep(1 - aa, 1 + aa, q2)
```

`A_i` дазваляе вельмі слабую material deformation; яна не павінна капіраваць кожны particle jitter.

### 2. Chord length

Для праекцыі сферы даўжыня прамяня ўнутры яе:

```text
ell = 2 * radius_i * sqrt(max(1 - q2, 0))
```

Гэта ўжо дае сапраўдную «тоўстую» сярэдзіну.

### 3. Saturated emissive core

Для homogeneous emissive/absorbing inclusion:

```text
core = 1 - exp(-sigma_orb * ell)
```

У лакальнай праверцы з `sigma_orb = 1.2`, normalized radius `q = 0.75` усё яшчэ дае `core ≈ 0.80`, а пры `q = 0.98` — `≈ 0.38`. Наіўны `1-q` у тых жа пунктах дае `0.25` і `0.02`: ён занадта лінейны, пусты ў сярэдзіне і рэзка памірае каля краю.

### 4. Attenuation праз жэле перад сферай

Няхай `z_i` — normalized depth: `0` каля пярэдняй паверхні, `1` глыбока:

```text
h_front = z_i * h_body
T_front = exp(-sigma_t * h_front)
```

Глыбокая сфера становіцца мякчэй, цямней і больш body-colored. Гэта галоўны cue, які прымушае яе быць **унутры**, а не зверху.

### 5. Двухмаштабная diffusion-воблака

Замест аднаго radial gradient:

```text
G(sigma, d) = exp(-dot(d,d) / (2 * sigma^2))
halo = 0.72 * G(0.42*r, d) + 0.28 * G(1.15*r, d)
```

Першы lobe трымае luminous core, другі імітуе святло, рассеянае жэле. Sum-of-Gaussians — стандартны realtime спосаб набліжаць diffusion profiles; падобны multi-scale прынцып выкарыстоўваецца ў realtime SSS: [GPU Gems 3 — Advanced Skin Rendering](https://developer.nvidia.com/gpugems/gpugems3/part-iii-rendering/chapter-14-advanced-techniques-realistic-real-time-skin).

Для Pet 2 broad lobe трэба clip-іць па body core, але не па orb radius. Таму ён можа працягвацца за геаметрычны край сферы, застаючыся ўнутры істоты.

### 6. Лёгкая сферычная форма

```text
n_orb = normalize(vec3(d / radius_i, sqrt(max(1 - q2, 0))))
shape_light = 0.55 + 0.45 * wrapped_dot(n_orb, light)
inner_rim = pow(1 - n_orb.z, 3.5)
```

`shape_light` дадаецца вельмі слаба. `inner_rim` не павінен ператвараць inclusions у мыльныя бурбалкі; яго задача — толькі паказаць сферу.

### 7. Поўны orb contribution

```text
life = smootherstep(0.00, 0.14, age01)
     * (1 - smootherstep(0.72, 1.00, age01))

L_orb = linear_rgb_i
      * intensity_i
      * life
      * T_front
      * body_core_mask
      * (0.62 * core + 0.28 * halo + 0.10 * shape_light * core)
```

Некалькі `L_orb` складаюцца additively ў linear HDR. Не выкарыстоўваць `screen`; overlap светлавых крыніц фізічна дадае radiance, а выбельванне кантралююць intensity, exposure і tone mapping.

### 8. Material refraction/parallax

Пазіцыю сферы трэба трохі ссунуць reconstructed surface gradient-ам:

```text
center_i_refracted = center_i
    + refract_scale * z_i * grad(H_f)
    + parallax_scale * z_i * view_xy
```

Зрух павінен быць маленькі. Моцны wobble, паводле Blake Johnson, хутка ператварае jello ў water effect.

## Як сферы павінны рухацца

Не выкарыстоўваць асобныя сінусы па `time` у shader. Яны не ведаюць пра дэфармацыю і будуць плыць адносна цела.

Кожная сфера мае CPU/runtime state:

```text
position, velocity, depth, radius, age, lifetime, phase, color, intensity
```

Рэкамендаваны velocity field:

1. sample velocity фактычнага liquid material па 4–8 бліжэйшых часціцах;
2. дадаць вельмі павольны divergence-free curl field;
3. дадаць density-gradient boundary return;
4. інтэграваць fixed timestep / RK2;
5. render position згладжваць асобна ад simulation position.

Для гладкага incompressible drift можна задаць stream function:

```text
psi(p,t) = sum_k a_k * sin(dot(k, p) + omega_k*t + phase_k)
u_curl   = vec2(dpsi/dy, -dpsi/dx)
```

Такое поле не мае divergence і не збірае ўсе сферы ў адзін бок. Амплітуды і фазы deterministic ад identity seed.

Boundary controller працуе толькі каля краю:

```text
v += k_boundary * smoothstep(D_safe, D_edge, D(p)) * normalize(grad D)
v *= exp(-drag * dt)
```

Сферы не павінны ўдзельнічаць у PBF і не павінны цягнуць body mass. Гэта візуальныя inclusions, не фізічныя кавалкі жэле.

## Brain → material mapping

Не падключаць 10 mind variables непасрэдна да shader. Спачатку зрабіць адзін згладжаны `BrainVisualDrive`:

```text
glow_energy = saturate(
    0.18
  + 0.42 * arousal
  + 0.18 * attachment
  + 0.14 * max(valence, 0)
  + 0.12 * social_focus
  - 0.24 * fatigue
)

flow_speed = base_speed * clamp(
    0.45 + 0.85*arousal + 0.35*curiosity - 0.32*fatigue,
    0.30, 1.65
)
```

| Псіхічны сігнал | Візуальная сувязь | Smoothing half-life | Абмежаванне |
|---|---|---:|---|
| valence | warm/cool palette mix | 3–5 s | не больш 20–25° hue drift за секунду |
| arousal | intensity, speed, active count | 0.8–1.5 s | без strobe/popping |
| attachment/social focus | больш цёплы broad halo | 2–4 s | не павышаць core да белага |
| curiosity/novelty | curl complexity і рэдкія новыя сферы | 1.5–3 s | не noise amplitude паверхні |
| stress/frustration | крыху вузейшы, больш кантрасны pulse; phase irregularity | 1–2 s | не рэзкі hue flash |
| fatigue | павольней, цьмяней, даўжэй lifetime | 2–5 s | не выключаць усё жыццё |
| confidence | больш стабільны central depth/coherence | 3–5 s | не павялічваць body opacity |

Колер лепш вылічаць на CPU ў OKLab/OKLCH паміж 2–3 curated palettes, затым перадаваць у shader як linear RGB. HSV interpolation можа праходзіць праз непажаданыя насычаныя колеры і даваць «RGB gaming» замест жывога настрою.

Колькасць актыўных сфер не павінна скакаць. Pool заўсёды мае, напрыклад, 12 slots; brain drive толькі мякка мяняе spawn/fade rate і active opacity з hysteresis.

## Знешнія пухіркі/кавалкі

Цяперашні periodic single-fragment emission выглядае механічна. Патрэбны deterministic stochastic event process.

### Час emission

Выкарыстоўваць seeded non-homogeneous Poisson-like schedule:

```text
lambda(t) = base_rate
          * (0.75 + 0.45*arousal + 0.30*curiosity + 0.15*max(valence,0))

delta_t = -ln(max(random01, eps)) / lambda(t)
```

Гэта дае сапраўды нерэгулярныя інтэрвалы без frame-dependent random.

### Burst size

На event:

```text
P(1 bubble) = 0.62
P(2 bubbles)= 0.28
P(3 bubbles)= 0.10
```

### Size mixture

Не uniform range, а выразная іерархія:

```text
62% satellites: 0.45–0.75 * base_radius
30% medium:     0.85–1.15 * base_radius
 8% hero:       1.35–1.90 * base_radius
```

Гэта якраз дае «часам вялікія, часам малыя», не робячы ўсе кроплі амаль аднолькавымі.

### Spawn position і motion

- spawn па boundary main component, а не па circle вакол COM;
- angle — golden-angle/blue-noise sequence + невялікі jitter, каб не паўтараць бакі і не ствараць stacks;
- velocity = local surface velocity + маленькі outward normal impulse + частка body acceleration;
- асобны exponential drag;
- не прывязваць іх жорстка да body origin;
- idle fragments не вяртаюць mass і не цягнуць blob назад.

### Lifetime curves

```text
alpha(u) = smootherstep(0.00, 0.10, u)
         * (1 - smootherstep(0.58, 1.00, u))

radius(u) = r0 * pow(1 - smootherstep(0.18, 1.00, u), 0.65)
```

Пухірок спачатку жыве, потым паступова змяншаецца і згасае. Не памяншаць яго лінейна ад першага кадра — гэта чытаецца як UI particle.

## Lab controls, якія сапраўды патрэбны

### Surface / volume

- Reference transmittance RGB;
- Reference thickness;
- Scattering strength;
- Thickness gamma;
- Surface smoothing radius/range;
- Refraction amount;
- Rough-refraction blur;
- Wet coat weight;
- Base roughness;
- Coat roughness;
- Rim backlight/thinness.

`F0` лепш трымаць у Advanced з safe range `0.018–0.04`, а не даваць лёгка зрабіць пластык.

### Internal light

- Orb count/pool visibility;
- Radius min/max;
- Rare hero probability;
- Depth min/max;
- Core optical density;
- Narrow/broad diffusion width;
- Intensity;
- Flow speed;
- Curl amount;
- Lifetime;
- Bloom threshold/strength/radius;
- `Brain drive: Off / Preview override / Live`.

### External fragments

- Mean event rate;
- Burstiness;
- Satellite/medium/hero probabilities;
- Lifetime;
- Initial speed;
- Drag;
- Shrink curve;
- Maximum pool size.

### Debug views

- raw density;
- filtered height;
- thickness;
- RGB transmittance;
- surface normal;
- orb core without halo;
- orb `T_front` attenuation;
- accumulated internal radiance;
- bloom extraction;
- final premultiplied alpha.

Без гэтых debug views slider lab зноў будзе выглядаць як набор настройкаў, пра якія незразумела, што яны змяняюць.

## Performance budget

Мэтавая high-quality канфігурацыя для невялікага pet window:

- 8–12 internal orbs;
- 1 instanced half-resolution orb pass;
- 1 two-pass half/quarter-resolution bloom;
- 1 narrow-range surface conditioning pass або bounded gather;
- без per-pixel full-volume raymarch;
- без full-screen 32-tap blur;
- усе intermediate light buffers — `RGBA16F`/linear;
- якасць маштабуецца orb count, bloom resolution і filter taps, не зменай physics.

Прапанаваны gate, які трэба **вымераць**, а не лічыць гарантыяй:

- дадатковы GPU cost material stack: `< 1.0 ms` пры 512×512 render target на мэтавым integrated GPU;
- CPU orb runtime: `< 0.15 ms` для 12 spheres;
- не больш 3 новых render passes у quality tier;
- no-allocation steady state;
- idle і emotion changes не ствараюць shader permutation rebuild.

Калі budget не праходзіць, першым адключаецца real bloom і застаецца analytical in-body diffusion halo. Не выкідваць thickness attenuation і wet coat — яны даюць асноўнае адчуванне матэрыялу.

## Acceptance criteria

1. Blob без internal spheres усё роўна чытаецца як тоўстае мокрае жэле.
2. На чорным, белым, шэрым і checker фоне захоўваюцца колер, gloss і silhouette.
3. Deep orb цямнейшы, мякчэйшы і больш body-colored за front orb таго ж памеру.
4. Orb не плыве адносна liquid material пры перамяшчэнні/дэфармацыі blob.
5. Пры аднолькавым intensity вялікая сфера мае больш шырокую воблаку, але не проста белы плоскі круг.
6. Bloom не размывае вочы, твар і wet highlight.
7. Surface normal не паказвае асобныя particle ovals і не пікселіцца пры змене render scale.
8. Emotion controls не ствараюць flicker, pop колькасці або рэзкую змену hue.
9. External bubbles маюць бачны size hierarchy і неперыядычны rhythm.
10. Transparent output не мае чорнай/шэрай аблямоўкі; RGB заўсёды premultiplied alpha.

## Fundamental limitation desktop transparency

Звычайны OS transparent window перадае compositor-у адзін alpha, а не RGB transmittance і не background-behind-window texture. Таму дакладна пераламаць і пафарбаваць рэальны desktop за pet немагчыма без desktop capture/compositor integration.

Практычныя tiers:

- **Lab / captured background:** поўны `T * background + scatter + emission + refraction`;
- **Production transparent window:** grey-alpha approximation для transmission + уласныя scatter/spec/emission;
- **Fallback:** opaque fake translucency, як у tech-art Rippley-падыходзе.

Гэта не shader bug, а абмежаванне метаду кампазіцыі. Colored transmittance патрабуе асобнага механізму; Epic таксама адрознівае grey і colored transmittance і адзначае, што поўны colored path даражэйшы і патрабуе адпаведнага blending path: [Substrate translucency](https://dev.epicgames.com/documentation/unreal-engine/overview-of-substrate-materials-in-unreal-engine?lang=en-US).

## Парадак будучай рэалізацыі

1. Surface-conditioning debug view і стабільная filtered normal.
2. Artist-friendly transmittance + physical wet coat calibration.
3. Static 3-orb prototype з chord/attenuation/diffusion без animation.
4. Half-resolution instanced orb field і source-specific bloom.
5. Material-space orb advection.
6. BrainVisualDrive з smoothing/hysteresis і preview overrides.
7. Stochastic external bubble emitter з size mixture.
8. Толькі пасля гэтага — look-dev у Lab і production transparency fallback.

Гэты парадак ізалюе прычыну і не дазваляе animation, bloom і эмоцыям маскіраваць дрэнны base material.
