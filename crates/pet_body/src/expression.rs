use lifecore::ExpressionState;

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ExpressionRuntime {
    pub current: ExpressionState,
    pub desired_geometry: lifecore::FaceGeometry,
    pub geometry_age_seconds: f32,
    pub geometry_90_seconds: Option<f32>,
    pub geometry_saturated: bool,
    initial_geometry_error: f32,
    procedural_suppression: f32,
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
        if target.face_pose == lifecore::FacePose::Tired && self.geometry_age_seconds < 0.8 {
            // One transition yawn; holding the same pose never restarts it.
            let phase = (self.geometry_age_seconds / 0.8).clamp(0.0, 1.0);
            target.mouth_open = target
                .mouth_open
                .max((phase * std::f32::consts::PI).sin() * 0.6);
        }
        self.current.face_pose = target.face_pose;
        self.current
            .geometry
            .approach(target.geometry, 1.0 - (-dt.clamp(0.0, 0.1) / 0.08).exp());
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
                0.040,
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
