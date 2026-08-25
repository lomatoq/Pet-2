use glam::Vec2;

use super::particles::{LiquidParticle, MAX_LIQUID_PARTICLES};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CharacterFieldParameters {
    /// Elliptical equipotential radii in body-local units.
    pub radii: Vec2,
    /// Asymptotic acceleration of the permanent character well.
    pub well_acceleration: f32,
    /// Scale applied to the comoving-frame inertial load.
    pub inertia_scale: f32,
    /// Hard physical bound on inertial acceleration, not a particle speed cap.
    pub maximum_inertial_acceleration: f32,
    /// Weak drag relative to the character field's comoving frame, in s^-1.
    /// This dissipates release energy without a COM or target-velocity servo.
    pub velocity_damping: f32,
    /// Body-local major axis of the continuous flight field. The axis is
    /// unoriented; reversing flight cannot flip the field.
    pub flight_axis: Vec2,
    /// Area-preserving major/minor aspect ratio. One is the neutral ellipse.
    pub flight_aspect: f32,
}

impl Default for CharacterFieldParameters {
    fn default() -> Self {
        Self {
            radii: Vec2::new(0.35, 0.43),
            well_acceleration: 3.0,
            inertia_scale: 0.90,
            maximum_inertial_acceleration: 6.0,
            velocity_damping: 1.4,
            flight_axis: Vec2::Y,
            flight_aspect: 1.0,
        }
    }
}

fn elliptical_metric(
    local: Vec2,
    radii: Vec2,
    flight_axis: Vec2,
    flight_aspect: f32,
) -> (f32, Vec2) {
    let normalized = local / radii;
    let physical_axis = if flight_axis.is_finite() {
        flight_axis.normalize_or_zero()
    } else {
        Vec2::ZERO
    };
    let physical_axis = if physical_axis.length_squared() > 1.0e-8 {
        physical_axis
    } else {
        Vec2::Y
    };
    // Convert a requested physical direction into the normalized ellipse's
    // basis. Multiplying this basis by `radii` maps it back to the requested
    // body-local axis even when the neutral well is not circular.
    let mut metric_axis = (physical_axis / radii).normalize_or_zero();
    if metric_axis.length_squared() <= 1.0e-8 {
        metric_axis = Vec2::Y;
    }
    let metric_perpendicular = Vec2::new(-metric_axis.y, metric_axis.x);
    let aspect = if flight_aspect.is_finite() {
        flight_aspect.clamp(1.0, 1.34)
    } else {
        1.0
    };
    let major = normalized.dot(metric_axis);
    let minor = normalized.dot(metric_perpendicular);
    let radius_squared = major * major / aspect + minor * minor * aspect;
    let normalized_gradient =
        metric_axis * (major / aspect) + metric_perpendicular * (minor * aspect);
    (radius_squared.max(0.0).sqrt(), normalized_gradient / radii)
}

/// Samples the one permanent character field and the comoving-frame inertial
/// load for every real particle.
///
/// This function deliberately has no component summary, centre-of-mass target,
/// face cohort, rest pose, or velocity target. Splitting and rejoining therefore
/// cannot switch the forces acting on a particle.
pub fn apply_character_field(
    particles: &mut [LiquidParticle; MAX_LIQUID_PARTICLES],
    count: usize,
    body_origin: Vec2,
    body_acceleration: Vec2,
    parameters: CharacterFieldParameters,
) {
    let count = count.min(MAX_LIQUID_PARTICLES);
    if count == 0 {
        return;
    }

    let radii = Vec2::new(
        parameters.radii.x.abs().max(1.0e-4),
        parameters.radii.y.abs().max(1.0e-4),
    );
    let well_acceleration = parameters.well_acceleration.max(0.0);
    let inertial_load = (-body_acceleration * parameters.inertia_scale.max(0.0))
        .clamp_length_max(parameters.maximum_inertial_acceleration.max(0.0));

    for particle in &mut particles[..count] {
        let local = particle.position - body_origin;
        let (elliptical_radius, metric_gradient) = elliptical_metric(
            local,
            radii,
            parameters.flight_axis,
            parameters.flight_aspect,
        );
        // Smooth harmonic behaviour at the center, asymptotically bounded far
        // away. This is the analytic gradient of
        // A*r_min*(sqrt(1 + q^2) - 1), so the well itself cannot add energy.
        // There is no activation radius or component-dependent return mode.
        let well_force = (-metric_gradient * well_acceleration * radii.min_element()
            / (1.0 + elliptical_radius.powi(2)).sqrt())
        .clamp_length_max(well_acceleration);
        particle.force += well_force + inertial_load;
        particle.force -= particle.velocity * parameters.velocity_damping.max(0.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn character_well_is_continuous_bounded_and_component_agnostic() {
        let mut particles = [LiquidParticle::default(); MAX_LIQUID_PARTICLES];
        for (index, x) in [-0.70_f32, -0.07, 0.07, 0.70].into_iter().enumerate() {
            particles[index].position = Vec2::new(x, 0.0);
            particles[index].component_id = index as u8;
        }
        let parameters = CharacterFieldParameters::default();
        apply_character_field(&mut particles, 4, Vec2::ZERO, Vec2::ZERO, parameters);

        assert!(particles[0].force.x > 0.0);
        assert!(particles[1].force.x > 0.0);
        assert!(particles[2].force.x < 0.0);
        assert!(particles[3].force.x < 0.0);
        assert!(
            particles[..4]
                .iter()
                .all(|particle| particle.force.length() <= parameters.well_acceleration + 1.0e-5)
        );
        assert!(particles[1].force.length() < particles[0].force.length());
        assert_eq!(particles[0].force, -particles[3].force);
        assert_eq!(particles[1].force, -particles[2].force);
    }

    #[test]
    fn flight_inertia_is_one_uniform_all_particle_load() {
        let mut without_acceleration = [LiquidParticle::default(); MAX_LIQUID_PARTICLES];
        without_acceleration[0].position = Vec2::new(-0.18, 0.10);
        without_acceleration[1].position = Vec2::new(0.25, -0.12);
        without_acceleration[0].component_id = 3;
        without_acceleration[1].component_id = 8;
        without_acceleration[0].face_weight = 1.0;
        without_acceleration[1].rest_position = Vec2::splat(10.0);
        let mut with_acceleration = without_acceleration;
        let parameters = CharacterFieldParameters::default();

        apply_character_field(
            &mut without_acceleration,
            2,
            Vec2::ZERO,
            Vec2::ZERO,
            parameters,
        );
        apply_character_field(
            &mut with_acceleration,
            2,
            Vec2::ZERO,
            Vec2::new(2.0, -1.0),
            parameters,
        );

        let first_load = with_acceleration[0].force - without_acceleration[0].force;
        let second_load = with_acceleration[1].force - without_acceleration[1].force;
        assert!(first_load.x < 0.0 && first_load.y > 0.0);
        assert!(first_load.distance(second_load) < 1.0e-6);
        assert!(first_load.length() <= parameters.maximum_inertial_acceleration + 1.0e-5);
    }

    #[test]
    fn field_relative_damping_is_continuous_and_has_no_velocity_target() {
        let mut particles = [LiquidParticle::default(); MAX_LIQUID_PARTICLES];
        particles[0].velocity = Vec2::new(0.8, -0.3);
        particles[0].component_id = 1;
        particles[1].velocity = Vec2::new(0.8, -0.3);
        particles[1].component_id = 19;
        let parameters = CharacterFieldParameters {
            well_acceleration: 0.0,
            inertia_scale: 0.0,
            velocity_damping: 1.5,
            ..CharacterFieldParameters::default()
        };
        apply_character_field(&mut particles, 2, Vec2::ZERO, Vec2::ZERO, parameters);
        let expected = Vec2::new(-1.2, 0.45);
        assert!(particles[0].force.distance(expected) < 1.0e-6);
        assert_eq!(particles[0].force, particles[1].force);
    }

    #[test]
    fn flight_metric_is_area_preserving_and_flattens_across_travel() {
        let aspect: f32 = 1.30;
        let major_radius = aspect.sqrt();
        let minor_radius = 1.0 / major_radius;
        assert!((major_radius * minor_radius - 1.0).abs() < 1.0e-6);

        let (major_boundary, _) =
            elliptical_metric(Vec2::X * major_radius, Vec2::ONE, Vec2::X, aspect);
        let (minor_boundary, _) =
            elliptical_metric(Vec2::Y * minor_radius, Vec2::ONE, Vec2::X, aspect);
        assert!((major_boundary - 1.0).abs() < 1.0e-6);
        assert!((minor_boundary - 1.0).abs() < 1.0e-6);

        let mut particles = [LiquidParticle::default(); MAX_LIQUID_PARTICLES];
        particles[0].position = Vec2::new(0.25, 0.0);
        particles[1].position = Vec2::new(0.0, 0.25);
        let parameters = CharacterFieldParameters {
            radii: Vec2::ONE,
            flight_axis: Vec2::X,
            flight_aspect: aspect,
            velocity_damping: 0.0,
            ..CharacterFieldParameters::default()
        };
        apply_character_field(&mut particles, 2, Vec2::ZERO, Vec2::ZERO, parameters);

        assert!(
            particles[0].force.length() < particles[1].force.length(),
            "expanded axis must be softer than the compressed axis"
        );
        assert!(
            particles[..2]
                .iter()
                .all(|particle| particle.force.length() <= parameters.well_acceleration + 1.0e-5)
        );
    }

    #[test]
    fn neutral_flight_metric_is_exactly_the_original_ellipse() {
        let radii = Vec2::new(0.35, 0.43);
        for local in [
            Vec2::new(-0.21, 0.13),
            Vec2::new(0.0, 0.32),
            Vec2::new(0.29, -0.08),
        ] {
            let (radius, gradient) = elliptical_metric(local, radii, Vec2::Y, 1.0);
            assert!((radius - (local / radii).length()).abs() < 1.0e-6);
            let original_gradient = Vec2::new(local.x / radii.x.powi(2), local.y / radii.y.powi(2));
            assert!(gradient.distance(original_gradient) < 1.0e-6);
        }
    }
}
