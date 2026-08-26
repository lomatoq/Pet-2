use pet_ecology::{EcologyState, ObjectLifecycle};

#[test]
fn ecology_json_roundtrip_preserves_den_slots_and_has_no_active_episode() {
    let mut state = EcologyState::new(55);
    let orb_id = state.objects[0].id;
    state.objects[0].lifecycle = ObjectLifecycle::StoredInDen;
    state.den.slots[0] = Some(orb_id);
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
