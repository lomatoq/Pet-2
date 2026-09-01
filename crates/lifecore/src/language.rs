use glam::Vec2;
use serde::{Deserialize, Serialize};

use crate::{
    ActionId, AffectState, BodyFeedback, Drives, InteractionGazeTarget, InteractionReasonCode,
    InteractionResponsePlan, LifeState, Rect, SensorFrame, VocalRequest, VocalTrigger,
};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SocialIntent {
    #[default]
    Notice,
    Contact,
    Acknowledge,
    Invite,
    Query,
    Effort,
    Boundary,
    Alarm,
    Relief,
}

impl SocialIntent {
    pub const ALL: [Self; 9] = [
        Self::Notice,
        Self::Contact,
        Self::Acknowledge,
        Self::Invite,
        Self::Query,
        Self::Effort,
        Self::Boundary,
        Self::Alarm,
        Self::Relief,
    ];

    #[must_use]
    pub const fn index(self) -> usize {
        self as usize
    }

    #[must_use]
    pub const fn for_trigger(trigger: VocalTrigger) -> Self {
        match trigger {
            VocalTrigger::VisualNotice | VocalTrigger::FoodInspect => Self::Notice,
            VocalTrigger::SoftTouch | VocalTrigger::Action(ActionId::Purr) => Self::Contact,
            VocalTrigger::RhythmEcho | VocalTrigger::FoodAccepted => Self::Acknowledge,
            VocalTrigger::ToyOffer
            | VocalTrigger::PlayfulRelease
            | VocalTrigger::Action(ActionId::Chirp | ActionId::MimicClickRhythm) => Self::Invite,
            VocalTrigger::NeedHelp => Self::Query,
            VocalTrigger::MissAndRetry | VocalTrigger::FragmentHelped => Self::Effort,
            VocalTrigger::CalmBoundary | VocalTrigger::FoodRefused => Self::Boundary,
            VocalTrigger::PhysicalStartle | VocalTrigger::ComponentDetached => Self::Alarm,
            VocalTrigger::CatchSuccess
            | VocalTrigger::ComponentRemerged
            | VocalTrigger::HomeReturn
            | VocalTrigger::SkillMastered => Self::Relief,
            VocalTrigger::Action(_) => Self::Acknowledge,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VoiceGesture {
    #[default]
    WarmChuff,
    PurrHum,
    MewWhine,
    LowRumble,
    ClippedPulse,
    ReliefExhale,
}

impl VoiceGesture {
    #[must_use]
    pub const fn for_trigger(trigger: VocalTrigger) -> Self {
        match trigger {
            VocalTrigger::Action(ActionId::Purr) | VocalTrigger::HomeReturn => Self::PurrHum,
            VocalTrigger::CalmBoundary | VocalTrigger::FoodRefused => Self::LowRumble,
            VocalTrigger::PhysicalStartle | VocalTrigger::ComponentDetached => Self::ClippedPulse,
            VocalTrigger::NeedHelp | VocalTrigger::Action(ActionId::Chirp) => Self::MewWhine,
            VocalTrigger::ComponentRemerged
            | VocalTrigger::FragmentHelped
            | VocalTrigger::FoodAccepted => Self::ReliefExhale,
            VocalTrigger::Action(_)
            | VocalTrigger::ToyOffer
            | VocalTrigger::CatchSuccess
            | VocalTrigger::MissAndRetry
            | VocalTrigger::FoodInspect
            | VocalTrigger::SkillMastered
            | VocalTrigger::RhythmEcho
            | VocalTrigger::VisualNotice
            | VocalTrigger::SoftTouch
            | VocalTrigger::PlayfulRelease => Self::WarmChuff,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct VocalLexeme {
    pub intent: SocialIntent,
    pub gesture: VoiceGesture,
    pub timing_scale: f32,
    pub rhythm_bias: f32,
    pub confidence: f32,
    pub positive_examples: u32,
    pub negative_examples: u32,
    pub updates: u32,
}

impl VocalLexeme {
    #[must_use]
    pub fn is_valid(self) -> bool {
        self.timing_scale.is_finite()
            && (0.88..=1.12).contains(&self.timing_scale)
            && self.rhythm_bias.is_finite()
            && (-0.12..=0.12).contains(&self.rhythm_bias)
            && self.confidence.is_finite()
            && (0.0..=1.0).contains(&self.confidence)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct VocalLexiconState {
    pub schema_version: u32,
    pub entries: [VocalLexeme; 9],
    pub update_count: u32,
    pub last_updated_episode: u64,
}

impl Default for VocalLexiconState {
    fn default() -> Self {
        Self {
            schema_version: 1,
            entries: SocialIntent::ALL.map(|intent| VocalLexeme {
                intent,
                gesture: default_gesture(intent),
                timing_scale: 1.0,
                rhythm_bias: 0.0,
                confidence: 0.0,
                positive_examples: 0,
                negative_examples: 0,
                updates: 0,
            }),
            update_count: 0,
            last_updated_episode: 0,
        }
    }
}

impl VocalLexiconState {
    #[must_use]
    pub fn is_valid(&self) -> bool {
        self.schema_version == 1
            && self
                .entries
                .iter()
                .enumerate()
                .all(|(index, entry)| entry.intent.index() == index && entry.is_valid())
    }

    #[must_use]
    pub const fn entry(&self, intent: SocialIntent) -> VocalLexeme {
        self.entries[intent.index()]
    }

    /// Learns only bounded timing/rhythm convention. Timbre and semantic
    /// gesture stay identity-owned; no microphone or raw waveform is involved.
    pub fn update_social_timing(&mut self, intent: SocialIntent, episode_id: u64, reward: f32) {
        if episode_id == 0 || episode_id == self.last_updated_episode || !reward.is_finite() {
            return;
        }
        let entry = &mut self.entries[intent.index()];
        let reward = reward.clamp(-1.0, 1.0);
        let rate = 0.018 * (1.0 - entry.confidence * 0.35);
        entry.timing_scale = (entry.timing_scale + reward * rate * 0.20).clamp(0.88, 1.12);
        entry.rhythm_bias = (entry.rhythm_bias + reward * rate * 0.12).clamp(-0.12, 0.12);
        entry.confidence = (entry.confidence + rate * 0.5).clamp(0.0, 1.0);
        if reward >= 0.0 {
            entry.positive_examples = entry.positive_examples.saturating_add(1);
        } else {
            entry.negative_examples = entry.negative_examples.saturating_add(1);
        }
        entry.updates = entry.updates.saturating_add(1);
        self.update_count = self.update_count.saturating_add(1);
        self.last_updated_episode = episode_id;
    }
}

const fn default_gesture(intent: SocialIntent) -> VoiceGesture {
    match intent {
        SocialIntent::Notice | SocialIntent::Contact | SocialIntent::Acknowledge => {
            VoiceGesture::WarmChuff
        }
        SocialIntent::Invite | SocialIntent::Query => VoiceGesture::MewWhine,
        SocialIntent::Effort | SocialIntent::Boundary => VoiceGesture::LowRumble,
        SocialIntent::Alarm => VoiceGesture::ClippedPulse,
        SocialIntent::Relief => VoiceGesture::ReliefExhale,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct LivingStateFrame {
    pub affect: AffectState,
    pub drives: Drives,
    pub current_action: ActionId,
    pub sleeping: bool,
    pub focus_mode: bool,
    pub quiet_preferred: bool,
}

impl LivingStateFrame {
    #[must_use]
    pub fn from_life(state: &LifeState) -> Self {
        let sleeping = state.current_action == ActionId::Sleep;
        Self {
            affect: state.affect,
            drives: state.drives,
            current_action: state.current_action,
            sleeping,
            focus_mode: state.focus_mode,
            quiet_preferred: sleeping || state.focus_mode || state.drives.sleep > 0.82,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct PhysicalExpressionContext {
    pub contact: bool,
    pub pressure: f32,
    pub deformation: f32,
    pub strain: f32,
    pub slosh_energy: f32,
    pub topology_budget_used: f32,
    pub detached_fraction: f32,
    pub speed: f32,
    pub acceleration: f32,
    pub airborne: bool,
}

impl PhysicalExpressionContext {
    #[must_use]
    pub fn from_frames(sensors: &SensorFrame, body: &BodyFeedback) -> Self {
        let physical = sensors.embodied_interaction;
        Self {
            contact: physical.contact.active,
            pressure: physical.contact.effective_pressure.clamp(0.0, 1.0),
            deformation: physical.material.deformation_energy.clamp(0.0, 1.0),
            strain: physical.material.maximum_strain.clamp(0.0, 1.0),
            slosh_energy: physical.material.slosh_energy.clamp(0.0, 1.0),
            topology_budget_used: (1.0 - physical.material.topology_budget_remaining)
                .clamp(0.0, 1.0),
            detached_fraction: physical.material.detached_mass_fraction.clamp(0.0, 1.0),
            speed: body.velocity.length().clamp(0.0, 1.0),
            acceleration: (body.acceleration.length() * 0.25).clamp(0.0, 1.0),
            airborne: !body.grounded && !body.clinging,
        }
    }

    #[must_use]
    pub fn load(self) -> f32 {
        (self.pressure * 0.30
            + self.deformation * 0.22
            + self.strain * 0.24
            + self.topology_budget_used * 0.16
            + self.detached_fraction * 0.40)
            .clamp(0.0, 1.0)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorldEntityKind {
    #[default]
    Viewer,
    Cursor,
    Contact,
    BodyComponent,
    Surface,
    ProceduralObject,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorldAffordance {
    #[default]
    Observe,
    Approach,
    Touch,
    Help,
    Land,
    Play,
    Avoid,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct WorldEntity {
    pub id: u64,
    pub kind: WorldEntityKind,
    pub bbox: Rect,
    pub position: Vec2,
    pub velocity: Vec2,
    pub confidence: f32,
    pub novelty: f32,
    pub familiarity: f32,
    pub salience: f32,
    pub affordance: WorldAffordance,
    pub affordances: [WorldAffordance; 4],
    pub affordance_count: u8,
    pub last_seen_seconds: f64,
}

impl Default for WorldEntity {
    fn default() -> Self {
        Self {
            id: 0,
            kind: WorldEntityKind::Viewer,
            bbox: Rect::default(),
            position: Vec2::splat(0.5),
            velocity: Vec2::ZERO,
            confidence: 0.0,
            novelty: 0.0,
            familiarity: 0.0,
            salience: 0.0,
            affordance: WorldAffordance::Observe,
            affordances: [WorldAffordance::Observe; 4],
            affordance_count: 1,
            last_seen_seconds: 0.0,
        }
    }
}

impl WorldEntity {
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub fn point(
        id: u64,
        kind: WorldEntityKind,
        position: Vec2,
        velocity: Vec2,
        confidence: f32,
        salience: f32,
        affordance: WorldAffordance,
        last_seen_seconds: f64,
    ) -> Self {
        let position = position.clamp(Vec2::ZERO, Vec2::ONE);
        Self {
            id,
            kind,
            bbox: Rect {
                minimum: (position - Vec2::splat(0.005)).clamp(Vec2::ZERO, Vec2::ONE),
                maximum: (position + Vec2::splat(0.005)).clamp(Vec2::ZERO, Vec2::ONE),
            },
            position,
            velocity: if velocity.is_finite() {
                velocity.clamp_length_max(4.0)
            } else {
                Vec2::ZERO
            },
            confidence: confidence.clamp(0.0, 1.0),
            salience: salience.clamp(0.0, 1.0),
            affordance,
            affordances: [affordance; 4],
            affordance_count: 1,
            last_seen_seconds: last_seen_seconds.max(0.0),
            ..Self::default()
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct WorldModelFrame {
    pub entities: [WorldEntity; 8],
    pub entity_count: u8,
    pub audience_present: bool,
    pub audience_attention: f32,
}

impl Default for WorldModelFrame {
    fn default() -> Self {
        Self {
            entities: [WorldEntity::default(); 8],
            entity_count: 0,
            audience_present: false,
            audience_attention: 0.0,
        }
    }
}

impl WorldModelFrame {
    #[must_use]
    pub fn from_frames(sensors: &SensorFrame, body: &BodyFeedback) -> Self {
        let mut world = Self {
            audience_present: sensors.user_presence.unwrap_or(0.0) >= 0.2,
            audience_attention: sensors
                .user_availability
                .unwrap_or_else(|| u8::from(sensors.user_idle_seconds < 45.0) as f32)
                .clamp(0.0, 1.0),
            ..Self::default()
        };
        world.observe(WorldEntity::point(
            1,
            WorldEntityKind::Cursor,
            sensors.cursor_position,
            sensors.cursor_velocity,
            1.0,
            (1.0 - sensors.cursor_distance_to_pet).clamp(0.0, 1.0),
            if sensors.embodied_interaction.contact.active {
                WorldAffordance::Touch
            } else {
                WorldAffordance::Approach
            },
            sensors.timestamp,
        ));
        if world.audience_present {
            world.observe(WorldEntity::point(
                2,
                WorldEntityKind::Viewer,
                Vec2::splat(0.5),
                Vec2::ZERO,
                world.audience_attention,
                world.audience_attention,
                WorldAffordance::Observe,
                sensors.timestamp,
            ));
        }
        if sensors.embodied_interaction.contact.active {
            world.observe(WorldEntity::point(
                3,
                WorldEntityKind::Contact,
                sensors.embodied_interaction.contact.point_world,
                sensors.embodied_interaction.contact.relative_velocity_local,
                1.0,
                1.0,
                WorldAffordance::Touch,
                sensors.timestamp,
            ));
        }
        for component in sensors.embodied_interaction.components
            [..usize::from(sensors.embodied_interaction.component_observation_count)]
            .iter()
        {
            world.observe(WorldEntity::point(
                100 + u64::from(component.component_id),
                WorldEntityKind::BodyComponent,
                component.center_world,
                Vec2::ZERO,
                1.0,
                (component.mass_fraction + component.distance_to_main * 0.25).clamp(0.0, 1.0),
                WorldAffordance::Help,
                sensors.timestamp,
            ));
        }
        for (index, surface) in sensors.visible_surfaces.iter().enumerate() {
            let position = (surface.rect.minimum + surface.rect.maximum) * 0.5;
            let mut entity = WorldEntity::point(
                500 + index as u64,
                WorldEntityKind::Surface,
                position,
                Vec2::ZERO,
                0.9,
                0.3,
                WorldAffordance::Land,
                sensors.timestamp,
            );
            entity.bbox = surface.rect;
            world.observe(entity);
        }
        if body.current_surface.is_some() {
            world.observe(WorldEntity::point(
                4,
                WorldEntityKind::Surface,
                body.world_position,
                body.velocity,
                1.0,
                0.4,
                WorldAffordance::Land,
                sensors.timestamp,
            ));
        }
        world
    }

    pub fn observe(&mut self, entity: WorldEntity) {
        let index = usize::from(self.entity_count);
        if index < self.entities.len() {
            self.entities[index] = entity;
            self.entity_count = self.entity_count.saturating_add(1);
        }
    }

    #[must_use]
    pub fn most_salient_object(&self) -> Option<WorldEntity> {
        self.entities[..usize::from(self.entity_count)]
            .iter()
            .copied()
            .filter(|entity| entity.kind == WorldEntityKind::ProceduralObject)
            .max_by(|left, right| left.salience.total_cmp(&right.salience))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CreaturePhrase {
    pub phrase_id: u64,
    pub episode_id: u64,
    pub intent: SocialIntent,
    pub voice_gesture: Option<VoiceGesture>,
    pub plan: InteractionResponsePlan,
    pub physical: PhysicalExpressionContext,
    pub priority: u8,
    pub quiet_suppressed: bool,
    pub audience_effect: f32,
}

#[derive(Debug, Clone, Copy)]
struct ActivePhrase {
    phrase: CreaturePhrase,
    remaining_seconds: f32,
}

/// Transient coordinator. Persistent affect, repertoire and learning remain in
/// `LifeCore`; this object owns only short phrase locks and cross-channel shape.
#[derive(Debug, Default)]
pub struct ExpressionDirector {
    active: Option<ActivePhrase>,
}

impl ExpressionDirector {
    pub fn tick(&mut self, dt: f32) {
        let Some(active) = &mut self.active else {
            return;
        };
        active.remaining_seconds = (active.remaining_seconds - dt.max(0.0)).max(0.0);
        if active.remaining_seconds <= f32::EPSILON {
            self.active = None;
        }
    }

    #[must_use]
    pub fn direct(
        &mut self,
        plan: InteractionResponsePlan,
        living: LivingStateFrame,
        physical: PhysicalExpressionContext,
    ) -> CreaturePhrase {
        self.direct_world(plan, living, physical, WorldModelFrame::default())
    }

    #[must_use]
    pub fn direct_world(
        &mut self,
        mut plan: InteractionResponsePlan,
        living: LivingStateFrame,
        physical: PhysicalExpressionContext,
        world: WorldModelFrame,
    ) -> CreaturePhrase {
        let intent = social_intent(plan.reason);
        let priority = intent_priority(intent);
        if let Some(active) = self.active
            && active.remaining_seconds > 0.0
            && active.phrase.episode_id == plan.episode_id
        {
            return active.phrase;
        }

        let load = physical.load();
        let (
            eye_aperture,
            eye_scale,
            brow_asymmetry,
            mouth_curve,
            mouth_compression,
            effort,
            relief,
        ) = expression_shape(intent);
        plan.expression.eye_aperture = eye_aperture;
        plan.expression.eye_scale = eye_scale;
        plan.expression.brow_asymmetry = brow_asymmetry;
        plan.expression.mouth_curve = mouth_curve;
        plan.expression.mouth_compression = (mouth_compression + load * 0.24).clamp(0.0, 1.0);
        plan.expression.mouth_asymmetry = if physical.airborne {
            (physical.speed - 0.35).clamp(0.0, 0.35)
        } else {
            0.0
        };
        plan.expression.effort = effort.max(load);
        plan.expression.relief = relief * (1.0 - load);
        let audience_effect = if world.audience_present {
            world.audience_attention * (0.35 + living.affect.attachment * 0.45)
        } else {
            0.0
        };
        plan.expression.amplitude =
            (0.72 + living.affect.arousal * 0.22 + load * 0.28 + audience_effect * 0.08)
                .clamp(0.55, 1.18);
        plan.body.local_pulse = plan
            .body
            .local_pulse
            .max((physical.slosh_energy * 0.012).clamp(0.0, 0.018));
        plan.body.recoil = plan.body.recoil.max((load * 0.055).clamp(0.0, 0.07));
        plan.body.lean = (plan.body.lean
            + (living.affect.attachment - 0.5) * world.audience_attention * 0.045)
            .clamp(-0.08, 0.08);
        if intent == SocialIntent::Boundary {
            plan.body.resistance = plan.body.resistance.max(0.62);
            plan.body.cooperation = 0.0;
            plan.gaze = InteractionGazeTarget::Away;
        } else if intent == SocialIntent::Alarm {
            plan.gaze = InteractionGazeTarget::Cursor;
        } else if matches!(intent, SocialIntent::Notice | SocialIntent::Query)
            && let Some(entity) = world.most_salient_object()
        {
            plan.gaze = InteractionGazeTarget::world_entity(entity.id, entity.position);
        } else if intent == SocialIntent::Relief
            || (audience_effect > 0.35
                && matches!(
                    intent,
                    SocialIntent::Acknowledge | SocialIntent::Invite | SocialIntent::Contact
                ))
        {
            plan.gaze = InteractionGazeTarget::Viewer;
        }
        plan.onset_seconds = (plan.onset_seconds
            * (1.18 - living.affect.attachment * 0.30 + living.affect.stress * 0.12))
            .clamp(0.04, 0.18);
        plan.hold_seconds = (0.80 + living.affect.attachment * 0.45 + load * 0.35).clamp(0.80, 2.0);
        plan.release_seconds = plan.release_seconds.clamp(0.18, 0.60);
        let quiet_suppressed = living.quiet_preferred && priority < 220;
        let voice_gesture = plan.voice_trigger.map(VoiceGesture::for_trigger);
        if quiet_suppressed {
            plan.voice_trigger = None;
        }
        plan.sanitize();
        let phrase = CreaturePhrase {
            phrase_id: plan.response_id,
            episode_id: plan.episode_id,
            intent,
            voice_gesture: (!quiet_suppressed).then_some(voice_gesture).flatten(),
            plan,
            physical,
            priority,
            quiet_suppressed,
            audience_effect,
        };
        self.active = Some(ActivePhrase {
            phrase,
            remaining_seconds: phrase.plan.onset_seconds
                + phrase.plan.hold_seconds
                + phrase.plan.release_seconds,
        });
        phrase
    }
}

#[derive(Debug, Default)]
pub struct VocalArbiter {
    next_allowed_seconds: f64,
    last_request_id: Option<u64>,
}

impl VocalArbiter {
    #[must_use]
    pub fn admit(
        &mut self,
        request: VocalRequest,
        now_seconds: f64,
        quiet_preferred: bool,
    ) -> Option<VocalRequest> {
        if (quiet_preferred && request.priority < 220)
            || now_seconds < self.next_allowed_seconds
            || self.last_request_id == Some(request.performance_seed)
        {
            return None;
        }
        let hold = match request.gesture {
            VoiceGesture::PurrHum => 0.55,
            VoiceGesture::LowRumble => 0.44,
            VoiceGesture::ReliefExhale => 0.36,
            VoiceGesture::MewWhine => 0.32,
            VoiceGesture::WarmChuff | VoiceGesture::ClippedPulse => 0.24,
        };
        self.next_allowed_seconds = now_seconds + hold;
        self.last_request_id = Some(request.performance_seed);
        Some(request)
    }
}

#[must_use]
pub const fn social_intent(reason: InteractionReasonCode) -> SocialIntent {
    match reason {
        InteractionReasonCode::CuriousInspection => SocialIntent::Query,
        InteractionReasonCode::GentleContact => SocialIntent::Contact,
        InteractionReasonCode::PlayfulCooperation | InteractionReasonCode::SharedRitual => {
            SocialIntent::Invite
        }
        InteractionReasonCode::PhysicalStartle | InteractionReasonCode::ComponentDetached => {
            SocialIntent::Alarm
        }
        InteractionReasonCode::EffortfulResistance => SocialIntent::Effort,
        InteractionReasonCode::CalmBoundary => SocialIntent::Boundary,
        InteractionReasonCode::RhythmRecognition => SocialIntent::Acknowledge,
        InteractionReasonCode::ComponentRecovery | InteractionReasonCode::SuccessfulReunion => {
            SocialIntent::Relief
        }
        InteractionReasonCode::QuietAcknowledgement => SocialIntent::Notice,
    }
}

const fn intent_priority(intent: SocialIntent) -> u8 {
    match intent {
        SocialIntent::Alarm => 255,
        SocialIntent::Boundary => 240,
        SocialIntent::Effort => 210,
        SocialIntent::Relief => 190,
        SocialIntent::Invite => 170,
        SocialIntent::Query => 150,
        SocialIntent::Contact => 130,
        SocialIntent::Acknowledge => 110,
        SocialIntent::Notice => 90,
    }
}

const fn expression_shape(intent: SocialIntent) -> (f32, f32, f32, f32, f32, f32, f32) {
    match intent {
        SocialIntent::Notice => (0.92, 1.08, 0.08, 0.08, 0.08, 0.08, 0.10),
        SocialIntent::Contact => (0.82, 1.04, 0.02, 0.34, 0.06, 0.04, 0.28),
        SocialIntent::Acknowledge => (0.88, 1.02, -0.10, 0.20, 0.10, 0.10, 0.20),
        SocialIntent::Invite => (0.96, 1.13, 0.16, 0.52, 0.03, 0.10, 0.18),
        SocialIntent::Query => (0.94, 1.14, 0.42, 0.02, 0.14, 0.16, 0.02),
        SocialIntent::Effort => (0.64, 0.94, -0.12, -0.22, 0.68, 0.88, 0.0),
        SocialIntent::Boundary => (0.52, 0.92, 0.0, -0.32, 0.86, 0.70, 0.0),
        SocialIntent::Alarm => (1.0, 1.18, 0.24, -0.28, 0.48, 1.0, 0.0),
        SocialIntent::Relief => (0.76, 1.0, -0.04, 0.40, 0.02, 0.0, 0.92),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{InteractionExpressionTarget, VocalTrigger};

    fn plan(reason: InteractionReasonCode, trigger: VocalTrigger) -> InteractionResponsePlan {
        InteractionResponsePlan {
            response_id: 7,
            episode_id: 9,
            reason,
            expression: InteractionExpressionTarget::default(),
            voice_trigger: Some(trigger),
            ..InteractionResponsePlan::default()
        }
    }

    #[test]
    fn physical_load_is_visible_and_phrase_timing_meets_contract() {
        let mut director = ExpressionDirector::default();
        let mut life = LifeState::new(crate::Genome::from_seed(8));
        life.affect.attachment = 0.7;
        let phrase = director.direct(
            plan(
                InteractionReasonCode::EffortfulResistance,
                VocalTrigger::CalmBoundary,
            ),
            LivingStateFrame::from_life(&life),
            PhysicalExpressionContext {
                strain: 0.9,
                deformation: 0.8,
                airborne: true,
                speed: 0.8,
                ..PhysicalExpressionContext::default()
            },
        );
        assert!(phrase.plan.onset_seconds <= 0.2);
        assert!((0.8..=2.0).contains(&phrase.plan.hold_seconds));
        assert!(phrase.plan.expression.effort >= 0.8);
        assert!(phrase.plan.expression.mouth_asymmetry > 0.0);
    }

    #[test]
    fn quiet_state_suppresses_low_priority_voice_but_not_boundary() {
        let mut director = ExpressionDirector::default();
        let mut life = LifeState::new(crate::Genome::from_seed(8));
        life.current_action = ActionId::Sleep;
        let quiet = director.direct(
            plan(
                InteractionReasonCode::GentleContact,
                VocalTrigger::SoftTouch,
            ),
            LivingStateFrame::from_life(&life),
            PhysicalExpressionContext::default(),
        );
        assert!(quiet.quiet_suppressed);
        assert!(quiet.plan.voice_trigger.is_none());

        director.tick(4.0);
        let boundary = director.direct(
            plan(
                InteractionReasonCode::CalmBoundary,
                VocalTrigger::CalmBoundary,
            ),
            LivingStateFrame::from_life(&life),
            PhysicalExpressionContext::default(),
        );
        assert!(!boundary.quiet_suppressed);
        assert_eq!(boundary.voice_gesture, Some(VoiceGesture::LowRumble));
    }

    #[test]
    fn vocal_arbiter_has_one_gate_and_quiet_wins() {
        let request = VocalRequest {
            motif_id: 1,
            performance_seed: 2,
            gain: 0.2,
            pan: 0.0,
            pitch_scale: 1.0,
            tempo_scale: 1.0,
            stress: 0.0,
            purr: false,
            gesture: VoiceGesture::WarmChuff,
            priority: 150,
            rhythm_intervals: [0.0; 8],
        };
        let mut arbiter = VocalArbiter::default();
        assert!(arbiter.admit(request.clone(), 1.0, true).is_none());
        let mut boundary = request;
        boundary.performance_seed = 3;
        boundary.gesture = VoiceGesture::LowRumble;
        boundary.priority = 240;
        assert!(arbiter.admit(boundary, 1.0, true).is_some());
    }

    #[test]
    fn audience_presence_changes_gaze_without_inventing_an_entity() {
        let mut director = ExpressionDirector::default();
        let life = LifeState::new(crate::Genome::from_seed(11));
        let phrase = director.direct_world(
            plan(
                InteractionReasonCode::GentleContact,
                VocalTrigger::SoftTouch,
            ),
            LivingStateFrame::from_life(&life),
            PhysicalExpressionContext::default(),
            WorldModelFrame {
                audience_present: true,
                audience_attention: 1.0,
                ..WorldModelFrame::default()
            },
        );
        assert!(phrase.audience_effect > 0.35);
        assert_eq!(phrase.plan.gaze, InteractionGazeTarget::Viewer);
    }

    #[test]
    fn query_gaze_references_the_observed_world_object() {
        let mut director = ExpressionDirector::default();
        let life = LifeState::new(crate::Genome::from_seed(12));
        let mut world = WorldModelFrame::default();
        world.observe(WorldEntity::point(
            77,
            WorldEntityKind::ProceduralObject,
            Vec2::new(0.72, 0.31),
            Vec2::ZERO,
            1.0,
            0.9,
            WorldAffordance::Help,
            2.0,
        ));
        let phrase = director.direct_world(
            plan(
                InteractionReasonCode::CuriousInspection,
                VocalTrigger::NeedHelp,
            ),
            LivingStateFrame::from_life(&life),
            PhysicalExpressionContext::default(),
            world,
        );
        let InteractionGazeTarget::WorldEntity { id, .. } = phrase.plan.gaze else {
            panic!("query did not retain a world-object referent")
        };
        assert_eq!(id, 77);
        assert!(
            phrase
                .plan
                .gaze
                .world_position()
                .is_some_and(|position| position.distance(Vec2::new(0.72, 0.31)) < 2.0e-5)
        );
    }

    #[test]
    fn lexicon_learning_is_bounded_and_changes_timing_not_identity() {
        let mut lexicon = VocalLexiconState::default();
        let gesture_before = lexicon.entry(SocialIntent::Invite).gesture;
        for episode in 1..=2_000 {
            lexicon.update_social_timing(SocialIntent::Invite, episode, 1.0);
        }
        let learned = lexicon.entry(SocialIntent::Invite);
        assert!(lexicon.is_valid());
        assert_eq!(learned.gesture, gesture_before);
        assert_eq!(learned.timing_scale, 1.12);
        assert!(learned.rhythm_bias <= 0.12);
        assert_eq!(lexicon.update_count, 2_000);
    }
}
