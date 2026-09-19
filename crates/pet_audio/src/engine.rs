use std::{
    fs::File,
    io::{self, Write},
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU32, Ordering},
    },
};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use lifecore::{BodyVoiceFrame, VocalMotif, VocalRequest, VoiceGenome};
use thiserror::Error;

use crate::{
    AudioCallbackLevels, AudioVisualBridge, AudioVisualFeedback, BodyVoiceBridge, COMMAND_CAPACITY,
    SpscRing, SynthVoice, VoiceCommand, VoiceDiagnostics, global_body_voice_bridge,
    global_visual_bridge,
};

const ERROR_CAPACITY: usize = 8;
static MASTER_OUTPUT_GAIN: AtomicU32 = AtomicU32::new(1.0_f32.to_bits());

struct OutputGain {
    current: f32,
    step: f32,
}

impl OutputGain {
    fn new(sample_rate: u32) -> Self {
        Self {
            current: 1.0,
            step: 1.0 / (0.05 * sample_rate.max(1) as f32),
        }
    }

    fn apply(&mut self, stereo: [f32; 2], target: f32) -> [f32; 2] {
        self.current += (target - self.current).clamp(-self.step, self.step);
        stereo.map(|sample| sample * self.current)
    }
}

#[test]
fn sleep_breath_uses_real_queue_renderer_without_phonation() {
    let voice = lifecore::Genome::from_seed(42).voice;
    let command = prepare_nonphonated(
        &voice,
        crate::NonPhonatedRequest {
            kind: crate::NonPhonatedKind::SleepBreath,
            intensity: 0.25,
            pan: 0.0,
            seed: 414,
        },
    );
    let render = || render_prepared_command(command, 24_000, 1, OfflineSampleFormat::F32);
    let first = render();
    assert_eq!(first, render());
    let OfflinePcm::F32(samples) = first.pcm else {
        panic!("float PCM expected")
    };
    assert!(samples.iter().all(|s| s.is_finite() && s.abs() < 0.02));
    assert!(samples.iter().any(|s| s.abs() > 0.000001));
    let rms = (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt();
    assert!((0.0002..0.003).contains(&rms), "quiet breath RMS {rms}");
    assert!(samples[29_000..].iter().all(|s| *s == 0.0));
    assert_eq!(command.motif_id, 0);
    let queue = Arc::new(SpscRing::new());
    queue.push(command).unwrap();
    let feedback = Arc::new(AudioVisualBridge::default());
    let mut synth = SynthVoice::with_bridges(
        queue,
        24_000,
        Arc::clone(&feedback),
        Arc::new(BodyVoiceBridge::default()),
    );
    for _ in 0..12_000 {
        let _ = synth.next_stereo_frame();
    }
    assert!(feedback.snapshot().active);
    assert_eq!(feedback.snapshot().glottal_openness, 0.0);
    assert_eq!(feedback.snapshot().shout, 0.0);
    for _ in 0..18_000 {
        let _ = synth.next_stereo_frame();
    }
    assert!(!feedback.snapshot().active);
}

/// Prepare an airflow-only command for the existing bounded output queue.
pub fn prepare_nonphonated(
    voice: &VoiceGenome,
    request: crate::NonPhonatedRequest,
) -> VoiceCommand {
    let motif = lifecore::generate_initial_motifs(voice).remove(0);
    let vocal = VocalRequest {
        motif_id: 0,
        performance_seed: request.seed,
        gain: 0.12,
        pan: request.pan,
        pitch_scale: 1.0,
        tempo_scale: 1.0,
        stress: 0.0,
        purr: false,
        gesture: lifecore::VoiceGesture::ReliefExhale,
        priority: 0,
        style: lifecore::VocalStyle::ContentMurmur,
        valence: 0.0,
        arousal: 0.0,
        fatigue: 0.0,
        confidence: 1.0,
        attachment: 0.0,
        rhythm_intervals: [0.0; 8],
        phenotype: Default::default(),
    };
    let mut command = VoiceCommand::prepare(voice, &motif, &vocal);
    command.motif_id = 0; // No learned vocal motif receives breath-event credit.
    command.nonphonated = Some(request);
    command.syllable_count = 1;
    command.syllables[0].duration_ms = 1300.0;
    command.syllables[0].gap_after_ms = 0.0;
    command
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeSampleFormat {
    F32,
    I16,
    U16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SelectedOutputConfig {
    pub sample_rate: u32,
    pub channels: u16,
    pub sample_format: RuntimeSampleFormat,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputConfigCandidate {
    pub min_sample_rate: u32,
    pub max_sample_rate: u32,
    pub channels: u16,
    pub sample_format: RuntimeSampleFormat,
}

#[must_use]
pub fn choose_output_config(candidates: &[OutputConfigCandidate]) -> Option<SelectedOutputConfig> {
    candidates
        .iter()
        .filter(|candidate| {
            candidate.channels > 0
                && candidate.min_sample_rate > 0
                && candidate.max_sample_rate >= candidate.min_sample_rate
        })
        .map(|candidate| {
            let sample_rate =
                48_000_u32.clamp(candidate.min_sample_rate, candidate.max_sample_rate);
            let channel_penalty = u64::from(candidate.channels.abs_diff(2)) * 10_000;
            let rate_penalty = u64::from(sample_rate.abs_diff(48_000));
            let format_penalty = match candidate.sample_format {
                RuntimeSampleFormat::F32 => 0,
                RuntimeSampleFormat::I16 => 1,
                RuntimeSampleFormat::U16 => 2,
            };
            (
                channel_penalty + rate_penalty + format_penalty,
                SelectedOutputConfig {
                    sample_rate,
                    channels: candidate.channels,
                    sample_format: candidate.sample_format,
                },
            )
        })
        .min_by_key(|(score, _)| *score)
        .map(|(_, config)| config)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioRuntimeEvent {
    StreamError,
}

#[derive(Debug, Error)]
pub enum AudioError {
    #[error("no default audio output device is available")]
    NoOutputDevice,
    #[error("could not enumerate audio output devices: {0}")]
    OutputDevices(#[from] cpal::DevicesError),
    #[error("audio output device is unavailable: {0}")]
    OutputDeviceUnavailable(String),
    #[error("could not enumerate output formats: {0}")]
    SupportedConfigs(#[from] cpal::SupportedStreamConfigsError),
    #[error("the output device exposes none of f32, i16, or u16")]
    NoSupportedConfig,
    #[error("could not create the audio stream: {0}")]
    BuildStream(#[from] cpal::BuildStreamError),
    #[error("could not start the audio stream: {0}")]
    PlayStream(#[from] cpal::PlayStreamError),
    #[error("the audio command ring is full")]
    CommandQueueFull,
}

pub struct AudioEngine {
    _stream: cpal::Stream,
    selected: SelectedOutputConfig,
    device_name: String,
    commands: Arc<SpscRing<VoiceCommand, COMMAND_CAPACITY>>,
    errors: Arc<SpscRing<AudioRuntimeEvent, ERROR_CAPACITY>>,
    feedback: Arc<AudioVisualBridge>,
    phrase_variation: Mutex<crate::PhraseVariationState>,
}

impl AudioEngine {
    /// Immediate process-local output boundary; active phrases and breath share
    /// the same 50 ms click-free ramp. Never raises the authored output level.
    pub fn set_master_gain(gain: f32) {
        let gain = if gain.is_finite() {
            gain.clamp(0.0, 1.0)
        } else {
            0.0
        };
        MASTER_OUTPUT_GAIN.store(gain.to_bits(), Ordering::Relaxed);
    }

    #[must_use]
    pub fn master_gain() -> f32 {
        f32::from_bits(MASTER_OUTPUT_GAIN.load(Ordering::Relaxed))
    }
    /// Caller retains quiet-mode, event admission and cooldown ownership.
    pub fn enqueue_nonphonated(
        &self,
        voice: &VoiceGenome,
        request: crate::NonPhonatedRequest,
    ) -> Result<(), AudioError> {
        self.commands
            .push(prepare_nonphonated(voice, request))
            .map_err(|_| AudioError::CommandQueueFull)
    }
    pub fn default_output_device_name() -> Result<String, AudioError> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or(AudioError::NoOutputDevice)?;
        Ok(device
            .name()
            .unwrap_or_else(|_| "unknown output device".into()))
    }

    pub fn try_start() -> Result<Self, AudioError> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or(AudioError::NoOutputDevice)?;
        Self::start_on_device(device)
    }

    /// Enumerates output names for a transient preview selector. Callers own any
    /// persistence policy; Pet Lab intentionally keeps this session-only.
    pub fn output_device_names() -> Result<Vec<String>, AudioError> {
        let host = cpal::default_host();
        let mut names = host
            .output_devices()?
            .filter_map(|device| device.name().ok())
            .collect::<Vec<_>>();
        names.sort_unstable();
        names.dedup();
        Ok(names)
    }

    pub fn try_start_on_device_name(name: &str) -> Result<Self, AudioError> {
        let host = cpal::default_host();
        let requested = name.trim();
        let device = host
            .output_devices()?
            .find(|device| device.name().is_ok_and(|candidate| candidate == requested))
            .ok_or_else(|| AudioError::OutputDeviceUnavailable(requested.to_owned()))?;
        Self::start_on_device(device)
    }

    fn start_on_device(device: cpal::Device) -> Result<Self, AudioError> {
        let device_name = device
            .name()
            .unwrap_or_else(|_| "unknown output device".into());
        let supported: Vec<_> = device.supported_output_configs()?.collect();
        let candidates: Vec<_> = supported.iter().filter_map(candidate_from_cpal).collect();
        let selected = choose_output_config(&candidates).ok_or(AudioError::NoSupportedConfig)?;
        let config = cpal::StreamConfig {
            channels: selected.channels,
            sample_rate: cpal::SampleRate(selected.sample_rate),
            buffer_size: cpal::BufferSize::Default,
        };
        let commands = Arc::new(SpscRing::new());
        let errors = Arc::new(SpscRing::new());
        let feedback = global_visual_bridge();
        let body_bridge = global_body_voice_bridge();
        let errors_for_callback = Arc::clone(&errors);
        let error_callback = move |_error: cpal::StreamError| {
            let _ = errors_for_callback.push(AudioRuntimeEvent::StreamError);
        };
        let stream = match selected.sample_format {
            RuntimeSampleFormat::F32 => {
                let mut synth = SynthVoice::with_bridges(
                    Arc::clone(&commands),
                    selected.sample_rate,
                    Arc::clone(&feedback),
                    Arc::clone(&body_bridge),
                );
                let channels = usize::from(selected.channels);
                let feedback_for_levels = Arc::clone(&feedback);
                let mut gain = OutputGain::new(selected.sample_rate);
                device.build_output_stream(
                    &config,
                    move |output: &mut [f32], _| {
                        fill_f32(
                            &mut synth,
                            output,
                            channels,
                            &feedback_for_levels,
                            &mut gain,
                        );
                    },
                    error_callback,
                    None,
                )?
            }
            RuntimeSampleFormat::I16 => {
                let mut synth = SynthVoice::with_bridges(
                    Arc::clone(&commands),
                    selected.sample_rate,
                    Arc::clone(&feedback),
                    Arc::clone(&body_bridge),
                );
                let channels = usize::from(selected.channels);
                let feedback_for_levels = Arc::clone(&feedback);
                let mut gain = OutputGain::new(selected.sample_rate);
                device.build_output_stream(
                    &config,
                    move |output: &mut [i16], _| {
                        fill_i16(
                            &mut synth,
                            output,
                            channels,
                            &feedback_for_levels,
                            &mut gain,
                        );
                    },
                    error_callback,
                    None,
                )?
            }
            RuntimeSampleFormat::U16 => {
                let mut synth = SynthVoice::with_bridges(
                    Arc::clone(&commands),
                    selected.sample_rate,
                    Arc::clone(&feedback),
                    Arc::clone(&body_bridge),
                );
                let channels = usize::from(selected.channels);
                let feedback_for_levels = Arc::clone(&feedback);
                let mut gain = OutputGain::new(selected.sample_rate);
                device.build_output_stream(
                    &config,
                    move |output: &mut [u16], _| {
                        fill_u16(
                            &mut synth,
                            output,
                            channels,
                            &feedback_for_levels,
                            &mut gain,
                        );
                    },
                    error_callback,
                    None,
                )?
            }
        };
        stream.play()?;
        Ok(Self {
            _stream: stream,
            selected,
            device_name,
            commands,
            errors,
            feedback,
            phrase_variation: Mutex::new(crate::PhraseVariationState::default()),
        })
    }

    pub fn enqueue(
        &self,
        voice: &VoiceGenome,
        motif: &VocalMotif,
        request: &VocalRequest,
    ) -> Result<(), AudioError> {
        // This lock/allocation is exclusively on the producer thread, never in
        // the audio callback. Failed queue admission does not consume history.
        let mut state = self
            .phrase_variation
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let mut next = state.clone();
        let command = next.prepare(voice, motif, request);
        self.commands
            .push(command)
            .map_err(|_| AudioError::CommandQueueFull)?;
        *state = next;
        Ok(())
    }

    #[must_use]
    pub fn selected_config(&self) -> SelectedOutputConfig {
        self.selected
    }

    #[must_use]
    pub fn device_name(&self) -> &str {
        &self.device_name
    }

    #[must_use]
    pub fn callback_levels(&self) -> AudioCallbackLevels {
        self.feedback.levels()
    }

    pub fn clear_visual_feedback(&self) {
        self.feedback.clear();
    }

    #[must_use]
    pub fn visual_feedback(&self) -> AudioVisualFeedback {
        self.feedback.snapshot()
    }

    pub fn poll_runtime_event(&self) -> Option<AudioRuntimeEvent> {
        self.errors.pop()
    }

    pub fn poll_started_request(&self) -> Option<u64> {
        self.feedback.pop_started_request()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OfflineSampleFormat {
    F32,
    I16,
    U16,
}

#[derive(Debug, Clone, PartialEq)]
pub enum OfflinePcm {
    F32(Vec<f32>),
    I16(Vec<i16>),
    U16(Vec<u16>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct OfflineRender {
    pub pcm: OfflinePcm,
    pub diagnostics: VoiceDiagnostics,
}

impl OfflinePcm {
    #[must_use]
    pub fn sample_count(&self) -> usize {
        match self {
            Self::F32(samples) => samples.len(),
            Self::I16(samples) => samples.len(),
            Self::U16(samples) => samples.len(),
        }
    }
}

#[must_use]
pub fn render_motif(
    voice: &VoiceGenome,
    motif: &VocalMotif,
    request: &VocalRequest,
    sample_rate: u32,
    channels: u16,
    format: OfflineSampleFormat,
) -> OfflinePcm {
    render_motif_with_body_timeline(
        voice,
        motif,
        request,
        &[],
        100,
        sample_rate,
        channels,
        format,
    )
    .pcm
}

/// Deterministically renders one call while publishing a scripted body state.
///
/// `body_timeline` is sampled at `body_frame_rate_hz`; an empty slice is the
/// neutral one-component body. This is the sole offline path used by Voice Lab.
#[must_use]
#[allow(clippy::too_many_arguments)]
pub fn render_motif_with_body_timeline(
    voice: &VoiceGenome,
    motif: &VocalMotif,
    request: &VocalRequest,
    body_timeline: &[BodyVoiceFrame],
    body_frame_rate_hz: u32,
    sample_rate: u32,
    channels: u16,
    format: OfflineSampleFormat,
) -> OfflineRender {
    let command = VoiceCommand::prepare(voice, motif, request);
    render_prepared_with_body_timeline(
        command,
        body_timeline,
        body_frame_rate_hz,
        sample_rate,
        channels,
        format,
    )
}

/// Renders the exact prepared command accepted by production enqueue, including
/// structural phrase variation. Uses the same SynthVoice/callback sample path.
#[must_use]
pub fn render_prepared_command(
    command: VoiceCommand,
    sample_rate: u32,
    channels: u16,
    format: OfflineSampleFormat,
) -> OfflineRender {
    render_prepared_with_body_timeline(command, &[], 100, sample_rate, channels, format)
}

fn render_prepared_with_body_timeline(
    command: VoiceCommand,
    body_timeline: &[BodyVoiceFrame],
    body_frame_rate_hz: u32,
    sample_rate: u32,
    channels: u16,
    format: OfflineSampleFormat,
) -> OfflineRender {
    let commands = Arc::new(SpscRing::new());
    commands
        .push(command)
        .expect("fresh offline command ring accepts one command");
    let feedback = Arc::new(AudioVisualBridge::default());
    let body_bridge = Arc::new(BodyVoiceBridge::default());
    let mut synth =
        SynthVoice::with_bridges(commands, sample_rate, feedback, Arc::clone(&body_bridge));
    let mut published_body_index = usize::MAX;
    if let Some(first) = body_timeline.first() {
        body_bridge.publish(*first);
        published_body_index = 0;
    }
    synth.begin_callback();
    let channels = usize::from(channels.max(1));
    let frame_count = command.total_frames(sample_rate);
    let mut f32_samples = Vec::with_capacity(frame_count * channels);
    for frame_index in 0..frame_count {
        if !body_timeline.is_empty() {
            let body_index = ((frame_index as u64 * u64::from(body_frame_rate_hz.max(1)))
                / u64::from(sample_rate.max(1))) as usize;
            let body_index = body_index.min(body_timeline.len() - 1);
            if body_index != published_body_index {
                body_bridge.publish(body_timeline[body_index]);
                synth.begin_callback();
                published_body_index = body_index;
            }
        }
        write_frame_f32(synth.next_stereo_frame(), &mut f32_samples, channels);
    }
    let diagnostics = synth.diagnostics();
    let pcm = match format {
        OfflineSampleFormat::F32 => OfflinePcm::F32(f32_samples),
        OfflineSampleFormat::I16 => OfflinePcm::I16(f32_samples.into_iter().map(to_i16).collect()),
        OfflineSampleFormat::U16 => OfflinePcm::U16(f32_samples.into_iter().map(to_u16).collect()),
    };
    OfflineRender { pcm, diagnostics }
}

pub fn export_debug_wav(
    path: &Path,
    samples: &[f32],
    sample_rate: u32,
    channels: u16,
) -> io::Result<()> {
    let mut file = File::create(path)?;
    let data_size = samples.len() as u32 * 2;
    file.write_all(b"RIFF")?;
    file.write_all(&(36 + data_size).to_le_bytes())?;
    file.write_all(b"WAVEfmt ")?;
    file.write_all(&16_u32.to_le_bytes())?;
    file.write_all(&1_u16.to_le_bytes())?;
    file.write_all(&channels.to_le_bytes())?;
    file.write_all(&sample_rate.to_le_bytes())?;
    let byte_rate = sample_rate * u32::from(channels) * 2;
    file.write_all(&byte_rate.to_le_bytes())?;
    file.write_all(&(channels * 2).to_le_bytes())?;
    file.write_all(&16_u16.to_le_bytes())?;
    file.write_all(b"data")?;
    file.write_all(&data_size.to_le_bytes())?;
    for sample in samples {
        file.write_all(&to_i16(*sample).to_le_bytes())?;
    }
    file.flush()
}

fn fill_f32(
    synth: &mut SynthVoice,
    output: &mut [f32],
    channels: usize,
    feedback: &AudioVisualBridge,
    gain: &mut OutputGain,
) {
    synth.begin_callback();
    let target_gain = AudioEngine::master_gain();
    let mut energy = 0.0;
    let mut peak = 0.0_f32;
    let mut count = 0.0_f32;
    for frame in output.chunks_mut(channels.max(1)) {
        let stereo = gain.apply(synth.next_stereo_frame(), target_gain);
        accumulate_levels(stereo, channels, &mut energy, &mut peak, &mut count);
        write_frame_slice(stereo, frame, |sample| sample);
    }
    feedback.publish_levels((energy / count.max(1.0)).sqrt(), peak);
}

fn fill_i16(
    synth: &mut SynthVoice,
    output: &mut [i16],
    channels: usize,
    feedback: &AudioVisualBridge,
    gain: &mut OutputGain,
) {
    synth.begin_callback();
    let target_gain = AudioEngine::master_gain();
    let mut energy = 0.0;
    let mut peak = 0.0_f32;
    let mut count = 0.0_f32;
    for frame in output.chunks_mut(channels.max(1)) {
        let stereo = gain.apply(synth.next_stereo_frame(), target_gain);
        accumulate_levels(stereo, channels, &mut energy, &mut peak, &mut count);
        write_frame_slice(stereo, frame, to_i16);
    }
    feedback.publish_levels((energy / count.max(1.0)).sqrt(), peak);
}

fn fill_u16(
    synth: &mut SynthVoice,
    output: &mut [u16],
    channels: usize,
    feedback: &AudioVisualBridge,
    gain: &mut OutputGain,
) {
    synth.begin_callback();
    let target_gain = AudioEngine::master_gain();
    let mut energy = 0.0;
    let mut peak = 0.0_f32;
    let mut count = 0.0_f32;
    for frame in output.chunks_mut(channels.max(1)) {
        let stereo = gain.apply(synth.next_stereo_frame(), target_gain);
        accumulate_levels(stereo, channels, &mut energy, &mut peak, &mut count);
        write_frame_slice(stereo, frame, to_u16);
    }
    feedback.publish_levels((energy / count.max(1.0)).sqrt(), peak);
}

fn accumulate_levels(
    stereo: [f32; 2],
    channels: usize,
    energy: &mut f32,
    peak: &mut f32,
    count: &mut f32,
) {
    if channels == 1 {
        let sample = mono_downmix(stereo);
        *energy += sample * sample;
        *peak = (*peak).max(sample.abs());
        *count += 1.0;
        return;
    }
    for sample in stereo {
        *energy += sample * sample;
        *peak = (*peak).max(sample.abs());
        *count += 1.0;
    }
}

fn write_frame_slice<T: Copy>(stereo: [f32; 2], frame: &mut [T], convert: impl Fn(f32) -> T) {
    if frame.is_empty() {
        return;
    }
    if frame.len() == 1 {
        frame[0] = convert(mono_downmix(stereo));
        return;
    }
    frame[0] = convert(stereo[0]);
    frame[1] = convert(stereo[1]);
    for channel in frame.iter_mut().skip(2) {
        *channel = convert(0.0);
    }
}

fn write_frame_f32(stereo: [f32; 2], output: &mut Vec<f32>, channels: usize) {
    if channels == 1 {
        output.push(mono_downmix(stereo));
        return;
    }
    output.push(stereo[0]);
    output.push(stereo[1]);
    output.extend(std::iter::repeat_n(0.0, channels.saturating_sub(2)));
}

fn mono_downmix(stereo: [f32; 2]) -> f32 {
    (stereo[0] + stereo[1]) * std::f32::consts::FRAC_1_SQRT_2
}

fn candidate_from_cpal(config: &cpal::SupportedStreamConfigRange) -> Option<OutputConfigCandidate> {
    let sample_format = match config.sample_format() {
        cpal::SampleFormat::F32 => RuntimeSampleFormat::F32,
        cpal::SampleFormat::I16 => RuntimeSampleFormat::I16,
        cpal::SampleFormat::U16 => RuntimeSampleFormat::U16,
        _ => return None,
    };
    Some(OutputConfigCandidate {
        min_sample_rate: config.min_sample_rate().0,
        max_sample_rate: config.max_sample_rate().0,
        channels: config.channels(),
        sample_format,
    })
}

fn to_i16(sample: f32) -> i16 {
    (sample.clamp(-1.0, 1.0) * f32::from(i16::MAX)).round() as i16
}

fn to_u16(sample: f32) -> u16 {
    ((sample.clamp(-1.0, 1.0) * 0.5 + 0.5) * f32::from(u16::MAX)).round() as u16
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quiet_boundary_attenuates_active_samples_with_bounded_ramp() {
        for rate in [16_000, 44_100, 48_000] {
            let mut gain = OutputGain::new(rate);
            let mut previous = 1.0;
            for _ in 0..rate / 10 {
                let frame = gain.apply([1.0, -0.5], 0.15);
                assert!(frame[0] <= previous + 1.0e-6);
                assert!((frame[0] - previous).abs() <= gain.step + 1.0e-6);
                assert!((frame[1] + frame[0] * 0.5).abs() < 1.0e-6);
                previous = frame[0];
            }
            assert!((previous - 0.15).abs() < 1.0e-5);
        }
    }

    fn panned(source: f32, pan: f32) -> [f32; 2] {
        [
            source * ((1.0 - pan) * 0.5).sqrt(),
            source * ((1.0 + pan) * 0.5).sqrt(),
        ]
    }

    #[test]
    fn mono_fold_down_is_symmetric_and_preserves_center_level() {
        let source = 0.6;
        let mut center = [0.0];
        let mut left = [0.0];
        let mut right = [0.0];
        write_frame_slice(panned(source, 0.0), &mut center, |sample| sample);
        write_frame_slice(panned(source, -0.8), &mut left, |sample| sample);
        write_frame_slice(panned(source, 0.8), &mut right, |sample| sample);

        assert!((center[0] - source).abs() < 0.000_001);
        assert!((left[0] - right[0]).abs() < 0.000_001);
        assert!(right[0] > source * 0.85);

        let mut offline = Vec::new();
        write_frame_f32(panned(source, 0.8), &mut offline, 1);
        assert_eq!(offline, right);
    }

    #[test]
    fn mono_callback_levels_measure_the_folded_output() {
        let stereo = panned(0.6, 0.8);
        let expected = mono_downmix(stereo);
        let mut energy = 0.0;
        let mut peak = 0.0;
        let mut count = 0.0;
        accumulate_levels(stereo, 1, &mut energy, &mut peak, &mut count);

        assert_eq!(count, 1.0);
        assert!((energy.sqrt() - expected.abs()).abs() < 0.000_001);
        assert!((peak - expected.abs()).abs() < 0.000_001);
    }

    #[test]
    fn callback_start_event_is_durable_even_if_visual_clip_state_is_cleared() {
        let genome = lifecore::Genome::from_seed(991);
        let voice = genome.voice.clone();
        let learned = lifecore::generate_initial_motifs(&voice);
        let request = VocalRequest {
            motif_id: learned[0].id,
            performance_seed: 0x51A7_E001,
            gain: 0.4,
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
        let commands = Arc::new(SpscRing::new());
        commands
            .push(VoiceCommand::prepare(&voice, &learned[0], &request))
            .unwrap();
        let feedback = Arc::new(AudioVisualBridge::default());
        let mut synth = SynthVoice::with_feedback(commands, 48_000, Arc::clone(&feedback));

        let _ = synth.next_stereo_frame();
        feedback.clear();

        assert_eq!(
            feedback.pop_started_request(),
            Some(request.performance_seed)
        );
        assert_eq!(feedback.pop_started_request(), None);
    }

    #[test]
    fn same_seed_body_timeline_is_sample_exact_deterministic() {
        let genome = lifecore::Genome::from_seed(0xB0D1);
        let motifs = lifecore::generate_initial_motifs(&genome.voice);
        let motif = &motifs[0];
        let request = VocalRequest {
            motif_id: motif.id,
            performance_seed: 0xB0D1_71AE,
            gain: 0.25,
            pan: 0.0,
            pitch_scale: 1.0,
            tempo_scale: 1.0,
            stress: 0.2,
            purr: false,
            gesture: lifecore::VoiceGesture::WarmChuff,
            priority: 180,
            style: lifecore::VocalStyle::SocialContact,
            valence: 0.3,
            arousal: 0.4,
            fatigue: 0.0,
            confidence: 0.8,
            attachment: 0.7,
            rhythm_intervals: [0.0; 8],
            phenotype: Default::default(),
        };
        let timeline = (0..80)
            .map(|frame| BodyVoiceFrame {
                stretch: frame as f32 / 79.0 * 0.8,
                shape_aspect_ratio: 1.0 + frame as f32 / 79.0,
                bond_strain: frame as f32 / 79.0 * 0.6,
                ..BodyVoiceFrame::default()
            })
            .collect::<Vec<_>>();
        let first = render_motif_with_body_timeline(
            &genome.voice,
            motif,
            &request,
            &timeline,
            100,
            48_000,
            1,
            OfflineSampleFormat::F32,
        );
        let second = render_motif_with_body_timeline(
            &genome.voice,
            motif,
            &request,
            &timeline,
            100,
            48_000,
            1,
            OfflineSampleFormat::F32,
        );
        assert_eq!(first, second);
    }
}
