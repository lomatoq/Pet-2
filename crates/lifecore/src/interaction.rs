use glam::Vec2;
use serde::{Deserialize, Serialize};

use crate::{AffectState, Drives, TemperamentGenome, VocalTrigger};

pub const MAX_TRACKED_BODY_COMPONENTS: usize = 4;
pub const MAX_GESTURE_CAUSES: usize = 6;
pub const MAX_INTERACTION_VARIANTS: usize = 3;

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PointerMaterialContact {
    pub active: bool,
    /// Body-local point relative to the accepted main-component COM.
    pub point_local: Vec2,
    /// Normalized virtual-desktop point used only for live gaze.
    pub point_world: Vec2,
    pub normal_local: Vec2,
    pub area_fraction: f32,
    /// Force-density proxy; a mouse is not a hardware pressure sensor.
    pub effective_pressure: f32,
    /// Body radii per second.
    pub pointer_speed: f32,
    /// Body radii per second squared.
    pub pointer_acceleration: f32,
    pub pointer_velocity_local: Vec2,
    pub pointer_acceleration_local: Vec2,
    pub material_velocity_local: Vec2,
    pub relative_velocity_local: Vec2,
    pub contact_seconds: f32,
    pub pressure_impulse: f32,
}

impl PointerMaterialContact {
    pub fn sanitize(&mut self) {
        self.point_local = finite_vector(self.point_local);
        self.point_world = finite_vector(self.point_world).clamp(Vec2::ZERO, Vec2::ONE);
        self.normal_local = finite_vector(self.normal_local).normalize_or_zero();
        self.area_fraction = unit(self.area_fraction);
        self.effective_pressure = unit(self.effective_pressure);
        self.pointer_speed = finite_non_negative(self.pointer_speed, 32.0);
        self.pointer_acceleration = finite_non_negative(self.pointer_acceleration, 512.0);
        self.pointer_velocity_local =
            finite_vector(self.pointer_velocity_local).clamp_length_max(32.0);
        self.pointer_acceleration_local =
            finite_vector(self.pointer_acceleration_local).clamp_length_max(512.0);
        self.material_velocity_local =
            finite_vector(self.material_velocity_local).clamp_length_max(32.0);
        self.relative_velocity_local =
            finite_vector(self.relative_velocity_local).clamp_length_max(64.0);
        self.contact_seconds = finite_non_negative(self.contact_seconds, 60.0);
        self.pressure_impulse = finite_non_negative(self.pressure_impulse, 60.0);
        if !self.active {
            self.area_fraction = 0.0;
            self.effective_pressure = 0.0;
            self.contact_seconds = 0.0;
        }
    }

    #[must_use]
    pub fn is_valid(self) -> bool {
        self.point_local.is_finite()
            && self.point_world.is_finite()
            && self.point_world.cmpge(Vec2::ZERO).all()
            && self.point_world.cmple(Vec2::ONE).all()
            && self.normal_local.is_finite()
            && self.normal_local.length() <= 1.000_1
            && self.pointer_velocity_local.is_finite()
            && self.pointer_velocity_local.length() <= 32.0 + 1.0e-4
            && self.pointer_acceleration_local.is_finite()
            && self.pointer_acceleration_local.length() <= 512.0 + 1.0e-3
            && self.material_velocity_local.is_finite()
            && self.material_velocity_local.length() <= 32.0 + 1.0e-4
            && self.relative_velocity_local.is_finite()
            && self.relative_velocity_local.length() <= 64.0 + 1.0e-4
            && [
                self.area_fraction,
                self.effective_pressure,
                self.pointer_speed,
                self.pointer_acceleration,
                self.contact_seconds,
                self.pressure_impulse,
            ]
            .into_iter()
            .all(f32::is_finite)
            && (0.0..=1.0).contains(&self.area_fraction)
            && (0.0..=1.0).contains(&self.effective_pressure)
            && (0.0..=32.0).contains(&self.pointer_speed)
            && (0.0..=512.0).contains(&self.pointer_acceleration)
            && (0.0..=60.0).contains(&self.contact_seconds)
            && (0.0..=60.0).contains(&self.pressure_impulse)
            && (self.active
                || (self.area_fraction == 0.0
                    && self.effective_pressure == 0.0
                    && self.contact_seconds == 0.0))
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BodyMaterialState {
    pub deformation_energy: f32,
    pub deformation_rate: f32,
    pub maximum_strain: f32,
    pub neck_tension: f32,
    pub neck_thickness: f32,
    pub slosh_energy: f32,
    pub internal_relative_speed: f32,
    pub detached_mass_fraction: f32,
    pub component_count: u8,
    pub mass_conservation_error: f32,
    pub topology_budget_remaining: f32,
    pub topology_budget_exhausted: bool,
}

impl BodyMaterialState {
    pub fn sanitize(&mut self) {
        self.deformation_energy = unit(self.deformation_energy);
        self.deformation_rate = finite_signed(self.deformation_rate, 32.0);
        self.maximum_strain = finite_non_negative(self.maximum_strain, 8.0);
        self.neck_tension = unit(self.neck_tension);
        self.neck_thickness = unit(self.neck_thickness);
        self.slosh_energy = unit(self.slosh_energy);
        self.internal_relative_speed = finite_non_negative(self.internal_relative_speed, 32.0);
        self.detached_mass_fraction = unit(self.detached_mass_fraction);
        self.component_count = self.component_count.min(MAX_TRACKED_BODY_COMPONENTS as u8);
        self.mass_conservation_error = finite_non_negative(self.mass_conservation_error, 1.0);
        self.topology_budget_remaining = unit(self.topology_budget_remaining);
    }

    #[must_use]
    pub fn is_valid(self) -> bool {
        let mut sanitized = self;
        sanitized.sanitize();
        sanitized == self && self.mass_conservation_error <= 1.0e-5
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ComponentLifecycle {
    #[default]
    Attached,
    Necking,
    Detached,
    Returning,
    Merging,
    Recovered,
    DeterministicRecovery,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ComponentDetachReason {
    #[default]
    None,
    PointerStrain,
    BodyInertia,
    Impact,
    IntentionalBud,
    Emergency,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BodyComponentObservation {
    pub component_id: u8,
    pub lifecycle: ComponentLifecycle,
    pub detach_reason: ComponentDetachReason,
    pub particle_count: u8,
    pub mass_fraction: f32,
    pub center_local: Vec2,
    pub center_world: Vec2,
    pub velocity_local: Vec2,
    pub angular_velocity: f32,
    pub deformation: f32,
    pub age_seconds: f32,
    pub distance_to_main: f32,
    pub offscreen_seconds: f32,
}

impl BodyComponentObservation {
    pub fn sanitize(&mut self) {
        self.particle_count = self.particle_count.min(96);
        self.mass_fraction = unit(self.mass_fraction);
        self.center_local = finite_vector(self.center_local);
        self.center_world = finite_vector(self.center_world).clamp(Vec2::ZERO, Vec2::ONE);
        self.velocity_local = finite_vector(self.velocity_local).clamp_length_max(32.0);
        self.angular_velocity = finite_signed(self.angular_velocity, 64.0);
        self.deformation = finite_non_negative(self.deformation, 8.0);
        self.age_seconds = finite_non_negative(self.age_seconds, 3_600.0);
        self.distance_to_main = finite_non_negative(self.distance_to_main, 64.0);
        self.offscreen_seconds = finite_non_negative(self.offscreen_seconds, 60.0);
    }

    #[must_use]
    pub fn is_valid(self) -> bool {
        let mut sanitized = self;
        sanitized.sanitize();
        sanitized == self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EmbodiedInteractionFrame {
    pub sequence: u64,
    pub timestamp: f64,
    pub contact: PointerMaterialContact,
    pub material: BodyMaterialState,
    pub components: [BodyComponentObservation; MAX_TRACKED_BODY_COMPONENTS],
    pub component_observation_count: u8,
    pub detached_event: Option<u8>,
    pub remerge_event: Option<u8>,
    pub recovery_event: Option<u8>,
}

impl Default for EmbodiedInteractionFrame {
    fn default() -> Self {
        Self {
            sequence: 0,
            timestamp: 0.0,
            contact: PointerMaterialContact::default(),
            material: BodyMaterialState {
                component_count: 1,
                topology_budget_remaining: 1.0,
                ..BodyMaterialState::default()
            },
            components: [BodyComponentObservation::default(); MAX_TRACKED_BODY_COMPONENTS],
            component_observation_count: 0,
            detached_event: None,
            remerge_event: None,
            recovery_event: None,
        }
    }
}

impl EmbodiedInteractionFrame {
    pub fn sanitize(&mut self) {
        if !self.timestamp.is_finite() || self.timestamp < 0.0 {
            self.timestamp = 0.0;
        }
        self.contact.sanitize();
        self.material.sanitize();
        self.component_observation_count = self
            .component_observation_count
            .min(MAX_TRACKED_BODY_COMPONENTS as u8);
        for (index, component) in self.components.iter_mut().enumerate() {
            component.sanitize();
            if index >= usize::from(self.component_observation_count) {
                *component = BodyComponentObservation::default();
            }
        }
        let event_is_observed = |event: Option<u8>| {
            event.is_none()
                || self.components[..usize::from(self.component_observation_count)]
                    .iter()
                    .any(|component| Some(component.component_id) == event)
        };
        if !event_is_observed(self.detached_event) {
            self.detached_event = None;
        }
        if !event_is_observed(self.remerge_event) {
            self.remerge_event = None;
        }
        if !event_is_observed(self.recovery_event) {
            self.recovery_event = None;
        }
    }

    #[must_use]
    pub fn is_valid(self) -> bool {
        let event_is_observed = |event: Option<u8>| {
            event.is_none()
                || self.components[..usize::from(self.component_observation_count)]
                    .iter()
                    .any(|component| Some(component.component_id) == event)
        };
        self.timestamp.is_finite()
            && self.timestamp >= 0.0
            && usize::from(self.component_observation_count) <= MAX_TRACKED_BODY_COMPONENTS
            && self.contact.is_valid()
            && self.material.is_valid()
            && self.components[..usize::from(self.component_observation_count)]
                .iter()
                .all(|component| component.is_valid())
            && self.components[usize::from(self.component_observation_count)..]
                .iter()
                .all(|component| *component == BodyComponentObservation::default())
            && event_is_observed(self.detached_event)
            && event_is_observed(self.remerge_event)
            && event_is_observed(self.recovery_event)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EmbodiedGestureKind {
    #[default]
    Unknown,
    SoftTouch,
    SlowStretch,
    Tickle,
    RhythmicTouch,
    CircularTwist,
    SharpFlick,
    Hold,
    PullAndRelease,
    FragmentSeparationAttempt,
    SharedPlayInvitation,
    FragmentHelp,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GestureCause {
    #[default]
    None,
    ContactOnset,
    LowPressure,
    SustainedPressure,
    RadialStretch,
    TangentialMotion,
    HighReleaseSpeed,
    HighAcceleration,
    RegularImpulseTrain,
    ClosedCircularPath,
    RisingNeckTension,
    ComponentSplit,
    FragmentTowardMain,
    PredictionMismatch,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GestureClassification {
    pub episode_id: u64,
    pub kind: EmbodiedGestureKind,
    pub confidence: f32,
    pub second_best_confidence: f32,
    pub intensity: f32,
    pub predicted_kind: EmbodiedGestureKind,
    pub prediction_confidence: f32,
    pub prediction_error: f32,
    pub causes: [GestureCause; MAX_GESTURE_CAUSES],
    pub cause_count: u8,
    pub committed: bool,
    pub ended: bool,
}

impl Default for GestureClassification {
    fn default() -> Self {
        Self {
            episode_id: 0,
            kind: EmbodiedGestureKind::Unknown,
            confidence: 0.0,
            second_best_confidence: 0.0,
            intensity: 0.0,
            predicted_kind: EmbodiedGestureKind::Unknown,
            prediction_confidence: 0.0,
            prediction_error: 0.0,
            causes: [GestureCause::None; MAX_GESTURE_CAUSES],
            cause_count: 0,
            committed: false,
            ended: false,
        }
    }
}

impl GestureClassification {
    pub fn sanitize(&mut self) {
        self.confidence = unit(self.confidence);
        self.second_best_confidence = unit(self.second_best_confidence).min(self.confidence);
        self.intensity = unit(self.intensity);
        self.prediction_confidence = unit(self.prediction_confidence);
        self.prediction_error = unit(self.prediction_error);
        self.cause_count = self.cause_count.min(MAX_GESTURE_CAUSES as u8);
        for cause in &mut self.causes[usize::from(self.cause_count)..] {
            *cause = GestureCause::None;
        }
        if self.kind == EmbodiedGestureKind::Unknown {
            self.committed = false;
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GestureBoundaryEvent {
    #[default]
    None,
    Overstrain,
    ExcessivePressure,
    TopologyBudgetExhausted,
    QuietOrSleep,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EmbodiedGestureEvent {
    pub classification: GestureClassification,
    pub frame: EmbodiedInteractionFrame,
    pub boundary: GestureBoundaryEvent,
    pub observation_quality: f32,
}

impl EmbodiedGestureEvent {
    pub fn sanitize(&mut self) {
        self.classification.sanitize();
        self.frame.sanitize();
        self.observation_quality = unit(self.observation_quality);
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct InteractionAppraisal {
    pub valence: f32,
    pub arousal: f32,
    pub controllability: f32,
    pub expectedness: f32,
    pub social_relevance: f32,
    pub effort: f32,
    pub play_opportunity: f32,
    pub boundary_need: f32,
    pub cooperation: f32,
    pub uncertainty: f32,
}

impl InteractionAppraisal {
    pub fn sanitize(&mut self) {
        self.valence = finite_signed(self.valence, 1.0);
        self.arousal = unit(self.arousal);
        self.controllability = unit(self.controllability);
        self.expectedness = unit(self.expectedness);
        self.social_relevance = unit(self.social_relevance);
        self.effort = unit(self.effort);
        self.play_opportunity = unit(self.play_opportunity);
        self.boundary_need = unit(self.boundary_need);
        self.cooperation = unit(self.cooperation).min(1.0 - self.boundary_need);
        self.uncertainty = unit(self.uncertainty);
    }

    #[must_use]
    pub fn is_valid(self) -> bool {
        let mut sanitized = self;
        sanitized.sanitize();
        sanitized == self
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InteractionReasonCode {
    #[default]
    CuriousInspection,
    GentleContact,
    PlayfulCooperation,
    PhysicalStartle,
    EffortfulResistance,
    CalmBoundary,
    RhythmRecognition,
    SharedRitual,
    ComponentDetached,
    ComponentRecovery,
    SuccessfulReunion,
    QuietAcknowledgement,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommunicativeIntent {
    #[default]
    Inspect,
    AcknowledgeContact,
    YieldAndInviteReturn,
    InviteRepeat,
    InviteSpin,
    ResistSafely,
    SetCalmBoundary,
    OrientToFragment,
    AcknowledgeHelp,
    Disengage,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InteractionGazeTarget {
    #[default]
    ContactPoint,
    Cursor,
    Viewer,
    MainComponent,
    DetachedComponent(u8),
    MergePoint,
    WorldEntity {
        id: u64,
        position_q16: [u16; 2],
    },
    Away,
}

impl InteractionGazeTarget {
    #[must_use]
    pub fn world_entity(id: u64, position: Vec2) -> Self {
        let position = position.clamp(Vec2::ZERO, Vec2::ONE);
        Self::WorldEntity {
            id,
            position_q16: [
                (position.x * f32::from(u16::MAX)).round() as u16,
                (position.y * f32::from(u16::MAX)).round() as u16,
            ],
        }
    }

    #[must_use]
    pub fn world_position(self) -> Option<Vec2> {
        let Self::WorldEntity { position_q16, .. } = self else {
            return None;
        };
        Some(Vec2::new(
            f32::from(position_q16[0]) / f32::from(u16::MAX),
            f32::from(position_q16[1]) / f32::from(u16::MAX),
        ))
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct InteractionExpressionTarget {
    pub eye_aperture: f32,
    pub eye_scale: f32,
    pub brow_asymmetry: f32,
    pub mouth_curve: f32,
    pub mouth_compression: f32,
    pub mouth_asymmetry: f32,
    pub effort: f32,
    pub relief: f32,
    pub amplitude: f32,
}

impl InteractionExpressionTarget {
    pub fn sanitize(&mut self) {
        self.eye_aperture = unit(self.eye_aperture);
        self.eye_scale = bounded(self.eye_scale, 0.88, 1.18, 1.0);
        self.brow_asymmetry = finite_signed(self.brow_asymmetry, 1.0);
        self.mouth_curve = finite_signed(self.mouth_curve, 1.0);
        self.mouth_compression = unit(self.mouth_compression);
        self.mouth_asymmetry = finite_signed(self.mouth_asymmetry, 1.0);
        self.effort = unit(self.effort);
        self.relief = unit(self.relief);
        self.amplitude = bounded(self.amplitude, 0.0, 1.25, 0.0);
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct InteractionBodyActuation {
    pub target_component: Option<u8>,
    pub compliance_delta: f32,
    pub cohesion_delta: f32,
    pub local_pulse: f32,
    pub lean: f32,
    pub recoil: f32,
    pub resistance: f32,
    pub cooperation: f32,
    pub allow_intentional_bud: bool,
}

impl InteractionBodyActuation {
    pub fn sanitize(&mut self) {
        self.compliance_delta = finite_signed(self.compliance_delta, 0.25);
        self.cohesion_delta = finite_signed(self.cohesion_delta, 0.25);
        self.local_pulse = bounded(self.local_pulse, 0.0, 0.03, 0.0);
        self.lean = finite_signed(self.lean, 0.08);
        self.recoil = bounded(self.recoil, 0.0, 0.08, 0.0);
        self.resistance = unit(self.resistance);
        self.cooperation = unit(self.cooperation);
        if self.cooperation > 0.8 {
            self.resistance = self.resistance.min(0.35);
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReceiverEffect {
    #[default]
    Notice,
    ContinueGently,
    RepeatMotif,
    HelpFragment,
    ReducePressure,
    AllowDisengagement,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct InteractionResponsePlan {
    pub response_id: u64,
    pub episode_id: u64,
    pub reason: InteractionReasonCode,
    pub communicative_intent: CommunicativeIntent,
    pub gaze: InteractionGazeTarget,
    pub expression: InteractionExpressionTarget,
    pub body: InteractionBodyActuation,
    pub voice_trigger: Option<VocalTrigger>,
    pub onset_seconds: f32,
    pub hold_seconds: f32,
    pub release_seconds: f32,
    pub await_user_seconds: f32,
    pub cooldown_seconds: f32,
    pub interruptibility: f32,
    pub expected_receiver_effect: ReceiverEffect,
}

impl Default for InteractionResponsePlan {
    fn default() -> Self {
        Self {
            response_id: 0,
            episode_id: 0,
            reason: InteractionReasonCode::CuriousInspection,
            communicative_intent: CommunicativeIntent::Inspect,
            gaze: InteractionGazeTarget::ContactPoint,
            expression: InteractionExpressionTarget {
                eye_aperture: 1.0,
                eye_scale: 1.0,
                ..InteractionExpressionTarget::default()
            },
            body: InteractionBodyActuation::default(),
            voice_trigger: None,
            onset_seconds: 0.08,
            hold_seconds: 0.25,
            release_seconds: 0.18,
            await_user_seconds: 1.15,
            cooldown_seconds: 1.25,
            interruptibility: 1.0,
            expected_receiver_effect: ReceiverEffect::Notice,
        }
    }
}

impl InteractionResponsePlan {
    pub fn sanitize(&mut self) {
        self.expression.sanitize();
        self.body.sanitize();
        self.onset_seconds = bounded(self.onset_seconds, 0.0, 1.0, 0.08);
        self.hold_seconds = bounded(self.hold_seconds, 0.0, 4.0, 0.25);
        self.release_seconds = bounded(self.release_seconds, 0.0, 2.0, 0.18);
        self.await_user_seconds = bounded(self.await_user_seconds, 0.45, 2.50, 1.15);
        self.cooldown_seconds = bounded(self.cooldown_seconds, 0.25, 5.0, 1.25);
        self.interruptibility = unit(self.interruptibility);
        if self.reason == InteractionReasonCode::CalmBoundary {
            self.body.cooperation = 0.0;
            self.body.resistance = self.body.resistance.max(0.45);
            self.expression.relief = 0.0;
        }
    }

    #[must_use]
    pub fn is_valid(self) -> bool {
        let mut sanitized = self;
        sanitized.sanitize();
        sanitized == self && self.response_id > 0 && self.episode_id > 0
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InteractionOutcomeKind {
    #[default]
    NoResponse,
    VoluntaryContinuation,
    ExplicitPositive,
    ExplicitNegative,
    Refusal,
    FragmentRemerged,
    InterruptedByRestart,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct InteractionOutcome {
    pub episode_id: u64,
    pub response_id: Option<u64>,
    pub kind: InteractionOutcomeKind,
    pub confidence: f32,
    pub elapsed_seconds: f32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InteractionTurnState {
    #[default]
    Idle,
    Anticipating,
    CreatureBid,
    UserContact,
    AwaitingClassification,
    Appraising,
    Responding,
    AwaitingUser,
    Repairing,
    Disengaging,
    Cooldown,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct InteractionTurnRuntime {
    pub episode_id: u64,
    pub response_id: Option<u64>,
    pub state: InteractionTurnState,
    pub elapsed_seconds: f32,
    pub response_emitted: bool,
    pub fresh_input_after_response: bool,
}

impl InteractionTurnRuntime {
    pub fn begin_episode(&mut self, episode_id: u64) -> bool {
        // A release/topology edge may arrive during Cooldown, but the physical
        // classifier's episode ID is still the same. Never reopen that episode;
        // a genuinely fresh classifier episode receives a fresh non-zero ID.
        if episode_id == 0 || self.episode_id == episode_id {
            return false;
        }
        *self = Self {
            episode_id,
            state: InteractionTurnState::UserContact,
            ..Self::default()
        };
        true
    }

    pub fn mark_fresh_input(&mut self) {
        if self.response_emitted
            && matches!(
                self.state,
                InteractionTurnState::AwaitingUser
                    | InteractionTurnState::Disengaging
                    | InteractionTurnState::Cooldown
            )
        {
            self.fresh_input_after_response = true;
        }
    }

    pub fn emit_response(&mut self, episode_id: u64, response_id: u64) -> bool {
        if episode_id == 0
            || response_id == 0
            || self.episode_id != episode_id
            || self.response_emitted
        {
            return false;
        }
        self.response_id = Some(response_id);
        self.response_emitted = true;
        self.state = InteractionTurnState::Responding;
        self.elapsed_seconds = 0.0;
        true
    }

    pub fn close_after_restart(&mut self) -> Option<InteractionOutcome> {
        let outcome = (self.episode_id != 0).then_some(InteractionOutcome {
            episode_id: self.episode_id,
            response_id: self.response_id,
            kind: InteractionOutcomeKind::InterruptedByRestart,
            confidence: 1.0,
            elapsed_seconds: self.elapsed_seconds,
        });
        *self = Self::default();
        outcome
    }

    pub fn tick(&mut self, plan: Option<InteractionResponsePlan>, dt: f32) {
        let dt = if dt.is_finite() {
            dt.clamp(0.0, 0.25)
        } else {
            0.0
        };
        self.elapsed_seconds = (self.elapsed_seconds + dt).min(60.0);
        match self.state {
            InteractionTurnState::Responding => {
                let response_duration = plan.map_or(0.45, |plan| {
                    plan.onset_seconds + plan.hold_seconds + plan.release_seconds
                });
                if self.elapsed_seconds >= response_duration {
                    self.state = InteractionTurnState::AwaitingUser;
                    self.elapsed_seconds = 0.0;
                }
            }
            InteractionTurnState::AwaitingUser => {
                let wait = plan.map_or(1.15, |plan| plan.await_user_seconds);
                if self.fresh_input_after_response || self.elapsed_seconds >= wait {
                    self.state = InteractionTurnState::Disengaging;
                    self.elapsed_seconds = 0.0;
                }
            }
            InteractionTurnState::Disengaging => {
                if self.elapsed_seconds >= 0.20 {
                    self.state = InteractionTurnState::Cooldown;
                    self.elapsed_seconds = 0.0;
                }
            }
            InteractionTurnState::Cooldown
                if self.elapsed_seconds >= plan.map_or(1.25, |plan| plan.cooldown_seconds) =>
            {
                *self = Self::default();
            }
            _ => {}
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PendingInteractionCredit {
    pub executed: bool,
    pub episode_id: u64,
    pub response_id: u64,
    pub variant: u8,
    pub expected_effect: ReceiverEffect,
    pub elapsed_seconds: f32,
    #[serde(default = "default_learning_openness")]
    pub learning_openness: f32,
}

const fn default_learning_openness() -> f32 {
    1.0
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PersistentInteractionState {
    pub pending_credit: Option<PendingInteractionCredit>,
    pub next_response_id: u64,
    pub last_responded_episode: u64,
    pub recent_boundary_events: u8,
    pub successful_voluntary_outcomes: u32,
    pub interrupted_outcomes: u32,
    pub last_appraisal: Option<InteractionAppraisal>,
    pub last_response_plan: Option<InteractionResponsePlan>,
    pub affect_before: Option<AffectState>,
    pub affect_after: Option<AffectState>,
}

impl Default for PersistentInteractionState {
    fn default() -> Self {
        Self {
            pending_credit: None,
            next_response_id: 1,
            last_responded_episode: 0,
            recent_boundary_events: 0,
            successful_voluntary_outcomes: 0,
            interrupted_outcomes: 0,
            last_appraisal: None,
            last_response_plan: None,
            affect_before: None,
            affect_after: None,
        }
    }
}

impl PersistentInteractionState {
    #[must_use]
    pub fn is_valid(self) -> bool {
        self.next_response_id > 0
            && self.recent_boundary_events <= 32
            && self.pending_credit.is_none_or(|credit| {
                credit.episode_id > 0
                    && credit.response_id > 0
                    && usize::from(credit.variant) < MAX_INTERACTION_VARIANTS
                    && credit.elapsed_seconds.is_finite()
                    && credit.elapsed_seconds >= 0.0
                    && credit.learning_openness.is_finite()
                    && (0.50..=1.25).contains(&credit.learning_openness)
            })
            && self
                .last_appraisal
                .is_none_or(InteractionAppraisal::is_valid)
            && self
                .last_response_plan
                .is_none_or(InteractionResponsePlan::is_valid)
            && self.affect_before.is_none_or(|affect| affect.is_finite())
            && self.affect_after.is_none_or(|affect| affect.is_finite())
    }
}

#[must_use]
pub fn appraise_embodied_gesture(
    event: EmbodiedGestureEvent,
    drives: Drives,
    affect: AffectState,
    temperament: &TemperamentGenome,
    successful_interactions: u32,
    recent_boundary_events: u8,
) -> InteractionAppraisal {
    let care = material_care_from_event(event);
    let pressure_integral = (event.frame.contact.pressure_impulse / 1.2).clamp(0.0, 1.0);
    let effort = event
        .frame
        .material
        .maximum_strain
        .max(event.frame.material.deformation_energy)
        .max(pressure_integral * (1.0 - care))
        .clamp(0.0, 1.0);
    let energy = (1.0 - drives.sleep.max(drives.comfort * 0.45)).clamp(0.0, 1.0);
    let recovery_confidence = (1.0
        - event.frame.material.detached_mass_fraction
        - event.frame.material.mass_conservation_error * 4.0)
        .clamp(0.0, 1.0);
    let controllability = ((1.0 - event.classification.prediction_error) * 0.72
        + recovery_confidence * 0.28)
        .clamp(0.0, 1.0);
    let sustained_overstrain =
        ((event.frame.material.maximum_strain - 0.58) / 0.42).clamp(0.0, 1.0);
    // Force integrated over a long, yielding caress is not excessive pressure.
    // Boundaries follow the current load; pressure/strain are still independent
    // witnesses even when the classifier calls a rough pull a slow stretch.
    let excessive_pressure =
        ((event.frame.contact.effective_pressure - 0.55) / 0.45).clamp(0.0, 1.0);
    let low_energy_high_demand = (1.0 - energy) * effort;
    let boundary_need = sustained_overstrain
        .max(excessive_pressure)
        .max(if event.frame.material.topology_budget_exhausted {
            1.0
        } else {
            0.0
        })
        .max(low_energy_high_demand)
        .clamp(0.0, 1.0);
    let success_memory = (successful_interactions as f32 / 12.0).clamp(0.0, 1.0);
    let boundary_memory = (f32::from(recent_boundary_events) / 8.0).clamp(0.0, 1.0);
    let interaction_trust = (affect.attachment * 0.24
        + (1.0 - drives.comfort) * 0.16
        + affect.confidence * 0.22
        + controllability * 0.18
        + success_memory * 0.20
        - drives.safety * 0.28
        - affect.frustration * 0.18
        - boundary_memory * 0.16)
        .clamp(0.0, 1.0);
    let play_opportunity = (temperament.playfulness * 0.45
        + drives.play * 0.30
        + event.classification.confidence * 0.25)
        .clamp(0.0, 1.0);
    let cooperation =
        (interaction_trust * temperament.playfulness * energy * (1.0 - boundary_need))
            .clamp(0.0, 1.0);
    let inherently_positive = matches!(
        event.classification.kind,
        EmbodiedGestureKind::SoftTouch
            | EmbodiedGestureKind::RhythmicTouch
            | EmbodiedGestureKind::Tickle
            | EmbodiedGestureKind::FragmentHelp
    );
    let care_valence = care * (1.0 - affect.stress) * (1.0 - affect.frustration);
    let mut appraisal = InteractionAppraisal {
        valence: ((if inherently_positive { 0.38 } else { 0.08 })
            + care_valence * 0.30
            + cooperation * 0.35
            - boundary_need * 0.72)
            .clamp(-1.0, 1.0),
        arousal: affect
            .arousal
            .max(effort * 0.60 + event.classification.prediction_error * 0.35)
            .clamp(0.0, 1.0),
        controllability,
        expectedness: 1.0 - event.classification.prediction_error,
        social_relevance: (0.35 + event.classification.confidence * 0.65).clamp(0.0, 1.0),
        effort,
        play_opportunity,
        boundary_need,
        cooperation,
        uncertainty: (1.0 - event.classification.confidence)
            .max(event.classification.prediction_error)
            .clamp(0.0, 1.0),
    };
    appraisal.sanitize();
    appraisal
}

#[must_use]
pub fn interaction_response_plan(
    event: EmbodiedGestureEvent,
    appraisal: InteractionAppraisal,
    response_id: u64,
    _variant: u8,
    response_amplitude: f32,
    await_user_seconds: f32,
    cooldown_seconds: f32,
) -> InteractionResponsePlan {
    let component_id = event
        .frame
        .components
        .iter()
        .take(usize::from(event.frame.component_observation_count))
        .find(|component| component.lifecycle != ComponentLifecycle::Attached)
        .map(|component| component.component_id);
    let component_gaze = component_id.map_or(InteractionGazeTarget::ContactPoint, |id| {
        InteractionGazeTarget::DetachedComponent(id)
    });
    let mut plan = InteractionResponsePlan {
        response_id,
        episode_id: event.classification.episode_id,
        await_user_seconds,
        cooldown_seconds,
        ..InteractionResponsePlan::default()
    };
    let mut expression = InteractionExpressionTarget {
        eye_aperture: 0.94,
        eye_scale: 1.0,
        amplitude: 0.45,
        ..InteractionExpressionTarget::default()
    };

    if event.boundary != GestureBoundaryEvent::None || appraisal.boundary_need >= 0.62 {
        plan.reason = InteractionReasonCode::CalmBoundary;
        plan.communicative_intent = CommunicativeIntent::SetCalmBoundary;
        plan.gaze = component_gaze;
        expression.eye_aperture = 0.72;
        expression.eye_scale = 0.96;
        expression.brow_asymmetry = -0.20;
        expression.mouth_curve = -0.12;
        expression.mouth_compression = 0.62;
        expression.effort = appraisal.effort;
        expression.amplitude = 0.55;
        plan.body.cohesion_delta = 0.18;
        plan.body.resistance = (0.45 + appraisal.boundary_need * 0.45).clamp(0.0, 1.0);
        plan.body.recoil = 0.025;
        plan.voice_trigger = Some(VocalTrigger::CalmBoundary);
        plan.expected_receiver_effect = ReceiverEffect::ReducePressure;
        plan.hold_seconds = 0.42;
    } else {
        match event.classification.kind {
            EmbodiedGestureKind::SoftTouch => {
                plan.reason = InteractionReasonCode::GentleContact;
                plan.communicative_intent = CommunicativeIntent::AcknowledgeContact;
                plan.gaze = InteractionGazeTarget::ContactPoint;
                expression.eye_aperture = 0.92;
                expression.eye_scale = 1.02;
                expression.mouth_curve = 0.24;
                expression.relief = 0.18;
                expression.amplitude = 0.46;
                plan.body.compliance_delta = 0.08;
                plan.body.local_pulse = 0.015;
                plan.voice_trigger = Some(VocalTrigger::SoftTouch);
                plan.expected_receiver_effect = ReceiverEffect::ContinueGently;
            }
            EmbodiedGestureKind::SlowStretch => {
                // Remove the generic cooperation term: willingness to play is
                // not evidence that a stressed pet enjoys the current touch.
                let accepted_care = ((appraisal.valence - 0.08 - appraisal.cooperation * 0.35
                    + appraisal.boundary_need * 0.72)
                    / 0.30)
                    .clamp(0.0, 1.0);
                let care = material_care_from_event(event)
                    * accepted_care
                    * (1.0 - appraisal.boundary_need);
                if care > 0.35 {
                    plan.reason = InteractionReasonCode::GentleContact;
                    plan.communicative_intent = CommunicativeIntent::AcknowledgeContact;
                    expression.eye_aperture = 0.94 - care * 0.10;
                    expression.mouth_curve = 0.12 + care * 0.18;
                    expression.relief = care * 0.16;
                    expression.mouth_compression = appraisal.effort * 0.12;
                    plan.body.compliance_delta = care * 0.08;
                    plan.body.cooperation = appraisal.cooperation;
                    plan.body.resistance = (1.0 - care) * 0.18;
                    plan.expected_receiver_effect = ReceiverEffect::ContinueGently;
                } else {
                    plan.reason = InteractionReasonCode::EffortfulResistance;
                    plan.communicative_intent = if appraisal.cooperation > 0.48 {
                        CommunicativeIntent::YieldAndInviteReturn
                    } else {
                        CommunicativeIntent::ResistSafely
                    };
                    expression.eye_aperture = 0.78;
                    expression.effort = appraisal.effort;
                    expression.mouth_compression = appraisal.effort * 0.72;
                    plan.body.cooperation = appraisal.cooperation;
                    plan.body.resistance = 1.0 - appraisal.cooperation;
                    plan.body.cohesion_delta = -0.06 * appraisal.cooperation;
                    plan.body.allow_intentional_bud = appraisal.cooperation > 0.62;
                }
            }
            EmbodiedGestureKind::Tickle | EmbodiedGestureKind::RhythmicTouch => {
                plan.reason = InteractionReasonCode::RhythmRecognition;
                plan.communicative_intent = CommunicativeIntent::InviteRepeat;
                plan.gaze = InteractionGazeTarget::Viewer;
                expression.eye_aperture = 1.0;
                expression.eye_scale = 1.07;
                expression.mouth_curve = 0.34;
                expression.amplitude = 0.66;
                plan.body.local_pulse = 0.020;
                plan.voice_trigger = Some(VocalTrigger::RhythmEcho);
                plan.expected_receiver_effect = ReceiverEffect::RepeatMotif;
            }
            EmbodiedGestureKind::CircularTwist => {
                plan.reason = InteractionReasonCode::SharedRitual;
                plan.communicative_intent = CommunicativeIntent::InviteSpin;
                plan.gaze = InteractionGazeTarget::Cursor;
                expression.eye_scale = 1.05;
                expression.brow_asymmetry = 0.18;
                expression.amplitude = 0.58;
                plan.body.lean = 0.045;
                plan.expected_receiver_effect = ReceiverEffect::RepeatMotif;
            }
            EmbodiedGestureKind::SharpFlick => {
                plan.reason = InteractionReasonCode::PhysicalStartle;
                plan.gaze = InteractionGazeTarget::Cursor;
                expression.eye_aperture = 1.0;
                expression.eye_scale = 1.12;
                expression.mouth_compression = 0.42;
                expression.amplitude = 0.70;
                plan.body.recoil = 0.065;
                plan.voice_trigger = Some(VocalTrigger::PhysicalStartle);
                plan.onset_seconds = 0.08;
                plan.hold_seconds = 0.18;
            }
            EmbodiedGestureKind::Hold => {
                plan.reason = InteractionReasonCode::QuietAcknowledgement;
                plan.communicative_intent = CommunicativeIntent::AcknowledgeContact;
                expression.eye_aperture = 0.76;
                expression.eye_scale = 0.98;
                expression.mouth_compression = 0.16;
                expression.amplitude = 0.34;
                plan.body.compliance_delta = 0.04;
            }
            EmbodiedGestureKind::PullAndRelease => {
                plan.reason = InteractionReasonCode::PlayfulCooperation;
                plan.communicative_intent = CommunicativeIntent::YieldAndInviteReturn;
                plan.gaze = InteractionGazeTarget::Cursor;
                expression.eye_scale = 1.08;
                expression.mouth_curve = 0.28;
                expression.amplitude = 0.68;
                plan.body.resistance = 0.35;
                plan.body.cooperation = 0.72;
                plan.body.local_pulse = 0.024;
                plan.voice_trigger = Some(VocalTrigger::PlayfulRelease);
                plan.expected_receiver_effect = ReceiverEffect::RepeatMotif;
            }
            EmbodiedGestureKind::FragmentSeparationAttempt => {
                plan.reason = InteractionReasonCode::ComponentDetached;
                plan.communicative_intent = CommunicativeIntent::OrientToFragment;
                plan.gaze = component_gaze;
                expression.eye_scale = 1.06;
                expression.effort = appraisal.effort;
                expression.amplitude = 0.60;
                plan.body.target_component = component_id;
                plan.body.allow_intentional_bud = appraisal.cooperation > 0.62;
                plan.voice_trigger = Some(VocalTrigger::ComponentDetached);
                plan.expected_receiver_effect = ReceiverEffect::HelpFragment;
            }
            EmbodiedGestureKind::FragmentHelp => {
                let reunited = event.frame.remerge_event.is_some();
                plan.reason = if reunited {
                    InteractionReasonCode::SuccessfulReunion
                } else {
                    InteractionReasonCode::ComponentRecovery
                };
                plan.communicative_intent = CommunicativeIntent::AcknowledgeHelp;
                plan.gaze = if reunited {
                    InteractionGazeTarget::MergePoint
                } else {
                    component_gaze
                };
                expression.eye_scale = 1.04;
                expression.mouth_curve = 0.32;
                expression.relief = if reunited { 0.72 } else { 0.0 };
                expression.amplitude = 0.62;
                plan.body.target_component = component_id;
                plan.body.cooperation = 0.85;
                plan.voice_trigger = Some(if reunited {
                    VocalTrigger::ComponentRemerged
                } else {
                    VocalTrigger::FragmentHelped
                });
                plan.expected_receiver_effect = ReceiverEffect::HelpFragment;
            }
            EmbodiedGestureKind::SharedPlayInvitation => {
                plan.reason = InteractionReasonCode::SharedRitual;
                plan.communicative_intent = CommunicativeIntent::InviteRepeat;
                plan.gaze = InteractionGazeTarget::Viewer;
                expression.eye_scale = 1.04;
                expression.mouth_curve = 0.24;
                expression.amplitude = 0.52;
                plan.body.cooperation = 0.72;
                plan.expected_receiver_effect = ReceiverEffect::RepeatMotif;
            }
            EmbodiedGestureKind::Unknown => {
                plan.reason = InteractionReasonCode::CuriousInspection;
                expression.eye_scale = 1.04;
                expression.brow_asymmetry = 0.12;
                expression.amplitude = 0.35;
            }
        }
    }
    let response_scale = response_amplitude.clamp(0.25, 1.25);
    expression.amplitude *= response_scale;
    plan.body.compliance_delta *= response_scale;
    plan.body.cohesion_delta *= response_scale;
    plan.body.local_pulse *= response_scale;
    plan.body.lean *= response_scale;
    plan.body.recoil *= response_scale;
    plan.expression = expression;
    plan.sanitize();
    plan
}

/// Shared physical evidence for the live interoceptive graph and event response.
/// A mouse's force-density proxy may be moderate while a small patch yields
/// safely. Neither elapsed holding nor the categorical gesture grants pleasure.
#[derive(Debug, Clone, Copy)]
pub(crate) struct MaterialCareEvidence {
    pub active: bool,
    pub area: f32,
    pub pressure: f32,
    /// Relative cursor/material speed in body radii per second.
    pub slip_speed: f32,
    pub strain: f32,
    pub neck_tension: f32,
    pub detached_mass: f32,
    pub topology_intact: bool,
}

impl MaterialCareEvidence {
    pub(crate) fn quality(self) -> f32 {
        let observations = [
            self.area,
            self.pressure,
            self.slip_speed,
            self.strain,
            self.neck_tension,
            self.detached_mass,
        ];
        if !self.active
            || !self.topology_intact
            || observations
                .iter()
                .any(|value| !value.is_finite() || *value < 0.0)
        {
            return 0.0;
        }
        let ramp = |lo: f32, hi: f32, value: f32| {
            let t = ((value - lo) / (hi - lo)).clamp(0.0, 1.0);
            t * t * (3.0 - 2.0 * t)
        };
        ramp(0.002, 0.03, self.area)
            * (1.0 - ramp(0.50, 0.75, self.area))
            * ramp(0.005, 0.025, self.pressure)
            * (1.0 - ramp(0.46, 0.72, self.pressure))
            * (1.0 - ramp(0.35, 1.40, self.slip_speed))
            * (1.0 - ramp(0.30, 0.58, self.strain))
            * (1.0 - ramp(0.18, 0.48, self.neck_tension))
            * (1.0 - ramp(0.005, 0.025, self.detached_mass))
    }
}

fn material_care_from_event(event: EmbodiedGestureEvent) -> f32 {
    let contact = event.frame.contact;
    let material = event.frame.material;
    let care = MaterialCareEvidence {
        active: contact.active,
        area: contact.area_fraction,
        pressure: contact.effective_pressure,
        slip_speed: contact.relative_velocity_local.length(),
        strain: material.maximum_strain,
        neck_tension: material.neck_tension,
        detached_mass: material.detached_mass_fraction,
        topology_intact: !material.topology_budget_exhausted && material.component_count == 1,
    }
    .quality();
    // A brief shock may already have entrained its material patch, so check the
    // actual pointer acceleration too rather than relying on slip alone.
    let acceleration = contact.pointer_acceleration;
    if !acceleration.is_finite() || acceleration < 0.0 {
        return 0.0;
    }
    let t = ((acceleration - 2.0) / 6.0).clamp(0.0, 1.0);
    care * (1.0 - t * t * (3.0 - 2.0 * t))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InteractionConsistencyError {
    BoundaryCelebration,
    FatiguedStartle,
    PrematureRelief,
    WrongGazeTarget,
    CooperationResistanceConflict,
    SleepVocalization,
}

pub fn validate_interaction_expression_consistency(
    appraisal: InteractionAppraisal,
    plan: InteractionResponsePlan,
) -> Result<(), InteractionConsistencyError> {
    if plan.reason == InteractionReasonCode::CalmBoundary
        && (plan.expression.relief > 0.05
            || matches!(plan.voice_trigger, Some(VocalTrigger::ComponentRemerged)))
    {
        return Err(InteractionConsistencyError::BoundaryCelebration);
    }
    if appraisal.effort > 0.85
        && plan.reason == InteractionReasonCode::PhysicalStartle
        && plan.expression.amplitude > 0.75
    {
        return Err(InteractionConsistencyError::FatiguedStartle);
    }
    if plan.expression.relief > 0.05
        && plan.reason != InteractionReasonCode::SuccessfulReunion
        && plan.reason != InteractionReasonCode::GentleContact
    {
        return Err(InteractionConsistencyError::PrematureRelief);
    }
    if plan.reason == InteractionReasonCode::ComponentRecovery
        && !matches!(
            plan.gaze,
            InteractionGazeTarget::DetachedComponent(_) | InteractionGazeTarget::MergePoint
        )
    {
        return Err(InteractionConsistencyError::WrongGazeTarget);
    }
    if plan.body.cooperation > 0.8 && plan.body.resistance > 0.35 {
        return Err(InteractionConsistencyError::CooperationResistanceConflict);
    }
    if plan.reason == InteractionReasonCode::QuietAcknowledgement && plan.voice_trigger.is_some() {
        return Err(InteractionConsistencyError::SleepVocalization);
    }
    Ok(())
}

fn finite_vector(value: Vec2) -> Vec2 {
    if value.is_finite() { value } else { Vec2::ZERO }
}

fn unit(value: f32) -> f32 {
    bounded(value, 0.0, 1.0, 0.0)
}

fn finite_non_negative(value: f32, maximum: f32) -> f32 {
    bounded(value, 0.0, maximum, 0.0)
}

fn finite_signed(value: f32, limit: f32) -> f32 {
    bounded(value, -limit, limit, 0.0)
}

fn bounded(value: f32, minimum: f32, maximum: f32, fallback: f32) -> f32 {
    if value.is_finite() {
        value.clamp(minimum, maximum)
    } else {
        fallback
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn yielding_stretch() -> EmbodiedGestureEvent {
        EmbodiedGestureEvent {
            classification: GestureClassification {
                episode_id: 1,
                kind: EmbodiedGestureKind::SlowStretch,
                confidence: 0.9,
                committed: true,
                ..Default::default()
            },
            frame: EmbodiedInteractionFrame {
                contact: PointerMaterialContact {
                    active: true,
                    area_fraction: 0.25,
                    effective_pressure: 0.50,
                    relative_velocity_local: Vec2::new(0.18, 0.0),
                    pointer_acceleration: 0.30,
                    pressure_impulse: 0.12,
                    ..Default::default()
                },
                material: BodyMaterialState {
                    maximum_strain: 0.24,
                    deformation_energy: 0.12,
                    neck_tension: 0.14,
                    ..EmbodiedInteractionFrame::default().material
                },
                ..Default::default()
            },
            boundary: GestureBoundaryEvent::None,
            observation_quality: 1.0,
        }
    }

    fn stretch_response(
        event: EmbodiedGestureEvent,
        stress: f32,
    ) -> (InteractionAppraisal, InteractionResponsePlan) {
        let genome = crate::Genome::from_seed(7);
        let mut drives = Drives::initial(&genome.temperament);
        drives.sleep = 0.10;
        let affect = AffectState {
            stress,
            frustration: 0.0,
            ..Default::default()
        };
        let appraisal = appraise_embodied_gesture(event, drives, affect, &genome.temperament, 0, 0);
        let plan = interaction_response_plan(event, appraisal, 1, 1, 1.0, 1.15, 1.25);
        validate_interaction_expression_consistency(appraisal, plan).unwrap();
        (appraisal, plan)
    }

    #[test]
    fn safe_yielding_stretch_is_care_without_elapsed_permission_or_forced_voice() {
        let brief = yielding_stretch();
        let mut long = brief;
        long.frame.contact.contact_seconds = 60.0;
        long.frame.contact.pressure_impulse = 60.0;
        let (short_appraisal, short_plan) = stretch_response(brief, 0.0);
        let (long_appraisal, long_plan) = stretch_response(long, 0.0);
        for plan in [short_plan, long_plan] {
            assert_eq!(plan.reason, InteractionReasonCode::GentleContact);
            assert_eq!(
                plan.expected_receiver_effect,
                ReceiverEffect::ContinueGently
            );
            assert!(plan.expression.mouth_curve > 0.2);
            assert!(plan.body.compliance_delta > 0.04 && plan.body.resistance < 0.05);
            assert_eq!(plan.voice_trigger, None);
            assert!(!plan.body.allow_intentional_bud);
        }
        assert!(long_appraisal.boundary_need < 0.10);
        assert!((short_appraisal.valence - long_appraisal.valence).abs() < 0.08);
    }

    #[test]
    fn gentle_label_cannot_turn_jerks_overstrain_fragments_or_stress_into_care() {
        let safe = yielding_stretch();
        let mut jerk = safe;
        jerk.frame.contact.pointer_acceleration = 12.0;
        let mut slip = safe;
        slip.frame.contact.relative_velocity_local = Vec2::new(2.0, 0.0);
        let mut overloaded = safe;
        overloaded.frame.material.maximum_strain = 0.92;
        overloaded.frame.contact.effective_pressure = 0.96;
        let mut necked = safe;
        necked.frame.material.neck_tension = 0.70;
        let mut fragmented = safe;
        fragmented.frame.material.component_count = 2;
        fragmented.frame.material.detached_mass_fraction = 0.15;
        let mut whole_body = safe;
        whole_body.frame.contact.area_fraction = 0.90;
        let mut absent = safe;
        absent.frame.contact.active = false;
        for event in [
            jerk, slip, overloaded, necked, fragmented, whole_body, absent,
        ] {
            let (_, plan) = stretch_response(event, 0.0);
            assert_ne!(
                plan.reason,
                InteractionReasonCode::GentleContact,
                "{event:?}"
            );
            assert_eq!(plan.expression.relief, 0.0);
        }
        let (_, stressed) = stretch_response(safe, 1.0);
        assert_ne!(stressed.reason, InteractionReasonCode::GentleContact);
        assert_eq!(stressed.expression.relief, 0.0);
    }

    #[test]
    fn physical_contact_validation_allows_normal_float_rounding() {
        let mut contact = PointerMaterialContact {
            active: true,
            point_world: Vec2::splat(0.5),
            normal_local: Vec2::new(0.872_408_1, 0.488_778_38),
            area_fraction: 0.57,
            effective_pressure: 0.78,
            pointer_speed: 0.02,
            pointer_acceleration: 0.09,
            contact_seconds: 0.06,
            pressure_impulse: 0.04,
            ..PointerMaterialContact::default()
        };
        contact.sanitize();
        assert!(contact.is_valid());
        assert!(
            EmbodiedInteractionFrame {
                contact,
                ..EmbodiedInteractionFrame::default()
            }
            .is_valid()
        );
    }

    #[test]
    fn embodied_frame_sanitizes_privacy_safe_bounded_values() {
        let mut frame = EmbodiedInteractionFrame {
            timestamp: f64::NAN,
            contact: PointerMaterialContact {
                active: true,
                point_world: Vec2::new(-4.0, 7.0),
                effective_pressure: f32::INFINITY,
                ..PointerMaterialContact::default()
            },
            component_observation_count: u8::MAX,
            ..EmbodiedInteractionFrame::default()
        };
        frame.sanitize();
        assert_eq!(frame.timestamp, 0.0);
        assert_eq!(frame.contact.point_world, Vec2::new(0.0, 1.0));
        assert_eq!(frame.contact.effective_pressure, 0.0);
        assert_eq!(
            frame.component_observation_count,
            MAX_TRACKED_BODY_COMPONENTS as u8
        );
    }

    #[test]
    fn interaction_appraisal_sanitization_is_idempotent() {
        let mut appraisal = InteractionAppraisal {
            boundary_need: 0.6,
            cooperation: 0.8,
            ..InteractionAppraisal::default()
        };
        appraisal.sanitize();
        let once = appraisal;
        appraisal.sanitize();
        assert_eq!(appraisal, once);
        assert!(appraisal.is_valid());
        assert!(appraisal.cooperation <= 1.0 - appraisal.boundary_need);
    }

    #[test]
    fn one_episode_emits_at_most_one_response() {
        let mut turn = InteractionTurnRuntime::default();
        assert!(turn.begin_episode(7));
        assert!(turn.emit_response(7, 11));
        assert!(!turn.emit_response(7, 12));
        assert_eq!(turn.response_id, Some(11));
    }
}
