use std::{error::Error, path::PathBuf};

use glam::Vec2;
use lifecore::{
    ActionId, AffectState, BehaviorGoalFrame, BodyIntent, DerivedNervousState, Drives,
    EmbodiedGestureKind, FeltStateV1, Genome, LocomotionMode, PoseIntent, SurfaceId,
    stable_hash_bytes,
};
use pet_motor::{
    BehaviorContextFrame, BehaviorPerformanceRuntime, MotorWorldEvent, MotorWorldGoal,
    SurfaceCandidate,
};

fn goal(action: ActionId, target: Vec2) -> BehaviorGoalFrame {
    let genome = Genome::from_seed(0xD37E_12A5);
    BehaviorGoalFrame {
        action,
        body_intent: BodyIntent {
            locomotion: LocomotionMode::Arrive,
            target_position: target,
            target_surface: None,
            desired_speed: 0.30,
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
            social_safety: 0.72,
            body_integrity: 1.0,
            ..FeltStateV1::default()
        },
        derived: DerivedNervousState::default(),
        attachment: 0.35,
        recent_outcome: None,
    }
}

fn run_scenario(
    name: &str,
    seed: u64,
    mut goal: BehaviorGoalFrame,
    mut context: BehaviorContextFrame,
    frames: u64,
    mutate: impl Fn(u64, &mut BehaviorGoalFrame, &mut BehaviorContextFrame),
    lines: &mut Vec<String>,
) -> Result<(), Box<dyn Error>> {
    let mut runtime = BehaviorPerformanceRuntime::new(seed);
    for frame in 0..frames {
        context.frame_id = frame + 1;
        context.timestamp_seconds = frame as f64 / 20.0;
        mutate(frame, &mut goal, &mut context);
        let packet = runtime.tick(&goal, &context, 1.0 / 20.0);
        let trace = runtime.trace_tail(1).pop();
        lines.push(serde_json::to_string(&serde_json::json!({
            "milestone": name,
            "frame": frame + 1,
            "program": packet.program,
            "phase": packet.phase,
            "phase_name": packet.phase_name,
            "phase_progress": packet.phase_progress,
            "source_bout_id": packet.source_bout_id,
            "target_locked": packet.locomotion.target_locked,
            "field_count": packet.field_count(),
            "support_active": packet.support.is_some(),
            "locomotion_pose": packet.locomotion.pose,
            "locomotion": packet.locomotion,
            "material": packet.material,
            "internal": packet.internal,
            "expression": packet.expression,
            "last_completion": runtime.last_completion(),
            "cause_trace": trace,
        }))?);
    }
    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    let output = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("reports/r13_deterministic_traces.jsonl"));
    let mut lines = Vec::new();

    let mut motion_context = BehaviorContextFrame::default();
    motion_context.body.motion.world_position = Vec2::new(0.18, 0.28);
    run_scenario(
        "motion",
        0x1001,
        goal(ActionId::ExploreScreen, Vec2::new(0.82, 0.66)),
        motion_context,
        18,
        |_, _, _| {},
        &mut lines,
    )?;

    let mut surface_goal = goal(ActionId::LandOnWindow, Vec2::new(0.72, 0.64));
    surface_goal.drives.sleep = 0.82;
    surface_goal.felt.sleep_pressure = 0.78;
    let mut surface_context = BehaviorContextFrame::default();
    surface_context.body.motion.world_position = Vec2::new(0.22, 0.24);
    surface_context.surfaces.push(SurfaceCandidate {
        surface_id: SurfaceId("trace-surface".to_owned()),
        minimum: Vec2::new(0.36, 0.58),
        maximum: Vec2::new(0.86, 0.88),
        velocity: Vec2::ZERO,
        familiarity: 0.72,
        recent_failed_landings: 0,
    });
    run_scenario(
        "surface",
        0x2002,
        surface_goal,
        surface_context,
        28,
        |frame, _, context| {
            if frame >= 7 {
                context.body.motion.world_position = Vec2::new(0.37, 0.54);
            }
            if frame >= 14 {
                context.somatic.contact_fraction = 0.28;
                context.somatic.support_stability = 0.82;
            }
            if frame >= 20 {
                context.somatic.supported = true;
                context.somatic.supported_seconds = (frame - 19) as f32 / 20.0;
            }
        },
        &mut lines,
    )?;

    let mut sleep_goal = goal(ActionId::Sleep, Vec2::new(0.50, 0.95));
    sleep_goal.drives.sleep = 0.92;
    sleep_goal.felt.sleep_pressure = 0.90;
    sleep_goal.felt.activation = 0.18;
    let mut sleep_context = BehaviorContextFrame::default();
    sleep_context.body.motion.world_position = Vec2::new(0.50, 0.62);
    sleep_context.body_bottom_extent = 0.05;
    sleep_context.surfaces.push(SurfaceCandidate {
        surface_id: SurfaceId("screen:bottom_edge".to_owned()),
        minimum: Vec2::new(0.0, 0.999),
        maximum: Vec2::ONE,
        velocity: Vec2::ZERO,
        familiarity: 1.0,
        recent_failed_landings: 0,
    });
    run_scenario(
        "sleep",
        0x2A02,
        sleep_goal,
        sleep_context,
        40,
        |frame, _, context| {
            if frame < 8 {
                context.screen_edge_gap_px = 64.0 - frame as f32 * 6.0;
                context.screen_edge_normal_velocity_px_s = 280.0;
            } else if frame < 14 {
                context.screen_edge_gap_px = 6.0 - (frame - 8) as f32 * 0.75;
                context.screen_edge_normal_velocity_px_s = 32.0;
            } else {
                context.screen_edge_gap_px = 2.0;
                context.screen_edge_normal_velocity_px_s = 0.0;
                context.screen_edge_support_stable_seconds = (frame - 13) as f32 / 20.0;
                context.screen_edge_supported = context.screen_edge_support_stable_seconds >= 0.30;
                context.somatic.contact_fraction = 0.32;
                context.somatic.support_stability = 1.0;
            }
        },
        &mut lines,
    )?;

    let mut touch_context = BehaviorContextFrame::default();
    touch_context.body.motion.world_position = Vec2::new(0.48, 0.48);
    touch_context.body.contact.normal = Vec2::NEG_Y;
    touch_context.body.contact.pressure = 0.24;
    touch_context.cursor_position = Vec2::new(0.50, 0.46);
    touch_context.gesture = EmbodiedGestureKind::SoftTouch;
    touch_context.gesture_confidence = 0.91;
    touch_context.pointer_down = true;
    run_scenario(
        "touch",
        0x3003,
        goal(ActionId::IdleHover, Vec2::new(0.48, 0.48)),
        touch_context,
        14,
        |frame, _, context| {
            if frame == 13 {
                context.gesture_ended = true;
                context.pointer_down = false;
                context.pointer_released = true;
            }
        },
        &mut lines,
    )?;

    let mut physiology_goal = goal(ActionId::IdleHover, Vec2::new(0.50, 0.50));
    physiology_goal.body_intent.desired_speed = 0.0;
    physiology_goal.affect.arousal = 0.78;
    physiology_goal.affect.valence = 0.0;
    physiology_goal.felt.comfort = 0.80;
    let mut physiology_context = BehaviorContextFrame::default();
    physiology_context.body.motion.world_position = Vec2::new(0.50, 0.50);
    run_scenario(
        "physiology",
        0x4004,
        physiology_goal,
        physiology_context,
        31,
        |_, _, _| {},
        &mut lines,
    )?;

    let mut den_context = BehaviorContextFrame::default();
    den_context.body.motion.world_position = Vec2::new(0.62, 0.42);
    den_context.den_anchor = Some(Vec2::new(0.04, 0.80));
    den_context.den_familiarity = 0.68;
    den_context.orb_position = Some(Vec2::new(0.18, 0.72));
    den_context.world_goal = MotorWorldGoal::ReturnHome;
    run_scenario(
        "den",
        0x5005,
        goal(ActionId::BringProceduralOrb, Vec2::new(0.04, 0.80)),
        den_context,
        30,
        |frame, _, context| {
            context.world_event = match frame {
                0 => MotorWorldEvent::DenFieldEntered,
                5 => MotorWorldEvent::OrbCaptureStarted,
                10 => MotorWorldEvent::OrbCaptureAcceleration,
                25 => MotorWorldEvent::OrbStored,
                _ => MotorWorldEvent::None,
            };
            if frame >= 25 {
                context.orb_stored = true;
                context.orb_position = context.den_anchor;
            }
        },
        &mut lines,
    )?;

    let mut payload = lines.join("\n");
    payload.push('\n');
    let digest = stable_hash_bytes(payload.as_bytes());
    payload.push_str(&serde_json::to_string(&serde_json::json!({
        "kind": "deterministic_digest",
        "algorithm": "lifecore_stable_hash_bytes",
        "record_count": lines.len(),
        "digest": format!("{digest:016x}"),
    }))?);
    payload.push('\n');
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&output, payload)?;
    println!("{}", output.display());
    Ok(())
}
