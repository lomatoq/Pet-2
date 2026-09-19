use glam::Vec2;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum GazeMode {
    #[default]
    Track,
    Inspect,
    SocialReference,
    MutualGaze,
    ContactMonitor,
    PredictiveIntercept,
    AvoidantCheck,
    Drowsy,
    Sleep,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct GazePlan {
    pub primary_target: Option<Vec2>,
    pub secondary_target: Option<Vec2>,
    pub target_velocity: Vec2,
    pub mode: GazeMode,
    pub acquire_tau: f32,
    pub release_tau: f32,
    pub dwell_min: f32,
    pub dwell_max: f32,
    pub lead_seconds: f32,
    pub micro_saccade_amplitude: f32,
    pub confidence: f32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct GazeOutput {
    pub target: Option<Vec2>,
    pub fixation_strength: f32,
    pub pupil_focus: f32,
    /// True only when the semantic fixation owner accepts a new target.
    pub fixation_started: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GazeController {
    current: Vec2,
    has_current: bool,
    dwell_seconds: f32,
    checkback_phase: bool,
    accepted_target: Option<Vec2>,
    pending_target: Option<Vec2>,
    pending_seconds: f32,
}

impl Default for GazeController {
    fn default() -> Self {
        Self {
            current: Vec2::splat(0.5),
            has_current: false,
            dwell_seconds: 0.0,
            checkback_phase: false,
            accepted_target: None,
            pending_target: None,
            pending_seconds: 0.0,
        }
    }
}

impl GazeController {
    #[must_use]
    pub const fn accepted_target(&self) -> Option<Vec2> {
        self.accepted_target
    }

    #[must_use]
    pub fn tick(&mut self, plan: GazePlan, dt: f32) -> GazeOutput {
        let dt = finite_dt(dt);
        if plan.mode == GazeMode::Sleep {
            self.has_current = false;
            self.dwell_seconds = 0.0;
            self.accepted_target = None;
            self.pending_target = None;
            return GazeOutput::default();
        }
        let primary = plan.primary_target.filter(|point| point.is_finite());
        let secondary = plan.secondary_target.filter(|point| point.is_finite());
        let target = match plan.mode {
            GazeMode::SocialReference if self.checkback_phase => secondary.or(primary),
            GazeMode::PredictiveIntercept => primary.map(|point| {
                (point + plan.target_velocity * plan.lead_seconds.clamp(0.0, 0.20))
                    .clamp(Vec2::ZERO, Vec2::ONE)
            }),
            _ => primary.or(secondary),
        };
        let Some(mut target) = target else {
            self.dwell_seconds = 0.0;
            return GazeOutput {
                target: if self.has_current {
                    Some(self.current)
                } else {
                    None
                },
                fixation_strength: 0.0,
                pupil_focus: 0.35,
                fixation_started: false,
            };
        };
        target = target.clamp(Vec2::ZERO, Vec2::ONE);
        // A one-frame attention change is not a new fixation. Confirm distant
        // ordinary targets briefly; tracking/interception and avoidance retain
        // their immediate response to meaningful motion or danger.
        let immediate = matches!(
            plan.mode,
            GazeMode::PredictiveIntercept | GazeMode::AvoidantCheck
        );
        let accepted_before = self.accepted_target;
        if let Some(accepted) = accepted_before {
            if !immediate && accepted.distance(target) > 0.12 {
                if self
                    .pending_target
                    .is_some_and(|pending| pending.distance(target) < 0.04)
                {
                    self.pending_seconds += dt;
                } else {
                    self.pending_seconds = dt;
                }
                self.pending_target = Some(target);
                if self.pending_seconds < 0.12 {
                    target = accepted;
                } else {
                    self.accepted_target = Some(target);
                    self.pending_target = None;
                    self.pending_seconds = 0.0;
                }
            } else {
                self.accepted_target = Some(target);
                self.pending_target = None;
                self.pending_seconds = 0.0;
            }
        } else {
            self.accepted_target = Some(target);
        }
        let fixation_started = match (accepted_before, self.accepted_target) {
            (None, Some(_)) => true,
            (Some(before), Some(after)) => before.distance(after) > 0.04,
            _ => false,
        };
        if fixation_started {
            self.dwell_seconds = 0.0;
        } else {
            self.dwell_seconds += dt;
        }

        if plan.mode == GazeMode::SocialReference
            && secondary.is_some()
            && self.dwell_seconds > plan.dwell_min.max(0.18)
        {
            self.checkback_phase = !self.checkback_phase;
            self.dwell_seconds = 0.0;
        }

        let tau = if self.has_current {
            plan.acquire_tau.clamp(0.025, 0.40)
        } else {
            0.045
        };
        let response = 1.0 - (-dt / tau).exp();
        self.current += (target - self.current) * response;
        self.has_current = true;

        let distance = self.current.distance(target);
        GazeOutput {
            // This is the world/semantic target. Eye-local ballistic offsets are
            // applied later by EmbodiedRuntime and never leak into head motion.
            target: Some(self.current),
            fixation_strength: (1.0 - distance * 8.0).clamp(0.0, 1.0)
                * plan.confidence.clamp(0.0, 1.0),
            pupil_focus: (0.45 + (1.0 - distance * 5.0).clamp(0.0, 1.0) * 0.45).clamp(0.0, 1.0),
            fixation_started,
        }
    }
}

fn finite_dt(dt: f32) -> f32 {
    if dt.is_finite() {
        dt.clamp(0.0, 0.1)
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn predictive_intercept_leads_motion() {
        let mut controller = GazeController::default();
        let plan = GazePlan {
            primary_target: Some(Vec2::new(0.4, 0.5)),
            target_velocity: Vec2::new(0.5, 0.0),
            mode: GazeMode::PredictiveIntercept,
            lead_seconds: 0.12,
            confidence: 1.0,
            ..GazePlan::default()
        };
        let out = controller.tick(plan, 0.1);
        assert!(out.target.unwrap().x > 0.4);
    }

    #[test]
    fn sleep_disables_gaze() {
        let mut controller = GazeController::default();
        let out = controller.tick(
            GazePlan {
                mode: GazeMode::Sleep,
                ..GazePlan::default()
            },
            0.05,
        );
        assert!(out.target.is_none());
    }

    #[test]
    fn held_fixation_has_no_continuous_world_target_jitter_at_any_rate() {
        let plan = GazePlan {
            primary_target: Some(Vec2::splat(0.5)),
            mode: GazeMode::Inspect,
            acquire_tau: 0.2,
            micro_saccade_amplitude: 0.01,
            confidence: 1.0,
            ..GazePlan::default()
        };
        for hz in [30, 60, 120] {
            let mut controller = GazeController::default();
            for tick in 1..=hz * 12 {
                let out = controller.tick(plan, 1.0 / hz as f32);
                let target = out.target.unwrap();
                assert_eq!(target, Vec2::splat(0.5), "hz={hz} tick={tick}");
                assert_eq!(controller.current, Vec2::splat(0.5));
            }
        }
    }

    #[test]
    fn fixation_started_is_an_event_not_label_chatter() {
        for hz in [30, 60, 120] {
            let mut controller = GazeController::default();
            let mut starts = 0;
            let mut plan = GazePlan {
                primary_target: Some(Vec2::new(0.30, 0.60)),
                mode: GazeMode::Inspect,
                acquire_tau: 0.07,
                confidence: 1.0,
                ..GazePlan::default()
            };
            for _ in 0..hz * 10 {
                starts += usize::from(controller.tick(plan, 1.0 / hz as f32).fixation_started);
            }
            assert_eq!(starts, 1, "hz={hz}");
            plan.primary_target = Some(Vec2::new(0.80, 0.20));
            for _ in 0..hz {
                starts += usize::from(controller.tick(plan, 1.0 / hz as f32).fixation_started);
            }
            assert_eq!(starts, 2, "hz={hz}");
        }
    }

    #[test]
    fn transient_opposite_targets_do_not_ping_pong_fixation() {
        for hz in [30, 60, 120] {
            let mut controller = GazeController::default();
            let mut plan = GazePlan {
                primary_target: Some(Vec2::splat(0.5)),
                mode: GazeMode::Inspect,
                acquire_tau: 0.07,
                confidence: 1.0,
                ..GazePlan::default()
            };
            let _ = controller.tick(plan, 1.0 / hz as f32);
            for tick in 0..hz * 3 {
                plan.primary_target = Some(Vec2::new(if tick % 2 == 0 { 0.05 } else { 0.95 }, 0.5));
                let out = controller.tick(plan, 1.0 / hz as f32);
                assert!(out.target.unwrap().distance(Vec2::splat(0.5)) < 0.0001);
            }
            plan.primary_target = Some(Vec2::new(0.95, 0.5));
            for _ in 0..hz {
                let _ = controller.tick(plan, 1.0 / hz as f32);
            }
            assert!(
                controller.current.x > 0.94,
                "a stable new target must acquire"
            );
        }
    }
}
