use lifecore::{VocalFamily, VocalStyle};

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ProsodyCurve {
    times: [f32; 5],
    log_pitch: [f32; 5],
}

impl Default for ProsodyCurve {
    fn default() -> Self {
        Self {
            times: [0.0, 0.16, 0.42, 0.78, 1.0],
            log_pitch: [0.0; 5],
        }
    }
}

impl ProsodyCurve {
    pub(crate) fn from_targets(
        start: f32,
        peak: f32,
        end: f32,
        seed: u64,
        family: VocalFamily,
        style: VocalStyle,
        valence: f32,
    ) -> Self {
        let peak_time = (0.24 + seeded_unit(seed ^ 0x31) * 0.39).clamp(0.24, 0.63);
        let inflection = (peak_time + 0.16 + seeded_unit(seed ^ 0x52) * 0.18).clamp(0.62, 0.90);
        let onset = (0.09 + seeded_unit(seed ^ 0x73) * 0.11).min(peak_time - 0.04);
        let start = start.max(0.01).log2();
        let peak = peak.max(0.01).log2();
        let end = end.max(0.01).log2();
        let family_bias = match family {
            VocalFamily::QuestionWhine | VocalFamily::SoftContact => 0.035,
            VocalFamily::FrustratedGrunt => -0.045,
            VocalFamily::StartleSqueak => 0.055,
            _ => 0.0,
        };
        let style_bias = match style {
            VocalStyle::PlayInvite | VocalStyle::AttentionCall => 0.025,
            VocalStyle::ContentMurmur | VocalStyle::Purr | VocalStyle::Frustrated => -0.025,
            _ => 0.0,
        };
        let early = start + (peak - start) * (0.46 + seeded_unit(seed ^ 0x94) * 0.20);
        let late = peak
            + (end - peak) * (0.44 + seeded_unit(seed ^ 0xb5) * 0.24)
            + family_bias
            + style_bias
            + valence.clamp(-1.0, 1.0) * 0.018;
        Self {
            times: [0.0, onset, peak_time, inflection, 1.0],
            log_pitch: [start, early, peak, late, end],
        }
    }

    pub(crate) fn sample(self, progress: f32) -> f32 {
        let x = progress.clamp(0.0, 1.0);
        let index = self
            .times
            .windows(2)
            .position(|window| x <= window[1])
            .unwrap_or(3);
        let x0 = self.times[index];
        let x1 = self.times[index + 1];
        let y0 = self.log_pitch[index];
        let y1 = self.log_pitch[index + 1];
        let previous = self.log_pitch[index.saturating_sub(1)];
        let next = self.log_pitch[(index + 2).min(4)];
        let m0 = (y1 - previous) * 0.5;
        let m1 = (next - y0) * 0.5;
        let t = ((x - x0) / (x1 - x0).max(0.001)).clamp(0.0, 1.0);
        let t2 = t * t;
        let t3 = t2 * t;
        let h00 = 2.0 * t3 - 3.0 * t2 + 1.0;
        let h10 = t3 - 2.0 * t2 + t;
        let h01 = -2.0 * t3 + 3.0 * t2;
        let h11 = t3 - t2;
        let value = h00 * y0 + h10 * m0 + h01 * y1 + h11 * m1;
        let global_min = self.log_pitch.into_iter().fold(f32::INFINITY, f32::min);
        let global_max = self.log_pitch.into_iter().fold(f32::NEG_INFINITY, f32::max);
        value.clamp(global_min, global_max).exp2()
    }
}

fn seeded_unit(mut value: u64) -> f32 {
    value = value.wrapping_add(0x9e37_79b9_7f4a_7c15);
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^= value >> 31;
    ((value >> 40) as u32) as f32 / 0x00ff_ffff as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn curve_is_bounded_and_not_midpoint_symmetric() {
        let curve = ProsodyCurve::from_targets(
            0.8,
            1.3,
            0.95,
            44,
            VocalFamily::QuestionWhine,
            VocalStyle::AttentionCall,
            0.2,
        );
        let values: Vec<_> = (0..101).map(|i| curve.sample(i as f32 / 100.0)).collect();
        assert!(values.iter().all(|value| (0.8..=1.3).contains(value)));
        let peak = values
            .iter()
            .enumerate()
            .max_by(|left, right| left.1.total_cmp(right.1))
            .unwrap()
            .0;
        assert_ne!(peak, 50);
    }
}
