use glam::Vec2;
use lifecore::BodyFeedback;

use crate::AnalyticTuning;

const MODE_2_LIMIT: f32 = 0.105;
const MODE_3_LIMIT: f32 = 0.050;
const MODE_4_LIMIT: f32 = 0.026;

/// Low-dimensional, area-preserving deformation of the liquid body.
///
/// Each `Vec2` stores the cosine and sine coefficient of one polar mode.
/// Translation (mode 1) is deliberately absent: locomotion owns it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ModalDeformation {
    pub mode2: Vec2,
    pub mode3: Vec2,
    pub mode4: Vec2,
    pub area_scale: f32,
}

impl Default for ModalDeformation {
    fn default() -> Self {
        Self {
            mode2: Vec2::ZERO,
            mode3: Vec2::ZERO,
            mode4: Vec2::ZERO,
            area_scale: 1.0,
        }
    }
}

impl ModalDeformation {
    #[must_use]
    pub fn radial_scale(self, angle: f32) -> f32 {
        let (s2, c2) = (angle * 2.0).sin_cos();
        let (s3, c3) = (angle * 3.0).sin_cos();
        let (s4, c4) = (angle * 4.0).sin_cos();
        let wave = self.mode2.dot(Vec2::new(c2, s2))
            + self.mode3.dot(Vec2::new(c3, s3))
            + self.mode4.dot(Vec2::new(c4, s4));
        (self.area_scale * (1.0 + wave)).clamp(0.78, 1.22)
    }

    #[must_use]
    pub fn deform_point(self, point: Vec2) -> Vec2 {
        if point.length_squared() <= f32::EPSILON {
            return point;
        }
        let normalized = point / Vec2::new(0.35, 0.43);
        point * self.radial_scale(normalized.y.atan2(normalized.x))
    }

    fn update_area_scale(&mut self) {
        // For r(theta) = 1 + sum(a_n cos(n theta) + b_n sin(n theta)),
        // mean(r^2) is 1 + 1/2 sum(a_n^2 + b_n^2). Its reciprocal square
        // root keeps the projected area stable without introducing a mode-1 drift.
        let modal_energy =
            self.mode2.length_squared() + self.mode3.length_squared() + self.mode4.length_squared();
        self.area_scale = (1.0 + modal_energy * 0.5).sqrt().recip();
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ModalDynamics {
    pub deformation: ModalDeformation,
    velocity2: Vec2,
    velocity3: Vec2,
    velocity4: Vec2,
    previous_acceleration: Vec2,
    tuning: AnalyticTuning,
}

impl Default for ModalDynamics {
    fn default() -> Self {
        Self {
            deformation: ModalDeformation::default(),
            velocity2: Vec2::ZERO,
            velocity3: Vec2::ZERO,
            velocity4: Vec2::ZERO,
            previous_acceleration: Vec2::ZERO,
            tuning: AnalyticTuning::default(),
        }
    }
}

impl ModalDynamics {
    pub fn set_tuning(&mut self, tuning: AnalyticTuning) {
        self.tuning = tuning;
    }

    pub fn update(&mut self, softness: f32, feedback: &BodyFeedback, dt: f32) {
        let dt = if dt.is_finite() {
            dt.clamp(0.0, 0.05)
        } else {
            0.0
        };
        if dt <= f32::EPSILON {
            return;
        }

        let softness = softness.clamp(0.0, 1.0);
        let acceleration = finite_vec2(feedback.acceleration).clamp_length_max(4.0);
        let jerk = ((acceleration - self.previous_acceleration) / dt).clamp_length_max(28.0);
        self.previous_acceleration = acceleration;
        let impact = feedback
            .collision
            .as_ref()
            .map_or(0.0, |collision| collision.intensity)
            .clamp(0.0, 1.0);

        let acceleration_direction = if acceleration.length_squared() > 0.000_1 {
            acceleration.normalize()
        } else if feedback.velocity.length_squared() > 0.000_1 {
            finite_vec2(feedback.velocity).normalize_or_zero()
        } else {
            Vec2::Y
        };
        let jerk_direction = if jerk.length_squared() > 0.000_1 {
            jerk.normalize()
        } else {
            acceleration_direction
        };
        let softness_gain = 0.62 + softness * 0.78;
        let force2 = harmonic(acceleration_direction, 2)
            * (acceleration.length() * 6.8 + jerk.length() * 0.12 + impact * 12.0)
            * softness_gain
            * self.tuning.modal_response;
        let force3 = harmonic(jerk_direction, 3)
            * (jerk.length() * 0.115 + impact * 5.4)
            * softness_gain
            * self.tuning.modal_response;
        let force4 = harmonic(acceleration_direction, 4)
            * (acceleration.length() * 0.42 + impact * 2.8)
            * softness_gain
            * self.tuning.modal_response;

        let omega2 = lerp(8.4, 5.2, softness) * self.tuning.modal_frequency;
        integrate_mode(
            &mut self.deformation.mode2,
            &mut self.velocity2,
            force2,
            omega2,
            lerp(0.30, 0.17, softness) * self.tuning.modal_damping,
            MODE_2_LIMIT * self.tuning.modal_amplitude,
            dt,
        );
        integrate_mode(
            &mut self.deformation.mode3,
            &mut self.velocity3,
            force3,
            omega2 * 1.58,
            lerp(0.38, 0.24, softness) * self.tuning.modal_damping,
            MODE_3_LIMIT * self.tuning.modal_amplitude,
            dt,
        );
        integrate_mode(
            &mut self.deformation.mode4,
            &mut self.velocity4,
            force4,
            omega2 * 2.12,
            lerp(0.48, 0.32, softness) * self.tuning.modal_damping,
            MODE_4_LIMIT * self.tuning.modal_amplitude,
            dt,
        );
        self.deformation.update_area_scale();

        if !self.is_finite() {
            let tuning = self.tuning;
            *self = Self::default();
            self.tuning = tuning;
        }
    }

    #[must_use]
    pub fn is_finite(&self) -> bool {
        self.deformation.mode2.is_finite()
            && self.deformation.mode3.is_finite()
            && self.deformation.mode4.is_finite()
            && self.deformation.area_scale.is_finite()
            && self.velocity2.is_finite()
            && self.velocity3.is_finite()
            && self.velocity4.is_finite()
    }
}

fn integrate_mode(
    position: &mut Vec2,
    velocity: &mut Vec2,
    force: Vec2,
    omega: f32,
    damping_ratio: f32,
    limit: f32,
    dt: f32,
) {
    // Semi-implicit Euler is stable here because dt is clamped and every retained
    // mode stays comfortably below the integrator's Nyquist limit.
    let acceleration =
        force - *velocity * (2.0 * damping_ratio * omega) - *position * omega.powi(2);
    *velocity += acceleration * dt;
    *position += *velocity * dt;
    if position.length() > limit {
        *position = position.normalize_or_zero() * limit;
        let outward_speed = velocity.dot(position.normalize_or_zero()).max(0.0);
        *velocity -= position.normalize_or_zero() * outward_speed * 0.82;
    }
}

fn harmonic(direction: Vec2, order: u32) -> Vec2 {
    if direction.length_squared() <= f32::EPSILON {
        return Vec2::ZERO;
    }
    let angle = direction.y.atan2(direction.x) * order as f32;
    Vec2::new(angle.cos(), angle.sin())
}

fn finite_vec2(value: Vec2) -> Vec2 {
    if value.is_finite() { value } else { Vec2::ZERO }
}

fn lerp(start: f32, end: f32, amount: f32) -> f32 {
    start + (end - start) * amount
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn acceleration_impulse_rings_then_decays_without_translation_mode() {
        let mut dynamics = ModalDynamics::default();
        let impulse = BodyFeedback {
            acceleration: Vec2::new(1.8, -0.4),
            ..BodyFeedback::default()
        };
        dynamics.update(0.82, &impulse, 1.0 / 120.0);
        let first_energy = dynamics.deformation.mode2.length_squared()
            + dynamics.deformation.mode3.length_squared();
        assert!(first_energy > 0.0);

        let mut peak = first_energy;
        for _ in 0..120 {
            dynamics.update(0.82, &BodyFeedback::default(), 1.0 / 120.0);
            peak = peak.max(
                dynamics.deformation.mode2.length_squared()
                    + dynamics.deformation.mode3.length_squared(),
            );
        }
        assert!(
            peak > first_energy,
            "the impulse should ring, not snap to a pose"
        );
        for _ in 0..1_800 {
            dynamics.update(0.82, &BodyFeedback::default(), 1.0 / 120.0);
        }
        assert!(dynamics.deformation.mode2.length() < 0.001);
        assert!(dynamics.deformation.mode3.length() < 0.001);
        assert!(dynamics.is_finite());
    }

    #[test]
    fn modal_area_normalizer_matches_the_analytic_mean_area() {
        let mut deformation = ModalDeformation {
            mode2: Vec2::new(0.09, -0.03),
            mode3: Vec2::new(0.03, 0.02),
            mode4: Vec2::new(-0.01, 0.02),
            area_scale: 1.0,
        };
        deformation.update_area_scale();
        let mut mean_radius_squared = 0.0;
        let samples = 4_096;
        for sample in 0..samples {
            let angle = std::f32::consts::TAU * sample as f32 / samples as f32;
            mean_radius_squared += deformation.radial_scale(angle).powi(2);
        }
        mean_radius_squared /= samples as f32;
        assert!((mean_radius_squared - 1.0).abs() < 0.000_2);
    }
}
