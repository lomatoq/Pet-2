//! Seeded causal audit of production LifeCore + motor + physiology. Physical
//! support/travel and eyelids are explicit fixtures, not a rendered body test.
use std::{env, error::Error, fs::{self, File}, io::{BufWriter, Write}, path::PathBuf};
use glam::Vec2;
use lifecore::{ActionId, BehaviorGoalFrame, BodyFeedback, BodyIntent, Genome, LifeCore, SensorFrame};
use pet_ecology::{DigestiveTract, DigestionFrame, ToiletingCoordinator, ToiletingEvidence, WasteWorld};
use pet_motor::{BehaviorContextFrame, BehaviorPerformanceRuntime, BehaviorProgramId, MotorPoseIntent};
use serde_json::json;

fn motor_goal(core: &LifeCore, intent: BodyIntent) -> BehaviorGoalFrame {
    BehaviorGoalFrame { action: core.state.current_action, body_intent: intent,
        affect: core.state.affect, drives: core.state.drives, felt: Default::default(),
        derived: Default::default(), attachment: core.state.affect.attachment, recent_outcome: None }
}

fn run(dir: &std::path::Path, case: &str, seed: u64, legacy: bool) -> Result<serde_json::Value, Box<dyn Error>> {
    const DT: f32 = 1.0 / 120.0;
    let asleep = case == "sleep" || case == "drag" || case == "far_site";
    let mut core = LifeCore::new(Genome::from_seed(seed), seed);
    core.state.current_action = if asleep { ActionId::Sleep } else { ActionId::IdleHover };
    core.state.drives.sleep = if asleep { 0.95 } else { 0.2 };
    let mut motor = BehaviorPerformanceRuntime::new(seed);
    let feedback = BodyFeedback { world_position: Vec2::new(0.5, 0.95), ..Default::default() };
    let mut context = BehaviorContextFrame {
        den_supported: true,
        // Explicitly measured in this reduced fixture, whose support plane is 0.98.
        den_support_point: Some(Vec2::new(0.5, 0.98)),
        ..Default::default()
    };
    context.body.motion.world_position = feedback.world_position;
    context.surfaces.push(pet_motor::SurfaceCandidate {
        surface_id: lifecore::SurfaceId("screen:bottom_edge".into()),
        minimum: Vec2::new(0.0, 0.98), maximum: Vec2::new(1.0, 0.98),
        velocity: Vec2::ZERO, familiarity: 1.0, recent_failed_landings: 0,
    });
    context.screen_edge_supported = true;
    context.screen_edge_support_stable_seconds = 12.0;
    context.screen_edge_gap_px = 0.0;
    let mut intent = core.tick(&SensorFrame::default(), &feedback, 0.05).body_intent;
    let mut packet = pet_motor::SomaticActuationPacket::default();
    // Establish the actual supported sleep motor before injecting a bowel need.
    for _ in 0..1200 {
        if asleep { intent.locomotion = lifecore::LocomotionMode::Sleep; intent.pose = lifecore::PoseIntent::Sleeping; }
        packet = motor.tick(&motor_goal(&core, intent.clone()), &context, DT);
    }
    let initial_sleep_pose = packet.locomotion.pose == MotorPoseIntent::SupportedSleep;
    if asleep { assert!(initial_sleep_pose, "sleep fixture never reached supported NREM: {:?}", packet.program); }
    let mut gut = DigestiveTract { bowel: if case == "no_need" { 0.0 } else { 0.24 },
        hydration: 0.8, ingested: if case == "no_need" { 0.0 } else { 0.24 }, ..Default::default() };
    let mut waste = WasteWorld::default();
    let mut coordinator = ToiletingCoordinator::default();
    let path = dir.join(format!("{case}-{seed}-{}.jsonl", if legacy { "legacy" } else { "fixed" }));
    let mut writer = BufWriter::new(File::create(path)?);
    let mut violations = 0_u32;
    let mut first_emission: Option<f32> = None;
    let mut first_approach: Option<f32> = None;
    let mut phases = std::collections::BTreeSet::new();
    let mut max_mass_error = 0.0_f64;
    let mut emitted_closed_awake = false;
    for tick in 0..2400_u32 {
        let seconds = tick as f32 * DT;
        let observed_action = core.state.current_action;
        let dragged = case == "drag" && seconds < 3.0;
        let interrupted = case == "interruption" && first_emission.is_some_and(|t| seconds >= t + 0.1 && seconds < t + 0.8);
        let supported = case != "unsupported" && !interrupted && !dragged;
        let at_site = case != "far_site" || first_approach.is_some_and(|t| seconds >= t + 0.8);
        let sleeping = core.state.current_action == ActionId::Sleep
            || packet.locomotion.pose == MotorPoseIntent::SupportedSleep
            || intent.locomotion == lifecore::LocomotionMode::Sleep
            || intent.pose == lifecore::PoseIntent::Sleeping;
        let waking = core.state.current_action == ActionId::WakeUp
            || packet.program == Some(BehaviorProgramId::RestRemDreamWake);
        let evidence = ToiletingEvidence { needed: gut.needs_to_go() || waste.active(),
            allowed: !dragged, sleeping, waking, at_site, supported, slow: !interrupted,
            distance_to_site: Some(if at_site { 0.0 } else { 0.2 }) };
        let execution = coordinator.step(evidence, DT);
        if !legacy && execution.reserve_awake { core.reserve_awake_for_physiology(); }
        if (execution.may_approach || (legacy && evidence.needed && evidence.allowed)) && first_approach.is_none() { first_approach = Some(seconds); }
        let settled = if legacy { at_site && supported && !dragged } else { execution.may_eliminate };
        let before = gut.expelled;
        waste.step(&mut gut, DigestionFrame { settled, floor: 0.98,
            outlet: Vec2::new(0.55, 0.95), ..Default::default() }, DT);
        let emission = (gut.expelled - before).max(0.0);
        // Controlled blink counterfactual: no classifier reads this column.
        let eye_closed = case == "awake_blink" && (seconds % 0.45) < 0.15;
        if emission > 1e-8 {
            first_emission.get_or_insert(seconds);
            if sleeping || waking || !supported || dragged { violations += 1; }
            emitted_closed_awake |= eye_closed && !sleeping && !waking;
        }
        let mass = f64::from(gut.bowel) + waste.chains.iter().map(|c| f64::from(c.mass)).sum::<f64>();
        max_mass_error = max_mass_error.max((mass - gut.ingested).abs());
        phases.insert(format!("{:?}", execution.phase));
        if tick % 6 == 0 {
            writeln!(writer, "{}", json!({"schema": 1, "source": "production_components_with_physical_fixtures",
                "case": case, "seed": seed, "legacy": legacy, "t": seconds,
                "action": format!("{observed_action:?}"), "action_after_reservation": format!("{:?}",core.state.current_action), "motor_program": packet.program.map(|p|format!("{p:?}")),
                "motor_phase": packet.phase_name, "motor_pose": packet.locomotion.pose,
                "sleeping": sleeping, "waking": waking, "eye_closed_fixture": eye_closed,
                "supported": supported, "at_site": at_site, "dragged": dragged,
                "need": evidence.needed, "bowel": gut.bowel, "expelled": gut.expelled,
                "emission": emission, "settled": settled, "toileting": execution }))?;
            let sensors = SensorFrame { pet_dragged: dragged, ..Default::default() };
            let output = core.tick(&sensors, &feedback, DT * 6.0);
            intent = output.body_intent;
            context.pet_dragged = dragged;
            context.den_supported = supported;
            context.screen_edge_supported = supported;
            packet = motor.tick(&motor_goal(&core, intent.clone()), &context, DT * 6.0);
        }
    }
    writer.flush()?;
    if !legacy {
        assert_eq!(violations, 0, "{case}/{seed}: unsafe emission");
        assert!(max_mass_error < 1e-6, "{case}/{seed}: conservation {max_mass_error}");
        if case == "unsupported" || case == "no_need" { assert!(first_emission.is_none()); }
        else { assert!(first_emission.is_some(), "{case}/{seed}: interlock deadlocked"); }
    }
    Ok(json!({"case":case,"seed":seed,"legacy":legacy,"initial_sleep_pose":initial_sleep_pose,
        "unsafe_emissions":violations,"first_emission_s":first_emission,"first_approach_s":first_approach,
        "phases":phases,"max_mass_error":max_mass_error,"closed_awake_emission":emitted_closed_awake}))
}

fn main() -> Result<(), Box<dyn Error>> {
    let dir = PathBuf::from(env::args().nth(1).unwrap_or_else(|| "behavior-traces".into()));
    fs::create_dir_all(&dir)?;
    let mut results = Vec::new();
    for case in ["sleep", "awake_blink", "unsupported", "drag", "far_site", "interruption", "no_need"] {
        for seed in [63, 164, 265] { for legacy in [true, false] { results.push(run(&dir, case, seed, legacy)?); } }
    }
    let summary = json!({"schema":1,"physics":"fixtures; not native rendered PBF", "results":results});
    fs::write(dir.join("summary.json"), serde_json::to_vec_pretty(&summary)?)?;
    println!("{}", json!({"traces":results.len(),"summary":dir.join("summary.json")}));
    Ok(())
}
