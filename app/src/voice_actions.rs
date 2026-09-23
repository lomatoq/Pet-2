use desktop_host::CueKind;
use glam::Vec2;
use lifecore::{BodyIntent, LocomotionMode, PoseIntent};
use pet_motor::BehaviorProgramId as P;

pub fn program(cue: CueKind) -> Option<P> {
    Some(match cue {
        CueKind::Blink => P::SocialSlowBlinkAffiliation,
        CueKind::Look | CueKind::Name => P::SocialMutualGazePulse,
        CueKind::Bow => P::PlayPlayBowAnalog,
        CueKind::Shake => P::DefensePostStressShakeOff,
        CueKind::Stretch => P::StateCuriousProbeBud,
        CueKind::Play => P::PlayCursorChaseBout,
        CueKind::Wake => P::RestRemDreamWake,
        _ => return None,
    })
}

#[derive(Clone, Debug)]
pub struct VoiceMotion {
    pub cue: CueKind,
    pub elapsed: f32,
    pub support_y: f32,
    pub autonomous_rest: bool,
    pub rest_drift: f32,
    origin: Vec2,
    target: Vec2,
    aspect: f32,
}
impl VoiceMotion {
    pub fn new(
        cue: CueKind,
        origin: Vec2,
        cursor: Vec2,
        floor: f32,
        home: Vec2,
        aspect: f32,
    ) -> Option<Self> {
        use CueKind::*;
        let aspect = aspect.max(0.25);
        let target = match cue {
            Sit | Sleep => Vec2::new(origin.x, floor),
            Come | Dash => cursor,
            Home => home,
            Up => origin - Vec2::Y * 0.20,
            Down => origin + Vec2::Y * 0.20,
            Left => origin - Vec2::X * (0.25 / aspect),
            Right => origin + Vec2::X * (0.25 / aspect),
            Circle | Jump | Stay => origin,
            _ => return None,
        };
        Some(Self {
            cue,
            elapsed: 0.0,
            support_y: floor,
            autonomous_rest: false,
            rest_drift: 0.0,
            origin,
            target,
            aspect,
        })
    }
    pub fn follow_supported_extent(&mut self, foot_normalized: f32) {
        if matches!(self.cue, CueKind::Sit | CueKind::Sleep) && foot_normalized.is_finite() {
            self.target.y = self.support_y - foot_normalized;
        }
    }
    pub fn support(&self, position: Vec2) -> Option<pet_motor::SurfaceAttachmentCommand> {
        if !matches!(self.cue, CueKind::Sit | CueKind::Sleep)
            || (position.y - self.target.y).abs() > 0.03
        {
            return None;
        }
        Some(pet_motor::SurfaceAttachmentCommand {
            surface_id: lifecore::SurfaceId("voice:taskbar".into()),
            anchor_point: Vec2::new(position.x, self.support_y),
            normal: -Vec2::Y,
            tangent: Vec2::X,
            target_contact_fraction: 0.30,
            normal_compliance: 0.65,
            tangent_friction: 0.8,
            adhesion: 0.0,
            load_fraction: 0.35,
            break_force: 0.75,
            release_half_life: 0.2,
        })
    }
    pub fn apply(&mut self, intent: &mut BodyIntent, cursor: Vec2, dt: f32) -> bool {
        use CueKind::*;
        self.elapsed += dt.max(0.0);
        let t = self.elapsed;
        let duration = match self.cue {
            Circle => 7.0,
            Sit | Sleep if self.autonomous_rest => 36.0,
            Sit | Sleep => 12.0,
            Stay => 8.0,
            Jump => 2.5,
            _ => 5.0,
        };
        if t > duration {
            return false;
        }
        intent.interaction_target = None;
        intent.target_surface = None;
        intent.locomotion = LocomotionMode::Arrive;
        intent.desired_speed = 0.75;
        intent.pose = PoseIntent::Curious;
        intent.expression.eye_aperture = 1.0;
        let mut target = self.target;
        match self.cue {
            Dash => {
                if t < 1.0 {
                    self.target = cursor;
                    target = self.origin;
                    intent.desired_speed = 0.0;
                } else {
                    target = self.target;
                    intent.desired_speed = 1.0;
                    intent.locomotion = LocomotionMode::Seek;
                    intent.pose = PoseIntent::Playful;
                }
            }
            Circle => {
                let phase = ((t / 6.0).min(1.0)) * std::f32::consts::TAU;
                let center = (self.origin - Vec2::new(0.10 / self.aspect, 0.0))
                    .clamp(Vec2::splat(0.22), Vec2::splat(0.78));
                target = center + Vec2::new(phase.cos() * 0.10 / self.aspect, phase.sin() * 0.10);
                intent.desired_speed = 0.85;
            }
            Jump => {
                target = self.origin
                    - Vec2::Y * (0.18 * (std::f32::consts::PI * (t / 1.8).min(1.0)).sin());
                intent.desired_speed = 1.0;
                intent.pose = PoseIntent::Playful;
            }
            Sit | Sleep => {
                intent.pose = if self.cue == Sleep {
                    PoseIntent::Sleeping
                } else {
                    PoseIntent::Neutral
                };
                intent.desired_speed = 0.45;
                if self.cue == Sleep {
                    intent.expression.eye_aperture = 0.08;
                }
            }
            Stay => {
                intent.desired_speed = 0.45;
                intent.pose = PoseIntent::Neutral;
            }
            _ => {}
        }
        if self.autonomous_rest {
            // A supported slow weight transfer, with long holds at either end.
            let drift_phase = ((t - 9.0).max(0.0) / 18.0).min(1.0) * std::f32::consts::TAU;
            target.x += (1.0 - drift_phase.cos()) * self.rest_drift / self.aspect;
            if t > 8.0 {
                intent.desired_speed = 0.18;
            }
            intent.expression.mouth_open = 0.0;
            intent.expression.brow_tension *= 0.3;
        }
        intent.target_position = target.clamp(Vec2::splat(0.03), Vec2::splat(0.97));
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn commanded_dash_reaches_fast_speed_and_circle_traverses_all_quadrants() {
        let genome = lifecore::Genome::from_seed(17);
        for cue in [CueKind::Dash, CueKind::Circle] {
            let origin = Vec2::splat(0.5);
            let mut motion = VoiceMotion::new(
                cue,
                origin,
                Vec2::new(0.85, 0.5),
                0.9,
                origin,
                1920.0 / 1080.0,
            )
            .unwrap();
            let mut body = pet_body::BodySimulation::new(17);
            body.set_motion_space_pixels(Vec2::new(1920.0, 1080.0));
            body.feedback.world_position = origin;
            let mut intent = BodyIntent {
                locomotion: LocomotionMode::Arrive,
                target_position: origin,
                target_surface: None,
                desired_speed: 0.0,
                facing_direction: 1.0,
                gaze_target: None,
                pose: PoseIntent::Neutral,
                expression: lifecore::ExpressionState::default(),
                interaction_target: None,
            };
            let mut max_speed = 0.0_f32;
            let mut quadrants = [false; 4];
            let center = origin - Vec2::new(0.10 / (1920.0 / 1080.0), 0.0);
            for _ in 0..840 {
                if !motion.apply(&mut intent, Vec2::new(0.85, 0.5), 1.0 / 120.0) {
                    break;
                }
                body.fixed_update(
                    &genome.body,
                    &intent,
                    &lifecore::SensorFrame::default(),
                    1.0 / 120.0,
                );
                max_speed =
                    max_speed.max((body.feedback.velocity * Vec2::new(1920.0, 1080.0)).length());
                let d = body.feedback.world_position - center;
                quadrants[usize::from(d.x > 0.0) + 2 * usize::from(d.y > 0.0)] = true;
            }
            if cue == CueKind::Dash {
                assert!(max_speed > 500.0, "{max_speed}");
            } else {
                assert!(quadrants.into_iter().all(|q| q), "{quadrants:?}");
            }
        }
    }
    #[test]
    fn every_command_has_a_real_handler() {
        for cue in CueKind::ALL {
            assert!(
                matches!(cue, CueKind::Name | CueKind::Quiet)
                    || program(cue).is_some()
                    || VoiceMotion::new(
                        cue,
                        Vec2::splat(0.5),
                        Vec2::splat(0.7),
                        0.9,
                        Vec2::splat(0.8),
                        1.8
                    )
                    .is_some(),
                "{cue:?}"
            );
        }
    }
}

/// A short, state-driven frustration discharge. No random animation selector.
#[derive(Default)]
pub struct EmotionBurst {
    pub active: bool,
    discharged: bool,
    elapsed: f32,
    cooldown: f32,
    origin: Vec2,
    direction: Vec2,
}
impl EmotionBurst {
    pub fn update(
        &mut self,
        frustration: f32,
        arousal: f32,
        allowed: bool,
        position: Vec2,
        aspect: f32,
        dt: f32,
    ) {
        self.cooldown = (self.cooldown - dt).max(0.0);
        if frustration < 0.45 {
            self.discharged = false;
        }
        if !allowed || frustration < 0.25 {
            self.active = false;
            return;
        }
        if !self.active
            && !self.discharged
            && self.cooldown <= 0.0
            && frustration > 0.65
            && arousal > 0.55
        {
            self.active = true;
            self.discharged = true;
            self.elapsed = 0.0;
            self.cooldown = 14.0;
            self.origin = position;
            self.direction = [
                (position.x * aspect, -Vec2::X / aspect),
                ((1.0 - position.x) * aspect, Vec2::X / aspect),
                (position.y, -Vec2::Y),
                (1.0 - position.y, Vec2::Y),
            ]
            .into_iter()
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .unwrap()
            .1;
        }
        if self.active {
            self.elapsed += dt;
            if self.elapsed >= 1.25 {
                self.active = false;
            }
        }
    }
    pub fn apply(&self, intent: &mut BodyIntent) {
        if !self.active {
            return;
        }
        // One bounded discharge per rise in frustration. Reversing the goal
        // 5.6 times a second used to inject contradictory flight accelerations.
        let phase = (self.elapsed / 1.25).clamp(0.0, 1.0);
        let excursion = (std::f32::consts::PI * phase).sin().powi(2);
        intent.target_position = (self.origin + self.direction * (0.045 * excursion))
            .clamp(Vec2::splat(0.02), Vec2::splat(0.98));
        intent.locomotion = LocomotionMode::Seek;
        intent.desired_speed = 0.20 + 0.30 * excursion;
        intent.pose = PoseIntent::Compact;
        intent.interaction_target = None;
        intent.expression.brow_tension = intent.expression.brow_tension.max(0.7);
        intent.expression.eye_aperture = 0.75;
    }
}

#[cfg(test)]
mod emotion_tests {
    use super::*;
    #[test]
    fn sustained_frustration_does_not_repeat_or_flip_the_flight_target() {
        let mut burst = EmotionBurst::default();
        let origin = Vec2::splat(0.5);
        let mut previous = origin;
        for step in 0..4000 {
            burst.update(0.9, 0.9, true, origin, 1.8, 0.01);
            if step > 125 {
                assert!(!burst.active, "persistent frustration retriggered");
            }
            if burst.active {
                let mut intent = lifecore::BodyIntent {
                    target_position: origin,
                    target_surface: None,
                    desired_speed: 0.0,
                    facing_direction: 1.0,
                    locomotion: LocomotionMode::Hover,
                    gaze_target: None,
                    pose: PoseIntent::Neutral,
                    expression: lifecore::ExpressionState::default(),
                    interaction_target: None,
                };
                burst.apply(&mut intent);
                assert!(intent.target_position.distance(previous) < 0.003);
                assert!(intent.target_position.distance(origin) <= 0.046);
                assert!(intent.desired_speed <= 0.501);
                previous = intent.target_position;
            }
        }
        assert!(burst.discharged);
        burst.update(0.3, 0.5, true, origin, 1.8, 0.01);
        burst.update(0.9, 0.9, true, origin, 1.8, 0.01);
        assert!(burst.active);
    }
    #[test]
    fn frustration_requires_context_and_releases_with_a_refractory_period() {
        let mut burst = EmotionBurst::default();
        burst.update(0.9, 0.9, false, Vec2::splat(0.5), 1.8, 0.01);
        assert!(!burst.active);
        burst.update(0.9, 0.9, true, Vec2::splat(0.5), 1.8, 0.01);
        assert!(burst.active);
        for _ in 0..300 {
            burst.update(0.9, 0.9, true, Vec2::splat(0.5), 1.8, 0.01);
        }
        assert!(!burst.active);
        assert!(burst.cooldown > 10.0);
    }
}

#[derive(Default)]
pub struct ActivityRecovery {
    effort: f32,
    available_seconds: f32,
}
impl ActivityRecovery {
    pub fn update(
        &mut self,
        speed: f32,
        fatigue: f32,
        arousal: f32,
        available: bool,
        dt: f32,
    ) -> bool {
        self.available_seconds = if available {
            self.available_seconds + dt
        } else {
            0.0
        };
        let expenditure = if speed > 0.07 {
            0.35 + speed.min(1.5) * 2.0
        } else {
            -0.8
        };
        self.effort = (self.effort + expenditure * dt).clamp(0.0, 60.0);
        let budget = 14.0 + (1.0 - fatigue.clamp(0.0, 1.0)) * 12.0 + arousal.clamp(0.0, 1.0) * 6.0;
        let tired_idle = fatigue > 0.65 && self.available_seconds > 32.0 + arousal * 12.0;
        if available && (self.effort >= budget || tired_idle) {
            self.available_seconds = 0.0;
            self.effort = 0.0;
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod recovery_tests {
    use super::*;
    #[test]
    fn exertion_or_tired_idle_requests_rest_without_parking_an_awake_pet() {
        let mut recovery = ActivityRecovery::default();
        for _ in 0..100 {
            assert!(!recovery.update(0.0, 0.5, 0.5, true, 0.1));
        }
        for _ in 0..600 {
            assert!(!recovery.update(0.5, 0.5, 0.5, false, 0.1));
        }
        assert!(recovery.update(0.5, 0.5, 0.5, true, 0.1));
        assert!(!recovery.update(0.0, 0.5, 0.5, true, 0.1));
        for _ in 0..6000 {
            assert!(!recovery.update(0.0, 0.5, 0.5, true, 0.1));
        }
        assert!(recovery.update(0.0, 0.75, 0.5, true, 0.1));
    }
}

/// Anticipation is attached to a locomotor decision, not a repeating idle animation.
#[derive(Default)]
pub struct LaunchPreparation {
    remaining: f32,
    duration: f32,
    cooldown: f32,
    target: Vec2,
    axis: Vec2,
}
impl LaunchPreparation {
    pub fn apply(
        &mut self,
        intent: &mut BodyIntent,
        position: Vec2,
        speed: f32,
        blocked: bool,
        dt: f32,
    ) -> (Vec2, f32) {
        self.cooldown = (self.cooldown - dt).max(0.0);
        if blocked {
            self.remaining = 0.0;
            return (Vec2::ZERO, 0.0);
        }
        if self.remaining <= 0.0
            && self.cooldown <= 0.0
            && speed < 0.08
            && intent.desired_speed >= 0.65
            && intent.target_position.distance(position) > 0.09
        {
            self.duration = (0.10
                + intent.desired_speed * 0.12
                + intent.target_position.distance(position) * 0.12)
                .clamp(0.14, 0.32);
            self.remaining = self.duration;
            self.cooldown = 2.5;
            self.target = intent.target_position;
            self.axis = (self.target - position).normalize_or_zero();
        }
        if self.remaining > 0.0 {
            self.remaining = (self.remaining - dt).max(0.0);
            if self.remaining > 0.0 {
                intent.target_position = position;
                intent.desired_speed = 0.0;
                intent.pose = PoseIntent::Compact;
                return (
                    self.axis,
                    (std::f32::consts::PI * (1.0 - self.remaining / self.duration)).sin(),
                );
            }
            intent.target_position = self.target;
            intent.desired_speed = 0.9;
        }
        (self.axis, 0.0)
    }
}

#[derive(Default)]
pub struct ContextGlance {
    cursor: Vec2,
    still: f32,
    urge: f32,
    remaining: f32,
    target: Vec2,
    entropy: u32,
}
impl ContextGlance {
    fn variation(&mut self) -> f32 {
        self.entropy = self.entropy.wrapping_mul(1664525).wrapping_add(1013904223);
        (self.entropy >> 8) as f32 / 16_777_216.0
    }
    pub fn target(
        &mut self,
        cursor: Vec2,
        position: Vec2,
        resting: bool,
        blocked: bool,
        dt: f32,
    ) -> Option<Vec2> {
        if cursor.distance(self.cursor) > 0.004 {
            self.still = 0.0;
        } else {
            self.still += dt;
        }
        self.cursor = cursor;
        if blocked {
            self.remaining = 0.0;
            self.urge *= (-dt).exp();
            return None;
        }
        let opportunity = if resting {
            0.40
        } else {
            ((self.still - 4.0) / 25.0).clamp(0.0, 0.16)
        };
        self.urge += opportunity * dt;
        if self.remaining > 0.0 {
            self.remaining -= dt;
            return Some(self.target);
        }
        if self.urge < 1.0 {
            return None;
        }
        self.urge = self.variation() * 0.25;
        self.remaining = 0.45 + self.variation() * 0.65;
        let angle = self.variation() * std::f32::consts::TAU;
        let reach = 0.06 + self.variation() * 0.09;
        self.target = position + Vec2::from_angle(angle) * reach;
        Some(self.target)
    }
}

#[cfg(test)]
mod contextual_tests {
    use super::*;
    #[test]
    fn preparation_holds_then_releases_without_repeating_during_motion() {
        let mut state = LaunchPreparation::default();
        let mut intent = BodyIntent {
            target_position: Vec2::new(0.7, 0.5),
            desired_speed: 0.9,
            locomotion: LocomotionMode::Arrive,
            target_surface: None,
            facing_direction: 1.0,
            gaze_target: None,
            pose: PoseIntent::Neutral,
            expression: lifecore::ExpressionState::default(),
            interaction_target: None,
        };
        let origin = Vec2::splat(0.5);
        assert!(state.apply(&mut intent, origin, 0.0, false, 0.02).1 > 0.0);
        assert_eq!(intent.desired_speed, 0.0);
        for _ in 0..20 {
            state.apply(&mut intent, origin, 0.0, false, 0.02);
        }
        assert!(intent.target_position.x > 0.6);
        assert_eq!(state.apply(&mut intent, origin, 0.5, false, 0.02).1, 0.0);
    }
    #[test]
    fn looking_around_requires_an_opportunity_and_yields_to_engagement() {
        let mut state = ContextGlance::default();
        let mut glanced = false;
        for _ in 0..1000 {
            glanced |= state
                .target(Vec2::ZERO, Vec2::splat(0.5), true, false, 0.02)
                .is_some();
        }
        assert!(glanced);
        assert!(
            state
                .target(Vec2::ZERO, Vec2::splat(0.5), true, true, 0.02)
                .is_none()
        );
    }
}
