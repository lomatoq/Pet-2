use std::time::Instant;

use desktop_host::{StateStore, StorageError};
use glam::Vec2;
use lifecore::{ActionId, BodyFeedback, BodyIntent, Drives, SensorFrame};
use morph_brain::{
    MORPH_ACTION_CONTROL_COUNT, MORPH_OBJECT_SLOT_COUNT, MorphObjectInput, MorphWorldInput,
};
use pet_body::EcologyVisualEffect;
use pet_ecology::{
    ActionSignature, ActivityEpisode, ContactSource, ConventionOutcome, EcologyBehaviorFrame,
    EcologyDecisionTrace, EcologyOutcome, EcologyOutput, EcologyState, EcologyVisualContext,
    EcologyVocalTrigger, EmbodiedEnvironmentFrame, EpisodeDirector, EpisodeGoal, ExternalContact,
    GestureConventionMatch, GestureConventionMeaning, GestureSignature, MAX_OBJECT_SPEED,
    MorselProfile, ObjectCommand, ObjectId, ObjectKind, ObjectLifecycle, ObjectPhysicsConfig,
    RhythmSignature, WindowAffordanceFrame, WorldObject, orb_is_inside_den_latch,
    resolve_object_body_contact, step_den_attraction, step_object_with_windows,
};

/// Application integration boundary for the portable habitat. Native input,
/// rendering and body physics stay in their existing owners.
pub struct EcologyRuntime {
    state: EcologyState,
    director: EpisodeDirector,
    pointer_down: bool,
    grabbed_object: Option<ObjectId>,
    grab_press_position: Option<Vec2>,
    grab_offset: Vec2,
    grab_active: bool,
    drag_velocity: Vec2,
    last_pointer_seconds: f64,
    environment: EmbodiedEnvironmentFrame,
    orb_trapped_seconds: f32,
    last_visual_context: EcologyVisualContext,
    visual_target: Option<Vec2>,
    visual_hue: f32,
    visual_strength: f32,
    visual_colorfulness: f32,
    visual_structure: f32,
    visual_surprise: f32,
    shared_attention: bool,
    click_rhythm: Option<RhythmSignature>,
    last_debug: EcologyDecisionTrace,
    last_vocal_trigger: Option<EcologyVocalTrigger>,
    last_motor_error: Option<f32>,
    episode_tick_microseconds: f64,
    object_physics_microseconds: f64,
    desktop_aspect: f32,
}

fn morph_object_priority(
    object: &WorldObject,
    pet_position: Vec2,
    desktop_aspect: f32,
    active_object: Option<ObjectId>,
    drives: Drives,
) -> f32 {
    let distance = desktop_distance(object.position, pet_position, desktop_aspect);
    let goal = if Some(object.id) == active_object {
        1.0
    } else {
        0.0
    };
    let kind_value = match object.kind {
        ObjectKind::Orb => drives.play,
        ObjectKind::Morsel => drives.curiosity,
    };
    goal * 4.0
        + (-distance * 3.2).exp()
        + object.novelty * 0.42
        + object.preference.max(0.0) * 0.30
        + kind_value * 0.36
        + (object.velocity.length() / MAX_OBJECT_SPEED).clamp(0.0, 1.0) * 0.40
}

fn morph_action_biases(
    goal: EpisodeGoal,
    distance: f32,
    drives: Drives,
) -> [f32; MORPH_ACTION_CONTROL_COUNT] {
    const SAMPLE: usize = 0;
    const PUSH: usize = 1;
    const TOUCH: usize = 2;
    const PULL: usize = 3;
    const LISTEN: usize = 4;
    const SNIFF: usize = 5;
    const GRASP: usize = 6;
    const RELEASE: usize = 7;
    let mut biases = [0.0; MORPH_ACTION_CONTROL_COUNT];
    match goal {
        EpisodeGoal::ChaseOrb | EpisodeGoal::InterceptOrb | EpisodeGoal::SoloOrbPlay => {
            if distance <= 0.17 {
                biases[PUSH] = 0.82 + drives.play * 0.72;
                biases[TOUCH] = 0.22 + drives.curiosity * 0.24;
            }
        }
        EpisodeGoal::RetrieveOrb
        | EpisodeGoal::CarryOrbHome
        | EpisodeGoal::ReturnOrb
        | EpisodeGoal::HideOrb
        | EpisodeGoal::StoreMorsel => {
            if distance <= 0.14 {
                biases[GRASP] = 0.86 + drives.autonomy * 0.54;
                biases[PULL] = 0.32 + drives.play * 0.28;
            }
        }
        EpisodeGoal::OfferOrb => {
            biases[RELEASE] = 0.92 + drives.social * 0.46;
        }
        EpisodeGoal::InspectMorsel => {
            if distance <= 0.25 {
                biases[SNIFF] = 0.78 + drives.curiosity * 0.58;
                biases[LISTEN] = 0.18 + drives.curiosity * 0.18;
            }
        }
        EpisodeGoal::EatMorsel => {
            if distance <= 0.12 {
                biases[SAMPLE] = 0.92 + drives.curiosity * 0.42;
            }
        }
        EpisodeGoal::RefuseMorsel => {
            if distance <= 0.16 {
                biases[PUSH] = 0.86;
            }
        }
        EpisodeGoal::InspectWindow | EpisodeGoal::SharedAttention => {
            biases[LISTEN] = 0.30 + drives.curiosity * 0.28;
        }
        _ => {}
    }
    biases
}

fn desktop_distance(left: Vec2, right: Vec2, desktop_aspect: f32) -> f32 {
    Vec2::new(
        (left.x - right.x) * desktop_aspect.clamp(0.25, 8.0),
        left.y - right.y,
    )
    .length()
}

fn hsv_to_rgb(hue: f32, saturation: f32, value: f32) -> [f32; 3] {
    let hue = hue.rem_euclid(1.0) * 6.0;
    let saturation = saturation.clamp(0.0, 1.0);
    let value = value.clamp(0.0, 1.0);
    let chroma = value * saturation;
    let x = chroma * (1.0 - (hue.rem_euclid(2.0) - 1.0).abs());
    let (red, green, blue) = match hue.floor() as u8 {
        0 => (chroma, x, 0.0),
        1 => (x, chroma, 0.0),
        2 => (0.0, chroma, x),
        3 => (0.0, x, chroma),
        4 => (x, 0.0, chroma),
        _ => (chroma, 0.0, x),
    };
    let offset = value - chroma;
    [red + offset, green + offset, blue + offset]
}

pub(crate) struct EcologyResolveFrame<'a> {
    pub selected_action: ActionId,
    pub drives: Drives,
    pub sensors: &'a SensorFrame,
    pub body: &'a BodyFeedback,
    pub focus_mode: bool,
    pub dt: f32,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct VisualAttentionSample {
    pub target: Option<Vec2>,
    pub hue: f32,
    pub strength: f32,
    pub explicit: bool,
    pub colorfulness: f32,
    pub structure: f32,
    pub surprise: f32,
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
            grab_press_position: None,
            grab_offset: Vec2::ZERO,
            grab_active: false,
            drag_velocity: Vec2::ZERO,
            last_pointer_seconds: 0.0,
            environment: EmbodiedEnvironmentFrame::default(),
            orb_trapped_seconds: 0.0,
            last_visual_context: EcologyVisualContext::default(),
            visual_target: None,
            visual_hue: 0.0,
            visual_strength: 0.0,
            visual_colorfulness: 0.0,
            visual_structure: 0.0,
            visual_surprise: 0.0,
            shared_attention: false,
            click_rhythm: None,
            last_debug: EcologyDecisionTrace::default(),
            last_vocal_trigger: None,
            last_motor_error: None,
            episode_tick_microseconds: 0.0,
            object_physics_microseconds: 0.0,
            desktop_aspect: 1.0,
        })
    }

    #[must_use]
    pub fn resolve_intent(
        &mut self,
        brain_intent: BodyIntent,
        frame: EcologyResolveFrame<'_>,
    ) -> EcologyOutput {
        let EcologyResolveFrame {
            selected_action,
            drives,
            sensors,
            body,
            focus_mode,
            dt,
        } = frame;
        let frame = EcologyBehaviorFrame {
            selected_action,
            pet_position: body.world_position,
            pet_velocity: body.velocity,
            desktop_aspect: self.desktop_aspect,
            cursor_position: sensors.cursor_position,
            pointer_down: sensors.pointer_down,
            user_activity: sensors.user_activity_rate,
            user_available: sensors.user_availability.unwrap_or({
                if sensors.user_idle_seconds < 120.0 {
                    1.0
                } else {
                    0.0
                }
            }),
            play_drive: drives.play,
            curiosity_drive: drives.curiosity,
            autonomy_drive: drives.autonomy,
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
            visual_target: self.visual_target,
            visual_hue: self.visual_hue,
            visual_strength: self.visual_strength,
            visual_colorfulness: self.visual_colorfulness,
            visual_structure: self.visual_structure,
            visual_surprise: self.visual_surprise,
            shared_attention: self.shared_attention,
            autonomous_play_ready: false,
            click_rhythm: self.click_rhythm,
            timestamp: sensors.timestamp,
        };
        let started = Instant::now();
        let output = self.director.tick(&mut self.state, frame, brain_intent, dt);
        self.episode_tick_microseconds = started.elapsed().as_secs_f64() * 1_000_000.0;
        self.last_visual_context = output.visual_context;
        self.last_debug = output.debug.clone();
        self.last_vocal_trigger = output.vocal_trigger;
        self.last_motor_error = output.outcomes[..output.outcome_count]
            .iter()
            .find_map(|outcome| match outcome {
                EcologyOutcome::SkillMotorError { error, .. } => Some(*error),
                _ => None,
            })
            .or(self.last_motor_error);
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
        let started = Instant::now();
        self.desktop_aspect = desktop_aspect.clamp(0.25, 8.0);
        let config = ObjectPhysicsConfig {
            desktop_aspect: self.desktop_aspect,
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
        let den_anchor = self.state.den.anchor;
        let orb_slot = self
            .state
            .objects
            .iter()
            .find(|object| object.kind == ObjectKind::Orb)
            .and_then(|orb| {
                orb.home_slot
                    .filter(|slot| {
                        self.state.den.slots[usize::from(*slot)].is_none()
                            || self.state.den.slots[usize::from(*slot)] == Some(orb.id)
                    })
                    .or_else(|| {
                        self.state
                            .den
                            .slots
                            .iter()
                            .position(Option::is_none)
                            .map(|slot| slot as u8)
                    })
            });
        let mut captured_orb = None;
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
            let window_contacts = &self.environment.contacts[contacts_before
                ..self
                    .environment
                    .contact_count
                    .min(self.environment.contacts.len())];
            let has_opposing_normals = window_contacts.iter().enumerate().any(|(index, left)| {
                left.source == ContactSource::Window
                    && window_contacts[index + 1..].iter().any(|right| {
                        right.source == ContactSource::Window
                            && left.normal_world.dot(right.normal_world) < -0.35
                    })
            });
            orb_has_opposing_contacts |= object.kind == ObjectKind::Orb
                && window_contact_count >= 2
                && has_opposing_normals
                && object.velocity.length() < 0.15;
            resolve_object_body_contact(
                object,
                body.world_position,
                body.velocity,
                config,
                &mut self.environment,
            );
            if step_den_attraction(object, den_anchor, config, dt)
                && let Some(slot) = orb_slot
            {
                object.lifecycle = ObjectLifecycle::StoredInDen;
                object.home_slot = Some(slot);
                captured_orb = Some((object.id, slot));
            }
            if object.kind == ObjectKind::Orb {
                self.environment.orb_position = Some(object.position);
                if object.lifecycle == ObjectLifecycle::StoredInDen {
                    self.environment.orb_grounded = false;
                }
            }
        }
        if let Some((object_id, slot)) = captured_orb {
            self.clear_den_slot_references(object_id);
            self.state.den.slots[usize::from(slot)] = Some(object_id);
            self.state.den.visits = self.state.den.visits.saturating_add(1);
            self.state.den.familiarity = (self.state.den.familiarity + 0.004).clamp(0.0, 1.0);
            self.environment.orb_grounded = false;
            self.environment.orb_position = self
                .state
                .objects
                .iter()
                .find(|object| object.kind == ObjectKind::Orb)
                .map(|object| object.position);
        }
        // Being inside a window bounding box is not a trap. Large/maximized
        // windows routinely cover the orb, and lower z-order edges may be fully
        // occluded. A trap requires sustained, physically resolved opposing
        // contacts with low escape velocity.
        let trapped_now = orb_has_opposing_contacts;
        self.orb_trapped_seconds = if trapped_now {
            (self.orb_trapped_seconds + dt.max(0.0)).min(8.0)
        } else {
            0.0
        };
        self.environment.orb_trapped = self.orb_trapped_seconds >= 0.75;
        self.object_physics_microseconds = started.elapsed().as_secs_f64() * 1_000_000.0;
        self.state.metabolism.advance(dt);
    }

    #[must_use]
    pub const fn environment(&self) -> &EmbodiedEnvironmentFrame {
        &self.environment
    }

    pub fn spawn_morsel(
        &mut self,
        position: Vec2,
        profile: MorselProfile,
        timestamp: f64,
    ) -> Option<ObjectId> {
        self.state.spawn_morsel(position, profile, timestamp)
    }

    pub fn set_visual_attention(&mut self, sample: VisualAttentionSample) {
        let VisualAttentionSample {
            target,
            hue,
            strength,
            explicit,
            colorfulness,
            structure,
            surprise,
        } = sample;
        self.visual_target = target.filter(|position| position.is_finite());
        self.visual_hue = if hue.is_finite() {
            hue.rem_euclid(1.0)
        } else {
            0.0
        };
        self.visual_strength = if strength.is_finite() {
            strength.clamp(0.0, 1.0)
        } else {
            0.0
        };
        self.visual_colorfulness = if colorfulness.is_finite() {
            colorfulness.clamp(0.0, 1.0)
        } else {
            0.0
        };
        self.visual_structure = if structure.is_finite() {
            structure.clamp(0.0, 1.0)
        } else {
            0.0
        };
        self.visual_surprise = if surprise.is_finite() {
            surprise.clamp(0.0, 1.0)
        } else {
            0.0
        };
        self.shared_attention = explicit && self.visual_target.is_some();
    }

    pub fn set_click_rhythm(&mut self, rhythm: Option<RhythmSignature>) {
        self.click_rhythm = rhythm;
    }

    #[must_use]
    pub const fn debug(&self) -> &EcologyDecisionTrace {
        &self.last_debug
    }

    #[must_use]
    pub const fn active_episode(&self) -> Option<&ActivityEpisode> {
        self.director.active_episode()
    }

    #[must_use]
    pub const fn last_vocal_trigger(&self) -> Option<EcologyVocalTrigger> {
        self.last_vocal_trigger
    }

    #[must_use]
    pub const fn last_motor_error(&self) -> Option<f32> {
        self.last_motor_error
    }

    #[must_use]
    pub const fn visual_context(&self) -> EcologyVisualContext {
        self.last_visual_context
    }

    #[must_use]
    pub const fn episode_tick_microseconds(&self) -> f64 {
        self.episode_tick_microseconds
    }

    #[must_use]
    pub const fn object_physics_microseconds(&self) -> f64 {
        self.object_physics_microseconds
    }

    pub fn observe_explicit_refusal(&mut self, timestamp: f64) -> bool {
        self.director
            .observe_explicit_refusal(&mut self.state, timestamp)
    }

    pub fn learn_signature(
        &mut self,
        signature: ActionSignature,
        timestamp: f64,
    ) -> Result<(u64, bool), pet_ecology::EcologyError> {
        self.state.skills.observe(signature, timestamp)
    }

    #[must_use]
    pub fn recognize_gesture_convention(
        &self,
        signature: &GestureSignature,
        quality: f32,
        safety_boundary_or_sleep: bool,
    ) -> Option<GestureConventionMatch> {
        self.state
            .gesture_conventions
            .recognize(signature, quality, safety_boundary_or_sleep)
    }

    #[must_use]
    pub fn nearest_gesture_convention(
        &self,
        meaning: GestureConventionMeaning,
        signature: &GestureSignature,
    ) -> Option<u64> {
        self.state
            .gesture_conventions
            .nearest_id(meaning, signature)
    }

    pub fn observe_gesture_convention_success(
        &mut self,
        meaning: GestureConventionMeaning,
        signature: GestureSignature,
        timestamp: f64,
        strong_or_explicit: bool,
        learning_openness: f32,
    ) -> Result<Option<(u64, bool)>, pet_ecology::EcologyError> {
        self.state
            .gesture_conventions
            .observe_success_with_openness(
                meaning,
                signature,
                timestamp,
                strong_or_explicit,
                learning_openness,
            )
    }

    pub fn record_gesture_convention_outcome(
        &mut self,
        id: u64,
        outcome: ConventionOutcome,
        timestamp: f64,
    ) -> Result<(), pet_ecology::EcologyError> {
        self.state
            .gesture_conventions
            .record_outcome(id, outcome, timestamp)
    }

    pub fn delete_gesture_convention(&mut self, id: u64) -> bool {
        self.state.gesture_conventions.delete(id)
    }

    pub fn clear_gesture_conventions(&mut self) -> bool {
        self.state.gesture_conventions.clear()
    }

    pub fn rollback_gesture_conventions(&mut self, version: u32) -> bool {
        self.state
            .gesture_conventions
            .rollback_to_version(u64::from(version))
    }

    #[must_use]
    pub fn visual_effect(&self) -> EcologyVisualEffect {
        let mut effect = EcologyVisualEffect::default();
        if let Some(food) = &self.state.metabolism.active_effect {
            let strength = self.state.metabolism.digestion.clamp(0.0, 1.0);
            effect.hue = food.hue;
            effect.color_blend = strength * 0.35;
            effect.flow_boost = food.stimulation * strength;
            effect.glow_boost = (0.30 + food.stimulation * 0.55) * strength;
            effect.cohesion_bias = food.cohesion_bias;
            effect.translucency_boost = food.warmth * strength * 0.12;
        }
        let context_blend = self
            .last_visual_context
            .chromatic_blend
            .max(self.last_visual_context.camouflage_blend)
            .clamp(0.0, 0.65);
        if context_blend > effect.color_blend {
            effect.hue = self.last_visual_context.chromatic_hue;
            effect.color_blend = context_blend;
        }
        let visual_energy = self
            .last_visual_context
            .visual_structure
            .max(self.last_visual_context.visual_surprise)
            .clamp(0.0, 1.0);
        if visual_energy > 0.0 {
            // Shape/change perception must alter the visible body, not only an
            // internal target. Structure tightens the liquid silhouette while
            // surprise produces a short flow/glow pulse.
            effect.flow_boost = effect.flow_boost.max(0.24 + visual_energy * 0.62);
            effect.glow_boost = effect.glow_boost.max(0.30 + visual_energy * 0.52);
            effect.cohesion_bias = effect
                .cohesion_bias
                .max(0.42 + self.last_visual_context.visual_structure * 0.46);
            effect.translucency_boost = effect
                .translucency_boost
                .max(self.last_visual_context.visual_surprise * 0.14);
        }
        effect.contrast_reduction = self.last_visual_context.camouflage_blend * 0.45;
        effect.bounded()
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
                    self.clear_den_slot_references(object_id);
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
                    self.clear_den_slot_references(object_id);
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
                    self.clear_den_slot_references(object_id);
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
                    let Some(object_index) = self
                        .state
                        .objects
                        .iter()
                        .position(|object| object.id == object_id)
                    else {
                        continue;
                    };
                    let object = &self.state.objects[object_index];
                    let config = ObjectPhysicsConfig {
                        desktop_aspect: self.desktop_aspect,
                        ..ObjectPhysicsConfig::default()
                    };
                    if object.lifecycle != ObjectLifecycle::CarriedByPet
                        || !orb_is_inside_den_latch(object, self.state.den.anchor, config)
                    {
                        continue;
                    }
                    self.clear_den_slot_references(object_id);
                    let object = &mut self.state.objects[object_index];
                    // The pet hands off inside the den field; the same viscous
                    // controller then pulls the released orb to the exact
                    // center before the slot becomes authoritative.
                    object.lifecycle = ObjectLifecycle::Free;
                    object.home_slot = Some(slot);
                    object.velocity = Vec2::ZERO;
                }
                ObjectCommand::Retrieve {
                    object_id,
                    target: _,
                } => {
                    self.clear_den_slot_references(object_id);
                    if let Some(object) = self
                        .state
                        .objects
                        .iter_mut()
                        .find(|object| object.id == object_id)
                    {
                        // Backward-compatible command semantics: take the
                        // object into the pet's carry state at its current
                        // position. Movement is subsequently explicit.
                        object.lifecycle = ObjectLifecycle::CarriedByPet;
                        object.home_slot = None;
                        object.velocity = Vec2::ZERO;
                    }
                }
                ObjectCommand::Consume { object_id } => {
                    if let Some(index) = self.state.objects.iter().position(|object| {
                        object.id == object_id && object.kind == ObjectKind::Morsel
                    }) {
                        self.state.objects.remove(index);
                        for slot in &mut self.state.den.slots {
                            if *slot == Some(object_id) {
                                *slot = None;
                            }
                        }
                    }
                }
                ObjectCommand::Store { .. } => {}
            }
        }
    }

    fn clear_den_slot_references(&mut self, object_id: ObjectId) {
        for slot in &mut self.state.den.slots {
            if *slot == Some(object_id) {
                *slot = None;
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
            let capture = self
                .state
                .objects
                .iter()
                .find(|object| {
                    object.kind == ObjectKind::Orb
                        && object.lifecycle != ObjectLifecycle::Consumed
                        && Vec2::new(
                            (cursor.x - object.position.x) * aspect,
                            cursor.y - object.position.y,
                        )
                        .length()
                            <= (object.radius_px_at_reference + 5.0) / desktop_height_px.max(1.0)
                })
                .map(|object| (object.id, object.position));
            if let Some((object_id, object_position)) = capture {
                self.grabbed_object = Some(object_id);
                self.grab_press_position = Some(cursor);
                self.grab_offset = object_position - cursor;
                self.grab_active = false;
                self.drag_velocity = Vec2::ZERO;
                self.last_pointer_seconds = timestamp;
            }
        }

        if down
            && self.grabbed_object.is_some()
            && let (Some(cursor), Some(press_position)) = (cursor, self.grab_press_position)
        {
            let drag_distance_px = Vec2::new(
                (cursor.x - press_position.x) * aspect,
                cursor.y - press_position.y,
            )
            .length()
                * desktop_height_px.max(1.0);
            if !self.grab_active && drag_distance_px >= 4.0 {
                self.grab_active = true;
                if let Some(object_id) = self.grabbed_object {
                    self.clear_den_slot_references(object_id);
                }
            }
        }

        if self.grab_active
            && let (Some(object_id), Some(cursor)) = (self.grabbed_object, cursor)
            && let Some(object) = self
                .state
                .objects
                .iter_mut()
                .find(|object| object.id == object_id)
        {
            let radius_y =
                (object.radius_px_at_reference / desktop_height_px.max(1.0)).clamp(0.001, 0.2);
            let radius_x = radius_y / aspect;
            let clamped = (cursor + self.grab_offset).clamp(
                Vec2::new(radius_x, radius_y),
                Vec2::new(1.0 - radius_x, 1.0 - radius_y),
            );
            let dt = (timestamp - self.last_pointer_seconds).clamp(1.0 / 1_000.0, 0.1) as f32;
            let follow = 1.0 - (-28.0 * dt).exp();
            let previous_position = object.position;
            let followed = previous_position.lerp(clamped, follow);
            let followed_delta = Vec2::new(
                (followed.x - previous_position.x) * aspect,
                followed.y - previous_position.y,
            )
            .clamp_length_max(MAX_OBJECT_SPEED * dt);
            object.position =
                previous_position + Vec2::new(followed_delta.x / aspect, followed_delta.y);
            let height_velocity = Vec2::new(
                (object.position.x - previous_position.x) * aspect / dt,
                (object.position.y - previous_position.y) / dt,
            )
            .clamp_length_max(MAX_OBJECT_SPEED);
            self.drag_velocity = self.drag_velocity.lerp(height_velocity, 0.58);
            object.velocity = self.drag_velocity;
            object.lifecycle = ObjectLifecycle::GrabbedByUser;
            object.last_interaction_seconds = timestamp.max(0.0);
        }
        let touched = pressed && self.grabbed_object.is_some();
        if released
            && let Some(object_id) = self.grabbed_object
            && let Some(object) = self
                .state
                .objects
                .iter_mut()
                .find(|object| object.id == object_id)
            && self.grab_active
        {
            object.lifecycle = ObjectLifecycle::Free;
            object.velocity = self.drag_velocity.clamp_length_max(MAX_OBJECT_SPEED);
            object.familiarity = (object.familiarity + 0.015).clamp(0.0, 1.0);
            object.novelty = (object.novelty - 0.008).clamp(0.0, 1.0);
            object.wear = (object.wear + 0.001).clamp(0.0, 1.0);
        }
        if released {
            self.grabbed_object = None;
            self.grab_press_position = None;
            self.grab_offset = Vec2::ZERO;
            self.grab_active = false;
            self.drag_velocity = Vec2::ZERO;
        }
        self.pointer_down = down;
        if cursor.is_some() {
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

    /// Converts the portable habitat into the bounded, anonymous object slots
    /// expected by Thandorcat/morph. Labels and hidden object kinds never cross
    /// this boundary; the neural side receives appearance, motion, familiarity,
    /// location and an affordance bias from the already-authoritative episode.
    #[must_use]
    pub fn morph_world_input(&self, pet_position: Vec2, drives: Drives) -> MorphWorldInput {
        let active_object = self
            .director
            .active_episode()
            .and_then(|episode| episode.object_id);
        let mut visible = self
            .state
            .objects
            .iter()
            .filter(|object| {
                !matches!(
                    object.lifecycle,
                    ObjectLifecycle::Consumed | ObjectLifecycle::StoredInDen
                )
            })
            .collect::<Vec<_>>();
        visible.sort_by(|left, right| {
            morph_object_priority(
                right,
                pet_position,
                self.desktop_aspect,
                active_object,
                drives,
            )
            .total_cmp(&morph_object_priority(
                left,
                pet_position,
                self.desktop_aspect,
                active_object,
                drives,
            ))
        });

        let mut input = MorphWorldInput::default();
        for (slot, object) in visible
            .into_iter()
            .take(MORPH_OBJECT_SLOT_COUNT)
            .enumerate()
        {
            let distance = desktop_distance(object.position, pet_position, self.desktop_aspect);
            let motion = (object.velocity.length() / MAX_OBJECT_SPEED).clamp(0.0, 1.0);
            let proximity = (-distance * 3.8).exp().clamp(0.0, 1.0);
            let goal_salience = if Some(object.id) == active_object {
                0.28
            } else {
                0.0
            };
            let salience = (0.10
                + proximity * 0.28
                + object.novelty * 0.24
                + object.preference.max(0.0) * 0.18
                + motion * 0.22
                + goal_salience)
                .clamp(0.0, 1.0);
            let edge_distance = object
                .position
                .x
                .min(1.0 - object.position.x)
                .min(object.position.y.min(1.0 - object.position.y));
            input.objects[slot] = Some(MorphObjectInput {
                position: object.position,
                salience,
                motion,
                size: (object.radius_px_at_reference / 64.0).clamp(0.0, 1.0),
                roundness: 1.0,
                color_rgb: hsv_to_rgb(object.hue, object.saturation, object.value),
                state: if motion > 0.04 || object.lifecycle != ObjectLifecycle::Free {
                    1.0
                } else {
                    0.0
                },
                familiarity: object.familiarity,
                edge: (1.0 - edge_distance / 0.12).clamp(0.0, 1.0),
                luminance: object.value,
                texture: (object.glow * 0.55 + object.wear * 0.45).clamp(0.0, 1.0),
            });
            if input.selected_slot.is_none()
                && (Some(object.id) == active_object || active_object.is_none())
            {
                input.selected_slot = Some(slot as u8);
            }
        }

        if let Some(active) = self.director.active_episode()
            && let Some(slot) = input.selected_slot
            && let Some(object) = input.objects[usize::from(slot)]
        {
            let distance = desktop_distance(object.position, pet_position, self.desktop_aspect);
            input.action_biases = morph_action_biases(active.goal, distance, drives);
        }
        input
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
    fn click_capture_does_not_snap_or_cancel_existing_orb_motion() {
        let directory = tempfile::tempdir().unwrap();
        let store = StateStore::at(directory.path());
        let mut runtime = EcologyRuntime::load_or_create(&store, 77, true).unwrap();
        runtime.state.objects[0].position = Vec2::splat(0.5);
        runtime.state.objects[0].velocity = Vec2::new(0.18, -0.07);
        runtime.state.objects[0].lifecycle = ObjectLifecycle::Free;
        let radius = runtime.state.objects[0].radius_px_at_reference / 1_080.0;
        let press = Vec2::new(0.5 + radius * 0.25 / (16.0 / 9.0), 0.5);
        let position_before = runtime.state.objects[0].position;
        let velocity_before = runtime.state.objects[0].velocity;

        assert!(runtime.observe_pointer(Some(press), true, true, 16.0 / 9.0, 1_080.0, 1.0));
        assert_eq!(runtime.state.objects[0].position, position_before);
        assert_eq!(runtime.state.objects[0].velocity, velocity_before);
        runtime.observe_pointer(Some(press), false, true, 16.0 / 9.0, 1_080.0, 1.03);

        assert_eq!(runtime.state.objects[0].position, position_before);
        assert_eq!(runtime.state.objects[0].velocity, velocity_before);
        assert_eq!(runtime.state.objects[0].lifecycle, ObjectLifecycle::Free);
    }

    #[test]
    fn pointer_drag_keeps_the_orb_center_inside_radius_aware_bounds() {
        let directory = tempfile::tempdir().unwrap();
        let store = StateStore::at(directory.path());
        let mut runtime = EcologyRuntime::load_or_create(&store, 78, true).unwrap();
        runtime.state.objects[0].position = Vec2::splat(0.5);
        runtime.state.objects[0].velocity = Vec2::ZERO;
        runtime.state.objects[0].lifecycle = ObjectLifecycle::Free;
        let aspect = 16.0 / 9.0;
        let desktop_height = 1_080.0;
        let radius = runtime.state.objects[0].radius_px_at_reference / desktop_height;

        assert!(runtime.observe_pointer(
            Some(Vec2::splat(0.5)),
            true,
            true,
            aspect,
            desktop_height,
            1.0,
        ));
        runtime.observe_pointer(
            Some(Vec2::new(0.5, 1.0)),
            true,
            true,
            aspect,
            desktop_height,
            1.02,
        );
        runtime.observe_pointer(
            Some(Vec2::new(0.5, 1.0)),
            false,
            true,
            aspect,
            desktop_height,
            1.04,
        );

        assert!(runtime.state.objects[0].position.y <= 1.0 - radius + 1.0e-6);
        assert!(runtime.state.objects[0].position.y >= radius - 1.0e-6);
        assert!(runtime.state.objects[0].velocity.is_finite());
        runtime.state.validate().unwrap();
    }

    #[test]
    fn retrieve_episode_begins_a_physical_carry_without_losing_orb() {
        use lifecore::{ExpressionState, LocomotionMode, PoseIntent};

        let directory = tempfile::tempdir().unwrap();
        let store = StateStore::at(directory.path());
        let mut runtime = EcologyRuntime::load_or_create(&store, 74, true).unwrap();
        let orb_id = runtime.state.objects[0].id;
        runtime.state.objects[0].lifecycle = ObjectLifecycle::StoredInDen;
        runtime.state.objects[0].position = runtime.state.den.anchor;
        runtime.state.den.slots[0] = Some(orb_id);
        let stored_position = runtime.state.objects[0].position;
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
            EcologyResolveFrame {
                selected_action: ActionId::BringProceduralOrb,
                drives: Drives::initial(&lifecore::Genome::from_seed(78).temperament),
                sensors: &sensors,
                body: &body,
                focus_mode: false,
                dt: 0.05,
            },
        );
        assert_eq!(runtime.state.den.slots, [None; 3]);
        assert_eq!(runtime.state.objects.len(), 1);
        assert_eq!(runtime.state.objects[0].position, stored_position);
        assert_eq!(
            runtime.state.objects[0].lifecycle,
            ObjectLifecycle::CarriedByPet
        );
        runtime.state.validate().unwrap();
    }

    #[test]
    fn user_can_drag_a_stored_orb_out_of_the_den() {
        let directory = tempfile::tempdir().unwrap();
        let store = StateStore::at(directory.path());
        let mut runtime = EcologyRuntime::load_or_create(&store, 79, true).unwrap();
        let aspect = 16.0 / 9.0;
        let desktop_height = 1_080.0;
        let orb_id = runtime.state.objects[0].id;
        let anchor = runtime.state.den.anchor;
        runtime.state.objects[0].lifecycle = ObjectLifecycle::StoredInDen;
        runtime.state.objects[0].position = anchor;
        runtime.state.objects[0].velocity = Vec2::ZERO;
        runtime.state.den.slots[0] = Some(orb_id);

        assert!(runtime.hit_test(anchor, aspect, desktop_height, 0.0));
        assert!(runtime.observe_pointer(Some(anchor), true, true, aspect, desktop_height, 1.0,));
        let visible_at_drag_start = anchor;
        runtime.observe_pointer(
            Some(Vec2::new(0.50, 0.42)),
            true,
            true,
            aspect,
            desktop_height,
            1.03,
        );

        assert_eq!(runtime.state.den.slots, [None; 3]);
        assert_eq!(
            runtime.state.objects[0].lifecycle,
            ObjectLifecycle::GrabbedByUser,
        );
        let first_drag_delta = Vec2::new(
            (runtime.state.objects[0].position.x - visible_at_drag_start.x) * aspect,
            runtime.state.objects[0].position.y - visible_at_drag_start.y,
        );
        assert!(first_drag_delta.length() <= MAX_OBJECT_SPEED * 0.03 + 1.0e-5);
        runtime.observe_pointer(
            Some(Vec2::new(0.50, 0.42)),
            false,
            true,
            aspect,
            desktop_height,
            1.05,
        );
        assert_eq!(runtime.state.objects[0].lifecycle, ObjectLifecycle::Free);
        assert!(runtime.state.objects[0].position.distance(anchor) > 0.05);
        runtime.state.validate().unwrap();
    }

    #[test]
    fn released_orb_outside_den_remains_physical_and_is_not_magically_stored() {
        let directory = tempfile::tempdir().unwrap();
        let store = StateStore::at(directory.path());
        let mut runtime = EcologyRuntime::load_or_create(&store, 80, true).unwrap();
        let aspect = 16.0 / 9.0;
        let anchor = runtime.state.den.anchor;
        let inward = if anchor.x < 0.5 { 0.16 } else { -0.16 };
        runtime.state.objects[0].position = anchor + Vec2::new(inward, 0.0);
        runtime.state.objects[0].velocity = Vec2::ZERO;
        runtime.state.objects[0].lifecycle = ObjectLifecycle::Free;
        runtime.state.den.slots = [None; 3];
        let windows = WindowAffordanceFrame::default();
        let body = BodyFeedback {
            world_position: Vec2::splat(0.45),
            ..BodyFeedback::default()
        };

        for _ in 0..1_200 {
            runtime.fixed_update(aspect, &windows, &body, 1.0 / 120.0);
        }

        assert_ne!(
            runtime.state.objects[0].lifecycle,
            ObjectLifecycle::StoredInDen
        );
        assert_eq!(runtime.state.den.slots, [None; 3]);
        runtime.state.validate().unwrap();
    }

    #[test]
    fn released_orb_inside_den_is_pulled_smoothly_to_center_then_latched() {
        let directory = tempfile::tempdir().unwrap();
        let store = StateStore::at(directory.path());
        let mut runtime = EcologyRuntime::load_or_create(&store, 81, true).unwrap();
        let config = ObjectPhysicsConfig::default();
        let orb_id = runtime.state.objects[0].id;
        runtime.state.den.slots = [None; 3];
        runtime.state.objects[0].position = runtime.state.den.anchor
            + Vec2::new(
                60.0 / config.reference_height_px / config.desktop_aspect,
                0.0,
            );
        runtime.state.objects[0].velocity = Vec2::new(0.04, -0.03);
        runtime.state.objects[0].lifecycle = ObjectLifecycle::Free;
        let windows = WindowAffordanceFrame::default();
        let body = BodyFeedback {
            world_position: Vec2::splat(0.5),
            ..BodyFeedback::default()
        };
        let mut previous = runtime.state.objects[0].position;
        for _ in 0..1_200 {
            runtime.fixed_update(config.desktop_aspect, &windows, &body, 1.0 / 120.0);
            let current = runtime.state.objects[0].position;
            let step_px = Vec2::new(
                (current.x - previous.x) * config.desktop_aspect,
                current.y - previous.y,
            )
            .length()
                * config.reference_height_px;
            assert!(step_px < 8.0, "den pull stepped {step_px:.3} px");
            previous = current;
            if runtime.state.objects[0].lifecycle == ObjectLifecycle::StoredInDen {
                break;
            }
        }
        assert_eq!(runtime.state.objects[0].position, runtime.state.den.anchor);
        assert_eq!(runtime.state.objects[0].velocity, Vec2::ZERO);
        assert_eq!(
            runtime.state.objects[0].lifecycle,
            ObjectLifecycle::StoredInDen
        );
        assert_eq!(runtime.state.den.slots[0], Some(orb_id));
        runtime.state.validate().unwrap();
    }

    #[test]
    fn object_commands_cannot_leave_stale_or_duplicate_den_slots() {
        use lifecore::{ExpressionState, LocomotionMode, PoseIntent};

        let directory = tempfile::tempdir().unwrap();
        let store = StateStore::at(directory.path());
        let mut runtime = EcologyRuntime::load_or_create(&store, 76, true).unwrap();
        let orb_id = runtime.state.objects[0].id;
        let intent = BodyIntent {
            locomotion: LocomotionMode::Hover,
            target_position: Vec2::splat(0.5),
            target_surface: None,
            desired_speed: 0.0,
            facing_direction: 1.0,
            gaze_target: None,
            pose: PoseIntent::Neutral,
            expression: ExpressionState::default(),
            interaction_target: None,
        };

        let apply = |runtime: &mut EcologyRuntime, command| {
            let mut output = runtime.director.tick_passthrough(intent.clone(), false);
            output.object_commands[0] = command;
            output.object_command_count = 1;
            runtime.apply_object_commands(&output, 0.05);
        };
        let restore_stored = |runtime: &mut EcologyRuntime| {
            runtime.state.den.slots = [Some(orb_id), None, None];
            runtime.state.objects[0].lifecycle = ObjectLifecycle::StoredInDen;
            runtime.state.objects[0].position = runtime.state.den.anchor;
            runtime.state.objects[0].velocity = Vec2::ZERO;
        };

        restore_stored(&mut runtime);
        apply(
            &mut runtime,
            ObjectCommand::ApplyImpulse {
                object_id: orb_id,
                impulse: Vec2::X * 0.04,
            },
        );
        assert_eq!(runtime.state.den.slots, [None; 3]);
        runtime.state.validate().unwrap();

        restore_stored(&mut runtime);
        apply(
            &mut runtime,
            ObjectCommand::MoveToward {
                object_id: orb_id,
                target: Vec2::splat(0.6),
                speed: 2.0,
            },
        );
        assert_eq!(runtime.state.den.slots, [None; 3]);
        runtime.state.validate().unwrap();

        restore_stored(&mut runtime);
        apply(
            &mut runtime,
            ObjectCommand::Release {
                object_id: orb_id,
                velocity: Vec2::Y * 0.05,
            },
        );
        assert_eq!(runtime.state.den.slots, [None; 3]);
        runtime.state.validate().unwrap();

        restore_stored(&mut runtime);
        runtime.state.objects[0].lifecycle = ObjectLifecycle::CarriedByPet;
        let config = ObjectPhysicsConfig::default();
        let physical_handoff_position = runtime.state.den.anchor
            + Vec2::new(
                60.0 / config.reference_height_px / config.desktop_aspect,
                0.0,
            );
        runtime.state.objects[0].position = physical_handoff_position;
        apply(
            &mut runtime,
            ObjectCommand::Store {
                object_id: orb_id,
                slot: 2,
            },
        );
        assert_eq!(runtime.state.den.slots, [None; 3]);
        assert_eq!(runtime.state.objects[0].lifecycle, ObjectLifecycle::Free);
        assert_eq!(runtime.state.objects[0].position, physical_handoff_position);
        let windows = WindowAffordanceFrame::default();
        let body = BodyFeedback {
            world_position: Vec2::splat(0.5),
            ..BodyFeedback::default()
        };
        for _ in 0..1_200 {
            runtime.fixed_update(config.desktop_aspect, &windows, &body, 1.0 / 120.0);
            if runtime.state.objects[0].lifecycle == ObjectLifecycle::StoredInDen {
                break;
            }
        }
        assert_eq!(runtime.state.den.slots, [None, None, Some(orb_id)]);
        assert_eq!(runtime.state.objects[0].position, runtime.state.den.anchor);
        runtime.state.validate().unwrap();
    }

    #[test]
    fn fixed_update_releases_orb_pinned_between_pet_and_top_edge() {
        let directory = tempfile::tempdir().unwrap();
        let store = StateStore::at(directory.path());
        let mut runtime = EcologyRuntime::load_or_create(&store, 75, true).unwrap();
        runtime.state.objects[0].position = Vec2::new(0.5, 0.0);
        runtime.state.objects[0].velocity = Vec2::ZERO;
        runtime.state.objects[0].lifecycle = ObjectLifecycle::Free;
        let body = BodyFeedback {
            world_position: Vec2::new(0.5, 0.01),
            ..BodyFeedback::default()
        };
        let windows = WindowAffordanceFrame::default();

        for _ in 0..240 {
            runtime.fixed_update(16.0 / 9.0, &windows, &body, 1.0 / 120.0);
        }

        let radius = runtime.state.objects[0].radius_px_at_reference
            / ObjectPhysicsConfig::default().reference_height_px;
        assert!(runtime.state.objects[0].position.y > radius + 0.01);
        runtime.state.validate().unwrap();
    }

    #[test]
    fn resting_inside_a_window_bbox_is_not_misclassified_as_trapped() {
        use pet_ecology::{NormalizedRect, WindowAffordance, WindowId};

        let directory = tempfile::tempdir().unwrap();
        let store = StateStore::at(directory.path());
        let mut runtime = EcologyRuntime::load_or_create(&store, 79, true).unwrap();
        runtime.state.objects[0].position = Vec2::new(0.5, 0.4);
        runtime.state.objects[0].velocity = Vec2::ZERO;
        runtime.state.objects[0].lifecycle = ObjectLifecycle::Free;
        let body = BodyFeedback {
            world_position: Vec2::new(0.1, 0.1),
            ..BodyFeedback::default()
        };
        let mut windows = WindowAffordanceFrame::default();
        windows.push(WindowAffordance {
            id: WindowId(1),
            bounds: NormalizedRect {
                minimum: Vec2::ZERO,
                maximum: Vec2::ONE,
            },
            is_visible: true,
            ..WindowAffordance::default()
        });
        windows.finish();

        for _ in 0..2_400 {
            runtime.fixed_update(16.0 / 9.0, &windows, &body, 1.0 / 120.0);
        }

        assert!(!runtime.environment.orb_trapped);
        assert!(runtime.environment.orb_grounded);
        assert_eq!(
            runtime.state.objects[0].lifecycle,
            ObjectLifecycle::Sleeping
        );
        runtime.state.validate().unwrap();
    }
}
