use lifecore::{BodyVoiceFrame, VocalGesture, VoiceAnatomy};

const ORAL_CAPACITY: usize = 24;
const NASAL_SECTIONS: usize = 6;

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct TractFrame {
    pub(crate) output: f32,
    pub(crate) oral_output: f32,
    pub(crate) nasal_output: f32,
    pub(crate) back_pressure: f32,
    pub(crate) mouth_aperture: f32,
    pub(crate) coefficient_delta: f32,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct DynamicTract {
    sample_rate: f32,
    anatomy: VoiceAnatomy,
    active_oral: usize,
    junction: usize,
    oral_area: [f32; ORAL_CAPACITY],
    target_oral_area: [f32; ORAL_CAPACITY],
    nasal_area: [f32; NASAL_SECTIONS],
    oral_right: [f32; ORAL_CAPACITY + 1],
    oral_left: [f32; ORAL_CAPACITY + 1],
    next_oral_right: [f32; ORAL_CAPACITY + 1],
    next_oral_left: [f32; ORAL_CAPACITY + 1],
    nasal_right: [f32; NASAL_SECTIONS + 1],
    nasal_left: [f32; NASAL_SECTIONS + 1],
    next_nasal_right: [f32; NASAL_SECTIONS + 1],
    next_nasal_left: [f32; NASAL_SECTIONS + 1],
    velum_area: f32,
    target_velum_area: f32,
    constriction_index: usize,
    turbulence_strength: f32,
    mouth_aperture: f32,
    previous_lip: f32,
    previous_nose: f32,
    back_pressure: f32,
}

impl DynamicTract {
    pub(crate) fn new(sample_rate: f32) -> Self {
        Self {
            sample_rate: sample_rate.max(1.0),
            anatomy: VoiceAnatomy::default(),
            active_oral: 18,
            junction: 9,
            oral_area: [0.8; ORAL_CAPACITY],
            target_oral_area: [0.8; ORAL_CAPACITY],
            nasal_area: [0.35; NASAL_SECTIONS],
            oral_right: [0.0; ORAL_CAPACITY + 1],
            oral_left: [0.0; ORAL_CAPACITY + 1],
            next_oral_right: [0.0; ORAL_CAPACITY + 1],
            next_oral_left: [0.0; ORAL_CAPACITY + 1],
            nasal_right: [0.0; NASAL_SECTIONS + 1],
            nasal_left: [0.0; NASAL_SECTIONS + 1],
            next_nasal_right: [0.0; NASAL_SECTIONS + 1],
            next_nasal_left: [0.0; NASAL_SECTIONS + 1],
            velum_area: 0.02,
            target_velum_area: 0.02,
            constriction_index: 12,
            turbulence_strength: 0.0,
            mouth_aperture: 0.5,
            previous_lip: 0.0,
            previous_nose: 0.0,
            back_pressure: 0.0,
        }
    }

    pub(crate) fn reset(&mut self, anatomy: VoiceAnatomy) {
        self.anatomy = anatomy;
        self.active_oral =
            (14 + (anatomy.tract_length.clamp(0.0, 1.0) * 8.0).round() as usize).clamp(14, 22);
        self.junction = (self.active_oral * 45 / 100).clamp(8, 11);
        for index in 0..ORAL_CAPACITY {
            let x = index as f32 / self.active_oral.saturating_sub(1).max(1) as f32;
            let throat = 0.62 + x * 0.34;
            let cavity = 0.22 * (1.0 - (x * 2.0 - 1.0).abs());
            let identity = (anatomy.tract_compliance - 0.5) * (x - 0.45) * 0.20;
            let area = (throat + cavity + identity).clamp(0.18, 1.35);
            self.oral_area[index] = area;
            self.target_oral_area[index] = area;
        }
        for (index, area) in self.nasal_area.iter_mut().enumerate() {
            let x = index as f32 / (NASAL_SECTIONS - 1) as f32;
            *area = (0.28 + anatomy.nasal_capacity * 0.28 + x * 0.12).clamp(0.16, 0.72);
        }
        self.oral_right.fill(0.0);
        self.oral_left.fill(0.0);
        self.next_oral_right.fill(0.0);
        self.next_oral_left.fill(0.0);
        self.nasal_right.fill(0.0);
        self.nasal_left.fill(0.0);
        self.next_nasal_right.fill(0.0);
        self.next_nasal_left.fill(0.0);
        self.velum_area = 0.02;
        self.target_velum_area = 0.02;
        self.previous_lip = 0.0;
        self.previous_nose = 0.0;
        self.back_pressure = 0.0;
    }

    pub(crate) fn set_targets(
        &mut self,
        gesture: VocalGesture,
        mouth_open: f32,
        body: BodyVoiceFrame,
    ) {
        let body = body.sanitized();
        let stretch_scale = 1.0 + body.stretch * 0.08 + (body.shape_aspect_ratio - 1.0) * 0.025;
        let compression_scale = 1.0 - body.compression * 0.09;
        let constriction_center = (0.56 + gesture.frontness * 0.24).clamp(0.22, 0.86)
            * self.active_oral.saturating_sub(1) as f32;
        self.constriction_index = constriction_center.round() as usize;
        let width = (1.3 + (1.0 - gesture.constriction) * 3.4).clamp(1.2, 4.8);
        for index in 0..self.active_oral {
            let x = index as f32 / self.active_oral.saturating_sub(1).max(1) as f32;
            let throat = 0.62 + x * 0.34;
            let cavity = 0.22 * (1.0 - (x * 2.0 - 1.0).abs());
            let identity = (self.anatomy.tract_compliance - 0.5) * (x - 0.45) * 0.20;
            let distance = (index as f32 - constriction_center) / width;
            let constriction = (-0.5 * distance * distance).exp()
                * gesture.constriction
                * (0.58 + gesture.tongue_height * 0.16);
            let mut area = (throat + cavity + identity) * stretch_scale * compression_scale;
            area *= 1.0 - constriction.clamp(0.0, 0.88);
            if index + 3 >= self.active_oral {
                let lip_region = (index + 3 - self.active_oral) as f32 / 3.0;
                area *= 0.72 + mouth_open.clamp(0.0, 1.0) * 0.78;
                area *= 1.0 - gesture.lip_rounding * 0.42 * lip_region;
            }
            self.target_oral_area[index] = area.clamp(0.045, 1.65);
        }
        self.target_velum_area = (0.012
            + gesture.nasality * (0.18 + self.anatomy.nasal_capacity * 0.34))
            .clamp(0.01, 0.48);
        self.turbulence_strength =
            (gesture.constriction - 0.22).max(0.0) * (0.35 + gesture.closure_sharpness * 0.65);
        self.mouth_aperture = mouth_open.clamp(0.0, 1.0);
    }

    pub(crate) fn process(
        &mut self,
        source: f32,
        aspiration: f32,
        constriction_noise: f32,
    ) -> TractFrame {
        let smoothing = (1.0 - (-1.0 / (self.sample_rate * 0.030)).exp()).clamp(0.000_1, 0.02);
        let mut maximum_delta = 0.0_f32;
        for index in 0..self.active_oral {
            let delta = (self.target_oral_area[index] - self.oral_area[index]) * smoothing;
            self.oral_area[index] += delta;
            maximum_delta = maximum_delta.max(delta.abs());
        }
        self.velum_area += (self.target_velum_area - self.velum_area) * smoothing;
        self.next_oral_right.fill(0.0);
        self.next_oral_left.fill(0.0);
        self.next_nasal_right.fill(0.0);
        self.next_nasal_left.fill(0.0);
        let source_loss = 0.68 + (1.0 - self.anatomy.oral_loss) * 0.18;
        self.next_oral_right[0] = (source + aspiration) + self.oral_left[0] * source_loss;
        for index in 0..self.active_oral {
            if index == self.junction {
                let areas = [
                    self.oral_area[index],
                    self.oral_area[index + 1],
                    self.velum_area,
                ];
                let incoming = [
                    self.oral_right[index],
                    self.oral_left[index + 1],
                    self.nasal_left[0],
                ];
                let pressure = three_port_pressure(areas, incoming);
                self.next_oral_left[index] = pressure - incoming[0];
                self.next_oral_right[index + 1] = pressure - incoming[1];
                self.next_nasal_right[0] = pressure - incoming[2];
            } else {
                let left_area = self.oral_area[index];
                let right_area = self.oral_area[(index + 1).min(self.active_oral - 1)];
                let incoming_left = self.oral_right[index];
                let incoming_right = self.oral_left[index + 1];
                let pressure = 2.0 * (left_area * incoming_left + right_area * incoming_right)
                    / (left_area + right_area).max(0.001);
                self.next_oral_left[index] = pressure - incoming_left;
                self.next_oral_right[index + 1] = pressure - incoming_right;
            }
        }
        for index in 0..NASAL_SECTIONS {
            let left_area = self.nasal_area[index];
            let right_area = self.nasal_area[(index + 1).min(NASAL_SECTIONS - 1)];
            let incoming_left = self.nasal_right[index];
            let incoming_right = self.nasal_left[index + 1];
            let pressure = 2.0 * (left_area * incoming_left + right_area * incoming_right)
                / (left_area + right_area).max(0.001);
            self.next_nasal_left[index] = pressure - incoming_left;
            self.next_nasal_right[index + 1] = pressure - incoming_right;
        }
        let turbulence = constriction_noise
            * self.turbulence_strength
            * self.target_oral_area[self.constriction_index]
                .recip()
                .min(8.0)
            * 0.018;
        let injection = self.constriction_index.min(self.active_oral - 1);
        self.next_oral_right[injection] += turbulence;
        self.next_oral_left[injection] -= turbulence * 0.72;
        let lip_wave = self.oral_right[self.active_oral];
        let nose_wave = self.nasal_right[NASAL_SECTIONS];
        self.next_oral_left[self.active_oral] = -lip_wave * 0.82;
        self.next_nasal_left[NASAL_SECTIONS] = -nose_wave * 0.74;
        // The glottal source is already a flow derivative.  A pure first
        // difference here differentiates it a second time, leaving a tiny,
        // brittle signal.  Radiation is therefore a mostly resistive load
        // with a smaller differentiating component at each opening.
        let oral_output = (lip_wave * 0.72 + (lip_wave - self.previous_lip) * 0.28) * 0.90;
        let nasal_output = (nose_wave * 0.76 + (nose_wave - self.previous_nose) * 0.24) * 0.62;
        self.previous_lip = lip_wave;
        self.previous_nose = nose_wave;
        let wall_loss = (0.985 - self.anatomy.oral_loss * 0.018).clamp(0.94, 0.99);
        for value in &mut self.next_oral_right[..=self.active_oral] {
            *value *= wall_loss;
        }
        for value in &mut self.next_oral_left[..=self.active_oral] {
            *value *= wall_loss;
        }
        for value in &mut self.next_nasal_right {
            *value *= 0.972;
        }
        for value in &mut self.next_nasal_left {
            *value *= 0.972;
        }
        self.oral_right = self.next_oral_right;
        self.oral_left = self.next_oral_left;
        self.nasal_right = self.next_nasal_right;
        self.nasal_left = self.next_nasal_left;
        self.back_pressure += (self.oral_left[0] - self.back_pressure) * 0.08;
        let output = oral_output + nasal_output;
        if !output.is_finite() {
            self.oral_right.fill(0.0);
            self.oral_left.fill(0.0);
            self.nasal_right.fill(0.0);
            self.nasal_left.fill(0.0);
        }
        TractFrame {
            output: if output.is_finite() {
                output.clamp(-3.0, 3.0)
            } else {
                0.0
            },
            oral_output: oral_output.clamp(-3.0, 3.0),
            nasal_output: nasal_output.clamp(-3.0, 3.0),
            back_pressure: self.back_pressure.clamp(-2.0, 2.0),
            mouth_aperture: self.mouth_aperture,
            coefficient_delta: maximum_delta,
        }
    }
}

fn three_port_pressure(areas: [f32; 3], incoming: [f32; 3]) -> f32 {
    2.0 * areas
        .iter()
        .zip(incoming)
        .map(|(area, wave)| area * wave)
        .sum::<f32>()
        / areas.into_iter().sum::<f32>().max(0.001)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oral_and_nasal_paths_both_reach_pcm() {
        let anatomy = lifecore::Genome::from_seed(4).voice.anatomy;
        let mut tract = DynamicTract::new(48_000.0);
        tract.reset(anatomy);
        let gesture = VocalGesture {
            nasality: 0.82,
            ..VocalGesture::default()
        };
        tract.set_targets(gesture, 0.55, BodyVoiceFrame::default());
        let mut oral = 0.0;
        let mut nasal = 0.0;
        for frame in 0..8_000 {
            let output = tract.process(if frame == 0 { 1.0 } else { 0.0 }, 0.0, 0.0);
            oral += output.oral_output.abs();
            nasal += output.nasal_output.abs();
        }
        assert!(oral > 0.001);
        assert!(nasal > 0.000_01);
    }
}
