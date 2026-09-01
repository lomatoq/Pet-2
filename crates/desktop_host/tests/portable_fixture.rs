use desktop_host::{
    DisplayTopology, MonitorId, MonitorInfo, PhysicalDesktopPoint, PortablePetState, RectI,
    StateStore,
};
use lifecore::LifeCore;
use pet_body::ProceduralBody;

const FIXTURE: &str = include_str!("../../../tests/fixtures/portable_pet_state_v1.json");

#[test]
fn portable_fixture_restores_without_native_handles_or_identity_drift() {
    let state: PortablePetState = serde_json::from_str(FIXTURE).expect("fixture JSON decodes");
    state.validate().expect("fixture validates");
    let legacy_identity = (
        state.life.state.genome.identity_seed,
        state.life.state.genome.lineage_id,
        state.life.state.genome.generation,
        state.life.state.genome.body.clone(),
        state.life.state.genome.temperament.clone(),
        state.life.state.genome.brain.clone(),
    );

    let restored = LifeCore::restore(state.life.clone()).expect("LifeCore restores");
    let restored_snapshot = restored.snapshot();
    let mut migrated_snapshot = state.life.clone();
    migrated_snapshot.repair_additive_voice_schema();
    migrated_snapshot.schema_version = lifecore::LIFE_SNAPSHOT_SCHEMA_VERSION;
    let mut normalized_restored = restored_snapshot.clone();
    normalized_restored.state.vocal_motifs = migrated_snapshot.state.vocal_motifs.clone();
    assert_eq!(normalized_restored, migrated_snapshot);
    assert_eq!(
        (
            restored_snapshot.state.genome.identity_seed,
            restored_snapshot.state.genome.lineage_id,
            restored_snapshot.state.genome.generation,
            restored_snapshot.state.genome.body.clone(),
            restored_snapshot.state.genome.temperament.clone(),
            restored_snapshot.state.genome.brain.clone(),
        ),
        legacy_identity
    );
    for legacy_motif in &migrated_snapshot.state.vocal_motifs {
        assert_eq!(
            restored_snapshot
                .state
                .vocal_motifs
                .iter()
                .find(|motif| motif.id == legacy_motif.id),
            Some(legacy_motif)
        );
    }
    assert_eq!(
        restored_snapshot.memories.short_term.len(),
        state.life.memories.short_term.len()
    );
    assert_eq!(restored_snapshot.habits.weights, state.life.habits.weights);
    assert!(state.life.habits.total_updates > 0);
    assert!(
        state
            .life
            .habits
            .weights
            .iter()
            .flatten()
            .any(|weight| *weight != 0.0)
    );
    assert!(!restored.state.vocal_motifs.is_empty());
    assert!(state.life.memories.is_valid());

    let body = ProceduralBody::generate(&restored.state.genome).expect("body regenerates");
    let regenerated = ProceduralBody::generate(&restored.state.genome)
        .expect("body deterministically regenerates");
    assert_eq!(body.mesh.stable_hash(), regenerated.mesh.stable_hash());
    assert_eq!(body.mesh.vertices.len(), regenerated.mesh.vertices.len());
    assert_eq!(body.mesh.indices.len(), regenerated.mesh.indices.len());

    let lower = FIXTURE.to_ascii_lowercase();
    for forbidden in [
        "hwnd",
        "nswindow",
        "raw_window_handle",
        "device_id",
        "audio_device",
        "active_application",
    ] {
        assert!(!lower.contains(forbidden), "fixture leaked {forbidden}");
    }

    let directory = tempfile::tempdir().expect("temporary state directory");
    let store = StateStore::at(directory.path());
    store.save_state(&state).expect("fixture saves atomically");
    assert_eq!(
        store.load_state().expect("fixture reloads"),
        Some(state.clone())
    );

    let fallback_topology = DisplayTopology::new(
        vec![MonitorInfo {
            id: MonitorId("other-platform-primary".into()),
            physical_bounds: RectI {
                minimum: PhysicalDesktopPoint { x: -1_000, y: 100 },
                maximum: PhysicalDesktopPoint { x: 1_000, y: 1_100 },
            },
            working_area: RectI {
                minimum: PhysicalDesktopPoint { x: -1_000, y: 100 },
                maximum: PhysicalDesktopPoint { x: 1_000, y: 1_060 },
            },
            scale_factor: 2.0,
            primary: true,
        }],
        1,
    );
    let remapped = fallback_topology.remap(&state.position);
    assert!(
        fallback_topology.monitors[0]
            .working_area
            .contains(remapped)
    );
}
