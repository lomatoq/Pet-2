use glam::Vec2;
use lifecore::{ActionId, BodyIntent, ExpressionState, LocomotionMode, PoseIntent};
use pet_ecology::{
    EcologyBehaviorFrame, EcologyState, EpisodeDirector, EpisodeGoal, PhysicalGrabFrame,
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
        play_state: Default::default(),
        social_contact: Default::default(),
        selected_action: action,
        pet_position: position,
        pet_velocity: Vec2::ZERO,
        desktop_aspect: 16.0 / 9.0,
        food_physical: None,
        orb_physical: PhysicalGrabFrame {
            socket_position: Vec2::splat(0.5),
            ..PhysicalGrabFrame::default()
        },
        cursor_position: Vec2::new(0.68, 0.42),
        pointer_down: false,
        user_activity: 0.35,
        user_available: 1.0,
        play_drive: 0.0,
        curiosity_drive: 0.0,
        autonomy_drive: 0.0,
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
        visual_colorfulness: 0.0,
        visual_structure: 0.0,
        visual_surprise: 0.0,
        shared_attention: false,
        autonomous_play_ready: false,
        click_rhythm: None,
        timestamp,
    }
}

#[test]
fn sleeping_pet_keeps_food_for_after_waking() {
    for satiation in [0.0, 0.95] {
        let mut state = EcologyState::new(53);
        state.metabolism.satiation = satiation;
        let profile = pet_ecology::MorselProfile {
            hue: 0.2,
            saturation: 0.8,
            value: 0.9,
            warmth: 0.5,
            pulse_rate: 0.5,
            stimulation: 0.5,
            cohesion_bias: 0.5,
            novelty: 0.8,
        };
        let id = state.spawn_morsel(state.den.anchor, profile, 0.0).unwrap();
        state
            .objects
            .iter_mut()
            .find(|o| o.id == id)
            .unwrap()
            .radius_px_at_reference = 8.0;
        let mut director = EpisodeDirector::default();
        let mut input = frame(ActionId::Sleep, state.den.anchor, 100.0);
        input.sleeping = true;
        let out = director.tick(&mut state, input, intent(input.pet_position), 0.05);
        assert_eq!(out.debug.active_goal, Some(EpisodeGoal::SleepInDen));
        assert!(
            state
                .objects
                .iter()
                .any(|o| o.id == id && o.is_active_morsel())
        );
        director.interrupt_for_shutdown();
        input.sleeping = false;
        input.selected_action = ActionId::IdleHover;
        let out = director.tick(&mut state, input, intent(input.pet_position), 0.05);
        assert_eq!(out.debug.active_goal, Some(EpisodeGoal::InspectMorsel));
    }
}

#[test]
fn recent_success_does_not_starve_a_long_unpracticed_skill() {
    let mut state = EcologyState::new(531);
    let signature = pet_ecology::ActionSignature::from_trace(
        &[Vec2::ZERO, Vec2::new(0.1, -0.1), Vec2::new(0.2, 0.0)],
        1.0,
    )
    .unwrap();
    let skill = |id, competence, last_used_seconds| pet_ecology::LearnedSkill {
        id,
        prototype: signature.clone(),
        competence,
        uncertainty: 0.5,
        motor_error_ema: 0.5,
        social_value: 0.0,
        demonstrations: 1,
        attempts: 2,
        successes: 1,
        last_used_seconds,
    };
    state.skills.skills = vec![skill(1, 0.98, 970.0), skill(2, 0.25, 0.0)];
    state.skills.next_skill_id = 3;
    let mut director = EpisodeDirector::default();
    let input = frame(ActionId::HappyDisplay, state.den.anchor, 1000.0);
    let out = director.tick(&mut state, input, intent(input.pet_position), 0.05);
    assert_eq!(out.debug.active_goal, Some(EpisodeGoal::PerformSkill));
    assert_eq!(director.active_episode().unwrap().object_id, Some(2));
    assert_eq!(
        state.skills.skills[1].attempts, 2,
        "selection is not proof of practice"
    );
    // Measured later use changes eligibility rather than a rotating counter.
    director.interrupt_for_shutdown();
    state.skills.skills[1].last_used_seconds = 995.0;
    let _ = director.tick(&mut state, input, intent(input.pet_position), 0.05);
    assert_eq!(director.active_episode().unwrap().object_id, Some(1));
}

#[test]
fn touch_preference_reverses_without_rewriting_lifetime_history() {
    let mut state = EcologyState::new(532);
    state.successful_touch_sides = [10_000, 0];
    for _ in 0..100 {
        state.observe_touch_preference(-1.0);
    }
    assert!(state.preferred_touch_side() < -0.8);
    for _ in 0..16 {
        state.observe_touch_preference(1.0);
    }
    assert!(state.preferred_touch_side() > 0.4);
    assert_eq!(state.successful_touch_sides, [10_000, 0]);
    let before = state.touch_preference;
    state.metabolism.apply_offline_seconds(30.0 * 86_400.0);
    assert_eq!(
        before, state.touch_preference,
        "absence is not negative evidence"
    );
    let restored: EcologyState =
        serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
    assert_eq!(
        restored.preferred_touch_side(),
        state.preferred_touch_side()
    );
    restored.validate().unwrap();
    let mut old = serde_json::to_value(state).unwrap();
    old.as_object_mut().unwrap().remove("touch_preference");
    let old: EcologyState = serde_json::from_value(old).unwrap();
    assert_eq!(
        old.preferred_touch_side(),
        0.0,
        "old counts do not fabricate recent evidence"
    );
    old.validate().unwrap();
}

#[test]
fn failed_practice_records_failure_and_yields_its_turn() {
    let mut state = EcologyState::new(534);
    let signature =
        pet_ecology::ActionSignature::from_trace(&[Vec2::ZERO, Vec2::new(0.2, 0.0)], 1.0).unwrap();
    state.skills.skills.push(pet_ecology::LearnedSkill {
        id: 1,
        prototype: signature,
        competence: 0.6,
        uncertainty: 0.6,
        motor_error_ema: 0.5,
        social_value: 0.0,
        demonstrations: 1,
        attempts: 0,
        successes: 0,
        last_used_seconds: 0.0,
    });
    state.skills.next_skill_id = 2;
    let mut director = EpisodeDirector::default();
    // Actual body remains at the start instead of teleporting along the command.
    for tick in 0..80 {
        let input = frame(
            ActionId::HappyDisplay,
            Vec2::splat(0.5),
            100.0 + tick as f64 * 0.05,
        );
        let _ = director.tick(&mut state, input, intent(input.pet_position), 0.05);
    }
    let skill = &state.skills.skills[0];
    assert_eq!(skill.attempts, 1);
    assert_eq!(skill.successes, 0);
    assert!(skill.last_used_seconds >= 103.0);
    assert!(skill.competence < 0.6);
    assert!(director.active_episode().is_none());
}

#[test]
fn orb_tactic_depends_on_context_and_stays_stable_during_a_bout() {
    let mut energetic = frame(ActionId::SelfPlay, Vec2::new(0.3, 0.5), 1.0);
    energetic.play_drive = 1.0;
    let mut tired = energetic;
    tired.play_drive = 0.1;
    tired.social_contact.fatigue = 0.9;
    let energetic_variant = pet_ecology::OrbExperience::default().choose(
        energetic,
        Vec2::splat(0.5),
        Vec2::new(0.3, 0.0),
        53,
    );
    let tired_variant =
        pet_ecology::OrbExperience::default().choose(tired, Vec2::splat(0.5), Vec2::ZERO, 53);
    assert_ne!(energetic_variant, tired_variant);
    let mut state = EcologyState::new(53);
    state.objects[0].lifecycle = pet_ecology::ObjectLifecycle::Free;
    state.objects[0].position = Vec2::new(0.6, 0.5);
    let mut director = EpisodeDirector::default();
    let _ = director.tick(&mut state, energetic, intent(energetic.pet_position), 0.05);
    let selected = director.active_episode().unwrap().play_variant;
    for tick in 0..20 {
        energetic.timestamp += 0.05;
        energetic.play_drive = if tick % 2 == 0 { 0.4 } else { 0.9 };
        let _ = director.tick(&mut state, energetic, intent(energetic.pet_position), 0.05);
        assert_eq!(director.active_episode().unwrap().play_variant, selected);
    }
}

#[test]
fn passive_user_held_contact_does_not_train_a_motor_tactic() {
    let mut state = EcologyState::new(535);
    state.objects[0].lifecycle = pet_ecology::ObjectLifecycle::GrabbedByUser;
    let mut director = EpisodeDirector::default();
    let mut input = frame(ActionId::PlayCursorChase, Vec2::splat(0.5), 1.0);
    input.play_drive = 0.9;
    input.orb_physical.contact = true;
    let output = director.tick(&mut state, input, intent(input.pet_position), 0.05);
    assert_eq!(output.object_command_count, 0, "user owns the ball");
    assert!(
        state.orb_experience.evidence.iter().all(|v| *v == 0.0),
        "no motor outcome occurred"
    );
}

#[test]
fn actual_free_flight_corrects_signed_prediction_without_learning_pickup_teleport() {
    let mut state = EcologyState::new(536);
    let mut director = EpisodeDirector::default();
    state.objects[0].lifecycle = pet_ecology::ObjectLifecycle::Free;
    state.objects[0].velocity = Vec2::new(0.2, 0.0);
    let mut input = frame(ActionId::IdleHover, state.den.anchor, 1.0);
    input.focus_mode = true;
    input.desktop_aspect = 2.0;
    // Actual height-space speed .1, reported model speed .2: overshoot must
    // acquire a negative x correction rather than simply lower confidence.
    for _ in 0..20 {
        state.objects[0].position.x += 0.0025;
        input.timestamp += 0.05;
        let _ = director.tick(&mut state, input, intent(input.pet_position), 0.05);
    }
    assert!(state.orb_experience.prediction_bias.x < 0.0);
    assert!(state.object_memories[0].prediction_error_ema > 0.0);
    let before = state.orb_experience.prediction_bias;
    state.objects[0].lifecycle = pet_ecology::ObjectLifecycle::GrabbedByUser;
    state.objects[0].position = Vec2::ZERO;
    let _ = director.tick(&mut state, input, intent(input.pet_position), 0.05);
    assert_eq!(state.orb_experience.prediction_bias, before);
    state.validate().unwrap();
}

// Thirty calendar-day sessions, each with exactly ten active minutes. Input
// events represent completed pleasant contacts and measured tactic outcomes;
// this is persistence/learning replay, not a month of native liquid simulation.
#[test]
fn thirty_daily_sessions_keep_new_evidence_bounded_reversible_and_offline_quiet() {
    let mut state = EcologyState::new(537);
    let identity = state.identity_seed;
    let mut active_ticks = 0_u64;
    let mut offline_seconds = 0.0_f64;
    for day in 0..30 {
        for tick in 0..12_000 {
            state.metabolism.advance(0.05);
            active_ticks += 1;
            if tick % 1_200 == 0 {
                state.observe_touch_preference(if day < 15 { -1.0 } else { 1.0 });
                state.orb_experience.observe_contact(5, day < 15);
            }
        }
        if day == 14 {
            assert!(state.preferred_touch_side() < -0.8);
            assert!(state.orb_experience.value[5] > 0.9);
        }
        let touch = state.touch_preference;
        let orb = state.orb_experience.clone();
        state.metabolism.apply_offline_seconds(85_800.0);
        offline_seconds += 85_800.0;
        assert_eq!(state.touch_preference, touch);
        assert_eq!(
            state.orb_experience, orb,
            "offline time creates no motor outcomes"
        );
        state = EcologyState::restore(
            serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap(),
        )
        .unwrap();
        assert_eq!(state.identity_seed, identity);
        assert_eq!(state.touch_preference, touch);
        assert_eq!(state.orb_experience, orb);
        assert!(state.orb_experience.valid());
        state.validate().unwrap();
    }
    assert_eq!(active_ticks, 360_000);
    assert_eq!(
        active_ticks as f64 * 0.05 + offline_seconds,
        30.0 * 86_400.0
    );
    assert!(state.preferred_touch_side() > 0.8);
    assert!(state.orb_experience.value[5] < -0.3);
    assert_eq!(state.orb_experience.evidence[5], 32.0);
    let mut legacy = serde_json::to_value(state).unwrap();
    legacy.as_object_mut().unwrap().remove("orb_experience");
    legacy.as_object_mut().unwrap().remove("touch_preference");
    let legacy: EcologyState = serde_json::from_value(legacy).unwrap();
    assert_eq!(legacy.touch_preference, [0.0; 2]);
    assert_eq!(legacy.orb_experience, pet_ecology::OrbExperience::default());
    legacy.validate().unwrap();
}
