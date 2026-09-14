use glam::Vec2;
use lifecore::{
    ActionId, BehaviorGoalFrame, Genome, LifeCore, PrimaryIntent, SensorFrame, SurfaceId,
};
use pet_motor::*;

fn fixture(action: ActionId) -> (BehaviorGoalFrame, BehaviorContextFrame) {
    let mut life = LifeCore::new(Genome::from_seed(42), 42);
    let body_intent = life
        .tick(&SensorFrame::default(), &Default::default(), 0.05)
        .body_intent;
    let goal = BehaviorGoalFrame {
        action,
        body_intent,
        affect: Default::default(),
        drives: life.state.drives,
        felt: Default::default(),
        derived: Default::default(),
        attachment: 0.4,
        recent_outcome: None,
    };
    let mut context = BehaviorContextFrame::default();
    context.body.motion.world_position = Vec2::new(0.9, 0.9);
    context.den_anchor = Some(Vec2::new(0.94, 0.9));
    context.body_bottom_extent = 0.05;
    context.screen_edge_gap_px = 34.0;
    context.surfaces.push(SurfaceCandidate {
        surface_id: SurfaceId("screen:bottom_edge".into()),
        minimum: Vec2::new(0.0, 0.999),
        maximum: Vec2::ONE,
        velocity: Vec2::ZERO,
        familiarity: 1.0,
        recent_failed_landings: 0,
    });
    (goal, context)
}

#[test]
fn social_intent_cannot_starve_committed_sleep() {
    let (goal, mut context) = fixture(ActionId::Sleep);
    context.companion_intent = PrimaryIntent::QuietCompanionship;
    context.companion_confidence = 1.0;
    context.world_goal = MotorWorldGoal::ReturnHome;
    let chosen = choose_program(&goal, &context, None, true, &[0.0; PROGRAM_COUNT]).unwrap();
    assert_eq!(chosen.program, BehaviorProgramId::RestSurfaceRoostSearch);
}

#[test]
fn carrying_an_orb_is_not_interrupted_by_ambient_inspection_or_home_rest() {
    let (goal, mut context) = fixture(ActionId::ObserveUserActivity);
    context.world_goal = MotorWorldGoal::CarryOrbHome;
    context.companion_intent = PrimaryIntent::Inspect;
    context.companion_confidence = 1.0;
    let chosen = choose_program(&goal, &context, None, true, &[0.0; PROGRAM_COUNT]).unwrap();
    assert_eq!(chosen.program, BehaviorProgramId::HomeDenReturnEscort);
}

#[test]
fn quiet_home_rest_lands_and_keeps_the_same_supported_bout() {
    let (goal, mut context) = fixture(ActionId::ObserveUserActivity);
    context.focus_mode = true;
    context.companion_intent = PrimaryIntent::Inspect;
    context.companion_confidence = 1.0;
    context.world_goal = MotorWorldGoal::ReturnHome;
    let mut runtime = BehaviorPerformanceRuntime::new(42);
    assert_eq!(
        runtime.tick(&goal, &context, 0.05).program,
        Some(BehaviorProgramId::RestSurfaceRoostSearch)
    );
    context.world_goal = MotorWorldGoal::None;
    context.screen_edge_gap_px = 1.0;
    context.screen_edge_supported = true;
    context.screen_edge_support_stable_seconds = 1.0;
    let mut bout = None;
    for tick in 1..=400 {
        context.frame_id = tick;
        context.timestamp_seconds += 0.05;
        let packet = runtime.tick(&goal, &context, 0.05);
        if tick > 80 {
            assert_eq!(packet.program, Some(BehaviorProgramId::RestSitSettle));
            assert_eq!(packet.locomotion.pose, MotorPoseIntent::SupportedRest);
            assert_eq!(packet.phase_name, "rest_hold");
            let flatten = packet
                .fields
                .iter()
                .flatten()
                .find(|field| field.kind == SomaticFieldKind::Flatten)
                .expect("supported hold must retain its loaded contact patch");
            assert!(flatten.strength > 0.0);
            assert_eq!(flatten.frequency_hz, 0.0);
            assert_eq!(
                *bout.get_or_insert(packet.source_bout_id),
                packet.source_bout_id
            );
        }
    }
    let mut danger = goal.clone();
    danger.felt.pain_like = 0.8;
    assert_eq!(
        runtime.tick(&danger, &context, 0.05).program,
        Some(BehaviorProgramId::DefenseLocalPainGuard)
    );
}

#[test]
fn rest_lease_survives_ambient_fluctuation_but_not_real_tasks() {
    let (mut goal, mut context) = fixture(ActionId::IdleHover);
    let mut runtime = BehaviorPerformanceRuntime::new(42);
    assert_eq!(
        runtime.tick(&goal, &context, 0.05).program,
        Some(BehaviorProgramId::RestSurfaceRoostSearch)
    );
    goal.affect.arousal = 0.75;
    goal.affect.stress = 0.40;
    context.companion_intent = PrimaryIntent::Inspect;
    context.companion_confidence = 1.0;
    for tick in 0..40 {
        goal.action = if tick % 2 == 0 {
            ActionId::ExploreScreen
        } else {
            ActionId::SelfPlay
        };
        let packet = runtime.tick(&goal, &context, 0.05);
        assert_eq!(
            packet.program,
            Some(BehaviorProgramId::RestSurfaceRoostSearch)
        );
    }
    context.screen_edge_gap_px = 0.0;
    context.screen_edge_supported = true;
    context.screen_edge_support_stable_seconds = 1.0;
    let mut hold_bout = None;
    for tick in 0..140 {
        let packet = runtime.tick(&goal, &context, 0.05);
        if tick > 65 {
            assert_eq!(packet.program, Some(BehaviorProgramId::RestSitSettle));
            assert_eq!(packet.phase_name, "rest_hold");
            assert_eq!(
                *hold_bout.get_or_insert(packet.source_bout_id),
                packet.source_bout_id
            );
        }
    }
    // Actual loss of support re-enters acquisition even during the lease.
    context.screen_edge_supported = false;
    context.screen_edge_support_stable_seconds = 0.0;
    context.screen_edge_gap_px = 30.0;
    assert_eq!(
        runtime.tick(&goal, &context, 0.05).program,
        Some(BehaviorProgramId::RestSurfaceRoostSearch)
    );
    context.world_goal = MotorWorldGoal::CarryOrbHome;
    // A newly entered phase may retain minimum readability, but cannot retain
    // the rest lease instead of accepting the purposeful object task.
    for _ in 0..80 {
        let _ = runtime.tick(&goal, &context, 0.05);
    }
    assert_ne!(
        runtime.last_packet().program,
        Some(BehaviorProgramId::RestSurfaceRoostSearch)
    );
}

#[test]
fn distant_rest_approach_retains_bout_through_ambient_proposals() {
    let (mut goal, mut context) = fixture(ActionId::IdleHover);
    context.screen_edge_gap_px = 390.0;
    let mut runtime = BehaviorPerformanceRuntime::new(42);
    let first = runtime.tick(&goal, &context, 0.05);
    assert_eq!(
        first.program,
        Some(BehaviorProgramId::RestSurfaceRoostSearch)
    );
    for tick in 0..100 {
        goal.action = if tick % 2 == 0 {
            ActionId::SelfPlay
        } else {
            ActionId::ExploreScreen
        };
        goal.affect.arousal = 0.75;
        goal.affect.stress = 0.4;
        context.companion_intent = PrimaryIntent::Inspect;
        context.companion_confidence = 1.0;
        context.screen_edge_gap_px = 390.0 - tick as f32 * 3.0;
        let packet = runtime.tick(&goal, &context, 0.05);
        assert_eq!(
            packet.source_bout_id, first.source_bout_id,
            "approach restarted at {tick}"
        );
        if tick > 5 {
            assert_eq!(packet.phase_name, "approach_commit");
            assert!(packet.locomotion.speed_multiplier > 0.1);
        }
    }
    context.screen_edge_gap_px = 7.0;
    assert_eq!(
        runtime.tick(&goal, &context, 0.05).program,
        Some(BehaviorProgramId::RestLandingSoftTouchdown)
    );
    context.screen_edge_gap_px = 0.0;
    context.screen_edge_supported = true;
    context.screen_edge_support_stable_seconds = 1.0;
    for _ in 0..40 {
        let _ = runtime.tick(&goal, &context, 0.05);
    }
    assert_eq!(
        runtime.last_packet().program,
        Some(BehaviorProgramId::RestSitSettle)
    );
    assert_eq!(runtime.last_packet().phase_name, "rest_hold");
}

#[test]
fn ambient_rest_lease_is_bounded_and_danger_still_interrupts() {
    let (mut goal, mut context) = fixture(ActionId::IdleHover);
    let mut runtime = BehaviorPerformanceRuntime::new(11);
    let _ = runtime.tick(&goal, &context, 0.05);
    goal.action = ActionId::ExploreScreen;
    goal.affect.arousal = 0.8;
    goal.felt.pain_like = 0.8;
    assert_eq!(
        runtime.tick(&goal, &context, 0.05).program,
        Some(BehaviorProgramId::DefenseLocalPainGuard)
    );
    let mut runtime = BehaviorPerformanceRuntime::new(12);
    goal.felt.pain_like = 0.0;
    goal.action = ActionId::IdleHover;
    goal.affect.arousal = 0.2;
    let _ = runtime.tick(&goal, &context, 0.05);
    goal.action = ActionId::ExploreScreen;
    goal.affect.arousal = 0.8;
    context.den_anchor = None;
    for _ in 0..180 {
        let _ = runtime.tick(&goal, &context, 0.05);
    }
    assert_ne!(
        runtime.last_packet().program,
        Some(BehaviorProgramId::RestSurfaceRoostSearch)
    );
}
