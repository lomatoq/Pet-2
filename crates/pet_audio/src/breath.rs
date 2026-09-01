use lifecore::{VocalGesture, VoiceAnatomy};

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct BreathFrame {
    pub(crate) pressure: f32,
    pub(crate) airflow: f32,
    pub(crate) capture: f32,
    pub(crate) aspiration: f32,
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct BreathPressureController {
    lung_pressure: f32,
    subglottal_pressure: f32,
    airflow: f32,
    onset_capture: f32,
}

impl BreathPressureController {
    pub(crate) fn reset(&mut self) {
        *self = Self::default();
    }

    pub(crate) fn begin_syllable(&mut self) {
        self.onset_capture = 0.0;
        self.lung_pressure *= 0.42;
        self.subglottal_pressure *= 0.30;
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn process(
        &mut self,
        progress: f32,
        gesture: VocalGesture,
        anatomy: VoiceAnatomy,
        performance_gain: f32,
        arousal: f32,
        fatigue: f32,
        attack_multiplier: f32,
        release_multiplier: f32,
        breath_phase_lock: f32,
        physical_impulse: f32,
        glottal_open_area: f32,
        tract_back_pressure: f32,
        dt: f32,
    ) -> BreathFrame {
        let progress = progress.clamp(0.0, 1.0);
        let fatigue = fatigue.clamp(0.0, 1.0);
        let phase_lock = breath_phase_lock.clamp(0.0, 1.0);
        let prelude_end = ((0.045 + (1.0 - gesture.adduction) * 0.035) * (1.0 - phase_lock * 0.30))
            .clamp(0.025, 0.09);
        let release_start = (0.72 + gesture.pressure_peak * 0.14).clamp(0.68, 0.88);
        let phase_pressure = if progress < prelude_end {
            0.12 + 0.48 * (progress / prelude_end)
        } else if progress < release_start {
            let onset = ((progress - prelude_end) / 0.12).clamp(0.0, 1.0);
            0.60 + onset * 0.40
        } else {
            let release =
                ((progress - release_start) / (1.0 - release_start).max(0.01)).clamp(0.0, 1.0);
            1.0 - release * release * 0.92
        };
        let sustainable = (1.0 - fatigue * 0.52).clamp(0.34, 1.0);
        let p_target = (gesture.pressure_peak
            * performance_gain.clamp(0.05, 1.2)
            * (0.90 + arousal.clamp(0.0, 1.0) * 0.18)
            * sustainable
            * phase_pressure
            + physical_impulse.clamp(0.0, 1.0) * 0.13)
            .clamp(0.0, 1.25);
        let attack_tau = ((0.010 + (1.0 - gesture.pressure_peak) * 0.026 + fatigue * 0.018)
            * attack_multiplier.clamp(0.62, 1.35))
        .clamp(0.006, 0.075);
        let release_tau = ((0.042 + fatigue * 0.070 + anatomy.tract_compliance * 0.018)
            * release_multiplier.clamp(0.65, 1.45))
        .clamp(0.025, 0.180);
        let tau = if p_target > self.lung_pressure {
            attack_tau
        } else {
            release_tau
        };
        self.lung_pressure += (p_target - self.lung_pressure) * (1.0 - (-dt / tau).exp());
        let leak = (anatomy.glottal_leak * 0.24 + fatigue * 0.18).clamp(0.0, 0.35);
        let differential = (self.lung_pressure - tract_back_pressure.abs() * 0.10).max(0.0);
        self.airflow = (glottal_open_area.clamp(0.0, 1.0) + leak) * differential.sqrt() * 0.62;
        let pressure_target =
            (self.lung_pressure - self.airflow * (0.10 + leak * 0.16)).clamp(0.0, 1.25);
        self.subglottal_pressure +=
            (pressure_target - self.subglottal_pressure) * (1.0 - (-dt / 0.006).exp());
        let threshold = (0.105
            + anatomy.fold_mass * 0.055
            + (1.0 - gesture.adduction) * 0.075
            + fatigue * 0.035)
            .clamp(0.08, 0.26);
        let above = ((self.subglottal_pressure - threshold) / 0.10).clamp(0.0, 1.0);
        let capture_tau = 0.005 + (1.0 - gesture.adduction) * 0.020 + fatigue * 0.008;
        self.onset_capture +=
            (above - self.onset_capture) * (1.0 - (-dt / capture_tau.max(0.004)).exp());
        let aspiration = self.airflow
            * (anatomy.glottal_leak * 0.50 + (1.0 - self.onset_capture) * 0.34)
            * (0.45 + gesture.open_quotient * 0.55);
        if !self.subglottal_pressure.is_finite() || !self.airflow.is_finite() {
            self.reset();
        }
        BreathFrame {
            pressure: self.subglottal_pressure.clamp(0.0, 1.25),
            airflow: self.airflow.clamp(0.0, 1.5),
            capture: self.onset_capture.clamp(0.0, 1.0),
            aspiration: aspiration.clamp(0.0, 1.0),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pressure_must_build_before_capture() {
        let mut controller = BreathPressureController::default();
        let anatomy = lifecore::Genome::from_seed(7).voice.anatomy;
        let mut first_capture = None;
        for frame in 0..4_800 {
            let state = controller.process(
                frame as f32 / 4_799.0,
                VocalGesture::default(),
                anatomy,
                0.8,
                0.3,
                0.0,
                1.0,
                1.0,
                0.0,
                0.0,
                0.5,
                0.0,
                1.0 / 48_000.0,
            );
            if first_capture.is_none() && state.capture > 0.5 {
                first_capture = Some(frame);
            }
        }
        assert!(first_capture.is_some_and(|frame| frame > 250));
    }
}
