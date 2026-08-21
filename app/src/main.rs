#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]

mod vita_runtime;

use std::{
    collections::BTreeMap,
    env,
    error::Error,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use desktop_host::{
    DisplayTopology, EventLogEntry, MonitorId, MonitorInfo, PORTABLE_STATE_SCHEMA_VERSION,
    PersistedPetPosition, PhysicalDesktopPoint, PlatformBackend, PointerState, PortablePetState,
    RectI, SensorNormalizer, StateStore, create_platform_backend,
    prepare_overlay_window_attributes,
};
use glam::Vec2;
use lifecore::{
    ActionId, BodyIntent, DebugState, ExpressionState, FeedbackEvent, Genome, LIFECORE_HZ,
    LifeCore, LocomotionMode, PoseIntent, SensorFrame, VitaOutput, stable_hash_bytes,
};
use pet_audio::AudioEngine;
use pet_body::{ProceduralBody, RenderOutcome, Renderer, VoiceVisualState};
use vita_runtime::VitaRuntime;
use winit::{
    application::ApplicationHandler,
    dpi::{LogicalSize, PhysicalPosition},
    event::{DeviceEvent, DeviceId, ElementState, MouseButton, MouseScrollDelta, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, DeviceEvents, EventLoop},
    keyboard::{KeyCode, ModifiersState, PhysicalKey},
    window::{Window, WindowId, WindowLevel},
};

const BODY_DT: f32 = 1.0 / 120.0;
const LIFE_DT: f32 = 1.0 / LIFECORE_HZ;
const ACTIVE_FRAME: Duration = Duration::from_micros(16_667);
const SLEEP_FRAME: Duration = Duration::from_micros(66_667);
const OVERLAY_LOGICAL_SIZE: f64 = 320.0;

fn main() -> Result<(), Box<dyn Error>> {
    let arguments = Arguments::parse(env::args().skip(1))?;
    if arguments.help {
        print_help();
        return Ok(());
    }
    let store = arguments
        .data_dir
        .as_ref()
        .map(StateStore::at)
        .map_or_else(StateStore::discover, Ok)?;
    if arguments.headless
        || arguments.headless_smoke_seconds.is_some()
        || arguments.simulate_hours.is_some()
    {
        return run_headless(arguments, store);
    }
    let prepared = prepare_state(&arguments, &store)?;
    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(ControlFlow::Wait);
    let mut application = PetApplication::new(arguments, store, prepared)?;
    event_loop.run_app(&mut application)?;
    Ok(())
}

#[derive(Debug, Clone, Default)]
struct Arguments {
    seed: Option<u64>,
    headless: bool,
    headless_smoke_seconds: Option<f32>,
    simulate_hours: Option<f32>,
    export_state: Option<PathBuf>,
    import_state: Option<PathBuf>,
    data_dir: Option<PathBuf>,
    reset_learning: bool,
    reset_pet: bool,
    evolve: bool,
    no_audio: bool,
    focus_mode: bool,
    help: bool,
}

impl Arguments {
    fn parse(arguments: impl Iterator<Item = String>) -> Result<Self, Box<dyn Error>> {
        let mut parsed = Self::default();
        let mut arguments = arguments.peekable();
        while let Some(argument) = arguments.next() {
            let value = |name: &str, arguments: &mut std::iter::Peekable<_>| {
                arguments
                    .next()
                    .ok_or_else(|| format!("{name} requires a value"))
            };
            match argument.as_str() {
                "--seed" => parsed.seed = Some(value("--seed", &mut arguments)?.parse()?),
                "--headless" => parsed.headless = true,
                "--headless-smoke" => {
                    parsed.headless_smoke_seconds =
                        Some(value("--headless-smoke", &mut arguments)?.parse()?);
                }
                "--simulate-hours" => {
                    parsed.simulate_hours =
                        Some(value("--simulate-hours", &mut arguments)?.parse()?);
                }
                "--export-state" => {
                    parsed.export_state =
                        Some(PathBuf::from(value("--export-state", &mut arguments)?));
                }
                "--import-state" => {
                    parsed.import_state =
                        Some(PathBuf::from(value("--import-state", &mut arguments)?));
                }
                "--data-dir" => {
                    parsed.data_dir = Some(PathBuf::from(value("--data-dir", &mut arguments)?));
                }
                "--reset-learning" => parsed.reset_learning = true,
                "--reset-pet" => parsed.reset_pet = true,
                "--evolve" => parsed.evolve = true,
                "--no-audio" => parsed.no_audio = true,
                "--focus-mode" => parsed.focus_mode = true,
                "--help" | "-h" => parsed.help = true,
                other => return Err(format!("unknown argument: {other}").into()),
            }
        }
        Ok(parsed)
    }
}

struct PreparedState {
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
        (LifeCore::restore(saved.life)?, saved.position, saved.vita)
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

fn run_headless(arguments: Arguments, store: StateStore) -> Result<(), Box<dyn Error>> {
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
        let vita_output = vita.think(&life.state, &sensors, &feedback, &output.body_intent, dt);
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

struct PetRuntime {
    window: Arc<Window>,
    renderer: Renderer,
    platform: Box<dyn PlatformBackend>,
    topology: DisplayTopology,
    normalizer: SensorNormalizer,
    life: LifeCore,
    vita: VitaRuntime,
    body: ProceduralBody,
    audio: Option<AudioEngine>,
    sensors: SensorFrame,
    intent: BodyIntent,
    pointer: PointerState,
    last_update: Instant,
    body_accumulator: f32,
    life_accumulator: f32,
    sensor_accumulator: f32,
    save_accumulator: f32,
    was_sleeping: bool,
    visible_after_first_frame: bool,
    modifiers: ModifiersState,
    debug_logging: bool,
    debug_accumulator: f32,
    fps_accumulator: f32,
    frames_since_fps: u32,
    fps: f32,
    brain_tick_microseconds: f64,
    last_debug: Option<DebugState>,
    last_vita: Option<VitaOutput>,
}

struct PetApplication {
    arguments: Arguments,
    store: StateStore,
    prepared: Option<PreparedState>,
    runtime: Option<PetRuntime>,
}

impl PetApplication {
    fn new(
        arguments: Arguments,
        store: StateStore,
        prepared: PreparedState,
    ) -> Result<Self, Box<dyn Error>> {
        Ok(Self {
            arguments,
            store,
            prepared: Some(prepared),
            runtime: None,
        })
    }

    fn persist(&self, runtime: &PetRuntime) {
        persist_runtime(&self.store, self.arguments.export_state.as_ref(), runtime);
    }

    fn update_runtime(&mut self, event_loop: &ActiveEventLoop) {
        let mut should_persist = false;
        let Some(runtime) = self.runtime.as_mut() else {
            return;
        };
        let now = Instant::now();
        let elapsed = (now - runtime.last_update).as_secs_f32().min(0.25);
        runtime.last_update = now;
        runtime.body_accumulator += elapsed;
        runtime.life_accumulator += elapsed;
        runtime.sensor_accumulator += elapsed;
        runtime.save_accumulator += elapsed;
        runtime.debug_accumulator += elapsed;
        runtime.fps_accumulator += elapsed;
        if runtime.fps_accumulator >= 1.0 {
            runtime.fps = runtime.frames_since_fps as f32 / runtime.fps_accumulator;
            runtime.frames_since_fps = 0;
            runtime.fps_accumulator = 0.0;
        }

        if runtime.sensor_accumulator >= 1.0 / 60.0 {
            runtime.sensor_accumulator %= 1.0 / 60.0;
            let snapshot = runtime.platform.poll_desktop(&runtime.topology);
            runtime.sensors = runtime.normalizer.normalize(
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
            runtime.pointer.pressed = false;
            runtime.pointer.released = false;
            runtime.pointer.pet_touched = false;
            if let Some(cursor) = snapshot.cursor {
                let accepts_input = hit_test_desktop_cursor(&runtime.window, &runtime.body, cursor);
                let _ = runtime
                    .platform
                    .set_cursor_hittest(&runtime.window, accepts_input);
            }
            move_overlay(
                &runtime.window,
                &runtime.topology,
                runtime.body.simulation.feedback.world_position,
            );
        }

        while runtime.body_accumulator >= BODY_DT {
            runtime.body.fixed_update(
                &runtime.life.state.genome,
                &runtime.intent,
                &runtime.sensors,
                BODY_DT,
            );
            let voice = voice_visual_state(runtime.audio.as_ref());
            runtime.body.embodied_update(
                &runtime.intent,
                &runtime.sensors,
                runtime.life.state.affect,
                voice,
                BODY_DT,
            );
            runtime.body_accumulator -= BODY_DT;
        }
        while runtime.life_accumulator >= LIFE_DT {
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
        if runtime.debug_accumulator >= 1.0 {
            runtime.debug_accumulator %= 1.0;
            if runtime.debug_logging
                && let Some(debug) = &runtime.last_debug
            {
                let _ = self.store.append_event(&EventLogEntry {
                    monotonic_seconds: runtime.normalizer.monotonic_seconds(),
                    kind: "debug_state".into(),
                    details: serde_json::json!({
                        "action": format!("{:?}", runtime.life.state.current_action),
                        "strongest_drive": format!("{:?}", debug.strongest_drive),
                        "strongest_drive_value": debug.strongest_drive_value,
                        "affect": runtime.life.state.affect,
                        "attachment": runtime.life.state.affect.attachment,
                        "attention_budget": debug.attention_budget,
                        "selected_motif": debug.selected_motif_id,
                        "predicted_action_value": debug.predicted_action_value,
                        "recent_reward": debug.recent_reward,
                        "plastic_weight_range": debug.plastic_weight_range,
                        "fps": runtime.fps,
                        "brain_tick_microseconds": runtime.brain_tick_microseconds,
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
                    }),
                });
            }
        }
        if runtime
            .audio
            .as_ref()
            .is_some_and(|audio| audio.poll_runtime_event().is_some())
        {
            runtime.audio = if self.arguments.no_audio {
                None
            } else {
                AudioEngine::try_start().ok()
            };
        }
        if runtime.save_accumulator >= 30.0 {
            runtime.save_accumulator %= 30.0;
            should_persist = true;
        }
        runtime.window.request_redraw();
        let sleeping = runtime.life.state.current_action == ActionId::Sleep;
        event_loop.set_control_flow(ControlFlow::WaitUntil(
            now + if sleeping { SLEEP_FRAME } else { ACTIVE_FRAME },
        ));
        if should_persist && let Some(runtime) = self.runtime.as_ref() {
            self.persist(runtime);
        }
    }
}

impl ApplicationHandler for PetApplication {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        event_loop.listen_device_events(DeviceEvents::Always);
        if self.runtime.is_some() {
            if let Some(runtime) = self.runtime.as_mut() {
                runtime.platform.on_resume();
            }
            return;
        }
        let Some(prepared) = self.prepared.take() else {
            event_loop.exit();
            return;
        };
        let attributes = prepare_overlay_window_attributes(
            Window::default_attributes()
                .with_title("Pet 2")
                .with_inner_size(LogicalSize::new(OVERLAY_LOGICAL_SIZE, OVERLAY_LOGICAL_SIZE))
                .with_resizable(false)
                .with_decorations(false)
                .with_transparent(true)
                .with_visible(false)
                .with_active(false)
                .with_window_level(WindowLevel::AlwaysOnTop),
        );
        let window = match event_loop.create_window(attributes) {
            Ok(window) => Arc::new(window),
            Err(error) => {
                eprintln!("could not create overlay window: {error}");
                event_loop.exit();
                return;
            }
        };
        let topology = topology_from_event_loop(event_loop, 1);
        let initial = topology.remap(&prepared.position);
        let initial_size = window.outer_size();
        window.set_outer_position(PhysicalPosition::new(
            initial.x - initial_size.width as i32 / 2,
            initial.y - initial_size.height as i32 / 2,
        ));
        let mut platform = create_platform_backend();
        if let Err(error) = platform
            .initialize(&window)
            .and_then(|_| platform.apply_overlay_policy(&window))
        {
            eprintln!("could not initialize desktop integration: {error}");
            event_loop.exit();
            return;
        }
        let body = match ProceduralBody::generate(&prepared.life.state.genome) {
            Ok(body) => body,
            Err(error) => {
                eprintln!("could not generate procedural body: {error}");
                event_loop.exit();
                return;
            }
        };
        let renderer = match pollster::block_on(Renderer::new(Arc::clone(&window), &body.mesh)) {
            Ok(renderer) => renderer,
            Err(error) => {
                eprintln!("could not initialize renderer: {error}");
                event_loop.exit();
                return;
            }
        };
        // A hidden Win32 composition surface may never become presentable, which would
        // deadlock the old "show after Presented" startup path. At this point the GPU
        // surface, transparent clear color, pipeline, and mesh are all ready, so making
        // the non-activating overlay visible cannot expose an uninitialized renderer.
        window.set_visible(true);
        let audio = if self.arguments.no_audio {
            None
        } else {
            AudioEngine::try_start().ok()
        };
        self.runtime = Some(PetRuntime {
            window,
            renderer,
            platform,
            topology,
            normalizer: SensorNormalizer::default(),
            sensors: SensorFrame::default(),
            intent: neutral_intent(),
            body,
            life: prepared.life,
            vita: prepared.vita,
            audio,
            pointer: PointerState::default(),
            last_update: Instant::now(),
            body_accumulator: 0.0,
            life_accumulator: 0.0,
            sensor_accumulator: 1.0,
            save_accumulator: 0.0,
            was_sleeping: false,
            visible_after_first_frame: false,
            modifiers: ModifiersState::empty(),
            debug_logging: false,
            debug_accumulator: 0.0,
            fps_accumulator: 0.0,
            frames_since_fps: 0,
            fps: 0.0,
            brain_tick_microseconds: 0.0,
            last_debug: None,
            last_vita: None,
        });
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        let Some(runtime) = self.runtime.as_mut() else {
            return;
        };
        if runtime.window.id() != window_id {
            return;
        }
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => runtime.renderer.resize(size),
            WindowEvent::ScaleFactorChanged { .. } => {
                runtime.renderer.resize(runtime.window.inner_size());
                runtime.topology =
                    topology_from_event_loop(event_loop, runtime.topology.revision + 1);
            }
            WindowEvent::Moved(_) => {
                runtime.topology =
                    topology_from_event_loop(event_loop, runtime.topology.revision + 1);
            }
            WindowEvent::MouseInput {
                state,
                button: MouseButton::Left,
                ..
            } => {
                let down = state == ElementState::Pressed;
                runtime.pointer.pressed = down && !runtime.pointer.down;
                runtime.pointer.released = !down && runtime.pointer.down;
                runtime.pointer.down = down;
                runtime.pointer.pet_touched = down;
                if down {
                    apply_shared_feedback(runtime, FeedbackEvent::PettingStarted);
                    runtime.save_accumulator = 30.0;
                }
            }
            WindowEvent::ModifiersChanged(modifiers) => runtime.modifiers = modifiers.state(),
            WindowEvent::KeyboardInput { event, .. }
                if event.state == ElementState::Pressed && !event.repeat =>
            {
                if runtime.modifiers.super_key() && runtime.modifiers.alt_key() {
                    match event.physical_key {
                        PhysicalKey::Code(KeyCode::KeyF) => {
                            let enabled = !runtime.life.state.focus_mode;
                            apply_shared_feedback(
                                runtime,
                                if enabled {
                                    FeedbackEvent::FocusModeEnabled
                                } else {
                                    FeedbackEvent::FocusModeDisabled
                                },
                            );
                            runtime.save_accumulator = 30.0;
                        }
                        PhysicalKey::Code(KeyCode::KeyM) => {
                            runtime.life.trigger_metamorphosis();
                            runtime.vita.note_metamorphosis();
                            if let Ok(body) = ProceduralBody::generate(&runtime.life.state.genome) {
                                runtime.renderer.replace_mesh(&body.mesh);
                                runtime.body = body;
                            }
                            runtime.save_accumulator = 30.0;
                        }
                        PhysicalKey::Code(KeyCode::KeyR) => {
                            apply_shared_feedback(runtime, FeedbackEvent::Reward(1.0));
                            runtime.save_accumulator = 30.0;
                        }
                        PhysicalKey::Code(KeyCode::KeyN) => {
                            apply_shared_feedback(runtime, FeedbackEvent::Reward(-1.0));
                            runtime.save_accumulator = 30.0;
                        }
                        PhysicalKey::Code(KeyCode::KeyD) => {
                            runtime.debug_logging = !runtime.debug_logging;
                            let _ = self.store.append_event(&EventLogEntry {
                                monotonic_seconds: runtime.normalizer.monotonic_seconds(),
                                kind: "debug_logging".into(),
                                details: serde_json::json!({
                                    "enabled": runtime.debug_logging,
                                }),
                            });
                        }
                        _ => {}
                    }
                }
            }
            WindowEvent::RedrawRequested => {
                let parameters = runtime.body.render_parameters(
                    &runtime.life.state.genome,
                    runtime.life.state.affect.arousal,
                );
                match runtime.renderer.render(parameters) {
                    RenderOutcome::Presented if !runtime.visible_after_first_frame => {
                        runtime.frames_since_fps = runtime.frames_since_fps.saturating_add(1);
                        runtime.window.set_visible(true);
                        runtime.visible_after_first_frame = true;
                    }
                    RenderOutcome::Presented => {
                        runtime.frames_since_fps = runtime.frames_since_fps.saturating_add(1);
                    }
                    RenderOutcome::OutOfMemory => {
                        persist_runtime(&self.store, self.arguments.export_state.as_ref(), runtime);
                        event_loop.exit();
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }

    fn device_event(
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
        self.update_runtime(event_loop);
    }

    fn suspended(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(runtime) = self.runtime.as_mut() {
            runtime.platform.on_suspend();
        }
    }

    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(runtime) = self.runtime.as_mut() {
            runtime.platform.shutdown();
        }
        if let Some(runtime) = self.runtime.as_ref() {
            self.persist(runtime);
        }
    }
}

fn persist_runtime(store: &StateStore, export: Option<&PathBuf>, runtime: &PetRuntime) {
    let size = runtime.window.outer_size();
    let position = runtime.topology.normalize(PhysicalDesktopPoint {
        x: runtime
            .window
            .outer_position()
            .map_or(0, |position| position.x + size.width as i32 / 2),
        y: runtime
            .window
            .outer_position()
            .map_or(0, |position| position.y + size.height as i32 / 2),
    });
    let portable = PortablePetState {
        schema_version: PORTABLE_STATE_SCHEMA_VERSION,
        life: runtime.life.snapshot(),
        vita: Some(runtime.vita.snapshot()),
        position,
    };
    if let Err(error) = store.save_state(&portable) {
        eprintln!("could not save Pet 2 state: {error}");
    }
    if let Some(path) = export
        && let Err(error) = store.export_state(&portable, path)
    {
        eprintln!("could not export Pet 2 state: {error}");
    }
}

fn apply_shared_feedback(runtime: &mut PetRuntime, event: FeedbackEvent) {
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
    BodyIntent {
        locomotion: LocomotionMode::Hover,
        target_position: Vec2::splat(0.5),
        target_surface: None,
        desired_speed: 0.08,
        facing_direction: 1.0,
        gaze_target: None,
        pose: PoseIntent::Neutral,
        expression: ExpressionState::default(),
        interaction_target: None,
    }
}

fn topology_from_event_loop(event_loop: &ActiveEventLoop, revision: u64) -> DisplayTopology {
    let primary = event_loop.primary_monitor();
    DisplayTopology::new(
        event_loop
            .available_monitors()
            .enumerate()
            .map(|(index, monitor)| {
                let position = monitor.position();
                let size = monitor.size();
                let is_primary = primary.as_ref().is_some_and(|primary| {
                    primary.position() == position && primary.size() == size
                });
                let bounds = RectI {
                    minimum: PhysicalDesktopPoint {
                        x: position.x,
                        y: position.y,
                    },
                    maximum: PhysicalDesktopPoint {
                        x: position.x.saturating_add(size.width as i32),
                        y: position.y.saturating_add(size.height as i32),
                    },
                };
                MonitorInfo {
                    id: MonitorId(format!(
                        "{}:{}:{}x{}:{}",
                        monitor.name().unwrap_or_else(|| "monitor".into()),
                        position.x,
                        size.width,
                        size.height,
                        index
                    )),
                    physical_bounds: bounds,
                    working_area: bounds,
                    scale_factor: monitor.scale_factor(),
                    primary: is_primary,
                }
            })
            .collect(),
        revision,
    )
}

fn move_overlay(window: &Window, topology: &DisplayTopology, normalized: Vec2) {
    let Some(first) = topology.monitors.first() else {
        return;
    };
    let bounds = topology
        .monitors
        .iter()
        .fold(first.physical_bounds, |bounds, monitor| RectI {
            minimum: PhysicalDesktopPoint {
                x: bounds.minimum.x.min(monitor.physical_bounds.minimum.x),
                y: bounds.minimum.y.min(monitor.physical_bounds.minimum.y),
            },
            maximum: PhysicalDesktopPoint {
                x: bounds.maximum.x.max(monitor.physical_bounds.maximum.x),
                y: bounds.maximum.y.max(monitor.physical_bounds.maximum.y),
            },
        });
    let size = window.outer_size();
    let desired_center = PhysicalDesktopPoint {
        x: bounds.minimum.x + (normalized.x.clamp(0.0, 1.0) * bounds.width() as f32).round() as i32,
        y: bounds.minimum.y
            + (normalized.y.clamp(0.0, 1.0) * bounds.height() as f32).round() as i32,
    };
    let working_area = topology
        .monitor_at(desired_center)
        .map_or(bounds, |monitor| monitor.working_area);
    let margin = 8_i32;
    let maximum_x =
        (working_area.maximum.x - size.width as i32 - margin).max(working_area.minimum.x + margin);
    let maximum_y =
        (working_area.maximum.y - size.height as i32 - margin).max(working_area.minimum.y + margin);
    let x = (desired_center.x - size.width as i32 / 2)
        .clamp(working_area.minimum.x + margin, maximum_x);
    let y = (desired_center.y - size.height as i32 / 2)
        .clamp(working_area.minimum.y + margin, maximum_y);
    window.set_outer_position(PhysicalPosition::new(x, y));
}

fn hit_test_desktop_cursor(
    window: &Window,
    body: &ProceduralBody,
    cursor: PhysicalDesktopPoint,
) -> bool {
    let Ok(position) = window.outer_position() else {
        return false;
    };
    let size = window.outer_size();
    if size.width == 0 || size.height == 0 {
        return false;
    }
    body.projected_hit_test(Vec2::new(
        (cursor.x - position.x) as f32 / size.width as f32,
        (cursor.y - position.y) as f32 / size.height as f32,
    ))
}

fn local_time_01() -> f32 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0.5, |duration| {
            (duration.as_secs() % 86_400) as f32 / 86_400.0
        })
}

fn print_help() {
    println!(
        "Pet 2\n\
         \nUSAGE: pet2 [OPTIONS]\n\
         \n  --seed N                 Create a deterministic identity\n\
         \n  --headless               Run a 60-second headless simulation\n\
         \n  --headless-smoke SECONDS Run a bounded headless smoke test\n\
         \n  --simulate-hours HOURS   Run an accelerated deterministic simulation\n\
         \n  --import-state PATH      Validate and import portable state\n\
         \n  --export-state PATH      Export portable state\n\
         \n  --data-dir PATH          Override the application data directory\n\
         \n  --reset-learning         Reset learned weights, habits, and memories\n\
         \n  --reset-pet              Start a new organism from --seed\n\
         \n  --evolve                 Trigger one bounded metamorphosis\n\
         \n  --focus-mode             Suppress unsolicited attention\n\
         \n  --no-audio               Disable the audio device\n"
    );
}
