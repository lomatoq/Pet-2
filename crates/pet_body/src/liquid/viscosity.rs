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

/// Dissipates loaded, nonrigid return motion without damping translation or spin.
/// Each central pair impulse is an exact relative-velocity contraction, so its
/// mechanical work is nonpositive even with many sequential neighbor pairs.
#[allow(clippy::too_many_arguments)]
pub fn apply_strain_rate_dissipation(
    particles: &mut [LiquidParticle; MAX_LIQUID_PARTICLES],
    count: usize,
    main_component: u8,
    kernel_radius: f32,
    strain: f32,
    release_mobility: f32,
    dt: f32,
) {
    let count = count.min(MAX_LIQUID_PARTICLES);
    if count < 2
        || !kernel_radius.is_finite()
        || kernel_radius <= 0.0
        || !strain.is_finite()
        || strain <= 0.18
        || !release_mobility.is_finite()
        || release_mobility <= 0.0
        || !dt.is_finite()
        || dt <= 0.0
    {
        return;
    }
    let mut mass = 0.0;
    let mut center = Vec2::ZERO;
    let mut mean_velocity = Vec2::ZERO;
    for particle in &particles[..count] {
        if particle.component_id != main_component {
            continue;
        }
        if !particle.position.is_finite()
            || !particle.velocity.is_finite()
            || !particle.inverse_mass.is_finite()
            || particle.inverse_mass <= 0.0
        {
            return;
        }
        let m = particle.inverse_mass.recip();
        mass += m;
        center += particle.position * m;
        mean_velocity += particle.velocity * m;
    }
    if mass <= 0.0 {
        return;
    }
    center /= mass;
    mean_velocity /= mass;
    let mut inertia = 0.0;
    let mut angular_momentum = 0.0;
    for particle in &particles[..count] {
        if particle.component_id == main_component {
            let q = particle.position - center;
            let m = particle.inverse_mass.recip();
            inertia += q.length_squared() * m;
            angular_momentum += q.perp_dot(particle.velocity - mean_velocity) * m;
        }
    }
    let spin = angular_momentum / inertia.max(1.0e-5);
    let internal_speed_squared = particles[..count]
        .iter()
        .filter(|particle| particle.component_id == main_component)
        .map(|particle| {
            let relative =
                particle.velocity - mean_velocity - (particle.position - center).perp() * spin;
            relative.length_squared() / particle.inverse_mass
        })
        .sum::<f32>()
        / mass;
    let smooth_gate = |value: f32| {
        let x = value.clamp(0.0, 1.0);
        x * x * (3.0 - 2.0 * x)
    };
    // Measured production replay bands: gentle return strain <= .145 and
    // nonrigid RMS <= .029; the large return reaches .294 and .079.
    let strain_gate = smooth_gate((strain - 0.18) / (0.32 - 0.18));
    let speed_gate = smooth_gate((internal_speed_squared.sqrt() - 0.03) / (0.08 - 0.03));
    let rate = 8.0 * strain_gate * speed_gate * release_mobility.clamp(0.0, 1.0);
    if !rate.is_finite() || rate <= f32::EPSILON {
        return;
    }
    for first in 0..count {
        if particles[first].component_id != main_component {
            continue;
        }
        for second in (first + 1)..count {
            if particles[second].component_id != main_component {
                continue;
            }
            let delta = particles[second].position - particles[first].position;
            let distance = delta.length();
            if distance <= 1.0e-6 || distance >= kernel_radius {
                continue;
            }
            let weight = density_kernel(delta, kernel_radius);
            let density = particles[first]
                .density
                .max(particles[second].density)
                .max(1.0);
            if !density.is_finite() {
                continue;
            }
            let blend = 1.0 - (-rate * weight / density * dt).exp();
            let axis = delta / distance;
            let relative = (particles[second].velocity - particles[first].velocity).dot(axis);
            let inverse_first = particles[first].inverse_mass;
            let inverse_second = particles[second].inverse_mass;
            let inverse_sum = inverse_first + inverse_second;
            let impulse = axis * (relative * blend / inverse_sum);
            particles[first].velocity += impulse * inverse_first;
            particles[second].velocity -= impulse * inverse_second;
        }
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

    fn loaded_patch() -> [LiquidParticle; MAX_LIQUID_PARTICLES] {
        let mut particles = [LiquidParticle::default(); MAX_LIQUID_PARTICLES];
        for (i, particle) in particles[..4].iter_mut().enumerate() {
            particle.position = Vec2::new(
                if i % 2 == 0 { -0.03 } else { 0.03 },
                if i < 2 { -0.03 } else { 0.03 },
            );
            particle.inverse_mass = 1.0 / (i + 1) as f32;
            particle.density = 3.0;
            particle.velocity =
                Vec2::new(0.1, -0.2) + particle.position * 4.0 + particle.position.perp() * 0.9;
        }
        particles[4] = particles[0];
        particles[4].component_id = 1;
        particles[4].velocity = Vec2::new(0.7, -0.4);
        particles
    }

    fn energy(particles: &[LiquidParticle], count: usize) -> f32 {
        particles[..count]
            .iter()
            .map(|p| p.velocity.length_squared() * 0.5 / p.inverse_mass)
            .sum()
    }

    fn angular_momentum(particles: &[LiquidParticle], count: usize) -> f32 {
        particles[..count]
            .iter()
            .map(|p| p.position.perp_dot(p.velocity) / p.inverse_mass)
            .sum()
    }

    #[test]
    fn loaded_central_pairs_dissipate_without_changing_momenta_or_detached_mass() {
        let mut particles = loaded_patch();
        let before = particles;
        let linear = momentum(&particles, 5);
        let angular = angular_momentum(&particles, 5);
        let initial_energy = energy(&particles, 5);
        let mut previous_energy = initial_energy;
        for _ in 0..24 {
            apply_strain_rate_dissipation(&mut particles, 5, 0, 0.145, 0.5, 1.0, 1.0 / 120.0);
            let current_energy = energy(&particles, 5);
            assert!(current_energy <= previous_energy + 1.0e-6);
            previous_energy = current_energy;
            assert!(momentum(&particles, 5).distance(linear) < 1.0e-5);
            assert!((angular_momentum(&particles, 5) - angular).abs() < 1.0e-6);
        }
        assert!(previous_energy < initial_energy - 0.001);
        for (particle, original) in particles[..5].iter().zip(&before[..5]) {
            assert_eq!(particle.position, original.position);
            assert_eq!(particle.inverse_mass, original.inverse_mass);
        }
        assert_eq!(particles[4], before[4]);
    }

    #[test]
    fn relaxed_slow_or_held_material_keeps_its_motion() {
        for (strain, mobility, speed) in [(0.1, 1.0, 1.0), (0.5, 0.0, 1.0), (0.5, 1.0, 0.01)] {
            let mut particles = loaded_patch();
            for particle in &mut particles[..4] {
                particle.velocity *= speed;
            }
            let before = particles;
            apply_strain_rate_dissipation(
                &mut particles,
                5,
                0,
                0.145,
                strain,
                mobility,
                1.0 / 120.0,
            );
            assert_eq!(particles, before);
        }
    }

    #[test]
    fn rigid_spin_and_translation_do_not_activate_return_dissipation() {
        let mut particles = loaded_patch();
        for particle in &mut particles[..4] {
            particle.velocity = Vec2::new(0.3, -0.2) + particle.position.perp() * 12.0;
        }
        let before = particles;
        apply_strain_rate_dissipation(&mut particles, 5, 0, 0.145, 0.7, 1.0, 1.0 / 120.0);
        assert_eq!(particles, before);
    }

    #[test]
    fn distant_material_and_nonfinite_inputs_do_not_receive_pair_impulses() {
        let mut particles = loaded_patch();
        for (i, particle) in particles[..4].iter_mut().enumerate() {
            particle.position *= 20.0;
            particle.velocity = Vec2::new(i as f32, -(i as f32));
        }
        let before = particles;
        apply_strain_rate_dissipation(&mut particles, 5, 0, 0.145, 0.7, 1.0, 1.0 / 120.0);
        assert_eq!(particles, before);
        let mut particles = loaded_patch();
        let before = particles;
        apply_strain_rate_dissipation(&mut particles, 5, 0, 0.145, f32::NAN, 1.0, 1.0 / 120.0);
        assert_eq!(particles, before);
    }
}
