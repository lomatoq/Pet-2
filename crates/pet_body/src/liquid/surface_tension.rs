use glam::Vec2;

use super::{
    kernels::density_gradient,
    particles::{LiquidParticle, MAX_LIQUID_PARTICLES},
};

/// Applies a compact 2D adaptation of Akinci-style particle cohesion.
///
/// Neighbours are discovered afresh from the current positions and the kernel
/// has no authored rest distance or persistent graph. Its close-range negative
/// lobe prevents coincident particles; its positive outer lobe minimizes exposed
/// surface. Every pair receives equal-and-opposite acceleration, so cohesion
/// cannot move the creature's centre of mass.
#[cfg(test)]
pub fn apply_surface_tension(
    particles: &mut [LiquidParticle; MAX_LIQUID_PARTICLES],
    count: usize,
    kernel_radius: f32,
    rest_density: f32,
    tension: f32,
) {
    apply_surface_tension_at_wall(particles, count, kernel_radius, rest_density, tension, None);
}

/// The solid-liquid interface has its own energy. Do not reduce cohesion in
/// the upper mass just because another particle touched a support surface.
pub(super) fn apply_surface_tension_at_wall(
    particles: &mut [LiquidParticle; MAX_LIQUID_PARTICLES],
    count: usize,
    kernel_radius: f32,
    rest_density: f32,
    tension: f32,
    wall: Option<super::xpbd::SupportPlane>,
) {
    let count = count.min(MAX_LIQUID_PARTICLES);
    let tension = tension.clamp(0.0, 2.5);
    if count < 2 || kernel_radius <= f32::EPSILON || tension <= f32::EPSILON {
        return;
    }

    let snapshot = *particles;
    let rest_density = rest_density.max(0.01);
    let mut normals = [Vec2::ZERO; MAX_LIQUID_PARTICLES];
    for first in 0..count {
        for second in 0..count {
            if first == second {
                continue;
            }
            let neighbor_inverse_mass = snapshot[second].inverse_mass.max(0.0);
            if neighbor_inverse_mass <= f32::EPSILON {
                continue;
            }
            let neighbor_mass = neighbor_inverse_mass.recip();
            let neighbor_density = snapshot[second].density.max(rest_density * 0.25);
            normals[first] += neighbor_mass / neighbor_density
                * density_gradient(
                    snapshot[first].position - snapshot[second].position,
                    kernel_radius,
                );
        }
        // Akinci's color-field normal is scaled by the support radius. The
        // density and gradient kernels omit the same normalization constant, so
        // their ratio remains the useful dimensionless 2D estimate here.
        normals[first] *= kernel_radius;
    }

    let mut accelerations = [Vec2::ZERO; MAX_LIQUID_PARTICLES];
    let support_squared = kernel_radius * kernel_radius;
    for first in 0..count {
        for second in (first + 1)..count {
            let delta = snapshot[second].position - snapshot[first].position;
            let distance_squared = delta.length_squared();
            if distance_squared <= 1.0e-12 || distance_squared >= support_squared {
                continue;
            }
            let distance = distance_squared.sqrt();
            let q = distance / kernel_radius;
            let cohesion = akinci_cohesion_2d(q);
            let density_correction = (2.0 * rest_density
                / (snapshot[first].density + snapshot[second].density).max(rest_density * 0.5))
            .clamp(0.5, 2.0);
            // The official Akinci force combines a cohesion kernel with the
            // difference of color-field normals (curvature), then applies K_ij.
            // These weights only normalize the two dimensionless 2D terms so the
            // existing slider remains an acceleration-scale authoring control.
            let cohesion_term = delta / distance * cohesion * 0.85;
            let curvature_term = -(normals[first] - normals[second]) * 0.15;
            let interface = wall.map_or(0.0, |wall| {
                let distance =
                    |p: Vec2| ((p - wall.point).dot(wall.normal) - wall.clearance).max(0.0);
                let q = (distance(snapshot[first].position)
                    .min(distance(snapshot[second].position))
                    / (kernel_radius * 0.70))
                    .clamp(0.0, 1.0);
                1.0 - q * q * (3.0 - 2.0 * q)
            });
            let pair_tension = tension + (tension.min(0.48) - tension) * interface;
            let pair_acceleration =
                (cohesion_term + curvature_term) * (pair_tension * density_correction);
            accelerations[first] += pair_acceleration;
            accelerations[second] -= pair_acceleration;
        }
    }

    for (particle, acceleration) in particles[..count]
        .iter_mut()
        .zip(accelerations[..count].iter().copied())
    {
        particle.force += acceleration;
    }
}

/// Dimensionless form of the compact cohesion spline. Removing the dimensional
/// normalization makes the tuning value an acceleration scale in body units.
fn akinci_cohesion_2d(q: f32) -> f32 {
    if !(0.0..1.0).contains(&q) {
        return 0.0;
    }
    let polynomial = (1.0 - q).powi(3) * q.powi(3);
    let value = if q > 0.5 {
        polynomial
    } else {
        2.0 * polynomial - 1.0 / 64.0
    };
    // The source spline reaches 1/64 at q=0.5. Normalize that peak to one so
    // `tension` has a useful, resolution-independent gameplay meaning.
    value * 64.0
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn wall_wetting_is_local_pair_symmetric_and_air_is_exactly_unchanged() {
        let mut particles = [LiquidParticle::default(); MAX_LIQUID_PARTICLES];
        for (i, p) in particles[..4].iter_mut().enumerate() {
            p.position = Vec2::new((i % 2) as f32 * 0.12, (i / 2) as f32 * 0.8);
            p.inverse_mass = 1.0;
            p.density = 1.0;
        }
        let mut free = particles;
        let mut near = particles;
        let mut far = particles;
        let wall = super::super::xpbd::SupportPlane {
            point: Vec2::ZERO,
            normal: Vec2::Y,
            clearance: 0.0,
        };
        apply_surface_tension(&mut free, 4, 0.2, 1.0, 2.0);
        apply_surface_tension_at_wall(&mut near, 4, 0.2, 1.0, 2.0, Some(wall));
        apply_surface_tension_at_wall(
            &mut far,
            4,
            0.2,
            1.0,
            2.0,
            Some(super::super::xpbd::SupportPlane {
                point: -Vec2::Y,
                ..wall
            }),
        );
        assert_eq!(free, far, "a far surface changed the bulk material");
        assert_eq!(near[2].force, free[2].force);
        assert_eq!(near[3].force, free[3].force);
        assert_eq!(near[0].force, -near[1].force);
        assert!(near[0].force.length() < free[0].force.length() * 0.3);
    }

    #[test]
    fn cohesion_is_pair_symmetric_and_compact() {
        let mut particles = [LiquidParticle::default(); MAX_LIQUID_PARTICLES];
        particles[0].position = Vec2::ZERO;
        particles[1].position = Vec2::new(0.12, 0.0);
        particles[0].inverse_mass = 1.0;
        particles[1].inverse_mass = 1.0;
        particles[0].density = 1.0;
        particles[1].density = 1.0;
        apply_surface_tension(&mut particles, 2, 0.20, 1.0, 0.8);
        assert!(particles[0].force.x > 0.0);
        assert_eq!(particles[0].force, -particles[1].force);

        particles[0].force = Vec2::ZERO;
        particles[1].force = Vec2::ZERO;
        particles[1].position = Vec2::new(0.20, 0.0);
        apply_surface_tension(&mut particles, 2, 0.20, 1.0, 0.8);
        assert_eq!(particles[0].force, Vec2::ZERO);
        assert_eq!(particles[1].force, Vec2::ZERO);
    }

    #[test]
    fn close_particles_are_separated_without_an_authored_rest_length() {
        let mut particles = [LiquidParticle::default(); MAX_LIQUID_PARTICLES];
        particles[0].position = Vec2::ZERO;
        particles[1].position = Vec2::new(0.02, 0.0);
        particles[0].inverse_mass = 1.0;
        particles[1].inverse_mass = 1.0;
        particles[0].density = 1.0;
        particles[1].density = 1.0;
        apply_surface_tension(&mut particles, 2, 0.20, 1.0, 1.0);
        assert!(particles[0].force.x < 0.0);
        assert_eq!(particles[0].force, -particles[1].force);
        assert!(
            particles[..2]
                .iter()
                .all(|particle| particle.force.is_finite())
        );
    }

    #[test]
    fn curvature_and_kij_remain_pair_symmetric() {
        let mut particles = [LiquidParticle::default(); MAX_LIQUID_PARTICLES];
        for (particle, position) in particles[..3].iter_mut().zip([
            Vec2::new(-0.08, 0.0),
            Vec2::new(0.02, 0.03),
            Vec2::new(0.09, -0.02),
        ]) {
            particle.position = position;
            particle.inverse_mass = 1.0;
            particle.density = 1.0;
        }
        apply_surface_tension(&mut particles, 3, 0.20, 1.0, 0.8);
        let net = particles[..3]
            .iter()
            .map(|particle| particle.force)
            .sum::<Vec2>();
        assert!(
            net.length() < 1.0e-6,
            "surface tension changed COM: {net:?}"
        );
        assert!(
            particles[..3]
                .iter()
                .all(|particle| particle.force.is_finite())
        );
    }
}
