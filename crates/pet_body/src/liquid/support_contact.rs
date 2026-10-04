//! Dissipative boundary layer of the loaded liquid; the free surface stays dynamic.
use super::{
    particles::{LiquidParticle, MAX_LIQUID_PARTICLES},
    xpbd::SupportPlane,
};

/// Suppress tiny lift/rebound cycles and tangential stirring at a solid contact.
/// A departing body drops the plane; a strong pull can also leave the thin skin.
pub(super) fn settle_contact_layer(
    particles: &mut [LiquidParticle; MAX_LIQUID_PARTICLES],
    count: usize,
    main_component: u8,
    plane: Option<SupportPlane>,
    dt: f32,
) {
    let Some(plane) = plane else {
        return;
    };
    let n = plane.normal.normalize_or_zero();
    if n.length_squared() < 0.5 || dt <= 0.0 {
        return;
    }
    let tangent = n.perp();
    for p in &mut particles[..count] {
        if p.component_id != main_component || p.inverse_mass <= 0.0 {
            continue;
        }
        let old_gap = (p.position - plane.point).dot(n) - plane.clearance;
        let new_gap = (p.predicted_position - plane.point).dot(n) - plane.clearance;
        if old_gap <= 0.012 {
            // A loaded wall absorbs normal motion instead of returning each
            // tiny pressure/breath impulse as a new upward surface wave.
            let slip = (p.predicted_position - p.position).dot(tangent);
            // Only dissipate *new separating motion*. The old implementation
            // snapped every near-wall point back onto the wall, even when
            // the comoving wall receded during takeoff. A 0.006-unit change
            // of reference at 120Hz then injected a 0.72-unit/s downward
            // velocity into an entire row and tore it off the rising body.
            // A unilateral contact cannot pull material across an existing gap.
            let separating_step = (new_gap - old_gap.max(0.0)).max(0.0);
            let proximity = (1.0 - old_gap.max(0.0) / 0.012).clamp(0.0, 1.0);
            // Finite boundary viscosity, not a latch. Cancelling the whole
            // outgoing step held any velocity below 0.025/dt forever and
            // therefore produced a different takeoff threshold at each Hz.
            p.predicted_position -= n * separating_step * proximity * (1.0 - (-24.0 * dt).exp());
            // Dissipate sliding without pinning x: pressure must still spread
            // the footprint as the upper volume settles onto the wall.
            p.predicted_position -= tangent * slip * (1.0 - (-16.0 * dt).exp()) * proximity;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec2;
    #[test]
    fn receding_wall_cannot_suction_stationary_material_or_inject_velocity() {
        let mut particles = [LiquidParticle::default(); MAX_LIQUID_PARTICLES];
        particles[0].inverse_mass = 1.0;
        particles[0].position = Vec2::new(0.0, 0.1);
        particles[0].predicted_position = particles[0].position;
        let initial = particles;
        for hz in [30, 60, 120] {
            for gap in [0.001, 0.003, 0.006, 0.010, 0.020] {
                particles = initial;
                let plane = SupportPlane {
                    point: Vec2::new(0.0, -gap),
                    normal: Vec2::Y,
                    clearance: 0.1,
                };
                settle_contact_layer(&mut particles, 1, 0, Some(plane), 1.0 / hz as f32);
                assert_eq!(
                    particles[0].predicted_position, initial[0].position,
                    "hz={hz} gap={gap}"
                );
            }
        }
    }
    #[test]
    fn genuine_separation_is_damped_at_a_finite_rate_at_every_step_size() {
        for hz in [30, 60, 120] {
            let dt = 1.0 / hz as f32;
            let mut particles = [LiquidParticle::default(); MAX_LIQUID_PARTICLES];
            particles[0].inverse_mass = 1.0;
            particles[0].position = Vec2::new(0.0, 0.1);
            let plane = SupportPlane {
                point: Vec2::ZERO,
                normal: Vec2::Y,
                clearance: 0.1,
            };
            let mut velocity = 0.9;
            for _ in 0..hz / 2 {
                particles[0].predicted_position = particles[0].position + Vec2::Y * velocity * dt;
                settle_contact_layer(&mut particles, 1, 0, Some(plane), dt);
                let outgoing = (particles[0].predicted_position.y - particles[0].position.y) / dt;
                assert!(
                    outgoing > 0.0 && outgoing <= velocity + 1e-5,
                    "hz={hz} v={outgoing}"
                );
                velocity = outgoing;
                particles[0].position = particles[0].predicted_position;
            }
            assert!(
                particles[0].position.y > 0.2,
                "hz={hz}: static wall pinned an upward launch"
            );
        }
    }

    #[test]
    fn loaded_skin_dissipates_rebound_but_releases_and_keeps_bulk_mobile() {
        let mut particles = [LiquidParticle::default(); MAX_LIQUID_PARTICLES];
        let plane = SupportPlane {
            point: Vec2::ZERO,
            normal: Vec2::Y,
            clearance: 0.1,
        };
        for (i, p) in particles[..3].iter_mut().enumerate() {
            p.inverse_mass = 1.0;
            p.position = Vec2::new(i as f32 * 0.08, 0.1 + i as f32 * 0.1);
            p.predicted_position = p.position + Vec2::new(0.008, 0.006);
        }
        let bulk = particles[1].predicted_position;
        particles[2].component_id = 1;
        let detached = particles[2].predicted_position;
        settle_contact_layer(&mut particles, 3, 0, Some(plane), 1.0 / 120.0);
        assert!(
            particles[0].predicted_position.y > 0.1 && particles[0].predicted_position.y < 0.106
        );
        assert!(
            particles[0].predicted_position.x > 0.007 && particles[0].predicted_position.x < 0.008
        );
        assert_eq!(particles[1].predicted_position, bulk);
        assert_eq!(particles[2].predicted_position, detached);
        let before = particles;
        settle_contact_layer(&mut particles, 3, 0, None, 1.0 / 120.0);
        assert_eq!(particles, before);
        particles[0].predicted_position.y = 0.15;
        settle_contact_layer(&mut particles, 3, 0, Some(plane), 1.0 / 120.0);
        assert!(
            particles[0].predicted_position.y > 0.14,
            "a real pull must detach"
        );
    }
}
