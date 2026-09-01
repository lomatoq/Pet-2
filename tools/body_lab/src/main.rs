use std::{
    collections::{BTreeSet, VecDeque},
    env,
    fs::{self, File},
    io::{BufRead, BufReader, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    process::Command,
    sync::Arc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use desktop_host::{
    LAB_CONTROL_SCHEMA_VERSION, LabControlCommand, LabControlEnvelope, LabDrive, StateStore,
};
use egui::{CollapsingHeader, Context, DragValue, Sense, Slider};
use egui_wgpu::{Renderer as EguiRenderer, RendererOptions, ScreenDescriptor, wgpu};
use egui_winit::State as EguiWinitState;
use glam::Vec2;
use lifecore::{
    AffectState, BodyIntent, EmotionKind, ExpressionState, Genome, InteractionTarget,
    LocomotionMode, PoseIntent, SensorFrame, VocalRequest, apply_emotion_to_expression,
    generate_initial_motifs,
};
use pet_audio::AudioEngine;
use pet_body::{
    BodyRenderMode, DebugView, LiquidDiagnostics, LiquidTuningAcknowledgement, LiquidTuningProfile,
    MaterialVariant, ProceduralBody, RenderOutcome, Renderer, ReviewBackground, VisualMindInput,
    VoiceVisualState,
};
use serde_json::Value;
use winit::{
    application::ApplicationHandler,
    dpi::{LogicalPosition, LogicalSize, PhysicalSize},
    event::{ElementState, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    keyboard::{KeyCode, PhysicalKey},
    window::{Window, WindowId},
};

const FRAME: Duration = Duration::from_micros(16_667);
const SEED: u64 = 42;
const DEFAULT_BACKGROUND: usize = 3;
const DEFAULT_PREVIEW_BOUNDS_MIN: Vec2 = Vec2::new(0.0, 0.035);
const DEFAULT_PREVIEW_BOUNDS_MAX: Vec2 = Vec2::new(0.67, 0.965);
const USER_PRESET_DIRECTORY: &str = "liquid-presets";
const LIVE_FRAME_HISTORY_CAP: usize = 3_600;
const LIVE_FRAME_HISTORY_BYTES: usize = 48 * 1_024 * 1_024;
const LIVE_BOOTSTRAP_BYTES: u64 = 24 * 1_024 * 1_024;
const EXPECTED_TELEMETRY_HZ: f32 = 5.0;
const LAB_CONTROL_ACK_TIMEOUT: Duration = Duration::from_secs(3);
const BACKGROUNDS: [ReviewBackground; 6] = [
    ReviewBackground::Black,
    ReviewBackground::White,
    ReviewBackground::Gray,
    ReviewBackground::BusyChecker,
    ReviewBackground::Warm,
    ReviewBackground::Cold,
];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut live_pet = false;
    let mut data_dir = None;
    let mut arguments = env::args().skip(1);
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--live-pet" => live_pet = true,
            "--data-dir" => {
                data_dir = Some(PathBuf::from(
                    arguments.next().ok_or("--data-dir requires a path")?,
                ));
            }
            "--help" | "-h" => {
                println!(
                    "Pet2 Dev Console\n\n  --live-pet       Start on Perception with live telemetry\n  --data-dir PATH  Use the same overridden Pet 2 data directory\n\n  F1–F4 switch Character, Perception, Behavior, and Diagnostics"
                );
                return Ok(());
            }
            _ => return Err(format!("unknown Pet2 Dev Console option: {argument}").into()),
        }
    }
    // One console owns both the authored character preview and live telemetry.
    let store = data_dir
        .as_deref()
        .map(StateStore::at)
        .or_else(|| StateStore::discover().ok());
    let event_loop = EventLoop::new()?;
    event_loop.run_app(&mut BodyLab::new(SEED, store, live_pet))?;
    Ok(())
}

struct BodyLab {
    genome: Genome,
    store: Option<StateStore>,
    start_live: bool,
    runtime: Option<LabRuntime>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DevPanel {
    Character,
    Perception,
    Behavior,
    Diagnostics,
}

impl DevPanel {
    const ALL: [Self; 4] = [
        Self::Character,
        Self::Perception,
        Self::Behavior,
        Self::Diagnostics,
    ];

    const fn label(self) -> &'static str {
        match self {
            Self::Character => "Character",
            Self::Perception => "Perception",
            Self::Behavior => "Behavior",
            Self::Diagnostics => "Diagnostics",
        }
    }

    const fn shortcut(self) -> &'static str {
        match self {
            Self::Character => "F1",
            Self::Perception => "F2",
            Self::Behavior => "F3",
            Self::Diagnostics => "F4",
        }
    }

    const fn is_live(self) -> bool {
        !matches!(self, Self::Character)
    }
}

struct LabRuntime {
    window: Arc<Window>,
    renderer: Renderer,
    body: ProceduralBody,
    egui_context: Context,
    egui_state: EguiWinitState,
    egui_renderer: EguiRenderer,
    ui: LabUi,
    last_update: Instant,
    next_frame: Instant,
    elapsed: f32,
    fps_window_started: Instant,
    last_present: Instant,
    fps_frames: u32,
    fps: f32,
    frame_times_ms: [f32; 120],
    frame_time_count: usize,
    frame_time_cursor: usize,
    p95_frame_ms: f32,
    simulation_accumulator: f32,
    audio: Option<AudioEngine>,
    audio_error: Option<String>,
    voice_preview_counter: u64,
    panel: DevPanel,
    live_monitor: Option<LivePetMonitor>,
}

struct LivePetMonitor {
    control_store: StateStore,
    telemetry_path: PathBuf,
    telemetry_previous_path: PathBuf,
    legacy_path: PathBuf,
    using_legacy: bool,
    previous_loaded: bool,
    current_bootstrapped: bool,
    offset: u64,
    frames: VecDeque<Value>,
    frame_sizes: VecDeque<usize>,
    frame_history_bytes: usize,
    history: VecDeque<LiveGraphSample>,
    selected: usize,
    follow_live: bool,
    playing: bool,
    playback_rate: f32,
    playback_accumulator: f32,
    last_playback_tick: Instant,
    control_drive: LabDrive,
    control_drive_delta: f32,
    control_drive_duration: f32,
    control_attention_duration: f32,
    control_reward: f32,
    control_counter: u64,
    last_sent_command_id: u64,
    pending_command_id: Option<u64>,
    pending_command_sent_at: Option<Instant>,
    control_status: String,
    evolution_scrub: EvolutionScrubState,
    canonical_evolution: CanonicalEvolutionCache,
    last_state_poll: Instant,
    state_file_signature: Option<(SystemTime, u64)>,
    status: String,
    last_poll: Instant,
    last_received: Option<Instant>,
}

#[derive(Clone, Copy, Debug, Default)]
struct LiveGraphSample {
    speed_px: f32,
    pressure: f32,
    deformation: f32,
    visual_blend: f32,
    orb_speed: f32,
}

#[derive(Debug, Default)]
struct EvolutionScrubState {
    history_key: String,
    selected: usize,
}

#[derive(Clone, Copy)]
enum EvolutionCheckpoint<'a> {
    Before {
        record_index: usize,
        record: &'a Value,
        genome: &'a Value,
    },
    After {
        record_index: usize,
        record: &'a Value,
        genome: Option<&'a Value>,
    },
}

#[derive(Debug, Default)]
struct CanonicalEvolutionCache {
    available: bool,
    lineage_id: String,
    generation: u32,
    history: Vec<Value>,
    status: String,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum PreviewScenario {
    #[default]
    Calm,
    FluidFlight,
    ConstantVelocity,
    HardStop,
    DirectionReversal,
    Impulse,
    DetachAndRemerge,
    WindowPressure,
}

struct LabUi {
    profile: LiquidTuningProfile,
    authored_profile: LiquidTuningProfile,
    store: Option<StateStore>,
    preset_directory: Option<PathBuf>,
    user_presets: Vec<UserPreset>,
    selected_preset: PresetSelection,
    preset_name_buffer: String,
    delete_preset_armed: bool,
    profile_path: String,
    json_buffer: String,
    status: String,
    scenario: PreviewScenario,
    background: usize,
    playing: bool,
    single_step: bool,
    reset_requested: bool,
    debug_view: DebugView,
    time_scale: f32,
    drag: PreviewDrag,
    preview_bounds_min: Vec2,
    preview_bounds_max: Vec2,
    scene_luminance: f32,
    focus_distance: f32,
    emotional_arousal: f32,
    interest: f32,
    preview_emotion: Option<EmotionKind>,
    preview_emotion_intensity: f32,
    focus_lock: bool,
    pending_revision: Option<u64>,
    next_ack_poll: Instant,
    test_voice_requested: bool,
    audio_status: String,
    audio_rms: f32,
    audio_peak: f32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum PresetSelection {
    AuthoredCurrent,
    MoonlitGlass,
    User(String),
}

#[derive(Debug, Clone)]
struct UserPreset {
    id: String,
    display_name: String,
    path: PathBuf,
    profile: LiquidTuningProfile,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct PreviewDrag {
    pointer_normalized: Vec2,
    velocity_normalized: Vec2,
    acceleration_normalized: Vec2,
    active: bool,
    pressed: bool,
    released: bool,
}

impl Default for PreviewDrag {
    fn default() -> Self {
        Self {
            pointer_normalized: Vec2::splat(0.5),
            velocity_normalized: Vec2::ZERO,
            acceleration_normalized: Vec2::ZERO,
            active: false,
            pressed: false,
            released: false,
        }
    }
}

impl BodyLab {
    fn new(seed: u64, store: Option<StateStore>, start_live: bool) -> Self {
        Self {
            genome: Genome::from_seed(seed),
            store,
            start_live,
            runtime: None,
        }
    }
}

impl ApplicationHandler for BodyLab {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.runtime.is_some() {
            return;
        }
        let window = match event_loop.create_window(
            Window::default_attributes()
                .with_title("Pet2 Dev Console")
                .with_inner_size(LogicalSize::new(1_180.0, 880.0))
                .with_min_inner_size(LogicalSize::new(880.0, 640.0))
                .with_position(LogicalPosition::new(32.0, 48.0))
                .with_resizable(true),
        ) {
            Ok(window) => Arc::new(window),
            Err(error) => {
                eprintln!("could not create Pet2 Dev Console window: {error}");
                event_loop.exit();
                return;
            }
        };
        let mut body = match ProceduralBody::generate(&self.genome) {
            Ok(body) => body,
            Err(error) => {
                eprintln!("could not generate liquid body: {error}");
                event_loop.exit();
                return;
            }
        };
        let store = self.store.clone().or_else(|| StateStore::discover().ok());
        let mut profile = LiquidTuningProfile::for_seed(self.genome.identity_seed);
        let mut migrated_material_preview = false;
        if let Some(saved) = store
            .as_ref()
            .and_then(|store| store.load_liquid_tuning::<LiquidTuningProfile>().ok())
            .flatten()
        {
            migrated_material_preview = (6..=8).contains(&saved.schema_version);
            if let Ok(saved) = saved.sanitized() {
                profile = saved;
            }
        }
        // Production migration is deliberately Current / Safe. Dev Console is the
        // opt-in experiment surface, so the first launch after schema migration
        // opens Cinematic Jelly while keeping the incumbent one click away.
        if migrated_material_preview {
            profile.material.variant = MaterialVariant::CinematicJelly;
            // The v1 glow discs used a different size/intensity meaning and are
            // the oversized central spheres rejected in review. Preserve every
            // authored physics/material control, but seed the new v2 inclusions
            // from their own calibrated defaults on first migrated preview.
            let defaults = pet_body::MaterialTuning::default();
            profile.material.internal_orb_count = defaults.internal_orb_count;
            profile.material.internal_orb_intensity = defaults.internal_orb_intensity;
            profile.material.internal_orb_size = defaults.internal_orb_size;
        }
        // This lab is deliberately one system: the soft-field particle liquid.
        // Legacy analytic remains a production rollback path, not a second preview
        // that steals unrelated sliders.
        profile.render_mode = BodyRenderMode::ParticlePbf;
        let authored_profile = profile.clone();
        let preset_directory = store
            .as_ref()
            .map(|store| store.paths.root.join(USER_PRESET_DIRECTORY));
        let (user_presets, preset_status) = match preset_directory.as_deref() {
            Some(directory) => match discover_user_presets(directory) {
                Ok((presets, skipped)) if skipped > 0 => (
                    presets,
                    Some(format!(
                        "Loaded user presets; skipped {skipped} invalid file(s)."
                    )),
                ),
                Ok((presets, _)) => (presets, None),
                Err(error) => (
                    Vec::new(),
                    Some(format!("Could not scan user presets: {error}")),
                ),
            },
            None => (
                Vec::new(),
                Some("Pet data directory is unavailable; user presets are disabled.".to_owned()),
            ),
        };
        let _ = body.apply_tuning_profile(profile.clone());
        body.embodiment.liquid.set_navigation_anchor_strength(0.0);
        sync_preview_viewport(
            &mut body,
            window.inner_size(),
            DEFAULT_PREVIEW_BOUNDS_MIN,
            DEFAULT_PREVIEW_BOUNDS_MAX,
        );
        let mut renderer = match pollster::block_on(Renderer::new(Arc::clone(&window), &body.mesh))
        {
            Ok(renderer) => renderer,
            Err(error) => {
                eprintln!("could not initialize liquid renderer: {error}");
                event_loop.exit();
                return;
            }
        };
        renderer.set_review_background(BACKGROUNDS[DEFAULT_BACKGROUND]);
        let egui_context = Context::default();
        egui_context.set_visuals(egui::Visuals::dark());
        let egui_state = EguiWinitState::new(
            egui_context.clone(),
            egui::ViewportId::ROOT,
            window.as_ref(),
            Some(window.scale_factor() as f32),
            window.theme(),
            None,
        );
        let egui_renderer = EguiRenderer::new(
            renderer.device(),
            renderer.surface_format(),
            RendererOptions::default(),
        );
        let profile_path = store.as_ref().map_or_else(
            || "liquid-tuning.json".to_owned(),
            |store| store.paths.liquid_tuning.display().to_string(),
        );
        let now = Instant::now();
        let (audio, audio_error) = match AudioEngine::try_start() {
            Ok(engine) => (Some(engine), None),
            Err(error) => (None, Some(error.to_string())),
        };
        let live_monitor = store.as_ref().map(LivePetMonitor::new);
        let ui = LabUi {
            json_buffer: serde_json::to_string_pretty(&profile).unwrap_or_default(),
            preset_name_buffer: format!("{} copy", profile.name),
            profile,
            authored_profile,
            store,
            preset_directory,
            user_presets,
            selected_preset: PresetSelection::AuthoredCurrent,
            delete_preset_armed: false,
            profile_path,
            status: preset_status.unwrap_or_else(|| {
                "Authored current preset loaded. Every visible control edits this preview."
                    .to_owned()
            }),
            scenario: PreviewScenario::Calm,
            background: DEFAULT_BACKGROUND,
            playing: true,
            single_step: false,
            reset_requested: false,
            debug_view: DebugView::Material,
            time_scale: 1.0,
            drag: PreviewDrag::default(),
            preview_bounds_min: DEFAULT_PREVIEW_BOUNDS_MIN,
            preview_bounds_max: DEFAULT_PREVIEW_BOUNDS_MAX,
            scene_luminance: 0.5,
            focus_distance: 0.5,
            emotional_arousal: 0.25,
            interest: 0.45,
            preview_emotion: None,
            preview_emotion_intensity: 1.0,
            focus_lock: false,
            pending_revision: None,
            next_ack_poll: now,
            test_voice_requested: false,
            audio_status: audio.as_ref().map_or_else(
                || "Failed".to_owned(),
                |engine| {
                    format!(
                        "Ready · {} · {:?}",
                        engine.device_name(),
                        engine.selected_config()
                    )
                },
            ),
            audio_rms: 0.0,
            audio_peak: 0.0,
        };
        self.runtime = Some(LabRuntime {
            window,
            renderer,
            body,
            egui_context,
            egui_state,
            egui_renderer,
            ui,
            last_update: now,
            next_frame: now,
            elapsed: 0.0,
            fps_window_started: now,
            last_present: now,
            fps_frames: 0,
            fps: 0.0,
            frame_times_ms: [0.0; 120],
            frame_time_count: 0,
            frame_time_cursor: 0,
            p95_frame_ms: 0.0,
            simulation_accumulator: 0.0,
            audio,
            audio_error,
            voice_preview_counter: 0,
            panel: if self.start_live {
                DevPanel::Perception
            } else {
                DevPanel::Character
            },
            live_monitor,
        });
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let Some(runtime) = self.runtime.as_mut() else {
            return;
        };
        let now = Instant::now();
        if now < runtime.next_frame {
            event_loop.set_control_flow(ControlFlow::WaitUntil(runtime.next_frame));
            return;
        }
        let wall_dt = (now - runtime.last_update).as_secs_f32().clamp(0.0, 0.05);
        runtime.last_update = now;
        runtime.next_frame = now
            + if runtime.panel.is_live() {
                Duration::from_micros(33_333)
            } else {
                FRAME
            };
        if let Some(monitor) = runtime.live_monitor.as_mut() {
            monitor.poll();
        }
        if runtime.panel.is_live() {
            runtime.window.request_redraw();
            event_loop.set_control_flow(ControlFlow::WaitUntil(runtime.next_frame));
            return;
        }
        let fixed_dt = 1.0 / runtime.ui.profile.pbf.fixed_hz.clamp(30.0, 120.0);
        if runtime.ui.playing || runtime.ui.drag.active {
            runtime.simulation_accumulator = (runtime.simulation_accumulator
                + wall_dt * runtime.ui.time_scale)
                .min(fixed_dt * 8.0);
        }
        if runtime.ui.single_step {
            runtime.ui.single_step = false;
            runtime.simulation_accumulator += fixed_dt;
        }
        let mut simulation_steps = 0;
        while runtime.simulation_accumulator >= fixed_dt && simulation_steps < 8 {
            runtime.elapsed = (runtime.elapsed + fixed_dt).rem_euclid(3_600.0);
            update_preview_bodies(runtime, fixed_dt);
            runtime.simulation_accumulator -= fixed_dt;
            simulation_steps += 1;
        }
        if runtime.ui.reset_requested {
            runtime.ui.reset_requested = false;
            reset_preview_bodies(runtime, &self.genome);
        }
        if runtime.ui.test_voice_requested {
            runtime.ui.test_voice_requested = false;
            if runtime.audio.is_none() {
                match AudioEngine::try_start() {
                    Ok(engine) => {
                        runtime.audio = Some(engine);
                        runtime.audio_error = None;
                    }
                    Err(error) => runtime.audio_error = Some(error.to_string()),
                }
            }
            if let Some(audio) = runtime.audio.as_ref() {
                let motifs = generate_initial_motifs(&self.genome.voice);
                if !motifs.is_empty() {
                    runtime.voice_preview_counter = runtime.voice_preview_counter.wrapping_add(1);
                    let performance_seed = SEED
                        ^ runtime
                            .voice_preview_counter
                            .wrapping_mul(0x9E37_79B9_7F4A_7C15);
                    let motif_index =
                        runtime.voice_preview_counter.saturating_sub(1) as usize % motifs.len();
                    let motif = &motifs[motif_index];
                    let gain_shape = preview_seeded_unit(performance_seed ^ 0xA24B_AED4);
                    let duration_shape = preview_seeded_unit(performance_seed ^ 0x9FB2_1C65);
                    let pitch_shape = preview_seeded_unit(performance_seed ^ 0xC13F_A9A9);
                    let duration_scale = 0.72 + duration_shape * (1.45 - 0.72);
                    let request = VocalRequest {
                        motif_id: motif.id,
                        performance_seed,
                        gain: self.genome.voice.maximum_loudness
                            * (0.58 + gain_shape * (1.04 - 0.58)),
                        pan: 0.0,
                        pitch_scale: 0.96 + pitch_shape * 0.08,
                        tempo_scale: duration_scale.recip(),
                        stress: 0.15,
                        purr: false,
                        rhythm_intervals: [0.0; 8],
                    };
                    if let Err(error) = audio.enqueue(&self.genome.voice, motif, &request) {
                        runtime.audio_error = Some(error.to_string());
                    } else {
                        runtime.ui.status = format!(
                            "Voice preview {}/{} · {:.0} ms · gain {:.2}",
                            motif_index + 1,
                            motifs.len(),
                            motif
                                .syllables
                                .iter()
                                .map(|syllable| syllable.duration_ms + syllable.gap_after_ms)
                                .sum::<f32>()
                                * duration_scale,
                            request.gain
                        );
                    }
                }
            }
        }
        if let Some(audio) = runtime.audio.as_ref() {
            let levels = audio.callback_levels();
            runtime.ui.audio_rms = levels.rms;
            runtime.ui.audio_peak = levels.peak;
            runtime.ui.audio_status = format!(
                "Ready · {} · {:?}",
                audio.device_name(),
                audio.selected_config()
            );
            if audio.poll_runtime_event().is_some() {
                audio.clear_visual_feedback();
                runtime.audio = None;
                runtime.audio_error = Some("audio stream runtime error".into());
            }
        } else {
            runtime.ui.audio_status = "Failed".into();
        }
        if let Some(error) = runtime.audio_error.as_ref() {
            runtime.ui.audio_status = format!("{} · {error}", runtime.ui.audio_status);
        }
        runtime
            .renderer
            .set_review_background(BACKGROUNDS[runtime.ui.background]);
        runtime.window.request_redraw();
        event_loop.set_control_flow(ControlFlow::WaitUntil(runtime.next_frame));
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
        if window_id == runtime.window.id() {
            let response = runtime
                .egui_state
                .on_window_event(runtime.window.as_ref(), &event);
            match event {
                WindowEvent::CloseRequested => event_loop.exit(),
                WindowEvent::Resized(size) => {
                    runtime.renderer.resize(size);
                    sync_preview_viewport(
                        &mut runtime.body,
                        size,
                        runtime.ui.preview_bounds_min,
                        runtime.ui.preview_bounds_max,
                    );
                }
                WindowEvent::KeyboardInput { event, .. }
                    if event.state == ElementState::Pressed && !event.repeat =>
                {
                    if let PhysicalKey::Code(code) = event.physical_key {
                        match code {
                            KeyCode::Escape => event_loop.exit(),
                            KeyCode::F1 => runtime.panel = DevPanel::Character,
                            KeyCode::F2 => runtime.panel = DevPanel::Perception,
                            KeyCode::F3 => runtime.panel = DevPanel::Behavior,
                            KeyCode::F4 => runtime.panel = DevPanel::Diagnostics,
                            KeyCode::Space if runtime.panel.is_live() => {
                                if let Some(monitor) = runtime.live_monitor.as_mut() {
                                    monitor.toggle_playback();
                                }
                            }
                            KeyCode::ArrowLeft if runtime.panel.is_live() => {
                                if let Some(monitor) = runtime.live_monitor.as_mut() {
                                    monitor.select_previous();
                                }
                            }
                            KeyCode::ArrowRight if runtime.panel.is_live() => {
                                if let Some(monitor) = runtime.live_monitor.as_mut() {
                                    monitor.select_next();
                                }
                            }
                            KeyCode::Space
                                if runtime.panel == DevPanel::Character && !response.consumed =>
                            {
                                runtime.ui.playing = !runtime.ui.playing;
                            }
                            KeyCode::KeyR
                                if runtime.panel == DevPanel::Character && !response.consumed =>
                            {
                                runtime.ui.reset_requested = true;
                            }
                            _ => {}
                        }
                    }
                }
                WindowEvent::RedrawRequested => render_main(runtime, &self.genome, event_loop),
                _ => {}
            }
        }
    }
}

fn sync_preview_space(body: &mut ProceduralBody, size: PhysicalSize<u32>) {
    let width = size.width.max(1) as f32;
    let height = size.height.max(1) as f32;
    body.set_render_aspect(width / height);
    body.set_desktop_motion_space(Vec2::new(width, height), height);
}

fn sync_preview_viewport(
    body: &mut ProceduralBody,
    size: PhysicalSize<u32>,
    normalized_minimum: Vec2,
    normalized_maximum: Vec2,
) {
    sync_preview_space(body, size);
    let full_size = Vec2::new(size.width.max(1) as f32, size.height.max(1) as f32);
    let minimum = normalized_minimum.clamp(Vec2::ZERO, Vec2::ONE);
    let maximum = normalized_maximum.clamp(Vec2::ZERO, Vec2::ONE);
    let center = (minimum + maximum) * 0.5;
    body.set_presentation_offset_pixels((center - Vec2::splat(0.5)) * full_size, full_size.y);
    body.set_liquid_preview_bounds(Some((minimum, maximum)));
}

fn reset_preview_bodies(runtime: &mut LabRuntime, genome: &Genome) {
    match ProceduralBody::generate(genome) {
        Ok(mut body) => {
            runtime.ui.profile.render_mode = BodyRenderMode::ParticlePbf;
            let _ = body.apply_tuning_profile(runtime.ui.profile.clone());
            let navigation_anchor = preview_navigation_anchor(runtime.ui.scenario, runtime.ui.drag);
            body.embodiment
                .liquid
                .set_navigation_anchor_strength(navigation_anchor);
            sync_preview_viewport(
                &mut body,
                runtime.window.inner_size(),
                runtime.ui.preview_bounds_min,
                runtime.ui.preview_bounds_max,
            );
            runtime.body = body;
            runtime.elapsed = 0.0;
            runtime.simulation_accumulator = 0.0;
            runtime.ui.status = "Preview reset with the same deterministic seed.".to_owned();
        }
        Err(error) => runtime.ui.status = format!("Reset failed: {error}"),
    }
}

fn update_preview_bodies(runtime: &mut LabRuntime, dt: f32) {
    if let Err(error) = apply_preview_profile(runtime) {
        runtime.ui.status = format!("Liquid profile rejected: {error}");
    }
    let drag = runtime.ui.drag;
    let audio_feedback = runtime
        .audio
        .as_ref()
        .map(AudioEngine::visual_feedback)
        .unwrap_or_default();
    let (velocity, acceleration, pressure, interaction_velocity) =
        resolved_preview_motion(runtime.ui.scenario, runtime.elapsed, drag);
    let body = &mut runtime.body;
    sync_preview_viewport(
        body,
        runtime.window.inner_size(),
        runtime.ui.preview_bounds_min,
        runtime.ui.preview_bounds_max,
    );
    body.simulation.feedback.velocity = velocity;
    body.simulation.feedback.acceleration = acceleration;
    if !drag.active && !drag.released {
        body.simulation.feedback.world_position = (body.simulation.feedback.world_position
            + velocity * dt * 0.035)
            .clamp(Vec2::splat(0.1), Vec2::splat(0.9));
    }
    let body_position = body.simulation.feedback.world_position;
    let gaze_target = (body_position + Vec2::new(0.10, -0.05)).clamp(Vec2::ZERO, Vec2::ONE);
    let mut expression = ExpressionState::default();
    if let Some(emotion) = runtime.ui.preview_emotion {
        apply_emotion_to_expression(
            &mut expression,
            emotion,
            runtime.ui.preview_emotion_intensity,
        );
    }
    if runtime.ui.focus_lock {
        expression.pupil_focus = 1.0;
    }
    let interaction_target = if runtime.ui.focus_distance < -0.25 {
        Some(InteractionTarget::Cursor)
    } else if runtime.ui.focus_distance > 0.25 {
        Some(InteractionTarget::User)
    } else {
        None
    };
    let intent = BodyIntent {
        locomotion: LocomotionMode::Hover,
        target_position: body_position,
        target_surface: None,
        desired_speed: velocity.length().clamp(0.0, 1.0),
        facing_direction: 1.0,
        gaze_target: Some(gaze_target),
        pose: PoseIntent::Neutral,
        expression,
        interaction_target,
    };
    let preview_center = (runtime.ui.preview_bounds_min + runtime.ui.preview_bounds_max) * 0.5;
    let pointer_from_preview_center = drag.pointer_normalized - preview_center;
    let cursor_position = body_position + pointer_from_preview_center;
    let sensors = SensorFrame {
        cursor_position,
        cursor_velocity: interaction_velocity,
        cursor_acceleration: drag.acceleration_normalized,
        cursor_distance_to_pet: if runtime.ui.focus_distance < -0.25 {
            0.03
        } else {
            pointer_from_preview_center.length()
        },
        pointer_down: drag.active,
        pointer_pressed: drag.pressed,
        pointer_released: drag.released,
        pet_hovered: pointer_from_preview_center.length() < 0.24,
        pet_touched: drag.active,
        pet_dragged: drag.active,
        local_luminance: Some(runtime.ui.scene_luminance),
        mean_luminance: Some(runtime.ui.scene_luminance),
        ..SensorFrame::default()
    };
    let mind = VisualMindInput {
        curiosity: runtime.ui.interest,
        attachment: 0.55,
        arousal: runtime
            .ui
            .emotional_arousal
            .max((velocity.length().max(interaction_velocity.length()) * 0.45).clamp(0.0, 1.0)),
        social_focus: runtime.ui.interest,
        attention_confidence: if runtime.ui.focus_lock { 1.0 } else { 0.45 },
        attention_commitment: if runtime.ui.focus_lock { 1.0 } else { 0.35 },
        window_pressure: pressure,
        local_luminance: runtime.ui.scene_luminance,
        confidence: 0.6,
        ..VisualMindInput::default()
    };
    body.embodied_update(
        &intent,
        &sensors,
        AffectState {
            arousal: mind.arousal,
            attachment: mind.attachment,
            ..AffectState::default()
        },
        mind,
        VoiceVisualState {
            active: audio_feedback.active,
            motif_id: audio_feedback.motif_id,
            syllable_index: audio_feedback.syllable_index,
            envelope: audio_feedback.envelope,
            mouth_open: audio_feedback.mouth_open,
            pitch_normalized: audio_feedback.pitch_normalized,
            noisiness: audio_feedback.noisiness,
            purr: audio_feedback.purr,
        },
        dt,
    );
    runtime.ui.drag.pressed = false;
    runtime.ui.drag.released = false;
}

fn resolved_preview_motion(
    scenario: PreviewScenario,
    elapsed: f32,
    drag: PreviewDrag,
) -> (Vec2, Vec2, f32, Vec2) {
    if drag.active || drag.released {
        // The preview body is rendered at a fixed center. Feeding pointer motion
        // into body locomotion moved the simulation origin without moving the
        // preview, so inertia visibly sent the blob opposite to the mouse. While
        // held, the cursor is exclusively a field acting on the real particles.
        return (Vec2::ZERO, Vec2::ZERO, 0.0, drag.velocity_normalized);
    }
    let (velocity, acceleration, pressure) = scenario_motion(scenario, elapsed);
    (velocity, acceleration, pressure, Vec2::ZERO)
}

fn apply_preview_profile(runtime: &mut LabRuntime) -> Result<(), pet_body::TuningProfileError> {
    runtime.ui.profile.render_mode = BodyRenderMode::ParticlePbf;
    let previous = runtime.body.tuning_profile().pbf;
    let next = runtime.ui.profile.pbf;
    let structural_change = previous.particle_count != next.particle_count
        || (previous.spacing_scale - next.spacing_scale).abs() > f32::EPSILON
        || (previous.kernel_radius_scale - next.kernel_radius_scale).abs() > f32::EPSILON
        || (previous.rest_density_scale - next.rest_density_scale).abs() > f32::EPSILON;
    runtime
        .body
        .apply_tuning_profile(runtime.ui.profile.clone())?;
    let navigation_anchor = preview_navigation_anchor(runtime.ui.scenario, runtime.ui.drag);
    runtime
        .body
        .embodiment
        .liquid
        .set_navigation_anchor_strength(navigation_anchor);
    sync_preview_viewport(
        &mut runtime.body,
        runtime.window.inner_size(),
        runtime.ui.preview_bounds_min,
        runtime.ui.preview_bounds_max,
    );
    if structural_change {
        runtime.ui.status =
            "Structural physics control changed: preview reset deterministically.".to_owned();
    }
    Ok(())
}

fn scenario_motion(scenario: PreviewScenario, time: f32) -> (Vec2, Vec2, f32) {
    match scenario {
        PreviewScenario::Calm => (Vec2::ZERO, Vec2::ZERO, 0.0),
        PreviewScenario::FluidFlight => {
            let (velocity, acceleration) = fluid_flight_motion(time);
            (velocity, acceleration, 0.0)
        }
        PreviewScenario::ConstantVelocity => (Vec2::new(0.55, -0.08), Vec2::ZERO, 0.0),
        PreviewScenario::HardStop => {
            let phase = time.rem_euclid(4.0);
            if phase < 2.0 {
                (Vec2::new(0.85, -0.10), Vec2::ZERO, 0.0)
            } else if phase < 2.14 {
                (Vec2::ZERO, Vec2::new(-6.0, 0.8), 0.0)
            } else {
                (Vec2::ZERO, Vec2::ZERO, 0.0)
            }
        }
        PreviewScenario::DirectionReversal => {
            let direction = if time.rem_euclid(3.0) < 1.5 {
                1.0
            } else {
                -1.0
            };
            (
                Vec2::new(direction * 0.72, 0.08),
                Vec2::new(direction * 2.8, 0.0),
                0.0,
            )
        }
        PreviewScenario::Impulse => {
            let impulse = if time.rem_euclid(2.5) < 0.10 {
                7.0
            } else {
                0.0
            };
            (Vec2::ZERO, Vec2::new(impulse, -impulse * 0.35), 0.0)
        }
        PreviewScenario::DetachAndRemerge => {
            let acceleration = if time.rem_euclid(6.0) < 0.22 {
                Vec2::new(10.0, -4.5)
            } else {
                Vec2::ZERO
            };
            (Vec2::new(0.34, 0.0), acceleration, 0.0)
        }
        PreviewScenario::WindowPressure => (Vec2::new(0.18, 0.0), Vec2::ZERO, 0.92),
    }
}

fn fluid_flight_motion(time: f32) -> (Vec2, Vec2) {
    const KEYFRAMES: [(f32, Vec2); 9] = [
        (0.00, Vec2::ZERO),
        (0.75, Vec2::new(0.82, -0.12)),
        (2.00, Vec2::new(0.62, 0.10)),
        (2.65, Vec2::new(0.22, 0.48)),
        (3.35, Vec2::new(-0.76, 0.12)),
        (4.80, Vec2::new(-0.58, -0.14)),
        (5.55, Vec2::new(0.12, -0.52)),
        (6.35, Vec2::new(0.50, 0.08)),
        (7.20, Vec2::ZERO),
    ];
    let phase = time.rem_euclid(8.0);
    if phase >= KEYFRAMES[KEYFRAMES.len() - 1].0 {
        return (Vec2::ZERO, Vec2::ZERO);
    }
    for pair in KEYFRAMES.windows(2) {
        let (start_time, start_velocity) = pair[0];
        let (end_time, end_velocity) = pair[1];
        if phase < end_time {
            return smooth_velocity_segment(
                phase,
                start_time,
                end_time,
                start_velocity,
                end_velocity,
            );
        }
    }
    (Vec2::ZERO, Vec2::ZERO)
}

fn smooth_velocity_segment(
    time: f32,
    start_time: f32,
    end_time: f32,
    start_velocity: Vec2,
    end_velocity: Vec2,
) -> (Vec2, Vec2) {
    let duration = (end_time - start_time).max(1.0e-4);
    let t = ((time - start_time) / duration).clamp(0.0, 1.0);
    let blend = t * t * t * (t * (t * 6.0 - 15.0) + 10.0);
    let derivative = 30.0 * t * t * (t * (t - 2.0) + 1.0) / duration;
    let delta = end_velocity - start_velocity;
    (start_velocity + delta * blend, delta * derivative)
}

fn preview_navigation_anchor(scenario: PreviewScenario, drag: PreviewDrag) -> f32 {
    if scenario == PreviewScenario::Calm || drag.active || drag.released {
        0.0
    } else {
        1.0
    }
}

fn render_main(runtime: &mut LabRuntime, genome: &Genome, event_loop: &ActiveEventLoop) {
    let diagnostics = runtime.body.embodiment.liquid.diagnostics();
    let raw_input = runtime.egui_state.take_egui_input(runtime.window.as_ref());
    let egui_context = runtime.egui_context.clone();
    let mut selected_panel = runtime.panel;
    let output = egui_context.run(raw_input, |context| {
        dev_console_navigation(context, &mut selected_panel, runtime.live_monitor.as_ref());
        match selected_panel {
            DevPanel::Character => {
                runtime.ui.show(
                    context,
                    diagnostics,
                    runtime.body.embodiment.pose.pupil_size,
                    runtime.body.embodiment.pose.pupil_asymmetry,
                    runtime.fps,
                    runtime.p95_frame_ms,
                );
            }
            panel => {
                if let Some(monitor) = runtime.live_monitor.as_mut() {
                    show_live_panel(context, monitor, panel);
                } else {
                    show_live_pet_unavailable(context);
                }
            }
        }
    });
    runtime.panel = selected_panel;
    runtime
        .egui_state
        .handle_platform_output(runtime.window.as_ref(), output.platform_output);
    if runtime.panel == DevPanel::Character
        && let Err(error) = apply_preview_profile(runtime)
    {
        runtime.ui.status = format!("Liquid profile rejected: {error}");
    }
    let paint_jobs = runtime
        .egui_context
        .tessellate(output.shapes, output.pixels_per_point);
    for (id, delta) in &output.textures_delta.set {
        runtime.egui_renderer.update_texture(
            runtime.renderer.device(),
            runtime.renderer.queue(),
            *id,
            delta,
        );
    }
    let size = runtime.window.inner_size();
    let screen = ScreenDescriptor {
        size_in_pixels: [size.width.max(1), size.height.max(1)],
        pixels_per_point: output.pixels_per_point,
    };
    let presentation_dt = (Instant::now() - runtime.last_present)
        .as_secs_f32()
        .clamp(0.0, 0.05);
    runtime.body.presentation_update(presentation_dt);
    let mut parameters = runtime.body.render_parameters(genome, 0.35);
    parameters.render_mode = BodyRenderMode::ParticlePbf;
    parameters.debug_view = runtime.ui.debug_view;
    let renderer = &mut runtime.renderer;
    let egui_renderer = &mut runtime.egui_renderer;
    let outcome = renderer.render_with_overlay(parameters, |device, queue, encoder, view| {
        let callbacks = egui_renderer.update_buffers(device, queue, encoder, &paint_jobs, &screen);
        debug_assert!(callbacks.is_empty());
        let pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Pet2 Dev Console egui overlay"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        egui_renderer.render(&mut pass.forget_lifetime(), &paint_jobs, &screen);
    });
    for id in &output.textures_delta.free {
        runtime.egui_renderer.free_texture(id);
    }
    let presented_at = Instant::now();
    let frame_ms = (presented_at - runtime.last_present).as_secs_f32() * 1_000.0;
    runtime.last_present = presented_at;
    runtime.frame_times_ms[runtime.frame_time_cursor] = frame_ms;
    runtime.frame_time_cursor = (runtime.frame_time_cursor + 1) % runtime.frame_times_ms.len();
    runtime.frame_time_count = (runtime.frame_time_count + 1).min(runtime.frame_times_ms.len());
    runtime.fps_frames += 1;
    let fps_elapsed = (presented_at - runtime.fps_window_started).as_secs_f32();
    if fps_elapsed >= 1.0 {
        runtime.fps = runtime.fps_frames as f32 / fps_elapsed;
        let mut sorted = runtime.frame_times_ms;
        sorted[..runtime.frame_time_count].sort_by(f32::total_cmp);
        let p95_index = ((runtime.frame_time_count as f32 * 0.95).ceil() as usize)
            .saturating_sub(1)
            .min(runtime.frame_time_count.saturating_sub(1));
        runtime.p95_frame_ms = sorted[p95_index];
        runtime.fps_frames = 0;
        runtime.fps_window_started = presented_at;
    }
    if outcome == RenderOutcome::OutOfMemory {
        event_loop.exit();
    }
}

impl LivePetMonitor {
    fn new(store: &StateStore) -> Self {
        Self {
            control_store: store.clone(),
            telemetry_path: store.paths.telemetry.clone(),
            telemetry_previous_path: store.paths.telemetry_previous.clone(),
            legacy_path: store.paths.events.clone(),
            using_legacy: false,
            previous_loaded: false,
            current_bootstrapped: false,
            offset: 0,
            frames: VecDeque::with_capacity(LIVE_FRAME_HISTORY_CAP),
            frame_sizes: VecDeque::with_capacity(LIVE_FRAME_HISTORY_CAP),
            frame_history_bytes: 0,
            history: VecDeque::with_capacity(LIVE_FRAME_HISTORY_CAP),
            selected: 0,
            follow_live: true,
            playing: false,
            playback_rate: 1.0,
            playback_accumulator: 0.0,
            last_playback_tick: Instant::now(),
            control_drive: LabDrive::Curiosity,
            control_drive_delta: 0.25,
            control_drive_duration: 4.0,
            control_attention_duration: 2.0,
            control_reward: 0.25,
            control_counter: 0,
            last_sent_command_id: store
                .load_lab_control()
                .ok()
                .flatten()
                .map_or(0, |control| control.command_id),
            pending_command_id: None,
            pending_command_sent_at: None,
            control_status: "No intervention sent from this Dev Console session.".into(),
            evolution_scrub: EvolutionScrubState::default(),
            canonical_evolution: CanonicalEvolutionCache::default(),
            last_state_poll: Instant::now() - Duration::from_secs(2),
            state_file_signature: None,
            status: format!("Waiting for {}", store.paths.telemetry.display()),
            last_poll: Instant::now() - Duration::from_secs(1),
            last_received: None,
        }
    }

    fn poll(&mut self) {
        self.advance_playback();
        self.expire_pending_control();
        self.poll_canonical_evolution();
        if self.last_poll.elapsed() < Duration::from_millis(80) {
            return;
        }
        self.last_poll = Instant::now();

        let telemetry_available =
            self.telemetry_path.exists() || self.telemetry_previous_path.exists();
        if telemetry_available && self.using_legacy {
            // The canonical bounded stream may appear after Dev Console starts.
            // Prefer it immediately and drop only the in-memory legacy copy;
            // no log on disk is ever changed here.
            self.using_legacy = false;
            self.previous_loaded = false;
            self.current_bootstrapped = false;
            self.offset = 0;
            self.frames.clear();
            self.frame_sizes.clear();
            self.frame_history_bytes = 0;
            self.history.clear();
            self.selected = 0;
            self.follow_live = true;
        } else if !telemetry_available && !self.using_legacy {
            self.using_legacy = true;
            self.current_bootstrapped = false;
            self.offset = 0;
        }

        if self.using_legacy && !self.legacy_path.exists() {
            self.status = "No telemetry yet. Start Pet2.exe with --dev-mode.".into();
            return;
        }

        let mut historical = 0_usize;
        if !self.using_legacy && !self.previous_loaded {
            self.previous_loaded = true;
            let mut previous_offset = 0;
            match read_telemetry_file(&self.telemetry_previous_path, &mut previous_offset, true) {
                Ok(events) => {
                    for event in events.into_iter().filter(is_telemetry_frame) {
                        self.ingest(event);
                        historical += 1;
                    }
                }
                Err(error) => self.status = error,
            }
        }

        let active_path = self.active_path().to_path_buf();
        let incremental_read = self.current_bootstrapped;
        let events = match read_telemetry_file(&active_path, &mut self.offset, true) {
            Ok(events) => events,
            Err(error) => {
                self.status = error;
                return;
            }
        };
        self.current_bootstrapped = true;
        let mut received = 0_usize;
        for event in events {
            let accepted = if self.using_legacy {
                event.get("kind").and_then(Value::as_str) == Some("debug_state")
            } else {
                is_telemetry_frame(&event)
            };
            if accepted {
                self.ingest(event);
                received += 1;
            }
        }
        if telemetry_read_is_live(incremental_read, received) {
            self.last_received = Some(Instant::now());
            self.status = format!(
                "LIVE · {received} new frame(s) · {}",
                if self.using_legacy {
                    "legacy events.jsonl fallback"
                } else {
                    "bounded telemetry.jsonl"
                }
            );
        } else if received > 0 || historical > 0 {
            self.status = format!(
                "Loaded {} historical frame(s); waiting for an incremental telemetry append",
                received.saturating_add(historical)
            );
        }
    }

    fn ingest(&mut self, event: Value) {
        self.observe_control_acknowledgement(&event);
        let frame_bytes = serde_json::to_vec(&event).map_or(0, |bytes| bytes.len());
        let velocity = vector2(&event, "/details/screen_velocity_px");
        let orb_velocity = vector2(&event, "/details/ecology/orb_velocity");
        let stretch = number(&event, "/details/liquid_stretch_ratio").unwrap_or(1.0) as f32;
        let sample = LiveGraphSample {
            speed_px: velocity.length(),
            pressure: number(&event, "/details/ecology/window_pressure").unwrap_or(0.0) as f32,
            deformation: (stretch - 1.0).abs(),
            visual_blend: number(&event, "/details/ecology_visual_effect/color_blend")
                .unwrap_or(0.0) as f32,
            orb_speed: orb_velocity.length(),
        };
        self.frames.push_back(event);
        self.frame_sizes.push_back(frame_bytes);
        self.frame_history_bytes = self.frame_history_bytes.saturating_add(frame_bytes);
        self.history.push_back(sample);
        while self.frames.len() > LIVE_FRAME_HISTORY_CAP
            || self.frame_history_bytes > LIVE_FRAME_HISTORY_BYTES
        {
            self.frames.pop_front();
            self.history.pop_front();
            if let Some(frame_bytes) = self.frame_sizes.pop_front() {
                self.frame_history_bytes = self.frame_history_bytes.saturating_sub(frame_bytes);
            }
            if !self.follow_live {
                self.selected = self.selected.saturating_sub(1);
            }
        }
        if self.follow_live {
            self.selected = self.frames.len().saturating_sub(1);
        } else {
            self.selected = self.selected.min(self.frames.len().saturating_sub(1));
        }
    }

    fn poll_canonical_evolution(&mut self) {
        if self.last_state_poll.elapsed() < Duration::from_secs(1) {
            return;
        }
        self.last_state_poll = Instant::now();
        let signature = fs::metadata(&self.control_store.paths.state)
            .ok()
            .and_then(|metadata| Some((metadata.modified().ok()?, metadata.len())));
        if signature.is_some() && signature == self.state_file_signature {
            return;
        }
        match self.control_store.load_state() {
            Ok(Some(state)) => {
                let history =
                    match serde_json::to_value(&state.life.state.development.mutation_history) {
                        Ok(Value::Array(history)) => history,
                        Ok(_) => Vec::new(),
                        Err(error) => {
                            self.canonical_evolution.available = false;
                            self.canonical_evolution.status =
                                format!("Canonical evolution serialization failed: {error}");
                            return;
                        }
                    };
                self.canonical_evolution = CanonicalEvolutionCache {
                    available: true,
                    lineage_id: format!("{:032x}", state.life.state.genome.lineage_id),
                    generation: state.life.state.genome.generation,
                    status: format!(
                        "Canonical validated state.json · {} bounded mutation record(s) · generation {}",
                        history.len(),
                        state.life.state.genome.generation
                    ),
                    history,
                };
                self.state_file_signature = signature;
            }
            Ok(None) => {
                self.canonical_evolution = CanonicalEvolutionCache {
                    status:
                        "Canonical state.json is not available; using telemetry evolution fallback."
                            .into(),
                    ..CanonicalEvolutionCache::default()
                };
                self.state_file_signature = None;
            }
            Err(error) => {
                self.canonical_evolution.available = false;
                self.canonical_evolution.status = format!(
                    "Canonical state.json failed validation ({error}); using telemetry evolution fallback."
                );
            }
        }
    }

    fn stale_seconds(&self) -> Option<f32> {
        self.last_received
            .map(|instant| instant.elapsed().as_secs_f32())
    }

    fn active_path(&self) -> &Path {
        if self.using_legacy {
            &self.legacy_path
        } else {
            &self.telemetry_path
        }
    }

    fn selected_frame(&self) -> Option<&Value> {
        self.frames.get(self.selected)
    }

    fn select_index(&mut self, index: usize) {
        if self.frames.is_empty() {
            return;
        }
        self.follow_live = false;
        self.playing = false;
        self.playback_accumulator = 0.0;
        self.selected = index.min(self.frames.len() - 1);
    }

    fn select_previous(&mut self) {
        self.select_index(self.selected.saturating_sub(1));
    }

    fn select_next(&mut self) {
        self.select_index(
            self.selected
                .saturating_add(1)
                .min(self.frames.len().saturating_sub(1)),
        );
    }

    fn jump_live(&mut self) {
        self.follow_live = true;
        self.playing = false;
        self.playback_accumulator = 0.0;
        self.selected = self.frames.len().saturating_sub(1);
    }

    fn toggle_playback(&mut self) {
        if self.frames.is_empty() {
            return;
        }
        if self.follow_live {
            self.follow_live = false;
            self.playing = false;
        } else {
            if self.selected >= self.frames.len().saturating_sub(1) {
                self.selected = 0;
            }
            self.playing = !self.playing;
        }
        self.last_playback_tick = Instant::now();
        self.playback_accumulator = 0.0;
    }

    fn advance_playback(&mut self) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_playback_tick).as_secs_f32();
        self.last_playback_tick = now;
        if !self.playing || self.follow_live || self.frames.is_empty() {
            return;
        }
        self.playback_accumulator += elapsed * EXPECTED_TELEMETRY_HZ * self.playback_rate;
        let advance = self.playback_accumulator.floor() as usize;
        if advance == 0 {
            return;
        }
        self.playback_accumulator -= advance as f32;
        self.selected = self.selected.saturating_add(advance);
        if self.selected >= self.frames.len().saturating_sub(1) {
            self.selected = self.frames.len().saturating_sub(1);
            self.playing = false;
        }
    }

    fn can_send_control(&self) -> bool {
        self.pending_command_id.is_none()
            && self.follow_live
            && !self.frames.is_empty()
            && self.selected == self.frames.len().saturating_sub(1)
            && self.stale_seconds().is_some_and(|seconds| seconds <= 1.2)
    }

    fn send_control(&mut self, command: LabControlCommand) {
        if !self.can_send_control() {
            self.control_status =
                "Control blocked: jump to a fresh Live frame before sending a command.".into();
            return;
        }
        let issued_unix_ms = unix_time_ms();
        self.control_counter = self.control_counter.wrapping_add(1).max(1);
        let command_id = next_lab_command_id(
            issued_unix_ms,
            self.control_counter,
            self.last_sent_command_id,
        );
        let envelope = build_lab_control_envelope(command_id, issued_unix_ms, command);
        let description = lab_control_description(&envelope.command);
        match self.control_store.save_lab_control(&envelope) {
            Ok(()) => {
                self.last_sent_command_id = envelope.command_id;
                self.pending_command_id = Some(envelope.command_id);
                self.pending_command_sent_at = Some(Instant::now());
                self.control_status = format!(
                    "Sent {description} · command {} · waiting for runtime acknowledgement · expires in {} ms",
                    envelope.command_id, envelope.expires_after_ms
                );
            }
            Err(error) => self.control_status = format!("Control write failed: {error}"),
        }
    }

    fn observe_control_acknowledgement(&mut self, event: &Value) {
        let Some(pending) = self.pending_command_id else {
            return;
        };
        let Some(observed) = event
            .pointer("/details/lab_interventions/last_command_id")
            .and_then(Value::as_u64)
        else {
            return;
        };
        if observed < pending {
            return;
        }
        let status = event
            .pointer("/details/lab_interventions/last_command_status")
            .and_then(Value::as_str)
            .unwrap_or("status_unavailable");
        self.control_status = if observed == pending {
            format!("Runtime acknowledged command {pending}: {status}")
        } else {
            format!("Command {pending} was superseded; runtime watermark is {observed}: {status}")
        };
        self.pending_command_id = None;
        self.pending_command_sent_at = None;
    }

    fn expire_pending_control(&mut self) {
        if self.pending_command_id.is_some()
            && self
                .pending_command_sent_at
                .is_some_and(|sent| sent.elapsed() >= LAB_CONTROL_ACK_TIMEOUT)
        {
            let pending = self.pending_command_id.take().unwrap_or_default();
            self.pending_command_sent_at = None;
            self.control_status = format!(
                "No telemetry acknowledgement for command {pending} within {} s; slot unlocked without assuming it applied.",
                LAB_CONTROL_ACK_TIMEOUT.as_secs()
            );
        }
    }
}

fn telemetry_read_is_live(stream_was_bootstrapped: bool, received: usize) -> bool {
    stream_was_bootstrapped && received > 0
}

fn unix_time_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u128::from(u64::MAX)) as u64
}

fn next_lab_command_id(issued_unix_ms: u64, counter: u64, last_sent_id: u64) -> u64 {
    issued_unix_ms
        .saturating_mul(1_024)
        .saturating_add(counter & 1_023)
        .max(last_sent_id.saturating_add(1))
        .max(1)
}

fn build_lab_control_envelope(
    command_id: u64,
    issued_unix_ms: u64,
    command: LabControlCommand,
) -> LabControlEnvelope {
    let expires_after_ms = match &command {
        LabControlCommand::CueAttention {
            duration_seconds, ..
        }
        | LabControlCommand::DrivePulse {
            duration_seconds, ..
        } => ((*duration_seconds * 1_000.0).ceil() as u32).saturating_add(2_000),
        LabControlCommand::Reward { .. }
        | LabControlCommand::FocusMode { .. }
        | LabControlCommand::ClearDrivePulses => 5_000,
    }
    .clamp(1_000, 30_000);
    LabControlEnvelope {
        schema_version: LAB_CONTROL_SCHEMA_VERSION,
        command_id: command_id.max(1),
        issued_unix_ms,
        expires_after_ms,
        command,
    }
}

fn lab_control_description(command: &LabControlCommand) -> String {
    match command {
        LabControlCommand::CueAttention { position, .. } => {
            format!("attention cue @ {:.2}, {:.2}", position[0], position[1])
        }
        LabControlCommand::DrivePulse { drive, delta, .. } => {
            format!("{} drive {delta:+.2}", lab_drive_label(*drive))
        }
        LabControlCommand::Reward { value } => format!("learning reward {value:+.2}"),
        LabControlCommand::FocusMode { enabled } => {
            format!("focus mode {}", if *enabled { "on" } else { "off" })
        }
        LabControlCommand::ClearDrivePulses => "clear temporary drive pulses".into(),
    }
}

fn lab_drive_label(drive: LabDrive) -> &'static str {
    match drive {
        LabDrive::Safety => "safety",
        LabDrive::Play => "play",
        LabDrive::Curiosity => "curiosity",
        LabDrive::Autonomy => "autonomy",
        LabDrive::Sleep => "sleep",
        LabDrive::Social => "social",
        LabDrive::Comfort => "comfort",
        LabDrive::Novelty => "novelty",
    }
}

fn read_telemetry_file(
    path: &Path,
    offset: &mut u64,
    tail_on_first_read: bool,
) -> Result<Vec<Value>, String> {
    let metadata = match fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(format!("Cannot inspect {}: {error}", path.display())),
    };
    if metadata.len() < *offset {
        *offset = 0;
    }
    let mut truncated_start = false;
    if tail_on_first_read && *offset == 0 && metadata.len() > LIVE_BOOTSTRAP_BYTES {
        *offset = metadata.len() - LIVE_BOOTSTRAP_BYTES;
        truncated_start = true;
    }
    let mut file =
        File::open(path).map_err(|error| format!("Cannot open {}: {error}", path.display()))?;
    file.seek(SeekFrom::Start(*offset))
        .map_err(|error| format!("Cannot seek {}: {error}", path.display()))?;
    let mut reader = BufReader::new(file);
    let mut line = String::new();
    if truncated_start {
        let bytes = reader
            .read_line(&mut line)
            .map_err(|error| format!("Cannot read {}: {error}", path.display()))?;
        *offset = offset.saturating_add(bytes as u64);
    }
    let mut events = Vec::new();
    loop {
        line.clear();
        let bytes = reader
            .read_line(&mut line)
            .map_err(|error| format!("Cannot read {}: {error}", path.display()))?;
        if bytes == 0 {
            break;
        }
        // A writer can be between bytes of the final JSON line. Leave the
        // cursor at its beginning so the completed record is read next poll.
        if !line.ends_with('\n') {
            break;
        }
        *offset = offset.saturating_add(bytes as u64);
        if let Ok(event) = serde_json::from_str::<Value>(&line) {
            events.push(event);
        }
    }
    Ok(events)
}

fn is_telemetry_frame(event: &Value) -> bool {
    event.get("kind").and_then(Value::as_str) == Some("debug_state")
        || event.get("kind").is_none() && event.get("details").is_some_and(Value::is_object)
}

fn dev_console_navigation(
    context: &Context,
    selected: &mut DevPanel,
    monitor: Option<&LivePetMonitor>,
) {
    egui::TopBottomPanel::top("dev_console_navigation")
        .frame(
            egui::Frame::default()
                .fill(egui::Color32::from_rgb(16, 20, 29))
                .inner_margin(egui::Margin::symmetric(14, 10)),
        )
        .show(context, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Pet2 Dev Console");
                ui.add_space(10.0);
                for panel in DevPanel::ALL {
                    let label = format!("{}  {}", panel.shortcut(), panel.label());
                    ui.selectable_value(selected, panel, label);
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let live = monitor
                        .and_then(LivePetMonitor::stale_seconds)
                        .is_some_and(|seconds| seconds <= 1.2);
                    ui.colored_label(
                        if live {
                            egui::Color32::from_rgb(105, 232, 172)
                        } else {
                            egui::Color32::from_rgb(255, 190, 92)
                        },
                        if live { "● LIVE" } else { "● OFFLINE" },
                    );
                });
            });
        });
}

fn show_live_pet_unavailable(context: &Context) {
    egui::CentralPanel::default().show(context, |ui| {
        ui.heading("Live data unavailable");
        ui.label("The Pet2 data directory could not be discovered.");
        ui.label("Character editing remains available in F1 Character.");
    });
}

fn show_live_panel(context: &Context, monitor: &mut LivePetMonitor, panel: DevPanel) {
    egui::CentralPanel::default()
        .frame(egui::Frame::default().fill(egui::Color32::from_rgb(12, 15, 22)))
        .show(context, |ui| {
            ui.heading(panel.label());
            ui.small(panel_description(panel));
            ui.small(&monitor.status);
            let stale_seconds = monitor.stale_seconds();
            if panel == DevPanel::Diagnostics {
                live_timeline(ui, monitor);
                ui.separator();
            }
            if panel == DevPanel::Behavior {
                live_behavior_controls(ui, monitor);
                ui.separator();
            }
            let Some(latest) = monitor.selected_frame().cloned() else {
                ui.add_space(20.0);
                ui.heading("Waiting for the organism");
                ui.label("Start Pet2 Dev Console from the installed application.");
                ui.label(
                    "No screen captures/pixel buffers, typed text, raw audio, or native window IDs are stored; legacy numeric desktop coordinates remain.",
                );
                return;
            };
            live_now_summary(ui, &latest);
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| match panel {
                    DevPanel::Perception => {
                        live_attention_map(ui, &latest);
                        ui.separator();
                        ui.columns(2, |columns| {
                            live_perception_and_decision(&mut columns[0], &latest);
                            live_desktop_rhythm(&mut columns[1], &latest);
                        });
                        ui.separator();
                        live_drives(ui, &latest);
                    }
                    DevPanel::Behavior => {
                        ui.columns(2, |columns| {
                            live_drives(&mut columns[0], &latest);
                            live_perception_and_decision(&mut columns[1], &latest);
                        });
                        ui.separator();
                        live_desktop_rhythm(ui, &latest);
                        ui.separator();
                        live_candidates(ui, &latest);
                        ui.separator();
                        live_learning_memory_social(ui, &latest);
                    }
                    DevPanel::Diagnostics => {
                        for (severity, message) in live_blockers(&latest, stale_seconds) {
                            live_status_card(ui, severity, &message);
                        }
                        live_chart(ui, &monitor.history, Some(monitor.selected));
                        ui.separator();
                        ui.columns(2, |columns| {
                            live_motor_and_body(&mut columns[0], &latest);
                            live_orb_and_performance(&mut columns[1], &latest);
                        });
                        ui.separator();
                        live_candidates(ui, &latest);
                        ui.separator();
                        live_identity_and_evolution(
                            ui,
                            &latest,
                            &monitor.canonical_evolution,
                            &mut monitor.evolution_scrub,
                        );
                        ui.separator();
                        ui.columns(2, |columns| {
                            live_morph_and_activity(&mut columns[0], &latest);
                            live_vita_and_fusion(&mut columns[1], &latest);
                        });
                        ui.separator();
                        live_raw_json(ui, &latest);
                    }
                    DevPanel::Character => {}
                });
        });
}

fn panel_description(panel: DevPanel) -> &'static str {
    match panel {
        DevPanel::Character => "Appearance, liquid physics, face, voice, and preview scenarios.",
        DevPanel::Perception => {
            "What the organism notices, what currently owns its gaze, and its coarse desktop model."
        }
        DevPanel::Behavior => {
            "Current motivation, action arbitration, and temporary safe behavior tuning."
        }
        DevPanel::Diagnostics => {
            "Replay, causal blockers, motor/body performance, learning, and raw telemetry."
        }
    }
}

fn live_status_card(ui: &mut egui::Ui, severity: u8, message: &str) {
    let color = match severity {
        2 => egui::Color32::from_rgb(255, 105, 96),
        1 => egui::Color32::from_rgb(255, 190, 92),
        _ => egui::Color32::from_rgb(108, 224, 166),
    };
    egui::Frame::default()
        .fill(color.gamma_multiply(0.13))
        .corner_radius(5.0)
        .inner_margin(egui::Margin::same(7))
        .show(ui, |ui| {
            ui.colored_label(color, message);
        });
    ui.add_space(3.0);
}

fn live_now_summary(ui: &mut egui::Ui, latest: &Value) {
    let action = text(latest, "/details/action");
    let goal = text(latest, "/details/ecology/active_goal");
    let attention = text(latest, "/details/gaze/attention_kind");
    let gaze_owner = text(latest, "/details/gaze/source");
    let desktop_context = text(latest, "/details/vita/desktop_rhythm/context");
    let gaze_target = format_vector(latest, "/details/gaze/intent_world_target");
    let explanation = interest_explanation(latest);
    egui::Frame::default()
        .fill(egui::Color32::from_rgb(20, 27, 39))
        .corner_radius(7.0)
        .inner_margin(egui::Margin::same(10))
        .show(ui, |ui| {
            ui.columns(4, |columns| {
                columns[0].small("DOING");
                columns[0].strong(if goal.is_empty() { &action } else { &goal });
                columns[1].small("INTEREST");
                columns[1].strong(if attention.is_empty() {
                    "None"
                } else {
                    &attention
                });
                columns[2].small("LOOKING");
                columns[2].strong(gaze_target);
                columns[3].small("CONTEXT");
                columns[3].strong(if desktop_context.is_empty() {
                    if gaze_owner.is_empty() {
                        "—"
                    } else {
                        &gaze_owner
                    }
                } else {
                    &desktop_context
                });
            });
            ui.add_space(6.0);
            ui.label(explanation);
            if !gaze_owner.is_empty() && !desktop_context.is_empty() {
                ui.small(format!("Gaze owner: {gaze_owner}"));
            }
        });
    ui.add_space(8.0);
}

fn interest_explanation(latest: &Value) -> String {
    let goal = text(latest, "/details/ecology/active_goal");
    let attention = text(latest, "/details/gaze/attention_kind");
    let saliency = text(latest, "/details/ecology/saliency_kind");
    let reason = text(latest, "/details/ecology/reason");
    let typing = number(latest, "/details/typing_rate_hz").unwrap_or(0.0);
    let scroll = number(latest, "/details/scroll_velocity")
        .unwrap_or(0.0)
        .abs();
    let desktop_context = text(latest, "/details/vita/desktop_rhythm/context");
    let opening = number(latest, "/details/vita/desktop_rhythm/social_opening").unwrap_or(0.0);
    if goal == "SleepInDen" {
        return "Rest owns behavior; passive window motion and typing are deliberately suppressed."
            .into();
    }
    if !saliency.is_empty() {
        return format!(
            "A coarse visual region ({saliency}) is competing for attention; reaction reason: {reason}."
        );
    }
    if attention == "Typing" || typing > 0.35 {
        return format!(
            "A typing burst was noticed at {typing:.1} Hz. Context is {desktop_context}; social opening is {opening:.0}%, so this remains a glance rather than a movement command.",
            opening = opening * 100.0,
        );
    }
    if attention == "Scroll" || scroll > 0.08 {
        return format!(
            "Scroll/motion energy ({scroll:.2}) was noticed and will habituate if it continues."
        );
    }
    if attention == "Window" {
        return "A window boundary won a rare environmental glance; stable large windows remain background."
            .into();
    }
    if attention == "Cursor" {
        return "The cursor currently wins the attention competition; it may still be ignored if distant or calm."
            .into();
    }
    if goal.is_empty() {
        "No external stimulus owns behavior; the organism is following its internal state.".into()
    } else {
        format!("{goal} currently owns behavior because of {reason}.")
    }
}

fn live_desktop_rhythm(ui: &mut egui::Ui, latest: &Value) {
    ui.heading("Shared desktop rhythm");
    let context = text(latest, "/details/vita/desktop_rhythm/context");
    let focused =
        number(latest, "/details/vita/desktop_rhythm/focused_work_seconds").unwrap_or(0.0);
    let quiet = number(latest, "/details/vita/desktop_rhythm/quiet_seconds").unwrap_or(0.0);
    let stimulation = number(latest, "/details/vita/desktop_rhythm/stimulation").unwrap_or(0.0);
    let opening = number(latest, "/details/vita/desktop_rhythm/social_opening").unwrap_or(0.0);
    let risk = number(latest, "/details/vita/desktop_rhythm/interruption_risk").unwrap_or(0.0);
    let protection = number(latest, "/details/perception/desktop_focus_pressure").unwrap_or(0.0);
    let cooldown = number(latest, "/details/vita/influence/cooldown_seconds").unwrap_or(0.0);
    let social_need = number(latest, "/details/drives/social").unwrap_or(0.0);
    let play_need = number(latest, "/details/drives/play").unwrap_or(0.0);
    let initiative = if protection > 0.62 {
        "protected by work".to_owned()
    } else if cooldown > 0.05 {
        format!("cooldown {cooldown:.1} s")
    } else if opening < 0.16 {
        "no social opening".to_owned()
    } else if social_need.max(play_need) < 0.38 {
        "no internal need".to_owned()
    } else {
        "quiet bid may compete".to_owned()
    };
    egui::Grid::new("desktop_rhythm_grid")
        .num_columns(2)
        .striped(true)
        .show(ui, |ui| {
            live_metric(
                ui,
                "Context",
                if context.is_empty() {
                    "—".into()
                } else {
                    context.clone()
                },
            );
            live_metric(ui, "Focused work", format!("{focused:.1} s"));
            live_metric(ui, "Quiet", format!("{quiet:.1} s"));
            live_metric(ui, "Stimulation", format!("{:.0}%", stimulation * 100.0));
            live_metric(ui, "Social opening", format!("{:.0}%", opening * 100.0));
            live_metric(ui, "Interruption risk", format!("{:.0}%", risk * 100.0));
            live_metric(
                ui,
                "Action protection",
                format!("{:.0}%", protection * 100.0),
            );
            live_metric(ui, "Initiative", initiative);
        });
    let explanation = match context.as_str() {
        "working" => {
            "Work is protected: motion may move the eyes, but cannot start a social or toy approach."
        }
        "pause" => "A brief work pause creates a small, learned opportunity for a quiet check-in.",
        "interactive" => {
            "The pointer gesture is an explicit invitation, so reciprocal play may start."
        }
        "ambient_motion" => {
            "The screen is changing in the background; it competes for occasional glances and habituates."
        }
        "away" => "The user appears away; autonomous rest and self-play own behavior.",
        _ => "The desktop is settled. Internal needs may surface without demanding attention.",
    };
    ui.small(explanation);
}

fn live_behavior_controls(ui: &mut egui::Ui, monitor: &mut LivePetMonitor) {
    ui.strong("Temporary behavior presets");
    ui.small("These change motivation for a bounded time and never overwrite personality or learned state.");
    let enabled = monitor.can_send_control();
    let mut command = None;
    ui.horizontal_wrapped(|ui| {
        if ui
            .add_enabled(enabled, egui::Button::new("Explore more"))
            .clicked()
        {
            command = Some(LabControlCommand::DrivePulse {
                drive: LabDrive::Curiosity,
                delta: 0.35,
                duration_seconds: 20.0,
            });
        }
        if ui
            .add_enabled(enabled, egui::Button::new("More playful"))
            .clicked()
        {
            command = Some(LabControlCommand::DrivePulse {
                drive: LabDrive::Play,
                delta: 0.32,
                duration_seconds: 20.0,
            });
        }
        if ui
            .add_enabled(enabled, egui::Button::new("More social"))
            .clicked()
        {
            command = Some(LabControlCommand::DrivePulse {
                drive: LabDrive::Social,
                delta: 0.28,
                duration_seconds: 20.0,
            });
        }
        if ui
            .add_enabled(enabled, egui::Button::new("Calm down"))
            .clicked()
        {
            command = Some(LabControlCommand::DrivePulse {
                drive: LabDrive::Sleep,
                delta: 0.28,
                duration_seconds: 20.0,
            });
        }
        if ui
            .add_enabled(enabled, egui::Button::new("Natural state"))
            .clicked()
        {
            command = Some(LabControlCommand::ClearDrivePulses);
        }
    });
    if let Some(command) = command {
        monitor.send_control(command);
    }
    live_controlled_intervention(ui, monitor);
}

fn live_timeline(ui: &mut egui::Ui, monitor: &mut LivePetMonitor) {
    let count = monitor.frames.len();
    ui.horizontal_wrapped(|ui| {
        let live_label = if monitor.follow_live {
            "● Following live"
        } else {
            "Jump to live"
        };
        if ui
            .add_enabled(count > 0, egui::Button::new(live_label))
            .clicked()
        {
            monitor.jump_live();
        }
        if ui
            .add_enabled(count > 0, egui::Button::new("⏮"))
            .on_hover_text("Previous telemetry frame (Left arrow)")
            .clicked()
        {
            monitor.select_previous();
        }
        let play_label = if monitor.follow_live || !monitor.playing {
            "▶"
        } else {
            "⏸"
        };
        if ui
            .add_enabled(count > 0, egui::Button::new(play_label))
            .on_hover_text("Play/pause replay (Space)")
            .clicked()
        {
            monitor.toggle_playback();
        }
        if ui
            .add_enabled(count > 0, egui::Button::new("⏭"))
            .on_hover_text("Next telemetry frame (Right arrow)")
            .clicked()
        {
            monitor.select_next();
        }
        ui.label("Replay:");
        for rate in [0.25_f32, 0.5, 1.0, 2.0, 4.0] {
            ui.selectable_value(&mut monitor.playback_rate, rate, format!("{rate}×"));
        }
    });
    if count > 0 {
        let last = count - 1;
        let mut selected = monitor.selected.min(last);
        let response = ui.add(
            Slider::new(&mut selected, 0..=last)
                .show_value(false)
                .text("telemetry history"),
        );
        if response.changed() {
            monitor.select_index(selected);
        }
        let frame = monitor.selected_frame();
        let timestamp = frame
            .and_then(|value| number(value, "/monotonic_seconds"))
            .map_or_else(|| "t= —".to_owned(), |time| format!("t={time:.3} s"));
        let sequence = frame
            .and_then(|value| first_scalar(value, &["/sequence", "/details/sequence"]))
            .map_or_else(String::new, |value| format!(" · sequence {value}"));
        ui.small(format!(
            "Frame {} / {} · {timestamp}{sequence} · bounded in-memory history ({LIVE_FRAME_HISTORY_CAP} max) · read-only",
            monitor.selected + 1,
            count
        ));
    } else {
        ui.small("No replay frames loaded yet · monitor is read-only");
    }
}

fn live_controlled_intervention(ui: &mut egui::Ui, monitor: &mut LivePetMonitor) {
    CollapsingHeader::new("Fine behavior controls")
        .default_open(true)
        .show(ui, |ui| {
            let enabled = monitor.can_send_control();
            ui.label(
                "Drive and attention changes are temporary and auto-expire. Reward changes learning; focus mode remains explicit. Timeline replay itself is always read-only.",
            );
            if !enabled {
                ui.colored_label(
                    egui::Color32::from_rgb(255, 190, 92),
                    monitor.pending_command_id.map_or(
                        "Controls disabled: select ● Following live on a fresh frame.".into(),
                        |command_id| {
                            format!(
                                "Controls locked until fresh telemetry acknowledges command {command_id}."
                            )
                        },
                    ),
                );
            }

            let mut pending_command = None;
            ui.horizontal_wrapped(|ui| {
                ui.strong("Drive pulse");
                egui::ComboBox::from_id_salt("lab_control_drive")
                    .selected_text(lab_drive_label(monitor.control_drive))
                    .show_ui(ui, |ui| {
                        for drive in [
                            LabDrive::Safety,
                            LabDrive::Play,
                            LabDrive::Curiosity,
                            LabDrive::Autonomy,
                            LabDrive::Sleep,
                            LabDrive::Social,
                            LabDrive::Comfort,
                            LabDrive::Novelty,
                        ] {
                            ui.selectable_value(
                                &mut monitor.control_drive,
                                drive,
                                lab_drive_label(drive),
                            );
                        }
                    });
                ui.label("Δ");
                ui.add(
                    DragValue::new(&mut monitor.control_drive_delta)
                        .range(-1.0..=1.0)
                        .speed(0.05),
                );
                ui.add(
                    Slider::new(&mut monitor.control_drive_duration, 0.1..=30.0)
                        .suffix(" s"),
                );
                if ui
                    .add_enabled(enabled, egui::Button::new("Apply temporary pulse"))
                    .clicked()
                {
                    pending_command = Some(LabControlCommand::DrivePulse {
                        drive: monitor.control_drive,
                        delta: monitor.control_drive_delta,
                        duration_seconds: monitor.control_drive_duration,
                    });
                }
                if ui
                    .add_enabled(enabled, egui::Button::new("Clear drive pulses"))
                    .clicked()
                {
                    pending_command = Some(LabControlCommand::ClearDrivePulses);
                }
            });

            let attention_position = monitor
                .selected_frame()
                .and_then(|frame| optional_vector2(frame, "/details/gaze/cursor_world"))
                .unwrap_or(Vec2::splat(0.5))
                .clamp(Vec2::ZERO, Vec2::ONE);
            ui.horizontal_wrapped(|ui| {
                ui.strong("Attention cue");
                ui.monospace(format!(
                    "cursor {:.3}, {:.3}",
                    attention_position.x, attention_position.y
                ));
                ui.add(
                    Slider::new(&mut monitor.control_attention_duration, 0.1..=10.0)
                        .suffix(" s"),
                );
                if ui
                    .add_enabled(enabled, egui::Button::new("Cue attention"))
                    .clicked()
                {
                    pending_command = Some(LabControlCommand::CueAttention {
                        position: attention_position.to_array(),
                        duration_seconds: monitor.control_attention_duration,
                    });
                }
            });

            ui.horizontal_wrapped(|ui| {
                ui.strong("Learning reward");
                ui.add(
                    DragValue::new(&mut monitor.control_reward)
                        .range(-1.0..=1.0)
                        .speed(0.05),
                );
                if ui
                    .add_enabled(
                        enabled && monitor.control_reward.abs() >= 0.001,
                        egui::Button::new("Apply reward"),
                    )
                    .clicked()
                {
                    pending_command = Some(LabControlCommand::Reward {
                        value: monitor.control_reward,
                    });
                }
                ui.strong("Focus");
                if ui
                    .add_enabled(enabled, egui::Button::new("On"))
                    .clicked()
                {
                    pending_command = Some(LabControlCommand::FocusMode { enabled: true });
                }
                if ui
                    .add_enabled(enabled, egui::Button::new("Off"))
                    .clicked()
                {
                    pending_command = Some(LabControlCommand::FocusMode { enabled: false });
                }
            });

            if let Some(command) = pending_command {
                monitor.send_control(command);
            }
            ui.small(&monitor.control_status);
            if let Some(status) = monitor
                .selected_frame()
                .and_then(|frame| frame.pointer("/details/lab_interventions"))
            {
                ui.strong("Runtime acknowledgement / active interventions");
                scalar_table(ui, "live_lab_intervention_status", status, 32);
            }
        });
}

fn live_blockers(latest: &Value, stale_seconds: Option<f32>) -> Vec<(u8, String)> {
    let mut blockers = Vec::new();
    if stale_seconds.is_none_or(|seconds| seconds > 1.2) {
        blockers.push((2, "TELEMETRY STALE — the pet is not in --dev-mode".into()));
    }
    let body_mode = text(latest, "/details/body_render_mode");
    let material = text(latest, "/details/material_variant");
    if body_mode != "ParticlePbf" || material != "CinematicJelly" {
        blockers.push((
            2,
            format!(
                "WRONG BODY — runtime is {body_mode} / {material}; expected ParticlePbf / CinematicJelly"
            ),
        ));
    }
    let pressure = number(latest, "/details/ecology/window_pressure").unwrap_or(0.0);
    let goal = text(latest, "/details/ecology/active_goal");
    if pressure >= 0.22 {
        blockers.push((
            2,
            format!("SAFETY OVERRIDE — window pressure {pressure:.2}; {goal} owns locomotion"),
        ));
    }
    let visual_age = number(latest, "/details/ecology/visual_grid_age_ms");
    if latest
        .pointer("/details/capabilities/screen_capture")
        .and_then(Value::as_bool)
        == Some(false)
    {
        blockers.push((
            2,
            "SCREEN RECORDING OFF — enable Pet2 in macOS Privacy & Security, then restart Pet2"
                .into(),
        ));
    }
    if visual_age.is_none_or(|age| age > 250.0) {
        blockers.push((2, "VISUAL OFFLINE — reduced desktop grid is stale".into()));
    }
    let saliency = text(latest, "/details/ecology/saliency_kind");
    let saliency_score = number(latest, "/details/ecology/saliency_score").unwrap_or(0.0);
    let color_blend = number(latest, "/details/ecology_visual_effect/color_blend").unwrap_or(0.0);
    let structure = number(latest, "/details/ecology/visual_structure").unwrap_or(0.0);
    let surprise = number(latest, "/details/ecology/visual_surprise").unwrap_or(0.0);
    if !saliency.is_empty()
        && saliency_score >= 0.12
        && color_blend < 0.03
        && structure < 0.05
        && surprise < 0.05
    {
        blockers.push((
            1,
            format!("VISUAL NOT EXPRESSED — {saliency} {saliency_score:.2} reached attention only"),
        ));
    }
    let speed = vector2(latest, "/details/screen_velocity_px").length();
    let position = vector2(latest, "/details/world_position");
    let target = vector2(latest, "/details/target_position");
    if position.distance(target) > 0.08 && speed < 24.0 {
        blockers.push((
            1,
            format!("MOTOR UNDERDRIVE — target is far but speed is only {speed:.1} px/s"),
        ));
    }
    let stretch = number(latest, "/details/liquid_stretch_ratio").unwrap_or(1.0);
    let liquid_speed = number(latest, "/details/liquid/maximum_speed").unwrap_or(0.0);
    if (stretch - 1.0).abs() < 0.035 && liquid_speed < 0.09 {
        blockers.push((
            1,
            format!("LOW DEFORMATION — stretch {stretch:.3}, internal speed {liquid_speed:.3}"),
        ));
    }
    if blockers.is_empty() {
        blockers.push((0, "No hard blocker in the current causal chain".into()));
    }
    blockers
}

fn live_attention_map(ui: &mut egui::Ui, latest: &Value) {
    ui.heading("Attention map (normalized desktop)");
    let width = ui.available_width().max(320.0);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 205.0), Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 5.0, egui::Color32::from_rgb(20, 26, 37));

    paint_coarse_visual_scene(&painter, rect, latest);

    let to_screen = |point: Vec2| {
        egui::pos2(
            rect.left() + point.x.clamp(0.0, 1.0) * rect.width(),
            rect.top() + point.y.clamp(0.0, 1.0) * rect.height(),
        )
    };
    let body = optional_vector2(latest, "/details/world_position").unwrap_or(Vec2::splat(0.5));
    let body_screen = to_screen(body);
    painter.circle_filled(body_screen, 6.0, egui::Color32::WHITE);
    painter.text(
        body_screen + egui::vec2(8.0, -7.0),
        egui::Align2::LEFT_CENTER,
        "BODY",
        egui::FontId::monospace(11.0),
        egui::Color32::WHITE,
    );

    let points = [
        (
            "/details/gaze/intent_world_target",
            "GAZE",
            egui::Color32::from_rgb(89, 220, 255),
            true,
        ),
        (
            "/details/gaze/attention_position",
            "VITA",
            egui::Color32::from_rgb(203, 126, 255),
            true,
        ),
        (
            "/details/ecology/saliency_target",
            "VISUAL",
            egui::Color32::from_rgb(255, 203, 80),
            true,
        ),
        (
            "/details/ecology/orb_position",
            "ORB",
            egui::Color32::from_rgb(94, 235, 151),
            false,
        ),
        (
            "/details/gaze/cursor_world",
            "CURSOR",
            egui::Color32::from_rgb(255, 126, 126),
            false,
        ),
    ];
    for (path, label, color, attention_line) in points {
        let Some(point) = optional_vector2(latest, path) else {
            continue;
        };
        let screen = to_screen(point);
        if attention_line {
            painter.line_segment([body_screen, screen], egui::Stroke::new(1.2, color));
        }
        painter.circle_filled(screen, 4.5, color);
        painter.text(
            screen + egui::vec2(7.0, 7.0),
            egui::Align2::LEFT_CENTER,
            label,
            egui::FontId::monospace(10.0),
            color,
        );
    }
    ui.small(format!(
        "owner: {} · backdrop={} · windows={} · cyan=final gaze · purple=VITA · yellow=visual",
        text(latest, "/details/gaze/source"),
        if boolean(latest, "/details/ecology/coarse_scene/available") {
            "coarse 8×5 vision"
        } else {
            "geometry only"
        },
        format_number(
            latest,
            "/details/ecology/coarse_scene/visible_window_count",
            0
        ),
    ));
}

fn paint_coarse_visual_scene(painter: &egui::Painter, rect: egui::Rect, latest: &Value) {
    let scene = &latest["details"]["ecology"]["coarse_scene"];
    let width = scene["width"].as_u64().unwrap_or(0) as usize;
    let height = scene["height"].as_u64().unwrap_or(0) as usize;
    if let Some(cells) = scene["cells"].as_array()
        && width > 0
        && height > 0
    {
        for (index, cell) in cells.iter().take(width * height).enumerate() {
            let Some(features) = cell.as_array() else {
                continue;
            };
            let feature = |index: usize| {
                features.get(index).and_then(Value::as_u64).unwrap_or(0) as f32 / 255.0
            };
            let luminance = feature(0);
            let hue = feature(1);
            let colorfulness = feature(2);
            let motion = feature(3);
            let column = index % width;
            let row = index / width;
            let cell_rect = egui::Rect::from_min_max(
                egui::pos2(
                    rect.left() + column as f32 / width as f32 * rect.width(),
                    rect.top() + row as f32 / height as f32 * rect.height(),
                ),
                egui::pos2(
                    rect.left() + (column + 1) as f32 / width as f32 * rect.width(),
                    rect.top() + (row + 1) as f32 / height as f32 * rect.height(),
                ),
            );
            painter.rect_filled(
                cell_rect.shrink(0.7),
                1.5,
                coarse_hsv_color(hue, colorfulness * 0.72, 0.08 + luminance * 0.56),
            );
            if motion > 0.08 {
                painter.rect_stroke(
                    cell_rect.shrink(1.3),
                    1.5,
                    egui::Stroke::new(0.7 + motion * 2.2, egui::Color32::from_rgb(255, 176, 76)),
                    egui::StrokeKind::Inside,
                );
            }
        }
    }

    if let Some(windows) = scene["windows"].as_array() {
        for window in windows {
            if let Some(window_rect) = normalized_json_rect(window, rect) {
                painter.rect_stroke(
                    window_rect,
                    2.0,
                    egui::Stroke::new(
                        1.0,
                        egui::Color32::from_rgba_unmultiplied(180, 205, 230, 105),
                    ),
                    egui::StrokeKind::Inside,
                );
            }
        }
    }
    if let Some(active) = normalized_json_rect(&scene["active_window"], rect) {
        painter.rect_stroke(
            active,
            2.0,
            egui::Stroke::new(1.8, egui::Color32::from_rgb(112, 218, 255)),
            egui::StrokeKind::Inside,
        );
    }
}

fn normalized_json_rect(value: &Value, canvas: egui::Rect) -> Option<egui::Rect> {
    let values = value.as_array()?;
    let number = |index: usize| values.get(index)?.as_f64().map(|value| value as f32);
    let left = number(0)?.clamp(0.0, 1.0);
    let top = number(1)?.clamp(0.0, 1.0);
    let right = number(2)?.clamp(left, 1.0);
    let bottom = number(3)?.clamp(top, 1.0);
    Some(egui::Rect::from_min_max(
        egui::pos2(
            canvas.left() + left * canvas.width(),
            canvas.top() + top * canvas.height(),
        ),
        egui::pos2(
            canvas.left() + right * canvas.width(),
            canvas.top() + bottom * canvas.height(),
        ),
    ))
}

fn coarse_hsv_color(hue: f32, saturation: f32, value: f32) -> egui::Color32 {
    let hue = hue.rem_euclid(1.0) * 6.0;
    let chroma = value * saturation;
    let x = chroma * (1.0 - (hue.rem_euclid(2.0) - 1.0).abs());
    let (red, green, blue) = match hue as u8 {
        0 => (chroma, x, 0.0),
        1 => (x, chroma, 0.0),
        2 => (0.0, chroma, x),
        3 => (0.0, x, chroma),
        4 => (x, 0.0, chroma),
        _ => (chroma, 0.0, x),
    };
    let offset = value - chroma;
    egui::Color32::from_rgb(
        ((red + offset).clamp(0.0, 1.0) * 255.0) as u8,
        ((green + offset).clamp(0.0, 1.0) * 255.0) as u8,
        ((blue + offset).clamp(0.0, 1.0) * 255.0) as u8,
    )
}

fn live_perception_and_decision(ui: &mut egui::Ui, latest: &Value) {
    ui.heading("Perception → decision");
    egui::Grid::new("live_perception_grid")
        .num_columns(2)
        .show(ui, |ui| {
            live_metric(ui, "Visual", text(latest, "/details/ecology/saliency_kind"));
            live_metric(
                ui,
                "Visual score",
                format_number(latest, "/details/ecology/saliency_score", 3),
            );
            live_metric(
                ui,
                "Visual age",
                format!(
                    "{} ms",
                    format_number(latest, "/details/ecology/visual_grid_age_ms", 1)
                ),
            );
            live_metric(ui, "Brain action", text(latest, "/details/action"));
            live_metric(
                ui,
                "Ecology goal",
                text(latest, "/details/ecology/active_goal"),
            );
            live_metric(ui, "Phase", text(latest, "/details/ecology/phase"));
            live_metric(ui, "Reason", text(latest, "/details/ecology/reason"));
            live_metric(ui, "Pose", text(latest, "/details/intent_pose"));
            live_metric(ui, "Gaze", text(latest, "/details/gaze_mode"));
            live_metric(ui, "Gaze owner", text(latest, "/details/gaze/source"));
            live_metric(
                ui,
                "World target",
                format_vector(latest, "/details/gaze/intent_world_target"),
            );
            live_metric(
                ui,
                "VITA attention",
                format!(
                    "{} @ {}",
                    text(latest, "/details/gaze/attention_kind"),
                    format_vector(latest, "/details/gaze/attention_position")
                ),
            );
            live_metric(
                ui,
                "Attention conf/hold",
                format!(
                    "{} / {} s",
                    format_number(latest, "/details/gaze/attention_confidence", 2),
                    format_number(latest, "/details/gaze/attention_commitment_remaining", 2)
                ),
            );
            live_metric(
                ui,
                "Eye target / shown",
                format!(
                    "{} / {}",
                    format_vector(latest, "/details/gaze/semantic_local_target"),
                    format_vector(latest, "/details/gaze/presented_local")
                ),
            );
            live_metric(
                ui,
                "Fixation lock",
                format!(
                    "{} · {} / {} s",
                    boolean(latest, "/details/gaze/fixation_locked"),
                    format_number(latest, "/details/gaze/fixation_elapsed", 2),
                    format_number(latest, "/details/gaze/fixation_duration", 2)
                ),
            );
        });
}

fn live_motor_and_body(ui: &mut egui::Ui, latest: &Value) {
    ui.heading("Motor → visible body");
    let speed = vector2(latest, "/details/screen_velocity_px").length();
    egui::Grid::new("live_motor_grid")
        .num_columns(2)
        .show(ui, |ui| {
            live_metric(ui, "Body", text(latest, "/details/body_render_mode"));
            live_metric(ui, "Material", text(latest, "/details/material_variant"));
            live_metric(ui, "Profile", text(latest, "/details/liquid_profile_name"));
            live_metric(ui, "Locomotion", text(latest, "/details/locomotion"));
            live_metric(ui, "Speed", format!("{speed:.1} px/s"));
            live_metric(
                ui,
                "Pressure",
                format_number(latest, "/details/ecology/window_pressure", 3),
            );
            live_metric(
                ui,
                "Stretch",
                format_number(latest, "/details/liquid_stretch_ratio", 3),
            );
            live_metric(
                ui,
                "Internal speed",
                format_number(latest, "/details/liquid/maximum_speed", 3),
            );
            live_metric(
                ui,
                "Color blend",
                format_number(latest, "/details/ecology_visual_effect/color_blend", 3),
            );
            live_metric(
                ui,
                "Flow / glow",
                format!(
                    "{} / {}",
                    format_number(latest, "/details/ecology_visual_effect/flow_boost", 2),
                    format_number(latest, "/details/ecology_visual_effect/glow_boost", 2)
                ),
            );
            live_metric(
                ui,
                "Mouth / body glow",
                format!(
                    "{} / {}",
                    format_number(latest, "/details/expression/mouth_curve", 2),
                    format_number(latest, "/details/expression/body_glow", 2)
                ),
            );
        });
}

fn live_drives(ui: &mut egui::Ui, latest: &Value) {
    ui.heading("Internal needs (pressure)");
    ui.small("Higher means less satisfied. Actions relieve only the matching need; neutral hovering no longer clears the whole mind.");
    let overlay_active = boolean(latest, "/details/lab_interventions/overlay_active");
    if overlay_active {
        ui.colored_label(
            egui::Color32::from_rgb(255, 205, 92),
            "Temporary overlay: natural → effective reaches Morph and VITA/Ecology; LifeCore homeostasis and resolved action remain natural.",
        );
    }
    for (drive, meaning) in [
        ("safety", "pressure to create distance from threat"),
        ("play", "need for active, voluntary play"),
        ("curiosity", "need to inspect something uncertain"),
        ("autonomy", "need to choose a self-directed activity"),
        ("sleep", "accumulated fatigue and circadian pressure"),
        ("social", "need for a small reciprocal contact"),
        ("comfort", "need to settle, land, purr, or rest"),
        ("novelty", "need for environmental variation"),
    ] {
        let natural_path = if overlay_active {
            format!("/details/lab_interventions/natural_drives/{drive}")
        } else {
            format!("/details/drives/{drive}")
        };
        let effective_path = format!("/details/lab_interventions/effective_drives/{drive}");
        let natural = number(latest, &natural_path)
            .or_else(|| number(latest, &format!("/details/drives/{drive}")))
            .unwrap_or(0.0) as f32;
        let effective = if overlay_active {
            number(latest, &effective_path).unwrap_or(f64::from(natural)) as f32
        } else {
            natural
        };
        ui.horizontal(|ui| {
            ui.label(format!("{drive:>9}")).on_hover_text(meaning);
            ui.add(
                egui::ProgressBar::new(effective.clamp(0.0, 1.0))
                    .desired_width(145.0)
                    .show_percentage(),
            );
            if overlay_active {
                ui.monospace(format!(
                    "{natural:.2} → {effective:.2}  Δ{:+.2}",
                    effective - natural
                ));
            }
        });
    }
}

fn live_orb_and_performance(ui: &mut egui::Ui, latest: &Value) {
    ui.heading("Orb / runtime");
    egui::Grid::new("live_orb_grid")
        .num_columns(2)
        .show(ui, |ui| {
            live_metric(ui, "Orb", text(latest, "/details/ecology/orb_state"));
            live_metric(
                ui,
                "Grounded / trapped",
                format!(
                    "{} / {}",
                    boolean(latest, "/details/ecology/orb_grounded"),
                    boolean(latest, "/details/ecology/orb_trapped")
                ),
            );
            live_metric(
                ui,
                "Orb speed",
                format!(
                    "{:.3}",
                    vector2(latest, "/details/ecology/orb_velocity").length()
                ),
            );
            live_metric(ui, "FPS", format_number(latest, "/details/fps", 1));
            live_metric(
                ui,
                "Physics p95",
                format!(
                    "{} ms",
                    array_number(latest, "/details/timings_ms/physics", 1, 2)
                ),
            );
            live_metric(
                ui,
                "Render p95",
                format!(
                    "{} ms",
                    array_number(latest, "/details/timings_ms/render", 1, 2)
                ),
            );
            live_metric(
                ui,
                "Brain tick",
                format!(
                    "{} µs",
                    format_number(latest, "/details/brain_tick_microseconds", 1)
                ),
            );
        });
}

fn live_candidates(ui: &mut egui::Ui, latest: &Value) {
    ui.heading("Eligible behavior candidates");
    let mut candidates = latest
        .pointer("/details/ecology/candidate_scores")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|candidate| candidate.get("eligible").and_then(Value::as_bool) == Some(true))
        .map(|candidate| {
            (
                candidate.get("goal").and_then(Value::as_str).unwrap_or("—"),
                candidate
                    .get("score")
                    .and_then(Value::as_f64)
                    .unwrap_or(0.0),
                candidate
                    .get("reason")
                    .and_then(Value::as_str)
                    .unwrap_or("—"),
            )
        })
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| right.1.total_cmp(&left.1));
    egui::Grid::new("live_candidates_grid")
        .striped(true)
        .show(ui, |ui| {
            ui.strong("Goal");
            ui.strong("Score");
            ui.strong("Reason");
            ui.end_row();
            for (goal, score, reason) in candidates.into_iter().take(8) {
                ui.label(goal);
                ui.label(format!("{score:.3}"));
                ui.label(reason);
                ui.end_row();
            }
        });
}

fn live_identity_and_evolution(
    ui: &mut egui::Ui,
    latest: &Value,
    canonical: &CanonicalEvolutionCache,
    scrub: &mut EvolutionScrubState,
) {
    CollapsingHeader::new("Identity / development / evolution")
        .default_open(true)
        .show(ui, |ui| {
            ui.colored_label(
                egui::Color32::from_rgb(255, 205, 92),
                "READ-ONLY HISTORY — scrubbing inspects recorded frames; it never rewrites the live organism.",
            );
            egui::Grid::new("live_identity_grid")
                .num_columns(2)
                .striped(true)
                .show(ui, |ui| {
                    live_metric(
                        ui,
                        "Identity seed",
                        first_scalar(
                            latest,
                            &[
                                "/details/identity/seed",
                                "/details/identity/identity_seed",
                                "/details/identity_seed",
                                "/details/genome/identity_seed",
                            ],
                        )
                        .unwrap_or_else(|| "—".into()),
                    );
                    live_metric(
                        ui,
                        "Lineage",
                        first_scalar(
                            latest,
                            &[
                                "/details/identity/lineage_id",
                                "/details/lineage_id",
                                "/details/genome/lineage_id",
                            ],
                        )
                        .unwrap_or_else(|| "—".into()),
                    );
                    live_metric(
                        ui,
                        "Generation",
                        first_scalar(
                            latest,
                            &[
                                "/details/identity/generation",
                                "/details/development/generation",
                                "/details/evolution/generation",
                                "/details/genome/generation",
                                "/details/generation",
                            ],
                        )
                        .unwrap_or_else(|| "—".into()),
                    );
                    live_metric(
                        ui,
                        "Stage",
                        first_scalar(
                            latest,
                            &[
                                "/details/development/stage",
                                "/details/evolution/stage",
                                "/details/identity/stage",
                            ],
                        )
                        .unwrap_or_else(|| "—".into()),
                    );
                    live_metric(
                        ui,
                        "Metamorphoses",
                        first_scalar(
                            latest,
                            &[
                                "/details/development/metamorphosis_count",
                                "/details/evolution/metamorphosis_count",
                            ],
                        )
                        .unwrap_or_else(|| "—".into()),
                    );
                    live_metric(
                        ui,
                        "Genome hash",
                        first_scalar(
                            latest,
                            &[
                                "/details/identity/genome_hash",
                                "/details/evolution/genome_hash",
                                "/details/genome_hash",
                            ],
                        )
                        .unwrap_or_else(|| "—".into()),
                    );
                });

            if let Some(lifetime) = pointer_any(
                latest,
                &[
                    "/details/development/lifetime",
                    "/details/evolution/lifetime",
                    "/details/lifetime_statistics",
                ],
            ) {
                ui.add_space(4.0);
                ui.strong("Lifetime statistics at this frame");
                scalar_table(ui, "live_lifetime_grid", lifetime, 24);
            }

            let (history, history_source) = evolution_history_for_frame(latest, canonical);
            ui.add_space(5.0);
            ui.strong(format!(
                "Evolution rewind · read-only checkpoints ({})",
                history.len()
            ));
            ui.small(history_source);
            if history.is_empty() {
                ui.small("No mutation checkpoint data is present in this telemetry schema.");
                return;
            }

            let checkpoints = evolution_checkpoints(&history);
            let history_key = mutation_history_key(&history);
            scrub.synchronize(history_key, checkpoints.len());
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(scrub.selected > 0, egui::Button::new("⏮ Previous"))
                    .clicked()
                {
                    scrub.selected = scrub.selected.saturating_sub(1);
                }
                if ui
                    .add_enabled(
                        scrub.selected + 1 < checkpoints.len(),
                        egui::Button::new("Next ⏭"),
                    )
                    .clicked()
                {
                    scrub.selected = (scrub.selected + 1).min(checkpoints.len() - 1);
                }
                if ui.button("Newest").clicked() {
                    scrub.selected = checkpoints.len() - 1;
                }
            });
            ui.add(
                Slider::new(&mut scrub.selected, 0..=checkpoints.len() - 1)
                    .show_value(false)
                    .text("evolution checkpoint"),
            );
            ui.small(format!(
                "Checkpoint {} / {} · spans all {} bounded mutation record(s) · no reconstruction or live mutation",
                scrub.selected + 1,
                checkpoints.len(),
                history.len()
            ));

            let legacy_count = history
                .iter()
                .filter(|record| {
                    record
                        .get("before_genome")
                        .is_none_or(Value::is_null)
                        || record.get("after_genome").is_none_or(Value::is_null)
                })
                .count();
            if legacy_count > 0 {
                ui.small(format!(
                    "{legacy_count} legacy mutation record(s) lack an exact before/after pair; their metadata remains scrub-able, but their genome transition cannot be reconstructed."
                ));
            }

            show_evolution_checkpoint(ui, checkpoints[scrub.selected], history.len());
        });
}

impl EvolutionScrubState {
    fn synchronize(&mut self, history_key: String, checkpoint_count: usize) {
        if self.history_key != history_key {
            self.history_key = history_key;
            self.selected = checkpoint_count.saturating_sub(1);
        } else {
            self.selected = self.selected.min(checkpoint_count.saturating_sub(1));
        }
    }
}

fn evolution_history_for_frame<'a>(
    latest: &'a Value,
    canonical: &'a CanonicalEvolutionCache,
) -> (Vec<&'a Value>, String) {
    let selected_lineage = pointer_any(
        latest,
        &["/details/identity/lineage_id", "/details/lineage_id"],
    )
    .and_then(Value::as_str);
    let selected_generation = pointer_any(
        latest,
        &[
            "/details/identity/generation",
            "/details/generation",
            "/details/development/generation",
        ],
    )
    .and_then(Value::as_u64);
    let selected_count = pointer_any(
        latest,
        &[
            "/details/development/mutation_history_count",
            "/details/evolution/mutation_history_count",
        ],
    )
    .and_then(Value::as_u64)
    .map(|count| count as usize);
    let latest_mutation = pointer_any(
        latest,
        &[
            "/details/development/latest_mutation",
            "/details/evolution/latest_mutation",
        ],
    )
    .filter(|value| !value.is_null());
    let lineage_matches = canonical.available
        && selected_lineage
            .is_some_and(|lineage| lineage.eq_ignore_ascii_case(&canonical.lineage_id));

    if lineage_matches {
        let mut history = canonical
            .history
            .iter()
            .filter(|record| {
                selected_generation.is_none_or(|generation| {
                    record
                        .get("generation")
                        .and_then(Value::as_u64)
                        .is_none_or(|record_generation| record_generation <= generation)
                })
            })
            .collect::<Vec<_>>();
        if let Some(count) = selected_count
            && history.len() > count
        {
            history.truncate(count);
        }
        if let Some(latest_mutation) = latest_mutation {
            let mutation_generation = latest_mutation.get("generation").and_then(Value::as_u64);
            let belongs_to_frame = selected_generation
                .zip(mutation_generation)
                .is_none_or(|(selected, mutation)| mutation <= selected);
            if belongs_to_frame {
                if let Some(index) = history.iter().position(|record| {
                    record.get("generation").and_then(Value::as_u64) == mutation_generation
                }) {
                    history[index] = latest_mutation;
                } else {
                    history.push(latest_mutation);
                }
                history.sort_by_key(|record| record.get("generation").and_then(Value::as_u64));
            }
        }
        return (
            history,
            format!(
                "{} · lineage matched · state generation {} · view truncated to generation {}{}",
                canonical.status,
                canonical.generation,
                selected_generation.map_or_else(|| "—".into(), |value| value.to_string()),
                selected_count.map_or_else(String::new, |count| {
                    format!(" / {count} mutation record(s)")
                })
            ),
        );
    }

    if let Some(history) = pointer_any(
        latest,
        &[
            "/details/development/mutation_history",
            "/details/evolution/mutation_history",
            "/details/evolution/mutations",
            "/details/mutation_history",
        ],
    )
    .and_then(Value::as_array)
    {
        return (
            history.iter().collect(),
            format!(
                "Legacy telemetry full-history fallback · {}{}",
                if canonical.available {
                    "canonical lineage mismatch · "
                } else {
                    ""
                },
                canonical.status
            ),
        );
    }

    if let Some(latest_mutation) = latest_mutation {
        return (
            vec![latest_mutation],
            format!(
                "Telemetry latest_mutation only · canonical history unavailable for selected lineage · {}",
                canonical.status
            ),
        );
    }
    (
        Vec::new(),
        if canonical.available {
            format!(
                "Evolution unavailable: selected lineage {} does not match canonical lineage {}.",
                selected_lineage.unwrap_or("—"),
                canonical.lineage_id
            )
        } else {
            canonical.status.clone()
        },
    )
}

fn mutation_history_key(history: &[&Value]) -> String {
    let first = history.first();
    let last = history.last();
    let exact_pairs = history
        .iter()
        .filter(|record| {
            record
                .get("before_genome")
                .is_some_and(|value| !value.is_null())
                && record
                    .get("after_genome")
                    .is_some_and(|value| !value.is_null())
        })
        .count();
    format!(
        "{}:{}:{}:{}:{}:{}",
        history.len(),
        first
            .and_then(|record| record.get("generation"))
            .map_or_else(|| "—".into(), compact_json),
        first
            .and_then(|record| record.get("parent_genome_hash"))
            .map_or_else(|| "—".into(), compact_json),
        last.and_then(|record| record.get("generation"))
            .map_or_else(|| "—".into(), compact_json),
        last.and_then(|record| record.get("child_genome_hash"))
            .map_or_else(|| "—".into(), compact_json),
        exact_pairs,
    )
}

fn evolution_checkpoints<'a>(history: &[&'a Value]) -> Vec<EvolutionCheckpoint<'a>> {
    let earliest_before = history.iter().position(|record| {
        record
            .get("before_genome")
            .is_some_and(|value| !value.is_null())
    });
    let mut checkpoints =
        Vec::with_capacity(history.len() + usize::from(earliest_before.is_some()));
    for (record_index, &record) in history.iter().enumerate() {
        if earliest_before == Some(record_index)
            && let Some(genome) = record.get("before_genome").filter(|value| !value.is_null())
        {
            checkpoints.push(EvolutionCheckpoint::Before {
                record_index,
                record,
                genome,
            });
        }
        checkpoints.push(EvolutionCheckpoint::After {
            record_index,
            record,
            genome: record.get("after_genome").filter(|value| !value.is_null()),
        });
    }
    checkpoints
}

fn show_evolution_checkpoint(
    ui: &mut egui::Ui,
    checkpoint: EvolutionCheckpoint<'_>,
    history_len: usize,
) {
    let (record_index, record, genome, is_baseline) = match checkpoint {
        EvolutionCheckpoint::Before {
            record_index,
            record,
            genome,
        } => (record_index, record, Some(genome), true),
        EvolutionCheckpoint::After {
            record_index,
            record,
            genome,
        } => (record_index, record, genome, false),
    };
    let generation = if is_baseline {
        genome
            .and_then(|genome| genome.get("generation"))
            .map_or_else(|| "—".into(), compact_json)
    } else {
        record
            .get("generation")
            .map_or_else(|| "—".into(), compact_json)
    };
    let stage = if is_baseline {
        genome
            .and_then(|genome| genome.get("generation"))
            .and_then(Value::as_u64)
            .map_or("pre-mutation", development_stage_for_generation)
            .to_owned()
    } else {
        record.get("stage").map_or_else(|| "—".into(), compact_json)
    };
    let hash = record
        .get(if is_baseline {
            "parent_genome_hash"
        } else {
            "child_genome_hash"
        })
        .map_or_else(|| "—".into(), compact_json);
    egui::Frame::default()
        .fill(egui::Color32::from_rgb(20, 26, 37))
        .corner_radius(5.0)
        .inner_margin(egui::Margin::same(7))
        .show(ui, |ui| {
            egui::Grid::new("selected_evolution_checkpoint")
                .num_columns(2)
                .striped(true)
                .show(ui, |ui| {
                    live_metric(
                        ui,
                        "Checkpoint kind",
                        if is_baseline {
                            "earliest exact before_genome".into()
                        } else if genome.is_some() {
                            "recorded after_genome".into()
                        } else {
                            "legacy metadata only".into()
                        },
                    );
                    live_metric(ui, "Generation", generation);
                    live_metric(ui, "Stage", stage);
                    live_metric(ui, "Genome hash", hash);
                    live_metric(
                        ui,
                        "Mutation record",
                        format!("{} / {history_len}", record_index + 1),
                    );
                });
            ui.horizontal_wrapped(|ui| {
                ui.label("Changed traits:");
                if is_baseline {
                    ui.monospace("baseline before the selected mutation");
                } else if let Some(traits) =
                    record.get("changed_traits").and_then(Value::as_array)
                {
                    for trait_name in traits.iter().filter_map(Value::as_str) {
                        ui.monospace(trait_name);
                    }
                } else {
                    ui.monospace("—");
                }
            });

            if let Some(lifetime) = record
                .get("lifetime_snapshot")
                .filter(|value| !value.is_null())
            {
                ui.strong("Lifetime snapshot that produced this mutation");
                scalar_table(ui, "selected_evolution_lifetime", lifetime, 24);
            } else {
                ui.small("Lifetime snapshot unavailable for this legacy record.");
            }

            if is_baseline {
                ui.small(
                    "This is the earliest exact parent genome in the bounded history; no preceding transition is reconstructed.",
                );
                return;
            }
            match (
                record.get("before_genome").filter(|value| !value.is_null()),
                record.get("after_genome").filter(|value| !value.is_null()),
            ) {
                (Some(before), Some(after)) => {
                    let mut differences = Vec::new();
                    collect_json_differences("genome", before, after, &mut differences, 64);
                    ui.strong("Exact trait transition · before → after");
                    if differences.is_empty() {
                        ui.small("Exact before/after genomes are equal.");
                    } else {
                        egui::Grid::new("selected_evolution_diffs")
                            .num_columns(3)
                            .striped(true)
                            .show(ui, |ui| {
                                ui.strong("Trait path");
                                ui.strong("Before");
                                ui.strong("After");
                                ui.end_row();
                                for (path, before, after) in differences {
                                    ui.monospace(path);
                                    ui.monospace(before);
                                    ui.monospace(after);
                                    ui.end_row();
                                }
                            });
                    }
                }
                _ => {
                    ui.colored_label(
                        egui::Color32::from_rgb(255, 190, 92),
                        "Legacy checkpoint: exact before→after trait diff is unavailable; nothing is inferred.",
                    );
                }
            };
        });
}

fn development_stage_for_generation(generation: u64) -> &'static str {
    match generation {
        0 => "juvenile",
        1 => "young",
        2 => "mature",
        _ => "evolved",
    }
}

fn live_morph_and_activity(ui: &mut egui::Ui, latest: &Value) {
    ui.heading("Thandorcat Morph");
    egui::Grid::new("live_morph_grid")
        .num_columns(2)
        .striped(true)
        .show(ui, |ui| {
            for (label, paths) in [
                (
                    "Command",
                    &[
                        "/details/morph/command",
                        "/details/morph_diagnostics/command",
                    ][..],
                ),
                (
                    "Attention",
                    &[
                        "/details/morph/attention",
                        "/details/morph_diagnostics/attention",
                    ][..],
                ),
                (
                    "Object slot",
                    &[
                        "/details/morph/attention_object_slot",
                        "/details/morph_diagnostics/attention_object_slot",
                    ][..],
                ),
                (
                    "Object target",
                    &[
                        "/details/morph/object_target",
                        "/details/morph_diagnostics/object_target",
                    ][..],
                ),
                (
                    "Reward trace",
                    &[
                        "/details/morph_diagnostics/reward_trace",
                        "/details/morph/diagnostics/reward_trace",
                        "/details/morph/reward_trace",
                    ][..],
                ),
                (
                    "Network age",
                    &[
                        "/details/morph_diagnostics/upstream_age_ms",
                        "/details/morph/diagnostics/upstream_age_ms",
                        "/details/morph_diagnostics/age",
                        "/details/morph/diagnostics/age",
                        "/details/morph/age",
                    ][..],
                ),
                (
                    "Spike count",
                    &[
                        "/details/morph_diagnostics/current_spike_count",
                        "/details/morph/diagnostics/current_spike_count",
                        "/details/morph_diagnostics/spike_count",
                        "/details/morph/diagnostics/spike_count",
                        "/details/morph/spike_count",
                    ][..],
                ),
            ] {
                live_metric(
                    ui,
                    label,
                    first_scalar(latest, paths).unwrap_or_else(|| "—".into()),
                );
            }
            let mean = first_scalar(
                latest,
                &[
                    "/details/morph_diagnostics/mean_population_rate",
                    "/details/morph/diagnostics/mean_population_rate",
                    "/details/morph_diagnostics/mean_rate",
                    "/details/morph/diagnostics/mean_rate",
                    "/details/morph/mean_rate",
                ],
            );
            let maximum = first_scalar(
                latest,
                &[
                    "/details/morph_diagnostics/max_population_rate",
                    "/details/morph/diagnostics/max_population_rate",
                    "/details/morph_diagnostics/max_rate",
                    "/details/morph/diagnostics/max_rate",
                    "/details/morph/max_rate",
                ],
            );
            live_metric(
                ui,
                "Mean / max (split)",
                format!(
                    "{} / {}",
                    mean.unwrap_or_else(|| "—".into()),
                    maximum.unwrap_or_else(|| "—".into())
                ),
            );
        });

    let population_rates = pointer_any(
        latest,
        &[
            "/details/morph_diagnostics/population_rates",
            "/details/morph/diagnostics/population_rates",
            "/details/morph/population_rates",
            "/details/morph_diagnostics/populations",
        ],
    );
    ui.strong("Named population rates");
    if let Some(rates) = population_rates {
        egui::Grid::new("live_morph_populations")
            .num_columns(4)
            .striped(true)
            .show(ui, |ui| {
                let mut shown = 0;
                for name in [
                    "EXP", "PROX", "MOT", "TCH", "VIB", "LOOM", "HAB", "NOV", "KC", "VALP", "VALN",
                    "MBON_A", "MBON_V", "ATT", "REST",
                ] {
                    if let Some(rate) = named_population_rate(rates, name) {
                        ui.monospace(name);
                        ui.monospace(format!("{rate:.3}"));
                        shown += 1;
                        if shown % 2 == 0 {
                            ui.end_row();
                        }
                    }
                }
                if shown % 2 != 0 {
                    ui.end_row();
                }
                if shown == 0 {
                    ui.label("Population diagnostics are present but use an unknown schema.");
                    ui.end_row();
                }
            });
    } else {
        ui.small("— not emitted by this runtime build");
    }

    if let Some(weights) = pointer_any(
        latest,
        &[
            "/details/morph_diagnostics/weight_ranges",
            "/details/morph/diagnostics/weight_ranges",
            "/details/morph/weight_ranges",
            "/details/morph_diagnostics/weights",
        ],
    ) {
        ui.strong("Weight ranges");
        scalar_table(ui, "live_morph_weights", weights, 18);
    }
    for (label, pointers, grid_id) in [
        (
            "Classical weights",
            &[
                "/details/morph_diagnostics/classical_weights",
                "/details/morph/diagnostics/classical_weights",
            ][..],
            "live_morph_classical_weights",
        ),
        (
            "Operant weights",
            &[
                "/details/morph_diagnostics/operant_weights",
                "/details/morph/diagnostics/operant_weights",
            ][..],
            "live_morph_operant_weights",
        ),
    ] {
        if let Some(weights) = pointer_any(latest, pointers) {
            ui.strong(label);
            scalar_table(ui, grid_id, weights, 12);
        }
    }

    ui.add_space(6.0);
    ui.strong("Thandorcat ActivitySystem mapping");
    if let Some(activity) = latest.pointer("/details/activity") {
        scalar_table(ui, "live_activity_grid", activity, 96);
    } else {
        egui::Grid::new("live_activity_fallback")
            .num_columns(2)
            .show(ui, |ui| {
                live_metric(ui, "Activity", text(latest, "/details/action"));
                live_metric(ui, "Phase", text(latest, "/details/ecology/phase"));
                live_metric(ui, "Reason", text(latest, "/details/ecology/reason"));
            });
        ui.small("Mapped ActivitySystem payload is absent; showing runtime fallbacks.");
    }
}

fn live_vita_and_fusion(ui: &mut egui::Ui, latest: &Value) {
    ui.heading("VITA / Fusion");
    egui::Grid::new("live_vita_grid")
        .num_columns(2)
        .striped(true)
        .show(ui, |ui| {
            for (label, paths) in [
                (
                    "Attention",
                    &[
                        "/details/vita/attention/kind",
                        "/details/vita/output/attention/kind",
                        "/details/vita_attention",
                    ][..],
                ),
                (
                    "Emotion",
                    &["/details/vita/emotion", "/details/vita_emotion"][..],
                ),
                (
                    "Influence",
                    &[
                        "/details/vita/influence/mode",
                        "/details/vita/output/influence/strategy",
                        "/details/vita_influence",
                    ][..],
                ),
                (
                    "Agency",
                    &[
                        "/details/vita/self_model/agency",
                        "/details/vita/agency",
                        "/details/vita_agency",
                    ][..],
                ),
                (
                    "Prediction error",
                    &[
                        "/details/vita/self_model/prediction_error",
                        "/details/vita/prediction_error",
                        "/details/vita_prediction_error",
                    ][..],
                ),
                (
                    "Uncertainty",
                    &[
                        "/details/vita/self_model/uncertainty",
                        "/details/vita/uncertainty",
                        "/details/vita_uncertainty",
                    ][..],
                ),
                (
                    "Calibration urge",
                    &[
                        "/details/vita/self_model/calibration_urge",
                        "/details/vita/calibration_urge",
                        "/details/vita_calibration_urge",
                    ][..],
                ),
            ] {
                live_metric(
                    ui,
                    label,
                    first_scalar(latest, paths).unwrap_or_else(|| "—".into()),
                );
            }
        });
    if let Some(vita) = latest.pointer("/details/vita") {
        CollapsingHeader::new("VITA state detail").show(ui, |ui| {
            scalar_table(ui, "live_vita_detail_grid", vita, 72);
        });
    }
    if let Some(fusion) = latest
        .pointer("/details/fusion")
        .filter(|value| !value.is_null())
    {
        ui.strong("Fusion authority");
        scalar_table(ui, "live_fusion_grid", fusion, 24);
    } else {
        ui.small("Fusion diagnostics unavailable in this frame.");
    }
}

fn live_learning_memory_social(ui: &mut egui::Ui, latest: &Value) {
    CollapsingHeader::new("Learning / memory / social")
        .default_open(true)
        .show(ui, |ui| {
            egui::Grid::new("live_learning_core")
                .num_columns(2)
                .striped(true)
                .show(ui, |ui| {
                    for (label, paths) in [
                        (
                            "Attention budget",
                            &[
                                "/details/lifecore/attention/budget",
                                "/details/learning/attention_budget",
                                "/details/attention_budget",
                            ][..],
                        ),
                        (
                            "Predicted action value",
                            &[
                                "/details/lifecore/action/predicted_value",
                                "/details/learning/predicted_action_value",
                                "/details/predicted_action_value",
                            ][..],
                        ),
                        (
                            "Recent reward",
                            &[
                                "/details/lifecore/recent_reward",
                                "/details/learning/recent_reward",
                                "/details/recent_reward",
                            ][..],
                        ),
                        (
                            "Plastic weight range",
                            &[
                                "/details/lifecore/learning/plastic_weight_range",
                                "/details/learning/plastic_weight_range",
                                "/details/plastic_weight_range",
                            ][..],
                        ),
                        (
                            "Attachment",
                            &[
                                "/details/lifecore/affect/attachment",
                                "/details/social/attachment",
                                "/details/attachment",
                            ][..],
                        ),
                        (
                            "Selected motif",
                            &[
                                "/details/lifecore/learning/vocal_repertoire/selected_motif_id",
                                "/details/learning/selected_motif",
                                "/details/selected_motif",
                            ][..],
                        ),
                    ] {
                        live_metric(
                            ui,
                            label,
                            first_scalar(latest, paths).unwrap_or_else(|| "—".into()),
                        );
                    }
                });
            for (label, pointers, grid_id) in [
                (
                    "Learning detail",
                    &[
                        "/details/lifecore/learning",
                        "/details/learning",
                        "/details/learning_state",
                    ][..],
                    "live_learning_detail",
                ),
                (
                    "Memory",
                    &[
                        "/details/lifecore/learning/memory",
                        "/details/memory",
                        "/details/memory_state",
                    ][..],
                    "live_memory_detail",
                ),
                (
                    "Social",
                    &["/details/social", "/details/social_state"][..],
                    "live_social_detail",
                ),
            ] {
                if let Some(group) = pointer_any(latest, pointers) {
                    CollapsingHeader::new(label).show(ui, |ui| {
                        scalar_table(ui, grid_id, group, 36);
                    });
                }
            }
        });
}

fn live_raw_json(ui: &mut egui::Ui, latest: &Value) {
    CollapsingHeader::new("Raw telemetry JSON")
        .default_open(false)
        .show(ui, |ui| {
            ui.small("Recorded frame exactly as received; read-only.");
            let pretty = serde_json::to_string_pretty(latest)
                .unwrap_or_else(|error| format!("<cannot format JSON: {error}>"));
            egui::ScrollArea::vertical()
                .id_salt("raw_telemetry_scroll")
                .max_height(520.0)
                .show(ui, |ui| {
                    ui.monospace(pretty);
                });
        });
}

fn live_chart(ui: &mut egui::Ui, history: &VecDeque<LiveGraphSample>, selected: Option<usize>) {
    ui.horizontal_wrapped(|ui| {
        ui.colored_label(egui::Color32::from_rgb(93, 210, 255), "speed");
        ui.colored_label(egui::Color32::from_rgb(255, 105, 96), "pressure");
        ui.colored_label(egui::Color32::from_rgb(105, 232, 172), "deformation");
        ui.colored_label(egui::Color32::from_rgb(198, 130, 255), "visual blend");
        ui.colored_label(egui::Color32::from_rgb(255, 205, 92), "orb speed");
    });
    let width = ui.available_width().max(100.0);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 125.0), Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 4.0, egui::Color32::from_rgb(7, 9, 14));
    if history.len() < 2 {
        return;
    }
    // Frame order is a stable x-axis even across Pet restarts, where the
    // monotonic timestamp can reset inside the same rotated log.
    let x_denominator = history.len().saturating_sub(1).max(1) as f32;
    let speed_scale = history
        .iter()
        .map(|sample| sample.speed_px)
        .fold(0.0_f32, f32::max)
        .mul_add(1.10, 0.0)
        .max(360.0);
    let draw = |values: Vec<f32>, maximum: f32, color: egui::Color32| {
        let points = values
            .into_iter()
            .enumerate()
            .map(|(index, value)| {
                egui::pos2(
                    rect.left() + index as f32 / x_denominator * rect.width(),
                    rect.bottom() - (value / maximum).clamp(0.0, 1.0) * rect.height(),
                )
            })
            .collect::<Vec<_>>();
        painter.add(egui::Shape::line(points, egui::Stroke::new(1.5, color)));
    };
    draw(
        history.iter().map(|sample| sample.speed_px).collect(),
        speed_scale,
        egui::Color32::from_rgb(93, 210, 255),
    );
    draw(
        history.iter().map(|sample| sample.pressure).collect(),
        1.0,
        egui::Color32::from_rgb(255, 105, 96),
    );
    draw(
        history.iter().map(|sample| sample.deformation).collect(),
        0.35,
        egui::Color32::from_rgb(105, 232, 172),
    );
    draw(
        history.iter().map(|sample| sample.visual_blend).collect(),
        0.65,
        egui::Color32::from_rgb(198, 130, 255),
    );
    draw(
        history.iter().map(|sample| sample.orb_speed).collect(),
        1.5,
        egui::Color32::from_rgb(255, 205, 92),
    );
    if let Some(selected) = selected {
        let x = rect.left()
            + selected.min(history.len().saturating_sub(1)) as f32 / x_denominator * rect.width();
        painter.line_segment(
            [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
            egui::Stroke::new(1.0, egui::Color32::WHITE.gamma_multiply(0.7)),
        );
    }
}

fn live_metric(ui: &mut egui::Ui, label: &str, value: String) {
    ui.label(label);
    ui.monospace(if value.is_empty() { "—" } else { &value });
    ui.end_row();
}

fn text(value: &Value, pointer: &str) -> String {
    value
        .pointer(pointer)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

fn pointer_any<'a>(value: &'a Value, pointers: &[&str]) -> Option<&'a Value> {
    pointers.iter().find_map(|pointer| value.pointer(pointer))
}

fn first_scalar(value: &Value, pointers: &[&str]) -> Option<String> {
    pointer_any(value, pointers).map(compact_json)
}

fn compact_json(value: &Value) -> String {
    match value {
        Value::Null => "—".into(),
        Value::Bool(value) => value.to_string(),
        Value::Number(value) => value.to_string(),
        Value::String(value) => value.clone(),
        Value::Array(values) if values.len() <= 8 => values
            .iter()
            .map(compact_json)
            .collect::<Vec<_>>()
            .join(", "),
        Value::Array(values) => format!("[{} values]", values.len()),
        Value::Object(values) => format!("{{{} fields}}", values.len()),
    }
}

fn scalar_table(ui: &mut egui::Ui, id: impl std::hash::Hash, value: &Value, limit: usize) {
    let mut rows = Vec::new();
    collect_scalar_rows(value, "", &mut rows, limit, 0);
    egui::Grid::new(id)
        .num_columns(2)
        .striped(true)
        .show(ui, |ui| {
            if rows.is_empty() {
                ui.label("—");
                ui.label("No scalar fields");
                ui.end_row();
            } else {
                for (label, value) in rows {
                    ui.label(label);
                    ui.monospace(value);
                    ui.end_row();
                }
            }
        });
}

fn collect_scalar_rows(
    value: &Value,
    prefix: &str,
    rows: &mut Vec<(String, String)>,
    limit: usize,
    depth: usize,
) {
    if rows.len() >= limit || depth > 4 {
        return;
    }
    match value {
        Value::Object(object) => {
            let mut keys = object.keys().collect::<Vec<_>>();
            keys.sort();
            for key in keys {
                if rows.len() >= limit {
                    break;
                }
                let label = if prefix.is_empty() {
                    key.clone()
                } else {
                    format!("{prefix}.{key}")
                };
                collect_scalar_rows(&object[key], &label, rows, limit, depth + 1);
            }
        }
        Value::Array(values)
            if values.len() <= 12
                && values
                    .iter()
                    .all(|value| !value.is_array() && !value.is_object()) =>
        {
            rows.push((prefix.to_owned(), compact_json(value)));
        }
        Value::Array(values) => {
            for (index, value) in values.iter().take(8).enumerate() {
                if rows.len() >= limit {
                    break;
                }
                collect_scalar_rows(value, &format!("{prefix}[{index}]"), rows, limit, depth + 1);
            }
            if values.len() > 8 && rows.len() < limit {
                rows.push((prefix.to_owned(), format!("{} entries", values.len())));
            }
        }
        _ => rows.push((
            if prefix.is_empty() {
                "value".into()
            } else {
                prefix.to_owned()
            },
            compact_json(value),
        )),
    }
}

fn named_population_rate(value: &Value, name: &str) -> Option<f64> {
    if let Some(object) = value.as_object() {
        return object
            .get(name)
            .or_else(|| object.get(&name.to_ascii_lowercase()))
            .and_then(|value| {
                value
                    .as_f64()
                    .or_else(|| value.get("rate").and_then(Value::as_f64))
            });
    }
    value.as_array()?.iter().find_map(|population| {
        let population_name = population
            .get("name")
            .or_else(|| population.get("population"))
            .and_then(Value::as_str)?;
        population_name.eq_ignore_ascii_case(name).then(|| {
            population
                .get("rate")
                .or_else(|| population.get("mean_rate"))
                .and_then(Value::as_f64)
        })?
    })
}

fn collect_json_differences(
    path: &str,
    before: &Value,
    after: &Value,
    differences: &mut Vec<(String, String, String)>,
    limit: usize,
) {
    if before == after || differences.len() >= limit {
        return;
    }
    match (before, after) {
        (Value::Object(before), Value::Object(after)) => {
            let keys = before
                .keys()
                .chain(after.keys())
                .cloned()
                .collect::<BTreeSet<_>>();
            for key in keys {
                if differences.len() >= limit {
                    break;
                }
                let child_path = format!("{path}.{key}");
                match (before.get(&key), after.get(&key)) {
                    (Some(before), Some(after)) => {
                        collect_json_differences(&child_path, before, after, differences, limit)
                    }
                    (before, after) => differences.push((
                        child_path,
                        before.map_or_else(|| "<missing>".into(), compact_json),
                        after.map_or_else(|| "<missing>".into(), compact_json),
                    )),
                }
            }
        }
        (Value::Array(before), Value::Array(after)) if before.len().max(after.len()) <= 32 => {
            for index in 0..before.len().max(after.len()) {
                if differences.len() >= limit {
                    break;
                }
                let child_path = format!("{path}[{index}]");
                match (before.get(index), after.get(index)) {
                    (Some(before), Some(after)) => {
                        collect_json_differences(&child_path, before, after, differences, limit)
                    }
                    (before, after) => differences.push((
                        child_path,
                        before.map_or_else(|| "<missing>".into(), compact_json),
                        after.map_or_else(|| "<missing>".into(), compact_json),
                    )),
                }
            }
        }
        _ => differences.push((path.to_owned(), compact_json(before), compact_json(after))),
    }
}

fn number(value: &Value, pointer: &str) -> Option<f64> {
    value.pointer(pointer).and_then(Value::as_f64)
}

fn format_number(value: &Value, pointer: &str, decimals: usize) -> String {
    number(value, pointer).map_or_else(|| "—".into(), |value| format!("{value:.decimals$}"))
}

fn array_number(value: &Value, pointer: &str, index: usize, decimals: usize) -> String {
    value
        .pointer(pointer)
        .and_then(Value::as_array)
        .and_then(|values| values.get(index))
        .and_then(Value::as_f64)
        .map_or_else(|| "—".into(), |value| format!("{value:.decimals$}"))
}

fn boolean(value: &Value, pointer: &str) -> bool {
    value
        .pointer(pointer)
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

fn vector2(value: &Value, pointer: &str) -> Vec2 {
    let Some(values) = value.pointer(pointer).and_then(Value::as_array) else {
        return Vec2::ZERO;
    };
    Vec2::new(
        values.first().and_then(Value::as_f64).unwrap_or(0.0) as f32,
        values.get(1).and_then(Value::as_f64).unwrap_or(0.0) as f32,
    )
}

fn optional_vector2(value: &Value, pointer: &str) -> Option<Vec2> {
    let values = value.pointer(pointer)?.as_array()?;
    Some(Vec2::new(
        values.first()?.as_f64()? as f32,
        values.get(1)?.as_f64()? as f32,
    ))
    .filter(|point| point.is_finite())
}

fn format_vector(value: &Value, pointer: &str) -> String {
    optional_vector2(value, pointer).map_or_else(
        || "—".into(),
        |point| format!("{:.3}, {:.3}", point.x, point.y),
    )
}

impl LabUi {
    fn show(
        &mut self,
        context: &Context,
        diagnostics: LiquidDiagnostics,
        pupil_size: f32,
        pupil_asymmetry: f32,
        fps: f32,
        p95_frame_ms: f32,
    ) {
        self.poll_pet_acknowledgement();
        egui::SidePanel::right("liquid_controls")
            .default_width(390.0)
            .min_width(330.0)
            .resizable(true)
            .show(context, |ui| {
                ui.heading("Character editor");
                ui.label("Appearance, liquid motion, face, voice, and preview scenarios");
                ui.separator();
                self.transport(ui);
                ui.separator();
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        self.preset_controls(ui);
                        self.physics_controls(ui);
                        self.material_controls(ui);
                        self.face_controls(ui);
                        self.compositor_controls(ui);
                        self.diagnostics(
                            ui,
                            diagnostics,
                            pupil_size,
                            pupil_asymmetry,
                            fps,
                            p95_frame_ms,
                        );
                        self.persistence_controls(ui);
                    });
            });
        self.preview_canvas(context);
    }

    fn preview_canvas(&mut self, context: &Context) {
        self.drag.pressed = false;
        self.drag.released = false;
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(context, |ui| {
                let canvas = ui.max_rect();
                let response = ui.allocate_rect(canvas, Sense::drag());
                let content = context.content_rect();
                let size = content.size().max(egui::Vec2::splat(1.0));
                // The right-side controls are not part of the visible simulation
                // canvas. Preserve horizontal room for the resting body, while a
                // small vertical inset keeps reconstructed kernels off the frame.
                let padded_canvas = egui::Rect::from_min_max(
                    canvas.min + egui::vec2(0.0, 20.0),
                    canvas.max - egui::vec2(0.0, 20.0),
                );
                self.preview_bounds_min = Vec2::new(
                    ((padded_canvas.min.x - content.min.x) / size.x).clamp(0.0, 1.0),
                    ((padded_canvas.min.y - content.min.y) / size.y).clamp(0.0, 1.0),
                );
                self.preview_bounds_max = Vec2::new(
                    ((padded_canvas.max.x - content.min.x) / size.x).clamp(0.0, 1.0),
                    ((padded_canvas.max.y - content.min.y) / size.y).clamp(0.0, 1.0),
                );
                if let Some(pointer) = response.interact_pointer_pos() {
                    self.drag.pointer_normalized = Vec2::new(
                        ((pointer.x - content.min.x) / size.x).clamp(0.0, 1.0),
                        ((pointer.y - content.min.y) / size.y).clamp(0.0, 1.0),
                    );
                }
                let was_active = self.drag.active;
                self.drag.active = response.dragged();
                self.drag.pressed = response.drag_started();
                self.drag.released = response.drag_stopped() || was_active && !self.drag.active;
                let (pointer_velocity, frame_dt) = context.input(|input| {
                    (
                        input.pointer.velocity(),
                        input.stable_dt.clamp(1.0 / 240.0, 0.05),
                    )
                });
                let next_velocity = if self.drag.active {
                    Vec2::new(pointer_velocity.x / size.x, pointer_velocity.y / size.y)
                        .clamp_length_max(3.0)
                } else if self.drag.released {
                    self.drag.velocity_normalized
                } else {
                    self.drag.velocity_normalized * 0.72
                };
                self.drag.acceleration_normalized =
                    ((next_velocity - self.drag.velocity_normalized) / frame_dt)
                        .clamp_length_max(24.0);
                self.drag.velocity_normalized = next_velocity;
                if response.hovered() || self.drag.active {
                    context.set_cursor_icon(if self.drag.active {
                        egui::CursorIcon::Grabbing
                    } else {
                        egui::CursorIcon::Grab
                    });
                }
                ui.painter().text(
                    canvas.left_top() + egui::vec2(14.0, 14.0),
                    egui::Align2::LEFT_TOP,
                    "Drag the liquid with the mouse · release to inspect slosh",
                    egui::FontId::proportional(14.0),
                    egui::Color32::from_white_alpha(180),
                );
            });
    }

    fn transport(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if ui
                .button(if self.playing { "Pause" } else { "Play" })
                .clicked()
            {
                self.playing = !self.playing;
            }
            if ui.button("Step").clicked() {
                self.single_step = true;
            }
            if ui.button("Reset same seed").clicked() {
                self.reset_requested = true;
            }
        });
        if ui.button("▶ Test fluid flight").clicked() {
            self.scenario = PreviewScenario::FluidFlight;
            self.playing = true;
            self.reset_requested = true;
            self.status =
                "Fluid-flight loop: launch, bank, reversal, dive, brake, settle.".to_owned();
        }
        ui.horizontal(|ui| {
            if ui.button("🔊 Test voice").clicked() {
                self.test_voice_requested = true;
            }
            ui.label(format!(
                "{} · RMS {:.3} · peak {:.3}",
                self.audio_status, self.audio_rms, self.audio_peak
            ));
        });
        egui::ComboBox::from_label("Motion scenario")
            .selected_text(scenario_name(self.scenario))
            .show_ui(ui, |ui| {
                for scenario in all_scenarios() {
                    ui.selectable_value(&mut self.scenario, scenario, scenario_name(scenario));
                }
            });
        egui::ComboBox::from_label("Face emotion")
            .selected_text(emotion_name(self.preview_emotion))
            .show_ui(ui, |ui| {
                for emotion in all_preview_emotions() {
                    ui.selectable_value(&mut self.preview_emotion, emotion, emotion_name(emotion));
                }
            });
        ui.add(
            Slider::new(&mut self.preview_emotion_intensity, 0.0..=1.0).text("Emotion intensity"),
        );
        ui.add(Slider::new(&mut self.time_scale, 0.05..=2.0).text("Simulation speed"));
        egui::ComboBox::from_label("Background")
            .selected_text(background_name(self.background))
            .show_ui(ui, |ui| {
                for index in 0..BACKGROUNDS.len() {
                    ui.selectable_value(&mut self.background, index, background_name(index));
                }
            });
        egui::ComboBox::from_label("Debug view")
            .selected_text(debug_view_name(self.debug_view))
            .show_ui(ui, |ui| {
                for view in all_debug_views() {
                    ui.selectable_value(&mut self.debug_view, view, debug_view_name(view));
                }
            });
    }

    fn preset_controls(&mut self, ui: &mut egui::Ui) {
        CollapsingHeader::new("Design presets")
            .default_open(true)
            .show(ui, |ui| {
                let mut next_selection = self.selected_preset.clone();
                egui::ComboBox::from_label("Preset")
                    .selected_text(self.selected_preset_label())
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut next_selection,
                            PresetSelection::AuthoredCurrent,
                            "1 · Authored current",
                        );
                        ui.selectable_value(
                            &mut next_selection,
                            PresetSelection::MoonlitGlass,
                            "2 · Moonlit Glass",
                        );
                        if !self.user_presets.is_empty() {
                            ui.separator();
                            for preset in &self.user_presets {
                                ui.selectable_value(
                                    &mut next_selection,
                                    PresetSelection::User(preset.id.clone()),
                                    format!("User · {}", preset.display_name),
                                );
                            }
                        }
                    });
                if next_selection != self.selected_preset {
                    self.select_design_preset(next_selection);
                }
                ui.small(self.selected_preset_description());
                if let Some(directory) = &self.preset_directory {
                    ui.small(format!("User preset folder: {}", directory.display()));
                }
                ui.label("User preset name");
                ui.text_edit_singleline(&mut self.preset_name_buffer);
                ui.horizontal_wrapped(|ui| {
                    if ui.button("Save / update user preset").clicked() {
                        self.save_current_user_preset();
                    }
                    let user_selected = matches!(self.selected_preset, PresetSelection::User(_));
                    if ui
                        .add_enabled(user_selected, egui::Button::new("Rename selected"))
                        .clicked()
                    {
                        self.rename_selected_user_preset();
                    }
                    let delete_label = if self.delete_preset_armed {
                        "Confirm delete"
                    } else {
                        "Delete selected"
                    };
                    if ui
                        .add_enabled(user_selected, egui::Button::new(delete_label))
                        .clicked()
                    {
                        self.delete_selected_user_preset();
                    }
                    if ui.button("Reload list").clicked() {
                        match self.refresh_user_preset_catalog() {
                            Ok(skipped) if skipped > 0 => {
                                self.status =
                                    format!("Reloaded presets; skipped {skipped} invalid file(s).")
                            }
                            Ok(_) => self.status = "Reloaded user presets.".to_owned(),
                            Err(error) => self.status = format!("Preset reload failed: {error}"),
                        }
                    }
                });
            });
    }

    fn selected_preset_label(&self) -> String {
        match &self.selected_preset {
            PresetSelection::AuthoredCurrent => "1 · Authored current".to_owned(),
            PresetSelection::MoonlitGlass => "2 · Moonlit Glass".to_owned(),
            PresetSelection::User(id) => self
                .user_presets
                .iter()
                .find(|preset| &preset.id == id)
                .map_or_else(
                    || "User · missing preset".to_owned(),
                    |preset| format!("User · {}", preset.display_name),
                ),
        }
    }

    fn selected_preset_description(&self) -> String {
        match &self.selected_preset {
            PresetSelection::AuthoredCurrent => {
                "The profile that was active when Dev Console opened; kept intact as the first choice."
                    .to_owned()
            }
            PresetSelection::MoonlitGlass => {
                "Cool translucent glass, warm inner light, softer low-viscosity motion, and the same stable v16 solver lane."
                    .to_owned()
            }
            PresetSelection::User(id) => self
                .user_presets
                .iter()
                .find(|preset| &preset.id == id)
                .map_or_else(
                    || "This user preset is no longer available on disk.".to_owned(),
                    |preset| format!("Loaded from {}", preset.path.display()),
                ),
        }
    }

    fn select_design_preset(&mut self, selection: PresetSelection) {
        let loaded = match &selection {
            PresetSelection::AuthoredCurrent => Some((
                self.authored_profile.clone(),
                format!("{} copy", self.authored_profile.name),
                "Loaded the preserved authored-current preset.".to_owned(),
            )),
            PresetSelection::MoonlitGlass => Some((
                moonlit_glass_preset(self.authored_profile.seed),
                "Moonlit Glass copy".to_owned(),
                "Loaded built-in Moonlit Glass.".to_owned(),
            )),
            PresetSelection::User(id) => self
                .user_presets
                .iter()
                .find(|preset| &preset.id == id)
                .map(|preset| {
                    (
                        preset.profile.clone(),
                        preset.display_name.clone(),
                        format!("Loaded user preset {}.", preset.display_name),
                    )
                }),
        };
        let Some((mut profile, name_buffer, status)) = loaded else {
            self.status = "Preset load failed: the selected file is missing.".to_owned();
            return;
        };
        profile.render_mode = BodyRenderMode::ParticlePbf;
        self.profile = profile;
        self.selected_preset = selection;
        self.preset_name_buffer = name_buffer;
        self.delete_preset_armed = false;
        self.reset_requested = true;
        self.refresh_json();
        self.status = status;
    }

    fn refresh_user_preset_catalog(&mut self) -> Result<usize, String> {
        let directory = self
            .preset_directory
            .as_deref()
            .ok_or_else(|| "Pet data directory is unavailable".to_owned())?;
        let (presets, skipped) = discover_user_presets(directory)?;
        self.user_presets = presets;
        let selected_user_is_missing = match &self.selected_preset {
            PresetSelection::User(id) => !self.user_presets.iter().any(|preset| &preset.id == id),
            PresetSelection::AuthoredCurrent | PresetSelection::MoonlitGlass => false,
        };
        if selected_user_is_missing {
            self.select_design_preset(PresetSelection::AuthoredCurrent);
        }
        Ok(skipped)
    }

    fn save_current_user_preset(&mut self) {
        let Some(directory) = self.preset_directory.clone() else {
            self.status = "Pet data directory is unavailable; preset was not saved.".to_owned();
            return;
        };
        match save_user_preset(&directory, self.preset_name_buffer.trim(), &self.profile) {
            Ok(saved) => {
                let selected = PresetSelection::User(saved.id.clone());
                let display_name = saved.display_name.clone();
                match self.refresh_user_preset_catalog() {
                    Ok(_) => {
                        self.select_design_preset(selected);
                        self.status = format!("Saved user preset {display_name}.");
                    }
                    Err(error) => {
                        self.status = format!("Preset saved, but list refresh failed: {error}")
                    }
                }
            }
            Err(error) => self.status = format!("Preset save failed: {error}"),
        }
    }

    fn rename_selected_user_preset(&mut self) {
        let PresetSelection::User(id) = self.selected_preset.clone() else {
            self.status = "Only user presets can be renamed.".to_owned();
            return;
        };
        let Some(directory) = self.preset_directory.clone() else {
            self.status = "Pet data directory is unavailable.".to_owned();
            return;
        };
        match rename_user_preset(&directory, &id, self.preset_name_buffer.trim()) {
            Ok(renamed) => {
                let selected = PresetSelection::User(renamed.id.clone());
                let display_name = renamed.display_name.clone();
                match self.refresh_user_preset_catalog() {
                    Ok(_) => {
                        self.select_design_preset(selected);
                        self.status = format!("Renamed user preset to {display_name}.");
                    }
                    Err(error) => {
                        self.status = format!("Preset renamed, but list refresh failed: {error}")
                    }
                }
            }
            Err(error) => self.status = format!("Preset rename failed: {error}"),
        }
    }

    fn delete_selected_user_preset(&mut self) {
        let PresetSelection::User(id) = self.selected_preset.clone() else {
            self.status = "Only user presets can be deleted.".to_owned();
            return;
        };
        if !self.delete_preset_armed {
            self.delete_preset_armed = true;
            self.status =
                "Click Confirm delete to remove this user preset. Active liquid-tuning.json is untouched."
                    .to_owned();
            return;
        }
        self.delete_preset_armed = false;
        let Some(directory) = self.preset_directory.clone() else {
            self.status = "Pet data directory is unavailable.".to_owned();
            return;
        };
        match delete_user_preset(&directory, &id) {
            Ok(()) => {
                let _ = self.refresh_user_preset_catalog();
                self.select_design_preset(PresetSelection::AuthoredCurrent);
                self.status = "Deleted the user preset. Active liquid-tuning.json was not changed."
                    .to_owned();
            }
            Err(error) => self.status = format!("Preset delete failed: {error}"),
        }
    }

    fn physics_controls(&mut self, ui: &mut egui::Ui) {
        CollapsingHeader::new("Stable XPBD liquid")
            .default_open(true)
            .show(ui, |ui| {
                let p = &mut self.profile.pbf;
                ui.small("One 120 Hz solver lane: the same density law runs during idle, pull, flight, and merge.");
                ui.separator();
                ui.label("Permanent character field");
                ui.add(
                    Slider::new(&mut p.character_field_radius_scale, 0.65..=1.5)
                        .text("Character field radius"),
                );
                ui.add(
                    Slider::new(&mut p.return_strength, 0.0..=1.0).text("Character field strength"),
                );
                ui.add(
                    Slider::new(&mut p.flight_stretch, 0.0..=2.0)
                        .text("Flight plasticity"),
                );
                ui.small("Flight plasticity reshapes this same field continuously; 0 keeps the neutral ellipse.");
                ui.separator();
                ui.label("Mouse gravity field");
                ui.small("A compact moving potential acts on real particles; it never captures a rigid chunk.");
                ui.add(
                    Slider::new(&mut p.pointer_support_scale, 1.5..=2.5)
                        .text("Pointer support (h)"),
                );
                ui.add(Slider::new(&mut p.grab_stiffness, 20.0..=400.0).text("Pointer strength"));
                ui.add(
                    Slider::new(&mut p.pointer_response_hz, 4.0..=40.0)
                        .logarithmic(true)
                        .text("Pointer response (Hz)"),
                );
                ui.separator();
                ui.label("Fluid solve and surface");
                ui.add(
                    Slider::new(&mut p.density_compliance, 1.0e-8..=5.0e-4)
                        .logarithmic(true)
                        .text("Density compliance"),
                );
                ui.add(Slider::new(&mut p.density_iterations, 1..=12).text("Density iterations"));
                ui.add(Slider::new(&mut p.surface_tension, 0.0..=2.5).text("Fluid cohesion"));
                ui.add(Slider::new(&mut p.viscosity, 0.0..=0.22).text("Artistic viscosity"));
                ui.add(
                    Slider::new(&mut p.numerical_xsph, 0.005..=0.030)
                        .text("Numerical XSPH"),
                );
                ui.add(Slider::new(&mut p.anisotropy_max, 1.0..=2.15).text("Anisotropy max"));
                ui.small("Legacy grab, bond, tear, impact, return-delay, shape-recovery, and physical idle controls remain load-compatible but are not active authoring knobs.");
                if ui.button("Reset stable liquid physics").clicked() {
                    self.profile.pbf = Default::default();
                }
            });
    }

    fn material_controls(&mut self, ui: &mut egui::Ui) {
        CollapsingHeader::new("Character look / jelly + SSS")
            .default_open(true)
            .show(ui, |ui| {
                let p = &mut self.profile.material;
                ui.label("Material A/B — Current is the preserved incumbent");
                ui.horizontal(|ui| {
                    ui.selectable_value(
                        &mut p.variant,
                        MaterialVariant::CurrentSafe,
                        "Current / Safe",
                    );
                    ui.selectable_value(
                        &mut p.variant,
                        MaterialVariant::CinematicJelly,
                        "Cinematic Jelly",
                    );
                });
                ui.separator();
                ui.checkbox(&mut p.override_genome_colors, "Override genome colors");
                hsv_controls(ui, "Primary HSV", &mut p.primary_hsv);
                hsv_controls(ui, "Secondary HSV", &mut p.secondary_hsv);
                hsv_controls(ui, "Glow HSV", &mut p.glow_hsv);
                ui.add(Slider::new(&mut p.absorption, 0.0..=4.0).text("Absorption"));
                ui.add(Slider::new(&mut p.scattering, 0.0..=4.0).text("SSS / scattering"));
                ui.add(Slider::new(&mut p.thickness, 0.10..=4.0).text("Optical thickness"));
                ui.add(Slider::new(&mut p.translucency, 0.0..=1.0).text("Translucency"));
                ui.add(Slider::new(&mut p.refraction, 0.0..=3.0).text("Refraction"));
                ui.add(Slider::new(&mut p.blur, 0.0..=5.0).text("Transmission blur"));
                ui.small(
                    "Checker background makes refraction and transmission blur easiest to judge.",
                );
                ui.add(Slider::new(&mut p.rim_strength, 0.0..=4.0).text("Rim strength"));
                ui.add(Slider::new(&mut p.rim_power, 0.5..=12.0).text("Rim power"));
                ui.add(Slider::new(&mut p.fresnel_f0, 0.01..=0.12).text("Fresnel F0"));
                ui.add(Slider::new(&mut p.broad_specular, 0.0..=1.0).text("Gloss body"));
                ui.add(Slider::new(&mut p.broad_specular_power, 4.0..=64.0).text("Gloss spread"));
                ui.add(Slider::new(&mut p.tight_specular, 0.0..=1.5).text("Gloss sparkle"));
                ui.add(
                    Slider::new(&mut p.tight_specular_power, 24.0..=192.0).text("Sparkle focus"),
                );
                ui.add(Slider::new(&mut p.core_level, 0.55..=3.0).text("Core density level"));
                ui.add(Slider::new(&mut p.thickness_gamma, 0.25..=2.5).text("Thickness gamma"));
                ui.add(Slider::new(&mut p.pseudo_depth, 0.05..=0.60).text("Pseudo depth"));
                ui.add(Slider::new(&mut p.normal_scale, 0.20..=4.0).text("Normal strength"));
                ui.add(Slider::new(&mut p.light_wrap, 0.0..=1.5).text("SSS light wrap"));
                ui.add(Slider::new(&mut p.ambient_scatter, 0.0..=1.5).text("Ambient scatter"));
                ui.add(Slider::new(&mut p.direct_scatter, 0.0..=2.0).text("Direct scatter"));
                ui.add(
                    Slider::new(&mut p.transmission_hue_preservation, 0.0..=1.0)
                        .text("Transmission hue"),
                );
                ui.add(Slider::new(&mut p.emission, 0.0..=2.0).text("Internal emission"));
                ui.add(Slider::new(&mut p.internal_flow, 0.0..=2.0).text("Living flow"));
                ui.add(Slider::new(&mut p.halo, 0.0..=0.30).text("Halo"));
                ui.add(Slider::new(&mut p.opacity, 0.05..=1.0).text("Opacity"));
                if p.variant == MaterialVariant::CinematicJelly {
                    ui.separator();
                    ui.colored_label(
                        egui::Color32::from_rgb(246, 187, 78),
                        "Cinematic Jelly v3.3 · KEEP EXPERIMENTAL",
                    );
                    ui.label("Cinematic internal volume");
                    ui.add(
                        Slider::new(&mut p.cinematic_smoothing, 0.0..=1.0)
                            .text("Volume normal smoothing"),
                    );
                    ui.add(Slider::new(&mut p.internal_orb_count, 0..=8).text("Glow spheres"));
                    ui.add(
                        Slider::new(&mut p.internal_orb_intensity, 0.0..=4.0)
                            .text("Sphere intensity"),
                    );
                    ui.add(
                        Slider::new(&mut p.internal_orb_size, 0.012..=0.10)
                            .text("Sphere size"),
                    );
                    ui.add(
                        Slider::new(&mut p.internal_orb_halo, 0.0..=3.0)
                            .text("Sphere halo"),
                    );
                    ui.add(
                        Slider::new(&mut p.internal_orb_speed, 0.0..=2.0)
                            .text("Sphere flow speed"),
                    );
                    ui.add(
                        Slider::new(&mut p.internal_orb_depth, 0.0..=1.0)
                            .text("Sphere depth spread"),
                    );
                    ui.add(
                        Slider::new(&mut p.internal_orb_spread, 0.0..=1.0)
                            .text("Internal bubble spread"),
                    );
                    ui.separator();
                    ui.label("Cinematic v3 surface / lighting");
                    ui.add(
                        Slider::new(&mut p.studio_intensity, 0.0..=4.0)
                            .text("Studio reflection"),
                    );
                    ui.add(
                        Slider::new(&mut p.studio_base_roughness, 0.05..=1.0)
                            .text("Gel roughness"),
                    );
                    ui.add(
                        Slider::new(&mut p.studio_coat_roughness, 0.02..=0.50)
                            .text("Wet coat roughness"),
                    );
                    ui.add(
                        Slider::new(&mut p.edge_light_width, 2.0..=32.0)
                            .text("Broad rim width px"),
                    );
                    ui.add(
                        Slider::new(&mut p.narrow_rim_strength, 0.0..=3.0)
                            .text("Narrow rim"),
                    );
                    ui.add(
                        Slider::new(&mut p.broad_rim_strength, 0.0..=3.0)
                            .text("Broad rim"),
                    );
                    ui.add(
                        Slider::new(&mut p.rim_saturation, 0.0..=1.6)
                            .text("Rim saturation"),
                    );
                    ui.add(
                        Slider::new(&mut p.caustic_strength, 0.0..=4.0)
                            .text("Flow caustics"),
                    );
                    ui.add(
                        Slider::new(&mut p.caustic_scale, 0.5..=12.0)
                            .text("Caustic scale"),
                    );
                    ui.add(
                        Slider::new(&mut p.caustic_speed, 0.0..=2.0)
                            .text("Caustic drift"),
                    );
                    ui.add(
                        Slider::new(&mut p.caustic_dispersion, 0.0..=1.0)
                            .text("Caustic RGB dispersion"),
                    );
                    ui.add(
                        Slider::new(&mut p.rounded_highlight_strength, 0.0..=3.0)
                            .text("Rounded highlights"),
                    );
                    ui.add(
                        Slider::new(&mut p.highlight_tint, 0.0..=0.5)
                            .text("Highlight body tint"),
                    );
                    ui.separator();
                    ui.label("Soul glow");
                    ui.add(Slider::new(&mut p.soul_glow_count, 0..=6).text("Soul lobes"));
                    ui.add(
                        Slider::new(&mut p.soul_glow_strength, 0.0..=2.0)
                            .text("Soul strength"),
                    );
                    ui.add(
                        Slider::new(&mut p.soul_glow_size, 0.04..=0.30)
                            .text("Soul lobe size"),
                    );
                    ui.add(
                        Slider::new(&mut p.soul_glow_speed, 0.0..=0.5)
                            .text("Soul drift speed"),
                    );
                    ui.add(
                        Slider::new(&mut p.soul_glow_pulse, 0.0..=0.6)
                            .text("Soul opacity pulse"),
                    );
                    ui.add(
                        Slider::new(&mut p.soul_glow_feather, 0.2..=1.5)
                            .text("Soul feather"),
                    );
                    ui.add(
                        Slider::new(&mut p.bloom_strength, 0.0..=3.0)
                            .text("HDR bloom"),
                    );
                    ui.small(
                        "Body, buds and detached bubbles share one implicit material field; studio debug uses R=gel, G=coat, B=fill.",
                    );
                }
                if ui.button("Reset material").clicked() {
                    self.profile.material = Default::default();
                }
            });
    }

    fn face_controls(&mut self, ui: &mut egui::Ui) {
        CollapsingHeader::new("Face anchor").show(ui, |ui| {
            let p = &mut self.profile.face;
            ui.checkbox(&mut p.visible, "Show face");
            ui.checkbox(&mut p.override_iris_color, "Use custom iris color");
            ui.add_enabled_ui(p.override_iris_color, |ui| {
                hsv_controls(ui, "Iris HSV", &mut p.iris_hsv);
            });
            ui.add(Slider::new(&mut p.origin[0], -0.25..=0.25).text("Origin X"));
            if ui.button("Center face horizontally").clicked() {
                p.origin[0] = 0.0;
            }
            ui.add(Slider::new(&mut p.origin[1], -0.15..=0.35).text("Origin Y"));
            ui.add(Slider::new(&mut p.scale[0], 0.55..=1.50).text("Face scale X"));
            ui.add(Slider::new(&mut p.scale[1], 0.55..=1.50).text("Face scale Y"));
            ui.add(Slider::new(&mut p.eye_size_scale, 0.55..=2.0).text("Eye size"));
            ui.add(Slider::new(&mut p.eye_spacing_scale, 0.60..=1.50).text("Eye spacing"));
            ui.add(Slider::new(&mut p.pupil_scale, 0.50..=1.50).text("Pupil scale"));
            ui.add(Slider::new(&mut p.eye_highlight_scale, 0.50..=2.50).text("Eye highlight size"));
            ui.add(Slider::new(&mut p.eye_socket_strength, 0.0..=1.0).text("Wet eye socket"));
            ui.add(Slider::new(&mut p.relief_strength, 0.0..=1.5).text("Mouth / brow wet relief"));
            ui.add(Slider::new(&mut p.relief_darkness, 0.35..=0.95).text("Relief darkness"));
            ui.add(Slider::new(&mut p.relief_coat_strength, 0.0..=2.5).text("Relief wet coat"));
            ui.separator();
            ui.label("Autonomic eyes");
            ui.add(Slider::new(&mut self.scene_luminance, 0.0..=1.0).text("Scene luminance"));
            ui.add(Slider::new(&mut self.focus_distance, -1.0..=1.0).text("Near ← focus → Far"));
            ui.add(Slider::new(&mut self.emotional_arousal, 0.0..=1.0).text("Emotional arousal"));
            ui.add(Slider::new(&mut self.interest, 0.0..=1.0).text("Interest"));
            ui.add(
                Slider::new(&mut p.pupil_light_response, 0.0..=2.0).text("Pupil light response"),
            );
            ui.add(
                Slider::new(&mut p.pupil_emotion_response, 0.0..=2.0)
                    .text("Pupil emotion response"),
            );
            ui.add(
                Slider::new(&mut p.pupil_focus_response, 0.0..=2.0).text("Pupil focus response"),
            );
            ui.add(Slider::new(&mut p.microsaccade_amount, 0.0..=2.0).text("Microsaccade amount"));
            ui.add(Slider::new(&mut p.microsaccade_rate, 0.0..=2.0).text("Microsaccade rate"));
            ui.checkbox(&mut self.focus_lock, "Focus lock");
            ui.add(Slider::new(&mut p.maximum_roll_radians, 0.0..=0.45).text("Maximum roll"));
            ui.add(
                Slider::new(&mut p.translation_smoothing, 0.5..=40.0).text("Translation smoothing"),
            );
            ui.add(Slider::new(&mut p.rotation_smoothing, 0.5..=40.0).text("Rotation smoothing"));
            ui.add(Slider::new(&mut p.scale_smoothing, 0.5..=40.0).text("Scale smoothing"));
            if ui.button("Reset face").clicked() {
                self.profile.face = Default::default();
            }
        });
    }

    fn compositor_controls(&mut self, ui: &mut egui::Ui) {
        CollapsingHeader::new("Ground shadow + exposure")
            .default_open(true)
            .show(ui, |ui| {
                let p = &mut self.profile.compositor;
                ui.label("macOS-style full-silhouette drop shadow");
                ui.add(
                    Slider::new(&mut p.shadow_horizontal_offset, -96.0..=96.0)
                        .text("Shadow X offset (px)"),
                );
                ui.add(
                    Slider::new(&mut p.shadow_vertical_offset, -96.0..=96.0)
                        .text("Shadow Y offset (px)"),
                );
                ui.add(
                    Slider::new(&mut p.shadow_feather, 2.0..=128.0)
                        .text("Shadow feather (px)"),
                );
                ui.add(
                    Slider::new(&mut p.shadow_opacity, 0.0..=0.50)
                        .text("Shadow opacity"),
                );
                ui.horizontal(|ui| {
                    ui.label("Shadow color");
                    ui.color_edit_button_rgb(&mut p.shadow_color);
                });
                ui.small("The body alpha is only offset and feathered; its silhouette is never flattened.");
                ui.horizontal(|ui| {
                    if ui.button("macOS soft preset").clicked() {
                        p.shadow_horizontal_offset = 0.0;
                        p.shadow_vertical_offset = 10.0;
                        p.shadow_feather = 42.0;
                        p.shadow_opacity = 0.12;
                        p.shadow_color = [0.008, 0.012, 0.020];
                    }
                    if ui.button("Reset previous look").clicked() {
                        *p = Default::default();
                    }
                });
                ui.separator();
                ui.add(Slider::new(&mut p.render_scale, 1..=2).text("Render scale"));
                ui.add(Slider::new(&mut p.exposure, 0.25..=3.0).text("Exposure"));
            });
    }

    fn diagnostics(
        &self,
        ui: &mut egui::Ui,
        d: LiquidDiagnostics,
        pupil_size: f32,
        pupil_asymmetry: f32,
        fps: f32,
        p95_frame_ms: f32,
    ) {
        CollapsingHeader::new("Live diagnostics")
            .default_open(true)
            .show(ui, |ui| {
                egui::Grid::new("diagnostics_grid").show(ui, |ui| {
                    metric(ui, "Lab FPS", format!("{fps:.1}"));
                    metric(ui, "p95 frame", format!("{p95_frame_ms:.2} ms"));
                    metric(ui, "Particles", d.particle_count.to_string());
                    metric(ui, "Components", d.component_count.to_string());
                    metric(ui, "Detached mass", format!("{:.2}", d.detached_mass));
                    metric(ui, "Density error", format!("{:.4}", d.density_error));
                    metric(ui, "Kinetic energy", format!("{:.4}", d.kinetic_energy));
                    metric(
                        ui,
                        "Max compression",
                        format!("{:.1}%", d.maximum_compression * 100.0),
                    );
                    metric(ui, "Maximum speed", format!("{:.3}", d.maximum_speed));
                    metric(ui, "Failsafe hits", d.failsafe_hits.to_string());
                    metric(ui, "Solver recoveries", d.recovery_count.to_string());
                    metric(
                        ui,
                        "Rigid spin",
                        format!("{:.4} rad/s", d.rigid_angular_velocity),
                    );
                    metric(ui, "Face support", format!("{:.3}", d.face_confidence));
                    metric(ui, "Autonomic pupil", format!("{pupil_size:.3}"));
                    metric(ui, "Pupil asymmetry", format!("{pupil_asymmetry:.4}"));
                    metric(ui, "Focus lock", self.focus_lock.to_string());
                    metric(ui, "COM follow", format!("{:.3}", d.com_follow_ratio));
                    metric(ui, "Stretch ratio", format!("{:.3}", d.stretch_ratio));
                    metric(ui, "Material stress", format!("{:.3}", d.stress_magnitude));
                    metric(ui, "Cursor distance", format!("{:.3}", d.cursor_distance));
                    metric(ui, "Finite", d.finite.to_string());
                });
            });
    }

    fn persistence_controls(&mut self, ui: &mut egui::Ui) {
        CollapsingHeader::new("Profile JSON + Apply to Pet")
            .default_open(true)
            .show(ui, |ui| {
                ui.text_edit_singleline(&mut self.profile.name);
                ui.horizontal(|ui| {
                    ui.label("Seed");
                    ui.add(DragValue::new(&mut self.profile.seed).speed(1.0));
                });
                ui.label("Save/load path");
                ui.text_edit_singleline(&mut self.profile_path);
                ui.horizontal_wrapped(|ui| {
                    if ui.button("Save JSON").clicked() {
                        self.save_path();
                    }
                    if ui.button("Load JSON").clicked() {
                        self.load_path();
                    }
                    if ui.button("Refresh JSON text").clicked() {
                        self.refresh_json();
                    }
                    if ui.button("Import JSON text").clicked() {
                        self.import_json_text();
                    }
                });
                ui.add(
                    egui::TextEdit::multiline(&mut self.json_buffer)
                        .desired_rows(8)
                        .code_editor(),
                );
                ui.horizontal_wrapped(|ui| {
                    if ui.button("Apply this liquid to running Pet").clicked() {
                        self.apply_to_pet();
                    }
                    if ui.button("Apply + launch desktop Pet").clicked() {
                        self.apply_and_launch_pet();
                    }
                    if ui.button("Reset all").clicked() {
                        self.profile = LiquidTuningProfile::for_seed(self.profile.seed);
                        self.profile.render_mode = BodyRenderMode::ParticlePbf;
                        self.reset_requested = true;
                        self.refresh_json();
                    }
                });
                ui.separator();
                ui.label(&self.status);
            });
    }

    fn refresh_json(&mut self) {
        self.profile.render_mode = BodyRenderMode::ParticlePbf;
        match self.profile.clone().sanitized() {
            Ok(profile) => match serde_json::to_string_pretty(&profile) {
                Ok(json) => {
                    self.json_buffer = json;
                    self.status = "JSON refreshed from current controls.".to_owned();
                }
                Err(error) => self.status = format!("Could not encode JSON: {error}"),
            },
            Err(error) => self.status = format!("Profile is invalid: {error}"),
        }
    }

    fn import_json_text(&mut self) {
        match decode_profile(self.json_buffer.as_bytes()) {
            Ok(profile) => {
                self.profile = profile;
                self.profile.render_mode = BodyRenderMode::ParticlePbf;
                self.reset_requested = true;
                self.status = "Imported and validated JSON text.".to_owned();
            }
            Err(error) => self.status = format!("JSON import rejected: {error}"),
        }
    }

    fn save_path(&mut self) {
        let path = PathBuf::from(self.profile_path.trim());
        match save_profile_file(&path, &self.profile) {
            Ok(()) => self.status = format!("Saved {}", path.display()),
            Err(error) => self.status = format!("Save failed: {error}"),
        }
    }

    fn load_path(&mut self) {
        let path = PathBuf::from(self.profile_path.trim());
        match load_profile_file(&path) {
            Ok(profile) => {
                self.profile = profile;
                self.profile.render_mode = BodyRenderMode::ParticlePbf;
                self.reset_requested = true;
                self.refresh_json();
                self.status = format!("Loaded {}", path.display());
            }
            Err(error) => self.status = format!("Load failed: {error}"),
        }
    }

    fn apply_to_pet(&mut self) {
        match self.save_active_profile() {
            Ok(revision) => {
                self.status = format!("Saved revision {revision}. Waiting for Pet acknowledgement…")
            }
            Err(error) => self.status = format!("Apply failed: {error}"),
        }
    }

    fn apply_and_launch_pet(&mut self) {
        let revision = match self.save_active_profile() {
            Ok(revision) => revision,
            Err(error) => {
                self.status = format!("Apply failed; desktop Pet was not launched: {error}");
                return;
            }
        };
        match launch_desktop_pet() {
            Ok(path) => {
                self.status = format!(
                    "Saved revision {revision} and launched {}. Waiting for acknowledgement…",
                    path.display()
                )
            }
            Err(error) => {
                self.status =
                    format!("Saved revision {revision}, but desktop Pet launch failed: {error}")
            }
        }
    }

    fn save_active_profile(&mut self) -> Result<u64, String> {
        let store = self
            .store
            .as_ref()
            .ok_or_else(|| "Pet data directory is unavailable".to_owned())?;
        let active_revision = store
            .load_liquid_tuning::<LiquidTuningProfile>()
            .map_err(|error| error.to_string())?
            .map_or(0, |profile| profile.profile_revision);
        let mut candidate = self.profile.clone();
        candidate.render_mode = BodyRenderMode::ParticlePbf;
        candidate.profile_revision = candidate
            .profile_revision
            .max(active_revision)
            .saturating_add(1);
        let profile = candidate.sanitized().map_err(|error| error.to_string())?;
        store
            .save_liquid_tuning(&profile)
            .map_err(|error| error.to_string())?;
        let saved = store
            .load_liquid_tuning::<LiquidTuningProfile>()
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "saved profile could not be reread".to_owned())?;
        if saved.profile_revision != profile.profile_revision
            || saved.schema_version != profile.schema_version
            || saved.material.variant != profile.material.variant
        {
            return Err("saved profile verification mismatch".to_owned());
        }
        self.profile = profile;
        self.pending_revision = Some(self.profile.profile_revision);
        self.next_ack_poll = Instant::now();
        self.json_buffer = serde_json::to_string_pretty(&self.profile).unwrap_or_default();
        Ok(self.profile.profile_revision)
    }

    fn poll_pet_acknowledgement(&mut self) {
        let Some(expected_revision) = self.pending_revision else {
            return;
        };
        let now = Instant::now();
        if now < self.next_ack_poll {
            return;
        }
        self.next_ack_poll = now + Duration::from_millis(250);
        let Some(store) = &self.store else {
            return;
        };
        match store.load_liquid_tuning_status::<LiquidTuningAcknowledgement>() {
            Ok(Some(ack))
                if ack.profile_revision == expected_revision
                    && ack.schema_version == self.profile.schema_version
                    && ack.material_variant == self.profile.material.variant =>
            {
                self.pending_revision = None;
                self.status = format!(
                    "Applied by Pet · revision {} · build {}",
                    ack.profile_revision, ack.build_version
                );
            }
            Ok(Some(_)) | Ok(None) => {
                self.status =
                    format!("Saved revision {expected_revision}. Waiting for Pet acknowledgement…");
            }
            Err(error) => {
                self.status = format!(
                    "Saved revision {expected_revision}; acknowledgement read failed: {error}"
                );
            }
        }
    }
}

fn moonlit_glass_preset(seed: u64) -> LiquidTuningProfile {
    let mut profile = LiquidTuningProfile::for_seed(seed);
    profile.name = "Moonlit Glass".to_owned();
    profile.render_mode = BodyRenderMode::ParticlePbf;
    profile.material.variant = MaterialVariant::CinematicJelly;
    profile.material.override_genome_colors = true;
    profile.material.primary_hsv = [0.55, 0.58, 0.78];
    profile.material.secondary_hsv = [0.68, 0.46, 0.92];
    profile.material.glow_hsv = [0.12, 0.64, 1.18];
    profile.material.absorption = 0.72;
    profile.material.scattering = 0.48;
    profile.material.thickness = 0.92;
    profile.material.translucency = 0.84;
    profile.material.refraction = 1.35;
    profile.material.blur = 1.45;
    profile.material.rim_strength = 0.72;
    profile.material.emission = 0.11;
    profile.material.internal_flow = 0.12;
    profile.material.internal_orb_count = 5;
    profile.material.internal_orb_intensity = 0.62;
    profile.material.internal_orb_size = 0.038;
    profile.material.internal_orb_speed = 0.20;
    profile.material.studio_intensity = 1.30;
    profile.material.studio_base_roughness = 0.22;
    profile.material.studio_coat_roughness = 0.06;
    profile.material.edge_light_width = 14.0;
    profile.material.caustic_strength = 0.55;
    profile.material.caustic_scale = 4.8;
    profile.material.caustic_speed = 0.18;
    profile.material.caustic_dispersion = 0.28;
    profile.material.soul_glow_count = 3;
    profile.material.soul_glow_strength = 0.13;
    profile.material.bloom_strength = 0.34;
    profile.face.scale = [0.96, 0.98];
    profile.face.eye_size_scale = 1.24;
    profile.face.eye_spacing_scale = 1.13;
    profile.face.pupil_scale = 1.08;
    profile.face.eye_highlight_scale = 1.62;
    profile.face.eye_socket_strength = 0.34;
    profile.face.override_iris_color = true;
    profile.face.iris_hsv = [0.53, 0.72, 0.74];
    profile.compositor.shadow_vertical_offset = 8.0;
    profile.compositor.shadow_feather = 64.0;
    profile.compositor.shadow_opacity = 0.22;
    profile.pbf.density_iterations = 6;
    profile.pbf.density_compliance = 1.2e-5;
    profile.pbf.numerical_xsph = 0.014;
    profile.pbf.viscosity = 0.035;
    profile.pbf.surface_tension = 0.78;
    profile.pbf.flight_stretch = 0.55;
    profile.pbf.return_strength = 0.30;
    profile.pbf.character_field_radius_scale = 1.10;
    profile.pbf.pointer_support_scale = 1.90;
    profile.pbf.grab_stiffness = 210.0;
    profile.pbf.pointer_response_hz = 16.0;
    profile.pbf.anisotropy_max = 1.75;
    profile.pbf.idle_fragment_size = 0.0;
    profile.pbf.idle_bud_pull_strength = 0.0;
    profile.pbf.pinch_bounce = 0.0;
    profile
        .sanitized()
        .expect("built-in Moonlit Glass preset must remain schema-valid")
}

fn validate_preset_display_name(name: &str) -> Result<String, String> {
    let trimmed = name.trim();
    let length = trimmed.chars().count();
    if length == 0 {
        return Err("preset name cannot be empty".to_owned());
    }
    if length > 64 {
        return Err("preset name must be 64 characters or fewer".to_owned());
    }
    if trimmed.chars().any(char::is_control) {
        return Err("preset name cannot contain control characters".to_owned());
    }
    Ok(trimmed.to_owned())
}

fn preset_file_stem(display_name: &str) -> Result<String, String> {
    let display_name = validate_preset_display_name(display_name)?;
    let mut stem = String::new();
    let mut separator_pending = false;
    for character in display_name.chars() {
        if character.is_ascii_alphanumeric() {
            if separator_pending && !stem.is_empty() && !stem.ends_with('-') {
                stem.push('-');
            }
            separator_pending = false;
            stem.push(character.to_ascii_lowercase());
        } else if character == '-' || character == '_' || character.is_whitespace() {
            separator_pending = !stem.is_empty();
        } else {
            if separator_pending && !stem.is_empty() && !stem.ends_with('-') {
                stem.push('-');
            }
            separator_pending = false;
            if !stem.is_empty() && !stem.ends_with('-') {
                stem.push('-');
            }
            stem.push('u');
            stem.push_str(&format!("{:x}", character as u32));
            stem.push('-');
        }
    }
    while stem.ends_with('-') {
        stem.pop();
    }
    if stem.is_empty() {
        return Err("preset name does not contain a usable filename".to_owned());
    }
    if stem.len() > 96 {
        let hash = stable_name_hash(display_name.as_bytes());
        stem.truncate(72);
        while stem.ends_with('-') {
            stem.pop();
        }
        stem.push_str(&format!("-{hash:016x}"));
    }
    let uppercase = stem.to_ascii_uppercase();
    let reserved = matches!(uppercase.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || uppercase.len() == 4
            && (uppercase.starts_with("COM") || uppercase.starts_with("LPT"))
            && uppercase.as_bytes()[3].is_ascii_digit()
            && uppercase.as_bytes()[3] != b'0';
    if reserved {
        stem.insert_str(0, "preset-");
    }
    Ok(stem)
}

fn stable_name_hash(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01B3);
    }
    hash
}

fn valid_preset_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
}

fn user_preset_path_from_id(directory: &Path, id: &str) -> Result<PathBuf, String> {
    if !valid_preset_id(id) {
        return Err("unsafe user preset identifier".to_owned());
    }
    let path = directory.join(format!("{id}.json"));
    if path.parent() != Some(directory) {
        return Err("user preset path escaped its directory".to_owned());
    }
    Ok(path)
}

fn user_preset_path(directory: &Path, display_name: &str) -> Result<(String, PathBuf), String> {
    let id = preset_file_stem(display_name)?;
    let path = user_preset_path_from_id(directory, &id)?;
    Ok((id, path))
}

fn save_user_preset(
    directory: &Path,
    display_name: &str,
    profile: &LiquidTuningProfile,
) -> Result<UserPreset, String> {
    let display_name = validate_preset_display_name(display_name)?;
    let (id, path) = user_preset_path(directory, &display_name)?;
    let mut saved = profile.clone();
    saved.name = display_name.clone();
    saved.profile_revision = 0;
    saved.render_mode = BodyRenderMode::ParticlePbf;
    let saved = saved.sanitized().map_err(|error| error.to_string())?;
    save_profile_file(&path, &saved)?;
    Ok(UserPreset {
        id,
        display_name,
        path,
        profile: saved,
    })
}

fn discover_user_presets(directory: &Path) -> Result<(Vec<UserPreset>, usize), String> {
    if !directory.exists() {
        return Ok((Vec::new(), 0));
    }
    let entries = fs::read_dir(directory).map_err(|error| error.to_string())?;
    let mut presets = Vec::new();
    let mut skipped = 0;
    for entry in entries {
        let Ok(entry) = entry else {
            skipped += 1;
            continue;
        };
        let Ok(file_type) = entry.file_type() else {
            skipped += 1;
            continue;
        };
        let path = entry.path();
        if !file_type.is_file()
            || !path
                .extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| extension.eq_ignore_ascii_case("json"))
        {
            continue;
        }
        let Some(id) = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .filter(|id| valid_preset_id(id))
            .map(str::to_owned)
        else {
            skipped += 1;
            continue;
        };
        match load_profile_file(&path) {
            Ok(profile) => presets.push(UserPreset {
                id,
                display_name: profile.name.clone(),
                path,
                profile,
            }),
            Err(_) => skipped += 1,
        }
    }
    presets.sort_by_key(|preset| preset.display_name.to_lowercase());
    Ok((presets, skipped))
}

fn rename_user_preset(
    directory: &Path,
    current_id: &str,
    new_display_name: &str,
) -> Result<UserPreset, String> {
    let current_path = user_preset_path_from_id(directory, current_id)?;
    let profile = load_profile_file(&current_path)?;
    let (new_id, new_path) = user_preset_path(directory, new_display_name)?;
    if new_path != current_path && new_path.exists() {
        return Err("a user preset with that name already exists".to_owned());
    }
    let renamed = save_user_preset(directory, new_display_name, &profile)?;
    if new_path != current_path {
        fs::remove_file(&current_path).map_err(|error| error.to_string())?;
    }
    debug_assert_eq!(renamed.id, new_id);
    Ok(renamed)
}

fn delete_user_preset(directory: &Path, id: &str) -> Result<(), String> {
    let path = user_preset_path_from_id(directory, id)?;
    fs::remove_file(path).map_err(|error| error.to_string())
}

fn canonical_pet_executable_name() -> &'static str {
    if cfg!(windows) { "Pet 2.exe" } else { "Pet 2" }
}

fn development_pet_executable_name() -> &'static str {
    if cfg!(windows) { "pet2.exe" } else { "pet2" }
}

fn pet_executable_candidates(current_exe: &Path, workspace_root: &Path) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    let mut push_unique = |candidate: PathBuf| {
        if !candidates.contains(&candidate) {
            candidates.push(candidate);
        }
    };
    if let Some(parent) = current_exe.parent() {
        push_unique(parent.join(canonical_pet_executable_name()));
    }
    push_unique(
        workspace_root
            .join("builds")
            .join("current")
            .join(canonical_pet_executable_name()),
    );
    push_unique(
        workspace_root
            .join("target")
            .join("release")
            .join(development_pet_executable_name()),
    );
    if let Some(parent) = current_exe.parent() {
        push_unique(parent.join(development_pet_executable_name()));
    }
    push_unique(
        workspace_root
            .join("target")
            .join("debug")
            .join(development_pet_executable_name()),
    );
    candidates
}

fn resolve_pet_executable(current_exe: &Path, workspace_root: &Path) -> Result<PathBuf, String> {
    let candidates = pet_executable_candidates(current_exe, workspace_root);
    candidates
        .iter()
        .find(|candidate| candidate.is_file())
        .cloned()
        .ok_or_else(|| {
            format!(
                "Pet executable not found; checked {}",
                candidates
                    .iter()
                    .map(|path| path.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })
}

fn launch_desktop_pet() -> Result<PathBuf, String> {
    let current_exe = std::env::current_exe().map_err(|error| error.to_string())?;
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let executable = resolve_pet_executable(&current_exe, &workspace_root)?;
    let mut command = Command::new(&executable);
    if let Some(parent) = executable.parent() {
        command.current_dir(parent);
    }
    command.spawn().map_err(|error| error.to_string())?;
    Ok(executable)
}

fn save_profile_file(path: &Path, profile: &LiquidTuningProfile) -> Result<(), String> {
    let profile = profile
        .clone()
        .sanitized()
        .map_err(|error| error.to_string())?;
    let json = serde_json::to_vec_pretty(&profile).map_err(|error| error.to_string())?;
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let mut file = File::create(path).map_err(|error| error.to_string())?;
    file.write_all(&json).map_err(|error| error.to_string())?;
    file.write_all(b"\n").map_err(|error| error.to_string())?;
    file.sync_all().map_err(|error| error.to_string())?;
    Ok(())
}

fn load_profile_file(path: &Path) -> Result<LiquidTuningProfile, String> {
    let reader = BufReader::new(File::open(path).map_err(|error| error.to_string())?);
    let profile: LiquidTuningProfile =
        serde_json::from_reader(reader).map_err(|error| error.to_string())?;
    profile.sanitized().map_err(|error| error.to_string())
}

fn decode_profile(bytes: &[u8]) -> Result<LiquidTuningProfile, String> {
    let profile: LiquidTuningProfile =
        serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
    profile.sanitized().map_err(|error| error.to_string())
}

fn hsv_controls(ui: &mut egui::Ui, label: &str, hsv: &mut [f32; 3]) {
    ui.label(label);
    ui.horizontal(|ui| {
        ui.add(DragValue::new(&mut hsv[0]).range(0.0..=1.0).speed(0.005));
        ui.add(DragValue::new(&mut hsv[1]).range(0.0..=1.0).speed(0.005));
        ui.add(DragValue::new(&mut hsv[2]).range(0.0..=2.0).speed(0.005));
    });
}

fn metric(ui: &mut egui::Ui, label: &str, value: String) {
    ui.label(label);
    ui.monospace(value);
    ui.end_row();
}

fn all_scenarios() -> [PreviewScenario; 8] {
    [
        PreviewScenario::Calm,
        PreviewScenario::FluidFlight,
        PreviewScenario::ConstantVelocity,
        PreviewScenario::HardStop,
        PreviewScenario::DirectionReversal,
        PreviewScenario::Impulse,
        PreviewScenario::DetachAndRemerge,
        PreviewScenario::WindowPressure,
    ]
}

fn scenario_name(scenario: PreviewScenario) -> &'static str {
    match scenario {
        PreviewScenario::Calm => "Calm",
        PreviewScenario::FluidFlight => "Fluid flight loop",
        PreviewScenario::ConstantVelocity => "Constant velocity",
        PreviewScenario::HardStop => "Hard stop",
        PreviewScenario::DirectionReversal => "Direction reversal",
        PreviewScenario::Impulse => "Impulse",
        PreviewScenario::DetachAndRemerge => "Detach / re-merge",
        PreviewScenario::WindowPressure => "Window pressure",
    }
}

fn all_preview_emotions() -> [Option<EmotionKind>; 12] {
    [
        None,
        Some(EmotionKind::Curiosity),
        Some(EmotionKind::Delight),
        Some(EmotionKind::Affection),
        Some(EmotionKind::Contentment),
        Some(EmotionKind::Surprise),
        Some(EmotionKind::Alarm),
        Some(EmotionKind::Fear),
        Some(EmotionKind::Frustration),
        Some(EmotionKind::Boredom),
        Some(EmotionKind::Shyness),
        Some(EmotionKind::Pride),
    ]
}

fn emotion_name(emotion: Option<EmotionKind>) -> &'static str {
    match emotion {
        None => "Neutral",
        Some(EmotionKind::Curiosity) => "Curiosity",
        Some(EmotionKind::Delight) => "Delight",
        Some(EmotionKind::Affection) => "Affection",
        Some(EmotionKind::Contentment) => "Contentment",
        Some(EmotionKind::Surprise) => "Surprise",
        Some(EmotionKind::Alarm) => "Alarm",
        Some(EmotionKind::Fear) => "Fear",
        Some(EmotionKind::Frustration) => "Frustration",
        Some(EmotionKind::Boredom) => "Boredom",
        Some(EmotionKind::Shyness) => "Shyness",
        Some(EmotionKind::Pride) => "Pride",
    }
}

fn background_name(index: usize) -> &'static str {
    ["Black", "White", "Gray", "Busy checker", "Warm", "Cold"][index % BACKGROUNDS.len()]
}

fn all_debug_views() -> [DebugView; 10] {
    [
        DebugView::Material,
        DebugView::Field,
        DebugView::Alpha,
        DebugView::FaceCoverage,
        DebugView::FlowOrComponent,
        DebugView::Thickness,
        DebugView::EdgeDistance,
        DebugView::MacroNormal,
        DebugView::StudioReflection,
        DebugView::Caustics,
    ]
}

fn debug_view_name(view: DebugView) -> &'static str {
    match view {
        DebugView::Material => "Material",
        DebugView::Field => "Field / density",
        DebugView::Alpha => "Alpha coverage",
        DebugView::FaceCoverage => "Face coverage",
        DebugView::FlowOrComponent => "Emission / pigment field",
        DebugView::Thickness => "Integrated thickness",
        DebugView::EdgeDistance => "Silhouette edge distance",
        DebugView::MacroNormal => "Macro surface normal",
        DebugView::StudioReflection => "Studio lobes (R base / G coat / B fill)",
        DebugView::Caustics => "Material-space caustics",
    }
}

fn preview_seeded_unit(mut value: u64) -> f32 {
    value = value.wrapping_add(0x9E37_79B9_7F4A_7C15);
    value = (value ^ (value >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    value ^= value >> 31;
    ((value >> 40) as u32) as f32 / 0xFF_FFFF as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn built_in_presets_are_visually_and_dynamically_distinct_but_solver_safe() {
        let authored = LiquidTuningProfile::for_seed(42).sanitized().unwrap();
        let moonlit = moonlit_glass_preset(42);

        assert_ne!(moonlit.material.primary_hsv, authored.material.primary_hsv);
        assert_ne!(
            moonlit.material.studio_base_roughness,
            authored.material.studio_base_roughness
        );
        assert_ne!(
            moonlit.compositor.shadow_feather,
            authored.compositor.shadow_feather
        );
        assert_ne!(moonlit.face.eye_size_scale, authored.face.eye_size_scale);
        assert!(moonlit.face.override_iris_color);
        assert_ne!(moonlit.face.iris_hsv, authored.face.iris_hsv);
        assert_ne!(moonlit.pbf.viscosity, authored.pbf.viscosity);
        assert_ne!(moonlit.pbf.surface_tension, authored.pbf.surface_tension);
        assert_ne!(moonlit.pbf.flight_stretch, authored.pbf.flight_stretch);
        assert_ne!(
            moonlit.pbf.character_field_radius_scale,
            authored.pbf.character_field_radius_scale
        );
        assert_eq!(moonlit.pbf.fixed_hz, 120.0);
        assert_eq!(moonlit.pbf.substeps, 1);
        assert_eq!(moonlit.pbf.impact_substeps, 1);
        assert_eq!(moonlit.pbf.density_iterations, 6);
        assert!((0.005..=0.030).contains(&moonlit.pbf.numerical_xsph));
        assert_eq!(moonlit.pbf.idle_fragment_size, 0.0);
        assert_eq!(moonlit.pbf.idle_bud_pull_strength, 0.0);
        assert_eq!(moonlit.pbf.pinch_bounce, 0.0);
        assert_eq!(moonlit.clone().sanitized().unwrap(), moonlit);
    }

    #[test]
    fn preset_filename_handling_never_escapes_or_targets_active_tuning() {
        let temporary = tempfile::tempdir().unwrap();
        let preset_directory = temporary.path().join(USER_PRESET_DIRECTORY);
        let active_profile = temporary.path().join("liquid-tuning.json");
        let (id, path) =
            user_preset_path(&preset_directory, "../..\\liquid-tuning.json / Мой пресет").unwrap();

        assert!(valid_preset_id(&id));
        assert_eq!(path.parent(), Some(preset_directory.as_path()));
        assert_ne!(path, active_profile);
        let filename = path.file_name().unwrap().to_string_lossy();
        assert!(!filename.contains('/') && !filename.contains('\\'));
        assert!(preset_file_stem("CON").unwrap().starts_with("preset-"));
        assert!(preset_file_stem("\0bad").is_err());
        assert!(preset_file_stem("   ").is_err());
        assert!(user_preset_path_from_id(&preset_directory, "../escape").is_err());
    }

    #[test]
    fn user_preset_round_trip_rename_delete_preserves_active_profile() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path();
        let preset_directory = root.join(USER_PRESET_DIRECTORY);
        let active_profile = root.join("liquid-tuning.json");
        fs::write(&active_profile, b"active-profile-must-survive").unwrap();
        let mut profile = moonlit_glass_preset(77);
        profile.material.caustic_strength = 0.91;

        let saved = save_user_preset(&preset_directory, "My Moon", &profile).unwrap();
        assert!(saved.path.is_file());
        let (loaded, skipped) = discover_user_presets(&preset_directory).unwrap();
        assert_eq!(skipped, 0);
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].display_name, "My Moon");
        assert_eq!(loaded[0].profile.material.caustic_strength, 0.91);
        assert_eq!(loaded[0].profile.profile_revision, 0);

        let renamed = rename_user_preset(&preset_directory, &saved.id, "Месяц").unwrap();
        assert!(!saved.path.exists());
        assert!(renamed.path.is_file());
        assert_eq!(load_profile_file(&renamed.path).unwrap().name, "Месяц");

        delete_user_preset(&preset_directory, &renamed.id).unwrap();
        assert!(!renamed.path.exists());
        assert_eq!(
            fs::read(&active_profile).unwrap(),
            b"active-profile-must-survive"
        );
    }

    #[test]
    fn pet_executable_resolution_prefers_canonical_then_release_then_debug() {
        let temporary = tempfile::tempdir().unwrap();
        let workspace = temporary.path();
        let current_directory = workspace.join("builds").join("current");
        fs::create_dir_all(&current_directory).unwrap();
        let current_exe = current_directory.join(if cfg!(windows) {
            "Pet2 Dev Console.exe"
        } else {
            "Pet2 Dev Console"
        });
        let canonical = current_directory.join(canonical_pet_executable_name());
        File::create(&canonical).unwrap();
        assert_eq!(
            resolve_pet_executable(&current_exe, workspace).unwrap(),
            canonical
        );

        fs::remove_file(&canonical).unwrap();
        let release = workspace
            .join("target")
            .join("release")
            .join(development_pet_executable_name());
        let debug = workspace
            .join("target")
            .join("debug")
            .join(development_pet_executable_name());
        fs::create_dir_all(release.parent().unwrap()).unwrap();
        fs::create_dir_all(debug.parent().unwrap()).unwrap();
        File::create(&release).unwrap();
        File::create(&debug).unwrap();
        assert_eq!(
            resolve_pet_executable(&current_exe, workspace).unwrap(),
            release
        );

        fs::remove_file(&release).unwrap();
        assert_eq!(
            resolve_pet_executable(&current_exe, workspace).unwrap(),
            debug
        );
    }

    #[test]
    fn preview_projection_is_centered_in_the_actual_canvas_not_the_hidden_window() {
        let genome = Genome::from_seed(7);
        let mut body = ProceduralBody::generate(&genome).unwrap();
        let mut profile = LiquidTuningProfile::for_seed(genome.identity_seed);
        profile.render_mode = BodyRenderMode::ParticlePbf;
        body.apply_tuning_profile(profile).unwrap();
        let minimum = Vec2::new(0.0, 0.035);
        let maximum = Vec2::new(0.67, 0.965);
        sync_preview_viewport(&mut body, PhysicalSize::new(1_180, 820), minimum, maximum);
        let parameters = body.render_parameters(&genome, 0.0);
        assert!(
            parameters.presentation_offset.x < -0.45,
            "preview was not shifted from full-window center: {:?}",
            parameters.presentation_offset
        );
        assert!(
            parameters.presentation_offset.y.abs() < 1.0e-4,
            "symmetric vertical canvas introduced a vertical offset: {:?}",
            parameters.presentation_offset
        );
    }

    #[test]
    fn lab_has_every_regression_scenario() {
        for scenario in all_scenarios() {
            assert!(!scenario_name(scenario).is_empty());
        }
    }

    #[test]
    fn lab_exposes_every_vita_emotion_for_face_review() {
        let emotions = all_preview_emotions();
        assert_eq!(emotions.len(), 12);
        for emotion in emotions {
            assert!(!emotion_name(emotion).is_empty());
        }
    }

    #[test]
    fn scenarios_are_finite_for_a_long_deterministic_run() {
        for scenario in all_scenarios() {
            for frame in 0..10_000 {
                let (velocity, acceleration, pressure) =
                    scenario_motion(scenario, frame as f32 / 120.0);
                assert!(velocity.is_finite());
                assert!(acceleration.is_finite());
                assert!(pressure.is_finite());
            }
        }
    }

    #[test]
    fn fluid_flight_contains_real_banks_reversals_and_a_settle_window() {
        let mut maximum_acceleration = 0.0_f32;
        let mut minimum_x_velocity = f32::INFINITY;
        let mut maximum_x_velocity = f32::NEG_INFINITY;
        let mut maximum_vertical_velocity = 0.0_f32;
        let mut previous_velocity = Vec2::ZERO;
        let mut maximum_tick_jump = 0.0_f32;
        for frame in 0..960 {
            let time = frame as f32 / 120.0;
            let (velocity, acceleration) = fluid_flight_motion(time);
            maximum_acceleration = maximum_acceleration.max(acceleration.length());
            minimum_x_velocity = minimum_x_velocity.min(velocity.x);
            maximum_x_velocity = maximum_x_velocity.max(velocity.x);
            maximum_vertical_velocity = maximum_vertical_velocity.max(velocity.y.abs());
            maximum_tick_jump = maximum_tick_jump.max(velocity.distance(previous_velocity));
            previous_velocity = velocity;
        }

        assert!(maximum_acceleration > 2.0);
        assert!(minimum_x_velocity < -0.70 && maximum_x_velocity > 0.75);
        assert!(maximum_vertical_velocity > 0.45);
        assert!(
            maximum_tick_jump < 0.05,
            "velocity discontinuity {maximum_tick_jump}"
        );
        assert_eq!(fluid_flight_motion(7.6), (Vec2::ZERO, Vec2::ZERO));
    }

    #[test]
    fn fluid_flight_driver_deforms_the_particle_body_instead_of_rigidly_translating_it() {
        let genome = Genome::from_seed(42);
        let mut body = ProceduralBody::generate(&genome).unwrap();
        let mut profile = LiquidTuningProfile::for_seed(genome.identity_seed);
        profile.render_mode = BodyRenderMode::ParticlePbf;
        body.apply_tuning_profile(profile).unwrap();
        body.set_desktop_motion_space(Vec2::new(1_180.0, 820.0), 820.0);
        body.embodiment.liquid.set_navigation_anchor_strength(1.0);
        body.simulation.feedback.world_position = Vec2::splat(0.5);
        let dt = 1.0 / 120.0;
        let mut minimum_width = f32::INFINITY;
        let mut maximum_width = f32::NEG_INFINITY;
        let mut maximum_relative_speed = 0.0_f32;

        for frame in 0..960 {
            let (velocity, acceleration) = fluid_flight_motion(frame as f32 * dt);
            body.simulation.feedback.velocity = velocity;
            body.simulation.feedback.acceleration = acceleration;
            body.simulation.feedback.world_position = (body.simulation.feedback.world_position
                + velocity * dt * 0.035)
                .clamp(Vec2::splat(0.1), Vec2::splat(0.9));
            let position = body.simulation.feedback.world_position;
            let intent = BodyIntent {
                locomotion: LocomotionMode::Hover,
                target_position: position,
                target_surface: None,
                desired_speed: velocity.length().clamp(0.0, 1.0),
                facing_direction: 1.0,
                gaze_target: Some(position),
                pose: PoseIntent::Neutral,
                expression: ExpressionState::default(),
                interaction_target: Some(InteractionTarget::User),
            };
            body.embodied_update(
                &intent,
                &SensorFrame::default(),
                AffectState {
                    arousal: velocity.length().clamp(0.0, 1.0),
                    ..AffectState::default()
                },
                VisualMindInput {
                    arousal: velocity.length().clamp(0.0, 1.0),
                    curiosity: 0.45,
                    attachment: 0.55,
                    confidence: 0.60,
                    local_luminance: 0.5,
                    ..VisualMindInput::default()
                },
                VoiceVisualState::default(),
                dt,
            );
            let state = body.embodiment.liquid.render_state();
            let main = state
                .particles
                .iter()
                .take(state.particle_count)
                .filter(|particle| particle.main_component);
            let (minimum_x, maximum_x) = main.clone().map(|particle| particle.position.x).fold(
                (f32::INFINITY, f32::NEG_INFINITY),
                |(minimum, maximum), x| (minimum.min(x), maximum.max(x)),
            );
            minimum_width = minimum_width.min(maximum_x - minimum_x);
            maximum_width = maximum_width.max(maximum_x - minimum_x);
            let mean_velocity = main.clone().map(|particle| particle.velocity).sum::<Vec2>()
                / main.clone().count().max(1) as f32;
            let relative_speed = main
                .map(|particle| particle.velocity.distance(mean_velocity))
                .sum::<f32>()
                / state.diagnostics.main_mass.max(1.0);
            maximum_relative_speed = maximum_relative_speed.max(relative_speed);
        }

        assert!(
            maximum_width - minimum_width > 0.045,
            "flight silhouette stayed rigid: width range {minimum_width:.3}..{maximum_width:.3}"
        );
        // Cruise velocity is translation-only in the comoving model; the
        // acceleration, braking and turn phases must still create visible
        // internal motion without requiring a permanently stretched tail.
        assert!(
            maximum_relative_speed > 0.06,
            "flight produced no internal slosh: {maximum_relative_speed:.3}"
        );
    }

    #[test]
    fn json_text_import_rejects_unknown_schema() {
        let mut profile = LiquidTuningProfile::default();
        profile.schema_version += 1;
        let json = serde_json::to_vec(&profile).unwrap();
        assert!(decode_profile(&json).is_err());
    }

    #[test]
    fn physics_panel_exposes_only_the_schema_sixteen_authoring_controls() {
        let source = include_str!("main.rs");
        let panel = source
            .split_once("fn physics_controls")
            .unwrap()
            .1
            .split_once("fn material_controls")
            .unwrap()
            .0;
        for active_label in [
            "Character field radius",
            "Character field strength",
            "Flight plasticity",
            "Pointer support (h)",
            "Pointer strength",
            "Pointer response (Hz)",
            "Density compliance",
            "Density iterations",
            "Fluid cohesion",
            "Artistic viscosity",
            "Numerical XSPH",
            "Anisotropy max",
        ] {
            assert!(panel.contains(active_label), "missing {active_label}");
        }
        for legacy_label in [
            "Impact substeps",
            "Shape recovery",
            "Field firm-up time",
            "Idle piece size",
            "Bud edge pull",
            "Pinch bounce",
            "Upright stabilization",
            "Grab damping",
            "Bond compliance",
            "Tear speed",
        ] {
            assert!(
                !panel.contains(legacy_label),
                "legacy control {legacy_label}"
            );
        }
    }

    #[test]
    fn preview_drag_drives_material_without_moving_hidden_body_origin() {
        let drag = PreviewDrag {
            velocity_normalized: Vec2::new(0.8, -0.3),
            acceleration_normalized: Vec2::new(4.0, 2.0),
            active: true,
            ..PreviewDrag::default()
        };
        let (body_velocity, body_acceleration, pressure, interaction_velocity) =
            resolved_preview_motion(PreviewScenario::DirectionReversal, 1.8, drag);
        assert_eq!(body_velocity, Vec2::ZERO);
        assert_eq!(body_acceleration, Vec2::ZERO);
        assert_eq!(pressure, 0.0);
        assert_eq!(interaction_velocity, drag.velocity_normalized);
        assert_eq!(
            preview_navigation_anchor(PreviewScenario::DirectionReversal, drag),
            0.0
        );
        assert_eq!(
            preview_navigation_anchor(PreviewScenario::Calm, PreviewDrag::default()),
            0.0
        );
    }

    #[test]
    fn controlled_intervention_builds_a_bounded_valid_drive_command() {
        let envelope = build_lab_control_envelope(
            0,
            42_000,
            LabControlCommand::DrivePulse {
                drive: LabDrive::Curiosity,
                delta: 0.35,
                duration_seconds: 2.25,
            },
        );
        assert_eq!(envelope.schema_version, LAB_CONTROL_SCHEMA_VERSION);
        assert_eq!(envelope.command_id, 1);
        assert_eq!(envelope.issued_unix_ms, 42_000);
        assert_eq!(envelope.expires_after_ms, 4_250);
        assert_eq!(
            envelope.command,
            LabControlCommand::DrivePulse {
                drive: LabDrive::Curiosity,
                delta: 0.35,
                duration_seconds: 2.25,
            }
        );
        envelope.validate().unwrap();
    }

    #[test]
    fn evolution_rewind_matches_lineage_truncates_future_and_merges_latest() {
        let canonical = CanonicalEvolutionCache {
            available: true,
            lineage_id: "0000000000000000000000000000002a".into(),
            generation: 3,
            history: vec![
                serde_json::json!({"generation": 1, "marker": "state-1"}),
                serde_json::json!({"generation": 2, "marker": "state-2"}),
                serde_json::json!({"generation": 3, "marker": "future-3"}),
            ],
            status: "canonical".into(),
        };
        let frame = serde_json::json!({
            "details": {
                "identity": {
                    "lineage_id": "0000000000000000000000000000002a",
                    "generation": 2
                },
                "development": {
                    "mutation_history_count": 2,
                    "latest_mutation": {"generation": 2, "marker": "telemetry-2"}
                }
            }
        });

        let (history, source) = evolution_history_for_frame(&frame, &canonical);
        assert_eq!(history.len(), 2);
        assert_eq!(history[0]["generation"], 1);
        assert_eq!(history[1]["marker"], "telemetry-2");
        assert!(source.contains("lineage matched"));

        let checkpoints = evolution_checkpoints(&history);
        let mut scrub = EvolutionScrubState::default();
        scrub.synchronize(mutation_history_key(&history), checkpoints.len());
        assert_eq!(scrub.selected, checkpoints.len() - 1);
        scrub.selected = 0;
        scrub.synchronize(mutation_history_key(&history), checkpoints.len());
        assert_eq!(scrub.selected, 0, "same history preserves scrub position");
    }

    #[test]
    fn bootstrap_frames_stay_stale_and_command_ids_survive_clock_rollback() {
        assert!(!telemetry_read_is_live(false, 20));
        assert!(telemetry_read_is_live(true, 1));
        assert!(!telemetry_read_is_live(true, 0));

        let prior = 9_000_000;
        let after_clock_rollback = next_lab_command_id(1, 1, prior);
        assert_eq!(after_clock_rollback, prior + 1);
        assert_ne!(after_clock_rollback, 0);
    }

    #[test]
    fn dev_console_has_four_clear_panels_and_only_character_is_offline() {
        assert_eq!(DevPanel::ALL.len(), 4);
        assert_eq!(DevPanel::Character.label(), "Character");
        assert_eq!(DevPanel::Perception.label(), "Perception");
        assert_eq!(DevPanel::Behavior.label(), "Behavior");
        assert_eq!(DevPanel::Diagnostics.label(), "Diagnostics");
        assert!(!DevPanel::Character.is_live());
        assert!(DevPanel::ALL[1..].iter().all(|panel| panel.is_live()));
    }

    #[test]
    fn interest_summary_explains_sleep_typing_and_window_glances() {
        let sleeping = serde_json::json!({
            "details": {"ecology": {"active_goal": "SleepInDen"}}
        });
        assert!(interest_explanation(&sleeping).contains("Rest owns behavior"));

        let typing = serde_json::json!({
            "details": {
                "gaze": {"attention_kind": "Typing"},
                "typing_rate_hz": 4.5,
                "ecology": {}
            }
        });
        assert!(interest_explanation(&typing).contains("glance rather than a movement command"));

        let window = serde_json::json!({
            "details": {
                "gaze": {"attention_kind": "Window"},
                "ecology": {}
            }
        });
        assert!(interest_explanation(&window).contains("background"));
    }
}
