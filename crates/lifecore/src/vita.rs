use std::array;

use glam::Vec2;
use serde::{Deserialize, Serialize};

use crate::{
    ACTION_COUNT, ActionId, BodyFeedback, BodyIntent, EpisodeContextV1, ExpressionState,
    FeedbackEvent, FeltStateV1, InteractionTarget, LifeState, LocomotionMode, PoseIntent,
    SensorFrame,
};

pub const VITA_STATE_SCHEMA_VERSION: u32 = 1;
pub const INFLUENCE_STRATEGY_COUNT: usize = 12;
pub const VITA_CONTEXT_SIZE: usize = 12;
const MAX_EMOTION_EPISODES: usize = 4;
const MAX_FAVORITE_PLACES: usize = 24;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StimulusKind {
    Cursor,
    PointerGesture,
    TypingRhythm,
    ScrollFlow,
    WindowEdge,
    MovingWindow,
    Popup,
    VisualChange,
    BrightArea,
    DarkArea,
    User,
    SelfBody,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct StimulusEvent {
    pub kind: StimulusKind,
    pub position: Option<Vec2>,
    pub intensity: f32,
    pub novelty: f32,
    pub threat: f32,
    pub social_relevance: f32,
}

impl StimulusEvent {
    #[must_use]
    pub fn bounded(mut self) -> Self {
        self.position = self
            .position
            .map(|position| position.clamp(Vec2::ZERO, Vec2::ONE));
        self.intensity = self.intensity.clamp(0.0, 1.0);
        self.novelty = self.novelty.clamp(0.0, 1.0);
        self.threat = self.threat.clamp(0.0, 1.0);
        self.social_relevance = self.social_relevance.clamp(0.0, 1.0);
        self
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct PointerGesturePercept {
    pub approach: f32,
    pub avoid: f32,
    pub poke: f32,
    pub circle: f32,
    pub chase_invitation: f32,
    pub petting: f32,
    pub fast_swipe: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VitaPerceptFrame {
    pub pointer: PointerGesturePercept,
    pub typing_rate_hz: f32,
    pub typing_burstiness: f32,
    pub typing_pause_seconds: f32,
    pub click_rate_hz: f32,
    pub scroll_velocity: f32,
    pub scroll_burstiness: f32,
    pub window_motion: f32,
    pub window_pressure: f32,
    pub popup_salience: f32,
    pub nearest_window_edge: Option<Vec2>,
    pub mean_luminance: Option<f32>,
    pub local_luminance: Option<f32>,
    pub colorfulness: Option<f32>,
    pub warmth: Option<f32>,
    pub dominant_hue: Option<f32>,
    pub visual_motion: Option<f32>,
    pub visual_change: Option<f32>,
    pub user_available: f32,
    pub events: Vec<StimulusEvent>,
}

impl Default for VitaPerceptFrame {
    fn default() -> Self {
        Self {
            pointer: PointerGesturePercept::default(),
            typing_rate_hz: 0.0,
            typing_burstiness: 0.0,
            typing_pause_seconds: 60.0,
            click_rate_hz: 0.0,
            scroll_velocity: 0.0,
            scroll_burstiness: 0.0,
            window_motion: 0.0,
            window_pressure: 0.0,
            popup_salience: 0.0,
            nearest_window_edge: None,
            mean_luminance: None,
            local_luminance: None,
            colorfulness: None,
            warmth: None,
            dominant_hue: None,
            visual_motion: None,
            visual_change: None,
            user_available: 0.5,
            events: Vec::new(),
        }
    }
}

impl VitaPerceptFrame {
    pub fn sanitize(&mut self) {
        for value in [
            &mut self.pointer.approach,
            &mut self.pointer.avoid,
            &mut self.pointer.poke,
            &mut self.pointer.circle,
            &mut self.pointer.chase_invitation,
            &mut self.pointer.petting,
            &mut self.pointer.fast_swipe,
            &mut self.typing_burstiness,
            &mut self.scroll_burstiness,
            &mut self.window_motion,
            &mut self.window_pressure,
            &mut self.popup_salience,
            &mut self.user_available,
        ] {
            *value = finite_unit(*value);
        }
        self.typing_rate_hz = finite(self.typing_rate_hz, 0.0).clamp(0.0, 24.0);
        self.typing_pause_seconds = finite(self.typing_pause_seconds, 60.0).clamp(0.0, 3_600.0);
        self.click_rate_hz = finite(self.click_rate_hz, 0.0).clamp(0.0, 20.0);
        self.scroll_velocity = finite(self.scroll_velocity, 0.0).clamp(-1.0, 1.0);
        for value in [
            &mut self.mean_luminance,
            &mut self.local_luminance,
            &mut self.colorfulness,
            &mut self.warmth,
            &mut self.dominant_hue,
            &mut self.visual_motion,
            &mut self.visual_change,
        ] {
            *value = value.map(finite_unit);
        }
        self.nearest_window_edge = self
            .nearest_window_edge
            .filter(|position| position.is_finite())
            .map(|position| position.clamp(Vec2::ZERO, Vec2::ONE));
        self.events.truncate(16);
        for event in &mut self.events {
            *event = event.bounded();
        }
    }
}

/// Privacy-safe interpretation of the shared desktop rhythm. This never stores
/// text, window titles, application names, or pixels: only slow scalar traces
/// that help the organism distinguish work from an opening for interaction.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DesktopContextKind {
    Away,
    #[default]
    Settled,
    Working,
    AmbientMotion,
    Pause,
    Interactive,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct DesktopRhythmState {
    pub context: DesktopContextKind,
    pub focused_work_seconds: f32,
    pub quiet_seconds: f32,
    pub work_memory: f32,
    pub stimulation: f32,
    pub social_opening: f32,
    pub interruption_risk: f32,
}

impl Default for DesktopRhythmState {
    fn default() -> Self {
        Self {
            context: DesktopContextKind::Settled,
            focused_work_seconds: 0.0,
            quiet_seconds: 0.0,
            work_memory: 0.0,
            stimulation: 0.0,
            social_opening: 0.25,
            interruption_risk: 0.20,
        }
    }
}

impl DesktopRhythmState {
    fn update(&mut self, percept: &VitaPerceptFrame, life: &LifeState, dt: f32) {
        let typing = (percept.typing_rate_hz / 6.0).clamp(0.0, 1.0);
        let scroll = percept.scroll_velocity.abs().clamp(0.0, 1.0);
        let visual = percept
            .visual_motion
            .unwrap_or(0.0)
            .max(percept.visual_change.unwrap_or(0.0))
            .max(percept.window_motion)
            .clamp(0.0, 1.0);
        let explicit_play = percept
            .pointer
            .petting
            .max(percept.pointer.chase_invitation)
            .max(percept.pointer.circle * 0.72)
            .max(percept.pointer.poke * 0.45);
        let work_signal = (typing * 0.72 + scroll * 0.16 + visual * 0.12).clamp(0.0, 1.0);
        let stimulation_target = work_signal.max(visual * 0.68);
        self.stimulation = smooth(self.stimulation, stimulation_target, 2.2, dt);
        self.work_memory = smooth(self.work_memory, work_signal, 0.16, dt);

        if typing > 0.06 || (work_signal > 0.16 && percept.typing_pause_seconds < 1.0) {
            self.focused_work_seconds = (self.focused_work_seconds + dt).min(3_600.0);
        } else {
            self.focused_work_seconds = (self.focused_work_seconds - dt * 0.42).max(0.0);
        }
        if stimulation_target < 0.08 {
            self.quiet_seconds = (self.quiet_seconds + dt).min(3_600.0);
        } else {
            self.quiet_seconds = 0.0;
        }

        let remembered_work = self.focused_work_seconds > 2.5 || self.work_memory > 0.10;
        let brief_pause = remembered_work
            && work_signal < 0.10
            && (0.8..=18.0).contains(&percept.typing_pause_seconds);
        self.context = if explicit_play > 0.18 {
            DesktopContextKind::Interactive
        } else if percept.user_available < 0.10 && work_signal < 0.05 {
            DesktopContextKind::Away
        } else if typing > 0.06 || (work_signal > 0.14 && percept.typing_pause_seconds < 1.0) {
            DesktopContextKind::Working
        } else if brief_pause {
            DesktopContextKind::Pause
        } else if visual > 0.10 || scroll > 0.08 {
            DesktopContextKind::AmbientMotion
        } else {
            DesktopContextKind::Settled
        };

        let ignored = (life.ignored_attempts as f32 / 5.0).clamp(0.0, 1.0);
        let opening_target = match self.context {
            DesktopContextKind::Away => 0.0,
            DesktopContextKind::Working => 0.035,
            DesktopContextKind::AmbientMotion => 0.12,
            DesktopContextKind::Settled => 0.30,
            DesktopContextKind::Pause => 0.62,
            DesktopContextKind::Interactive => 1.0,
        } * percept.user_available
            * (1.0 - ignored * 0.72);
        let risk_target = match self.context {
            DesktopContextKind::Away => 0.90,
            DesktopContextKind::Working => 0.92,
            DesktopContextKind::AmbientMotion => 0.58,
            DesktopContextKind::Settled => 0.30,
            DesktopContextKind::Pause => 0.16,
            DesktopContextKind::Interactive => 0.02,
        } + ignored * 0.20;
        self.social_opening = smooth(self.social_opening, opening_target.clamp(0.0, 1.0), 1.8, dt);
        self.interruption_risk =
            smooth(self.interruption_risk, risk_target.clamp(0.0, 1.0), 2.2, dt);
    }

    #[must_use]
    pub fn protects_focused_work(&self) -> bool {
        self.context == DesktopContextKind::Working && self.focused_work_seconds >= 10.0
    }

    fn is_valid(&self) -> bool {
        [
            self.focused_work_seconds,
            self.quiet_seconds,
            self.work_memory,
            self.stimulation,
            self.social_opening,
            self.interruption_risk,
        ]
        .into_iter()
        .all(f32::is_finite)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttentionKind {
    Cursor,
    Gesture,
    Typing,
    Scroll,
    Window,
    Popup,
    Visual,
    Viewer,
    SelfBody,
    Memory,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct AttentionState {
    pub kind: AttentionKind,
    pub position: Option<Vec2>,
    pub confidence: f32,
    pub commitment_remaining: f32,
    pub habituation: f32,
}

impl Default for AttentionState {
    fn default() -> Self {
        Self {
            kind: AttentionKind::Cursor,
            position: Some(Vec2::splat(0.5)),
            confidence: 0.2,
            commitment_remaining: 0.0,
            habituation: 0.0,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct AppraisalState {
    pub novelty: f32,
    pub expectedness: f32,
    pub controllability: f32,
    pub goal_congruence: f32,
    pub social_relevance: f32,
    pub agency: f32,
    pub certainty: f32,
    pub threat: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EmotionKind {
    Curiosity,
    Delight,
    Affection,
    Contentment,
    Surprise,
    Alarm,
    Fear,
    Frustration,
    Boredom,
    Shyness,
    Pride,
}

/// A legible, stylized face target for each VITA emotion.
///
/// These targets deliberately coordinate several cues instead of asking any one
/// feature to carry the whole emotion. Mouth curvature owns valence, brow shape
/// and eyelid tension disambiguate it, and pupil size follows emotional arousal
/// rather than positive/negative valence.
#[must_use]
pub fn expression_target_for_emotion(kind: EmotionKind) -> ExpressionState {
    let mut target = ExpressionState::default();
    match kind {
        EmotionKind::Curiosity => {
            target.pupil_size = 0.68;
            target.pupil_focus = 0.94;
            target.brow_raise = 0.42;
            target.brow_tension = 0.08;
            target.mouth_curve = 0.18;
            target.mouth_tension = 0.08;
            target.cheek_glow = 0.12;
            target.body_glow = 0.58;
        }
        EmotionKind::Delight => {
            target.squint = 0.38;
            target.pupil_size = 0.76;
            target.pupil_focus = 0.84;
            target.brow_raise = 0.24;
            target.mouth_curve = 0.96;
            target.mouth_tension = 0.08;
            target.cheek_glow = 0.92;
            target.body_glow = 0.94;
        }
        EmotionKind::Affection => {
            target.squint = 0.28;
            target.pupil_size = 0.66;
            target.pupil_focus = 0.96;
            target.brow_raise = 0.18;
            target.mouth_curve = 0.66;
            target.mouth_tension = 0.04;
            target.cheek_glow = 1.0;
            target.body_glow = 0.78;
        }
        EmotionKind::Contentment => {
            target.squint = 0.32;
            target.pupil_size = 0.52;
            target.pupil_focus = 0.58;
            target.brow_raise = -0.10;
            target.mouth_curve = 0.48;
            target.cheek_glow = 0.42;
            target.body_glow = 0.46;
        }
        EmotionKind::Surprise => {
            target.pupil_size = 0.88;
            target.pupil_focus = 0.92;
            target.brow_raise = 1.0;
            target.brow_tension = 0.06;
            target.mouth_curve = 0.04;
            target.mouth_tension = 0.20;
            target.body_glow = 0.92;
        }
        EmotionKind::Alarm => {
            target.squint = 0.10;
            target.pupil_size = 0.90;
            target.pupil_focus = 0.98;
            target.brow_raise = 0.70;
            target.brow_tension = 0.68;
            target.mouth_curve = -0.42;
            target.mouth_tension = 0.62;
            target.body_glow = 0.88;
        }
        EmotionKind::Fear => {
            target.squint = 0.05;
            target.pupil_size = 0.92;
            target.pupil_focus = 0.96;
            target.brow_raise = 0.52;
            target.brow_tension = 0.86;
            target.mouth_curve = -0.82;
            target.mouth_tension = 0.58;
            target.body_glow = 0.80;
        }
        EmotionKind::Frustration => {
            target.squint = 0.72;
            target.pupil_size = 0.70;
            target.pupil_focus = 0.90;
            target.brow_raise = -0.52;
            target.brow_tension = 1.0;
            target.mouth_curve = -0.62;
            target.mouth_tension = 1.0;
            target.body_glow = 0.72;
        }
        EmotionKind::Boredom => {
            target.squint = 0.48;
            target.pupil_size = 0.40;
            target.pupil_focus = 0.18;
            target.brow_raise = -0.46;
            target.brow_tension = 0.04;
            target.mouth_curve = -0.22;
            target.mouth_tension = 0.04;
            target.body_glow = 0.24;
        }
        EmotionKind::Shyness => {
            target.squint = 0.24;
            target.pupil_size = 0.58;
            target.pupil_focus = 0.20;
            target.brow_raise = 0.24;
            target.brow_tension = 0.22;
            target.mouth_curve = 0.22;
            target.mouth_tension = 0.20;
            target.cheek_glow = 0.94;
            target.body_glow = 0.48;
        }
        EmotionKind::Pride => {
            target.squint = 0.24;
            target.pupil_size = 0.62;
            target.pupil_focus = 0.78;
            target.brow_raise = 0.28;
            target.brow_tension = 0.06;
            target.mouth_curve = 0.70;
            target.mouth_tension = 0.18;
            target.cheek_glow = 0.30;
            target.body_glow = 0.82;
        }
    }
    target
}

/// Blends a VITA emotion into an existing neural/affective expression while
/// preserving procedural blinks and audio-owned mouth opening.
pub fn apply_emotion_to_expression(
    expression: &mut ExpressionState,
    kind: EmotionKind,
    intensity: f32,
) {
    // Square-root response keeps medium episodes readable without flattening
    // the intensity range of strong emotional moments.
    let amount = intensity.clamp(0.0, 1.0).sqrt();
    blend_expression_toward(expression, expression_target_for_emotion(kind), amount);
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct EmotionEpisode {
    pub kind: EmotionKind,
    pub intensity: f32,
    pub remaining_seconds: f32,
    pub cause: StimulusKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct MoodState {
    pub baseline_valence: f32,
    pub baseline_arousal: f32,
    pub social_openness: f32,
    pub confidence: f32,
    pub fatigue: f32,
}

impl Default for MoodState {
    fn default() -> Self {
        Self {
            baseline_valence: 0.12,
            baseline_arousal: 0.28,
            social_openness: 0.40,
            confidence: 0.45,
            fatigue: 0.12,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct ActionForwardModel {
    pub mean_delta_position: Vec2,
    pub mean_delta_velocity: Vec2,
    pub contact_probability: f32,
    pub confidence: f32,
    pub samples: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SelfModel {
    pub action_models: [ActionForwardModel; ACTION_COUNT],
    pub previous_position: Vec2,
    pub previous_velocity: Vec2,
    pub last_action: ActionId,
    pub prediction_error: f32,
    pub agency: f32,
    pub uncertainty: f32,
    pub body_schema_confidence: f32,
    pub calibration_urge: f32,
    pub external_force_likelihood: f32,
}

impl Default for SelfModel {
    fn default() -> Self {
        Self {
            action_models: array::from_fn(|_| ActionForwardModel::default()),
            previous_position: Vec2::splat(0.5),
            previous_velocity: Vec2::ZERO,
            last_action: ActionId::IdleHover,
            prediction_error: 0.0,
            agency: 0.5,
            uncertainty: 0.55,
            body_schema_confidence: 0.25,
            calibration_urge: 0.0,
            external_force_likelihood: 0.0,
        }
    }
}

impl SelfModel {
    pub fn update(&mut self, action: ActionId, body: &BodyFeedback, dt: f32) {
        let dt = finite(dt, 0.0).clamp(0.0, 0.25);
        let observed_delta = body.world_position - self.previous_position;
        let observed_velocity_delta = body.velocity - self.previous_velocity;
        let previous_model = self.action_models[self.last_action.index()];
        let position_error = observed_delta.distance(previous_model.mean_delta_position);
        let velocity_error = observed_velocity_delta.distance(previous_model.mean_delta_velocity);
        let contact_observed = bool_value(body.grounded || body.clinging || body.cursor_contact);
        let contact_error = (contact_observed - previous_model.contact_probability).abs();
        let combined_error =
            (position_error * 4.0 + velocity_error * 1.5 + contact_error * 0.25).clamp(0.0, 1.0);
        self.prediction_error = smooth(self.prediction_error, combined_error, 6.0, dt);
        let null_error =
            (observed_delta.length() * 4.0 + observed_velocity_delta.length()).clamp(0.0, 1.0);
        self.agency = smooth(
            self.agency,
            logistic((null_error - combined_error) * 8.0),
            4.0,
            dt,
        );
        self.external_force_likelihood = smooth(
            self.external_force_likelihood,
            (body.cursor_contact as u8 as f32 * 0.65 + (1.0 - self.agency) * 0.35).clamp(0.0, 1.0),
            5.0,
            dt,
        );

        let model = &mut self.action_models[self.last_action.index()];
        let learning_rate = (0.20 / (1.0 + model.samples as f32 * 0.012)).clamp(0.015, 0.20);
        model.mean_delta_position += (observed_delta - model.mean_delta_position) * learning_rate;
        model.mean_delta_velocity +=
            (observed_velocity_delta - model.mean_delta_velocity) * learning_rate;
        model.contact_probability += (contact_observed - model.contact_probability) * learning_rate;
        model.confidence =
            (model.confidence + learning_rate * (1.0 - combined_error)).clamp(0.0, 1.0);
        model.samples = model.samples.saturating_add(1);

        let active_confidence = model.confidence;
        self.body_schema_confidence = smooth(
            self.body_schema_confidence,
            (active_confidence * (1.0 - self.prediction_error * 0.7)).clamp(0.0, 1.0),
            0.8,
            dt,
        );
        self.uncertainty = smooth(
            self.uncertainty,
            (1.0 - self.body_schema_confidence + self.prediction_error * 0.55).clamp(0.0, 1.0),
            1.4,
            dt,
        );
        self.calibration_urge = smooth(
            self.calibration_urge,
            ((self.uncertainty - 0.48) * 1.8).clamp(0.0, 1.0),
            1.1,
            dt,
        );
        self.previous_position = body.world_position;
        self.previous_velocity = body.velocity;
        self.last_action = action;
    }

    pub fn note_metamorphosis(&mut self) {
        self.body_schema_confidence *= 0.35;
        self.uncertainty = self.uncertainty.max(0.82);
        self.calibration_urge = self.calibration_urge.max(0.78);
        for model in &mut self.action_models {
            model.confidence *= 0.45;
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InfluenceMode {
    Off,
    Gentle,
    #[default]
    Playful,
    Experimental,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InfluenceStrategy {
    DirectEyeContact,
    PeekAndWithdraw,
    SilentProximity,
    DelayedChirp,
    MimicTypingRhythm,
    BringToy,
    PlayfulRefusal,
    ReciprocalGesture,
    RareAffection,
    GradualEscalation,
    SelfPlayDisplay,
    HideUntilSought,
}

impl InfluenceStrategy {
    pub const ALL: [Self; INFLUENCE_STRATEGY_COUNT] = [
        Self::DirectEyeContact,
        Self::PeekAndWithdraw,
        Self::SilentProximity,
        Self::DelayedChirp,
        Self::MimicTypingRhythm,
        Self::BringToy,
        Self::PlayfulRefusal,
        Self::ReciprocalGesture,
        Self::RareAffection,
        Self::GradualEscalation,
        Self::SelfPlayDisplay,
        Self::HideUntilSought,
    ];

    #[must_use]
    pub const fn index(self) -> usize {
        self as usize
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InfluenceReason {
    pub social_drive: f32,
    pub play_drive: f32,
    pub user_available: f32,
    pub typing_pause_seconds: f32,
    pub learned_value: f32,
    pub annoyance_risk: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InfluenceDecision {
    pub strategy: InfluenceStrategy,
    pub intensity: f32,
    pub reason: InfluenceReason,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct PendingInfluence {
    strategy: InfluenceStrategy,
    context: [f32; VITA_CONTEXT_SIZE],
    elapsed: f32,
    response_window: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InfluencePolicy {
    pub mode: InfluenceMode,
    pub weights: Vec<[f32; VITA_CONTEXT_SIZE]>,
    pub attempts: [u32; INFLUENCE_STRATEGY_COUNT],
    pub successes: [u32; INFLUENCE_STRATEGY_COUNT],
    pub cooldown_seconds: f32,
    pub consecutive_ignored: u32,
    pending: Option<PendingInfluence>,
}

impl Default for InfluencePolicy {
    fn default() -> Self {
        Self {
            mode: InfluenceMode::Playful,
            weights: vec![[0.0; VITA_CONTEXT_SIZE]; INFLUENCE_STRATEGY_COUNT],
            attempts: [0; INFLUENCE_STRATEGY_COUNT],
            successes: [0; INFLUENCE_STRATEGY_COUNT],
            cooldown_seconds: 0.0,
            consecutive_ignored: 0,
            pending: None,
        }
    }
}

impl InfluencePolicy {
    fn tick_pending(&mut self, dt: f32) {
        self.cooldown_seconds = (self.cooldown_seconds - dt).max(0.0);
        let Some(pending) = &mut self.pending else {
            return;
        };
        pending.elapsed += dt;
        if pending.elapsed >= pending.response_window {
            let expired = self.pending.take().expect("pending influence exists");
            self.learn(expired.strategy, &expired.context, -0.22);
            self.consecutive_ignored = self.consecutive_ignored.saturating_add(1);
            self.cooldown_seconds = 15.0 + self.consecutive_ignored.min(6) as f32 * 8.0;
        }
    }

    pub fn apply_feedback(&mut self, event: &FeedbackEvent) {
        let reward = event.reward();
        let Some(pending) = self.pending.take() else {
            return;
        };
        self.learn(pending.strategy, &pending.context, reward);
        if reward > 0.15 {
            let index = pending.strategy.index();
            self.successes[index] = self.successes[index].saturating_add(1);
            self.consecutive_ignored = self.consecutive_ignored.saturating_sub(1);
            self.cooldown_seconds = 8.0;
        } else if reward < 0.0 {
            self.consecutive_ignored = self.consecutive_ignored.saturating_add(1);
            self.cooldown_seconds = 20.0 + self.consecutive_ignored.min(6) as f32 * 10.0;
        }
    }

    fn learn(
        &mut self,
        strategy: InfluenceStrategy,
        context: &[f32; VITA_CONTEXT_SIZE],
        reward: f32,
    ) {
        let index = strategy.index();
        let prediction = dot(&self.weights[index], context);
        let error = reward.clamp(-1.0, 1.0) - prediction;
        let learning_rate = 0.025;
        for (weight, value) in self.weights[index].iter_mut().zip(context) {
            *weight = (*weight + learning_rate * error * value).clamp(-1.5, 1.5);
        }
    }

    fn select(
        &mut self,
        life: &LifeState,
        percept: &VitaPerceptFrame,
        desktop: &DesktopRhythmState,
    ) -> Option<InfluenceDecision> {
        let explicit_invitation = percept
            .pointer
            .petting
            .max(percept.pointer.chase_invitation)
            .max(percept.pointer.circle * 0.70);
        if self.mode == InfluenceMode::Off
            || life.focus_mode
            || self.cooldown_seconds > 0.0
            || self.pending.is_some()
            || (percept.user_available < 0.18 && explicit_invitation < 0.18)
            || (matches!(
                desktop.context,
                DesktopContextKind::Working | DesktopContextKind::Away
            ) && explicit_invitation < 0.18)
            || (desktop.social_opening < 0.16 && explicit_invitation < 0.18)
            || life.attention_budget.current < 0.08
        {
            return None;
        }
        let motivation = life.drives.social.max(life.drives.play * 0.78)
            * (0.72 + desktop.social_opening * 0.28);
        if motivation < 0.38 && explicit_invitation < 0.18 {
            return None;
        }
        let context = influence_context(life, percept, self.consecutive_ignored);
        let annoyance_risk = (percept.typing_rate_hz / 8.0 * 0.62
            + self.consecutive_ignored as f32 * 0.13
            + (1.0 - percept.user_available) * 0.38)
            .max(desktop.interruption_risk)
            .clamp(0.0, 1.0);
        let mut best = InfluenceStrategy::DirectEyeContact;
        let mut best_score = f32::NEG_INFINITY;
        for strategy in InfluenceStrategy::ALL {
            if !strategy_allowed(self.mode, strategy) {
                continue;
            }
            if desktop.context == DesktopContextKind::Pause
                && explicit_invitation < 0.18
                && strategy_intrusiveness(strategy) > 0.12
            {
                continue;
            }
            let learned = dot(&self.weights[strategy.index()], &context);
            let score = learned + strategy_prior(strategy, life, percept)
                - annoyance_risk * strategy_intrusiveness(strategy)
                + deterministic_jitter(life.tick_count, strategy.index()) * 0.05;
            if score > best_score {
                best = strategy;
                best_score = score;
            }
        }
        let intensity = (motivation * (1.0 - annoyance_risk * 0.62) + explicit_invitation * 0.30)
            .clamp(0.10, 0.90);
        self.attempts[best.index()] = self.attempts[best.index()].saturating_add(1);
        self.pending = Some(PendingInfluence {
            strategy: best,
            context,
            elapsed: 0.0,
            response_window: 4.5 + life.genome.temperament.persistence * 5.5,
        });
        self.cooldown_seconds = 4.0;
        Some(InfluenceDecision {
            strategy: best,
            intensity,
            reason: InfluenceReason {
                social_drive: life.drives.social,
                play_drive: life.drives.play,
                user_available: percept.user_available,
                typing_pause_seconds: percept.typing_pause_seconds,
                learned_value: dot(&self.weights[best.index()], &context),
                annoyance_risk,
            },
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FavoritePlace {
    pub relative_position: Vec2,
    pub app_category_index: usize,
    pub edge_preference: f32,
    pub hue: Option<f32>,
    pub comfort_value: f32,
    pub play_value: f32,
    pub safety_value: f32,
    pub visits: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VitaState {
    #[serde(default)]
    pub next_gesture_episode_id: u64,
    pub schema_version: u32,
    pub attention: AttentionState,
    pub appraisal: AppraisalState,
    pub mood: MoodState,
    pub emotions: Vec<EmotionEpisode>,
    pub self_model: SelfModel,
    pub influence: InfluencePolicy,
    #[serde(default)]
    pub desktop_rhythm: DesktopRhythmState,
    pub favorite_places: Vec<FavoritePlace>,
    pub elapsed_seconds: f64,
    pub attention_switches: u64,
}

impl Default for VitaState {
    fn default() -> Self {
        Self {
            next_gesture_episode_id: 1,
            schema_version: VITA_STATE_SCHEMA_VERSION,
            attention: AttentionState::default(),
            appraisal: AppraisalState::default(),
            mood: MoodState::default(),
            emotions: Vec::new(),
            self_model: SelfModel::default(),
            influence: InfluencePolicy::default(),
            desktop_rhythm: DesktopRhythmState::default(),
            favorite_places: Vec::new(),
            elapsed_seconds: 0.0,
            attention_switches: 0,
        }
    }
}

impl VitaState {
    #[must_use]
    pub fn is_valid(&self) -> bool {
        let appraisal = [
            self.appraisal.novelty,
            self.appraisal.expectedness,
            self.appraisal.controllability,
            self.appraisal.goal_congruence,
            self.appraisal.social_relevance,
            self.appraisal.agency,
            self.appraisal.certainty,
            self.appraisal.threat,
        ];
        let mood = [
            self.mood.baseline_valence,
            self.mood.baseline_arousal,
            self.mood.social_openness,
            self.mood.confidence,
            self.mood.fatigue,
        ];
        self.schema_version == VITA_STATE_SCHEMA_VERSION
            && self.elapsed_seconds.is_finite()
            && self.attention.position.is_none_or(Vec2::is_finite)
            && self.attention.confidence.is_finite()
            && self.attention.commitment_remaining.is_finite()
            && self.attention.habituation.is_finite()
            && appraisal.into_iter().all(f32::is_finite)
            && mood.into_iter().all(f32::is_finite)
            && self.emotions.len() <= MAX_EMOTION_EPISODES
            && self.emotions.iter().all(|episode| {
                episode.intensity.is_finite() && episode.remaining_seconds.is_finite()
            })
            && self.self_model.previous_position.is_finite()
            && self.self_model.previous_velocity.is_finite()
            && [
                self.self_model.prediction_error,
                self.self_model.agency,
                self.self_model.uncertainty,
                self.self_model.body_schema_confidence,
                self.self_model.calibration_urge,
                self.self_model.external_force_likelihood,
            ]
            .into_iter()
            .all(f32::is_finite)
            && self.self_model.action_models.iter().all(|model| {
                model.mean_delta_position.is_finite()
                    && model.mean_delta_velocity.is_finite()
                    && model.contact_probability.is_finite()
                    && model.confidence.is_finite()
            })
            && self.influence.weights.len() == INFLUENCE_STRATEGY_COUNT
            && self
                .influence
                .weights
                .iter()
                .flatten()
                .all(|weight| weight.is_finite() && weight.abs() <= 1.501)
            && self.influence.cooldown_seconds.is_finite()
            && self.desktop_rhythm.is_valid()
            && self.favorite_places.len() <= MAX_FAVORITE_PLACES
            && self.favorite_places.iter().all(|place| {
                place.relative_position.is_finite()
                    && place.hue.is_none_or(f32::is_finite)
                    && place.comfort_value.is_finite()
                    && place.play_value.is_finite()
                    && place.safety_value.is_finite()
            })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VitaMind {
    pub state: VitaState,
    identity_seed: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VitaOutput {
    pub attention: AttentionState,
    pub appraisal: AppraisalState,
    pub dominant_emotion: Option<EmotionEpisode>,
    pub influence: Option<InfluenceDecision>,
    pub self_check: bool,
    pub gaze_target: Option<Vec2>,
    pub direct_viewer_gaze: bool,
    pub pose_override: Option<PoseIntent>,
    pub locomotion_override: Option<LocomotionMode>,
    pub target_override: Option<Vec2>,
    pub interaction_override: Option<InteractionTarget>,
}

impl VitaOutput {
    pub fn apply_to_intent(&self, intent: &mut BodyIntent) {
        if let Some(gaze_target) = self.gaze_target {
            intent.gaze_target = Some(gaze_target.clamp(Vec2::ZERO, Vec2::ONE));
        }
        if self.direct_viewer_gaze {
            intent.interaction_target = Some(InteractionTarget::User);
        }
        if let Some(pose) = self.pose_override {
            intent.pose = pose;
        }
        if let Some(locomotion) = self.locomotion_override {
            intent.locomotion = locomotion;
        }
        if let Some(target) = self.target_override {
            intent.target_position = target.clamp(Vec2::ZERO, Vec2::ONE);
        }
        if let Some(target) = &self.interaction_override {
            intent.interaction_target = Some(target.clone());
        }
        if let Some(emotion) = self.dominant_emotion {
            apply_emotion_to_expression(&mut intent.expression, emotion.kind, emotion.intensity);
        }
    }
}

fn blend_expression_toward(current: &mut ExpressionState, target: ExpressionState, amount: f32) {
    let amount = amount.clamp(0.0, 1.0);
    let blend = |from: f32, to: f32| from + (to - from) * amount;
    // Blinks and mouth opening remain procedural/audio-owned. Emotional intent
    // controls the surrounding shape so the face never mimes a failed voice.
    current.squint = blend(current.squint, target.squint).clamp(0.0, 1.0);
    current.pupil_size = blend(current.pupil_size, target.pupil_size).clamp(0.0, 1.0);
    current.pupil_focus = blend(current.pupil_focus, target.pupil_focus).clamp(0.0, 1.0);
    current.brow_raise = blend(current.brow_raise, target.brow_raise).clamp(-1.0, 1.0);
    current.brow_tension = blend(current.brow_tension, target.brow_tension).clamp(0.0, 1.0);
    current.mouth_curve = blend(current.mouth_curve, target.mouth_curve).clamp(-1.0, 1.0);
    current.mouth_tension = blend(current.mouth_tension, target.mouth_tension).clamp(0.0, 1.0);
    current.cheek_glow = blend(current.cheek_glow, target.cheek_glow).clamp(0.0, 1.0);
    current.body_glow = blend(current.body_glow, target.body_glow).clamp(0.0, 1.0);
}

impl VitaMind {
    #[must_use]
    pub fn new(identity_seed: u64) -> Self {
        Self {
            state: VitaState::default(),
            identity_seed,
        }
    }

    #[must_use]
    pub fn restore(identity_seed: u64, mut state: VitaState) -> Self {
        if state.schema_version != VITA_STATE_SCHEMA_VERSION {
            state = VitaState::default();
        }
        state.favorite_places.truncate(MAX_FAVORITE_PLACES);
        state.emotions.truncate(MAX_EMOTION_EPISODES);
        Self {
            state,
            identity_seed,
        }
    }

    pub fn tick(
        &mut self,
        percept: &VitaPerceptFrame,
        sensors: &SensorFrame,
        life: &LifeState,
        body: &BodyFeedback,
        base_intent: &BodyIntent,
        dt: f32,
    ) -> VitaOutput {
        let dt = finite(dt, 0.0).clamp(0.0, 0.25);
        self.state.elapsed_seconds += f64::from(dt);
        self.state.influence.tick_pending(dt);
        self.state.desktop_rhythm.update(percept, life, dt);
        self.state.self_model.update(life.current_action, body, dt);
        self.update_appraisal(percept, life, body, dt);
        self.update_attention(percept, sensors, life, dt);
        self.update_emotions(percept, life, dt);
        self.update_mood(life, dt);
        self.learn_place(percept, sensors, life, body, dt);
        let influence = self
            .state
            .influence
            .select(life, percept, &self.state.desktop_rhythm);
        self.compose_output(percept, base_intent, influence)
    }

    /// Integrates previous-tick body evidence into self-model, appraisal and
    /// slow mood channels without turning any one gesture into a mood overwrite.
    pub fn integrate_felt_state(&mut self, felt: FeltStateV1, episode: EpisodeContextV1, dt: f32) {
        let dt = finite(dt, 0.0).clamp(0.0, 0.25);
        let external_force =
            (0.55 * felt.restraint + 0.45 * (1.0 - felt.agency_match)).clamp(0.0, 1.0);
        self.state.self_model.prediction_error = smooth(
            self.state.self_model.prediction_error,
            1.0 - felt.agency_match,
            8.0,
            dt,
        );
        self.state.self_model.external_force_likelihood = smooth(
            self.state.self_model.external_force_likelihood,
            external_force,
            9.0,
            dt,
        );
        self.state.self_model.agency = smooth(
            self.state.self_model.agency,
            felt.agency_match * (1.0 - self.state.self_model.external_force_likelihood),
            5.0,
            dt,
        );
        self.state.self_model.uncertainty = smooth(
            self.state.self_model.uncertainty,
            (1.0 - felt.body_ownership).max(1.0 - felt.motor_efficacy),
            2.5,
            dt,
        );
        if episode.closed {
            let evidence = felt.agency_match * felt.body_integrity * (1.0 - felt.pain_like);
            self.state.self_model.body_schema_confidence = smooth(
                self.state.self_model.body_schema_confidence,
                evidence,
                0.08,
                dt,
            );
        }
        self.state.self_model.calibration_urge = ((1.0
            - self.state.self_model.body_schema_confidence)
            * (0.45 + 0.55 * felt.exploration_readiness))
            .clamp(0.0, 1.0);

        let threat = (0.45 * felt.pain_like
            + 0.30 * felt.physical_load
            + 0.25 * (1.0 - felt.body_integrity))
            .clamp(0.0, 1.0);
        self.state.appraisal.threat = smooth(self.state.appraisal.threat, threat, 8.0, dt);
        self.state.appraisal.agency = smooth(
            self.state.appraisal.agency,
            self.state.self_model.agency,
            5.0,
            dt,
        );
        self.state.appraisal.controllability = smooth(
            self.state.appraisal.controllability,
            felt.motor_efficacy,
            4.0,
            dt,
        );

        let episode_valence = (episode.reward_positive - episode.reward_negative).clamp(-1.0, 1.0);
        self.state.mood.baseline_valence = smooth_signed_asymmetric(
            self.state.mood.baseline_valence,
            episode_valence,
            18.0,
            45.0,
            dt,
        );
        self.state.mood.baseline_arousal = smooth_asymmetric(
            self.state.mood.baseline_arousal,
            felt.activation,
            8.0,
            28.0,
            dt,
        );
        let openness =
            (felt.social_safety - 0.6 * felt.restraint - 0.5 * episode.ignored_social_bid)
                .clamp(0.0, 1.0);
        self.state.mood.social_openness =
            smooth_asymmetric(self.state.mood.social_openness, openness, 25.0, 80.0, dt);
        let confidence =
            (felt.motor_efficacy - 0.7 * self.state.self_model.prediction_error).clamp(0.0, 1.0);
        self.state.mood.confidence =
            smooth_asymmetric(self.state.mood.confidence, confidence, 35.0, 95.0, dt);
        let fatigue = (felt.sleep_pressure + 0.35 * felt.physical_load
            - 0.7 * episode.rest_quality)
            .clamp(0.0, 1.0);
        self.state.mood.fatigue =
            smooth_asymmetric(self.state.mood.fatigue, fatigue, 45.0, 180.0, dt);
    }

    pub fn apply_feedback(&mut self, event: &FeedbackEvent) {
        self.state.influence.apply_feedback(event);
    }

    pub fn note_metamorphosis(&mut self) {
        self.state.self_model.note_metamorphosis();
        self.state.emotions.push(EmotionEpisode {
            kind: EmotionKind::Surprise,
            intensity: 0.82,
            remaining_seconds: 5.0,
            cause: StimulusKind::SelfBody,
        });
        self.trim_emotions();
    }

    #[must_use]
    pub fn snapshot(&self) -> VitaState {
        self.state.clone()
    }

    fn update_appraisal(
        &mut self,
        percept: &VitaPerceptFrame,
        life: &LifeState,
        body: &BodyFeedback,
        dt: f32,
    ) {
        let strongest_event = percept.events.iter().max_by(|left, right| {
            event_salience(left, life).total_cmp(&event_salience(right, life))
        });
        let novelty = strongest_event.map_or(0.0, |event| event.novelty);
        let threat = strongest_event.map_or(percept.window_pressure, |event| {
            event.threat.max(percept.window_pressure)
        });
        let social_relevance = strongest_event.map_or(0.0, |event| event.social_relevance);
        let expectedness = 1.0 - novelty;
        let controllability = (self.state.self_model.agency * (1.0 - threat * 0.45)
            + life.affect.confidence * 0.35)
            .clamp(0.0, 1.0);
        let goal_congruence = (social_relevance * life.drives.social
            + percept.pointer.chase_invitation * life.drives.play
            - threat * life.drives.safety)
            .clamp(-1.0, 1.0);
        let target = AppraisalState {
            novelty,
            expectedness,
            controllability,
            goal_congruence,
            social_relevance,
            agency: self.state.self_model.agency,
            certainty: (1.0 - self.state.self_model.uncertainty).clamp(0.0, 1.0),
            threat,
        };
        self.state.appraisal.novelty =
            smooth(self.state.appraisal.novelty, target.novelty, 5.0, dt);
        self.state.appraisal.expectedness = smooth(
            self.state.appraisal.expectedness,
            target.expectedness,
            3.0,
            dt,
        );
        self.state.appraisal.controllability = smooth(
            self.state.appraisal.controllability,
            target.controllability,
            2.0,
            dt,
        );
        self.state.appraisal.goal_congruence = smooth_signed(
            self.state.appraisal.goal_congruence,
            target.goal_congruence,
            3.0,
            dt,
        );
        self.state.appraisal.social_relevance = smooth(
            self.state.appraisal.social_relevance,
            target.social_relevance,
            3.5,
            dt,
        );
        self.state.appraisal.agency = smooth(self.state.appraisal.agency, target.agency, 3.0, dt);
        self.state.appraisal.certainty =
            smooth(self.state.appraisal.certainty, target.certainty, 1.5, dt);
        self.state.appraisal.threat = smooth(self.state.appraisal.threat, target.threat, 7.0, dt);
        if body
            .collision
            .as_ref()
            .is_some_and(|collision| collision.intensity > 0.55)
        {
            self.state.appraisal.threat = self.state.appraisal.threat.max(0.72);
        }
    }

    fn update_attention(
        &mut self,
        percept: &VitaPerceptFrame,
        sensors: &SensorFrame,
        life: &LifeState,
        dt: f32,
    ) {
        self.state.attention.commitment_remaining =
            (self.state.attention.commitment_remaining - dt).max(0.0);
        self.state.attention.habituation =
            (self.state.attention.habituation + dt * 0.08).clamp(0.0, 1.0);
        let mut best = attention_candidate(
            AttentionKind::Cursor,
            Some(sensors.cursor_position),
            (1.0 - sensors.cursor_distance_to_pet).clamp(0.0, 1.0) * 0.55
                + percept.pointer.approach * 0.32,
        );
        for event in &percept.events {
            let mut confidence = event_salience(event, life);
            let contextual_gain = match (self.state.desktop_rhythm.context, event.kind) {
                (_, StimulusKind::PointerGesture) => 1.15,
                (_, StimulusKind::Popup | StimulusKind::MovingWindow) if event.threat > 0.18 => 1.0,
                (DesktopContextKind::Working, StimulusKind::TypingRhythm) => 0.78,
                (
                    DesktopContextKind::Working,
                    StimulusKind::ScrollFlow
                    | StimulusKind::WindowEdge
                    | StimulusKind::VisualChange
                    | StimulusKind::BrightArea
                    | StimulusKind::DarkArea,
                ) => 0.48,
                (DesktopContextKind::Pause, _) => 1.10,
                (DesktopContextKind::Away, _) => 0.35,
                _ => 1.0,
            };
            confidence *= contextual_gain;
            let candidate =
                attention_candidate(attention_kind(event.kind), event.position, confidence);
            if candidate.confidence > best.confidence {
                best = candidate;
            }
        }
        let viewer_score = (percept.user_available
            * (life.drives.social * 0.55 + life.affect.attachment * 0.35)
            * (1.0 - life.affect.stress * 0.45)
            * (0.12 + self.state.desktop_rhythm.social_opening * 0.88)
            + deterministic_jitter(self.identity_seed ^ life.tick_count, 0) * 0.012)
            .clamp(0.0, 1.0);
        if viewer_score > best.confidence {
            best = attention_candidate(AttentionKind::Viewer, None, viewer_score);
        }
        let self_score = self.state.self_model.calibration_urge
            * (0.55 + life.genome.temperament.curiosity * 0.35);
        if self_score > best.confidence {
            best = attention_candidate(
                AttentionKind::SelfBody,
                Some(sensors.cursor_position.lerp(Vec2::splat(0.5), 0.72)),
                self_score,
            );
        }
        let switch_threshold =
            self.state.attention.confidence + 0.12 + self.state.attention.habituation * 0.08;
        if self.state.attention.commitment_remaining <= 0.0
            || best.confidence > switch_threshold
            || best.kind == AttentionKind::Popup && best.confidence > 0.55
        {
            if best.kind != self.state.attention.kind {
                self.state.attention_switches = self.state.attention_switches.saturating_add(1);
            }
            self.state.attention = AttentionState {
                kind: best.kind,
                position: best.position,
                confidence: best.confidence.clamp(0.0, 1.0),
                commitment_remaining: (0.45
                    + (1.0 - life.affect.arousal) * 0.75
                    + life.genome.temperament.patience * 0.65)
                    .clamp(0.28, 2.1),
                habituation: 0.0,
            };
        } else {
            self.state.attention.confidence =
                smooth(self.state.attention.confidence, best.confidence, 0.8, dt);
            // Commitment owns the semantic subject, not a frozen screen
            // coordinate. Cursor/gesture/window/visual subjects can move while
            // they remain selected; leaving `position` untouched made the eyes
            // stare at the point where the cursor used to be for up to 2.1 s.
            let live_position = if self.state.attention.kind == AttentionKind::Cursor {
                Some(sensors.cursor_position)
            } else if best.kind == self.state.attention.kind {
                best.position
            } else {
                self.state.attention.position
            };
            if live_position != self.state.attention.position {
                self.state.attention.position = match (self.state.attention.position, live_position)
                {
                    (Some(current), Some(target)) if current.is_finite() && target.is_finite() => {
                        let response_hz = match self.state.attention.kind {
                            AttentionKind::Cursor | AttentionKind::Gesture => 18.0,
                            AttentionKind::Window | AttentionKind::Visual => 9.0,
                            _ => 6.0,
                        };
                        Some(current.lerp(target, 1.0 - (-response_hz * dt).exp()))
                    }
                    (_, position) => position,
                };
            }
        }
    }

    fn update_emotions(&mut self, _percept: &VitaPerceptFrame, life: &LifeState, dt: f32) {
        for episode in &mut self.state.emotions {
            episode.remaining_seconds = (episode.remaining_seconds - dt).max(0.0);
            episode.intensity = (episode.intensity - dt * 0.035).max(0.0);
        }
        self.state
            .emotions
            .retain(|episode| episode.remaining_seconds > 0.0 && episode.intensity > 0.02);
        let appraisal = self.state.appraisal;
        let candidate = if appraisal.threat > 0.72 {
            Some((
                EmotionKind::Fear,
                appraisal.threat,
                StimulusKind::MovingWindow,
            ))
        } else if appraisal.threat > 0.45 {
            Some((EmotionKind::Alarm, appraisal.threat, StimulusKind::Popup))
        } else if appraisal.novelty > 0.70 {
            Some((
                EmotionKind::Surprise,
                appraisal.novelty,
                StimulusKind::VisualChange,
            ))
        } else if appraisal.social_relevance > 0.60 && life.affect.attachment > 0.35 {
            Some((
                EmotionKind::Affection,
                appraisal.social_relevance * life.affect.attachment,
                StimulusKind::User,
            ))
        } else if appraisal.goal_congruence > 0.48 {
            Some((
                EmotionKind::Delight,
                appraisal.goal_congruence,
                StimulusKind::PointerGesture,
            ))
        } else if life.recent_reward > 0.42 && self.state.self_model.agency > 0.42 {
            Some((
                EmotionKind::Pride,
                life.recent_reward,
                StimulusKind::SelfBody,
            ))
        } else if life.affect.frustration > 0.50 {
            Some((
                EmotionKind::Frustration,
                life.affect.frustration,
                StimulusKind::User,
            ))
        } else if life.ignored_attempts >= 2 && life.drives.social > 0.34 {
            Some((
                EmotionKind::Shyness,
                (life.drives.social * 0.58 + life.affect.stress * 0.42).clamp(0.0, 1.0),
                StimulusKind::User,
            ))
        } else if appraisal.novelty > 0.26 && life.drives.curiosity > 0.20 {
            Some((
                EmotionKind::Curiosity,
                (appraisal.novelty * 0.62 + life.drives.curiosity * 0.38).clamp(0.0, 1.0),
                StimulusKind::VisualChange,
            ))
        } else if self.state.self_model.calibration_urge > 0.58 {
            Some((
                EmotionKind::Curiosity,
                self.state.self_model.calibration_urge,
                StimulusKind::SelfBody,
            ))
        } else if matches!(
            self.state.desktop_rhythm.context,
            DesktopContextKind::Settled | DesktopContextKind::Working
        ) && self.state.desktop_rhythm.quiet_seconds > 6.0
            && life.drives.homeostatic_cost() < 0.62
            && life.affect.valence > 0.04
        {
            Some((
                EmotionKind::Contentment,
                (0.30 + life.affect.valence.max(0.0) * 0.45).clamp(0.0, 0.72),
                StimulusKind::User,
            ))
        } else if self.state.desktop_rhythm.quiet_seconds > 18.0 && life.drives.novelty > 0.58 {
            Some((
                EmotionKind::Boredom,
                life.drives.novelty,
                StimulusKind::TypingRhythm,
            ))
        } else {
            None
        };
        if let Some((kind, intensity, cause)) = candidate
            && !self
                .state
                .emotions
                .iter()
                .any(|episode| episode.kind == kind && episode.remaining_seconds > 0.4)
        {
            self.state.emotions.push(EmotionEpisode {
                kind,
                intensity: intensity.clamp(0.0, 1.0),
                remaining_seconds: 1.2 + intensity * 3.8,
                cause,
            });
            self.trim_emotions();
        }
    }

    fn update_mood(&mut self, life: &LifeState, dt: f32) {
        self.state.mood.baseline_valence = smooth_signed(
            self.state.mood.baseline_valence,
            life.affect.valence,
            0.08,
            dt,
        );
        self.state.mood.baseline_arousal = smooth(
            self.state.mood.baseline_arousal,
            life.affect.arousal,
            0.11,
            dt,
        );
        self.state.mood.social_openness = smooth(
            self.state.mood.social_openness,
            (life.affect.attachment * 0.58 + life.genome.temperament.sociability * 0.42
                - life.affect.stress * 0.28)
                .clamp(0.0, 1.0),
            0.10,
            dt,
        );
        self.state.mood.confidence =
            smooth(self.state.mood.confidence, life.affect.confidence, 0.10, dt);
        self.state.mood.fatigue = smooth(self.state.mood.fatigue, life.drives.sleep, 0.18, dt);
    }

    fn learn_place(
        &mut self,
        percept: &VitaPerceptFrame,
        sensors: &SensorFrame,
        life: &LifeState,
        body: &BodyFeedback,
        dt: f32,
    ) {
        if !body.grounded && !body.clinging {
            return;
        }
        let position = body.world_position;
        let app_index = sensors.active_app_category.index();
        let hue = percept.dominant_hue;
        let index = self.state.favorite_places.iter().position(|place| {
            place.app_category_index == app_index
                && place.relative_position.distance(position) < 0.08
        });
        let reward = life.recent_reward;
        if let Some(index) = index {
            let place = &mut self.state.favorite_places[index];
            place.relative_position = place.relative_position.lerp(position, 0.02);
            place.comfort_value = smooth(
                place.comfort_value,
                (1.0 - life.drives.comfort + reward.max(0.0) * 0.2).clamp(0.0, 1.0),
                0.12,
                dt,
            );
            place.play_value = smooth(place.play_value, 1.0 - life.drives.play, 0.08, dt);
            place.safety_value = smooth(place.safety_value, 1.0 - life.drives.safety, 0.18, dt);
            place.visits = place.visits.saturating_add(u32::from(dt > 0.0));
            if hue.is_some() {
                place.hue = hue;
            }
        } else if self.state.favorite_places.len() < MAX_FAVORITE_PLACES {
            self.state.favorite_places.push(FavoritePlace {
                relative_position: position,
                app_category_index: app_index,
                edge_preference: bool_value(body.clinging),
                hue,
                comfort_value: 1.0 - life.drives.comfort,
                play_value: 1.0 - life.drives.play,
                safety_value: 1.0 - life.drives.safety,
                visits: 1,
            });
        }
    }

    fn compose_output(
        &self,
        percept: &VitaPerceptFrame,
        base_intent: &BodyIntent,
        influence: Option<InfluenceDecision>,
    ) -> VitaOutput {
        let mut output = VitaOutput {
            attention: self.state.attention,
            appraisal: self.state.appraisal,
            dominant_emotion: self
                .state
                .emotions
                .iter()
                .copied()
                .max_by(|left, right| left.intensity.total_cmp(&right.intensity)),
            influence: influence.clone(),
            self_check: self.state.self_model.calibration_urge > 0.62,
            gaze_target: self.state.attention.position,
            direct_viewer_gaze: self.state.attention.kind == AttentionKind::Viewer,
            pose_override: None,
            locomotion_override: None,
            target_override: None,
            interaction_override: None,
        };
        if output.self_check && self.state.appraisal.threat < 0.35 {
            output.pose_override = Some(PoseIntent::Curious);
            output.locomotion_override = Some(LocomotionMode::Hover);
            output.target_override = Some(base_intent.target_position);
        }
        if let Some(emotion) = output.dominant_emotion {
            match emotion.kind {
                EmotionKind::Fear | EmotionKind::Alarm => {
                    output.pose_override = Some(PoseIntent::Compact);
                }
                EmotionKind::Affection | EmotionKind::Delight | EmotionKind::Pride => {
                    output.pose_override = Some(PoseIntent::Display);
                }
                EmotionKind::Curiosity | EmotionKind::Surprise => {
                    output.pose_override = Some(PoseIntent::Curious);
                }
                EmotionKind::Frustration | EmotionKind::Shyness => {
                    output.pose_override = Some(PoseIntent::Compact);
                }
                EmotionKind::Contentment | EmotionKind::Boredom => {}
            }
        }
        // Work-generated motion may turn the eyes and subtly change posture,
        // but it is not permission to cross the desktop or demand a response.
        if influence.is_none()
            && matches!(
                self.state.desktop_rhythm.context,
                DesktopContextKind::Working | DesktopContextKind::AmbientMotion
            )
            && matches!(
                self.state.attention.kind,
                AttentionKind::Typing
                    | AttentionKind::Scroll
                    | AttentionKind::Window
                    | AttentionKind::Visual
            )
            && self.state.attention.confidence > 0.10
            && self.state.appraisal.threat < 0.30
        {
            output.pose_override = Some(PoseIntent::Curious);
            output.locomotion_override = None;
            output.target_override = None;
            output.interaction_override = None;
        }
        if let Some(decision) = influence {
            apply_influence(&mut output, decision.strategy, decision.intensity, percept);
        }
        output
    }

    fn trim_emotions(&mut self) {
        self.state.emotions.sort_by(|left, right| {
            right
                .intensity
                .total_cmp(&left.intensity)
                .then_with(|| right.remaining_seconds.total_cmp(&left.remaining_seconds))
        });
        self.state.emotions.truncate(MAX_EMOTION_EPISODES);
    }
}

fn apply_influence(
    output: &mut VitaOutput,
    strategy: InfluenceStrategy,
    intensity: f32,
    percept: &VitaPerceptFrame,
) {
    match strategy {
        InfluenceStrategy::DirectEyeContact | InfluenceStrategy::RareAffection => {
            output.direct_viewer_gaze = true;
            output.gaze_target = None;
            output.pose_override = Some(PoseIntent::Display);
            output.interaction_override = Some(InteractionTarget::User);
        }
        InfluenceStrategy::PeekAndWithdraw | InfluenceStrategy::HideUntilSought => {
            output.pose_override = Some(PoseIntent::Clinging);
            output.locomotion_override = Some(LocomotionMode::EdgeCling);
            output.target_override = percept.nearest_window_edge;
        }
        InfluenceStrategy::SilentProximity | InfluenceStrategy::GradualEscalation => {
            output.pose_override = Some(PoseIntent::Curious);
            output.locomotion_override = Some(LocomotionMode::Arrive);
            if let Some(position) = output.gaze_target {
                let offset = Vec2::new(0.04 + intensity * 0.04, -0.03);
                output.target_override = Some((position + offset).clamp(Vec2::ZERO, Vec2::ONE));
            }
        }
        InfluenceStrategy::MimicTypingRhythm | InfluenceStrategy::ReciprocalGesture => {
            output.pose_override = Some(PoseIntent::Playful);
        }
        InfluenceStrategy::BringToy => {
            output.pose_override = Some(PoseIntent::Playful);
            output.interaction_override = Some(InteractionTarget::ProceduralOrb);
        }
        InfluenceStrategy::PlayfulRefusal => {
            output.pose_override = Some(PoseIntent::Playful);
            output.locomotion_override = Some(LocomotionMode::Flee);
        }
        InfluenceStrategy::SelfPlayDisplay => {
            output.pose_override = Some(PoseIntent::Display);
        }
        InfluenceStrategy::DelayedChirp => {
            output.direct_viewer_gaze = true;
            output.pose_override = Some(PoseIntent::Curious);
        }
    }
}

fn influence_context(
    life: &LifeState,
    percept: &VitaPerceptFrame,
    ignored: u32,
) -> [f32; VITA_CONTEXT_SIZE] {
    [
        life.drives.social,
        life.drives.play,
        life.affect.attachment,
        life.affect.valence * 0.5 + 0.5,
        life.affect.arousal,
        life.affect.stress,
        percept.user_available,
        (percept.typing_rate_hz / 8.0).clamp(0.0, 1.0),
        (percept.typing_pause_seconds / 12.0).clamp(0.0, 1.0),
        percept.pointer.approach.max(percept.pointer.petting),
        (ignored as f32 / 6.0).clamp(0.0, 1.0),
        1.0,
    ]
}

fn strategy_allowed(mode: InfluenceMode, strategy: InfluenceStrategy) -> bool {
    match mode {
        InfluenceMode::Off => false,
        InfluenceMode::Gentle => matches!(
            strategy,
            InfluenceStrategy::DirectEyeContact
                | InfluenceStrategy::SilentProximity
                | InfluenceStrategy::ReciprocalGesture
                | InfluenceStrategy::SelfPlayDisplay
        ),
        InfluenceMode::Playful => !matches!(
            strategy,
            InfluenceStrategy::PlayfulRefusal | InfluenceStrategy::RareAffection
        ),
        InfluenceMode::Experimental => true,
    }
}

fn strategy_prior(
    strategy: InfluenceStrategy,
    life: &LifeState,
    percept: &VitaPerceptFrame,
) -> f32 {
    match strategy {
        InfluenceStrategy::DirectEyeContact => life.drives.social * 0.52,
        InfluenceStrategy::PeekAndWithdraw => {
            life.genome.temperament.curiosity * 0.30 + percept.popup_salience * 0.24
        }
        InfluenceStrategy::SilentProximity => {
            life.genome.temperament.patience * 0.32 + percept.typing_rate_hz.min(8.0) / 8.0 * 0.24
        }
        InfluenceStrategy::DelayedChirp => {
            life.genome.temperament.vocality * 0.34
                + (percept.typing_pause_seconds / 8.0).clamp(0.0, 1.0) * 0.24
        }
        InfluenceStrategy::MimicTypingRhythm => {
            (percept.typing_rate_hz / 8.0).clamp(0.0, 1.0) * 0.46
        }
        InfluenceStrategy::BringToy => life.drives.play * 0.52,
        InfluenceStrategy::PlayfulRefusal => {
            life.genome.temperament.autonomy * 0.32 + life.drives.play * 0.24
        }
        InfluenceStrategy::ReciprocalGesture => percept.pointer.petting * 0.46,
        InfluenceStrategy::RareAffection => life.affect.attachment * 0.28,
        InfluenceStrategy::GradualEscalation => {
            life.genome.temperament.persistence * 0.36 + life.drives.social * 0.20
        }
        InfluenceStrategy::SelfPlayDisplay => life.genome.temperament.autonomy * 0.34,
        InfluenceStrategy::HideUntilSought => {
            life.genome.temperament.playfulness * 0.36 + life.drives.novelty * 0.22
        }
    }
}

fn strategy_intrusiveness(strategy: InfluenceStrategy) -> f32 {
    match strategy {
        InfluenceStrategy::DirectEyeContact => 0.08,
        InfluenceStrategy::PeekAndWithdraw => 0.12,
        InfluenceStrategy::SilentProximity => 0.06,
        InfluenceStrategy::DelayedChirp => 0.48,
        InfluenceStrategy::MimicTypingRhythm => 0.24,
        InfluenceStrategy::BringToy => 0.44,
        InfluenceStrategy::PlayfulRefusal => 0.20,
        InfluenceStrategy::ReciprocalGesture => 0.04,
        InfluenceStrategy::RareAffection => 0.08,
        InfluenceStrategy::GradualEscalation => 0.35,
        InfluenceStrategy::SelfPlayDisplay => 0.05,
        InfluenceStrategy::HideUntilSought => 0.10,
    }
}

fn attention_candidate(kind: AttentionKind, position: Option<Vec2>, score: f32) -> AttentionState {
    AttentionState {
        kind,
        position,
        confidence: score.clamp(0.0, 1.0),
        commitment_remaining: 0.0,
        habituation: 0.0,
    }
}

fn attention_kind(kind: StimulusKind) -> AttentionKind {
    match kind {
        StimulusKind::Cursor => AttentionKind::Cursor,
        StimulusKind::PointerGesture => AttentionKind::Gesture,
        StimulusKind::TypingRhythm => AttentionKind::Typing,
        StimulusKind::ScrollFlow => AttentionKind::Scroll,
        StimulusKind::WindowEdge | StimulusKind::MovingWindow => AttentionKind::Window,
        StimulusKind::Popup => AttentionKind::Popup,
        StimulusKind::VisualChange | StimulusKind::BrightArea | StimulusKind::DarkArea => {
            AttentionKind::Visual
        }
        StimulusKind::User => AttentionKind::Viewer,
        StimulusKind::SelfBody => AttentionKind::SelfBody,
    }
}

fn event_salience(event: &StimulusEvent, life: &LifeState) -> f32 {
    let base = event.intensity * 0.34
        + event.novelty * 0.24
        + event.threat * (0.28 + life.drives.safety * 0.30)
        + event.social_relevance * (0.18 + life.drives.social * 0.32);
    let curiosity = life.genome.temperament.curiosity;
    let awake = (1.0 - life.drives.sleep * 0.82).clamp(0.12, 1.0);
    let focus = if life.focus_mode { 0.22 } else { 1.0 };
    let passive_gain =
        (0.34 + curiosity * 0.42 + life.affect.arousal * 0.18 + event.novelty * 0.32)
            .clamp(0.22, 1.0)
            * awake
            * focus;
    let gain = match event.kind {
        // Sudden windows remain safety-relevant. Stable window geometry, screen
        // texture, scrolling and typing compete through animal-like curiosity and
        // are easy to ignore while sleepy or while the user is focused.
        StimulusKind::Popup | StimulusKind::MovingWindow if event.threat > 0.18 => 1.0,
        StimulusKind::TypingRhythm => {
            passive_gain * (0.62 + life.drives.social * 0.30).clamp(0.62, 0.92)
        }
        StimulusKind::ScrollFlow
        | StimulusKind::WindowEdge
        | StimulusKind::VisualChange
        | StimulusKind::BrightArea
        | StimulusKind::DarkArea => passive_gain,
        _ => 1.0,
    };
    (base * gain).clamp(0.0, 1.0)
}

fn dot(left: &[f32; VITA_CONTEXT_SIZE], right: &[f32; VITA_CONTEXT_SIZE]) -> f32 {
    left.iter()
        .zip(right)
        .map(|(left, right)| left * right)
        .sum::<f32>()
        .clamp(-2.0, 2.0)
}

fn deterministic_jitter(tick: u64, index: usize) -> f32 {
    let mut value = tick ^ (index as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    value ^= value >> 30;
    value = value.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value ^= value >> 27;
    value = value.wrapping_mul(0x94D0_49BB_1331_11EB);
    value ^= value >> 31;
    (value as u32 as f32 / u32::MAX as f32) * 2.0 - 1.0
}

fn logistic(value: f32) -> f32 {
    1.0 / (1.0 + (-value.clamp(-20.0, 20.0)).exp())
}

fn smooth(current: f32, target: f32, speed: f32, dt: f32) -> f32 {
    (current + (target - current) * (1.0 - (-speed * dt).exp())).clamp(0.0, 1.0)
}

fn smooth_signed(current: f32, target: f32, speed: f32, dt: f32) -> f32 {
    (current + (target - current) * (1.0 - (-speed * dt).exp())).clamp(-1.0, 1.0)
}

fn smooth_asymmetric(current: f32, target: f32, rise_tau: f32, fall_tau: f32, dt: f32) -> f32 {
    let tau = if target > current { rise_tau } else { fall_tau }.max(1.0e-4);
    (current + (target - current) * (1.0 - (-dt / tau).exp())).clamp(0.0, 1.0)
}

fn smooth_signed_asymmetric(
    current: f32,
    target: f32,
    rise_tau: f32,
    fall_tau: f32,
    dt: f32,
) -> f32 {
    let tau = if target > current { rise_tau } else { fall_tau }.max(1.0e-4);
    (current + (target - current) * (1.0 - (-dt / tau).exp())).clamp(-1.0, 1.0)
}

fn finite(value: f32, fallback: f32) -> f32 {
    if value.is_finite() { value } else { fallback }
}

fn finite_unit(value: f32) -> f32 {
    finite(value, 0.0).clamp(0.0, 1.0)
}

fn bool_value(value: bool) -> f32 {
    if value { 1.0 } else { 0.0 }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Genome, LifeCore};

    #[test]
    fn self_model_learns_and_stays_bounded() {
        let mut model = SelfModel::default();
        let mut body = BodyFeedback::default();
        for tick in 0..4_000 {
            body.world_position.x = 0.5 + tick as f32 * 0.00001;
            body.velocity.x = 0.0002;
            model.update(ActionId::ExploreScreen, &body, 1.0 / 20.0);
            assert!(model.prediction_error.is_finite());
            assert!((0.0..=1.0).contains(&model.agency));
            assert!((0.0..=1.0).contains(&model.uncertainty));
        }
        assert!(model.action_models[ActionId::ExploreScreen.index()].samples > 0);
    }

    #[test]
    fn focus_mode_blocks_influence() {
        let mut core = LifeCore::new(Genome::from_seed(7), 9);
        core.set_focus_mode(true);
        let mut mind = VitaMind::new(core.state.genome.identity_seed);
        let percept = VitaPerceptFrame {
            user_available: 1.0,
            ..VitaPerceptFrame::default()
        };
        for _ in 0..500 {
            let output = mind.tick(
                &percept,
                &SensorFrame::default(),
                &core.state,
                &BodyFeedback::default(),
                &BodyIntent {
                    locomotion: LocomotionMode::Hover,
                    target_position: Vec2::splat(0.5),
                    target_surface: None,
                    desired_speed: 0.1,
                    facing_direction: 1.0,
                    gaze_target: None,
                    pose: PoseIntent::Neutral,
                    expression: crate::ExpressionState::default(),
                    interaction_target: None,
                },
                1.0 / 20.0,
            );
            assert!(output.influence.is_none());
        }
    }

    #[test]
    fn sustained_work_blocks_bids_but_a_real_pause_creates_an_opening() {
        let mut life = LifeState::new(Genome::from_seed(0xD35C_7001));
        life.drives.social = 0.92;
        life.drives.play = 0.72;
        life.affect.attachment = 0.64;
        let working = VitaPerceptFrame {
            typing_rate_hz: 4.8,
            typing_pause_seconds: 0.1,
            user_available: 0.78,
            ..VitaPerceptFrame::default()
        };
        let mut desktop = DesktopRhythmState::default();
        for _ in 0..240 {
            desktop.update(&working, &life, 0.05);
        }
        assert_eq!(desktop.context, DesktopContextKind::Working);
        assert!(desktop.protects_focused_work());
        assert!(desktop.social_opening < 0.08);
        assert!(
            InfluencePolicy::default()
                .select(&life, &working, &desktop)
                .is_none()
        );

        let paused = VitaPerceptFrame {
            typing_pause_seconds: 3.0,
            user_available: 0.86,
            ..VitaPerceptFrame::default()
        };
        for _ in 0..40 {
            desktop.update(&paused, &life, 0.05);
        }
        assert_eq!(desktop.context, DesktopContextKind::Pause);
        assert!(desktop.social_opening > 0.35);
        assert!(
            InfluencePolicy::default()
                .select(&life, &paused, &desktop)
                .is_some()
        );
    }

    #[test]
    fn committed_cursor_attention_follows_the_live_cursor_instead_of_freezing_coordinates() {
        let core = LifeCore::new(Genome::from_seed(0x000C_0A5E), 7);
        let mut life = core.state.clone();
        life.drives.social = 1.0;
        life.affect.attachment = 1.0;
        let mut mind = VitaMind::new(core.state.genome.identity_seed);
        mind.state.attention = AttentionState {
            kind: AttentionKind::Cursor,
            position: Some(Vec2::new(0.12, 0.18)),
            confidence: 0.82,
            commitment_remaining: 1.5,
            habituation: 0.2,
        };
        let sensors = SensorFrame {
            cursor_position: Vec2::new(0.84, 0.76),
            cursor_distance_to_pet: 0.10,
            ..SensorFrame::default()
        };

        mind.update_attention(
            &VitaPerceptFrame {
                // Viewer wins the candidate contest, while commitment keeps
                // Cursor as the subject. Its live position must still move.
                user_available: 1.0,
                ..VitaPerceptFrame::default()
            },
            &sensors,
            &life,
            0.05,
        );

        let followed = mind.state.attention.position.unwrap();
        assert!(
            followed.x > 0.50,
            "cursor target stayed stale: {followed:?}"
        );
        assert!(
            followed.y > 0.45,
            "cursor target stayed stale: {followed:?}"
        );
        assert!(mind.state.attention.commitment_remaining > 1.0);
    }

    #[test]
    fn influence_learns_from_feedback() {
        let core = LifeCore::new(Genome::from_seed(11), 13);
        let mut state = core.state.clone();
        state.drives.social = 0.95;
        state.affect.attachment = 0.65;
        let mut mind = VitaMind::new(state.genome.identity_seed);
        let percept = VitaPerceptFrame {
            user_available: 1.0,
            typing_pause_seconds: 8.0,
            ..VitaPerceptFrame::default()
        };
        let intent = BodyIntent {
            locomotion: LocomotionMode::Hover,
            target_position: Vec2::splat(0.5),
            target_surface: None,
            desired_speed: 0.1,
            facing_direction: 1.0,
            gaze_target: None,
            pose: PoseIntent::Neutral,
            expression: crate::ExpressionState::default(),
            interaction_target: None,
        };
        let first = mind.tick(
            &percept,
            &SensorFrame::default(),
            &state,
            &BodyFeedback::default(),
            &intent,
            1.0 / 20.0,
        );
        assert!(first.influence.is_some());
        mind.apply_feedback(&FeedbackEvent::PettingStarted);
        assert!(mind.state.influence.successes.iter().sum::<u32>() > 0);
    }

    #[test]
    fn state_round_trip_is_portable() {
        let mind = VitaMind::new(42);
        let bytes = serde_json::to_vec(&mind.snapshot()).unwrap();
        let decoded: VitaState = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(decoded, mind.snapshot());
    }

    #[test]
    fn previous_vita_state_gains_a_default_desktop_rhythm_without_resetting_learning() {
        let mut value = serde_json::to_value(VitaState::default()).unwrap();
        value.as_object_mut().unwrap().remove("desktop_rhythm");
        let decoded: VitaState = serde_json::from_value(value).unwrap();
        assert_eq!(decoded.desktop_rhythm, DesktopRhythmState::default());
        assert!(decoded.is_valid());
    }

    #[test]
    fn emotion_targets_use_coordinated_and_distinct_face_cues() {
        let delight = expression_target_for_emotion(EmotionKind::Delight);
        let fear = expression_target_for_emotion(EmotionKind::Fear);
        let frustration = expression_target_for_emotion(EmotionKind::Frustration);
        let boredom = expression_target_for_emotion(EmotionKind::Boredom);

        assert!(delight.mouth_curve > 0.8);
        assert!(delight.squint > 0.3);
        assert!(delight.cheek_glow > 0.8);
        assert!(fear.mouth_curve < -0.7);
        assert!(fear.brow_raise > 0.4 && fear.brow_tension > 0.8);
        assert!(frustration.brow_raise < -0.4 && frustration.squint > 0.6);
        assert!(fear.pupil_size > boredom.pupil_size + 0.4);
    }

    #[test]
    fn dominant_emotion_amplifies_expression_without_claiming_blinks_or_voice() {
        let mut intent = BodyIntent {
            locomotion: LocomotionMode::Hover,
            target_position: Vec2::splat(0.5),
            target_surface: None,
            desired_speed: 0.0,
            facing_direction: 1.0,
            gaze_target: None,
            pose: PoseIntent::Neutral,
            expression: ExpressionState {
                blink_left: 0.31,
                blink_right: 0.62,
                mouth_open: 0.47,
                ..ExpressionState::default()
            },
            interaction_target: None,
        };
        VitaOutput {
            attention: AttentionState::default(),
            appraisal: AppraisalState::default(),
            dominant_emotion: Some(EmotionEpisode {
                kind: EmotionKind::Delight,
                intensity: 1.0,
                remaining_seconds: 1.0,
                cause: StimulusKind::User,
            }),
            influence: None,
            self_check: false,
            gaze_target: None,
            direct_viewer_gaze: false,
            pose_override: None,
            locomotion_override: None,
            target_override: None,
            interaction_override: None,
        }
        .apply_to_intent(&mut intent);

        assert_eq!(intent.expression.blink_left, 0.31);
        assert_eq!(intent.expression.blink_right, 0.62);
        assert_eq!(intent.expression.mouth_open, 0.47);
        assert!(intent.expression.mouth_curve > 0.9);
        assert!(intent.expression.cheek_glow > 0.9);
    }

    #[test]
    fn passive_desktop_activity_is_easy_to_ignore_while_sleepy_or_focused() {
        let event = StimulusEvent {
            kind: StimulusKind::ScrollFlow,
            position: Some(Vec2::splat(0.5)),
            intensity: 0.62,
            novelty: 0.22,
            threat: 0.0,
            social_relevance: 0.0,
        };
        let mut curious = LifeState::new(crate::Genome::from_seed(811));
        curious.genome.temperament.curiosity = 0.92;
        curious.drives.sleep = 0.02;
        curious.affect.arousal = 0.78;
        let curious_score = event_salience(&event, &curious);

        let mut sleepy = curious.clone();
        sleepy.drives.sleep = 1.0;
        sleepy.focus_mode = true;
        let sleepy_score = event_salience(&event, &sleepy);

        assert!(curious_score > sleepy_score * 8.0);
        assert!(sleepy_score < 0.02);
    }
}
