use glam::Vec2;
use lifecore::{
    ActionId, BehaviorGoalFrame, FastPhenotypeActuation, Genome, LifeCore, SensorFrame, SurfaceId,
};
use pet_body::{ProceduralBody, VisualMindInput, VoiceVisualState};
use pet_motor::{
    BehaviorContextFrame, BehaviorPerformanceRuntime, SomaticActuationBus, SurfaceCandidate,
};

#[test]
fn sustained_supported_load_changes_real_liquid_profile() {
    check_supported_profile(0.0, 60, false);
}

#[test]
fn mild_startle_guard_preserves_loaded_spread_and_release() {
    check_supported_profile(0.50, 60, false);
}

#[test]
fn production_120hz_supported_profile_and_release() {
    check_supported_profile(0.0, 120, true);
}

#[test]
fn production_tuning_60hz_supported_profile_and_release() {
    check_supported_profile(0.0, 60, true);
}

fn check_supported_profile(startle: f32, hz: u64, production_tuning: bool) {
    let dt = 1.0 / hz as f32;
    let genome = Genome::from_seed(42);
    let mut life = LifeCore::new(genome.clone(), 42);
    let mut intent = life
        .tick(&SensorFrame::default(), &Default::default(), 0.05)
        .body_intent;
    intent.desired_speed = 0.0;
    intent.target_position = Vec2::new(0.5, 0.9);
    let goal = BehaviorGoalFrame {
        action: ActionId::IdleHover,
        body_intent: intent.clone(),
        affect: Default::default(),
        drives: life.state.drives,
        felt: lifecore::FeltStateV1 {
            startle,
            ..Default::default()
        },
        derived: Default::default(),
        attachment: 0.4,
        recent_outcome: None,
    };
    let mut context = BehaviorContextFrame::default();
    context.body.motion.world_position = intent.target_position;
    context.den_anchor = Some(intent.target_position);
    context.screen_edge_supported = true;
    context.screen_edge_support_stable_seconds = 1.0;
    context.screen_edge_gap_px = 0.0;
    context.body_bottom_extent = 0.05;
    context.surfaces.push(SurfaceCandidate {
        surface_id: SurfaceId("screen:bottom_edge".into()),
        minimum: Vec2::new(0.0, 0.999),
        maximum: Vec2::ONE,
        velocity: Vec2::ZERO,
        familiarity: 1.0,
        recent_failed_landings: 0,
    });
    let mut motor = BehaviorPerformanceRuntime::new(42);
    let mut loaded = ProceduralBody::generate(&genome).unwrap();
    let mut neutral = ProceduralBody::generate(&genome).unwrap();
    for body in [&mut loaded, &mut neutral] {
        if production_tuning {
            // Explicit fixture of the relevant native v8 settings; never read or
            // mutate a user's settings from a regression test.
            let mut profile = body.tuning_profile().clone();
            profile.pbf.fixed_hz = 120.0;
            profile.pbf.density_iterations = 6;
            profile.pbf.surface_tension = 2.5;
            profile.pbf.return_strength = 0.05;
            profile.pbf.viscosity = 0.015;
            profile.pbf.numerical_xsph = 0.014;
            profile.pbf.spacing_scale = 0.88;
            profile.pbf.kernel_radius_scale = 1.14;
            profile.pbf.anisotropy_max = 2.01;
            profile.pbf.character_field_radius_scale = 1.17;
            profile.pbf.iso_threshold = 0.34;
            body.apply_tuning_profile(profile).unwrap();
        }
        body.simulation.feedback.world_position = intent.target_position;
        body.simulation
            .set_motion_space_pixels(Vec2::new(1920.0, 1080.0));
        body.set_desktop_motion_space(Vec2::new(1920.0, 1080.0), 360.0);
    }
    let sensors = SensorFrame::default();
    let mut profiles = [Vec2::ZERO; 2];
    let mut footprint_fractions = [0.0_f32; 2];
    let mut supported_frames = 0_u64;
    let mut late_supported_frames = 0_u64;
    let mut aspect_sum = 0.0_f32;
    let mut aspect_max = 0.0_f32;
    let mut aspect_end = 0.0_f32;
    let mut previous_loaded_extent: Option<Vec2> = None;
    for frame in 0..6 * hz {
        // Model the native host's physical floor projection, using the actual
        // unpadded hull rather than fabricating contact at an airborne center.
        let floor_root = Vec2::new(
            0.5,
            1.0 - loaded.liquid_contact_bounds_pixels(360.0).maximum.y / 1080.0,
        );
        loaded.simulation.feedback.world_position = floor_root;
        loaded.simulation.feedback.velocity = Vec2::ZERO;
        context.body.motion.world_position = floor_root;
        context.frame_id = frame;
        context.timestamp_seconds = frame as f64 / hz as f64;
        let packet = motor.tick(&goal, &context, dt);
        let mut loaded_intent = intent.clone();
        SomaticActuationBus::apply_to_intent(&packet, &context, &mut loaded_intent);
        let mut phenotype = FastPhenotypeActuation::default();
        SomaticActuationBus::compose(&mut phenotype, &packet);
        loaded.set_fast_phenotype_actuation(phenotype);
        loaded.set_somatic_actuation(packet);
        for (index, body) in [&mut loaded, &mut neutral].into_iter().enumerate() {
            let current_intent = if index == 0 { &loaded_intent } else { &intent };
            body.fixed_update(&genome, current_intent, &sensors, dt);
            if index == 0 {
                body.simulation.feedback.world_position = floor_root;
                body.simulation.feedback.velocity = Vec2::ZERO;
                body.simulation.feedback.acceleration = Vec2::ZERO;
            }
            body.embodied_update(
                current_intent,
                &sensors,
                goal.affect,
                VisualMindInput::default(),
                VoiceVisualState::default(),
                dt,
            );
            let d = body.embodiment.liquid.diagnostics();
            if index == 0 {
                supported_frames += u64::from(d.support_field_load > 0.0);
                if frame >= 5 * hz {
                    late_supported_frames += u64::from(d.support_field_load > 0.0);
                }
                aspect_sum += d.permanent_field_aspect;
                aspect_max = aspect_max.max(d.permanent_field_aspect);
                aspect_end = d.permanent_field_aspect;
            }
            assert!(d.finite);
            assert_eq!(d.failsafe_hits, 0);
            assert_eq!(d.recovery_count, 0);
            if index == 1 {
                assert_eq!(d.support_field_load, 0.0);
                assert!(
                    d.permanent_field_aspect <= 1.3401,
                    "airborne control acquired a loaded posture: {}",
                    d.permanent_field_aspect
                );
            }
            let hull = body.liquid_contact_bounds_pixels(360.0);
            if index == 0 {
                let extent = hull.maximum - hull.minimum;
                if let Some(previous) = previous_loaded_extent {
                    assert!(
                        (extent - previous).abs().max_element() < 3.0,
                        "loaded shape changed discontinuously: {previous:?} -> {extent:?}"
                    );
                }
                previous_loaded_extent = Some(extent);
            }
            if frame >= 5 * hz {
                profiles[index] += (hull.maximum - hull.minimum) / hz as f32;
                if frame % hz == 0 {
                    footprint_fractions[index] = near_floor_footprint_fraction(body, &genome, 3.0);
                }
            }
        }
    }
    eprintln!(
        "hz={hz} production={production_tuning} support frames={supported_frames}/{} last-second={late_supported_frames}/{hz} aspect avg={} max={aspect_max} end={aspect_end}",
        6 * hz,
        aspect_sum / (6 * hz) as f32
    );
    eprintln!(
        "hz={hz} production={production_tuning} near-floor width fraction (3px) loaded={} neutral={}",
        footprint_fractions[0], footprint_fractions[1]
    );
    eprintln!(
        "hz={hz} production={production_tuning} liquid profile loaded={:?} neutral={:?}; masses {} {}",
        profiles[0],
        profiles[1],
        loaded.embodiment.liquid.diagnostics().main_mass,
        neutral.embodiment.liquid.diagnostics().main_mass
    );
    assert!(profiles[0].is_finite() && profiles[1].is_finite());
    if production_tuning {
        assert!(
            footprint_fractions[0] >= 0.45,
            "loaded contact remains a rounded point, not a broad foot: {footprint_fractions:?}"
        );
        // The requested shape is a broad supporting base, not a globally wider
        // ellipse. Absolute contact growth and the whole-width guard together
        // prevent satisfying the normalized criterion by shrinking the body.
        let loaded_foot = profiles[0].x * footprint_fractions[0];
        let neutral_foot = profiles[1].x * footprint_fractions[1];
        assert!(
            loaded_foot >= neutral_foot * 1.5,
            "support must widen the actual contact patch: {loaded_foot} vs {neutral_foot}"
        );
    }
    assert!(
        profiles[0].x > profiles[1].x * if production_tuning { 0.95 } else { 1.03 },
        "supported body must visibly spread laterally: {profiles:?}"
    );
    assert!(
        profiles[0].y < profiles[1].y * if production_tuning { 0.90 } else { 0.97 },
        "supported body must have a lower profile: {profiles:?}"
    );
    // Conserved particle mass is an actual solver measure; hull area is not
    // liquid volume and is deliberately not used as a volume-conservation claim.
    assert_eq!(
        loaded.embodiment.liquid.diagnostics().main_mass,
        neutral.embodiment.liquid.diagnostics().main_mass
    );

    // Release both the load and contact command, then allow the same physical
    // well to relax. No reset/rebuild or render squash is used for recovery.
    loaded.set_somatic_actuation(Default::default());
    loaded.set_fast_phenotype_actuation(Default::default());
    let mut released_profiles = [Vec2::ZERO; 2];
    for frame in 0..6 * hz {
        for (index, body) in [&mut loaded, &mut neutral].into_iter().enumerate() {
            body.fixed_update(&genome, &intent, &sensors, dt);
            body.embodied_update(
                &intent,
                &sensors,
                goal.affect,
                VisualMindInput::default(),
                VoiceVisualState::default(),
                dt,
            );
            let hull = body.liquid_contact_bounds_pixels(360.0);
            let extent = hull.maximum - hull.minimum;
            if index == 0 {
                assert!(
                    (extent - previous_loaded_extent.unwrap())
                        .abs()
                        .max_element()
                        < 3.0
                );
                previous_loaded_extent = Some(extent);
            }
            if frame >= 5 * hz {
                released_profiles[index] += extent / hz as f32;
            }
            let d = body.embodiment.liquid.diagnostics();
            assert!(d.finite);
            assert_eq!(d.failsafe_hits, 0);
            assert_eq!(d.recovery_count, 0);
            assert_eq!(d.main_mass, 96.0);
        }
    }
    eprintln!(
        "released={:?} neutral={:?}",
        released_profiles[0], released_profiles[1]
    );
    let relative_error = (released_profiles[0] / released_profiles[1] - Vec2::ONE).abs();
    assert!(
        relative_error.max_element() < 0.05,
        "release did not recover: {released_profiles:?}"
    );
}

// Independent raw shader-density oracle: contiguous occupied width three pixels
// above the lowest iso contour, divided by whole silhouette width. Unlike a
// bounding box, this detects an oval balancing on a tiny lower contact patch.
fn near_floor_footprint_fraction(body: &ProceduralBody, genome: &Genome, depth_px: f32) -> f32 {
    let params = body.render_parameters(genome, 0.0);
    let particles = &params.liquid.particles[..params.liquid.particle_count];
    let (minimum, maximum) =
        pet_body::contact_surface_bounds(particles, params.liquid_iso_threshold, Vec2::ZERO)
            .unwrap();
    let hull = body.liquid_contact_bounds_pixels(360.0);
    let units_per_pixel = (maximum.y - minimum.y) / (hull.maximum.y - hull.minimum.y);
    let y = minimum.y + depth_px * units_per_pixel;
    let mut longest = 0;
    let mut run = 0;
    for sample in 0..512 {
        let x = minimum.x + (maximum.x - minimum.x) * (sample as f32 + 0.5) / 512.0;
        let point = Vec2::new(x, y);
        let density: f32 = particles
            .iter()
            .map(|p| {
                let axis = if p.axis_major.length_squared() < 0.25 {
                    Vec2::X
                } else {
                    p.axis_major.normalize()
                };
                let delta = point - p.position;
                let q = Vec2::new(
                    delta.dot(axis) / p.major_radius.clamp(0.002, 0.40),
                    delta.dot(axis.perp()) / p.minor_radius.clamp(0.002, 0.40),
                );
                let w = (1.0 - q.length_squared()).max(0.0);
                p.density.clamp(0.0, 2.0) * w * w * w
            })
            .sum();
        if density >= params.liquid_iso_threshold {
            run += 1;
            longest = longest.max(run);
        } else {
            run = 0;
        }
    }
    longest as f32 / 512.0
}
