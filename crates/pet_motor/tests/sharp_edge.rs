use glam::Vec2;
use lifecore::{ActionId, BehaviorGoalFrame, BodyIntent};
use pet_motor::*;

fn goal() -> BehaviorGoalFrame {
    BehaviorGoalFrame {
        action: ActionId::InvitePetting,
        body_intent: BodyIntent {
            locomotion: lifecore::LocomotionMode::Arrive,
            target_position: Vec2::splat(0.5),
            target_surface: None,
            desired_speed: 0.25,
            facing_direction: 1.0,
            gaze_target: None,
            pose: lifecore::PoseIntent::Curious,
            expression: Default::default(),
            interaction_target: None,
        },
        affect: Default::default(),
        drives: lifecore::Drives::initial(&lifecore::Genome::from_seed(42).temperament),
        felt: Default::default(),
        derived: Default::default(),
        attachment: 0.5,
        recent_outcome: None,
    }
}

fn waiting(touched: bool) -> (ActivePerformance, BehaviorContextFrame) {
    let context = BehaviorContextFrame {
        frame_id: 10,
        timestamp_seconds: 1.0,
        pet_touched: touched,
        ..Default::default()
    };
    let mut runtime = BehaviorPerformanceRuntime::new(42);
    runtime.begin_lab_fixture(
        BehaviorProgramId::SocialPettingSolicitation,
        &goal(),
        &context,
    );
    let mut active = runtime.active().unwrap().clone();
    active.phase.index = 4;
    (active, context)
}

#[test]
fn human_wait_is_not_accelerated_or_completed_at_minimum() {
    let (mut active, mut context) = waiting(false);
    for i in 0..60 {
        context.frame_id += 1;
        context.timestamp_seconds += 0.05;
        assert_eq!(
            advance_phase(&mut active, &context, 0.05),
            PhaseAdvance::Hold,
            "tick {i}"
        );
    }
    assert!((active.phase_time - 3.0).abs() < 0.001);
    for _ in 0..12 {
        let _ = advance_phase(&mut active, &context, 0.05);
    }
    assert_eq!(active.phase.index, 5);
    assert!(!active.social_bid.unwrap().response_received);
}

#[test]
fn old_touch_cannot_accept_but_new_contextual_touch_ends_wait_immediately() {
    let (mut active, mut context) = waiting(true);
    assert_eq!(
        advance_phase(&mut active, &context, 0.05),
        PhaseAdvance::Hold
    );
    context.frame_id += 1;
    context.timestamp_seconds += 0.05;
    assert_eq!(
        advance_phase(&mut active, &context, 0.05),
        PhaseAdvance::Hold
    );
    context.pet_touched = false;
    let _ = advance_phase(&mut active, &context, 0.05);
    context.frame_id += 1;
    context.timestamp_seconds += 0.05;
    context.pet_touched = true;
    assert_eq!(
        advance_phase(&mut active, &context, 0.05),
        PhaseAdvance::Advanced
    );
    assert!(active.social_bid.unwrap().response_received);
}

#[test]
fn focus_ends_wait_without_success_credit() {
    let (mut active, mut context) = waiting(false);
    let _ = advance_phase(&mut active, &context, 0.05);
    context.focus_mode = true;
    assert_eq!(
        advance_phase(&mut active, &context, 0.05),
        PhaseAdvance::Finished(CompletionReason::GracefulWithdrawal)
    );
    assert!(!active.social_bid.unwrap().response_received);
}

#[test]
fn toy_is_never_a_food_affordance() {
    let mut goal = goal();
    goal.action = ActionId::IdleHover;
    goal.drives.comfort = 0.60;
    let mut context = BehaviorContextFrame {
        orb_position: Some(Vec2::splat(0.6)),
        object_affordance: ObjectAffordance::Toy,
        ..Default::default()
    };
    let decision = choose_program(&goal, &context, None, true, &[0.0; PROGRAM_COUNT]);
    assert!(!decision.is_some_and(|d| matches!(
        d.program,
        BehaviorProgramId::HomeFoodInspectSample
            | BehaviorProgramId::HomeFoodAcceptTransport
            | BehaviorProgramId::HomeFoodRefusePushAway
    )));
    context.edible_position = Some(Vec2::splat(0.7));
    assert_eq!(
        choose_program(&goal, &context, None, true, &[0.0; PROGRAM_COUNT])
            .unwrap()
            .program,
        BehaviorProgramId::HomeFoodInspectSample
    );
}

#[test]
fn autonomous_selector_preserves_social_hold_after_three_seconds() {
    let mut runtime = BehaviorPerformanceRuntime::new(42);
    let mut context = BehaviorContextFrame::default();
    let goal = goal();
    runtime.begin_lab_fixture(
        BehaviorProgramId::SocialPettingSolicitation,
        &goal,
        &context,
    );
    for _ in 0..160 {
        context.frame_id += 1;
        context.timestamp_seconds += 0.05;
        let packet = runtime.tick_lab_fixture(&goal, &context, 0.05);
        if packet.phase_name == "look_wait" {
            let mut other = goal.clone();
            other.action = ActionId::ExploreScreen;
            for _ in 0..60 {
                context.frame_id += 1;
                context.timestamp_seconds += 0.05;
                let held = runtime.tick(&other, &context, 0.05);
                assert_eq!(held.phase_name, "look_wait");
            }
            return;
        }
    }
    panic!("social hold never reached");
}

#[test]
fn protection_masks_smile_affiliation_and_voice_after_composition() {
    let mut phenotype = lifecore::FastPhenotypeActuation::default();
    phenotype.expression.mouth_curve = 0.9;
    phenotype.expression.cheek_glow = 0.9;
    phenotype.voice.purr_amount = 0.9;
    phenotype.voice.trill_amount = 0.9;
    phenotype.action.social_approach = 0.8;
    let packet = SomaticActuationPacket {
        program: Some(BehaviorProgramId::DefenseThreatHardenCompact),
        ..Default::default()
    };
    SomaticActuationBus::compose(&mut phenotype, &packet);
    assert!(phenotype.expression.mouth_curve <= 0.0);
    assert_eq!(phenotype.expression.cheek_glow, 0.0);
    assert_eq!(phenotype.voice.purr_amount, 0.0);
    assert_eq!(phenotype.voice.trill_amount, 0.0);
    assert_eq!(phenotype.action.social_approach, 0.0);
}
