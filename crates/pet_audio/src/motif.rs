use std::sync::Arc;

use lifecore::{
    BodyVoiceFrame, Syllable, VocalFamily, VocalGesture, VocalMotif, VocalRequest, VocalStyle,
    VoiceAnatomy, VoiceGenome, VoiceGesture,
};

use crate::{
    AudioVisualBridge, AudioVisualFeedback, BodyVoiceBridge, VoiceDiagnostics,
    body_resonance::LiquidBodyResonance,
    breath::BreathPressureController,
    filter::{DcBlocker, EarlyReflections, OnePoleLowPass, soft_limit},
    glottis::HybridLfGlottis,
    noise::NoiseSource,
    prosody::ProsodyCurve,
    ring::SpscRing,
    tract::DynamicTract,
};

pub const MAX_SYLLABLES: usize = 6;
pub const COMMAND_CAPACITY: usize = 32;
const INTER_SYLLABLE_RELEASE_MS: f32 = 64.0;
const PHONATION_RELEASE_BASE_MS: f32 = 260.0;
const PHONATION_RELEASE_FATIGUE_MS: f32 = 75.0;
const ROOM_TAIL_MS: f32 = 240.0;
const MINIMUM_F0_HZ: f32 = 85.0;
const MAXIMUM_F0_HZ: f32 = 1_600.0;
const CONTROL_RATE_HZ: f32 = 400.0;

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
    pub gesture: VocalGesture,
    prosody: ProsodyCurve,
}

impl PreparedSyllable {
    fn prepare(value: &Syllable, seed: u64, family: VocalFamily, request: &VocalRequest) -> Self {
        let phenotype = request.phenotype;
        let mut gesture = value.gesture;
        gesture.pressure_peak = (gesture.pressure_peak
            * (0.92 + request.arousal.clamp(0.0, 1.0) * 0.16)
            * (1.0 - request.fatigue.clamp(0.0, 1.0) * 0.34))
            .clamp(0.0, 1.0);
        gesture.adduction = (gesture.adduction + request.arousal.clamp(0.0, 1.0) * 0.07
            - request.fatigue.clamp(0.0, 1.0) * 0.05)
            .clamp(0.0, 1.0);
        gesture.open_quotient = (gesture.open_quotient
            + request.valence.clamp(-1.0, 1.0) * 0.035
            + request.fatigue.clamp(0.0, 1.0) * 0.045)
            .clamp(0.30, 0.82);
        gesture.constriction = (gesture.constriction - request.valence.clamp(-1.0, 1.0) * 0.05
            + request.stress.clamp(0.0, 1.0) * 0.10)
            .clamp(0.0, 1.0);
        gesture.nasality =
            (gesture.nasality + request.fatigue.clamp(0.0, 1.0) * 0.12).clamp(0.0, 1.0);
        gesture.instability = (gesture.instability
            + request.stress.clamp(0.0, 1.0) * 0.24
            + (1.0 - request.confidence.clamp(0.0, 1.0)) * 0.12)
            .clamp(0.0, 1.0);
        gesture.body_excitation =
            (gesture.body_excitation + request.attachment.clamp(0.0, 1.0) * 0.08).clamp(0.0, 1.0);
        // `enabled` gates spontaneous call initiation, not the physical voice
        // phenotype of a call that another behavior owner has already issued.
        gesture.open_quotient =
            (gesture.open_quotient + phenotype.breathiness_delta * 0.18).clamp(0.30, 0.82);
        gesture.constriction =
            (gesture.constriction + phenotype.roughness_delta * 0.16).clamp(0.0, 1.0);
        gesture.frontness =
            (gesture.frontness + phenotype.brightness_delta * 0.18).clamp(-1.0, 1.0);
        gesture.instability = (gesture.instability
            + phenotype.roughness_delta * 0.25
            + phenotype.effort_noise * 0.18)
            .clamp(0.0, 1.0);
        match request.gesture {
            VoiceGesture::PurrHum => {
                gesture.pressure_peak *= 0.72;
                gesture.nasality += 0.24;
                gesture.body_excitation += 0.24;
                gesture.constriction += 0.10;
            }
            VoiceGesture::WarmChuff => {
                gesture.open_quotient += 0.04;
                gesture.body_excitation += 0.08;
            }
            VoiceGesture::MewWhine => {
                gesture.frontness += 0.20;
                gesture.pressure_peak += 0.08;
                gesture.constriction -= 0.06;
            }
            VoiceGesture::LowRumble => {
                gesture.adduction += 0.14;
                gesture.constriction += 0.20;
                gesture.body_excitation += 0.18;
            }
            VoiceGesture::ClippedPulse => {
                gesture.adduction += 0.18;
                gesture.closure_sharpness += 0.24;
                gesture.open_quotient -= 0.08;
            }
            VoiceGesture::ReliefExhale => {
                gesture.pressure_peak *= 0.68;
                gesture.open_quotient += 0.10;
                gesture.adduction -= 0.12;
                gesture.nasality += 0.08;
            }
        }
        gesture.sanitize();
        Self {
            duration_ms: value.duration_ms,
            gap_after_ms: value.gap_after_ms,
            pitch_start: value.pitch_start,
            pitch_peak: value.pitch_peak,
            pitch_end: value.pitch_end,
            amplitude: value.amplitude,
            noisiness: (value.noisiness
                + phenotype.breathiness_delta.max(0.0) * 0.30
                + phenotype.roughness_delta.max(0.0) * 0.18)
                .clamp(0.0, 1.0),
            click: value.click,
            mouth_open: value.mouth_open,
            trill_amount: (value.trill_amount + phenotype.trill_amount * 0.35).clamp(0.0, 1.0),
            vibrato_amount: (value.vibrato_amount
                * phenotype.pitch_variation_multiplier.clamp(0.65, 1.35))
            .clamp(0.0, 1.0),
            gesture,
            prosody: ProsodyCurve::from_targets(
                value.pitch_start,
                value.pitch_peak,
                value.pitch_end,
                seed,
                family,
                request.style,
                request.valence,
                phenotype.phrase_contour,
            ),
        }
    }

    #[must_use]
    pub fn pitch_ratio(&self, progress: f32) -> f32 {
        self.prosody.sample(progress)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VoiceCommand {
    pub request_id: u64,
    pub motif_id: u64,
    pub seed: u64,
    pub family: VocalFamily,
    pub syllable_count: u8,
    pub syllables: [PreparedSyllable; MAX_SYLLABLES],
    pub base_pitch_hz: f32,
    pub minimum_f0_hz: f32,
    pub maximum_f0_hz: f32,
    pub anatomy: VoiceAnatomy,
    pub brightness: f32,
    pub breathiness: f32,
    pub roughness: f32,
    pub gain: f32,
    pub pan: f32,
    pub pitch_scale: f32,
    pub tempo_scale: f32,
    pub stress: f32,
    pub purr: bool,
    pub purr_rate: f32,
    pub gesture: VoiceGesture,
    pub style: VocalStyle,
    pub valence: f32,
    pub arousal: f32,
    pub fatigue: f32,
    pub confidence: f32,
    pub attachment: f32,
    pub attack_multiplier: f32,
    pub release_multiplier: f32,
    pub breath_phase_lock: f32,
    pub maximum_loudness: f32,
    pub room_mix: f32,
}

impl VoiceCommand {
    #[must_use]
    pub fn prepare(voice: &VoiceGenome, motif: &VocalMotif, request: &VocalRequest) -> Self {
        let performance_seed = request.performance_seed ^ motif.seed.rotate_left(17);
        let mut syllables = [PreparedSyllable::default(); MAX_SYLLABLES];
        for (index, (target, source)) in syllables.iter_mut().zip(&motif.syllables).enumerate() {
            *target = PreparedSyllable::prepare(
                source,
                performance_seed ^ index as u64,
                motif.family,
                request,
            );
        }
        let rhythm_interval_count = request
            .rhythm_intervals
            .iter()
            .position(|interval| !interval.is_finite() || *interval <= 0.0)
            .unwrap_or(request.rhythm_intervals.len())
            .min(MAX_SYLLABLES.saturating_sub(1));
        let mut syllable_count = motif.syllables.len().min(MAX_SYLLABLES);
        if rhythm_interval_count > 0 && !motif.syllables.is_empty() {
            syllable_count = rhythm_interval_count + 1;
            for (index, target) in syllables.iter_mut().enumerate().take(syllable_count) {
                let source = &motif.syllables[index % motif.syllables.len()];
                *target = PreparedSyllable::prepare(
                    source,
                    performance_seed ^ index as u64,
                    VocalFamily::RhythmMimic,
                    request,
                );
                target.duration_ms = target.duration_ms.clamp(55.0, 92.0);
                if index < rhythm_interval_count {
                    let onset_ms = 220.0 * request.rhythm_intervals[index].clamp(0.1, 4.0);
                    target.gap_after_ms = (onset_ms - target.duration_ms).clamp(4.0, 792.0);
                } else {
                    target.gap_after_ms = 0.0;
                }
            }
        }
        let identity = identity_register(request.style, request.gesture);
        let room_unit = seeded_unit(performance_seed ^ 0x91e1_0da5_c79e_7b1d);
        let phenotype = request.phenotype;
        let mut anatomy = voice.anatomy;
        let formant = phenotype.formant_scale_multiplier.clamp(0.94, 1.06);
        anatomy.tract_length = (anatomy.tract_length / formant).clamp(0.0, 1.0);
        anatomy.glottal_leak =
            (anatomy.glottal_leak + phenotype.breathiness_delta * 0.20).clamp(0.0, 1.0);
        anatomy.instability_susceptibility =
            (anatomy.instability_susceptibility + phenotype.roughness_delta * 0.22).clamp(0.0, 1.0);
        Self {
            request_id: request.performance_seed,
            motif_id: motif.id,
            seed: splitmix64(performance_seed),
            family: motif.family,
            syllable_count: syllable_count as u8,
            syllables,
            base_pitch_hz: voice
                .base_pitch_hz
                .clamp(identity.minimum_f0, identity.maximum_f0),
            minimum_f0_hz: MINIMUM_F0_HZ,
            maximum_f0_hz: MAXIMUM_F0_HZ,
            anatomy,
            brightness: (voice.brightness + phenotype.brightness_delta).clamp(0.0, 1.0),
            breathiness: (voice.breathiness + phenotype.breathiness_delta).clamp(0.0, 1.0),
            roughness: (voice.roughness + phenotype.roughness_delta).clamp(0.0, 1.0),
            gain: (request.gain * 1.65).clamp(0.0, voice.maximum_loudness),
            pan: request.pan.clamp(-1.0, 1.0),
            pitch_scale: request.pitch_scale.clamp(0.62, 1.48),
            tempo_scale: request.tempo_scale.clamp(0.50, 1.80),
            stress: request.stress.clamp(0.0, 1.0),
            purr: request.purr
                || phenotype.purr_amount >= 0.55
                || request.style == VocalStyle::Purr
                || request.gesture == VoiceGesture::PurrHum,
            purr_rate: voice.purr_rate.clamp(22.0, 31.0),
            gesture: request.gesture,
            style: request.style,
            valence: request.valence.clamp(-1.0, 1.0),
            arousal: request.arousal.clamp(0.0, 1.0),
            fatigue: request.fatigue.clamp(0.0, 1.0),
            confidence: request.confidence.clamp(0.0, 1.0),
            attachment: request.attachment.clamp(0.0, 1.0),
            attack_multiplier: phenotype.attack_multiplier.clamp(0.62, 1.35),
            release_multiplier: phenotype.release_multiplier.clamp(0.65, 1.45),
            breath_phase_lock: phenotype.breath_phase_lock.clamp(0.0, 1.0),
            maximum_loudness: voice.maximum_loudness,
            room_mix: 0.015 + room_unit * 0.025,
        }
    }

    #[must_use]
    pub fn total_frames(&self, sample_rate: u32) -> usize {
        self.syllables[..usize::from(self.syllable_count)]
            .iter()
            .enumerate()
            .map(|(index, syllable)| {
                milliseconds_to_frames(syllable.duration_ms / self.tempo_scale, sample_rate as f32)
                    + milliseconds_to_frames_allow_zero(
                        syllable.gap_after_ms / self.tempo_scale,
                        sample_rate as f32,
                    )
                    + self.phonation_release_frames(index, sample_rate.max(1) as f32)
            })
            .sum::<usize>()
            + milliseconds_to_frames(ROOM_TAIL_MS, sample_rate.max(1) as f32)
    }

    fn phonation_release_frames(&self, syllable_index: usize, sample_rate: f32) -> usize {
        if self.syllable_count == 0 {
            return 0;
        }
        let final_syllable = syllable_index + 1 >= usize::from(self.syllable_count);
        let duration_ms = if final_syllable {
            ((PHONATION_RELEASE_BASE_MS * self.release_multiplier
                + PHONATION_RELEASE_FATIGUE_MS * self.fatigue)
                / self.tempo_scale.clamp(0.50, 1.80))
            .clamp(180.0, 420.0)
        } else {
            (INTER_SYLLABLE_RELEASE_MS * self.release_multiplier
                / self.tempo_scale.clamp(0.50, 1.80))
            .clamp(42.0, 110.0)
        };
        milliseconds_to_frames(duration_ms, sample_rate)
    }
}

#[derive(Clone, Copy)]
struct IdentityRegister {
    minimum_f0: f32,
    maximum_f0: f32,
}

const fn identity_register(style: VocalStyle, gesture: VoiceGesture) -> IdentityRegister {
    match (style, gesture) {
        (VocalStyle::Purr | VocalStyle::ContentMurmur, _) | (_, VoiceGesture::PurrHum) => {
            IdentityRegister {
                minimum_f0: 180.0,
                maximum_f0: 420.0,
            }
        }
        (VocalStyle::Startle, _) => IdentityRegister {
            minimum_f0: 300.0,
            maximum_f0: 800.0,
        },
        (VocalStyle::Frustrated, _) | (_, VoiceGesture::LowRumble) => IdentityRegister {
            minimum_f0: 180.0,
            maximum_f0: 420.0,
        },
        _ => IdentityRegister {
            minimum_f0: 220.0,
            maximum_f0: 720.0,
        },
    }
}

pub struct SynthVoice {
    commands: Arc<SpscRing<VoiceCommand, COMMAND_CAPACITY>>,
    feedback: Arc<AudioVisualBridge>,
    body_bridge: Arc<BodyVoiceBridge>,
    sample_rate: f32,
    current: Option<VoiceCommand>,
    syllable_index: usize,
    frame_in_syllable: usize,
    gap_frames_remaining: usize,
    release_frames_remaining: usize,
    release_frames_total: usize,
    tail_frames_remaining: usize,
    breath: BreathPressureController,
    glottis: HybridLfGlottis,
    tract: DynamicTract,
    body_resonance: LiquidBodyResonance,
    aspiration_noise: NoiseSource,
    constriction_noise: NoiseSource,
    cycle_noise: NoiseSource,
    slosh_noise: NoiseSource,
    purr_noise: NoiseSource,
    spectral_tilt: OnePoleLowPass,
    voice_low_cut: OnePoleLowPass,
    dc_blocker: DcBlocker,
    room: EarlyReflections,
    body_target: BodyVoiceFrame,
    body_current: BodyVoiceFrame,
    control_interval: usize,
    control_countdown: usize,
    tract_back_pressure: f32,
    last_glottal_openness: f32,
    emitted_energy: f32,
    purr_frames_until_event: usize,
    purr_closure_remaining: usize,
    purr_closure_total: usize,
    purr_aspiration_remaining: usize,
    purr_aspiration_total: usize,
    purr_event_amplitude: f32,
    purr_group_remaining: usize,
    purr_group_exhale: bool,
    diagnostics: VoiceDiagnostics,
    last_pan: f32,
}

impl SynthVoice {
    #[must_use]
    pub fn new(commands: Arc<SpscRing<VoiceCommand, COMMAND_CAPACITY>>, sample_rate: u32) -> Self {
        Self::with_bridges(
            commands,
            sample_rate,
            Arc::new(AudioVisualBridge::default()),
            Arc::new(BodyVoiceBridge::default()),
        )
    }

    #[must_use]
    pub fn with_feedback(
        commands: Arc<SpscRing<VoiceCommand, COMMAND_CAPACITY>>,
        sample_rate: u32,
        feedback: Arc<AudioVisualBridge>,
    ) -> Self {
        Self::with_bridges(
            commands,
            sample_rate,
            feedback,
            Arc::new(BodyVoiceBridge::default()),
        )
    }

    #[must_use]
    pub fn with_bridges(
        commands: Arc<SpscRing<VoiceCommand, COMMAND_CAPACITY>>,
        sample_rate: u32,
        feedback: Arc<AudioVisualBridge>,
        body_bridge: Arc<BodyVoiceBridge>,
    ) -> Self {
        let sample_rate = sample_rate.max(1) as f32;
        Self {
            commands,
            feedback,
            body_bridge,
            sample_rate,
            current: None,
            syllable_index: 0,
            frame_in_syllable: 0,
            gap_frames_remaining: 0,
            release_frames_remaining: 0,
            release_frames_total: 1,
            tail_frames_remaining: 0,
            breath: BreathPressureController::default(),
            glottis: HybridLfGlottis::new(sample_rate),
            tract: DynamicTract::new(sample_rate),
            body_resonance: LiquidBodyResonance::new(sample_rate),
            aspiration_noise: NoiseSource::new(1),
            constriction_noise: NoiseSource::new(2),
            cycle_noise: NoiseSource::new(3),
            slosh_noise: NoiseSource::new(4),
            purr_noise: NoiseSource::new(5),
            spectral_tilt: OnePoleLowPass::default(),
            voice_low_cut: OnePoleLowPass::default(),
            dc_blocker: DcBlocker::default(),
            room: EarlyReflections::new(sample_rate),
            body_target: BodyVoiceFrame::default(),
            body_current: BodyVoiceFrame::default(),
            control_interval: (sample_rate / CONTROL_RATE_HZ).round().max(1.0) as usize,
            control_countdown: 0,
            tract_back_pressure: 0.0,
            last_glottal_openness: 0.55,
            emitted_energy: 0.0,
            purr_frames_until_event: 0,
            purr_closure_remaining: 0,
            purr_closure_total: 1,
            purr_aspiration_remaining: 0,
            purr_aspiration_total: 1,
            purr_event_amplitude: 0.0,
            purr_group_remaining: 0,
            purr_group_exhale: true,
            diagnostics: VoiceDiagnostics::default(),
            last_pan: 0.0,
        }
    }

    pub fn begin_callback(&mut self) {
        self.body_target = self.body_bridge.snapshot_or(self.body_target);
    }

    #[must_use]
    pub fn diagnostics(&self) -> VoiceDiagnostics {
        self.diagnostics.normalized()
    }

    pub fn next_stereo_frame(&mut self) -> [f32; 2] {
        if self.current.is_none() {
            if self.tail_frames_remaining > 0 {
                self.tail_frames_remaining -= 1;
                let frame = self.render_unvoiced_tail(self.last_pan, 0.76);
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
            self.publish_feedback(0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0);
            let command = self.current.expect("gap belongs to active command");
            return self.render_unvoiced_tail(command.pan, command.maximum_loudness);
        }
        let command = self.current.expect("command is active");
        if self.release_frames_remaining > 0 {
            return self.render_phonation_release(command);
        }
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
            self.release_frames_total =
                command.phonation_release_frames(self.syllable_index, self.sample_rate);
            self.release_frames_remaining = self.release_frames_total;
            return self.next_stereo_frame();
        }
        let progress = self.frame_in_syllable as f32 / total_frames.saturating_sub(1).max(1) as f32;
        self.body_current = smooth_body_frame(self.body_current, self.body_target, 0.0025);
        if self.control_countdown == 0 {
            self.tract
                .set_targets(syllable.gesture, syllable.mouth_open, self.body_current);
            self.body_resonance
                .set_targets(self.body_current, syllable.gesture);
            self.control_countdown = self.control_interval;
            self.diagnostics.observe_control();
        }
        self.control_countdown = self.control_countdown.saturating_sub(1);
        let (purr_closure, purr_aspiration, purr_body, purr_energy) = self.process_purr(command);
        let mut gesture = syllable.gesture;
        gesture.adduction = (gesture.adduction + purr_closure * 0.28).clamp(0.0, 1.0);
        gesture.nasality =
            (gesture.nasality + if self.purr_group_exhale { 0.02 } else { 0.08 }).clamp(0.0, 1.0);
        let physical_impulse = (self.body_current.collision_impulse * 0.30
            + self.body_current.contact_impulse * 0.24
            + self.body_current.release_impulse * 0.18
            + self.body_current.detach_impulse * 0.22
            + self.body_current.remerge_impulse * 0.20
            + purr_closure * 0.10)
            .clamp(0.0, 1.0);
        let breath = self.breath.process(
            progress,
            gesture,
            command.anatomy,
            // Performance gain is an acoustic output control, not the animal's
            // entire available lung pressure.  Keep the pressure gesture alive
            // at quiet listening levels and let `command.gain` act once below.
            (0.76 + command.gain * 0.42).clamp(0.72, 1.05),
            command.arousal,
            command.fatigue,
            command.attack_multiplier,
            command.release_multiplier,
            command.breath_phase_lock,
            physical_impulse,
            self.last_glottal_openness,
            self.tract_back_pressure,
            1.0 / self.sample_rate,
        );
        self.diagnostics.observe_pressure(breath.pressure);
        let body_tension = 1.0 + self.body_current.stretch * 0.025;
        let target_f0 = (command.base_pitch_hz
            * command.pitch_scale
            * syllable.pitch_ratio(progress)
            * body_tension)
            .clamp(command.minimum_f0_hz, command.maximum_f0_hz);
        let instability = (gesture.instability
            + command.stress * 0.22
            + command.roughness * 0.12
            + self.body_current.bond_strain * 0.30
            + self.body_current.material_stress * 0.16
            + command.anatomy.instability_susceptibility * 0.12)
            .clamp(0.0, 1.0);
        let glottal = self.glottis.process(
            target_f0,
            breath.pressure,
            breath.capture,
            gesture,
            instability,
            self.tract_back_pressure,
            &mut self.cycle_noise,
        );
        if glottal.cycle_boundary {
            self.diagnostics
                .observe_cycle(glottal.f0_hz, glottal.regime);
        }
        self.last_glottal_openness = glottal.openness;
        let aspiration = self.aspiration_noise.colored(command.brightness)
            * (breath.aspiration * glottal.aspiration_gate
                + purr_aspiration
                + syllable.noisiness * breath.airflow * 0.08);
        let turbulence = self.constriction_noise.colored(command.brightness)
            * breath.airflow
            * (0.30 + gesture.constriction * 0.70);
        let tract = self
            .tract
            .process(glottal.excitation, aspiration, turbulence);
        self.tract_back_pressure = tract.back_pressure;
        let body = self.body_resonance.process(
            tract.output + purr_body * 0.045,
            self.body_current,
            self.slosh_noise.colored(0.38),
        );
        let raw =
            (tract.output + body.signal) * command.gain * syllable.amplitude.clamp(0.0, 1.0) * 6.4;
        self.frame_in_syllable += 1;
        let mono = self.post_process_mono(raw, command.maximum_loudness);
        self.emitted_energy += (mono.abs() - self.emitted_energy) * 0.035;
        self.diagnostics.observe_signal(
            mono,
            tract.oral_output,
            tract.nasal_output,
            body.energy,
            tract.coefficient_delta,
        );
        self.publish_feedback(
            self.emitted_energy,
            breath.pressure,
            glottal.openness,
            tract.mouth_aperture,
            glottal.f0_hz / command.base_pitch_hz.max(1.0),
            breath.aspiration,
            body.energy,
            purr_energy,
            glottal.regime as u8,
        );
        pan(mono, command.pan)
    }

    fn start_command(&mut self, command: VoiceCommand) {
        self.feedback.publish_started_request(command.request_id);
        self.current = Some(command);
        self.syllable_index = 0;
        self.frame_in_syllable = 0;
        self.gap_frames_remaining = 0;
        self.release_frames_remaining = 0;
        self.release_frames_total = 1;
        self.tail_frames_remaining = 0;
        self.last_pan = command.pan;
        self.breath.reset();
        self.breath.begin_syllable();
        self.glottis.reset(
            command.anatomy,
            seeded_unit(command.seed ^ 0x243f_6a88_85a3_08d3),
        );
        self.tract.reset(command.anatomy);
        self.body_resonance.reset(command.anatomy);
        self.aspiration_noise.reseed(command.seed ^ 0x11);
        self.constriction_noise.reseed(command.seed ^ 0x22);
        self.cycle_noise.reseed(command.seed ^ 0x33);
        self.slosh_noise.reseed(command.seed ^ 0x44);
        self.purr_noise.reseed(command.seed ^ 0x55);
        self.spectral_tilt
            .configure(4_800.0 + command.brightness * 1_200.0, self.sample_rate);
        self.spectral_tilt.reset();
        self.voice_low_cut.configure(
            if matches!(command.style, VocalStyle::Purr | VocalStyle::ContentMurmur) {
                42.0
            } else {
                58.0
            },
            self.sample_rate,
        );
        self.voice_low_cut.reset();
        self.dc_blocker.configure(18.0, self.sample_rate);
        self.dc_blocker.reset();
        self.room.reset();
        self.room.configure(command.room_mix);
        self.control_countdown = 0;
        self.tract_back_pressure = 0.0;
        self.last_glottal_openness = 0.55;
        self.emitted_energy = 0.0;
        self.purr_frames_until_event = 0;
        self.purr_closure_remaining = 0;
        self.purr_aspiration_remaining = 0;
        self.purr_group_remaining = 0;
        self.purr_group_exhale = true;
        self.publish_feedback(0.0, 0.0, 0.0, 0.0, 1.0, command.breathiness, 0.0, 0.0, 0);
    }

    fn process_purr(&mut self, command: VoiceCommand) -> (f32, f32, f32, f32) {
        if !command.purr {
            return (0.0, 0.0, 0.0, 0.0);
        }
        if self.purr_group_remaining == 0 {
            self.purr_group_exhale = !self.purr_group_exhale;
            let seconds = 0.6 + self.purr_noise.white().abs() * 0.8;
            self.purr_group_remaining = (seconds * self.sample_rate) as usize;
        }
        self.purr_group_remaining = self.purr_group_remaining.saturating_sub(1);
        if self.purr_frames_until_event == 0 {
            let interval_jitter =
                1.0 + self.purr_noise.white() * 0.09 + self.purr_noise.correlated(0.46) * 0.025;
            self.purr_frames_until_event = (self.sample_rate / command.purr_rate.max(1.0)
                * interval_jitter)
                .round()
                .max(1.0) as usize;
            self.purr_closure_total =
                milliseconds_to_frames(2.0 + self.purr_noise.white().abs() * 4.0, self.sample_rate);
            self.purr_closure_remaining = self.purr_closure_total;
            self.purr_aspiration_total = milliseconds_to_frames(
                8.0 + self.purr_noise.white().abs() * 17.0,
                self.sample_rate,
            );
            self.purr_aspiration_remaining = self.purr_aspiration_total;
            self.purr_event_amplitude =
                (0.58 + self.purr_noise.white() * 0.16 + command.attachment * 0.08)
                    .clamp(0.35, 0.82);
            self.diagnostics.observe_purr_event(self.sample_rate);
        }
        self.purr_frames_until_event = self.purr_frames_until_event.saturating_sub(1);
        let closure = pulse_shape(self.purr_closure_remaining, self.purr_closure_total)
            * self.purr_event_amplitude;
        let aspiration = pulse_shape(self.purr_aspiration_remaining, self.purr_aspiration_total)
            * self.purr_event_amplitude
            * if self.purr_group_exhale { 0.16 } else { 0.11 };
        self.purr_closure_remaining = self.purr_closure_remaining.saturating_sub(1);
        self.purr_aspiration_remaining = self.purr_aspiration_remaining.saturating_sub(1);
        (
            closure,
            aspiration,
            closure * 0.72,
            closure.max(aspiration * 2.0),
        )
    }

    fn render_unvoiced_tail(&mut self, pan_value: f32, maximum_loudness: f32) -> [f32; 2] {
        let tract = self.tract.process(0.0, 0.0, 0.0);
        let body = self.body_resonance.process(
            tract.output,
            self.body_current,
            self.slosh_noise.colored(0.28) * 0.05,
        );
        let mono = self.post_process_mono(tract.output + body.signal, maximum_loudness);
        self.diagnostics.observe_signal(
            mono,
            tract.oral_output,
            tract.nasal_output,
            body.energy,
            tract.coefficient_delta,
        );
        pan(mono, pan_value)
    }

    fn render_phonation_release(&mut self, command: VoiceCommand) -> [f32; 2] {
        let syllable_index = self.syllable_index.min(MAX_SYLLABLES - 1);
        let syllable = command.syllables[syllable_index];
        let remaining = self.release_frames_remaining.max(1) as f32;
        let total = self.release_frames_total.max(1) as f32;
        let remaining_normalized = (remaining / total).clamp(0.0, 1.0);
        let release_progress = 1.0 - remaining_normalized;
        // Smoothstep has zero slope at both endpoints: the first release sample
        // is continuous with the syllable and the final source sample reaches
        // silence without a click.
        let release_gain =
            remaining_normalized * remaining_normalized * (3.0 - 2.0 * remaining_normalized);

        self.body_current = smooth_body_frame(self.body_current, self.body_target, 0.0025);
        let mut gesture = syllable.gesture;
        gesture.pressure_peak *= remaining_normalized;
        gesture.adduction = (gesture.adduction * (1.0 - release_progress * 0.72)).clamp(0.0, 1.0);
        gesture.open_quotient = (gesture.open_quotient + release_progress * 0.14).clamp(0.30, 0.88);
        let mouth_target = syllable.mouth_open * remaining_normalized.sqrt();
        if self.control_countdown == 0 {
            self.tract
                .set_targets(gesture, mouth_target, self.body_current);
            self.body_resonance.set_targets(self.body_current, gesture);
            self.control_countdown = self.control_interval;
            self.diagnostics.observe_control();
        }
        self.control_countdown = self.control_countdown.saturating_sub(1);

        let breath = self.breath.process(
            0.76 + release_progress * 0.24,
            gesture,
            command.anatomy,
            (0.76 + command.gain * 0.42).clamp(0.72, 1.05),
            command.arousal,
            command.fatigue,
            command.attack_multiplier,
            command.release_multiplier,
            command.breath_phase_lock,
            0.0,
            self.last_glottal_openness,
            self.tract_back_pressure,
            1.0 / self.sample_rate,
        );
        self.diagnostics.observe_pressure(breath.pressure);
        let target_f0 = (command.base_pitch_hz * command.pitch_scale * syllable.pitch_ratio(1.0))
            .clamp(command.minimum_f0_hz, command.maximum_f0_hz);
        let instability = (gesture.instability
            + command.stress * 0.22
            + command.roughness * 0.12
            + command.anatomy.instability_susceptibility * 0.12)
            .clamp(0.0, 1.0);
        let glottal = self.glottis.process(
            target_f0,
            breath.pressure,
            breath.capture,
            gesture,
            instability,
            self.tract_back_pressure,
            &mut self.cycle_noise,
        );
        if glottal.cycle_boundary {
            self.diagnostics
                .observe_cycle(glottal.f0_hz, glottal.regime);
        }
        self.last_glottal_openness = glottal.openness;
        let aspiration = self.aspiration_noise.colored(command.brightness)
            * (breath.aspiration * glottal.aspiration_gate
                + syllable.noisiness * breath.airflow * 0.08);
        let turbulence = self.constriction_noise.colored(command.brightness)
            * breath.airflow
            * (0.30 + gesture.constriction * 0.70);
        let tract = self
            .tract
            .process(glottal.excitation, aspiration, turbulence);
        self.tract_back_pressure = tract.back_pressure;
        let body = self.body_resonance.process(
            tract.output,
            self.body_current,
            self.slosh_noise.colored(0.32) * release_gain,
        );
        let raw = (tract.output + body.signal)
            * command.gain
            * syllable.amplitude.clamp(0.0, 1.0)
            * 6.4
            * release_gain;
        let mono = self.post_process_mono(raw, command.maximum_loudness);
        self.emitted_energy += (mono.abs() - self.emitted_energy) * 0.035;
        self.diagnostics.observe_signal(
            mono,
            tract.oral_output,
            tract.nasal_output,
            body.energy,
            tract.coefficient_delta,
        );
        self.publish_feedback(
            self.emitted_energy * release_gain,
            breath.pressure,
            glottal.openness,
            tract.mouth_aperture,
            glottal.f0_hz / command.base_pitch_hz.max(1.0),
            breath.aspiration,
            body.energy,
            0.0,
            glottal.regime as u8,
        );
        self.release_frames_remaining = self.release_frames_remaining.saturating_sub(1);
        if self.release_frames_remaining == 0 {
            self.gap_frames_remaining = milliseconds_to_frames_allow_zero(
                syllable.gap_after_ms / command.tempo_scale.max(0.1),
                self.sample_rate,
            );
            self.syllable_index += 1;
            self.frame_in_syllable = 0;
            if self.syllable_index < usize::from(command.syllable_count) {
                self.breath.begin_syllable();
                self.control_countdown = 0;
            }
        }
        pan(mono, command.pan)
    }

    fn post_process_mono(&mut self, mono: f32, maximum_loudness: f32) -> f32 {
        let tilted = self.spectral_tilt.process(mono);
        let high_passed = tilted - self.voice_low_cut.process(tilted);
        let blocked = self.dc_blocker.process(high_passed);
        let saturated = blocked / (1.0 + blocked.abs() * 0.12);
        let room = self.room.process(saturated);
        let output = soft_limit(room, maximum_loudness.clamp(0.72, 0.78));
        if output.is_finite() {
            output
        } else {
            self.diagnostics.observe_non_finite_reset();
            0.0
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn publish_feedback(
        &self,
        emitted_energy: f32,
        breath_pressure: f32,
        glottal_openness: f32,
        mouth_aperture: f32,
        pitch_normalized: f32,
        aspiration: f32,
        body_resonance_energy: f32,
        purr_event_energy: f32,
        phonation_regime: u8,
    ) {
        let Some(command) = self.current else {
            self.feedback.clear();
            return;
        };
        let acoustic_activity = (emitted_energy * 24.0).clamp(0.0, 1.0);
        let pneumatic_activity =
            (breath_pressure * 0.82 + aspiration * 0.24 + body_resonance_energy * 0.12)
                .clamp(0.0, 1.0);
        let visual_envelope = acoustic_activity.max(pneumatic_activity);
        self.feedback.publish(AudioVisualFeedback {
            active: visual_envelope > 0.015,
            request_id: command.request_id,
            motif_id: command.motif_id,
            syllable_index: self.syllable_index.min(u8::MAX as usize) as u8,
            emitted_energy: emitted_energy.clamp(0.0, 1.0),
            breath_pressure: breath_pressure.clamp(0.0, 1.0),
            glottal_openness: glottal_openness.clamp(0.0, 1.0),
            mouth_aperture: mouth_aperture.clamp(0.0, 1.0),
            pitch_normalized: pitch_normalized.clamp(0.25, 4.0),
            aspiration: aspiration.clamp(0.0, 1.0),
            body_resonance_energy: body_resonance_energy.clamp(0.0, 1.0),
            purr_event_energy: purr_event_energy.clamp(0.0, 1.0),
            phonation_regime,
            // Raw PCM energy is intentionally quiet and therefore unsuitable
            // as a direct 0..1 animation weight. This normalized physical
            // envelope keeps the mouth coupled to audible energy and airflow.
            envelope: visual_envelope,
            mouth_open: mouth_aperture.clamp(0.0, 1.0),
            noisiness: aspiration.clamp(0.0, 1.0),
            purr: purr_event_energy.clamp(0.0, 1.0),
        });
    }
}

fn smooth_body_frame(
    current: BodyVoiceFrame,
    target: BodyVoiceFrame,
    amount: f32,
) -> BodyVoiceFrame {
    let smooth = |a: f32, b: f32| a + (b - a) * amount;
    BodyVoiceFrame {
        main_mass_ratio: smooth(current.main_mass_ratio, target.main_mass_ratio),
        detached_mass_ratio: smooth(current.detached_mass_ratio, target.detached_mass_ratio),
        component_count: target.component_count,
        shape_aspect_ratio: smooth(current.shape_aspect_ratio, target.shape_aspect_ratio),
        stretch: smooth(current.stretch, target.stretch),
        compression: smooth(current.compression, target.compression),
        bond_strain: smooth(current.bond_strain, target.bond_strain),
        material_stress: smooth(current.material_stress, target.material_stress),
        contact_area: smooth(current.contact_area, target.contact_area),
        slosh_energy: smooth(current.slosh_energy, target.slosh_energy),
        internal_speed: smooth(current.internal_speed, target.internal_speed),
        collision_impulse: smooth(current.collision_impulse, target.collision_impulse),
        contact_impulse: smooth(current.contact_impulse, target.contact_impulse),
        release_impulse: smooth(current.release_impulse, target.release_impulse),
        detach_impulse: smooth(current.detach_impulse, target.detach_impulse),
        remerge_impulse: smooth(current.remerge_impulse, target.remerge_impulse),
    }
    .sanitized()
}

fn pulse_shape(remaining: usize, total: usize) -> f32 {
    if remaining == 0 || total == 0 {
        return 0.0;
    }
    let progress = 1.0 - remaining as f32 / total as f32;
    (4.0 * progress * (1.0 - progress)).clamp(0.0, 1.0)
}

fn pan(mono: f32, pan: f32) -> [f32; 2] {
    let pan = pan.clamp(-1.0, 1.0);
    [
        mono * ((1.0 - pan) * 0.5).sqrt(),
        mono * ((1.0 + pan) * 0.5).sqrt(),
    ]
}

fn milliseconds_to_frames(milliseconds: f32, sample_rate: f32) -> usize {
    (milliseconds.max(0.0) * sample_rate / 1_000.0)
        .round()
        .max(1.0) as usize
}

fn milliseconds_to_frames_allow_zero(milliseconds: f32, sample_rate: f32) -> usize {
    (milliseconds.max(0.0) * sample_rate / 1_000.0).round() as usize
}

fn splitmix64(mut value: u64) -> u64 {
    value = value.wrapping_add(0x9e37_79b9_7f4a_7c15);
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

fn seeded_unit(seed: u64) -> f32 {
    ((splitmix64(seed) >> 40) as u32) as f32 / 0x00ff_ffff as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(motif_id: u64, style: VocalStyle) -> VocalRequest {
        VocalRequest {
            motif_id,
            performance_seed: 17,
            gain: 0.24,
            pan: 0.0,
            pitch_scale: 1.0,
            tempo_scale: 1.0,
            stress: 0.0,
            purr: style == VocalStyle::Purr,
            gesture: VoiceGesture::WarmChuff,
            priority: 128,
            style,
            valence: 0.3,
            arousal: 0.25,
            fatigue: 0.0,
            confidence: 0.8,
            attachment: 0.5,
            rhythm_intervals: [0.0; 8],
            phenotype: Default::default(),
        }
    }

    #[test]
    fn prepared_calls_use_bounded_mammalian_register() {
        let mut voice = lifecore::Genome::from_seed(41).voice;
        voice.base_pitch_hz = 120.0;
        let motif = lifecore::generate_initial_motifs(&voice)[0].clone();
        let command = VoiceCommand::prepare(
            &voice,
            &motif,
            &request(motif.id, VocalStyle::SocialContact),
        );
        assert_eq!(command.base_pitch_hz, 220.0);
        assert_eq!(command.minimum_f0_hz, 85.0);
        assert_eq!(command.maximum_f0_hz, 1_600.0);
    }

    #[test]
    fn grounded_rhythm_request_shapes_inter_onset_intervals() {
        let voice = lifecore::Genome::from_seed(42).voice;
        let motif = lifecore::generate_initial_motifs(&voice)[0].clone();
        let mut request = request(motif.id, VocalStyle::RhythmMimic);
        request.rhythm_intervals = [0.50, 1.00, 0.75, 1.75, 0.0, 0.0, 0.0, 0.0];
        let command = VoiceCommand::prepare(&voice, &motif, &request);
        assert_eq!(command.syllable_count, 5);
        let actual = std::array::from_fn::<_, 4, _>(|index| {
            (command.syllables[index].duration_ms + command.syllables[index].gap_after_ms) / 220.0
        });
        let mean_actual = actual.iter().sum::<f32>() / actual.len() as f32;
        let covariance = actual
            .iter()
            .zip(request.rhythm_intervals)
            .take(4)
            .map(|(actual, requested)| (*actual - mean_actual) * (requested - 1.0))
            .sum::<f32>();
        let actual_energy = actual
            .iter()
            .map(|actual| (*actual - mean_actual).powi(2))
            .sum::<f32>();
        let requested_energy = request.rhythm_intervals[..4]
            .iter()
            .map(|requested| (*requested - 1.0).powi(2))
            .sum::<f32>();
        assert!(covariance / (actual_energy * requested_energy).sqrt() >= 0.85);
    }

    #[test]
    fn continuous_voice_phenotype_shapes_an_existing_call_even_when_gate_is_closed() {
        let voice = lifecore::Genome::from_seed(45).voice;
        let motif = lifecore::generate_initial_motifs(&voice)[0].clone();
        let neutral = VoiceCommand::prepare(
            &voice,
            &motif,
            &request(motif.id, VocalStyle::SocialContact),
        );
        let mut shaped_request = request(motif.id, VocalStyle::SocialContact);
        shaped_request.phenotype = lifecore::VoicePhenotypeActuation {
            enabled: false,
            formant_scale_multiplier: 1.06,
            breathiness_delta: 0.18,
            attack_multiplier: 0.62,
            release_multiplier: 1.45,
            breath_phase_lock: 0.9,
            phrase_contour: lifecore::PhraseContourWeights {
                curiosity_question: 1.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let shaped = VoiceCommand::prepare(&voice, &motif, &shaped_request);
        assert!(shaped.anatomy.tract_length < neutral.anatomy.tract_length);
        assert!(shaped.anatomy.glottal_leak > neutral.anatomy.glottal_leak);
        assert_eq!(shaped.attack_multiplier, 0.62);
        assert_eq!(shaped.release_multiplier, 1.45);
        assert_eq!(shaped.breath_phase_lock, 0.9);
        assert!(shaped.syllables[0].pitch_ratio(0.95) > neutral.syllables[0].pitch_ratio(0.95));
    }

    #[test]
    fn purr_is_irregular_event_excitation_not_final_bus_amplitude_modulation() {
        let voice = lifecore::Genome::from_seed(43).voice;
        let motif = lifecore::generate_initial_motifs(&voice)
            .into_iter()
            .find(|motif| motif.family == VocalFamily::Purr)
            .unwrap();
        let command = VoiceCommand::prepare(&voice, &motif, &request(motif.id, VocalStyle::Purr));
        let commands = Arc::new(SpscRing::new());
        commands.push(command).unwrap();
        let mut synth = SynthVoice::new(commands, 48_000);
        for _ in 0..command.total_frames(48_000) {
            let frame = synth.next_stereo_frame();
            assert!(frame.into_iter().all(f32::is_finite));
        }
        let diagnostics = synth.diagnostics();
        assert!(diagnostics.purr_event_count > 2);
        assert!(diagnostics.purr_interval_cv > 0.005);
    }

    #[test]
    fn final_syllable_drains_through_phonation_release_without_a_hard_cut() {
        let voice = lifecore::Genome::from_seed(47).voice;
        let mut motif = lifecore::generate_initial_motifs(&voice)[0].clone();
        motif.syllables.truncate(1);
        motif.syllables[0].gap_after_ms = 0.0;
        let command = VoiceCommand::prepare(
            &voice,
            &motif,
            &request(motif.id, VocalStyle::SocialContact),
        );
        let phrase_frames = command.syllables[..usize::from(command.syllable_count)]
            .iter()
            .map(|syllable| {
                milliseconds_to_frames(syllable.duration_ms / command.tempo_scale, 48_000.0)
                    + milliseconds_to_frames_allow_zero(
                        syllable.gap_after_ms / command.tempo_scale,
                        48_000.0,
                    )
            })
            .sum::<usize>();
        let release_frames = command.phonation_release_frames(0, 48_000.0);
        let commands = Arc::new(SpscRing::new());
        commands.push(command).unwrap();
        let mut synth = SynthVoice::new(commands, 48_000);
        let mono = (0..command.total_frames(48_000))
            .map(|_| {
                let frame = synth.next_stereo_frame();
                (frame[0] + frame[1]) * 0.5
            })
            .collect::<Vec<_>>();

        let transition_jump = (mono[phrase_frames] - mono[phrase_frames - 1]).abs();
        assert!(
            transition_jump < command.maximum_loudness * 0.25,
            "final syllable still cuts at release boundary: jump={transition_jump}"
        );
        let release = &mono[phrase_frames..phrase_frames + release_frames];
        let quarter = (release.len() / 4).max(1);
        let early_rms = rms(&release[..quarter]);
        let late_rms = rms(&release[release.len() - quarter..]);
        assert!(
            early_rms > 0.000_01,
            "release carries no physical voice energy"
        );
        assert!(
            late_rms < early_rms * 0.40,
            "release does not decay: early={early_rms}, late={late_rms}"
        );
        assert!(
            mono[mono.len() - 64..]
                .iter()
                .all(|sample| sample.abs() < 0.002),
            "room tail must settle before the stream returns digital silence"
        );
    }

    #[test]
    fn every_syllable_enters_a_physical_release_before_silence() {
        let voice = lifecore::Genome::from_seed(53).voice;
        let mut motif = lifecore::generate_initial_motifs(&voice)[0].clone();
        motif.syllables.truncate(2);
        for syllable in &mut motif.syllables {
            syllable.gap_after_ms = 24.0;
        }
        let command = VoiceCommand::prepare(
            &voice,
            &motif,
            &request(motif.id, VocalStyle::SocialContact),
        );
        let commands = Arc::new(SpscRing::new());
        commands.push(command).unwrap();
        let feedback = Arc::new(AudioVisualBridge::default());
        let mut synth = SynthVoice::with_feedback(commands, 48_000, Arc::clone(&feedback));
        let first_duration = milliseconds_to_frames(
            command.syllables[0].duration_ms / command.tempo_scale,
            48_000.0,
        );
        for _ in 0..=first_duration {
            let _ = synth.next_stereo_frame();
        }
        let snapshot = feedback.snapshot();
        assert!(
            snapshot.active,
            "inter-syllable release was replaced by digital silence"
        );
        assert!(snapshot.envelope > 0.02);
        assert_eq!(snapshot.syllable_index, 0);
    }

    fn rms(samples: &[f32]) -> f32 {
        (samples.iter().map(|sample| sample * sample).sum::<f32>() / samples.len().max(1) as f32)
            .sqrt()
    }
}
