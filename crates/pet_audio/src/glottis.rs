use lifecore::{VocalGesture, VoiceAnatomy};

use crate::{
    noise::NoiseSource,
    phonation::{PhonationRegime, PhonationState},
};

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct GlottalFrame {
    pub(crate) excitation: f32,
    pub(crate) openness: f32,
    pub(crate) aspiration_gate: f32,
    pub(crate) f0_hz: f32,
    pub(crate) cycle_boundary: bool,
    pub(crate) regime: PhonationRegime,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct HybridLfGlottis {
    sample_rate: f32,
    anatomy: VoiceAnatomy,
    phase: f32,
    secondary_phase: f32,
    previous_flow: f32,
    previous_subsample: f32,
    period_state: f32,
    amplitude_state: f32,
    quotient_state: f32,
    current_open_quotient: f32,
    cycle_amplitude: f32,
    cycle_index: u64,
    phonation: PhonationState,
}

impl HybridLfGlottis {
    pub(crate) fn new(sample_rate: f32) -> Self {
        Self {
            sample_rate: sample_rate.max(1.0),
            anatomy: VoiceAnatomy::default(),
            phase: 0.0,
            secondary_phase: 0.0,
            previous_flow: 0.0,
            previous_subsample: 0.0,
            period_state: 0.0,
            amplitude_state: 0.0,
            quotient_state: 0.0,
            current_open_quotient: 0.58,
            cycle_amplitude: 1.0,
            cycle_index: 0,
            phonation: PhonationState::default(),
        }
    }

    pub(crate) fn reset(&mut self, anatomy: VoiceAnatomy, initial_phase: f32) {
        self.anatomy = anatomy;
        self.phase = initial_phase.rem_euclid(1.0);
        self.secondary_phase = (initial_phase * 0.731 + 0.173).fract();
        self.previous_flow = 0.0;
        self.previous_subsample = 0.0;
        self.period_state = 0.0;
        self.amplitude_state = 0.0;
        self.quotient_state = 0.0;
        self.current_open_quotient = 0.58;
        self.cycle_amplitude = 1.0;
        self.cycle_index = 0;
        self.phonation.reset();
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn process(
        &mut self,
        target_f0_hz: f32,
        pressure: f32,
        capture: f32,
        gesture: VocalGesture,
        instability_drive: f32,
        tract_back_pressure: f32,
        cycle_noise: &mut NoiseSource,
    ) -> GlottalFrame {
        let effective_pressure = (pressure
            - self.anatomy.source_feedback.clamp(0.02, 0.18) * tract_back_pressure.abs().min(1.0))
        .max(0.0);
        let mass_shift = 1.06 - self.anatomy.fold_mass * 0.12;
        let tension_shift =
            0.94 + (self.anatomy.neutral_tension * 0.55 + gesture.adduction * 0.20) * 0.10;
        let jitter_range =
            (0.0015 + instability_drive.clamp(0.0, 1.0) * 0.0205).clamp(0.0015, 0.05);
        let mut frequency =
            (target_f0_hz * mass_shift * tension_shift * (1.0 + self.period_state * jitter_range))
                .clamp(85.0, 1_600.0);
        if self.phonation.regime() == PhonationRegime::Subharmonic2 && self.cycle_index & 1 == 1 {
            frequency *= 0.985;
        }
        let oversample_rate = self.sample_rate * 2.0;
        let mut subsamples = [0.0; 2];
        let mut openness = 0.0;
        let mut boundary = false;
        for subsample in &mut subsamples {
            let increment = (frequency / oversample_rate).clamp(0.0, 0.20);
            self.phase += increment;
            if self.phase >= 1.0 {
                self.phase -= 1.0;
                boundary = true;
                self.cycle_index = self.cycle_index.wrapping_add(1);
                self.update_cycle(gesture, instability_drive, effective_pressure, cycle_noise);
            }
            let regime = self.phonation.regime();
            let voice_break = regime == PhonationRegime::VoiceBreak;
            let oq = self.current_open_quotient.clamp(0.30, 0.82);
            let sharpness = gesture.closure_sharpness.clamp(0.0, 1.0);
            let flow = if capture > 0.015 && !voice_break {
                lf_flow(self.phase, oq, sharpness)
            } else {
                0.0
            };
            openness += flow;
            let derivative = (flow - self.previous_flow) / increment.max(0.000_1);
            self.previous_flow = flow;
            let shimmer_range = 0.003 + instability_drive.clamp(0.0, 1.0) * 0.037;
            let pressure_gain = effective_pressure.sqrt()
                * capture
                * (0.20 + gesture.adduction * 0.80)
                * self.cycle_amplitude
                * (1.0 + self.amplitude_state * shimmer_range);
            let mut excitation = derivative.clamp(-3.0, 3.0) * pressure_gain * 0.30;
            if regime == PhonationRegime::Aperiodic {
                excitation *= 0.78 + cycle_noise.white().abs() * 0.30;
            } else if regime == PhonationRegime::Subharmonic2 && self.cycle_index & 1 == 1 {
                excitation *= 0.78;
            } else if regime == PhonationRegime::Biphonic {
                self.secondary_phase = (self.secondary_phase
                    + increment * (1.35 + self.anatomy.instability_susceptibility * 0.45))
                    .fract();
                let secondary = lf_flow(
                    self.secondary_phase,
                    (oq + 0.05).clamp(0.30, 0.82),
                    sharpness * 0.75,
                );
                excitation += (secondary - flow) * pressure_gain * 0.08;
            }
            *subsample = if excitation.is_finite() {
                excitation.clamp(-2.0, 2.0)
            } else {
                0.0
            };
        }
        let excitation =
            self.previous_subsample * 0.25 + subsamples[0] * 0.50 + subsamples[1] * 0.25;
        self.previous_subsample = subsamples[1];
        let openness = (openness * 0.5).clamp(0.0, 1.0);
        GlottalFrame {
            excitation,
            openness,
            aspiration_gate: (openness * (0.42 + self.anatomy.glottal_leak * 0.58)).clamp(0.0, 1.0),
            f0_hz: frequency,
            cycle_boundary: boundary,
            regime: self.phonation.regime(),
        }
    }

    fn update_cycle(
        &mut self,
        gesture: VocalGesture,
        instability_drive: f32,
        pressure: f32,
        noise: &mut NoiseSource,
    ) {
        self.period_state = self.period_state * 0.82 + noise.white() * 0.18;
        self.amplitude_state = self.amplitude_state * 0.76 + noise.white() * 0.24;
        self.quotient_state = self.quotient_state * 0.86 + noise.white() * 0.14;
        let quotient_range = 0.010 + instability_drive.clamp(0.0, 1.0) * 0.050;
        self.current_open_quotient = (gesture.open_quotient
            + self.quotient_state * quotient_range
            + self.anatomy.glottal_leak * 0.025)
            .clamp(0.30, 0.82);
        self.cycle_amplitude =
            (1.0 + self.amplitude_state * (0.012 + instability_drive * 0.028)).clamp(0.90, 1.10);
        let tension = (self.anatomy.neutral_tension + gesture.adduction * 0.40).clamp(0.0, 1.0);
        self.phonation
            .update_cycle(instability_drive, pressure, tension, noise.white());
    }
}

fn lf_flow(phase: f32, open_quotient: f32, closure_sharpness: f32) -> f32 {
    if phase < open_quotient {
        let x = (phase / open_quotient.max(0.001)).clamp(0.0, 1.0);
        x * x * (3.0 - 2.0 * x) * (1.0 - closure_sharpness * x * 0.10)
    } else {
        let y = ((phase - open_quotient) / (1.0 - open_quotient).max(0.001)).clamp(0.0, 1.0);
        let smooth_return = (1.0 - y) * (1.0 - y) * (1.0 + 2.0 * y);
        smooth_return * (1.0 - closure_sharpness * y * 0.36)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cycles_are_related_but_not_sample_identical() {
        let anatomy = lifecore::Genome::from_seed(88).voice.anatomy;
        let mut glottis = HybridLfGlottis::new(48_000.0);
        glottis.reset(anatomy, 0.0);
        let mut noise = NoiseSource::new(44);
        let mut samples = Vec::new();
        for _ in 0..2_000 {
            samples.push(
                glottis
                    .process(
                        240.0,
                        0.62,
                        1.0,
                        VocalGesture::default(),
                        0.18,
                        0.0,
                        &mut noise,
                    )
                    .excitation,
            );
        }
        let period = 200;
        let difference = samples[..period]
            .iter()
            .zip(&samples[period..period * 2])
            .map(|(a, b)| (a - b).abs())
            .sum::<f32>();
        assert!(difference > 0.001);
        assert!(samples.iter().all(|sample| sample.is_finite()));
    }
}
