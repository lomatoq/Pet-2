# Независимый критический аудит мозга Pet2

Дата: 2026-09-08. Проверенный HEAD: `6af2e42`. Фаза 1: текущая реализация. Приложение и исходники не изменялись; выполнены чтение кода и изолированные воспроизведения через публичный API lifecore. Это аудит программной причинности и обучения, а не подтверждение биологической достоверности.

## Приоритетные подтверждённые находки

### C1 — P1: отрицательная оценка становится положительным социальным исходом

**Код:** [crates/lifecore/src/lib.rs:286](/Users/darap/projects/Pet-2/crates/lifecore/src/lib.rs:286) относит любой `FeedbackEvent::Reward(_)` к `ExplicitPositive`; `:489`–`:496` превращают этот исход в `+0.90`, `:510`–`:520` увеличивают счётчик успешных добровольных исходов. Одновременно `:309`–`:311` передают исходное отрицательное значение MicroBrain. В live отрицательная кнопка действительно вызывает общий feedback: [app/src/main.rs:3461](/Users/darap/projects/Pet-2/app/src/main.rs:3461), `:3665`–`:3668`.

**Воспроизведение:** новый LifeCore, pending interaction с episode_id=1, затем `apply_feedback(Reward(-1.0))`. Реальный вывод: `recent_reward=-1, successful_voluntary_outcomes=1`.

**Эффект:** один и тот же отказ наказывает общую политику и поощряет социальный результат; при наличии плана положительно учит social timing lexicon. `Reward(0.0)` также ошибочно становится успехом.

**Исправление/проверка:** классифицировать Reward по знаку, ноль трактовать как нейтральный. Табличный тест -1/0/+1 с pending и без pending; проверить знак во всех потребителях, счётчики и поглощение credit ровно один раз.

### C2 — P1: отрицательный телесный сигнал может закрепиться бессрочно

**Код:** [app/src/nervous_system_runtime.rs:209](/Users/darap/projects/Pet-2/app/src/nervous_system_runtime.rs:209) копирует сохранённый episode; `:229` вычисляет `reward_negative=max(previous,negative)`; `:233` сохраняет результат. Сброс существует только при `episode.closed` (`:234`–`:238`) либо новом observe_gesture (`:143`). Телесный удар/боль сами по себе не закрывают episode. Default episode не закрыт.

**Доказательство:** при отсутствии новых gesture events рекуррентное правило `r[n+1]=max(r[n],negative[n])` монотонно. После единственного ненулевого pain/restraint сигнал не может снизиться даже если тело полностью восстановилось. Он продолжает поступать в `interoception.rs:587` и валентность настроения `vita.rs:1157`–`:1163`.

**Эффект:** краткое неприятное воздействие превращается в постоянную отрицательную телесную оценку до следующего подходящего жеста/перезапуска. Не следует путать это с осмысленной долговременной памятью: нет контекста, времени жизни или правила восстановления.

**Исправление/проверка:** разделить текущую телесную стоимость, затухающий аффект и терминальный исход эпизода. Пиковый negative допустим как статистика открытого эпизода, но не как вечный вход каждого tick. Тест «короткий удар без жеста → 60 секунд покоя» должен показать исчезновение непосредственного negative и ограниченное восстановление настроения.

### C3 — P2: последний жест остаётся активным источником новизны после завершения

**Код:** [app/src/nervous_system_runtime.rs:123](/Users/darap/projects/Pet-2/app/src/nervous_system_runtime.rs:123)–`:142` сохраняет GestureFrameV1. При закрытии очищается только EpisodeContextV1 (`:234`–`:238`); gesture нигде не очищается и не стареет. `source()` всегда включает его (`:312`). [crates/lifecore/src/interoception.rs:572](/Users/darap/projects/Pet-2/crates/lifecore/src/interoception.rs:572)–`:574` постоянно использует repetition_similarity и novelty.

**Эффект:** один жест с высоким prediction_error создаёт длительную добавку к Morph novelty, а привычный жест — к habituation даже после отпускания и длительного отсутствия контакта. Фильтры сглаживают этот постоянный вход, но не гасят его.

**Исправление/проверка:** время наблюдения и TTL/затухание, отдельно last classification для диагностики. Тесты одинакового тела после novel/repeated gesture: по окончании TTL непосредственные novelty/habituation-вклады должны совпасть с нейтральным baseline; долговременное обучение остаётся отдельным состоянием.

### C4 — P1 для достоверности обучения: offline/replay исполняют другой мозг

**Код live:** [app/src/main.rs:2355](/Users/darap/projects/Pet-2/app/src/main.rs:2355) prepare_cognition_tick, `:2393` observe_gesture, `:2485` resolve_actuation, `:2499` применение phenotype, `:2201` публикация BodyFeedbackV2.

**Код offline:** [app/src/pointer_replay_runner.rs:164](/Users/darap/projects/Pet-2/app/src/pointer_replay_runner.rs:164)–`:225` и [app/src/evolution_runner.rs:948](/Users/darap/projects/Pet-2/app/src/evolution_runner.rs:948)–`:978` вызывают Vita/Morph/LifeCore напрямую, не используют NervousSystemRuntime. В quiet advance evolution `:844` сохраняется одна копия body feedback, `:854`–`:869` с ней тикает мозг без развития тела. CalendarOnly отдельно и честно назван приближением (`:837`–`:842`).

**Эффект:** хорошая оценка offline-эпизода не подтверждает хороший живой нервный цикл R12: отсутствуют interoception, соматические входы, часть гомеостаза/развития и итоговая моторная коррекция. Это не означает, что runner бесполезен: он годится для явно ограниченной проверки жестов/компонентов.

**Исправление/проверка:** единый детерминированный step runner для мозга/тела, с адаптерами сенсоров/аудио/экрана. До этого маркировать оценки областью применимости, не продвигать изменения полного мозга по неполному симулятору. Golden trace одинаковых seed+начального состояния+сенсорной ленты должен совпадать по решениям, learning events и packet ids live-adapter/headless-adapter.

### C5 — P2: консолидация повторно считает старые события новым опытом

**Код:** [crates/lifecore/src/memory.rs:113](/Users/darap/projects/Pet-2/crates/lifecore/src/memory.rs:113)–`:123` при каждом consolidate обходят весь short_term и увеличивают observations через update_preference (`:228`–`:240`). Ничего не отмечает уже консолидированные записи. `:192`–`:212` повторно находит тот же набор из последних положительных событий и увеличивает habit.use_count/success_score. Live вызывает consolidate при каждом входе в Sleep ([app/src/main.rs:2516](/Users/darap/projects/Pet-2/app/src/main.rs:2516)–`:2519`; `lifecore/src/lib.rs:532`–`:535`).

**Воспроизведение:** три положительных события Chirp, consolidate дважды без новых событий. Первый вызов: `preference_observations=3, habit_use_count=1`; второй: `preference_observations=6, habit_use_count=2`.

**Эффект:** статистическая уверенность и успешность привычки зависят от числа засыпаний, а не числа наблюдений/исполнений. Пока эти поля преимущественно диагностические, но подключение к новым политикам наследует ложную уверенность. `discover_habit` дополнительно выкидывает все неуспешные события и склеивает раздельные успехи в якобы последовательность.

**Исправление/проверка:** persistent cursor/id консолидированных записей, раздельные experience_count и replay_count; replay может менять веса, но не число независимых наблюдений/использований. Проверить идемпотентность без нового опыта; последовательность успех–неуспех–успех не должна становиться успешной непрерывной привычкой.

### C6 — P2: заявленное обучение вариантов реакции сейчас отключено

**Код:** [crates/lifecore/src/lib.rs:405](/Users/darap/projects/Pet-2/crates/lifecore/src/lib.rs:405)–`:408` прямо фиксирует `variant=1` и объясняет переход на семантический план/lexicon timing. В `resolve_interaction_outcome` (`:473`–`:530`) нет update вариантов: обновляется social timing, счётчики, затем очищается credit. `PendingInteractionCredit.learning_openness` сохраняется (`:442`), но в этом пути не применяется. Старый `InteractionVariantBandit::update` остаётся (`interaction.rs:849`) и тестируется отдельно, однако не вызывается живым LifeCore.

**Эффект:** нельзя обещать, что результат взаимодействия обучает выбор разных телесных ответов. Обучение gesture conventions в ecology и social timing lexicon реально существует; это другие механизмы, и их не следует объявлять отсутствующими. Общий ползунок learning_openness влияет на ecology conventions, но не на варианты lifecore-response.

**Исправление/проверка:** явно решить продуктовую семантику. Либо убрать устаревшие поля/метрики/обещания, либо вводить несколько осмысленно различающихся безопасных стратегий с контекстом и outcome credit. Не возвращать косметический ±амплитудный bandit ради формального «обучения». Тест должен демонстрировать изменение вероятности именно полезной стратегии при сохранении границ и возможности отказа.

### C7 — P2: Morph подкрепляет собственное предложение команды, не подтверждённое действие

**Код:** [crates/morph_brain/src/lib.rs:367](/Users/darap/projects/Pet-2/crates/morph_brain/src/lib.rs:367) вызывает note_controls для собственного MorphOutput; `:1500`–`:1521` выбирают/сохраняют last_command по внутренним rates. Эта переменная не сбрасывается при Idle. При feedback `:1546`–`:1563` обучение выбирает last_command. Между предложением Morph и фактическим intent live имеются Vita arbitration, ecology и phenotype ([app/src/main.rs:2428](/Users/darap/projects/Pet-2/app/src/main.rs:2428), `:2440`, `:2499`). В обратную связь `apply_shared_feedback` передаётся лишь событие без id принятого действия (`:3667`).

**Эффект:** operant learning может отнести награду к предложенной, подавленной или давно завершённой команде. Наличие kc_trace ограничивает чувствительность к древнему контексту, но не доказывает, что именно эта команда исполнялась. Это структурная ошибка атрибуции, а не доказательство конкретной видимой деградации у текущего питомца.

**Исправление/проверка:** executed_action acknowledgement после окончательного arbitration: action/episode/id, доля реального исполнения, время начала/конца и причина override. Eligibility и reward связывать с этим token. Контрпример-тест: Morph предлагает Approach, safety/vita выбирает Retreat, последующий reward не увеличивает operant Approach; награда после истечения окна не обучает последнюю случайную команду.

### C8 — P2: snapshot сохраняет Morph reward trace без сопряжённого контекста

**Код:** [crates/morph_brain/src/lib.rs:238](/Users/darap/projects/Pet-2/crates/morph_brain/src/lib.rs:238)–`:243`, `:386`–`:393` сохраняют веса, reward_trace, age; не сохраняют сетевое состояние, kc_trace, pending_feedback, last_command, RNG progression. Restore создаёт новую сеть и свежие runtime-поля, затем восстанавливает reward_trace (`:309`–`:312`). Следующий update продолжает классическое подкрепление с этим reward (`:360`–`:362`, `:1530`–`:1543`). NervousSystemRuntime всегда восстанавливается default ([app/src/main.rs:3238](/Users/darap/projects/Pet-2/app/src/main.rs:3238)), несмотря на сохранённые LifeCore/Vita состояния.

**Эффект:** загрузка — сохранение идентичности/весов, но не точное продолжение всего мозга. Недавняя награда может действовать на новые post-restart сенсорные traces. Перезапуск также сбрасывает somatic baselines/фильтры и создаёт переходный процесс. Размер видимого эффекта требует измерения; потеря идентичности не доказана и не утверждается.

**Исправление/проверка:** определить два контракта: пользовательский restart с очисткой всех кратких learning traces или точный экспериментальный checkpoint с полным причинным состоянием и RNG. Не переносить reward без eligibility. Continuation A/B до/после checkpoint сравнивать по всем компонентам; отдельный restart-тест исключает подкрепление первого нового стимула старой наградой.

### C9 — P2: идентификатор первого жеста после перезапуска может совпасть со старым

**Код:** [crates/pet_perception/src/embodied_gesture.rs:156](/Users/darap/projects/Pet-2/crates/pet_perception/src/embodied_gesture.rs:156) начинает счётчик с 1, `:260`–`:262` выдаёт следующий id; [app/src/vita_runtime.rs:326](/Users/darap/projects/Pet-2/app/src/vita_runtime.rs:326) создаёт perception default даже при восстановленном VitaState. LifeCore сохраняет last_responded_episode и на restore очищает только pending_credit ([crates/lifecore/src/lib.rs:615](/Users/darap/projects/Pet-2/crates/lifecore/src/lib.rs:615)–`:628`); фильтр `:382` отклоняет совпавший episode id.

**Воспроизведение:** принять SoftTouch episode=1, сохранить/восстановить LifeCore, передать первый жест новой сессии, снова episode=1. Фактический вывод: `before restart response=true, first gesture after restart response=false`.

**Эффект:** если предыдущая сессия закончилась на первом эпизоде, первый новый жест считается дублем, а некоторые developmental counters также используют сохранённый last id. Для произвольного большого прошлого id не утверждается массовый пропуск первых N жестов: фильтр сравнивает только последний id, а не весь диапазон. Корень проблемы — смешение persistent и session-local пространств идентификаторов.

**Исправление/проверка:** сохранять монотонный allocator или использовать `(session_id, sequence)` повсеместно; аккуратно мигрировать старые keys. После рестарта новый первый gesture должен приниматься, повторная доставка того же события в одной сессии — отклоняться.

## Дополнительные подтверждённые ограничения

- `MemorySystem.user_model.usual_active_hours` вычисляет часы из elapsed lifetime (`memory.rs:116`; запись `lib.rs:337`), а не локального времени суток. Это не корректный профиль часов пользователя. Сейчас не найден потребитель, влияющий на выбор действий; перед его подключением хранить явный local/UTC timestamp и timezone policy.
- `LifeCore.integrate_felt_state` меняет drives/affect/development (`lib.rs:122`–`:135`), но не вызывает MicroBrain reward update. Телесные outcome идут в Morph как сенсорные каналы, а не автоматически как reward_trace. Нельзя называть каждый соматический канал полноценным обучением последствий.
- Физика заканчивает всю порцию шагов перед `while life_accumulator >= LIFE_DT` (`main.rs:2199`–`:2204`, `:2347`); при catch-up несколько cognition ticks могут читать один последний body packet. `BodyInteroceptionDirector::tick` не дедуплицирует frame_id (`interoception.rs:128`–`:149`). Это нарушение буквального комментария «consumes it once», но нужен нагрузочный trace, чтобы оценить практический вклад в пропуск коротких событий. Не следует просто пропускать все cognition ticks с повторным id: состояние может законно интегрироваться по времени; нужен общий scheduler и контракт sample-and-hold.

## Что не следует объявлять дефектом без дополнительного опыта

Несколько уровней мозга сами по себе не ошибка: быстрые ограниченные реакции, мотивация и выразительность могут иметь разных владельцев. Проблема возникает при неподтверждённой атрибуции и непрозрачном окончательном выборе. Отсутствие полной биологической модели тоже не дефект настольного питомца. Больше нейронов/эмоциональных терминов не является доказательством более содержательного поведения.

## Проверки и ограничения аудита

Внешний временный probe `/tmp/pet2-brain-audit` собран offline с path dependency на текущий lifecore; исходники проекта не менялись. Реально выполнены три контрпримера C1/C5/C9, все подтвердились. Остальные выводы — статический разбор конкретных путей, не имитация проведённых live-экспериментов. Полный пакет тестов и установленное приложение в этой read-only фазе не перезапускались и не обновлялись.

## Барьер перед расширением

Сначала исправить C1/C2/C3 и измерить действительную цепочку «наблюдение → принятое действие → физический результат → единственный исход → изменение обучения». Затем общий live/replay step и честный checkpoint; после этого подключать память, prediction error и персональное обучение. Все новые предложения должны иметь наблюдаемый пользовательский эффект, одного владельца обновления, bounded learning и проверку, не зависящую от самооценки мозга.

Фаза 2 завершена и сохранена отдельно: [критика предложений](/Users/darap/projects/Pet-2/reports/PET2_BRAIN_PROPOSAL_REVIEW.md).

## Приложение: воспроизводимые контрпримеры C1/C5/C9

Основной исследователь повторно запустил тот же изолированный probe; вывод совпал. Это воспроизведения существующих ошибок, не новые проверки в наборе приложения и не их исправления. Для повторения создать отдельный временный Cargo-проект со следующими файлами и запустить `cargo run --offline --manifest-path <temporary-project>/Cargo.toml`, используя отдельный временный `CARGO_TARGET_DIR`. Нужны уже установленные зависимости текущего проекта.

Cargo.toml:

```toml
[package]
name = "pet2-brain-audit-probe"
version = "0.1.0"
edition = "2024"
[dependencies]
lifecore = { path = "/Users/darap/projects/Pet-2/crates/lifecore" }
```

src/main.rs:

```rust
use lifecore::*;
fn main() {
    let mut core = LifeCore::new(Genome::from_seed(7), 11);
    core.state.interactions.pending_credit = Some(PendingInteractionCredit { episode_id: 1, response_id: 1, learning_openness: 1.0, ..Default::default() });
    core.apply_feedback(FeedbackEvent::Reward(-1.0));
    println!("negative reward: recent_reward={}, successful_voluntary_outcomes={}", core.state.recent_reward, core.state.interactions.successful_voluntary_outcomes);
    assert_eq!(core.state.interactions.successful_voluntary_outcomes, 1);
    let mut memory = MemorySystem::default();
    for i in 0..3 {
        memory.record(EventRecord { timestamp: 3600.0 + i as f64, context: [0.0; CONTEXT_SIZE], action: ActionId::Chirp, outcome: Outcome::Success, reward: 0.8, salience: 0.8 });
    }
    memory.consolidate();
    println!("consolidation 1: preference_observations={}, habit_use_count={}", memory.user_model.preferred_attention_strategies[0].observations, memory.habits[0].use_count);
    memory.consolidate();
    println!("consolidation 2 without new events: preference_observations={}, habit_use_count={}", memory.user_model.preferred_attention_strategies[0].observations, memory.habits[0].use_count);
    let mut before_restart = LifeCore::new(Genome::from_seed(8), 12);
    let gesture = EmbodiedGestureEvent { classification: GestureClassification { episode_id: 1, kind: EmbodiedGestureKind::SoftTouch, confidence: 0.95, committed: true, ..Default::default() }, observation_quality: 1.0, ..Default::default() };
    let first = before_restart.observe_embodied_gesture(gesture);
    assert!(first.is_some());
    let mut after_restart = LifeCore::restore(before_restart.snapshot()).unwrap();
    let next_session_first = after_restart.observe_embodied_gesture(gesture);
    println!("gesture id=1: before restart response={}, first gesture after restart response={}", first.is_some(), next_session_first.is_some());
}
```

Наблюдённый вывод на HEAD `6af2e42`:

```text
negative reward: recent_reward=-1, successful_voluntary_outcomes=1
consolidation 1: preference_observations=3, habit_use_count=1
consolidation 2 without new events: preference_observations=6, habit_use_count=2
gesture id=1: before restart response=true, first gesture after restart response=false
```
