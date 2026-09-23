//! Finite-thickness bowl contact in desktop pixels, open above and below its base.
use crate::cradle_runtime::CradleGeometry;
use glam::Vec2;
use pet_ecology::{ObjectKind, ObjectLifecycle, WorldObject};

pub fn constrain(
    object: &mut WorldObject,
    previous: Vec2,
    anchor: Vec2,
    scale: f32,
    viewport: [u32; 2],
) -> bool {
    if object.kind != ObjectKind::Orb {
        return false;
    }
    let extent = Vec2::new(viewport[0].max(1) as f32, viewport[1].max(1) as f32);
    let g = CradleGeometry::new(anchor * extent, viewport, scale);
    let radius =
        object.radius_px_at_reference / pet_ecology::REFERENCE_DESKTOP_HEIGHT_PX * extent.y;
    let width = g.half_width / 0.285;
    let outer = width * 0.45;
    let bottom = g.anchor.y + width * 0.292;
    let lip = g.anchor.y - width * 0.015;
    let boxes = [
        (
            Vec2::new(g.anchor.x - outer, lip),
            Vec2::new(g.anchor.x - g.half_width, bottom),
        ),
        (
            Vec2::new(g.anchor.x + g.half_width, lip),
            Vec2::new(g.anchor.x + outer, bottom),
        ),
        (
            Vec2::new(g.anchor.x - outer, g.floor),
            Vec2::new(g.anchor.x + outer, bottom),
        ),
    ];
    let mut position = previous * extent;
    let mut goal = object.position * extent;
    if !position.is_finite() || !goal.is_finite() || !radius.is_finite() {
        return false;
    }
    if object.lifecycle == ObjectLifecycle::StoredInDen {
        // Repair old saved overlap and radius changes without allowing the bottom exit.
        let limit = (g.half_width - radius).max(0.0);
        for p in [&mut position, &mut goal] {
            p.x = p.x.clamp(g.anchor.x - limit, g.anchor.x + limit);
            p.y = p.y.min(g.floor - radius);
        }
    }
    let delta = goal - position;
    // Geometric substeps test the entire swept path, including pointer motion.
    // Two pixels is smaller than every wall and the minimum rendered orb radius.
    let count = (delta.length() / 2.0).ceil().max(1.0) as usize;
    let step = delta / count as f32;
    let mut velocity = object.velocity * extent.y;
    let mut touched = false;
    for _ in 0..count {
        position += step;
        for _ in 0..2 {
            for (min, max) in boxes {
                let nearest = position.clamp(min, max);
                let distance = position - nearest;
                let length = distance.length();
                if length >= radius {
                    continue;
                }
                let (normal, depth) = if length > 0.0001 {
                    (distance / length, radius - length)
                } else {
                    let candidates = [
                        (position.x - min.x, Vec2::NEG_X),
                        (max.x - position.x, Vec2::X),
                        (position.y - min.y, Vec2::NEG_Y),
                        (max.y - position.y, Vec2::Y),
                    ];
                    let (d, n) = candidates
                        .into_iter()
                        .min_by(|a, b| a.0.total_cmp(&b.0))
                        .unwrap();
                    (n, radius + d)
                };
                position += normal * depth;
                let inward = velocity.dot(normal).min(0.0);
                velocity -= normal * inward;
                touched = true;
            }
        }
    }
    object.position = position / extent;
    object.velocity = velocity / extent.y;
    touched
}

#[cfg(test)]
mod tests {
    use super::*;
    fn orb() -> WorldObject {
        pet_ecology::EcologyState::new(42).objects[0].clone()
    }
    #[test]
    fn fast_inside_motion_cannot_cross_floor_or_side_walls_but_can_exit_above() {
        for destination in [
            Vec2::new(100.0, 310.0),
            Vec2::new(520.0, 310.0),
            Vec2::new(300.0, 650.0),
            Vec2::new(300.0, 100.0),
        ] {
            let mut o = orb();
            o.lifecycle = ObjectLifecycle::Free;
            o.radius_px_at_reference = 25.0;
            let extent = Vec2::new(800.0, 1080.0);
            let previous = Vec2::new(300.0, 315.0) / extent;
            o.position = destination / extent;
            o.velocity = (destination - previous * extent) / 1080.0;
            constrain(
                &mut o,
                previous,
                Vec2::new(300.0, 300.0) / extent,
                1.0,
                [800, 1080],
            );
            let p = o.position * extent;
            if destination.y < 200.0 {
                assert!((p - destination).length() < 0.01);
            } else {
                assert!(p.x > 235.0 && p.x < 365.0, "side {p:?}");
                assert!(p.y < 345.0, "floor {p:?}");
            }
        }
    }
    #[test]
    fn outside_orb_can_pass_under_the_base_without_being_captured() {
        let mut o = orb();
        o.lifecycle = ObjectLifecycle::Free;
        let extent = Vec2::new(800.0, 1080.0);
        let start = Vec2::new(100.0, 460.0) / extent;
        o.position = Vec2::new(550.0, 460.0) / extent;
        let target = o.position;
        assert!(!constrain(
            &mut o,
            start,
            Vec2::new(300.0, 300.0) / extent,
            1.0,
            [800, 1080]
        ));
        assert!((o.position - target).length() < 0.001);
    }
}
