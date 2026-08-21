#[derive(Debug, Clone, Copy, Default)]
pub struct Oscillator {
    phase: f32,
}

impl Oscillator {
    pub fn sample(&mut self, frequency_hz: f32, sample_rate: f32, harmonic_mix: [f32; 4]) -> f32 {
        let increment = (frequency_hz / sample_rate.max(1.0)).clamp(0.0, 0.45);
        self.phase = (self.phase + increment).fract();
        let angle = self.phase * std::f32::consts::TAU;
        let sine = angle.sin();
        let triangle = 1.0 - 4.0 * (self.phase - 0.5).abs();
        let pulse = band_limited_pulse(self.phase, increment, 0.42);
        let second = (angle * 2.0).sin();
        sine * harmonic_mix[0]
            + triangle * harmonic_mix[1]
            + pulse * harmonic_mix[2]
            + second * harmonic_mix[3]
    }
}

#[derive(Debug, Clone, Copy)]
pub struct NoiseSource {
    state: u64,
    lowpass: f32,
}

impl NoiseSource {
    pub fn new(seed: u64) -> Self {
        Self {
            state: seed.max(1),
            lowpass: 0.0,
        }
    }

    pub fn reseed(&mut self, seed: u64) {
        self.state = seed.max(1);
        self.lowpass = 0.0;
    }

    pub fn sample(&mut self, brightness: f32) -> f32 {
        self.state ^= self.state << 13;
        self.state ^= self.state >> 7;
        self.state ^= self.state << 17;
        let white = (self.state as u32 as f32 / u32::MAX as f32) * 2.0 - 1.0;
        let coefficient = (0.03 + brightness.clamp(0.0, 1.0) * 0.22).clamp(0.01, 0.3);
        self.lowpass += (white - self.lowpass) * coefficient;
        self.lowpass
    }
}

fn band_limited_pulse(phase: f32, increment: f32, duty: f32) -> f32 {
    let naive = if phase < duty { 1.0 } else { -1.0 };
    let rising = poly_blep(phase, increment);
    let falling_phase = (phase - duty).rem_euclid(1.0);
    naive + rising - poly_blep(falling_phase, increment)
}

fn poly_blep(phase: f32, increment: f32) -> f32 {
    if increment <= f32::EPSILON {
        return 0.0;
    }
    if phase < increment {
        let t = phase / increment;
        t + t - t * t - 1.0
    } else if phase > 1.0 - increment {
        let t = (phase - 1.0) / increment;
        t * t + t + t + 1.0
    } else {
        0.0
    }
}
