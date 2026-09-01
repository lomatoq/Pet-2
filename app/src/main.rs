#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]
#![recursion_limit = "512"]

mod ecology_runtime;
mod evolution_runner;
mod nervous_system_runtime;
mod pointer_replay_runner;
#[allow(dead_code)]
mod replay;
mod vita_runtime;

use std::{
    collections::{BTreeMap, VecDeque},
    env,
    error::Error,
    path::PathBuf,
    sync::{
        Arc,
        mpsc::{self, Receiver, SyncSender, TryRecvError, TrySendError},
    },
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use desktop_host::{
    DesktopVisualFrame, DisplayTopology, EventLogEntry, EvolutionPersistence, LabControlCommand,
    LabControlEnvelope, LabDrive, LabGesture, MonitorId, MonitorInfo,
    PORTABLE_STATE_SCHEMA_VERSION, PersistedPetPosition, PhysicalDesktopPoint, PlatformBackend,
    PointerState, PortablePetState, RUNTIME_LOAD_ACK_SCHEMA_VERSION, RectI,
    RuntimeLoadAcknowledgement, RuntimeLoadStatus, SensorNormalizer, StateStore,
    create_platform_backend, prepare_overlay_window_attributes,
};
use ecology_runtime::{EcologyResolveFrame, EcologyRuntime, VisualAttentionSample};
use glam::Vec2;
#[cfg(test)]
use lifecore::MAX_MUTATION_HISTORY;
use lifecore::{
    ActionId, BodyIntent, CollisionEvent, DebugState, Drives, EmbodiedGestureEvent,
    EmbodiedGestureKind, ExpressionDirector, ExpressionState, FeedbackEvent, Genome,
    GestureBoundaryEvent, LIFECORE_HZ, LifeCore, LivingStateFrame, LocomotionMode, PoseIntent,
    SensorFrame, VitaOutput, VocalArbiter, VocalTrigger, persisted_life_snapshot_hash,
    stable_hash_bytes,
};
use morph_brain::{MorphBrain, MorphBrainState, MorphCommand, MorphOutput};
use nervous_system_runtime::NervousSystemRuntime;
use pet_audio::{
    AudioCallbackLevels, AudioEngine, AudioVisualFeedback, BodyVoiceAnalyzer, SelectedOutputConfig,
    global_body_voice_bridge,
};
use pet_body::{
    BodyMaterialSnapshot, BodyRenderMode, EcologyRenderer, LiquidTuningAcknowledgement,
    LiquidTuningProfile, MaterialVariant, ProceduralBody, RenderOutcome, Renderer, VisualMindInput,
    VoiceVisualState,
};
#[cfg(not(feature = "legacy-expression-fallback"))]
use pet_ecology::ObjectLifecycle;
use pet_ecology::{
    ActionSignature, ConventionOutcome, EcologyOutcome, EcologyVocalTrigger, EpisodeGoal,
    EpisodePhase, GestureConventionMeaning, GestureSignature, MorselProfile, ObjectKind,
};
use pet_perception::{
    EmbodiedGestureClassifierTuning, SpatialVisualCell, SpatialVisualFrame, VisualFeatureFrame,
};
use vita_runtime::{BrainMode, VitaRuntime};
use winit::{
    application::ApplicationHandler,
    dpi::{PhysicalPosition, PhysicalSize},
    event::{DeviceEvent, DeviceId, ElementState, MouseButton, MouseScrollDelta, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, DeviceEvents, EventLoop},
    keyboard::{KeyCode, ModifiersState, PhysicalKey},
    window::{Window, WindowId, WindowLevel},
};

const LIFE_DT: f32 = 1.0 / LIFECORE_HZ;
const SENSOR_DT: f32 = 1.0 / 120.0;
const ACTIVE_FRAME: Duration = Duration::from_micros(8_333);
// Fast desktop flight presents at the same 120 Hz cadence as default liquid
// physics. Pointer interaction stays at 60 Hz presentation while the accumulator
// continues solving at 120 Hz: drawing the virtual-desktop-sized transparent
// surface twice as often costs far more than the compact pointer field itself.
// LifeCore remains on its own slow accumulator.
const SLEEP_FRAME: Duration = Duration::from_micros(16_667);
// Production owns one stable virtual-desktop composition surface. The previous
// 1152 px camera window had to jump by hundreds of pixels as the creature flew;
// DWM could briefly compose an old swapchain image at the new HWND origin and
// produce an A -> B -> A flash. A fixed host removes that failure mode entirely:
// only the liquid's presentation offset changes during ordinary flight.
const PRODUCTION_FLIGHT_RESPONSE_SCALE: f32 = 2.15;
const PRODUCTION_FLIGHT_ACCELERATION_LIMIT: f32 = 4.20;
const VISUAL_SAMPLE_INTERVAL: f32 = 1.0 / 8.0;
// A delayed event-loop callback must not replay a quarter-second of navigation
// before presenting one frame. Three fixed ticks preserve normal 120 Hz
// cadence while turning a long stall into a small time drop instead of a visual
// teleport.
const MAX_BODY_STEPS_PER_FRAME: u32 = 3;
// Reference calibration for the 1152 px production host.
const PRESENTATION_REFERENCE_HEIGHT: f32 = 1_152.0;
const PRODUCTION_PRESENTATION_SCALE: f32 = 1.845;
// The static desktop host can be much larger than the old camera surface.
// Running its liquid targets at 2x would clear several desktop-sized HDR
// attachments per frame even though drawing is scissored to the creature.
// Keep Lab at authored 2x and use native resolution for production.
const PRODUCTION_RENDER_SCALE: u32 = 1;
const DEBUG_LOG_INTERVAL: f32 = 1.0;
const DEV_MODE_LOG_INTERVAL: f32 = 0.20;
const LAB_CONTROL_POLL_INTERVAL_SECONDS: f32 = 0.10;
const CAUSAL_TELEMETRY_SCHEMA_VERSION: u32 = 4;
const TELEMETRY_RECENT_MEMORY_LIMIT: usize = 16;
const TELEMETRY_EPISODE_MEMORY_LIMIT: usize = 16;

fn main() -> Result<(), Box<dyn Error>> {
    let arguments = Arguments::parse(env::args().skip(1))?;
    startup_probe(arguments.debug_log, "arguments parsed");
    if arguments.help {
        print_help();
        return Ok(());
    }
    let store = arguments
        .data_dir
        .as_ref()
        .map(StateStore::at)
        .map_or_else(StateStore::discover, Ok)?;
    startup_probe(arguments.debug_log, "state store ready");
    if arguments.pointer_replay.is_some() {
        let prepared = prepare_state(&arguments, &store)?;
        return pointer_replay_runner::run(&arguments, &store, prepared);
    }
    if arguments.evolution_config.is_some() || arguments.simulate_hours.is_some() {
        let prepared = prepare_state(&arguments, &store)?;
        return evolution_runner::run(&arguments, &store, prepared);
    }
    if arguments.headless || arguments.headless_smoke_seconds.is_some() {
        return run_headless(arguments, store);
    }
    let Some(_single_instance) = SingleInstanceGuard::acquire()? else {
        eprintln!("Pet 2 is already running; refusing to create a second desktop organism");
        return Ok(());
    };
    let prepared = prepare_state(&arguments, &store)?;
    startup_probe(arguments.debug_log, "portable state prepared");
    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(ControlFlow::Wait);
    let mut application = PetApplication::new(arguments, store, prepared)?;
    startup_probe(application.arguments.debug_log, "application assembled");
    event_loop.run_app(&mut application)?;
    Ok(())
}

fn startup_probe(enabled: bool, stage: &str) {
    if enabled {
        eprintln!("Pet 2 startup: {stage}");
    }
}

#[cfg(windows)]
struct SingleInstanceGuard(windows_sys::Win32::Foundation::HANDLE);

#[cfg(windows)]
impl SingleInstanceGuard {
    fn acquire() -> Result<Option<Self>, std::io::Error> {
        use std::ptr;
        use windows_sys::Win32::{
            Foundation::{ERROR_ALREADY_EXISTS, GetLastError},
            System::Threading::CreateMutexW,
        };

        let name: Vec<u16> = "Local\\Pet2.DesktopOrganism.Singleton.v1\0"
            .encode_utf16()
            .collect();
        // SAFETY: the name is a stable, NUL-terminated UTF-16 allocation and the
        // optional security attributes pointer is null. The returned handle is
        // owned by the guard below and closed exactly once.
        let handle = unsafe { CreateMutexW(ptr::null(), 1, name.as_ptr()) };
        if handle.is_null() {
            return Err(std::io::Error::last_os_error());
        }
        // GetLastError must be observed before another Win32 call. A named mutex
        // survives exactly as long as at least one process holds a handle, so a
        // crashed Pet cannot leave a stale filesystem lock behind.
        let already_running = unsafe { GetLastError() } == ERROR_ALREADY_EXISTS;
        if already_running {
            // SAFETY: CreateMutexW returned a valid owned handle above.
            unsafe { windows_sys::Win32::Foundation::CloseHandle(handle) };
            Ok(None)
        } else {
            Ok(Some(Self(handle)))
        }
    }
}

#[cfg(windows)]
impl Drop for SingleInstanceGuard {
    fn drop(&mut self) {
        // SAFETY: this guard uniquely owns the non-null mutex handle.
        unsafe { windows_sys::Win32::Foundation::CloseHandle(self.0) };
    }
}

#[cfg(not(windows))]
struct SingleInstanceGuard;

#[cfg(not(windows))]
impl SingleInstanceGuard {
    fn acquire() -> Result<Option<Self>, std::io::Error> {
        Ok(Some(Self))
    }
}

#[derive(Debug, Clone, Default)]
struct Arguments {
    seed: Option<u64>,
    headless: bool,
    headless_smoke_seconds: Option<f32>,
    simulate_hours: Option<f32>,
    pointer_replay: Option<PathBuf>,
    evolution_config: Option<PathBuf>,
    evolution_report: Option<PathBuf>,
    evolution_progress: Option<PathBuf>,
    evolution_persist: Option<EvolutionPersistence>,
    evolution_max_generations: Option<u32>,
    export_state: Option<PathBuf>,
    import_state: Option<PathBuf>,
    data_dir: Option<PathBuf>,
    reset_learning: bool,
    reset_pet: bool,
    no_audio: bool,
    focus_mode: bool,
    brain_mode: BrainMode,
    debug_log: bool,
    dev_mode: bool,
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
                "--pointer-replay" => {
                    parsed.pointer_replay =
                        Some(PathBuf::from(value("--pointer-replay", &mut arguments)?));
                }
                "--evolution-config" => {
                    parsed.evolution_config =
                        Some(PathBuf::from(value("--evolution-config", &mut arguments)?));
                }
                "--evolution-report" => {
                    parsed.evolution_report =
                        Some(PathBuf::from(value("--evolution-report", &mut arguments)?));
                }
                "--evolution-progress" => {
                    parsed.evolution_progress = Some(PathBuf::from(value(
                        "--evolution-progress",
                        &mut arguments,
                    )?));
                }
                "--evolution-persist" => {
                    parsed.evolution_persist =
                        Some(value("--evolution-persist", &mut arguments)?.parse()?);
                }
                "--evolution-max-generations" => {
                    parsed.evolution_max_generations =
                        Some(value("--evolution-max-generations", &mut arguments)?.parse()?);
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
                "--evolve" => {
                    return Err(
                        "--evolve was retired because an unconditional metamorphosis has no eligibility report; use --evolution-config with an explicit policy"
                            .into(),
                    );
                }
                "--no-audio" | "--no-audio-output" => parsed.no_audio = true,
                "--focus-mode" => parsed.focus_mode = true,
                "--brain-mode" => {
                    parsed.brain_mode = value("--brain-mode", &mut arguments)?.parse()?;
                }
                "--debug-log" => parsed.debug_log = true,
                "--dev-mode" => {
                    parsed.debug_log = true;
                    parsed.dev_mode = true;
                }
                "--help" | "-h" => parsed.help = true,
                other => return Err(format!("unknown argument: {other}").into()),
            }
        }
        Ok(parsed)
    }
}

struct PreparedState {
    life: LifeCore,
    loaded_life_state_hash: u64,
    position: PersistedPetPosition,
    vita: VitaRuntime,
    morph: MorphBrain,
    ecology: EcologyRuntime,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AudioManagerState {
    Disabled,
    Ready,
    Failed,
    Recovering,
}

struct AudioManager {
    command_sender: Option<SyncSender<AudioWorkerCommand>>,
    event_receiver: Option<Receiver<AudioWorkerEvent>>,
    state: AudioManagerState,
    selected_config: Option<SelectedOutputConfig>,
    device_name: Option<String>,
    last_error: Option<String>,
    last_request: Option<u64>,
    retry_attempt: usize,
    retry_at: Option<Instant>,
    pending_requests: VecDeque<u64>,
    heard_requests: VecDeque<u64>,
    unheard_rejections: VecDeque<u64>,
    accepted_requests: u64,
    rejected_requests: u64,
    recent_rms: f32,
    recent_peak: f32,
    body_voice_analyzer: BodyVoiceAnalyzer,
}

enum AudioWorkerCommand {
    Enqueue {
        voice: lifecore::VoiceGenome,
        motif: lifecore::VocalMotif,
        request: lifecore::VocalRequest,
    },
}

#[derive(Debug, Clone, PartialEq)]
enum AudioWorkerEvent {
    Ready {
        config: SelectedOutputConfig,
        device_name: String,
    },
    StartFailed {
        error: String,
    },
    RequestHeard {
        request_id: u64,
    },
    RequestRejected {
        request_id: u64,
        error: String,
    },
    RuntimeLost {
        error: String,
    },
}

struct AudioWorkerChannels {
    command_sender: SyncSender<AudioWorkerCommand>,
    event_receiver: Receiver<AudioWorkerEvent>,
}

const AUDIO_COMMAND_CAPACITY: usize = 32;
const AUDIO_EVENT_CAPACITY: usize = 64;
const AUDIO_OWNER_POLL_INTERVAL: Duration = Duration::from_millis(10);

#[derive(Debug, Clone, Copy, Default)]
struct TimingPercentiles {
    p50: f32,
    p95: f32,
    p99: f32,
}

struct TimingWindow {
    samples_ms: [f32; 240],
    count: usize,
    cursor: usize,
}

#[derive(Debug, Clone, Copy, Default)]
struct PresentedPoseMonitor {
    older_center: Option<Vec2>,
    previous_center: Option<Vec2>,
    frame_count: u64,
    maximum_step_px: f32,
    large_step_events: u64,
    ping_pong_events: u64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct PresentationCadence {
    fast_flight: bool,
}

impl PresentationCadence {
    fn frame_interval(&mut self, screen_speed_px: f32) -> Duration {
        let screen_speed_px = if screen_speed_px.is_finite() {
            screen_speed_px.max(0.0)
        } else {
            0.0
        };
        if self.fast_flight {
            if screen_speed_px < 8.0 {
                self.fast_flight = false;
            }
        } else if screen_speed_px > 12.0 {
            self.fast_flight = true;
        }
        if self.fast_flight {
            ACTIVE_FRAME
        } else {
            SLEEP_FRAME
        }
    }
}

impl PresentedPoseMonitor {
    fn observe(&mut self, center: Vec2) {
        if !center.is_finite() {
            return;
        }
        if let Some(previous) = self.previous_center {
            let step = center.distance(previous);
            self.maximum_step_px = self.maximum_step_px.max(step);
            if step > 12.0 {
                self.large_step_events = self.large_step_events.saturating_add(1);
            }
            if let Some(older) = self.older_center
                && previous.distance(older) > 12.0
                && center.distance(older) < 3.0
            {
                self.ping_pong_events = self.ping_pong_events.saturating_add(1);
            }
        }
        self.older_center = self.previous_center;
        self.previous_center = Some(center);
        self.frame_count = self.frame_count.saturating_add(1);
    }
}

impl Default for TimingWindow {
    fn default() -> Self {
        Self {
            samples_ms: [0.0; 240],
            count: 0,
            cursor: 0,
        }
    }
}

impl TimingWindow {
    fn observe(&mut self, milliseconds: f32) {
        if !milliseconds.is_finite() || milliseconds < 0.0 {
            return;
        }
        self.samples_ms[self.cursor] = milliseconds;
        self.cursor = (self.cursor + 1) % self.samples_ms.len();
        self.count = (self.count + 1).min(self.samples_ms.len());
    }

    fn percentiles(&self) -> TimingPercentiles {
        if self.count == 0 {
            return TimingPercentiles::default();
        }
        let mut sorted = self.samples_ms;
        sorted[..self.count].sort_by(f32::total_cmp);
        let at = |quantile: f32| {
            let index = ((self.count as f32 * quantile).ceil() as usize)
                .saturating_sub(1)
                .min(self.count - 1);
            sorted[index]
        };
        TimingPercentiles {
            p50: at(0.50),
            p95: at(0.95),
            p99: at(0.99),
        }
    }
}

impl AudioManager {
    fn new(disabled: bool) -> Self {
        let mut manager = Self {
            command_sender: None,
            event_receiver: None,
            state: if disabled {
                AudioManagerState::Disabled
            } else {
                AudioManagerState::Recovering
            },
            selected_config: None,
            device_name: None,
            last_error: None,
            last_request: None,
            retry_attempt: 0,
            retry_at: None,
            pending_requests: VecDeque::with_capacity(AUDIO_COMMAND_CAPACITY),
            heard_requests: VecDeque::with_capacity(AUDIO_COMMAND_CAPACITY),
            unheard_rejections: VecDeque::with_capacity(AUDIO_COMMAND_CAPACITY),
            accepted_requests: 0,
            rejected_requests: 0,
            recent_rms: 0.0,
            recent_peak: 0.0,
            body_voice_analyzer: BodyVoiceAnalyzer::default(),
        };
        if !disabled {
            manager.start_in_background();
        }
        manager
    }

    fn start_in_background(&mut self) {
        if self.state == AudioManagerState::Disabled
            || self.command_sender.is_some()
            || self.event_receiver.is_some()
        {
            return;
        }
        self.state = AudioManagerState::Recovering;
        self.retry_at = None;
        match spawn_audio_owner_worker() {
            Ok(channels) => {
                self.command_sender = Some(channels.command_sender);
                self.event_receiver = Some(channels.event_receiver);
            }
            Err(error) => self.lose_worker(error),
        }
    }

    fn poll_worker_events(&mut self) {
        loop {
            let event = match self.event_receiver.as_ref().map(Receiver::try_recv) {
                Some(Ok(event)) => event,
                Some(Err(TryRecvError::Empty)) | None => break,
                Some(Err(TryRecvError::Disconnected)) => {
                    self.lose_worker("audio owner stopped without a final status".into());
                    break;
                }
            };
            match event {
                AudioWorkerEvent::Ready {
                    config,
                    device_name,
                } => {
                    self.state = AudioManagerState::Ready;
                    self.selected_config = Some(config);
                    self.device_name = Some(device_name);
                    self.retry_attempt = 0;
                    self.retry_at = None;
                    self.last_error = None;
                }
                AudioWorkerEvent::StartFailed { error }
                | AudioWorkerEvent::RuntimeLost { error } => {
                    self.lose_worker(error);
                    break;
                }
                AudioWorkerEvent::RequestHeard { request_id } => {
                    if self.remove_pending_request(request_id) {
                        self.heard_requests.push_back(request_id);
                        self.accepted_requests = self.accepted_requests.saturating_add(1);
                        self.last_error = None;
                    }
                }
                AudioWorkerEvent::RequestRejected { request_id, error } => {
                    self.reject_pending_request(request_id, error);
                }
            }
        }
    }

    fn lose_worker(&mut self, error: String) {
        self.command_sender = None;
        self.event_receiver = None;
        self.selected_config = None;
        self.device_name = None;
        while let Some(request_id) = self.pending_requests.pop_front() {
            self.unheard_rejections.push_back(request_id);
            self.rejected_requests = self.rejected_requests.saturating_add(1);
        }
        self.state = AudioManagerState::Failed;
        self.last_error = Some(error.clone());
        let delays = [0.5, 1.0, 2.0, 5.0];
        let delay = delays[self.retry_attempt.min(delays.len() - 1)];
        self.retry_attempt = self.retry_attempt.saturating_add(1);
        self.retry_at = Some(Instant::now() + Duration::from_secs_f32(delay));
        eprintln!("audio start failed; retrying in {delay:.1}s: {error}");
    }

    fn remove_pending_request(&mut self, request_id: u64) -> bool {
        let Some(index) = self
            .pending_requests
            .iter()
            .position(|pending| *pending == request_id)
        else {
            return false;
        };
        self.pending_requests.remove(index);
        true
    }

    fn reject_pending_request(&mut self, request_id: u64, error: String) {
        if self.remove_pending_request(request_id) {
            self.unheard_rejections.push_back(request_id);
            self.rejected_requests = self.rejected_requests.saturating_add(1);
        }
        self.last_error = Some(error);
    }

    fn tick(&mut self) {
        if self.state == AudioManagerState::Disabled {
            return;
        }
        self.poll_worker_events();
        let levels = self.callback_levels();
        self.recent_rms = levels.rms.max(self.recent_rms * 0.965);
        self.recent_peak = levels.peak.max(self.recent_peak * 0.965);
        if self.command_sender.is_none()
            && self.event_receiver.is_none()
            && self
                .retry_at
                .is_some_and(|deadline| Instant::now() >= deadline)
        {
            self.start_in_background();
        }
    }

    fn enqueue(
        &mut self,
        voice: &lifecore::VoiceGenome,
        motif: &lifecore::VocalMotif,
        request: &lifecore::VocalRequest,
    ) -> bool {
        self.last_request = Some(request.motif_id);
        if self.state != AudioManagerState::Ready {
            self.rejected_requests = self.rejected_requests.saturating_add(1);
            self.last_error = Some(format!(
                "vocal motif {} rejected: audio output is not ready",
                request.motif_id
            ));
            return false;
        }
        let Some(sender) = self.command_sender.as_ref() else {
            self.rejected_requests = self.rejected_requests.saturating_add(1);
            self.last_error = Some(format!(
                "vocal motif {} rejected: audio owner is unavailable",
                request.motif_id
            ));
            return false;
        };
        let command = AudioWorkerCommand::Enqueue {
            voice: voice.clone(),
            motif: motif.clone(),
            request: request.clone(),
        };
        match sender.try_send(command) {
            Ok(()) => {
                self.pending_requests.push_back(request.performance_seed);
                self.last_error = None;
                true
            }
            Err(TrySendError::Full(_)) => {
                self.rejected_requests = self.rejected_requests.saturating_add(1);
                self.last_error = Some(format!(
                    "vocal motif {} rejected: audio command queue is full",
                    request.motif_id
                ));
                false
            }
            Err(TrySendError::Disconnected(_)) => {
                self.rejected_requests = self.rejected_requests.saturating_add(1);
                self.lose_worker("audio owner command channel disconnected".into());
                false
            }
        }
    }

    fn publish_body_voice(&mut self, raw: lifecore::BodyVoiceFrame, dt: f32) {
        let frame = self.body_voice_analyzer.update(raw, dt);
        global_body_voice_bridge().publish(frame);
    }

    fn take_unheard_rejection(&mut self) -> Option<u64> {
        self.unheard_rejections.pop_front()
    }

    fn take_heard_request(&mut self) -> Option<u64> {
        self.heard_requests.pop_front()
    }

    fn visual_feedback(&self) -> AudioVisualFeedback {
        if self.state == AudioManagerState::Ready {
            pet_audio::global_visual_feedback()
        } else {
            AudioVisualFeedback::default()
        }
    }

    fn callback_levels(&self) -> AudioCallbackLevels {
        if self.state == AudioManagerState::Ready {
            pet_audio::global_visual_bridge().levels()
        } else {
            AudioCallbackLevels::default()
        }
    }

    fn selected_config(&self) -> Option<SelectedOutputConfig> {
        self.selected_config
    }

    fn device_name(&self) -> Option<&str> {
        self.device_name.as_deref()
    }
}

fn spawn_audio_owner_worker() -> Result<AudioWorkerChannels, String> {
    spawn_audio_owner_worker_with(audio_owner_loop)
}

fn spawn_audio_owner_worker_with(
    run: impl FnOnce(Receiver<AudioWorkerCommand>, SyncSender<AudioWorkerEvent>) + Send + 'static,
) -> Result<AudioWorkerChannels, String> {
    let (command_sender, command_receiver) = mpsc::sync_channel(AUDIO_COMMAND_CAPACITY);
    let (event_sender, event_receiver) = mpsc::sync_channel(AUDIO_EVENT_CAPACITY);
    thread::Builder::new()
        .name("pet2-audio-owner".into())
        .spawn(move || run(command_receiver, event_sender))
        .map_err(|error| format!("could not start audio owner worker: {error}"))?;
    Ok(AudioWorkerChannels {
        command_sender,
        event_receiver,
    })
}

fn audio_owner_loop(commands: Receiver<AudioWorkerCommand>, events: SyncSender<AudioWorkerEvent>) {
    let engine = match AudioEngine::try_start() {
        Ok(engine) => engine,
        Err(error) => {
            let _ = events.send(AudioWorkerEvent::StartFailed {
                error: error.to_string(),
            });
            return;
        }
    };
    engine.clear_visual_feedback();
    if events
        .send(AudioWorkerEvent::Ready {
            config: engine.selected_config(),
            device_name: engine.device_name().to_owned(),
        })
        .is_err()
    {
        return;
    }

    let mut pending_unheard = VecDeque::with_capacity(AUDIO_COMMAND_CAPACITY);
    loop {
        while let Some(request_id) = engine.poll_started_request() {
            if let Some(index) = pending_unheard
                .iter()
                .position(|(pending_id, _)| *pending_id == request_id)
            {
                pending_unheard.remove(index);
                if events
                    .send(AudioWorkerEvent::RequestHeard { request_id })
                    .is_err()
                {
                    return;
                }
            }
        }
        if engine.poll_runtime_event().is_some() {
            engine.clear_visual_feedback();
            for (request_id, _) in pending_unheard.drain(..) {
                let _ = events.send(AudioWorkerEvent::RequestRejected {
                    request_id,
                    error: "audio stream failed before the request was heard".into(),
                });
            }
            let _ = events.send(AudioWorkerEvent::RuntimeLost {
                error: "audio output stream reported a runtime error".into(),
            });
            return;
        }

        match commands.recv_timeout(AUDIO_OWNER_POLL_INTERVAL) {
            Ok(AudioWorkerCommand::Enqueue {
                voice,
                motif,
                request,
            }) => match engine.enqueue(&voice, &motif, &request) {
                Ok(()) => pending_unheard.push_back((request.performance_seed, request.motif_id)),
                Err(error) => {
                    if events
                        .send(AudioWorkerEvent::RequestRejected {
                            request_id: request.performance_seed,
                            error: error.to_string(),
                        })
                        .is_err()
                    {
                        return;
                    }
                }
            },
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => return,
        }
    }
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
    let imported_state = imported.is_some();
    let saved = if arguments.reset_pet {
        None
    } else if imported.is_some() {
        imported
    } else {
        store.load_state()?
    };
    let persisted_loaded_hash = saved
        .as_ref()
        .map(|saved| persisted_life_snapshot_hash(&saved.life))
        .transpose()?;
    let (mut life, position, vita_state) = if let Some(saved) = saved {
        (LifeCore::restore(saved.life)?, saved.position, saved.vita)
    } else {
        (
            LifeCore::new(Genome::from_seed(seed), seed ^ 0xA11F_EC0A),
            PersistedPetPosition::default(),
            None,
        )
    };
    life.reset_vocal_delivery_session();
    let identity_seed = life.state.genome.identity_seed;
    let mut vita = VitaRuntime::new(identity_seed, vita_state);
    let morph_state = if arguments.reset_pet || arguments.reset_learning || imported_state {
        None
    } else {
        store.load_morph_brain::<MorphBrainState>()?
    };
    let morph = MorphBrain::new(identity_seed, morph_state)?;
    let ecology = EcologyRuntime::load_or_create(
        store,
        identity_seed,
        arguments.reset_pet || imported_state,
    )?;
    if arguments.reset_learning {
        life.reset_learning();
        vita.reset_learning(identity_seed);
    }
    life.set_focus_mode(arguments.focus_mode);
    let loaded_life_state_hash =
        persisted_loaded_hash.map_or_else(|| persisted_life_snapshot_hash(&life.snapshot()), Ok)?;
    Ok(PreparedState {
        life,
        loaded_life_state_hash,
        position,
        vita,
        morph,
        ecology,
    })
}

fn run_headless(arguments: Arguments, store: StateStore) -> Result<(), Box<dyn Error>> {
    let brain_mode = arguments.brain_mode;
    let PreparedState {
        mut life,
        loaded_life_state_hash: _,
        position,
        mut vita,
        mut morph,
        mut ecology,
    } = prepare_state(&arguments, &store)?;
    let mut body = ProceduralBody::generate(&life.state.genome)?;
    load_migrate_apply_liquid_tuning(&store, &mut body)?;
    vita.set_embodied_gesture_tuning(classifier_tuning(body.tuning_profile().interaction));
    if !arguments.reset_pet && arguments.import_state.is_none() {
        load_restore_body_state(&store, &mut body)?;
    }
    let duration_seconds = arguments.headless_smoke_seconds.unwrap_or(60.0).max(0.05);
    let dt = LIFE_DT;
    let tick_count = (duration_seconds / dt).ceil() as u64;
    let mut sensors = SensorFrame::default();
    let mut feedback = body.simulation.feedback.clone();
    let mut action_counts = BTreeMap::<String, u64>::new();
    let mut morph_command_counts = BTreeMap::<String, u64>::new();
    let mut morph_control_counts = BTreeMap::<String, u64>::new();
    let mut morph_switches = 0_u64;
    let mut previous_morph_command = MorphCommand::Idle;
    let mut morph_tick_microseconds = Vec::with_capacity(tick_count.min(20_000) as usize);
    let mut distance_traveled = 0.0_f32;
    let mut moving_ticks = 0_u64;
    let mut goal_progress_ticks = 0_u64;
    let mut zone_transitions = 0_u64;
    let mut previous_zone = spatial_zone(feedback.world_position);
    let mut episode_starts = BTreeMap::<String, u64>::new();
    let mut episode_completions = BTreeMap::<String, u64>::new();
    let mut orb_contact_count = 0_u64;
    let mut orb_distance_traveled = 0.0_f32;
    let mut previous_orb_position = ecology
        .state()
        .objects
        .iter()
        .find(|object| object.kind == ObjectKind::Orb)
        .map(|object| object.position);
    let mut minimum_position = feedback.world_position;
    let mut maximum_position = feedback.world_position;
    let feedback_interval = (5.0 / dt).round().max(1.0) as u64;
    let empty_window_affordances = pet_ecology::WindowAffordanceFrame::default();
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
        sensors.embodied_interaction = body.embodied_interaction_frame();
        feedback.cursor_contact = sensors.embodied_interaction.contact.active;
        if tick % 11 == 0 {
            vita.note_key_activity(sensors.timestamp);
        }
        if tick % 97 == 0 {
            vita.note_scroll((time * 0.7).sin());
        }
        vita.observe(&sensors, &feedback, dt);
        let morph_started = Instant::now();
        let morph_world = ecology.morph_world_input(feedback.world_position, life.state.drives);
        let morph_output =
            morph.tick_with_world(&sensors, &feedback, &life.state, &morph_world, dt);
        morph_tick_microseconds.push(morph_started.elapsed().as_secs_f64() * 1_000_000.0);
        *morph_command_counts
            .entry(morph_output.command.as_wire().to_owned())
            .or_default() += 1;
        for (channel, control) in [
            ("manipulation", morph_output.manipulation),
            ("perception", morph_output.perception),
            ("carry", morph_output.carry),
        ] {
            if control != MorphCommand::Idle {
                *morph_control_counts
                    .entry(format!("{channel}:{}", control.as_wire()))
                    .or_default() += 1;
            }
        }
        if previous_morph_command != MorphCommand::Idle
            && morph_output.command != previous_morph_command
        {
            morph_switches = morph_switches.saturating_add(1);
        }
        previous_morph_command = morph_output.command;
        let mut output = life.tick(&sensors, &feedback, dt);
        if let Some(event) = vita.take_embodied_gesture() {
            let episode_id = event.classification.episode_id;
            if let Some(plan) = life.observe_embodied_gesture(event) {
                if vita.accept_interaction_response(plan) && output.vocal_request.is_none() {
                    output.vocal_request = plan
                        .voice_trigger
                        .and_then(|trigger| life.request_vocalization(trigger, &sensors));
                }
            } else {
                vita.finish_interaction_appraisal_without_response(episode_id);
            }
        }
        if let Some(request) = output.vocal_request.take() {
            life.cancel_vocal_request(request.performance_seed);
        }
        let (resolved_intent, vita_output) = vita.resolve_intent_with_morph(
            brain_mode,
            &life.state,
            &sensors,
            &feedback,
            output.body_intent,
            Some(morph_output),
            dt,
        );
        sensors.interaction_actuation = vita.interaction_actuation();
        output.body_intent = resolved_intent;
        let ecology_output = ecology.resolve_intent(
            output.body_intent,
            EcologyResolveFrame {
                selected_action: output.selected_action,
                drives: life.state.drives,
                sensors: &sensors,
                body: &feedback,
                focus_mode: life.state.focus_mode,
                dt,
            },
        );
        for outcome in ecology_output.outcomes[..ecology_output.outcome_count]
            .iter()
            .copied()
        {
            match outcome {
                EcologyOutcome::EpisodeStarted(goal) => {
                    *episode_starts.entry(format!("{goal:?}")).or_default() += 1;
                }
                EcologyOutcome::EpisodeCompleted(goal) => {
                    *episode_completions.entry(format!("{goal:?}")).or_default() += 1;
                }
                EcologyOutcome::ObjectContact(_) => {
                    orb_contact_count = orb_contact_count.saturating_add(1);
                }
                _ => {}
            }
        }
        output.body_intent = ecology_output.body_intent;
        let visual_mind = visual_mind_input(
            &life,
            &sensors,
            vita_output.as_ref(),
            vita.state().self_model.uncertainty,
            vita.percept().scroll_velocity,
            vita.percept().window_pressure,
        );
        *action_counts
            .entry(format!("{:?}", output.selected_action))
            .or_default() += 1;
        let goal_distance_before = feedback
            .world_position
            .distance(output.body_intent.target_position);
        body.fixed_update(
            &life.state.genome,
            &output.body_intent,
            &sensors,
            dt.min(1.0 / 30.0),
        );
        apply_headless_rect_domain(&mut body.simulation.feedback);
        ecology.fixed_update(
            16.0 / 9.0,
            &empty_window_affordances,
            &body.simulation.feedback,
            dt.min(1.0 / 30.0),
        );
        body.set_embodied_environment(ecology.environment());
        body.set_ecology_visual_effect(ecology.visual_effect());
        body.embodied_update(
            &output.body_intent,
            &sensors,
            output.affect,
            visual_mind,
            VoiceVisualState::default(),
            dt.min(0.05),
        );
        let next_feedback = body.simulation.feedback.clone();
        let step_distance = next_feedback
            .world_position
            .distance(feedback.world_position);
        distance_traveled += step_distance;
        if step_distance > 0.000_1 {
            moving_ticks = moving_ticks.saturating_add(1);
        }
        let goal_distance_after = next_feedback
            .world_position
            .distance(output.body_intent.target_position);
        if goal_distance_before - goal_distance_after > 0.000_05 {
            goal_progress_ticks = goal_progress_ticks.saturating_add(1);
        }
        let zone = spatial_zone(next_feedback.world_position);
        if zone != previous_zone {
            zone_transitions = zone_transitions.saturating_add(1);
            previous_zone = zone;
        }
        if let Some(orb_position) = ecology
            .state()
            .objects
            .iter()
            .find(|object| object.kind == ObjectKind::Orb)
            .map(|object| object.position)
        {
            if let Some(previous) = previous_orb_position {
                orb_distance_traveled += orb_position.distance(previous);
            }
            previous_orb_position = Some(orb_position);
        }
        minimum_position = minimum_position.min(next_feedback.world_position);
        maximum_position = maximum_position.max(next_feedback.world_position);
        feedback = next_feedback;
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
            morph.apply_feedback(&event);
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
    morph_tick_microseconds.sort_by(f64::total_cmp);
    let morph_p50_us = percentile(&morph_tick_microseconds, 0.50);
    let morph_p95_us = percentile(&morph_tick_microseconds, 0.95);
    let morph_max_us = morph_tick_microseconds.last().copied().unwrap_or(0.0);
    let morph_state = morph.snapshot();
    let ecology_state = ecology.snapshot();
    let ecology_bytes = serde_json::to_vec(&ecology_state)?;
    let behavior_acceptance_required = arguments.headless_smoke_seconds.is_some()
        && !arguments.focus_mode
        && duration_seconds >= 60.0;
    let behavior_acceptance = HeadlessBehaviorAcceptance {
        bounded: minimum_position.cmpge(Vec2::splat(0.099)).all()
            && maximum_position.cmple(Vec2::splat(0.901)).all(),
        travels: distance_traveled >= 0.50 && zone_transitions >= 2,
        pursues_goals: goal_progress_ticks as f64 / tick_count.max(1) as f64 >= 0.08,
        plays_with_orb: orb_contact_count >= 1 && orb_distance_traveled >= 0.25,
    };
    let summary = serde_json::json!({
        "schema_version": portable.schema_version,
        "brain_mode": brain_mode.as_str(),
        "simulated_seconds": duration_seconds,
        "ticks": tick_count,
        "generation": life.state.genome.generation,
        "life_hash": stable_hash_bytes(&bytes),
        "genome_hash": life.state.genome.stable_hash(),
        "mesh_hash": body.mesh.stable_hash(),
        "current_action": format!("{:?}", life.state.current_action),
        "action_counts": action_counts,
        "distance_traveled": distance_traveled,
        "moving_fraction": moving_ticks as f64 / tick_count.max(1) as f64,
        "goal_progress_fraction": goal_progress_ticks as f64 / tick_count.max(1) as f64,
        "zone_transitions": zone_transitions,
        "position_bounds": {
            "minimum": [minimum_position.x, minimum_position.y],
            "maximum": [maximum_position.x, maximum_position.y],
        },
        "behavior_acceptance": {
            "required": behavior_acceptance_required,
            "bounded": behavior_acceptance.bounded,
            "travels": behavior_acceptance.travels,
            "pursues_goals": behavior_acceptance.pursues_goals,
            "plays_with_orb": behavior_acceptance.plays_with_orb,
            "passed": behavior_acceptance.passed(),
        },
        "drives": life.state.drives,
        "affect": life.state.affect,
        "vita_attention": format!("{:?}", vita.state().attention.kind),
        "vita_agency": vita.state().self_model.agency,
        "vita_uncertainty": vita.state().self_model.uncertainty,
        "vita_calibration_urge": vita.state().self_model.calibration_urge,
        "vita_attention_switches": vita.state().attention_switches,
        "fusion": vita.fusion_diagnostics().map(|fusion| serde_json::json!({
            "authority": fusion.authority,
            "attention_confidence": fusion.attention_confidence,
            "local_kernel_protected": fusion.local_kernel_protected,
            "used_vita_target": fusion.used_vita_target,
            "morph_authority": fusion.morph_authority,
            "used_morph": fusion.used_morph,
        })),
        "morph": {
            "upstream_commit": morph_brain::UPSTREAM_COMMIT,
            "network": { "neurons": 526, "synapses": 17475, "populations": 57 },
            "command_counts": morph_command_counts,
            "control_counts": morph_control_counts,
            "command_switches": morph_switches,
            "tick_p50_us": morph_p50_us,
            "tick_p95_us": morph_p95_us,
            "tick_max_us": morph_max_us,
            "state_bytes": serde_json::to_vec(&morph_state)?.len(),
            "last": morph_output_json(morph.last_output()),
        },
        "ecology": {
            "schema_version": ecology_state.schema_version,
            "object_count": ecology_state.objects.len(),
            "state_hash": stable_hash_bytes(&ecology_bytes),
            "episode_starts": episode_starts,
            "episode_completions": episode_completions,
            "orb_contact_count": orb_contact_count,
            "orb_distance_traveled": orb_distance_traveled,
        },
    });
    println!("{}", serde_json::to_string_pretty(&summary)?);
    if behavior_acceptance_required && !behavior_acceptance.passed() {
        return Err(format!(
            "headless behavior acceptance failed: bounded={}, travels={}, pursues_goals={}, plays_with_orb={}",
            behavior_acceptance.bounded,
            behavior_acceptance.travels,
            behavior_acceptance.pursues_goals,
            behavior_acceptance.plays_with_orb,
        )
        .into());
    }
    store.save_state(&portable)?;
    store.save_morph_brain(&morph_state)?;
    store.save_ecology_state(&ecology_state)?;
    store.save_body_state(&body.body_material_snapshot())?;
    if let Some(path) = &arguments.export_state {
        store.export_state(&portable, path)?;
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct HeadlessBehaviorAcceptance {
    bounded: bool,
    travels: bool,
    pursues_goals: bool,
    plays_with_orb: bool,
}

impl HeadlessBehaviorAcceptance {
    fn passed(self) -> bool {
        self.bounded && self.travels && self.pursues_goals && self.plays_with_orb
    }
}

fn apply_headless_rect_domain(feedback: &mut lifecore::BodyFeedback) {
    const MARGIN: f32 = 0.10;
    for axis in 0..2 {
        if feedback.world_position[axis] < MARGIN {
            feedback.world_position[axis] = MARGIN;
            if feedback.velocity[axis] < 0.0 {
                feedback.velocity[axis] *= -0.12;
            }
        } else if feedback.world_position[axis] > 1.0 - MARGIN {
            feedback.world_position[axis] = 1.0 - MARGIN;
            if feedback.velocity[axis] > 0.0 {
                feedback.velocity[axis] *= -0.12;
            }
        }
    }
}

fn spatial_zone(position: Vec2) -> u8 {
    let column = (position.x.clamp(0.0, 0.999_999) * 4.0) as u8;
    let row = (position.y.clamp(0.0, 0.999_999) * 3.0) as u8;
    row * 4 + column
}

struct TeachRecorder {
    active: bool,
    started_seconds: f64,
    last_timestamp: f64,
    points: Vec<Vec2>,
}

impl Default for TeachRecorder {
    fn default() -> Self {
        Self {
            active: false,
            started_seconds: 0.0,
            last_timestamp: 0.0,
            points: Vec::with_capacity(720),
        }
    }
}

impl TeachRecorder {
    fn start(&mut self, position: Vec2, timestamp: f64) {
        self.active = true;
        self.started_seconds = timestamp.max(0.0);
        self.last_timestamp = self.started_seconds;
        self.points.clear();
        if position.is_finite() {
            self.points.push(position.clamp(Vec2::ZERO, Vec2::ONE));
        }
    }

    fn observe(&mut self, position: Vec2, timestamp: f64) -> Option<ActionSignature> {
        if !self.active || !position.is_finite() || !timestamp.is_finite() {
            return None;
        }
        if timestamp > self.last_timestamp && self.points.len() < 720 {
            let point = position.clamp(Vec2::ZERO, Vec2::ONE);
            if self
                .points
                .last()
                .is_none_or(|previous| previous.distance(point) >= 0.000_5)
            {
                self.points.push(point);
            }
            self.last_timestamp = timestamp;
        }
        if timestamp - self.started_seconds >= 6.0 {
            return self.finish(timestamp);
        }
        None
    }

    fn finish(&mut self, timestamp: f64) -> Option<ActionSignature> {
        if !self.active {
            return None;
        }
        self.active = false;
        let duration = (timestamp - self.started_seconds).clamp(0.05, 6.0) as f32;
        ActionSignature::from_trace(&self.points, duration)
    }
}

#[derive(Debug, Clone, Default)]
struct GazeCausalTrace {
    lifecore_target: Option<Vec2>,
    vita_target: Option<Vec2>,
    ecology_target: Option<Vec2>,
    source: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LabCommandStatus {
    AwaitingCommand,
    Applied,
    NoChange,
    Expired,
    PredatesProcess,
    Invalid,
    LoadError,
}

impl LabCommandStatus {
    const fn as_str(self) -> &'static str {
        match self {
            Self::AwaitingCommand => "awaiting_command",
            Self::Applied => "applied",
            Self::NoChange => "no_change",
            Self::Expired => "expired",
            Self::PredatesProcess => "predates_process",
            Self::Invalid => "invalid",
            Self::LoadError => "load_error",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LabCommandAdmission {
    Execute,
    Stale,
    Expired,
    PredatesProcess,
    Invalid,
}

#[derive(Debug, Clone)]
struct ActiveDrivePulse {
    command_id: u64,
    drive: LabDrive,
    delta: f32,
    expires_at: Instant,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct AppliedDriveOverlay {
    natural: Drives,
    effective: Drives,
}

impl AppliedDriveOverlay {
    fn apply_to(self, drives: &mut Drives) {
        *drives = self.effective;
    }

    fn restore_exact(self, drives: &mut Drives) {
        *drives = self.natural;
    }
}

#[derive(Debug)]
struct LabInterventionState {
    process_started_unix_ms: u64,
    last_seen_command_id: Option<u64>,
    last_command_kind: Option<&'static str>,
    last_command_status: LabCommandStatus,
    active_pulses: Vec<ActiveDrivePulse>,
    last_natural_drives: Option<Drives>,
    last_effective_drives: Option<Drives>,
}

impl Default for LabInterventionState {
    fn default() -> Self {
        Self::new(0)
    }
}

impl LabInterventionState {
    fn new(process_started_unix_ms: u64) -> Self {
        Self {
            process_started_unix_ms,
            last_seen_command_id: None,
            last_command_kind: None,
            last_command_status: LabCommandStatus::AwaitingCommand,
            active_pulses: Vec::new(),
            last_natural_drives: None,
            last_effective_drives: None,
        }
    }

    fn admit_command(
        &mut self,
        envelope: &LabControlEnvelope,
        now_unix_ms: u64,
    ) -> LabCommandAdmission {
        if envelope.validate().is_err() {
            self.last_command_status = LabCommandStatus::Invalid;
            return LabCommandAdmission::Invalid;
        }
        if self
            .last_seen_command_id
            .is_some_and(|last_seen| envelope.command_id <= last_seen)
        {
            return LabCommandAdmission::Stale;
        }

        // Advancing the monotonic watermark before checking expiry prevents a
        // newer-but-expired slot from being retried, or from allowing an older
        // command to replay after it.
        self.last_seen_command_id = Some(envelope.command_id);
        self.last_command_kind = Some(lab_command_name(&envelope.command));
        if envelope.issued_unix_ms < self.process_started_unix_ms {
            self.last_command_status = LabCommandStatus::PredatesProcess;
            return LabCommandAdmission::PredatesProcess;
        }
        if envelope.is_expired(now_unix_ms) {
            self.last_command_status = LabCommandStatus::Expired;
            LabCommandAdmission::Expired
        } else {
            LabCommandAdmission::Execute
        }
    }

    fn note_applied(&mut self, changed: bool) {
        self.last_command_status = if changed {
            LabCommandStatus::Applied
        } else {
            LabCommandStatus::NoChange
        };
    }

    fn note_load_error(&mut self) {
        self.last_command_status = LabCommandStatus::LoadError;
    }

    fn replace_drive_pulse(
        &mut self,
        command_id: u64,
        drive: LabDrive,
        delta: f32,
        duration_seconds: f32,
        now: Instant,
    ) {
        self.expire_pulses(now);
        self.active_pulses.retain(|pulse| pulse.drive != drive);
        let safe_seconds = if duration_seconds.is_finite() {
            duration_seconds.clamp(0.1, 30.0)
        } else {
            0.0
        };
        let duration = Duration::from_secs_f32(safe_seconds);
        let expires_at = now.checked_add(duration).unwrap_or(now);
        self.active_pulses.push(ActiveDrivePulse {
            command_id,
            drive,
            delta: delta.clamp(-1.0, 1.0),
            expires_at,
        });
    }

    fn expire_pulses(&mut self, now: Instant) {
        self.active_pulses.retain(|pulse| pulse.expires_at > now);
    }

    fn drive_overlay(&mut self, natural: Drives, now: Instant) -> AppliedDriveOverlay {
        self.expire_pulses(now);
        let mut effective = natural;
        for pulse in &self.active_pulses {
            add_drive_delta(&mut effective, pulse.drive, pulse.delta);
        }
        self.last_natural_drives = Some(natural);
        self.last_effective_drives = Some(effective);
        AppliedDriveOverlay { natural, effective }
    }
}

struct PetRuntime {
    window: Arc<Window>,
    renderer: Renderer,
    ecology_renderer: EcologyRenderer,
    platform: Box<dyn PlatformBackend>,
    topology: DisplayTopology,
    normalizer: SensorNormalizer,
    life: LifeCore,
    vita: VitaRuntime,
    morph: MorphBrain,
    ecology: EcologyRuntime,
    brain_mode: BrainMode,
    dev_mode: bool,
    body: ProceduralBody,
    audio: AudioManager,
    sensors: SensorFrame,
    intent: BodyIntent,
    pointer: PointerState,
    pointer_tracker: DesktopPointerTracker,
    cursor_hittest_latch: CursorHitTestLatch,
    acknowledged_window_origin: PhysicalPosition<i32>,
    window_origin_changes: u64,
    screen_body_center: Vec2,
    screen_velocity_px: Vec2,
    screen_collision_half_extent_px: Vec2,
    screen_edge_contact: bool,
    last_update: Instant,
    last_present: Instant,
    next_frame_deadline: Instant,
    body_accumulator: f32,
    dropped_body_time_seconds: f32,
    maximum_body_steps_per_frame: u32,
    maximum_screen_tick_step_px: f32,
    screen_tick_jump_events: u64,
    presented_pose: PresentedPoseMonitor,
    presentation_cadence: PresentationCadence,
    skipped_render_frames: u64,
    life_accumulator: f32,
    sensor_accumulator: f32,
    visual_poll_accumulator: f32,
    last_visual_sample: Option<DesktopVisualFrame>,
    save_accumulator: f32,
    tuning_poll_accumulator: f32,
    lab_control_poll_accumulator: f32,
    lab_interventions: LabInterventionState,
    tuning_last_modified: Option<SystemTime>,
    was_sleeping: bool,
    visible_after_first_frame: bool,
    modifiers: ModifiersState,
    debug_logging: bool,
    debug_interval: f32,
    debug_accumulator: f32,
    telemetry_sequence: u64,
    fps_accumulator: f32,
    frames_since_fps: u32,
    fps: f32,
    max_frame_gap_ms: f32,
    desktop_poll_microseconds: f64,
    background_capture_microseconds: f64,
    background_capture_timestamp: f64,
    background_capture_sequence: u64,
    background_luminance: f32,
    background_contrast: f32,
    expression_director: ExpressionDirector,
    nervous_system: NervousSystemRuntime,
    vocal_arbiter: VocalArbiter,
    last_phrase: Option<lifecore::CreaturePhrase>,
    overlay_move_microseconds: f64,
    render_microseconds: f64,
    brain_tick_microseconds: f64,
    last_debug: Option<DebugState>,
    last_vita: Option<VitaOutput>,
    gaze_trace: GazeCausalTrace,
    last_morph: MorphOutput,
    teach: TeachRecorder,
    physics_timings: TimingWindow,
    render_timings: TimingWindow,
    frame_gap_timings: TimingWindow,
    desktop_poll_timings: TimingWindow,
    camera_timings: TimingWindow,
    background_timings: TimingWindow,
    pending_gesture_convention: Option<PendingGestureConvention>,
    lab_pointer_fixture: Option<LabPointerFixture>,
    last_convention_update_episode: u64,
    shutdown_for_promotion: bool,
    runtime_ack_accumulator: f32,
    loaded_life_state_hash: u64,
    loaded_genome_hash: u64,
}

#[derive(Clone)]
struct PendingGestureConvention {
    episode_id: u64,
    meaning: GestureConventionMeaning,
    signature: GestureSignature,
    existing_id: Option<u64>,
    observed_at: f64,
}

#[derive(Clone, Copy)]
struct LabPointerFixture {
    gesture: LabGesture,
    intensity: f32,
    duration_seconds: f32,
    elapsed_seconds: f32,
    previous_local: Vec2,
    previous_down: bool,
}

#[derive(Clone, Copy)]
struct LabPointerSample {
    local: Vec2,
    velocity_local: Vec2,
    down: bool,
    pressed: bool,
    released: bool,
    finished: bool,
}

impl LabPointerFixture {
    fn new(gesture: LabGesture, intensity: f32, duration_seconds: f32) -> Self {
        Self {
            gesture,
            intensity: intensity.clamp(0.0, 1.0),
            duration_seconds: duration_seconds.clamp(0.1, 10.0),
            elapsed_seconds: 0.0,
            previous_local: Vec2::ZERO,
            previous_down: false,
        }
    }

    fn step(&mut self, dt: f32) -> LabPointerSample {
        let dt = dt.clamp(1.0 / 240.0, 0.05);
        let phase = (self.elapsed_seconds / self.duration_seconds).clamp(0.0, 1.0);
        let strength = 0.35 + self.intensity * 0.65;
        let (local, mut down) = match self.gesture {
            LabGesture::SoftTouch => (Vec2::new(-0.07, 0.01), phase < 0.72),
            LabGesture::SlowStretch => (
                Vec2::new(-0.06 + phase.min(0.82) * 0.30 * strength, 0.01),
                phase < 0.84,
            ),
            LabGesture::Tickle => (
                Vec2::new(
                    -0.03 + (phase * std::f32::consts::TAU * 9.0).sin() * 0.055 * strength,
                    (phase * std::f32::consts::TAU * 13.0).sin() * 0.035 * strength,
                ),
                phase < 0.90,
            ),
            LabGesture::ThreeBeatRhythm => {
                let beat_phase = (phase * 3.0).fract();
                (Vec2::new(-0.04, 0.015), phase < 0.92 && beat_phase < 0.46)
            }
            LabGesture::CircularTwist => {
                let angle = phase * std::f32::consts::TAU * 1.15;
                (
                    Vec2::new(angle.cos(), angle.sin()) * (0.10 + 0.05 * strength),
                    phase < 0.90,
                )
            }
            LabGesture::SharpFlick => (
                Vec2::new(-0.10 + phase.min(0.58) / 0.58 * 0.34 * strength, -0.01),
                phase < 0.58,
            ),
            LabGesture::Hold => (Vec2::new(-0.045, 0.02), phase < 0.92),
            LabGesture::PullRelease => (
                Vec2::new(-0.05 + phase.min(0.72) / 0.72 * 0.35 * strength, 0.0),
                phase < 0.72,
            ),
            LabGesture::RealSplitRemerge => {
                let distance = phase.min(0.78) / 0.78 * 0.58 * strength;
                (Vec2::new(0.24 + distance, 0.03), phase < 0.80)
            }
            LabGesture::FragmentHelp => {
                if phase < 0.55 {
                    (
                        Vec2::new(0.24 + phase / 0.55 * 0.58 * strength, 0.025),
                        true,
                    )
                } else if phase < 0.65 {
                    (Vec2::new(0.82, 0.025), false)
                } else {
                    let help = ((phase - 0.65) / 0.30).clamp(0.0, 1.0);
                    (Vec2::new(0.82 - help * 0.70, 0.025), phase < 0.95)
                }
            }
            LabGesture::OverstrainBoundary => (
                Vec2::new(-0.04 + phase.min(0.82) / 0.82 * 0.56 * strength, 0.0),
                phase < 0.84,
            ),
            LabGesture::SleepQuietInteraction => (Vec2::new(-0.03, 0.015), phase < 0.38),
        };
        self.elapsed_seconds = (self.elapsed_seconds + dt).min(self.duration_seconds);
        let finished = self.elapsed_seconds >= self.duration_seconds;
        if finished {
            down = false;
        }
        let velocity_local = (local - self.previous_local) / dt;
        let sample = LabPointerSample {
            local,
            velocity_local,
            down,
            pressed: down && !self.previous_down,
            released: !down && self.previous_down,
            finished,
        };
        self.previous_local = local;
        self.previous_down = down;
        sample
    }
}

fn apply_lab_pointer_fixture(runtime: &mut PetRuntime, sensors: &mut SensorFrame, dt: f32) {
    let body_position = runtime.body.simulation.feedback.world_position;
    let scale = runtime.body.embodiment.world_to_body_scale();
    let safe_scale = Vec2::new(
        if scale.x.abs() > 1.0e-5 { scale.x } else { 1.0 },
        if scale.y.abs() > 1.0e-5 {
            scale.y
        } else {
            -1.0
        },
    );
    let Some(fixture) = runtime.lab_pointer_fixture.as_mut() else {
        return;
    };
    let gesture = fixture.gesture;
    let fixture_phase = (fixture.elapsed_seconds / fixture.duration_seconds).clamp(0.0, 1.0);
    let sample = fixture.step(dt);
    sensors.cursor_position = body_position + sample.local / safe_scale;
    sensors.cursor_velocity = sample.velocity_local / safe_scale;
    sensors.cursor_acceleration = Vec2::ZERO;
    sensors.cursor_distance_to_pet = sensors.cursor_position.distance(body_position);
    sensors.pointer_down = sample.down;
    sensors.pointer_pressed = sample.pressed;
    sensors.pointer_released = sample.released;
    sensors.pet_hovered = true;
    sensors.pet_dragged = sample.down;
    sensors.pet_touched = sample.pressed;
    if sample.down
        && (gesture == LabGesture::RealSplitRemerge
            || (gesture == LabGesture::FragmentHelp && fixture_phase < 0.55))
    {
        sensors.interaction_actuation.allow_intentional_bud = true;
        sensors.interaction_actuation.compliance_delta = 0.25;
        sensors.interaction_actuation.cohesion_delta = -0.25;
        sensors.interaction_actuation.cooperation = 1.0;
        sensors.interaction_actuation.resistance = 0.0;
    }
    if sample.finished {
        runtime.lab_pointer_fixture = None;
    }
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
        if now < runtime.next_frame_deadline {
            event_loop.set_control_flow(ControlFlow::WaitUntil(runtime.next_frame_deadline));
            return;
        }
        let elapsed = (now - runtime.last_update).as_secs_f32().min(0.25);
        runtime.last_update = now;
        runtime.body_accumulator += elapsed;
        runtime.life_accumulator += elapsed;
        runtime.sensor_accumulator += elapsed;
        runtime.visual_poll_accumulator += elapsed;
        runtime.save_accumulator += elapsed;
        runtime.tuning_poll_accumulator += elapsed;
        runtime.lab_control_poll_accumulator += elapsed;
        runtime.runtime_ack_accumulator += elapsed;
        runtime.debug_accumulator += elapsed;
        runtime.fps_accumulator += elapsed;
        runtime.max_frame_gap_ms = runtime.max_frame_gap_ms.max(elapsed * 1_000.0);
        runtime.frame_gap_timings.observe(elapsed * 1_000.0);

        if runtime.lab_control_poll_accumulator >= LAB_CONTROL_POLL_INTERVAL_SECONDS {
            runtime.lab_control_poll_accumulator %= LAB_CONTROL_POLL_INTERVAL_SECONDS;
            poll_lab_control(&self.store, runtime, SystemTime::now(), now);
        }
        if runtime.shutdown_for_promotion {
            runtime.platform.shutdown();
            runtime.ecology.prepare_shutdown();
            if let Some(runtime) = self.runtime.as_ref() {
                self.persist(runtime);
                write_runtime_ack(&self.store, runtime, RuntimeLoadStatus::Stopped);
            }
            event_loop.exit();
            return;
        }
        if runtime.runtime_ack_accumulator >= 1.0 {
            runtime.runtime_ack_accumulator %= 1.0;
            write_runtime_ack(&self.store, runtime, RuntimeLoadStatus::Running);
        }
        runtime.lab_interventions.expire_pulses(now);

        if runtime.tuning_poll_accumulator >= 0.25 {
            runtime.tuning_poll_accumulator %= 0.25;
            let modified = std::fs::metadata(&self.store.paths.liquid_tuning)
                .and_then(|metadata| metadata.modified())
                .ok();
            if modified.is_some() && modified != runtime.tuning_last_modified {
                match load_migrate_apply_liquid_tuning(&self.store, &mut runtime.body) {
                    Ok(Some(_)) => {
                        runtime.tuning_last_modified =
                            std::fs::metadata(&self.store.paths.liquid_tuning)
                                .and_then(|metadata| metadata.modified())
                                .ok();
                    }
                    Ok(None) => eprintln!("liquid tuning file disappeared during reload"),
                    Err(error) => eprintln!("could not reload liquid tuning profile: {error}"),
                }
            }
        }
        if runtime.fps_accumulator >= 1.0 {
            runtime.fps = runtime.frames_since_fps as f32 / runtime.fps_accumulator;
            runtime.frames_since_fps = 0;
            runtime.fps_accumulator = 0.0;
        }

        if runtime.sensor_accumulator >= SENSOR_DT {
            // Poll latest-state input once, but advance perception by the real
            // elapsed observation interval. A 60/75 Hz compositor or short stall
            // must not make Vita run at half real time merely because there was no
            // second event-loop callback available for a synthetic 120 Hz sample.
            let observation_dt = runtime.sensor_accumulator.min(0.05);
            runtime.sensor_accumulator = 0.0;
            let desktop_poll_started = Instant::now();
            let snapshot = runtime.platform.poll_desktop(&runtime.topology);
            runtime.desktop_poll_microseconds =
                desktop_poll_started.elapsed().as_secs_f64() * 1_000_000.0;
            runtime
                .desktop_poll_timings
                .observe(runtime.desktop_poll_microseconds as f32 / 1_000.0);
            let desktop_size = Vec2::new(
                runtime.topology.virtual_physical_bounds.width().max(1) as f32,
                runtime.topology.virtual_physical_bounds.height().max(1) as f32,
            );
            let predictive_margin = predictive_cursor_margin_pixels(
                runtime.sensors.cursor_velocity,
                desktop_size,
                runtime.screen_velocity_px.length(),
            );
            let desktop_aspect = desktop_size.x / desktop_size.y.max(1.0);
            let cursor_normalized = snapshot.cursor.map(|cursor| {
                let bounds = runtime.topology.virtual_physical_bounds;
                Vec2::new(
                    (cursor.x - bounds.minimum.x) as f32 / bounds.width().max(1) as f32,
                    (cursor.y - bounds.minimum.y) as f32 / bounds.height().max(1) as f32,
                )
                .clamp(Vec2::ZERO, Vec2::ONE)
            });
            let ecology_hover_predictive = cursor_normalized.is_some_and(|cursor| {
                runtime
                    .ecology
                    .hit_test(cursor, desktop_aspect, desktop_size.y, predictive_margin)
            });
            let ecology_hover_hysteresis = cursor_normalized.is_some_and(|cursor| {
                runtime.ecology.hit_test(
                    cursor,
                    desktop_aspect,
                    desktop_size.y,
                    (predictive_margin + 12.0).min(72.0),
                )
            });
            let ecology_hover_precise = cursor_normalized.is_some_and(|cursor| {
                runtime
                    .ecology
                    .hit_test(cursor, desktop_aspect, desktop_size.y, 4.0)
            });
            let hovered_predictive = snapshot.cursor.is_some_and(|cursor| {
                hit_test_desktop_cursor(
                    &runtime.window,
                    runtime.acknowledged_window_origin,
                    &runtime.body,
                    cursor,
                    predictive_margin,
                )
            });
            let hovered_hysteresis = snapshot.cursor.is_some_and(|cursor| {
                hit_test_desktop_cursor(
                    &runtime.window,
                    runtime.acknowledged_window_origin,
                    &runtime.body,
                    cursor,
                    (predictive_margin + 12.0).min(72.0),
                )
            });
            let hovered_liquid = snapshot.cursor.is_some_and(|cursor| {
                hit_test_desktop_cursor(
                    &runtime.window,
                    runtime.acknowledged_window_origin,
                    &runtime.body,
                    cursor,
                    4.0,
                )
            });
            let primary_down = snapshot.primary_button_down.unwrap_or(runtime.pointer.down);
            let pet_capture_active = runtime.pointer_tracker.captured;
            let petting_started = update_pointer_state(
                &mut runtime.pointer,
                &mut runtime.pointer_tracker,
                primary_down,
                hovered_liquid && (!ecology_hover_precise || pet_capture_active),
            );
            let pet_dragged = runtime.pointer.pet_dragged;
            let orb_touched = runtime.ecology.observe_pointer(
                cursor_normalized,
                primary_down,
                !pet_dragged,
                desktop_aspect,
                desktop_size.y,
                runtime.normalizer.monotonic_seconds(),
            );
            let accepts_cursor = runtime.cursor_hittest_latch.resolve(
                hovered_predictive || ecology_hover_predictive,
                hovered_hysteresis || ecology_hover_hysteresis,
                pet_dragged || runtime.ecology.is_dragging_object(),
            );
            let _ = runtime
                .platform
                .set_cursor_hittest(&runtime.window, accepts_cursor);
            runtime.sensors = runtime.normalizer.normalize(
                &snapshot,
                &runtime.topology,
                runtime.body.simulation.feedback.world_position,
                runtime.pointer,
                local_time_01(),
            );
            runtime.sensors.embodied_interaction = runtime.body.embodied_interaction_frame();
            runtime.body.simulation.feedback.cursor_contact =
                runtime.sensors.embodied_interaction.contact.active;
            if petting_started {
                // Resolve any previous utterance before the mind chooses this
                // touch response. The freshly normalized sensor frame gives the
                // learner the actual contact context instead of the prior poll.
                apply_shared_feedback(runtime, FeedbackEvent::PettingStarted);
            }
            if orb_touched {
                apply_shared_feedback(runtime, FeedbackEvent::PlayStarted);
                runtime.save_accumulator = 30.0;
            }
            if runtime.visual_poll_accumulator >= VISUAL_SAMPLE_INTERVAL {
                runtime.visual_poll_accumulator %= VISUAL_SAMPLE_INTERVAL;
                runtime.last_visual_sample = runtime.platform.poll_visual_features(
                    &runtime.topology,
                    runtime.body.simulation.feedback.world_position,
                );
            }
            if let Some(sample) = runtime.last_visual_sample {
                runtime.sensors.mean_luminance = Some(sample.summary.mean_luminance);
                runtime.sensors.local_luminance = Some(sample.summary.local_luminance);
                runtime.background_luminance = sample.summary.mean_luminance;
                runtime.background_contrast = sample.summary.contrast;
                let (summary, spatial) = perception_visual_frames(sample);
                runtime.vita.set_visual_features(summary, spatial);
            } else {
                runtime.sensors.mean_luminance = Some(0.5);
                runtime.sensors.local_luminance = Some(0.5);
            }
            runtime.vita.set_embodied_gesture_tuning(classifier_tuning(
                runtime.body.tuning_profile().interaction,
            ));
            runtime.vita.observe(
                &runtime.sensors,
                &runtime.body.simulation.feedback,
                observation_dt,
            );
            expire_pending_gesture_convention(runtime);
            let click_rhythm = runtime.vita.recent_click_rhythm();
            runtime.sensors.recent_click_rhythm =
                click_rhythm.map_or([0.0; 8], |rhythm| rhythm.intervals);
            runtime.ecology.set_click_rhythm(click_rhythm);
            let visual_target = runtime.vita.visual_attention_target();
            let visual_cell = visual_target.and_then(|target| {
                runtime
                    .last_visual_sample
                    .map(|frame| frame.cell_at(target.position))
            });
            let visual_hue = visual_cell.map_or(0.0, |cell| cell.hue);
            runtime.ecology.set_visual_attention(VisualAttentionSample {
                target: visual_target.map(|target| target.position),
                hue: visual_hue,
                strength: visual_target.map_or(0.0, |target| target.score),
                explicit: visual_target.is_some_and(|target| target.explicit),
                colorfulness: visual_cell.map_or(0.0, |cell| cell.colorfulness),
                structure: visual_cell.map_or(0.0, |cell| cell.edge_density),
                surprise: visual_cell.map_or(0.0, |cell| cell.sudden_change.max(cell.motion)),
            });
            if let Some(signature) = runtime
                .teach
                .observe(runtime.sensors.cursor_position, runtime.sensors.timestamp)
                && runtime
                    .ecology
                    .learn_signature(signature, runtime.sensors.timestamp.max(0.0))
                    .is_ok()
            {
                runtime.save_accumulator = 30.0;
            }
            runtime.pointer.pressed = false;
            runtime.pointer.released = false;
            runtime.pointer.pet_touched = false;
        }

        configure_visual_motion_space(
            &mut runtime.body,
            &runtime.topology,
            runtime.window.inner_size(),
        );

        let body_dt = 1.0
            / runtime
                .body
                .tuning_profile()
                .pbf
                .fixed_hz
                .clamp(30.0, 120.0);
        let (capped_backlog, dropped_backlog) =
            cap_body_physics_backlog(runtime.body_accumulator, body_dt);
        runtime.body_accumulator = capped_backlog;
        runtime.dropped_body_time_seconds += dropped_backlog;
        let physics_started = Instant::now();
        let mut physics_steps = 0_u32;
        while runtime.body_accumulator >= body_dt {
            let visual_mind = visual_mind_input(
                &runtime.life,
                &runtime.sensors,
                runtime.last_vita.as_ref(),
                runtime.vita.state().self_model.uncertainty,
                runtime.vita.percept().scroll_velocity,
                runtime.vita.percept().window_pressure,
            );
            let mut body_sensors = runtime.sensors.clone();
            apply_lab_pointer_fixture(runtime, &mut body_sensors, body_dt);
            runtime.body.fixed_update(
                &runtime.life.state.genome,
                &runtime.intent,
                &body_sensors,
                body_dt,
            );
            let body_voice = body_voice_frame(&runtime.body, &body_sensors);
            runtime.audio.publish_body_voice(body_voice, body_dt);
            let bounds = runtime.topology.virtual_physical_bounds;
            let desktop_aspect = bounds.width() as f32 / bounds.height().max(1) as f32;
            runtime.ecology.fixed_update(
                desktop_aspect,
                runtime.vita.window_affordances(),
                &runtime.body.simulation.feedback,
                body_dt,
            );
            runtime
                .body
                .set_embodied_environment(runtime.ecology.environment());
            let mut ecology_effect = runtime.ecology.visual_effect();
            if runtime.teach.active {
                ecology_effect.hue = 0.78;
                ecology_effect.color_blend = ecology_effect.color_blend.max(0.08);
                ecology_effect.glow_boost = ecology_effect.glow_boost.max(0.24);
            }
            runtime.body.set_ecology_visual_effect(ecology_effect);
            let previous_screen_center = runtime.screen_body_center;
            apply_screen_domain(
                &mut runtime.body,
                &runtime.topology,
                runtime.window.inner_size(),
                &mut runtime.screen_body_center,
                &mut runtime.screen_velocity_px,
                &mut runtime.screen_collision_half_extent_px,
                &mut runtime.screen_edge_contact,
                body_dt,
            );
            let screen_tick_step = runtime.screen_body_center.distance(previous_screen_center);
            runtime.maximum_screen_tick_step_px =
                runtime.maximum_screen_tick_step_px.max(screen_tick_step);
            let jump_threshold = 12.0 * (body_dt * 120.0).clamp(1.0, 4.0);
            if screen_tick_step > jump_threshold {
                runtime.screen_tick_jump_events = runtime.screen_tick_jump_events.saturating_add(1);
            }
            let voice = voice_visual_state(&runtime.audio);
            runtime.body.embodied_update(
                &runtime.intent,
                &runtime.sensors,
                runtime.life.state.affect,
                visual_mind,
                voice,
                body_dt,
            );
            runtime
                .nervous_system
                .observe_body(&runtime.body, &runtime.intent, &body_sensors);
            runtime.body_accumulator -= body_dt;
            physics_steps += 1;
        }
        runtime.maximum_body_steps_per_frame =
            runtime.maximum_body_steps_per_frame.max(physics_steps);
        if physics_steps > 0 {
            runtime
                .physics_timings
                .observe(physics_started.elapsed().as_secs_f32() * 1_000.0);
        }
        let overlay_move_started = Instant::now();
        update_desktop_presentation(
            &runtime.window,
            &runtime.topology,
            runtime.body.simulation.feedback.world_position,
            &mut runtime.body,
            runtime.acknowledged_window_origin,
        );
        runtime.overlay_move_microseconds =
            overlay_move_started.elapsed().as_secs_f64() * 1_000_000.0;
        runtime
            .camera_timings
            .observe(runtime.overlay_move_microseconds as f32 / 1_000.0);
        // The checker is evaluated analytically in the material shader. Its UVs
        // still use global desktop coordinates, so moving either the Pet or its
        // camera cannot make the pattern slide across the liquid.
        let background_started = Instant::now();
        let size = runtime.window.inner_size();
        let (scale, offset) = background_uv_transform(
            runtime.acknowledged_window_origin,
            size,
            runtime.topology.virtual_physical_bounds,
        );
        runtime.renderer.set_background_uv_transform(scale, offset);
        runtime.renderer.set_background_freshness(0.0);
        runtime.background_capture_microseconds =
            background_started.elapsed().as_secs_f64() * 1_000_000.0;
        runtime
            .background_timings
            .observe(runtime.background_capture_microseconds as f32 / 1_000.0);
        synchronize_audio_learning(runtime);
        runtime.nervous_system.observe_voice(
            runtime.audio.visual_feedback(),
            runtime.audio.callback_levels(),
            &runtime.life.state.genome.voice,
        );
        runtime.nervous_system.set_selected_salience(
            runtime
                .vita
                .visual_attention_target()
                .map_or(0.0, |target| target.score),
        );
        while runtime.life_accumulator >= LIFE_DT {
            let tick_started = Instant::now();
            let soft_touch_pressure_max = runtime
                .body
                .tuning_profile()
                .interaction
                .soft_touch_pressure_max;
            runtime.nervous_system.prepare_cognition_tick(
                &mut runtime.life,
                &mut runtime.vita,
                &mut runtime.morph,
                soft_touch_pressure_max,
                LIFE_DT,
            );
            // Lab drive pulses are an observational experiment layer, never a
            // second homeostasis owner. Morph sees a reversible overlay on the
            // pre-LifeCore state, then LifeCore advances from its exact natural
            // drives as it always has.
            let morph_overlay = runtime
                .lab_interventions
                .drive_overlay(runtime.life.state.drives, tick_started);
            morph_overlay.apply_to(&mut runtime.life.state.drives);
            let morph_world = runtime.ecology.morph_world_input(
                runtime.body.simulation.feedback.world_position,
                runtime.life.state.drives,
            );
            let morph_output = runtime.morph.tick_with_world(
                &runtime.sensors,
                &runtime.body.simulation.feedback,
                &runtime.life.state,
                &morph_world,
                LIFE_DT,
            );
            morph_overlay.restore_exact(&mut runtime.life.state.drives);
            let mut output =
                runtime
                    .life
                    .tick(&runtime.sensors, &runtime.body.simulation.feedback, LIFE_DT);
            runtime.expression_director.tick(LIFE_DT);
            if let Some((mut event, signature)) = runtime.vita.take_embodied_gesture_observation() {
                let episode_id = event.classification.episode_id;
                if let Some(signature) = signature {
                    stage_gesture_convention(runtime, &mut event, signature);
                }
                runtime.nervous_system.observe_gesture(&event);
                let interaction_tuning = runtime.body.tuning_profile().interaction;
                if let Some(plan) = runtime.life.observe_embodied_gesture_with_tuning(
                    event,
                    interaction_tuning.response_amplitude,
                    interaction_tuning.turn_wait_seconds,
                    interaction_tuning.turn_cooldown_seconds,
                    interaction_tuning.learning_openness,
                ) {
                    let plan = coordinate_interaction_response(runtime, plan);
                    if runtime.vita.accept_interaction_response(plan) {
                        if output.vocal_request.is_none() {
                            output.vocal_request = plan.voice_trigger.and_then(|trigger| {
                                runtime.life.request_vocalization(trigger, &runtime.sensors)
                            });
                        }
                    } else {
                        runtime
                            .vita
                            .finish_interaction_appraisal_without_response(episode_id);
                    }
                } else {
                    runtime
                        .vita
                        .finish_interaction_appraisal_without_response(episode_id);
                }
            }
            let lifecore_gaze = output.body_intent.gaze_target;
            // Resolution gets a second overlay based on the newly advanced
            // natural drives. Restoring by assignment (rather than subtracting
            // deltas) is exact even when an effective value was clamped.
            let resolution_overlay = runtime
                .lab_interventions
                .drive_overlay(runtime.life.state.drives, tick_started);
            resolution_overlay.apply_to(&mut runtime.life.state.drives);
            let (resolved_intent, vita_output) = runtime.vita.resolve_intent_with_morph(
                runtime.brain_mode,
                &runtime.life.state,
                &runtime.sensors,
                &runtime.body.simulation.feedback,
                output.body_intent,
                Some(morph_output),
                LIFE_DT,
            );
            let vita_interaction = runtime.vita.interaction_actuation();
            let vita_gaze = resolved_intent.gaze_target;
            output.body_intent = resolved_intent;
            let ecology_output = runtime.ecology.resolve_intent(
                output.body_intent,
                EcologyResolveFrame {
                    selected_action: output.selected_action,
                    drives: runtime.life.state.drives,
                    sensors: &runtime.sensors,
                    body: &runtime.body.simulation.feedback,
                    focus_mode: runtime.life.state.focus_mode,
                    dt: LIFE_DT,
                },
            );
            resolution_overlay.restore_exact(&mut runtime.life.state.drives);
            let ecology_gaze = ecology_output.body_intent.gaze_target;
            let gaze_source = if let Some(goal) = ecology_output.debug.active_goal {
                format!("ecology:{goal:?}")
            } else if vita_gaze != lifecore_gaze
                || vita_output
                    .as_ref()
                    .is_some_and(|vita| vita.direct_viewer_gaze)
            {
                format!(
                    "vita:{}",
                    vita_output.as_ref().map_or_else(
                        || "filtered".to_owned(),
                        |vita| format!("{:?}", vita.attention.kind)
                    )
                )
            } else {
                format!("lifecore:{:?}", output.selected_action)
            };
            runtime.gaze_trace = GazeCausalTrace {
                lifecore_target: lifecore_gaze,
                vita_target: vita_gaze,
                ecology_target: ecology_gaze,
                source: gaze_source,
            };
            output.body_intent = ecology_output.body_intent;
            let ecology_vocal_trigger = ecology_output.vocal_trigger;
            preserve_navigation_during_material_drag(
                &runtime.intent,
                &mut output.body_intent,
                runtime.sensors.pet_dragged,
            );
            project_navigation_target_to_monitor_union(&mut output.body_intent, &runtime.topology);
            let phenotype = runtime.nervous_system.resolve_actuation(
                &runtime.life,
                &runtime.vita,
                &runtime.morph,
                vita_interaction,
                soft_touch_pressure_max,
                LIFE_DT,
            );
            phenotype.apply_to_intent(
                &mut output.body_intent,
                &runtime.sensors,
                runtime.body.simulation.feedback.world_position,
            );
            // The nervous action blend can move the semantic target after the
            // ecology projection, so enforce the monitor-union invariant once
            // more at the final intent boundary.
            project_navigation_target_to_monitor_union(&mut output.body_intent, &runtime.topology);
            runtime.sensors.interaction_actuation = phenotype.interaction;
            runtime.body.set_fast_phenotype_actuation(phenotype);
            runtime.brain_tick_microseconds = tick_started.elapsed().as_secs_f64() * 1_000_000.0;
            runtime.last_debug = Some(output.debug.clone());
            runtime.last_vita = vita_output;
            runtime.last_morph = morph_output;
            runtime.intent = output.body_intent;
            let sleeping = output.selected_action == ActionId::Sleep;
            if sleeping && !runtime.was_sleeping {
                runtime.life.consolidate_sleep();
                runtime.save_accumulator = 30.0;
            }
            runtime.was_sleeping = sleeping;
            if let Some(trigger) = ecology_vocal_trigger {
                if let Some(request) = output.vocal_request.take() {
                    runtime.life.cancel_vocal_request(request.performance_seed);
                }
                output.vocal_request = request_ecology_voice(runtime, trigger);
            }
            if let Some(request) = output.vocal_request {
                enqueue_vocal_candidate(runtime, request);
            }
            runtime.life_accumulator -= LIFE_DT;
        }
        if runtime.debug_accumulator >= runtime.debug_interval {
            runtime.debug_accumulator %= runtime.debug_interval;
            if runtime.debug_logging
                && let Some(debug) = &runtime.last_debug
            {
                runtime.telemetry_sequence = runtime.telemetry_sequence.saturating_add(1);
                let telemetry_sequence = runtime.telemetry_sequence;
                let liquid = runtime.body.embodiment.liquid.diagnostics();
                let physics_timing = runtime.physics_timings.percentiles();
                let render_timing = runtime.render_timings.percentiles();
                let frame_gap_timing = runtime.frame_gap_timings.percentiles();
                let desktop_timing = runtime.desktop_poll_timings.percentiles();
                let camera_timing = runtime.camera_timings.percentiles();
                let background_timing = runtime.background_timings.percentiles();
                let ecology_debug = runtime.ecology.debug();
                let ecology_scores = ecology_debug.scores[..ecology_debug.score_count]
                    .iter()
                    .map(|score| {
                        serde_json::json!({
                            "goal": score.goal.map(|goal| format!("{goal:?}")),
                            "score": score.score,
                            "eligible": score.eligible,
                            "reason": format!("{:?}", score.reason),
                        })
                    })
                    .collect::<Vec<_>>();
                let active_ecology = runtime.ecology.active_episode();
                let active_skill_id = active_ecology
                    .filter(|episode| {
                        matches!(
                            episode.goal,
                            EpisodeGoal::PracticeSkill | EpisodeGoal::PerformSkill
                        )
                    })
                    .and_then(|episode| episode.object_id);
                let active_skill = active_skill_id.and_then(|skill_id| {
                    runtime
                        .ecology
                        .state()
                        .skills
                        .skills
                        .iter()
                        .find(|skill| skill.id == skill_id)
                });
                let ecology_visual = runtime.ecology.visual_context();
                let ecology_visual_effect = runtime.ecology.visual_effect();
                let saliency_target = runtime.vita.visual_attention_target();
                let ecology_state = runtime.ecology.state();
                let orb = ecology_state
                    .objects
                    .iter()
                    .find(|object| object.kind == ObjectKind::Orb);
                let ecology_environment = runtime.ecology.environment();
                let (gaze_fixation_elapsed, gaze_fixation_duration) =
                    runtime.body.embodiment.fixation_timing();
                let ecology_contacts = ecology_environment.contacts[..ecology_environment
                    .contact_count
                    .min(ecology_environment.contacts.len())]
                    .iter()
                    .map(|contact| {
                        serde_json::json!({
                            "source": format!("{:?}", contact.source),
                            "point": contact.point_world.to_array(),
                            "normal": contact.normal_world.to_array(),
                            "penetration_px": contact.penetration_px,
                            "relative_velocity_px": contact.relative_velocity_px.to_array(),
                            "intensity": contact.intensity,
                        })
                    })
                    .collect::<Vec<_>>();
                let liquid_debug = serde_json::json!({
                    "particles": liquid.particle_count,
                    "components": liquid.component_count,
                    "main_mass": liquid.main_mass,
                    "detached_mass": liquid.detached_mass,
                    "density_error": liquid.density_error,
                    "maximum_speed": liquid.maximum_speed,
                    "maximum_bond_strain": liquid.maximum_bond_strain,
                    "visual_weber": liquid.visual_weber,
                    "visual_ohnesorge": liquid.visual_ohnesorge,
                    "visual_deborah": liquid.visual_deborah,
                    "finite": liquid.finite,
                });
                let life_details = lifecore_telemetry_json(&runtime.life, debug);
                let activity_details = activity_telemetry_json(
                    &runtime.life,
                    ecology_debug,
                    active_ecology,
                    &ecology_scores,
                );
                let vita_details = vita_telemetry_json(&runtime.vita, runtime.last_vita.as_ref());
                let morph_diagnostics = runtime.morph.diagnostics();
                let morph_details =
                    morph_telemetry_json(runtime.last_morph, runtime.brain_mode, morph_diagnostics);
                let lab_interventions =
                    lab_interventions_telemetry_json(&runtime.lab_interventions, Instant::now());
                let body_interaction = body_interaction_telemetry_json(runtime);
                let nervous_snapshot = runtime.nervous_system.snapshot();
                let nervous_body_feedback = runtime.nervous_system.body_feedback();
                let nervous_actuation = runtime.nervous_system.actuation();
                let mutation_history_count = runtime.life.state.development.mutation_history.len();
                let latest_mutation = runtime
                    .life
                    .state
                    .development
                    .mutation_history
                    .last()
                    .map(mutation_record_telemetry_json);
                let audio_visual = runtime.audio.visual_feedback();
                let audio_levels = runtime.audio.callback_levels();
                let body_feedback = &runtime.body.simulation.feedback;
                let action_definition = runtime.life.state.current_action.definition();
                let mut debug_details = serde_json::json!({
                    "telemetry_schema": "pet2.causal_telemetry",
                    "telemetry_schema_version": CAUSAL_TELEMETRY_SCHEMA_VERSION,
                    "schema_version": CAUSAL_TELEMETRY_SCHEMA_VERSION,
                    "sequence": telemetry_sequence,
                    "identity_seed": runtime.life.state.genome.identity_seed,
                    "lineage_id": format!("{:032x}", runtime.life.state.genome.lineage_id),
                    "generation": runtime.life.state.genome.generation,
                    "genome_hash": runtime.life.state.genome.stable_hash(),
                    "tick": runtime.life.state.tick_count,
                    "elapsed_seconds": runtime.life.state.elapsed_seconds,
                    "identity": {
                        "identity_seed": runtime.life.state.genome.identity_seed,
                        "identity_seed_hex": format!("{:016x}", runtime.life.state.genome.identity_seed),
                        "lineage_id": format!("{:032x}", runtime.life.state.genome.lineage_id),
                        "generation": runtime.life.state.genome.generation,
                        "genome_hash": runtime.life.state.genome.stable_hash(),
                        "genome": genome_telemetry_json(&runtime.life.state.genome),
                    },
                    "development": {
                        "stage": runtime.life.state.development.stage,
                        "metamorphosis_count": runtime.life.state.development.metamorphosis_count,
                        "lifetime": &runtime.life.state.development.lifetime,
                        // Full bounded lineage history already lives in the
                        // validated atomic state.json that Body Lab reads for
                        // evolution replay. Repeating ~266 KiB at 5 Hz would
                        // collapse the bounded telemetry timeline to seconds.
                        "mutation_history_count": mutation_history_count,
                        "latest_mutation": latest_mutation,
                    },
                    "lifecore": life_details,
                    "activity": activity_details,
                    "action": format!("{:?}", runtime.life.state.current_action),
                    "action_elapsed_seconds": runtime.life.state.action_elapsed_seconds,
                    "action_minimum_seconds": action_definition.minimum_duration,
                    "action_maximum_seconds": action_definition.maximum_duration,
                    "action_cooldowns": runtime.life.state.action_cooldowns,
                    "recent_actions": runtime.life.state.recent_actions.iter().copied().map(action_wire_name).collect::<Vec<_>>(),
                    "pending_attention": runtime.life.state.pending_attention.as_ref().map(|pending| serde_json::json!({
                        "action": action_wire_name(pending.action),
                        "elapsed_seconds": pending.elapsed_seconds,
                        "response_window_seconds": pending.response_window_seconds,
                    })),
                    "successful_interactions": runtime.life.state.successful_interactions,
                    "ignored_attempts": runtime.life.state.ignored_attempts,
                    "focus_mode": runtime.life.state.focus_mode,
                    "locomotion": format!("{:?}", runtime.intent.locomotion),
                    "body_render_mode": format!("{:?}", runtime.body.tuning_profile().render_mode),
                    "material_variant": format!("{:?}", runtime.body.tuning_profile().material.variant),
                    "liquid_profile_name": runtime.body.tuning_profile().name.as_str(),
                    "liquid_profile_revision": runtime.body.tuning_profile().profile_revision,
                    "world_position": runtime.body.simulation.feedback.world_position.to_array(),
                    "target_position": runtime.intent.target_position.to_array(),
                    "velocity": runtime.body.simulation.feedback.velocity.to_array(),
                    "screen_body_center_px": runtime.screen_body_center.to_array(),
                    "screen_velocity_px": runtime.screen_velocity_px.to_array(),
                    "screen_acceleration_px": (runtime.body.simulation.feedback.acceleration
                        * Vec2::new(
                            runtime.topology.virtual_physical_bounds.width().max(1) as f32,
                            runtime.topology.virtual_physical_bounds.height().max(1) as f32,
                        )).to_array(),
                    "screen_collision_half_extent_px": runtime.screen_collision_half_extent_px.to_array(),
                    "screen_edge_contact": runtime.screen_edge_contact,
                    "maximum_screen_tick_step_px": runtime.maximum_screen_tick_step_px,
                    "screen_tick_jump_events": runtime.screen_tick_jump_events,
                    "dropped_body_time_seconds": runtime.dropped_body_time_seconds,
                    "maximum_body_steps_per_frame": runtime.maximum_body_steps_per_frame,
                    "presented_frame_count": runtime.presented_pose.frame_count,
                    "maximum_presented_step_px": runtime.presented_pose.maximum_step_px,
                    "presented_large_step_events": runtime.presented_pose.large_step_events,
                    "presented_ping_pong_events": runtime.presented_pose.ping_pong_events,
                    "skipped_render_frames": runtime.skipped_render_frames,
                    "acknowledged_window_origin": [runtime.acknowledged_window_origin.x, runtime.acknowledged_window_origin.y],
                    "window_origin_changes": runtime.window_origin_changes,
                    "visual_motion_scale": runtime.body.embodiment.world_to_body_scale().to_array(),
                    "droplet_centroid": runtime.body.embodiment.droplets.centroid().to_array(),
                    "liquid_stretch_ratio": liquid.stretch_ratio,
                    "liquid_com_follow_ratio": liquid.com_follow_ratio,
                    "liquid_orientation": liquid.orientation,
                    "liquid_lean_target": liquid.lean_target,
                    "liquid": liquid_debug.clone(),
                    "body": {
                        "render_mode": format!("{:?}", runtime.body.tuning_profile().render_mode),
                        "material_variant": format!("{:?}", runtime.body.tuning_profile().material.variant),
                        "profile_revision": runtime.body.tuning_profile().profile_revision,
                        "world_position": body_feedback.world_position.to_array(),
                        "target_position": runtime.intent.target_position.to_array(),
                        "velocity": body_feedback.velocity.to_array(),
                        "acceleration": body_feedback.acceleration.to_array(),
                        "grounded": body_feedback.grounded,
                        "clinging": body_feedback.clinging,
                        "cursor_contact": body_feedback.cursor_contact,
                        "collision": body_feedback.collision.as_ref().map(|collision| serde_json::json!({
                            "normal": collision.normal.to_array(),
                            "intensity": collision.intensity,
                        })),
                        "pose_error": body_feedback.pose_error,
                        "locomotion_completed": body_feedback.locomotion_completed,
                        "locomotion": format!("{:?}", runtime.intent.locomotion),
                        "desired_speed": runtime.intent.desired_speed,
                        "pose": format!("{:?}", runtime.intent.pose),
                        "expression": runtime.intent.expression,
                        "liquid": liquid_debug,
                    },
                    "perception": {
                        // Deliberately aggregate-only: no key text, window IDs,
                        // surface IDs, absolute pixels, or captured screen data.
                        "timestamp": runtime.sensors.timestamp,
                        "day_phase": runtime.sensors.day_phase,
                        "time_of_day_01": runtime.sensors.time_of_day_01,
                        "cursor_distance_to_pet": runtime.sensors.cursor_distance_to_pet,
                        "cursor_approach_speed": runtime.sensors.cursor_approach_speed,
                        "cursor_speed": runtime.sensors.cursor_velocity.length(),
                        "pointer": {
                            "down": runtime.sensors.pointer_down,
                            "pressed": runtime.sensors.pointer_pressed,
                            "released": runtime.sensors.pointer_released,
                            "pet_hovered": runtime.sensors.pet_hovered,
                            "pet_touched": runtime.sensors.pet_touched,
                            "pet_dragged": runtime.sensors.pet_dragged,
                        },
                        "user_idle_seconds": runtime.sensors.user_idle_seconds,
                        "user_activity_rate": runtime.sensors.user_activity_rate,
                        "user_presence": runtime.sensors.user_presence,
                        "user_availability": runtime.sensors.user_availability,
                        "active_app_category": runtime.sensors.active_app_category,
                        "audio_rms": runtime.sensors.audio_rms,
                        "voice_activity": runtime.sensors.voice_activity,
                        "mean_luminance": runtime.sensors.mean_luminance,
                        "local_luminance": runtime.sensors.local_luminance,
                    },
                    "strongest_drive": format!("{:?}", debug.strongest_drive),
                    "strongest_drive_value": debug.strongest_drive_value,
                    "drives": runtime.life.state.drives,
                    "affect": runtime.life.state.affect,
                    "attachment": runtime.life.state.affect.attachment,
                    "attention_budget": debug.attention_budget,
                    "selected_motif": debug.selected_motif_id,
                    "predicted_action_value": debug.predicted_action_value,
                    "recent_reward": debug.recent_reward,
                    "plastic_weight_range": debug.plastic_weight_range,
                    "fps": runtime.fps,
                    "max_frame_gap_ms": runtime.max_frame_gap_ms,
                    "desktop_poll_microseconds": runtime.desktop_poll_microseconds,
                    "background_capture_microseconds": runtime.background_capture_microseconds,
                    "background_capture_sequence": runtime.background_capture_sequence,
                    "background_capture_age_ms": (runtime.normalizer.monotonic_seconds()
                        - runtime.background_capture_timestamp).max(0.0) * 1_000.0,
                    "background_luminance": runtime.background_luminance,
                    "background_contrast": runtime.background_contrast,
                    "overlay_move_microseconds": runtime.overlay_move_microseconds,
                    "render_microseconds": runtime.render_microseconds,
                    "brain_tick_microseconds": runtime.brain_tick_microseconds,
                    "brain_mode": runtime.brain_mode.as_str(),
                    "morph": morph_details,
                    "morph_diagnostics": morph_diagnostics,
                    "lab_interventions": lab_interventions,
                    "body_interaction": body_interaction,
                    "nervous_system": {
                        "body_feedback_v2": nervous_body_feedback,
                        "derived": nervous_snapshot.derived,
                        "felt_state_v1": nervous_snapshot.felt,
                        "emotional_readouts": nervous_snapshot.emotions,
                        "morph_somatic_input": nervous_snapshot.morph_sensors,
                        "source_frame_id": nervous_snapshot.source_frame_id,
                        "source_episode_id": nervous_snapshot.source_episode_id,
                        "fast_actuation": nervous_actuation,
                    },
                    "fusion": serde_json::Value::Null,
                    "intent_pose": format!("{:?}", runtime.intent.pose),
                    "gaze_mode": format!("{:?}", runtime.body.embodiment.pose.gaze_mode),
                    "living_language": runtime.last_phrase,
                    "vocal_lexicon": &runtime.life.state.vocal_lexicon,
                    "gaze": {
                        "source": runtime.gaze_trace.source.as_str(),
                        "lifecore_target": runtime.gaze_trace.lifecore_target.map(|target| target.to_array()),
                        "vita_resolved_target": runtime.gaze_trace.vita_target.map(|target| target.to_array()),
                        "ecology_resolved_target": runtime.gaze_trace.ecology_target.map(|target| target.to_array()),
                        "intent_world_target": runtime.intent.gaze_target.map(|target| target.to_array()),
                        "interaction_target": runtime.intent.interaction_target.as_ref().map(|target| format!("{target:?}")),
                        "cursor_world": runtime.sensors.cursor_position.to_array(),
                        "semantic_local_target": runtime.body.embodiment.semantic_gaze_target().to_array(),
                        "presented_local": runtime.body.embodiment.pose.gaze.to_array(),
                        "fixation_locked": runtime.body.embodiment.fixation_locked(),
                        "fixation_elapsed": gaze_fixation_elapsed,
                        "fixation_duration": gaze_fixation_duration,
                        "attention_kind": runtime.last_vita.as_ref().map(|output| format!("{:?}", output.attention.kind)),
                        "attention_position": runtime.last_vita.as_ref().and_then(|output| output.attention.position).map(|target| target.to_array()),
                        "attention_confidence": runtime.last_vita.as_ref().map(|output| output.attention.confidence),
                        "attention_commitment_remaining": runtime.last_vita.as_ref().map(|output| output.attention.commitment_remaining),
                    },
                    "blink_left": runtime.body.embodiment.pose.blink_left,
                    "blink_right": runtime.body.embodiment.pose.blink_right,
                    "pupil_size": runtime.body.embodiment.pose.pupil_size,
                    "pupil_luminance": runtime.body.embodiment.filtered_luminance(),
                    "microsaccade_offset": runtime.body.embodiment.microsaccade_offset().to_array(),
                    "microsaccade_events": runtime.body.embodiment.microsaccade_sequence(),
                    "fixation_lock": runtime.body.embodiment.fixation_locked(),
                    "pose_tilt": runtime.body.embodiment.pose.tilt,
                    "pointer": {
                        "down": runtime.pointer.down,
                        "hovered": runtime.pointer.pet_hovered,
                        "dragged": runtime.pointer.pet_dragged,
                    },
                    "expression_blink_left": runtime.intent.expression.blink_left,
                    "expression_blink_right": runtime.intent.expression.blink_right,
                    "expression": runtime.intent.expression,
                    "ecology_visual_effect": {
                        "hue": ecology_visual_effect.hue,
                        "color_blend": ecology_visual_effect.color_blend,
                        "flow_boost": ecology_visual_effect.flow_boost,
                        "glow_boost": ecology_visual_effect.glow_boost,
                        "cohesion_bias": ecology_visual_effect.cohesion_bias,
                        "translucency_boost": ecology_visual_effect.translucency_boost,
                        "contrast_reduction": ecology_visual_effect.contrast_reduction,
                    },
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
                    "vita": vita_details,
                    "ecology": {
                        "ecology_schema": ecology_state.schema_version,
                        "active_episode_id": active_ecology.map(|episode| episode.id),
                        "active_goal": ecology_debug.active_goal.map(|goal| format!("{goal:?}")),
                        "phase": ecology_debug.active_phase.map(|phase| format!("{phase:?}")),
                        "reason": format!("{:?}", ecology_debug.selected_reason),
                        "phase_elapsed": active_ecology.map(|episode| episode.elapsed_seconds),
                        "phase_elapsed_seconds": active_ecology.map(|episode| episode.phase_elapsed_seconds),
                        "commitment_remaining_seconds": active_ecology.map(|episode| episode.commitment_remaining),
                        "attempts": active_ecology.map(|episode| episode.attempts),
                        "prediction_confidence": active_ecology.map(|episode| episode.prediction_confidence),
                        "expected_outcome": active_ecology.map(|episode| format!("{:?}", episode.expected_outcome)),
                        "decision_tick": ecology_debug.tick,
                        "focus_mode_filtered": ecology_debug.focus_mode_filtered,
                        "candidate_scores": ecology_scores,
                        "escape_confidence": active_ecology
                            .filter(|episode| episode.goal == EpisodeGoal::EscapePressure)
                            .map(|episode| episode.prediction_confidence),
                        "help_requested": runtime.ecology.last_vocal_trigger()
                            == Some(EcologyVocalTrigger::NeedHelp)
                            || active_ecology.is_some_and(|episode| {
                                episode.goal == EpisodeGoal::RetrieveOrb
                                    && episode.phase == EpisodePhase::AskForHelp
                            }),
                        "orb_state": orb.map(|object| format!("{:?}", object.lifecycle)),
                        "orb_grounded": ecology_environment.orb_grounded,
                        "orb_trapped": ecology_environment.orb_trapped,
                        "orb_position": orb.map(|object| object.position.to_array()),
                        "orb_velocity": orb.map(|object| object.velocity.to_array()),
                        "orb_familiarity": orb.map(|object| object.familiarity),
                        "orb_preference": orb.map(|object| object.preference),
                        "den_anchor": ecology_state.den.anchor.to_array(),
                        "den_slots": ecology_state.den.slots,
                        "metabolic_reserve": ecology_state.metabolism.reserve,
                        "satiation": ecology_state.metabolism.satiation,
                        "active_food_effect": ecology_state.metabolism.active_effect,
                        "window_pressure": ecology_environment.pressure,
                        "external_contacts": ecology_contacts,
                        "saliency_target": saliency_target.map(|target| target.position.to_array()),
                        "saliency_kind": saliency_target.map(|target| format!("{:?}", target.kind)),
                        "saliency_score": saliency_target.map(|target| target.score),
                        "chromatic_blend": ecology_visual.chromatic_blend,
                        "camouflage_blend": ecology_visual.camouflage_blend,
                        "visual_structure": ecology_visual.visual_structure,
                        "visual_surprise": ecology_visual.visual_surprise,
                        "ecology_vocal_trigger": runtime.ecology.last_vocal_trigger().map(|trigger| format!("{trigger:?}")),
                        "active_skill": active_skill_id,
                        "skill_competence": active_skill.map(|skill| skill.competence),
                        "skill_uncertainty": active_skill.map(|skill| skill.uncertainty),
                        "motor_error": runtime.ecology.last_motor_error(),
                        "object_physics_us": runtime.ecology.object_physics_microseconds(),
                        "episode_tick_us": runtime.ecology.episode_tick_microseconds(),
                        "visual_grid_age_ms": runtime.vita.visual_age_seconds() * 1_000.0,
                    },
                    "timings_ms": {
                        "physics": [physics_timing.p50, physics_timing.p95, physics_timing.p99],
                        "render": [render_timing.p50, render_timing.p95, render_timing.p99],
                        "frame_gap": [frame_gap_timing.p50, frame_gap_timing.p95, frame_gap_timing.p99],
                        "desktop_poll": [desktop_timing.p50, desktop_timing.p95, desktop_timing.p99],
                        "camera": [camera_timing.p50, camera_timing.p95, camera_timing.p99],
                        "background_poll_upload": [background_timing.p50, background_timing.p95, background_timing.p99],
                    },
                    "performance": {
                        "fps": runtime.fps,
                        "maximum_frame_gap_ms": runtime.max_frame_gap_ms,
                        "brain_tick_microseconds": runtime.brain_tick_microseconds,
                        "telemetry_interval_seconds": runtime.debug_interval,
                        "timings_ms": {
                            "physics": [physics_timing.p50, physics_timing.p95, physics_timing.p99],
                            "render": [render_timing.p50, render_timing.p95, render_timing.p99],
                            "frame_gap": [frame_gap_timing.p50, frame_gap_timing.p95, frame_gap_timing.p99],
                            "desktop_poll": [desktop_timing.p50, desktop_timing.p95, desktop_timing.p99],
                            "camera": [camera_timing.p50, camera_timing.p95, camera_timing.p99],
                            "background_poll_upload": [background_timing.p50, background_timing.p95, background_timing.p99],
                        },
                        "skipped_render_frames": runtime.skipped_render_frames,
                        "dropped_body_time_seconds": runtime.dropped_body_time_seconds,
                    },
                    "audio_state": format!("{:?}", runtime.audio.state),
                    "audio_device": runtime.audio.device_name(),
                    "audio_config": runtime.audio.selected_config().map(|config| format!("{:?}", config)),
                    "audio_last_request": runtime.audio.last_request,
                    "audio_last_error": runtime.audio.last_error,
                    "audio_rms": runtime.audio.callback_levels().rms,
                    "audio_peak": runtime.audio.callback_levels().peak,
                    "audio_recent_rms": runtime.audio.recent_rms,
                    "audio_recent_peak": runtime.audio.recent_peak,
                    "audio_accepted_requests": runtime.audio.accepted_requests,
                    "audio_rejected_requests": runtime.audio.rejected_requests,
                    "audio": {
                        "state": format!("{:?}", runtime.audio.state),
                        "configured": runtime.audio.selected_config().is_some(),
                        "active": audio_visual.active,
                        "motif_id": audio_visual.motif_id,
                        "syllable_index": audio_visual.syllable_index,
                        "envelope": audio_visual.envelope,
                        "mouth_open": audio_visual.mouth_open,
                        "pitch_normalized": audio_visual.pitch_normalized,
                        "noisiness": audio_visual.noisiness,
                        "purr": audio_visual.purr,
                        "callback_rms": audio_levels.rms,
                        "callback_peak": audio_levels.peak,
                        "recent_rms": runtime.audio.recent_rms,
                        "recent_peak": runtime.audio.recent_peak,
                        "accepted_requests": runtime.audio.accepted_requests,
                        "rejected_requests": runtime.audio.rejected_requests,
                    },
                    "privacy": {
                        "raw_key_text": false,
                        "window_identifiers": false,
                        "screen_capture_pixels": false,
                    },
                });
                if let Some(fusion) = runtime.vita.fusion_diagnostics()
                    && let Some(details) = debug_details.as_object_mut()
                {
                    details.insert(
                        "fusion".into(),
                        serde_json::json!({
                            "authority": fusion.authority,
                            "attention_confidence": fusion.attention_confidence,
                            "local_kernel_protected": fusion.local_kernel_protected,
                            "used_vita_target": fusion.used_vita_target,
                            "morph_authority": fusion.morph_authority,
                            "used_morph": fusion.used_morph,
                        }),
                    );
                }
                let _ = self.store.append_telemetry(&EventLogEntry {
                    monotonic_seconds: runtime.normalizer.monotonic_seconds(),
                    kind: "debug_state".into(),
                    details: debug_details,
                });
                runtime.max_frame_gap_ms = 0.0;
            }
        }
        if runtime.save_accumulator >= 30.0 {
            runtime.save_accumulator %= 30.0;
            should_persist = true;
        }
        runtime.window.request_redraw();
        let screen_speed_px = runtime.screen_velocity_px.length();
        runtime.next_frame_deadline =
            now + runtime.presentation_cadence.frame_interval(screen_speed_px);
        event_loop.set_control_flow(ControlFlow::WaitUntil(runtime.next_frame_deadline));
        if should_persist && let Some(runtime) = self.runtime.as_ref() {
            self.persist(runtime);
        }
    }
}

impl ApplicationHandler for PetApplication {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        startup_probe(self.arguments.debug_log, "native resume entered");
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
        let topology = topology_from_event_loop(event_loop, 1);
        let desktop_bounds = topology.virtual_physical_bounds;
        if !desktop_bounds.is_valid() {
            eprintln!("could not create overlay: virtual desktop bounds are invalid");
            event_loop.exit();
            return;
        }
        let (host_origin, host_size) = desktop_host_geometry(desktop_bounds);
        let initial = topology.remap(&prepared.position);
        let initial_center = safe_body_center(
            &topology,
            Vec2::new(initial.x as f32, initial.y as f32),
            host_size,
        );
        let attributes = prepare_overlay_window_attributes(
            Window::default_attributes()
                .with_title("Pet 2")
                .with_inner_size(host_size)
                .with_position(host_origin)
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
        startup_probe(self.arguments.debug_log, "overlay window created");
        let initial_size = window.outer_size();
        let mut platform = create_platform_backend();
        if let Err(error) = platform
            .initialize(&window)
            .and_then(|_| platform.apply_overlay_policy(&window))
        {
            eprintln!("could not initialize desktop integration: {error}");
            event_loop.exit();
            return;
        }
        startup_probe(self.arguments.debug_log, "desktop integration initialized");
        let mut body = match ProceduralBody::generate(&prepared.life.state.genome) {
            Ok(body) => body,
            Err(error) => {
                eprintln!("could not generate procedural body: {error}");
                event_loop.exit();
                return;
            }
        };
        startup_probe(self.arguments.debug_log, "procedural body generated");
        if let Err(error) = load_migrate_apply_liquid_tuning(&self.store, &mut body) {
            eprintln!(
                "could not load production liquid tuning; using built-in Particle PBF / Cinematic Jelly: {error}"
            );
        }
        if !self.arguments.reset_pet
            && self.arguments.import_state.is_none()
            && let Err(error) = load_restore_body_state(&self.store, &mut body)
        {
            eprintln!(
                "could not restore physical body snapshot; deterministically reinitialized full mass: {error}"
            );
        }
        body.set_presentation_scale(production_presentation_scale(initial_size.height));
        body.simulation.feedback.world_position =
            physical_to_virtual_normalized(&topology, initial_center);
        configure_visual_motion_space(&mut body, &topology, initial_size);
        body.embodiment
            .set_motion_response_scale(PRODUCTION_FLIGHT_RESPONSE_SCALE);
        body.embodiment
            .set_motion_acceleration_limit(PRODUCTION_FLIGHT_ACCELERATION_LIMIT);
        synchronize_presentation_offset(
            &window,
            &topology,
            body.simulation.feedback.world_position,
            &mut body,
            host_origin,
        );
        let tuning_last_modified = std::fs::metadata(&self.store.paths.liquid_tuning)
            .and_then(|metadata| metadata.modified())
            .ok();
        let mut renderer = match pollster::block_on(Renderer::new_with_render_scale(
            Arc::clone(&window),
            &body.mesh,
            PRODUCTION_RENDER_SCALE,
        )) {
            Ok(renderer) => renderer,
            Err(error) => {
                eprintln!("could not initialize renderer: {error}");
                event_loop.exit();
                return;
            }
        };
        startup_probe(self.arguments.debug_log, "renderer initialized");
        // Use the same deterministic material-formation backdrop as Body Lab. It
        // remains transparent in the final compositor; only the refracted liquid
        // sees the screen-anchored checker.
        renderer.set_studio_material_backdrop(true);
        let ecology_renderer = EcologyRenderer::new(renderer.device(), renderer.surface_format());
        // A hidden Win32 composition surface may never become presentable, which would
        // deadlock the old "show after Presented" startup path. At this point the GPU
        // surface, transparent clear color, pipeline, and mesh are all ready, so making
        // the non-activating overlay visible cannot expose an uninitialized renderer.
        window.set_visible(true);
        let audio = AudioManager::new(self.arguments.no_audio);
        let acknowledged_window_origin = window.outer_position().unwrap_or(host_origin);
        let runtime_started = Instant::now();
        let runtime_started_unix_ms = unix_time_millis(SystemTime::now());
        let last_morph = prepared.morph.last_output();
        let loaded_life_state_hash = prepared.loaded_life_state_hash;
        let loaded_genome_hash = prepared.life.state.genome.stable_hash();
        self.runtime = Some(PetRuntime {
            window,
            renderer,
            ecology_renderer,
            platform,
            topology,
            normalizer: SensorNormalizer::default(),
            sensors: SensorFrame::default(),
            intent: neutral_intent(),
            body,
            life: prepared.life,
            vita: prepared.vita,
            morph: prepared.morph,
            ecology: prepared.ecology,
            brain_mode: self.arguments.brain_mode,
            dev_mode: self.arguments.dev_mode,
            audio,
            pointer: PointerState::default(),
            pointer_tracker: DesktopPointerTracker::default(),
            cursor_hittest_latch: CursorHitTestLatch::default(),
            acknowledged_window_origin,
            window_origin_changes: 0,
            screen_body_center: initial_center,
            screen_velocity_px: Vec2::ZERO,
            screen_collision_half_extent_px: Vec2::ZERO,
            screen_edge_contact: false,
            last_update: runtime_started,
            last_present: runtime_started,
            next_frame_deadline: runtime_started,
            body_accumulator: 0.0,
            dropped_body_time_seconds: 0.0,
            maximum_body_steps_per_frame: 0,
            maximum_screen_tick_step_px: 0.0,
            screen_tick_jump_events: 0,
            presented_pose: PresentedPoseMonitor::default(),
            presentation_cadence: PresentationCadence::default(),
            skipped_render_frames: 0,
            life_accumulator: 0.0,
            sensor_accumulator: 1.0,
            visual_poll_accumulator: VISUAL_SAMPLE_INTERVAL,
            last_visual_sample: None,
            save_accumulator: 0.0,
            tuning_poll_accumulator: 0.0,
            lab_control_poll_accumulator: LAB_CONTROL_POLL_INTERVAL_SECONDS,
            lab_interventions: LabInterventionState::new(runtime_started_unix_ms),
            tuning_last_modified,
            was_sleeping: false,
            visible_after_first_frame: false,
            modifiers: ModifiersState::empty(),
            debug_logging: self.arguments.debug_log,
            debug_interval: if self.arguments.dev_mode {
                DEV_MODE_LOG_INTERVAL
            } else {
                DEBUG_LOG_INTERVAL
            },
            debug_accumulator: 0.0,
            telemetry_sequence: 0,
            fps_accumulator: 0.0,
            frames_since_fps: 0,
            fps: 0.0,
            max_frame_gap_ms: 0.0,
            desktop_poll_microseconds: 0.0,
            background_capture_microseconds: 0.0,
            background_capture_timestamp: 0.0,
            background_capture_sequence: 0,
            background_luminance: 0.5,
            background_contrast: 0.0,
            expression_director: ExpressionDirector::default(),
            nervous_system: NervousSystemRuntime::default(),
            vocal_arbiter: VocalArbiter::default(),
            last_phrase: None,
            overlay_move_microseconds: 0.0,
            render_microseconds: 0.0,
            brain_tick_microseconds: 0.0,
            last_debug: None,
            last_vita: None,
            gaze_trace: GazeCausalTrace::default(),
            last_morph,
            teach: TeachRecorder::default(),
            physics_timings: TimingWindow::default(),
            render_timings: TimingWindow::default(),
            frame_gap_timings: TimingWindow::default(),
            desktop_poll_timings: TimingWindow::default(),
            camera_timings: TimingWindow::default(),
            background_timings: TimingWindow::default(),
            pending_gesture_convention: None,
            lab_pointer_fixture: None,
            last_convention_update_episode: 0,
            shutdown_for_promotion: false,
            runtime_ack_accumulator: 1.0,
            loaded_life_state_hash,
            loaded_genome_hash,
        });
        if let Some(runtime) = self.runtime.as_ref() {
            write_runtime_ack(&self.store, runtime, RuntimeLoadStatus::Running);
        }
        startup_probe(self.arguments.debug_log, "native runtime installed");
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
            WindowEvent::Resized(size) => {
                runtime.renderer.resize(size);
                runtime
                    .body
                    .set_presentation_scale(production_presentation_scale(size.height));
                configure_visual_motion_space(&mut runtime.body, &runtime.topology, size);
                synchronize_presentation_offset(
                    &runtime.window,
                    &runtime.topology,
                    runtime.body.simulation.feedback.world_position,
                    &mut runtime.body,
                    runtime.acknowledged_window_origin,
                );
            }
            WindowEvent::ScaleFactorChanged { .. } => {
                runtime.topology =
                    topology_from_event_loop(event_loop, runtime.topology.revision + 1);
                let (origin, size) =
                    desktop_host_geometry(runtime.topology.virtual_physical_bounds);
                runtime.window.set_outer_position(origin);
                let _ = runtime.window.request_inner_size(size);
                runtime.acknowledged_window_origin = origin;
                runtime
                    .body
                    .set_presentation_scale(production_presentation_scale(size.height));
                configure_visual_motion_space(&mut runtime.body, &runtime.topology, size);
                runtime
                    .body
                    .embodiment
                    .set_motion_response_scale(PRODUCTION_FLIGHT_RESPONSE_SCALE);
                runtime
                    .body
                    .embodiment
                    .set_motion_acceleration_limit(PRODUCTION_FLIGHT_ACCELERATION_LIMIT);
                synchronize_presentation_offset(
                    &runtime.window,
                    &runtime.topology,
                    runtime.body.simulation.feedback.world_position,
                    &mut runtime.body,
                    runtime.acknowledged_window_origin,
                );
                runtime.renderer.resize(runtime.window.inner_size());
            }
            WindowEvent::Moved(position) => {
                if position != runtime.acknowledged_window_origin {
                    runtime.window_origin_changes = runtime.window_origin_changes.saturating_add(1);
                }
                runtime.acknowledged_window_origin = position;
                synchronize_presentation_offset(
                    &runtime.window,
                    &runtime.topology,
                    runtime.body.simulation.feedback.world_position,
                    &mut runtime.body,
                    position,
                );
            }
            WindowEvent::MouseInput {
                state,
                button: MouseButton::Left,
                ..
            } => {
                let down = state == ElementState::Pressed;
                runtime.pointer_tracker.record_window_event(down);
            }
            WindowEvent::ModifiersChanged(modifiers) => runtime.modifiers = modifiers.state(),
            WindowEvent::KeyboardInput { event, .. }
                if event.state == ElementState::Pressed && !event.repeat =>
            {
                if runtime.modifiers.super_key() && runtime.modifiers.alt_key() {
                    match event.physical_key {
                        PhysicalKey::Code(KeyCode::KeyF) => {
                            let cursor = runtime.sensors.cursor_position;
                            let profile = morsel_profile_from_visual(
                                runtime.last_visual_sample.as_ref(),
                                cursor,
                                runtime.life.state.genome.body.primary_color_hsv.x,
                            );
                            if runtime
                                .ecology
                                .spawn_morsel(cursor, profile, runtime.sensors.timestamp.max(0.0))
                                .is_some()
                            {
                                runtime.save_accumulator = 30.0;
                            }
                        }
                        PhysicalKey::Code(KeyCode::KeyP) => {
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
                        PhysicalKey::Code(KeyCode::KeyL) => {
                            runtime
                                .vita
                                .cue_shared_attention(runtime.sensors.cursor_position, 3.0);
                        }
                        PhysicalKey::Code(KeyCode::KeyT) => {
                            let timestamp = runtime.sensors.timestamp.max(0.0);
                            if runtime.teach.active {
                                if let Some(signature) = runtime.teach.finish(timestamp)
                                    && runtime
                                        .ecology
                                        .learn_signature(signature, timestamp)
                                        .is_ok()
                                {
                                    runtime.save_accumulator = 30.0;
                                }
                            } else {
                                runtime
                                    .teach
                                    .start(runtime.sensors.cursor_position, timestamp);
                            }
                        }
                        PhysicalKey::Code(KeyCode::KeyM) => {
                            runtime.life.trigger_metamorphosis();
                            runtime.vita.note_metamorphosis();
                            if let Ok(mut body) =
                                ProceduralBody::generate(&runtime.life.state.genome)
                            {
                                if let Err(error) =
                                    load_migrate_apply_liquid_tuning(&self.store, &mut body)
                                {
                                    eprintln!(
                                        "could not restore liquid tuning after metamorphosis: {error}"
                                    );
                                }
                                let current_position =
                                    runtime.body.simulation.feedback.world_position;
                                body.set_presentation_scale(production_presentation_scale(
                                    runtime.window.outer_size().height,
                                ));
                                body.simulation.feedback.world_position = current_position;
                                configure_visual_motion_space(
                                    &mut body,
                                    &runtime.topology,
                                    runtime.window.outer_size(),
                                );
                                body.embodiment
                                    .set_motion_response_scale(PRODUCTION_FLIGHT_RESPONSE_SCALE);
                                body.embodiment.set_motion_acceleration_limit(
                                    PRODUCTION_FLIGHT_ACCELERATION_LIMIT,
                                );
                                synchronize_presentation_offset(
                                    &runtime.window,
                                    &runtime.topology,
                                    current_position,
                                    &mut body,
                                    runtime.acknowledged_window_origin,
                                );
                                runtime.renderer.replace_mesh(&body.mesh);
                                runtime.body = body;
                            }
                            runtime.save_accumulator = 30.0;
                        }
                        PhysicalKey::Code(KeyCode::KeyB) => {
                            runtime.brain_mode = runtime.brain_mode.toggled();
                            if runtime.brain_mode == BrainMode::Classic {
                                runtime.last_vita = None;
                            }
                            let _ = self.store.append_event(&EventLogEntry {
                                monotonic_seconds: runtime.normalizer.monotonic_seconds(),
                                kind: "brain_mode".into(),
                                details: serde_json::json!({
                                    "mode": runtime.brain_mode.as_str(),
                                }),
                            });
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
                let present_now = Instant::now();
                let present_dt = (present_now - runtime.last_present)
                    .as_secs_f32()
                    .clamp(0.0, 0.05);
                runtime.last_present = present_now;
                runtime.body.presentation_update(present_dt);
                let mut parameters = runtime.body.render_parameters(
                    &runtime.life.state.genome,
                    runtime.life.state.affect.arousal,
                );
                parameters.render_scale = PRODUCTION_RENDER_SCALE;
                let render_started = Instant::now();
                let bounds = runtime.topology.virtual_physical_bounds;
                let desktop_aspect = bounds.width().max(1) as f32 / bounds.height().max(1) as f32;
                let ecology_state = runtime.ecology.state();
                let ecology_renderer = &mut runtime.ecology_renderer;
                let ecology_time = runtime.normalizer.monotonic_seconds() as f32;
                let render_outcome = runtime.renderer.render_with_overlay(
                    parameters,
                    |_device, queue, encoder, view| {
                        ecology_renderer.render(
                            queue,
                            encoder,
                            view,
                            ecology_state,
                            desktop_aspect,
                            ecology_time,
                        );
                    },
                );
                runtime.render_microseconds = render_started.elapsed().as_secs_f64() * 1_000_000.0;
                runtime
                    .render_timings
                    .observe(runtime.render_microseconds as f32 / 1_000.0);
                match render_outcome {
                    RenderOutcome::Presented => {
                        runtime.frames_since_fps = runtime.frames_since_fps.saturating_add(1);
                        runtime.presented_pose.observe(runtime.screen_body_center);
                        if !runtime.visible_after_first_frame {
                            runtime.window.set_visible(true);
                            runtime.visible_after_first_frame = true;
                        }
                    }
                    RenderOutcome::Skipped => {
                        runtime.skipped_render_frames =
                            runtime.skipped_render_frames.saturating_add(1);
                    }
                    RenderOutcome::OutOfMemory => {
                        persist_runtime(&self.store, self.arguments.export_state.as_ref(), runtime);
                        event_loop.exit();
                    }
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
            runtime.ecology.prepare_shutdown();
        }
        if let Some(runtime) = self.runtime.as_ref() {
            self.persist(runtime);
            write_runtime_ack(&self.store, runtime, RuntimeLoadStatus::Stopped);
        }
    }
}

fn write_runtime_ack(store: &StateStore, runtime: &PetRuntime, status: RuntimeLoadStatus) {
    let acknowledgement = RuntimeLoadAcknowledgement {
        schema_version: RUNTIME_LOAD_ACK_SCHEMA_VERSION,
        status,
        pid: std::process::id(),
        executable_version: env!("CARGO_PKG_VERSION").to_owned(),
        loaded_life_state_hash: runtime.loaded_life_state_hash,
        loaded_genome_hash: runtime.loaded_genome_hash,
        updated_unix_ms: unix_time_millis(SystemTime::now()),
    };
    if let Err(error) = store.save_runtime_ack(&acknowledgement) {
        eprintln!("could not write runtime load acknowledgement: {error}");
    }
}

fn persist_runtime(store: &StateStore, export: Option<&PathBuf>, runtime: &PetRuntime) {
    let size = runtime.window.outer_size();
    let body_center = safe_body_center(
        &runtime.topology,
        virtual_normalized_to_physical(
            &runtime.topology,
            runtime.body.simulation.feedback.world_position,
        ),
        size,
    );
    let position = runtime.topology.normalize(PhysicalDesktopPoint {
        x: body_center.x.round() as i32,
        y: body_center.y.round() as i32,
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
    if let Err(error) = store.save_morph_brain(&runtime.morph.snapshot()) {
        eprintln!("could not save Morph brain state: {error}");
    }
    if let Err(error) = store.save_ecology_state(&runtime.ecology.snapshot()) {
        eprintln!("could not save habitat ecology state: {error}");
    }
    if let Err(error) = store.save_body_state(&runtime.body.body_material_snapshot()) {
        eprintln!("could not save physical body state: {error}");
    }
    if let Some(path) = export
        && let Err(error) = store.export_state(&portable, path)
    {
        eprintln!("could not export Pet 2 state: {error}");
    }
}

fn apply_shared_feedback(runtime: &mut PetRuntime, event: FeedbackEvent) {
    // Attribute a same-frame response to a performance the callback has already
    // started, even when the UI event arrives before the normal frame poll.
    synchronize_audio_learning(runtime);
    if matches!(
        event,
        FeedbackEvent::Ignored | FeedbackEvent::PushedAway | FeedbackEvent::MuteOrHide
    ) {
        runtime
            .ecology
            .observe_explicit_refusal(runtime.sensors.timestamp);
    }
    apply_gesture_convention_feedback(runtime, &event);
    runtime.vita.apply_feedback(&event);
    runtime.morph.apply_feedback(&event);
    runtime.life.apply_feedback(event);
}

fn stage_gesture_convention(
    runtime: &mut PetRuntime,
    event: &mut EmbodiedGestureEvent,
    signature: GestureSignature,
) {
    let sleeping = runtime.life.state.current_action == ActionId::Sleep;
    let safety_boundary = event.boundary != GestureBoundaryEvent::None;
    if sleeping
        || safety_boundary
        || event.observation_quality < pet_ecology::GESTURE_CONVENTION_MIN_QUALITY
    {
        return;
    }

    let recognized =
        runtime
            .ecology
            .recognize_gesture_convention(&signature, event.observation_quality, false);
    let meaning = recognized.map_or_else(
        || convention_meaning_for_gesture(event.classification.kind),
        |matched| Some(matched.meaning),
    );
    let Some(meaning) = meaning else {
        return;
    };
    if let Some(matched) = recognized {
        event.classification.kind = gesture_for_convention_meaning(matched.meaning);
        event.classification.confidence = event.classification.confidence.max(matched.confidence);
        event.classification.committed = true;
    }
    let existing_id = recognized.map(|matched| matched.id).or_else(|| {
        runtime
            .ecology
            .nearest_gesture_convention(meaning, &signature)
    });

    if runtime
        .pending_gesture_convention
        .as_ref()
        .is_some_and(|pending| pending.episode_id != event.classification.episode_id)
    {
        expire_pending_gesture_convention_now(runtime, true);
    }
    runtime.pending_gesture_convention = Some(PendingGestureConvention {
        episode_id: event.classification.episode_id,
        meaning,
        signature,
        existing_id,
        observed_at: runtime.sensors.timestamp.max(0.0),
    });
}

fn convention_meaning_for_gesture(kind: EmbodiedGestureKind) -> Option<GestureConventionMeaning> {
    match kind {
        EmbodiedGestureKind::CircularTwist => Some(GestureConventionMeaning::SpinGame),
        EmbodiedGestureKind::RhythmicTouch => Some(GestureConventionMeaning::RhythmMotif),
        EmbodiedGestureKind::SharpFlick | EmbodiedGestureKind::PullAndRelease => {
            Some(GestureConventionMeaning::LaunchReturn)
        }
        EmbodiedGestureKind::FragmentHelp => Some(GestureConventionMeaning::FragmentHelp),
        _ => None,
    }
}

const fn gesture_for_convention_meaning(meaning: GestureConventionMeaning) -> EmbodiedGestureKind {
    match meaning {
        GestureConventionMeaning::SpinGame => EmbodiedGestureKind::CircularTwist,
        GestureConventionMeaning::RhythmMotif => EmbodiedGestureKind::RhythmicTouch,
        GestureConventionMeaning::LaunchReturn => EmbodiedGestureKind::PullAndRelease,
        GestureConventionMeaning::FragmentHelp => EmbodiedGestureKind::FragmentHelp,
    }
}

fn apply_gesture_convention_feedback(runtime: &mut PetRuntime, event: &FeedbackEvent) {
    let outcome = match event {
        FeedbackEvent::PlayStarted => Some(ConventionOutcome::Positive),
        FeedbackEvent::Reward(value) if *value > 0.0 => Some(ConventionOutcome::Positive),
        FeedbackEvent::PushedAway | FeedbackEvent::MuteOrHide => {
            Some(ConventionOutcome::ExplicitNegative)
        }
        FeedbackEvent::Reward(value) if *value < 0.0 => Some(ConventionOutcome::ExplicitNegative),
        FeedbackEvent::Ignored | FeedbackEvent::Reward(_) => {
            Some(ConventionOutcome::AmbiguousOrNoResponse)
        }
        _ => None,
    };
    let Some(outcome) = outcome else {
        return;
    };
    let Some(pending) = runtime.pending_gesture_convention.take() else {
        return;
    };
    let now = runtime.sensors.timestamp.max(0.0);
    if now - pending.observed_at > 8.0 {
        if let Some(id) = pending.existing_id {
            let _ = runtime.ecology.record_gesture_convention_outcome(
                id,
                ConventionOutcome::AmbiguousOrNoResponse,
                now,
            );
        }
        return;
    }
    let episode_id = pending.episode_id;
    let learning_openness = runtime.body.tuning_profile().interaction.learning_openness;
    let changed = match outcome {
        ConventionOutcome::Positive => matches!(
            runtime.ecology.observe_gesture_convention_success(
                pending.meaning,
                pending.signature,
                now,
                true,
                learning_openness,
            ),
            Ok(Some(_))
        ),
        ConventionOutcome::AmbiguousOrNoResponse | ConventionOutcome::ExplicitNegative => {
            pending.existing_id.is_some_and(|id| {
                runtime
                    .ecology
                    .record_gesture_convention_outcome(id, outcome, now)
                    .is_ok()
            })
        }
    };
    if changed {
        runtime.last_convention_update_episode = episode_id;
        runtime.save_accumulator = 30.0;
    }
}

fn expire_pending_gesture_convention(runtime: &mut PetRuntime) {
    let expired = runtime
        .pending_gesture_convention
        .as_ref()
        .is_some_and(|pending| runtime.sensors.timestamp.max(0.0) - pending.observed_at > 8.0);
    if expired {
        expire_pending_gesture_convention_now(runtime, true);
    }
}

fn expire_pending_gesture_convention_now(runtime: &mut PetRuntime, ambiguous: bool) {
    let Some(pending) = runtime.pending_gesture_convention.take() else {
        return;
    };
    if ambiguous
        && let Some(id) = pending.existing_id
        && runtime
            .ecology
            .record_gesture_convention_outcome(
                id,
                ConventionOutcome::AmbiguousOrNoResponse,
                runtime.sensors.timestamp.max(0.0),
            )
            .is_ok()
    {
        runtime.save_accumulator = 30.0;
    }
}

fn poll_lab_control(
    store: &StateStore,
    runtime: &mut PetRuntime,
    wall_now: SystemTime,
    monotonic_now: Instant,
) {
    let envelope = match store.load_lab_control() {
        Ok(Some(envelope)) => envelope,
        Ok(None) => return,
        Err(_) => {
            // The external slot is untrusted. Storage already enforces its
            // schema and byte cap; telemetry records only a categorical error
            // so paths or payload fragments never leak into the monitor.
            runtime.lab_interventions.note_load_error();
            return;
        }
    };
    let admission = runtime
        .lab_interventions
        .admit_command(&envelope, unix_time_millis(wall_now));
    if admission != LabCommandAdmission::Execute {
        return;
    }

    let command_id = envelope.command_id;
    let changed = match envelope.command {
        LabControlCommand::CueAttention {
            position,
            duration_seconds,
        } => {
            runtime
                .vita
                .cue_shared_attention(Vec2::from_array(position), duration_seconds);
            true
        }
        LabControlCommand::DrivePulse {
            drive,
            delta,
            duration_seconds,
        } => {
            runtime.lab_interventions.replace_drive_pulse(
                command_id,
                drive,
                delta,
                duration_seconds,
                monotonic_now,
            );
            true
        }
        LabControlCommand::Reward { value } => {
            apply_shared_feedback(runtime, FeedbackEvent::Reward(value));
            runtime.save_accumulator = 30.0;
            true
        }
        LabControlCommand::FocusMode { enabled } => {
            if runtime.life.state.focus_mode == enabled {
                false
            } else {
                apply_shared_feedback(
                    runtime,
                    if enabled {
                        FeedbackEvent::FocusModeEnabled
                    } else {
                        FeedbackEvent::FocusModeDisabled
                    },
                );
                runtime.save_accumulator = 30.0;
                true
            }
        }
        LabControlCommand::ClearDrivePulses => {
            let changed = !runtime.lab_interventions.active_pulses.is_empty();
            runtime.lab_interventions.active_pulses.clear();
            changed
        }
        LabControlCommand::StimulatePointerGesture {
            gesture,
            intensity,
            duration_seconds,
        } => {
            if runtime.dev_mode {
                runtime.lab_pointer_fixture =
                    Some(LabPointerFixture::new(gesture, intensity, duration_seconds));
                true
            } else {
                false
            }
        }
        LabControlCommand::DeleteGestureConvention { convention_id } => {
            let changed = runtime.ecology.delete_gesture_convention(convention_id);
            if changed {
                runtime.pending_gesture_convention = None;
                runtime.save_accumulator = 30.0;
            }
            changed
        }
        LabControlCommand::RollbackGestureConventions { version } => {
            let changed = runtime.ecology.rollback_gesture_conventions(version);
            if changed {
                runtime.pending_gesture_convention = None;
                runtime.save_accumulator = 30.0;
            }
            changed
        }
        LabControlCommand::ClearGestureConventions => {
            let changed = runtime.ecology.clear_gesture_conventions();
            if changed {
                runtime.pending_gesture_convention = None;
                runtime.save_accumulator = 30.0;
            }
            changed
        }
        LabControlCommand::ShutdownForPromotion => {
            runtime.shutdown_for_promotion = true;
            true
        }
    };
    runtime.lab_interventions.note_applied(changed);
}

fn unix_time_millis(time: SystemTime) -> u64 {
    time.duration_since(UNIX_EPOCH).map_or(0, |duration| {
        duration.as_millis().min(u128::from(u64::MAX)) as u64
    })
}

const fn lab_command_name(command: &LabControlCommand) -> &'static str {
    match command {
        LabControlCommand::CueAttention { .. } => "cue_attention",
        LabControlCommand::DrivePulse { .. } => "drive_pulse",
        LabControlCommand::Reward { .. } => "reward",
        LabControlCommand::FocusMode { .. } => "focus_mode",
        LabControlCommand::ClearDrivePulses => "clear_drive_pulses",
        LabControlCommand::StimulatePointerGesture { .. } => "stimulate_pointer_gesture",
        LabControlCommand::DeleteGestureConvention { .. } => "delete_gesture_convention",
        LabControlCommand::RollbackGestureConventions { .. } => "rollback_gesture_conventions",
        LabControlCommand::ClearGestureConventions => "clear_gesture_conventions",
        LabControlCommand::ShutdownForPromotion => "shutdown_for_promotion",
    }
}

const fn lab_drive_name(drive: LabDrive) -> &'static str {
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

fn add_drive_delta(drives: &mut Drives, drive: LabDrive, delta: f32) {
    let value = match drive {
        LabDrive::Safety => &mut drives.safety,
        LabDrive::Play => &mut drives.play,
        LabDrive::Curiosity => &mut drives.curiosity,
        LabDrive::Autonomy => &mut drives.autonomy,
        LabDrive::Sleep => &mut drives.sleep,
        LabDrive::Social => &mut drives.social,
        LabDrive::Comfort => &mut drives.comfort,
        LabDrive::Novelty => &mut drives.novelty,
    };
    *value = (*value + delta).clamp(0.0, 1.0);
}

fn morph_output_json(output: MorphOutput) -> serde_json::Value {
    serde_json::json!({
        "command": output.command.as_wire(),
        "manipulation": output.manipulation.as_wire(),
        "perception": output.perception.as_wire(),
        "carry": output.carry.as_wire(),
        "command_rates": output.command_rates,
        "winner_rate": output.winner_rate,
        "manipulation_rate": output.manipulation_rate,
        "perception_rate": output.perception_rate,
        "carry_rate": output.carry_rate,
        "confidence": output.confidence,
        "attention": format!("{:?}", output.attention),
        "attention_object_slot": output.attention_object_slot,
        "object_target": output.object_target.map(|target| target.to_array()),
        "valence": output.valence,
        "arousal": output.arousal,
        "conflict": output.conflict,
        "turn": output.turn,
    })
}

fn action_wire_name(action: ActionId) -> String {
    serde_json::to_value(action)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_else(|| format!("{action:?}"))
}

/// Keep lineage checkpoints self-contained for an offline, read-only evolution
/// scrubber. Restored legacy saves may temporarily contain a longer v1 vector,
/// so the telemetry boundary applies the same newest-first bound as LifeCore.
fn genome_telemetry_json(genome: &Genome) -> serde_json::Value {
    // serde_json cannot represent an arbitrary u128 as a JSON number. Keep the
    // full genome while encoding the lineage identifier losslessly as hex.
    serde_json::json!({
        "identity_seed": genome.identity_seed,
        "lineage_id": format!("{:032x}", genome.lineage_id),
        "generation": genome.generation,
        "body": &genome.body,
        "voice": &genome.voice,
        "temperament": &genome.temperament,
        "brain": &genome.brain,
        "mutation_rate": genome.mutation_rate,
        "developmental_plasticity": genome.developmental_plasticity,
    })
}

fn mutation_record_telemetry_json(record: &lifecore::MutationRecord) -> serde_json::Value {
    serde_json::json!({
        "generation": record.generation,
        "stage": record.stage,
        "changed_traits": &record.changed_traits,
        "before_genome": record.before_genome.as_ref().map(genome_telemetry_json),
        "after_genome": record.after_genome.as_ref().map(genome_telemetry_json),
        "parent_genome_hash": record.parent_genome_hash,
        "child_genome_hash": record.child_genome_hash,
        "lifetime_snapshot": &record.lifetime_snapshot,
    })
}

#[cfg(test)]
fn mutation_history_json(life: &LifeCore) -> serde_json::Value {
    let history = &life.state.development.mutation_history;
    let start = history.len().saturating_sub(MAX_MUTATION_HISTORY);
    serde_json::Value::Array(
        history[start..]
            .iter()
            .map(mutation_record_telemetry_json)
            .collect(),
    )
}

fn lifecore_telemetry_json(life: &LifeCore, debug: &DebugState) -> serde_json::Value {
    let state = &life.state;
    let definition = state.current_action.definition();
    let cooldowns = ActionId::ALL
        .iter()
        .copied()
        .zip(state.action_cooldowns)
        .map(|(action, remaining_seconds)| {
            serde_json::json!({
                "action": action_wire_name(action),
                "remaining_seconds": remaining_seconds,
            })
        })
        .collect::<Vec<_>>();
    let recent_actions = state
        .recent_actions
        .iter()
        .copied()
        .map(action_wire_name)
        .collect::<Vec<_>>();
    let pending_attention = state.pending_attention.as_ref().map(|pending| {
        serde_json::json!({
            "action": action_wire_name(pending.action),
            "elapsed_seconds": pending.elapsed_seconds,
            "response_window_seconds": pending.response_window_seconds,
            "progress": (pending.elapsed_seconds / pending.response_window_seconds.max(f32::EPSILON))
                .clamp(0.0, 1.0),
        })
    });

    let memory = life.memory_system();
    let recent_memory_start = memory
        .short_term
        .len()
        .saturating_sub(TELEMETRY_RECENT_MEMORY_LIMIT);
    let recent_memory = memory
        .short_term
        .iter()
        .skip(recent_memory_start)
        .map(|event| {
            // Context vectors can encode user routines. The causal summary keeps
            // only the organism-side choice and outcome, never the raw context.
            serde_json::json!({
                "timestamp": event.timestamp,
                "action": action_wire_name(event.action),
                "outcome": event.outcome,
                "reward": event.reward,
                "salience": event.salience,
            })
        })
        .collect::<Vec<_>>();
    let episodic = memory
        .episodic
        .iter()
        .take(TELEMETRY_EPISODE_MEMORY_LIMIT)
        .map(|episode| {
            serde_json::json!({
                "action": action_wire_name(episode.representative.action),
                "outcome": episode.representative.outcome,
                "occurrence_count": episode.occurrence_count,
                "mean_reward": episode.mean_reward,
                "salience": episode.representative.salience,
            })
        })
        .collect::<Vec<_>>();
    let habits = memory
        .habits
        .iter()
        .map(|habit| {
            serde_json::json!({
                "actions": habit.actions.iter().copied().map(action_wire_name).collect::<Vec<_>>(),
                "success_score": habit.success_score,
                "use_count": habit.use_count,
            })
        })
        .collect::<Vec<_>>();

    let bandit = life.contextual_bandit();
    let mut learned_min = f32::INFINITY;
    let mut learned_max = f32::NEG_INFINITY;
    for weight in bandit.weights.iter().flatten().copied() {
        learned_min = learned_min.min(weight);
        learned_max = learned_max.max(weight);
    }
    if !learned_min.is_finite() || !learned_max.is_finite() {
        learned_min = 0.0;
        learned_max = 0.0;
    }
    let vocal_motifs = state
        .vocal_motifs
        .iter()
        .map(|motif| {
            serde_json::json!({
                "id": motif.id,
                "expected_reward": motif.expected_reward,
                "novelty": motif.novelty,
                "use_count": motif.use_count,
                "success_count": motif.success_count,
            })
        })
        .collect::<Vec<_>>();
    serde_json::json!({
        "tick": state.tick_count,
        "elapsed_seconds": state.elapsed_seconds,
        "action": {
            "current": action_wire_name(state.current_action),
            "elapsed_seconds": state.action_elapsed_seconds,
            "minimum_seconds": definition.minimum_duration,
            "maximum_seconds": definition.maximum_duration,
            "minimum_satisfied": state.action_elapsed_seconds >= definition.minimum_duration,
            "progress": (state.action_elapsed_seconds / definition.maximum_duration.max(f32::EPSILON))
                .clamp(0.0, 1.0),
            "cooldowns": cooldowns,
            "recent": recent_actions,
            "predicted_value": debug.predicted_action_value,
        },
        "attention": {
            "budget": state.attention_budget,
            "pending": pending_attention,
            "ignored_attempts": state.ignored_attempts,
            "successful_interactions": state.successful_interactions,
            "focus_mode": state.focus_mode,
        },
        "drives": state.drives,
        "affect": state.affect,
        "recent_reward": state.recent_reward,
        "learning": {
            "plastic_weight_range": debug.plastic_weight_range,
            "brain_divergence_count": debug.brain_divergence_count,
            "contextual_bandit": {
                "total_updates": bandit.total_updates,
                "learning_rate": bandit.learning_rate,
                "attempts": bandit.attempts,
                "weight_range": [learned_min, learned_max],
            },
            "memory": {
                "short_term_count": memory.short_term.len(),
                "episodic_count": memory.episodic.len(),
                "habit_count": memory.habits.len(),
                "recent": recent_memory,
                "episodic": episodic,
                "habits": habits,
                "user_model": {
                    "average_response_delay": memory.user_model.average_response_delay,
                    "preferred_sound_intensity": memory.user_model.preferred_sound_intensity,
                    "typical_activity_rate": memory.user_model.typical_activity_rate,
                    "attention_preference_count": memory.user_model.preferred_attention_strategies.len(),
                    "app_category_count": memory.user_model.interaction_by_app.len(),
                },
            },
            "vocal_repertoire": {
                "motif_count": state.vocal_motifs.len(),
                "selected_motif_id": state.selected_motif_id,
                "motifs": vocal_motifs,
                "pending_delivery": state.pending_vocal_delivery.as_ref().map(|pending| serde_json::json!({
                    "request_id": pending.request_id,
                    "motif_id": pending.motif_id,
                    "response_window_seconds": pending.response_window_seconds,
                    "penalize_if_ignored": pending.penalize_if_ignored,
                })),
                "pending_credit": state.pending_vocal_credit.as_ref().map(|pending| serde_json::json!({
                    "motif_id": pending.motif_id,
                    "elapsed_seconds": pending.elapsed_seconds,
                    "response_window_seconds": pending.response_window_seconds,
                    "penalize_if_ignored": pending.penalize_if_ignored,
                })),
            },
        },
        "development": {
            "stage": state.development.stage,
            "metamorphosis_count": state.development.metamorphosis_count,
            "lifetime": state.development.lifetime,
            // Exact before/after genomes live once at
            // /details/development/mutation_history. Repeating the whole
            // lineage here at 5 Hz would halve the bounded replay window.
            "mutation_history_count": state.development.mutation_history.len(),
        },
    })
}

fn activity_telemetry_json(
    life: &LifeCore,
    trace: &pet_ecology::EcologyDecisionTrace,
    episode: Option<&pet_ecology::ActivityEpisode>,
    scores: &[serde_json::Value],
) -> serde_json::Value {
    let state = &life.state;
    let definition = state.current_action.definition();
    let opportunities = trace.scores[..trace.score_count.min(trace.scores.len())]
        .iter()
        .filter(|score| score.eligible)
        .map(|score| {
            serde_json::json!({
                "goal": score.goal.map(|goal| format!("{goal:?}")),
                "score": score.score,
                "reason": format!("{:?}", score.reason),
            })
        })
        .collect::<Vec<_>>();
    let recent_outcome_start = life
        .memory_system()
        .short_term
        .len()
        .saturating_sub(TELEMETRY_RECENT_MEMORY_LIMIT);
    let recent_outcomes = life
        .memory_system()
        .short_term
        .iter()
        .skip(recent_outcome_start)
        .map(|event| {
            serde_json::json!({
                "action": action_wire_name(event.action),
                "outcome": event.outcome,
                "reward": event.reward,
            })
        })
        .collect::<Vec<_>>();
    let activity = activity_family(state.current_action, episode);

    serde_json::json!({
        // Mirrors the upstream ActivitySystem contract using Pet2's existing
        // LifeCore action arbiter plus the EpisodeDirector, without creating a
        // second behavior supervisor.
        "activity": activity,
        "concrete_action": action_wire_name(state.current_action),
        "source": if episode.is_some() { "ecology_episode" } else { "lifecore_action" },
        "phase": episode.map_or_else(
            || "lifecore".to_owned(),
            |active| format!("{:?}", active.phase),
        ),
        "reason": episode.map_or_else(
            || "lifecore_arbitration".to_owned(),
            |active| format!("{:?}", active.reason_code),
        ),
        "elapsed_ms": f64::from(state.action_elapsed_seconds) * 1_000.0,
        "min_ms": f64::from(definition.minimum_duration) * 1_000.0,
        "max_ms": f64::from(definition.maximum_duration) * 1_000.0,
        "progress": (state.action_elapsed_seconds / definition.maximum_duration.max(f32::EPSILON))
            .clamp(0.0, 1.0),
        "scores": scores,
        "opportunities": opportunities,
        "bouts": state.recent_actions.iter().copied().map(action_wire_name).collect::<Vec<_>>(),
        "recent_outcomes": recent_outcomes,
        "episode": episode.map(|active| serde_json::json!({
            "id": active.id,
            "goal": format!("{:?}", active.goal),
            "phase": format!("{:?}", active.phase),
            "reason": format!("{:?}", active.reason_code),
            "elapsed_ms": f64::from(active.elapsed_seconds) * 1_000.0,
            "phase_elapsed_ms": f64::from(active.phase_elapsed_seconds) * 1_000.0,
            "commitment_remaining_ms": f64::from(active.commitment_remaining) * 1_000.0,
            "attempts": active.attempts,
            "prediction_confidence": active.prediction_confidence,
            "expected_outcome": format!("{:?}", active.expected_outcome),
        })),
    })
}

fn activity_family(
    action: ActionId,
    episode: Option<&pet_ecology::ActivityEpisode>,
) -> &'static str {
    if let Some(episode) = episode {
        return match episode.goal {
            EpisodeGoal::EscapePressure | EpisodeGoal::RecoverAfterPressure => "safety",
            EpisodeGoal::InspectMorsel
            | EpisodeGoal::EatMorsel
            | EpisodeGoal::RefuseMorsel
            | EpisodeGoal::StoreMorsel => "forage",
            EpisodeGoal::SleepInDen => "sleep",
            EpisodeGoal::OfferOrb
            | EpisodeGoal::ChaseOrb
            | EpisodeGoal::InterceptOrb
            | EpisodeGoal::RetrieveOrb
            | EpisodeGoal::ReturnOrb
            | EpisodeGoal::CarryOrbHome
            | EpisodeGoal::SoloOrbPlay
            | EpisodeGoal::HideOrb
            | EpisodeGoal::SeekOrb
            | EpisodeGoal::PracticeSkill
            | EpisodeGoal::PerformSkill
            | EpisodeGoal::RhythmEcho => "play",
            EpisodeGoal::SharedAttention | EpisodeGoal::ChromaticEcho => "socialize",
            EpisodeGoal::ReturnHome
            | EpisodeGoal::ExitDen
            | EpisodeGoal::PeekFromDen
            | EpisodeGoal::InspectWindow
            | EpisodeGoal::RideWindow
            | EpisodeGoal::Camouflage => "explore",
        };
    }

    match action {
        ActionId::RetreatFromCursor | ActionId::FrustratedRetreat => "safety",
        ActionId::Sleep => "sleep",
        ActionId::IdleHover | ActionId::LandOnWindow | ActionId::ClingToWindowSide => "rest",
        ActionId::InviteCursorChase
        | ActionId::PlayCursorChase
        | ActionId::BringProceduralOrb
        | ActionId::HideAndSeek
        | ActionId::SelfPlay => "play",
        ActionId::ObserveUserActivity
        | ActionId::ApproachCursor
        | ActionId::InvitePetting
        | ActionId::MimicClickRhythm
        | ActionId::SilentStare
        | ActionId::HappyDisplay
        | ActionId::Chirp
        | ActionId::Purr => "socialize",
        ActionId::ObserveCursor
        | ActionId::WakeUp
        | ActionId::ExploreScreen
        | ActionId::PeekFromEdge
        | ActionId::Metamorphosis => "explore",
    }
}

fn morph_telemetry_json(
    output: MorphOutput,
    brain_mode: BrainMode,
    diagnostics: morph_brain::MorphDiagnostics,
) -> serde_json::Value {
    let mut morph = morph_output_json(output);
    if let Some(object) = morph.as_object_mut() {
        object.insert(
            "upstream_commit".into(),
            serde_json::json!(morph_brain::UPSTREAM_COMMIT),
        );
        object.insert("brain_mode".into(), serde_json::json!(brain_mode.as_str()));
        object.insert("diagnostics".into(), serde_json::json!(diagnostics));
    }
    morph
}

fn lab_interventions_telemetry_json(
    state: &LabInterventionState,
    now: Instant,
) -> serde_json::Value {
    let active_pulses = state
        .active_pulses
        .iter()
        .filter_map(|pulse| {
            pulse
                .expires_at
                .checked_duration_since(now)
                .map(|remaining| {
                    serde_json::json!({
                        "command_id": pulse.command_id,
                        "drive": lab_drive_name(pulse.drive),
                        "delta": pulse.delta,
                        "expires_in_ms": remaining.as_millis().min(u128::from(u64::MAX)) as u64,
                    })
                })
        })
        .collect::<Vec<_>>();
    let active_pulse_count = active_pulses.len();

    serde_json::json!({
        "last_command_id": state.last_seen_command_id,
        "last_command": state.last_command_kind,
        "last_command_status": state.last_command_status.as_str(),
        "active_pulse_count": active_pulse_count,
        "active_pulses": active_pulses,
        "natural_drives": state.last_natural_drives,
        "effective_drives": state.last_effective_drives,
        "overlay_active": active_pulse_count > 0,
        "overlay_stages": ["pre_morph", "post_lifecore_resolution"],
        "persistent_homeostasis_owner": "lifecore",
    })
}

fn body_interaction_telemetry_json(runtime: &PetRuntime) -> serde_json::Value {
    let frame = runtime.sensors.embodied_interaction;
    let classification = runtime.vita.latest_embodied_gesture();
    let turn = runtime.vita.interaction_turn();
    let persisted = &runtime.life.state.interactions;
    let plan = runtime
        .vita
        .active_interaction_plan()
        .or(persisted.last_response_plan);
    let appraisal = persisted.last_appraisal;
    let components = frame.components[..usize::from(frame.component_observation_count)]
        .iter()
        .skip(1)
        .take(3)
        .map(|component| {
            serde_json::json!({
                "slot": component.component_id,
                "lifecycle": component.lifecycle,
                "detach_reason": component.detach_reason,
                "particle_count": component.particle_count,
                "mass_fraction": component.mass_fraction,
                "center_local": component.center_local.to_array(),
                "velocity_local": component.velocity_local.to_array(),
                "angular_velocity": component.angular_velocity,
                "deformation": component.deformation,
                "age_seconds": component.age_seconds,
                "distance_to_main": component.distance_to_main,
                "offscreen_seconds": component.offscreen_seconds,
                "recovery_stage": component.lifecycle,
            })
        })
        .collect::<Vec<_>>();
    let conventions = &runtime.ecology.state().gesture_conventions;
    let convention_items = conventions
        .conventions
        .iter()
        .map(|convention| {
            serde_json::json!({
                "id": convention.id,
                "meaning": convention.meaning,
                "confidence": convention.confidence,
                "positive_outcomes": convention.positive_outcomes,
                "negative_outcomes": convention.negative_outcomes,
                "demonstrations": convention.demonstrations,
                "last_used_seconds": convention.last_used_seconds,
                "version": convention.version,
                "updated_this_episode": runtime.last_convention_update_episode
                    == classification.episode_id,
            })
        })
        .collect::<Vec<_>>();
    let selected_convention = runtime.pending_gesture_convention.as_ref().map(|pending| {
        serde_json::json!({
            "meaning": pending.meaning,
            "existing_id": pending.existing_id,
            "updated_this_episode": runtime.last_convention_update_episode
                == pending.episode_id,
        })
    });
    let causes = classification.causes[..usize::from(classification.cause_count)].to_vec();

    serde_json::json!({
        "episode_id": classification.episode_id,
        "phase": turn.state,
        "gesture": {
            "kind": classification.kind,
            "confidence": classification.confidence,
            "second_best": classification.second_best_confidence,
            "predicted": classification.predicted_kind,
            "prediction_confidence": classification.prediction_confidence,
            "prediction_error": classification.prediction_error,
            "intensity": classification.intensity,
            "committed": classification.committed,
            "ended": classification.ended,
            "causes": causes,
        },
        "contact": {
            "active": frame.contact.active,
            "point_local": frame.contact.point_local.to_array(),
            "normal_local": frame.contact.normal_local.to_array(),
            "area_fraction": frame.contact.area_fraction,
            "effective_pressure": frame.contact.effective_pressure,
            "relative_velocity_local": frame.contact.relative_velocity_local.to_array(),
            "relative_speed": frame.contact.relative_velocity_local.length(),
            "pointer_speed": frame.contact.pointer_speed,
            "pointer_acceleration": frame.contact.pointer_acceleration,
            "pressure_impulse": frame.contact.pressure_impulse,
            "contact_seconds": frame.contact.contact_seconds,
        },
        "material": {
            "deformation_energy": frame.material.deformation_energy,
            "deformation_rate": frame.material.deformation_rate,
            "maximum_strain": frame.material.maximum_strain,
            "neck_tension": frame.material.neck_tension,
            "neck_thickness": frame.material.neck_thickness,
            "slosh_energy": frame.material.slosh_energy,
            "internal_relative_speed": frame.material.internal_relative_speed,
            "mass_conservation_error": frame.material.mass_conservation_error,
            "topology_budget_remaining": frame.material.topology_budget_remaining,
            "topology_budget_exhausted": frame.material.topology_budget_exhausted,
        },
        "components": {
            "count": frame.material.component_count,
            "detached_mass_fraction": frame.material.detached_mass_fraction,
            "main_mass_fraction": (1.0 - frame.material.detached_mass_fraction).clamp(0.0, 1.0),
            "selected": components,
            "detached_event": frame.detached_event,
            "remerge_event": frame.remerge_event,
            "recovery_event": frame.recovery_event,
            "emergency_recovery_count": runtime.body.embodiment.liquid.diagnostics().recovery_count,
        },
        "appraisal": appraisal,
        "affect": {
            "before": persisted.affect_before,
            "after": persisted.affect_after,
        },
        "response": plan.map(|plan| serde_json::json!({
            "response_id": plan.response_id,
            "reason": plan.reason,
            "communicative_intent": plan.communicative_intent,
            "gaze": plan.gaze,
            "cooperation": plan.body.cooperation,
            "resistance": plan.body.resistance,
            "boundary": appraisal.map_or(0.0, |value| value.boundary_need),
            "expected_receiver_effect": plan.expected_receiver_effect,
            "voice_trigger": plan.voice_trigger,
            "response_emitted": turn.response_emitted,
        })),
        "turn": {
            "state": turn.state,
            "response_id": turn.response_id,
            "response_emitted": turn.response_emitted,
            "fresh_input_required": turn.response_emitted && !turn.fresh_input_after_response,
            "fresh_input_after_response": turn.fresh_input_after_response,
            "elapsed_seconds": turn.elapsed_seconds,
        },
        "learned_convention": selected_convention,
        "learned_conventions": {
            "version": conventions.version,
            "count": conventions.conventions.len(),
            "history_count": conventions.history_len(),
            "rollback_versions": conventions.rollback_versions(),
            "items": convention_items,
        },
        "lab_fixture_active": runtime.lab_pointer_fixture.is_some(),
    })
}

fn vita_telemetry_json(runtime: &VitaRuntime, output: Option<&VitaOutput>) -> serde_json::Value {
    let state = runtime.state();
    let percept = runtime.percept();
    let influence = &state.influence;
    let attempts = influence
        .attempts
        .iter()
        .copied()
        .fold(0_u32, u32::saturating_add);
    let successes = influence
        .successes
        .iter()
        .copied()
        .fold(0_u32, u32::saturating_add);
    let current_model = state.self_model.action_models[state.self_model.last_action.index()];
    let interaction_turn = runtime.interaction_turn();

    serde_json::json!({
        "schema_version": state.schema_version,
        "elapsed_seconds": state.elapsed_seconds,
        "attention": state.attention,
        "attention_switches": state.attention_switches,
        "appraisal": state.appraisal,
        "mood": state.mood,
        "emotions": state.emotions,
        "self_model": {
            "last_action": action_wire_name(state.self_model.last_action),
            "prediction_error": state.self_model.prediction_error,
            "agency": state.self_model.agency,
            "uncertainty": state.self_model.uncertainty,
            "body_schema_confidence": state.self_model.body_schema_confidence,
            "calibration_urge": state.self_model.calibration_urge,
            "external_force_likelihood": state.self_model.external_force_likelihood,
            "current_action_model": {
                "mean_delta_position": current_model.mean_delta_position.to_array(),
                "mean_delta_velocity": current_model.mean_delta_velocity.to_array(),
                "contact_probability": current_model.contact_probability,
                "confidence": current_model.confidence,
                "samples": current_model.samples,
            },
        },
        "influence": {
            "mode": influence.mode,
            "cooldown_seconds": influence.cooldown_seconds,
            "consecutive_ignored": influence.consecutive_ignored,
            "attempts": attempts,
            "successes": successes,
        },
        "favorite_place_count": state.favorite_places.len(),
        "interaction_turn": {
            "episode_id": interaction_turn.episode_id,
            "response_id": interaction_turn.response_id,
            "state": format!("{:?}", interaction_turn.state),
            "elapsed_seconds": interaction_turn.elapsed_seconds,
            "response_emitted": interaction_turn.response_emitted,
            "fresh_input_after_response": interaction_turn.fresh_input_after_response,
        },
        "percept": {
            "pointer_gesture": percept.pointer,
            "typing_rate_hz": percept.typing_rate_hz,
            "typing_burstiness": percept.typing_burstiness,
            "typing_pause_seconds": percept.typing_pause_seconds,
            "click_rate_hz": percept.click_rate_hz,
            "scroll_velocity": percept.scroll_velocity,
            "scroll_burstiness": percept.scroll_burstiness,
            "window_motion": percept.window_motion,
            "window_pressure": percept.window_pressure,
            "popup_salience": percept.popup_salience,
            "mean_luminance": percept.mean_luminance,
            "local_luminance": percept.local_luminance,
            "colorfulness": percept.colorfulness,
            "warmth": percept.warmth,
            "dominant_hue": percept.dominant_hue,
            "visual_motion": percept.visual_motion,
            "visual_change": percept.visual_change,
            "user_available": percept.user_available,
            "event_count": percept.events.len(),
        },
        "output": output.map(|value| serde_json::json!({
            "attention": value.attention,
            "appraisal": value.appraisal,
            "dominant_emotion": value.dominant_emotion,
            "influence": value.influence,
            "self_check": value.self_check,
            "gaze_target": value.gaze_target.map(|target| target.to_array()),
            "direct_viewer_gaze": value.direct_viewer_gaze,
            "pose_override": value.pose_override.map(|pose| format!("{pose:?}")),
            "locomotion_override": value.locomotion_override.map(|mode| format!("{mode:?}")),
            "target_override": value.target_override.map(|target| target.to_array()),
            "interaction_override": value.interaction_override.as_ref().map(|target| format!("{target:?}")),
        })),
    })
}

fn percentile(sorted: &[f64], fraction: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let index = ((sorted.len() - 1) as f64 * fraction.clamp(0.0, 1.0)).round() as usize;
    sorted[index]
}

fn synchronize_audio_learning(runtime: &mut PetRuntime) {
    runtime.audio.tick();
    while let Some(request_id) = runtime.audio.take_heard_request() {
        runtime.life.confirm_vocal_request_heard(request_id);
    }
    while let Some(request_id) = runtime.audio.take_unheard_rejection() {
        runtime.life.cancel_vocal_request(request_id);
    }
}

#[cfg(not(feature = "legacy-expression-fallback"))]
fn coordinate_interaction_response(
    runtime: &mut PetRuntime,
    plan: lifecore::InteractionResponsePlan,
) -> lifecore::InteractionResponsePlan {
    let living = LivingStateFrame::from_life(&runtime.life.state);
    let physical = lifecore::PhysicalExpressionContext::from_frames(
        &runtime.sensors,
        &runtime.body.simulation.feedback,
    );
    let mut world =
        lifecore::WorldModelFrame::from_frames(&runtime.sensors, &runtime.body.simulation.feedback);
    for object in runtime.ecology.state().objects.iter().filter(|object| {
        !matches!(
            object.lifecycle,
            ObjectLifecycle::Consumed | ObjectLifecycle::StoredInDen
        )
    }) {
        let affordance = if object.kind == ObjectKind::Orb {
            lifecore::WorldAffordance::Play
        } else {
            lifecore::WorldAffordance::Approach
        };
        let mut entity = lifecore::WorldEntity::point(
            object.id,
            lifecore::WorldEntityKind::ProceduralObject,
            object.position,
            object.velocity,
            1.0,
            (object.novelty * 0.46 + object.familiarity * 0.18 + object.velocity.length() * 0.20)
                .clamp(0.0, 1.0),
            affordance,
            runtime.sensors.timestamp,
        );
        entity.novelty = object.novelty;
        entity.familiarity = object.familiarity;
        world.observe(entity);
    }
    let phrase = runtime
        .expression_director
        .direct_world(plan, living, physical, world);
    runtime.last_phrase = Some(phrase);
    phrase.plan
}

#[cfg(feature = "legacy-expression-fallback")]
fn coordinate_interaction_response(
    _runtime: &mut PetRuntime,
    plan: lifecore::InteractionResponsePlan,
) -> lifecore::InteractionResponsePlan {
    plan
}

fn preserve_navigation_during_material_drag(
    previous: &BodyIntent,
    next: &mut BodyIntent,
    material_dragged: bool,
) {
    if !material_dragged {
        return;
    }
    next.locomotion = previous.locomotion;
    next.target_position = previous.target_position;
    next.target_surface.clone_from(&previous.target_surface);
    next.desired_speed = previous.desired_speed;
    next.facing_direction = previous.facing_direction;
}

fn request_ecology_voice(
    runtime: &mut PetRuntime,
    trigger: EcologyVocalTrigger,
) -> Option<lifecore::VocalRequest> {
    let trigger = match trigger {
        EcologyVocalTrigger::ToyOffer => VocalTrigger::ToyOffer,
        EcologyVocalTrigger::CatchSuccess => VocalTrigger::CatchSuccess,
        EcologyVocalTrigger::MissAndRetry => VocalTrigger::MissAndRetry,
        EcologyVocalTrigger::NeedHelp => VocalTrigger::NeedHelp,
        EcologyVocalTrigger::FoodInspect => VocalTrigger::FoodInspect,
        EcologyVocalTrigger::FoodAccepted => VocalTrigger::FoodAccepted,
        EcologyVocalTrigger::FoodRefused => VocalTrigger::FoodRefused,
        EcologyVocalTrigger::HomeReturn => VocalTrigger::HomeReturn,
        EcologyVocalTrigger::SkillMastered => VocalTrigger::SkillMastered,
        EcologyVocalTrigger::RhythmEcho => VocalTrigger::RhythmEcho,
        EcologyVocalTrigger::VisualNotice => VocalTrigger::VisualNotice,
    };
    runtime.life.request_vocalization(trigger, &runtime.sensors)
}

fn enqueue_vocal_candidate(runtime: &mut PetRuntime, mut request: lifecore::VocalRequest) {
    let voice = runtime.life.state.genome.voice.clone();
    runtime
        .nervous_system
        .actuation()
        .voice
        .apply_to_request(&mut request, &voice);
    let request_id = request.performance_seed;
    let living = LivingStateFrame::from_life(&runtime.life.state);
    let Some(request) = runtime.vocal_arbiter.admit(
        request,
        runtime.sensors.timestamp.max(0.0),
        living.quiet_preferred,
    ) else {
        runtime.life.cancel_vocal_request(request_id);
        return;
    };
    let Some(motif) = runtime
        .life
        .state
        .vocal_motifs
        .iter()
        .find(|motif| motif.id == request.motif_id)
        .cloned()
    else {
        runtime.life.cancel_vocal_request(request.performance_seed);
        return;
    };
    if !runtime.audio.enqueue(&voice, &motif, &request) {
        runtime.life.cancel_vocal_request(request.performance_seed);
    }
}

fn voice_visual_state(audio: &AudioManager) -> VoiceVisualState {
    let feedback = audio.visual_feedback();
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

fn body_voice_frame(body: &ProceduralBody, sensors: &SensorFrame) -> lifecore::BodyVoiceFrame {
    let diagnostics = body.embodiment.liquid.diagnostics();
    let physical = body.embodied_interaction_frame();
    let total_mass = (diagnostics.main_mass + diagnostics.detached_mass).max(f32::EPSILON);
    let normalized_strain =
        ((diagnostics.maximum_bond_strain - 0.06) / (0.55 - 0.06)).clamp(0.0, 1.0);
    let bond_strain = normalized_strain * normalized_strain * (3.0 - 2.0 * normalized_strain);
    let collision_impulse = body
        .simulation
        .feedback
        .collision
        .as_ref()
        .map_or(0.0, |collision| collision.intensity.clamp(0.0, 1.0));
    lifecore::BodyVoiceFrame {
        main_mass_ratio: (diagnostics.main_mass / total_mass).clamp(0.0, 1.0),
        detached_mass_ratio: (diagnostics.detached_mass / total_mass).clamp(0.0, 1.0),
        component_count: diagnostics.component_count.clamp(1, 4) as u8,
        shape_aspect_ratio: diagnostics.stretch_ratio.clamp(1.0, 4.0),
        stretch: ((diagnostics.stretch_ratio - 1.0) / 0.45).clamp(-1.0, 1.0),
        compression: diagnostics.maximum_compression.clamp(0.0, 1.0),
        bond_strain,
        material_stress: diagnostics
            .stress_magnitude
            .max(physical.contact.effective_pressure)
            .max(physical.material.deformation_energy)
            .clamp(0.0, 1.0),
        contact_area: physical.contact.area_fraction.clamp(0.0, 1.0),
        slosh_energy: physical.material.slosh_energy.clamp(0.0, 1.0),
        internal_speed: (physical.material.internal_relative_speed / 8.0).clamp(0.0, 1.0),
        collision_impulse,
        contact_impulse: 0.0,
        release_impulse: if sensors.pointer_released {
            (physical.material.internal_relative_speed / 5.0
                + physical.contact.effective_pressure * 0.35)
                .clamp(0.0, 1.0)
        } else {
            0.0
        },
        detach_impulse: if physical.detached_event.is_some() {
            0.82
        } else {
            0.0
        },
        remerge_impulse: if physical.remerge_event.is_some() {
            0.86
        } else {
            0.0
        },
    }
    .sanitized()
}

fn perception_visual_frames(frame: DesktopVisualFrame) -> (VisualFeatureFrame, SpatialVisualFrame) {
    let summary = frame.summary;
    (
        VisualFeatureFrame {
            mean_luminance: summary.mean_luminance,
            local_luminance: summary.local_luminance,
            contrast: summary.contrast,
            colorfulness: summary.colorfulness,
            warmth: summary.warmth,
            dominant_hue: summary.dominant_hue,
            motion_energy: summary.motion_energy,
            edge_density: summary.edge_density,
            sudden_change: summary.sudden_change,
        },
        SpatialVisualFrame {
            cells: std::array::from_fn(|index| {
                let cell = frame.cells[index];
                SpatialVisualCell {
                    luminance: cell.luminance,
                    contrast: cell.contrast,
                    colorfulness: cell.colorfulness,
                    warmth: cell.warmth,
                    hue: cell.hue,
                    motion: cell.motion,
                    edge_density: cell.edge_density,
                    sudden_change: cell.sudden_change,
                }
            }),
            sequence: frame.sequence,
            timestamp: frame.timestamp,
        },
    )
}

fn morsel_profile_from_visual(
    frame: Option<&DesktopVisualFrame>,
    position: Vec2,
    fallback_hue: f32,
) -> MorselProfile {
    let cell = frame.map(|frame| frame.cell_at(position));
    MorselProfile {
        hue: cell.map_or(fallback_hue.rem_euclid(1.0), |cell| cell.hue),
        saturation: cell.map_or(0.68, |cell| 0.42 + cell.colorfulness * 0.50),
        value: cell.map_or(0.88, |cell| 0.55 + cell.luminance * 0.40),
        warmth: cell.map_or(0.5, |cell| cell.warmth),
        pulse_rate: cell.map_or(0.38, |cell| 0.25 + cell.motion * 0.65),
        stimulation: cell.map_or(0.44, |cell| {
            0.20 + cell.motion * 0.42 + cell.sudden_change * 0.30
        }),
        cohesion_bias: cell.map_or(0.62, |cell| 0.45 + cell.contrast * 0.35),
        novelty: cell.map_or(0.72, |cell| {
            0.52 + cell.sudden_change * 0.34 + cell.colorfulness * 0.12
        }),
    }
}

fn visual_mind_input(
    life: &LifeCore,
    sensors: &SensorFrame,
    vita: Option<&VitaOutput>,
    self_uncertainty: f32,
    scroll_velocity: f32,
    window_pressure: f32,
) -> VisualMindInput {
    let mut input = VisualMindInput {
        valence: life.state.affect.valence,
        arousal: life.state.affect.arousal,
        stress: life.state.affect.stress,
        attachment: life.state.affect.attachment,
        confidence: life.state.affect.confidence,
        frustration: life.state.affect.frustration,
        fatigue: life.state.drives.sleep,
        curiosity: life.state.drives.curiosity,
        novelty: vita.map_or(0.0, |output| output.appraisal.novelty),
        social_focus: vita.map_or(
            0.0,
            |output| {
                if output.direct_viewer_gaze { 1.0 } else { 0.0 }
            },
        ),
        attention_confidence: vita.map_or(0.0, |output| output.attention.confidence),
        attention_commitment: vita.map_or(0.0, |output| {
            (output.attention.commitment_remaining / 2.1).clamp(0.0, 1.0)
        }),
        self_uncertainty,
        local_luminance: sensors
            .local_luminance
            .or(sensors.mean_luminance)
            .unwrap_or(0.5),
        scroll_velocity,
        window_pressure,
        eye_modifiers: Default::default(),
    };
    input.sanitize();
    input
}

fn load_migrate_apply_liquid_tuning(
    store: &StateStore,
    body: &mut ProceduralBody,
) -> Result<Option<LiquidTuningProfile>, String> {
    let stored = store
        .load_liquid_tuning::<LiquidTuningProfile>()
        .map_err(|error| error.to_string())?;
    let was_persisted = stored.is_some();
    let authored =
        stored.unwrap_or_else(|| approved_production_liquid_tuning(body.tuning_profile().seed));
    let sanitized = authored
        .clone()
        .sanitized()
        .map_err(|error| error.to_string())?;
    let applied = production_liquid_tuning(sanitized);
    body.apply_tuning_profile(applied.clone())
        .map_err(|error| error.to_string())?;
    if !was_persisted || authored != applied {
        store
            .save_liquid_tuning(&applied)
            .map_err(|error| error.to_string())?;
    }
    store
        .save_liquid_tuning_status(&LiquidTuningAcknowledgement {
            profile_revision: applied.profile_revision,
            schema_version: applied.schema_version,
            material_variant: applied.material.variant,
            build_version: env!("CARGO_PKG_VERSION").to_owned(),
        })
        .map_err(|error| error.to_string())?;
    Ok(Some(applied))
}

fn load_restore_body_state(store: &StateStore, body: &mut ProceduralBody) -> Result<bool, String> {
    let identity_seed = body.tuning_profile().seed;
    let tuning_schema = body.tuning_profile().schema_version;
    let pbf = body.tuning_profile().pbf;
    let snapshot = store
        .load_body_state_validated(|snapshot: &BodyMaterialSnapshot| {
            snapshot.validate(identity_seed, tuning_schema, pbf).is_ok()
        })
        .map_err(|error| error.to_string())?;
    let Some(snapshot) = snapshot else {
        return Ok(false);
    };
    body.restore_body_material_snapshot(&snapshot)
        .map_err(|error| error.to_string())?;
    Ok(true)
}

/// Production has one approved body/material pair. The analytic body and safe
/// material remain available inside Body Lab for diagnostics, but a stale user
/// profile must not silently switch the desktop organism back to either lane.
fn production_liquid_tuning(mut profile: LiquidTuningProfile) -> LiquidTuningProfile {
    let reference = approved_production_liquid_tuning(profile.seed);
    let stale_body = profile.render_mode != BodyRenderMode::ParticlePbf;
    let stale_material = profile.material.variant != MaterialVariant::CinematicJelly;
    if stale_body {
        profile.render_mode = BodyRenderMode::ParticlePbf;
        profile.pbf = reference.pbf;
        profile.material = reference.material;
        profile.face = reference.face;
        profile.compositor = reference.compositor;
    }
    if stale_material {
        profile.material = reference.material;
    }
    profile
}

/// Reproduces the user-approved `Black copy` profile without relying on the
/// profile being present in one particular Windows AppData directory.
fn approved_production_liquid_tuning(seed: u64) -> LiquidTuningProfile {
    let mut profile = LiquidTuningProfile::for_seed(seed);
    profile.name = "PET-2 Production Black".to_owned();
    profile.render_mode = BodyRenderMode::ParticlePbf;

    profile.pbf.density_compliance = 1.2e-5;
    profile.pbf.numerical_xsph = 0.014;
    profile.pbf.viscosity = 0.015;
    profile.pbf.surface_tension = 2.5;
    profile.pbf.flight_stretch = 1.9;
    profile.pbf.grab_stiffness = 390.0;
    profile.pbf.pointer_support_scale = 2.4;
    profile.pbf.pointer_response_hz = 35.0;
    profile.pbf.anisotropy_max = 2.01;
    profile.pbf.return_strength = 0.05;
    profile.pbf.character_field_radius_scale = 1.17;

    profile.material.variant = MaterialVariant::CinematicJelly;
    profile.material.override_genome_colors = true;
    profile.material.primary_hsv = [0.0, 0.0, 0.0];
    profile.material.secondary_hsv = [0.0, 0.39, 0.0];
    profile.material.glow_hsv = [0.0, 0.0, 0.06];
    profile.material.absorption = 0.5;
    profile.material.scattering = 2.8;
    profile.material.thickness = 0.9;
    profile.material.translucency = 0.84;
    profile.material.refraction = 2.4;
    profile.material.blur = 2.8;
    profile.material.rim_strength = 4.0;
    profile.material.rim_power = 5.5;
    profile.material.broad_specular = 0.25;
    profile.material.broad_specular_power = 11.0;
    profile.material.tight_specular = 0.65;
    profile.material.tight_specular_power = 64.0;
    profile.material.emission = 0.3;
    profile.material.fresnel_f0 = 0.035;
    profile.material.core_level = 1.35;
    profile.material.thickness_gamma = 0.25;
    profile.material.pseudo_depth = 0.3;
    profile.material.normal_scale = 1.3;
    profile.material.light_wrap = 1.2;
    profile.material.ambient_scatter = 0.9;
    profile.material.direct_scatter = 0.55;
    profile.material.transmission_hue_preservation = 0.25;
    profile.material.internal_flow = 0.12;
    profile.material.halo = 0.09;
    profile.material.opacity = 0.6;
    profile.material.cinematic_smoothing = 0.07;
    profile.material.internal_orb_count = 6;
    profile.material.internal_orb_intensity = 0.5;
    profile.material.internal_orb_size = 0.033;
    profile.material.internal_orb_halo = 2.05;
    profile.material.internal_orb_speed = 0.4;
    profile.material.internal_orb_depth = 0.4;
    profile.material.internal_orb_spread = 0.72;
    profile.material.studio_intensity = 1.5;
    profile.material.studio_base_roughness = 0.62;
    profile.material.studio_coat_roughness = 0.21;
    profile.material.narrow_rim_strength = 3.0;
    profile.material.broad_rim_strength = 0.62;
    profile.material.rim_saturation = 1.15;
    profile.material.edge_light_width = 23.0;
    profile.material.caustic_strength = 0.55;
    profile.material.caustic_scale = 2.0;
    profile.material.caustic_speed = 0.55;
    profile.material.caustic_dispersion = 0.65;
    profile.material.rounded_highlight_strength = 0.15;
    profile.material.highlight_tint = 0.18;
    profile.material.soul_glow_count = 3;
    profile.material.soul_glow_strength = 0.95;
    profile.material.soul_glow_size = 0.195;
    profile.material.soul_glow_speed = 0.28;
    profile.material.soul_glow_pulse = 0.18;
    profile.material.soul_glow_feather = 0.8;
    profile.material.bloom_strength = 0.05;

    profile.face.origin = [0.0, -0.03];
    profile.face.scale = [0.96, 1.02];
    profile.face.eye_size_scale = 0.55;
    profile.face.eye_spacing_scale = 1.0;
    profile.face.pupil_scale = 1.5;
    profile.face.eye_highlight_scale = 2.15;
    profile.face.eye_socket_strength = 0.34;
    profile.face.relief_strength = 0.6;
    profile.face.relief_darkness = 0.79;
    profile.face.relief_coat_strength = 1.6;
    profile.face.pupil_light_response = 1.65;
    profile.face.pupil_emotion_response = 1.42;
    profile.face.pupil_focus_response = 1.65;
    profile.face.microsaccade_amount = 1.65;
    profile.face.microsaccade_rate = 1.35;
    profile.face.maximum_roll_radians = 0.13;
    profile.face.translation_smoothing = 16.0;
    profile.face.rotation_smoothing = 10.0;
    profile.face.scale_smoothing = 13.0;

    profile.compositor.render_scale = 2;
    profile.compositor.shadow_horizontal_offset = 0.0;
    profile.compositor.shadow_vertical_offset = 0.0;
    profile.compositor.shadow_feather = 64.0;
    profile.compositor.shadow_opacity = 0.31;
    profile.compositor.shadow_color = [1.0, 1.0, 1.0];
    profile.compositor.exposure = 1.0;
    profile
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

fn update_desktop_presentation(
    window: &Window,
    topology: &DisplayTopology,
    normalized: Vec2,
    body: &mut ProceduralBody,
    acknowledged_origin: PhysicalPosition<i32>,
) {
    // Ordinary locomotion changes only the liquid's offset inside one stable
    // virtual-desktop surface. There is deliberately no SetWindowPos here.
    synchronize_presentation_offset(window, topology, normalized, body, acknowledged_origin);
}

fn desktop_host_geometry(bounds: RectI) -> (PhysicalPosition<i32>, PhysicalSize<u32>) {
    (
        PhysicalPosition::new(bounds.minimum.x, bounds.minimum.y),
        PhysicalSize::new(bounds.width().max(1) as u32, bounds.height().max(1) as u32),
    )
}

fn production_presentation_scale(host_height: u32) -> f32 {
    PRODUCTION_PRESENTATION_SCALE * host_height.max(1) as f32 / PRESENTATION_REFERENCE_HEIGHT
}

fn cap_body_physics_backlog(accumulator: f32, body_dt: f32) -> (f32, f32) {
    if !accumulator.is_finite() || !body_dt.is_finite() || body_dt <= 0.0 {
        return (0.0, 0.0);
    }
    let maximum = body_dt * MAX_BODY_STEPS_PER_FRAME as f32;
    let capped = accumulator.clamp(0.0, maximum);
    (capped, (accumulator - capped).max(0.0))
}

fn body_offset_for_origin(
    desired_body_center: Vec2,
    acknowledged_origin: PhysicalPosition<i32>,
    size: PhysicalSize<u32>,
) -> Vec2 {
    let half_size = Vec2::new(size.width as f32, size.height as f32) * 0.5;
    let acknowledged_center =
        Vec2::new(acknowledged_origin.x as f32, acknowledged_origin.y as f32) + half_size;
    desired_body_center - acknowledged_center
}

fn synchronize_presentation_offset(
    window: &Window,
    topology: &DisplayTopology,
    normalized: Vec2,
    body: &mut ProceduralBody,
    acknowledged_origin: PhysicalPosition<i32>,
) {
    let size = window.outer_size();
    if !topology.virtual_physical_bounds.is_valid() || size.width == 0 || size.height == 0 {
        return;
    }
    let desired_body_center = virtual_normalized_to_physical(topology, normalized);
    body.set_presentation_offset_pixels(
        body_offset_for_origin(desired_body_center, acknowledged_origin, size),
        window.inner_size().height.max(1) as f32,
    );
}

fn virtual_normalized_to_physical(topology: &DisplayTopology, normalized: Vec2) -> Vec2 {
    let bounds = topology.virtual_physical_bounds;
    Vec2::new(
        bounds.minimum.x as f32 + normalized.x.clamp(0.0, 1.0) * bounds.width() as f32,
        bounds.minimum.y as f32 + normalized.y.clamp(0.0, 1.0) * bounds.height() as f32,
    )
}

fn physical_to_virtual_normalized(topology: &DisplayTopology, physical: Vec2) -> Vec2 {
    let bounds = topology.virtual_physical_bounds;
    if !bounds.is_valid() {
        return Vec2::splat(0.5);
    }
    Vec2::new(
        (physical.x - bounds.minimum.x as f32) / bounds.width().max(1) as f32,
        (physical.y - bounds.minimum.y as f32) / bounds.height().max(1) as f32,
    )
    .clamp(Vec2::ZERO, Vec2::ONE)
}

fn safe_body_center(
    topology: &DisplayTopology,
    desired: Vec2,
    _overlay_size: PhysicalSize<u32>,
) -> Vec2 {
    // This margin belongs to the apparent organism, never to the transparent
    // desktop host. Deriving it from a full-screen window would keep the Pet
    // hundreds of unnecessary pixels away from monitor edges.
    let margin = 176.0;
    nearest_covered_center(topology, desired, Vec2::splat(-margin), Vec2::splat(margin))
}

fn project_navigation_target_to_monitor_union(
    intent: &mut lifecore::BodyIntent,
    topology: &DisplayTopology,
) {
    if !matches!(
        intent.locomotion,
        LocomotionMode::Seek
            | LocomotionMode::Arrive
            | LocomotionMode::Orbit
            | LocomotionMode::Wander
    ) || !intent.target_position.is_finite()
        || !topology.virtual_physical_bounds.is_valid()
    {
        return;
    }
    let desired = virtual_normalized_to_physical(topology, intent.target_position);
    let projected = safe_body_center(topology, desired, PhysicalSize::new(1, 1));
    intent.target_position = physical_to_virtual_normalized(topology, projected);
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct DesktopPointerTracker {
    sampled_down: bool,
    captured: bool,
    event_pressed: bool,
    event_released: bool,
    synthetic_release_pending: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct CursorHitTestLatch {
    enabled: bool,
}

impl CursorHitTestLatch {
    fn resolve(
        &mut self,
        inside_enter_margin: bool,
        inside_retain_margin: bool,
        captured: bool,
    ) -> bool {
        if captured || inside_enter_margin {
            self.enabled = true;
        } else if !inside_retain_margin {
            self.enabled = false;
        }
        self.enabled
    }
}

fn predictive_cursor_margin_pixels(
    normalized_cursor_velocity: Vec2,
    desktop_size_pixels: Vec2,
    body_speed_pixels_per_second: f32,
) -> f32 {
    let cursor_speed_pixels_per_second =
        if normalized_cursor_velocity.is_finite() && desktop_size_pixels.is_finite() {
            (normalized_cursor_velocity * desktop_size_pixels.max(Vec2::ONE)).length()
        } else {
            0.0
        };
    let body_speed_pixels_per_second = if body_speed_pixels_per_second.is_finite() {
        body_speed_pixels_per_second.max(0.0)
    } else {
        0.0
    };
    // Arm at least one 120 Hz global-input sample before contact. The previous
    // margin tracked only Pet velocity, so a fast cursor could reach the liquid
    // before Win32 finished changing the full-overlay hit-test style.
    (10.0 + (cursor_speed_pixels_per_second + body_speed_pixels_per_second) / 120.0)
        .clamp(10.0, 60.0)
}

impl DesktopPointerTracker {
    fn record_window_event(&mut self, down: bool) {
        if down {
            self.event_pressed = true;
        } else {
            self.event_released = true;
        }
    }
}

fn update_pointer_state(
    pointer: &mut PointerState,
    tracker: &mut DesktopPointerTracker,
    down: bool,
    hovered: bool,
) -> bool {
    let quick_click = tracker.event_pressed && tracker.event_released && !down;
    let (effective_down, pressed, released) = if tracker.synthetic_release_pending {
        tracker.synthetic_release_pending = false;
        (false, false, true)
    } else if quick_click {
        tracker.synthetic_release_pending = true;
        (true, true, false)
    } else {
        (
            down,
            down && !tracker.sampled_down || tracker.event_pressed,
            !down && tracker.sampled_down || tracker.event_released,
        )
    };
    tracker.event_pressed = false;
    tracker.event_released = false;
    if pressed && hovered {
        tracker.captured = true;
    }
    if released {
        tracker.captured = false;
    }
    tracker.sampled_down = effective_down;
    pointer.down = effective_down;
    pointer.pressed = pressed;
    pointer.released = released;
    pointer.pet_hovered = hovered;
    pointer.pet_touched = pressed && hovered;
    pointer.pet_dragged = tracker.captured && effective_down;
    pointer.pet_touched
}

#[allow(clippy::too_many_arguments)]
fn apply_screen_domain(
    body: &mut ProceduralBody,
    topology: &DisplayTopology,
    overlay_size: PhysicalSize<u32>,
    previous_center: &mut Vec2,
    previous_velocity_px: &mut Vec2,
    collision_half_extent_px: &mut Vec2,
    was_in_contact: &mut bool,
    dt: f32,
) {
    if dt <= 0.0 || !topology.virtual_physical_bounds.is_valid() {
        return;
    }
    let proposed =
        virtual_normalized_to_physical(topology, body.simulation.feedback.world_position);
    let visual = body.liquid_visual_bounds_pixels(overlay_size.height.max(1) as f32);
    let observed_half_extent = visual
        .main_minimum
        .abs()
        .max(visual.main_maximum.abs())
        .max(Vec2::splat(24.0));
    if collision_half_extent_px.min_element() <= 0.0 || !collision_half_extent_px.is_finite() {
        *collision_half_extent_px = observed_half_extent;
    } else {
        // The collision hull is symmetric around the navigation center. Local
        // liquid lag therefore deforms at the edge instead of moving the whole
        // organism in the opposite direction. Expansion is immediate; release is
        // intentionally slow so a one-frame surface fluctuation cannot produce an
        // A -> B -> A presentation flash.
        let release = 1.0 - (-1.4 * dt).exp();
        collision_half_extent_px.x = if observed_half_extent.x >= collision_half_extent_px.x {
            observed_half_extent.x
        } else {
            collision_half_extent_px.x
                + (observed_half_extent.x - collision_half_extent_px.x) * release
        };
        collision_half_extent_px.y = if observed_half_extent.y >= collision_half_extent_px.y {
            observed_half_extent.y
        } else {
            collision_half_extent_px.y
                + (observed_half_extent.y - collision_half_extent_px.y) * release
        };
    }
    let constrained = constrain_center_swept(
        topology,
        *previous_center,
        proposed,
        -*collision_half_extent_px,
        *collision_half_extent_px,
    );
    let rejected = proposed - constrained;
    let collided = rejected.length_squared() > 0.25;
    let desktop_size = Vec2::new(
        topology.virtual_physical_bounds.width().max(1) as f32,
        topology.virtual_physical_bounds.height().max(1) as f32,
    );
    let attempted_velocity = body.simulation.feedback.velocity * desktop_size;
    let mut physical_velocity = (constrained - *previous_center) / dt;
    if collided {
        let outward = rejected.normalize_or_zero();
        let normal_speed = attempted_velocity.dot(outward).max(0.0);
        let tangent = attempted_velocity - outward * attempted_velocity.dot(outward);
        physical_velocity = tangent * 0.85 - outward * normal_speed * 0.12;
        if !*was_in_contact {
            body.simulation.feedback.collision = Some(CollisionEvent {
                normal: -outward,
                intensity: (normal_speed / 900.0).clamp(0.0, 1.0),
            });
        } else {
            body.simulation.feedback.collision = None;
        }
        // Positive screen Y points downward. A rejected downward move is a
        // real support contact, allowing the next motor tick to distinguish
        // standing/resting weight from free flight.
        if outward.y > 0.55 {
            body.simulation.feedback.grounded = true;
        }
    }
    let acceleration_px = (physical_velocity - *previous_velocity_px) / dt;
    body.simulation.feedback.world_position = physical_to_virtual_normalized(topology, constrained);
    body.simulation.feedback.velocity = physical_velocity / desktop_size;
    body.simulation.feedback.acceleration = acceleration_px / desktop_size;
    *previous_center = constrained;
    *previous_velocity_px = physical_velocity;
    *was_in_contact = collided;
}

fn constrain_center_swept(
    topology: &DisplayTopology,
    previous: Vec2,
    desired: Vec2,
    bounds_minimum: Vec2,
    bounds_maximum: Vec2,
) -> Vec2 {
    let start = if aabb_covered(
        topology,
        previous + bounds_minimum,
        previous + bounds_maximum,
    ) {
        previous
    } else {
        nearest_covered_center(topology, previous, bounds_minimum, bounds_maximum)
    };
    let distance = start.distance(desired);
    let steps = (distance / 8.0).ceil().clamp(1.0, 128.0) as usize;
    let mut last_valid = start;
    for step in 1..=steps {
        let candidate = start.lerp(desired, step as f32 / steps as f32);
        if aabb_covered(
            topology,
            candidate + bounds_minimum,
            candidate + bounds_maximum,
        ) {
            last_valid = candidate;
            continue;
        }
        let mut low = last_valid;
        let mut high = candidate;
        for _ in 0..14 {
            let middle = (low + high) * 0.5;
            if aabb_covered(topology, middle + bounds_minimum, middle + bounds_maximum) {
                low = middle;
            } else {
                high = middle;
            }
        }
        return low;
    }
    last_valid
}

fn nearest_covered_center(
    topology: &DisplayTopology,
    desired: Vec2,
    bounds_minimum: Vec2,
    bounds_maximum: Vec2,
) -> Vec2 {
    if aabb_covered(topology, desired + bounds_minimum, desired + bounds_maximum) {
        return desired;
    }
    topology
        .monitors
        .iter()
        .filter_map(|monitor| {
            let area = monitor.physical_bounds;
            let minimum = Vec2::new(area.minimum.x as f32, area.minimum.y as f32) - bounds_minimum;
            let maximum = Vec2::new(area.maximum.x as f32, area.maximum.y as f32) - bounds_maximum;
            (minimum.x <= maximum.x && minimum.y <= maximum.y).then(|| {
                let candidate = desired.clamp(minimum, maximum);
                (candidate, candidate.distance_squared(desired))
            })
        })
        .min_by(|(_, distance_a), (_, distance_b)| distance_a.total_cmp(distance_b))
        .map_or(desired, |(candidate, _)| candidate)
}

fn aabb_covered(topology: &DisplayTopology, minimum: Vec2, maximum: Vec2) -> bool {
    if !minimum.is_finite()
        || !maximum.is_finite()
        || minimum.x >= maximum.x
        || minimum.y >= maximum.y
    {
        return false;
    }
    let mut xs = vec![minimum.x, maximum.x];
    let mut ys = vec![minimum.y, maximum.y];
    for monitor in &topology.monitors {
        let bounds = monitor.physical_bounds;
        let left = (bounds.minimum.x as f32).clamp(minimum.x, maximum.x);
        let right = (bounds.maximum.x as f32).clamp(minimum.x, maximum.x);
        let top = (bounds.minimum.y as f32).clamp(minimum.y, maximum.y);
        let bottom = (bounds.maximum.y as f32).clamp(minimum.y, maximum.y);
        xs.extend([left, right]);
        ys.extend([top, bottom]);
    }
    xs.sort_by(f32::total_cmp);
    ys.sort_by(f32::total_cmp);
    xs.dedup_by(|a, b| (*a - *b).abs() < 0.01);
    ys.dedup_by(|a, b| (*a - *b).abs() < 0.01);
    xs.windows(2).all(|x| {
        ys.windows(2).all(|y| {
            if x[1] - x[0] <= 0.001 || y[1] - y[0] <= 0.001 {
                return true;
            }
            let point = Vec2::new((x[0] + x[1]) * 0.5, (y[0] + y[1]) * 0.5);
            topology.monitors.iter().any(|monitor| {
                let bounds = monitor.physical_bounds;
                point.x >= bounds.minimum.x as f32
                    && point.x <= bounds.maximum.x as f32
                    && point.y >= bounds.minimum.y as f32
                    && point.y <= bounds.maximum.y as f32
            })
        })
    })
}

fn background_uv_transform(
    current_origin: PhysicalPosition<i32>,
    current_size: PhysicalSize<u32>,
    captured_rect: RectI,
) -> (Vec2, Vec2) {
    let capture_size = Vec2::new(
        captured_rect.width().max(1) as f32,
        captured_rect.height().max(1) as f32,
    );
    let current_origin = Vec2::new(current_origin.x as f32, current_origin.y as f32);
    let captured_origin = Vec2::new(
        captured_rect.minimum.x as f32,
        captured_rect.minimum.y as f32,
    );
    (
        Vec2::new(current_size.width as f32, current_size.height as f32) / capture_size,
        (current_origin - captured_origin) / capture_size,
    )
}

fn configure_visual_motion_space(
    body: &mut ProceduralBody,
    topology: &DisplayTopology,
    overlay_physical_size: winit::dpi::PhysicalSize<u32>,
) {
    let bounds = topology.virtual_physical_bounds;
    if !bounds.is_valid() || overlay_physical_size.height == 0 {
        return;
    }
    body.set_render_aspect(
        overlay_physical_size.width.max(1) as f32 / overlay_physical_size.height as f32,
    );
    body.simulation.set_motion_space_pixels(Vec2::new(
        bounds.width().max(1) as f32,
        bounds.height().max(1) as f32,
    ));
    body.set_desktop_motion_space(
        liquid_motion_desktop_size(bounds),
        overlay_physical_size.height as f32,
    );
}

fn classifier_tuning(tuning: pet_body::InteractionTuning) -> EmbodiedGestureClassifierTuning {
    EmbodiedGestureClassifierTuning {
        window_seconds: tuning.gesture_window_seconds,
        commit_confidence: tuning.gesture_commit_confidence,
        ambiguity_margin: tuning.gesture_ambiguity_margin,
        soft_touch_pressure_max: tuning.soft_touch_pressure_max,
        stretch_strain_min: tuning.stretch_strain_min,
        stretch_strain_max: tuning.stretch_strain_max,
        flick_speed_min: tuning.flick_speed_min,
        rhythm_interval_cv_max: tuning.rhythm_interval_cv_max,
        rhythm_min_impulses: tuning.rhythm_min_impulses,
        boundary_strain: tuning.boundary_strain,
    }
}

fn liquid_motion_desktop_size(bounds: RectI) -> Vec2 {
    Vec2::new(bounds.width().max(1) as f32, bounds.height().max(1) as f32)
}

fn hit_test_desktop_cursor(
    window: &Window,
    acknowledged_origin: PhysicalPosition<i32>,
    body: &ProceduralBody,
    cursor: PhysicalDesktopPoint,
    margin_pixels: f32,
) -> bool {
    let size = window.outer_size();
    if size.width == 0 || size.height == 0 {
        return false;
    }
    body.projected_hit_test_with_margin(
        Vec2::new(
            (cursor.x - acknowledged_origin.x) as f32 / size.width as f32,
            (cursor.y - acknowledged_origin.y) as f32 / size.height as f32,
        ),
        size.height as f32,
        margin_pixels,
    )
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
         \n  --simulate-hours HOURS   Compatibility alias for approximate calendar-only evolution\n\
         \n  --evolution-config PATH  Run a typed deterministic multi-rate curriculum\n\
         \n  --evolution-report PATH  Write the atomic schema-2 evolution report\n\
         \n  --evolution-progress PATH Atomically update bounded live progress\n\
         \n  --evolution-persist MODE dry-run, fork, or save-final\n\
         \n  --evolution-max-generations N  Override the bounded generation cap\n\
         \n  --pointer-replay PATH     Replay a bounded body-local physical interaction\n\
         \n  --import-state PATH      Validate and import portable state\n\
         \n  --export-state PATH      Export portable state\n\
         \n  --data-dir PATH          Override the application data directory\n\
         \n  --reset-learning         Reset learned weights, habits, and memories\n\
         \n  --reset-pet              Start a new organism from --seed\n\
         \n  --focus-mode             Suppress unsolicited attention\n\
         \n  --brain-mode MODE        classic, morphic, fusion, morph-shadow, or morph-fusion\n\
         \n  --debug-log              Write 1 Hz frame and embodiment diagnostics\n\
         \n  --dev-mode               Start bounded 5 Hz causal telemetry\n\
         \n  --no-audio-output        Disable device playback, not offline synthesis\n\
         \n\nHOTKEY: Win+Alt+D toggles bounded causal telemetry at runtime\n"
    );
}

#[cfg(test)]
mod tests {
    use pet_body::{BodyRenderMode, MaterialVariant};

    use super::*;

    #[test]
    fn command_line_parses_pointer_replay_and_typed_evolution_controls() {
        let parsed = Arguments::parse(
            [
                "--headless",
                "--pointer-replay",
                "fixture.json",
                "--evolution-config",
                "config.json",
                "--evolution-report",
                "report.json",
                "--evolution-persist",
                "dry-run",
                "--evolution-max-generations",
                "1",
            ]
            .into_iter()
            .map(str::to_owned),
        )
        .unwrap();

        assert!(parsed.headless);
        assert_eq!(parsed.pointer_replay, Some(PathBuf::from("fixture.json")));
        assert_eq!(parsed.evolution_config, Some(PathBuf::from("config.json")));
        assert_eq!(parsed.evolution_report, Some(PathBuf::from("report.json")));
        assert_eq!(parsed.evolution_persist, Some(EvolutionPersistence::DryRun));
        assert_eq!(parsed.evolution_max_generations, Some(1));
    }

    #[test]
    fn unconditional_evolve_flag_is_rejected() {
        let error = Arguments::parse(["--evolve".to_owned()].into_iter()).unwrap_err();
        assert!(error.to_string().contains("eligibility report"));
    }

    #[test]
    fn causal_telemetry_keeps_one_exact_bounded_lineage_history() {
        let mut life = LifeCore::new(Genome::from_seed(0x00CA_55A1), 0x0005_1A7E);
        let _ = life.trigger_metamorphosis();
        let output = life.tick(&SensorFrame::default(), &Default::default(), LIFE_DT);

        let details = lifecore_telemetry_json(&life, &output.debug);
        let mutation_history = mutation_history_json(&life);
        let history = mutation_history
            .as_array()
            .expect("mutation history is a JSON array");

        assert!(!history.is_empty());
        assert!(history.len() <= MAX_MUTATION_HISTORY);
        let latest = history.last().unwrap();
        assert!(latest["before_genome"].is_object());
        assert!(latest["after_genome"].is_object());
        assert!(latest["parent_genome_hash"].is_u64());
        assert!(latest["child_genome_hash"].is_u64());
        assert!(latest["lifetime_snapshot"].is_object());
        assert_eq!(
            latest["child_genome_hash"].as_u64(),
            Some(life.state.genome.stable_hash())
        );
        assert_eq!(details["tick"].as_u64(), Some(life.state.tick_count));
        assert_eq!(details["attention"]["focus_mode"].as_bool(), Some(false));
        assert_eq!(
            details["development"]["mutation_history_count"].as_u64(),
            Some(history.len() as u64)
        );
        assert!(details["development"].get("mutation_history").is_none());

        // Behavioral outcomes are observable, but raw context vectors that can
        // encode user routines never cross this telemetry boundary.
        let encoded = serde_json::to_string(&details).unwrap();
        assert!(!encoded.contains("\"context\":"));
    }

    #[test]
    fn maximum_lineage_history_leaves_room_in_the_bounded_telemetry_record() {
        let mut life = LifeCore::new(Genome::from_seed(0xE701_0710), 0x64);
        for _ in 0..MAX_MUTATION_HISTORY {
            let _ = life.trigger_metamorphosis();
        }

        let history = mutation_history_json(&life);
        assert_eq!(history.as_array().map(Vec::len), Some(MAX_MUTATION_HISTORY));
        let bytes = serde_json::to_vec(&history).unwrap().len() as u64;
        assert!(
            bytes < desktop_host::TELEMETRY_LOG_MAX_BYTES / 2,
            "lineage payload ({bytes} bytes) leaves too little room for a causal frame"
        );
    }

    #[test]
    fn activity_telemetry_matches_the_upstream_activity_contract_without_a_second_supervisor() {
        let mut life = LifeCore::new(Genome::from_seed(77), 91);
        let _ = life.tick(&SensorFrame::default(), &Default::default(), LIFE_DT);
        let trace = pet_ecology::EcologyDecisionTrace::default();
        let activity = activity_telemetry_json(&life, &trace, None, &[]);

        for key in [
            "activity",
            "phase",
            "reason",
            "elapsed_ms",
            "min_ms",
            "max_ms",
            "progress",
            "scores",
            "opportunities",
            "bouts",
            "recent_outcomes",
        ] {
            assert!(activity.get(key).is_some(), "missing activity key {key}");
        }
        assert_eq!(activity["activity"], "rest");
        assert_eq!(activity["concrete_action"], "idle_hover");
        assert_eq!(activity["source"], "lifecore_action");
        assert_eq!(activity["phase"], "lifecore");
    }

    #[test]
    fn morph_telemetry_names_its_source_commit_and_contains_live_diagnostics() {
        let brain = MorphBrain::new(0xD1A6_0057, None).unwrap();
        let morph = morph_telemetry_json(
            MorphOutput::default(),
            BrainMode::MorphFusion,
            brain.diagnostics(),
        );
        assert_eq!(morph["upstream_commit"], morph_brain::UPSTREAM_COMMIT);
        assert_eq!(morph["brain_mode"], "morph-fusion");
        assert!(morph["diagnostics"].is_object());
        assert!(morph["diagnostics"]["population_rates"].is_object());
        assert!(morph["diagnostics"]["classical_weights"].is_object());
        assert!(morph.get("command").is_some());
    }

    #[test]
    fn lab_control_is_strictly_monotonic_and_marks_newer_expired_commands_seen() {
        let command = LabControlCommand::DrivePulse {
            drive: LabDrive::Curiosity,
            delta: 0.4,
            duration_seconds: 2.0,
        };
        let envelope = |command_id, issued_unix_ms, expires_after_ms| LabControlEnvelope {
            schema_version: desktop_host::LAB_CONTROL_SCHEMA_VERSION,
            command_id,
            issued_unix_ms,
            expires_after_ms,
            command: command.clone(),
        };
        let mut state = LabInterventionState::default();

        assert_eq!(
            state.admit_command(&envelope(7, 1_000, 2_000), 1_100),
            LabCommandAdmission::Execute
        );
        state.note_applied(true);
        assert_eq!(
            state.admit_command(&envelope(7, 1_000, 2_000), 1_100),
            LabCommandAdmission::Stale
        );
        assert_eq!(
            state.admit_command(&envelope(6, 1_000, 2_000), 1_100),
            LabCommandAdmission::Stale
        );
        assert_eq!(
            state.admit_command(&envelope(8, 1_000, 100), 1_100),
            LabCommandAdmission::Expired
        );
        assert_eq!(state.last_seen_command_id, Some(8));
        assert_eq!(state.last_command_status, LabCommandStatus::Expired);
        assert_eq!(
            state.admit_command(&envelope(7, 1_000, 2_000), 1_100),
            LabCommandAdmission::Stale
        );
    }

    #[test]
    fn lab_control_from_before_this_process_cannot_replay_after_restart() {
        let envelope = |command_id, issued_unix_ms| LabControlEnvelope {
            schema_version: desktop_host::LAB_CONTROL_SCHEMA_VERSION,
            command_id,
            issued_unix_ms,
            expires_after_ms: 30_000,
            command: LabControlCommand::Reward { value: 0.5 },
        };
        let mut state = LabInterventionState::new(2_000);

        assert_eq!(
            state.admit_command(&envelope(41, 1_999), 2_001),
            LabCommandAdmission::PredatesProcess
        );
        assert_eq!(state.last_seen_command_id, Some(41));
        assert_eq!(state.last_command_status, LabCommandStatus::PredatesProcess);
        assert_eq!(
            state.admit_command(&envelope(42, 2_000), 2_001),
            LabCommandAdmission::Execute
        );
    }

    #[test]
    fn lab_drive_overlay_restores_post_lifecore_drives_exactly() {
        let now = Instant::now();
        let mut interventions = LabInterventionState::default();
        interventions.replace_drive_pulse(11, LabDrive::Curiosity, 1.0, 5.0, now);
        let natural = LifeCore::new(Genome::from_seed(33), 44).state.drives;
        let overlay = interventions.drive_overlay(natural, now);
        let mut runtime_drives = natural;

        overlay.apply_to(&mut runtime_drives);
        assert_eq!(runtime_drives.curiosity, 1.0);
        assert_ne!(runtime_drives, natural);
        overlay.restore_exact(&mut runtime_drives);

        assert_eq!(runtime_drives, natural);
        assert_eq!(interventions.last_natural_drives, Some(natural));
        assert_eq!(interventions.last_effective_drives, Some(overlay.effective));
    }

    #[test]
    fn lab_wall_clock_conversion_and_expiry_never_panic_before_unix_epoch() {
        let before_epoch = UNIX_EPOCH
            .checked_sub(Duration::from_millis(1))
            .expect("one millisecond before the epoch is representable");
        assert_eq!(unix_time_millis(before_epoch), 0);
    }

    #[test]
    fn teach_recorder_is_explicit_bounded_and_stores_only_a_signature() {
        let mut recorder = TeachRecorder::default();
        assert!(recorder.observe(Vec2::ZERO, 0.0).is_none());
        recorder.start(Vec2::splat(0.5), 1.0);
        for index in 1..=720 {
            let phase = index as f32 / 719.0 * std::f32::consts::TAU;
            let point = Vec2::splat(0.5) + Vec2::new(phase.sin(), phase.sin() * phase.cos()) * 0.2;
            let _ = recorder.observe(point, 1.0 + index as f64 / 120.0);
        }
        let signature = recorder.finish(7.0).or_else(|| {
            recorder.start(Vec2::splat(0.5), 1.0);
            recorder.observe(Vec2::new(0.7, 0.5), 7.0)
        });
        assert!(signature.is_some());
        assert!(!recorder.active);
        assert!(recorder.points.len() <= 720);
        assert!(signature.unwrap().is_valid());
    }

    #[test]
    fn audio_owner_starts_without_blocking_its_caller() {
        let (entered_sender, entered_receiver) = mpsc::sync_channel(1);
        let (release_sender, release_receiver) = mpsc::sync_channel(1);
        let channels = spawn_audio_owner_worker_with(move |_commands, events| {
            entered_sender.send(()).unwrap();
            release_receiver.recv().unwrap();
            events
                .send(AudioWorkerEvent::StartFailed {
                    error: "expected test failure".into(),
                })
                .unwrap();
        })
        .unwrap();

        entered_receiver
            .recv_timeout(Duration::from_secs(1))
            .expect("background owner starts");
        assert!(matches!(
            channels.event_receiver.try_recv(),
            Err(TryRecvError::Empty)
        ));
        release_sender.send(()).unwrap();
        let result = channels
            .event_receiver
            .recv_timeout(Duration::from_secs(1))
            .expect("background owner returns its status");
        assert!(matches!(
            result,
            AudioWorkerEvent::StartFailed { error } if error == "expected test failure"
        ));
    }

    #[test]
    fn disabled_audio_rejects_ecology_voice_without_training_or_blocking() {
        let mut manager = AudioManager::new(true);
        let mut core = LifeCore::new(Genome::from_seed(711), 731);
        let request = core
            .request_vocalization(VocalTrigger::NeedHelp, &SensorFrame::default())
            .unwrap();
        let motif = core
            .state
            .vocal_motifs
            .iter()
            .find(|motif| motif.id == request.motif_id)
            .unwrap()
            .clone();
        assert!(!manager.enqueue(&core.state.genome.voice, &motif, &request));
        core.cancel_vocal_request(request.performance_seed);
        assert!(core.state.pending_vocal_delivery.is_none());
        assert_eq!(manager.state, AudioManagerState::Disabled);
        assert_eq!(manager.accepted_requests, 0);
        assert_eq!(manager.rejected_requests, 1);
    }

    #[test]
    fn worker_acknowledgement_cancels_rejected_audio_but_credits_heard_audio() {
        let (command_sender, command_receiver) = mpsc::sync_channel(AUDIO_COMMAND_CAPACITY);
        let (event_sender, event_receiver) = mpsc::sync_channel(AUDIO_EVENT_CAPACITY);
        let mut manager = AudioManager::new(true);
        manager.state = AudioManagerState::Ready;
        manager.command_sender = Some(command_sender);
        manager.event_receiver = Some(event_receiver);
        let core = LifeCore::new(Genome::from_seed(71), 73);
        let voice = core.state.genome.voice.clone();
        let first_motif = core.state.vocal_motifs[0].clone();
        let first_request = lifecore::VocalRequest {
            motif_id: first_motif.id,
            performance_seed: 1,
            gain: 0.6,
            pan: 0.0,
            pitch_scale: 1.0,
            tempo_scale: 1.0,
            stress: 0.0,
            purr: false,
            gesture: lifecore::VoiceGesture::WarmChuff,
            priority: 128,
            style: lifecore::VocalStyle::SocialContact,
            valence: 0.0,
            arousal: 0.2,
            fatigue: 0.0,
            confidence: 0.8,
            attachment: 0.4,
            rhythm_intervals: [0.0; 8],
            phenotype: Default::default(),
        };
        assert!(manager.enqueue(&voice, &first_motif, &first_request));
        assert!(matches!(
            command_receiver.recv_timeout(Duration::from_secs(1)),
            Ok(AudioWorkerCommand::Enqueue { request, .. }) if request.motif_id == first_motif.id
        ));
        event_sender
            .send(AudioWorkerEvent::RequestRejected {
                request_id: first_request.performance_seed,
                error: "fake output rejected request".into(),
            })
            .unwrap();
        manager.poll_worker_events();
        assert_eq!(manager.take_unheard_rejection(), Some(1));
        assert_eq!(manager.take_unheard_rejection(), None);
        assert_eq!(manager.take_heard_request(), None);
        assert_eq!(manager.rejected_requests, 1);

        let second_motif = core.state.vocal_motifs[1].clone();
        let second_request = lifecore::VocalRequest {
            motif_id: second_motif.id,
            performance_seed: 2,
            ..first_request
        };
        assert!(manager.enqueue(&voice, &second_motif, &second_request));
        let _ = command_receiver
            .recv_timeout(Duration::from_secs(1))
            .unwrap();
        event_sender
            .send(AudioWorkerEvent::RequestHeard {
                request_id: second_request.performance_seed,
            })
            .unwrap();
        manager.poll_worker_events();
        assert_eq!(manager.accepted_requests, 1);
        assert_eq!(manager.take_heard_request(), Some(2));
        assert_eq!(manager.take_heard_request(), None);
        assert_eq!(manager.take_unheard_rejection(), None);
        assert!(manager.pending_requests.is_empty());
    }

    fn two_monitor_topology() -> DisplayTopology {
        DisplayTopology::new(
            vec![
                MonitorInfo {
                    id: MonitorId("left".into()),
                    physical_bounds: RectI {
                        minimum: PhysicalDesktopPoint { x: 0, y: 0 },
                        maximum: PhysicalDesktopPoint { x: 1_920, y: 1_080 },
                    },
                    working_area: RectI {
                        minimum: PhysicalDesktopPoint { x: 0, y: 0 },
                        maximum: PhysicalDesktopPoint { x: 1_920, y: 1_080 },
                    },
                    scale_factor: 1.0,
                    primary: true,
                },
                MonitorInfo {
                    id: MonitorId("right".into()),
                    physical_bounds: RectI {
                        minimum: PhysicalDesktopPoint { x: 1_920, y: 0 },
                        maximum: PhysicalDesktopPoint { x: 3_840, y: 1_080 },
                    },
                    working_area: RectI {
                        minimum: PhysicalDesktopPoint { x: 1_920, y: 0 },
                        maximum: PhysicalDesktopPoint { x: 3_840, y: 1_080 },
                    },
                    scale_factor: 1.0,
                    primary: false,
                },
            ],
            1,
        )
    }

    #[test]
    fn static_host_covers_the_complete_virtual_desktop_once() {
        let topology = two_monitor_topology();
        let (origin, size) = desktop_host_geometry(topology.virtual_physical_bounds);
        assert_eq!(origin, PhysicalPosition::new(0, 0));
        assert_eq!(size, PhysicalSize::new(3_840, 1_080));

        let negative = RectI {
            minimum: PhysicalDesktopPoint { x: -1_920, y: -900 },
            maximum: PhysicalDesktopPoint { x: 3_440, y: 1_440 },
        };
        let (origin, size) = desktop_host_geometry(negative);
        assert_eq!(origin, PhysicalPosition::new(-1_920, -900));
        assert_eq!(size, PhysicalSize::new(5_360, 2_340));
    }

    #[test]
    fn desktop_host_height_keeps_the_authored_apparent_pet_size() {
        let reference_apparent_height =
            PRESENTATION_REFERENCE_HEIGHT / PRODUCTION_PRESENTATION_SCALE;
        for height in [1_080, 1_440, 2_160] {
            let apparent_height = height as f32 / production_presentation_scale(height);
            assert!((apparent_height - reference_apparent_height).abs() < 0.01);
        }
    }

    #[test]
    fn delayed_frame_cannot_replay_a_navigation_teleport() {
        let body_dt = 1.0 / 120.0;
        let (capped, dropped) = cap_body_physics_backlog(0.250, body_dt);
        assert!((capped - body_dt * MAX_BODY_STEPS_PER_FRAME as f32).abs() < 1.0e-6);
        assert!((dropped - 0.225).abs() < 1.0e-6);

        let (ordinary, dropped) = cap_body_physics_backlog(1.0 / 59.0, body_dt);
        assert!((ordinary - 1.0 / 59.0).abs() < 1.0e-6);
        assert_eq!(dropped, 0.0);
    }

    #[test]
    fn material_drag_preserves_navigation_but_keeps_fresh_expression() {
        let previous = BodyIntent {
            locomotion: LocomotionMode::Wander,
            target_position: Vec2::new(0.72, 0.36),
            target_surface: None,
            desired_speed: 0.06,
            facing_direction: -1.0,
            gaze_target: Some(Vec2::new(0.2, 0.4)),
            pose: PoseIntent::Neutral,
            expression: ExpressionState::default(),
            interaction_target: None,
        };
        let mut next = BodyIntent {
            locomotion: LocomotionMode::Flee,
            target_position: Vec2::new(0.08, 0.91),
            target_surface: None,
            desired_speed: 0.1,
            facing_direction: 1.0,
            gaze_target: Some(Vec2::new(0.8, 0.6)),
            pose: PoseIntent::Compact,
            expression: ExpressionState {
                brow_raise: 0.8,
                ..ExpressionState::default()
            },
            interaction_target: None,
        };

        preserve_navigation_during_material_drag(&previous, &mut next, true);

        assert_eq!(next.locomotion, previous.locomotion);
        assert_eq!(next.target_position, previous.target_position);
        assert_eq!(next.desired_speed, previous.desired_speed);
        assert_eq!(next.facing_direction, previous.facing_direction);
        assert_eq!(next.gaze_target, Some(Vec2::new(0.8, 0.6)));
        assert_eq!(next.expression.brow_raise, 0.8);
    }

    #[test]
    fn presented_pose_monitor_detects_a_to_b_to_a_flash() {
        let mut monitor = PresentedPoseMonitor::default();
        for center in [
            Vec2::new(100.0, 200.0),
            Vec2::new(101.0, 200.5),
            Vec2::new(140.0, 220.0),
            Vec2::new(101.5, 201.0),
        ] {
            monitor.observe(center);
        }
        assert_eq!(monitor.frame_count, 4);
        assert_eq!(monitor.large_step_events, 2);
        assert_eq!(monitor.ping_pong_events, 1);

        let mut smooth = PresentedPoseMonitor::default();
        for index in 0..240 {
            smooth.observe(Vec2::new(index as f32 * 1.2, (index as f32 * 0.03).sin()));
        }
        assert_eq!(smooth.large_step_events, 0);
        assert_eq!(smooth.ping_pong_events, 0);
    }

    #[test]
    fn presentation_cadence_stays_sixty_for_pointer_and_audio_only_work() {
        let mut cadence = PresentationCadence::default();
        // Pointer/audio state is deliberately absent from this decision. Their
        // simulation and envelopes still update before each 60 Hz presentation.
        assert_eq!(cadence.frame_interval(0.0), SLEEP_FRAME);
        assert_eq!(cadence.frame_interval(7.9), SLEEP_FRAME);

        assert_eq!(cadence.frame_interval(12.1), ACTIVE_FRAME);
        assert_eq!(cadence.frame_interval(10.0), ACTIVE_FRAME);
        assert_eq!(cadence.frame_interval(7.9), SLEEP_FRAME);
    }

    #[test]
    fn body_center_stays_inside_outer_edges_but_crosses_adjacent_monitors_continuously() {
        let topology = two_monitor_topology();
        let size = PhysicalSize::new(3_840, 1_080);
        let outer = safe_body_center(&topology, Vec2::new(0.0, 540.0), size);
        assert!(outer.x >= 176.0);
        let seam = safe_body_center(&topology, Vec2::new(1_920.0, 540.0), size);
        assert!((seam.x - 1_920.0).abs() < f32::EPSILON);
    }

    #[test]
    fn static_host_offset_reconstructs_every_screen_position_without_moving_hwnd() {
        let size = PhysicalSize::new(3_840, 1_080);
        let origin = PhysicalPosition::new(0, 0);
        let half = Vec2::new(size.width as f32, size.height as f32) * 0.5;
        for center in [
            Vec2::new(176.0, 176.0),
            Vec2::new(1_920.0, 540.0),
            Vec2::new(3_664.0, 904.0),
        ] {
            let offset = body_offset_for_origin(center, origin, size);
            let presented = Vec2::new(origin.x as f32, origin.y as f32) + half + offset;
            assert!(presented.distance(center) < 0.001);
        }
    }

    #[test]
    fn liquid_motion_metric_uses_real_physical_desktop_pixels() {
        let topology = two_monitor_topology();
        let metric = liquid_motion_desktop_size(topology.virtual_physical_bounds);
        assert_eq!(metric, Vec2::new(3_840.0, 1_080.0));
    }

    #[test]
    fn screen_domain_allows_shared_seams_but_rejects_empty_monitor_gaps() {
        let topology = DisplayTopology::new(
            vec![
                two_monitor_topology().monitors[0].clone(),
                MonitorInfo {
                    id: MonitorId("upper-right".into()),
                    physical_bounds: RectI {
                        minimum: PhysicalDesktopPoint { x: 1_920, y: -900 },
                        maximum: PhysicalDesktopPoint { x: 3_200, y: 0 },
                    },
                    working_area: RectI {
                        minimum: PhysicalDesktopPoint { x: 1_920, y: -900 },
                        maximum: PhysicalDesktopPoint { x: 3_200, y: 0 },
                    },
                    scale_factor: 1.5,
                    primary: false,
                },
            ],
            2,
        );
        let extent = Vec2::splat(80.0);
        assert!(aabb_covered(
            &two_monitor_topology(),
            Vec2::new(1_840.0, 460.0),
            Vec2::new(2_000.0, 620.0),
        ));
        let stopped = constrain_center_swept(
            &topology,
            Vec2::new(1_700.0, 540.0),
            Vec2::new(2_300.0, 540.0),
            -extent,
            extent,
        );
        assert!(
            stopped.x <= 1_840.1,
            "crossed an empty L-layout gap: {stopped:?}"
        );
    }

    #[test]
    fn navigation_target_in_monitor_gap_is_projected_to_reachable_screen_space() {
        let topology = DisplayTopology::new(
            vec![
                two_monitor_topology().monitors[0].clone(),
                MonitorInfo {
                    id: MonitorId("upper-right".into()),
                    physical_bounds: RectI {
                        minimum: PhysicalDesktopPoint { x: 1_920, y: -900 },
                        maximum: PhysicalDesktopPoint { x: 3_200, y: 0 },
                    },
                    working_area: RectI {
                        minimum: PhysicalDesktopPoint { x: 1_920, y: -900 },
                        maximum: PhysicalDesktopPoint { x: 3_200, y: 0 },
                    },
                    scale_factor: 1.5,
                    primary: false,
                },
            ],
            3,
        );
        let unreachable = Vec2::new(2_300.0, 540.0);
        let mut intent = BodyIntent {
            locomotion: LocomotionMode::Wander,
            target_position: physical_to_virtual_normalized(&topology, unreachable),
            target_surface: None,
            desired_speed: 0.1,
            facing_direction: 1.0,
            gaze_target: None,
            pose: PoseIntent::Curious,
            expression: ExpressionState::default(),
            interaction_target: None,
        };

        project_navigation_target_to_monitor_union(&mut intent, &topology);

        let projected = virtual_normalized_to_physical(&topology, intent.target_position);
        assert!(aabb_covered(
            &topology,
            projected - Vec2::splat(176.0),
            projected + Vec2::splat(176.0),
        ));
        assert!(projected.distance(unreachable) > 100.0);
    }

    #[test]
    fn global_button_capture_survives_leaving_the_overlay_until_release() {
        let mut pointer = PointerState::default();
        let mut tracker = DesktopPointerTracker::default();
        assert!(update_pointer_state(&mut pointer, &mut tracker, true, true));
        assert!(pointer.pet_dragged);
        assert!(!update_pointer_state(
            &mut pointer,
            &mut tracker,
            true,
            false
        ));
        assert!(pointer.pet_dragged);
        assert!(!update_pointer_state(
            &mut pointer,
            &mut tracker,
            false,
            false
        ));
        assert!(pointer.released);
        assert!(!pointer.pet_dragged);
    }

    #[test]
    fn cursor_hittest_hysteresis_arms_early_and_releases_immediately_after_a_far_drag() {
        let mut latch = CursorHitTestLatch::default();
        assert!(!latch.resolve(false, false, false));
        assert!(latch.resolve(true, true, false));
        // A narrow spatial gap does not rewrite Win32 styles every sample.
        assert!(latch.resolve(false, true, false));
        // Capture wins even after the cursor leaves both margins.
        assert!(latch.resolve(false, false, true));
        // The first uncaptured far sample restores desktop click-through.
        assert!(!latch.resolve(false, false, false));
    }

    #[test]
    fn predictive_cursor_margin_covers_a_fixed_input_tick_and_is_bounded() {
        let desktop = Vec2::new(3_840.0, 1_080.0);
        assert_eq!(
            predictive_cursor_margin_pixels(Vec2::ZERO, desktop, 0.0),
            10.0
        );
        let ordinary = predictive_cursor_margin_pixels(Vec2::new(0.5, 0.0), desktop, 120.0);
        assert!((ordinary - 27.0).abs() < 0.001);
        let maximum = predictive_cursor_margin_pixels(Vec2::splat(100.0), desktop, 10_000.0);
        assert_eq!(maximum, 60.0);
        assert_eq!(
            predictive_cursor_margin_pixels(Vec2::splat(f32::NAN), desktop, f32::NAN),
            10.0
        );
    }

    #[test]
    fn quick_winit_click_between_global_polls_still_produces_capture_then_release() {
        let mut pointer = PointerState::default();
        let mut tracker = DesktopPointerTracker::default();
        tracker.record_window_event(true);
        tracker.record_window_event(false);
        assert!(update_pointer_state(
            &mut pointer,
            &mut tracker,
            false,
            true
        ));
        assert!(pointer.down);
        assert!(pointer.pet_dragged);
        assert!(!update_pointer_state(
            &mut pointer,
            &mut tracker,
            false,
            false
        ));
        assert!(pointer.released);
        assert!(!pointer.pet_dragged);
    }

    #[test]
    fn one_global_desktop_point_keeps_the_same_capture_texel_after_camera_move() {
        let captured = RectI {
            minimum: PhysicalDesktopPoint { x: 800, y: -120 },
            maximum: PhysicalDesktopPoint { x: 1_952, y: 1_032 },
        };
        let size = PhysicalSize::new(1_152, 1_152);
        let global = Vec2::new(1_500.0, 420.0);
        let first_origin = PhysicalPosition::new(800, -120);
        let second_origin = PhysicalPosition::new(940, -40);
        let (first_scale, first_offset) = background_uv_transform(first_origin, size, captured);
        let (second_scale, second_offset) = background_uv_transform(second_origin, size, captured);
        let first_local = (global - Vec2::new(first_origin.x as f32, first_origin.y as f32))
            / Vec2::splat(1_152.0);
        let second_local = (global - Vec2::new(second_origin.x as f32, second_origin.y as f32))
            / Vec2::splat(1_152.0);
        let first_texel = first_local * first_scale + first_offset;
        let second_texel = second_local * second_scale + second_offset;
        assert!(first_texel.distance(second_texel) < 1.0e-6);
    }

    #[test]
    fn liquid_profile_revision_variant_and_values_survive_restart_apply_path() {
        let directory = tempfile::tempdir().unwrap();
        let store = StateStore::at(directory.path());
        let genome = lifecore::Genome::from_seed(0xC1E);
        let mut profile = LiquidTuningProfile::for_seed(genome.identity_seed);
        profile.profile_revision = 27;
        profile.render_mode = BodyRenderMode::ParticlePbf;
        profile.material.variant = MaterialVariant::CinematicJelly;
        profile.pbf.viscosity = 0.041;
        store.save_liquid_tuning(&profile).unwrap();

        let mut first_body = ProceduralBody::generate(&genome).unwrap();
        let first = load_migrate_apply_liquid_tuning(&store, &mut first_body)
            .unwrap()
            .unwrap();
        let acknowledgement = store
            .load_liquid_tuning_status::<LiquidTuningAcknowledgement>()
            .unwrap()
            .unwrap();
        assert_eq!(acknowledgement.profile_revision, 27);
        assert_eq!(
            acknowledgement.material_variant,
            MaterialVariant::CinematicJelly
        );

        let mut restarted_body = ProceduralBody::generate(&genome).unwrap();
        let restarted = load_migrate_apply_liquid_tuning(&store, &mut restarted_body)
            .unwrap()
            .unwrap();
        assert_eq!(restarted, first);
        assert_eq!(restarted.profile_revision, 27);
        assert_eq!(restarted.render_mode, BodyRenderMode::ParticlePbf);
        assert_eq!(restarted.material.variant, MaterialVariant::CinematicJelly);
        assert_eq!(restarted.pbf.viscosity, 0.041);
        assert_eq!(restarted_body.tuning_profile(), &restarted);
    }

    #[test]
    fn fresh_store_persists_the_approved_second_body_and_material() {
        let directory = tempfile::tempdir().unwrap();
        let store = StateStore::at(directory.path());
        let genome = lifecore::Genome::from_seed(0xB0D1);
        let mut body = ProceduralBody::generate(&genome).unwrap();

        let applied = load_migrate_apply_liquid_tuning(&store, &mut body)
            .unwrap()
            .unwrap();
        let persisted = store
            .load_liquid_tuning::<LiquidTuningProfile>()
            .unwrap()
            .unwrap();

        assert_eq!(applied.render_mode, BodyRenderMode::ParticlePbf);
        assert_eq!(applied.material.variant, MaterialVariant::CinematicJelly);
        assert_eq!(applied.material.primary_hsv, [0.0, 0.0, 0.0]);
        assert_eq!(applied.material.opacity, 0.6);
        assert_eq!(applied.pbf.flight_stretch, 1.9);
        assert_eq!(persisted, applied);
        assert_eq!(body.tuning_profile(), &applied);
    }

    #[test]
    fn stale_analytic_safe_profile_cannot_replace_the_production_body() {
        let directory = tempfile::tempdir().unwrap();
        let store = StateStore::at(directory.path());
        let genome = lifecore::Genome::from_seed(0xB0D2);
        let mut stale = LiquidTuningProfile::for_seed(genome.identity_seed);
        stale.render_mode = BodyRenderMode::AnalyticJelly;
        stale.material.variant = MaterialVariant::CurrentSafe;
        stale.pbf.flight_stretch = 0.1;
        stale.material.opacity = 1.0;
        store.save_liquid_tuning(&stale).unwrap();
        let mut body = ProceduralBody::generate(&genome).unwrap();

        let applied = load_migrate_apply_liquid_tuning(&store, &mut body)
            .unwrap()
            .unwrap();

        assert_eq!(applied.render_mode, BodyRenderMode::ParticlePbf);
        assert_eq!(applied.material.variant, MaterialVariant::CinematicJelly);
        assert_eq!(applied.pbf.flight_stretch, 1.9);
        assert_eq!(applied.material.opacity, 0.6);
        assert_eq!(
            store
                .load_liquid_tuning::<LiquidTuningProfile>()
                .unwrap()
                .unwrap(),
            applied
        );
    }
}
