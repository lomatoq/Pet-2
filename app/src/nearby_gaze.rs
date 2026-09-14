//! A neutral body-center target is not evidence of attending to the viewer.
//! Resolve only uncommitted gaze; the existing fixation controller owns timing.
use glam::Vec2;
use pet_motor::{BehaviorContextFrame, MotorWorldGoal};

pub fn local_target(context: &BehaviorContextFrame, proposed: Option<Vec2>) -> Option<Vec2> {
    let origin = context.body.motion.world_position;
    if context.world_goal != MotorWorldGoal::None
        || context.pet_dragged
        || context.pet_touched
        || context.companion_intent == lifecore::PrimaryIntent::Sleep
    {
        return None;
    }
    let floor = context
        .surfaces
        .iter()
        .filter(|s| {
            s.minimum.is_finite()
                && s.maximum.is_finite()
                && s.minimum.y >= origin.y
                && s.minimum.y - origin.y < 0.25
                && s.minimum.x <= origin.x
                && s.maximum.x >= origin.x
        })
        .min_by(|a, b| a.minimum.y.total_cmp(&b.minimum.y))
        .map(|s| Vec2::new(origin.x, s.minimum.y));
    if let Some(target) = proposed.filter(|p| p.is_finite() && p.distance(origin) > 0.025) {
        // Preserve the near semantic mode when the previous real target is
        // echoed by another attention writer. Otherwise gain would alternate.
        return [
            context.orb_position,
            context.den_anchor,
            context.edible_position,
            floor,
        ]
        .into_iter()
        .flatten()
        .any(|p| p.is_finite() && p.distance(origin) < 0.40 && p.distance(target) < 0.02)
        .then_some(target);
    }
    if context.support_confirmed()
        && matches!(
            context.companion_intent,
            lifecore::PrimaryIntent::Rest
                | lifecore::PrimaryIntent::SettleAfterStress
                | lifecore::PrimaryIntent::QuietCompanionship
        )
        && floor.is_some()
    {
        return floor;
    }
    [
        context.orb_position,
        context.den_anchor,
        context.edible_position,
    ]
    .into_iter()
    .flatten()
    .filter(|p| p.is_finite() && (0.03..0.40).contains(&p.distance(origin)))
    .min_by(|a, b| {
        a.distance_squared(origin)
            .total_cmp(&b.distance_squared(origin))
    })
    .or(floor)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn neutral_gaze_attends_a_real_nearby_object_but_preserves_intent() {
        let mut c = BehaviorContextFrame::default();
        c.body.motion.world_position = Vec2::splat(0.5);
        c.orb_position = Some(Vec2::new(0.63, 0.55));
        assert_eq!(local_target(&c, Some(Vec2::splat(0.5))), c.orb_position);
        assert_eq!(local_target(&c, c.orb_position), c.orb_position);
        assert_eq!(local_target(&c, Some(Vec2::new(0.2, 0.3))), None);
        c.world_goal = MotorWorldGoal::CarryOrbHome;
        assert_eq!(local_target(&c, None), None);
        c.world_goal = MotorWorldGoal::None;
        c.companion_intent = lifecore::PrimaryIntent::Sleep;
        assert_eq!(local_target(&c, None), None);
    }
    #[test]
    fn quiet_supported_rest_can_stare_at_the_actual_floor_without_an_oscillator() {
        let mut c = BehaviorContextFrame::default();
        c.body.motion.world_position = Vec2::new(0.5, 0.9);
        c.somatic.supported = true;
        c.companion_intent = lifecore::PrimaryIntent::Rest;
        c.surfaces.push(pet_motor::SurfaceCandidate {
            surface_id: lifecore::SurfaceId("floor".into()),
            minimum: Vec2::new(0.0, 1.0),
            maximum: Vec2::ONE,
            velocity: Vec2::ZERO,
            familiarity: 1.0,
            recent_failed_landings: 0,
        });
        c.orb_position = Some(Vec2::new(0.65, 0.9));
        for _ in 0..600 {
            assert_eq!(local_target(&c, None), Some(Vec2::new(0.5, 1.0)));
        }
        c.surfaces.clear();
        assert_eq!(local_target(&c, None), c.orb_position);
    }

    #[test]
    fn no_invented_target_when_scene_is_empty_or_invalid() {
        let mut c = BehaviorContextFrame::default();
        assert_eq!(local_target(&c, None), None);
        c.orb_position = Some(Vec2::splat(f32::NAN));
        assert_eq!(local_target(&c, None), None);
    }
}
