use glam::Vec2;
use lifecore::*;

#[test]
fn focus_is_availability_without_reward_or_relationship_damage() {
    let mut life = LifeCore::new(Genome::from_seed(42), 42);
    let affect = life.state.affect;
    let reward = life.state.recent_reward;
    let ignored = life.state.ignored_attempts;
    let success = life.state.successful_interactions;
    for _ in 0..100 {
        life.apply_feedback(FeedbackEvent::FocusModeEnabled);
        assert!(life.state.focus_mode);
        life.apply_feedback(FeedbackEvent::FocusModeDisabled);
    }
    assert_eq!(life.state.affect, affect);
    assert_eq!(life.state.recent_reward, reward);
    assert_eq!(life.state.ignored_attempts, ignored);
    assert_eq!(life.state.successful_interactions, success);
}

#[test]
fn salience_does_not_turn_an_old_contact_into_a_gaze_target() {
    let genome = Genome::from_seed(42);
    let mut source = EmbodimentSourceFrame {
        frame_id: 1,
        affect: AffectState::default(),
        drives: Drives::initial(&genome.temperament),
        temperament: genome.temperament,
        voice_seed: genome.voice.voice_seed,
        vita: Default::default(),
        morph: Default::default(),
        body: Default::default(),
        voice_feedback: Default::default(),
        gesture: Default::default(),
        episode: Default::default(),
        perception: Default::default(),
        soft_touch_pressure_max: 0.42,
    };
    source.body.contact.point_world = Vec2::new(0.1, 0.1);
    source.perception.selected_salience = 1.0;
    let mut director = BodyPhenotypeDirector::default();
    assert_eq!(
        director
            .tick(&source, Default::default(), 0.05)
            .face
            .gaze_target,
        None
    );
    source.perception.attention_target_position = Some(Vec2::new(0.8, 0.6));
    source.perception.attention_target_kind = AttentionTargetKind::ObjectGoal;
    source.perception.attention_confidence = 0.9;
    assert_eq!(
        director
            .tick(&source, Default::default(), 0.05)
            .face
            .gaze_target,
        Some(Vec2::new(0.8, 0.6))
    );
}
