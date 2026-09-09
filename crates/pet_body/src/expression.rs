use lifecore::ExpressionState;

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ExpressionRuntime {
    pub current: ExpressionState,
    pub desired_geometry: lifecore::FaceGeometry,
    pub geometry_age_seconds: f32,
    pub geometry_90_seconds: Option<f32>,
    pub geometry_saturated: bool,
    initial_geometry_error: f32,
}

impl ExpressionRuntime {
    pub fn update(&mut self, target: ExpressionState, procedural_blink: f32, dt: f32) {
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
        let response = 1.0 - (-14.0 * dt.clamp(0.0, 0.1)).exp();
        self.current.blink_left = smooth(
            self.current.blink_left,
            target.blink_left.max(procedural_blink),
            response,
        );
        self.current.blink_right = smooth(
            self.current.blink_right,
            target.blink_right.max(procedural_blink),
            response,
        );
        for (index, (current, target)) in [
            (&mut self.current.squint, target.squint),
            (&mut self.current.pupil_size, target.pupil_size),
            (&mut self.current.pupil_focus, target.pupil_focus),
            (&mut self.current.brow_raise, target.brow_raise),
            (&mut self.current.brow_tension, target.brow_tension),
            (&mut self.current.mouth_open, target.mouth_open),
            (&mut self.current.mouth_curve, target.mouth_curve),
            (&mut self.current.mouth_tension, target.mouth_tension),
            (&mut self.current.cheek_glow, target.cheek_glow),
            (&mut self.current.body_glow, target.body_glow),
            (&mut self.current.eye_aperture, target.eye_aperture),
            (&mut self.current.eye_scale, target.eye_scale),
            (&mut self.current.brow_asymmetry, target.brow_asymmetry),
            (
                &mut self.current.mouth_compression,
                target.mouth_compression,
            ),
            (&mut self.current.mouth_asymmetry, target.mouth_asymmetry),
            (&mut self.current.effort, target.effort),
            (&mut self.current.relief, target.relief),
        ]
        .into_iter()
        .enumerate()
        {
            let neutral = if index == 10 || index == 11 { 1.0 } else { 0.0 };
            let tau = if (target - neutral).abs() > (*current - neutral).abs() {
                0.06
            } else {
                0.22
            };
            *current = smooth(*current, target, 1.0 - (-dt.clamp(0.0, 0.1) / tau).exp());
        }
    }
}

fn smooth(current: f32, target: f32, response: f32) -> f32 {
    current + (target - current) * response
}
