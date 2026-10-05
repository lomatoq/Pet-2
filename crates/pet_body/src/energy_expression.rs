//! Optical expression of measured affect. This does not actuate the body.
use glam::{Vec3, Vec4};
use lifecore::{AffectState, ExpressionState};

use crate::VisualMindInput;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EnergyAppearance {
    /// Linear emission color and its blend weight into the spectral membrane.
    pub palette: Vec4,
    /// Two integrated circulation phases, energy amplitude, and agitation.
    pub dynamics: Vec4,
}

impl Default for EnergyAppearance {
    fn default() -> Self {
        Self {
            palette: Vec4::new(0.34, 0.44, 1.0, 0.32),
            dynamics: Vec4::new(0.0, 0.0, 0.42, 0.0),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EnergyExpression {
    pub appearance: EnergyAppearance,
    palette_drive: Vec4,
    intensity_drive: f32,
    agitation_drive: f32,
}

impl Default for EnergyExpression {
    fn default() -> Self {
        let appearance = EnergyAppearance::default();
        Self {
            palette_drive: appearance.palette,
            intensity_drive: appearance.dynamics.z,
            agitation_drive: 0.0,
            appearance,
        }
    }
}

fn unit(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

impl EnergyExpression {
    pub fn update(
        &mut self,
        affect: AffectState,
        mut mind: VisualMindInput,
        face: ExpressionState,
        sleeping: bool,
        dt: f32,
    ) {
        if !dt.is_finite() || dt <= 0.0 {
            return;
        }
        mind.sanitize();
        let stress = unit(affect.stress);
        let frustration = unit(affect.frustration);
        let arousal = unit(affect.arousal);
        let joy = unit(affect.valence).max(unit(face.mouth_curve) * 0.7)
            * (1.0 - stress)
            * (1.0 - frustration * 0.6);
        let anger = frustration * (0.35 + 0.65 * arousal);
        let fear = stress * (1.0 - unit(affect.confidence)) * (1.0 - frustration * 0.6);
        let rest = if sleeping { 1.0 } else { mind.fatigue * 0.32 } * (1.0 - stress * 0.8);
        let curiosity = mind.curiosity * (1.0 - stress) * (1.0 - rest);
        let contact = unit(affect.attachment) * unit(face.relief) * (1.0 - stress);
        // This palette is an authored readable vocabulary, not a claim that
        // animals biologically encode anger or fear in these colors.
        let weights = [
            joy * 1.1 + contact * 0.5,
            anger * 2.0,
            fear * 1.6,
            rest * 1.8,
            curiosity * 0.55,
        ];
        let colors = [
            Vec3::new(0.26, 1.00, 0.70),
            Vec3::new(1.30, 0.12, 0.24),
            Vec3::new(0.40, 0.78, 1.28),
            Vec3::new(0.36, 0.28, 0.72),
            Vec3::new(0.22, 0.72, 1.10),
        ];
        let mut color = Vec3::new(0.34, 0.44, 1.0) * 0.38;
        let mut sum = 0.38;
        for (weight, tint) in weights.into_iter().zip(colors) {
            color += tint * weight;
            sum += weight;
        }
        color /= sum;
        let color_weight = 0.32 + 0.52 * unit(joy.max(anger).max(fear).max(rest));
        let palette = color.extend(color_weight);
        let intensity = ((0.30 + arousal * 0.34 + joy * 0.20 + anger * 0.16) * (1.0 - rest * 0.78))
            .clamp(0.055, 0.95);
        let agitation = ((anger * 0.70 + fear * 0.54) * (1.0 - rest)).clamp(0.0, 0.8);
        // Cascaded filters give finite onset and release. Phase integrates the
        // changing rate; multiplying elapsed time by a new rate would teleport
        // every light feature on an emotion change.
        let dt = dt.min(0.25);
        let steps = (dt * 120.0).ceil().max(1.0) as u32;
        let h = dt / steps as f32;
        for _ in 0..steps {
            let a = 1.0 - (-h / 0.42).exp();
            let b = 1.0 - (-h / 0.72).exp();
            self.palette_drive += (palette - self.palette_drive) * a;
            self.appearance.palette += (self.palette_drive - self.appearance.palette) * b;
            self.intensity_drive += (intensity - self.intensity_drive) * a;
            self.agitation_drive += (agitation - self.agitation_drive) * a;
            self.appearance.dynamics.z += (self.intensity_drive - self.appearance.dynamics.z) * b;
            self.appearance.dynamics.w += (self.agitation_drive - self.appearance.dynamics.w) * b;
            // Continuous speed follows the filtered amplitude, not an action
            // label. Quiet broad pools remain visibly advected at desktop size.
            let wake = ((self.appearance.dynamics.z - 0.07) / 0.23).clamp(0.0, 1.0);
            let rate = 0.10 + wake * 0.16 + self.appearance.dynamics.z * 0.62;
            self.appearance.dynamics.x =
                (self.appearance.dynamics.x + h * rate).rem_euclid(std::f32::consts::TAU);
            self.appearance.dynamics.y =
                (self.appearance.dynamics.y + h * rate * 0.6744).rem_euclid(std::f32::consts::TAU);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settle(affect: AffectState, sleep: bool, hz: u32) -> EnergyAppearance {
        let mut energy = EnergyExpression::default();
        for _ in 0..hz * 8 {
            energy.update(
                affect,
                VisualMindInput::default(),
                ExpressionState::default(),
                sleep,
                1.0 / hz as f32,
            );
        }
        energy.appearance
    }

    #[test]
    fn same_arousal_with_different_appraisal_has_distinct_color_and_motion() {
        let happy = settle(
            AffectState {
                valence: 0.9,
                arousal: 0.8,
                stress: 0.0,
                frustration: 0.0,
                ..Default::default()
            },
            false,
            120,
        );
        let angry = settle(
            AffectState {
                valence: -0.8,
                arousal: 0.8,
                stress: 0.8,
                frustration: 0.9,
                ..Default::default()
            },
            false,
            120,
        );
        let fear = settle(
            AffectState {
                valence: -0.8,
                arousal: 0.8,
                stress: 0.9,
                confidence: 0.05,
                frustration: 0.02,
                ..Default::default()
            },
            false,
            120,
        );
        let sleep = settle(
            AffectState {
                arousal: 0.1,
                stress: 0.0,
                ..Default::default()
            },
            true,
            120,
        );
        assert!(happy.palette.y > happy.palette.x * 1.8);
        assert!(angry.palette.x > angry.palette.y * 2.0);
        assert!(fear.palette.z > fear.palette.x * 1.8);
        assert!(sleep.dynamics.z < happy.dynamics.z * 0.25);
        assert!(angry.dynamics.w > happy.dynamics.w + 0.3);
    }

    #[test]
    fn transitions_do_not_reset_phase_and_match_across_frame_rates() {
        let affect = AffectState {
            valence: -0.7,
            arousal: 0.95,
            stress: 0.8,
            frustration: 0.95,
            ..Default::default()
        };
        let a = settle(affect, false, 30);
        let b = settle(affect, false, 120);
        assert!(a.palette.distance(b.palette) < 0.001);
        assert!(a.dynamics.distance(b.dynamics) < 0.002);
        let mut energy = EnergyExpression::default();
        for _ in 0..1200 {
            let before = energy.appearance;
            energy.update(
                affect,
                VisualMindInput::default(),
                ExpressionState::default(),
                false,
                1.0 / 60.0,
            );
            let after = energy.appearance;
            assert!(after.palette.distance(before.palette) < 0.025);
            let phase_step =
                (after.dynamics.x - before.dynamics.x).rem_euclid(std::f32::consts::TAU);
            assert!(phase_step > 0.0 && phase_step < 0.016);
            assert!(after.palette.is_finite() && after.dynamics.is_finite());
        }
    }
}
