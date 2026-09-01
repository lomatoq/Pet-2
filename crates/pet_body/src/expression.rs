use lifecore::ExpressionState;

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ExpressionRuntime {
    pub current: ExpressionState,
}

impl ExpressionRuntime {
    pub fn update(&mut self, target: ExpressionState, procedural_blink: f32, dt: f32) {
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
        for (current, target) in [
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
        ] {
            *current = smooth(*current, target, response);
        }
    }
}

fn smooth(current: f32, target: f32, response: f32) -> f32 {
    current + (target - current) * response
}
