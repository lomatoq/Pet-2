//! Small, conservative material motion in the production PBF lane.
//! Uses the existing integrated breathing phase; never moves the face/root directly.
use super::{
    particles::{LiquidParticle, MAX_LIQUID_PARTICLES},
    xpbd::SupportPlane,
};
use crate::VisualMindInput;
use glam::Vec2;

#[allow(clippy::too_many_arguments)]
pub(super) fn apply(
    particles: &mut [LiquidParticle; MAX_LIQUID_PARTICLES],
    count: usize,
    main_component: u8,
    origin: Vec2,
    phase: f32,
    mut mind: VisualMindInput,
    multiplier: f32,
    support: Option<SupportPlane>,
    travel_speed: f32,
) -> f32 {
    if count == 0
        || count > MAX_LIQUID_PARTICLES
        || !origin.is_finite()
        || !phase.is_finite()
        || !multiplier.is_finite()
        || multiplier <= 0.0
        || !travel_speed.is_finite()
    {
        return 0.0;
    }
    mind.sanitize();
    let amplitude = 0.055
        * multiplier.clamp(0.0, 2.0)
        * (1.0 - mind.fatigue * 0.85)
        * (1.0 - mind.stress * 0.6)
        / (1.0 + travel_speed.max(0.0).powi(2) * 12.0)
        * if support.is_some() { 0.18 } else { 1.0 };
    let mut mobility = [0.0_f32; MAX_LIQUID_PARTICLES];
    let mut mass = [0.0_f32; MAX_LIQUID_PARTICLES];
    let mut forces = [Vec2::ZERO; MAX_LIQUID_PARTICLES];
    let mut weight = 0.0;
    let mut center = Vec2::ZERO;
    for (i, p) in particles[..count].iter().enumerate() {
        if p.component_id != main_component || !p.is_finite() || p.inverse_mass <= 0.0 {
            continue;
        }
        mass[i] = p.inverse_mass.recip();
        // The loaded skin and detached parcels get no decorative acceleration.
        // The same mobility multiplies ALL later projections so compensation
        // cannot put forces back into the protected contact layer.
        let free = support.map_or(1.0, |plane| {
            let gap =
                (p.position - plane.point).dot(plane.normal.normalize_or_zero()) - plane.clearance;
            let x = ((gap - 0.10) / 0.16).clamp(0.0, 1.0);
            x * x * (3.0 - 2.0 * x)
        });
        mobility[i] = free * (1.0 - p.face_weight.clamp(0.0, 1.0) * 0.35);
        let w = mass[i] * mobility[i];
        weight += w;
        center += p.position * w;
    }
    if weight < 1e-5 {
        return 0.0;
    }
    center /= weight;
    let mut net = Vec2::ZERO;
    for (i, p) in particles[..count].iter().enumerate() {
        if mobility[i] == 0.0 {
            continue;
        }
        let r = p.position - origin;
        let theta = r.y.atan2(r.x);
        let falloff = (-r.length_squared() / 0.32).exp();
        let radial = r.normalize_or_zero();
        let lobes = (2.0 * theta - phase).sin() + 0.28 * (3.0 * theta + 2.0 * phase).sin();
        let mut curl = Vec2::ZERO;
        for (offset, spin) in [
            (Vec2::new(-0.11, 0.08), 1.0),
            (Vec2::new(0.13, -0.06), -0.80),
        ] {
            let d = r - offset;
            curl += d.perp() * (-d.length_squared() / 0.06).exp() * spin;
        }
        forces[i] = (radial * lobes * falloff * 0.34 + curl) * amplitude * mobility[i];
        net += forces[i] * mass[i];
    }
    let mean = net / weight;
    let mut torque = 0.0;
    let mut dilation = 0.0;
    let mut inertia = 0.0;
    for (i, p) in particles[..count].iter().enumerate() {
        if mobility[i] == 0.0 {
            continue;
        }
        forces[i] -= mean * mobility[i];
        let arm = p.position - center;
        torque += arm.perp_dot(forces[i]) * mass[i];
        dilation += arm.dot(forces[i]) * mass[i];
        inertia += arm.length_squared() * mass[i] * mobility[i];
    }
    if inertia < 1e-6 {
        return 0.0;
    }
    let mut energy = 0.0;
    for (i, p) in particles[..count].iter_mut().enumerate() {
        if mobility[i] == 0.0 {
            continue;
        }
        let arm = p.position - center;
        let correction = (arm.perp() * torque + arm * dilation) / inertia * mobility[i];
        let acceleration = forces[i] - correction;
        p.force += acceleration;
        energy += acceleration.length_squared() * mass[i];
    }
    energy
}

#[cfg(test)]
mod tests {
    use super::super::particles::initialize_particles;
    use super::*;
    #[test]
    fn live_modes_change_without_translation_spin_or_bulk_pumping() {
        let mut signatures = Vec::new();
        for phase in [0.0, 1.0, 2.7] {
            let (mut p, n) = initialize_particles(42);
            for (i, p) in p[..n].iter_mut().enumerate() {
                p.inverse_mass = if i % 2 == 0 { 0.5 } else { 1.0 };
            }
            let energy = apply(
                &mut p,
                n,
                0,
                Vec2::ZERO,
                phase,
                VisualMindInput::default(),
                1.0,
                None,
                0.0,
            );
            let net = p[..n]
                .iter()
                .map(|p| p.force / p.inverse_mass)
                .sum::<Vec2>();
            let torque: f32 = p[..n]
                .iter()
                .map(|p| p.position.perp_dot(p.force) / p.inverse_mass)
                .sum();
            let dilation: f32 = p[..n]
                .iter()
                .map(|p| p.position.dot(p.force) / p.inverse_mass)
                .sum();
            assert!(energy > 1e-7 && energy < 0.2, "{energy}");
            assert!(net.length() < 1e-5, "{net:?}");
            assert!(torque.abs() < 1e-5 && dilation.abs() < 1e-5);
            signatures.push(p[0].force);
        }
        assert!(signatures[0].distance(signatures[1]) > 1e-5);
    }
    #[test]
    fn contact_skin_and_detached_material_are_not_actuated() {
        let (mut p, n) = initialize_particles(42);
        p[0].component_id = 1;
        let plane = SupportPlane {
            point: Vec2::new(0.0, -0.25),
            normal: Vec2::Y,
            clearance: 0.0,
        };
        let energy = apply(
            &mut p,
            n,
            0,
            Vec2::ZERO,
            1.0,
            VisualMindInput::default(),
            1.0,
            Some(plane),
            0.0,
        );
        assert!(energy > 0.0);
        assert_eq!(p[0].force, Vec2::ZERO);
        for p in &p[..n] {
            if p.position.y <= -0.15 {
                assert_eq!(p.force, Vec2::ZERO);
            }
        }
        let net = p[..n]
            .iter()
            .map(|p| p.force / p.inverse_mass)
            .sum::<Vec2>();
        assert!(net.length() < 1e-5);
    }
    #[test]
    fn off_and_invalid_inputs_add_no_force() {
        let (mut p, n) = initialize_particles(42);
        let before = p;
        assert_eq!(
            apply(
                &mut p,
                n,
                0,
                Vec2::ZERO,
                0.0,
                VisualMindInput::default(),
                0.0,
                None,
                0.0
            ),
            0.0
        );
        assert_eq!(
            apply(
                &mut p,
                n,
                0,
                Vec2::ZERO,
                f32::NAN,
                VisualMindInput::default(),
                1.0,
                None,
                0.0
            ),
            0.0
        );
        assert_eq!(p, before);
    }
}
