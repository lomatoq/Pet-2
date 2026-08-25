use lifecore::Genome;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DerivedVisualTraits {
    pub shell_opacity: f32,
    pub translucency: f32,
    pub metabolic_frequency: f32,
    pub metabolic_amplitude: f32,
    pub flow_scale: f32,
    pub flow_speed: f32,
    pub flow_warp: f32,
    pub core_size: f32,
    pub iris_scale: f32,
    pub iris_fiber_count: f32,
    pub iris_contrast: f32,
    pub limbal_strength: f32,
    pub cornea_strength: f32,
    pub droplet_count: u8,
    pub droplet_size: f32,
    pub droplet_spread: f32,
    pub droplet_elasticity: f32,
    pub droplet_drag: f32,
    pub droplet_lag: f32,
    pub droplet_cohesion: f32,
    pub halo_strength: f32,
}

impl DerivedVisualTraits {
    #[must_use]
    pub fn from_genome(genome: &Genome) -> Self {
        let seed = genome.identity_seed ^ genome.body.pattern_seed.rotate_left(17);
        Self {
            shell_opacity: lerp(0.94, 0.985, unit_from_hash(seed ^ 0x01)),
            translucency: lerp(0.18, 0.42, unit_from_hash(seed ^ 0x02)),
            metabolic_frequency: lerp(0.11, 0.19, unit_from_hash(seed ^ 0x03)),
            metabolic_amplitude: lerp(0.006, 0.014, unit_from_hash(seed ^ 0x04)),
            flow_scale: lerp(1.2, 2.8, unit_from_hash(seed ^ 0x05)),
            flow_speed: lerp(0.08, 0.22, unit_from_hash(seed ^ 0x06)),
            flow_warp: lerp(0.05, 0.14, unit_from_hash(seed ^ 0x07)),
            core_size: lerp(0.28, 0.46, unit_from_hash(seed ^ 0x08)),
            iris_scale: lerp(0.58, 0.66, unit_from_hash(seed ^ 0x09)),
            iris_fiber_count: lerp(24.0, 42.0, unit_from_hash(seed ^ 0x0a)),
            iris_contrast: lerp(0.12, 0.28, unit_from_hash(seed ^ 0x0b)),
            limbal_strength: lerp(0.30, 0.58, unit_from_hash(seed ^ 0x0c)),
            cornea_strength: lerp(0.45, 0.78, unit_from_hash(seed ^ 0x0d)),
            droplet_count: 6 + (hash_u64(seed ^ 0x0e) % 3) as u8,
            droplet_size: lerp(0.042, 0.068, unit_from_hash(seed ^ 0x0f)),
            droplet_spread: lerp(0.18, 0.31, unit_from_hash(seed ^ 0x10)),
            droplet_elasticity: lerp(6.0, 13.0, unit_from_hash(seed ^ 0x11)),
            droplet_drag: lerp(3.0, 7.0, unit_from_hash(seed ^ 0x12)),
            droplet_lag: lerp(0.10, 0.24, unit_from_hash(seed ^ 0x13)),
            droplet_cohesion: lerp(0.45, 0.86, unit_from_hash(seed ^ 0x14)),
            halo_strength: lerp(0.03, 0.09, unit_from_hash(seed ^ 0x15)),
        }
    }

    #[must_use]
    pub fn is_valid(self) -> bool {
        let values = [
            self.shell_opacity,
            self.translucency,
            self.metabolic_frequency,
            self.metabolic_amplitude,
            self.flow_scale,
            self.flow_speed,
            self.flow_warp,
            self.core_size,
            self.iris_scale,
            self.iris_fiber_count,
            self.iris_contrast,
            self.limbal_strength,
            self.cornea_strength,
            self.droplet_size,
            self.droplet_spread,
            self.droplet_elasticity,
            self.droplet_drag,
            self.droplet_lag,
            self.droplet_cohesion,
            self.halo_strength,
        ];
        values.into_iter().all(f32::is_finite) && (6..=8).contains(&self.droplet_count)
    }
}

pub(crate) fn hash_u64(mut value: u64) -> u64 {
    value ^= value >> 30;
    value = value.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value ^= value >> 27;
    value = value.wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

pub(crate) fn unit_from_hash(value: u64) -> f32 {
    let bits = (hash_u64(value) >> 40) as u32;
    bits as f32 / 0x00ff_ffff as f32
}

fn lerp(start: f32, end: f32, amount: f32) -> f32 {
    start + (end - start) * amount
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visual_traits_are_deterministic_bounded_and_distinct() {
        let first = DerivedVisualTraits::from_genome(&Genome::from_seed(42));
        let replay = DerivedVisualTraits::from_genome(&Genome::from_seed(42));
        let other = DerivedVisualTraits::from_genome(&Genome::from_seed(43));
        assert_eq!(first, replay);
        assert_ne!(first, other);
        assert!(first.is_valid());
        assert!((6..=8).contains(&first.droplet_count));
        assert!((0.94..=0.985).contains(&first.shell_opacity));
        assert!((0.58..=0.66).contains(&first.iris_scale));
    }
}
