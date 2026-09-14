#[path = "../src/surface_care_runtime.rs"]
#[allow(dead_code)]
mod surface_care_runtime;
use glam::Vec2;
use surface_care_runtime::*;

fn input() -> SurfaceCareInput {
    SurfaceCareInput {
        position: Vec2::new(0.3, 0.5),
        velocity: Vec2::ZERO,
        half_extent: Vec2::splat(0.05),
        desktop_min: Vec2::ZERO,
        desktop_max: Vec2::ONE,
        physical_effort: 0.0,
        sleeping: false,
        quiet: false,
        dragged: false,
        gripping: false,
        danger: false,
        purposeful: false,
        contact_normal: None,
        contact_load: 0.0,
        surface_friction: Some(0.3),
        surface_velocity: Vec2::ZERO,
    }
}

#[test]
fn stationary_time_cannot_invent_strain_or_surface_actions() {
    let mut state = SurfaceCareRuntime::default();
    for _ in 0..12000 {
        let out = state.tick(input(), 0.05);
        assert!(out.target.is_none() && out.events.is_empty() && out.pressure == 0.0);
    }
    assert_eq!(state.strain, 0.0);
}

#[test]
fn effort_causes_approach_but_air_never_admits_pressure_then_contact_runs_one_stroke() {
    for dt in [1.0 / 30.0, 1.0 / 60.0, 1.0 / 120.0] {
        let mut state = SurfaceCareRuntime::default();
        let mut context = input();
        context.velocity = Vec2::new(0.25, -0.12);
        context.physical_effort = 0.9;
        for _ in 0..(8.0 / dt) as usize {
            state.tick(context, dt);
        }
        assert_eq!(state.phase, SurfaceCarePhase::Approach);
        let out = state.tick(context, dt);
        assert!(out.target.unwrap().x < context.position.x);
        assert_eq!(out.pressure, 0.0);
        context.position = Vec2::new(0.05, 0.5);
        context.contact_normal = Some(Vec2::X);
        context.contact_load = 0.2;
        context.velocity = Vec2::new(0.0, 0.03);
        let mut ids = Vec::new();
        let mut ys = Vec::new();
        for _ in 0..(6.8 / dt) as usize {
            let out = state.tick(context, dt);
            ids.extend(out.events.iter().map(|e| e.id));
            assert!((0.0..=0.16001).contains(&out.pressure));
            if let Some(target) = out.target {
                ys.push(target.y);
            }
        }
        assert!(
            ids.contains(&92) && ids.contains(&93) && ids.contains(&98),
            "{ids:?}"
        );
        assert_eq!(ids.iter().filter(|&&id| id == 92).count(), 1);
        assert!(ys.iter().any(|y| *y > 0.52) && ys.iter().any(|y| *y < 0.48));
        assert_eq!(state.phase, SurfaceCarePhase::Idle);
        assert!(state.tick(context, dt).target.is_none());
    }
}

#[test]
fn danger_drag_grip_sleep_and_quiet_abort_without_force_or_delayed_action() {
    for gate in 0..5 {
        let mut state = SurfaceCareRuntime::default();
        let mut context = input();
        state.strain = 0.8;
        state.tick(context, 0.05);
        match gate {
            0 => context.danger = true,
            1 => context.dragged = true,
            2 => context.gripping = true,
            3 => context.sleeping = true,
            _ => context.quiet = true,
        }
        let out = state.tick(context, 0.05);
        assert_eq!(state.phase, SurfaceCarePhase::Idle);
        assert!(out.target.is_none() && out.pressure == 0.0 && out.events.is_empty());
        assert!(state.tick(input(), 0.05).target.is_none());
    }
}

#[test]
fn spatial_hooks_require_actual_topology_route_support_changes() {
    let mut state = SpatialEvidenceEvents::default();
    let mut evidence = SpatialEvidence {
        monitor_count: 2,
        route_length: Some(1.0),
        candidate_support: Some((10, 1.3)),
        supported_id: Some(10),
        stable_support: true,
        available_support_ids: vec![10],
        ..Default::default()
    };
    assert!(state.observe(&evidence, 0.05).iter().any(|e| e.id == 182));
    let mut learned = 0;
    for _ in 0..170 {
        learned += state
            .observe(&evidence, 0.05)
            .iter()
            .filter(|e| e.id == 193)
            .count();
    }
    assert_eq!(learned, 1);
    evidence.monitor_count = 1;
    evidence.valid_recovery_position = Some(Vec2::splat(0.5));
    evidence.route_length = Some(0.5);
    evidence.route_blocked = true;
    evidence.alternate_route_target = Some(Vec2::new(0.4, 0.7));
    evidence.available_support_ids.clear();
    evidence.candidate_support = Some((20, 0.5));
    evidence.corner_contact_count = 2;
    let ids: Vec<_> = state
        .observe(&evidence, 0.05)
        .into_iter()
        .map(|e| e.id)
        .collect();
    for id in [183, 185, 186, 187, 188, 199] {
        assert!(ids.contains(&id), "{id}: {ids:?}");
    }
}

#[test]
fn pressure_does_not_restart_at_rub_or_jump_up_when_unloading_overload() {
    let mut state = SurfaceCareRuntime::default();
    state.strain = 0.8;
    let mut context = input();
    state.tick(context, 0.05);
    context.contact_normal = Some(Vec2::X);
    context.velocity.y = 0.03;
    let mut last = 0.0_f32;
    for _ in 0..45 {
        let out = state.tick(context, 0.05);
        assert!((out.pressure - last).abs() <= 0.012501);
        if state.phase == SurfaceCarePhase::Rub {
            assert!(out.pressure > 0.14);
        }
        last = out.pressure;
    }
    context.contact_load = 0.9;
    for _ in 0..55 {
        let out = state.tick(context, 0.05);
        assert!(
            out.pressure <= last + 0.00001,
            "overload/unload increased load"
        );
        assert!((out.pressure - last).abs() <= 0.012501);
        last = out.pressure;
    }
    context.contact_normal = None;
    assert_eq!(state.tick(context, 0.05).pressure, 0.0);
}

#[test]
fn upper_and_lower_strain_produce_opposite_first_physical_strokes() {
    let mut positions = Vec::new();
    for sign in [-1.0, 1.0] {
        let mut state = SurfaceCareRuntime::default();
        let mut context = input();
        context.velocity = Vec2::new(0.25, sign * 0.12);
        context.physical_effort = 0.9;
        for _ in 0..150 {
            state.tick(context, 0.05);
        }
        context.contact_normal = Some(Vec2::X);
        context.velocity = Vec2::new(0.0, sign * 0.03);
        let mut first = None;
        for _ in 0..32 {
            let out = state.tick(context, 0.05);
            if state.phase == SurfaceCarePhase::Rub && state.phase_age >= 0.5 {
                first = out.target.map(|target| target.y);
                break;
            }
        }
        positions.push(first.unwrap());
    }
    assert!(positions[0] < 0.49 && positions[1] > 0.51, "{positions:?}");
}

#[test]
fn general_boundary_hit_emits_once_without_starting_care_or_changing_motion() {
    let mut state = SurfaceCareRuntime::default();
    let mut context = input();
    context.position.x = 0.05;
    context.velocity.x = -0.6;
    context.contact_normal = Some(Vec2::X);
    context.contact_load = 0.4;
    context.purposeful = true;
    for tick in 0..20 {
        let out = state.tick(context, 0.05);
        assert_eq!(
            out.events.iter().filter(|e| e.id == 180).count(),
            usize::from(tick == 0)
        );
        assert!(out.target.is_none() && out.pressure == 0.0);
        assert_eq!(state.phase, SurfaceCarePhase::Idle);
    }
}

#[test]
fn bout_variation_is_deterministic_and_frozen_until_next_causal_bout() {
    fn run() -> Vec<(f32, f32, f32)> {
        let mut state = SurfaceCareRuntime::default();
        let mut result = Vec::new();
        for _ in 0..3 {
            state.strain = 0.8;
            let mut context = input();
            state.tick(context, 0.05);
            let sampled = (
                state.stroke_duration,
                state.stroke_amplitude,
                state.stroke_shape,
            );
            result.push(sampled);
            for _ in 0..20 {
                state.tick(context, 0.05);
                assert_eq!(
                    sampled,
                    (
                        state.stroke_duration,
                        state.stroke_amplitude,
                        state.stroke_shape
                    )
                );
            }
            context.danger = true;
            state.tick(context, 0.05);
            // Let the cooldown expire without motion creating another cause.
            state.strain = 0.0;
            for _ in 0..1900 {
                state.tick(input(), 0.05);
            }
        }
        result
    }
    let a = run();
    assert_eq!(a, run());
    assert!(a.windows(2).all(|p| p[0] != p[1]));
}
