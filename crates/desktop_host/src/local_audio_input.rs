//! Opt-in local microphone capture for teachable acoustic cues.
//!
//! CPAL's realtime callback only downmixes and attempts a bounded queue send.
//! Resampling, endpointing, feature extraction, enrollment, and DTW all run on
//! the owned worker thread. Disabling or dropping this type stops the stream.

use std::{
    collections::VecDeque,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{self, Receiver, SyncSender, TryRecvError, TrySendError},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use cpal::{
    Device, SampleFormat, SizedSample, Stream, StreamConfig,
    traits::{DeviceTrait, HostTrait, StreamTrait},
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    CueDecision, CueKind, CueModelV1, CueModelValidationError, CueTrainer, SegmentEvidence,
    TARGET_SAMPLE_RATE, TrainingCue, TrainingProgress, classify_segment, extract_features,
};

const AUDIO_QUEUE_CAPACITY: usize = 24;
const COMMAND_QUEUE_CAPACITY: usize = 32;
const PERCEPT_QUEUE_CAPACITY: usize = 128;
const MAX_CALLBACK_MONO_SAMPLES: usize = 8_192;
const ANALYSIS_BLOCK_SAMPLES: usize = 160;
const PRE_ROLL_BLOCKS: usize = 5;
const END_SILENCE_BLOCKS: usize = 16;
const MAX_SEGMENT_SAMPLES: usize = 40_000;
const RETRY_DELAY: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudioInputConfig {
    /// None selects CPAL's current default input device.
    pub device_name: Option<String>,
    /// A finite retry budget for initial failure or a disconnected device.
    pub retry_limit: u8,
}

impl Default for AudioInputConfig {
    fn default() -> Self {
        Self {
            device_name: None,
            retry_limit: 8,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AudioInputState {
    Disabled,
    Starting,
    Listening,
    Retrying,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AudioInputStatus {
    pub state: AudioInputState,
    pub enabled: bool,
    pub device_name: Option<String>,
    pub native_sample_rate: Option<u32>,
    pub native_channels: Option<u16>,
    pub rms: f32,
    pub noise_floor: f32,
    pub voice_activity: bool,
    pub dropped_audio_chunks: u64,
    pub retry_count: u8,
    pub last_error: Option<String>,
    pub training: Option<TrainingProgress>,
}

impl Default for AudioInputStatus {
    fn default() -> Self {
        Self {
            state: AudioInputState::Disabled,
            enabled: false,
            device_name: None,
            native_sample_rate: None,
            native_channels: None,
            rms: 0.0,
            noise_floor: 0.006,
            voice_activity: false,
            dropped_audio_chunks: 0,
            retry_count: 0,
            last_error: None,
            training: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct OutputReferenceFrame {
    pub rms: f32,
    pub active: bool,
}

impl Default for OutputReferenceFrame {
    fn default() -> Self {
        Self {
            rms: 0.0,
            active: false,
        }
    }
}

impl OutputReferenceFrame {
    fn sanitized(self) -> Self {
        Self {
            rms: if self.rms.is_finite() {
                self.rms.clamp(0.0, 1.0)
            } else {
                0.0
            },
            active: self.active,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AudioPercept {
    Levels {
        rms: f32,
        noise_floor: f32,
        voice_activity: bool,
    },
    Onset {
        rms: f32,
    },
    Segment {
        duration_ms: u16,
        voice_likeness: f32,
        contaminated: bool,
    },
    CueAccepted {
        decision: CueDecision,
    },
    CueRejected {
        decision: CueDecision,
    },
    TrainingProgress {
        progress: TrainingProgress,
    },
    TrainingComplete {
        cue: TrainingCue,
        model_ready: bool,
    },
    TrainingRejected {
        cue: TrainingCue,
        reason: String,
    },
}

#[derive(Debug, Error)]
pub enum LocalAudioError {
    #[error(transparent)]
    InvalidModel(#[from] CueModelValidationError),
    #[error("microphone worker is not running")]
    NotRunning,
    #[error("microphone command queue is full")]
    CommandQueueFull,
    #[error("microphone worker disconnected")]
    WorkerDisconnected,
    #[error("microphone worker thread could not be created: {0}")]
    ThreadSpawn(String),
}

enum WorkerCommand {
    BeginTraining(TrainingCue),
    CancelTraining,
    OutputReference(OutputReferenceFrame),
    Stop,
}

pub struct LocalAudioInput {
    config: AudioInputConfig,
    model: Arc<Mutex<CueModelV1>>,
    status: Arc<Mutex<AudioInputStatus>>,
    dirty: Arc<AtomicBool>,
    commands: Option<SyncSender<WorkerCommand>>,
    percepts: Option<Receiver<AudioPercept>>,
    worker: Option<JoinHandle<()>>,
}

impl LocalAudioInput {
    pub fn new(config: AudioInputConfig, model: CueModelV1) -> Result<Self, LocalAudioError> {
        model.validate()?;
        Ok(Self {
            config,
            model: Arc::new(Mutex::new(model)),
            status: Arc::new(Mutex::new(AudioInputStatus::default())),
            dirty: Arc::new(AtomicBool::new(false)),
            commands: None,
            percepts: None,
            worker: None,
        })
    }

    /// Starts an opt-in background capture worker. Device and permission errors
    /// are reported through `status`; creation itself never blocks on permission.
    pub fn start(&mut self) -> Result<(), LocalAudioError> {
        if self.worker.is_some() {
            return Ok(());
        }
        let (command_tx, command_rx) = mpsc::sync_channel(COMMAND_QUEUE_CAPACITY);
        let (percept_tx, percept_rx) = mpsc::sync_channel(PERCEPT_QUEUE_CAPACITY);
        let config = self.config.clone();
        let model = Arc::clone(&self.model);
        let status = Arc::clone(&self.status);
        let dirty = Arc::clone(&self.dirty);
        update_status(&status, |value| {
            value.enabled = true;
            value.state = AudioInputState::Starting;
            value.last_error = None;
            value.retry_count = 0;
        });
        let worker = thread::Builder::new()
            .name("pet2-local-hearing".to_owned())
            .spawn(move || run_worker(config, model, dirty, status, command_rx, percept_tx))
            .map_err(|error| {
                update_status(&self.status, |value| {
                    value.enabled = false;
                    value.state = AudioInputState::Failed;
                    value.last_error = Some(error.to_string());
                });
                LocalAudioError::ThreadSpawn(error.to_string())
            })?;
        self.commands = Some(command_tx);
        self.percepts = Some(percept_rx);
        self.worker = Some(worker);
        Ok(())
    }

    pub fn stop(&mut self) {
        if let Some(commands) = &self.commands {
            let _ = commands.send(WorkerCommand::Stop);
        }
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
        self.commands = None;
        self.percepts = None;
        update_status(&self.status, |value| {
            value.state = AudioInputState::Disabled;
            value.enabled = false;
            value.voice_activity = false;
            value.training = None;
        });
    }

    #[must_use]
    pub fn status(&self) -> AudioInputStatus {
        lock_recover(&self.status).clone()
    }

    pub fn set_model(&mut self, model: CueModelV1) -> Result<(), LocalAudioError> {
        model.validate()?;
        *lock_recover(&self.model) = model;
        Ok(())
    }

    #[must_use]
    pub fn model(&self) -> CueModelV1 {
        lock_recover(&self.model).clone()
    }

    pub fn begin_training(&self, cue: TrainingCue) -> Result<(), LocalAudioError> {
        self.send(WorkerCommand::BeginTraining(cue))
    }

    pub fn cancel_training(&self) -> Result<(), LocalAudioError> {
        self.send(WorkerCommand::CancelTraining)
    }

    #[must_use]
    pub fn training_progress(&self) -> Option<TrainingProgress> {
        lock_recover(&self.status).training
    }

    pub fn submit_output_reference(
        &self,
        reference: OutputReferenceFrame,
    ) -> Result<(), LocalAudioError> {
        self.send(WorkerCommand::OutputReference(reference.sanitized()))
    }

    #[must_use]
    pub fn drain_percepts(&self) -> Vec<AudioPercept> {
        let Some(percepts) = &self.percepts else {
            return Vec::new();
        };
        percepts.try_iter().collect()
    }

    /// Returns a validated model once after successful enrollment or Forget.
    /// Clear the dirty bit only after taking a clone so failed persistence can be
    /// retried by calling `set_model` or by retaining this returned snapshot.
    pub fn take_dirty_model_snapshot(&self) -> Option<CueModelV1> {
        if !self.dirty.swap(false, Ordering::AcqRel) {
            return None;
        }
        Some(self.model())
    }

    pub fn forget(&mut self) -> Result<(), LocalAudioError> {
        // Serialize deletion with the worker lifecycle. An asynchronous cancel
        // alone is insufficient: a segment already being finalized could publish
        // its candidate after this method clears the shared model. Joining first
        // guarantees no old trainer can resurrect deleted features.
        let restart = self.worker.is_some();
        if restart {
            self.stop();
        }
        *lock_recover(&self.model) = CueModelV1::new();
        self.dirty.store(true, Ordering::Release);
        if restart {
            self.start()?;
        }
        Ok(())
    }

    fn send(&self, command: WorkerCommand) -> Result<(), LocalAudioError> {
        let Some(commands) = &self.commands else {
            return Err(LocalAudioError::NotRunning);
        };
        match commands.try_send(command) {
            Ok(()) => Ok(()),
            Err(TrySendError::Full(_)) => Err(LocalAudioError::CommandQueueFull),
            Err(TrySendError::Disconnected(_)) => Err(LocalAudioError::WorkerDisconnected),
        }
    }
}

impl Drop for LocalAudioInput {
    fn drop(&mut self) {
        self.stop();
    }
}

#[must_use]
pub fn available_input_devices() -> Vec<String> {
    cpal::default_host()
        .input_devices()
        .map(|devices| devices.filter_map(|device| device.name().ok()).collect())
        .unwrap_or_default()
}

struct AudioChunk {
    samples: Vec<f32>,
    sample_rate: u32,
}

fn run_worker(
    config: AudioInputConfig,
    model: Arc<Mutex<CueModelV1>>,
    dirty: Arc<AtomicBool>,
    status: Arc<Mutex<AudioInputStatus>>,
    commands: Receiver<WorkerCommand>,
    percepts: SyncSender<AudioPercept>,
) {
    let (audio_tx, audio_rx) = mpsc::sync_channel(AUDIO_QUEUE_CAPACITY);
    let dropped = Arc::new(AtomicU64::new(0));
    let stream_error = Arc::new(Mutex::new(None::<String>));
    let mut processor = AudioProcessor::new(model, dirty, Arc::clone(&status), percepts);
    let mut retry_count = 0u8;
    let mut stopping = false;

    while !stopping {
        processor.handle_pending_commands(&commands, &mut stopping);
        if stopping {
            break;
        }
        update_status(&status, |value| {
            value.state = if retry_count == 0 {
                AudioInputState::Starting
            } else {
                AudioInputState::Retrying
            };
            value.retry_count = retry_count;
        });
        *lock_recover(&stream_error) = None;
        match create_input_stream(
            &config,
            audio_tx.clone(),
            Arc::clone(&dropped),
            Arc::clone(&stream_error),
            Arc::clone(&status),
        ) {
            Ok(stream) => {
                retry_count = 0;
                update_status(&status, |value| {
                    value.state = AudioInputState::Listening;
                    value.retry_count = 0;
                    value.last_error = None;
                });
                while !stopping {
                    processor.handle_pending_commands(&commands, &mut stopping);
                    if stopping {
                        break;
                    }
                    if let Some(error) = lock_recover(&stream_error).take() {
                        update_status(&status, |value| value.last_error = Some(error));
                        break;
                    }
                    match audio_rx.recv_timeout(Duration::from_millis(12)) {
                        Ok(chunk) => processor.process_chunk(chunk),
                        Err(mpsc::RecvTimeoutError::Timeout) => {}
                        Err(mpsc::RecvTimeoutError::Disconnected) => {
                            stopping = true;
                        }
                    }
                    update_status(&status, |value| {
                        value.dropped_audio_chunks = dropped.load(Ordering::Relaxed)
                    });
                }
                drop(stream);
            }
            Err(error) => update_status(&status, |value| value.last_error = Some(error)),
        }
        if stopping {
            break;
        }
        retry_count = retry_count.saturating_add(1);
        if retry_count > config.retry_limit {
            update_status(&status, |value| {
                value.state = AudioInputState::Failed;
                value.retry_count = retry_count;
            });
            while !stopping {
                match commands.recv_timeout(Duration::from_millis(50)) {
                    Ok(command) => processor.handle_command(command, &mut stopping),
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                    Err(mpsc::RecvTimeoutError::Disconnected) => stopping = true,
                }
            }
            break;
        }
        update_status(&status, |value| {
            value.state = AudioInputState::Retrying;
            value.retry_count = retry_count;
        });
        let deadline = Instant::now() + RETRY_DELAY;
        while !stopping && Instant::now() < deadline {
            match commands.recv_timeout(Duration::from_millis(50)) {
                Ok(command) => processor.handle_command(command, &mut stopping),
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => stopping = true,
            }
        }
    }
    update_status(&status, |value| {
        value.state = AudioInputState::Disabled;
        value.enabled = false;
        value.voice_activity = false;
        value.training = None;
    });
}

fn create_input_stream(
    config: &AudioInputConfig,
    audio_tx: SyncSender<AudioChunk>,
    dropped: Arc<AtomicU64>,
    stream_error: Arc<Mutex<Option<String>>>,
    status: Arc<Mutex<AudioInputStatus>>,
) -> Result<Stream, String> {
    let host = cpal::default_host();
    let device = if let Some(requested) = &config.device_name {
        host.input_devices()
            .map_err(|error| error.to_string())?
            .find(|device| device.name().ok().as_deref() == Some(requested.as_str()))
            .ok_or_else(|| format!("input device `{requested}` is unavailable"))?
    } else {
        host.default_input_device()
            .ok_or_else(|| "no default input device is available".to_owned())?
    };
    let device_name = device.name().unwrap_or_else(|_| "Unknown input".to_owned());
    let supported = device
        .default_input_config()
        .map_err(|error| error.to_string())?;
    let sample_format = supported.sample_format();
    let stream_config: StreamConfig = supported.into();
    update_status(&status, |value| {
        value.device_name = Some(device_name);
        value.native_sample_rate = Some(stream_config.sample_rate.0);
        value.native_channels = Some(stream_config.channels);
    });
    let stream = match sample_format {
        SampleFormat::I8 => {
            build_typed_stream::<i8>(&device, &stream_config, audio_tx, dropped, stream_error)
        }
        SampleFormat::I16 => {
            build_typed_stream::<i16>(&device, &stream_config, audio_tx, dropped, stream_error)
        }
        SampleFormat::I32 => {
            build_typed_stream::<i32>(&device, &stream_config, audio_tx, dropped, stream_error)
        }
        SampleFormat::I64 => {
            build_typed_stream::<i64>(&device, &stream_config, audio_tx, dropped, stream_error)
        }
        SampleFormat::U8 => {
            build_typed_stream::<u8>(&device, &stream_config, audio_tx, dropped, stream_error)
        }
        SampleFormat::U16 => {
            build_typed_stream::<u16>(&device, &stream_config, audio_tx, dropped, stream_error)
        }
        SampleFormat::U32 => {
            build_typed_stream::<u32>(&device, &stream_config, audio_tx, dropped, stream_error)
        }
        SampleFormat::U64 => {
            build_typed_stream::<u64>(&device, &stream_config, audio_tx, dropped, stream_error)
        }
        SampleFormat::F32 => {
            build_typed_stream::<f32>(&device, &stream_config, audio_tx, dropped, stream_error)
        }
        SampleFormat::F64 => {
            build_typed_stream::<f64>(&device, &stream_config, audio_tx, dropped, stream_error)
        }
        format => return Err(format!("unsupported input sample format {format}")),
    }?;
    stream.play().map_err(|error| error.to_string())?;
    Ok(stream)
}

trait InputSample: SizedSample + Send + 'static + Copy {
    fn to_f32(self) -> f32;
}

macro_rules! signed_sample {
    ($sample:ty) => {
        impl InputSample for $sample {
            fn to_f32(self) -> f32 {
                (self as f64 / <$sample>::MAX as f64) as f32
            }
        }
    };
}

macro_rules! unsigned_sample {
    ($sample:ty) => {
        impl InputSample for $sample {
            fn to_f32(self) -> f32 {
                (self as f64 / <$sample>::MAX as f64 * 2.0 - 1.0) as f32
            }
        }
    };
}

signed_sample!(i8);
signed_sample!(i16);
signed_sample!(i32);
signed_sample!(i64);
unsigned_sample!(u8);
unsigned_sample!(u16);
unsigned_sample!(u32);
unsigned_sample!(u64);

impl InputSample for f32 {
    fn to_f32(self) -> f32 {
        self.clamp(-1.0, 1.0)
    }
}

impl InputSample for f64 {
    fn to_f32(self) -> f32 {
        self.clamp(-1.0, 1.0) as f32
    }
}

fn build_typed_stream<T: InputSample>(
    device: &Device,
    config: &StreamConfig,
    audio_tx: SyncSender<AudioChunk>,
    dropped: Arc<AtomicU64>,
    stream_error: Arc<Mutex<Option<String>>>,
) -> Result<Stream, String> {
    let channels = usize::from(config.channels).max(1);
    let sample_rate = config.sample_rate.0;
    device
        .build_input_stream(
            config,
            move |data: &[T], _| {
                let frame_count = (data.len() / channels).min(MAX_CALLBACK_MONO_SAMPLES);
                let mut mono = Vec::with_capacity(frame_count);
                for frame in data.chunks_exact(channels).take(frame_count) {
                    let sample =
                        frame.iter().map(|value| value.to_f32()).sum::<f32>() / channels as f32;
                    mono.push(sample.clamp(-1.0, 1.0));
                }
                if !mono.is_empty()
                    && audio_tx
                        .try_send(AudioChunk {
                            samples: mono,
                            sample_rate,
                        })
                        .is_err()
                {
                    dropped.fetch_add(1, Ordering::Relaxed);
                }
            },
            move |error| {
                *lock_recover(&stream_error) = Some(error.to_string());
            },
            None,
        )
        .map_err(|error| error.to_string())
}

fn continuation_trainer(cue: TrainingCue, model: &CueModelV1) -> Option<CueTrainer> {
    let count = match cue {
        TrainingCue::Name => model.class(CueKind::Name).map_or(0, |c| c.examples.len()),
        TrainingCue::Quiet => model.class(CueKind::Quiet).map_or(0, |c| c.examples.len()),
        TrainingCue::Command(cue) => model.class(cue).map_or(0, |c| c.examples.len()),
        TrainingCue::Other => model.other_examples.len(),
    };
    if count < 40 {
        CueTrainer::begin(cue, model.clone()).ok()
    } else {
        None
    }
}

struct AudioProcessor {
    model: Arc<Mutex<CueModelV1>>,
    dirty: Arc<AtomicBool>,
    status: Arc<Mutex<AudioInputStatus>>,
    percepts: SyncSender<AudioPercept>,
    trainer: Option<CueTrainer>,
    output: OutputReferenceFrame,
    resampler: LinearResampler,
    pending: VecDeque<f32>,
    endpoint: EndpointDetector,
    last_level_percept: Instant,
}

impl AudioProcessor {
    fn new(
        model: Arc<Mutex<CueModelV1>>,
        dirty: Arc<AtomicBool>,
        status: Arc<Mutex<AudioInputStatus>>,
        percepts: SyncSender<AudioPercept>,
    ) -> Self {
        Self {
            model,
            dirty,
            status,
            percepts,
            trainer: None,
            output: OutputReferenceFrame::default(),
            resampler: LinearResampler::default(),
            pending: VecDeque::new(),
            endpoint: EndpointDetector::default(),
            last_level_percept: Instant::now(),
        }
    }

    fn handle_pending_commands(&mut self, commands: &Receiver<WorkerCommand>, stopping: &mut bool) {
        loop {
            match commands.try_recv() {
                Ok(command) => self.handle_command(command, stopping),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    *stopping = true;
                    break;
                }
            }
        }
    }

    fn handle_command(&mut self, command: WorkerCommand, stopping: &mut bool) {
        match command {
            WorkerCommand::BeginTraining(cue) => {
                self.endpoint = EndpointDetector::default();
                self.pending.clear();
                let base = lock_recover(&self.model).clone();
                match CueTrainer::begin(cue, base) {
                    Ok(trainer) => {
                        let progress = trainer.progress();
                        self.trainer = Some(trainer);
                        update_status(&self.status, |value| value.training = Some(progress));
                        send_percept(&self.percepts, AudioPercept::TrainingProgress { progress });
                    }
                    Err(error) => send_percept(
                        &self.percepts,
                        AudioPercept::TrainingRejected {
                            cue,
                            reason: error.to_string(),
                        },
                    ),
                }
            }
            WorkerCommand::CancelTraining => {
                self.trainer = None;
                update_status(&self.status, |value| value.training = None);
            }
            WorkerCommand::OutputReference(reference) => self.output = reference,
            WorkerCommand::Stop => *stopping = true,
        }
    }

    fn process_chunk(&mut self, chunk: AudioChunk) {
        let samples = self.resampler.process(&chunk.samples, chunk.sample_rate);
        self.pending.extend(samples);
        while self.pending.len() >= ANALYSIS_BLOCK_SAMPLES {
            let block: Vec<f32> = self.pending.drain(..ANALYSIS_BLOCK_SAMPLES).collect();
            let result = self.endpoint.push(&block, self.output);
            update_status(&self.status, |value| {
                value.rms = result.rms;
                value.noise_floor = result.noise_floor;
                value.voice_activity = result.voice_activity;
            });
            if result.onset {
                send_percept(&self.percepts, AudioPercept::Onset { rms: result.rms });
            }
            if self.last_level_percept.elapsed() >= Duration::from_millis(100) {
                send_percept(
                    &self.percepts,
                    AudioPercept::Levels {
                        rms: result.rms,
                        noise_floor: result.noise_floor,
                        voice_activity: result.voice_activity,
                    },
                );
                self.last_level_percept = Instant::now();
            }
            if let Some(segment) = result.segment {
                self.process_segment(segment);
            }
        }
    }

    fn process_segment(&mut self, segment: SegmentEvidence) {
        send_percept(
            &self.percepts,
            AudioPercept::Segment {
                duration_ms: segment.duration_ms,
                voice_likeness: segment.voice_likeness,
                contaminated: segment.output_active_fraction > 0.08,
            },
        );
        if self.trainer.is_some() {
            self.process_training_segment(segment);
            return;
        }
        let model = lock_recover(&self.model).clone();
        let decision = classify_segment(&model, &segment);
        let percept = if decision.accepted {
            AudioPercept::CueAccepted { decision }
        } else {
            AudioPercept::CueRejected { decision }
        };
        send_percept(&self.percepts, percept);
    }

    fn process_training_segment(&mut self, segment: SegmentEvidence) {
        let progress = self
            .trainer
            .as_ref()
            .expect("training existence checked")
            .progress();
        let require_voice = progress.cue != TrainingCue::Other;
        if segment.output_active_fraction > 0.08 {
            send_percept(
                &self.percepts,
                AudioPercept::TrainingRejected {
                    cue: progress.cue,
                    reason: "pet output was active; wait for silence and repeat the example"
                        .to_owned(),
                },
            );
            return;
        }
        let features = match extract_features(&segment.samples, require_voice) {
            Ok(features) => features,
            Err(error) => {
                send_percept(
                    &self.percepts,
                    AudioPercept::TrainingRejected {
                        cue: progress.cue,
                        reason: error.to_string(),
                    },
                );
                return;
            }
        };
        let trainer = self.trainer.as_mut().expect("training existence checked");
        if let Err(error) = trainer.accept_feature_segment(features) {
            send_percept(
                &self.percepts,
                AudioPercept::TrainingRejected {
                    cue: progress.cue,
                    reason: error.to_string(),
                },
            );
            return;
        }
        let progress = trainer.progress();
        update_status(&self.status, |value| value.training = Some(progress));
        send_percept(&self.percepts, AudioPercept::TrainingProgress { progress });
        if progress.accepted < progress.required {
            return;
        }
        let trainer = self.trainer.take().expect("training existence checked");
        match trainer.clone().finalize() {
            Ok(candidate) => {
                let model_ready = match progress.cue {
                    TrainingCue::Name => candidate.is_ready(CueKind::Name),
                    TrainingCue::Quiet => candidate.is_ready(CueKind::Quiet),
                    TrainingCue::Command(cue) => candidate.is_ready(cue),
                    TrainingCue::Other => {
                        CueKind::ALL.into_iter().any(|cue| candidate.is_ready(cue))
                    }
                };
                self.trainer = continuation_trainer(progress.cue, &candidate);
                *lock_recover(&self.model) = candidate;
                self.dirty.store(true, Ordering::Release);
                update_status(&self.status, |value| {
                    value.training = self.trainer.as_ref().map(CueTrainer::progress)
                });
                send_percept(
                    &self.percepts,
                    AudioPercept::TrainingComplete {
                        cue: progress.cue,
                        model_ready,
                    },
                );
            }
            Err(error) => {
                let mut trainer = trainer;
                trainer.retry_oldest();
                update_status(&self.status, |value| {
                    value.training = Some(trainer.progress())
                });
                self.trainer = Some(trainer);
                send_percept(
                    &self.percepts,
                    AudioPercept::TrainingRejected {
                        cue: progress.cue,
                        reason: error.to_string(),
                    },
                );
            }
        }
    }
}

fn send_percept(sender: &SyncSender<AudioPercept>, percept: AudioPercept) {
    let _ = sender.try_send(percept);
}

#[derive(Default)]
struct LinearResampler {
    input_rate: u32,
    source: Vec<f32>,
    position: f64,
}

impl LinearResampler {
    fn process(&mut self, input: &[f32], input_rate: u32) -> Vec<f32> {
        if input_rate == 0 || input.is_empty() {
            return Vec::new();
        }
        if input_rate == TARGET_SAMPLE_RATE {
            self.input_rate = input_rate;
            self.source.clear();
            self.position = 0.0;
            return input.to_vec();
        }
        if self.input_rate != input_rate {
            self.input_rate = input_rate;
            self.source.clear();
            self.position = 0.0;
        }
        self.source.extend_from_slice(input);
        let step = input_rate as f64 / TARGET_SAMPLE_RATE as f64;
        let mut output = Vec::with_capacity(
            ((self.source.len() as f64 - self.position).max(0.0) / step) as usize + 1,
        );
        while self.position + 1.0 < self.source.len() as f64 {
            let index = self.position.floor() as usize;
            let fraction = (self.position - index as f64) as f32;
            output.push(self.source[index] * (1.0 - fraction) + self.source[index + 1] * fraction);
            self.position += step;
        }
        // `position` may advance just beyond this chunk. Subtract only samples
        // actually removed; the remaining fractional/integer offset carries the
        // next requested source position across arbitrary callback boundaries.
        let drained = (self.position.floor() as usize).min(self.source.len());
        if drained > 0 {
            self.source.drain(..drained);
            self.position -= drained as f64;
        }
        output
    }
}

#[derive(Default)]
struct EndpointDetector {
    noise_floor: f32,
    pre_roll: VecDeque<Vec<f32>>,
    active: bool,
    samples: Vec<f32>,
    active_frames: usize,
    voice_frames: usize,
    output_active_frames: usize,
    quiet_frames: usize,
    onset_rms: f32,
    onset_output_rms: f32,
}

struct EndpointResult {
    rms: f32,
    noise_floor: f32,
    voice_activity: bool,
    onset: bool,
    segment: Option<SegmentEvidence>,
}

impl EndpointDetector {
    fn push(&mut self, block: &[f32], output: OutputReferenceFrame) -> EndpointResult {
        if self.noise_floor <= 0.0 {
            self.noise_floor = 0.006;
        }
        let rms = (block.iter().map(|sample| sample * sample).sum::<f32>()
            / block.len().max(1) as f32)
            .sqrt();
        let crossings = block
            .windows(2)
            .filter(|pair| pair[0].is_sign_positive() != pair[1].is_sign_positive())
            .count() as f32
            / block.len().saturating_sub(1).max(1) as f32;
        let threshold = (self.noise_floor * 2.8).max(0.003);
        let voice_like = rms > threshold && (0.008..=0.40).contains(&crossings);
        let mut onset = false;
        let mut segment = None;

        if !self.active {
            if rms < threshold {
                let bounded = rms.min(self.noise_floor * 1.7 + 0.001);
                self.noise_floor = (self.noise_floor * 0.985 + bounded * 0.015).clamp(0.0005, 0.12);
            }
            self.pre_roll.push_back(block.to_vec());
            while self.pre_roll.len() > PRE_ROLL_BLOCKS {
                self.pre_roll.pop_front();
            }
            if rms > threshold {
                self.active = true;
                onset = true;
                self.samples.clear();
                for previous in &self.pre_roll {
                    self.samples.extend_from_slice(previous);
                }
                self.active_frames = 1;
                self.voice_frames = if voice_like { 1 } else { 0 };
                self.output_active_frames = if output.active { 1 } else { 0 };
                self.quiet_frames = 0;
                self.onset_rms = rms;
                self.onset_output_rms = output.rms;
            }
        } else {
            self.samples.extend_from_slice(block);
            self.active_frames += 1;
            self.voice_frames += if voice_like { 1 } else { 0 };
            self.output_active_frames += if output.active { 1 } else { 0 };
            if rms < threshold * 0.72 {
                self.quiet_frames += 1;
            } else {
                self.quiet_frames = 0;
            }
            if self.quiet_frames >= END_SILENCE_BLOCKS || self.samples.len() >= MAX_SEGMENT_SAMPLES
            {
                let trailing = if self.quiet_frames >= END_SILENCE_BLOCKS {
                    (END_SILENCE_BLOCKS - 4) * ANALYSIS_BLOCK_SAMPLES
                } else {
                    0
                };
                self.samples
                    .truncate(self.samples.len().saturating_sub(trailing));
                let duration_ms = ((self.samples.len() as u64 * 1_000) / TARGET_SAMPLE_RATE as u64)
                    .min(u16::MAX as u64) as u16;
                segment = Some(SegmentEvidence {
                    samples: std::mem::take(&mut self.samples),
                    duration_ms,
                    voice_likeness: self.voice_frames as f32 / self.active_frames.max(1) as f32,
                    output_active_fraction: self.output_active_frames as f32
                        / self.active_frames.max(1) as f32,
                    onset_rms: self.onset_rms,
                    onset_output_rms: self.onset_output_rms,
                });
                self.active = false;
                self.pre_roll.clear();
                self.active_frames = 0;
                self.voice_frames = 0;
                self.output_active_frames = 0;
                self.quiet_frames = 0;
            }
        }
        EndpointResult {
            rms,
            noise_floor: self.noise_floor,
            voice_activity: self.active && voice_like,
            onset,
            segment,
        }
    }
}

fn update_status(
    status: &Arc<Mutex<AudioInputStatus>>,
    update: impl FnOnce(&mut AudioInputStatus),
) {
    update(&mut lock_recover(status));
}

fn lock_recover<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CueTemplateV1, SpectralFrameV1};

    #[test]
    fn name_and_negative_recording_continue_across_batches_until_forty() {
        for cue in [TrainingCue::Name, TrainingCue::Other] {
            let mut model = CueModelV1::new();
            for batch in 0..8 {
                let mut trainer = continuation_trainer(cue, &model).expect("record next batch");
                for _ in 0..5 {
                    trainer
                        .accept_feature_segment(CueTemplateV1 {
                            frames: vec![SpectralFrameV1 { values: [0.2; 24] }; 4],
                            duration_ms: 300,
                        })
                        .unwrap();
                }
                model = trainer.finalize().unwrap();
                assert_eq!(continuation_trainer(cue, &model).is_some(), batch < 7);
            }
        }
    }

    #[test]
    fn integer_and_float_sample_formats_map_to_unit_float() {
        assert!((InputSample::to_f32(i16::MIN) + 1.0).abs() < 0.0001);
        assert_eq!(InputSample::to_f32(0i16), 0.0);
        assert_eq!(InputSample::to_f32(i16::MAX), 1.0);
        assert!((InputSample::to_f32(0u16) + 1.0).abs() < 0.0001);
        assert!((InputSample::to_f32(u16::MAX) - 1.0).abs() < 0.0001);
        assert_eq!(InputSample::to_f32(2.0f32), 1.0);
        assert_eq!(InputSample::to_f32(-2.0f64), -1.0);
    }

    #[test]
    fn streaming_resampler_preserves_duration_and_frequency_shape() {
        let source: Vec<f32> = (0..4_800)
            .map(|index| (std::f32::consts::TAU * 440.0 * index as f32 / 48_000.0).sin())
            .collect();
        let mut resampler = LinearResampler::default();
        let mut result = Vec::new();
        for chunk in source.chunks(317) {
            result.extend(resampler.process(chunk, 48_000));
        }
        assert!((result.len() as isize - 1_600).abs() <= 2);
        let crossings = result
            .windows(2)
            .filter(|pair| pair[0].is_sign_positive() != pair[1].is_sign_positive())
            .count();
        assert!((crossings as isize - 88).abs() <= 3);
    }

    #[test]
    fn endpoint_ignores_silence_and_rejects_a_short_clap_as_a_cue() {
        let mut endpoint = EndpointDetector::default();
        for _ in 0..40 {
            let result = endpoint.push(
                &[0.0; ANALYSIS_BLOCK_SAMPLES],
                OutputReferenceFrame::default(),
            );
            assert!(result.segment.is_none());
        }
        let mut impulse = [0.0; ANALYSIS_BLOCK_SAMPLES];
        impulse[40] = 1.0;
        assert!(
            endpoint
                .push(&impulse, OutputReferenceFrame::default())
                .onset
        );
        let mut emitted = None;
        for _ in 0..END_SILENCE_BLOCKS + 2 {
            let result = endpoint.push(
                &[0.0; ANALYSIS_BLOCK_SAMPLES],
                OutputReferenceFrame::default(),
            );
            emitted = emitted.or(result.segment);
        }
        let segment = emitted.expect("endpoint should close the impulse");
        assert!(extract_features(&segment.samples, true).is_err());
    }

    #[test]
    fn forget_joins_an_inflight_worker_before_clearing_the_model() {
        let learned = CueModelV1 {
            commands: Vec::new(),
            schema_version: crate::CUE_MODEL_SCHEMA_VERSION,
            name: None,
            quiet: None,
            other_examples: vec![CueTemplateV1 {
                frames: vec![SpectralFrameV1 { values: [0.0; 24] }; 2],
                duration_ms: 120,
            }],
        };
        learned.validate().unwrap();
        let mut input = LocalAudioInput::new(
            AudioInputConfig {
                retry_limit: 0,
                ..AudioInputConfig::default()
            },
            learned.clone(),
        )
        .unwrap();

        // Stand in for a worker that finishes an enrollment exactly as Forget
        // arrives. It publishes the old candidate after observing Stop.
        let shared_model = Arc::clone(&input.model);
        let (command_tx, command_rx) = mpsc::sync_channel(1);
        let (_percept_tx, percept_rx) = mpsc::sync_channel(1);
        input.commands = Some(command_tx);
        input.percepts = Some(percept_rx);
        input.worker = Some(thread::spawn(move || {
            let _ = command_rx.recv();
            *lock_recover(&shared_model) = learned;
        }));

        input.forget().unwrap();
        assert_eq!(input.model(), CueModelV1::new());
        assert_eq!(input.take_dirty_model_snapshot(), Some(CueModelV1::new()));
        input.stop();
    }
}
