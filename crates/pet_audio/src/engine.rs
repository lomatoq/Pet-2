use std::{
    fs::File,
    io::{self, Write},
    path::Path,
    sync::Arc,
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
}

impl AudioEngine {
    pub fn try_start() -> Result<Self, AudioError> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or(AudioError::NoOutputDevice)?;
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
                device.build_output_stream(
                    &config,
                    move |output: &mut [f32], _| {
                        fill_f32(&mut synth, output, channels, &feedback_for_levels);
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
                device.build_output_stream(
                    &config,
                    move |output: &mut [i16], _| {
                        fill_i16(&mut synth, output, channels, &feedback_for_levels);
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
                device.build_output_stream(
                    &config,
                    move |output: &mut [u16], _| {
                        fill_u16(&mut synth, output, channels, &feedback_for_levels);
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
        })
    }

    pub fn enqueue(
        &self,
        voice: &VoiceGenome,
        motif: &VocalMotif,
        request: &VocalRequest,
    ) -> Result<(), AudioError> {
        self.commands
            .push(VoiceCommand::prepare(voice, motif, request))
            .map_err(|_| AudioError::CommandQueueFull)
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
) {
    synth.begin_callback();
    let mut energy = 0.0;
    let mut peak = 0.0_f32;
    let mut count = 0.0_f32;
    for frame in output.chunks_mut(channels.max(1)) {
        let stereo = synth.next_stereo_frame();
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
) {
    synth.begin_callback();
    let mut energy = 0.0;
    let mut peak = 0.0_f32;
    let mut count = 0.0_f32;
    for frame in output.chunks_mut(channels.max(1)) {
        let stereo = synth.next_stereo_frame();
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
) {
    synth.begin_callback();
    let mut energy = 0.0;
    let mut peak = 0.0_f32;
    let mut count = 0.0_f32;
    for frame in output.chunks_mut(channels.max(1)) {
        let stereo = synth.next_stereo_frame();
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
