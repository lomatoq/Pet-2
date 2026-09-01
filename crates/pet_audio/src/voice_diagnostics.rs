use serde::Serialize;

use crate::phonation::PhonationRegime;

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct VoiceDiagnostics {
    pub rendered_frames: u64,
    pub glottal_cycles: u64,
    pub control_updates: u64,
    pub pressure_onset_frame: Option<u64>,
    pub minimum_f0_hz: f32,
    pub maximum_f0_hz: f32,
    pub maximum_sample: f32,
    pub dc_mean: f32,
    pub maximum_discontinuity: f32,
    pub maximum_tract_coefficient_delta: f32,
    pub oral_energy: f32,
    pub nasal_energy: f32,
    pub body_resonance_energy: f32,
    pub purr_event_count: u64,
    pub purr_interval_mean_ms: f32,
    pub purr_interval_cv: f32,
    pub phonation_regime_counts: [u64; 6],
    pub non_finite_resets: u64,
    #[serde(skip)]
    sample_sum: f64,
    #[serde(skip)]
    previous_sample: f32,
    #[serde(skip)]
    purr_interval_sum: f64,
    #[serde(skip)]
    purr_interval_squared_sum: f64,
    #[serde(skip)]
    purr_interval_count: u64,
    #[serde(skip)]
    last_purr_frame: Option<u64>,
}

impl Default for VoiceDiagnostics {
    fn default() -> Self {
        Self {
            rendered_frames: 0,
            glottal_cycles: 0,
            control_updates: 0,
            pressure_onset_frame: None,
            minimum_f0_hz: f32::INFINITY,
            maximum_f0_hz: 0.0,
            maximum_sample: 0.0,
            dc_mean: 0.0,
            maximum_discontinuity: 0.0,
            maximum_tract_coefficient_delta: 0.0,
            oral_energy: 0.0,
            nasal_energy: 0.0,
            body_resonance_energy: 0.0,
            purr_event_count: 0,
            purr_interval_mean_ms: 0.0,
            purr_interval_cv: 0.0,
            phonation_regime_counts: [0; 6],
            non_finite_resets: 0,
            sample_sum: 0.0,
            previous_sample: 0.0,
            purr_interval_sum: 0.0,
            purr_interval_squared_sum: 0.0,
            purr_interval_count: 0,
            last_purr_frame: None,
        }
    }
}

impl VoiceDiagnostics {
    pub(crate) fn observe_control(&mut self) {
        self.control_updates = self.control_updates.saturating_add(1);
    }

    pub(crate) fn observe_cycle(&mut self, f0_hz: f32, regime: PhonationRegime) {
        self.glottal_cycles = self.glottal_cycles.saturating_add(1);
        if f0_hz.is_finite() {
            self.minimum_f0_hz = self.minimum_f0_hz.min(f0_hz);
            self.maximum_f0_hz = self.maximum_f0_hz.max(f0_hz);
        }
        self.phonation_regime_counts[regime as usize] =
            self.phonation_regime_counts[regime as usize].saturating_add(1);
    }

    pub(crate) fn observe_pressure(&mut self, pressure: f32) {
        if self.pressure_onset_frame.is_none() && pressure > 0.11 {
            self.pressure_onset_frame = Some(self.rendered_frames);
        }
    }

    pub(crate) fn observe_signal(
        &mut self,
        sample: f32,
        oral: f32,
        nasal: f32,
        body_energy: f32,
        coefficient_delta: f32,
    ) {
        self.rendered_frames = self.rendered_frames.saturating_add(1);
        if !sample.is_finite() {
            self.non_finite_resets = self.non_finite_resets.saturating_add(1);
            return;
        }
        self.maximum_sample = self.maximum_sample.max(sample.abs());
        self.maximum_discontinuity = self
            .maximum_discontinuity
            .max((sample - self.previous_sample).abs());
        self.previous_sample = sample;
        self.sample_sum += f64::from(sample);
        self.dc_mean = (self.sample_sum / self.rendered_frames.max(1) as f64) as f32;
        self.oral_energy += oral * oral;
        self.nasal_energy += nasal * nasal;
        self.body_resonance_energy = self.body_resonance_energy.max(body_energy);
        self.maximum_tract_coefficient_delta =
            self.maximum_tract_coefficient_delta.max(coefficient_delta);
    }

    pub(crate) fn observe_purr_event(&mut self, sample_rate: f32) {
        self.purr_event_count = self.purr_event_count.saturating_add(1);
        if let Some(previous) = self.last_purr_frame {
            let interval_ms = (self.rendered_frames.saturating_sub(previous)) as f64 * 1_000.0
                / f64::from(sample_rate.max(1.0));
            self.purr_interval_sum += interval_ms;
            self.purr_interval_squared_sum += interval_ms * interval_ms;
            self.purr_interval_count = self.purr_interval_count.saturating_add(1);
            let mean = self.purr_interval_sum / self.purr_interval_count as f64;
            let variance = (self.purr_interval_squared_sum / self.purr_interval_count as f64
                - mean * mean)
                .max(0.0);
            self.purr_interval_mean_ms = mean as f32;
            self.purr_interval_cv = if mean > f64::EPSILON {
                (variance.sqrt() / mean) as f32
            } else {
                0.0
            };
        }
        self.last_purr_frame = Some(self.rendered_frames);
    }

    pub(crate) fn observe_non_finite_reset(&mut self) {
        self.non_finite_resets = self.non_finite_resets.saturating_add(1);
    }

    #[must_use]
    pub fn normalized(mut self) -> Self {
        if !self.minimum_f0_hz.is_finite() {
            self.minimum_f0_hz = 0.0;
        }
        if self.rendered_frames > 0 {
            self.oral_energy /= self.rendered_frames as f32;
            self.nasal_energy /= self.rendered_frames as f32;
        }
        self
    }
}
