use glam::Vec2;

use super::{
    density::update_density_and_surface,
    kernels::{density_gradient, density_kernel},
    particles::{LiquidParticle, MAX_LIQUID_PARTICLES},
};

#[derive(Debug, Clone, Copy)]
pub struct DensityConstraintParameters {
    pub rest_density: f32,
    pub kernel_radius: f32,
    pub compliance: f32,
    pub scorr_k: f32,
    pub scorr_q_ratio: f32,
    pub scorr_power: u32,
    pub iterations: usize,
    pub dt: f32,
    /// Absolute particle-space bounds. Containment is a unilateral positional
    /// constraint in the same projection loop, with no restitution impulse.
    pub containment_bounds: Option<(Vec2, Vec2)>,
}

pub fn solve_density_constraints(
    particles: &mut [LiquidParticle; MAX_LIQUID_PARTICLES],
    count: usize,
    parameters: DensityConstraintParameters,
) {
    let DensityConstraintParameters {
        rest_density,
        kernel_radius,
        compliance,
        scorr_k,
        scorr_q_ratio,
        scorr_power,
        iterations,
        dt,
        containment_bounds,
    } = parameters;
    let count = count.min(MAX_LIQUID_PARTICLES);
    if count == 0
        || iterations == 0
        || dt <= f32::EPSILON
        || kernel_radius <= f32::EPSILON
        || rest_density <= f32::EPSILON
    {
        return;
    }

    // XPBD multipliers persist only across the iterations of this solve. Carrying
    // them into the next fixed tick stores stale pressure and produces a large
    // contact impulse when separated material re-enters the fluid.
    let mut lambdas = [0.0_f32; MAX_LIQUID_PARTICLES];
    let mut delta_lambdas = [0.0_f32; MAX_LIQUID_PARTICLES];
    for particle in &mut particles[..count] {
        particle.lambda = 0.0;
    }
    let inverse_rest_density = rest_density.max(0.01).recip();
    let alpha = compliance.max(0.0) / (dt * dt).max(1.0e-8);
    let reference_weight = density_kernel(
        Vec2::new(kernel_radius * scorr_q_ratio.clamp(0.10, 0.40), 0.0),
        kernel_radius,
    )
    .max(1.0e-5);

    for _ in 0..iterations {
        update_density_and_surface(particles, count, kernel_radius);
        let snapshot = *particles;
        for index in 0..count {
            let inverse_mass = snapshot[index].inverse_mass.max(0.0);
            if inverse_mass <= f32::EPSILON {
                delta_lambdas[index] = 0.0;
                continue;
            }

            // Unilateral incompressibility: an under-dense free surface is legal.
            // Cohesion is owned by the surface model, not by negative pressure.
            let constraint = (snapshot[index].density * inverse_rest_density - 1.0).max(0.0);
            let mut gradient_i = Vec2::ZERO;
            let mut gradient_norm_sum = 0.0;
            for other in 0..count {
                if index == other {
                    continue;
                }
                let gradient = density_gradient(
                    snapshot[index].predicted_position - snapshot[other].predicted_position,
                    kernel_radius,
                ) * inverse_rest_density;
                gradient_i += gradient;
                gradient_norm_sum +=
                    snapshot[other].inverse_mass.max(0.0) * gradient.length_squared();
            }
            gradient_norm_sum += inverse_mass * gradient_i.length_squared();
            let unconstrained_delta =
                (-constraint - alpha * lambdas[index]) / (gradient_norm_sum + alpha + 1.0e-6);
            let next_lambda = (lambdas[index] + unconstrained_delta).min(0.0);
            delta_lambdas[index] = next_lambda - lambdas[index];
            lambdas[index] = next_lambda;
            particles[index].lambda = lambdas[index];
        }

        let snapshot = *particles;
        let mut corrections = [Vec2::ZERO; MAX_LIQUID_PARTICLES];
        for index in 0..count {
            let mut correction = Vec2::ZERO;
            for other in 0..count {
                if index == other {
                    continue;
                }
                let delta = snapshot[index].predicted_position - snapshot[other].predicted_position;
                let weight = density_kernel(delta, kernel_radius);
                if weight <= 0.0 {
                    continue;
                }
                // Artificial pressure only regularizes an active compression
                // solve. Applying it to an under-dense isolated pair would be an
                // always-on repulsion and make the pet expand while idle.
                let artificial_pressure = if lambdas[index] < 0.0 || lambdas[other] < 0.0 {
                    -scorr_k.clamp(0.0, 0.04)
                        * (weight / reference_weight).powi(scorr_power.clamp(2, 8) as i32)
                } else {
                    0.0
                };
                correction += (delta_lambdas[index] + delta_lambdas[other] + artificial_pressure)
                    * density_gradient(delta, kernel_radius)
                    * inverse_rest_density
                    * snapshot[index].inverse_mass.max(0.0);
            }
            corrections[index] = correction;
        }
        for index in 0..count {
            particles[index].predicted_position += corrections[index];
        }
        if let Some((minimum, maximum)) = containment_bounds {
            for particle in &mut particles[..count] {
                particle.predicted_position = particle.predicted_position.clamp(minimum, maximum);
            }
        }
    }
    update_density_and_surface(particles, count, kernel_radius);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::liquid::{
        density::{calibrate_rest_density, mean_density_error},
        particles::{KERNEL_RADIUS, initialize_particles_with},
    };

    #[test]
    fn density_projection_is_zero_mean_and_nearly_stationary_on_the_calibrated_pack() {
        let (mut particles, count) = initialize_particles_with(117, 96, 0.88);
        let kernel_radius = KERNEL_RADIUS * 1.14;
        let rest_density = calibrate_rest_density(&particles, count, kernel_radius);
        update_density_and_surface(&mut particles, count, kernel_radius);
        let before_center = particles[..count]
            .iter()
            .map(|particle| particle.position)
            .sum::<Vec2>()
            / count as f32;
        for particle in &mut particles[..count] {
            particle.predicted_position = particle.position;
        }
        solve_density_constraints(
            &mut particles,
            count,
            DensityConstraintParameters {
                rest_density,
                kernel_radius,
                compliance: 8.0e-6,
                scorr_k: 0.005,
                scorr_q_ratio: 0.20,
                scorr_power: 4,
                iterations: 6,
                dt: 1.0 / 240.0,
                containment_bounds: None,
            },
        );
        let after_center = particles[..count]
            .iter()
            .map(|particle| particle.predicted_position)
            .sum::<Vec2>()
            / count as f32;
        let maximum_displacement = particles[..count]
            .iter()
            .map(|particle| particle.predicted_position.distance(particle.position))
            .fold(0.0_f32, f32::max);
        let error = mean_density_error(&particles, count, rest_density);
        assert!(after_center.distance(before_center) < 1.0e-5);
        assert!(maximum_displacement < 0.10);
        assert!(error < 0.25);
    }

    #[test]
    fn inequality_projection_reduces_compression_without_attracting_free_surfaces() {
        let (reference, count) = initialize_particles_with(71, 48, 1.0);
        let kernel_radius = KERNEL_RADIUS * 1.14;
        let rest_density = calibrate_rest_density(&reference, count, kernel_radius);
        let mut compressed = reference;
        for particle in &mut compressed[..count] {
            particle.predicted_position = particle.position * 0.78;
        }
        update_density_and_surface(&mut compressed, count, kernel_radius);
        let before_peak = compressed[..count]
            .iter()
            .map(|particle| particle.density / rest_density - 1.0)
            .fold(0.0_f32, f32::max);
        let before_center = compressed[..count]
            .iter()
            .map(|particle| particle.predicted_position)
            .sum::<Vec2>()
            / count as f32;

        solve_density_constraints(
            &mut compressed,
            count,
            DensityConstraintParameters {
                rest_density,
                kernel_radius,
                compliance: 8.0e-6,
                scorr_k: 0.005,
                scorr_q_ratio: 0.20,
                scorr_power: 4,
                iterations: 6,
                dt: 1.0 / 120.0,
                containment_bounds: None,
            },
        );
        let after_peak = compressed[..count]
            .iter()
            .map(|particle| particle.density / rest_density - 1.0)
            .fold(0.0_f32, f32::max);
        let after_center = compressed[..count]
            .iter()
            .map(|particle| particle.predicted_position)
            .sum::<Vec2>()
            / count as f32;
        assert!(
            after_peak < before_peak,
            "before={before_peak}, after={after_peak}"
        );
        assert!(after_center.distance(before_center) < 2.0e-5);

        let mut isolated = [LiquidParticle::default(); MAX_LIQUID_PARTICLES];
        isolated[0].inverse_mass = 1.0;
        isolated[0].predicted_position = Vec2::new(-0.09, 0.0);
        isolated[1].inverse_mass = 1.0;
        isolated[1].predicted_position = Vec2::new(0.09, 0.0);
        let before = [
            isolated[0].predicted_position,
            isolated[1].predicted_position,
        ];
        solve_density_constraints(
            &mut isolated,
            2,
            DensityConstraintParameters {
                rest_density: 10.0,
                kernel_radius: 0.25,
                compliance: 8.0e-6,
                scorr_k: 0.04,
                scorr_q_ratio: 0.20,
                scorr_power: 4,
                iterations: 6,
                dt: 1.0 / 120.0,
                containment_bounds: None,
            },
        );
        assert_eq!(isolated[0].predicted_position, before[0]);
        assert_eq!(isolated[1].predicted_position, before[1]);
    }

    #[test]
    fn multipliers_reset_each_tick_and_containment_is_unilateral() {
        let mut particles = [LiquidParticle::default(); MAX_LIQUID_PARTICLES];
        for (particle, position) in particles[..2]
            .iter_mut()
            .zip([Vec2::new(-0.01, 0.0), Vec2::new(0.01, 0.0)])
        {
            particle.inverse_mass = 1.0;
            particle.predicted_position = position;
        }
        let parameters = DensityConstraintParameters {
            rest_density: 1.05,
            kernel_radius: 0.20,
            compliance: 1.0e-6,
            scorr_k: 0.0,
            scorr_q_ratio: 0.20,
            scorr_power: 4,
            iterations: 6,
            dt: 1.0 / 120.0,
            containment_bounds: Some((Vec2::splat(-0.05), Vec2::splat(0.05))),
        };
        solve_density_constraints(&mut particles, 2, parameters);
        assert!(particles[..2].iter().any(|particle| particle.lambda < 0.0));
        assert!(particles[..2].iter().all(|particle| {
            particle.predicted_position.cmpge(Vec2::splat(-0.05)).all()
                && particle.predicted_position.cmple(Vec2::splat(0.05)).all()
        }));

        particles[0].predicted_position = Vec2::new(-0.05, 0.0);
        particles[1].predicted_position = Vec2::new(0.05, 0.0);
        solve_density_constraints(
            &mut particles,
            2,
            DensityConstraintParameters {
                rest_density: 10.0,
                containment_bounds: None,
                ..parameters
            },
        );
        assert!(particles[..2].iter().all(|particle| particle.lambda == 0.0));
    }
}
