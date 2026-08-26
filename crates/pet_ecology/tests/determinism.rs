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
