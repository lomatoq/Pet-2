use desktop_host::{StateStore, StorageError};
use lifecore::BodyIntent;
use pet_ecology::{EcologyOutput, EcologyState, EpisodeDirector, ObjectPhysicsConfig, step_object};

/// Application integration boundary for the portable habitat. Native input,
/// rendering and body physics stay in their existing owners.
pub struct EcologyRuntime {
    state: EcologyState,
    director: EpisodeDirector,
}

impl EcologyRuntime {
    pub fn load_or_create(
        store: &StateStore,
        identity_seed: u64,
        reset: bool,
    ) -> Result<Self, StorageError> {
        let state = if reset {
            EcologyState::new(identity_seed)
        } else {
            store
                .load_ecology_state()?
                .filter(|state| state.identity_seed == identity_seed)
                .unwrap_or_else(|| EcologyState::new(identity_seed))
        };
        Ok(Self {
            state,
            director: EpisodeDirector::default(),
        })
    }

    #[must_use]
    pub fn resolve_intent(&mut self, brain_intent: BodyIntent, focus_mode: bool) -> EcologyOutput {
        self.director.tick_passthrough(brain_intent, focus_mode)
    }

    pub fn fixed_update(&mut self, desktop_aspect: f32, dt: f32) {
        let config = ObjectPhysicsConfig {
            desktop_aspect,
            ..ObjectPhysicsConfig::default()
        };
        for object in &mut self.state.objects {
            step_object(object, config, dt);
        }
        self.state.metabolism.advance(dt);
    }

    #[must_use]
    pub fn snapshot(&self) -> EcologyState {
        self.state.snapshot()
    }

    pub fn prepare_shutdown(&mut self) {
        if self.director.interrupt_for_shutdown().is_some() {
            self.state.episode_stats.interrupted_by_shutdown = self
                .state
                .episode_stats
                .interrupted_by_shutdown
                .saturating_add(1);
        }
    }
}
