use glam::Vec2;
use lifecore::{
    BodyIntent, ExpressionState, Genome, LocomotionMode, PoseIntent, SensorFrame, SurfaceId,
};
use pet_body::{ProceduralBody, VisualMindInput, VoiceVisualState};
use pet_motor::{SomaticActuationPacket, SurfaceAttachmentCommand};

fn body() -> ProceduralBody {
    let mut body = ProceduralBody::generate(&Genome::from_seed(42)).unwrap();
    let mut profile = body.tuning_profile().clone();
    profile.pbf.fixed_hz = 120.0;
    profile.pbf.density_iterations = 6;
    profile.pbf.surface_tension = 2.0;
    profile.pbf.bond_compliance = 0.0012;
    profile.pbf.bond_relaxation_time = 0.18;
    profile.pbf.return_strength = 0.05;
    profile.pbf.flight_inertia = 0.72;
    profile.pbf.flight_damping = 3.2;
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

#[test]
fn physical_landing_load_and_release_do_not_wait_for_a_motor_phase() {
    for hz in [30, 60, 120] {
        let dt = 1.0 / hz as f32;
        let mut early = body();
        let mut late = body();
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
        for body in [&mut early, &mut late] {
            for _ in 0..hz {
                body.embodied_update(
                    &intent,
                    &sensors,
                    Default::default(),
                    VisualMindInput::default(),
                    VoiceVisualState::default(),
                    dt,
                );
            }
        }
        let before = early.liquid_contact_bounds_pixels(360.0);
        let root_y = 1.0 - before.maximum.y / 1080.0;
        let mut settled_height = 0.0;
        let mut peak_contact_before_motor = 0.0_f32;
        for tick in 0..6 * hz {
            let t = tick as f32 * dt;
            let y = root_y - (0.75 - t).max(0.0) * 0.08 - (t - 4.0).clamp(0.0, 0.7) * 0.10;
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
            for (body, command_time) in [(&mut early, 0.0), (&mut late, 1.5)] {
                body.simulation.feedback.world_position = Vec2::new(0.5, y);
                body.simulation.feedback.velocity = Vec2::ZERO;
                body.simulation.feedback.acceleration = Vec2::ZERO;
                body.embodiment
                    .liquid
                    .set_environment_support(Some(support.clone()));
                body.set_somatic_actuation(SomaticActuationPacket {
                    support: (t >= command_time && t < 4.0).then_some(support.clone()),
                    ..Default::default()
                });
                body.embodied_update(
                    &intent,
                    &sensors,
                    Default::default(),
                    VisualMindInput::default(),
                    VoiceVisualState::default(),
                    dt,
                );
            }
            let a = early.embodiment.liquid.render_state();
            let b = late.embodiment.liquid.render_state();
            // Causal intervention: changing only the delayed behavior's
            // attachment cannot alter any physical or presented particle.
            assert_eq!(a.particles, b.particles, "hz={hz} t={t}");
            assert!(a.diagnostics.finite);
            assert_eq!(a.diagnostics.failsafe_hits, 0);
            assert_eq!(a.diagnostics.recovery_count, 0);
            assert_eq!(
                a.diagnostics.main_mass, 96.0,
                "hz={hz} t={t} {:?}",
                a.diagnostics
            );
            assert_eq!(a.diagnostics.component_count, 1, "hz={hz} t={t}");
            if !(0.50..=4.5).contains(&t) {
                assert_eq!(
                    a.diagnostics.support_field_load, 0.0,
                    "phantom contact hz={hz} t={t}"
                );
            }
            if (0.85..1.45).contains(&t) {
                peak_contact_before_motor =
                    peak_contact_before_motor.max(b.diagnostics.support_field_load);
            }
            if (3.8..4.0).contains(&t) {
                settled_height = early.liquid_contact_bounds_pixels(360.0).maximum.y
                    - early.liquid_contact_bounds_pixels(360.0).minimum.y;
            }
        }
        assert!(
            peak_contact_before_motor > 0.2,
            "no physical load before motor hz={hz}"
        );
        assert!(
            settled_height < (before.maximum.y - before.minimum.y) * 0.8,
            "no actual liquid deformation hz={hz}: {settled_height}"
        );
        assert!(
            early.embodiment.liquid.diagnostics().permanent_field_aspect < 1.01,
            "support shape failed to release hz={hz}"
        );
    }
}
