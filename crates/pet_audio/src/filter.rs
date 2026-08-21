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

#[must_use]
pub fn soft_limit(sample: f32, maximum: f32) -> f32 {
    let maximum = maximum.clamp(0.05, 0.92);
    (sample / maximum).tanh() * maximum
}
