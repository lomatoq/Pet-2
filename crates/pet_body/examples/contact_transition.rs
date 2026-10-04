//! Deterministic contact-owner ablation. No save files, host process, or RNG mutation.
use glam::Vec2;
use lifecore::{
    AffectState, BodyIntent, ExpressionState, Genome, LocomotionMode, PoseIntent, SensorFrame,
    SurfaceId,
};
use pet_body::{ProceduralBody, VisualMindInput, VoiceVisualState};
use pet_motor::{SomaticActuationPacket, SurfaceAttachmentCommand};
use serde_json::json;
use std::io::Write;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let output = std::path::Path::new(&args[1]);
    std::fs::create_dir_all(output)?;
    let hz: u32 = args.get(2).map_or(Ok(120), |s| s.parse())?;
    let dt = 1.0 / hz as f32;
    let physical = args.get(3).is_some_and(|s| s == "physical");
    for (name, command_time) in [("late-motor", 1.5), ("early-motor", 0.0)] {
        let genome = Genome::from_seed(42);
        let mut body = ProceduralBody::generate(&genome)?;
        let mut profile = body.tuning_profile().clone();
        profile.pbf.fixed_hz = 120.0;
        profile.pbf.density_iterations = 6;
        profile.pbf.surface_tension = 2.0;
        profile.pbf.bond_compliance = 0.0012;
        profile.pbf.bond_relaxation_time = 0.18;
        profile.pbf.flight_inertia = 0.72;
        profile.pbf.flight_damping = 3.2;
        profile.pbf.return_strength = 0.05;
        profile.pbf.viscosity = 0.009;
        profile.pbf.numerical_xsph = 0.014;
        profile.pbf.spacing_scale = 0.88;
        profile.pbf.kernel_radius_scale = 1.14;
        profile.pbf.anisotropy_max = 2.01;
        profile.pbf.character_field_radius_scale = 1.17;
        profile.pbf.iso_threshold = 0.34;
        body.apply_tuning_profile(profile)?;
        body.simulation
            .set_motion_space_pixels(Vec2::new(1920.0, 1080.0));
        body.set_desktop_motion_space(Vec2::new(1920.0, 1080.0), 360.0);
        let intent = BodyIntent {
            locomotion: LocomotionMode::Hover,
            target_position: Vec2::splat(0.5),
            target_surface: None,
            desired_speed: 0.0,
            facing_direction: 1.0,
            gaze_target: None,
            pose: PoseIntent::Curious,
            expression: ExpressionState::default(),
            interaction_target: None,
        };
        let sensors = SensorFrame::default();
        for _ in 0..hz {
            body.embodied_update(
                &intent,
                &sensors,
                AffectState::default(),
                VisualMindInput::default(),
                VoiceVisualState::default(),
                dt,
            );
        }
        let root_y = 1.0 - body.liquid_contact_bounds_pixels(360.0).maximum.y / 1080.0;
        let mut writer = std::io::BufWriter::new(std::fs::File::create(
            output.join(format!("{name}-{hz}.jsonl")),
        )?);
        for tick in 0..6 * hz {
            let t = tick as f32 * dt;
            let y = root_y - (0.75 - t).max(0.0) * 0.08 - (t - 4.0).clamp(0.0, 0.7) * 0.10;
            body.simulation.feedback.world_position = Vec2::new(0.5, y);
            body.simulation.feedback.velocity = Vec2::ZERO;
            body.simulation.feedback.acceleration = Vec2::ZERO;
            let support = SurfaceAttachmentCommand {
                surface_id: SurfaceId("screen:bottom_edge".into()),
                anchor_point: Vec2::new(0.5, 1.0),
                normal: Vec2::NEG_Y,
                tangent: Vec2::X,
                target_contact_fraction: 0.32,
                normal_compliance: 0.35,
                tangent_friction: 0.7,
                adhesion: 0.0,
                load_fraction: 0.27,
                break_force: 0.7,
                release_half_life: 0.25,
            };
            body.embodiment
                .liquid
                .set_environment_support(physical.then_some(support.clone()));
            body.set_somatic_actuation(SomaticActuationPacket {
                support: (t >= command_time && t < 4.0).then_some(support),
                ..Default::default()
            });
            body.embodied_update(
                &intent,
                &sensors,
                AffectState::default(),
                VisualMindInput::default(),
                VoiceVisualState::default(),
                dt,
            );
            let d = body.embodiment.liquid.diagnostics();
            let hull = body.liquid_contact_bounds_pixels(360.0);
            writeln!(
                writer,
                "{}",
                json!({"t":t,"root_y":y,"motor":t>=command_time && t<4.0,"load":d.support_field_load,"normal_impulse":d.support_normal_impulse,"bulk_tension":d.bulk_surface_tension,"aspect":d.permanent_field_aspect,"extent":(hull.maximum-hull.minimum).to_array(),"gap_px":1080.0-y*1080.0-hull.maximum.y,"speed":d.maximum_speed,"kinetic":d.kinetic_energy,"mass":d.main_mass,"components":d.component_count,"failsafe":d.failsafe_hits,"recovery":d.recovery_count})
            )?;
            assert!(d.finite && d.failsafe_hits == 0 && d.recovery_count == 0);
        }
    }
    Ok(())
}
