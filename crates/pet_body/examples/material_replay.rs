//! Replay the authoritative material with a copied profile/snapshot; never edits saves.
use glam::Vec2;
use lifecore::{
    AffectState, BodyIntent, ExpressionState, Genome, LocomotionMode, PoseIntent, SensorFrame,
};
use pet_body::{
    BodyMaterialSnapshot, LiquidTuningProfile, ProceduralBody, VisualMindInput, VoiceVisualState,
};
use pet_motor::{ShapeIntent, ShapeMode, SomaticActuationPacket};
use serde_json::json;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let profile: LiquidTuningProfile = serde_json::from_str(&std::fs::read_to_string(&args[1])?)?;
    let snapshot: BodyMaterialSnapshot = serde_json::from_str(&std::fs::read_to_string(&args[2])?)?;
    let genome = Genome::from_seed(profile.seed);
    let mut body = ProceduralBody::generate(&genome)?;
    body.apply_tuning_profile(profile)?;
    body.restore_body_material_snapshot(&snapshot)?;
    body.set_desktop_motion_space(Vec2::new(1920.0, 1080.0), 1080.0);
    body.set_render_aspect(1920.0 / 1080.0);
    body.simulation.feedback.world_position = Vec2::splat(0.5);
    let dt = 1.0 / 120.0;
    let mut peak_speed = 0.0_f32;
    let mut peak_stretch = 0.0_f32;
    let mut peak_components = 1;
    let mut peak_compression = 0.0_f32;
    let mut min_mass = 96.0_f32;
    let mut last_components = 0;
    for tick in 0..36000 {
        let t = tick as f32 * dt;
        let block = (t / 30.0) as u32;
        let pose = if block.is_multiple_of(3) {
            PoseIntent::Sleeping
        } else {
            PoseIntent::Curious
        };
        let target = if block.is_multiple_of(3) {
            Vec2::splat(0.5)
        } else {
            Vec2::new(0.5 + 0.35 * (t * 0.13).sin(), 0.5 + 0.30 * (t * 0.19).cos())
        };
        let intent = BodyIntent {
            locomotion: LocomotionMode::Arrive,
            target_position: target,
            target_surface: None,
            desired_speed: if block.is_multiple_of(3) { 0.0 } else { 0.85 },
            facing_direction: 1.0,
            gaze_target: None,
            pose,
            expression: ExpressionState::default(),
            interaction_target: None,
        };
        let shape = ShapeIntent {
            mode: if block % 3 == 2 {
                ShapeMode::Reach
            } else {
                ShapeMode::Neutral
            },
            axis: Vec2::from_angle(t * 0.2),
            strength: if block % 3 == 2 { 0.75 } else { 0.0 },
        };
        body.set_somatic_actuation(SomaticActuationPacket {
            shape,
            ..Default::default()
        });
        let sensors = SensorFrame::default();
        body.fixed_update(&genome, &intent, &sensors, dt);
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
        peak_stretch = peak_stretch.max(d.stretch_ratio);
        peak_compression = peak_compression.max(d.maximum_compression);
        peak_components = peak_components.max(d.component_count);
        min_mass = min_mass.min(d.main_mass);
        if d.component_count != last_components || d.failsafe_hits > 0 || !d.finite {
            println!(
                "{}",
                json!({"t":t,"components":d.component_count,"main_mass":d.main_mass,
                "speed":d.maximum_speed,"stretch":d.stretch_ratio,"compression":d.maximum_compression,
                "failsafe_hits":d.failsafe_hits,"recoveries":d.recovery_count,"finite":d.finite})
            );
            last_components = d.component_count;
        }
        if d.failsafe_hits > 0 || !d.finite {
            break;
        }
    }
    println!(
        "{}",
        json!({"summary":true,"simulated_seconds":300,"peak_components":peak_components,
        "minimum_main_mass":min_mass,"peak_speed":peak_speed,"peak_stretch":peak_stretch,"peak_compression":peak_compression,
        "failsafe_hits":body.embodiment.liquid.diagnostics().failsafe_hits,
        "recovery_count":body.embodiment.liquid.diagnostics().recovery_count})
    );
    Ok(())
}
