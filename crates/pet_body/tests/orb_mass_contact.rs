use glam::Vec2;
use lifecore::{Genome, LifeCore, SensorFrame};
use pet_body::{ProceduralBody, VisualMindInput, VoiceVisualState};
use pet_ecology::{ContactSource, EmbodiedEnvironmentFrame, ExternalContact};

fn profile(hz: usize, force: f32, impact: bool, release: bool) -> (Vec2, f32, f32, f32) {
    let genome = Genome::from_seed(42);
    let mut life = LifeCore::new(genome.clone(), 42);
    let mut intent = life
        .tick(&SensorFrame::default(), &Default::default(), 0.05)
        .body_intent;
    intent.desired_speed = 0.0;
    let mut body = ProceduralBody::generate(&genome).unwrap();
    body.simulation.feedback.world_position = Vec2::splat(0.5);
    body.set_desktop_motion_space(Vec2::new(1920.0, 1080.0), 360.0);
    let scale = body.embodiment.world_to_body_scale();
    let mut environment = EmbodiedEnvironmentFrame::default();
    environment.push_contact(ExternalContact {
        source: ContactSource::Orb,
        point_world: Vec2::splat(0.5) + Vec2::new(0.23, 0.0) / scale,
        normal_world: Vec2::Y,
        body_force_world: Vec2::Y * force,
        relative_velocity_px: if impact { Vec2::Y * 240.0 } else { Vec2::ZERO },
        ..Default::default()
    });
    body.set_embodied_environment(&environment);
    for _ in 0..hz * 3 {
        body.embodied_update(
            &intent,
            &SensorFrame::default(),
            Default::default(),
            VisualMindInput::default(),
            VoiceVisualState::default(),
            1.0 / hz as f32,
        );
        let d = body.embodiment.liquid.diagnostics();
        assert!(d.finite);
        assert_eq!(d.failsafe_hits, 0);
        assert_eq!(d.recovery_count, 0);
        assert!((d.main_mass + d.detached_mass - 96.0).abs() < 0.01);
        assert_eq!(body.simulation.feedback.world_position, Vec2::splat(0.5));
    }
    if release {
        body.set_embodied_environment(&EmbodiedEnvironmentFrame::default());
        for _ in 0..hz * 4 {
            body.embodied_update(
                &intent,
                &SensorFrame::default(),
                Default::default(),
                VisualMindInput::default(),
                VoiceVisualState::default(),
                1.0 / hz as f32,
            );
            let d = body.embodiment.liquid.diagnostics();
            assert!(d.finite);
            assert_eq!(d.recovery_count, 0);
            assert_eq!(d.failsafe_hits, 0);
        }
    }
    let render = body.embodiment.liquid.render_state();
    let mut centroid = Vec2::ZERO;
    let mut sides = [(0.0, 0.0); 2];
    for p in &render.particles[..render.particle_count] {
        centroid += p.position;
        let side = usize::from(p.material_coordinate.x > 0.0);
        sides[side].0 += p.position.y;
        sides[side].1 += 1.0;
    }
    centroid /= render.particle_count as f32;
    let d = body.embodiment.liquid.diagnostics();
    (
        centroid,
        d.object_load,
        d.object_contact_impulse,
        sides[1].0 / sides[1].1 - sides[0].0 / sides[0].1,
    )
}

#[test]
fn real_orb_weight_deforms_liquid_without_root_teleport_at_three_frame_rates() {
    for hz in [30, 60, 120] {
        let neutral = profile(hz, 0.0, false, false);
        let loaded = profile(hz, 0.5184, false, false);
        let impact = profile(hz, 0.5184, true, false);
        eprintln!("hz{hz}: neutral={neutral:?}, loaded={loaded:?}, impact={impact:?}");
        assert_eq!(neutral.1, 0.0);
        assert_eq!(neutral.2, 0.0);
        assert!(
            loaded.0.y < neutral.0.y - 0.001,
            "weight must sag actual particles"
        );
        assert!(
            loaded.3 < neutral.3 - 0.001,
            "contact side must sag relative to distant material"
        );
        assert!((loaded.1 - 0.1728).abs() < 0.02);
        assert_eq!(loaded.2, 0.0, "static weight is not an impact");
        assert!(impact.2 > 0.005);
    }
    let recovered = profile(60, 0.5184, false, true);
    assert!(
        recovered.0.length() < 0.04,
        "released body should return: {recovered:?}"
    );
    assert_eq!(recovered.1, 0.0);
}
