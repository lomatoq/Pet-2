use glam::Vec2;
use lifecore::{ActionId, BodyIntent, ExpressionState, LocomotionMode, PoseIntent};
use pet_ecology::{
    EcologyBehaviorFrame, EcologyState, EpisodeDirector, EpisodeGoal, METABOLIC_RESERVE_FLOOR,
    ObjectLifecycle,
};

fn intent(position: Vec2) -> BodyIntent {
    BodyIntent {
        locomotion: LocomotionMode::Hover,
        target_position: position,
        target_surface: None,
        desired_speed: 0.2,
        facing_direction: 1.0,
        gaze_target: None,
        pose: PoseIntent::Neutral,
        expression: ExpressionState::default(),
        interaction_target: None,
    }
}

fn frame(action: ActionId, position: Vec2, timestamp: f64) -> EcologyBehaviorFrame {
    EcologyBehaviorFrame {
        selected_action: action,
        pet_position: position,
        pet_velocity: Vec2::ZERO,
        cursor_position: Vec2::new(0.68, 0.42),
        pointer_down: false,
        user_activity: 0.35,
        focus_mode: false,
        sleeping: false,
        window_pressure: 0.0,
        window_escape_direction: Vec2::ZERO,
        nearest_window_edge: None,
        window_motion: 0.0,
        orb_trapped: false,
        visual_target: None,
        visual_hue: 0.0,
        visual_strength: 0.0,
        shared_attention: false,
        click_rhythm: None,
        timestamp,
    }
}

#[test]
fn accelerated_twenty_four_hour_habitat_stays_finite_and_non_coercive() {
    let mut state = EcologyState::new(24_024);
    let mut director = EpisodeDirector::default();
    let position = Vec2::splat(0.5);
    let mut timestamp = 0.0;
    for tick in 0..345_600 {
        timestamp += 0.25;
        let action = if tick % 9_600 < 80 {
            ActionId::SelfPlay
        } else {
            ActionId::IdleHover
        };
        let output = director.tick(
            &mut state,
            frame(action, position, timestamp),
            intent(position),
            0.25,
        );
        assert!(output.body_intent.target_position.is_finite());
        state.metabolism.advance(0.25);
    }
    state.validate().unwrap();
    assert!(state.metabolism.reserve >= METABOLIC_RESERVE_FLOOR);
    assert!(state.episode_stats.started[EpisodeGoal::SoloOrbPlay.index()] > 0);
}

#[test]
fn fixed_seed_seven_day_schedule_change_is_replay_deterministic() {
    fn replay() -> EcologyState {
        let mut state = EcologyState::new(70_007);
        let mut director = EpisodeDirector::default();
        let position = state.den.anchor;
        for slot in 0..(7 * 24 * 4) {
            let day = slot / (24 * 4);
            let action = if day < 3 {
                ActionId::BringProceduralOrb
            } else {
                ActionId::SelfPlay
            };
            let timestamp = slot as f64 * 900.0;
            let _ = director.tick(
                &mut state,
                frame(action, position, timestamp),
                intent(position),
                0.25,
            );
            state.metabolism.advance(60.0);
        }
        state
    }

    let first = replay();
    let second = replay();
    assert_eq!(first, second);
    assert!(first.episode_stats.started[EpisodeGoal::OfferOrb.index()] > 0);
    assert!(first.episode_stats.started[EpisodeGoal::SoloOrbPlay.index()] > 0);
    first.validate().unwrap();
}

#[test]
fn fourteen_day_absence_recovers_reserve_without_crisis_or_object_loss() {
    let mut state = EcologyState::new(14_014);
    let orb_id = state.objects[0].id;
    state.metabolism.reserve = METABOLIC_RESERVE_FLOOR;
    state.metabolism.satiation = 0.92;
    state.metabolism.apply_offline_seconds(14.0 * 86_400.0);
    assert!(state.metabolism.reserve > METABOLIC_RESERVE_FLOOR);
    assert!(state.metabolism.satiation < 0.92);
    assert_eq!(state.objects[0].id, orb_id);
    state.validate().unwrap();
}

#[test]
fn dragging_pet_during_orb_episode_does_not_teleport_or_corrupt_object() {
    let mut state = EcologyState::new(90_090);
    let mut director = EpisodeDirector::default();
    let start = Vec2::new(0.22, 0.72);
    let _ = director.tick(
        &mut state,
        frame(ActionId::PlayCursorChase, start, 1.0),
        intent(start),
        0.05,
    );
    state.objects[0].lifecycle = ObjectLifecycle::Free;
    state.objects[0].velocity = Vec2::new(0.8, -0.2);
    let dragged = Vec2::new(0.82, 0.18);
    let output = director.tick(
        &mut state,
        frame(ActionId::PlayCursorChase, dragged, 1.05),
        intent(dragged),
        0.05,
    );
    assert!(output.body_intent.target_position.is_finite());
    assert!(output.body_intent.target_position.cmpge(Vec2::ZERO).all());
    assert!(output.body_intent.target_position.cmple(Vec2::ONE).all());
    assert!(state.objects[0].position.is_finite());
    state.validate().unwrap();
}
