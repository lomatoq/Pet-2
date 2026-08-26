use glam::Vec2;
use pet_ecology::{
    ActionSignature, EcologyState, ObjectLifecycle, ObjectPhysicsConfig, signature_distance,
    step_object,
};

#[test]
fn fixed_seed_object_simulation_is_bit_deterministic() {
    let mut first = EcologyState::new(8_122);
    let mut second = first.clone();
    for state in [&mut first, &mut second] {
        state.objects[0].lifecycle = ObjectLifecycle::Free;
        state.objects[0].velocity = Vec2::new(1.8, -1.1);
    }
    for _ in 0..72_000 {
        step_object(
            &mut first.objects[0],
            ObjectPhysicsConfig::default(),
            1.0 / 120.0,
        );
        step_object(
            &mut second.objects[0],
            ObjectPhysicsConfig::default(),
            1.0 / 120.0,
        );
    }
    assert_eq!(first, second);
    first.validate().unwrap();
}

#[test]
fn arc_length_resampling_ignores_event_density() {
    let sparse = [
        Vec2::new(0.0, 0.0),
        Vec2::new(0.5, 1.0),
        Vec2::new(1.0, 0.0),
    ];
    let dense = [
        Vec2::new(0.0, 0.0),
        Vec2::new(0.25, 0.5),
        Vec2::new(0.5, 1.0),
        Vec2::new(0.75, 0.5),
        Vec2::new(1.0, 0.0),
    ];
    let sparse = ActionSignature::from_trace(&sparse, 1.0).unwrap();
    let dense = ActionSignature::from_trace(&dense, 1.0).unwrap();
    assert!(signature_distance(&sparse, &dense) < 0.015);
    assert_eq!(
        signature_distance(&sparse, &dense),
        signature_distance(&dense, &sparse)
    );
}

#[test]
fn object_simulation_is_identical_at_30_60_and_144_hz_presentation() {
    fn replay(presentation_hz: u32) -> EcologyState {
        let mut state = EcologyState::new(8_123);
        state.objects[0].lifecycle = ObjectLifecycle::Free;
        state.objects[0].velocity = Vec2::new(1.45, -0.83);
        let mut presentation_accumulator = 0.0_f64;
        let presentation_dt = 1.0 / f64::from(presentation_hz);
        for _ in 0..14_400 {
            step_object(
                &mut state.objects[0],
                ObjectPhysicsConfig::default(),
                1.0 / 120.0,
            );
            presentation_accumulator += 1.0 / 120.0;
            if presentation_accumulator >= presentation_dt {
                let _snapshot = state.objects[0].clone();
                presentation_accumulator %= presentation_dt;
            }
        }
        state
    }

    let at_30 = replay(30);
    assert_eq!(at_30, replay(60));
    assert_eq!(at_30, replay(144));
}
