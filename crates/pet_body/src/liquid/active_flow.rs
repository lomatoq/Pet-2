use glam::Vec2;

use crate::VisualMindInput;

use super::particles::{LiquidParticle, MAX_LIQUID_PARTICLES};

pub fn apply_active_flow(
    particles: &mut [LiquidParticle; MAX_LIQUID_PARTICLES],
    count: usize,
    body_origin: Vec2,
    seed_phase: f32,
    elapsed: f32,
    mind: VisualMindInput,
    activity: f32,
) {
    let strength = (mind.curiosity * 0.34 + mind.arousal * 0.26 + mind.stress * 0.10
        - mind.fatigue * 0.22)
        .max(0.0)
        * activity.clamp(0.0, 2.0);
    if strength <= 0.005 {
        return;
    }
    let centers = [
        Vec2::new(
            (seed_phase + elapsed * 0.11).sin() * 0.11,
            (seed_phase * 1.7 + elapsed * 0.08).cos() * 0.13,
        ),
        Vec2::new(
            (seed_phase * 0.7 - elapsed * 0.07).cos() * 0.15,
            (seed_phase * 1.3 + elapsed * 0.09).sin() * 0.10,
        ),
    ];
    let mut proposed = [Vec2::ZERO; MAX_LIQUID_PARTICLES];
    let mut weights = [0.0_f32; MAX_LIQUID_PARTICLES];
    let mut weighted_center = Vec2::ZERO;
    let mut weight_sum = 0.0;
    for (particle_index, particle) in particles[..count].iter().enumerate() {
        let body_delta = particle.position - body_origin;
        let weight = (-body_delta.length_squared() / 0.18).exp();
        weights[particle_index] = weight;
        weighted_center += particle.position * weight;
        weight_sum += weight;
        for (vortex_index, center) in centers.into_iter().enumerate() {
            let delta = particle.position - (body_origin + center);
            let radius = if vortex_index == 0 { 0.24 } else { 0.20 };
            let falloff = (-delta.length_squared() / (radius * radius)).exp();
            let sign = if vortex_index == 0 { 1.0 } else { -0.72 };
            proposed[particle_index] += Vec2::new(-delta.y, delta.x) * strength * falloff * sign;
        }
    }
    if weight_sum <= 1.0e-5 {
        return;
    }
    weighted_center /= weight_sum;

    // Internal circulation must not become locomotion or rigid-body spin. Project
    // the force field onto the zero-net-force, zero-net-torque subspace.
    let total_force = proposed[..count].iter().copied().sum::<Vec2>();
    let mean_force = total_force / weight_sum;
    let mut corrected = [Vec2::ZERO; MAX_LIQUID_PARTICLES];
    let mut torque = 0.0;
    let mut inertia = 0.0;
    for index in 0..count {
        corrected[index] = proposed[index] - mean_force * weights[index];
        let arm = particles[index].position - weighted_center;
        torque += arm.perp_dot(corrected[index]);
        inertia += arm.length_squared() * weights[index];
    }
    let angular_projection = torque / inertia.max(1.0e-5);
    for index in 0..count {
        let arm = particles[index].position - weighted_center;
        corrected[index] -= Vec2::new(-arm.y, arm.x) * angular_projection * weights[index];
        particles[index].force += corrected[index];
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::liquid::particles::initialize_particles;

    #[test]
    fn internal_flow_has_no_net_force_or_rigid_body_torque() {
        let (mut particles, count) = initialize_particles(42);
        apply_active_flow(
            &mut particles,
            count,
            Vec2::ZERO,
            0.37,
            4.2,
            VisualMindInput {
                arousal: 0.8,
                curiosity: 0.9,
                ..VisualMindInput::default()
            },
            1.0,
        );
        let center = particles[..count]
            .iter()
            .map(|particle| particle.position)
            .sum::<Vec2>()
            / count as f32;
        let total_force = particles[..count]
            .iter()
            .map(|particle| particle.force)
            .sum::<Vec2>();
        let torque = particles[..count]
            .iter()
            .map(|particle| (particle.position - center).perp_dot(particle.force))
            .sum::<f32>();
        assert!(total_force.length() < 1.0e-4, "net force {total_force:?}");
        assert!(torque.abs() < 1.0e-4, "net torque {torque}");
    }
}
