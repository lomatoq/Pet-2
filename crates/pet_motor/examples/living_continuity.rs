//! Fixed-seed causal comparisons; measured inputs are fixtures, not a body playtest.
use glam::Vec2;
use lifecore::*;
use pet_motor::*;

fn main() {
    for seed in [7, 63, 164] {
        for case in [
            "alert_cursor",
            "tired_cursor",
            "supported_sleep_wake",
            "sleep_nomination",
            "gentle_contact_release",
            "forceful_contact_release",
            "exertion_recovery",
            "quiet_absence",
        ] {
            let mut life = LifeCore::new(Genome::from_seed(seed), seed);
            life.state.drives.sleep = if case == "tired_cursor" { 0.9 } else { 0.12 };
            life.state.drives.play = 0.8;
            if case.contains("sleep") {
                life.state.current_action = ActionId::Sleep;
                life.state.drives.sleep = 0.8;
            }
            let mut motor = BehaviorPerformanceRuntime::new(seed);
            let mut last_program = None;
            for step in 0..900 {
                let seconds = step as f32 * 0.05;
                let contact = case.contains("contact") && (3.0..9.0).contains(&seconds);
                let forceful = contact && case.starts_with("forceful");
                let resting = case == "supported_sleep_wake" && seconds < 20.0;
                let effort = case == "exertion_recovery" && seconds < 20.0;
                let snapshot = InteroceptionSnapshot {
                    felt: FeltStateV1 {
                        contact_pleasantness: if contact && !forceful { 0.8 } else { 0.0 },
                        social_safety: if forceful { 0.0 } else { 0.9 },
                        pain_like: if forceful { 0.65 } else { 0.0 },
                        restraint: if forceful { 0.8 } else { 0.0 },
                        physical_load: if effort { 0.75 } else { 0.0 },
                        agency_match: 0.9,
                        motor_efficacy: 0.8,
                        body_integrity: 1.0,
                        ..Default::default()
                    },
                    ..Default::default()
                };
                life.integrate_felt_state(
                    snapshot,
                    EpisodeContextV1 {
                        sleeping_or_deep_rest: f32::from(resting),
                        rest_quality: f32::from(resting),
                        safe_social_exchange: f32::from(contact && !forceful),
                        ..Default::default()
                    },
                    0.05,
                );
                if case == "supported_sleep_wake" && step == 400 {
                    life.reserve_awake_for_physiology();
                }
                let cursor = case.ends_with("cursor");
                let sensors = SensorFrame {
                    timestamp: f64::from(seconds),
                    pet_touched: contact,
                    pointer_pressed: contact,
                    cursor_position: Vec2::new(0.62, 0.5),
                    cursor_distance_to_pet: 0.12,
                    cursor_velocity: if cursor {
                        Vec2::new(0.6, 0.0)
                    } else {
                        Vec2::ZERO
                    },
                    user_presence: Some(f32::from(case != "quiet_absence")),
                    user_availability: Some(if case == "quiet_absence" { 0.0 } else { 0.7 }),
                    ..Default::default()
                };
                let output = life.tick(&sensors, &BodyFeedback::default(), 0.05);
                let goal = BehaviorGoalFrame {
                    action: output.selected_action,
                    body_intent: output.body_intent,
                    affect: output.affect,
                    drives: life.state.drives,
                    felt: snapshot.felt,
                    derived: DerivedNervousState {
                        fatigue: life.state.drives.sleep,
                        arousal: output.affect.arousal,
                        ..Default::default()
                    },
                    attachment: life.state.affect.attachment,
                    recent_outcome: None,
                };
                let mut context = BehaviorContextFrame {
                    frame_id: step + 1,
                    timestamp_seconds: f64::from(seconds),
                    cursor_position: sensors.cursor_position,
                    cursor_velocity: sensors.cursor_velocity,
                    pet_touched: contact,
                    pointer_down: contact,
                    boundary_violation: f32::from(forceful),
                    ..Default::default()
                };
                context.body.contact.contact_count = u16::from(contact);
                context.body.contact.duration = if contact { seconds - 3.0 } else { 0.0 };
                context.body.contact.point_world = sensors.cursor_position;
                context.body.contact.pressure = if forceful { 0.85 } else { 0.18 };
                let packet = motor.tick(&goal, &context, 0.05);
                if step % 40 == 0 || packet.program != last_program {
                    println!(
                        "{}",
                        serde_json::json!({"case":case,"seed":seed,"seconds":seconds,
                        "activity":life.state.activity,"drowsiness":life.state.activity.drowsiness(life.state.drives.sleep),
                        "vigor":life.state.activity.vigor(life.state.drives.sleep),"action":goal.action,
                        "affect":goal.affect,"sleep_debt":goal.drives.sleep,
                        "program":packet.program,"phase":packet.phase_name,
                        "eye_aperture_delta":packet.expression.eye_aperture_delta,"relief":packet.expression.relief,
                        "breath_speed":packet.internal.breath_speed_multiplier,
                        "scope":"production cognition+motor; immutable measured input fixtures; no rendered-body claim"})
                    );
                }
                last_program = packet.program;
            }
        }
    }
}
