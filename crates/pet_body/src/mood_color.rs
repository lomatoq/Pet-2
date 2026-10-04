//! Slow, bounded optical expression. No timer-driven mood or random hue cycling.
use glam::Vec3;
use lifecore::{AffectState, ExpressionState};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MoodColor {
    drive: Vec3,
    pub tint: Vec3,
    pub trail: Vec3,
    joy_drive: f32,
    pub joy: f32,
    excitement: f32,
    excitement_wave: f32,
}
impl Default for MoodColor {
    fn default() -> Self {
        Self {
            drive: Vec3::ONE,
            tint: Vec3::ONE,
            trail: Vec3::ONE,
            joy_drive: 0.0,
            joy: 0.0,
            excitement: 0.0,
            excitement_wave: 0.0,
        }
    }
}
fn unit(v: f32) -> f32 {
    if v.is_finite() {
        v.clamp(0.0, 1.0)
    } else {
        0.0
    }
}
fn ease(v: f32) -> f32 {
    let v = unit(v);
    v * v * (3.0 - 2.0 * v)
}
impl MoodColor {
    pub fn set_excitation(&mut self, intensity: f32, wave: f32) {
        self.excitement = unit(intensity);
        self.excitement_wave = unit(wave / 0.095);
    }
    pub fn update(&mut self, affect: AffectState, face: ExpressionState, dt: f32) {
        if !dt.is_finite() || dt <= 0.0 {
            return;
        }
        let smile = unit(face.mouth_curve);
        let tension = unit(face.brow_tension).max(unit(affect.stress));
        let warm = smile * (1.0 - tension * 0.8);
        let subdued = unit(-face.mouth_curve) * (1.0 - tension);
        let curious = unit(face.brow_raise) * (1.0 - tension) * (1.0 - smile);
        let calm = (1.0 - unit(affect.arousal)) * (1.0 - tension) * (1.0 - smile);
        let target = (Vec3::ONE
            + Vec3::new(0.0, -0.10, -0.17) * warm
            + Vec3::new(-0.12, -0.06, 0.0) * subdued
            + Vec3::new(-0.10, 0.0, -0.025) * curious
            + Vec3::new(-0.065, -0.025, 0.0) * calm
            + Vec3::new(0.0, -0.12, -0.055) * tension
            + Vec3::new(
                -0.10 * self.excitement_wave,
                -0.08 * (1.0 - self.excitement_wave),
                -0.09 * (1.0 - self.excitement_wave),
            ) * self.excitement)
            .clamp(Vec3::splat(0.80), Vec3::ONE);
        let happy =
            ease((smile - 0.30) / 0.55) * (1.0 - tension) * (0.65 + 0.35 * unit(affect.valence));
        // Cascaded low-pass stages give a soft start as well as a soft arrival.
        // Internal substeps keep the envelope consistent on slow/fast displays.
        let dt = dt.min(0.25);
        let steps = (dt / (1.0 / 120.0)).ceil().max(1.0) as u32;
        let h = dt / steps as f32;
        for _ in 0..steps {
            self.drive += (target - self.drive) * (1.0 - (-h / 0.65).exp());
            self.tint += (self.drive - self.tint) * (1.0 - (-h / 0.90).exp());
            self.trail += (self.tint - self.trail) * (1.0 - (-h / 1.30).exp());
            let tau = if happy > self.joy_drive { 0.8 } else { 1.8 };
            self.joy_drive += (happy - self.joy_drive) * (1.0 - (-h / tau).exp());
            self.joy += (self.joy_drive - self.joy) * (1.0 - (-h / 0.75).exp());
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn excitement_palette_changes_softly_and_returns_without_flashing() {
        let mut m = MoodColor::default();
        for i in 0..1200 {
            let before = m.tint;
            m.set_excitation(0.9, ((i as f32 / 120.0).sin() * 0.5 + 0.5) * 0.09);
            m.update(
                AffectState::default(),
                ExpressionState::default(),
                1.0 / 60.0,
            );
            assert!(m.tint.distance(before) < 0.003);
            assert!(m.tint.min_element() >= 0.8 && m.tint.max_element() <= 1.0);
        }
        m.set_excitation(0.0, 0.0);
        let mut quiet = MoodColor::default();
        for _ in 0..2400 {
            m.update(
                AffectState::default(),
                ExpressionState::default(),
                1.0 / 60.0,
            );
            quiet.update(
                AffectState::default(),
                ExpressionState::default(),
                1.0 / 60.0,
            );
        }
        assert!(m.tint.distance(quiet.tint) < 0.001);
    }
    #[test]
    fn color_and_glow_are_continuous_bounded_and_recover() {
        let mut m = MoodColor::default();
        let face = ExpressionState {
            mouth_curve: 0.95,
            brow_tension: 0.0,
            ..Default::default()
        };
        let affect = AffectState {
            valence: 0.8,
            arousal: 0.7,
            stress: 0.0,
            ..Default::default()
        };
        m.update(affect, face, 1.0 / 60.0);
        assert!(m.joy < 0.002 && m.tint.distance(Vec3::ONE) < 0.002);
        for _ in 0..600 {
            m.update(affect, face, 1.0 / 60.0);
        }
        assert!(m.joy > 0.8 && m.tint.z < m.tint.x - 0.1);
        let mut quiet = face;
        quiet.mouth_curve = 0.0;
        let before = m.joy;
        m.update(affect, quiet, 1.0 / 60.0);
        assert!(m.joy > before - 0.01);
        for _ in 0..1800 {
            m.update(affect, quiet, 1.0 / 60.0);
        }
        assert!(m.joy < 0.001);
        assert!(m.tint.min_element() >= 0.8 && m.tint.max_element() <= 1.0);
    }
    #[test]
    fn frame_rate_does_not_change_mood_color() {
        let mut a = MoodColor::default();
        let mut b = a;
        let face = ExpressionState {
            mouth_curve: 0.9,
            ..Default::default()
        };
        for _ in 0..120 {
            a.update(AffectState::default(), face, 1.0 / 30.0);
        }
        for _ in 0..480 {
            b.update(AffectState::default(), face, 1.0 / 120.0);
        }
        assert!(a.tint.distance(b.tint) < 0.002 && (a.joy - b.joy).abs() < 0.002);
    }
}
