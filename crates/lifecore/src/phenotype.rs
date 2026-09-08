//! Typed contracts for the R12 brain/body nervous-system map.
//!
//! These are deliberately parameter values, never solver configuration. The
//! authoritative body publishes one immutable [`BodyFeedbackV2`] and consumes
//! one bounded [`FastPhenotypeActuation`] on the following cognition tick.

use glam::{Vec2, Vec3};
use serde::{Deserialize, Serialize};

use crate::{
    AffectState, AppraisalState, BodyIntent, Drives, ExpressionState, InteractionBodyActuation,
    MoodState, PoseIntent, SensorFrame, TemperamentGenome, VoiceGenome,
};

pub const NERVOUS_SYSTEM_SCHEMA_VERSION: u32 = 1;
pub const EMOTION_READOUT_COUNT: usize = 22;
pub const MORPH_POPULATION_COUNT: usize = 15;
pub const MORPH_COMMAND_COUNT: usize = 16;
pub const MORPH_COMMAND_NAMES: [&str; MORPH_COMMAND_COUNT] = [
    "C_FLEE",
    "C_APPR",
    "C_TURN_L",
    "C_TURN_R",
    "C_PERK",
    "C_MELT",
    "C_GROOM",
    "C_PLAY",
    "C_SAMPLE",
    "C_PUSH",
    "C_TOUCH",
    "C_PULL",
    "C_LISTEN",
    "C_SNIFF",
    "C_GRASP",
    "C_RELEASE",
];

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BodyContactFeedbackV2 {
    pub component_id: Option<u8>,
    pub point_world: Vec2,
    pub point_local: Vec2,
    pub normal: Vec2,
    pub area: f32,
    pub pressure: f32,
    pub tangential_speed: f32,
    pub duration: f32,
    pub contact_count: u16,
    pub user_force_estimate: Vec2,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BodyShapeFeedbackV2 {
    pub body_area_ratio: f32,
    pub body_length_ratio: f32,
    pub body_width_ratio: f32,
    pub roundness: f32,
    pub deformation_energy: f32,
    pub maximum_strain: f32,
    pub neck_tension: f32,
    pub center_of_mass_offset: Vec2,
    pub orientation_radians: f32,
    pub angular_velocity: f32,
}

impl Default for BodyShapeFeedbackV2 {
    fn default() -> Self {
        Self {
            body_area_ratio: 1.0,
            body_length_ratio: 1.0,
            body_width_ratio: 1.0,
            roundness: 0.5,
            deformation_energy: 0.0,
            maximum_strain: 0.0,
            neck_tension: 0.0,
            center_of_mass_offset: Vec2::ZERO,
            orientation_radians: 0.0,
            angular_velocity: 0.0,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BodyFluidFeedbackV2 {
    pub slosh_energy: f32,
    pub internal_relative_speed: f32,
    pub pressure_variance: f32,
    pub settle_error: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BodyTopologyFeedbackV2 {
    pub connected_components: u16,
    pub detached_mass_fraction: f32,
    pub largest_fragment_fraction: f32,
    pub budget_remaining: f32,
    pub merge_progress: f32,
    pub recovery_active: bool,
}

impl Default for BodyTopologyFeedbackV2 {
    fn default() -> Self {
        Self {
            connected_components: 1,
            detached_mass_fraction: 0.0,
            largest_fragment_fraction: 0.0,
            budget_remaining: 1.0,
            merge_progress: 0.0,
            recovery_active: false,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BodyMotionFeedbackV2 {
    pub world_position: Vec2,
    pub velocity: Vec2,
    pub acceleration: Vec2,
    pub jerk: Vec2,
    pub grounded: bool,
    pub clinging: bool,
    pub collision_impulse: f32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EfferenceCopyV2 {
    pub intended_velocity: Vec2,
    pub intended_turn: f32,
    pub intended_shape_delta: Vec3,
    pub actual_velocity: Vec2,
    pub actual_turn: f32,
    pub actual_shape_delta: Vec3,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BodyEnvironmentFeedbackV2 {
    pub distance_to_screen_edge: f32,
    pub clipped_fraction: f32,
    pub available_motion_radius: f32,
    pub cursor_distance: f32,
    pub cursor_loom_rate: f32,
    pub user_present: bool,
    pub seconds_since_interaction: f32,
}

impl Default for BodyEnvironmentFeedbackV2 {
    fn default() -> Self {
        Self {
            distance_to_screen_edge: 1.0,
            clipped_fraction: 0.0,
            available_motion_radius: 1.0,
            cursor_distance: 1.0,
            cursor_loom_rate: 0.0,
            user_present: false,
            seconds_since_interaction: 0.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BodyFeedbackV2 {
    pub schema_version: u32,
    pub frame_id: u64,
    pub contact: BodyContactFeedbackV2,
    pub shape: BodyShapeFeedbackV2,
    pub fluid: BodyFluidFeedbackV2,
    pub topology: BodyTopologyFeedbackV2,
    pub motion: BodyMotionFeedbackV2,
    pub efference_copy: EfferenceCopyV2,
    pub environment: BodyEnvironmentFeedbackV2,
}

impl Default for BodyFeedbackV2 {
    fn default() -> Self {
        Self {
            schema_version: NERVOUS_SYSTEM_SCHEMA_VERSION,
            frame_id: 0,
            contact: BodyContactFeedbackV2::default(),
            shape: BodyShapeFeedbackV2::default(),
            fluid: BodyFluidFeedbackV2::default(),
            topology: BodyTopologyFeedbackV2::default(),
            motion: BodyMotionFeedbackV2::default(),
            efference_copy: EfferenceCopyV2::default(),
            environment: BodyEnvironmentFeedbackV2::default(),
        }
    }
}

impl BodyFeedbackV2 {
    pub fn sanitize(&mut self) {
        self.schema_version = NERVOUS_SYSTEM_SCHEMA_VERSION;
        self.contact.point_world =
            finite_vec2(self.contact.point_world).clamp(Vec2::ZERO, Vec2::ONE);
        self.contact.point_local = finite_vec2(self.contact.point_local).clamp_length_max(8.0);
        self.contact.normal = finite_vec2(self.contact.normal).normalize_or_zero();
        self.contact.area = unit(self.contact.area);
        self.contact.pressure = unit(self.contact.pressure);
        self.contact.tangential_speed = unit(self.contact.tangential_speed);
        self.contact.duration = finite(self.contact.duration, 0.0).clamp(0.0, 60.0);
        self.contact.user_force_estimate =
            finite_vec2(self.contact.user_force_estimate).clamp_length_max(1.0);
        self.shape.body_area_ratio = finite(self.shape.body_area_ratio, 1.0).clamp(0.5, 1.5);
        self.shape.body_length_ratio = finite(self.shape.body_length_ratio, 1.0).clamp(0.5, 1.8);
        self.shape.body_width_ratio = finite(self.shape.body_width_ratio, 1.0).clamp(0.5, 1.8);
        self.shape.roundness = unit(self.shape.roundness);
        self.shape.deformation_energy = unit(self.shape.deformation_energy);
        self.shape.maximum_strain = unit(self.shape.maximum_strain);
        self.shape.neck_tension = unit(self.shape.neck_tension);
        self.shape.center_of_mass_offset =
            finite_vec2(self.shape.center_of_mass_offset).clamp_length_max(2.0);
        self.shape.orientation_radians = finite(self.shape.orientation_radians, 0.0);
        self.shape.angular_velocity = bounded_signed(self.shape.angular_velocity);
        self.fluid.slosh_energy = unit(self.fluid.slosh_energy);
        self.fluid.internal_relative_speed = unit(self.fluid.internal_relative_speed);
        self.fluid.pressure_variance = unit(self.fluid.pressure_variance);
        self.fluid.settle_error = unit(self.fluid.settle_error);
        self.topology.connected_components = self.topology.connected_components.clamp(1, 4);
        self.topology.detached_mass_fraction = unit(self.topology.detached_mass_fraction);
        self.topology.largest_fragment_fraction = unit(self.topology.largest_fragment_fraction);
        self.topology.budget_remaining = unit(self.topology.budget_remaining);
        self.topology.merge_progress = unit(self.topology.merge_progress);
        self.motion.world_position =
            finite_vec2(self.motion.world_position).clamp(Vec2::ZERO, Vec2::ONE);
        self.motion.velocity = finite_vec2(self.motion.velocity).clamp_length_max(1.0);
        self.motion.acceleration = finite_vec2(self.motion.acceleration).clamp_length_max(1.0);
        self.motion.jerk = finite_vec2(self.motion.jerk).clamp_length_max(1.0);
        self.motion.collision_impulse = unit(self.motion.collision_impulse);
        self.efference_copy.intended_velocity =
            finite_vec2(self.efference_copy.intended_velocity).clamp_length_max(1.0);
        self.efference_copy.intended_turn = bounded_signed(self.efference_copy.intended_turn);
        self.efference_copy.intended_shape_delta =
            finite_vec3(self.efference_copy.intended_shape_delta)
                .clamp(Vec3::splat(-1.0), Vec3::ONE);
        self.efference_copy.actual_velocity =
            finite_vec2(self.efference_copy.actual_velocity).clamp_length_max(1.0);
        self.efference_copy.actual_turn = bounded_signed(self.efference_copy.actual_turn);
        self.efference_copy.actual_shape_delta =
            finite_vec3(self.efference_copy.actual_shape_delta).clamp(Vec3::splat(-1.0), Vec3::ONE);
        self.environment.distance_to_screen_edge = unit(self.environment.distance_to_screen_edge);
        self.environment.clipped_fraction = unit(self.environment.clipped_fraction);
        self.environment.available_motion_radius = unit(self.environment.available_motion_radius);
        self.environment.cursor_distance = unit(self.environment.cursor_distance);
        self.environment.cursor_loom_rate = unit(self.environment.cursor_loom_rate);
        self.environment.seconds_since_interaction =
            finite(self.environment.seconds_since_interaction, 0.0).clamp(0.0, 86_400.0);
    }

    #[must_use]
    pub fn is_valid(mut self) -> bool {
        let original = self;
        self.sanitize();
        self == original
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct VoiceFeedbackV1 {
    pub envelope: f32,
    pub phonating: bool,
    pub exhale_phase: f32,
    pub spectral_flux: f32,
    pub actual_pitch_hz: f32,
    pub actual_loudness: f32,
}

impl VoiceFeedbackV1 {
    pub fn sanitize(&mut self, maximum_loudness: f32) {
        self.envelope = unit(self.envelope);
        self.exhale_phase = unit(self.exhale_phase);
        self.spectral_flux = unit(self.spectral_flux);
        self.actual_pitch_hz = finite(self.actual_pitch_hz, 0.0).clamp(0.0, 8_000.0);
        self.actual_loudness =
            finite(self.actual_loudness, 0.0).clamp(0.0, finite(maximum_loudness, 0.0).max(0.0));
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FeltStateV1 {
    pub physical_load: f32,
    pub pain_like: f32,
    pub comfort: f32,
    pub body_integrity: f32,
    pub body_ownership: f32,
    pub agency_match: f32,
    pub motor_efficacy: f32,
    pub restraint: f32,
    pub balance: f32,
    pub activation: f32,
    pub social_safety: f32,
    pub contact_pleasantness: f32,
    pub vulnerability: f32,
    pub effort: f32,
    pub surprise: f32,
    pub startle: f32,
    pub relief: f32,
    pub boredom: f32,
    pub loneliness: f32,
    pub play_readiness: f32,
    pub exploration_readiness: f32,
    pub sleep_pressure: f32,
}

impl FeltStateV1 {
    pub fn sanitize(&mut self) {
        for value in self.values_mut() {
            *value = unit(*value);
        }
    }

    #[must_use]
    pub fn is_valid(self) -> bool {
        self.values()
            .into_iter()
            .all(|value| (0.0..=1.0).contains(&value))
    }

    #[must_use]
    pub fn values(self) -> [f32; 22] {
        [
            self.physical_load,
            self.pain_like,
            self.comfort,
            self.body_integrity,
            self.body_ownership,
            self.agency_match,
            self.motor_efficacy,
            self.restraint,
            self.balance,
            self.activation,
            self.social_safety,
            self.contact_pleasantness,
            self.vulnerability,
            self.effort,
            self.surprise,
            self.startle,
            self.relief,
            self.boredom,
            self.loneliness,
            self.play_readiness,
            self.exploration_readiness,
            self.sleep_pressure,
        ]
    }

    fn values_mut(&mut self) -> [&mut f32; 22] {
        [
            &mut self.physical_load,
            &mut self.pain_like,
            &mut self.comfort,
            &mut self.body_integrity,
            &mut self.body_ownership,
            &mut self.agency_match,
            &mut self.motor_efficacy,
            &mut self.restraint,
            &mut self.balance,
            &mut self.activation,
            &mut self.social_safety,
            &mut self.contact_pleasantness,
            &mut self.vulnerability,
            &mut self.effort,
            &mut self.surprise,
            &mut self.startle,
            &mut self.relief,
            &mut self.boredom,
            &mut self.loneliness,
            &mut self.play_readiness,
            &mut self.exploration_readiness,
            &mut self.sleep_pressure,
        ]
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MorphPopulationFrame {
    pub exp: f32,
    pub prox: f32,
    pub mot: f32,
    pub tch: f32,
    pub vib: f32,
    pub loom: f32,
    pub hab: f32,
    pub nov: f32,
    pub kc: f32,
    pub valp: f32,
    pub valn: f32,
    pub mbon_a: f32,
    pub mbon_v: f32,
    pub att: f32,
    pub rest: f32,
}

impl MorphPopulationFrame {
    #[must_use]
    pub fn values(self) -> [f32; MORPH_POPULATION_COUNT] {
        [
            self.exp,
            self.prox,
            self.mot,
            self.tch,
            self.vib,
            self.loom,
            self.hab,
            self.nov,
            self.kc,
            self.valp,
            self.valn,
            self.mbon_a,
            self.mbon_v,
            self.att,
            self.rest,
        ]
    }

    #[must_use]
    pub fn from_values(v: [f32; MORPH_POPULATION_COUNT]) -> Self {
        Self {
            exp: v[0],
            prox: v[1],
            mot: v[2],
            tch: v[3],
            vib: v[4],
            loom: v[5],
            hab: v[6],
            nov: v[7],
            kc: v[8],
            valp: v[9],
            valn: v[10],
            mbon_a: v[11],
            mbon_v: v[12],
            att: v[13],
            rest: v[14],
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MorphNervousSystemFrame {
    pub populations: MorphPopulationFrame,
    pub winner_rate: f32,
    pub confidence: f32,
    pub valence: f32,
    pub arousal: f32,
    pub conflict: f32,
    pub turn: f32,
    pub command_rates: [f32; MORPH_COMMAND_COUNT],
}

impl MorphNervousSystemFrame {
    #[must_use]
    pub fn command_rate(self, wire_name: &str) -> Option<f32> {
        MORPH_COMMAND_NAMES
            .iter()
            .position(|name| *name == wire_name)
            .map(|index| unit(self.command_rates[index] / 45.0))
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct VitaSomaticFrame {
    pub appraisal: AppraisalState,
    pub mood: MoodState,
    pub prediction_error: f32,
    pub agency: f32,
    pub uncertainty: f32,
    pub body_schema_confidence: f32,
    pub calibration_urge: f32,
    pub external_force_likelihood: f32,
    pub attention_confidence: f32,
    pub attention_commitment_remaining: f32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GestureFrameV1 {
    pub episode_id: u64,
    pub confidence: f32,
    pub ambiguity_margin: f32,
    pub novelty: f32,
    pub repetition_similarity: f32,
    pub rhythm_phase: Option<f32>,
    pub rhythm_strength: f32,
    pub boundary_violation: f32,
    pub target_component: Option<u8>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EpisodeContextV1 {
    pub episode_id: u64,
    pub open: bool,
    pub closed: bool,
    pub reward_positive: f32,
    pub reward_negative: f32,
    pub successful_play: f32,
    pub successful_exploration: f32,
    pub goal_congruent_motor_success: f32,
    pub rhythmic_synchrony: f32,
    pub safe_social_exchange: f32,
    pub safe_predictable_episode: f32,
    pub self_initiated_success: f32,
    pub ignored_social_bid: f32,
    pub boundary_violation: f32,
    pub repeated_intentional_failure: f32,
    pub novel_goal_congruent_episode: f32,
    pub sleeping_or_deep_rest: f32,
    pub rest_quality: f32,
    pub user_absent: f32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PerceptionSelectionV1 {
    pub selected_salience: f32,
    pub selected_object_slot: Option<u8>,
}

/// Slow, bounded evidence accumulated from closed embodied episodes. These
/// channels are phenotype-development evidence, not fast expression controls
/// and never contain identity seeds or solver parameters.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DevelopmentalEvidence {
    pub flight_mastery: f32,
    pub social_security: f32,
    pub exploration_mastery: f32,
    pub rest_adaptation: f32,
    pub physical_resilience: f32,
    pub communication_mastery: f32,
}

impl DevelopmentalEvidence {
    pub fn sanitize(&mut self) {
        self.flight_mastery = unit(self.flight_mastery);
        self.social_security = unit(self.social_security);
        self.exploration_mastery = unit(self.exploration_mastery);
        self.rest_adaptation = unit(self.rest_adaptation);
        self.physical_resilience = unit(self.physical_resilience);
        self.communication_mastery = unit(self.communication_mastery);
    }

    #[must_use]
    pub fn is_valid(self) -> bool {
        [
            self.flight_mastery,
            self.social_security,
            self.exploration_mastery,
            self.rest_adaptation,
            self.physical_resilience,
            self.communication_mastery,
        ]
        .into_iter()
        .all(|value| (0.0..=1.0).contains(&value))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EmbodimentSourceFrame {
    pub frame_id: u64,
    pub affect: AffectState,
    pub drives: Drives,
    pub temperament: TemperamentGenome,
    pub voice_seed: u64,
    pub vita: VitaSomaticFrame,
    pub morph: MorphNervousSystemFrame,
    pub body: BodyFeedbackV2,
    pub voice_feedback: VoiceFeedbackV1,
    pub gesture: GestureFrameV1,
    pub episode: EpisodeContextV1,
    pub perception: PerceptionSelectionV1,
    pub soft_touch_pressure_max: f32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DerivedNervousState {
    pub positive_valence: f32,
    pub negative_valence: f32,
    pub arousal: f32,
    pub stress: f32,
    pub fatigue: f32,
    pub confidence: f32,
    pub curiosity: f32,
    pub social_warmth: f32,
    pub neural_approach: f32,
    pub neural_threat: f32,
    pub neural_novelty: f32,
    pub neural_rest: f32,
    pub attention_strength: f32,
    pub attention_commitment: f32,
    pub self_uncertainty: f32,
    pub habituation: f32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EmotionReadouts {
    pub calm: f32,
    pub contentment: f32,
    pub joy: f32,
    pub interest: f32,
    pub playfulness: f32,
    pub affection: f32,
    pub anticipation: f32,
    pub surprise: f32,
    pub startle: f32,
    pub fear: f32,
    pub anxiety: f32,
    pub protest: f32,
    pub sadness: f32,
    pub frustration: f32,
    pub confusion: f32,
    pub discomfort: f32,
    pub relief: f32,
    pub boredom: f32,
    pub loneliness: f32,
    pub determination: f32,
    pub exhaustion: f32,
    pub vulnerability: f32,
}

impl EmotionReadouts {
    #[must_use]
    pub fn values(self) -> [f32; EMOTION_READOUT_COUNT] {
        [
            self.calm,
            self.contentment,
            self.joy,
            self.interest,
            self.playfulness,
            self.affection,
            self.anticipation,
            self.surprise,
            self.startle,
            self.fear,
            self.anxiety,
            self.protest,
            self.sadness,
            self.frustration,
            self.confusion,
            self.discomfort,
            self.relief,
            self.boredom,
            self.loneliness,
            self.determination,
            self.exhaustion,
            self.vulnerability,
        ]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AnalyticRuntimeActuation {
    pub body_length_scale: f32,
    pub body_width_scale: f32,
    pub roundness_bias: f32,
    pub softness_bias: f32,
    pub modal_response_multiplier: f32,
    pub modal_frequency_multiplier: f32,
    pub modal_damping_multiplier: f32,
    pub modal_amplitude_multiplier: f32,
}

impl Default for AnalyticRuntimeActuation {
    fn default() -> Self {
        Self {
            body_length_scale: 1.0,
            body_width_scale: 1.0,
            roundness_bias: 0.0,
            softness_bias: 0.0,
            modal_response_multiplier: 1.0,
            modal_frequency_multiplier: 1.0,
            modal_damping_multiplier: 1.0,
            modal_amplitude_multiplier: 1.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PbfRuntimeActuation {
    pub density_compliance_multiplier: f32,
    pub viscosity_multiplier: f32,
    pub surface_tension_multiplier: f32,
    pub flight_inertia_multiplier: f32,
    pub flight_stretch_multiplier: f32,
    pub flight_damping_multiplier: f32,
    pub flight_max_lag_multiplier: f32,
    pub angular_damping_multiplier: f32,
    pub motor_gain_multiplier: f32,
    pub shape_recovery_delta: f32,
    pub upright_stabilization_delta: f32,
    pub idle_breath_amplitude_multiplier: f32,
    pub idle_breath_speed_multiplier: f32,
    pub idle_lean_angle_multiplier: f32,
    pub idle_lean_rate_multiplier: f32,
}

impl Default for PbfRuntimeActuation {
    fn default() -> Self {
        Self {
            density_compliance_multiplier: 1.0,
            viscosity_multiplier: 1.0,
            surface_tension_multiplier: 1.0,
            flight_inertia_multiplier: 1.0,
            flight_stretch_multiplier: 1.0,
            flight_damping_multiplier: 1.0,
            flight_max_lag_multiplier: 1.0,
            angular_damping_multiplier: 1.0,
            motor_gain_multiplier: 1.0,
            shape_recovery_delta: 0.0,
            upright_stabilization_delta: 0.0,
            idle_breath_amplitude_multiplier: 1.0,
            idle_breath_speed_multiplier: 1.0,
            idle_lean_angle_multiplier: 1.0,
            idle_lean_rate_multiplier: 1.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MaterialRuntimeActuation {
    pub hue_shift_turns: f32,
    pub saturation_delta: f32,
    pub value_delta: f32,
    pub emission_multiplier: f32,
    pub soul_glow_strength_multiplier: f32,
    pub soul_glow_pulse_multiplier: f32,
    pub opacity_multiplier: f32,
    pub translucency_delta: f32,
    pub internal_flow_multiplier: f32,
    pub caustic_speed_multiplier: f32,
    pub halo_multiplier: f32,
    pub bloom_multiplier: f32,
    pub internal_orb_speed_multiplier: f32,
    pub internal_orb_intensity_multiplier: f32,
}

impl Default for MaterialRuntimeActuation {
    fn default() -> Self {
        Self {
            hue_shift_turns: 0.0,
            saturation_delta: 0.0,
            value_delta: 0.0,
            emission_multiplier: 1.0,
            soul_glow_strength_multiplier: 1.0,
            soul_glow_pulse_multiplier: 1.0,
            opacity_multiplier: 1.0,
            translucency_delta: 0.0,
            internal_flow_multiplier: 1.0,
            caustic_speed_multiplier: 1.0,
            halo_multiplier: 1.0,
            bloom_multiplier: 1.0,
            internal_orb_speed_multiplier: 1.0,
            internal_orb_intensity_multiplier: 1.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FaceRuntimeActuation {
    pub gaze_target: Option<Vec2>,
    pub microsaccade_amount_multiplier: f32,
    pub microsaccade_rate_multiplier: f32,
    pub semantic_roll: f32,
    pub translation_offset: Vec2,
    pub scale_multiplier: f32,
    pub blink_rate_multiplier: f32,
}

impl Default for FaceRuntimeActuation {
    fn default() -> Self {
        Self {
            gaze_target: None,
            microsaccade_amount_multiplier: 1.0,
            microsaccade_rate_multiplier: 1.0,
            semantic_roll: 0.0,
            translation_offset: Vec2::ZERO,
            scale_multiplier: 1.0,
            blink_rate_multiplier: 1.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct VisualPhysiologyActuation {
    pub pulse_amplitude: f32,
    pub inner_density_multiplier: f32,
    pub shell_opacity_multiplier: f32,
    pub translucency_delta: f32,
    pub core_glow_multiplier: f32,
    pub halo_multiplier: f32,
    pub iris_activity: f32,
    pub eye_wetness: f32,
    pub flow_strength_multiplier: f32,
    pub flow_speed_multiplier: f32,
    pub droplet_energy: f32,
    pub droplet_spread: f32,
    pub droplet_cohesion: f32,
}

impl Default for VisualPhysiologyActuation {
    fn default() -> Self {
        Self {
            pulse_amplitude: 0.16,
            inner_density_multiplier: 1.0,
            shell_opacity_multiplier: 1.0,
            translucency_delta: 0.0,
            core_glow_multiplier: 1.0,
            halo_multiplier: 1.0,
            iris_activity: 0.3,
            eye_wetness: 0.42,
            flow_strength_multiplier: 1.0,
            flow_speed_multiplier: 1.0,
            droplet_energy: 0.2,
            droplet_spread: 0.16,
            droplet_cohesion: 0.55,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PhraseContourWeights {
    pub joy_rise: f32,
    pub sadness_fall: f32,
    pub curiosity_question: f32,
    pub protest_firm: f32,
    pub calm_level: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct VoicePhenotypeActuation {
    pub enabled: bool,
    pub pitch_multiplier: f32,
    pub pitch_variation_multiplier: f32,
    pub formant_scale_multiplier: f32,
    pub breathiness_delta: f32,
    pub roughness_delta: f32,
    pub brightness_delta: f32,
    pub phrase_speed_multiplier: f32,
    pub attack_multiplier: f32,
    pub release_multiplier: f32,
    pub loudness_multiplier: f32,
    pub purr_amount: f32,
    pub trill_amount: f32,
    pub call_probability: f32,
    pub phrase_contour: PhraseContourWeights,
    pub breath_phase_lock: f32,
    pub effort_noise: f32,
}

impl Default for VoicePhenotypeActuation {
    fn default() -> Self {
        Self {
            enabled: false,
            pitch_multiplier: 1.0,
            pitch_variation_multiplier: 1.0,
            formant_scale_multiplier: 1.0,
            breathiness_delta: 0.0,
            roughness_delta: 0.0,
            brightness_delta: 0.0,
            phrase_speed_multiplier: 1.0,
            attack_multiplier: 1.0,
            release_multiplier: 1.0,
            loudness_multiplier: 1.0,
            purr_amount: 0.0,
            trill_amount: 0.0,
            call_probability: 0.0,
            phrase_contour: PhraseContourWeights::default(),
            breath_phase_lock: 0.0,
            effort_noise: 0.0,
        }
    }
}

impl VoicePhenotypeActuation {
    pub fn apply_to_request(self, request: &mut crate::VocalRequest, genome: &VoiceGenome) {
        // `enabled` is the spontaneous-call gate. Once another behavior owner
        // has already emitted a request, the continuous voice phenotype still
        // shapes the heard call; neutral/default values remain a strict no-op.
        request.pitch_scale = (request.pitch_scale * self.pitch_multiplier).clamp(0.62, 1.48);
        request.tempo_scale =
            (request.tempo_scale * self.phrase_speed_multiplier).clamp(0.50, 1.80);
        request.gain =
            (request.gain * self.loudness_multiplier).clamp(0.0, genome.maximum_loudness);
        request.stress = unit(request.stress + self.effort_noise * 0.35);
        request.purr |= self.purr_amount >= 0.55;
        request.phenotype = self;
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BodyActionIntent {
    pub approach: f32,
    pub avoid: f32,
    pub speed: f32,
    pub turn: f32,
    pub gaze_commitment: f32,
    pub explore: f32,
    pub play: f32,
    pub settle: f32,
    pub protect: f32,
    pub social_approach: f32,
    pub resist: f32,
    pub cooperate: f32,
    pub calibrate_body: f32,
    pub intentional_bud_request: f32,
    pub vocalize: f32,
}

impl BodyActionIntent {
    pub fn apply(self, intent: &mut BodyIntent, sensors: &SensorFrame, body_position: Vec2) {
        let directional = self.approach - self.avoid;
        if directional > 0.12 {
            intent.target_position = intent.target_position.lerp(
                sensors.cursor_position,
                (directional * 0.22).clamp(0.0, 0.22),
            );
        } else if directional < -0.12 {
            let away = (body_position - sensors.cursor_position).normalize_or_zero();
            let target = (body_position + away * 0.22).clamp(Vec2::ZERO, Vec2::ONE);
            intent.target_position = intent
                .target_position
                .lerp(target, (-directional * 0.28).clamp(0.0, 0.28));
        }
        intent.desired_speed = (intent.desired_speed * (0.55 + self.speed * 0.90)).clamp(0.02, 1.0);
        intent.facing_direction = bounded_signed(intent.facing_direction + self.turn * 0.28);
        if self.protect > 0.58 || self.resist > 0.62 {
            intent.pose = PoseIntent::Compact;
        } else if self.play > 0.62 {
            intent.pose = PoseIntent::Playful;
        } else if self.calibrate_body > 0.58 || self.explore > 0.62 {
            intent.pose = PoseIntent::Curious;
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CausalTargetRecord {
    pub frame_id: u64,
    pub episode_id: u64,
    pub target_path: String,
    pub coupling_id: String,
    pub formula: String,
    pub source_terms: Vec<CausalSourceTerm>,
    pub identity_or_profile_base: f32,
    pub raw_target: f32,
    pub influence_budget_scale: f32,
    pub clamp_min: f32,
    pub clamp_max: f32,
    pub clamp_reason: String,
    pub filtered_value: f32,
    pub effective_value: f32,
    pub persistence_class: String,
    pub rise_tau_seconds: f32,
    pub fall_tau_seconds: f32,
    pub component_id_if_local: Option<u8>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CausalSourceTerm {
    pub path: String,
    pub value: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FastPhenotypeActuation {
    pub schema_version: u32,
    pub frame_id: u64,
    pub episode_id: u64,
    pub analytic: AnalyticRuntimeActuation,
    pub pbf: PbfRuntimeActuation,
    pub material: MaterialRuntimeActuation,
    pub face: FaceRuntimeActuation,
    pub visual_physiology: VisualPhysiologyActuation,
    pub voice: VoicePhenotypeActuation,
    pub action: BodyActionIntent,
    pub interaction: InteractionBodyActuation,
    pub expression: ExpressionState,
    pub apparent_scale: f32,
    pub trace: Vec<CausalTargetRecord>,
}

impl Default for FastPhenotypeActuation {
    fn default() -> Self {
        Self {
            schema_version: NERVOUS_SYSTEM_SCHEMA_VERSION,
            frame_id: 0,
            episode_id: 0,
            analytic: AnalyticRuntimeActuation::default(),
            pbf: PbfRuntimeActuation::default(),
            material: MaterialRuntimeActuation::default(),
            face: FaceRuntimeActuation::default(),
            visual_physiology: VisualPhysiologyActuation::default(),
            voice: VoicePhenotypeActuation::default(),
            action: BodyActionIntent::default(),
            interaction: InteractionBodyActuation::default(),
            expression: ExpressionState::default(),
            apparent_scale: 1.0,
            trace: Vec::new(),
        }
    }
}

impl FastPhenotypeActuation {
    pub fn apply_to_intent(
        &self,
        intent: &mut BodyIntent,
        sensors: &SensorFrame,
        body_position: Vec2,
    ) {
        self.action.apply(intent, sensors, body_position);
        if let Some(gaze) = self.face.gaze_target {
            intent.gaze_target = Some(gaze.clamp(Vec2::ZERO, Vec2::ONE));
        }
        // Callback audio and blink scheduler remain authoritative for mouth_open and blinks.
        let mouth_open = intent.expression.mouth_open;
        let blink_left = intent.expression.blink_left;
        let blink_right = intent.expression.blink_right;
        intent.expression = self.expression;
        intent.expression.mouth_open = mouth_open;
        intent.expression.blink_left = blink_left;
        intent.expression.blink_right = blink_right;
    }

    #[must_use]
    pub fn is_finite(&self) -> bool {
        self.trace.iter().all(|r| {
            r.identity_or_profile_base.is_finite()
                && r.raw_target.is_finite()
                && r.filtered_value.is_finite()
                && r.effective_value.is_finite()
        })
    }
}

pub(crate) fn asymmetric(current: f32, target: f32, rise: f32, fall: f32, dt: f32) -> f32 {
    let tau = if target > current { rise } else { fall }.max(1.0e-4);
    current + (target - current) * (1.0 - (-finite(dt, 0.0).clamp(0.0, 0.25) / tau).exp())
}

pub(crate) fn unit(value: f32) -> f32 {
    finite(value, 0.0).clamp(0.0, 1.0)
}
pub(crate) fn bounded_signed(value: f32) -> f32 {
    finite(value, 0.0).clamp(-1.0, 1.0)
}
pub(crate) fn finite(value: f32, fallback: f32) -> f32 {
    if value.is_finite() { value } else { fallback }
}
fn finite_vec2(value: Vec2) -> Vec2 {
    if value.is_finite() { value } else { Vec2::ZERO }
}
fn finite_vec3(value: Vec3) -> Vec3 {
    if value.is_finite() { value } else { Vec3::ZERO }
}
