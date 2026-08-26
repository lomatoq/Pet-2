use std::sync::Arc;

use lifecore::{Syllable, VocalMotif, VocalRequest, VoiceGenome};

use crate::{
    AudioVisualBridge, AudioVisualFeedback,
    envelope::amplitude_envelope,
    filter::{DcBlocker, EarlyReflections, OnePoleLowPass, Resonator, soft_limit},
    oscillator::{NoiseSource, Oscillator},
    ring::SpscRing,
};

pub const MAX_SYLLABLES: usize = 6;
pub const COMMAND_CAPACITY: usize = 32;
const ROOM_TAIL_MS: f32 = 36.0;
const MIN_CALL_FUNDAMENTAL_HZ: f32 = 260.0;
const MAX_CALL_FUNDAMENTAL_HZ: f32 = 8_000.0;

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct PreparedSyllable {
    pub duration_ms: f32,
    pub gap_after_ms: f32,
    pub pitch_start: f32,
    pub pitch_peak: f32,
    pub pitch_end: f32,
    pub amplitude: f32,
    pub noisiness: f32,
    pub click: f32,
    pub mouth_open: f32,
    pub trill_amount: f32,
    pub vibrato_amount: f32,
}

impl From<&Syllable> for PreparedSyllable {
    fn from(value: &Syllable) -> Self {
        Self {
            duration_ms: value.duration_ms,
            gap_after_ms: value.gap_after_ms,
            pitch_start: value.pitch_start,
            pitch_peak: value.pitch_peak,
            pitch_end: value.pitch_end,
            amplitude: value.amplitude,
            noisiness: value.noisiness,
            click: value.click,
            mouth_open: value.mouth_open,
            trill_amount: value.trill_amount,
            vibrato_amount: value.vibrato_amount,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VoiceCommand {
    pub request_id: u64,
    pub motif_id: u64,
    pub seed: u64,
    pub syllable_count: u8,
    pub syllables: [PreparedSyllable; MAX_SYLLABLES],
    pub base_pitch_hz: f32,
    pub harmonic_mix: [f32; 4],
    pub breathiness: f32,
    pub roughness: f32,
    pub brightness: f32,
    pub formant_scale: f32,
    pub formant_spacing: f32,
    pub mouth_resonance: f32,
    pub vibrato_rate: f32,
    pub vibrato_depth: f32,
    pub trill_rate: f32,
    pub attack_ms: f32,
    pub release_ms: f32,
    pub click_amount: f32,
    pub purr_rate: f32,
    pub gain: f32,
    pub pan: f32,
    pub pitch_scale: f32,
    pub tempo_scale: f32,
    pub stress: f32,
    pub purr: bool,
    pub maximum_loudness: f32,
    pub spectral_drift: f32,
    pub formant_drift: f32,
    pub room_mix: f32,
}

impl VoiceCommand {
    #[must_use]
    pub fn prepare(voice: &VoiceGenome, motif: &VocalMotif, request: &VocalRequest) -> Self {
        let mut syllables = [PreparedSyllable::default(); MAX_SYLLABLES];
        for (target, source) in syllables.iter_mut().zip(&motif.syllables) {
            *target = PreparedSyllable::from(source);
        }
        let rhythm_interval_count = request
            .rhythm_intervals
            .iter()
            .position(|interval| !interval.is_finite() || *interval <= 0.0)
            .unwrap_or(request.rhythm_intervals.len())
            .min(MAX_SYLLABLES.saturating_sub(1));
        if rhythm_interval_count > 0 && !motif.syllables.is_empty() {
            for (index, syllable) in syllables
                .iter_mut()
                .enumerate()
                .take(rhythm_interval_count + 1)
            {
                *syllable = PreparedSyllable::from(&motif.syllables[index % motif.syllables.len()]);
                syllable.duration_ms = syllable.duration_ms.clamp(55.0, 88.0);
                if index < rhythm_interval_count {
                    let onset_ms = 220.0 * request.rhythm_intervals[index].clamp(0.1, 4.0);
                    syllable.gap_after_ms = (onset_ms - syllable.duration_ms).clamp(4.0, 792.0);
                } else {
                    syllable.gap_after_ms = 0.0;
                }
            }
        }
        let performance_seed = request.performance_seed ^ motif.seed.rotate_left(17);
        let spectral_drift = seeded_signed(performance_seed ^ 0xA24B_AED4_963E_E407);
        let formant_drift = seeded_signed(performance_seed ^ 0x9FB2_1C65_1E98_DF25);
        let envelope_drift = seeded_signed(performance_seed ^ 0xC13F_A9A9_02A6_328F);
        let room_unit = seeded_unit(performance_seed ^ 0x91E1_0DA5_C79E_7B1D);
        Self {
            request_id: request.performance_seed,
            motif_id: motif.id,
            seed: splitmix64(performance_seed),
            syllable_count: if rhythm_interval_count > 0 {
                rhythm_interval_count.saturating_add(1) as u8
            } else {
                motif.syllables.len().min(MAX_SYLLABLES) as u8
            },
            syllables,
            // Normal cat calls cluster around a few hundred hertz and carry
            // their strongest energy near 1-2 kHz. Very low legacy genomes
            // produced a mains-like electronic drone, so audible calls enter a
            // conservative animal-vocal range while motifs retain pitch shape.
            base_pitch_hz: voice.base_pitch_hz.clamp(320.0, 720.0),
            harmonic_mix: softened_harmonic_mix(voice.harmonic_mix),
            breathiness: voice.breathiness,
            roughness: voice.roughness,
            brightness: voice.brightness,
            formant_scale: voice.formant_scale,
            formant_spacing: voice.formant_spacing,
            mouth_resonance: voice.mouth_resonance,
            vibrato_rate: voice.vibrato_rate,
            vibrato_depth: voice.vibrato_depth,
            trill_rate: voice.trill_rate,
            attack_ms: voice.attack_ms * (1.0 + envelope_drift * 0.18),
            release_ms: voice.release_ms * (1.0 - envelope_drift * 0.22),
            click_amount: voice.click_amount,
            // Purr is amplitude texture, never a directly audible pure tone.
            purr_rate: voice.purr_rate.clamp(24.0, 32.0),
            // Keep rendition dynamics audible. The previous hard 0.45 floor
            // collapsed almost every quiet/normal request onto one loudness.
            gain: (request.gain * 2.0).clamp(0.08, 0.62),
            pan: request.pan,
            pitch_scale: request.pitch_scale,
            tempo_scale: request.tempo_scale,
            stress: request.stress,
            purr: request.purr,
            // Genome loudness already shaped `request.gain`; this is only a
            // transparent safety ceiling, not a second compressor.
            maximum_loudness: 0.68,
            spectral_drift,
            formant_drift,
            room_mix: 0.015 + room_unit * 0.020,
        }
    }

    #[must_use]
    pub fn total_frames(&self, sample_rate: u32) -> usize {
        self.syllables[..usize::from(self.syllable_count)]
            .iter()
            .map(|syllable| {
                let tempo = self.tempo_scale.max(0.1);
                milliseconds_to_frames(syllable.duration_ms / tempo, sample_rate as f32)
                    + milliseconds_to_frames_allow_zero(
                        syllable.gap_after_ms / tempo,
                        sample_rate as f32,
                    )
            })
            .sum::<usize>()
            + milliseconds_to_frames(ROOM_TAIL_MS, sample_rate.max(1) as f32)
    }
}

pub struct SynthVoice {
    commands: Arc<SpscRing<VoiceCommand, COMMAND_CAPACITY>>,
    feedback: Arc<AudioVisualBridge>,
    sample_rate: f32,
    current: Option<VoiceCommand>,
    syllable_index: usize,
    frame_in_syllable: usize,
    gap_frames_remaining: usize,
    oscillator: Oscillator,
    noise: NoiseSource,
    modulation_noise: NoiseSource,
    formants: [Resonator; 3],
    tone_lowpass: OnePoleLowPass,
    voice_low_cut: OnePoleLowPass,
    breath_low_cut: OnePoleLowPass,
    breath_high_cut: OnePoleLowPass,
    modulation_lowpass: OnePoleLowPass,
    dc_blocker: DcBlocker,
    room: EarlyReflections,
    vibrato_phase: f32,
    trill_phase: f32,
    purr_phase: f32,
    purr_secondary_phase: f32,
    tail_frames_remaining: usize,
    last_pan: f32,
}

impl SynthVoice {
    #[must_use]
    pub fn new(commands: Arc<SpscRing<VoiceCommand, COMMAND_CAPACITY>>, sample_rate: u32) -> Self {
        Self::with_feedback(
            commands,
            sample_rate,
            Arc::new(AudioVisualBridge::default()),
        )
    }

    #[must_use]
    pub fn with_feedback(
        commands: Arc<SpscRing<VoiceCommand, COMMAND_CAPACITY>>,
        sample_rate: u32,
        feedback: Arc<AudioVisualBridge>,
    ) -> Self {
        let sample_rate = sample_rate.max(1) as f32;
        Self {
            commands,
            feedback,
            sample_rate,
            current: None,
            syllable_index: 0,
            frame_in_syllable: 0,
            gap_frames_remaining: 0,
            oscillator: Oscillator::default(),
            noise: NoiseSource::new(1),
            modulation_noise: NoiseSource::new(2),
            formants: [Resonator::default(); 3],
            tone_lowpass: OnePoleLowPass::default(),
            voice_low_cut: OnePoleLowPass::default(),
            breath_low_cut: OnePoleLowPass::default(),
            breath_high_cut: OnePoleLowPass::default(),
            modulation_lowpass: OnePoleLowPass::default(),
            dc_blocker: DcBlocker::default(),
            room: EarlyReflections::new(sample_rate),
            vibrato_phase: 0.0,
            trill_phase: 0.0,
            purr_phase: 0.0,
            purr_secondary_phase: 0.0,
            tail_frames_remaining: 0,
            last_pan: 0.0,
        }
    }

    pub fn next_stereo_frame(&mut self) -> [f32; 2] {
        if self.current.is_none() {
            if self.tail_frames_remaining > 0 {
                self.tail_frames_remaining -= 1;
                let frame = self.post_process(0.0, self.last_pan, 0.68);
                if self.tail_frames_remaining == 0 {
                    self.feedback.clear();
                }
                return frame;
            }
            let Some(command) = self.commands.pop() else {
                self.feedback.clear();
                return [0.0; 2];
            };
            self.start_command(command);
        }
        if self.gap_frames_remaining > 0 {
            self.gap_frames_remaining -= 1;
            self.publish_feedback(0.0, 0.0, 0.0, 0.0);
            return self.render_silence_frame();
        }
        let command = self.current.expect("command is active");
        if self.syllable_index >= usize::from(command.syllable_count) {
            self.current = None;
            self.tail_frames_remaining = milliseconds_to_frames(ROOM_TAIL_MS, self.sample_rate);
            return self.next_stereo_frame();
        }
        let syllable = command.syllables[self.syllable_index];
        let total_frames = milliseconds_to_frames(
            syllable.duration_ms / command.tempo_scale.max(0.1),
            self.sample_rate,
        );
        if self.frame_in_syllable >= total_frames {
            self.gap_frames_remaining = milliseconds_to_frames_allow_zero(
                syllable.gap_after_ms / command.tempo_scale.max(0.1),
                self.sample_rate,
            );
            self.syllable_index += 1;
            self.frame_in_syllable = 0;
            if self.syllable_index < usize::from(command.syllable_count) {
                self.configure_formants(command.syllables[self.syllable_index]);
            }
            self.publish_feedback(0.0, 0.0, 0.0, 0.0);
            return self.next_stereo_frame();
        }

        let progress = self.frame_in_syllable as f32 / total_frames.saturating_sub(1).max(1) as f32;
        let pitch_contour = if progress < 0.5 {
            smooth_lerp(syllable.pitch_start, syllable.pitch_peak, progress * 2.0)
        } else {
            smooth_lerp(
                syllable.pitch_peak,
                syllable.pitch_end,
                (progress - 0.5) * 2.0,
            )
        };
        self.vibrato_phase = (self.vibrato_phase + command.vibrato_rate / self.sample_rate).fract();
        self.trill_phase = (self.trill_phase + command.trill_rate / self.sample_rate).fract();
        let vibrato = (self.vibrato_phase * std::f32::consts::TAU).sin()
            * command.vibrato_depth
            * syllable.vibrato_amount;
        let trill =
            (self.trill_phase * std::f32::consts::TAU).sin() * 0.030 * syllable.trill_amount;
        let modulation = self
            .modulation_lowpass
            .process(self.modulation_noise.sample(0.0));
        let organic_jitter = modulation * (command.stress * 0.010 + command.roughness * 0.006);
        let frequency = (command.base_pitch_hz
            * pitch_contour
            * command.pitch_scale
            * (1.0 + vibrato + trill + organic_jitter).clamp(0.75, 1.25))
        .clamp(MIN_CALL_FUNDAMENTAL_HZ, MAX_CALL_FUNDAMENTAL_HZ);
        let primary = self
            .oscillator
            .sample(frequency, self.sample_rate, command.harmonic_mix);
        // Two almost-identical oscillators created a perfectly periodic beat
        // that listeners heard as computer hum. Roughness now comes from the
        // already band-limited aspiration path and bounded aperiodic jitter.
        let voiced = self.tone_lowpass.process(primary);
        let noisiness = (command.breathiness + syllable.noisiness * 0.65 + command.stress * 0.12)
            .clamp(0.0, 0.78);
        let raw_noise = self.noise.sample(command.brightness);
        let breath_high = raw_noise - self.breath_low_cut.process(raw_noise);
        let breath = self.breath_high_cut.process(breath_high);
        let burst_frames = milliseconds_to_frames(5.0, self.sample_rate);
        let burst_progress = self.frame_in_syllable as f32 / burst_frames.max(1) as f32;
        let articulation = if burst_progress < 1.0 {
            breath
                * (command.click_amount + syllable.click).clamp(0.0, 1.4)
                * 0.22
                * (std::f32::consts::PI * burst_progress).sin()
        } else {
            0.0
        };
        let purr = if command.purr {
            self.purr_phase = (self.purr_phase + command.purr_rate / self.sample_rate).fract();
            self.purr_secondary_phase =
                (self.purr_secondary_phase + command.purr_rate * 0.47 / self.sample_rate).fract();
            0.84 + 0.10 * (self.purr_phase * std::f32::consts::TAU).sin()
                + 0.06 * (self.purr_secondary_phase * std::f32::consts::TAU).sin()
        } else {
            1.0
        };
        let attack_frames = milliseconds_to_frames(command.attack_ms, self.sample_rate);
        let release_frames = milliseconds_to_frames(command.release_ms, self.sample_rate);
        let envelope = amplitude_envelope(
            self.frame_in_syllable,
            total_frames,
            attack_frames,
            release_frames,
        );
        self.frame_in_syllable += 1;

        self.publish_feedback(
            envelope,
            syllable.mouth_open,
            frequency / command.base_pitch_hz.max(1.0),
            noisiness,
        );

        let source = (voiced * (1.0 - noisiness * 0.25) + breath * noisiness * 0.30 + articulation)
            * envelope
            * syllable.amplitude
            * purr;
        let resonated = self
            .formants
            .iter_mut()
            .enumerate()
            .map(|(index, formant)| formant.process(source) * [0.55, 0.30, 0.15][index])
            .sum::<f32>();
        let formant_mix = 0.09 + command.mouth_resonance.clamp(0.0, 1.0) * 0.13;
        let mono = (source * (1.0 - formant_mix) + resonated * formant_mix) * command.gain;
        self.post_process(mono, command.pan, command.maximum_loudness)
    }

    fn start_command(&mut self, command: VoiceCommand) {
        self.feedback.publish_started_request(command.request_id);
        self.noise.reseed(command.seed);
        self.modulation_noise
            .reseed(command.seed ^ 0xD6E8_FEB8_6659_FD93);
        self.current = Some(command);
        self.syllable_index = 0;
        self.frame_in_syllable = 0;
        self.gap_frames_remaining = 0;
        self.tail_frames_remaining = 0;
        self.last_pan = command.pan;
        self.oscillator
            .set_phase(seeded_unit(command.seed ^ 0x243F_6A88_85A3_08D3));
        self.vibrato_phase = seeded_unit(command.seed ^ 0xA076_1D64_78BD_642F);
        self.trill_phase = seeded_unit(command.seed ^ 0xE703_7ED1_A0B4_28DB);
        self.purr_phase = seeded_unit(command.seed ^ 0x8EBC_6AF0_9C88_C6E3);
        self.purr_secondary_phase = seeded_unit(command.seed ^ 0x5899_65CC_7537_4CC3);
        let tone_cutoff = (4_800.0 + command.brightness.clamp(0.0, 1.0) * 3_600.0)
            * (1.0 + command.spectral_drift * 0.10);
        let breath_cutoff = (4_500.0 + command.brightness.clamp(0.0, 1.0) * 3_000.0)
            * (1.0 + command.spectral_drift * 0.08);
        self.tone_lowpass.configure(tone_cutoff, self.sample_rate);
        self.voice_low_cut.configure(190.0, self.sample_rate);
        self.breath_low_cut.configure(520.0, self.sample_rate);
        self.breath_high_cut
            .configure(breath_cutoff, self.sample_rate);
        self.modulation_lowpass.configure(18.0, self.sample_rate);
        self.dc_blocker.configure(24.0, self.sample_rate);
        self.tone_lowpass.reset();
        self.voice_low_cut.reset();
        self.breath_low_cut.reset();
        self.breath_high_cut.reset();
        self.modulation_lowpass.reset();
        self.dc_blocker.reset();
        self.room.reset();
        self.room.configure(command.room_mix);
        for formant in &mut self.formants {
            formant.reset();
        }
        if command.syllable_count > 0 {
            self.configure_formants(command.syllables[0]);
        }
        self.publish_feedback(0.0, 0.0, 1.0, command.breathiness);
    }

    fn configure_formants(&mut self, syllable: PreparedSyllable) {
        let command = self.current.expect("formants need an active command");
        let openness = syllable.mouth_open.clamp(0.0, 1.0);
        let base = [680.0, 1_480.0, 2_460.0];
        for (index, formant) in self.formants.iter_mut().enumerate() {
            let frequency = base[index]
                * command.formant_scale
                * (1.0 + index as f32 * (command.formant_spacing - 1.0) * 0.35)
                * (0.86 + openness * 0.28)
                * (1.0 + command.formant_drift * 0.020);
            let resonance = command.mouth_resonance.clamp(0.0, 1.0);
            // Wider resonances shape a moving vocal tract without ringing like
            // narrow electronic filters between syllables.
            let bandwidth = [270.0, 390.0, 520.0][index] * (1.08 - resonance * 0.12);
            formant.configure(frequency, bandwidth, self.sample_rate);
        }
    }

    fn render_silence_frame(&mut self) -> [f32; 2] {
        let command = self.current.expect("gap belongs to an active command");
        // A speech resonator ringing through every authored gap was the other
        // persistent pitched tone. Only the tiny natural room tail may bridge a
        // gap; the vocal tract itself receives silence.
        self.post_process(0.0, command.pan, command.maximum_loudness)
    }

    fn post_process(&mut self, mono: f32, pan: f32, maximum_loudness: f32) -> [f32; 2] {
        let mono = mono - self.voice_low_cut.process(mono);
        let mono = self.dc_blocker.process(mono);
        let mono = self.room.process(mono);
        let mono = soft_limit(mono, maximum_loudness.min(0.78));
        let pan = pan.clamp(-1.0, 1.0);
        let left = mono * ((1.0 - pan) * 0.5).sqrt();
        let right = mono * ((1.0 + pan) * 0.5).sqrt();
        [left, right]
    }

    fn publish_feedback(
        &self,
        envelope: f32,
        mouth_open: f32,
        pitch_normalized: f32,
        noisiness: f32,
    ) {
        let Some(command) = self.current else {
            self.feedback.clear();
            return;
        };
        self.feedback.publish(AudioVisualFeedback {
            active: true,
            request_id: command.request_id,
            motif_id: command.motif_id,
            syllable_index: self.syllable_index.min(u8::MAX as usize) as u8,
            envelope: envelope.clamp(0.0, 1.0),
            mouth_open: mouth_open.clamp(0.0, 1.0),
            pitch_normalized: pitch_normalized.clamp(0.25, 4.0),
            noisiness: noisiness.clamp(0.0, 1.0),
            purr: if command.purr { 1.0 } else { 0.0 },
        });
    }
}

fn milliseconds_to_frames(milliseconds: f32, sample_rate: f32) -> usize {
    (milliseconds.max(0.0) * sample_rate / 1000.0)
        .round()
        .max(1.0) as usize
}

fn milliseconds_to_frames_allow_zero(milliseconds: f32, sample_rate: f32) -> usize {
    (milliseconds.max(0.0) * sample_rate / 1000.0).round() as usize
}

fn smooth_lerp(a: f32, b: f32, t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    let t = t * t * (3.0 - 2.0 * t);
    a + (b - a) * t
}

fn splitmix64(mut value: u64) -> u64 {
    value = value.wrapping_add(0x9E37_79B9_7F4A_7C15);
    value = (value ^ (value >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    value ^ (value >> 31)
}

fn seeded_unit(seed: u64) -> f32 {
    let bits = (splitmix64(seed) >> 40) as u32;
    bits as f32 / 0xFF_FFFF as f32
}

fn seeded_signed(seed: u64) -> f32 {
    seeded_unit(seed) * 2.0 - 1.0
}

fn softened_harmonic_mix(mut mix: [f32; 4]) -> [f32; 4] {
    for value in &mut mix {
        if !value.is_finite() {
            *value = 0.0;
        }
        *value = value.max(0.0);
    }
    // Pulse-rich sources read as a cheap buzzer at desktop volume. Preserve a
    // little animal rasp but move most energy into the continuous sine source.
    if mix[2] > 0.035 {
        let removed = mix[2] - 0.035;
        mix[2] = 0.035;
        mix[0] += removed;
    }
    let digital = mix[1] + mix[2];
    if digital > 0.12 {
        let retained = 0.12 / digital;
        let removed = digital - 0.12;
        mix[1] *= retained;
        mix[2] *= retained;
        mix[0] += removed * 0.90;
        mix[3] += removed * 0.10;
    }
    let sum = mix.iter().sum::<f32>();
    if sum <= f32::EPSILON {
        return [1.0, 0.0, 0.0, 0.0];
    }
    for value in &mut mix {
        *value /= sum;
    }
    mix
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prepared_calls_avoid_the_legacy_hum_band() {
        let mut voice = lifecore::Genome::from_seed(41).voice;
        voice.base_pitch_hz = 120.0;
        voice.purr_rate = 12.0;
        let motif = lifecore::generate_initial_motifs(&voice)
            .into_iter()
            .next()
            .expect("generated motif");
        let request = VocalRequest {
            performance_seed: 17,
            motif_id: motif.id,
            gain: 0.2,
            pan: 0.0,
            pitch_scale: 1.0,
            tempo_scale: 1.0,
            stress: 0.0,
            purr: true,
            rhythm_intervals: [0.0; 8],
        };
        let command = VoiceCommand::prepare(&voice, &motif, &request);
        assert_eq!(command.base_pitch_hz, 320.0);
        assert_eq!(command.purr_rate, 24.0);
    }

    #[test]
    fn grounded_rhythm_request_shapes_inter_onset_intervals() {
        let voice = lifecore::Genome::from_seed(42).voice;
        let motif = lifecore::generate_initial_motifs(&voice)
            .into_iter()
            .next()
            .expect("generated motif");
        let requested = [0.50, 1.00, 0.75, 1.75, 0.0, 0.0, 0.0, 0.0];
        let request = VocalRequest {
            performance_seed: 18,
            motif_id: motif.id,
            gain: 0.2,
            pan: 0.0,
            pitch_scale: 1.0,
            tempo_scale: 1.0,
            stress: 0.0,
            purr: false,
            rhythm_intervals: requested,
        };
        let command = VoiceCommand::prepare(&voice, &motif, &request);
        assert_eq!(command.syllable_count, 5);
        let actual = std::array::from_fn::<_, 4, _>(|index| {
            (command.syllables[index].duration_ms + command.syllables[index].gap_after_ms) / 220.0
        });
        let mean_actual = actual.iter().sum::<f32>() / actual.len() as f32;
        let covariance = actual
            .iter()
            .zip(requested)
            .take(4)
            .map(|(actual, requested)| (*actual - mean_actual) * (requested - 1.0))
            .sum::<f32>();
        let actual_energy = actual
            .iter()
            .map(|actual| (*actual - mean_actual).powi(2))
            .sum::<f32>();
        let requested_energy = requested[..4]
            .iter()
            .map(|requested| (*requested - 1.0).powi(2))
            .sum::<f32>();
        let correlation = covariance / (actual_energy * requested_energy).sqrt();
        assert!(correlation >= 0.85, "correlation={correlation}");
    }

    #[test]
    fn buzzy_waveform_energy_is_strictly_bounded() {
        let mix = softened_harmonic_mix([0.1, 0.4, 0.4, 0.1]);
        assert!(mix[2] <= 0.035_001);
        assert!(mix[1] + mix[2] <= 0.120_001);
        assert!((mix.iter().sum::<f32>() - 1.0).abs() < 1.0e-6);
    }
}
