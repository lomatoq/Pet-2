use glam::Vec2;

use super::{
    kernels::density_kernel,
    particles::{LiquidParticle, MAX_LIQUID_PARTICLES},
};

pub fn apply_xsph_viscosity(
    particles: &mut [LiquidParticle; MAX_LIQUID_PARTICLES],
    count: usize,
    kernel_radius: f32,
    numerical_coefficient: f32,
    authored_coefficient: f32,
    dt: f32,
) {
    let count = count.min(MAX_LIQUID_PARTICLES);
    if count < 2 || kernel_radius <= f32::EPSILON || dt <= 0.0 {
        return;
    }
    // Numerical XSPH is a small stability floor independent from the authored
    // gooiness slider. Their sum keeps the legacy 120 Hz coefficient meaning.
    let reference_blend =
        (numerical_coefficient.max(0.0) + authored_coefficient.max(0.0)).clamp(0.0, 0.32);
    let blend = 1.0 - (1.0 - reference_blend).powf((dt * 120.0).clamp(0.0, 4.0));
    if blend <= f32::EPSILON {
        return;
    }

    let snapshot = *particles;
    let mut deltas = [Vec2::ZERO; MAX_LIQUID_PARTICLES];
    for first in 0..count {
        for second in (first + 1)..count {
            let weight = density_kernel(
                snapshot[first].position - snapshot[second].position,
                kernel_radius,
            );
            if weight <= 0.0 {
                continue;
            }
            let inverse_mass_first = snapshot[first].inverse_mass.max(0.0);
            let inverse_mass_second = snapshot[second].inverse_mass.max(0.0);
            let inverse_mass_sum = inverse_mass_first + inverse_mass_second;
            if inverse_mass_sum <= f32::EPSILON {
                continue;
            }
            // A symmetric density denominator bounds each particle's aggregate
            // blend without the asymmetric per-particle normalization that used
            // to inject momentum at free surfaces.
            let density_scale = snapshot[first]
                .density
                .max(snapshot[second].density)
                .max(1.0);
            let pair_blend = (blend * weight / density_scale).clamp(0.0, 1.0);
            let relative_velocity = snapshot[second].velocity - snapshot[first].velocity;
            let impulse = relative_velocity * (pair_blend / inverse_mass_sum);
            deltas[first] += impulse * inverse_mass_first;
            deltas[second] -= impulse * inverse_mass_second;
        }
    }

    for (particle, delta) in particles[..count]
        .iter_mut()
        .zip(deltas[..count].iter().copied())
    {
        particle.velocity += delta;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn momentum(particles: &[LiquidParticle], count: usize) -> Vec2 {
        particles[..count]
            .iter()
            .map(|particle| particle.velocity * particle.inverse_mass.max(1.0e-5).recip())
            .sum()
    }

    #[test]
    fn pair_symmetric_xsph_preserves_momentum_and_reduces_relative_speed() {
        let mut particles = [LiquidParticle::default(); MAX_LIQUID_PARTICLES];
        particles[0].position = Vec2::new(-0.03, 0.0);
        particles[0].velocity = Vec2::new(1.0, 0.2);
        particles[0].inverse_mass = 1.0;
        particles[0].density = 2.0;
        particles[1].position = Vec2::new(0.03, 0.0);
        particles[1].velocity = Vec2::new(-1.0, -0.2);
        particles[1].inverse_mass = 0.5;
        particles[1].density = 2.0;
        let before_momentum = momentum(&particles, 2);
        let before_relative = particles[0].velocity.distance(particles[1].velocity);

        apply_xsph_viscosity(&mut particles, 2, 0.145, 0.01, 0.0, 1.0 / 120.0);

        assert!(momentum(&particles, 2).distance(before_momentum) < 1.0e-6);
        assert!(particles[0].velocity.distance(particles[1].velocity) < before_relative);
    }

    #[test]
    fn authored_zero_still_keeps_the_numerical_floor() {
        let mut particles = [LiquidParticle::default(); MAX_LIQUID_PARTICLES];
        particles[0].position = Vec2::new(-0.03, 0.0);
        particles[0].velocity = Vec2::X;
        particles[0].inverse_mass = 1.0;
        particles[0].density = 2.0;
        particles[1].position = Vec2::new(0.03, 0.0);
        particles[1].velocity = -Vec2::X;
        particles[1].inverse_mass = 1.0;
        particles[1].density = 2.0;

        apply_xsph_viscosity(&mut particles, 2, 0.145, 0.01, 0.0, 1.0 / 120.0);
        assert!(particles[0].velocity.x < 1.0);
        assert!(particles[1].velocity.x > -1.0);
    }
}
