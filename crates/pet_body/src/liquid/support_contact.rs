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
        if old_gap <= 0.012 && new_gap <= 0.025 {
            // A loaded wall absorbs normal motion instead of returning each
            // tiny pressure/breath impulse as a new upward surface wave.
            let slip = (p.predicted_position - p.position).dot(tangent);
            p.predicted_position -= n * new_gap;
            // Dissipate sliding without pinning x: pressure must still spread
            // the footprint as the upper volume settles onto the wall.
            p.predicted_position -= tangent * slip * (1.0 - (-8.0 * dt).exp());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec2;
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
        assert!((particles[0].predicted_position.y - 0.1).abs() < 1e-6);
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
        assert_eq!(
            particles[0].predicted_position.y, 0.15,
            "a real pull must detach"
        );
    }
}
