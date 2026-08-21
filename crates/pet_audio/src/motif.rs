use std::sync::Arc;

use lifecore::{Syllable, VocalMotif, VocalRequest, VoiceGenome};

use crate::{
    envelope::amplitude_envelope,
    filter::{Resonator, soft_limit},
    oscillator::{NoiseSource, Oscillator},
    ring::SpscRing,
};

pub const MAX_SYLLABLES: usize = 6;
pub const COMMAND_CAPACITY: usize = 32;

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
}

impl VoiceCommand {
    #[must_use]
    pub fn prepare(voice: &VoiceGenome, motif: &VocalMotif, request: &VocalRequest) -> Self {
        let mut syllables = [PreparedSyllable::default(); MAX_SYLLABLES];
        for (target, source) in syllables.iter_mut().zip(&motif.syllables) {
            *target = PreparedSyllable::from(source);
        }
        Self {
            motif_id: motif.id,
            seed: motif.seed,
            syllable_count: motif.syllables.len().min(MAX_SYLLABLES) as u8,
            syllables,
            base_pitch_hz: voice.base_pitch_hz,
            harmonic_mix: voice.harmonic_mix,
            breathiness: voice.breathiness,
            roughness: voice.roughness,
            brightness: voice.brightness,
            formant_scale: voice.formant_scale,
            formant_spacing: voice.formant_spacing,
            vibrato_rate: voice.vibrato_rate,
            vibrato_depth: voice.vibrato_depth,
            trill_rate: voice.trill_rate,
            attack_ms: voice.attack_ms,
            release_ms: voice.release_ms,
            click_amount: voice.click_amount,
            purr_rate: voice.purr_rate,
            gain: request.gain,
            pan: request.pan,
            pitch_scale: request.pitch_scale,
            tempo_scale: request.tempo_scale,
            stress: request.stress,
            purr: request.purr,
            maximum_loudness: voice.maximum_loudness,
        }
    }

    #[must_use]
    pub fn total_frames(&self, sample_rate: u32) -> usize {
        self.syllables[..usize::from(self.syllable_count)]
            .iter()
            .map(|syllable| {
                (((syllable.duration_ms + syllable.gap_after_ms) / self.tempo_scale.max(0.1)
                    * sample_rate as f32
                    / 1000.0)
                    .round()
                    .max(1.0)) as usize
            })
            .sum()
    }
}

pub struct SynthVoice {
    commands: Arc<SpscRing<VoiceCommand, COMMAND_CAPACITY>>,
    sample_rate: f32,
    current: Option<VoiceCommand>,
    syllable_index: usize,
    frame_in_syllable: usize,
    gap_frames_remaining: usize,
    oscillator: Oscillator,
    noise: NoiseSource,
    formants: [Resonator; 3],
    click_pending: bool,
    lfo_phase: f32,
}

impl SynthVoice {
    #[must_use]
    pub fn new(commands: Arc<SpscRing<VoiceCommand, COMMAND_CAPACITY>>, sample_rate: u32) -> Self {
        Self {
            commands,
            sample_rate: sample_rate.max(1) as f32,
            current: None,
            syllable_index: 0,
            frame_in_syllable: 0,
            gap_frames_remaining: 0,
            oscillator: Oscillator::default(),
            noise: NoiseSource::new(1),
            formants: [Resonator::default(); 3],
            click_pending: false,
            lfo_phase: 0.0,
        }
    }

    pub fn next_stereo_frame(&mut self) -> [f32; 2] {
        if self.current.is_none() {
            let Some(command) = self.commands.pop() else {
                return [0.0; 2];
            };
            self.start_command(command);
        }
        if self.gap_frames_remaining > 0 {
            self.gap_frames_remaining -= 1;
            return [0.0; 2];
        }
        let command = self.current.expect("command is active");
        if self.syllable_index >= usize::from(command.syllable_count) {
            self.current = None;
            return self.next_stereo_frame();
        }
        let syllable = command.syllables[self.syllable_index];
        let total_frames = milliseconds_to_frames(
            syllable.duration_ms / command.tempo_scale.max(0.1),
            self.sample_rate,
        );
        if self.frame_in_syllable >= total_frames {
            self.gap_frames_remaining = milliseconds_to_frames(
                syllable.gap_after_ms / command.tempo_scale.max(0.1),
                self.sample_rate,
            );
            self.syllable_index += 1;
            self.frame_in_syllable = 0;
            self.click_pending = true;
            if self.syllable_index < usize::from(command.syllable_count) {
                self.configure_formants(command.syllables[self.syllable_index]);
            }
            return [0.0; 2];
        }

        let progress = self.frame_in_syllable as f32 / total_frames.max(1) as f32;
        let pitch_contour = if progress < 0.5 {
            lerp(syllable.pitch_start, syllable.pitch_peak, progress * 2.0)
        } else {
            lerp(
                syllable.pitch_peak,
                syllable.pitch_end,
                (progress - 0.5) * 2.0,
            )
        };
        self.lfo_phase = (self.lfo_phase + 1.0 / self.sample_rate).fract();
        let vibrato = (self.lfo_phase * command.vibrato_rate * std::f32::consts::TAU).sin()
            * command.vibrato_depth
            * syllable.vibrato_amount;
        let trill = (self.lfo_phase * command.trill_rate * std::f32::consts::TAU).sin()
            * 0.035
            * syllable.trill_amount;
        let stress_jitter = self.noise.sample(command.brightness) * command.stress * 0.018;
        let frequency = command.base_pitch_hz
            * pitch_contour
            * command.pitch_scale
            * (1.0 + vibrato + trill + stress_jitter).clamp(0.75, 1.25);
        let voiced = self
            .oscillator
            .sample(frequency, self.sample_rate, command.harmonic_mix);
        let noise = self.noise.sample(command.brightness)
            * (command.breathiness + syllable.noisiness * 0.65 + command.stress * 0.12);
        let impulse = if self.click_pending {
            self.click_pending = false;
            (command.click_amount + syllable.click) * 0.45
        } else {
            0.0
        };
        let purr = if command.purr {
            0.72 + 0.28 * (self.lfo_phase * command.purr_rate * std::f32::consts::TAU).sin()
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

        let source = (voiced * (1.0 - command.breathiness * 0.4) + noise + impulse)
            * envelope
            * syllable.amplitude
            * purr;
        let resonated = self
            .formants
            .iter_mut()
            .enumerate()
            .map(|(index, formant)| formant.process(source) * [0.52, 0.32, 0.18][index])
            .sum::<f32>();
        let mono = soft_limit(
            (source * 0.42 + resonated) * command.gain,
            command.maximum_loudness.min(0.85),
        );
        let pan = command.pan.clamp(-1.0, 1.0);
        let left = mono * ((1.0 - pan) * 0.5).sqrt();
        let right = mono * ((1.0 + pan) * 0.5).sqrt();
        [left, right]
    }

    fn start_command(&mut self, command: VoiceCommand) {
        self.noise.reseed(command.seed);
        self.current = Some(command);
        self.syllable_index = 0;
        self.frame_in_syllable = 0;
        self.gap_frames_remaining = 0;
        self.click_pending = true;
        if command.syllable_count > 0 {
            self.configure_formants(command.syllables[0]);
        }
    }

    fn configure_formants(&mut self, syllable: PreparedSyllable) {
        let command = self.current.expect("formants need an active command");
        let openness = syllable.mouth_open.clamp(0.0, 1.0);
        let base = [620.0, 1_420.0, 2_520.0];
        for (index, formant) in self.formants.iter_mut().enumerate() {
            let frequency = base[index]
                * command.formant_scale
                * (1.0 + index as f32 * (command.formant_spacing - 1.0) * 0.35)
                * (0.86 + openness * 0.28);
            formant.configure(frequency, 110.0 + index as f32 * 90.0, self.sample_rate);
        }
    }
}

fn milliseconds_to_frames(milliseconds: f32, sample_rate: f32) -> usize {
    (milliseconds.max(0.0) * sample_rate / 1000.0)
        .round()
        .max(1.0) as usize
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t.clamp(0.0, 1.0)
}
