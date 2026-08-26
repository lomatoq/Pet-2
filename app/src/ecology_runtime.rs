use desktop_host::{StateStore, StorageError};
use glam::Vec2;
use lifecore::{ActionId, BodyFeedback, BodyIntent, SensorFrame};
use pet_ecology::{
    EcologyBehaviorFrame, EcologyOutput, EcologyState, EpisodeDirector, MAX_OBJECT_SPEED,
    ObjectCommand, ObjectId, ObjectKind, ObjectLifecycle, ObjectPhysicsConfig, step_object,
};

/// Application integration boundary for the portable habitat. Native input,
/// rendering and body physics stay in their existing owners.
pub struct EcologyRuntime {
    state: EcologyState,
    director: EpisodeDirector,
    pointer_down: bool,
    grabbed_object: Option<ObjectId>,
    last_pointer_position: Option<Vec2>,
    last_pointer_seconds: f64,
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
            pointer_down: false,
            grabbed_object: None,
            last_pointer_position: None,
            last_pointer_seconds: 0.0,
        })
    }

    #[must_use]
    pub fn resolve_intent(
        &mut self,
        brain_intent: BodyIntent,
        selected_action: ActionId,
        sensors: &SensorFrame,
        body: &BodyFeedback,
        focus_mode: bool,
        dt: f32,
    ) -> EcologyOutput {
        let frame = EcologyBehaviorFrame {
            selected_action,
            pet_position: body.world_position,
            pet_velocity: body.velocity,
            cursor_position: sensors.cursor_position,
            pointer_down: sensors.pointer_down,
            user_activity: sensors.user_activity_rate,
            focus_mode,
            sleeping: selected_action == ActionId::Sleep,
            timestamp: sensors.timestamp,
        };
        let output = self.director.tick(&mut self.state, frame, brain_intent, dt);
        self.apply_object_commands(&output, dt);
        output
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

    fn apply_object_commands(&mut self, output: &EcologyOutput, dt: f32) {
        let dt = if dt.is_finite() {
            dt.clamp(0.0, 0.25)
        } else {
            0.0
        };
        for command in output
            .object_commands
            .iter()
            .copied()
            .take(output.object_command_count)
        {
            match command {
                ObjectCommand::None => {}
                ObjectCommand::ApplyImpulse { object_id, impulse } => {
                    if let Some(object) = self
                        .state
                        .objects
                        .iter_mut()
                        .find(|object| object.id == object_id)
                    {
                        object.lifecycle = ObjectLifecycle::Free;
                        object.velocity += impulse / object.mass.max(0.05);
                        if object.velocity.length_squared() > MAX_OBJECT_SPEED * MAX_OBJECT_SPEED {
                            object.velocity =
                                object.velocity.normalize_or_zero() * MAX_OBJECT_SPEED;
                        }
                    }
                }
                ObjectCommand::MoveToward {
                    object_id,
                    target,
                    speed,
                } => {
                    if let Some(object) = self
                        .state
                        .objects
                        .iter_mut()
                        .find(|object| object.id == object_id)
                    {
                        let alpha = 1.0 - (-speed.clamp(0.0, 12.0) * dt).exp();
                        let previous = object.position;
                        object.position = object
                            .position
                            .lerp(target.clamp(Vec2::ZERO, Vec2::ONE), alpha);
                        object.velocity = if dt > f32::EPSILON {
                            (object.position - previous) / dt
                        } else {
                            Vec2::ZERO
                        };
                        object.lifecycle = ObjectLifecycle::CarriedByPet;
                    }
                }
                ObjectCommand::Release {
                    object_id,
                    velocity,
                } => {
                    if let Some(object) = self
                        .state
                        .objects
                        .iter_mut()
                        .find(|object| object.id == object_id)
                    {
                        object.lifecycle = ObjectLifecycle::Free;
                        object.velocity =
                            if velocity.length_squared() > MAX_OBJECT_SPEED * MAX_OBJECT_SPEED {
                                velocity.normalize_or_zero() * MAX_OBJECT_SPEED
                            } else {
                                velocity
                            };
                    }
                }
                ObjectCommand::Store { object_id, slot } if usize::from(slot) < 3 => {
                    self.state.den.slots[usize::from(slot)] = Some(object_id);
                    if let Some(object) = self
                        .state
                        .objects
                        .iter_mut()
                        .find(|object| object.id == object_id)
                    {
                        object.lifecycle = ObjectLifecycle::StoredInDen;
                        object.home_slot = Some(slot);
                        object.position = self.state.den.anchor;
                        object.velocity = Vec2::ZERO;
                    }
                }
                ObjectCommand::Consume { object_id } => {
                    if let Some(object) =
                        self.state.objects.iter_mut().find(|object| {
                            object.id == object_id && object.kind == ObjectKind::Morsel
                        })
                    {
                        object.lifecycle = ObjectLifecycle::Consumed;
                        object.velocity = Vec2::ZERO;
                    }
                }
                ObjectCommand::Store { .. } => {}
            }
        }
    }

    #[must_use]
    pub fn hit_test(
        &self,
        cursor: Vec2,
        desktop_aspect: f32,
        desktop_height_px: f32,
        margin_px: f32,
    ) -> bool {
        if !cursor.is_finite() || desktop_height_px <= 0.0 {
            return false;
        }
        let aspect = desktop_aspect.clamp(0.25, 8.0);
        self.state.objects.iter().any(|object| {
            object.kind == ObjectKind::Orb
                && object.lifecycle != ObjectLifecycle::Consumed
                && object.lifecycle != ObjectLifecycle::StoredInDen
                && Vec2::new(
                    (cursor.x - object.position.x) * aspect,
                    cursor.y - object.position.y,
                )
                .length()
                    <= (object.radius_px_at_reference + margin_px.max(0.0)) / desktop_height_px
        })
    }

    /// Owns orb capture independently from body petting. Returns true only on
    /// the press edge so learning receives one causally grounded play event.
    #[allow(clippy::too_many_arguments)]
    pub fn observe_pointer(
        &mut self,
        cursor: Option<Vec2>,
        down: bool,
        allow_new_capture: bool,
        desktop_aspect: f32,
        desktop_height_px: f32,
        timestamp: f64,
    ) -> bool {
        let pressed = down && !self.pointer_down;
        let released = !down && self.pointer_down;
        let aspect = desktop_aspect.clamp(0.25, 8.0);
        if pressed
            && allow_new_capture
            && let Some(cursor) = cursor
        {
            self.grabbed_object = self
                .state
                .objects
                .iter()
                .find(|object| {
                    object.kind == ObjectKind::Orb
                        && object.lifecycle != ObjectLifecycle::StoredInDen
                        && object.lifecycle != ObjectLifecycle::Consumed
                        && Vec2::new(
                            (cursor.x - object.position.x) * aspect,
                            cursor.y - object.position.y,
                        )
                        .length()
                            <= (object.radius_px_at_reference + 5.0) / desktop_height_px.max(1.0)
                })
                .map(|object| object.id);
            if let Some(object_id) = self.grabbed_object
                && let Some(object) = self
                    .state
                    .objects
                    .iter_mut()
                    .find(|object| object.id == object_id)
            {
                object.lifecycle = ObjectLifecycle::GrabbedByUser;
                object.velocity = Vec2::ZERO;
                object.last_interaction_seconds = timestamp.max(0.0);
            }
        }
        if let (Some(object_id), Some(cursor)) = (self.grabbed_object, cursor)
            && let Some(object) = self
                .state
                .objects
                .iter_mut()
                .find(|object| object.id == object_id)
        {
            let clamped = cursor.clamp(Vec2::splat(0.001), Vec2::splat(0.999));
            let dt = (timestamp - self.last_pointer_seconds).clamp(1.0 / 1_000.0, 0.1) as f32;
            if let Some(previous) = self.last_pointer_position {
                let height_velocity = Vec2::new(
                    (clamped.x - previous.x) * aspect / dt,
                    (clamped.y - previous.y) / dt,
                );
                let clamped_velocity =
                    if height_velocity.length_squared() > MAX_OBJECT_SPEED * MAX_OBJECT_SPEED {
                        height_velocity.normalize_or_zero() * MAX_OBJECT_SPEED
                    } else {
                        height_velocity
                    };
                object.velocity = object.velocity.lerp(clamped_velocity, 0.58);
            }
            object.position = clamped;
            object.last_interaction_seconds = timestamp.max(0.0);
        }
        let touched = pressed && self.grabbed_object.is_some();
        if released
            && let Some(object_id) = self.grabbed_object.take()
            && let Some(object) = self
                .state
                .objects
                .iter_mut()
                .find(|object| object.id == object_id)
        {
            object.lifecycle = ObjectLifecycle::Free;
            if object.velocity.length_squared() > MAX_OBJECT_SPEED * MAX_OBJECT_SPEED {
                object.velocity = object.velocity.normalize_or_zero() * MAX_OBJECT_SPEED;
            }
            object.familiarity = (object.familiarity + 0.015).clamp(0.0, 1.0);
            object.novelty = (object.novelty - 0.008).clamp(0.0, 1.0);
            object.wear = (object.wear + 0.001).clamp(0.0, 1.0);
        }
        self.pointer_down = down;
        if let Some(cursor) = cursor {
            self.last_pointer_position = Some(cursor);
            self.last_pointer_seconds = timestamp;
        }
        touched
    }

    #[must_use]
    pub const fn is_dragging_object(&self) -> bool {
        self.grabbed_object.is_some()
    }

    #[must_use]
    pub const fn state(&self) -> &EcologyState {
        &self.state
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn orb_hit_test_and_throw_capture_are_bounded() {
        let directory = tempfile::tempdir().unwrap();
        let store = StateStore::at(directory.path());
        let mut runtime = EcologyRuntime::load_or_create(&store, 73, true).unwrap();
        let orb_position = runtime.state.objects[0].position;
        assert!(runtime.hit_test(orb_position, 16.0 / 9.0, 1_080.0, 0.0));
        assert!(runtime.observe_pointer(Some(orb_position), true, true, 16.0 / 9.0, 1_080.0, 1.0,));
        assert!(runtime.is_dragging_object());
        runtime.observe_pointer(
            Some(Vec2::new(0.95, 0.08)),
            true,
            true,
            16.0 / 9.0,
            1_080.0,
            1.01,
        );
        runtime.observe_pointer(
            Some(Vec2::new(0.95, 0.08)),
            false,
            true,
            16.0 / 9.0,
            1_080.0,
            1.02,
        );
        assert!(!runtime.is_dragging_object());
        assert!(runtime.state.objects[0].velocity.length() <= MAX_OBJECT_SPEED + 1.0e-5);
        runtime.state.validate().unwrap();
    }
}
