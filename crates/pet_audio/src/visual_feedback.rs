use std::sync::{
    Arc, OnceLock,
    atomic::{AtomicU32, AtomicU64, Ordering},
};

static GLOBAL_VISUAL_BRIDGE: OnceLock<Arc<AudioVisualBridge>> = OnceLock::new();

#[must_use]
pub fn global_visual_bridge() -> Arc<AudioVisualBridge> {
    Arc::clone(GLOBAL_VISUAL_BRIDGE.get_or_init(|| Arc::new(AudioVisualBridge::default())))
}

#[must_use]
pub fn global_visual_feedback() -> AudioVisualFeedback {
    GLOBAL_VISUAL_BRIDGE
        .get()
        .map_or_else(AudioVisualFeedback::default, |bridge| bridge.snapshot())
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct AudioVisualFeedback {
    pub active: bool,
    pub motif_id: u64,
    pub syllable_index: u8,
    pub envelope: f32,
    pub mouth_open: f32,
    pub pitch_normalized: f32,
    pub noisiness: f32,
    pub purr: f32,
}

#[derive(Debug, Default)]
pub struct AudioVisualBridge {
    state: AtomicU32,
    motif_id: AtomicU64,
    envelope: AtomicU32,
    mouth_open: AtomicU32,
    pitch_normalized: AtomicU32,
    noisiness: AtomicU32,
    purr: AtomicU32,
}

impl AudioVisualBridge {
    #[must_use]
    pub fn snapshot(&self) -> AudioVisualFeedback {
        let state = self.state.load(Ordering::Relaxed);
        AudioVisualFeedback {
            active: state & 1 != 0,
            syllable_index: ((state >> 8) & 0xff) as u8,
            motif_id: self.motif_id.load(Ordering::Relaxed),
            envelope: load_f32(&self.envelope),
            mouth_open: load_f32(&self.mouth_open),
            pitch_normalized: load_f32(&self.pitch_normalized),
            noisiness: load_f32(&self.noisiness),
            purr: load_f32(&self.purr),
        }
    }

    pub(crate) fn publish(&self, feedback: AudioVisualFeedback) {
        self.motif_id.store(feedback.motif_id, Ordering::Relaxed);
        store_f32(&self.envelope, feedback.envelope);
        store_f32(&self.mouth_open, feedback.mouth_open);
        store_f32(&self.pitch_normalized, feedback.pitch_normalized);
        store_f32(&self.noisiness, feedback.noisiness);
        store_f32(&self.purr, feedback.purr);
        let state = u32::from(feedback.active) | (u32::from(feedback.syllable_index) << 8);
        self.state.store(state, Ordering::Release);
    }

    pub(crate) fn clear(&self) {
        self.state.store(0, Ordering::Release);
        store_f32(&self.envelope, 0.0);
        store_f32(&self.mouth_open, 0.0);
        store_f32(&self.pitch_normalized, 0.0);
        store_f32(&self.noisiness, 0.0);
        store_f32(&self.purr, 0.0);
    }
}

fn store_f32(target: &AtomicU32, value: f32) {
    target.store(value.to_bits(), Ordering::Relaxed);
}

fn load_f32(source: &AtomicU32) -> f32 {
    f32::from_bits(source.load(Ordering::Relaxed))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bridge_round_trips_without_locks() {
        let bridge = AudioVisualBridge::default();
        let feedback = AudioVisualFeedback {
            active: true,
            motif_id: 44,
            syllable_index: 3,
            envelope: 0.72,
            mouth_open: 0.61,
            pitch_normalized: 1.14,
            noisiness: 0.22,
            purr: 0.9,
        };
        bridge.publish(feedback);
        assert_eq!(bridge.snapshot(), feedback);
        bridge.clear();
        assert!(!bridge.snapshot().active);
    }
}
