use glam::Vec2;
use lifecore::{ActionId, BodyIntent};
use pet_ecology::*;
fn frame(p: Vec2, contact: bool) -> EcologyBehaviorFrame {
    EcologyBehaviorFrame {
        play_state: Default::default(),
        social_contact: Default::default(),
        selected_action: ActionId::SelfPlay,
        pet_position: p,
        pet_velocity: Vec2::ZERO,
        seated_in_den: false,
        desktop_aspect: 1.0,
        orb_physical: PhysicalGrabFrame {
            contact,
            socket_position: p,
            ..Default::default()
        },
        food_physical: None,
        cursor_position: Vec2::splat(0.5),
        pointer_down: false,
        user_activity: 0.0,
        user_available: 0.8,
        play_drive: 0.7,
        curiosity_drive: 0.7,
        autonomy_drive: 0.4,
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
        autonomous_play_ready: true,
        click_rhythm: None,
        timestamp: 0.0,
    }
}
#[test]
fn old_ecology_snapshot_migrates_without_invented_experience() {
    let s = EcologyState::new(42);
    let mut json = serde_json::to_value(&s).unwrap();
    json.as_object_mut().unwrap().remove("grounded");
    let restored = EcologyState::restore(serde_json::from_value(json).unwrap()).unwrap();
    assert_eq!(restored.identity_seed, 42);
    assert!(restored.grounded.episodes.is_empty());
    assert_eq!(restored.objects[0], s.objects[0]);
}
#[test]
fn hidden_object_does_not_leak_current_coordinates_into_belief() {
    let mut s = EcologyState::new(42);
    let o = &mut s.objects[0];
    o.position = Vec2::new(0.2, 0.3);
    let mut m = GroundedMemory::default();
    m.observe(&s.objects, Vec2::splat(0.5), 1.0, 0.05, false);
    s.objects[0].lifecycle = ObjectLifecycle::StoredInDen;
    s.objects[0].position = Vec2::new(0.8, 0.9);
    m.observe(&s.objects, Vec2::splat(0.5), 1.0, 0.05, false);
    assert_eq!(m.beliefs[0].last_position, Vec2::new(0.2, 0.3));
    assert!(!m.beliefs[0].visible);
    assert!(m.beliefs[0].container);
    assert!(m.beliefs[0].uncertainty > 0.0);
}
#[test]
fn observation_alone_and_blocked_action_never_train_effect_weights() {
    let s = EcologyState::new(42);
    let mut m = GroundedMemory::default();
    for _ in 0..100 {
        m.observe(&s.objects, Vec2::splat(0.5), 1.0, 0.05, false);
    }
    let before = m.beliefs[0].impulse_columns;
    let c = ObjectCommand::ApplyImpulse {
        object_id: s.objects[0].id,
        impulse: Vec2::X * 0.1,
    };
    m.receipt(c, Some(&s.objects[0]), Some(&s.objects[0]), 1, 1.0, false);
    assert_eq!(before, m.beliefs[0].impulse_columns);
    assert_eq!(m.stats.learning_updates, 0);
    assert_eq!(m.stats.blocked, 1);
    assert_eq!(m.episodes[0].outcome, CausalOutcome::Blocked);
}
#[test]
fn confirmed_interventions_learn_object_specific_jacobian_and_change_next_command() {
    let mut s = EcologyState::new(42);
    let mut m = GroundedMemory::default();
    s.objects[0].mass = 0.42;
    s.objects[0].velocity = Vec2::ZERO;
    m.observe(&s.objects, Vec2::splat(0.5), 1.0, 0.05, false);
    let probe = Vec2::new(0.06, 0.08);
    let target = probe / 0.42;
    let first_error = m.beliefs[0].effect(probe).distance(target);
    let first_command = m.beliefs[0].impulse_for_velocity(Vec2::X * 0.25, Vec2::ZERO);
    for i in 0..50 {
        let mut before = s.objects[0].clone();
        before.velocity = Vec2::ZERO;
        let impulse = if i % 2 == 0 {
            Vec2::X * 0.12
        } else {
            Vec2::Y * 0.10
        };
        let mut after = before.clone();
        after.velocity += impulse / before.mass;
        m.receipt(
            ObjectCommand::ApplyImpulse {
                object_id: before.id,
                impulse,
            },
            Some(&before),
            Some(&after),
            i + 1,
            1.0,
            true,
        );
    }
    assert!(m.beliefs[0].effect(probe).distance(target) < first_error * 0.1);
    assert!(
        m.beliefs[0]
            .impulse_for_velocity(Vec2::X * 0.25, Vec2::ZERO)
            .distance(Vec2::X * 0.105)
            < first_command.distance(Vec2::X * 0.105)
    );
    assert_eq!(m.stats.learning_updates, 50);
    assert!(m.valid());
}
#[test]
fn user_takeover_confounds_pending_effect_instead_of_becoming_pet_success() {
    let mut s = EcologyState::new(42);
    let mut m = GroundedMemory::default();
    m.observe(&s.objects, Vec2::splat(0.5), 1.0, 0.05, false);
    let before = s.objects[0].clone();
    s.objects[0].velocity += Vec2::X * 0.1;
    let event = m
        .receipt(
            ObjectCommand::ApplyImpulse {
                object_id: before.id,
                impulse: Vec2::X * 0.072,
            },
            Some(&before),
            Some(&s.objects[0]),
            1,
            1.0,
            true,
        )
        .unwrap();
    s.objects[0].lifecycle = ObjectLifecycle::GrabbedByUser;
    m.observe(&s.objects, Vec2::splat(0.5), 1.0, 0.05, false);
    assert_eq!(
        m.episodes.iter().find(|e| e.id == event).unwrap().outcome,
        CausalOutcome::Confounded
    );
    assert_eq!(m.stats.observed_outcomes, 0);
}
#[test]
fn later_actual_observation_records_prediction_error_not_simulated_reward() {
    let mut s = EcologyState::new(42);
    let mut m = GroundedMemory::default();
    m.observe(&s.objects, Vec2::splat(0.5), 1.0, 0.05, false);
    let before = s.objects[0].clone();
    s.objects[0].velocity = Vec2::X * 0.14;
    m.receipt(
        ObjectCommand::ApplyImpulse {
            object_id: before.id,
            impulse: Vec2::X * 0.1,
        },
        Some(&before),
        Some(&s.objects[0]),
        1,
        1.0,
        true,
    );
    for _ in 0..4 {
        let v = s.objects[0].velocity;
        s.objects[0].position += v * 0.05;
        m.observe(&s.objects, Vec2::splat(0.5), 1.0, 0.05, false);
    }
    assert_eq!(m.stats.observed_outcomes, 1);
    assert!(m.episodes[0].prediction_error.unwrap() < 0.02);
    assert_eq!(m.stats.plan_successes, 0); // observed motion is not task mastery
}
#[test]
fn restart_invalidates_in_flight_receipt_but_keeps_learned_weights() {
    let s = EcologyState::new(42);
    let mut m = GroundedMemory::default();
    m.observe(&s.objects, Vec2::splat(0.5), 1.0, 0.05, false);
    let mut after = s.objects[0].clone();
    after.velocity = Vec2::X * 0.3;
    m.receipt(
        ObjectCommand::ApplyImpulse {
            object_id: after.id,
            impulse: Vec2::X * 0.1,
        },
        Some(&s.objects[0]),
        Some(&after),
        1,
        1.0,
        true,
    );
    let mut restored: GroundedMemory =
        serde_json::from_str(&serde_json::to_string(&m).unwrap()).unwrap();
    restored.after_native_restart();
    assert_eq!(
        restored.beliefs[0].impulse_columns,
        m.beliefs[0].impulse_columns
    );
    assert_eq!(restored.episodes[0].outcome, CausalOutcome::Unobserved);
    assert!(!restored.beliefs[0].visible);
    assert!(restored.valid());
}
#[test]
fn compiler_generates_bounded_parameterized_multi_step_tasks_for_all_families() {
    for family in GameFamily::ALL {
        for level in 0..5 {
            let p = GroundedMemory::compile_exercise(
                1,
                2,
                family,
                level,
                Vec2::new(0.35, 0.7),
                Vec2::new(0.9, 0.8),
                2.4,
                0.0,
            );
            assert!(p.valid());
            assert!(p.steps.len() >= 3);
            assert!(p.steps.len() <= 24);
        }
    }
}
#[test]
fn malformed_or_oversized_program_cannot_enter_scheduler() {
    let mut m = GroundedMemory::default();
    let mut p = GroundedMemory::compile_exercise(
        1,
        2,
        GameFamily::Fetch,
        0,
        Vec2::splat(0.5),
        Vec2::splat(0.8),
        1.0,
        0.0,
    );
    p.steps[0].target.x = f32::NAN;
    assert!(m.request_exercise(p).is_err());
    let mut p = GroundedMemory::compile_exercise(
        1,
        2,
        GameFamily::Fetch,
        0,
        Vec2::splat(0.5),
        Vec2::splat(0.8),
        1.0,
        0.0,
    );
    while p.steps.len() < 25 {
        p.steps.push(p.steps[0].clone());
    }
    assert!(m.request_exercise(p).is_err());
}
#[test]
fn user_refusal_clears_continuations_without_changing_bond_or_learning_weights() {
    let mut m = GroundedMemory::default();
    m.exercise = Some(GroundedMemory::compile_exercise(
        1,
        2,
        GameFamily::Roll,
        0,
        Vec2::splat(0.5),
        Vec2::splat(0.8),
        1.0,
        0.0,
    ));
    m.suspend_exercise(false);
    assert_eq!(m.suspended.len(), 1);
    m.suspend_exercise(true);
    assert!(m.suspended.is_empty());
    assert!(m.cooldown >= 45.0);
    assert_eq!(m.stats.plan_failures, 0);
}
#[test]
fn sleep_or_focus_cannot_emit_an_exercise_command() {
    let s = EcologyState::new(42);
    let mut m = GroundedMemory::default();
    m.observe(&s.objects, Vec2::splat(0.5), 1.0, 0.05, false);
    for sleeping in [true, false] {
        let mut f = frame(Vec2::splat(0.5), true);
        f.sleeping = sleeping;
        f.focus_mode = !sleeping;
        assert!(
            m.run_exercise(f, 1, &s.objects[0], s.den.anchor, intent(), 0.05)
                .is_none()
        );
    }
    assert_eq!(m.stats.executed, 0);
}
#[test]
fn native_object_physics_supports_a_complete_roll_exercise_and_real_receipts() {
    let mut s = EcologyState::new(42);
    let id = s.objects[0].id;
    s.objects[0].position = Vec2::new(0.35, 0.973);
    s.objects[0].velocity = Vec2::ZERO;
    let mut m = GroundedMemory::default();
    let mut pet = Vec2::new(0.30, 0.93);
    m.exercise = Some(GroundedMemory::compile_exercise(
        10,
        id,
        GameFamily::Roll,
        0,
        s.objects[0].position,
        s.den.anchor,
        1.0,
        0.0,
    ));
    let mut outcome = None;
    for _ in 0..1000 {
        for _ in 0..6 {
            step_object(
                &mut s.objects[0],
                ObjectPhysicsConfig {
                    desktop_aspect: 1.0,
                    ..Default::default()
                },
                1.0 / 120.0,
            );
        }
        m.observe(&s.objects, pet, 1.0, 0.05, false);
        let contact = pet.distance(s.objects[0].position) < 0.065;
        let f = frame(pet, contact);
        let Some(o) = m.run_exercise(f, 10, &s.objects[0], s.den.anchor, intent(), 0.05) else {
            continue;
        };
        pet += (o.intent.target_position - pet).clamp_length_max(o.intent.desired_speed * 0.05);
        if let Some(c) = o.command {
            let before = s.objects[0].clone();
            if let ObjectCommand::ApplyImpulse { impulse, .. } = c {
                assert!(contact);
                s.objects[0].lifecycle = ObjectLifecycle::Free;
                let mass = s.objects[0].mass;
                s.objects[0].velocity += impulse / mass;
            }
            m.receipt(c, Some(&before), Some(&s.objects[0]), 10, 1.0, true);
        }
        if o.status != ExerciseStatus::Running {
            outcome = Some(o.status);
            break;
        }
    }
    assert_eq!(
        outcome,
        Some(ExerciseStatus::Succeeded),
        "{:?} {:?}",
        m.exercise,
        m.episodes
    );
    assert_eq!(m.stats.plan_successes, 1);
    assert!(m.stats.learning_updates > 0);
    assert!(m.valid());
}

fn intent() -> BodyIntent {
    lifecore::LifeCore::new(lifecore::Genome::from_seed(42), 42)
        .tick(
            &lifecore::SensorFrame::default(),
            &lifecore::BodyFeedback::default(),
            0.05,
        )
        .body_intent
}

#[test]
fn blocked_request_preserves_pending_real_effect_and_applied_replacement_is_counted() {
    let mut s = EcologyState::new(42);
    let mut m = GroundedMemory::default();
    m.observe(&s.objects, Vec2::splat(0.5), 1.0, 0.05, false);
    let before = s.objects[0].clone();
    s.objects[0].velocity = Vec2::X * 0.14;
    let command = ObjectCommand::ApplyImpulse { object_id: before.id, impulse: Vec2::X * 0.1 };
    let first = m.receipt(command, Some(&before), Some(&s.objects[0]), 1, 1.0, true).unwrap();
    m.receipt(command, Some(&s.objects[0]), Some(&s.objects[0]), 1, 1.0, false);
    assert_eq!(m.episodes.iter().find(|e| e.id==first).unwrap().outcome, CausalOutcome::AwaitingObservation);
    let before_second = s.objects[0].clone();
    s.objects[0].velocity += Vec2::Y * 0.1;
    m.receipt(ObjectCommand::ApplyImpulse { object_id: before.id, impulse: Vec2::Y * 0.07 }, Some(&before_second), Some(&s.objects[0]), 1, 1.0, true);
    assert_eq!(m.episodes.iter().find(|e| e.id==first).unwrap().outcome, CausalOutcome::Confounded);
    assert_eq!(m.stats.confounded, 1);
    assert!(m.valid());
}

#[test]
fn receipt_from_another_object_cannot_train_this_objects_effect_model() {
    let s = EcologyState::new(42);
    let mut m = GroundedMemory::default();
    m.observe(&s.objects, Vec2::splat(0.5), 1.0, 0.05, false);
    let mut wrong = s.objects[0].clone(); wrong.id += 10; wrong.velocity = Vec2::X * 0.3;
    m.receipt(ObjectCommand::ApplyImpulse { object_id: s.objects[0].id, impulse: Vec2::X * 0.1 }, Some(&s.objects[0]), Some(&wrong), 1, 1.0, true);
    assert_eq!(m.stats.learning_updates, 0);
    assert_eq!(m.stats.executed, 0);
    assert_eq!(m.stats.blocked, 1);
}

#[test]
fn still_held_object_cannot_satisfy_a_released_settling_goal() {
    let mut s = EcologyState::new(42);
    s.objects[0].lifecycle = ObjectLifecycle::CarriedByPet;
    s.objects[0].velocity = Vec2::ZERO;
    let origin = s.objects[0].position;
    let mut m = GroundedMemory::default();
    let mut x = GroundedMemory::compile_exercise(1, s.objects[0].id, GameFamily::Fetch, 0, origin, origin, 1.0, 0.0);
    x.index = x.steps.len()-1; x.executed_steps = 3; x.confirmed_interventions = 2;
    m.exercise = Some(x);
    for _ in 0..16 {
        m.observe(&s.objects, origin, 1.0, 0.05, false);
        let result = m.run_exercise(frame(origin, true), 1, &s.objects[0], origin, intent(), 0.05).unwrap();
        assert_eq!(result.status, ExerciseStatus::Running);
    }
    assert_eq!(m.stats.plan_successes, 0);
}

#[test]
fn stored_toy_is_not_evidence_of_an_unfinished_arbitrary_fetch_goal() {
    let mut s = EcologyState::new(42);
    let mut m = GroundedMemory::default();
    let mut x = GroundedMemory::compile_exercise(1, s.objects[0].id, GameFamily::Fetch, 0, s.objects[0].position, Vec2::new(0.2,0.3), 1.0, 0.0);
    x.index = 3; x.executed_steps = 2; x.confirmed_interventions = 1;
    m.exercise = Some(x);
    s.objects[0].lifecycle = ObjectLifecycle::StoredInDen;
    assert!(m.run_exercise(frame(Vec2::splat(0.5),false),1,&s.objects[0],s.den.anchor,intent(),0.05).is_none());
    assert_eq!(m.stats.plan_successes,0);
}

#[test]
fn latest_explicit_exercise_takes_priority_over_suspended_old_work() {
    let s = EcologyState::new(42);
    let mut m = GroundedMemory::default();
    let origin = s.objects[0].position;
    m.exercise = Some(GroundedMemory::compile_exercise(1,s.objects[0].id,GameFamily::Roll,0,origin,s.den.anchor,1.0,0.0));
    let next = GroundedMemory::compile_exercise(2,s.objects[0].id,GameFamily::OrbitInspect,0,origin,s.den.anchor,1.0,0.0);
    m.request_exercise(next).unwrap();
    m.observe(&s.objects,origin,1.0,0.05,false);
    m.run_exercise(frame(origin,true),3,&s.objects[0],s.den.anchor,intent(),0.05).unwrap();
    assert_eq!(m.exercise.as_ref().unwrap().family,GameFamily::OrbitInspect);
    assert_eq!(m.suspended.len(),1);
}

/// Closed-loop fixture: real object integrator, kinematic pet controller and
/// contact-gated actuation. This is not the native soft-body end-to-end test.
fn complete_family(family: GameFamily, aspect: f32) -> (ExerciseStatus, GroundedMemory) {
    let mut s = EcologyState::new(42);
    let start = Vec2::new(if family==GameFamily::Rebound { 0.06/aspect } else { 0.35 }, 0.973);
    s.objects[0].position = start;
    s.objects[0].velocity = if family==GameFamily::Stop { Vec2::X * 0.8 } else { Vec2::ZERO };
    s.objects[0].lifecycle=ObjectLifecycle::Free;
    let home=Vec2::new(0.60,0.973);
    let mut pet=start-Vec2::new(0.025/aspect,0.02);
    let mut m=GroundedMemory::default();
    m.exercise=Some(GroundedMemory::compile_exercise(20,s.objects[0].id,family,0,start,home,aspect,0.0));
    let scale=Vec2::new(aspect,1.0);
    let mut result=ExerciseStatus::Running;
    for _ in 0..2400 {
        for _ in 0..6 {
            if s.objects[0].lifecycle==ObjectLifecycle::CarriedByPet {
                let delta=(pet-s.objects[0].position)*scale;
                s.objects[0].velocity=delta.clamp_length_max(1.0)*12.0;
                s.objects[0].velocity=s.objects[0].velocity.clamp_length_max(MAX_OBJECT_SPEED);
                let v=s.objects[0].velocity;
                s.objects[0].position+=(v/scale)/120.0;
            } else {
                step_object(&mut s.objects[0],ObjectPhysicsConfig { desktop_aspect:aspect,..Default::default() },1.0/120.0);
            }
        }
        m.observe(&s.objects,pet,aspect,0.05,false);
        let contact=((pet-s.objects[0].position)*scale).length()<0.065;
        let mut f=frame(pet,contact); f.desktop_aspect=aspect;
        let Some(o)=m.run_exercise(f,20,&s.objects[0],home,intent(),0.05) else {continue};
        let delta=((o.intent.target_position-pet)*scale).clamp_length_max(o.intent.desired_speed*0.05);
        pet=(pet+delta/scale).clamp(Vec2::ZERO,Vec2::ONE);
        if let Some(c)=o.command {
            let before=s.objects[0].clone();
            let object=&mut s.objects[0];
            let applied=match c {
                ObjectCommand::ApplyImpulse{impulse,..} if contact => {object.lifecycle=ObjectLifecycle::Free;object.velocity=(object.velocity+impulse/object.mass).clamp_length_max(MAX_OBJECT_SPEED);true},
                ObjectCommand::MoveToward{..} if contact || object.lifecycle==ObjectLifecycle::CarriedByPet => {object.lifecycle=ObjectLifecycle::CarriedByPet;true},
                ObjectCommand::Release{velocity,..} if object.lifecycle==ObjectLifecycle::CarriedByPet => {object.lifecycle=ObjectLifecycle::Free;object.velocity=velocity;true},
                _=>false,
            };
            m.receipt(c,Some(&before),Some(object),20,aspect,applied);
        }
        if o.status!=ExerciseStatus::Running {result=o.status;break;}
    }
    (result,m)
}

#[test]
fn game_families_reach_measured_outcomes_with_native_object_dynamics() {
    for aspect in [1.0, 2.4] {
        for family in GameFamily::ALL {
            let (result,m)=complete_family(family,aspect);
            assert_eq!(result,ExerciseStatus::Succeeded,"family={family:?} aspect={aspect} active={:?} history={:?}",m.exercise,m.episodes);
            assert!(m.valid());
        }
    }
}
