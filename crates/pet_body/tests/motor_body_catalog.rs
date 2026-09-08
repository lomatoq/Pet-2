use glam::Vec2;
use lifecore::{
    ActionId, AffectState, BehaviorGoalFrame, BodyIntent, DerivedNervousState, Drives,
    FastPhenotypeActuation, FeltStateV1, Genome, LocomotionMode, PoseIntent, SensorFrame,
    SurfaceId, SurfaceRect,
};
use pet_body::{ProceduralBody, VisualMindInput, VoiceVisualState};
use pet_motor::{
    BehaviorContextFrame, BehaviorPerformanceRuntime, BehaviorProgramId, MotorWorldEvent,
    SomaticActuationBus, SurfaceCandidate,
};

const DESKTOP: Vec2 = Vec2::new(1_920.0, 1_080.0);

fn goal(genome: &Genome) -> BehaviorGoalFrame {
    let target = Vec2::new(0.74, 0.56);
    BehaviorGoalFrame {
        action: ActionId::IdleHover,
        body_intent: BodyIntent {
            locomotion: LocomotionMode::Arrive,
            target_position: target,
            target_surface: None,
            desired_speed: 0.36,
            facing_direction: 1.0,
            gaze_target: Some(target),
            pose: PoseIntent::Curious,
            expression: Default::default(),
            interaction_target: None,
        },
        affect: AffectState::default(),
        drives: Drives::initial(&genome.temperament),
        felt: FeltStateV1 {
            activation: 0.62,
            agency_match: 0.82,
            social_safety: 0.78,
            body_integrity: 1.0,
            ..FeltStateV1::default()
        },
        derived: DerivedNervousState::default(),
        attachment: 0.35,
        recent_outcome: None,
    }
}

fn context() -> BehaviorContextFrame {
    let bottom = SurfaceId("screen:bottom_edge".into());
    let mut context = BehaviorContextFrame::default();
    context.body.motion.world_position = Vec2::new(0.50, 0.91);
    context.cursor_position = Vec2::new(0.78, 0.43);
    context.cursor_velocity = Vec2::new(0.08, -0.03);
    context.pointer_down = true;
    context.pointer_pressed = true;
    context.pointer_released = true;
    context.pet_touched = true;
    context.pet_dragged = true;
    context.gesture_ended = true;
    context.selected_salience = 1.0;
    context.locomotion_completed = true;
    context.body_bottom_extent = 0.05;
    context.screen_edge_gap_px = 1.0;
    context.screen_edge_normal_velocity_px_s = 0.0;
    context.screen_edge_support_stable_seconds = 1.0;
    context.screen_edge_supported = true;
    context.somatic.contact_fraction = 0.40;
    context.somatic.support_stability = 1.0;
    context.somatic.supported = true;
    context.den_anchor = Some(Vec2::new(0.16, 0.91));
    context.den_familiarity = 1.0;
    context.orb_position = Some(Vec2::new(0.71, 0.52));
    context.orb_stored = true;
    context.world_event = MotorWorldEvent::OrbStored;
    context.surfaces.push(SurfaceCandidate {
        surface_id: bottom,
        minimum: Vec2::new(0.0, 0.999),
        maximum: Vec2::ONE,
        velocity: Vec2::ZERO,
        familiarity: 1.0,
        recent_failed_landings: 0,
    });
    context
}

fn run_program(program: BehaviorProgramId, cadence: &[f32]) {
    let seed = 0x6400_B0D1_u64 ^ program.index() as u64;
    let genome = Genome::from_seed(seed);
    let goal = goal(&genome);
    let mut context = context();
    let mut motor = BehaviorPerformanceRuntime::new(seed);
    let mut body = ProceduralBody::generate(&genome).expect("catalog body must generate");
    body.simulation.feedback.world_position = Vec2::new(0.50, 0.86);
    body.simulation.set_motion_space_pixels(DESKTOP);
    body.set_desktop_motion_space(DESKTOP, 360.0);
    motor.begin_lab_fixture(program, &goal, &context);

    let sensors = SensorFrame {
        cursor_position: context.cursor_position,
        visible_surfaces: vec![SurfaceRect {
            id: SurfaceId("screen:bottom_edge".into()),
            rect: lifecore::Rect {
                minimum: Vec2::new(0.0, 0.999),
                maximum: Vec2::ONE,
            },
        }],
        ..SensorFrame::default()
    };
    let mut elapsed = 0.0_f32;
    let mut frame = 0_u64;
    let mut cadence_index = 0_usize;
    let mut previous_velocity_px_s = Vec2::ZERO;
    let mut previous_body_v2 = None;
    let mut collision_impulses = 0_u64;

    while motor.active().is_some() && elapsed < 45.0 {
        let dt = cadence[cadence_index % cadence.len()];
        cadence_index += 1;
        frame += 1;
        elapsed += dt;
        context.frame_id = frame;
        context.timestamp_seconds = f64::from(elapsed);

        let packet = motor.tick_lab_fixture(&goal, &context, dt);
        let mut intent = goal.body_intent.clone();
        SomaticActuationBus::apply_to_intent(&packet, &context, &mut intent);
        let mut phenotype = FastPhenotypeActuation::default();
        SomaticActuationBus::compose(&mut phenotype, &packet);
        body.set_fast_phenotype_actuation(phenotype);
        body.set_somatic_actuation(packet);

        let legacy = body.fixed_update(&genome, &intent, &sensors, dt).clone();
        body.embodied_update(
            &intent,
            &sensors,
            goal.affect,
            VisualMindInput::default(),
            VoiceVisualState::default(),
            dt,
        );

        let diagnostics = body.simulation.diagnostics();
        let jerk_budget = match intent.locomotion {
            LocomotionMode::Seek | LocomotionMode::Flee | LocomotionMode::Orbit => 48.0,
            LocomotionMode::Landing | LocomotionMode::Sleep | LocomotionMode::Cocoon => 18.0,
            _ => 32.0,
        } * DESKTOP.min_element();
        assert!(
            diagnostics.jerk_px_s3.length() <= jerk_budget + 1.0,
            "{} exceeded jerk budget at {:?}: {} > {}",
            program.wire_name(),
            intent.locomotion,
            diagnostics.jerk_px_s3.length(),
            jerk_budget
        );
        assert!(
            diagnostics.saturation_fraction().is_finite()
                && diagnostics.saturation_fraction() <= 1.0,
            "{} invalid controller saturation",
            program.wire_name()
        );

        let velocity_px_s = legacy.velocity * DESKTOP;
        let displacement_px = velocity_px_s * dt;
        assert!(
            displacement_px.length() <= 52.0,
            "{} teleported {} px in one frame",
            program.wire_name(),
            displacement_px.length()
        );
        if previous_velocity_px_s.length() > 120.0 && velocity_px_s.length() > 120.0 {
            assert!(
                previous_velocity_px_s.dot(velocity_px_s) >= 0.0,
                "{} reversed direction in one frame: {:?} -> {:?}",
                program.wire_name(),
                previous_velocity_px_s,
                velocity_px_s
            );
        }
        previous_velocity_px_s = velocity_px_s;
        collision_impulses += u64::from(legacy.collision.is_some());

        let body_v2 = body.body_feedback_v2(&intent, &sensors, previous_body_v2.as_ref(), frame);
        let liquid = body.embodiment.liquid.diagnostics();
        assert!(
            liquid.finite,
            "{} produced non-finite PBF",
            program.wire_name()
        );
        assert_eq!(
            liquid.failsafe_hits,
            0,
            "{} hit PBF failsafe",
            program.wire_name()
        );
        assert_eq!(
            liquid.recovery_count,
            0,
            "{} triggered whole-solver recovery",
            program.wire_name()
        );
        assert!(legacy.world_position.is_finite());
        assert!(legacy.velocity.is_finite());
        assert!(legacy.acceleration.is_finite());

        context.body = body_v2;
        context.body.contact.contact_count = 1;
        context.body.contact.duration = elapsed;
        context.body.contact.tangential_speed = 0.02;
        context.somatic = body.somatic_feedback();
        context.somatic.contact_fraction = context.somatic.contact_fraction.max(0.40);
        context.somatic.support_stability = 1.0;
        context.somatic.supported = true;
        context.locomotion_completed = legacy.locomotion_completed || elapsed > 0.5;
        context.screen_edge_gap_px = 1.0;
        context.screen_edge_normal_velocity_px_s = 0.0;
        context.screen_edge_support_stable_seconds = 1.0;
        context.screen_edge_supported = true;
        previous_body_v2 = Some(body_v2);
    }

    assert!(
        motor.active().is_none(),
        "{} did not finish at cadence {:?} after {elapsed:.2}s",
        program.wire_name(),
        cadence
    );
    assert!(collision_impulses <= frame, "collision counter overflow");
}

#[test]
fn all_64_motor_programs_are_finite_and_jerk_bounded_at_30_60_120_and_variable_hz() {
    let cadences: [&[f32]; 4] = [
        &[1.0 / 30.0],
        &[1.0 / 60.0],
        &[1.0 / 120.0],
        &[1.0 / 30.0, 1.0 / 120.0, 1.0 / 50.0, 1.0 / 90.0, 1.0 / 60.0],
    ];
    for program in BehaviorProgramId::ALL {
        for cadence in cadences {
            run_program(program, cadence);
        }
    }
}
