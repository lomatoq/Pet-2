use std::sync::{
    Arc, OnceLock,
    atomic::{AtomicU32, AtomicU64, Ordering},
};

use lifecore::BodyVoiceFrame;

const FLOAT_FIELD_COUNT: usize = 15;
static GLOBAL_BODY_BRIDGE: OnceLock<Arc<BodyVoiceBridge>> = OnceLock::new();

#[must_use]
pub fn global_body_voice_bridge() -> Arc<BodyVoiceBridge> {
    Arc::clone(GLOBAL_BODY_BRIDGE.get_or_init(|| Arc::new(BodyVoiceBridge::default())))
}

pub struct BodyVoiceBridge {
    sequence: AtomicU64,
    component_count: AtomicU32,
    fields: [AtomicU32; FLOAT_FIELD_COUNT],
}

impl Default for BodyVoiceBridge {
    fn default() -> Self {
        let frame = BodyVoiceFrame::default();
        let values = frame_values(frame);
        Self {
            sequence: AtomicU64::new(0),
            component_count: AtomicU32::new(u32::from(frame.component_count)),
            fields: std::array::from_fn(|index| AtomicU32::new(values[index].to_bits())),
        }
    }
}

impl BodyVoiceBridge {
    pub fn publish(&self, frame: BodyVoiceFrame) {
        let frame = frame.sanitized();
        let sequence = self.sequence.load(Ordering::Relaxed).wrapping_add(1) | 1;
        self.sequence.store(sequence, Ordering::Release);
        self.component_count
            .store(u32::from(frame.component_count), Ordering::Relaxed);
        for (target, value) in self.fields.iter().zip(frame_values(frame)) {
            target.store(value.to_bits(), Ordering::Relaxed);
        }
        self.sequence
            .store(sequence.wrapping_add(1), Ordering::Release);
    }

    #[must_use]
    pub fn snapshot_or(&self, fallback: BodyVoiceFrame) -> BodyVoiceFrame {
        for _ in 0..4 {
            let before = self.sequence.load(Ordering::Acquire);
            if before & 1 != 0 {
                continue;
            }
            let component_count = self.component_count.load(Ordering::Relaxed) as u8;
            let values = std::array::from_fn(|index| {
                f32::from_bits(self.fields[index].load(Ordering::Relaxed))
            });
            let after = self.sequence.load(Ordering::Acquire);
            if before == after && after & 1 == 0 {
                return values_frame(values, component_count).sanitized();
            }
        }
        fallback.sanitized()
    }
}

#[derive(Debug, Clone, Copy)]
pub struct BodyVoiceAnalyzer {
    previous: BodyVoiceFrame,
    initialized: bool,
    detach_cooldown: f32,
    remerge_cooldown: f32,
    contact_cooldown: f32,
    release_cooldown: f32,
    impulses: [f32; 5],
}

impl Default for BodyVoiceAnalyzer {
    fn default() -> Self {
        Self {
            previous: BodyVoiceFrame::default(),
            initialized: false,
            detach_cooldown: 0.0,
            remerge_cooldown: 0.0,
            contact_cooldown: 0.0,
            release_cooldown: 0.0,
            impulses: [0.0; 5],
        }
    }
}

impl BodyVoiceAnalyzer {
    #[must_use]
    pub fn update(&mut self, raw: BodyVoiceFrame, dt: f32) -> BodyVoiceFrame {
        let mut frame = raw.sanitized();
        let dt = if dt.is_finite() {
            dt.clamp(0.0, 0.1)
        } else {
            0.0
        };
        for cooldown in [
            &mut self.detach_cooldown,
            &mut self.remerge_cooldown,
            &mut self.contact_cooldown,
            &mut self.release_cooldown,
        ] {
            *cooldown = (*cooldown - dt).max(0.0);
        }
        let decay = (-dt / 0.075).exp();
        for impulse in &mut self.impulses {
            *impulse *= decay;
        }
        if self.initialized {
            let detached_growth = frame.detached_mass_ratio - self.previous.detached_mass_ratio;
            let detached_drop = self.previous.detached_mass_ratio - frame.detached_mass_ratio;
            if self.detach_cooldown == 0.0
                && (frame.component_count > self.previous.component_count
                    || detached_growth > 0.025)
            {
                self.impulses[3] = (0.35 + detached_growth.max(0.0) * 4.5).clamp(0.0, 1.0);
                self.detach_cooldown = 0.18;
            }
            if self.remerge_cooldown == 0.0
                && (frame.component_count < self.previous.component_count || detached_drop > 0.025)
            {
                self.impulses[4] = (0.38 + detached_drop.max(0.0) * 4.0).clamp(0.0, 1.0);
                self.remerge_cooldown = 0.22;
            }
            let pressure_rise = frame.material_stress - self.previous.material_stress;
            if self.contact_cooldown == 0.0 && pressure_rise > 0.08 {
                self.impulses[1] =
                    (pressure_rise * 2.4 + frame.contact_area * 0.35).clamp(0.0, 1.0);
                self.contact_cooldown = 0.08;
            }
            let pressure_drop = self.previous.material_stress - frame.material_stress;
            if self.release_cooldown == 0.0
                && self.previous.contact_area > 0.02
                && frame.contact_area <= 0.01
            {
                self.impulses[2] =
                    (pressure_drop * 1.8 + frame.internal_speed * 0.35).clamp(0.0, 1.0);
                self.release_cooldown = 0.12;
            }
        }
        self.impulses[0] = self.impulses[0].max(frame.collision_impulse);
        self.impulses[1] = self.impulses[1].max(frame.contact_impulse);
        self.impulses[2] = self.impulses[2].max(frame.release_impulse);
        self.impulses[3] = self.impulses[3].max(frame.detach_impulse);
        self.impulses[4] = self.impulses[4].max(frame.remerge_impulse);
        frame.collision_impulse = self.impulses[0];
        frame.contact_impulse = self.impulses[1];
        frame.release_impulse = self.impulses[2];
        frame.detach_impulse = self.impulses[3];
        frame.remerge_impulse = self.impulses[4];
        self.previous = frame;
        self.initialized = true;
        frame
    }
}

fn frame_values(frame: BodyVoiceFrame) -> [f32; FLOAT_FIELD_COUNT] {
    [
        frame.main_mass_ratio,
        frame.detached_mass_ratio,
        frame.shape_aspect_ratio,
        frame.stretch,
        frame.compression,
        frame.bond_strain,
        frame.material_stress,
        frame.contact_area,
        frame.slosh_energy,
        frame.internal_speed,
        frame.collision_impulse,
        frame.contact_impulse,
        frame.release_impulse,
        frame.detach_impulse,
        frame.remerge_impulse,
    ]
}

fn values_frame(values: [f32; FLOAT_FIELD_COUNT], component_count: u8) -> BodyVoiceFrame {
    BodyVoiceFrame {
        main_mass_ratio: values[0],
        detached_mass_ratio: values[1],
        component_count,
        shape_aspect_ratio: values[2],
        stretch: values[3],
        compression: values[4],
        bond_strain: values[5],
        material_stress: values[6],
        contact_area: values[7],
        slosh_energy: values[8],
        internal_speed: values[9],
        collision_impulse: values[10],
        contact_impulse: values[11],
        release_impulse: values[12],
        detach_impulse: values[13],
        remerge_impulse: values[14],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bridge_round_trips_a_sanitized_frame() {
        let bridge = BodyVoiceBridge::default();
        let frame = BodyVoiceFrame {
            component_count: 2,
            detached_mass_ratio: 0.22,
            slosh_energy: 0.71,
            ..BodyVoiceFrame::default()
        };
        bridge.publish(frame);
        assert_eq!(bridge.snapshot_or(BodyVoiceFrame::default()), frame);
    }

    #[test]
    fn topology_events_are_one_shot_and_decay() {
        let mut analyzer = BodyVoiceAnalyzer::default();
        let _ = analyzer.update(BodyVoiceFrame::default(), 1.0 / 120.0);
        let detached = analyzer.update(
            BodyVoiceFrame {
                component_count: 2,
                detached_mass_ratio: 0.2,
                ..BodyVoiceFrame::default()
            },
            1.0 / 120.0,
        );
        let later = analyzer.update(
            BodyVoiceFrame {
                component_count: 2,
                detached_mass_ratio: 0.2,
                ..BodyVoiceFrame::default()
            },
            0.1,
        );
        assert!(detached.detach_impulse > 0.0);
        assert!(later.detach_impulse < detached.detach_impulse);
    }
}
