//! Physical carrier-stop replay. Fixed seed, no live files or host windows.
use glam::Vec2;
use lifecore::{
    BodyIntent, ExpressionState, Genome, LocomotionMode, PoseIntent, SensorFrame, SurfaceId,
};
use pet_body::ProceduralBody;
use pet_motor::SurfaceAttachmentCommand;
use serde_json::json;
use std::io::Write;

fn body() -> ProceduralBody {
    let genome = Genome::from_seed(42);
    let mut body = ProceduralBody::generate(&genome).unwrap();
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
    body.apply_tuning_profile(profile).unwrap();
    body.simulation
        .set_motion_space_pixels(Vec2::new(1920.0, 1080.0));
    body.set_desktop_motion_space(Vec2::new(1920.0, 1080.0), 360.0);
    body
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let output = std::path::Path::new(&args[1]);
    std::fs::create_dir_all(output)?;
    std::fs::write(
        output.join("profile.json"),
        serde_json::to_vec_pretty(body().tuning_profile())?,
    )?;
    std::fs::write(
        output.join("genome.json"),
        serde_json::to_vec_pretty(&Genome::from_seed(42))?,
    )?;
    let hz: u32 = args.get(2).map_or(Ok(120), |s| s.parse())?;
    let dt = 1.0 / hz as f32;
    for speed_px in [24.0_f32, 86.4, 324.0] {
        let mut body = body();
        body.embodiment.liquid.set_environment_support(None);
        let intent = BodyIntent {
            locomotion: LocomotionMode::Hover,
            target_position: Vec2::splat(0.5),
            target_surface: None,
            desired_speed: 0.0,
            facing_direction: 1.0,
            gaze_target: None,
            pose: PoseIntent::Neutral,
            expression: ExpressionState::default(),
            interaction_target: None,
        };
        let sensors = SensorFrame::default();
        for _ in 0..hz {
            body.embodied_update(
                &intent,
                &sensors,
                Default::default(),
                Default::default(),
                Default::default(),
                dt,
            );
        }
        let mut y = 1.0
            - (body.main_liquid_contact_bounds_pixels(360.0).maximum.y + speed_px * 0.75) / 1080.0;
        body.simulation.feedback.world_position = Vec2::new(0.5, y);
        let mut previous_v = speed_px / 1080.0;
        let mut landed_at = None;
        let mut writer = std::io::BufWriter::new(std::fs::File::create(
            output.join(format!("landing-{speed_px:.0}-{hz}.jsonl")),
        )?);
        for tick in 0..4 * hz {
            let t = tick as f32 * dt;
            let hull = body.main_liquid_contact_bounds_pixels(360.0);
            let proposed = y + if landed_at.is_none() {
                speed_px / 1080.0 * dt
            } else {
                0.0
            };
            let first_contact = landed_at.is_none() && proposed * 1080.0 + hull.maximum.y >= 1080.0;
            if first_contact {
                y = 1.0 - hull.maximum.y / 1080.0;
                landed_at = Some(t);
                // Match production screen-domain's existing impact bridge.
                if speed_px > 36.0 {
                    body.embodiment
                        .liquid
                        .apply_wall_impact(Vec2::NEG_Y, (speed_px / 900.0).clamp(0.0, 1.0));
                }
            } else {
                y = proposed;
            }
            let v = if landed_at.is_some() {
                0.0
            } else {
                speed_px / 1080.0
            };
            body.simulation.feedback.world_position = Vec2::new(0.5, y);
            body.simulation.feedback.velocity = Vec2::new(0.0, v);
            body.simulation.feedback.acceleration = Vec2::new(0.0, (v - previous_v) / dt);
            previous_v = v;
            body.embodiment
                .liquid
                .set_environment_support(Some(SurfaceAttachmentCommand {
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
                }));
            body.embodied_update(
                &intent,
                &sensors,
                Default::default(),
                Default::default(),
                Default::default(),
                dt,
            );
            body.presentation_update(dt);
            let render = body.embodiment.liquid.render_state();
            let d = render.diagnostics;
            let hull = body.main_liquid_contact_bounds_pixels(360.0);
            let particles: Vec<_> = render.particles[..render.particle_count].iter().map(|p| json!({
                "position":p.position.to_array(),"material_coordinate":p.material_coordinate.to_array(),
                "axis_major":p.axis_major.to_array(),"major_radius":p.major_radius,"minor_radius":p.minor_radius,
                "velocity":p.velocity.to_array(),"density":p.density,"optical_thickness":p.optical_thickness,
                "emission":p.emission,"pigment":p.pigment,"face_weight":p.face_weight,
                "component_id":p.component_id,"main_component":p.main_component,
            })).collect();
            let phase = landed_at.map(|landed| t - landed);
            writeln!(
                writer,
                "{}",
                json!({"t":t,"phase":phase,"root_y":y,"velocity_px":v*1080.0,
                    "extent":(hull.maximum-hull.minimum).to_array(),"load":d.support_field_load,
                    "impulse":d.support_normal_impulse,"carrier_delta_v":d.carrier_contact_delta_v,"aspect":d.permanent_field_aspect,
                    "speed":d.maximum_speed,"mass":d.main_mass,"components":d.component_count,
                    "failsafe":d.failsafe_hits,"recovery":d.recovery_count,
                    "particles": if tick % (hz/60).max(1)==0 {Some(particles)} else {None},
                })
            )?;
            if let Some(phase) = phase {
                for mark in [0.0_f32, 0.05, 0.10, 0.20, 0.50, 1.0, 2.0] {
                    if (phase - mark).abs() < dt * 0.49 {
                        std::fs::write(
                            output.join(format!(
                                "landing-{speed_px:.0}-{hz}-{mark:.2}.snapshot.json"
                            )),
                            serde_json::to_vec(&body.body_material_snapshot())?,
                        )?;
                    }
                }
            }
            assert!(d.finite && d.failsafe_hits == 0 && d.recovery_count == 0);
        }
    }
    Ok(())
}
