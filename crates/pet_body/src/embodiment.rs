use glam::Vec2;
use lifecore::{
    AffectState, BodyFeedback, BodyGenome, BodyIntent, FaceRuntimeActuation, InteractionTarget,
    PoseIntent, SensorFrame, VisualPhysiologyActuation,
};
use serde::{Deserialize, Serialize};

use crate::{
    DerivedVisualTraits, DropletMotion, DropletRuntime, FaceTuning, LiquidMorphRuntime,
    ModalDeformation, ModalDynamics, VisualMindInput, VisualPhysiologyRuntime,
};

const LUMINANCE_HISTORY: usize = 32;
const MAX_GAZE_STEP: f32 = 0.08;
const LOCKED_MICROSACCADE_LIMIT: f32 = 0.018;

#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct LuminanceSample {
    time: f32,
    value: f32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GazeMode {
    #[default]
    TrackWorldTarget,
    DirectViewer,
    Scan,
    AvoidEyeContact,
    SideEye,
    Sleep,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct VoiceVisualState {
    pub active: bool,
    pub motif_id: u64,
    pub syllable_index: u8,
    pub envelope: f32,
    pub mouth_open: f32,
    pub pitch_normalized: f32,
    pub noisiness: f32,
    pub purr: f32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct EmbodiedPose {
    pub gaze: Vec2,
    pub gaze_mode: GazeMode,
    /// Brain-authorized translation of the complete facial mask toward the
    /// current semantic focus. This is a target; presentation smoothing lives
    /// in the permanent liquid face carrier.
    pub face_attention_offset: Vec2,
    /// Additive semantic head-turn roll. Neutral attention is exactly zero and
    /// never inherits rotation from the deforming liquid material.
    pub face_attention_roll: f32,
    pub vergence: f32,
    pub pupil_size: f32,
    pub pupil_asymmetry: f32,
    pub blink_left: f32,
    pub blink_right: f32,
    pub squint: f32,
    pub eye_aperture: f32,
    pub eye_scale: f32,
    pub brow_raise: f32,
    pub brow_tension: f32,
    pub brow_asymmetry: f32,
    pub mouth_open: f32,
    pub mouth_curve: f32,
    pub mouth_tension: f32,
    pub mouth_compression: f32,
    pub mouth_asymmetry: f32,
    pub effort: f32,
    pub relief: f32,
    pub cheek_glow: f32,
    pub squash: Vec2,
    pub tilt: f32,
    pub head_lag: Vec2,
    pub tail_lag: Vec2,
    pub breath: f32,
    pub compression: f32,
    pub audio_envelope: f32,
    pub purr: f32,
    pub morph: ModalDeformation,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EmbodiedRuntime {
    pub pose: EmbodiedPose,
    pub physiology: VisualPhysiologyRuntime,
    pub droplets: DropletRuntime,
    pub modal_dynamics: ModalDynamics,
    pub liquid: LiquidMorphRuntime,
    seed_phase: f32,
    seed: u64,
    elapsed: f32,
    fixation_elapsed: f32,
    fixation_duration: f32,
    gaze_target: Vec2,
    base_gaze: Vec2,
    gaze_velocity: Vec2,
    saccade_strength: f32,
    microsaccade_from: Vec2,
    microsaccade_offset: Vec2,
    microsaccade_target: Vec2,
    microsaccade_elapsed: f32,
    microsaccade_duration: f32,
    next_microsaccade: f32,
    microsaccade_sequence: u64,
    fixation_locked: bool,
    face_attention_fixation: Vec2,
    face_attention_sequence: u64,
    face_attention_translation_gain: f32,
    face_attention_roll_gain: f32,
    face_attention_roll_bias: f32,
    face_attention_mode: GazeMode,
    face_attention_engaged: bool,
    filtered_luminance: f32,
    luminance_history: [LuminanceSample; LUMINANCE_HISTORY],
    luminance_history_count: usize,
    luminance_history_cursor: usize,
    blink_phase: f32,
    blink_kind: BlinkKind,
    blink_clock: f32,
    next_blink: f32,
    slow_blink_cooldown: f32,
    wink_cooldown: f32,
    soft_velocity: Vec2,
    head_velocity: Vec2,
    tail_velocity: Vec2,
    world_to_body_scale: Vec2,
    motion_response_scale: f32,
    motion_acceleration_limit: f32,
    previous_world_position: Option<Vec2>,
    runtime_face: FaceRuntimeActuation,
    runtime_visual: VisualPhysiologyActuation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BlinkKind {
    None,
    Normal,
    Slow,
    WinkLeft,
    WinkRight,
    Startle,
}

impl EmbodiedRuntime {
    #[must_use]
    pub fn new(seed: u64, traits: &DerivedVisualTraits) -> Self {
        let seed_phase = (seed as u32 as f32 / u32::MAX as f32) * std::f32::consts::TAU;
        Self {
            pose: EmbodiedPose {
                pupil_size: 0.52,
                eye_aperture: 1.0,
                eye_scale: 1.0,
                squash: Vec2::ONE,
                breath: 0.5,
                ..EmbodiedPose::default()
            },
            physiology: VisualPhysiologyRuntime::new(seed, traits),
            droplets: DropletRuntime::new(seed, traits),
            modal_dynamics: ModalDynamics::default(),
            liquid: LiquidMorphRuntime::new(seed),
            seed_phase,
            seed,
            elapsed: 0.0,
            fixation_elapsed: 0.0,
            fixation_duration: 0.35 + seed_phase.sin().abs() * 0.45,
            gaze_target: Vec2::ZERO,
            base_gaze: Vec2::ZERO,
            gaze_velocity: Vec2::ZERO,
            saccade_strength: 0.0,
            microsaccade_from: Vec2::ZERO,
            microsaccade_offset: Vec2::ZERO,
            microsaccade_target: Vec2::ZERO,
            microsaccade_elapsed: 0.0,
            microsaccade_duration: 0.05,
            next_microsaccade: 0.32 + deterministic_unit(seed, 0, 0x51) * 0.42,
            microsaccade_sequence: 0,
            fixation_locked: false,
            face_attention_fixation: Vec2::ZERO,
            face_attention_sequence: 0,
            face_attention_translation_gain: 1.0,
            face_attention_roll_gain: 1.0,
            face_attention_roll_bias: 0.0,
            face_attention_mode: GazeMode::TrackWorldTarget,
            face_attention_engaged: false,
            filtered_luminance: 0.5,
            luminance_history: [LuminanceSample::default(); LUMINANCE_HISTORY],
            luminance_history_count: 0,
            luminance_history_cursor: 0,
            blink_phase: 0.0,
            blink_kind: BlinkKind::None,
            blink_clock: 0.0,
            next_blink: 1.8 + seed_phase.cos().abs() * 3.2,
            slow_blink_cooldown: 2.0,
            wink_cooldown: 3.0,
            soft_velocity: Vec2::ZERO,
            head_velocity: Vec2::ZERO,
            tail_velocity: Vec2::ZERO,
            world_to_body_scale: Vec2::ONE,
            motion_response_scale: 1.0,
            motion_acceleration_limit: 8.0,
            previous_world_position: None,
            runtime_face: FaceRuntimeActuation::default(),
            runtime_visual: VisualPhysiologyActuation::default(),
        }
    }

    pub fn set_nervous_system_actuation(
        &mut self,
        face: FaceRuntimeActuation,
        visual: VisualPhysiologyActuation,
    ) {
        self.runtime_face = face;
        self.runtime_visual = visual;
    }

    pub fn set_world_to_body_scale(&mut self, scale: Vec2) {
        if scale.is_finite() {
            self.world_to_body_scale = scale.clamp(Vec2::splat(-64.0), Vec2::splat(64.0));
        }
    }

    /// Scales only inertial visual response, never cursor/world transforms. The
    /// production Pet travels far slower than Body Lab's diagnostic flight loop;
    /// this keeps the same readable slosh without increasing desktop speed.
    pub fn set_motion_response_scale(&mut self, scale: f32) {
        if scale.is_finite() {
            self.motion_response_scale = scale.clamp(0.25, 4.0);
        }
    }

    /// Caps only the comoving visual acceleration seen by shell parcels and the
    /// liquid motor field. Navigation and physical desktop motion are untouched.
    pub fn set_motion_acceleration_limit(&mut self, limit: f32) {
        if limit.is_finite() {
            self.motion_acceleration_limit = limit.clamp(1.0, 8.0);
        }
    }

    #[must_use]
    pub fn world_to_body_scale(&self) -> Vec2 {
        self.world_to_body_scale
    }

    #[must_use]
    pub fn filtered_luminance(&self) -> f32 {
        self.filtered_luminance
    }

    #[must_use]
    pub fn microsaccade_offset(&self) -> Vec2 {
        self.microsaccade_offset
    }

    #[must_use]
    pub fn microsaccade_sequence(&self) -> u64 {
        self.microsaccade_sequence
    }

    #[must_use]
    pub fn fixation_locked(&self) -> bool {
        self.fixation_locked
    }

    /// Eye-local semantic target selected before presentation smoothing and
    /// microsaccades. Exposed for causal dev telemetry only.
    #[must_use]
    pub fn semantic_gaze_target(&self) -> Vec2 {
        self.gaze_target
    }

    /// Elapsed and scheduled duration of the current semantic fixation.
    #[must_use]
    pub fn fixation_timing(&self) -> (f32, f32) {
        (self.fixation_elapsed, self.fixation_duration)
    }

    /// Advances presentation-only trackers once per redraw. Simulation updates
    /// sample targets but never consume the per-frame motion budget.
    pub fn presentation_update(&mut self, dt: f32) {
        let dt = if dt.is_finite() {
            dt.clamp(0.0, 0.05)
        } else {
            0.0
        };
        self.present_gaze(dt);
        self.liquid.presentation_update(dt);
    }

    #[allow(clippy::too_many_arguments)]
    pub fn update(
        &mut self,
        genome: &BodyGenome,
        visual_traits: &DerivedVisualTraits,
        mut mind: VisualMindInput,
        intent: &BodyIntent,
        sensors: &SensorFrame,
        feedback: &BodyFeedback,
        affect: AffectState,
        expression: lifecore::ExpressionState,
        mut face_tuning: FaceTuning,
        voice: VoiceVisualState,
        dt: f32,
    ) {
        mind.sanitize();
        face_tuning.microsaccade_amount = (face_tuning.microsaccade_amount
            * self.runtime_face.microsaccade_amount_multiplier)
            .clamp(0.0, 2.0);
        face_tuning.microsaccade_rate = (face_tuning.microsaccade_rate
            * self.runtime_face.microsaccade_rate_multiplier)
            .clamp(0.01, 2.0);
        let dt = dt.clamp(0.0, 0.05);
        self.elapsed += dt;
        self.slow_blink_cooldown = (self.slow_blink_cooldown - dt).max(0.0);
        self.wink_cooldown = (self.wink_cooldown - dt).max(0.0);

        let mode = gaze_mode(intent, affect);
        let desired_gaze = desired_gaze(
            mode,
            intent,
            sensors,
            feedback.world_position,
            self.elapsed,
            self.seed_phase,
        );
        self.update_gaze(
            mode,
            desired_gaze,
            affect,
            mind,
            expression,
            face_tuning,
            dt,
        );
        self.update_attention_face_pose(mode, mind, feedback);
        let authored_blink = expression.blink_left.max(expression.blink_right);
        if authored_blink > 0.02 {
            self.pose.blink_left = expression.blink_left.clamp(0.0, 1.0);
            self.pose.blink_right = expression.blink_right.clamp(0.0, 1.0);
        } else {
            self.update_blink(mode, intent, affect, expression, dt);
        }
        self.pose.eye_aperture = smooth(
            self.pose.eye_aperture,
            expression.eye_aperture.clamp(0.0, 1.0),
            16.0,
            dt,
        );
        self.pose.eye_scale = smooth(
            self.pose.eye_scale,
            expression.eye_scale.clamp(0.88, 1.18),
            12.0,
            dt,
        );
        let aperture_closure = 1.0 - self.pose.eye_aperture;
        self.pose.blink_left = self.pose.blink_left.max(aperture_closure);
        self.pose.blink_right = self.pose.blink_right.max(aperture_closure);
        self.update_pupil(mode, intent, sensors, mind, expression, face_tuning, dt);
        self.update_soft_body(genome, intent, feedback, affect, dt);

        let voice_mouth = voice_mouth_target(voice);
        // An audible callback is the sole authority for a visibly open cavity.
        // Emotion still controls curve/tension below, but cannot mime failed audio.
        let mouth_target = voice_mouth;
        self.pose.mouth_open = smooth(self.pose.mouth_open, mouth_target, 22.0, dt);
        self.pose.mouth_curve = smooth(
            self.pose.mouth_curve,
            (expression.mouth_curve
                + expression.relief * 0.08
                + expression.mouth_asymmetry * 0.08
                + affect.valence * 0.08)
                .clamp(-1.0, 1.0),
            11.0,
            dt,
        );
        self.pose.mouth_tension = smooth(
            self.pose.mouth_tension,
            (expression.mouth_tension
                + expression.mouth_compression * 0.62
                + expression.effort * 0.28
                + affect.frustration * 0.35)
                .clamp(0.0, 1.0),
            13.0,
            dt,
        );
        self.pose.brow_raise = smooth(
            self.pose.brow_raise,
            (expression.brow_raise + affect.arousal * 0.06).clamp(-1.0, 1.0),
            12.0,
            dt,
        );
        self.pose.brow_tension = smooth(
            self.pose.brow_tension,
            (expression.brow_tension + affect.stress * 0.10).clamp(0.0, 1.0),
            14.0,
            dt,
        );
        let procedural_asymmetry = (self.elapsed * 0.41 + self.seed_phase).sin() * 0.035
            + if intent.pose == PoseIntent::Curious {
                0.05
            } else {
                0.0
            };
        let interaction_weight = expression
            .effort
            .max(expression.relief)
            .max(expression.brow_asymmetry.abs())
            .max(expression.mouth_compression)
            .clamp(0.0, 1.0);
        let asymmetry = procedural_asymmetry * (1.0 - interaction_weight)
            + expression.brow_asymmetry * interaction_weight;
        self.pose.brow_asymmetry = smooth(self.pose.brow_asymmetry, asymmetry, 7.0, dt);
        self.pose.mouth_compression = smooth(
            self.pose.mouth_compression,
            expression.mouth_compression,
            13.0,
            dt,
        );
        self.pose.mouth_asymmetry = smooth(
            self.pose.mouth_asymmetry,
            expression.mouth_asymmetry,
            10.0,
            dt,
        );
        self.pose.effort = smooth(self.pose.effort, expression.effort, 10.0, dt);
        self.pose.relief = smooth(self.pose.relief, expression.relief, 8.0, dt);
        self.pose.squint = smooth(
            self.pose.squint,
            (expression.squint + affect.stress * 0.28).clamp(0.0, 1.0),
            13.0,
            dt,
        );
        self.pose.cheek_glow = smooth(
            self.pose.cheek_glow,
            (expression.cheek_glow * 0.7 + affect.attachment * 0.38).clamp(0.0, 1.0),
            5.0,
            dt,
        );
        self.pose.audio_envelope = smooth(self.pose.audio_envelope, voice.envelope, 28.0, dt);
        self.pose.purr = smooth(self.pose.purr, voice.purr, 18.0, dt);
        self.physiology.update(visual_traits, mind, dt);
        self.physiology.pose.droplet_energy = self.runtime_visual.droplet_energy.clamp(0.0, 1.0);
        self.physiology.pose.droplet_spread = self.runtime_visual.droplet_spread.clamp(0.0, 1.0);
        self.physiology.pose.droplet_cohesion =
            self.runtime_visual.droplet_cohesion.clamp(0.0, 1.0);
        let normalized_displacement = self
            .previous_world_position
            .map_or(Vec2::ZERO, |previous| feedback.world_position - previous);
        self.previous_world_position = Some(feedback.world_position);
        let mut droplet_motion = DropletMotion::from_normalized_desktop(
            feedback,
            normalized_displacement,
            self.world_to_body_scale,
            self.pose.tilt,
            self.pose.squash,
        );
        droplet_motion.velocity =
            (droplet_motion.velocity * self.motion_response_scale).clamp_length_max(16.0);
        droplet_motion.acceleration = (droplet_motion.acceleration * self.motion_response_scale)
            .clamp_length_max(self.motion_acceleration_limit);
        let mut liquid_motion = DropletMotion::from_screen_space(
            feedback,
            normalized_displacement,
            self.world_to_body_scale,
        );
        liquid_motion.velocity =
            (liquid_motion.velocity * self.motion_response_scale).clamp_length_max(16.0);
        liquid_motion.acceleration = (liquid_motion.acceleration * self.motion_response_scale)
            .clamp_length_max(self.motion_acceleration_limit);
        self.droplets.update_with_morph(
            visual_traits,
            self.physiology.pose,
            mind,
            droplet_motion,
            self.pose.morph,
            self.pose.gaze,
            dt,
        );
        self.liquid.set_face_attention_pose(
            self.pose.face_attention_offset,
            self.pose.face_attention_roll,
        );
        self.liquid.update_with_tilt(
            genome,
            visual_traits,
            self.physiology.pose,
            mind,
            intent,
            sensors,
            feedback,
            liquid_motion,
            self.pose.morph,
            self.pose.breath,
            self.pose.tilt,
            dt,
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn update_gaze(
        &mut self,
        mode: GazeMode,
        desired: Vec2,
        affect: AffectState,
        mind: VisualMindInput,
        expression: lifecore::ExpressionState,
        face_tuning: FaceTuning,
        dt: f32,
    ) {
        self.fixation_elapsed += dt;
        let target_distance = desired.distance(self.gaze_target);
        let should_saccade = target_distance > 0.065
            || self.fixation_elapsed >= self.fixation_duration
            || self.pose.gaze_mode != mode;
        if should_saccade && desired.is_finite() {
            self.gaze_target = desired.clamp(Vec2::splat(-0.92), Vec2::splat(0.92));
            self.fixation_elapsed = 0.0;
            let attention_hold = mind.attention_commitment * 0.20;
            self.fixation_duration = (0.22
                + 0.58 * (0.5 + 0.5 * (self.elapsed * 0.89 + self.seed_phase).sin())
                + attention_hold
                + affect.stress * 0.14)
                .clamp(0.18, 1.15);
            self.saccade_strength = target_distance.clamp(0.0, 1.0);
        }
        self.saccade_strength = (self.saccade_strength - dt * 7.5).max(0.0);
        let stable_target = desired.distance(self.gaze_target) < 0.035;
        let sleep_lock = mode == GazeMode::Sleep;
        let strong_fixation = !sleep_lock
            && stable_target
            && ((mind.attention_confidence >= 0.72 && mind.attention_commitment >= 0.35)
                || expression.pupil_focus >= 0.82);
        self.fixation_locked = sleep_lock || strong_fixation;
        if sleep_lock {
            let decay = 1.0 - (-18.0 * dt).exp();
            self.microsaccade_offset = self.microsaccade_offset.lerp(Vec2::ZERO, decay);
            if self.microsaccade_offset.length_squared() < 1.0e-8 {
                self.microsaccade_offset = Vec2::ZERO;
            }
            self.microsaccade_from = self.microsaccade_offset;
            self.microsaccade_target = Vec2::ZERO;
            self.microsaccade_elapsed = self.microsaccade_duration;
        } else {
            if strong_fixation {
                // A fixation is a semantic lock, not a frozen eyeball. Keep
                // its deterministic micro-motion below a much tighter cap.
                let limit =
                    LOCKED_MICROSACCADE_LIMIT * face_tuning.microsaccade_amount.clamp(0.0, 1.0);
                self.microsaccade_offset = self.microsaccade_offset.clamp_length_max(limit);
                self.microsaccade_from = self.microsaccade_from.clamp_length_max(limit);
                self.microsaccade_target = self.microsaccade_target.clamp_length_max(limit);
            }
            self.next_microsaccade -= dt;
            if self.next_microsaccade <= 0.0 {
                let sequence = self.microsaccade_sequence;
                let random_angle =
                    deterministic_unit(self.seed, sequence, 0x91) * std::f32::consts::TAU;
                let attention_direction = (desired - self.gaze_target).normalize_or_zero();
                let random_direction = Vec2::from_angle(random_angle);
                let direction =
                    (random_direction * 0.72 + attention_direction * 0.28).normalize_or_zero();
                // `pose.gaze` is normalized to the full gaze range and the shader
                // maps it by 0.32 eye radii. Convert the authored 0.006..0.018
                // eye-local microsaccade into that normalized space so it remains
                // subtle but actually visible at desktop scale.
                let fixation_scale = if strong_fixation { 0.32 } else { 1.0 };
                let amplitude = (0.018 + deterministic_unit(self.seed, sequence, 0xA7) * 0.038)
                    * face_tuning.microsaccade_amount
                    * fixation_scale;
                self.microsaccade_from = self.microsaccade_offset;
                self.microsaccade_target = direction * amplitude;
                self.microsaccade_elapsed = 0.0;
                self.microsaccade_duration =
                    0.028 + deterministic_unit(self.seed, sequence, 0xC1) * 0.030;
                let fixation_rate = if strong_fixation { 0.82 } else { 1.0 };
                let rate = (0.85 + affect.arousal * 1.65)
                    * face_tuning.microsaccade_rate.max(0.01)
                    * fixation_rate;
                let interval_jitter = 0.62 + deterministic_unit(self.seed, sequence, 0xD3) * 0.44;
                self.next_microsaccade = interval_jitter / rate.max(0.05);
                self.microsaccade_sequence = self.microsaccade_sequence.wrapping_add(1);
            }
            if self.microsaccade_elapsed < self.microsaccade_duration {
                self.microsaccade_elapsed += dt;
                let phase = smoothstep(
                    0.0,
                    1.0,
                    self.microsaccade_elapsed / self.microsaccade_duration.max(1.0e-5),
                );
                self.microsaccade_offset =
                    self.microsaccade_from.lerp(self.microsaccade_target, phase);
            }
        }
        self.pose.gaze_mode = mode;
    }

    /// Converts the already-selected semantic fixation and current flight into
    /// a restrained whole-face pose. Attention confidence remains the authority
    /// for deliberate turns; actual body velocity supplies a continuous flight
    /// cue so launch and braking do not require a controller-mode switch.
    /// Variation is sampled only when a meaningful fixation changes, so a held
    /// focus remains calm while separate moments do not repeat mechanically.
    fn update_attention_face_pose(
        &mut self,
        mode: GazeMode,
        mind: VisualMindInput,
        feedback: &BodyFeedback,
    ) {
        let allows_head_turn = matches!(mode, GazeMode::TrackWorldTarget | GazeMode::SideEye);
        let fixation = if allows_head_turn && self.gaze_target.is_finite() {
            self.gaze_target.clamp_length_max(0.92)
        } else {
            Vec2::ZERO
        };
        let fixation_magnitude = fixation.length();
        let fixation_direction = fixation.normalize_or_zero();
        let cognitive_drive = (mind.attention_confidence
            * (0.52
                + mind.attention_commitment * 0.30
                + mind.curiosity * 0.10
                + mind.novelty * 0.08)
            + mind.social_focus * mind.attention_confidence * 0.08)
            .clamp(0.0, 1.0);

        let semantic_engaged = if self.face_attention_engaged {
            cognitive_drive >= 0.40 && fixation_magnitude >= 0.08 && allows_head_turn
        } else {
            cognitive_drive >= 0.54 && fixation_magnitude >= 0.12 && allows_head_turn
        };
        let local_flight_velocity =
            feedback.velocity * self.world_to_body_scale * self.motion_response_scale;
        let flight_speed = if local_flight_velocity.is_finite() {
            local_flight_velocity.length()
        } else {
            0.0
        };
        let flight_direction = local_flight_velocity.normalize_or_zero();
        let flight_drive = if mode == GazeMode::Sleep {
            0.0
        } else {
            smoothstep(0.045, 0.42, flight_speed) * 0.90
        };

        if !semantic_engaged {
            self.face_attention_engaged = false;
            self.face_attention_mode = mode;
        }

        if semantic_engaged {
            let fixation_changed = !self.face_attention_engaged
                || self.face_attention_mode != mode
                || fixation_direction.distance(self.face_attention_fixation) > 0.28;
            if fixation_changed {
                self.face_attention_sequence = self.face_attention_sequence.wrapping_add(1);
                let sequence = self.face_attention_sequence;
                self.face_attention_translation_gain =
                    0.94 + deterministic_unit(self.seed, sequence, 0x00FA_CE01) * 0.26;
                self.face_attention_roll_gain =
                    0.90 + deterministic_unit(self.seed, sequence, 0x00FA_CE02) * 0.28;
                self.face_attention_roll_bias =
                    (deterministic_unit(self.seed, sequence, 0x00FA_CE03) * 2.0 - 1.0)
                        * (0.006 + mind.novelty * 0.012);
                self.face_attention_fixation = fixation_direction;
            }
            self.face_attention_engaged = true;
            self.face_attention_mode = mode;
        }

        let semantic_strength = if semantic_engaged {
            smoothstep(0.38, 0.92, cognitive_drive) * smoothstep(0.08, 0.55, fixation_magnitude)
        } else {
            0.0
        };
        let turn_vector =
            fixation_direction * semantic_strength * self.face_attention_translation_gain
                + flight_direction * flight_drive * (1.0 - semantic_strength * 0.20);
        let turn_strength = turn_vector.length().clamp(0.0, 1.0);
        if turn_strength <= 1.0e-5 {
            self.pose.face_attention_offset = Vec2::ZERO;
            self.pose.face_attention_roll = 0.0;
            return;
        }
        let turn_direction = turn_vector.normalize_or_zero();
        let offset = Vec2::new(turn_direction.x * 0.076, turn_direction.y * 0.048) * turn_strength;
        self.pose.face_attention_offset = offset.clamp_length_max(0.082);
        let semantic_roll =
            -fixation_direction.x * 0.098 * self.face_attention_roll_gain * semantic_strength;
        let flight_roll = -flight_direction.x * 0.082 * flight_drive;
        self.pose.face_attention_roll =
            (semantic_roll + flight_roll + self.face_attention_roll_bias * semantic_strength)
                .clamp(-0.15, 0.15);
    }

    fn present_gaze(&mut self, dt: f32) {
        let presented_gaze_before = if self.pose.gaze.is_finite() {
            self.pose.gaze
        } else {
            Vec2::ZERO
        };
        let target = if self.pose.gaze_mode == GazeMode::Sleep {
            Vec2::ZERO
        } else {
            self.gaze_target
        };
        let frequency = 36.0 + self.saccade_strength * 48.0;
        // The explicit spring used by the soft-body presentation can overshoot
        // or become unstable during a frame hitch. Gaze uses the closed-form
        // critically damped solution and recovers to the last presented value,
        // never by snapping to a newly selected target.
        if !self.base_gaze.is_finite() {
            self.base_gaze = presented_gaze_before;
            self.gaze_velocity = Vec2::ZERO;
        }
        critical_damped_vec2(
            &mut self.base_gaze,
            &mut self.gaze_velocity,
            target,
            frequency,
            dt,
        );
        let proposed_gaze = (self.base_gaze + self.microsaccade_offset)
            .clamp(Vec2::splat(-0.95), Vec2::splat(0.95));
        if proposed_gaze.is_finite() {
            // A large saccade takes at least three visible frames at 60 Hz and
            // remains bounded through the maximum accepted 50 ms hitch.
            let step = (proposed_gaze - presented_gaze_before).clamp_length_max(MAX_GAZE_STEP);
            self.pose.gaze = presented_gaze_before + step;
        } else {
            self.pose.gaze = presented_gaze_before;
            self.gaze_velocity = Vec2::ZERO;
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn update_pupil(
        &mut self,
        mode: GazeMode,
        intent: &BodyIntent,
        sensors: &SensorFrame,
        mind: VisualMindInput,
        expression: lifecore::ExpressionState,
        face_tuning: FaceTuning,
        dt: f32,
    ) {
        self.luminance_history[self.luminance_history_cursor] = LuminanceSample {
            time: self.elapsed,
            value: mind.local_luminance,
        };
        self.luminance_history_cursor = (self.luminance_history_cursor + 1) % LUMINANCE_HISTORY;
        self.luminance_history_count = (self.luminance_history_count + 1).min(LUMINANCE_HISTORY);
        let delayed_time = self.elapsed - 0.18;
        let delayed_luminance = self.luminance_history[..self.luminance_history_count]
            .iter()
            .filter(|sample| sample.time <= delayed_time)
            .max_by(|a, b| a.time.total_cmp(&b.time))
            .map_or(mind.local_luminance, |sample| sample.value);
        let luminance_speed = std::f32::consts::LN_2 / 0.18;
        self.filtered_luminance = smooth(
            self.filtered_luminance,
            delayed_luminance,
            luminance_speed,
            dt,
        );

        let cursor_near = ((0.28 - sensors.cursor_distance_to_pet) / 0.28).clamp(0.0, 1.0);
        let near_focus = match intent.interaction_target.as_ref() {
            Some(InteractionTarget::Cursor) => cursor_near,
            Some(InteractionTarget::ProceduralOrb) => 0.65,
            Some(InteractionTarget::Surface(_)) => 0.45,
            Some(InteractionTarget::User) | None => 0.0,
        };
        let far_focus = if matches!(mode, GazeMode::DirectViewer | GazeMode::Scan) {
            1.0
        } else {
            0.0
        };
        // Emotional dilation follows arousal for pleasant and unpleasant states;
        // expression.pupil_size carries the VITA episode's authored intensity.
        let emotional_drive = mind.arousal * 0.20
            + mind.stress * mind.arousal * 0.10
            + (mind.curiosity * 0.65 + mind.social_focus * 0.35) * 0.08
            + mind.eye_modifiers.pain * 0.10
            + (expression.pupil_size - 0.5) * 0.18;
        let light_drive = (0.50 - self.filtered_luminance) * 0.60;
        let focus_drive = far_focus * 0.05 - near_focus * 0.10;
        let mut target = 0.50
            + light_drive * face_tuning.pupil_light_response
            + emotional_drive * face_tuning.pupil_emotion_response
            + focus_drive * face_tuning.pupil_focus_response
            + mind.eye_modifiers.pharmacologic_mydriasis * 0.28
            - mind.eye_modifiers.pharmacologic_miosis * 0.28;
        target = target.clamp(0.22, 0.86);
        target = self.pose.pupil_size
            + (target - self.pose.pupil_size) * (1.0 - mind.eye_modifiers.neural_reactivity_loss);
        let half_life = if target < self.pose.pupil_size {
            0.20
        } else {
            0.70
        };
        self.pose.pupil_size = smooth(
            self.pose.pupil_size,
            target,
            std::f32::consts::LN_2 / half_life,
            dt,
        );
        self.pose.pupil_asymmetry = smooth(
            self.pose.pupil_asymmetry,
            mind.eye_modifiers.anisocoria * 0.14,
            5.0,
            dt,
        );
        self.pose.vergence = smooth(self.pose.vergence, near_focus * 0.08, 8.0, dt);
    }

    fn update_blink(
        &mut self,
        mode: GazeMode,
        intent: &BodyIntent,
        affect: AffectState,
        expression: lifecore::ExpressionState,
        dt: f32,
    ) {
        self.blink_clock += dt;
        if self.blink_kind == BlinkKind::None {
            let startle = affect.stress > 0.72 && self.saccade_strength > 0.35;
            let slow = mode == GazeMode::DirectViewer
                && affect.attachment > 0.36
                && affect.stress < 0.35
                && self.slow_blink_cooldown <= 0.0;
            let wink = intent.pose == PoseIntent::Display
                && affect.valence > 0.46
                && self.wink_cooldown <= 0.0;
            let neural_urge = expression.blink_left.max(expression.blink_right) > 0.84;
            if startle {
                self.start_blink(BlinkKind::Startle);
            } else if slow {
                self.start_blink(BlinkKind::Slow);
                self.slow_blink_cooldown = 6.0 + affect.attachment * 8.0;
            } else if wink {
                let left = (self.elapsed * 0.73 + self.seed_phase).sin() >= 0.0;
                self.start_blink(if left {
                    BlinkKind::WinkLeft
                } else {
                    BlinkKind::WinkRight
                });
                self.wink_cooldown = 10.0;
            } else if self.blink_clock
                >= self.next_blink / self.runtime_face.blink_rate_multiplier.clamp(0.55, 1.55)
                || neural_urge
                || self.saccade_strength > 0.72
            {
                self.start_blink(BlinkKind::Normal);
            }
        }

        let duration = match self.blink_kind {
            BlinkKind::None => 1.0,
            BlinkKind::Normal => 0.18,
            BlinkKind::Slow => 0.72,
            BlinkKind::WinkLeft | BlinkKind::WinkRight => 0.34,
            BlinkKind::Startle => 0.11,
        };
        if self.blink_kind != BlinkKind::None {
            self.blink_phase += dt / duration;
            let envelope = blink_envelope(self.blink_phase, self.blink_kind == BlinkKind::Slow);
            let (left, right) = match self.blink_kind {
                BlinkKind::WinkLeft => (envelope, expression.blink_right * 0.18),
                BlinkKind::WinkRight => (expression.blink_left * 0.18, envelope),
                _ => (envelope, envelope),
            };
            self.pose.blink_left = left.clamp(0.0, 1.0);
            self.pose.blink_right = right.clamp(0.0, 1.0);
            if self.blink_phase >= 1.0 {
                self.blink_kind = BlinkKind::None;
                self.blink_phase = 0.0;
                self.blink_clock = 0.0;
                self.next_blink =
                    2.0 + 4.2 * (0.5 + 0.5 * (self.elapsed * 0.37 + self.seed_phase).sin());
            }
        } else if mode == GazeMode::Sleep {
            self.pose.blink_left = smooth(self.pose.blink_left, 0.92, 4.0, dt);
            self.pose.blink_right = smooth(self.pose.blink_right, 0.92, 4.0, dt);
        } else {
            self.pose.blink_left = smooth(self.pose.blink_left, 0.0, 30.0, dt);
            self.pose.blink_right = smooth(self.pose.blink_right, 0.0, 30.0, dt);
        }
    }

    fn start_blink(&mut self, kind: BlinkKind) {
        self.blink_kind = kind;
        self.blink_phase = 0.0;
    }

    fn update_soft_body(
        &mut self,
        genome: &BodyGenome,
        intent: &BodyIntent,
        feedback: &BodyFeedback,
        affect: AffectState,
        dt: f32,
    ) {
        let velocity = (feedback.velocity * self.world_to_body_scale * self.motion_response_scale)
            .clamp_length_max(16.0);
        let acceleration =
            (feedback.acceleration * self.world_to_body_scale * self.motion_response_scale)
                .clamp_length_max(self.motion_acceleration_limit);
        let speed = velocity.length().clamp(0.0, 1.2);
        let softness = genome.softness.clamp(0.0, 1.0);
        let pose_compression = match intent.pose {
            PoseIntent::Compact | PoseIntent::Sleeping | PoseIntent::Cocoon => 0.20,
            PoseIntent::Landing | PoseIntent::Clinging => 0.11,
            PoseIntent::Playful => -0.05,
            _ => 0.0,
        };
        let impact = feedback
            .collision
            .as_ref()
            .map_or(0.0, |collision| collision.intensity)
            .clamp(0.0, 1.0);
        let compression_target =
            (pose_compression + impact * 0.35 + affect.stress * 0.08).clamp(-0.08, 0.48);
        self.pose.compression = smooth(self.pose.compression, compression_target, 15.0, dt);

        let stretch =
            (speed * (0.12 + softness * 0.16) - self.pose.compression * 0.48).clamp(-0.18, 0.30);
        let squash_target = Vec2::new(
            (1.0 - stretch * 0.58 + self.pose.compression * 0.30).clamp(0.72, 1.28),
            (1.0 + stretch - self.pose.compression * 0.45).clamp(0.68, 1.34),
        );
        spring_vec2(
            &mut self.pose.squash,
            &mut self.soft_velocity,
            squash_target,
            10.0 + (1.0 - softness) * 8.0,
            dt,
        );
        let volume = (self.pose.squash.x * self.pose.squash.y).max(0.05).sqrt();
        self.pose.squash /= volume;

        let tilt_activity = ((speed - 0.10) / 0.28)
            .clamp(0.0, 1.0)
            .max(((acceleration.length() - 0.18) / 0.85).clamp(0.0, 1.0));
        let tilt_target =
            ((-velocity.x * 0.20 - acceleration.x * 0.065).clamp(-0.18, 0.18)) * tilt_activity;
        self.pose.tilt = smooth(self.pose.tilt, tilt_target, 8.0, dt);
        let head_target = Vec2::new(-acceleration.x, -acceleration.y) * (0.028 + softness * 0.035);
        spring_vec2(
            &mut self.pose.head_lag,
            &mut self.head_velocity,
            head_target.clamp_length_max(0.12),
            9.0 + (1.0 - softness) * 6.0,
            dt,
        );
        let tail_target = Vec2::new(-velocity.x, -velocity.y) * (0.10 + softness * 0.13);
        spring_vec2(
            &mut self.pose.tail_lag,
            &mut self.tail_velocity,
            tail_target.clamp_length_max(0.26),
            5.0 + (1.0 - softness) * 4.0,
            dt,
        );
        let breathing_rate =
            1.2 + affect.arousal * 1.8 + voice_breath_boost(self.pose.audio_envelope);
        let breath_target = 0.5 + 0.5 * (self.elapsed * breathing_rate + self.seed_phase).sin();
        self.pose.breath = smooth(self.pose.breath, breath_target, 3.0, dt);
        self.modal_dynamics.update(softness, feedback, dt);
        self.pose.morph = self.modal_dynamics.deformation;
    }
}

fn voice_mouth_target(voice: VoiceVisualState) -> f32 {
    if !voice.active {
        return 0.0;
    }
    let activity = voice.envelope.clamp(0.0, 1.0).sqrt();
    let non_purr = 1.0 - voice.purr.clamp(0.0, 1.0) * 0.62;
    let audible_aperture_floor = (0.16 + voice.noisiness.clamp(0.0, 1.0) * 0.16) * non_purr;
    let articulated_aperture = voice.mouth_open.clamp(0.0, 1.0).max(audible_aperture_floor);
    // `voice.envelope` is a normalized physical activity signal. Keep quiet
    // phonation visibly articulated instead of multiplying the tract aperture
    // by raw near-zero PCM energy.
    articulated_aperture * (0.30 + activity * 0.70)
}

fn gaze_mode(intent: &BodyIntent, affect: AffectState) -> GazeMode {
    if intent.pose == PoseIntent::Sleeping {
        GazeMode::Sleep
    } else if matches!(
        intent.interaction_target.as_ref(),
        Some(InteractionTarget::User)
    ) || intent.pose == PoseIntent::Display
    {
        GazeMode::DirectViewer
    } else if affect.stress > 0.72 {
        GazeMode::Scan
    } else if affect.attachment < 0.16 && affect.stress > 0.36 {
        GazeMode::AvoidEyeContact
    } else if intent.pose == PoseIntent::Curious && affect.confidence > 0.58 {
        GazeMode::SideEye
    } else {
        GazeMode::TrackWorldTarget
    }
}

fn desired_gaze(
    mode: GazeMode,
    intent: &BodyIntent,
    sensors: &SensorFrame,
    body_position: Vec2,
    elapsed: f32,
    seed_phase: f32,
) -> Vec2 {
    match mode {
        GazeMode::DirectViewer | GazeMode::Sleep => Vec2::ZERO,
        GazeMode::Scan => Vec2::new(
            (elapsed * 1.9 + seed_phase).sin() * 0.72,
            (elapsed * 1.37 + seed_phase * 1.4).cos() * 0.48,
        ),
        GazeMode::AvoidEyeContact => Vec2::new(
            if sensors.cursor_position.x < body_position.x {
                0.68
            } else {
                -0.68
            },
            0.20,
        ),
        GazeMode::SideEye => {
            let side = if sensors.cursor_position.x < body_position.x {
                -1.0
            } else {
                1.0
            };
            Vec2::new(side * 0.62, -0.08)
        }
        GazeMode::TrackWorldTarget => {
            let target = intent.gaze_target.unwrap_or(sensors.cursor_position);
            ((target - body_position) * Vec2::new(2.2, -2.2))
                .clamp(Vec2::splat(-0.88), Vec2::splat(0.88))
        }
    }
}

fn blink_envelope(phase: f32, slow: bool) -> f32 {
    let phase = phase.clamp(0.0, 1.0);
    if slow {
        if phase < 0.34 {
            smoothstep(0.0, 0.34, phase)
        } else if phase < 0.66 {
            1.0
        } else {
            1.0 - smoothstep(0.66, 1.0, phase)
        }
    } else {
        // `sin(PI)` can be a tiny negative f32. A fractional power of that value
        // becomes NaN and used to poison both eyelid uniforms after the first blink.
        (phase * std::f32::consts::PI).sin().max(0.0).powf(0.72)
    }
}

fn voice_breath_boost(envelope: f32) -> f32 {
    envelope.clamp(0.0, 1.0) * 2.2
}

fn spring_vec2(current: &mut Vec2, velocity: &mut Vec2, target: Vec2, frequency: f32, dt: f32) {
    let acceleration = (target - *current) * frequency * frequency - *velocity * (2.0 * frequency);
    *velocity += acceleration * dt;
    *current += *velocity * dt;
    if !current.is_finite() || !velocity.is_finite() {
        *current = target;
        *velocity = Vec2::ZERO;
    }
}

/// Exact solution of a critically damped second-order tracker over `dt`.
/// Invalid input preserves the last valid presentation instead of using the
/// target as a recovery value.
fn critical_damped_vec2(
    current: &mut Vec2,
    velocity: &mut Vec2,
    target: Vec2,
    frequency: f32,
    dt: f32,
) {
    if !current.is_finite()
        || !velocity.is_finite()
        || !target.is_finite()
        || !frequency.is_finite()
        || !dt.is_finite()
    {
        *velocity = Vec2::ZERO;
        return;
    }
    let omega = frequency.max(0.0);
    let dt = dt.max(0.0);
    let offset = *current - target;
    let helper = *velocity + offset * omega;
    let decay = (-omega * dt).exp();
    *current = target + (offset + helper * dt) * decay;
    *velocity = (*velocity - helper * (omega * dt)) * decay;
}

fn smooth(current: f32, target: f32, speed: f32, dt: f32) -> f32 {
    current + (target - current) * (1.0 - (-speed * dt).exp())
}

fn smoothstep(edge0: f32, edge1: f32, value: f32) -> f32 {
    let t = ((value - edge0) / (edge1 - edge0).max(f32::EPSILON)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn deterministic_unit(seed: u64, sequence: u64, salt: u64) -> f32 {
    let mut value = seed
        ^ sequence.wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ salt.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value ^= value >> 30;
    value = value.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value ^= value >> 27;
    value = value.wrapping_mul(0x94D0_49BB_1331_11EB);
    value ^= value >> 31;
    (value as u32) as f32 / u32::MAX as f32
}

#[cfg(test)]
mod tests {
    use super::*;
    use lifecore::{BodyIntent, Genome, LocomotionMode, PoseIntent};

    fn intent() -> BodyIntent {
        BodyIntent {
            locomotion: LocomotionMode::Hover,
            target_position: Vec2::splat(0.5),
            target_surface: None,
            desired_speed: 0.1,
            facing_direction: 1.0,
            gaze_target: Some(Vec2::new(0.9, 0.2)),
            pose: PoseIntent::Curious,
            expression: lifecore::ExpressionState::default(),
            interaction_target: None,
        }
    }

    #[test]
    fn embodiment_stays_finite_and_volume_bounded() {
        let genome = Genome::from_seed(42);
        let traits = DerivedVisualTraits::from_genome(&genome);
        let mut runtime = EmbodiedRuntime::new(genome.identity_seed, &traits);
        let feedback = BodyFeedback {
            velocity: Vec2::new(0.8, -0.2),
            acceleration: Vec2::new(0.4, 0.1),
            ..BodyFeedback::default()
        };
        let sensors = SensorFrame::default();
        for _ in 0..10_000 {
            runtime.update(
                &genome.body,
                &traits,
                VisualMindInput::default(),
                &intent(),
                &sensors,
                &feedback,
                AffectState::default(),
                lifecore::ExpressionState::default(),
                FaceTuning::default(),
                VoiceVisualState::default(),
                1.0 / 120.0,
            );
            assert!(runtime.pose.gaze.is_finite());
            assert!(runtime.pose.squash.is_finite());
            assert!(runtime.pose.blink_left.is_finite());
            assert!(runtime.pose.blink_right.is_finite());
            assert!(runtime.pose.squash.min_element() > 0.55);
            assert!(runtime.pose.squash.max_element() < 1.65);
        }
    }

    #[test]
    fn production_motion_cap_keeps_normal_flight_cohesive_and_recovers() {
        let genome = Genome::from_seed(0xF11A7);
        let traits = DerivedVisualTraits::from_genome(&genome);
        let mut runtime = EmbodiedRuntime::new(genome.identity_seed, &traits);
        runtime.set_world_to_body_scale(Vec2::new(13.8, -5.8));
        runtime.set_motion_response_scale(2.15);
        runtime.set_motion_acceleration_limit(4.20);
        let sensors = SensorFrame::default();
        let dt = 1.0 / 120.0;
        let mut feedback = BodyFeedback {
            velocity: Vec2::new(0.10, 0.0),
            acceleration: Vec2::new(0.34, 0.0),
            ..BodyFeedback::default()
        };
        let mut maximum_flight_lean = 0.0_f32;
        for _ in 0..60 {
            runtime.update(
                &genome.body,
                &traits,
                VisualMindInput::default(),
                &intent(),
                &sensors,
                &feedback,
                AffectState::default(),
                lifecore::ExpressionState::default(),
                FaceTuning::default(),
                VoiceVisualState::default(),
                dt,
            );
            maximum_flight_lean = maximum_flight_lean.max(runtime.pose.tilt.abs());
            assert!(runtime.liquid.diagnostics().main_mass >= 92.0);
        }
        assert!(maximum_flight_lean >= 0.08, "lean={maximum_flight_lean}");
        feedback.acceleration = Vec2::ZERO;
        for _ in 0..480 {
            runtime.update(
                &genome.body,
                &traits,
                VisualMindInput::default(),
                &intent(),
                &sensors,
                &feedback,
                AffectState::default(),
                lifecore::ExpressionState::default(),
                FaceTuning::default(),
                VoiceVisualState::default(),
                dt,
            );
            assert!(runtime.liquid.diagnostics().main_mass >= 92.0);
        }
        let diagnostics = runtime.liquid.diagnostics();
        assert!(diagnostics.stretch_ratio <= 1.18, "{diagnostics:?}");
        assert!(diagnostics.finite);
    }

    #[test]
    fn viewer_target_produces_direct_gaze() {
        let genome = Genome::from_seed(9);
        let traits = DerivedVisualTraits::from_genome(&genome);
        let mut runtime = EmbodiedRuntime::new(genome.identity_seed, &traits);
        let mut display = intent();
        display.pose = PoseIntent::Display;
        display.interaction_target = Some(InteractionTarget::User);
        for _ in 0..240 {
            runtime.update(
                &genome.body,
                &traits,
                VisualMindInput::default(),
                &display,
                &SensorFrame::default(),
                &BodyFeedback::default(),
                AffectState::default(),
                display.expression,
                FaceTuning::default(),
                VoiceVisualState::default(),
                1.0 / 120.0,
            );
        }
        assert_eq!(runtime.pose.gaze_mode, GazeMode::DirectViewer);
        assert!(runtime.pose.gaze.length() < 0.08);
    }

    fn settled_pupil(
        luminance: f32,
        arousal: f32,
        curiosity: f32,
        target: Option<InteractionTarget>,
    ) -> f32 {
        let genome = Genome::from_seed(0xE1E);
        let traits = DerivedVisualTraits::from_genome(&genome);
        let mut runtime = EmbodiedRuntime::new(genome.identity_seed, &traits);
        let mut intent = intent();
        intent.interaction_target = target;
        let sensors = SensorFrame {
            cursor_distance_to_pet: 0.02,
            ..SensorFrame::default()
        };
        for _ in 0..360 {
            runtime.elapsed += 1.0 / 120.0;
            runtime.update_pupil(
                GazeMode::TrackWorldTarget,
                &intent,
                &sensors,
                VisualMindInput {
                    local_luminance: luminance,
                    arousal,
                    curiosity,
                    ..VisualMindInput::default()
                },
                lifecore::ExpressionState::default(),
                FaceTuning::default(),
                1.0 / 120.0,
            );
        }
        runtime.pose.pupil_size
    }

    #[test]
    fn pupil_response_is_physiologically_monotonic_and_symmetric_by_default() {
        let bright = settled_pupil(1.0, 0.0, 0.0, None);
        let dark = settled_pupil(0.0, 0.0, 0.0, None);
        let aroused = settled_pupil(0.5, 1.0, 0.0, None);
        let interested = settled_pupil(0.5, 0.0, 1.0, None);
        let near = settled_pupil(0.5, 0.0, 0.0, Some(InteractionTarget::Cursor));
        let far = settled_pupil(0.5, 0.0, 0.0, Some(InteractionTarget::User));
        assert!(bright < 0.50 && dark > 0.50, "bright={bright} dark={dark}");
        assert!(aroused > 0.50 && interested > 0.50);
        assert!(near < far, "near={near} far={far}");

        let genome = Genome::from_seed(0xA11CE);
        let traits = DerivedVisualTraits::from_genome(&genome);
        let runtime = EmbodiedRuntime::new(genome.identity_seed, &traits);
        assert_eq!(runtime.pose.pupil_asymmetry, 0.0);
    }

    #[test]
    fn strong_fixation_keeps_small_deterministic_microsaccades_but_sleep_is_still() {
        let genome = Genome::from_seed(0xF0C05);
        let traits = DerivedVisualTraits::from_genome(&genome);
        let mut runtime = EmbodiedRuntime::new(genome.identity_seed, &traits);
        runtime.microsaccade_offset = Vec2::new(0.018, -0.009);
        runtime.next_microsaccade = 0.0;
        let sequence = runtime.microsaccade_sequence;
        let mut expression = lifecore::ExpressionState {
            pupil_focus: 1.0,
            ..lifecore::ExpressionState::default()
        };
        for _ in 0..12 {
            runtime.update_gaze(
                GazeMode::TrackWorldTarget,
                Vec2::ZERO,
                AffectState::default(),
                VisualMindInput {
                    attention_confidence: 1.0,
                    attention_commitment: 1.0,
                    ..VisualMindInput::default()
                },
                expression,
                FaceTuning::default(),
                1.0 / 120.0,
            );
        }
        assert!(runtime.fixation_locked);
        assert_eq!(runtime.microsaccade_sequence, sequence + 1);
        assert!(runtime.microsaccade_target.length() > 0.0);
        assert!(
            runtime.microsaccade_target.length() <= LOCKED_MICROSACCADE_LIMIT + 1.0e-6,
            "target={:?}",
            runtime.microsaccade_target
        );
        assert!(runtime.microsaccade_offset.length() > 0.0);
        assert!(runtime.microsaccade_offset.length() <= LOCKED_MICROSACCADE_LIMIT + 1.0e-6);

        let sleeping_sequence = runtime.microsaccade_sequence;
        runtime.next_microsaccade = 0.0;
        runtime.microsaccade_offset = Vec2::new(0.012, -0.006);
        for _ in 0..120 {
            runtime.update_gaze(
                GazeMode::Sleep,
                Vec2::ZERO,
                AffectState::default(),
                VisualMindInput {
                    attention_confidence: 1.0,
                    attention_commitment: 1.0,
                    ..VisualMindInput::default()
                },
                expression,
                FaceTuning::default(),
                1.0 / 120.0,
            );
        }
        assert!(runtime.fixation_locked);
        assert_eq!(runtime.microsaccade_sequence, sleeping_sequence);
        assert_eq!(runtime.microsaccade_offset, Vec2::ZERO);

        expression.pupil_focus = 0.5;
        runtime.next_microsaccade = 0.0;
        runtime.update_gaze(
            GazeMode::TrackWorldTarget,
            Vec2::ZERO,
            AffectState::default(),
            VisualMindInput::default(),
            expression,
            FaceTuning::default(),
            1.0 / 120.0,
        );
        assert_eq!(runtime.microsaccade_sequence, sleeping_sequence + 1);
        assert!(
            (0.018..=0.056_1).contains(&runtime.microsaccade_target.length()),
            "target={:?}",
            runtime.microsaccade_target
        );
    }

    #[test]
    fn gaze_acquires_subtle_target_changes_quickly_with_short_bounded_fixations() {
        let genome = Genome::from_seed(0xFA57_E1E5);
        let traits = DerivedVisualTraits::from_genome(&genome);
        let mut runtime = EmbodiedRuntime::new(genome.identity_seed, &traits);
        runtime.fixation_duration = 10.0;
        let subtle_target = Vec2::new(0.075, 0.0);

        runtime.update_gaze(
            GazeMode::TrackWorldTarget,
            subtle_target,
            AffectState::default(),
            VisualMindInput::default(),
            lifecore::ExpressionState::default(),
            FaceTuning::default(),
            1.0 / 120.0,
        );
        assert_eq!(runtime.semantic_gaze_target(), subtle_target);
        assert!((0.18..=1.15).contains(&runtime.fixation_duration));

        let far_target = Vec2::new(0.80, -0.42);
        runtime.update_gaze(
            GazeMode::TrackWorldTarget,
            far_target,
            AffectState::default(),
            VisualMindInput::default(),
            lifecore::ExpressionState::default(),
            FaceTuning::default(),
            1.0 / 60.0,
        );
        let mut acquired_frame = None;
        for frame in 1..=15 {
            runtime.present_gaze(1.0 / 60.0);
            if runtime.pose.gaze.distance(far_target) <= 0.04 {
                acquired_frame = Some(frame);
                break;
            }
        }
        assert!(
            acquired_frame.is_some_and(|frame| frame <= 13),
            "presented gaze did not acquire target quickly: frame={acquired_frame:?} gaze={:?}",
            runtime.pose.gaze
        );
        assert!(runtime.pose.gaze.max_element() <= 0.95);
        assert!(runtime.pose.gaze.min_element() >= -0.95);
    }

    #[test]
    fn large_saccade_spans_at_least_three_frames_and_each_step_is_bounded() {
        let genome = Genome::from_seed(0x05AC_CADE);
        let traits = DerivedVisualTraits::from_genome(&genome);
        let mut runtime = EmbodiedRuntime::new(genome.identity_seed, &traits);
        let target = Vec2::new(0.90, -0.72);
        let mut previous = runtime.pose.gaze;

        for frame in 0..3 {
            runtime.update_gaze(
                GazeMode::TrackWorldTarget,
                target,
                AffectState::default(),
                VisualMindInput::default(),
                lifecore::ExpressionState::default(),
                FaceTuning::default(),
                1.0 / 60.0,
            );
            runtime.present_gaze(1.0 / 60.0);
            let step = runtime.pose.gaze.distance(previous);
            assert!(step <= MAX_GAZE_STEP + 1.0e-6, "frame={frame} step={step}");
            previous = runtime.pose.gaze;
            if frame < 2 {
                assert!(
                    runtime.pose.gaze.distance(target) > MAX_GAZE_STEP,
                    "saccade completed before its third presented frame"
                );
            }
        }
    }

    #[test]
    fn simulation_gaze_sampling_does_not_consume_multiple_presentation_steps() {
        let genome = Genome::from_seed(0xFACE_1200);
        let traits = DerivedVisualTraits::from_genome(&genome);
        let mut runtime = EmbodiedRuntime::new(genome.identity_seed, &traits);
        let before = runtime.pose.gaze;
        for _ in 0..4 {
            runtime.update_gaze(
                GazeMode::TrackWorldTarget,
                Vec2::new(0.90, 0.70),
                AffectState::default(),
                VisualMindInput::default(),
                lifecore::ExpressionState::default(),
                FaceTuning::default(),
                1.0 / 120.0,
            );
        }
        assert_eq!(runtime.pose.gaze, before);

        runtime.present_gaze(1.0 / 60.0);
        assert!(runtime.pose.gaze.distance(before) <= MAX_GAZE_STEP + 1.0e-6);
        assert!(runtime.pose.gaze.distance(before) > 0.0);
    }

    #[test]
    fn fifty_millisecond_hitch_cannot_jump_gaze_or_snap_invalid_state_to_target() {
        let genome = Genome::from_seed(0x00A1_7C45);
        let traits = DerivedVisualTraits::from_genome(&genome);
        let mut runtime = EmbodiedRuntime::new(genome.identity_seed, &traits);
        runtime.pose.gaze = Vec2::new(0.22, -0.11);
        runtime.base_gaze = Vec2::splat(f32::NAN);
        runtime.gaze_velocity = Vec2::splat(f32::INFINITY);
        let before = runtime.pose.gaze;
        let target = Vec2::new(-0.90, 0.90);

        runtime.update_gaze(
            GazeMode::TrackWorldTarget,
            target,
            AffectState::default(),
            VisualMindInput::default(),
            lifecore::ExpressionState::default(),
            FaceTuning::default(),
            0.05,
        );
        runtime.present_gaze(0.05);

        assert!(runtime.pose.gaze.is_finite());
        assert!(runtime.pose.gaze.distance(before) <= MAX_GAZE_STEP + 1.0e-6);
        assert!(runtime.pose.gaze.distance(target) > 0.5);
    }

    #[test]
    fn semantic_attention_turn_has_exact_neutral_and_stable_fixation_variation() {
        let genome = Genome::from_seed(0xA77E_1710);
        let traits = DerivedVisualTraits::from_genome(&genome);
        let mut runtime = EmbodiedRuntime::new(genome.identity_seed, &traits);
        runtime.gaze_target = Vec2::new(0.84, 0.22);

        runtime.update_attention_face_pose(
            GazeMode::TrackWorldTarget,
            VisualMindInput::default(),
            &BodyFeedback::default(),
        );
        assert_eq!(runtime.pose.face_attention_offset, Vec2::ZERO);
        assert_eq!(runtime.pose.face_attention_roll, 0.0);

        let strong_attention = VisualMindInput {
            curiosity: 0.88,
            novelty: 0.74,
            attention_confidence: 0.96,
            attention_commitment: 0.91,
            ..VisualMindInput::default()
        };
        runtime.update_attention_face_pose(
            GazeMode::TrackWorldTarget,
            strong_attention,
            &BodyFeedback::default(),
        );
        assert!(runtime.pose.face_attention_offset.x > 0.0);
        assert!(runtime.pose.face_attention_offset.length() <= 0.082_001);
        assert!(runtime.pose.face_attention_roll < 0.0);
        assert!(runtime.pose.face_attention_roll.abs() <= 0.150_001);
        let first_sequence = runtime.face_attention_sequence;
        let first_signature = (
            runtime.face_attention_translation_gain,
            runtime.face_attention_roll_gain,
            runtime.face_attention_roll_bias,
        );
        let first_pose = (
            runtime.pose.face_attention_offset,
            runtime.pose.face_attention_roll,
        );

        for _ in 0..240 {
            runtime.update_attention_face_pose(
                GazeMode::TrackWorldTarget,
                strong_attention,
                &BodyFeedback::default(),
            );
        }
        assert_eq!(runtime.face_attention_sequence, first_sequence);
        assert_eq!(
            (
                runtime.pose.face_attention_offset,
                runtime.pose.face_attention_roll
            ),
            first_pose,
            "a held fixation must not reroll or jitter"
        );

        runtime.update_attention_face_pose(
            GazeMode::TrackWorldTarget,
            VisualMindInput::default(),
            &BodyFeedback::default(),
        );
        assert_eq!(runtime.pose.face_attention_offset, Vec2::ZERO);
        assert_eq!(runtime.pose.face_attention_roll, 0.0);
        runtime.gaze_target = Vec2::new(-0.68, 0.36);
        runtime.update_attention_face_pose(
            GazeMode::TrackWorldTarget,
            strong_attention,
            &BodyFeedback::default(),
        );
        let second_signature = (
            runtime.face_attention_translation_gain,
            runtime.face_attention_roll_gain,
            runtime.face_attention_roll_bias,
        );
        assert_eq!(runtime.face_attention_sequence, first_sequence + 1);
        assert_ne!(second_signature, first_signature);

        runtime.update_attention_face_pose(
            GazeMode::Sleep,
            strong_attention,
            &BodyFeedback::default(),
        );
        assert_eq!(runtime.pose.face_attention_offset, Vec2::ZERO);
        assert_eq!(runtime.pose.face_attention_roll, 0.0);
    }

    #[test]
    fn attention_pose_replay_is_seed_deterministic() {
        let genome = Genome::from_seed(0x5EED_FACE);
        let traits = DerivedVisualTraits::from_genome(&genome);
        let mut first = EmbodiedRuntime::new(genome.identity_seed, &traits);
        let mut replay = EmbodiedRuntime::new(genome.identity_seed, &traits);
        let strong_attention = VisualMindInput {
            curiosity: 0.82,
            novelty: 0.93,
            social_focus: 0.61,
            attention_confidence: 0.97,
            attention_commitment: 0.84,
            ..VisualMindInput::default()
        };

        for target in [
            Vec2::new(0.72, 0.18),
            Vec2::ZERO,
            Vec2::new(-0.48, -0.62),
            Vec2::ZERO,
            Vec2::new(0.16, 0.82),
        ] {
            first.gaze_target = target;
            replay.gaze_target = target;
            let mind = if target == Vec2::ZERO {
                VisualMindInput::default()
            } else {
                strong_attention
            };
            first.update_attention_face_pose(
                GazeMode::TrackWorldTarget,
                mind,
                &BodyFeedback::default(),
            );
            replay.update_attention_face_pose(
                GazeMode::TrackWorldTarget,
                mind,
                &BodyFeedback::default(),
            );
            assert_eq!(
                first.pose.face_attention_offset,
                replay.pose.face_attention_offset
            );
            assert_eq!(
                first.pose.face_attention_roll,
                replay.pose.face_attention_roll
            );
            assert_eq!(
                first.face_attention_sequence,
                replay.face_attention_sequence
            );
        }
    }

    #[test]
    fn flight_turns_the_face_then_returns_to_exact_horizontal_target() {
        let genome = Genome::from_seed(0xF11A_1170);
        let traits = DerivedVisualTraits::from_genome(&genome);
        let mut runtime = EmbodiedRuntime::new(genome.identity_seed, &traits);
        runtime.set_world_to_body_scale(Vec2::splat(1.0));
        let flying = BodyFeedback {
            velocity: Vec2::new(0.72, 0.48),
            ..BodyFeedback::default()
        };
        runtime.update_attention_face_pose(
            GazeMode::DirectViewer,
            VisualMindInput::default(),
            &flying,
        );
        assert!(runtime.pose.face_attention_offset.x > 0.035);
        assert!(runtime.pose.face_attention_offset.y > 0.015);
        assert!(runtime.pose.face_attention_roll < -0.03);

        runtime.update_attention_face_pose(
            GazeMode::DirectViewer,
            VisualMindInput::default(),
            &BodyFeedback::default(),
        );
        assert_eq!(runtime.pose.face_attention_offset, Vec2::ZERO);
        assert_eq!(runtime.pose.face_attention_roll, 0.0);
    }

    #[test]
    fn blink_envelope_is_finite_at_every_endpoint_and_phase() {
        for step in 0..=1_000 {
            let phase = step as f32 / 1_000.0;
            for slow in [false, true] {
                let value = blink_envelope(phase, slow);
                assert!(value.is_finite(), "phase={phase} slow={slow}");
                assert!((0.0..=1.0).contains(&value));
            }
        }
    }

    #[test]
    fn normalized_voice_activity_keeps_spoken_mouth_visibly_open() {
        let speaking = voice_mouth_target(VoiceVisualState {
            active: true,
            envelope: 0.52,
            mouth_open: 0.46,
            noisiness: 0.18,
            ..VoiceVisualState::default()
        });
        let quiet_purr = voice_mouth_target(VoiceVisualState {
            active: true,
            envelope: 0.28,
            mouth_open: 0.08,
            purr: 1.0,
            ..VoiceVisualState::default()
        });
        assert!(
            speaking > 0.34,
            "spoken aperture was visually closed: {speaking}"
        );
        assert!(quiet_purr < speaking);
        assert_eq!(voice_mouth_target(VoiceVisualState::default()), 0.0);
    }
}
