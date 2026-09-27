//! Smooth native routes for causal excitement bouts; physics retains position ownership.
use glam::Vec2;
use lifecore::{
    BodyIntent, ExcitationCue, ExcitationFeedback, ExcitationMotorFrame, ExcitationTactic,
    LocomotionMode, PoseIntent,
};

#[derive(Debug, Clone)]
pub struct ExcitationRuntime {
    pub executing: bool,
    active: bool,
    tactic: ExcitationTactic,
    anchor: Vec2,
    target: Vec2,
    previous_position: Option<Vec2>,
    requested_direction: Vec2,
    speed: f32,
    cue_window: Option<CueWindow>,
    last_event_id: u64,
}
#[derive(Debug, Clone, Copy)]
struct CueWindow {
    cue: Option<ExcitationCue>,
    tactic: Option<ExcitationTactic>,
    arousal: f32,
    elapsed: f32,
    exposure: f32,
    pleasantness: f32,
    baseline_pleasantness: f32,
    movement: f32,
    position: Vec2,
    emitted: bool,
}
impl Default for ExcitationRuntime {
    fn default() -> Self {
        Self {
            executing: false,
            active: false,
            tactic: ExcitationTactic::Dash,
            anchor: Vec2::splat(0.5),
            target: Vec2::splat(0.5),
            previous_position: None,
            requested_direction: Vec2::ZERO,
            speed: 0.0,
            cue_window: None,
            last_event_id: 0,
        }
    }
}
impl ExcitationRuntime {
    /// Returns ownership, not proof of execution. `executing` is measured
    /// from movement along the previously requested direction, not this request.
    pub fn apply(
        &mut self,
        frame: ExcitationMotorFrame,
        position: Vec2,
        dt: f32,
        intent: &mut BodyIntent,
    ) -> bool {
        let dt = finite(dt).clamp(0.0, 0.05);
        if !position.is_finite()
            || !frame.intensity.is_finite()
            || frame.intensity <= 0.02
            || dt == 0.0
        {
            self.active = false;
            self.executing = false;
            self.previous_position = None;
            self.speed = 0.0;
            return false;
        }
        let observed = self
            .previous_position
            .map_or(Vec2::ZERO, |old| position - old);
        self.executing = self.active
            && observed.length() < 0.10
            && observed.dot(self.requested_direction) > dt * 0.003;
        self.previous_position = Some(position);
        if !self.active || self.tactic != frame.tactic {
            self.anchor = position.clamp(Vec2::splat(0.10), Vec2::splat(0.90));
            self.target = self.anchor;
            self.speed = 0.0;
            self.executing = false;
        }
        self.active = true;
        self.tactic = frame.tactic;
        let intensity = frame.intensity.clamp(0.0, 1.0);
        let phase = finite(frame.phase).rem_euclid(1.0);
        let turn = finite(frame.turn_bias).clamp(-1.0, 1.0);
        let side = if turn.abs() > 0.02 {
            turn.signum()
        } else if self.anchor.x > 0.5 {
            -1.0
        } else {
            1.0
        };
        let width = finite(frame.arc_width).clamp(0.0, 1.0);
        let radius = 0.055 + width * 0.14;
        // A little look-ahead initiates a route even before measured movement
        // allows the organism's phase to advance. It never advances phase here.
        let angle = (phase + 0.045) * std::f32::consts::TAU;
        let raw = match frame.tactic {
            ExcitationTactic::Dash => {
                self.anchor + Vec2::new(side, turn * 0.28) * angle.sin() * radius
            }
            ExcitationTactic::Orbit => {
                self.anchor
                    + Vec2::new(
                        side * (angle.cos() - 1.0) * radius * 0.65,
                        angle.sin() * radius * 0.55,
                    )
            }
            ExcitationTactic::Zigzag => {
                self.anchor
                    + Vec2::new(
                        side * angle.sin() * radius,
                        (angle * 2.0).sin() * radius * 0.38,
                    )
            }
            ExcitationTactic::Roll => {
                self.anchor + Vec2::new(side * angle.sin() * radius, angle.sin() * radius * 0.08)
            }
            ExcitationTactic::WallBounce => {
                let offset = ((self.anchor.x - 0.5) / 0.39).clamp(-1.0, 1.0).asin();
                Vec2::new(
                    0.5 + 0.39 * (angle * side + offset).sin(),
                    self.anchor.y + (angle * 2.0).sin() * radius * 0.22,
                )
            }
        };
        // Reflect unavailable portions into the free interior. Merely clamping
        // an outward initial target at a corner deadlocks measured phase zero.
        let reflect = |value: f32| {
            let folded = (value - 0.10).rem_euclid(1.60);
            0.10 + if folded <= 0.80 {
                folded
            } else {
                1.60 - folded
            }
        };
        let raw = Vec2::new(reflect(raw.x), reflect(raw.y));
        let target_step = (raw - self.target).clamp_length_max((0.10 + intensity * 0.30) * dt);
        self.target = (self.target + target_step).clamp(Vec2::splat(0.10), Vec2::splat(0.90));
        let desired = (0.12 + intensity * 0.36) * finite(frame.speed_scale).clamp(0.5, 1.6);
        let soft = finite(frame.bounce_softness).clamp(0.0, 1.0);
        self.speed +=
            (desired - self.speed).clamp(-(0.8 + soft) * dt, (0.5 + intensity * 0.7) * dt);
        self.requested_direction = (self.target - position).normalize_or_zero();
        intent.target_position = self.target;
        intent.target_surface = None;
        intent.locomotion = LocomotionMode::Arrive;
        intent.desired_speed = self.speed.clamp(0.0, 0.78);
        intent.pose = PoseIntent::Playful;
        // Gaze is still owned by current action/attention; do not snap the face
        // to a synthetic screen coordinate on every trajectory sample.
        true
    }

    /// One evidence sample per sustained cue. Repeated held contact cannot
    /// reinforce itself every second; it must end/change before another sample.
    pub fn observe(
        &mut self,
        cue: Option<ExcitationCue>,
        strength: f32,
        arousal: f32,
        pleasantness: f32,
        position: Vec2,
        dt: f32,
    ) -> Option<ExcitationFeedback> {
        let dt = finite(dt).clamp(0.0, 0.25);
        if !position.is_finite() || dt == 0.0 {
            return None;
        }
        let strength = finite(strength).clamp(0.0, 1.0);
        let cue = cue.filter(|_| strength > 0.0);
        let tactic = self.active.then_some(self.tactic);
        if cue.is_none() && tactic.is_none() {
            self.cue_window = None;
            return None;
        }
        // A brief stationary pause must not reset an already measured tactic's
        // window or permit duplicate reinforcement while the same cue is held.
        let same = self
            .cue_window
            .is_some_and(|w| w.cue == cue && (cue.is_some() || w.tactic == tactic));
        if !same {
            self.cue_window = Some(CueWindow {
                cue,
                tactic,
                arousal: finite(arousal).clamp(0.0, 1.0),
                elapsed: 0.0,
                exposure: 0.0,
                pleasantness: 0.0,
                baseline_pleasantness: finite(pleasantness).clamp(-1.0, 1.0),
                movement: 0.0,
                position,
                emitted: false,
            });
        }
        let w = self.cue_window.as_mut()?;
        if w.emitted {
            return None;
        }
        w.elapsed += dt;
        let weight = if cue.is_some() {
            strength
        } else {
            f32::from(self.executing)
        };
        w.exposure += weight * dt;
        w.pleasantness += finite(pleasantness).clamp(-1.0, 1.0) * weight * dt;
        let step = position.distance(w.position);
        if self.executing && step < 0.10 {
            w.movement += step;
            w.tactic = tactic;
        }
        w.position = position;
        if w.elapsed < 0.85 || w.exposure < 0.50 {
            return None;
        }
        w.emitted = true;
        let id = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(1, |t| {
                (t.as_micros().min(u128::from(u64::MAX)) as u64).max(1)
            });
        self.last_event_id = id.max(self.last_event_id.saturating_add(1));
        Some(ExcitationFeedback {
            event_id: self.last_event_id,
            cue: w.cue,
            tactic: w.tactic.filter(|_| w.movement >= 0.015),
            arousal_delta: (finite(arousal) - w.arousal).clamp(-1.0, 1.0),
            pleasantness: ((w.pleasantness / w.exposure.max(0.001) - w.baseline_pleasantness)
                * 2.0)
                .clamp(-1.0, 1.0),
        })
    }
}
fn finite(value: f32) -> f32 {
    if value.is_finite() {
        value
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn intent() -> BodyIntent {
        BodyIntent {
            locomotion: LocomotionMode::Hover,
            target_position: Vec2::splat(0.5),
            target_surface: None,
            desired_speed: 0.0,
            facing_direction: 0.0,
            gaze_target: Some(Vec2::splat(0.4)),
            pose: PoseIntent::Neutral,
            expression: Default::default(),
            interaction_target: None,
        }
    }
    #[test]
    fn routes_are_bounded_continuous_and_never_teleport_or_claim_frozen_execution() {
        for tactic in ExcitationTactic::ALL {
            let mut runtime = ExcitationRuntime::default();
            let mut intent = intent();
            let position = Vec2::splat(0.5);
            let mut previous = position;
            for i in 0..600 {
                let frame = ExcitationMotorFrame {
                    intensity: 1.0,
                    tactic,
                    phase: i as f32 / 600.0,
                    speed_scale: 1.5,
                    arc_width: 0.8,
                    turn_bias: 0.6,
                    ..Default::default()
                };
                assert!(runtime.apply(frame, position, 1.0 / 120.0, &mut intent));
                assert!(
                    intent.target_position.min_element() >= 0.10
                        && intent.target_position.max_element() <= 0.90
                );
                assert!(intent.target_position.distance(previous) <= 0.004);
                assert!(!runtime.executing);
                assert!(intent.desired_speed <= 0.78);
                assert_eq!(intent.gaze_target, Some(Vec2::splat(0.4)));
                previous = intent.target_position;
            }
            let before = intent.clone();
            assert!(!runtime.apply(Default::default(), position, 1.0 / 120.0, &mut intent));
            assert_eq!(intent, before);
            assert!(!runtime.executing);
        }
    }
    #[test]
    fn accepted_motion_is_measured_and_a_held_cue_is_learned_only_once() {
        let mut r = ExcitationRuntime::default();
        let mut intent = intent();
        let mut position = Vec2::splat(0.5);
        let frame = ExcitationMotorFrame {
            intensity: 1.0,
            speed_scale: 1.0,
            arc_width: 0.8,
            ..Default::default()
        };
        let mut events = Vec::new();
        for i in 0..400 {
            r.apply(frame, position, 0.02, &mut intent);
            position += (intent.target_position - position).clamp_length_max(0.001);
            if let Some(event) = r.observe(
                Some(ExcitationCue::QuickPlay),
                1.0,
                i as f32 * 0.001,
                0.7,
                position,
                0.02,
            ) {
                events.push(event);
            }
        }
        assert_eq!(events.len(), 1);
        assert!(events[0].event_id > 0);
        assert_eq!(events[0].tactic, Some(ExcitationTactic::Dash));
        assert!(events[0].arousal_delta > 0.0);
        r.apply(Default::default(), position, 0.02, &mut intent);
        r.observe(None, 0.0, 0.4, 0.0, position, 0.02);
        let mut second = None;
        for _ in 0..60 {
            second = second.or(r.observe(
                Some(ExcitationCue::GentleStroke),
                1.0,
                0.3,
                0.8,
                position,
                0.02,
            ));
        }
        assert!(second.is_some());
        assert!(second.unwrap().event_id > events[0].event_id);
        assert_eq!(second.unwrap().tactic, None);
    }
    #[test]
    fn realistic_play_moves_and_recovery_releases_native_ownership() {
        use lifecore::{AffectState, Drives, ExcitationInput, ExcitationState, Genome};
        let genome = Genome::from_seed(4);
        let mut drives = Drives::initial(&genome.temperament);
        drives.sleep = 0.3;
        drives.safety = 0.0;
        let mut core = ExcitationState::default();
        let mut runtime = ExcitationRuntime::default();
        let mut body = intent();
        let mut position = Vec2::splat(0.5);
        let mut distance = 0.0;
        let mut acquired = false;
        let dt = 0.02;
        for i in 0..12000 {
            let input = ExcitationInput {
                play_engagement: if i < 2000 { 0.75 } else { 0.0 },
                energy: 0.72,
                executing: runtime.executing,
                ..Default::default()
            };
            let frame = core.tick(
                input,
                &genome.temperament,
                &mut drives,
                AffectState {
                    stress: 0.1,
                    valence: 0.4,
                    ..Default::default()
                },
                4,
                dt,
            );
            if runtime.apply(frame, position, dt, &mut body) {
                acquired = true;
                let step =
                    (body.target_position - position).clamp_length_max(body.desired_speed * dt);
                position += step;
                distance += step.length();
            }
        }
        assert!(acquired, "moderate real play must recruit a bout");
        assert!(distance > 0.1, "recruited route must actually move");
        assert!(
            !runtime.active && !runtime.executing,
            "recovered bout must relinquish locomotion"
        );
        let before = body.clone();
        assert!(!runtime.apply(
            ExcitationMotorFrame {
                intensity: 0.019,
                ..Default::default()
            },
            position,
            dt,
            &mut body
        ));
        assert_eq!(body, before);
    }
    #[test]
    fn an_already_happy_baseline_does_not_reward_a_tactic() {
        let mut runtime = ExcitationRuntime::default();
        let mut body = intent();
        let mut position = Vec2::splat(0.5);
        let mut event = None;
        for _ in 0..100 {
            runtime.apply(
                ExcitationMotorFrame {
                    intensity: 1.0,
                    speed_scale: 1.0,
                    arc_width: 0.8,
                    ..Default::default()
                },
                position,
                0.02,
                &mut body,
            );
            position += (body.target_position - position).clamp_length_max(0.001);
            event = event.or(runtime.observe(
                Some(ExcitationCue::QuickPlay),
                1.0,
                0.7,
                0.8,
                position,
                0.02,
            ));
        }
        let event = event.expect("sustained real play should be observed");
        assert!(event.tactic.is_some());
        assert!(
            event.pleasantness.abs() < 0.00001,
            "baseline happiness is not a tactic outcome"
        );
    }
    #[test]
    fn corner_routes_start_without_a_synthetic_phase_clock() {
        for tactic in ExcitationTactic::ALL {
            for x in [0.1, 0.9] {
                for y in [0.1, 0.9] {
                    for turn in [-0.8, 0.8] {
                        let mut runtime = ExcitationRuntime::default();
                        let mut body = intent();
                        let mut position = Vec2::new(x, y);
                        let start = position;
                        let mut phase = 0.0;
                        let mut moved = false;
                        for _ in 0..200 {
                            runtime.apply(
                                ExcitationMotorFrame {
                                    intensity: 0.6,
                                    tactic,
                                    phase,
                                    speed_scale: 1.2,
                                    arc_width: 0.8,
                                    turn_bias: turn,
                                    ..Default::default()
                                },
                                position,
                                0.02,
                                &mut body,
                            );
                            if runtime.executing {
                                phase = (phase + 0.005) % 1.0;
                                moved = true;
                            }
                            position += (body.target_position - position)
                                .clamp_length_max(body.desired_speed * 0.02);
                        }
                        assert!(
                            moved && position.distance(start) > 0.005,
                            "{tactic:?} corner {start:?} direction {turn} is stuck"
                        );
                    }
                }
            }
        }
    }
}
