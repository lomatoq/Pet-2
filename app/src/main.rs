#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]
#![recursion_limit = "512"]

mod ecology_runtime;
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
    DesktopVisualFrame, DisplayTopology, EventLogEntry, MonitorId, MonitorInfo,
    PORTABLE_STATE_SCHEMA_VERSION, PersistedPetPosition, PhysicalDesktopPoint, PlatformBackend,
    PointerState, PortablePetState, RectI, SensorNormalizer, StateStore, create_platform_backend,
    prepare_overlay_window_attributes,
};
use ecology_runtime::EcologyRuntime;
use glam::Vec2;
use lifecore::{
    ActionId, BodyIntent, CollisionEvent, DebugState, ExpressionState, FeedbackEvent, Genome,
    LIFECORE_HZ, LifeCore, LocomotionMode, PoseIntent, SensorFrame, VitaOutput, VocalTrigger,
    stable_hash_bytes,
};
use morph_brain::{MorphBrain, MorphBrainState, MorphCommand, MorphOutput};
use pet_audio::{AudioCallbackLevels, AudioEngine, AudioVisualFeedback, SelectedOutputConfig};
use pet_body::{
    EcologyRenderer, LiquidTuningAcknowledgement, LiquidTuningProfile, ProceduralBody,
    RenderOutcome, Renderer, VisualMindInput, VoiceVisualState,
};
use pet_ecology::{ActionSignature, EcologyVocalTrigger, EpisodeGoal, MorselProfile};
use pet_perception::{SpatialVisualCell, SpatialVisualFrame, VisualFeatureFrame};
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

fn main() -> Result<(), Box<dyn Error>> {
    let Some(_single_instance) = SingleInstanceGuard::acquire()? else {
        eprintln!("Pet 2 is already running; refusing to create a second desktop organism");
        return Ok(());
    };
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
    export_state: Option<PathBuf>,
    import_state: Option<PathBuf>,
    data_dir: Option<PathBuf>,
    reset_learning: bool,
    reset_pet: bool,
    evolve: bool,
    no_audio: bool,
    focus_mode: bool,
    brain_mode: BrainMode,
    debug_log: bool,
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
                "--brain-mode" => {
                    parsed.brain_mode = value("--brain-mode", &mut arguments)?.parse()?;
                }
                "--debug-log" => parsed.debug_log = true,
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
    if arguments.evolve {
        life.trigger_metamorphosis();
        vita.note_metamorphosis();
    }
    life.set_focus_mode(arguments.focus_mode);
    Ok(PreparedState {
        life,
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
        position,
        mut vita,
        mut morph,
        mut ecology,
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
    let mut morph_command_counts = BTreeMap::<String, u64>::new();
    let mut morph_switches = 0_u64;
    let mut previous_morph_command = MorphCommand::Idle;
    let mut morph_tick_microseconds = Vec::with_capacity(tick_count.min(20_000) as usize);
    let mut distance_traveled = 0.0_f32;
    let mut moving_ticks = 0_u64;
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
        if tick % 11 == 0 {
            vita.note_key_activity(sensors.timestamp);
        }
        if tick % 97 == 0 {
            vita.note_scroll((time * 0.7).sin());
        }
        vita.observe(&sensors, &feedback, dt);
        let morph_started = Instant::now();
        let morph_output = morph.tick(&sensors, &feedback, &life.state, dt);
        morph_tick_microseconds.push(morph_started.elapsed().as_secs_f64() * 1_000_000.0);
        *morph_command_counts
            .entry(morph_output.command.as_wire().to_owned())
            .or_default() += 1;
        if previous_morph_command != MorphCommand::Idle
            && morph_output.command != previous_morph_command
        {
            morph_switches = morph_switches.saturating_add(1);
        }
        previous_morph_command = morph_output.command;
        let mut output = life.tick(&sensors, &feedback, dt);
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
        output.body_intent = resolved_intent;
        output.body_intent = ecology
            .resolve_intent(
                output.body_intent,
                output.selected_action,
                &sensors,
                &feedback,
                life.state.focus_mode,
                dt,
            )
            .body_intent;
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
        body.fixed_update(
            &life.state.genome,
            &output.body_intent,
            &sensors,
            dt.min(1.0 / 30.0),
        );
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
        "position_bounds": {
            "minimum": [minimum_position.x, minimum_position.y],
            "maximum": [maximum_position.x, maximum_position.y],
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
        },
    });
    println!("{}", serde_json::to_string_pretty(&summary)?);
    store.save_state(&portable)?;
    store.save_morph_brain(&morph_state)?;
    store.save_ecology_state(&ecology_state)?;
    if let Some(path) = &arguments.export_state {
        store.export_state(&portable, path)?;
    }
    Ok(())
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
    tuning_last_modified: Option<SystemTime>,
    was_sleeping: bool,
    visible_after_first_frame: bool,
    modifiers: ModifiersState,
    debug_logging: bool,
    debug_accumulator: f32,
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
    last_touch_voice: Option<Instant>,
    overlay_move_microseconds: f64,
    render_microseconds: f64,
    brain_tick_microseconds: f64,
    last_debug: Option<DebugState>,
    last_vita: Option<VitaOutput>,
    last_morph: MorphOutput,
    teach: TeachRecorder,
    physics_timings: TimingWindow,
    render_timings: TimingWindow,
    frame_gap_timings: TimingWindow,
    desktop_poll_timings: TimingWindow,
    camera_timings: TimingWindow,
    background_timings: TimingWindow,
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
        runtime.debug_accumulator += elapsed;
        runtime.fps_accumulator += elapsed;
        runtime.max_frame_gap_ms = runtime.max_frame_gap_ms.max(elapsed * 1_000.0);
        runtime.frame_gap_timings.observe(elapsed * 1_000.0);

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
            if petting_started {
                // Resolve any previous utterance before the mind chooses this
                // touch response. The freshly normalized sensor frame gives the
                // learner the actual contact context instead of the prior poll.
                apply_shared_feedback(runtime, FeedbackEvent::PettingStarted);
                enqueue_touch_voice(runtime, now);
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
            runtime.vita.observe(
                &runtime.sensors,
                &runtime.body.simulation.feedback,
                observation_dt,
            );
            let click_rhythm = runtime.vita.recent_click_rhythm();
            runtime.sensors.recent_click_rhythm =
                click_rhythm.map_or([0.0; 8], |rhythm| rhythm.intervals);
            runtime.ecology.set_click_rhythm(click_rhythm);
            let visual_target = runtime.vita.visual_attention_target();
            let visual_hue = visual_target
                .and_then(|target| {
                    runtime
                        .last_visual_sample
                        .map(|frame| frame.cell_at(target.position).hue)
                })
                .unwrap_or(0.0);
            runtime.ecology.set_visual_attention(
                visual_target.map(|target| target.position),
                visual_hue,
                visual_target.map_or(0.0, |target| target.score),
                visual_target.is_some_and(|target| target.explicit),
            );
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
            runtime.body.fixed_update(
                &runtime.life.state.genome,
                &runtime.intent,
                &runtime.sensors,
                body_dt,
            );
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
        while runtime.life_accumulator >= LIFE_DT {
            let tick_started = Instant::now();
            let morph_output = runtime.morph.tick(
                &runtime.sensors,
                &runtime.body.simulation.feedback,
                &runtime.life.state,
                LIFE_DT,
            );
            let mut output =
                runtime
                    .life
                    .tick(&runtime.sensors, &runtime.body.simulation.feedback, LIFE_DT);
            let (resolved_intent, vita_output) = runtime.vita.resolve_intent_with_morph(
                runtime.brain_mode,
                &runtime.life.state,
                &runtime.sensors,
                &runtime.body.simulation.feedback,
                output.body_intent,
                Some(morph_output),
                LIFE_DT,
            );
            output.body_intent = resolved_intent;
            let ecology_output = runtime.ecology.resolve_intent(
                output.body_intent,
                output.selected_action,
                &runtime.sensors,
                &runtime.body.simulation.feedback,
                runtime.life.state.focus_mode,
                LIFE_DT,
            );
            output.body_intent = ecology_output.body_intent;
            let ecology_vocal_trigger = ecology_output.vocal_trigger;
            preserve_navigation_during_material_drag(
                &runtime.intent,
                &mut output.body_intent,
                runtime.sensors.pet_dragged,
            );
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
                enqueue_ecology_voice(runtime, trigger);
            } else if let Some(request) = output.vocal_request
                && let Some(motif) = runtime
                    .life
                    .state
                    .vocal_motifs
                    .iter()
                    .find(|motif| motif.id == request.motif_id)
            {
                let accepted =
                    runtime
                        .audio
                        .enqueue(&runtime.life.state.genome.voice, motif, &request);
                if !accepted {
                    runtime.life.cancel_vocal_request(request.performance_seed);
                }
            }
            runtime.life_accumulator -= LIFE_DT;
        }
        if runtime.debug_accumulator >= 1.0 {
            runtime.debug_accumulator %= 1.0;
            if runtime.debug_logging
                && let Some(debug) = &runtime.last_debug
            {
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
                let saliency_target = runtime.vita.visual_attention_target();
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
                let mut debug_details = serde_json::json!({
                    "action": format!("{:?}", runtime.life.state.current_action),
                    "locomotion": format!("{:?}", runtime.intent.locomotion),
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
                    "liquid": liquid_debug,
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
                    "morph": morph_output_json(runtime.last_morph),
                    "intent_pose": format!("{:?}", runtime.intent.pose),
                    "gaze_mode": format!("{:?}", runtime.body.embodiment.pose.gaze_mode),
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
                    "ecology": {
                        "active_goal": ecology_debug.active_goal.map(|goal| format!("{goal:?}")),
                        "phase": ecology_debug.active_phase.map(|phase| format!("{phase:?}")),
                        "reason": format!("{:?}", ecology_debug.selected_reason),
                        "candidate_scores": ecology_scores,
                        "escape_confidence": active_ecology
                            .filter(|episode| episode.goal == EpisodeGoal::EscapePressure)
                            .map(|episode| episode.prediction_confidence),
                        "help_requested": runtime.ecology.last_vocal_trigger()
                            == Some(EcologyVocalTrigger::NeedHelp),
                        "saliency_target": saliency_target.map(|target| target.position.to_array()),
                        "chromatic_blend": ecology_visual.chromatic_blend,
                        "camouflage_blend": ecology_visual.camouflage_blend,
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
                let _ = self.store.append_event(&EventLogEntry {
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
        let mut body = match ProceduralBody::generate(&prepared.life.state.genome) {
            Ok(body) => body,
            Err(error) => {
                eprintln!("could not generate procedural body: {error}");
                event_loop.exit();
                return;
            }
        };
        if let Err(error) = load_migrate_apply_liquid_tuning(&self.store, &mut body) {
            eprintln!("could not load liquid tuning profile; using analytic jelly: {error}");
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
        let last_morph = prepared.morph.last_output();
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
            tuning_last_modified,
            was_sleeping: false,
            visible_after_first_frame: false,
            modifiers: ModifiersState::empty(),
            debug_logging: self.arguments.debug_log,
            debug_accumulator: 0.0,
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
            last_touch_voice: None,
            overlay_move_microseconds: 0.0,
            render_microseconds: 0.0,
            brain_tick_microseconds: 0.0,
            last_debug: None,
            last_vita: None,
            last_morph,
            teach: TeachRecorder::default(),
            physics_timings: TimingWindow::default(),
            render_timings: TimingWindow::default(),
            frame_gap_timings: TimingWindow::default(),
            desktop_poll_timings: TimingWindow::default(),
            camera_timings: TimingWindow::default(),
            background_timings: TimingWindow::default(),
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
        }
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
    runtime.vita.apply_feedback(&event);
    runtime.morph.apply_feedback(&event);
    runtime.life.apply_feedback(event);
}

fn morph_output_json(output: MorphOutput) -> serde_json::Value {
    serde_json::json!({
        "command": output.command.as_wire(),
        "command_rates": output.command_rates,
        "winner_rate": output.winner_rate,
        "confidence": output.confidence,
        "attention": format!("{:?}", output.attention),
        "valence": output.valence,
        "arousal": output.arousal,
        "conflict": output.conflict,
        "turn": output.turn,
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

fn enqueue_touch_voice(runtime: &mut PetRuntime, now: Instant) {
    if runtime
        .last_touch_voice
        .is_some_and(|previous| now.duration_since(previous) < Duration::from_millis(750))
    {
        return;
    }
    let Some(request) = runtime
        .life
        .request_vocalization(VocalTrigger::Touch, &runtime.sensors)
    else {
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
    let voice = runtime.life.state.genome.voice.clone();
    let accepted = runtime.audio.enqueue(&voice, &motif, &request);
    if accepted {
        runtime.last_touch_voice = Some(now);
    } else {
        runtime.life.cancel_vocal_request(request.performance_seed);
    }
}

fn enqueue_ecology_voice(runtime: &mut PetRuntime, trigger: EcologyVocalTrigger) {
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
    };
    let Some(request) = runtime.life.request_vocalization(trigger, &runtime.sensors) else {
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
    let voice = runtime.life.state.genome.voice.clone();
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
    let Some(authored) = store
        .load_liquid_tuning::<LiquidTuningProfile>()
        .map_err(|error| error.to_string())?
    else {
        return Ok(None);
    };
    let applied = authored
        .clone()
        .sanitized()
        .map_err(|error| error.to_string())?;
    body.apply_tuning_profile(applied.clone())
        .map_err(|error| error.to_string())?;
    if authored != applied {
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
         \n  --simulate-hours HOURS   Run an accelerated deterministic simulation\n\
         \n  --import-state PATH      Validate and import portable state\n\
         \n  --export-state PATH      Export portable state\n\
         \n  --data-dir PATH          Override the application data directory\n\
         \n  --reset-learning         Reset learned weights, habits, and memories\n\
         \n  --reset-pet              Start a new organism from --seed\n\
         \n  --evolve                 Trigger one bounded metamorphosis\n\
         \n  --focus-mode             Suppress unsolicited attention\n\
         \n  --brain-mode MODE        classic, morphic, fusion, morph-shadow, or morph-fusion\n\
         \n  --debug-log             Write live frame and embodiment diagnostics\n\
         \n  --no-audio               Disable the audio device\n"
    );
}

#[cfg(test)]
mod tests {
    use pet_body::{BodyRenderMode, MaterialVariant};

    use super::*;

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
            rhythm_intervals: [0.0; 8],
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
}
