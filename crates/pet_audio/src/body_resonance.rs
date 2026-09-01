use lifecore::{BodyVoiceFrame, VocalGesture, VoiceAnatomy};

const MODE_COUNT: usize = 8;

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct BodyResonanceFrame {
    pub(crate) signal: f32,
    pub(crate) energy: f32,
}

#[derive(Debug, Clone, Copy, Default)]
struct ModalMode {
    coefficient: f32,
    radius_squared: f32,
    gain: f32,
    target_coefficient: f32,
    target_radius_squared: f32,
    target_gain: f32,
    y1: f32,
    y2: f32,
}

impl ModalMode {
    fn set_target(&mut self, frequency_hz: f32, q: f32, gain: f32, sample_rate: f32) {
        let frequency_hz = frequency_hz.clamp(35.0, sample_rate * 0.42);
        let bandwidth = (frequency_hz / q.clamp(1.1, 4.0)).max(28.0);
        let radius = (-std::f32::consts::PI * bandwidth / sample_rate.max(1.0)).exp();
        self.target_coefficient =
            2.0 * radius * (std::f32::consts::TAU * frequency_hz / sample_rate).cos();
        self.target_radius_squared = radius * radius;
        self.target_gain = gain.clamp(0.0, 0.35) * (1.0 - radius).clamp(0.003, 0.45);
        if self.coefficient == 0.0 && self.radius_squared == 0.0 {
            self.coefficient = self.target_coefficient;
            self.radius_squared = self.target_radius_squared;
            self.gain = self.target_gain;
        }
    }

    fn reset(&mut self) {
        self.y1 = 0.0;
        self.y2 = 0.0;
    }

    fn process(&mut self, input: f32) -> f32 {
        self.coefficient += (self.target_coefficient - self.coefficient) * 0.004;
        self.radius_squared += (self.target_radius_squared - self.radius_squared) * 0.004;
        self.gain += (self.target_gain - self.gain) * 0.004;
        let output = input * self.gain + self.coefficient * self.y1 - self.radius_squared * self.y2;
        self.y2 = self.y1;
        self.y1 = if output.is_finite() {
            output.clamp(-3.0, 3.0)
        } else {
            0.0
        };
        self.y1
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct LiquidBodyResonance {
    sample_rate: f32,
    anatomy: VoiceAnatomy,
    modes: [ModalMode; MODE_COUNT],
    active_modes: usize,
    mix: f32,
    previous_events: [f32; 5],
    event_excitation: f32,
    energy: f32,
}

impl LiquidBodyResonance {
    pub(crate) fn new(sample_rate: f32) -> Self {
        Self {
            sample_rate: sample_rate.max(1.0),
            anatomy: VoiceAnatomy::default(),
            modes: [ModalMode::default(); MODE_COUNT],
            active_modes: 6,
            mix: 0.10,
            previous_events: [0.0; 5],
            event_excitation: 0.0,
            energy: 0.0,
        }
    }

    pub(crate) fn reset(&mut self, anatomy: VoiceAnatomy) {
        self.anatomy = anatomy;
        for mode in &mut self.modes {
            mode.reset();
        }
        self.previous_events = [0.0; 5];
        self.event_excitation = 0.0;
        self.energy = 0.0;
    }

    pub(crate) fn set_targets(&mut self, body: BodyVoiceFrame, gesture: VocalGesture) {
        let body = body.sanitized();
        let bases = [120.0, 190.0, 310.0, 510.0, 830.0, 1_340.0];
        let identity_scale = 0.82 + (1.0 - self.anatomy.body_coupling) * 0.28;
        let mass_scale = (1.0 + body.detached_mass_ratio * 0.24).clamp(0.90, 1.30);
        let compression_scale = 1.0 - body.compression * 0.10;
        for (index, base) in bases.into_iter().enumerate() {
            let spread = 1.0
                + body.stretch * (index as f32 - 2.5) * 0.016
                + (body.shape_aspect_ratio - 1.0) * (index as f32 - 2.5) * 0.010;
            let frequency = base * identity_scale * mass_scale * compression_scale * spread;
            let q = (1.25 + index as f32 * 0.32 + self.anatomy.body_coupling * 0.52
                - body.compression * 0.42)
                .clamp(1.1, 4.0);
            let lost_mass = if index < 2 {
                1.0 - body.detached_mass_ratio * 0.72
            } else {
                1.0 - body.detached_mass_ratio * 0.28
            };
            self.modes[index].set_target(
                frequency,
                q,
                (0.22 - index as f32 * 0.018).max(0.08) * lost_mass,
                self.sample_rate,
            );
        }
        self.active_modes = if body.component_count > 1 { 8 } else { 6 };
        if self.active_modes > 6 {
            let satellite_base = 390.0 + body.detached_mass_ratio * 620.0;
            self.modes[6].set_target(satellite_base, 1.35, 0.055, self.sample_rate);
            self.modes[7].set_target(satellite_base * 1.63, 1.65, 0.035, self.sample_rate);
        }
        self.mix = (0.05 + self.anatomy.body_coupling * 0.09 + gesture.body_excitation * 0.04)
            .clamp(0.05, 0.18);
    }

    pub(crate) fn process(
        &mut self,
        tract_output: f32,
        body: BodyVoiceFrame,
        slosh_noise: f32,
    ) -> BodyResonanceFrame {
        let events = [
            body.collision_impulse,
            body.contact_impulse,
            body.release_impulse,
            body.detach_impulse,
            body.remerge_impulse,
        ];
        for (index, event) in events.into_iter().enumerate() {
            let rise = (event - self.previous_events[index]).max(0.0);
            let weight = [0.12, 0.08, 0.07, 0.10, 0.18][index];
            self.event_excitation += rise * weight;
            self.previous_events[index] = event;
        }
        let excitation = tract_output * (0.035 + self.anatomy.body_coupling * 0.055)
            + slosh_noise * (body.slosh_energy * 0.020 + body.internal_speed * 0.008)
            + self.event_excitation;
        self.event_excitation *= 0.94;
        let mut modal = 0.0;
        for mode in &mut self.modes[..self.active_modes] {
            modal += mode.process(excitation);
        }
        let signal = (modal * self.mix).clamp(-1.0, 1.0);
        self.energy += (signal.abs() - self.energy) * 0.018;
        BodyResonanceFrame {
            signal,
            energy: self.energy.clamp(0.0, 1.0),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detach_changes_modal_response_without_an_f0_parameter() {
        let anatomy = lifecore::Genome::from_seed(17).voice.anatomy;
        let mut attached = LiquidBodyResonance::new(48_000.0);
        let mut detached = LiquidBodyResonance::new(48_000.0);
        attached.reset(anatomy);
        detached.reset(anatomy);
        attached.set_targets(BodyVoiceFrame::default(), VocalGesture::default());
        detached.set_targets(
            BodyVoiceFrame {
                component_count: 2,
                main_mass_ratio: 0.75,
                detached_mass_ratio: 0.25,
                detach_impulse: 0.8,
                ..BodyVoiceFrame::default()
            },
            VocalGesture::default(),
        );
        let mut difference = 0.0;
        for frame in 0..4_000 {
            let input = if frame == 0 { 0.8 } else { 0.0 };
            let a = attached
                .process(input, BodyVoiceFrame::default(), 0.0)
                .signal;
            let b = detached
                .process(
                    input,
                    BodyVoiceFrame {
                        component_count: 2,
                        main_mass_ratio: 0.75,
                        detached_mass_ratio: 0.25,
                        detach_impulse: if frame == 0 { 0.8 } else { 0.0 },
                        ..BodyVoiceFrame::default()
                    },
                    0.0,
                )
                .signal;
            difference += (a - b).abs();
        }
        assert!(difference > 0.000_1);
    }
}
