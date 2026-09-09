use glam::Vec2;

use crate::{BehaviorContextFrame, SurfaceCandidate, SurfaceTarget};

#[must_use]
pub fn rank_surface(
    context: &BehaviorContextFrame,
    tired: bool,
    social: bool,
) -> Option<SurfaceTarget> {
    context
        .surfaces
        .iter()
        .filter_map(|candidate| score_surface(candidate, context, tired, social))
        .max_by(|left, right| left.score.total_cmp(&right.score))
}

fn score_surface(
    candidate: &SurfaceCandidate,
    context: &BehaviorContextFrame,
    tired: bool,
    social: bool,
) -> Option<SurfaceTarget> {
    if tired && candidate.surface_id.0 != "screen:bottom_edge" {
        return None;
    }
    if !candidate.minimum.is_finite()
        || !candidate.maximum.is_finite()
        || !candidate.maximum.cmpgt(candidate.minimum).all()
    {
        return None;
    }
    let width = (candidate.maximum.x - candidate.minimum.x).clamp(0.0, 1.0);
    let height = (candidate.maximum.y - candidate.minimum.y).clamp(0.0, 1.0);
    let body = context.body.motion.world_position;
    let edges = [
        (
            Vec2::new(
                body.x.clamp(candidate.minimum.x, candidate.maximum.x),
                candidate.minimum.y,
            ),
            Vec2::NEG_Y,
            width,
        ),
        (
            Vec2::new(
                body.x.clamp(candidate.minimum.x, candidate.maximum.x),
                candidate.maximum.y,
            ),
            Vec2::Y,
            width,
        ),
        (
            Vec2::new(
                candidate.minimum.x,
                body.y.clamp(candidate.minimum.y, candidate.maximum.y),
            ),
            Vec2::NEG_X,
            height,
        ),
        (
            Vec2::new(
                candidate.maximum.x,
                body.y.clamp(candidate.minimum.y, candidate.maximum.y),
            ),
            Vec2::X,
            height,
        ),
    ];
    // A tired body must roost on a load-bearing horizontal top. A vertical
    // window side is a cling target, not a visible sitting/sleeping pose.
    let (anchor, normal, clearance) = edges
        .into_iter()
        .filter(|(_, normal, _)| !tired || normal.y < -0.5)
        .min_by(|left, right| {
            body.distance_squared(left.0)
                .total_cmp(&body.distance_squared(right.0))
        })?;
    let tangent = Vec2::new(-normal.y, normal.x);
    let distance = body.distance(anchor);
    let den_bonus = context.den_anchor.map_or(0.0, |den| {
        if tired {
            (1.0 - den.distance(anchor) * 3.0).clamp(0.0, 1.0) * 0.32
        } else {
            0.0
        }
    });
    let user_bonus = if social {
        (1.0 - context.cursor_position.distance(anchor) * 2.0).clamp(0.0, 1.0) * 0.18
    } else {
        0.0
    };
    let stability = (1.0 - candidate.velocity.length() * 4.0).clamp(0.0, 1.0);
    let score = clearance.clamp(0.0, 0.45) * 1.2
        + stability * 0.70
        + candidate.familiarity.clamp(0.0, 1.0) * 0.35
        + den_bonus
        + user_bonus
        - distance * 0.55
        - f32::from(candidate.recent_failed_landings) * 0.18;
    Some(SurfaceTarget {
        surface_id: candidate.surface_id.clone(),
        anchor_point: anchor.clamp(Vec2::ZERO, Vec2::ONE),
        normal,
        tangent,
        center_clearance: if normal.y < -0.5 {
            context.body_bottom_extent.clamp(0.012, 0.25)
        } else {
            0.038
        },
        score,
    })
}

#[cfg(test)]
mod tests {
    use crate::BehaviorTarget;
    use lifecore::SurfaceId;

    use super::*;

    #[test]
    fn stable_familiar_surface_beats_fast_near_surface() {
        let mut context = BehaviorContextFrame::default();
        context.body.motion.world_position = Vec2::new(0.5, 0.4);
        context.surfaces = vec![
            SurfaceCandidate {
                surface_id: SurfaceId("fast".into()),
                minimum: Vec2::new(0.2, 0.5),
                maximum: Vec2::new(0.8, 0.8),
                velocity: Vec2::new(0.8, 0.0),
                familiarity: 0.0,
                recent_failed_landings: 0,
            },
            SurfaceCandidate {
                surface_id: SurfaceId("stable".into()),
                minimum: Vec2::new(0.1, 0.58),
                maximum: Vec2::new(0.9, 0.9),
                velocity: Vec2::ZERO,
                familiarity: 0.8,
                recent_failed_landings: 0,
            },
        ];
        assert_eq!(
            rank_surface(&context, false, false)
                .expect("surface")
                .surface_id
                .0,
            "stable"
        );
    }

    #[test]
    fn tired_body_uses_top_edge_and_real_bottom_metaball_extent() {
        let mut context = BehaviorContextFrame::default();
        context.body.motion.world_position = Vec2::new(0.82, 0.62);
        context.body_bottom_extent = 0.091;
        context.surfaces = vec![SurfaceCandidate {
            surface_id: SurfaceId("screen:bottom_edge".into()),
            minimum: Vec2::new(0.0, 0.995),
            maximum: Vec2::ONE,
            velocity: Vec2::ZERO,
            familiarity: 0.5,
            recent_failed_landings: 0,
        }];

        let target = rank_surface(&context, true, false).expect("top roost");
        assert_eq!(target.normal, Vec2::NEG_Y);
        assert!((target.center_clearance - 0.091).abs() < 1.0e-6);
        let centre = BehaviorTarget::Surface(target).world_position().unwrap();
        assert!((centre.y - 0.904).abs() < 1.0e-6);
    }
}
