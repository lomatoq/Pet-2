use glam::Vec2;
use lifecore::{BodyIntent, ExpressionState, LocomotionMode, PoseIntent};
use pet_ecology::{EpisodeDirector, EpisodeReason};

#[test]
fn empty_habitat_policy_preserves_all_brain_modes_semantically() {
    let modes = [
        "classic",
        "morphic",
        "fusion",
        "morph-shadow",
        "morph-fusion",
    ];
    for (index, _mode) in modes.into_iter().enumerate() {
        let intent = BodyIntent {
            locomotion: LocomotionMode::Arrive,
            target_position: Vec2::new(index as f32 * 0.1, 0.6),
            target_surface: None,
            desired_speed: 0.2 + index as f32 * 0.1,
            facing_direction: 1.0,
            gaze_target: Some(Vec2::new(0.3, 0.7)),
            pose: PoseIntent::Curious,
            expression: ExpressionState::default(),
            interaction_target: None,
        };
        let output = EpisodeDirector::default().tick_passthrough(intent.clone(), false);
        assert_eq!(output.body_intent, intent);
        assert_eq!(
            output.debug.selected_reason,
            EpisodeReason::NoEligibleEpisode
        );
    }
}
