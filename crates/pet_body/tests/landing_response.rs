use glam::Vec2;
use lifecore::{
    BodyIntent, ExpressionState, Genome, LocomotionMode, PoseIntent, SensorFrame, SurfaceId,
};
use pet_body::ProceduralBody;
use pet_motor::{SomaticActuationPacket, SurfaceAttachmentCommand};
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

fn replay(hz: u32, speed_px: f32, surface: &str, environment_owner: bool) -> (f32, f32, f32) {
    let dt = 1.0 / hz as f32;
    let mut body = body();
    if environment_owner {
        body.embodiment.liquid.set_environment_support(None);
    }
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
    let mut y =
        1.0 - (body.main_liquid_contact_bounds_pixels(360.0).maximum.y + speed_px * 0.75) / 1080.0;
    body.simulation.feedback.world_position = Vec2::new(0.5, y);
    let mut previous_v = speed_px / 1080.0;
    let mut landed_at = None;
    let mut height_at_contact = 0.0;
    let mut response_100ms = None;
    let mut peak_delta_v = 0.0_f32;
    let mut minimum_load = 1.0_f32;
    for tick in 0..2 * hz {
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
        let support = SurfaceAttachmentCommand {
            surface_id: SurfaceId(surface.into()),
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
        // Same somatic request and root feedback; only explicit geometry owner differs.
        body.set_somatic_actuation(SomaticActuationPacket {
            support: Some(support.clone()),
            ..Default::default()
        });
        if environment_owner {
            body.embodiment
                .liquid
                .set_environment_support(Some(support));
        }
        body.embodied_update(
            &intent,
            &sensors,
            Default::default(),
            Default::default(),
            Default::default(),
            dt,
        );
        body.presentation_update(dt);
        let d = body.embodiment.liquid.diagnostics();
        assert!(d.finite && d.failsafe_hits == 0 && d.recovery_count == 0);
        assert_eq!(d.main_mass, 96.0, "hz={hz} speed={speed_px} t={t}");
        assert_eq!(d.component_count, 1);
        if let Some(landed) = landed_at {
            let phase = t - landed;
            let hull = body.main_liquid_contact_bounds_pixels(360.0);
            let height = (hull.maximum - hull.minimum).y;
            if first_contact {
                height_at_contact = height;
            }
            peak_delta_v = peak_delta_v.max(d.carrier_contact_delta_v);
            minimum_load = minimum_load.min(d.support_field_load);
            if phase >= 0.099 && response_100ms.is_none() {
                response_100ms = Some(1.0 - height / height_at_contact);
            }
        }
    }
    let response = response_100ms.unwrap();
    println!(
        "landing surface={surface} owner={environment_owner} hz={hz} speed={speed_px}: response100={response:.4}, minload={minimum_load:.4}, retained_dv={peak_delta_v:.4}"
    );
    (response, minimum_load, peak_delta_v)
}

#[test]
fn landing_preserves_slow_and_fast_momentum_and_deforms_without_a_delayed_phase() {
    for surface in ["screen:bottom_edge", "den:cushion"] {
        for hz in [30, 60, 120] {
            for speed in [24.0, 324.0] {
                let (response, load, delta_v) = replay(hz, speed, surface, true);
                assert!(response > if speed < 36.0 { 0.025 } else { 0.10 });
                assert!(
                    delta_v > 0.0,
                    "slow landings must not need a 36px/s impact event"
                );
                assert!(
                    load > 0.20,
                    "a held carrier must not manufacture a second landing"
                );
            }
        }
    }
}

#[test]
fn identical_supported_feedback_preserves_both_host_contracts() {
    let (physical, physical_load, _) = replay(60, 24.0, "screen:bottom_edge", true);
    let (legacy, legacy_load, _) = replay(60, 24.0, "screen:bottom_edge", false);
    assert!(physical_load > 0.20 && legacy_load > 0.20);
    assert!(
        legacy > 0.005,
        "legacy support must still deform actual material"
    );
    assert!(
        physical > legacy,
        "explicit stable geometry must remove additional contour-feedback lag: physical={physical} legacy={legacy}"
    );
}
