use glam::Vec2;

use super::particles::{LiquidParticle, MAX_LIQUID_PARTICLES};

/// Gently restores the relaxed second moments of the main liquid component.
///
/// This is deliberately not a rest-pose spring: particles remain free to exchange
/// places and the correction has zero mean, so it cannot pin or translate the blob.
/// It only removes long-lived stretch after external interaction has ended.
pub fn apply_shape_homeostasis(
    particles: &mut [LiquidParticle; MAX_LIQUID_PARTICLES],
    count: usize,
    main_component: u8,
    strength: f32,
    activation: f32,
) {
    let gain = strength.clamp(0.0, 24.0) * activation.clamp(0.0, 1.0);
    if gain <= f32::EPSILON {
        return;
    }

    let mut current_center = Vec2::ZERO;
    let mut rest_center = Vec2::ZERO;
    let mut member_count = 0.0_f32;
    for particle in &particles[..count] {
        if particle.component_id == main_component {
            current_center += particle.position;
            rest_center += particle.rest_position;
            member_count += 1.0;
        }
    }
    if member_count < 4.0 {
        return;
    }
    current_center /= member_count;
    rest_center /= member_count;

    let mut current_variance = Vec2::ZERO;
    let mut rest_variance = Vec2::ZERO;
    for particle in &particles[..count] {
        if particle.component_id != main_component {
            continue;
        }
        let current = particle.position - current_center;
        let rest = particle.rest_position - rest_center;
        current_variance += current * current;
        rest_variance += rest * rest;
    }
    let current_variance = (current_variance / member_count).max(Vec2::splat(1.0e-6));
    let rest_variance = (rest_variance / member_count).max(Vec2::splat(1.0e-6));
    let current_rms = Vec2::new(current_variance.x.sqrt(), current_variance.y.sqrt());
    let rest_rms = Vec2::new(rest_variance.x.sqrt(), rest_variance.y.sqrt());
    let scale_error =
        (rest_rms / current_rms - Vec2::ONE).clamp(Vec2::splat(-0.42), Vec2::splat(0.42));

    let mut corrections = [Vec2::ZERO; MAX_LIQUID_PARTICLES];
    let mut mean_correction = Vec2::ZERO;
    for (index, particle) in particles[..count].iter().enumerate() {
        if particle.component_id != main_component {
            continue;
        }
        let local = particle.position - current_center;
        let correction = (local * scale_error * gain).clamp_length_max(3.5);
        corrections[index] = correction;
        mean_correction += correction;
    }
    mean_correction /= member_count;

    for (index, particle) in particles[..count].iter_mut().enumerate() {
        if particle.component_id == main_component {
            particle.force += corrections[index] - mean_correction;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::liquid::particles::initialize_particles;

    #[test]
    fn recovery_is_translation_invariant_and_zero_sum() {
        let (mut particles, count) = initialize_particles(41);
        let translation = Vec2::new(0.73, -0.28);
        for particle in &mut particles[..count] {
            particle.position = particle.position * Vec2::new(1.55, 0.78) + translation;
            particle.component_id = 0;
        }

        apply_shape_homeostasis(&mut particles, count, 0, 8.0, 1.0);

        let force_sum = particles[..count]
            .iter()
            .map(|particle| particle.force)
            .sum::<Vec2>();
        assert!(force_sum.length() < 1.0e-4, "net force {force_sum:?}");
        let rightmost = particles[..count]
            .iter()
            .max_by(|a, b| a.position.x.total_cmp(&b.position.x))
            .unwrap();
        let topmost = particles[..count]
            .iter()
            .max_by(|a, b| a.position.y.total_cmp(&b.position.y))
            .unwrap();
        assert!(rightmost.force.x < 0.0);
        assert!(topmost.force.y > 0.0);
    }
}
