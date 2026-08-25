#[derive(Debug, Clone, Copy)]
pub struct Resonator {
    coefficient: f32,
    radius_squared: f32,
    gain: f32,
    y1: f32,
    y2: f32,
}

impl Default for Resonator {
    fn default() -> Self {
        Self {
            coefficient: 0.0,
            radius_squared: 0.0,
            gain: 0.0,
            y1: 0.0,
            y2: 0.0,
        }
    }
}

impl Resonator {
    pub fn configure(&mut self, frequency_hz: f32, bandwidth_hz: f32, sample_rate: f32) {
        let frequency_hz = frequency_hz.clamp(20.0, sample_rate * 0.42);
        let radius = (-std::f32::consts::PI * bandwidth_hz.max(20.0) / sample_rate).exp();
        self.coefficient =
            2.0 * radius * (std::f32::consts::TAU * frequency_hz / sample_rate).cos();
        self.radius_squared = radius * radius;
        self.gain = (1.0 - radius).clamp(0.001, 0.45);
    }

    pub fn reset(&mut self) {
        self.y1 = 0.0;
        self.y2 = 0.0;
    }

    pub fn process(&mut self, input: f32) -> f32 {
        let output = input * self.gain + self.coefficient * self.y1 - self.radius_squared * self.y2;
        self.y2 = self.y1;
        self.y1 = if output.is_finite() {
            output.clamp(-4.0, 4.0)
        } else {
            0.0
        };
        self.y1
    }
}

/// A tiny real-time-safe spectral tilt filter. Coefficients are configured off
/// the hot sample path and state is kept inline with the voice.
#[derive(Debug, Clone, Copy, Default)]
pub struct OnePoleLowPass {
    coefficient: f32,
    state: f32,
}

impl OnePoleLowPass {
    pub fn configure(&mut self, cutoff_hz: f32, sample_rate: f32) {
        let sample_rate = sample_rate.max(1.0);
        let cutoff_hz = cutoff_hz.clamp(8.0, sample_rate * 0.42);
        self.coefficient =
            (1.0 - (-std::f32::consts::TAU * cutoff_hz / sample_rate).exp()).clamp(0.0, 1.0);
    }

    pub fn reset(&mut self) {
        self.state = 0.0;
    }

    pub fn process(&mut self, input: f32) -> f32 {
        self.state += (input - self.state) * self.coefficient;
        if !self.state.is_finite() {
            self.state = 0.0;
        }
        self.state
    }
}

#[derive(Debug, Clone, Copy)]
pub struct DcBlocker {
    pole: f32,
    previous_input: f32,
    previous_output: f32,
}

impl Default for DcBlocker {
    fn default() -> Self {
        Self {
            pole: 0.995,
            previous_input: 0.0,
            previous_output: 0.0,
        }
    }
}

impl DcBlocker {
    pub fn configure(&mut self, cutoff_hz: f32, sample_rate: f32) {
        self.pole = (-std::f32::consts::TAU * cutoff_hz.max(4.0) / sample_rate.max(1.0))
            .exp()
            .clamp(0.90, 0.999_9);
    }

    pub fn reset(&mut self) {
        self.previous_input = 0.0;
        self.previous_output = 0.0;
    }

    pub fn process(&mut self, input: f32) -> f32 {
        let output = input - self.previous_input + self.pole * self.previous_output;
        self.previous_input = input;
        self.previous_output = if output.is_finite() { output } else { 0.0 };
        self.previous_output
    }
}

/// Two quiet, damped early reflections. Storage is allocated when the audio
/// stream is created, never from the real-time callback or on first utterance.
#[derive(Debug)]
pub struct EarlyReflections {
    delay: Box<[f32]>,
    write_index: usize,
    tap_a: usize,
    tap_b: usize,
    damping: f32,
    wet: f32,
}

impl EarlyReflections {
    #[must_use]
    pub fn new(sample_rate: f32) -> Self {
        let sample_rate = sample_rate.max(1.0);
        let tap_a = (sample_rate * 0.017).round().max(1.0) as usize;
        let tap_b = (sample_rate * 0.029).round().max((tap_a + 1) as f32) as usize;
        Self {
            delay: vec![0.0; tap_b + 1].into_boxed_slice(),
            write_index: 0,
            tap_a,
            tap_b,
            damping: 0.0,
            wet: 0.06,
        }
    }

    pub fn configure(&mut self, wet: f32) {
        self.wet = wet.clamp(0.0, 0.10);
    }

    pub fn reset(&mut self) {
        self.delay.fill(0.0);
        self.write_index = 0;
        self.damping = 0.0;
    }

    pub fn process(&mut self, input: f32) -> f32 {
        let len = self.delay.len();
        let read_a = (self.write_index + len - self.tap_a) % len;
        let read_b = (self.write_index + len - self.tap_b) % len;
        let reflected = self.delay[read_a] * 0.62 + self.delay[read_b] * 0.38;
        // Air/material loss keeps the taps from becoming a metallic comb.
        self.damping += (reflected - self.damping) * 0.24;
        self.delay[self.write_index] = (input + self.damping * 0.10).clamp(-2.0, 2.0);
        self.write_index = (self.write_index + 1) % len;
        input * (1.0 - self.wet) + self.damping * self.wet
    }
}

#[must_use]
pub fn soft_limit(sample: f32, maximum: f32) -> f32 {
    let maximum = maximum.clamp(0.05, 0.92);
    let magnitude = sample.abs();
    let knee = maximum * 0.82;
    if magnitude <= knee {
        return sample;
    }
    let span = (maximum - knee).max(f32::EPSILON);
    let limited = knee + span * (1.0 - (-(magnitude - knee) / span).exp());
    sample.signum() * limited.min(maximum)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dc_blocker_rejects_a_constant_without_instability() {
        let mut blocker = DcBlocker::default();
        blocker.configure(24.0, 48_000.0);
        let mut output = 0.0;
        for _ in 0..48_000 {
            output = blocker.process(0.5);
            assert!(output.is_finite());
        }
        assert!(output.abs() < 0.001);
    }

    #[test]
    fn early_reflections_are_bounded_and_have_a_short_tail() {
        let mut room = EarlyReflections::new(48_000.0);
        room.configure(0.08);
        let mut saw_tail = false;
        let mut peak = 0.0_f32;
        for frame in 0..2_400 {
            let sample = room.process(if frame == 0 { 0.5 } else { 0.0 });
            peak = peak.max(sample.abs());
            saw_tail |= frame > 500 && sample.abs() > 0.000_001;
        }
        assert!(saw_tail);
        assert!(peak <= 0.5);
    }

    #[test]
    fn soft_limiter_is_transparent_below_its_knee() {
        for sample in [-0.40, -0.12, 0.0, 0.12, 0.40] {
            assert_eq!(soft_limit(sample, 0.68), sample);
        }
        assert!(soft_limit(4.0, 0.68) <= 0.68);
        assert!(soft_limit(-4.0, 0.68) >= -0.68);
    }
}
