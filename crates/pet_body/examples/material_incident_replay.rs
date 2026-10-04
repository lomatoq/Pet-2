//! Read-only replay of a copied material/profile and the actual diagnostic motor packet.
//! The captured packet is held constant: this isolates solver drift from new brain decisions.
use glam::Vec2;
use lifecore::{
    AffectState, BodyIntent, ExpressionState, FastPhenotypeActuation, Genome, LocomotionMode,
    PoseIntent, SensorFrame,
};
use pet_body::{
    BodyMaterialSnapshot, LiquidTuningProfile, ProceduralBody, VisualMindInput, VoiceVisualState,
};
use pet_motor::{
    BehaviorProgramId, FieldSpace, LocalSomaticField, MotorPoseIntent, SomaticActuationBus,
    SomaticActuationPacket, SomaticFieldKind,
};
use serde_json::{Value, json};

fn read<T: serde::de::DeserializeOwned>(path: &str) -> Result<T, Box<dyn std::error::Error>> {
    Ok(serde_json::from_str(&std::fs::read_to_string(path)?)?)
}

fn local_bounds(body: &ProceduralBody, iso: f32) -> (Vec2, Vec2) {
    let state = body.embodiment.liquid.render_state();
    pet_body::contact_surface_bounds(&state.particles[..state.particle_count], iso, Vec2::ZERO)
        .expect("captured main contour")
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let profile: LiquidTuningProfile = read(&args[1])?;
    let snapshot: BodyMaterialSnapshot = read(&args[2])?;
    let debug: Value = read(&args[3])?;
    let seconds: f32 = args.get(4).map_or(Ok(900.0), |s| s.parse())?;
    let scenario = args.get(5).map_or("supported", String::as_str);
    let launch_at: f32 = args.get(7).map_or(Ok(30.0), |s| s.parse())?;
    let launch: Option<SomaticActuationPacket> = args
        .get(6)
        .map(|path| -> Result<_, Box<dyn std::error::Error>> {
            let incident: Value = read(path)?;
            Ok(serde_json::from_value(
                incident["details"]["somatic"].clone(),
            )?)
        })
        .transpose()?;
    let mut packet: SomaticActuationPacket =
        serde_json::from_value(debug["motor"]["packet"].clone())?;
    let scale: Vec2 = serde_json::from_value(debug["visual_motion_scale"].clone())?;
    // Reconstruct the native desktop dimensions from the captured pixel/root pair.
    // Navigation uses pixels; the material uses the independently captured local mapping.
    let captured_root: Vec2 = serde_json::from_value(debug["body"]["world_position"].clone())?;
    let captured_center: Vec2 = serde_json::from_value(debug["screen_body_center_px"].clone())?;
    let screen = (captured_center / captured_root).round();
    let iso = profile.pbf.iso_threshold;
    let mut world_position: Vec2 = serde_json::from_value(debug["body"]["world_position"].clone())?;
    let genome = Genome::from_seed(profile.seed);
    let mut body = ProceduralBody::generate(&genome)?;
    body.apply_tuning_profile(profile)?;
    body.restore_body_material_snapshot(&snapshot)?;
    body.simulation.set_motion_space_pixels(screen);
    body.set_render_aspect(screen.x / screen.y);
    body.embodiment.set_world_to_body_scale(scale);
    body.simulation.feedback.world_position = world_position;
    let dt = 1.0 / 120.0;
    let mut peak_speed = 0.0_f32;
    let mut peak_components = 0;
    let mut minimum_main_mass = 96.0_f32;
    let mut post_launch_peak_energy = 0.0_f32;
    let mut peak_stretch = 0.0_f32;
    let mut peak_energy = 0.0_f32;
    let mut late_energy = 0.0_f32;
    let mut first_late_extent = None;
    let mut final_extent = Vec2::ZERO;
    let mut ticks = 0;
    let floor = packet.support.as_ref().map(|s| s.anchor_point.y);
    let initial_world_position = world_position;
    for tick in 0..(seconds / dt) as usize {
        let t = tick as f32 * dt;
        let supported = matches!(
            scenario,
            "supported" | "chasing" | "without_posture" | "without_flatten"
        ) || (matches!(scenario, "release" | "startle" | "captured_startle")
            && t < launch_at);
        if scenario == "without_posture" {
            packet.shape = Default::default();
        }
        if scenario == "without_flatten" {
            packet.fields[0] = None;
        }
        if supported {
            // Acquire on the actual contour once; native V63 holds that support
            // frame fixed. `chasing` is the older every-tick host projection ablation.
            let floor = packet
                .support
                .as_ref()
                .ok_or("captured packet has no support")?
                .anchor_point
                .y;
            if tick == 0 || scenario == "chasing" {
                world_position.y = floor - local_bounds(&body, iso).0.y / scale.y;
            }
            body.simulation.feedback.world_position = world_position;
            body.simulation.feedback.velocity = Vec2::ZERO;
        } else {
            packet.support = None;
            packet.fields = Default::default();
            packet.shape = Default::default();
        }
        if scenario == "startle" && t >= launch_at {
            packet = SomaticActuationPacket::default();
            packet.program = Some(BehaviorProgramId::DefenseStartleOrientFreeze);
            let cycle = (t - launch_at).rem_euclid(3.0);
            packet.locomotion.pose = if cycle < 0.18 {
                MotorPoseIntent::Travel
            } else {
                MotorPoseIntent::Brake
            };
            packet.material.flight_stretch_multiplier = if cycle < 0.18 { 1.24 } else { 1.0 };
            packet.material.viscosity_multiplier = if cycle < 0.18 { 0.86 } else { 1.0 };
            packet.fields[0] = Some(LocalSomaticField {
                kind: SomaticFieldKind::Gather,
                space: FieldSpace::BodyLocal,
                center: Vec2::ZERO,
                axis: -Vec2::Y,
                radius: 0.62,
                strength: 0.25 * (1.0 - cycle.min(1.0)),
                falloff: 2.2,
                frequency_hz: 0.0,
                phase_01: cycle.min(1.0),
                target_component: None,
            });
        }
        if scenario == "captured_startle" && t >= launch_at {
            packet = launch
                .clone()
                .ok_or("captured_startle requires incident JSON argument")?;
            if t >= launch_at + 0.18 {
                packet.locomotion.pose = MotorPoseIntent::Brake;
                packet.material.flight_stretch_multiplier = 1.0;
                packet.shape.mode = pet_motor::ShapeMode::Recoil;
                packet.shape.strength = 0.68;
            }
        }
        let braking = matches!(scenario, "captured_startle" | "startle")
            && !supported
            && packet.locomotion.pose == MotorPoseIntent::Brake;
        let previous_velocity = body.simulation.feedback.velocity;
        let intent = BodyIntent {
            locomotion: if supported {
                LocomotionMode::Sleep
            } else {
                LocomotionMode::Arrive
            },
            target_position: if supported || braking {
                world_position
            } else {
                if scenario == "captured_startle" {
                    packet
                        .locomotion
                        .target_position
                        .ok_or("captured launch has no target")?
                } else if scenario == "startle" {
                    initial_world_position - Vec2::Y * 0.135
                } else {
                    Vec2::new(0.5 + 0.3 * (t * 0.13).sin(), 0.5 + 0.3 * (t * 0.19).cos())
                }
            },
            target_surface: None,
            desired_speed: if supported || braking {
                0.0
            } else if matches!(scenario, "startle" | "captured_startle") {
                1.5
            } else {
                0.85
            },
            facing_direction: 1.0,
            gaze_target: None,
            pose: if supported {
                PoseIntent::Sleeping
            } else if scenario == "captured_startle" {
                PoseIntent::Compact
            } else {
                PoseIntent::Curious
            },
            expression: ExpressionState::default(),
            interaction_target: None,
        };
        body.set_somatic_actuation(packet.clone());
        let mut phenotype = FastPhenotypeActuation::default();
        SomaticActuationBus::compose(&mut phenotype, &packet);
        body.set_fast_phenotype_actuation(phenotype);
        let sensors = SensorFrame::default();
        body.fixed_update(&genome, &intent, &sensors, dt);
        body.embodiment.set_world_to_body_scale(scale);
        if supported {
            body.simulation.feedback.world_position = world_position;
            body.simulation.feedback.velocity = Vec2::ZERO;
        } else if scenario == "captured_startle" {
            // Same unilateral native bottom wall after physical support release.
            let maximum_root =
                floor.unwrap() - body.liquid_contact_bounds_pixels(1.0).maximum.y / screen.y;
            if body.simulation.feedback.world_position.y > maximum_root {
                body.simulation.feedback.world_position.y = maximum_root;
                if body.simulation.feedback.velocity.y > 0.0 {
                    body.simulation.feedback.velocity.y *= -0.12;
                }
                body.simulation.feedback.acceleration =
                    (body.simulation.feedback.velocity - previous_velocity) / dt;
            }
        }
        body.embodied_update(
            &intent,
            &sensors,
            AffectState::default(),
            VisualMindInput::default(),
            VoiceVisualState::default(),
            dt,
        );
        body.presentation_update(dt);
        let d = body.embodiment.liquid.diagnostics();
        peak_speed = peak_speed.max(d.maximum_speed);
        peak_components = peak_components.max(d.component_count);
        minimum_main_mass = minimum_main_mass.min(d.main_mass);
        if t >= launch_at {
            post_launch_peak_energy = post_launch_peak_energy.max(d.kinetic_energy);
        }
        peak_stretch = peak_stretch.max(d.stretch_ratio);
        peak_energy = peak_energy.max(d.kinetic_energy);
        if tick % 120 == 0 || tick + 1 == (seconds / dt) as usize {
            let (minimum, maximum) = local_bounds(&body, iso);
            final_extent = maximum - minimum;
        }
        if t >= seconds * 0.75 {
            first_late_extent.get_or_insert(final_extent);
            late_energy = late_energy.max(d.kinetic_energy);
        }
        ticks = tick + 1;
        if tick % 3600 == 0 || d.failsafe_hits > 0 || !d.finite {
            println!(
                "{}",
                json!({"t":t,"components":d.component_count,"main_mass":d.main_mass,"energy":d.kinetic_energy,
                "speed":d.maximum_speed,"stretch":d.stretch_ratio,"compression":d.maximum_compression,"aspect":d.permanent_field_aspect,
                "support_load":d.support_field_load,"extent":final_extent,"failsafe_hits":d.failsafe_hits,"recoveries":d.recovery_count,"finite":d.finite})
            );
        }
        if d.failsafe_hits > 0 || !d.finite {
            break;
        }
    }
    println!(
        "{}",
        json!({"summary":true,"scenario":scenario,"simulated_seconds":ticks as f32*dt,"captured_sequence":snapshot.deterministic_sequence,
        "screen":screen,"peak_components":peak_components,"minimum_main_mass":minimum_main_mass,"post_launch_peak_energy":post_launch_peak_energy,"peak_speed":peak_speed,"peak_stretch":peak_stretch,"peak_energy":peak_energy,"late_peak_energy":late_energy,"late_start_extent":first_late_extent,
        "final_extent":final_extent,"failsafe_hits":body.embodiment.liquid.diagnostics().failsafe_hits,"recoveries":body.embodiment.liquid.diagnostics().recovery_count})
    );
    if scenario == "captured_startle"
        && (minimum_main_mass < 96.0
            || peak_components != 1
            || body.embodiment.liquid.diagnostics().failsafe_hits != 0
            || body.embodiment.liquid.diagnostics().recovery_count != 0)
    {
        return Err(format!("loaded Startle lost cohesion: main mass {minimum_main_mass}/96, components {peak_components}").into());
    }
    Ok(())
}
