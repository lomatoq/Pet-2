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
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GazeController {
    current: Vec2,
    has_current: bool,
    dwell_seconds: f32,
    checkback_phase: bool,
    deterministic_phase: f32,
}

impl Default for GazeController {
    fn default() -> Self {
        Self {
            current: Vec2::splat(0.5),
            has_current: false,
            dwell_seconds: 0.0,
            checkback_phase: false,
            deterministic_phase: 0.0,
        }
    }
}

impl GazeController {
    #[must_use]
    pub fn tick(&mut self, plan: GazePlan, dt: f32) -> GazeOutput {
        let dt = finite_dt(dt);
        if plan.mode == GazeMode::Sleep {
            self.has_current = false;
            self.dwell_seconds = 0.0;
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
            };
        };
        target = target.clamp(Vec2::ZERO, Vec2::ONE);
        self.dwell_seconds += dt;
        self.deterministic_phase = (self.deterministic_phase + dt * 1.37).rem_euclid(1000.0);

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

        // Micro-saccade is intentionally tiny and disabled for interception/threat-like gaze.
        if matches!(
            plan.mode,
            GazeMode::Inspect | GazeMode::MutualGaze | GazeMode::ContactMonitor
        ) && plan.confidence > 0.45
        {
            let a = plan.micro_saccade_amplitude.clamp(0.0, 0.015);
            let offset = Vec2::new(
                (self.deterministic_phase * 3.17).sin(),
                (self.deterministic_phase * 2.41 + 0.7).cos(),
            ) * a;
            self.current = (self.current + offset).clamp(Vec2::ZERO, Vec2::ONE);
        }

        let distance = self.current.distance(target);
        GazeOutput {
            target: Some(self.current),
            fixation_strength: (1.0 - distance * 8.0).clamp(0.0, 1.0)
                * plan.confidence.clamp(0.0, 1.0),
            pupil_focus: (0.45 + (1.0 - distance * 5.0).clamp(0.0, 1.0) * 0.45).clamp(0.0, 1.0),
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
}
