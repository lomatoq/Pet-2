//! Authored, event-driven expression recipes matching ORGANIC_BEHAVIOR_REPERTOIRE.md.
//!
//! These are bounded *presentation contributions*, not 200 independent physics
//! controllers. The runtime owns evidence, target selection, phase envelopes,
//! interruption, support/object validation and channel arbitration. GAZE carries
//! target intent only; BODY cannot move the root, create support, or own a grip.
//! Landing, throwing, learning and contact in a cause require the existing motor
//! or cognitive subsystem to perform that action; this catalog adds its visible
//! accompaniment. Cooldown expiry is never evidence to start a recipe.
//!
//! Every float in an effect is a residual from neutral (including eye_scale and
//! eye_aperture). Apply a continuous attack/hold/release envelope over duration;
//! pulses counts deliberate within-bout accents, never independent noisy clocks.

use serde::Serialize;

pub const REPERTOIRE_COUNT: usize = 200;
pub const CHANNEL_GAZE: u16 = 1;
pub const CHANNEL_LIDS: u16 = 2;
pub const CHANNEL_BROWS: u16 = 4;
pub const CHANNEL_MOUTH: u16 = 8;
pub const CHANNEL_BODY: u16 = 16;
pub const CHANNEL_BREATH: u16 = 32;

#[derive(Default, Clone, Copy, Debug, PartialEq, Serialize)]
pub struct RepertoireEffect {
    pub brow_asymmetry: f32,
    pub mouth_asymmetry: f32,
    pub mouth_curve: f32,
    pub mouth_open: f32,
    pub eye_aperture: f32,
    pub eye_scale: f32,
    pub squint: f32,
    pub breath: f32,
    pub lean: f32,
    pub roll: f32,
    pub soft_yield: f32,
    pub pulses: u8,
}

impl RepertoireEffect {
    pub const ZERO: Self = Self {
        brow_asymmetry: 0.0,
        mouth_asymmetry: 0.0,
        mouth_curve: 0.0,
        mouth_open: 0.0,
        eye_aperture: 0.0,
        eye_scale: 0.0,
        squint: 0.0,
        breath: 0.0,
        lean: 0.0,
        roll: 0.0,
        soft_yield: 0.0,
        pulses: 0,
    };
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct RepertoireRecipe {
    pub id: u16,
    pub name: &'static str,
    pub cause: &'static str,
    pub family: u8,
    pub duration: f32,
    pub cooldown: f32,
    pub priority: u8,
    pub requires_support: bool,
    pub requires_object: bool,
    pub channels: u16,
    pub effect: RepertoireEffect,
}

// Sparse authored knobs keep each recipe auditable. There is no random parameter
// generation or fallback preset; omitted fields contribute exactly zero.
macro_rules! recipe {
    ($id:literal, $name:literal, $cause:literal, $duration:literal, $cooldown:literal,
     $priority:literal, $support:literal, $object:literal, $channels:expr;
     $($field:ident: $value:expr),+ $(,)?) => {
        RepertoireRecipe {
            id: $id, name: $name, cause: $cause,
            family: (($id - 1) / 10 + 1) as u8,
            duration: $duration, cooldown: $cooldown, priority: $priority,
            requires_support: $support, requires_object: $object,
            channels: $channels,
            effect: RepertoireEffect { $($field: $value,)+ ..RepertoireEffect::ZERO },
        }
    };
}

// Public const is the catalog contract; callers borrow through repertoire_recipe.
#[allow(clippy::large_const_arrays)]
pub const REPERTOIRE: [RepertoireRecipe; REPERTOIRE_COUNT] = [
    // 1. Near-world attention
    recipe!(1, "near_ball_focus", "Мяч входит в ближнюю зону → свести взгляд к его центру.", 0.65, 5.0, 2, false, true, CHANNEL_GAZE | CHANNEL_BROWS;
        brow_asymmetry: 0.06, pulses: 1),
    recipe!(2, "rolling_ball_track", "Мяч начинает катиться → вести глазами с прогнозом остановки.", 1.40, 3.0, 2, false, true, CHANNEL_GAZE | CHANNEL_LIDS;
        squint: 0.06, pulses: 1),
    recipe!(3, "ball_direction_correction", "Мяч резко меняет направление → короткая корректирующая саккада.", 0.28, 2.0, 2, false, true, CHANNEL_GAZE | CHANNEL_BROWS;
        brow_asymmetry: 0.12, pulses: 1),
    recipe!(4, "occluded_ball_peek", "Мяч скрыт телом → проверить последний видимый край.", 0.80, 7.0, 2, false, true, CHANNEL_GAZE | CHANNEL_BODY;
        lean: 0.07, pulses: 1),
    recipe!(5, "home_entrance_check", "Домик рядом после прогулки → посмотреть на вход перед поворотом тела.", 1.10, 12.0, 2, false, false, CHANNEL_GAZE | CHANNEL_BROWS;
        brow_asymmetry: 0.09, pulses: 1),
    recipe!(6, "landing_contact_check", "Подход к опоре → взгляд вниз на будущий контакт.", 0.70, 8.0, 2, false, false, CHANNEL_GAZE | CHANNEL_BODY;
        lean: -0.05, pulses: 1),
    recipe!(7, "gap_head_tilt", "Обнаружена узкая щель → наклон головы для проверки просвета.", 1.30, 15.0, 2, false, false, CHANNEL_GAZE | CHANNEL_BODY | CHANNEL_BROWS;
        roll: 0.09, brow_asymmetry: 0.14, pulses: 1),
    recipe!(8, "contact_point_inspect", "После касания предмета → посмотреть на точку контакта.", 0.60, 4.0, 2, false, false, CHANNEL_GAZE | CHANNEL_LIDS;
        squint: 0.04, pulses: 1),
    recipe!(9, "lost_target_scan", "Потеряна прежняя цель → один ограниченный поиск по соседней зоне.", 1.60, 14.0, 2, false, false, CHANNEL_GAZE | CHANNEL_BODY;
        roll: -0.04, pulses: 1),
    recipe!(10, "nearby_idle_attention", "Нет значимой цели → взгляд на ближайший доступный объект, затем спокойный центр.", 2.30, 10.0, 2, false, false, CHANNEL_GAZE | CHANNEL_LIDS;
        eye_aperture: -0.04, pulses: 1),
    // 2. Binocular expression
    recipe!(11, "near_object_convergence", "Очень близкий объект → ограниченная конвергенция глаз.", 0.80, 5.0, 2, false, true, CHANNEL_GAZE | CHANNEL_LIDS;
        eye_aperture: 0.05, pulses: 1),
    recipe!(12, "far_object_release", "Объект удалился → мягкое возвращение к обычной конвергенции.", 1.30, 5.0, 2, false, true, CHANNEL_GAZE | CHANNEL_LIDS;
        eye_aperture: -0.02, pulses: 1),
    recipe!(13, "stalled_toy_double_take", "Неожиданная остановка игрушки → один глаз кратко догоняет второй.", 0.55, 12.0, 2, false, true, CHANNEL_GAZE | CHANNEL_BROWS;
        brow_asymmetry: 0.18, pulses: 1),
    recipe!(14, "side_inspection_lid", "Осмотр сбоку → ближнее веко чуть уже из-за позы.", 1.10, 9.0, 2, false, false, CHANNEL_GAZE | CHANNEL_LIDS | CHANNEL_BODY;
        squint: 0.12, roll: 0.035, pulses: 1),
    recipe!(15, "playful_one_eye_doubt", "Игровое недоверие после фокуса → один глаз прищурен, второй открыт.", 1.60, 40.0, 2, false, false, CHANNEL_GAZE | CHANNEL_LIDS | CHANNEL_BROWS;
        squint: 0.24, brow_asymmetry: 0.3, pulses: 1),
    recipe!(16, "drowsy_eye_catchup", "Переход от сонливости к вниманию → один глаз открывается раньше.", 1.90, 30.0, 2, true, false, CHANNEL_GAZE | CHANNEL_LIDS;
        eye_aperture: 0.12, squint: 0.07, pulses: 1),
    recipe!(17, "near_side_eye_lead", "Обращение с близкой стороны → глаза поворачиваются до головы.", 0.45, 7.0, 2, false, false, CHANNEL_GAZE | CHANNEL_BROWS | CHANNEL_BODY;
        brow_asymmetry: 0.07, roll: 0.025, pulses: 1),
    recipe!(18, "head_turn_eye_compensation", "Завершение поворота головы → глаза компенсируют движение головы.", 0.80, 4.0, 2, false, false, CHANNEL_GAZE | CHANNEL_BODY;
        roll: -0.025, pulses: 1),
    recipe!(19, "prediction_error_fixation", "Ошибка предсказания без угрозы → краткая остановка глаз на месте события.", 0.70, 10.0, 2, false, false, CHANNEL_GAZE | CHANNEL_LIDS;
        eye_aperture: 0.16, pulses: 1),
    recipe!(20, "own_ball_comic_check", "Комический осмотр собственного мяча → краткий перекос взгляда с явным восстановлением.", 1.20, 55.0, 2, false, true, CHANNEL_GAZE | CHANNEL_LIDS | CHANNEL_BROWS | CHANNEL_BODY;
        squint: 0.17, brow_asymmetry: -0.25, roll: -0.055, pulses: 1),
    // 3. Lids and blink ownership
    recipe!(21, "ordinary_blink", "Накопилась потребность обычного blink → коротко закрыть оба глаза.", 0.18, 5.0, 3, false, false, CHANNEL_LIDS;
        eye_aperture: -0.85, pulses: 1),
    recipe!(22, "trusted_slow_blink", "Спокойный доверительный контакт → один медленный blink.", 0.72, 35.0, 3, false, false, CHANNEL_LIDS;
        eye_aperture: -0.7, pulses: 1),
    recipe!(23, "protective_eye_close", "Резкий приближающийся объект → защитно закрыться на событие.", 0.14, 3.0, 3, false, false, CHANNEL_LIDS;
        eye_aperture: -0.95, pulses: 1),
    recipe!(24, "wake_lid_unfold", "Пробуждение → открыть глаза постепенно, без перезапуска сна.", 2.20, 60.0, 3, true, false, CHANNEL_LIDS;
        eye_aperture: 0.2, pulses: 1),
    recipe!(25, "tired_heavy_lids", "Усталость во время безопасной паузы → тяжёлые верхние веки.", 3.20, 35.0, 3, false, false, CHANNEL_LIDS;
        eye_aperture: -0.35, pulses: 1),
    recipe!(26, "supported_drowse_hold", "Дремота на опоре → краткое полузакрытие с удержанием.", 3.80, 50.0, 3, true, false, CHANNEL_LIDS;
        eye_aperture: -0.5, pulses: 1),
    recipe!(27, "drowse_startle_open", "Неожиданность после дремоты → быстро раскрыть веки.", 0.32, 15.0, 3, true, false, CHANNEL_LIDS;
        eye_aperture: 0.55, pulses: 1),
    recipe!(28, "play_success_wink", "Успешная шутка в активной игре → короткое намеренное подмигивание.", 0.45, 65.0, 3, false, false, CHANNEL_LIDS | CHANNEL_BROWS;
        squint: 0.3, brow_asymmetry: 0.2, pulses: 1),
    recipe!(29, "irritation_soft_close", "Локальное виртуальное раздражение → одно дополнительное мягкое закрытие.", 0.38, 75.0, 3, false, false, CHANNEL_LIDS;
        eye_aperture: -0.6, pulses: 1),
    recipe!(30, "precision_release_blink", "Завершён напряжённый манёвр → blink на разрядке, не посреди точного контакта.", 0.26, 14.0, 3, false, false, CHANNEL_LIDS | CHANNEL_BREATH;
        eye_aperture: -0.75, breath: -0.06, pulses: 1),
    // 4. Brows
    recipe!(31, "uncertain_single_brow", "Неясный объект → одна бровь приподнята.", 1.20, 18.0, 2, false, false, CHANNEL_BROWS;
        brow_asymmetry: 0.42, pulses: 1),
    recipe!(32, "object_comparison_brow", "Внимательное сравнение объектов → внутренние концы немного сближены.", 1.60, 16.0, 2, false, true, CHANNEL_BROWS | CHANNEL_LIDS;
        brow_asymmetry: 0.08, squint: 0.11, pulses: 1),
    recipe!(33, "threat_brows_rise", "Внезапная угроза → обе брови резко вверх с разной задержкой.", 0.40, 10.0, 2, false, false, CHANNEL_BROWS | CHANNEL_LIDS;
        brow_asymmetry: 0.25, eye_aperture: 0.36, pulses: 1),
    recipe!(34, "false_alarm_brows_settle", "Угроза оказалась ложной → брови медленно опускаются несимметрично.", 2.10, 18.0, 2, false, false, CHANNEL_BROWS | CHANNEL_BREATH;
        brow_asymmetry: -0.16, breath: -0.13, pulses: 1),
    recipe!(35, "home_recognition_brows", "Узнан домик → мягко раскрыть внутреннюю часть бровей.", 1.50, 25.0, 2, false, false, CHANNEL_BROWS | CHANNEL_MOUTH;
        brow_asymmetry: 0.11, mouth_curve: 0.1, pulses: 1),
    recipe!(36, "failed_grasp_concentration", "Неудачная попытка захвата → короткое сосредоточенное сведение.", 0.85, 12.0, 2, false, true, CHANNEL_BROWS | CHANNEL_LIDS;
        brow_asymmetry: -0.12, squint: 0.18, pulses: 1),
    recipe!(37, "repeated_trick_skeptic", "Пользователь повторил игровой трюк → односторонняя скептическая дуга.", 1.80, 50.0, 2, false, false, CHANNEL_BROWS;
        brow_asymmetry: -0.48, pulses: 1),
    recipe!(38, "new_game_brow_invitation", "Предложение новой игры → внешний конец брови вверх.", 1.25, 40.0, 2, false, false, CHANNEL_BROWS | CHANNEL_MOUTH;
        brow_asymmetry: 0.34, mouth_curve: 0.08, pulses: 1),
    recipe!(39, "tired_relaxed_brows", "Усталое наблюдение → расслабленная низкая линия без злого нахмуривания.", 3.10, 40.0, 2, false, false, CHANNEL_BROWS | CHANNEL_LIDS;
        brow_asymmetry: -0.05, eye_aperture: -0.18, pulses: 1),
    recipe!(40, "throw_anticipation_brow", "Предвкушение броска → быстрое приподнимание перед отпусканием.", 0.48, 10.0, 2, false, true, CHANNEL_BROWS | CHANNEL_LIDS;
        brow_asymmetry: 0.2, eye_aperture: 0.13, pulses: 1),
    // 5. Mouth and asymmetry
    recipe!(41, "small_success_corner_smile", "Небольшой успех → улыбка начинается с одного уголка.", 1.40, 15.0, 2, false, false, CHANNEL_MOUTH;
        mouth_asymmetry: 0.28, mouth_curve: 0.2, pulses: 1),
    recipe!(42, "large_success_spreading_smile", "Большой игровой успех → улыбка расширяется к другой стороне.", 1.90, 30.0, 2, false, false, CHANNEL_MOUTH | CHANNEL_LIDS;
        mouth_asymmetry: 0.12, mouth_curve: 0.48, squint: 0.1, pulses: 1),
    recipe!(43, "curious_contact_mouth", "Любопытство перед контактом → слегка приоткрытый рот.", 1.15, 14.0, 2, false, true, CHANNEL_MOUTH;
        mouth_open: 0.12, pulses: 1),
    recipe!(44, "surprise_round_mouth", "Неожиданное событие → краткая округлая форма рта.", 0.60, 15.0, 2, false, false, CHANNEL_MOUTH;
        mouth_open: 0.43, mouth_curve: -0.03, pulses: 1),
    recipe!(45, "error_thinking_lips", "Размышление после ошибки → губы смещены в сторону.", 1.70, 24.0, 2, false, false, CHANNEL_MOUTH;
        mouth_asymmetry: -0.32, mouth_curve: -0.07, pulses: 1),
    recipe!(46, "precise_hold_lip_press", "Нужна точность удержания → лёгкое поджатие рта.", 1.30, 9.0, 2, false, true, CHANNEL_MOUTH;
        mouth_open: -0.12, mouth_curve: -0.08, pulses: 1),
    recipe!(47, "funny_ball_fall_smirk", "Забавное падение мяча → косая ухмылка, затем расслабление.", 1.80, 40.0, 2, false, true, CHANNEL_MOUTH;
        mouth_asymmetry: 0.4, mouth_curve: 0.27, pulses: 1),
    recipe!(48, "awkward_pose_grimace", "Неудобная поза → краткая односторонняя гримаса.", 0.75, 20.0, 2, false, false, CHANNEL_MOUTH;
        mouth_asymmetry: -0.38, mouth_curve: -0.18, pulses: 1),
    recipe!(49, "exhale_mouth_lag", "Завершён длинный выдох → рот закрывается позже тела.", 1.60, 18.0, 2, false, false, CHANNEL_MOUTH | CHANNEL_BREATH;
        mouth_open: 0.06, breath: -0.2, pulses: 1),
    recipe!(50, "pretend_surprise_mouth", "Игровая имитация удивления → преувеличенная вытянутая форма без постоянного дрожания.", 1.00, 90.0, 2, false, false, CHANNEL_MOUTH | CHANNEL_LIDS;
        mouth_open: 0.65, eye_scale: 0.12, pulses: 1),
    // 6. Laughter and play
    recipe!(51, "lucky_bounce_eye_chuckle", "Неожиданно удачный отскок → один беззвучный смешок глазами.", 0.85, 45.0, 1, false, true, CHANNEL_LIDS | CHANNEL_MOUTH;
        squint: 0.25, mouth_curve: 0.24, pulses: 1),
    recipe!(52, "rally_double_laugh", "Повторный удачный обмен мячом → два коротких импульса смеха.", 1.40, 60.0, 1, false, true, CHANNEL_LIDS | CHANNEL_MOUTH | CHANNEL_BREATH;
        squint: 0.2, mouth_curve: 0.45, breath: 0.16, pulses: 2),
    recipe!(53, "consensual_tickle_laugh", "Щекочущее касание при согласии → улыбка с прерывистым выдохом.", 1.70, 60.0, 1, false, false, CHANNEL_MOUTH | CHANNEL_BREATH;
        mouth_asymmetry: 0.15, mouth_curve: 0.42, breath: 0.2, pulses: 3),
    recipe!(54, "safe_miss_shy_smile", "Свой неловкий, безопасный промах → смущённая усмешка.", 1.90, 45.0, 1, false, false, CHANNEL_MOUTH | CHANNEL_BROWS | CHANNEL_BODY;
        mouth_asymmetry: -0.2, mouth_curve: 0.16, brow_asymmetry: -0.15, roll: -0.03, pulses: 1),
    recipe!(55, "returned_ball_joy", "Пользователь вернул потерянный мяч → радостное раскрытие лица.", 1.25, 45.0, 1, false, true, CHANNEL_LIDS | CHANNEL_MOUTH;
        eye_aperture: 0.25, mouth_curve: 0.5, mouth_open: 0.15, pulses: 1),
    recipe!(56, "play_laugh_bloom", "Нарастание весёлой игры → смех начинается глазами, затем рот.", 2.10, 70.0, 1, false, false, CHANNEL_LIDS | CHANNEL_MOUTH | CHANNEL_BREATH;
        squint: 0.22, mouth_curve: 0.55, mouth_open: 0.22, breath: 0.12, pulses: 2),
    recipe!(57, "laugh_trailing_corner", "Смех затихает → остаточная односторонняя улыбка.", 2.40, 55.0, 1, false, false, CHANNEL_MOUTH;
        mouth_asymmetry: 0.24, mouth_curve: 0.12, pulses: 1),
    recipe!(58, "playful_suppressed_smile", "Пытается сохранить серьёзность в игре → поджатый улыбающийся рот.", 1.80, 65.0, 1, false, false, CHANNEL_MOUTH;
        mouth_open: -0.1, mouth_curve: 0.32, mouth_asymmetry: 0.06, pulses: 1),
    recipe!(59, "play_reply_smile_hold", "Игривое ожидание ответа → улыбка удерживается, тело замирает.", 3.00, 50.0, 1, false, false, CHANNEL_MOUTH | CHANNEL_BODY;
        mouth_curve: 0.26, soft_yield: 0.04, pulses: 1),
    recipe!(60, "unanswered_smile_release", "Ответа нет → спокойно убрать улыбку и заняться своим делом.", 2.20, 50.0, 1, false, false, CHANNEL_MOUTH | CHANNEL_BREATH;
        mouth_curve: -0.04, breath: -0.08, pulses: 1),
    // 7. Virtual scent exploration
    recipe!(61, "new_ball_double_sniff", "Новый мяч → приблизить лицо и сделать два малых вдоха.", 1.25, 40.0, 1, false, true, CHANNEL_GAZE | CHANNEL_BODY | CHANNEL_BREATH;
        lean: 0.08, breath: 0.1, pulses: 2),
    recipe!(62, "changed_home_sniff", "Домик после изменения сцены → обследовать вход короткими вдохами.", 1.80, 75.0, 1, false, false, CHANNEL_GAZE | CHANNEL_BODY | CHANNEL_BREATH;
        lean: 0.05, breath: 0.08, pulses: 3),
    recipe!(63, "virtual_trace_sample", "Найден виртуальный след → перемещаться между точками градиента.", 2.00, 45.0, 1, false, false, CHANNEL_GAZE | CHANNEL_BREATH | CHANNEL_BODY;
        roll: 0.03, breath: 0.07, pulses: 2),
    recipe!(64, "weak_source_resample", "Слабый виртуальный источник → остановиться для второго измерения.", 1.40, 40.0, 1, false, false, CHANNEL_LIDS | CHANNEL_BREATH;
        squint: 0.08, breath: 0.13, pulses: 2),
    recipe!(65, "side_source_face_turn", "Источник сбоку → повернуть лицо до корпуса.", 0.90, 25.0, 1, false, false, CHANNEL_GAZE | CHANNEL_BODY | CHANNEL_BREATH;
        roll: 0.06, breath: 0.05, pulses: 1),
    recipe!(66, "raised_source_stretch", "Источник выше → вытянуть переднюю часть тела без отрыва опоры.", 1.70, 50.0, 1, true, false, CHANNEL_BODY | CHANNEL_BREATH;
        lean: 0.12, soft_yield: -0.06, breath: 0.08, pulses: 1),
    recipe!(67, "familiar_source_sniff", "Знакомый источник → один короткий вдох и быстрое узнавание.", 0.70, 35.0, 1, false, false, CHANNEL_BROWS | CHANNEL_MOUTH | CHANNEL_BREATH;
        brow_asymmetry: 0.05, mouth_curve: 0.09, breath: 0.08, pulses: 1),
    recipe!(68, "unusual_source_recheck", "Необычный источник → вдох, отступление, повторная проверка.", 2.40, 70.0, 1, false, false, CHANNEL_BODY | CHANNEL_BROWS | CHANNEL_BREATH;
        lean: -0.07, brow_asymmetry: 0.23, breath: 0.12, pulses: 2),
    recipe!(69, "moved_ball_compare", "Мяч перенесён → сверить его с прежней точкой.", 1.60, 35.0, 1, false, true, CHANNEL_GAZE | CHANNEL_BROWS;
        brow_asymmetry: -0.19, pulses: 1),
    recipe!(70, "vanished_source_last_sniff", "Источник исчез → проверить воздух один раз и прекратить поиск.", 1.10, 65.0, 1, false, false, CHANNEL_GAZE | CHANNEL_MOUTH | CHANNEL_BREATH;
        mouth_open: 0.04, breath: 0.06, pulses: 1),
    // 8. Breathing gestures
    recipe!(71, "sneeze_preparation", "Виртуальное раздражение накопилось → подготовка чиха с подрагиванием лица.", 1.20, 300.0, 2, false, false, CHANNEL_LIDS | CHANNEL_MOUTH | CHANNEL_BREATH;
        squint: 0.26, mouth_open: 0.16, breath: 0.18, pulses: 2),
    recipe!(72, "single_sneeze_release", "Порог чиха достигнут → один короткий чих с возвратом массы.", 0.48, 300.0, 2, false, false, CHANNEL_LIDS | CHANNEL_MOUTH | CHANNEL_BODY | CHANNEL_BREATH;
        eye_aperture: -0.8, mouth_open: 0.38, lean: -0.1, breath: -0.35, soft_yield: 0.12, pulses: 1),
    recipe!(73, "aborted_sneeze_puzzlement", "Раздражение исчезло до порога → несостоявшийся чих и удивлённый взгляд.", 1.60, 300.0, 2, false, false, CHANNEL_BROWS | CHANNEL_MOUTH | CHANNEL_BREATH;
        brow_asymmetry: 0.36, mouth_open: 0.07, breath: -0.05, pulses: 1),
    recipe!(74, "sneeze_recovery_breath", "После чиха → короткое восстановительное дыхание.", 2.00, 120.0, 2, false, false, CHANNEL_LIDS | CHANNEL_BREATH;
        eye_aperture: -0.1, breath: 0.11, pulses: 1),
    recipe!(75, "post_play_deep_inhale", "Завершена активная игра → глубокий вдох на опоре.", 2.50, 60.0, 2, true, false, CHANNEL_BODY | CHANNEL_BREATH;
        soft_yield: 0.05, breath: 0.3, pulses: 1),
    recipe!(76, "relief_long_exhale", "Напряжение безопасно спало → длинный облегчённый выдох.", 3.40, 50.0, 2, false, false, CHANNEL_MOUTH | CHANNEL_BREATH;
        mouth_open: 0.08, breath: -0.28, pulses: 1),
    recipe!(77, "sleepy_supported_yawn", "Сонливость и свободный рот → один медленный зевок.", 4.20, 180.0, 2, true, false, CHANNEL_LIDS | CHANNEL_MOUTH | CHANNEL_BREATH;
        eye_aperture: -0.32, mouth_open: 0.72, breath: 0.22, pulses: 1),
    recipe!(78, "interrupted_yawn_refocus", "Зевок прерван значимым событием → рот закрывается, внимание возвращается.", 0.80, 90.0, 2, false, false, CHANNEL_GAZE | CHANNEL_LIDS | CHANNEL_MOUTH;
        eye_aperture: 0.24, mouth_open: -0.15, pulses: 1),
    recipe!(79, "throw_breath_hold", "Точная подготовка броска → короткое удержание дыхательного акцента.", 0.60, 12.0, 2, false, true, CHANNEL_MOUTH | CHANNEL_BREATH;
        mouth_open: -0.08, breath: -0.12, pulses: 1),
    recipe!(80, "rest_breath_reengage", "После отдыха → обычный дыхательный ритм постепенно увеличивается.", 3.60, 60.0, 2, true, false, CHANNEL_BREATH;
        breath: 0.09, pulses: 1),
    // 9. Soft-body grooming
    recipe!(81, "side_tension_smoothing", "Локальное поверхностное напряжение → волна разглаживания по боку.", 2.60, 55.0, 1, true, false, CHANNEL_BODY;
        soft_yield: 0.16, roll: 0.025, pulses: 1),
    recipe!(82, "contact_upper_shake", "После тесного контакта → встряхнуть верхнюю часть тела.", 0.90, 40.0, 1, true, false, CHANNEL_BODY;
        roll: 0.07, soft_yield: 0.08, pulses: 2),
    recipe!(83, "heavy_carry_front_release", "После переноса тяжёлого мяча → расправить передний участок тела.", 2.00, 55.0, 1, true, false, CHANNEL_BODY | CHANNEL_BREATH;
        lean: 0.09, soft_yield: 0.13, breath: 0.06, pulses: 1),
    recipe!(84, "stillness_side_stretch", "Долгая неподвижность → медленно вытянуть один бок.", 3.80, 120.0, 1, true, false, CHANNEL_BODY;
        lean: 0.14, roll: 0.045, soft_yield: -0.04, pulses: 1),
    recipe!(85, "uneven_load_counterstretch", "Асимметричная нагрузка → вытянуть противоположную сторону.", 2.80, 75.0, 1, true, false, CHANNEL_BODY;
        lean: -0.12, roll: -0.065, soft_yield: 0.06, pulses: 1),
    recipe!(86, "landing_load_equalize", "После приземления → выровнять распределение массы.", 1.50, 18.0, 1, true, false, CHANNEL_BODY;
        soft_yield: 0.18, lean: -0.025, pulses: 1),
    recipe!(87, "itch_rub_preparation", "Локальный зуд-модель у поверхности → подготовить бок для трения.", 1.40, 100.0, 1, true, false, CHANNEL_GAZE | CHANNEL_BODY;
        roll: 0.08, soft_yield: 0.1, pulses: 1),
    recipe!(88, "virtual_mark_gather_wave", "След от виртуального взаимодействия → короткая собирающая волна.", 1.60, 75.0, 1, true, false, CHANNEL_BODY;
        soft_yield: -0.1, lean: 0.035, pulses: 1),
    recipe!(89, "groomed_side_inspect", "Завершён уход → осмотреть обработанную сторону.", 1.20, 55.0, 1, true, false, CHANNEL_GAZE | CHANNEL_BODY | CHANNEL_BROWS;
        roll: -0.08, brow_asymmetry: 0.06, pulses: 1),
    recipe!(90, "ineffective_groom_pause", "Уход не помог → сменить способ один раз, затем пауза.", 2.20, 120.0, 1, true, false, CHANNEL_BROWS | CHANNEL_BODY;
        brow_asymmetry: 0.22, soft_yield: 0.04, pulses: 1),
    // 10. Wall support and rubbing
    recipe!(91, "tired_wall_lean", "Усталость у стены → опереться боком и удерживать вес.", 4.50, 60.0, 3, true, false, CHANNEL_BODY | CHANNEL_BREATH;
        lean: 0.18, soft_yield: 0.2, breath: -0.06, pulses: 1),
    recipe!(92, "side_comfort_wall_rub", "Комфорт низкий на одном боку → медленно потереться им о стену.", 3.60, 90.0, 3, true, false, CHANNEL_BODY;
        roll: 0.06, soft_yield: 0.17, pulses: 2),
    recipe!(93, "upper_itch_contact_shift", "Локальный зуд ближе к верху → сместить пятно контакта вверх.", 2.00, 90.0, 3, true, false, CHANNEL_BODY;
        lean: 0.1, roll: 0.04, soft_yield: 0.12, pulses: 1),
    recipe!(94, "lower_itch_crouch", "Зуд ниже → присесть, сохранив касание стены.", 2.40, 90.0, 3, true, false, CHANNEL_BODY;
        lean: -0.08, soft_yield: 0.24, pulses: 1),
    recipe!(95, "smooth_wall_rub_stop", "Поверхность оказалась гладкой → прекратить бесполезное трение.", 1.40, 70.0, 3, true, false, CHANNEL_BROWS | CHANNEL_BODY;
        brow_asymmetry: -0.1, soft_yield: 0.07, pulses: 1),
    recipe!(96, "excess_wall_pressure_yield", "Контакт слишком сильный → ослабить нормальную силу без отскока.", 0.75, 12.0, 3, true, false, CHANNEL_BODY;
        soft_yield: 0.35, lean: -0.04, pulses: 1),
    recipe!(97, "moving_wall_rebalance", "Стена сместилась → перестроить опору, не телепортироваться.", 1.10, 8.0, 3, true, false, CHANNEL_GAZE | CHANNEL_BODY;
        soft_yield: 0.14, roll: -0.025, pulses: 1),
    recipe!(98, "wall_contact_unload", "Трение закончено → мягко разгрузить стену перед отходом.", 2.10, 25.0, 3, true, false, CHANNEL_BODY | CHANNEL_BREATH;
        soft_yield: 0.11, breath: 0.04, pulses: 1),
    recipe!(99, "wall_rest_face_observe", "Хочется наблюдать во время отдыха → повернуть лицо, сохранив боковую опору.", 3.20, 30.0, 3, true, false, CHANNEL_GAZE | CHANNEL_BODY | CHANNEL_LIDS;
        roll: 0.035, eye_aperture: -0.06, pulses: 1),
    recipe!(100, "opposite_wall_side_prepare", "Нужна другая сторона → отойти, повернуться, опереться другим боком.", 2.50, 70.0, 3, true, false, CHANNEL_GAZE | CHANNEL_BODY;
        roll: -0.09, soft_yield: 0.09, pulses: 1),
    // 11. Landing and supported rest
    recipe!(101, "home_supported_settle", "Спокойствие у домика → выбрать опору, снизиться, сесть.", 3.50, 40.0, 3, true, false, CHANNEL_BODY | CHANNEL_LIDS | CHANNEL_BREATH;
        soft_yield: 0.23, eye_aperture: -0.12, breath: -0.08, pulses: 1),
    recipe!(102, "first_touch_soft_compression", "Первое касание → погасить вертикальную скорость мягким сжатием.", 0.65, 8.0, 3, true, false, CHANNEL_BODY;
        soft_yield: 0.32, lean: -0.03, pulses: 1),
    recipe!(103, "seated_weight_adjust", "Масса смещена после посадки → один небольшой перенос центра тяжести.", 1.40, 25.0, 3, true, false, CHANNEL_BODY;
        roll: 0.025, soft_yield: 0.15, pulses: 1),
    recipe!(104, "long_sit_weight_shift", "Длительное сидение → сменить распределение массы без подъёма.", 3.20, 90.0, 3, true, false, CHANNEL_BODY;
        roll: -0.035, soft_yield: 0.19, pulses: 1),
    recipe!(105, "low_energy_seated_watch", "Скука при низкой энергии → наблюдать сидя, а не взлетать.", 4.80, 45.0, 3, true, false, CHANNEL_GAZE | CHANNEL_LIDS | CHANNEL_BREATH;
        eye_aperture: -0.14, breath: -0.04, pulses: 1),
    recipe!(106, "tired_face_lower", "Усталость нарастает → опустить лицо и уменьшить тонус.", 3.60, 60.0, 3, true, false, CHANNEL_BODY | CHANNEL_LIDS;
        lean: -0.11, eye_aperture: -0.24, soft_yield: 0.2, pulses: 1),
    recipe!(107, "home_sleep_arrange", "Домик свободен и нужен сон → устроиться у входа/в допустимом месте.", 4.00, 100.0, 3, true, false, CHANNEL_BODY | CHANNEL_BREATH;
        roll: 0.04, soft_yield: 0.26, breath: -0.1, pulses: 1),
    recipe!(108, "seated_ball_follow", "Подошёл интересный мяч → проследить глазами, оставаясь сидеть.", 2.10, 15.0, 3, true, true, CHANNEL_GAZE | CHANNEL_LIDS;
        eye_aperture: 0.06, pulses: 1),
    recipe!(109, "supported_rise_preload", "Решил встать → предварительная загрузка опоры и плавный подъём.", 1.30, 25.0, 3, true, false, CHANNEL_BODY | CHANNEL_BREATH;
        soft_yield: 0.1, lean: 0.055, breath: 0.08, pulses: 1),
    recipe!(110, "lost_support_recover", "Опора исчезла → немедленное контролируемое восстановление вместо висящей позы.", 0.50, 3.0, 3, false, false, CHANNEL_GAZE | CHANNEL_LIDS | CHANNEL_BODY;
        eye_aperture: 0.3, soft_yield: 0.12, pulses: 1),
    // 12. Sleep and wake
    recipe!(111, "rest_pose_search", "Подходит время внутреннего отдыха → искать удобную позу.", 3.40, 100.0, 2, true, false, CHANNEL_GAZE | CHANNEL_BODY;
        roll: -0.045, soft_yield: 0.12, pulses: 1),
    recipe!(112, "sleep_lids_after_settle", "Переход ко сну → глаза закрываются после стабилизации тела.", 4.80, 180.0, 2, true, false, CHANNEL_LIDS | CHANNEL_BREATH;
        eye_aperture: -0.65, breath: -0.07, pulses: 1),
    recipe!(113, "stable_sleep_breath", "Сон установился → мягкое редкое дыхание без дрейфа.", 6.50, 90.0, 2, true, false, CHANNEL_BREATH;
        breath: -0.03, pulses: 1),
    recipe!(114, "sleep_pressure_small_turn", "Неудобная локальная нагрузка во сне → один малый поворот.", 3.90, 150.0, 2, true, false, CHANNEL_BODY;
        roll: 0.018, soft_yield: 0.08, pulses: 1),
    recipe!(115, "sleep_minor_event_response", "Тихое незначимое событие → краткая реакция без полного пробуждения.", 1.40, 45.0, 2, true, false, CHANNEL_BREATH;
        breath: 0.025, pulses: 1),
    recipe!(116, "sleep_contact_eye_check", "Значимый контакт → приоткрыть один глаз для проверки.", 1.10, 50.0, 2, true, false, CHANNEL_GAZE | CHANNEL_LIDS;
        eye_aperture: 0.15, pulses: 1),
    recipe!(117, "safe_check_resume_sleep", "Причина безопасна → закрыть глаз и продолжить сон.", 2.80, 60.0, 2, true, false, CHANNEL_LIDS | CHANNEL_BREATH;
        eye_aperture: -0.25, breath: -0.045, pulses: 1),
    recipe!(118, "wake_focus_before_motion", "Полное пробуждение → взгляд фокусируется до движения.", 2.00, 90.0, 2, true, false, CHANNEL_GAZE | CHANNEL_LIDS;
        eye_aperture: 0.18, pulses: 1),
    recipe!(119, "wake_supported_stretch", "После пробуждения → потянуться на опоре.", 4.10, 150.0, 2, true, false, CHANNEL_BODY | CHANNEL_BREATH;
        lean: 0.16, soft_yield: -0.07, breath: 0.18, pulses: 1),
    recipe!(120, "wake_near_world_check", "Готов к активности → проверить близкий мир и выбрать цель.", 2.60, 100.0, 2, true, false, CHANNEL_GAZE | CHANNEL_BROWS | CHANNEL_MOUTH;
        brow_asymmetry: 0.13, mouth_curve: 0.07, pulses: 1),
    // 13. Soft grasp accompaniment
    recipe!(121, "ball_approach_brake_expression", "Мяч доступен → затормозить до контакта.", 1.00, 8.0, 4, false, true, CHANNEL_LIDS | CHANNEL_MOUTH | CHANNEL_BODY;
        squint: 0.1, mouth_open: -0.06, lean: -0.035, pulses: 1),
    recipe!(122, "first_grip_contact_yield", "Первое касание → податливо подстроить место захвата.", 0.80, 8.0, 4, false, true, CHANNEL_BODY;
        soft_yield: 0.22, lean: 0.025, pulses: 1),
    recipe!(123, "rolling_grip_match_attention", "Мяч ещё катится → согласовать скорость перед удержанием.", 1.20, 8.0, 4, false, true, CHANNEL_GAZE | CHANNEL_LIDS;
        squint: 0.14, pulses: 1),
    recipe!(124, "confirmed_grip_gather", "Захват подтверждён → плавно подтянуть в socket.", 1.50, 10.0, 4, false, true, CHANNEL_MOUTH | CHANNEL_BODY;
        mouth_curve: 0.08, soft_yield: 0.14, pulses: 1),
    recipe!(125, "distant_socket_effort", "Мяч далеко от socket → сильнее тянуть лишь в пределах ускорения.", 1.30, 12.0, 4, false, true, CHANNEL_MOUTH | CHANNEL_BODY;
        mouth_open: -0.09, lean: 0.075, pulses: 1),
    recipe!(126, "slipped_object_regrip_pause", "Предмет скользнул → перехватить с паузой, не телепортировать.", 1.70, 18.0, 4, false, true, CHANNEL_GAZE | CHANNEL_BROWS;
        brow_asymmetry: 0.27, pulses: 1),
    recipe!(127, "heavy_object_counterlean", "Увеличилась нагрузка → наклонить тело в противовес.", 2.20, 15.0, 4, false, true, CHANNEL_BODY;
        lean: -0.16, roll: -0.03, pulses: 1),
    recipe!(128, "wall_near_object_clearance", "Близка стена → перенести мяч на свободную сторону траектории.", 1.40, 15.0, 4, false, true, CHANNEL_GAZE | CHANNEL_BODY;
        roll: 0.055, soft_yield: 0.06, pulses: 1),
    recipe!(129, "quick_throw_grip_prepare", "Поступило намерение бросить → быстро, но непрерывно усилить удержание.", 0.45, 10.0, 4, false, true, CHANNEL_MOUTH | CHANNEL_BODY;
        mouth_open: -0.13, soft_yield: -0.05, pulses: 1),
    recipe!(130, "user_took_ball_follow", "Пользователь забрал мяч → отпустить владение и проследить взглядом.", 1.60, 12.0, 4, false, false, CHANNEL_GAZE | CHANNEL_LIDS | CHANNEL_MOUTH;
        eye_aperture: 0.1, mouth_curve: 0.04, pulses: 1),
    // 14. Place and throw accompaniment
    recipe!(131, "home_place_approach", "Подносит мяч к дому → замедлить объект раньше тела.", 2.00, 15.0, 4, false, true, CHANNEL_GAZE | CHANNEL_MOUTH | CHANNEL_BODY;
        mouth_open: -0.04, lean: -0.055, pulses: 1),
    recipe!(132, "place_soften_contact", "Мяч в зоне укладки → понизить жёсткость захвата.", 1.40, 12.0, 4, false, true, CHANNEL_BODY;
        soft_yield: 0.28, lean: 0.02, pulses: 1),
    recipe!(133, "supported_ball_release", "Подтверждена опора мяча → разгрузить grip и убрать удержание.", 1.80, 15.0, 4, true, true, CHANNEL_MOUTH | CHANNEL_BODY | CHANNEL_BREATH;
        mouth_curve: 0.12, soft_yield: 0.16, breath: -0.07, pulses: 1),
    recipe!(134, "placed_ball_wait", "Мяч после укладки шевелится → подождать, не пихать телом.", 2.80, 20.0, 4, false, true, CHANNEL_GAZE | CHANNEL_LIDS;
        squint: 0.07, pulses: 1),
    recipe!(135, "slow_roll_single_return", "Мяч выкатился слишком медленно → осторожно вернуть одной попыткой.", 1.60, 35.0, 4, false, true, CHANNEL_GAZE | CHANNEL_BROWS | CHANNEL_BODY;
        brow_asymmetry: 0.1, lean: 0.045, pulses: 1),
    recipe!(136, "gentle_pass_prepare", "Решил лёгкую передачу → маленькая подготовка и направленный отпуск.", 0.90, 20.0, 4, false, true, CHANNEL_MOUTH | CHANNEL_BODY;
        mouth_curve: 0.15, lean: 0.06, pulses: 1),
    recipe!(137, "strong_throw_windup", "Решил сильный бросок → заметное накопление и быстрый импульс.", 1.10, 30.0, 4, false, true, CHANNEL_MOUTH | CHANNEL_BODY | CHANNEL_BREATH;
        mouth_open: -0.05, lean: -0.2, breath: 0.15, pulses: 1),
    recipe!(138, "upward_throw_apex_watch", "Бросок вверх при свободном месте → взгляд предсказывает вершину.", 1.70, 20.0, 4, false, true, CHANNEL_GAZE | CHANNEL_LIDS | CHANNEL_BROWS;
        eye_aperture: 0.14, brow_asymmetry: 0.16, pulses: 1),
    recipe!(139, "confined_throw_reserve", "Бросок в ограниченном месте → снизить силу по доступной дистанции.", 1.25, 25.0, 4, false, true, CHANNEL_LIDS | CHANNEL_MOUTH | CHANNEL_BODY;
        squint: 0.13, mouth_open: -0.07, lean: -0.065, pulses: 1),
    recipe!(140, "throw_followthrough_settle", "Бросок выполнен → follow-through тела без повторного захвата.", 1.50, 15.0, 4, false, false, CHANNEL_BODY | CHANNEL_BREATH;
        lean: 0.11, soft_yield: 0.1, breath: -0.09, pulses: 1),
    // 15. Solo ball play
    recipe!(141, "idle_ball_test_nudge", "Мяч покоится и интерес высок → один пробный толчок.", 1.00, 25.0, 2, false, true, CHANNEL_GAZE | CHANNEL_BROWS | CHANNEL_BODY;
        brow_asymmetry: 0.17, lean: 0.04, pulses: 1),
    recipe!(142, "predictable_bounce_meet", "Мяч отскакивает предсказуемо → встретить его сбоку.", 1.20, 12.0, 2, false, true, CHANNEL_GAZE | CHANNEL_LIDS | CHANNEL_BODY;
        squint: 0.09, roll: 0.045, pulses: 1),
    recipe!(143, "unexpected_bounce_pause", "Отскок неожиданен → пауза удивления перед погоней.", 0.85, 15.0, 2, false, true, CHANNEL_LIDS | CHANNEL_BROWS | CHANNEL_MOUTH;
        eye_aperture: 0.28, brow_asymmetry: -0.17, mouth_open: 0.2, pulses: 1),
    recipe!(144, "clear_lane_ball_roll", "Есть свободный путь → прокатить мяч вдоль него.", 1.90, 25.0, 2, false, true, CHANNEL_GAZE | CHANNEL_BODY | CHANNEL_MOUTH;
        lean: 0.055, mouth_curve: 0.11, pulses: 1),
    recipe!(145, "wall_bounce_test", "Мяч у стены → испытать слабый отскок.", 1.50, 30.0, 2, false, true, CHANNEL_GAZE | CHANNEL_BROWS | CHANNEL_BODY;
        brow_asymmetry: -0.14, roll: 0.05, pulses: 1),
    recipe!(146, "catch_result_pride", "Удалось поймать → коротко показать результат лицом.", 1.60, 25.0, 2, false, true, CHANNEL_BROWS | CHANNEL_MOUTH;
        brow_asymmetry: 0.15, mouth_curve: 0.35, mouth_asymmetry: 0.1, pulses: 1),
    recipe!(147, "missed_catch_retarget", "Поймать не удалось → проследить и выбрать новую точку перехвата.", 1.30, 15.0, 2, false, true, CHANNEL_GAZE | CHANNEL_BROWS;
        brow_asymmetry: -0.23, pulses: 1),
    recipe!(148, "rally_turn_wait", "Пользователь отвечает передачами → принять очередь и ждать следующего мяча.", 3.10, 20.0, 2, false, true, CHANNEL_GAZE | CHANNEL_MOUTH;
        mouth_curve: 0.18, mouth_asymmetry: 0.05, pulses: 1),
    recipe!(149, "rally_solo_transition", "Пользователь перестал отвечать → перейти к одиночной игре без требований.", 2.40, 45.0, 2, false, true, CHANNEL_GAZE | CHANNEL_MOUTH | CHANNEL_BREATH;
        mouth_curve: 0.05, breath: -0.04, pulses: 1),
    recipe!(150, "tired_play_finish", "Энергия упала → закончить игру укладкой мяча.", 3.30, 70.0, 2, false, true, CHANNEL_LIDS | CHANNEL_BODY | CHANNEL_BREATH;
        eye_aperture: -0.2, soft_yield: 0.12, breath: -0.11, pulses: 1),
    // 16. Touch response
    recipe!(151, "gentle_stroke_lean", "Медленное приятное поглаживание → наклониться в его сторону.", 2.30, 12.0, 4, false, false, CHANNEL_BODY | CHANNEL_LIDS | CHANNEL_MOUTH;
        lean: 0.13, eye_aperture: -0.1, mouth_curve: 0.14, pulses: 1),
    recipe!(152, "stroke_direction_follow", "Направление поглаживания сменилось → плавно перестроить наклон.", 1.70, 8.0, 4, false, false, CHANNEL_BODY;
        lean: -0.09, roll: 0.025, pulses: 1),
    recipe!(153, "touch_release_return", "Контакт прекратился → тело возвращается с мягким запаздыванием.", 2.60, 12.0, 4, false, false, CHANNEL_BODY | CHANNEL_BREATH;
        soft_yield: 0.13, breath: -0.05, pulses: 1),
    recipe!(154, "fast_touch_recoil", "Слишком быстрый контакт → кратко собраться и отойти.", 0.60, 15.0, 4, false, false, CHANNEL_BODY | CHANNEL_LIDS;
        soft_yield: -0.09, eye_aperture: 0.22, pulses: 1),
    recipe!(155, "side_touch_point_look", "Нежное касание сбоку → повернуть взгляд к точке.", 0.85, 10.0, 4, false, false, CHANNEL_GAZE | CHANNEL_BODY;
        roll: 0.04, pulses: 1),
    recipe!(156, "repeated_soft_touch_relax", "Повторное согласованное касание → расслабить веки.", 3.00, 25.0, 4, false, false, CHANNEL_LIDS | CHANNEL_MOUTH | CHANNEL_BREATH;
        eye_aperture: -0.22, mouth_curve: 0.1, breath: -0.07, pulses: 1),
    recipe!(157, "held_calm_attention", "Пользователь держит питомца → поддерживать цель без борьбы автоматики.", 3.50, 15.0, 4, false, false, CHANNEL_GAZE | CHANNEL_LIDS | CHANNEL_BODY;
        eye_aperture: -0.03, soft_yield: 0.1, pulses: 1),
    recipe!(158, "drag_secondary_yield", "Началось перетаскивание → убрать самодвижение, сохранить вторичную инерцию.", 1.00, 5.0, 4, false, false, CHANNEL_BODY;
        soft_yield: 0.25, roll: 0.015, pulses: 1),
    recipe!(159, "released_over_support_settle", "Отпущен над опорой → приземлиться и восстановить форму.", 1.60, 12.0, 4, true, false, CHANNEL_BODY | CHANNEL_BREATH;
        soft_yield: 0.29, breath: -0.06, pulses: 1),
    recipe!(160, "released_airborne_orient", "Отпущен без опоры → выбрать безопасный путь вниз.", 0.90, 6.0, 4, false, false, CHANNEL_GAZE | CHANNEL_LIDS | CHANNEL_BODY;
        eye_aperture: 0.2, lean: -0.045, pulses: 1),
    // 17. Social proximity
    recipe!(161, "near_cursor_acknowledge", "Курсор остановился рядом → короткий ответный взгляд.", 0.90, 20.0, 1, false, false, CHANNEL_GAZE | CHANNEL_BROWS;
        brow_asymmetry: 0.085, pulses: 1),
    recipe!(162, "departing_cursor_release", "Курсор удалился → проводить взглядом и отпустить внимание.", 1.70, 20.0, 1, false, false, CHANNEL_GAZE | CHANNEL_LIDS;
        eye_aperture: -0.05, pulses: 1),
    recipe!(163, "busy_user_quiet_rest", "Пользователь занят → тихо устроиться сбоку.", 5.00, 90.0, 1, true, false, CHANNEL_LIDS | CHANNEL_BREATH;
        eye_aperture: -0.16, breath: -0.055, pulses: 1),
    recipe!(164, "returning_user_greeting", "Пользователь вернулся к взаимодействию → небольшое приветственное раскрытие лица.", 1.80, 60.0, 1, false, false, CHANNEL_LIDS | CHANNEL_BROWS | CHANNEL_MOUTH;
        eye_aperture: 0.17, brow_asymmetry: 0.12, mouth_curve: 0.22, pulses: 1),
    recipe!(165, "show_ball_joint_attention", "Хочет показать мяч → посмотреть мяч → пользователь → мяч.", 2.70, 60.0, 1, false, true, CHANNEL_GAZE | CHANNEL_BROWS | CHANNEL_MOUTH;
        brow_asymmetry: 0.19, mouth_curve: 0.19, pulses: 2),
    recipe!(166, "noticed_invitation_wait", "Приглашение замечено → удержать паузу для ответа.", 3.80, 60.0, 1, false, false, CHANNEL_GAZE | CHANNEL_MOUTH;
        mouth_curve: 0.17, pulses: 1),
    recipe!(167, "declined_invitation_release", "Приглашение отклонено → спокойно отойти.", 2.50, 90.0, 1, false, false, CHANNEL_MOUTH | CHANNEL_BREATH;
        mouth_curve: -0.025, breath: -0.065, pulses: 1),
    recipe!(168, "uncertain_gesture_clarification", "Знакомый жест распознан неуверенно → один уточняющий наклон и ожидание повторения, без эскалации.", 1.35, 45.0, 1, false, false, CHANNEL_GAZE | CHANNEL_BROWS | CHANNEL_BODY;
        brow_asymmetry: 0.24, roll: 0.075, pulses: 1),
    recipe!(169, "shared_play_satisfaction", "Совместная игра завершена → короткое удовлетворённое выражение.", 2.90, 65.0, 1, false, false, CHANNEL_LIDS | CHANNEL_MOUTH | CHANNEL_BREATH;
        squint: 0.09, mouth_curve: 0.29, breath: -0.075, pulses: 1),
    recipe!(170, "socially_sated_rest", "Социальная насыщенность высока → заняться самостоятельным отдыхом.", 4.60, 120.0, 1, true, false, CHANNEL_LIDS | CHANNEL_BREATH;
        eye_aperture: -0.13, breath: -0.08, pulses: 1),
    // 18. Startle and recovery
    recipe!(171, "close_startle_whole_eye", "Внезапный близкий стимул → увеличить целую форму глаз и собрать тело.", 0.42, 12.0, 5, false, false, CHANNEL_LIDS | CHANNEL_MOUTH | CHANNEL_BODY;
        eye_scale: 0.45, eye_aperture: 0.4, mouth_open: 0.25, soft_yield: -0.12, pulses: 1),
    recipe!(172, "safe_startle_exhale", "Источник оказался безопасным → выдох и мягкое уменьшение глаз.", 3.00, 18.0, 5, false, false, CHANNEL_LIDS | CHANNEL_MOUTH | CHANNEL_BREATH;
        eye_scale: -0.04, mouth_open: 0.05, breath: -0.24, pulses: 1),
    recipe!(173, "distant_small_surprise", "Слабая неожиданность вдали → только бровь и саккада.", 0.65, 18.0, 5, false, false, CHANNEL_GAZE | CHANNEL_BROWS;
        brow_asymmetry: 0.31, pulses: 1),
    recipe!(174, "precision_error_refocus", "Ошибка точного касания → сосредоточиться перед новой попыткой.", 1.10, 15.0, 5, false, false, CHANNEL_GAZE | CHANNEL_LIDS | CHANNEL_MOUTH;
        squint: 0.16, mouth_open: -0.08, pulses: 1),
    recipe!(175, "repeated_failure_reconsider", "Несколько одинаковых неудач → сменить подход, а не дёргаться повторно.", 2.30, 45.0, 5, false, false, CHANNEL_GAZE | CHANNEL_BROWS | CHANNEL_MOUTH;
        brow_asymmetry: -0.29, mouth_asymmetry: -0.16, pulses: 1),
    recipe!(176, "vanished_task_object_check", "Предмет исчез во время задачи → проверить место и завершить невалидную цель.", 1.80, 25.0, 5, false, false, CHANNEL_GAZE | CHANNEL_BROWS;
        brow_asymmetry: 0.21, pulses: 1),
    recipe!(177, "blocked_route_assess", "Траектория заблокирована → замереть, оценить обход.", 2.00, 20.0, 5, false, false, CHANNEL_GAZE | CHANNEL_LIDS | CHANNEL_BROWS;
        squint: 0.15, brow_asymmetry: -0.13, pulses: 1),
    recipe!(178, "new_route_commit", "Обход найден → уверенно продолжить с уменьшенным колебанием.", 1.30, 20.0, 5, false, false, CHANNEL_GAZE | CHANNEL_MOUTH | CHANNEL_BODY;
        mouth_curve: 0.06, soft_yield: 0.045, pulses: 1),
    recipe!(179, "acceleration_shape_damp", "После сильного ускорения → погасить остаточную деформацию.", 1.70, 12.0, 5, false, false, CHANNEL_BODY;
        soft_yield: 0.21, pulses: 1),
    recipe!(180, "boundary_contact_recover", "После случайного удара о границу → восстановить дистанцию, затем спокойный взгляд.", 1.50, 10.0, 5, false, false, CHANNEL_GAZE | CHANNEL_LIDS | CHANNEL_BODY | CHANNEL_BREATH;
        eye_aperture: 0.08, soft_yield: 0.17, breath: -0.08, pulses: 1),
    // 19. Spatial exploration
    recipe!(181, "new_surface_edge_inspect", "Новая доступная поверхность → осмотреть её край.", 1.80, 35.0, 3, false, false, CHANNEL_GAZE | CHANNEL_BROWS | CHANNEL_BODY;
        brow_asymmetry: 0.18, roll: 0.065, pulses: 1),
    recipe!(182, "wide_surface_trial_settle", "Поверхность достаточно широка → пробная посадка с проверкой опоры.", 2.80, 45.0, 3, true, false, CHANNEL_GAZE | CHANNEL_BODY;
        soft_yield: 0.2, lean: -0.02, pulses: 1),
    recipe!(183, "narrow_surface_decline", "Поверхность слишком узкая → отказаться до опасного манёвра.", 1.60, 45.0, 3, false, false, CHANNEL_GAZE | CHANNEL_BROWS | CHANNEL_BODY;
        brow_asymmetry: -0.18, lean: -0.06, pulses: 1),
    recipe!(184, "relocated_home_survey", "Дом перемещён → исследовать новую окрестность.", 3.40, 75.0, 3, false, false, CHANNEL_GAZE | CHANNEL_BROWS | CHANNEL_BODY;
        brow_asymmetry: 0.26, roll: 0.035, pulses: 1),
    recipe!(185, "shorter_home_route_recognize", "Путь к дому стал короче → выбрать прямой маршрут после оценки.", 1.40, 35.0, 3, false, false, CHANNEL_GAZE | CHANNEL_BROWS | CHANNEL_MOUTH;
        brow_asymmetry: 0.14, mouth_curve: 0.13, pulses: 1),
    recipe!(186, "blocked_familiar_route_scan", "Знакомый путь перекрыт → проверить соседний проход.", 2.60, 40.0, 3, false, false, CHANNEL_GAZE | CHANNEL_BROWS | CHANNEL_BODY;
        brow_asymmetry: -0.21, roll: -0.06, pulses: 1),
    recipe!(187, "comfortable_corner_assess", "Найден удобный угол → прислониться и оценить комфорт.", 4.00, 75.0, 3, true, false, CHANNEL_BODY | CHANNEL_LIDS | CHANNEL_BREATH;
        soft_yield: 0.24, eye_aperture: -0.09, breath: -0.045, pulses: 1),
    recipe!(188, "removed_monitor_reorient", "Монитор удалён → выбрать валидную позицию восстановления.", 1.00, 10.0, 3, false, false, CHANNEL_GAZE | CHANNEL_LIDS | CHANNEL_BROWS;
        eye_aperture: 0.23, brow_asymmetry: 0.28, pulses: 1),
    recipe!(189, "moving_support_compensate", "Поверхность движется медленно → компенсировать её скорость на опоре.", 2.20, 10.0, 3, true, false, CHANNEL_GAZE | CHANNEL_BODY;
        soft_yield: 0.13, lean: 0.03, pulses: 1),
    recipe!(190, "fast_support_release_prepare", "Поверхность движется слишком быстро → безопасно отпустить опору.", 0.70, 12.0, 3, true, false, CHANNEL_GAZE | CHANNEL_LIDS | CHANNEL_BODY;
        eye_aperture: 0.21, soft_yield: 0.07, pulses: 1),
    // 20. Learned preference expression
    recipe!(191, "successful_place_side_confidence", "Несколько успешных укладок с одной стороны → предпочитать этот подход.", 1.60, 60.0, 1, false, true, CHANNEL_GAZE | CHANNEL_MOUTH | CHANNEL_BODY;
        mouth_curve: 0.14, roll: 0.02, pulses: 1),
    recipe!(192, "failed_habit_reassessment", "Прежний подход перестал работать → ослабить предпочтение после подтверждений.", 2.40, 90.0, 1, false, false, CHANNEL_GAZE | CHANNEL_BROWS | CHANNEL_MOUTH;
        brow_asymmetry: -0.24, mouth_asymmetry: -0.12, pulses: 1),
    recipe!(193, "familiar_stable_pose_ease", "Нашёл особенно устойчивую позу → чаще возвращаться к ней в сходном контексте.", 4.40, 100.0, 1, true, false, CHANNEL_BODY | CHANNEL_BREATH;
        soft_yield: 0.22, breath: -0.035, pulses: 1),
    recipe!(194, "fatigued_game_long_pause", "Несколько игр утомили → делать более длинную паузу между бросками.", 4.80, 120.0, 1, false, false, CHANNEL_LIDS | CHANNEL_BREATH;
        eye_aperture: -0.19, breath: -0.09, pulses: 1),
    recipe!(195, "accepted_game_offer", "Вариант игры часто принят → предлагать его при подходящей доступности.", 2.10, 90.0, 1, false, true, CHANNEL_GAZE | CHANNEL_BROWS | CHANNEL_MOUTH;
        brow_asymmetry: 0.22, mouth_curve: 0.21, pulses: 1),
    recipe!(196, "declined_game_self_redirect", "Вариант игры неоднократно отклонён → реже его предлагать.", 2.70, 120.0, 1, false, false, CHANNEL_GAZE | CHANNEL_MOUTH | CHANNEL_BREATH;
        mouth_curve: 0.025, breath: -0.05, pulses: 1),
    recipe!(197, "successful_gesture_pace", "Взгляд/жест дал понятный ответ → сохранять его темп в этом контексте.", 2.20, 75.0, 1, false, false, CHANNEL_GAZE | CHANNEL_BROWS | CHANNEL_MOUTH;
        brow_asymmetry: 0.075, mouth_curve: 0.16, pulses: 1),
    recipe!(198, "familiar_object_quick_check", "Незнакомый предмет стал знакомым → сокращать осмотр, оставляя короткую проверку.", 0.85, 45.0, 1, false, true, CHANNEL_GAZE | CHANNEL_MOUTH | CHANNEL_BREATH;
        mouth_curve: 0.065, breath: 0.035, pulses: 1),
    recipe!(199, "unavailable_favorite_support", "Любимая опора временно недоступна → выбрать запасную без фиксации на старом месте.", 2.50, 80.0, 1, false, false, CHANNEL_GAZE | CHANNEL_BROWS | CHANNEL_BODY;
        brow_asymmetry: -0.16, roll: -0.04, pulses: 1),
    recipe!(200, "long_absence_scene_recheck", "После длинного перерыва → осторожно перепроверить привычную сцену, не изображая обиду.", 3.70, 150.0, 1, false, false, CHANNEL_GAZE | CHANNEL_BROWS | CHANNEL_LIDS;
        brow_asymmetry: 0.195, eye_aperture: 0.075, pulses: 1),
];

pub fn repertoire_recipe(id: u16) -> Option<&'static RepertoireRecipe> {
    id.checked_sub(1)
        .and_then(|index| REPERTOIRE.get(index as usize))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn all_two_hundred_recipes_have_unique_stable_ids_names_and_causes() {
        assert_eq!(REPERTOIRE.len(), 200);
        let mut names = HashSet::new();
        let mut causes = HashSet::new();
        let mut family_counts = [0; 20];
        for (index, recipe) in REPERTOIRE.iter().enumerate() {
            assert_eq!(recipe.id as usize, index + 1);
            assert_eq!(recipe.family as usize, index / 10 + 1);
            assert!(names.insert(recipe.name), "duplicate name: {}", recipe.name);
            assert!(
                causes.insert(recipe.cause),
                "duplicate cause: {}",
                recipe.id
            );
            assert!(recipe.cause.contains('→'));
            assert!(
                recipe
                    .name
                    .bytes()
                    .all(|c| c.is_ascii_lowercase() || c == b'_')
            );
            family_counts[recipe.family as usize - 1] += 1;
            assert_eq!(repertoire_recipe(recipe.id), Some(recipe));
        }
        assert_eq!(family_counts, [10; 20]);
        assert_eq!(repertoire_recipe(0), None);
        assert_eq!(repertoire_recipe(201), None);
        assert_eq!(repertoire_recipe(u16::MAX), None);
    }

    #[test]
    fn phases_cooldowns_and_effects_are_finite_and_bounded() {
        for recipe in &REPERTOIRE {
            assert!(recipe.duration.is_finite() && (0.1..=10.0).contains(&recipe.duration));
            assert!(recipe.cooldown.is_finite() && (2.0..=900.0).contains(&recipe.cooldown));
            assert!(recipe.cooldown >= recipe.duration);
            assert!((1..=5).contains(&recipe.priority));
            assert!(recipe.channels != 0 && recipe.channels & !63 == 0);
            let e = recipe.effect;
            assert!((1..=3).contains(&e.pulses));
            for value in [
                e.brow_asymmetry,
                e.mouth_asymmetry,
                e.mouth_curve,
                e.mouth_open,
                e.eye_aperture,
                e.eye_scale,
                e.squint,
                e.breath,
                e.lean,
                e.roll,
                e.soft_yield,
            ] {
                assert!(
                    value.is_finite() && (-1.0..=1.0).contains(&value),
                    "invalid effect in {}",
                    recipe.name
                );
            }
            assert!(e.lean.abs() <= 0.2 && e.roll.abs() <= 0.1);
        }
    }

    #[test]
    fn nonzero_contributions_declare_their_channel() {
        for r in &REPERTOIRE {
            let e = r.effect;
            for (active, channel) in [
                (e.brow_asymmetry != 0.0, CHANNEL_BROWS),
                (
                    e.mouth_asymmetry != 0.0 || e.mouth_curve != 0.0 || e.mouth_open != 0.0,
                    CHANNEL_MOUTH,
                ),
                (
                    e.eye_aperture != 0.0 || e.eye_scale != 0.0 || e.squint != 0.0,
                    CHANNEL_LIDS,
                ),
                (e.breath != 0.0, CHANNEL_BREATH),
                (
                    e.lean != 0.0 || e.roll != 0.0 || e.soft_yield != 0.0,
                    CHANNEL_BODY,
                ),
            ] {
                assert!(
                    !active || r.channels & channel != 0,
                    "{} channel {}",
                    r.name,
                    channel
                );
            }
        }
    }

    #[test]
    fn quiet_sleep_breath_has_no_eye_or_root_motion_contribution() {
        let sleep = repertoire_recipe(113).unwrap();
        assert!(sleep.requires_support);
        assert_eq!(sleep.channels, CHANNEL_BREATH);
        assert_eq!(sleep.effect.eye_aperture, 0.0);
        assert_eq!(sleep.effect.eye_scale, 0.0);
        assert_eq!(sleep.effect.lean, 0.0);
        assert_eq!(sleep.effect.roll, 0.0);
        assert_eq!(RepertoireEffect::ZERO, RepertoireEffect::default());
    }
}
