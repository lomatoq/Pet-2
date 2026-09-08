#[derive(Debug, Clone, Copy)]
pub(crate) struct NoiseSource {
    state: u64,
    colored: f32,
    slow: f32,
}

impl NoiseSource {
    pub(crate) fn new(seed: u64) -> Self {
        Self {
            state: seed.max(1),
            colored: 0.0,
            slow: 0.0,
        }
    }

    pub(crate) fn reseed(&mut self, seed: u64) {
        self.state = seed.max(1);
        self.colored = 0.0;
        self.slow = 0.0;
    }

    pub(crate) fn white(&mut self) -> f32 {
        self.state ^= self.state << 13;
        self.state ^= self.state >> 7;
        self.state ^= self.state << 17;
        (self.state as u32 as f32 / u32::MAX as f32) * 2.0 - 1.0
    }

    pub(crate) fn colored(&mut self, brightness: f32) -> f32 {
        let white = self.white();
        let coefficient = 0.025 + brightness.clamp(0.0, 1.0) * 0.28;
        self.colored += (white - self.colored) * coefficient;
        self.colored
    }

    pub(crate) fn correlated(&mut self, correlation: f32) -> f32 {
        let correlation = correlation.clamp(0.0, 0.999);
        self.slow = self.slow * correlation + self.white() * (1.0 - correlation);
        self.slow.clamp(-1.0, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_streams_repeat_exactly() {
        let mut left = NoiseSource::new(41);
        let mut right = NoiseSource::new(41);
        for _ in 0..4_096 {
            assert_eq!(left.colored(0.63), right.colored(0.63));
        }
    }
}
