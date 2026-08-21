#!/usr/bin/env python3
from __future__ import annotations

from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]


def replace_once(path: Path, old: str, new: str) -> None:
    text = path.read_text(encoding="utf-8")
    if new in text:
        return
    if old not in text:
        raise RuntimeError(f"expected patch anchor was not found in {path}: {old[:120]!r}")
    path.write_text(text.replace(old, new, 1), encoding="utf-8")


def patch_cargo() -> None:
    replace_once(
        ROOT / "app/Cargo.toml",
        'pet_body = { path = "../crates/pet_body" }\n',
        'pet_body = { path = "../crates/pet_body" }\npet_perception = { path = "../crates/pet_perception" }\n',
    )


def patch_vita_validation() -> None:
    path = ROOT / "crates/lifecore/src/vita.rs"
    marker = "impl VitaState {\n    #[must_use]\n    pub fn is_valid(&self) -> bool {"
    if marker in path.read_text(encoding="utf-8"):
        return
    anchor = """impl Default for VitaState {
    fn default() -> Self {
        Self {
            schema_version: VITA_STATE_SCHEMA_VERSION,
            attention: AttentionState::default(),
            appraisal: AppraisalState::default(),
            mood: MoodState::default(),
            emotions: Vec::new(),
            self_model: SelfModel::default(),
            influence: InfluencePolicy::default(),
            favorite_places: Vec::new(),
            elapsed_seconds: 0.0,
            attention_switches: 0,
        }
    }
}

"""
    validation = anchor + """impl VitaState {
    #[must_use]
    pub fn is_valid(&self) -> bool {
        let appraisal = [
            self.appraisal.novelty,
            self.appraisal.expectedness,
            self.appraisal.controllability,
            self.appraisal.goal_congruence,
            self.appraisal.social_relevance,
            self.appraisal.agency,
            self.appraisal.certainty,
            self.appraisal.threat,
        ];
        let mood = [
            self.mood.baseline_valence,
            self.mood.baseline_arousal,
            self.mood.social_openness,
            self.mood.confidence,
            self.mood.fatigue,
        ];
        self.schema_version == VITA_STATE_SCHEMA_VERSION
            && self.elapsed_seconds.is_finite()
            && self.attention.position.is_none_or(Vec2::is_finite)
            && self.attention.confidence.is_finite()
            && self.attention.commitment_remaining.is_finite()
            && self.attention.habituation.is_finite()
            && appraisal.into_iter().all(f32::is_finite)
            && mood.into_iter().all(f32::is_finite)
            && self.emotions.len() <= MAX_EMOTION_EPISODES
            && self.emotions.iter().all(|episode| {
                episode.intensity.is_finite() && episode.remaining_seconds.is_finite()
            })
            && self.self_model.previous_position.is_finite()
            && self.self_model.previous_velocity.is_finite()
            && [
                self.self_model.prediction_error,
                self.self_model.agency,
                self.self_model.uncertainty,
                self.self_model.body_schema_confidence,
                self.self_model.calibration_urge,
                self.self_model.external_force_likelihood,
            ]
            .into_iter()
            .all(f32::is_finite)
            && self.self_model.action_models.iter().all(|model| {
                model.mean_delta_position.is_finite()
                    && model.mean_delta_velocity.is_finite()
                    && model.contact_probability.is_finite()
                    && model.confidence.is_finite()
            })
            && self.influence.weights.len() == INFLUENCE_STRATEGY_COUNT
            && self
                .influence
                .weights
                .iter()
                .flatten()
                .all(|weight| weight.is_finite() && weight.abs() <= 1.501)
            && self.influence.cooldown_seconds.is_finite()
            && self.favorite_places.len() <= MAX_FAVORITE_PLACES
            && self.favorite_places.iter().all(|place| {
                place.relative_position.is_finite()
                    && place.hue.is_none_or(f32::is_finite)
                    && place.comfort_value.is_finite()
                    && place.play_value.is_finite()
                    && place.safety_value.is_finite()
            })
    }
}

"""
    replace_once(path, anchor, validation)


def patch_storage() -> None:
    path = ROOT / "crates/desktop_host/src/storage.rs"
    replace_once(
        path,
        "use lifecore::{LifeError, LifeSnapshot};",
        "use lifecore::{LifeError, LifeSnapshot, VitaState};",
    )
    replace_once(
        path,
        """pub struct PortablePetState {
    pub schema_version: u32,
    pub life: LifeSnapshot,
    pub position: PersistedPetPosition,
}
""",
        """pub struct PortablePetState {
    pub schema_version: u32,
    pub life: LifeSnapshot,
    #[serde(default, skip_serializing_if = \"Option::is_none\")]
    pub vita: Option<VitaState>,
    pub position: PersistedPetPosition,
}
""",
    )
    replace_once(
        path,
        """        self.life.validate()?;
        let normalized = self.position.normalized;
""",
        """        self.life.validate()?;
        if let Some(vita) = &self.vita
            && !vita.is_valid()
        {
            return Err(StorageError::InvalidVitaState);
        }
        let normalized = self.position.normalized;
""",
    )
    replace_once(
        path,
        """    #[error(\"portable state contains an invalid normalized position\")]
    InvalidPosition,
""",
        """    #[error(\"portable state contains an invalid normalized position\")]
    InvalidPosition,
    #[error(\"portable state contains an invalid VITA mind\")]
    InvalidVitaState,
""",
    )
    text = path.read_text(encoding="utf-8")
    text = text.replace(
        "life: life.snapshot(),\n            position:",
        "life: life.snapshot(),\n            vita: None,\n            position:",
    )
    path.write_text(text, encoding="utf-8")


def patch_main() -> None:
    path = ROOT / "app/src/main.rs"
    replace_once(
        path,
        ")]\n\nuse std::{",
        ")]\n\nmod vita_runtime;\n\nuse std::{",
    )
    replace_once(
        path,
        """    ActionId, BodyIntent, DebugState, ExpressionState, FeedbackEvent, Genome, LIFECORE_HZ,
    LifeCore, LocomotionMode, PoseIntent, SensorFrame, stable_hash_bytes,
""",
        """    ActionId, BodyIntent, DebugState, ExpressionState, FeedbackEvent, Genome, LIFECORE_HZ,
    LifeCore, LocomotionMode, PoseIntent, SensorFrame, VitaOutput, stable_hash_bytes,
""",
    )
    replace_once(
        path,
        "use pet_body::{ProceduralBody, RenderOutcome, Renderer};\n",
        "use pet_body::{ProceduralBody, RenderOutcome, Renderer, VoiceVisualState};\nuse vita_runtime::VitaRuntime;\n",
    )
    replace_once(
        path,
        """    event::{ElementState, MouseButton, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
""",
        """    event::{
        DeviceEvent, DeviceId, ElementState, MouseButton, MouseScrollDelta, WindowEvent,
    },
    event_loop::{ActiveEventLoop, ControlFlow, DeviceEvents, EventLoop},
""",
    )
    replace_once(
        path,
        """struct PreparedState {
    life: LifeCore,
    position: PersistedPetPosition,
}

fn prepare_state(
    arguments: &Arguments,
    store: &StateStore,
) -> Result<PreparedState, Box<dyn Error>> {
    let seed = arguments.seed.unwrap_or(0x5045_5432_D15C_0A57);
    let imported = arguments
        .import_state
        .as_ref()
        .map(|path| store.import_state(path))
        .transpose()?;
    let saved = if arguments.reset_pet {
        None
    } else if imported.is_some() {
        imported
    } else {
        store.load_state()?
    };
    let (mut life, position) = if let Some(saved) = saved {
        (LifeCore::restore(saved.life)?, saved.position)
    } else {
        (
            LifeCore::new(Genome::from_seed(seed), seed ^ 0xA11F_EC0A),
            PersistedPetPosition::default(),
        )
    };
    if arguments.reset_learning {
        life.reset_learning();
    }
    if arguments.evolve {
        life.trigger_metamorphosis();
    }
    life.set_focus_mode(arguments.focus_mode);
    Ok(PreparedState { life, position })
}

""",
        """struct PreparedState {
    life: LifeCore,
    position: PersistedPetPosition,
    vita: VitaRuntime,
}

fn prepare_state(
    arguments: &Arguments,
    store: &StateStore,
) -> Result<PreparedState, Box<dyn Error>> {
    let seed = arguments.seed.unwrap_or(0x5045_5432_D15C_0A57);
    let imported = arguments
        .import_state
        .as_ref()
        .map(|path| store.import_state(path))
        .transpose()?;
    let saved = if arguments.reset_pet {
        None
    } else if imported.is_some() {
        imported
    } else {
        store.load_state()?
    };
    let (mut life, position, vita_state) = if let Some(saved) = saved {
        (
            LifeCore::restore(saved.life)?,
            saved.position,
            saved.vita,
        )
    } else {
        (
            LifeCore::new(Genome::from_seed(seed), seed ^ 0xA11F_EC0A),
            PersistedPetPosition::default(),
            None,
        )
    };
    let identity_seed = life.state.genome.identity_seed;
    let mut vita = VitaRuntime::new(identity_seed, vita_state);
    if arguments.reset_learning {
        life.reset_learning();
        vita.reset_learning(identity_seed);
    }
    if arguments.evolve {
        life.trigger_metamorphosis();
        vita.note_metamorphosis();
    }
    life.set_focus_mode(arguments.focus_mode);
    Ok(PreparedState {
        life,
        position,
        vita,
    })
}

""",
    )
    start = path.read_text(encoding="utf-8")
    old_headless_start = start.index("fn run_headless(")
    old_headless_end = start.index("\nstruct PetRuntime", old_headless_start)
    old_headless = start[old_headless_start:old_headless_end]
    new_headless = """fn run_headless(arguments: Arguments, store: StateStore) -> Result<(), Box<dyn Error>> {
    let PreparedState {
        mut life,
        position,
        mut vita,
    } = prepare_state(&arguments, &store)?;
    let mut body = ProceduralBody::generate(&life.state.genome)?;
    let duration_seconds = arguments
        .simulate_hours
        .map(|hours| hours.max(0.0) * 3_600.0)
        .or(arguments.headless_smoke_seconds)
        .unwrap_or(60.0)
        .max(0.05);
    let dt = if arguments.simulate_hours.is_some() {
        0.25
    } else {
        LIFE_DT
    };
    let tick_count = (duration_seconds / dt).ceil() as u64;
    let mut sensors = SensorFrame::default();
    let mut feedback = body.simulation.feedback.clone();
    let mut action_counts = BTreeMap::<String, u64>::new();
    let feedback_interval = (5.0 / dt).round().max(1.0) as u64;
    for tick in 0..tick_count {
        let time = tick as f32 * dt;
        sensors.timestamp = f64::from(time);
        sensors.time_of_day_01 = (time / 86_400.0).fract();
        sensors.cursor_position = Vec2::new(
            0.5 + 0.31 * (time * 0.37).cos(),
            0.5 + 0.23 * (time * 0.29).sin(),
        );
        sensors.cursor_velocity = Vec2::new(
            -0.31 * 0.37 * (time * 0.37).sin(),
            0.23 * 0.29 * (time * 0.29).cos(),
        );
        sensors.cursor_distance_to_pet = sensors.cursor_position.distance(feedback.world_position);
        sensors.user_idle_seconds = 3.0 + 15.0 * (time * 0.013).sin().abs();
        sensors.user_activity_rate = (1.0 - sensors.user_idle_seconds / 30.0).clamp(0.0, 1.0);
        if tick % 11 == 0 {
            vita.note_key_activity(sensors.timestamp);
        }
        if tick % 97 == 0 {
            vita.note_scroll((time * 0.7).sin());
        }
        vita.observe(&sensors, &feedback, dt);
        let mut output = life.tick(&sensors, &feedback, dt);
        let vita_output = vita.think(
            &life.state,
            &sensors,
            &feedback,
            &output.body_intent,
            dt,
        );
        vita_output.apply_to_intent(&mut output.body_intent);
        *action_counts
            .entry(format!("{:?}", output.selected_action))
            .or_default() += 1;
        body.fixed_update(
            &life.state.genome,
            &output.body_intent,
            &sensors,
            dt.min(1.0 / 30.0),
        );
        body.embodied_update(
            &output.body_intent,
            &sensors,
            output.affect,
            VoiceVisualState::default(),
            dt.min(0.05),
        );
        feedback = body.simulation.feedback.clone();
        if output.selected_action == ActionId::Sleep && tick % 1_200 == 0 {
            life.consolidate_sleep();
        }
        if tick > 0 && tick % feedback_interval == 0 {
            let interaction = tick / feedback_interval;
            let event = if interaction.is_multiple_of(3) {
                FeedbackEvent::Ignored
            } else {
                FeedbackEvent::PettingStarted
            };
            vita.apply_feedback(&event);
            life.apply_feedback(event);
        }
    }
    let portable = PortablePetState {
        schema_version: PORTABLE_STATE_SCHEMA_VERSION,
        life: life.snapshot(),
        vita: Some(vita.snapshot()),
        position,
    };
    portable.validate()?;
    let bytes = serde_json::to_vec(&portable)?;
    let summary = serde_json::json!({
        "schema_version": portable.schema_version,
        "simulated_seconds": duration_seconds,
        "ticks": tick_count,
        "generation": life.state.genome.generation,
        "life_hash": stable_hash_bytes(&bytes),
        "genome_hash": life.state.genome.stable_hash(),
        "mesh_hash": body.mesh.stable_hash(),
        "current_action": format!("{:?}", life.state.current_action),
        "action_counts": action_counts,
        "drives": life.state.drives,
        "affect": life.state.affect,
        "vita_attention": format!("{:?}", vita.state().attention.kind),
        "vita_agency": vita.state().self_model.agency,
        "vita_uncertainty": vita.state().self_model.uncertainty,
        "vita_calibration_urge": vita.state().self_model.calibration_urge,
        "vita_attention_switches": vita.state().attention_switches,
    });
    println!("{}", serde_json::to_string_pretty(&summary)?);
    store.save_state(&portable)?;
    if let Some(path) = &arguments.export_state {
        store.export_state(&portable, path)?;
    }
    Ok(())
}
"""
    path.write_text(start[:old_headless_start] + new_headless + start[old_headless_end:], encoding="utf-8")

    replace_once(
        path,
        """    life: LifeCore,
    body: ProceduralBody,
""",
        """    life: LifeCore,
    vita: VitaRuntime,
    body: ProceduralBody,
""",
    )
    replace_once(
        path,
        """    brain_tick_microseconds: f64,
    last_debug: Option<DebugState>,
""",
        """    brain_tick_microseconds: f64,
    last_debug: Option<DebugState>,
    last_vita: Option<VitaOutput>,
""",
    )
    replace_once(
        path,
        """            runtime.sensors = runtime.normalizer.normalize(
                &snapshot,
                &runtime.topology,
                runtime.body.simulation.feedback.world_position,
                runtime.pointer,
                local_time_01(),
            );
""",
        """            runtime.sensors = runtime.normalizer.normalize(
                &snapshot,
                &runtime.topology,
                runtime.body.simulation.feedback.world_position,
                runtime.pointer,
                local_time_01(),
            );
            runtime.vita.observe(
                &runtime.sensors,
                &runtime.body.simulation.feedback,
                1.0 / 60.0,
            );
""",
    )
    replace_once(
        path,
        """            runtime.body.animation_update(
                &runtime.intent,
                runtime.life.state.affect.arousal,
                BODY_DT,
            );
""",
        """            let voice = voice_visual_state(runtime.audio.as_ref());
            runtime.body.embodied_update(
                &runtime.intent,
                &runtime.sensors,
                runtime.life.state.affect,
                voice,
                BODY_DT,
            );
""",
    )
    text = path.read_text(encoding="utf-8")
    loop_start = text.index("        while runtime.life_accumulator >= LIFE_DT {")
    loop_end = text.index("        if runtime.debug_accumulator >= 1.0 {", loop_start)
    new_loop = """        while runtime.life_accumulator >= LIFE_DT {
            let tick_started = Instant::now();
            let mut output =
                runtime
                    .life
                    .tick(&runtime.sensors, &runtime.body.simulation.feedback, LIFE_DT);
            let vita_output = runtime.vita.think(
                &runtime.life.state,
                &runtime.sensors,
                &runtime.body.simulation.feedback,
                &output.body_intent,
                LIFE_DT,
            );
            vita_output.apply_to_intent(&mut output.body_intent);
            runtime.brain_tick_microseconds = tick_started.elapsed().as_secs_f64() * 1_000_000.0;
            runtime.last_debug = Some(output.debug.clone());
            runtime.last_vita = Some(vita_output);
            runtime.intent = output.body_intent;
            let sleeping = output.selected_action == ActionId::Sleep;
            if sleeping && !runtime.was_sleeping {
                runtime.life.consolidate_sleep();
                runtime.save_accumulator = 30.0;
            }
            runtime.was_sleeping = sleeping;
            if let (Some(audio), Some(request)) = (&runtime.audio, output.vocal_request)
                && let Some(motif) = runtime
                    .life
                    .state
                    .vocal_motifs
                    .iter()
                    .find(|motif| motif.id == request.motif_id)
            {
                let _ = audio.enqueue(&runtime.life.state.genome.voice, motif, &request);
            }
            runtime.life_accumulator -= LIFE_DT;
        }
"""
    path.write_text(text[:loop_start] + new_loop + text[loop_end:], encoding="utf-8")

    replace_once(
        path,
        """                        "brain_tick_microseconds": runtime.brain_tick_microseconds,
""",
        """                        "brain_tick_microseconds": runtime.brain_tick_microseconds,
                        "vita_attention": runtime.last_vita.as_ref().map(|output| format!("{:?}", output.attention.kind)),
                        "vita_emotion": runtime.last_vita.as_ref().and_then(|output| output.dominant_emotion).map(|emotion| format!("{:?}", emotion.kind)),
                        "vita_influence": runtime.last_vita.as_ref().and_then(|output| output.influence.as_ref()).map(|decision| format!("{:?}", decision.strategy)),
                        "vita_agency": runtime.vita.state().self_model.agency,
                        "vita_prediction_error": runtime.vita.state().self_model.prediction_error,
                        "vita_uncertainty": runtime.vita.state().self_model.uncertainty,
                        "vita_calibration_urge": runtime.vita.state().self_model.calibration_urge,
                        "typing_rate_hz": runtime.vita.percept().typing_rate_hz,
                        "scroll_velocity": runtime.vita.percept().scroll_velocity,
                        "window_pressure": runtime.vita.percept().window_pressure,
""",
    )
    replace_once(
        path,
        """    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.runtime.is_some() {
""",
        """    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        event_loop.listen_device_events(DeviceEvents::Always);
        if self.runtime.is_some() {
""",
    )
    replace_once(
        path,
        """            body,
            life: prepared.life,
            audio,
""",
        """            body,
            life: prepared.life,
            vita: prepared.vita,
            audio,
""",
    )
    replace_once(
        path,
        """            brain_tick_microseconds: 0.0,
            last_debug: None,
""",
        """            brain_tick_microseconds: 0.0,
            last_debug: None,
            last_vita: None,
""",
    )
    replace_once(
        path,
        """                if down {
                    runtime.life.apply_feedback(FeedbackEvent::PettingStarted);
                    runtime.save_accumulator = 30.0;
                }
""",
        """                if down {
                    apply_shared_feedback(runtime, FeedbackEvent::PettingStarted);
                    runtime.save_accumulator = 30.0;
                }
""",
    )
    replace_once(
        path,
        """                            runtime.life.apply_feedback(if enabled {
                                FeedbackEvent::FocusModeEnabled
                            } else {
                                FeedbackEvent::FocusModeDisabled
                            });
""",
        """                            apply_shared_feedback(
                                runtime,
                                if enabled {
                                    FeedbackEvent::FocusModeEnabled
                                } else {
                                    FeedbackEvent::FocusModeDisabled
                                },
                            );
""",
    )
    replace_once(
        path,
        """                            runtime.life.trigger_metamorphosis();
                            if let Ok(body) = ProceduralBody::generate(&runtime.life.state.genome) {
""",
        """                            runtime.life.trigger_metamorphosis();
                            runtime.vita.note_metamorphosis();
                            if let Ok(body) = ProceduralBody::generate(&runtime.life.state.genome) {
""",
    )
    replace_once(
        path,
        "runtime.life.apply_feedback(FeedbackEvent::Reward(1.0));",
        "apply_shared_feedback(runtime, FeedbackEvent::Reward(1.0));",
    )
    replace_once(
        path,
        "runtime.life.apply_feedback(FeedbackEvent::Reward(-1.0));",
        "apply_shared_feedback(runtime, FeedbackEvent::Reward(-1.0));",
    )
    replace_once(
        path,
        """    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
""",
        """    fn device_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _device_id: DeviceId,
        event: DeviceEvent,
    ) {
        let Some(runtime) = self.runtime.as_mut() else {
            return;
        };
        let timestamp = runtime.normalizer.monotonic_seconds();
        match event {
            DeviceEvent::Key(event) if event.state == ElementState::Pressed => {
                runtime.vita.note_key_activity(timestamp);
            }
            DeviceEvent::MouseWheel { delta } => {
                runtime.vita.note_scroll(normalized_scroll(delta));
            }
            DeviceEvent::Button {
                state: ElementState::Pressed,
                ..
            } => {
                runtime.vita.note_click(timestamp);
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
""",
    )
    replace_once(
        path,
        """        life: runtime.life.snapshot(),
        position,
""",
        """        life: runtime.life.snapshot(),
        vita: Some(runtime.vita.snapshot()),
        position,
""",
    )
    replace_once(
        path,
        """fn neutral_intent() -> BodyIntent {
""",
        """fn apply_shared_feedback(runtime: &mut PetRuntime, event: FeedbackEvent) {
    runtime.vita.apply_feedback(&event);
    runtime.life.apply_feedback(event);
}

fn voice_visual_state(audio: Option<&AudioEngine>) -> VoiceVisualState {
    let feedback = audio.map(AudioEngine::visual_feedback).unwrap_or_default();
    VoiceVisualState {
        active: feedback.active,
        motif_id: feedback.motif_id,
        syllable_index: feedback.syllable_index,
        envelope: feedback.envelope,
        mouth_open: feedback.mouth_open,
        pitch_normalized: feedback.pitch_normalized,
        noisiness: feedback.noisiness,
        purr: feedback.purr,
    }
}

fn normalized_scroll(delta: MouseScrollDelta) -> f32 {
    match delta {
        MouseScrollDelta::LineDelta(_, vertical) => (vertical / 6.0).clamp(-1.0, 1.0),
        MouseScrollDelta::PixelDelta(position) => (position.y as f32 / 360.0).clamp(-1.0, 1.0),
    }
}

fn neutral_intent() -> BodyIntent {
""",
    )


def main() -> None:
    patch_cargo()
    patch_vita_validation()
    patch_storage()
    patch_main()


if __name__ == "__main__":
    main()
