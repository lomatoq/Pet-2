use std::{fmt, str::FromStr};

use glam::Vec2;
use lifecore::{
    ActionId, BodyFeedback, BodyIntent, EmbodiedGestureEvent, FeedbackEvent,
    InteractionBodyActuation, InteractionGazeTarget, InteractionResponsePlan,
    InteractionTurnRuntime, InteractionTurnState, LifeState, LocomotionMode, PoseIntent,
    SensorFrame, VitaMind, VitaOutput, VitaPerceptFrame, VitaState, apply_emotion_to_expression,
};
#[cfg(test)]
use morph_brain::MORPH_COMMAND_COUNT;
use morph_brain::{MorphAttention, MorphCommand, MorphOutput};
use pet_ecology::{GestureSignature, RhythmSignature, WindowAffordanceFrame};
use pet_perception::{
    EmbodiedGestureClassifierTuning, PerceptionRuntime, SpatialVisualFrame, VisualAttentionTarget,
    VisualFeatureFrame,
};

/// Selects the single high-level behavior policy. Physics, rendering and audio
/// remain locally authoritative in both modes; the mode only changes which
/// semantic intent leaves the brain boundary.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum BrainMode {
    Classic,
    Morphic,
    Fusion,
    MorphShadow,
    #[default]
    MorphFusion,
}

impl BrainMode {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Classic => "classic",
            Self::Morphic => "morphic",
            Self::Fusion => "fusion",
            Self::MorphShadow => "morph-shadow",
            Self::MorphFusion => "morph-fusion",
        }
    }

    #[must_use]
    pub const fn toggled(self) -> Self {
        match self {
            Self::Classic => Self::Morphic,
            Self::Morphic => Self::Fusion,
            Self::Fusion => Self::MorphShadow,
            Self::MorphShadow => Self::MorphFusion,
            Self::MorphFusion => Self::Classic,
        }
    }
}

impl fmt::Display for BrainMode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for BrainMode {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "classic" => Ok(Self::Classic),
            "morphic" | "vita" | "smart" => Ok(Self::Morphic),
            "fusion" | "fused" | "hybrid" => Ok(Self::Fusion),
            "morph-shadow" | "colleague-shadow" => Ok(Self::MorphShadow),
            "morph-fusion" | "morph" | "colleague" | "neural-fusion" => Ok(Self::MorphFusion),
            _ => Err(format!(
                "unknown brain mode {value:?}; expected classic, morphic, fusion, morph-shadow, or morph-fusion"
            )),
        }
    }
}

/// Small, inspectable summary of the joint LifeCore/VITA decision. Fusion is
/// intentionally an in-process policy seam: it never owns body particles,
/// rendering, audio samples, native input, or another update thread.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct FusionDiagnostics {
    pub authority: f32,
    pub attention_confidence: f32,
    pub local_kernel_protected: bool,
    pub used_vita_target: bool,
    pub morph_authority: f32,
    pub used_morph: bool,
}

#[derive(Debug, Clone)]
struct FusionArbiter {
    filtered_target: Vec2,
    filtered_gaze: Option<Vec2>,
    held_pose: Option<PoseIntent>,
    pose_hold_seconds: f32,
}

impl Default for FusionArbiter {
    fn default() -> Self {
        Self {
            filtered_target: Vec2::splat(0.5),
            filtered_gaze: None,
            held_pose: None,
            pose_hold_seconds: 0.0,
        }
    }
}

impl FusionArbiter {
    fn enter(&mut self, base_intent: &BodyIntent) {
        self.filtered_target = base_intent.target_position;
        self.filtered_gaze = base_intent.gaze_target;
        self.held_pose = None;
        self.pose_hold_seconds = 0.0;
    }

    fn resolve(
        &mut self,
        life: &LifeState,
        sensors: &SensorFrame,
        base_intent: BodyIntent,
        mut vita: VitaOutput,
        morph_authority: f32,
        dt: f32,
    ) -> (BodyIntent, VitaOutput, FusionDiagnostics) {
        let dt = finite_dt(dt);
        let attention = (vita.attention.confidence.clamp(0.0, 1.0)
            * (1.0 - vita.attention.habituation.clamp(0.0, 1.0) * 0.45))
            .clamp(0.0, 1.0);
        let emotion = vita
            .dominant_emotion
            .map_or(0.0, |episode| episode.intensity.clamp(0.0, 1.0));
        let appraisal = vita
            .appraisal
            .novelty
            .max(vita.appraisal.threat)
            .max(vita.appraisal.social_relevance)
            .clamp(0.0, 1.0);
        let life_signal = (life.drives.curiosity * 0.42
            + life.drives.social * 0.28
            + life.affect.arousal * 0.18
            + life.affect.attachment * 0.12)
            .clamp(0.0, 1.0);
        let mut authority =
            (0.10 + attention * 0.34 + emotion * 0.22 + appraisal * 0.20 + life_signal * 0.14)
                .clamp(0.0, 1.0);

        let local_kernel_protected = local_kernel_protected(life, sensors, &base_intent);
        if local_kernel_protected {
            authority = authority.min(0.26);
        }

        // The visible VITA state is the same state that participated in the
        // arbitration. Scaling the episode avoids a presentation-only emotion
        // silently overpowering the fused semantic decision.
        if let Some(episode) = &mut vita.dominant_emotion {
            episode.intensity *= 0.35 + authority * 0.65;
        }

        let mut intent = base_intent;
        if let Some(episode) = vita.dominant_emotion {
            apply_emotion_to_expression(&mut intent.expression, episode.kind, episode.intensity);
        }

        if local_kernel_protected {
            self.filtered_gaze = intent.gaze_target;
        } else {
            let gaze_target = if vita.direct_viewer_gaze {
                Some(Vec2::splat(0.5))
            } else {
                vita.gaze_target.or(intent.gaze_target)
            };
            if let Some(gaze_target) = gaze_target.filter(|target| target.is_finite()) {
                let gaze_target = gaze_target.clamp(Vec2::ZERO, Vec2::ONE);
                let alpha = analytic_alpha(7.5 + authority * 4.5, dt);
                let filtered = self
                    .filtered_gaze
                    .unwrap_or(gaze_target)
                    .lerp(gaze_target, alpha);
                self.filtered_gaze = Some(filtered);
                intent.gaze_target = Some(filtered);
            }
        }

        self.pose_hold_seconds = (self.pose_hold_seconds - dt).max(0.0);
        if local_kernel_protected {
            self.filtered_target = intent.target_position;
            self.held_pose = None;
            self.pose_hold_seconds = 0.0;
        } else {
            let proposed_pose = vita.pose_override.filter(|_| authority >= 0.30);
            if let Some(proposed_pose) = proposed_pose {
                let may_switch = self.held_pose == Some(proposed_pose)
                    || self.held_pose.is_none()
                    || self.pose_hold_seconds <= f32::EPSILON;
                if may_switch {
                    self.held_pose = Some(proposed_pose);
                    self.pose_hold_seconds =
                        (0.24 + vita.attention.commitment_remaining * 0.20).clamp(0.24, 0.72);
                }
            } else if self.pose_hold_seconds <= f32::EPSILON {
                self.held_pose = None;
            }
            if let Some(pose) = self.held_pose {
                intent.pose = pose;
            }

            if authority >= 0.52
                && let Some(locomotion) = vita.locomotion_override
            {
                intent.locomotion = locomotion;
            }
            if authority >= 0.44
                && let Some(vita_target) = vita.target_override.filter(|target| target.is_finite())
            {
                let vita_target = vita_target.clamp(Vec2::ZERO, Vec2::ONE);
                let joint_target = intent.target_position.lerp(vita_target, authority);
                self.filtered_target = self
                    .filtered_target
                    .lerp(joint_target, analytic_alpha(4.5 + authority * 3.5, dt));
                intent.target_position = self.filtered_target.clamp(Vec2::ZERO, Vec2::ONE);
            } else {
                self.filtered_target = intent.target_position;
            }
            if authority >= 0.48
                && let Some(interaction) = &vita.interaction_override
            {
                intent.interaction_target = Some(interaction.clone());
            }
            if vita.direct_viewer_gaze && authority >= 0.42 {
                intent.interaction_target = Some(lifecore::InteractionTarget::User);
            }
        }

        let diagnostics = FusionDiagnostics {
            authority,
            attention_confidence: attention,
            local_kernel_protected,
            used_vita_target: !local_kernel_protected
                && authority >= 0.44
                && vita.target_override.is_some(),
            morph_authority,
            used_morph: morph_authority > 0.0,
        };
        (intent, vita, diagnostics)
    }
}

fn finite_dt(dt: f32) -> f32 {
    if dt.is_finite() {
        dt.clamp(0.0, 0.25)
    } else {
        0.0
    }
}

fn analytic_alpha(response_hz: f32, dt: f32) -> f32 {
    1.0 - (-response_hz.max(0.0) * finite_dt(dt)).exp()
}

/// Owns VITA's persistent mind and its transient privacy-preserving perception state.
/// Raw device events are reduced to timestamps or scalar features before entering here.
pub struct VitaRuntime {
    mind: VitaMind,
    perception: PerceptionRuntime,
    percept: VitaPerceptFrame,
    active_mode: BrainMode,
    fusion: FusionArbiter,
    last_fusion: Option<FusionDiagnostics>,
    interaction_turn: InteractionTurnRuntime,
    queued_embodied_gesture: Option<EmbodiedGestureEvent>,
    queued_embodied_signature: Option<GestureSignature>,
    active_interaction_plan: Option<InteractionResponsePlan>,
}

impl VitaRuntime {
    pub fn set_embodied_gesture_tuning(&mut self, tuning: EmbodiedGestureClassifierTuning) {
        self.perception.set_embodied_gesture_tuning(tuning);
    }

    pub fn set_visual_features(
        &mut self,
        summary: VisualFeatureFrame,
        spatial: SpatialVisualFrame,
    ) {
        self.perception.set_visual_features(summary);
        self.perception.set_spatial_visual(spatial);
    }

    pub fn cue_shared_attention(&mut self, position: Vec2, duration_seconds: f32) {
        self.perception
            .cue_shared_attention(position, duration_seconds);
    }

    #[must_use]
    pub const fn visual_attention_target(&self) -> Option<VisualAttentionTarget> {
        self.perception.visual_attention_target()
    }

    #[must_use]
    pub const fn visual_age_seconds(&self) -> f32 {
        self.perception.visual_age_seconds()
    }

    #[must_use]
    pub fn recent_click_rhythm(&self) -> Option<RhythmSignature> {
        self.perception.recent_click_rhythm()
    }

    #[must_use]
    pub fn new(identity_seed: u64, restored: Option<VitaState>) -> Self {
        Self {
            mind: restored.map_or_else(
                || VitaMind::new(identity_seed),
                |state| VitaMind::restore(identity_seed, state),
            ),
            perception: PerceptionRuntime::default(),
            percept: VitaPerceptFrame::default(),
            // Force the first non-classic resolve through its entry path so
            // the arbiter starts from the actual current body target.
            active_mode: BrainMode::Classic,
            fusion: FusionArbiter::default(),
            last_fusion: None,
            interaction_turn: InteractionTurnRuntime::default(),
            queued_embodied_gesture: None,
            queued_embodied_signature: None,
            active_interaction_plan: None,
        }
    }

    pub fn reset_learning(&mut self, identity_seed: u64) {
        *self = Self::new(identity_seed, None);
    }

    pub fn observe(&mut self, sensors: &SensorFrame, body: &BodyFeedback, dt: f32) {
        self.percept = self.perception.update(sensors, body, dt);
        if let Some(event) = self.perception.take_embodied_gesture() {
            self.queued_embodied_signature = self.perception.take_embodied_gesture_signature();
            if self
                .interaction_turn
                .begin_episode(event.classification.episode_id)
            {
                self.interaction_turn.state = InteractionTurnState::AwaitingClassification;
            } else if self.interaction_turn.episode_id == event.classification.episode_id {
                self.interaction_turn.mark_fresh_input();
            }
            self.queued_embodied_gesture = Some(event);
        }
    }

    pub fn take_embodied_gesture(&mut self) -> Option<EmbodiedGestureEvent> {
        let event = self.queued_embodied_gesture.take()?;
        self.queued_embodied_signature = None;
        // Release/detach edges may update the same physical episode after its
        // one response has already been emitted. Consuming that observation
        // must not strand the turn in Appraising when LifeCore correctly
        // refuses a duplicate response.
        if !self.interaction_turn.response_emitted {
            self.interaction_turn.state = InteractionTurnState::Appraising;
        }
        Some(event)
    }

    pub fn take_embodied_gesture_observation(
        &mut self,
    ) -> Option<(EmbodiedGestureEvent, Option<GestureSignature>)> {
        let event = self.queued_embodied_gesture.take()?;
        let signature = self.queued_embodied_signature.take();
        if !self.interaction_turn.response_emitted {
            self.interaction_turn.state = InteractionTurnState::Appraising;
        }
        Some((event, signature))
    }

    pub fn accept_interaction_response(&mut self, plan: InteractionResponsePlan) -> bool {
        if !self
            .interaction_turn
            .emit_response(plan.episode_id, plan.response_id)
        {
            return false;
        }
        self.active_interaction_plan = Some(plan);
        true
    }

    /// Closes a transient appraisal that LifeCore intentionally did not turn
    /// into a response (for example an ambiguous release edge). This owns no
    /// policy: LifeCore has already made the decision, and VITA only prevents
    /// its turn-state machine from remaining open indefinitely.
    pub fn finish_interaction_appraisal_without_response(&mut self, episode_id: u64) {
        if episode_id == self.interaction_turn.episode_id
            && self.interaction_turn.state == InteractionTurnState::Appraising
            && !self.interaction_turn.response_emitted
        {
            self.interaction_turn.state = InteractionTurnState::Disengaging;
            self.interaction_turn.elapsed_seconds = 0.0;
        }
    }

    #[must_use]
    pub const fn interaction_turn(&self) -> InteractionTurnRuntime {
        self.interaction_turn
    }

    #[must_use]
    pub const fn active_interaction_plan(&self) -> Option<InteractionResponsePlan> {
        self.active_interaction_plan
    }

    #[must_use]
    pub fn latest_embodied_gesture(&self) -> lifecore::GestureClassification {
        self.perception.latest_embodied_gesture()
    }

    #[must_use]
    pub fn interaction_actuation(&self) -> InteractionBodyActuation {
        if self.interaction_turn.state == InteractionTurnState::Responding {
            self.active_interaction_plan
                .map_or_else(InteractionBodyActuation::default, |plan| plan.body)
        } else {
            InteractionBodyActuation::default()
        }
    }

    #[must_use]
    pub const fn window_affordances(&self) -> &WindowAffordanceFrame {
        self.perception.window_affordances()
    }

    #[must_use]
    pub fn think(
        &mut self,
        life: &LifeState,
        sensors: &SensorFrame,
        body: &BodyFeedback,
        base_intent: &BodyIntent,
        dt: f32,
    ) -> VitaOutput {
        self.mind
            .tick(&self.percept, sensors, life, body, base_intent, dt)
    }

    /// Produces exactly one final semantic body intent. Classic preserves the
    /// LifeCore decision verbatim; Morphic lets VITA enrich it directly; Fusion
    /// gives LifeCore and VITA to one confidence-weighted joint arbiter. No
    /// downstream system needs to know which policy produced it.
    #[must_use]
    #[allow(dead_code)]
    pub fn resolve_intent(
        &mut self,
        mode: BrainMode,
        life: &LifeState,
        sensors: &SensorFrame,
        body: &BodyFeedback,
        base_intent: BodyIntent,
        dt: f32,
    ) -> (BodyIntent, Option<VitaOutput>) {
        self.resolve_intent_with_morph(mode, life, sensors, body, base_intent, None, dt)
    }

    /// Resolves LifeCore, VITA and the colleague's Morph network through one
    /// final arbiter. Morph Shadow records a real Morph decision without using
    /// it; Morph Fusion lets it enrich VITA at a strictly bounded authority.
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub fn resolve_intent_with_morph(
        &mut self,
        mode: BrainMode,
        life: &LifeState,
        sensors: &SensorFrame,
        body: &BodyFeedback,
        mut base_intent: BodyIntent,
        morph: Option<MorphOutput>,
        dt: f32,
    ) -> (BodyIntent, Option<VitaOutput>) {
        if self.active_mode != mode {
            if matches!(
                mode,
                BrainMode::Fusion | BrainMode::MorphShadow | BrainMode::MorphFusion
            ) {
                self.fusion.enter(&base_intent);
            }
            self.active_mode = mode;
        }
        let result = match mode {
            BrainMode::Classic => {
                self.last_fusion = None;
                (base_intent, None)
            }
            BrainMode::Morphic => {
                self.last_fusion = None;
                let output = self.think(life, sensors, body, &base_intent, dt);
                output.apply_to_intent(&mut base_intent);
                (base_intent, Some(output))
            }
            BrainMode::Fusion | BrainMode::MorphShadow | BrainMode::MorphFusion => {
                let mut output = self.think(life, sensors, body, &base_intent, dt);
                let morph_authority = if mode == BrainMode::MorphFusion
                    && !local_kernel_protected(life, sensors, &base_intent)
                {
                    morph.map_or(0.0, |morph| {
                        enrich_with_morph(&mut output, morph, sensors, body)
                    })
                } else {
                    0.0
                };
                let (intent, output, diagnostics) =
                    self.fusion
                        .resolve(life, sensors, base_intent, output, morph_authority, dt);
                self.last_fusion = Some(diagnostics);
                (intent, Some(output))
            }
        };
        let (mut intent, output) = result;
        self.apply_interaction_response(&mut intent, sensors, body, dt);
        (intent, output)
    }

    fn apply_interaction_response(
        &mut self,
        intent: &mut BodyIntent,
        sensors: &SensorFrame,
        body: &BodyFeedback,
        dt: f32,
    ) {
        self.interaction_turn.tick(self.active_interaction_plan, dt);
        let Some(plan) = self.active_interaction_plan else {
            return;
        };
        if self.interaction_turn.state != InteractionTurnState::Responding {
            if self.interaction_turn.state == InteractionTurnState::Idle {
                self.active_interaction_plan = None;
            }
            return;
        }
        let elapsed = self.interaction_turn.elapsed_seconds;
        let weight = if elapsed < plan.onset_seconds {
            elapsed / plan.onset_seconds.max(1.0e-4)
        } else if elapsed < plan.onset_seconds + plan.hold_seconds {
            1.0
        } else {
            1.0 - (elapsed - plan.onset_seconds - plan.hold_seconds)
                / plan.release_seconds.max(1.0e-4)
        }
        .clamp(0.0, 1.0)
            * plan.expression.amplitude;
        let gaze = match plan.gaze {
            InteractionGazeTarget::ContactPoint => Some(
                sensors
                    .embodied_interaction
                    .contact
                    .point_world
                    .clamp(Vec2::ZERO, Vec2::ONE),
            ),
            InteractionGazeTarget::Cursor => Some(sensors.cursor_position),
            InteractionGazeTarget::Viewer => Some(Vec2::splat(0.5)),
            InteractionGazeTarget::MainComponent | InteractionGazeTarget::MergePoint => {
                Some(body.world_position)
            }
            InteractionGazeTarget::DetachedComponent(id) => sensors.embodied_interaction.components
                [..usize::from(sensors.embodied_interaction.component_observation_count)]
                .iter()
                .find(|component| component.component_id == id)
                .map(|component| component.center_world),
            target @ InteractionGazeTarget::WorldEntity { .. } => target.world_position(),
            InteractionGazeTarget::Away => Some(
                (body.world_position
                    + (body.world_position - sensors.cursor_position).normalize_or_zero() * 0.18)
                    .clamp(Vec2::ZERO, Vec2::ONE),
            ),
        };
        if let Some(gaze) = gaze.filter(|gaze| gaze.is_finite()) {
            intent.gaze_target = Some(gaze.clamp(Vec2::ZERO, Vec2::ONE));
        }
        let target = plan.expression;
        blend(
            &mut intent.expression.eye_aperture,
            target.eye_aperture,
            weight,
        );
        blend(&mut intent.expression.eye_scale, target.eye_scale, weight);
        blend(
            &mut intent.expression.brow_asymmetry,
            target.brow_asymmetry,
            weight,
        );
        blend(
            &mut intent.expression.mouth_curve,
            target.mouth_curve,
            weight,
        );
        blend(
            &mut intent.expression.mouth_compression,
            target.mouth_compression,
            weight,
        );
        blend(
            &mut intent.expression.mouth_asymmetry,
            target.mouth_asymmetry,
            weight,
        );
        blend(&mut intent.expression.effort, target.effort, weight);
        blend(&mut intent.expression.relief, target.relief, weight);
        intent.expression.brow_tension = intent
            .expression
            .brow_tension
            .max(target.effort * weight * 0.65);
        intent.expression.mouth_tension = intent
            .expression
            .mouth_tension
            .max(target.mouth_compression * weight);
        if plan.body.recoil > 0.0 || plan.body.resistance > 0.45 {
            intent.pose = PoseIntent::Compact;
        } else if plan.body.cooperation > 0.55 {
            intent.pose = PoseIntent::Playful;
        } else {
            intent.pose = PoseIntent::Curious;
        }
    }

    pub fn apply_feedback(&mut self, event: &FeedbackEvent) {
        self.mind.apply_feedback(event);
    }

    pub fn note_metamorphosis(&mut self) {
        self.mind.note_metamorphosis();
    }

    /// Records only that a key event occurred; key identity and text never enter VITA.
    pub fn note_key_activity(&mut self, timestamp: f64) {
        self.perception.note_key_activity(timestamp);
    }

    pub fn note_click(&mut self, timestamp: f64) {
        self.perception.note_click(timestamp);
    }

    pub fn note_scroll(&mut self, normalized_delta: f32) {
        self.perception.note_scroll(normalized_delta);
    }

    #[must_use]
    pub fn snapshot(&self) -> VitaState {
        self.mind.snapshot()
    }

    #[must_use]
    pub fn state(&self) -> &VitaState {
        &self.mind.state
    }

    #[must_use]
    pub fn percept(&self) -> &VitaPerceptFrame {
        &self.percept
    }

    #[must_use]
    pub const fn fusion_diagnostics(&self) -> Option<FusionDiagnostics> {
        self.last_fusion
    }
}

fn enrich_with_morph(
    vita: &mut VitaOutput,
    morph: MorphOutput,
    sensors: &SensorFrame,
    body: &BodyFeedback,
) -> f32 {
    if !morph.confidence.is_finite() || !morph.conflict.is_finite() || !morph.arousal.is_finite() {
        return 0.0;
    }
    let object_control_confidence = (morph
        .manipulation_rate
        .max(morph.perception_rate)
        .max(morph.carry_rate)
        / 45.0)
        .clamp(0.0, 1.0)
        * (1.0 - morph.conflict.clamp(0.0, 1.0));
    let neural_confidence = morph.confidence.max(object_control_confidence);
    let authority =
        (neural_confidence * (1.0 - morph.conflict.clamp(0.0, 1.0) * 0.55) * 0.30).clamp(0.0, 0.30);
    if authority < 0.025 {
        return 0.0;
    }

    let local_attention = vita.attention.confidence;
    let local_habituation = vita.attention.habituation;
    let local_novelty = vita.appraisal.novelty;
    let local_threat = vita.appraisal.threat;
    let local_gaze = vita.gaze_target;
    let local_target = vita.target_override;

    let candidate_attention = local_attention.max((morph.confidence * 0.72).clamp(0.0, 1.0));
    let candidate_habituation = local_habituation * 0.55;
    let candidate_novelty = local_novelty.max(morph.arousal.clamp(0.0, 1.0));
    vita.attention.confidence = blend_scalar(local_attention, candidate_attention, authority);
    vita.attention.habituation = blend_scalar(local_habituation, candidate_habituation, authority);
    vita.appraisal.novelty = blend_scalar(local_novelty, candidate_novelty, authority);

    if morph.attention == MorphAttention::Cursor {
        vita.gaze_target = Some(sensors.cursor_position.clamp(Vec2::ZERO, Vec2::ONE));
    } else if morph.attention == MorphAttention::Object {
        vita.gaze_target = morph
            .object_target
            .map(|target| target.clamp(Vec2::ZERO, Vec2::ONE));
    }
    match morph.command {
        MorphCommand::Approach if morph.attention == MorphAttention::Cursor => {
            vita.target_override = Some(sensors.cursor_position.clamp(Vec2::ZERO, Vec2::ONE));
        }
        MorphCommand::Play if morph.attention == MorphAttention::Cursor => {
            vita.target_override = Some(sensors.cursor_position.clamp(Vec2::ZERO, Vec2::ONE));
        }
        MorphCommand::Flee if sensors.cursor_approach_speed > 0.08 && morph.confidence >= 0.72 => {
            let away = (body.world_position - sensors.cursor_position).normalize_or_zero();
            vita.target_override =
                Some((body.world_position + away * 0.22).clamp(Vec2::ZERO, Vec2::ONE));
            vita.appraisal.threat = vita.appraisal.threat.max(morph.confidence * 0.75);
        }
        _ => {}
    }
    if let Some(target) = morph
        .object_target
        .map(|target| target.clamp(Vec2::ZERO, Vec2::ONE))
    {
        match morph.manipulation {
            MorphCommand::Push | MorphCommand::Touch => {
                vita.gaze_target = Some(target);
                vita.target_override = Some(target);
                vita.pose_override = Some(PoseIntent::Playful);
            }
            MorphCommand::Pull | MorphCommand::Sample => {
                vita.gaze_target = Some(target);
                vita.target_override = Some(target);
                vita.pose_override = Some(PoseIntent::Curious);
            }
            _ => {}
        }
        match morph.perception {
            MorphCommand::Listen | MorphCommand::Sniff => {
                vita.gaze_target = Some(target);
                vita.pose_override = Some(PoseIntent::Curious);
            }
            _ => {}
        }
        match morph.carry {
            MorphCommand::Grasp => {
                vita.gaze_target = Some(target);
                vita.target_override = Some(target);
                vita.pose_override = Some(PoseIntent::Compact);
            }
            MorphCommand::Release => {
                vita.gaze_target = Some(target);
                vita.pose_override = Some(PoseIntent::Display);
            }
            _ => {}
        }
    }
    vita.appraisal.threat = blend_scalar(local_threat, vita.appraisal.threat, authority);
    vita.gaze_target =
        blend_optional_point(local_gaze, vita.gaze_target, body.world_position, authority);
    vita.target_override = blend_optional_point(
        local_target,
        vita.target_override,
        body.world_position,
        authority,
    );
    authority
}

fn blend_optional_point(
    local: Option<Vec2>,
    candidate: Option<Vec2>,
    neutral: Vec2,
    authority: f32,
) -> Option<Vec2> {
    let candidate = candidate.filter(|point| point.is_finite())?;
    let local = local.filter(|point| point.is_finite()).unwrap_or(neutral);
    Some(
        local
            .lerp(candidate, authority.clamp(0.0, 0.30))
            .clamp(Vec2::ZERO, Vec2::ONE),
    )
}

fn blend_scalar(local: f32, candidate: f32, authority: f32) -> f32 {
    local + (candidate - local) * authority.clamp(0.0, 0.30)
}

fn blend(value: &mut f32, target: f32, weight: f32) {
    *value += (target - *value) * weight.clamp(0.0, 1.0);
}

fn local_kernel_protected(
    life: &LifeState,
    sensors: &SensorFrame,
    base_intent: &BodyIntent,
) -> bool {
    life.focus_mode
        || sensors.pet_dragged
        || matches!(
            life.current_action,
            ActionId::Sleep
                | ActionId::WakeUp
                | ActionId::RetreatFromCursor
                | ActionId::FrustratedRetreat
                | ActionId::Metamorphosis
        )
        || matches!(
            base_intent.locomotion,
            LocomotionMode::Sleep | LocomotionMode::Cocoon
        )
}

#[cfg(test)]
mod tests {
    use glam::Vec2;
    use lifecore::{ExpressionState, Genome, LifeCore, LocomotionMode, PoseIntent};

    use super::*;

    #[test]
    fn bridge_restores_persistent_mind_but_not_raw_history() {
        let core = LifeCore::new(Genome::from_seed(17), 19);
        let mut runtime = VitaRuntime::new(core.state.genome.identity_seed, None);
        runtime.note_key_activity(1.0);
        runtime.observe(
            &SensorFrame {
                timestamp: 1.0,
                cursor_position: Vec2::new(0.7, 0.4),
                ..SensorFrame::default()
            },
            &BodyFeedback::default(),
            1.0 / 60.0,
        );
        let restored = VitaRuntime::new(core.state.genome.identity_seed, Some(runtime.snapshot()));
        assert_eq!(runtime.state(), restored.state());
        assert_eq!(restored.percept(), &VitaPerceptFrame::default());
    }

    #[test]
    fn same_episode_release_observation_does_not_strand_an_emitted_turn() {
        let core = LifeCore::new(Genome::from_seed(21), 23);
        let mut runtime = VitaRuntime::new(core.state.genome.identity_seed, None);
        runtime.interaction_turn = InteractionTurnRuntime {
            episode_id: 7,
            response_id: Some(9),
            state: InteractionTurnState::AwaitingUser,
            response_emitted: true,
            ..InteractionTurnRuntime::default()
        };
        runtime.queued_embodied_gesture = Some(EmbodiedGestureEvent::default());
        let _ = runtime.take_embodied_gesture_observation();
        assert_eq!(
            runtime.interaction_turn.state,
            InteractionTurnState::AwaitingUser
        );
    }

    #[test]
    fn declined_ambiguous_appraisal_disengages_without_emitting_a_response() {
        let core = LifeCore::new(Genome::from_seed(25), 27);
        let mut runtime = VitaRuntime::new(core.state.genome.identity_seed, None);
        runtime.interaction_turn = InteractionTurnRuntime {
            episode_id: 11,
            state: InteractionTurnState::Appraising,
            ..InteractionTurnRuntime::default()
        };
        runtime.finish_interaction_appraisal_without_response(11);
        assert_eq!(
            runtime.interaction_turn.state,
            InteractionTurnState::Disengaging
        );
        assert!(!runtime.interaction_turn.response_emitted);
    }

    #[test]
    fn brain_mode_parser_has_a_reversible_morph_fusion_default() {
        assert_eq!(BrainMode::default(), BrainMode::MorphFusion);
        assert_eq!("classic".parse::<BrainMode>(), Ok(BrainMode::Classic));
        assert_eq!("smart".parse::<BrainMode>(), Ok(BrainMode::Morphic));
        assert_eq!("hybrid".parse::<BrainMode>(), Ok(BrainMode::Fusion));
        assert_eq!("morph".parse::<BrainMode>(), Ok(BrainMode::MorphFusion));
        assert_eq!(BrainMode::Morphic.toggled(), BrainMode::Fusion);
        assert_eq!(BrainMode::Fusion.toggled(), BrainMode::MorphShadow);
        assert_eq!(BrainMode::MorphShadow.toggled(), BrainMode::MorphFusion);
        assert_eq!(BrainMode::MorphFusion.toggled(), BrainMode::Classic);
        assert_eq!(BrainMode::Classic.toggled(), BrainMode::Morphic);
    }

    #[test]
    fn classic_mode_preserves_the_lifecore_intent_exactly() {
        let core = LifeCore::new(Genome::from_seed(27), 29);
        let mut runtime = VitaRuntime::new(core.state.genome.identity_seed, None);
        let intent = BodyIntent {
            locomotion: LocomotionMode::Hover,
            target_position: Vec2::new(0.23, 0.81),
            target_surface: None,
            desired_speed: 0.08,
            facing_direction: 1.0,
            gaze_target: None,
            pose: PoseIntent::Neutral,
            expression: ExpressionState::default(),
            interaction_target: None,
        };
        let (resolved, output) = runtime.resolve_intent(
            BrainMode::Classic,
            &core.state,
            &SensorFrame::default(),
            &BodyFeedback::default(),
            intent.clone(),
            1.0 / 20.0,
        );
        assert_eq!(resolved, intent);
        assert!(output.is_none());
    }

    #[test]
    fn fusion_blends_a_joint_target_instead_of_teleporting_to_vita() {
        let core = LifeCore::new(Genome::from_seed(37), 41);
        let mut arbiter = FusionArbiter::default();
        let base = BodyIntent {
            locomotion: LocomotionMode::Hover,
            target_position: Vec2::new(0.20, 0.40),
            target_surface: None,
            desired_speed: 0.08,
            facing_direction: 1.0,
            gaze_target: Some(Vec2::splat(0.5)),
            pose: PoseIntent::Neutral,
            expression: ExpressionState::default(),
            interaction_target: None,
        };
        arbiter.enter(&base);
        let vita = VitaOutput {
            attention: lifecore::AttentionState {
                confidence: 1.0,
                commitment_remaining: 1.0,
                ..lifecore::AttentionState::default()
            },
            appraisal: lifecore::AppraisalState {
                novelty: 1.0,
                social_relevance: 1.0,
                ..lifecore::AppraisalState::default()
            },
            dominant_emotion: None,
            influence: None,
            self_check: false,
            gaze_target: Some(Vec2::new(0.82, 0.60)),
            direct_viewer_gaze: false,
            pose_override: Some(PoseIntent::Curious),
            locomotion_override: Some(LocomotionMode::Arrive),
            target_override: Some(Vec2::new(0.90, 0.80)),
            interaction_override: None,
        };
        let (resolved, _, diagnostics) = arbiter.resolve(
            &core.state,
            &SensorFrame::default(),
            base.clone(),
            vita,
            0.0,
            1.0 / 20.0,
        );
        assert!(diagnostics.authority > 0.5);
        assert!(diagnostics.used_vita_target);
        assert!(resolved.target_position.x > base.target_position.x);
        assert!(resolved.target_position.x < 0.90);
        assert_eq!(resolved.pose, PoseIntent::Curious);
    }

    #[test]
    fn fusion_local_kernel_preserves_sleep() {
        let mut core = LifeCore::new(Genome::from_seed(43), 47);
        core.state.current_action = ActionId::Sleep;
        let base = BodyIntent {
            locomotion: LocomotionMode::Sleep,
            target_position: Vec2::new(0.35, 0.70),
            target_surface: None,
            desired_speed: 0.0,
            facing_direction: 1.0,
            gaze_target: None,
            pose: PoseIntent::Sleeping,
            expression: ExpressionState::default(),
            interaction_target: None,
        };
        let mut arbiter = FusionArbiter::default();
        arbiter.enter(&base);
        let vita = VitaOutput {
            attention: lifecore::AttentionState::default(),
            appraisal: lifecore::AppraisalState::default(),
            dominant_emotion: None,
            influence: None,
            self_check: false,
            gaze_target: Some(Vec2::ONE),
            direct_viewer_gaze: false,
            pose_override: Some(PoseIntent::Playful),
            locomotion_override: Some(LocomotionMode::Flee),
            target_override: Some(Vec2::ONE),
            interaction_override: None,
        };
        let (resolved, _, diagnostics) = arbiter.resolve(
            &core.state,
            &SensorFrame::default(),
            base.clone(),
            vita,
            0.0,
            1.0 / 20.0,
        );
        assert!(diagnostics.local_kernel_protected);
        assert_eq!(resolved.locomotion, LocomotionMode::Sleep);
        assert_eq!(resolved.pose, PoseIntent::Sleeping);
        assert_eq!(resolved.target_position, base.target_position);
    }

    #[test]
    fn morph_fusion_is_bounded_and_shadow_has_zero_authority() {
        let core = LifeCore::new(Genome::from_seed(53), 59);
        let mut runtime = VitaRuntime::new(core.state.genome.identity_seed, None);
        let sensors = SensorFrame {
            cursor_position: Vec2::new(0.82, 0.35),
            cursor_distance_to_pet: 0.24,
            ..SensorFrame::default()
        };
        let base = BodyIntent {
            locomotion: LocomotionMode::Hover,
            target_position: Vec2::splat(0.5),
            target_surface: None,
            desired_speed: 0.05,
            facing_direction: 1.0,
            gaze_target: None,
            pose: PoseIntent::Neutral,
            expression: ExpressionState::default(),
            interaction_target: None,
        };
        let morph = MorphOutput {
            command: MorphCommand::Perk,
            command_rates: {
                let mut rates = [0.0; MORPH_COMMAND_COUNT];
                rates[4] = 40.0;
                rates
            },
            winner_rate: 40.0,
            confidence: 0.90,
            attention: MorphAttention::Cursor,
            valence: 0.2,
            arousal: 0.8,
            conflict: 0.05,
            turn: 0.4,
            ..MorphOutput::default()
        };
        let _ = runtime.resolve_intent_with_morph(
            BrainMode::MorphShadow,
            &core.state,
            &sensors,
            &BodyFeedback::default(),
            base.clone(),
            Some(morph),
            0.05,
        );
        assert_eq!(runtime.fusion_diagnostics().unwrap().morph_authority, 0.0);
        let (resolved, _) = runtime.resolve_intent_with_morph(
            BrainMode::MorphFusion,
            &core.state,
            &sensors,
            &BodyFeedback::default(),
            base,
            Some(morph),
            0.05,
        );
        let diagnostics = runtime.fusion_diagnostics().unwrap();
        assert!(diagnostics.used_morph);
        assert!(diagnostics.morph_authority <= 0.30);
        assert!(
            resolved
                .gaze_target
                .is_some_and(|gaze| gaze.distance(sensors.cursor_position) < 0.000_01)
        );
    }

    #[test]
    fn morph_enrichment_blends_continuous_outputs_by_actual_authority() {
        let sensors = SensorFrame {
            cursor_position: Vec2::new(0.90, 0.10),
            ..SensorFrame::default()
        };
        let body = BodyFeedback::default();
        let mut vita = VitaOutput {
            attention: lifecore::AttentionState::default(),
            appraisal: lifecore::AppraisalState::default(),
            dominant_emotion: None,
            influence: None,
            self_check: false,
            gaze_target: None,
            direct_viewer_gaze: false,
            pose_override: None,
            locomotion_override: None,
            target_override: None,
            interaction_override: None,
        };
        let morph = MorphOutput {
            command: MorphCommand::Approach,
            command_rates: [0.0; MORPH_COMMAND_COUNT],
            winner_rate: 80.0,
            confidence: 1.0,
            attention: MorphAttention::Cursor,
            valence: 0.5,
            arousal: 1.0,
            conflict: 0.0,
            turn: 0.0,
            ..MorphOutput::default()
        };

        let local_confidence = vita.attention.confidence;
        let local_novelty = vita.appraisal.novelty;
        let authority = enrich_with_morph(&mut vita, morph, &sensors, &body);
        let expected = body.world_position.lerp(sensors.cursor_position, authority);
        let expected_confidence = blend_scalar(local_confidence, 0.72, authority);
        let expected_novelty = blend_scalar(local_novelty, 1.0, authority);
        assert!((authority - 0.30).abs() < 0.000_01);
        assert!(
            vita.gaze_target
                .is_some_and(|gaze| gaze.distance(expected) < 0.000_01)
        );
        assert!(
            vita.target_override
                .is_some_and(|target| target.distance(expected) < 0.000_01)
        );
        assert!((vita.attention.confidence - expected_confidence).abs() < 0.000_01);
        assert!((vita.appraisal.novelty - expected_novelty).abs() < 0.000_01);
        assert_eq!(vita.pose_override, None);
        assert_eq!(vita.locomotion_override, None);
    }

    #[test]
    fn morph_cannot_override_the_protected_kernel() {
        let mut core = LifeCore::new(Genome::from_seed(61), 67);
        core.state.current_action = ActionId::Sleep;
        let mut runtime = VitaRuntime::new(core.state.genome.identity_seed, None);
        let sensors = SensorFrame {
            cursor_position: Vec2::ONE,
            ..SensorFrame::default()
        };
        let base = BodyIntent {
            locomotion: LocomotionMode::Sleep,
            target_position: Vec2::new(0.35, 0.70),
            target_surface: None,
            desired_speed: 0.0,
            facing_direction: 1.0,
            gaze_target: Some(Vec2::new(0.35, 0.70)),
            pose: PoseIntent::Sleeping,
            expression: ExpressionState::default(),
            interaction_target: None,
        };
        let morph = MorphOutput {
            command: MorphCommand::Play,
            command_rates: [0.0; MORPH_COMMAND_COUNT],
            winner_rate: 90.0,
            confidence: 1.0,
            attention: MorphAttention::Cursor,
            valence: 0.7,
            arousal: 1.0,
            conflict: 0.0,
            turn: 1.0,
            ..MorphOutput::default()
        };

        let (resolved, _) = runtime.resolve_intent_with_morph(
            BrainMode::MorphFusion,
            &core.state,
            &sensors,
            &BodyFeedback::default(),
            base.clone(),
            Some(morph),
            0.05,
        );
        let diagnostics = runtime.fusion_diagnostics().unwrap();
        assert!(diagnostics.local_kernel_protected);
        assert_eq!(diagnostics.morph_authority, 0.0);
        assert!(!diagnostics.used_morph);
        assert_eq!(resolved.gaze_target, base.gaze_target);
        assert_eq!(resolved.pose, PoseIntent::Sleeping);
        assert_eq!(resolved.locomotion, LocomotionMode::Sleep);
    }
}
