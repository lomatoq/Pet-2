# Персик: причинная органика, живой взгляд, жидкое тело и локальный слух

## Вывод для реализации

Персик станет убедительнее не от увеличения числа случайных микродвижений, названий эмоций или автономных анимационных циклов. Минимальная связная система должна замкнуть один наблюдаемый контур:

```text
внутренняя ошибка или внешнее событие
→ оценка значимости и доступности действия
→ ориентация и телесная подготовка
→ действие или социальный сигнал
→ ожидание конкретного результата
→ замеченный результат / отсутствие результата
→ обновление потребности, уверенности и привычки
→ восстановление или новая цель
```

Домашние потребности можно формализовать как регулируемые переменные: в homeostatic reinforcement learning ценность результата зависит от того, насколько он уменьшает отклонение внутреннего состояния от желаемой области.^1 Это применимо к искусственному организму как инженерная модель, но не доказывает биологическую жизнь. Гипотеза о нейромодуляции Дои полезна ещё уже: глобальные величины могут менять скорость обучения, горизонт ожидания и ширину исследования, а не непосредственно выбирать «эмоциональную анимацию».^2

Для v17 рекомендуется **каузальный гибрид**, который сохраняет LifeCore владельцем выбора действия, motor runtime — владельцем читаемой последовательности, а PBF/XPBD — владельцем формы и контакта. Добавляется небольшой регулятор возбуждения/восстановления, но он модулирует уже выбранное поведение и физику. Реакционно-диффузионные системы, Lenia и Flow Lenia показывают, что простые локальные правила, нелинейная динамика и сохранение массы способны создавать сложную пространственную активность,^3 ^4 ^5 однако перенос полной клеточной автоматики в Персика сейчас неоправдан. В текущем теле уже есть более подходящая основа: частицы, плотность, поверхностное натяжение, вязкость, внутренний поток, contact surface и XPBD.

Минимальный релиз должен одновременно дать четыре наблюдаемых свойства:

1. **Последовательность с причиной.** У каждого заметного изменения есть `episode_id`, причина, ожидаемый результат и исход. Пока Персик ждёт ответа, он не начинает новый социальный запрос.
2. **Спокойный событийный взгляд.** Фиксация остаётся стабильной; редкая микросаккада — короткий баллистический сдвиг внутри глаза, после которого взгляд возвращается или получает новую точку фиксации. Она не двигает голову и не переписывает мировую цель.
3. **Тело несёт нагрузку.** Опора задаётся плоскостью, нормалью, касательной, площадью контакта и оценкой переданной нагрузки. Нижняя, верхняя и боковая поверхности используют одну механику контакта, но разные правила допустимости и удержания.
4. **Настоящий локальный слух.** RMS, onset и голосоподобность сообщают, что появился звук. Узнавание «Персик» и команды тишины возникает только после сопоставления акустических признаков с обученными шаблонами. Сырые записи после обучения удаляются.

## Что уже есть и где разрыв

Аудит выполнен по `crates/pet_body/src/blink_controller.rs`, `gaze_controller.rs`, `companion_expression_director.rs`, `crates/lifecore/src/companion.rs`, `drives.rs`, `phenotype.rs`, `phenotype_director.rs`, `crates/pet_body/src/liquid/*`, `crates/pet_motor/src/surface.rs`, `programs/rest.rs`, `crates/desktop_host/src/sensors.rs` и `app/src/*`.

### Взгляд и моргание

`BlinkController` задаёт физиологическое моргание длительностью 0,14 с через симметричную `sin²`-огибающую. В живом приложении контроллер продвигается с шагом LifeCore 0,05 с, после чего веки дополнительно сглаживаются. Получаются лишь несколько исходных отсчётов, а быстрое закрытие и более медленное открытие не представлены. `QuietCompanionship` запрашивает social blink каждый тик, и фактический ритм определяется главным образом рефрактером 7,5 с, хотя нового социального события нет.

`GazeController` фильтрует цель разумно, но «микросаккада» реализована как непрерывные синус и косинус в нормализованных координатах всего рабочего стола с амплитудой до 0,015. При ширине 3840 px это может означать десятки пикселей. Биологические микросаккады — быстрые отдельные события; в эксперименте Engbert и Mergenthaler их появление было связано с низким retinal image slip примерно за 200 мс до события, а не с непрерывной гармонической дрожью.^6 В естественном просмотре и фиксации их частота и размеры меняются вместе с задачей; в одной работе при фиксации наблюдалось около 0,8 малых саккад в секунду, со средней длительностью около 13 мс.^7 Это ограничения формы сигнала, а не требование копировать человеческую частоту буквально.

### Потребности, исходы и привычки

LifeCore уже содержит нужные опоры: `Drives`, `AppraisedEvent`, `CompanionIntentFrame`, `ExpectedOutcome`, `episode_id`, социальную память, предпочтения, habituation и моторные программы с фазами. Но видимая причинность рассредоточена. Потребности имеют синусоидальные эндогенные множители; множество affect/readout-полей способно менять внешность независимо; в интерфейсе нет одной записи «почему начался эпизод — чего ждали — что произошло».

Это важнее, чем добавить ещё один эмоциональный класс. В классическом тесте outcome devaluation одно и то же инструментальное действие после ограниченного обучения оставалось чувствительным к ценности результата, а после длительной тренировки могло стать привычным и продолжаться при обесцененном результате.^8 Для Персика практический перенос таков: привычка — это уверенная связь `контекст → действие → исход`, но текущая потребность и изменившаяся ценность исхода всегда могут её подавить. Повторение само по себе не должно порождать вечный цикл.

### Жидкость и опора

PBF формулирует несжимаемость частиц как ограничение плотности и даёт устойчивую интерактивную жидкость,^9 а XPBD вводит compliance и накопленный множитель Лагранжа, уменьшая зависимость жёсткости от шага времени и числа итераций.^10 Unified Particle Physics демонстрирует, что контакт разных материалов можно решать в общей системе частиц и ограничений.^11 Это прямо поддерживает сохранение нынешнего PBF/XPBD вместо отдельной декоративной деформации поверх физики.

В v17 уже есть внутренний `active_flow`, который проектирует силы в подпространство с нулевыми суммарной силой и крутящим моментом, есть контактная плоскость и поддерживаемый отдых. Основной разрыв выше физики: `build_motor_context` всегда добавляет лишь `screen:bottom_edge`, а `rank_surface(..., tired=true)` отбрасывает всё, кроме этой поверхности. Кроме того, удерживаемая деформация в `RestSitSettle` ориентирована на глобальную `Y`, хотя остальная программа уже знает нормаль цели.

### Слух

На Windows и macOS `desktop_host` сейчас сообщает `microphone=false`. `SensorFrame.audio_rms` и `voice_activity` заполняются `None`. Закреплённый `cpal = 0.15.3` используется для вывода; входного потока нет. Официальный CPAL поддерживает перечисление входных устройств, их конфигураций и создание input stream, поэтому отдельный облачный сервис или крупная модель для первого релиза не нужны.^12

Критическое различие:

| Выход | Что он действительно означает | Чего он не означает |
|---|---|---|
| RMS/peak/noise floor | стало громче или тише фонового уровня | человек произнёс слово |
| onset/offset/VAD | начался и закончился акустический фрагмент, возможно речевой | какое слово сказано |
| F0/voicing/tempo | грубая просодия голосоподобного фрагмента | эмоция человека или его личность |
| MFCC + DTW к обученным примерам | фрагмент акустически похож на конкретный локально обученный сигнал | общее распознавание речи, диктора или смысла |

Классический endpoint detector использовал энергию и zero-crossing rate для нахождения границ отдельного высказывания,^13 MFCC показали сильное представление для распознавания слов в раннем сравнительном эксперименте,^14 а dynamic time warping выравнивает разные темпы произнесения по временной оси.^15 YIN даёт локальную оценку F0 с малой задержкой, но её следует использовать как необязательный просодический признак, а не как детектор эмоций.^16

## Органический регулятор без случайной смены состояний

### 1. Ошибки регулирования

Сохранить восемь существующих `Drives`, но сделать их интерпретацию явной. Для потребности `i`:

```text
e_i = clamp((drive_i - comfort_low_i) / (comfort_high_i - comfort_low_i), 0, 1)
urgency_i = smoothstep(enter_i, critical_i, e_i)
```

Рост `drive_i` определяется расходом, контекстом и измеренным исходом; действие уменьшает его только при подтверждённом результате. Например, `InvitePlay` не уменьшает play/social в момент начала. Уменьшение происходит после `shared_play` или добровольного контакта. Отсутствие ответа не повышает «страдание»: оно снижает оценку доступности этого действия в данном контексте и включает рефрактер.

Синусоидальные `social_rhythm`, `play_rhythm`, `curiosity_rhythm` лучше не использовать как прямое объяснение видимой инициативы. Если их оставить, они должны быть слабым медленным prior и присутствовать в trace. Более понятный источник вариативности — накопленная ошибка, время после последнего успешного эпизода, novelty/habituation, доступность пользователя, освоенная привычка и конкретное внешнее событие.

### 2. Двухпеременная возбудимость

FitzHugh показал, что две нелинейные переменные могут представлять возбудимость и рефрактерность, включая устойчивый покой и импульсный ответ.^17 Goodwin исследовал осцилляции в системах отрицательной обратной связи.^18 Для Персика это **инженерное вдохновение**, не виртуальная биохимия. Нужен устойчивый, вынуждаемый событиями регулятор:

```text
da/dt = (input - a)/tau_a - recovery_gain*r
dr/dt = (a - r)/tau_r
input = 0.45*strongest_urgency + 0.30*appraised_novelty
      + 0.35*prediction_error + 0.25*direct_contact - 0.30*safe_rest
```

`a` управляет готовностью ориентироваться, скоростью взгляда и амплитудой внутреннего потока. `r` создаёт рефрактер и плавное восстановление. Обе величины ограничены `[0,1]`, интегрируются фиксированным шагом и не выбирают действие. При нулевом input система возвращается к покою. Это даёт волну `спокойствие → мобилизация → действие → спад`, когда есть причина, и исключает самопроизвольную карусель названий состояний.

### 3. Один владелец причинного эпизода

Добавить компактную структуру, не новый параллельный мозг:

```rust
pub enum CausalPhase {
    Latent,
    Orienting,
    Preparing,
    Acting,
    AwaitingOutcome,
    IntegratingOutcome,
    Recovering,
}

pub struct CausalEpisodeTrace {
    pub episode_id: u64,
    pub cause: CauseCode,
    pub drive: Option<DriveKind>,
    pub selected_action: ActionId,
    pub expected: ExpectedOutcome,
    pub phase: CausalPhase,
    pub elapsed: f32,
    pub deadline: f32,
    pub observed: OutcomeCode,
    pub prediction_error: f32,
    pub habit_key: Option<HabitKey>,
}
```

Переходы выполняются только по измеряемым условиям: фиксация достигнута; мотор сообщил phase completion; контакт подтверждён; пользователь ответил; истёк deadline; опора потеряна. Affect/readout поля становятся следствием этого trace и внутренних ошибок. Они не должны сами инициировать независимые жесты.

### 4. Социальная взаимность

Исследования joint attention показывают, что совместный фокус — это координация вокруг уже доступного объекта/события; в опыте Tomasello и Farrar дети лучше учили название предмета, на котором внимание уже было сосредоточено, чем при перенаправлении внимания взрослым.^19 Оригинальный still-face experiment показал, что противоречивое прекращение обычной ответности меняет последовательность поведения младенца,^20 но переносить детский дистресс на цифрового питомца нельзя. Полезен только принцип измеряемой контингентности.

HRI-работы дают более прямой инженерный перенос: gaze-contingent robot в реальном времени связывал взгляд партнёра с mutual gaze и совместным вниманием,^21 а модель turn-taking разделяла engagement, regulation и disengagement и использовала состояния `seizing/passing/holding/listening` вместе со взглядом, просодией и жестом.^22

Социальный эпизод Персика:

```text
social error + user_available + bid_not_refractory
→ Orienting: короткий перевод взгляда на user proxy/cursor
→ Preparing: небольшая собирающаяся деформация, вдох
→ Acting: один тихий сигнал/предложение
→ AwaitingOutcome: стабильный взгляд, поза ожидания, никаких новых просьб
→ response: синхронный follow-up и positive contingency update
   или timeout: нейтральный check-away, habituation++, спокойное solo-действие
→ Recovering: запрет повторной заявки 30–120 с по контексту
```

Отсутствие ответа не создаёт protest, fear или attachment loss. Оно лишь обновляет вероятность доступности пользователя с ограниченным шагом и коротким периодом полураспада. Поздний добровольный контакт может исправить оценку. Это делает взаимодействие взаимным, но не манипулятивным.

### 5. Привычки

`HabitKey = {context_bin, need_bin, action, target_kind}`. Значение хранит `success_beta`, `failure_beta`, `outcome_value`, `confidence`, `last_used`, `refractory_until`. Обновление происходит только в `IntegratingOutcome`:

```text
success: alpha += w_confidence
failure/timeout: beta += small_w
value += eta * (observed_relief - value)
strength = confidence * posterior_success * max(value, 0)
```

У привычки есть три воротца: текущий результат всё ещё ценен; потребность релевантна; границы пользователя разрешают действие. При явной команде тишины все звуковые привычки немедленно блокируются. Привычка не является периодическим таймером.

## Естественные глаза без дрожания

### Моргание

Кинематические исследования различают спонтанные, произвольные и рефлекторные моргания; их параметры различаются.^23 Высокоскоростная запись показывает асимметрию: закрытие быстрее открытия, а открытие имеет быструю и медленную заключительную часть.^24 Межморгательные интервалы у людей сильно варьируют; они могут иметь серии частых морганий и длинные паузы, а простое среднее скрывает структуру.^25 Поэтому фиксированный период и симметричная синусоида — плохая визуальная модель.

Реализация:

- тип события: `Physiological`, `Protective`, `SocialResponse`, `Drowsy`, `Sleep`, `SleepCheck`;
- физиологический hazard растёт с `time_since_blink`, dryness proxy (время бодрствования), fatigue и переходом между фиксациями; он подавляется в коротком ожидании важного исхода;
- псевдослучайность допустима только при семплировании момента из hazard с сохранённым seed и reason code; она не выбирает смысл события;
- обычная огибающая имеет четыре фазы: закрытие 55–90 мс, hold 10–35 мс, основное открытие 90–150 мс, мягкий хвост 60–140 мс; значения являются стилизацией в пределах опубликованной асимметрии, а не заявкой на точную человеческую физиологию;
- social blink запускается конкретным outcome/acknowledgement, а не постоянным `QuietCompanionship`;
- итоговые веки оцениваются в render time либо аналитически интерполируются между LifeCore ticks; дополнительное сглаживание не должно удлинять закрытые глаза без ограничения.

### Фиксация и микросаккады

`FixationTarget` должен быть единственным мировым владельцем взгляда. Presentation layer получает локальный offset в единицах радиуса глаза/зрачка, а не нормализованного desktop. Событие состоит из `launch 12–25 ms → hold 35–90 ms → settle 70–160 ms`; во время события offset интерполируется ease-out, затем возвращается к нулю или фиксирует новую подцель. Между событиями offset ровно нулевой.

Триггер только при устойчивой фиксации (`dwell > 0.4 s`, low target velocity, confidence > 0.6), при необходимости уточнить цель/проверить контакт. Запретить во время interception, угрозы, крупной саккады, моргания, сна и смены владельца. Амплитуда на экране должна масштабироваться относительно отрисованного глаза и ограничиваться визуально примерно 0,5–1,5 px при обычном размере питомца. Микросаккада не меняет `accepted_target`, не проходит в head turn и не сбрасывает dwell.

## Выразительная жидкость, которая остаётся физическим телом

### Внутренняя активность

Flow Lenia полезна двумя принципами: локальная активность может быть сложной при простых ядрах, а сохранение массы повышает устойчивость локализованного паттерна к препятствиям.^5 Не следует встраивать Flow Lenia как вторую симуляцию. Текущий `active_flow` уже делает главное: локальные вихри с удалением net force/net torque.

Заменить свободно бегущие центры вихрей на фазу причинного эпизода:

| Фаза | Телесный поток |
|---|---|
| Latent/Rest | один медленный малый замкнутый контур, почти нулевая амплитуда |
| Orienting | перенос внутренней активности к стороне цели без net translation |
| Preparing | краткое центростремительное «собирание», рост поверхностного натяжения |
| Acting | направленная волна от задней к передней части вдоль motor axis |
| AwaitingOutcome | затухание, сохранение небольшого переднего внимания |
| Positive outcome | одна расходящаяся волна/relief, затем спад |
| Timeout | нейтральное рассеивание, без collapse/наказания |

Параметры остаются множителями с узкими пределами. Никакой внутренний поток не создаёт поступательное движение root. Проверки нулевых net force/net torque сохраняются.

### Опора на нижней, верхней и боковой плоскости

Унифицировать каждую поверхность как:

```rust
pub struct SupportPlane {
    pub id: SurfaceId,
    pub anchor: Vec2,
    pub normal: Vec2,       // из поверхности в доступное пространство
    pub tangent: Vec2,
    pub velocity: Vec2,
    pub extent: f32,
    pub familiarity: f32,
}

pub struct SupportState {
    pub plane: Option<SupportPlane>,
    pub contact_fraction: f32,
    pub normal_load: f32,
    pub tangential_slip: f32,
    pub grip_demand: f32,
    pub grip_capacity: f32,
    pub stable_seconds: f32,
}
```

На нижней поверхности гравитация создаёт положительную нормальную нагрузку; тело оседает и распластывается по касательной. На боковой/верхней поверхности обычной опоры недостаточно: требуется явное безопасное `grip_capacity`, которое расходует motor effort и имеет ограниченную длительность. Это игровая адгезия, не утверждение о биологическом механизме. Сон допускается только при пассивно устойчивой нижней опоре. Side/top cling — короткое бодрствующее действие, которое завершается `release → controlled fall/flight → settle` до истощения удержания.

Для любой ориентации:

```text
normal_error = min_particle_signed_distance - target_clearance
contact constraint solves normal penetration/load
flatten axis = -plane.normal
spread axis = plane.tangent
stable = contact_fraction >= min_patch
      && abs(tangential_slip) <= slip_limit
      && normal_load within band
      && (passive_support || grip_demand <= grip_capacity)
```

Нужны четыре screen-edge candidates и существующие грани окон. Surface ranking учитывает размер площадки, скорость окна, прошлые неудачи, знакомство, требование сцепления и доступный выход. При исчезновении/движении окна support немедленно переоценивается; форма не должна зависать в воздухе.

## Локальный микрофон и обучаемые сигналы

### Поток обработки

```text
CPAL input callback
→ bounded lock-free/ring buffer (никаких файлов и тяжёлой работы в callback)
→ mono float + resample to 16 kHz
→ 25 ms frames / 10 ms hop
→ adaptive noise floor, RMS, peak, ZCR, spectral flux
→ endpoint segment (примерно 120–1500 ms)
→ voice-like confidence + F0/tempo summaries
→ MFCC (+ delta) sequence
→ DTW against enrolled templates
→ positive distance, negative distance, margin, confidence
→ AudioPercept event; PCM zeroized/discarded
```

Нейронный KWS возможен в малом footprint и даёт хорошие компромиссы false reject/compute,^26 а Speech Commands задаёт полезную методологию целевых слов, unknown и background/silence.^27 Но готовой русской модели в проекте нет, а загрузка крупной модели запрещена требованиями. Поэтому первый релиз — speaker-dependent MFCC+DTW. Он проще, прозрачен и действительно распознаёт сходство с обученным словом. Позже его можно сравнить с маленьким встроенным KWS на одном и том же тестовом корпусе.

### Формат события

```rust
pub enum LearnedCueKind { Nickname, Quiet }

pub struct AudioPercept {
    pub onset: bool,
    pub rms_dbfs: f32,
    pub snr_db: f32,
    pub voice_likelihood: f32,
    pub pitch_hz: Option<f32>,
    pub pitch_slope: f32,
    pub duration_ms: u16,
    pub cue: Option<LearnedCueKind>,
    pub cue_confidence: f32,
    pub rejection_margin: f32,
    pub self_output_contamination: f32,
}
```

`AudioPercept` не хранит строку расшифровки. `Nickname` порождает `AppraisedEvent` только при достижении порога и после cooldown; это может вызвать orient/acknowledgement, если safety/quiet/focus разрешают. `Quiet` — приоритетная граница: сразу снижает applied master gain и блокирует новые vocal requests на выбранный срок. Команда не должна ждать следующего поведения.

### Обучение в Dev Console

1. Переключатель «Микрофон: выключен/включён». До явного включения поток не создаётся. UI показывает устройство, разрешение ОС, input level и ошибку.
2. Кнопка «Обучить имя». Пользователь произносит сигнал 6 раз: обычным голосом, чуть тише/громче и с двух разумных расстояний. Каждый пример показывается как `слишком тихо / обрезано / принято`; плохой пример не сохраняется.
3. Кнопка «Обучить тишину». Ещё 6 примеров пользовательской фразы, например «тише». UI явно сообщает, что распознаётся звук этой фразы, а не любой синоним.
4. Записать 10–15 с фонового шума и 6 отрицательных фрагментов: обычная речь и похожие слова. Это обязательный rejection set, а не постоянное подслушивание.
5. Выполнить leave-one-out: каждый положительный пример сравнить с остальными; выбрать medoid/templates и порог, который принимает согласованные положительные, затем проверить отрицательные. Если интервалы пересекаются, UI пишет «сигнал недостаточно различим; повторите», а не сохраняет плохую модель.
6. Режим «Проверить 30 секунд» считает `принято / отклонено / возможное ложное срабатывание`, но не обучается автоматически. Пользователь подтверждает набор.
7. Сохранить только версию feature pipeline, нормализацию, MFCC-шаблоны, пороги и статистику качества. Сырые PCM-буферы очищаются. Есть отдельные «переобучить» и «удалить голосовые сигналы».

Порог требует одновременно: `voice_likelihood`, достаточно высокий SNR, положительную DTW-дистанцию ниже `T_pos`, отрыв от ближайшего отрицательного шаблона выше `M_neg` и cooldown. Для имени допустим один acknowledgement на сегмент. Для quiet можно использовать более строгий порог во время собственного звука Персика.

### Собственный голос и barge-in

Полностью выключать микрофон на время вокализации нельзя: тогда пользователь не сможет сказать «тише» именно в нужный момент. Поток остаётся включён, а audio output публикует reference envelope/feature summary и временные границы motif. Сегмент получает `self_output_contamination` по временному перекрытию, сходству огибающих и корреляции спектральной энергии.

- обычный nickname cue при высокой contamination отклоняется;
- quiet cue допускается только при повышенном пороге, новом onset поверх собственной огибающей, хорошем positive/negative margin и достаточном SNR;
- после cue applied master gain плавно падает за 30–80 мс, активный motif обрывается/затухает, новые запросы блокируются;
- экранная кнопка mute остаётся безошибочным способом остановки.

Без синхронизированного acoustic echo cancellation нельзя обещать надёжное дальнеполевое распознавание команды поверх громкого динамика. Reference-based rejection уменьшает ложные срабатывания, но увеличивает пропуски. Это следует измерять отдельно и честно показывать как ограничение.

### Разрешения и приватность

Windows позволяет пользователю запрещать микрофон в системных privacy settings; приложение обязано обрабатывать отсутствие/отзыв доступа и может дать ссылку на `ms-settings:privacy-microphone`.^28 На macOS требуется `NSMicrophoneUsageDescription`, а система запрашивает явное разрешение; попытка доступа без ключа может завершить приложение.^29 CPAL даёт общий поток, но не отменяет правила ОС.

Политика продукта: off by default; заметный live indicator; никакой сети; никакого PCM на диске; фиксированный короткий ring buffer в RAM; шаблоны удаляемы; ошибки устройств видимы; при denied/unplugged приложение продолжает работать без слуха. Из `voice_activity` нельзя выводить личность, настроение, пол, здоровье или содержание разговора.

## Три независимые области реализации

### Область A — глаза

**Владение:** `crates/pet_body/src/blink_controller.rs`, `gaze_controller.rs`, при необходимости узкие изменения `companion_expression_director.rs` и тесты `pet_body`. Не менять LifeCore, audio и surface physics.

**Результат:** асимметричная фазовая огибающая blink; context hazard/reason codes; social blink только на событие; sparse ballistic fixation offsets в eye-local units; запрет передачи offset в мировую цель/head movement.

**Интерфейс интеграции:** сохранить существующие `BlinkRequest`, `BlinkOutput`, `GazePlan`, `GazeOutput` совместимыми; допустимо добавить `BlinkContext`, `BlinkPhase`, `FixationMicroEvent`, `GazeOutput.presentation_offset`. Root передаёт render-time `dt` либо получает аналитическую оценку фазы.

### Область B — локальный слух и cue matcher

**Владение:** новый модуль `crates/desktop_host/src/local_audio_input.rs` (или изолированный sibling), `crates/desktop_host/Cargo.toml`, модульные тесты/fixtures. Не менять `app/main.rs`, nervous system, LifeCore, pet audio worker или UI; root подключает API.

**Результат:** CPAL input lifecycle; bounded ring buffer; frame features; endpointing; MFCC; constrained DTW; enrollment/evaluation/persistence DTO; positive/negative rejection; self-output contamination input; raw PCM never serialized.

**Интерфейс интеграции:** `LocalAudioInput::start/stop/status/drain_percepts`, `CueTrainer::begin/accept_segment/finalize`, `CueModelV1`, `AudioPercept`, `OutputReferenceFrame`. Ошибка/нет устройства — обычный status, не panic.

### Область C — причинная регуляция, привычки и поддерживаемое жидкое тело

**Владение:** новый `crates/lifecore/src/organic_regulation.rs` и его тесты; узкие изменения `drives.rs`/`companion.rs`; `crates/pet_body/src/liquid/active_flow.rs`, `contact_surface.rs`; `crates/pet_motor/src/surface.rs`, `programs/rest.rs` и соответствующие тесты. Не менять глаза, audio input, `app/main.rs` и Dev Console.

**Результат:** `CausalEpisodeTrace`; event-driven activation/recovery; outcome-gated relief; bounded habit table; waiting/no-response semantics; phase-driven zero-net-force flow; orientation-independent support plane; explicit finite grip for side/top; bottom-only sleep.

**Интерфейс интеграции:** `OrganicRegulator::tick(RegulationInput)->RegulationOutput`; output enriches existing `BehaviorGoalFrame`/`CompanionIntentFrame`, но не выбирает действие параллельно LifeCore. `SupportState` входит в body feedback; motor получает `SupportPlane`. Root создаёт external `AudioPercept → AppraisedEvent`, добавляет четыре screen-edge candidates и сохраняет/мигрирует `CueModelV1` и habit state.

## Приёмочные испытания

### Детерминизм и причинность

- При фиксированных seed и event log trace побитово/численно повторим при 30/60/120 Гц; выбор действия и episode transitions одинаковы.
- В течение 30 минут без внешних событий каждый заметный social bid имеет ненулевой drive error, reason code и доступность; нет цикла, объясняемого лишь временем синусоиды.
- После одного bid система входит в `AwaitingOutcome`; до response/timeout не появляется второй bid.
- На response фиксируются ожидание, наблюдаемый исход и prediction error; relief применяется после исхода.
- На три timeout подряд частота заявок уменьшается через habituation/refractory; valence не становится наказующей, attachment не падает, автономное занятие продолжается.
- При изменении ценности/quiet boundary сильная привычка не выполняется; после смены контекста она может восстановиться без переписывания личности.

### Глаза

- Финальная видимая огибающая, включая downstream smoothing, имеет более быстрое закрытие, более медленное открытие и хотя бы 6 монотонных samples при 60 Гц.
- За 10 минут спокойной фиксации нет непрерывного ненулевого offset; события разделены нулевыми интервалами.
- Presentation offset не превышает eye-local лимит, не меняет `accepted_target`, не вызывает head turn и не зависит от разрешения рабочего стола.
- Нет micro-event во время interception/protective blink/sleep/смены цели. Поведение одинаково по времени при 30/60/120 Гц.
- `QuietCompanionship` без нового outcome не создаёт регулярный social blink.

### Физика и опора

- Для active flow суммарные сила и крутящий момент остаются ниже текущего допуска во всех causal phases.
- Масса/число частиц не дрейфуют из-за выражения; density violation и максимальная скорость остаются конечными.
- На bottom support контакт стабилен не менее 10 с, есть видимый spread по tangent и отсутствует floating gap.
- На left/right/top одна и та же форма ориентируется по plane normal/tangent; нет глобального-Y flatten.
- Side/top cling требует `grip_capacity`, имеет конечный budget и заканчивается контролируемым release. Sleep на side/top отклоняется.
- При удалении или быстром движении окна support снимается, root не зависает, выбирается recovery.

### Аудио

- Unit fixtures проверяют `f32/i16/u16`, mono/stereo downmix, sample-rate conversion, unplug/restart и bounded buffer overrun без panic.
- Silence/room noise не дают cue. Loud clap даёт onset, но не nickname/quiet. Произнесённая невыученная речь даёт voice activity, но cue остаётся `None`.
- Leave-one-out принимает минимум 5 из 6 качественных enrollment examples; набор с пересекающимися positive/negative distances отклоняется.
- На отдельном локальном тесте каждая cue оценивается как false accepts/hour и false reject rate. Порог не принимается только по training accuracy.
- При pet silent quiet cue глушит applied output. При pet vocalizing высокоуверенный cue с отдельным onset может затушить текущий motif; echo-only fixture не срабатывает.
- Ни один serialized state/log не содержит PCM, спектрограмму или текст речи. Disable/delete немедленно останавливает stream и удаляет cue features.

## Что нельзя разумно утверждать

- Эта система не делает Персика живым, сознательным или биологически гомеостатическим.
- Двухпеременный регулятор и локальные потоки не являются настоящей биохимией, нейромодуляцией или метаболизмом.
- Flow Lenia не доказывает, что несколько вихрей в PBF создадут эмерджентный разум; из неё заимствуются только локальность и сохранение.
- Человеческие данные о моргании/микросаккадах не задают единственно правильную анимацию нечеловеческого жидкого питомца.
- Prosody summary не распознаёт эмоцию, намерение или личность пользователя.
- MFCC+DTW не является общим распознаванием русской речи; он распознаёт только обученные акустические сигналы, преимущественно для того же голоса и комнаты.
- Без AEC нельзя гарантировать barge-in поверх громкого собственного звука, телевизора или дальней речи.
- Короткий демонстрационный прогон не доказывает привычку или развитие. Нужны многосессионные traces, outcome devaluation, изменение рутины и проверка восстановления.

## Источники

1. Mehdi Keramati, Boris Gutkin. “[Homeostatic reinforcement learning for integrating reward collection and physiological stability](https://elifesciences.org/articles/04811).” *eLife* 3:e04811, 2014. DOI 10.7554/eLife.04811.
2. Kenji Doya. “[Metalearning and neuromodulation](https://doi.org/10.1016/S0893-6080(02)00044-8).” *Neural Networks* 15(4–6), 495–506, 2002.
3. Alan M. Turing. “[The Chemical Basis of Morphogenesis](https://doi.org/10.1098/rstb.1952.0012).” *Philosophical Transactions of the Royal Society B* 237, 37–72, 1952.
4. Bert Wang-Chak Chan. “[Lenia: Biology of Artificial Life](https://doi.org/10.25088/ComplexSystems.28.3.251).” *Complex Systems* 28(3), 251–286, 2019.
5. Erwan Plantec et al. “[Flow-Lenia: Towards open-ended evolution in cellular automata through mass conservation and parameter localization](https://arxiv.org/abs/2212.07906).” Original research preprint, 2022.
6. Ralf Engbert, Konstantin Mergenthaler. “[Microsaccades are triggered by low retinal image slip](https://pmc.ncbi.nlm.nih.gov/articles/PMC1459039/).” *PNAS* 103(18), 7192–7197, 2006.
7. Jorge Otero-Millan et al. “[Saccades and microsaccades during visual fixation, exploration, and search: foundations for a common saccadic generator](https://doi.org/10.1167/8.14.21).” *Journal of Vision* 8(14):21, 2008.
8. Anthony Dickinson. “[Actions and habits: the development of behavioural autonomy](https://doi.org/10.1098/rstb.1985.0010).” *Philosophical Transactions of the Royal Society B* 308, 67–78, 1985.
9. Miles Macklin, Matthias Müller. “[Position Based Fluids](https://mmacklin.com/pbf_sig_preprint.pdf).” *ACM Transactions on Graphics* 32(4), 2013. DOI 10.1145/2461912.2461984.
10. Miles Macklin, Matthias Müller, Nuttapong Chentanez. “[XPBD: Position-Based Simulation of Compliant Constrained Dynamics](https://mmacklin.com/xpbd.pdf).” *Motion in Games*, 49–54, 2016. DOI 10.1145/2994258.2994272.
11. Miles Macklin et al. “[Unified Particle Physics for Real-Time Applications](https://mmacklin.com/uppfrta_preprint.pdf).” *ACM Transactions on Graphics* 33(4), 2014. DOI 10.1145/2601097.2601152.
12. RustAudio contributors. “[CPAL — Cross-Platform Audio Library](https://github.com/RustAudio/cpal).” Official project source and API documentation.
13. Lawrence R. Rabiner, Michael R. Sambur. “[An Algorithm for Determining the Endpoints of Isolated Utterances](https://www.nokia.com/bell-labs/publications-and-media/publications/an-algorithm-for-determining-the-endpoints-of-isolated-utterances/).” *Bell System Technical Journal* 54(2), 297–315, 1975. DOI 10.1002/j.1538-7305.1975.tb02840.x.
14. Steven B. Davis, Paul Mermelstein. “[Comparison of Parametric Representations for Monosyllabic Word Recognition in Continuously Spoken Sentences](https://courses.physics.illinois.edu/ece417/fa2017/davis80.pdf).” *IEEE Transactions on Acoustics, Speech, and Signal Processing* 28(4), 357–366, 1980.
15. Hiroaki Sakoe, Seibi Chiba. “[Dynamic Programming Algorithm Optimization for Spoken Word Recognition](https://jeffe.cs.illinois.edu/teaching/compgeom/refs/Sakoe-Chiba-DTW.pdf).” *IEEE Transactions on Acoustics, Speech, and Signal Processing* 26(1), 43–49, 1978.
16. Alain de Cheveigné, Hideki Kawahara. “[YIN, a fundamental frequency estimator for speech and music](https://pubmed.ncbi.nlm.nih.gov/12002874/).” *Journal of the Acoustical Society of America* 111(4), 1917–1930, 2002. DOI 10.1121/1.1458024.
17. Richard FitzHugh. “[Impulses and Physiological States in Theoretical Models of Nerve Membrane](https://pmc.ncbi.nlm.nih.gov/articles/PMC1366333/).” *Biophysical Journal* 1(6), 445–466, 1961.
18. Brian C. Goodwin. “[Oscillatory behavior in enzymatic control processes](https://pubmed.ncbi.nlm.nih.gov/5861813/).” *Advances in Enzyme Regulation* 3, 425–438, 1965. DOI 10.1016/0065-2571(65)90067-1.
19. Michael Tomasello, Michael J. Farrar. “[Joint Attention and Early Language](https://doi.org/10.1111/j.1467-8624.1986.tb00470.x).” *Child Development* 57(6), 1454–1463, 1986.
20. Edward Tronick et al. “[The Infant’s Response to Entrapment between Contradictory Messages in Face-to-Face Interaction](https://publications.aap.org/pediatrics/article/62/3/403/49072/).” *Pediatrics* 62(3), 403–408, 1978.
21. Tian (Linger) Xu, Hui Zhang, Chen Yu. “[See You See Me: The Role of Eye Contact in Multimodal Human-Robot Interaction](https://pubmed.ncbi.nlm.nih.gov/28966875/).” *ACM Transactions on Interactive Intelligent Systems* 6(1), Article 2, 2016. DOI 10.1145/2882970.
22. Crystal Chao, Andrea L. Thomaz. “[Turn-Taking for Human-Robot Interaction](https://sim.ece.utexas.edu/static/papers/chao10_dwr_turntaking.pdf).” AAAI Fall Symposium: Dialog with Robots, 2010.
23. Frans VanderWerf et al. “[Eyelid Movements: Behavioral Studies of Blinking in Humans Under Different Stimulus Conditions](https://doi.org/10.1152/jn.00557.2002).” *Journal of Neurophysiology* 89(5), 2784–2796, 2003.
24. Kyung-Ah Kwon, Rebecca J. Shipley, Mohan Edirisinghe, Daniel G. Ezra. “[High-speed camera characterization of voluntary eye blinking kinematics](https://pmc.ncbi.nlm.nih.gov/articles/PMC4043155/).” *Journal of the Royal Society Interface* 10(85):20130227, 2013. DOI 10.1098/rsif.2013.0227.
25. Jaime Kaminer et al. “[Characterizing the Spontaneous Blink Generator: An Animal Model](https://pmc.ncbi.nlm.nih.gov/articles/PMC3156585/).” *Journal of Neuroscience* 31(31), 11256–11267, 2011.
26. Tara N. Sainath, Carolina Parada. “[Convolutional Neural Networks for Small-footprint Keyword Spotting](https://www.isca-archive.org/interspeech_2015/sainath15b_interspeech.pdf).” *Interspeech*, 1478–1482, 2015.
27. Pete Warden. “[Speech Commands: A Dataset for Limited-Vocabulary Speech Recognition](https://arxiv.org/abs/1804.03209).” Original dataset paper, 2018.
28. Microsoft. “[Launch Windows Settings: privacy microphone URI](https://learn.microsoft.com/en-us/windows/apps/develop/launch/launch-settings).” Official Windows documentation.
29. Apple. “[Requesting Authorization for Media Capture on macOS](https://developer.apple.com/documentation/bundleresources/requesting-authorization-for-media-capture-on-macos).” Official platform documentation.
