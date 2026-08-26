use glam::Vec2;
use pet_ecology::{EcologyState, MorselProfile, ObjectKind, ObjectLifecycle};

#[test]
fn ecology_json_roundtrip_preserves_den_slots_and_has_no_active_episode() {
    let mut state = EcologyState::new(55);
    let orb_id = state.objects[0].id;
    state.objects[0].lifecycle = ObjectLifecycle::StoredInDen;
    state.den.slots[0] = Some(orb_id);
    state.taste.hue_bins[3] = 0.42;
    state.taste.warmth_preference = -0.18;
    state.taste.confidence = 0.37;
    let json = serde_json::to_string_pretty(&state).unwrap();
    assert!(!json.contains("active_episode"));
    assert!(!json.contains("phase_elapsed_seconds"));
    let restored: EcologyState = serde_json::from_str(&json).unwrap();
    restored.validate().unwrap();
    assert_eq!(restored, state);
}

#[test]
fn restore_rejects_corrupt_skill_library() {
    let mut state = EcologyState::new(55);
    state.skills.next_skill_id = 0;
    let encoded = serde_json::to_vec(&state).unwrap();
    let decoded: EcologyState = serde_json::from_slice(&encoded).unwrap();
    assert!(EcologyState::restore(decoded).is_err());
}

#[test]
fn restart_repairs_every_transient_object_lifecycle_without_losing_identity() {
    for lifecycle in [
        ObjectLifecycle::Free,
        ObjectLifecycle::Sleeping,
        ObjectLifecycle::GrabbedByUser,
        ObjectLifecycle::CarriedByPet,
        ObjectLifecycle::StoredInDen,
    ] {
        let mut state = EcologyState::new(56);
        let orb_id = state.objects[0].id;
        state.objects[0].lifecycle = lifecycle;
        state.objects[0].velocity = Vec2::new(0.2, -0.1);
        if lifecycle == ObjectLifecycle::StoredInDen {
            state.objects[0].position = state.den.anchor;
            state.objects[0].velocity = Vec2::ZERO;
            state.den.slots[0] = Some(orb_id);
        }
        let restored = EcologyState::restore(
            serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap(),
        )
        .unwrap();
        assert_eq!(restored.objects[0].id, orb_id);
        let expected = match lifecycle {
            ObjectLifecycle::GrabbedByUser | ObjectLifecycle::CarriedByPet => ObjectLifecycle::Free,
            other => other,
        };
        assert_eq!(restored.objects[0].lifecycle, expected);
        if matches!(
            lifecycle,
            ObjectLifecycle::GrabbedByUser | ObjectLifecycle::CarriedByPet
        ) {
            assert_eq!(restored.objects[0].velocity, Vec2::ZERO);
        }
    }

    let mut state = EcologyState::new(57);
    let morsel_id = state
        .spawn_morsel(
            Vec2::splat(0.5),
            MorselProfile {
                hue: 0.2,
                saturation: 0.8,
                value: 0.9,
                warmth: 0.6,
                pulse_rate: 0.4,
                stimulation: 0.5,
                cohesion_bias: 0.7,
                novelty: 0.8,
            },
            1.0,
        )
        .unwrap();
    state
        .objects
        .iter_mut()
        .find(|object| object.id == morsel_id)
        .unwrap()
        .lifecycle = ObjectLifecycle::Consumed;
    let restored = EcologyState::restore(state).unwrap();
    assert!(
        restored
            .objects
            .iter()
            .all(|object| object.kind == ObjectKind::Orb || object.id != morsel_id)
    );
}
