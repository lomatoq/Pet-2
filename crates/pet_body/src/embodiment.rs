use glam::Vec2;
use lifecore::{
    AffectState, BodyFeedback, BodyGenome, BodyIntent, InteractionTarget, PoseIntent, SensorFrame,
};
use serde::{Deserialize, Serialize};

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
    pub vergence: f32,
    pub pupil_size: f32,
    pub blink_left: f32,
    pub blink_right: f32,
    pub squint: f32,
    pub brow_raise: f32,
    pub brow_tension: f32,
    pub brow_asymmetry: f32,
    pub mouth_open: f32,
    pub mouth_curve: f32,
    pub mouth_tension: f32,
    pub cheek_glow: f32,
    pub squash: Vec2,
    pub tilt: f32,
    pub head_lag: Vec2,
    pub tail_lag: Vec2,
    pub breath: f32,
    pub compression: f32,
    pub audio_envelope: f32,
    pub purr: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EmbodiedRuntime {
    pub pose: EmbodiedPose,
    seed_phase: f32,
    elapsed: f32,
    fixation_elapsed: f32,
    fixation_duration: f32,
    gaze_target: Vec2,
    gaze_velocity: Vec2,
    saccade_strength: f32,
    blink_phase: f32,
    blink_kind: BlinkKind,
    blink_clock: f32,
    next_blink: f32,
    slow_blink_cooldown: f32,
    wink_cooldown: f32,
    soft_velocity: Vec2,
    head_velocity: Vec2,
    tail_velocity: Vec2,
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
    pub fn new(seed: u64) -> Self {
        let seed_phase = (seed as u32 as f32 / u32::MAX as f32) * std::f32::consts::TAU;
        Self {
            pose: EmbodiedPose {
                pupil_size: 0.52,
                squash: Vec2::ONE,
                breath: 0.5,
                ..EmbodiedPose::default()
            },
            seed_phase,
            elapsed: 0.0,
            fixation_elapsed: 0.0,
            fixation_duration: 0.7 + seed_phase.sin().abs() * 0.8,
            gaze_target: Vec2::ZERO,
            gaze_velocity: Vec2::ZERO,
            saccade_strength: 0.0,
            blink_phase: 0.0,
            blink_kind: BlinkKind::None,
            blink_clock: 0.0,
            next_blink: 1.8 + seed_phase.cos().abs() * 3.2,
            slow_blink_cooldown: 2.0,
            wink_cooldown: 3.0,
            soft_velocity: Vec2::ZERO,
            head_velocity: Vec2::ZERO,
            tail_velocity: Vec2::ZERO,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn update(
        &mut self,
        genome: &BodyGenome,
        intent: &BodyIntent,
        sensors: &SensorFrame,
        feedback: &BodyFeedback,
        affect: AffectState,
        expression: lifecore::ExpressionState,
        voice: VoiceVisualState,
        dt: f32,
    ) {
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
        self.update_gaze(mode, desired_gaze, affect, dt);
        self.update_blink(mode, intent, affect, expression, dt);
        self.update_soft_body(genome, intent, feedback, affect, dt);

        let voice_mouth = if voice.active {
            voice.mouth_open.clamp(0.0, 1.0) * voice.envelope.clamp(0.0, 1.0)
        } else {
            0.0
        };
        let emotional_mouth = expression.mouth_open.clamp(0.0, 1.0) * 0.62;
        let mouth_target = voice_mouth.max(emotional_mouth);
        self.pose.mouth_open = smooth(self.pose.mouth_open, mouth_target, 22.0, dt);
        self.pose.mouth_curve = smooth(
            self.pose.mouth_curve,
            (expression.mouth_curve + affect.valence * 0.34).clamp(-1.0, 1.0),
            11.0,
            dt,
        );
        self.pose.mouth_tension = smooth(
            self.pose.mouth_tension,
            (expression.mouth_tension + affect.frustration * 0.35).clamp(0.0, 1.0),
            13.0,
            dt,
        );
        self.pose.brow_raise = smooth(
            self.pose.brow_raise,
            (expression.brow_raise + affect.arousal * 0.22).clamp(-1.0, 1.0),
            12.0,
            dt,
        );
        self.pose.brow_tension = smooth(
            self.pose.brow_tension,
            (expression.brow_tension + affect.stress * 0.36).clamp(0.0, 1.0),
            14.0,
            dt,
        );
        let asymmetry = (self.elapsed * 0.41 + self.seed_phase).sin() * 0.12
            + if intent.pose == PoseIntent::Curious {
                0.18
            } else {
                0.0
            };
        self.pose.brow_asymmetry = smooth(self.pose.brow_asymmetry, asymmetry, 7.0, dt);
        self.pose.squint = smooth(
            self.pose.squint,
            (expression.squint + affect.stress * 0.28).clamp(0.0, 1.0),
            13.0,
            dt,
        );
        let luminance = sensors
            .local_luminance
            .or(sensors.mean_luminance)
            .unwrap_or(0.5);
        let light_response = (0.65 - luminance).clamp(-0.35, 0.35);
        let pupil_target =
            (0.43 + affect.arousal * 0.27 + light_response * 0.42 + self.saccade_strength * 0.08
                - affect.stress * 0.08)
                .clamp(0.22, 0.86);
        self.pose.pupil_size = smooth(self.pose.pupil_size, pupil_target, 8.0, dt);
        self.pose.cheek_glow = smooth(
            self.pose.cheek_glow,
            (expression.cheek_glow * 0.7 + affect.attachment * 0.38).clamp(0.0, 1.0),
            5.0,
            dt,
        );
        self.pose.audio_envelope = smooth(self.pose.audio_envelope, voice.envelope, 28.0, dt);
        self.pose.purr = smooth(self.pose.purr, voice.purr, 18.0, dt);
    }

    fn update_gaze(&mut self, mode: GazeMode, desired: Vec2, affect: AffectState, dt: f32) {
        self.fixation_elapsed += dt;
        let target_distance = desired.distance(self.gaze_target);
        let should_saccade = target_distance > 0.12
            || self.fixation_elapsed >= self.fixation_duration
            || matches!(mode, GazeMode::DirectViewer | GazeMode::Sleep)
                && self.pose.gaze_mode != mode;
        if should_saccade {
            self.gaze_target = desired.clamp(Vec2::splat(-0.92), Vec2::splat(0.92));
            self.fixation_elapsed = 0.0;
            self.fixation_duration = (0.45
                + 1.35 * (0.5 + 0.5 * (self.elapsed * 0.71 + self.seed_phase).sin())
                + affect.stress * 0.28)
                .clamp(0.28, 2.2);
            self.saccade_strength = target_distance.clamp(0.0, 1.0);
        }
        self.saccade_strength = (self.saccade_strength - dt * 5.5).max(0.0);
        let microsaccade = Vec2::new(
            (self.elapsed * 13.7 + self.seed_phase).sin(),
            (self.elapsed * 11.3 + self.seed_phase * 1.7).cos(),
        ) * (0.003 + affect.arousal * 0.0035);
        let target = if mode == GazeMode::Sleep {
            Vec2::ZERO
        } else {
            self.gaze_target + microsaccade
        };
        let frequency = 28.0 + self.saccade_strength * 38.0;
        spring_vec2(
            &mut self.pose.gaze,
            &mut self.gaze_velocity,
            target,
            frequency,
            dt,
        );
        self.pose.gaze = self.pose.gaze.clamp(Vec2::splat(-0.95), Vec2::splat(0.95));
        self.pose.gaze_mode = mode;
        self.pose.vergence = if mode == GazeMode::DirectViewer {
            0.08
        } else {
            (1.0 - self.pose.gaze.length()).clamp(0.0, 1.0) * 0.035
        };
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
            } else if self.blink_clock >= self.next_blink
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
        let velocity = feedback.velocity;
        let acceleration = feedback.acceleration;
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

        let tilt_target = (-velocity.x * 0.34 - acceleration.x * 0.10).clamp(-0.42, 0.42);
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
    }
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
        (phase * std::f32::consts::PI).sin().powf(0.72)
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

fn smooth(current: f32, target: f32, speed: f32, dt: f32) -> f32 {
    current + (target - current) * (1.0 - (-speed * dt).exp())
}

fn smoothstep(edge0: f32, edge1: f32, value: f32) -> f32 {
    let t = ((value - edge0) / (edge1 - edge0).max(f32::EPSILON)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
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
        let mut runtime = EmbodiedRuntime::new(genome.identity_seed);
        let feedback = BodyFeedback {
            velocity: Vec2::new(0.8, -0.2),
            acceleration: Vec2::new(0.4, 0.1),
            ..BodyFeedback::default()
        };
        let sensors = SensorFrame::default();
        for _ in 0..10_000 {
            runtime.update(
                &genome.body,
                &intent(),
                &sensors,
                &feedback,
                AffectState::default(),
                lifecore::ExpressionState::default(),
                VoiceVisualState::default(),
                1.0 / 120.0,
            );
            assert!(runtime.pose.gaze.is_finite());
            assert!(runtime.pose.squash.is_finite());
            assert!(runtime.pose.squash.min_element() > 0.55);
            assert!(runtime.pose.squash.max_element() < 1.65);
        }
    }

    #[test]
    fn viewer_target_produces_direct_gaze() {
        let genome = Genome::from_seed(9);
        let mut runtime = EmbodiedRuntime::new(genome.identity_seed);
        let mut display = intent();
        display.pose = PoseIntent::Display;
        display.interaction_target = Some(InteractionTarget::User);
        for _ in 0..240 {
            runtime.update(
                &genome.body,
                &display,
                &SensorFrame::default(),
                &BodyFeedback::default(),
                AffectState::default(),
                display.expression,
                VoiceVisualState::default(),
                1.0 / 120.0,
            );
        }
        assert_eq!(runtime.pose.gaze_mode, GazeMode::DirectViewer);
        assert!(runtime.pose.gaze.length() < 0.08);
    }
}
