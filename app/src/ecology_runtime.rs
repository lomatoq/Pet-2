use desktop_host::{StateStore, StorageError};
use glam::Vec2;
use lifecore::{ActionId, BodyFeedback, BodyIntent, SensorFrame};
use pet_ecology::{
    ContactSource, EcologyBehaviorFrame, EcologyOutput, EcologyState, EmbodiedEnvironmentFrame,
    EpisodeDirector, ExternalContact, MAX_OBJECT_SPEED, ObjectCommand, ObjectId, ObjectKind,
    ObjectLifecycle, ObjectPhysicsConfig, WindowAffordanceFrame, resolve_object_body_contact,
    step_object_with_windows,
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
    environment: EmbodiedEnvironmentFrame,
    orb_trapped_seconds: f32,
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
            environment: EmbodiedEnvironmentFrame::default(),
            orb_trapped_seconds: 0.0,
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
            window_pressure: self.environment.pressure,
            window_escape_direction: self.environment.escape_direction,
            nearest_window_edge: self.environment.contacts[..self
                .environment
                .contact_count
                .min(self.environment.contacts.len())]
                .iter()
                .find(|contact| contact.source == ContactSource::Window)
                .map(|contact| contact.point_world),
            window_motion: self.environment.contacts[..self
                .environment
                .contact_count
                .min(self.environment.contacts.len())]
                .iter()
                .filter(|contact| contact.source == ContactSource::Window)
                .map(|contact| (contact.relative_velocity_px.length() / 1_200.0).clamp(0.0, 1.0))
                .fold(0.0_f32, f32::max),
            orb_trapped: self.environment.orb_trapped,
            timestamp: sensors.timestamp,
        };
        let output = self.director.tick(&mut self.state, frame, brain_intent, dt);
        self.apply_object_commands(&output, dt);
        output
    }

    pub fn fixed_update(
        &mut self,
        desktop_aspect: f32,
        windows: &WindowAffordanceFrame,
        body: &BodyFeedback,
        dt: f32,
    ) {
        let config = ObjectPhysicsConfig {
            desktop_aspect,
            ..ObjectPhysicsConfig::default()
        };
        self.environment = EmbodiedEnvironmentFrame {
            den_anchor: Some(self.state.den.anchor),
            pressure: windows.pressure,
            escape_direction: windows.escape_direction,
            ..EmbodiedEnvironmentFrame::default()
        };
        let reference_height = config.reference_height_px.max(64.0);
        let aspect = config.desktop_aspect.clamp(0.25, 8.0);
        for window in windows.as_slice().iter().take(4) {
            if window.overlap_pressure <= 0.01 {
                continue;
            }
            self.environment.push_contact(ExternalContact {
                source: ContactSource::Window,
                point_world: window.nearest_edge_point,
                normal_world: window.nearest_edge_normal,
                penetration_px: window.overlap_pressure * 96.0,
                relative_velocity_px: Vec2::new(window.velocity.x * aspect, window.velocity.y)
                    * reference_height,
                intensity: window
                    .overlap_pressure
                    .max(window.motion_energy)
                    .clamp(0.0, 1.0),
            });
        }
        let mut orb_has_opposing_contacts = false;
        for object in &mut self.state.objects {
            let contacts_before = self.environment.contact_count;
            step_object_with_windows(object, config, windows, dt, &mut self.environment);
            let window_contact_count = self.environment.contacts[contacts_before
                ..self
                    .environment
                    .contact_count
                    .min(self.environment.contacts.len())]
                .iter()
                .filter(|contact| contact.source == ContactSource::Window)
                .count();
            orb_has_opposing_contacts |= object.kind == ObjectKind::Orb
                && window_contact_count >= 2
                && object.velocity.length() < 0.15;
            resolve_object_body_contact(
                object,
                body.world_position,
                body.velocity,
                config,
                &mut self.environment,
            );
        }
        let trapped_now = orb_has_opposing_contacts
            || self.state.objects.iter().any(|object| {
                object.kind == ObjectKind::Orb
                    && matches!(
                        object.lifecycle,
                        ObjectLifecycle::Free | ObjectLifecycle::Sleeping
                    )
                    && object.velocity.length() < 0.08
                    && windows.as_slice().iter().any(|window| {
                        object.position.cmpge(window.bounds.minimum).all()
                            && object.position.cmple(window.bounds.maximum).all()
                    })
            });
        self.orb_trapped_seconds = if trapped_now {
            (self.orb_trapped_seconds + dt.max(0.0)).min(8.0)
        } else {
            0.0
        };
        self.environment.orb_trapped = self.orb_trapped_seconds >= 0.75;
        self.state.metabolism.advance(dt);
    }

    #[must_use]
    pub const fn environment(&self) -> &EmbodiedEnvironmentFrame {
        &self.environment
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
                ObjectCommand::Retrieve { object_id, target } => {
                    for slot in &mut self.state.den.slots {
                        if *slot == Some(object_id) {
                            *slot = None;
                        }
                    }
                    if let Some(object) = self
                        .state
                        .objects
                        .iter_mut()
                        .find(|object| object.id == object_id)
                    {
                        object.lifecycle = ObjectLifecycle::Free;
                        object.position = target.clamp(Vec2::ZERO, Vec2::ONE);
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

    #[test]
    fn retrieve_episode_clears_den_slot_without_losing_orb() {
        use lifecore::{ExpressionState, LocomotionMode, PoseIntent};

        let directory = tempfile::tempdir().unwrap();
        let store = StateStore::at(directory.path());
        let mut runtime = EcologyRuntime::load_or_create(&store, 74, true).unwrap();
        let orb_id = runtime.state.objects[0].id;
        runtime.state.objects[0].lifecycle = ObjectLifecycle::StoredInDen;
        runtime.state.objects[0].position = runtime.state.den.anchor;
        runtime.state.den.slots[0] = Some(orb_id);
        let sensors = SensorFrame {
            cursor_position: Vec2::new(0.5, 0.4),
            ..SensorFrame::default()
        };
        let body = BodyFeedback {
            world_position: runtime.state.den.anchor,
            ..BodyFeedback::default()
        };
        let intent = BodyIntent {
            locomotion: LocomotionMode::Hover,
            target_position: body.world_position,
            target_surface: None,
            desired_speed: 0.0,
            facing_direction: 1.0,
            gaze_target: None,
            pose: PoseIntent::Neutral,
            expression: ExpressionState::default(),
            interaction_target: None,
        };
        let _ = runtime.resolve_intent(
            intent,
            ActionId::BringProceduralOrb,
            &sensors,
            &body,
            false,
            0.05,
        );
        assert_eq!(runtime.state.den.slots, [None; 3]);
        assert_eq!(runtime.state.objects.len(), 1);
        assert_eq!(runtime.state.objects[0].lifecycle, ObjectLifecycle::Free);
        runtime.state.validate().unwrap();
    }
}
