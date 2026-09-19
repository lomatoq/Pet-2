use lifecore::ExpressionState;

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ExpressionRuntime {
    /// Managed motor/director input owns actions; diagnostic pose labels do not.
    pub managed_actions: bool,
    pub current: ExpressionState,
    pub desired_geometry: lifecore::FaceGeometry,
    pub geometry_age_seconds: f32,
    pub geometry_90_seconds: Option<f32>,
    pub geometry_saturated: bool,
    initial_geometry_error: f32,
    procedural_suppression: f32,
    was_tired: bool,
    yawn_elapsed: Option<f32>,
    yawn_cooldown: f32,
    geometry_velocity: [f32; 20],
}

impl ExpressionRuntime {
    pub fn update(&mut self, target: ExpressionState, procedural_blink: f32, dt: f32) {
        if !dt.is_finite() || dt <= 0.0 {
            return;
        }
        let dt = dt.min(0.10);
        let mut target = target;
        let geometry = target.geometry.sanitized();
        self.geometry_saturated = geometry != target.geometry;
        if geometry != self.desired_geometry {
            self.desired_geometry = geometry;
            self.geometry_age_seconds = 0.0;
            self.geometry_90_seconds = None;
            self.initial_geometry_error = self.current.geometry.maximum_error(geometry);
        }
        self.geometry_age_seconds += dt.clamp(0.0, 0.1);
        self.yawn_cooldown = (self.yawn_cooldown - dt).max(0.0);
        let tired = target.face_pose == lifecore::FacePose::Tired;
        if !self.managed_actions && tired && !self.was_tired && self.yawn_cooldown <= 0.0 {
            self.yawn_elapsed = Some(0.0);
            self.yawn_cooldown = 8.0;
        }
        self.was_tired = tired;
        if self.managed_actions || !tired {
            self.yawn_elapsed = None;
        }
        if let Some(elapsed) = self.yawn_elapsed {
            // A yawn belongs to a pose-entry event, not the geometry clock:
            // independent brow/lid motion must never restart the mouth cycle.
            let elapsed = elapsed + dt;
            self.yawn_elapsed = (elapsed < 0.8).then_some(elapsed);
            let phase = (elapsed / 0.8).clamp(0.0, 1.0);
            target.mouth_open = target
                .mouth_open
                .max((phase * std::f32::consts::PI).sin() * 0.6);
        }
        self.current.face_pose = target.face_pose;
        // Separate tissue recruitment/relaxation, preserving velocity through
        // retargets. A pose switch must not restart one identical crossfade for
        // every point of both eyelids, brows and lips.
        for (index, (current, desired)) in self
            .current
            .geometry
            .lids
            .iter_mut()
            .flatten()
            .chain(self.current.geometry.brows.iter_mut().flatten())
            .chain(self.current.geometry.mouth.iter_mut())
            .zip(
                geometry
                    .lids
                    .iter()
                    .flatten()
                    .chain(geometry.brows.iter().flatten())
                    .chain(geometry.mouth.iter()),
            )
            .enumerate()
        {
            let omega = if index < 8 {
                if desired > current { 13.0 } else { 9.0 }
            } else if index < 16 {
                if desired.abs() > current.abs() {
                    9.0
                } else {
                    6.5
                }
            } else if desired.abs() > current.abs() {
                11.0
            } else {
                8.0
            };
            let displacement = *current - desired;
            let j = self.geometry_velocity[index] + omega * displacement;
            let decay = (-omega * dt).exp();
            *current = desired + (displacement + j * dt) * decay;
            self.geometry_velocity[index] =
                (self.geometry_velocity[index] - omega * j * dt) * decay;
        }
        if self.geometry_90_seconds.is_none()
            && self.current.geometry.maximum_error(geometry)
                <= self.initial_geometry_error * 0.1 + 1.0e-5
        {
            self.geometry_90_seconds = Some(self.geometry_age_seconds);
        }
        let authored = target.blink_left.max(target.blink_right).clamp(0.0, 1.0);
        if authored > 0.04 {
            self.procedural_suppression = 1.8;
        } else {
            self.procedural_suppression = (self.procedural_suppression - dt).max(0.0);
        }
        let automatic = if self.procedural_suppression > 0.0 {
            0.0
        } else {
            finite(procedural_blink, 0.0).clamp(0.0, 1.0)
        };
        if self.managed_actions {
            // The managed blink owner already evaluates a bounded analytic
            // envelope at body cadence. A second low-pass would erase fast
            // closure and keep the eye shut well into the opening tail.
            self.current.blink_left = finite(target.blink_left, 0.0).clamp(0.0, 1.0);
            self.current.blink_right = finite(target.blink_right, 0.0).clamp(0.0, 1.0);
        } else {
            self.current.blink_left = follow(
                self.current.blink_left,
                target.blink_left.max(automatic),
                dt,
                0.032,
                0.0,
                1.0,
            );
            self.current.blink_right = follow(
                self.current.blink_right,
                target.blink_right.max(automatic),
                dt,
                0.034,
                0.0,
                1.0,
            );
        }
        for (current, desired, tau, low, high) in [
            (&mut self.current.squint, target.squint, 0.12, 0.0, 1.0),
            (
                &mut self.current.pupil_size,
                target.pupil_size,
                0.26,
                0.0,
                1.0,
            ),
            (
                &mut self.current.pupil_focus,
                target.pupil_focus,
                0.10,
                0.0,
                1.0,
            ),
            (
                &mut self.current.brow_raise,
                target.brow_raise,
                0.15,
                -1.0,
                1.0,
            ),
            (
                &mut self.current.brow_tension,
                target.brow_tension,
                0.17,
                0.0,
                1.0,
            ),
            (
                &mut self.current.mouth_open,
                target.mouth_open,
                0.16,
                0.0,
                1.0,
            ),
            (
                &mut self.current.mouth_curve,
                target.mouth_curve,
                0.19,
                -1.0,
                1.0,
            ),
            (
                &mut self.current.mouth_tension,
                target.mouth_tension,
                0.14,
                0.0,
                1.0,
            ),
            (
                &mut self.current.cheek_glow,
                target.cheek_glow,
                0.30,
                0.0,
                1.0,
            ),
            (
                &mut self.current.body_glow,
                target.body_glow,
                0.36,
                0.0,
                1.0,
            ),
            (
                &mut self.current.eye_aperture,
                target.eye_aperture,
                0.10,
                0.0,
                1.0,
            ),
            (
                &mut self.current.eye_scale,
                target.eye_scale,
                0.20,
                0.5,
                1.5,
            ),
            (
                &mut self.current.brow_asymmetry,
                target.brow_asymmetry,
                0.18,
                -1.0,
                1.0,
            ),
            (
                &mut self.current.mouth_compression,
                target.mouth_compression,
                0.11,
                0.0,
                1.0,
            ),
            (
                &mut self.current.mouth_asymmetry,
                target.mouth_asymmetry,
                0.18,
                -1.0,
                1.0,
            ),
            (&mut self.current.effort, target.effort, 0.08, 0.0, 1.0),
            (&mut self.current.relief, target.relief, 0.22, 0.0, 1.0),
        ] {
            *current = follow(*current, desired, dt, tau, low, high);
        }
    }
}

fn finite(value: f32, fallback: f32) -> f32 {
    if value.is_finite() { value } else { fallback }
}

fn follow(current: f32, target: f32, dt: f32, tau: f32, low: f32, high: f32) -> f32 {
    let current = finite(current, 0.0).clamp(low, high);
    let target = finite(target, current).clamp(low, high);
    current + (target - current) * (1.0 - (-dt / tau.max(0.001)).exp())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn managed_diagnostic_tired_label_cannot_generate_yawns() {
        for hz in [30, 60, 120] {
            let mut runtime = ExpressionRuntime {
                managed_actions: true,
                ..Default::default()
            };
            for tick in 0..hz * 30 {
                let target = if (tick / (hz * 2)) % 2 == 0 {
                    lifecore::FacePose::Tired.expression()
                } else {
                    lifecore::FacePose::Awake.expression()
                };
                runtime.update(target, 0.0, 1.0 / hz as f32);
                assert!(runtime.current.mouth_open < 0.001);
            }
            let mut authored_yawn = lifecore::FacePose::Tired.expression();
            authored_yawn.mouth_open = 0.8;
            for _ in 0..hz {
                runtime.update(authored_yawn, 0.0, 1.0 / hz as f32);
            }
            assert!(runtime.current.mouth_open > 0.79);
        }
    }

    #[test]
    fn semantic_mouth_flicker_is_bounded_but_a_held_expression_arrives() {
        for hz in [30, 60, 120] {
            let mut runtime = ExpressionRuntime::default();
            let mut previous = 0.0;
            for tick in 0..hz * 4 {
                let mut target = lifecore::FacePose::Awake.expression();
                // Alternating 50 ms semantic states, independent of render Hz.
                target.mouth_open = if (tick * 20 / hz) % 2 == 0 { 1.0 } else { 0.0 };
                target.geometry.mouth[1] = target.mouth_open;
                runtime.update(target, 0.0, 1.0 / hz as f32);
                assert!((runtime.current.mouth_open - previous).abs() < 0.20);
                if tick > hz {
                    assert!(runtime.current.mouth_open > 0.25 && runtime.current.mouth_open < 0.75);
                }
                previous = runtime.current.mouth_open;
            }
            let mut held = lifecore::FacePose::Awake.expression();
            held.mouth_open = 1.0;
            held.geometry.mouth[1] = 1.0;
            for _ in 0..hz {
                runtime.update(held, 0.0, 1.0 / hz as f32);
            }
            assert!(runtime.current.mouth_open > 0.99);
            assert!(runtime.current.geometry.mouth[1] > 0.99);
        }
    }

    #[test]
    fn moving_tired_brows_cannot_restart_yawn() {
        let mut runtime = ExpressionRuntime::default();
        for tick in 0..600 {
            let mut target = lifecore::FacePose::Tired.expression();
            target.mouth_open = 0.0;
            target.geometry.brows[0][0] = (tick as f32 * 0.1).sin() * 0.1;
            runtime.update(target, 0.0, 1.0 / 60.0);
            if tick > 150 {
                assert!(runtime.current.mouth_open < 0.001, "tick={tick}");
            }
        }
    }

    #[test]
    fn managed_blink_samples_are_not_stretched_by_downstream_smoothing() {
        for hz in [30, 60, 120] {
            let mut runtime = ExpressionRuntime {
                managed_actions: true,
                ..Default::default()
            };
            let dt = 1.0 / hz as f32;
            let mut target = ExpressionState {
                blink_left: 1.0,
                blink_right: 0.985,
                ..Default::default()
            };
            runtime.update(target, 0.0, dt);
            assert_eq!(runtime.current.blink_left, 1.0);
            assert_eq!(runtime.current.blink_right, 0.985);
            target.blink_left = 0.0;
            target.blink_right = 0.0;
            runtime.update(target, 0.0, dt);
            assert_eq!(runtime.current.blink_left, 0.0);
            assert_eq!(runtime.current.blink_right, 0.0);
        }
    }
}
