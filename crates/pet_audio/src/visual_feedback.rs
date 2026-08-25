use std::sync::{
    Arc, OnceLock,
    atomic::{AtomicU32, AtomicU64, Ordering},
};

use crate::{COMMAND_CAPACITY, SpscRing};

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
    pub request_id: u64,
    pub motif_id: u64,
    pub syllable_index: u8,
    pub envelope: f32,
    pub mouth_open: f32,
    pub pitch_normalized: f32,
    pub noisiness: f32,
    pub purr: f32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct AudioCallbackLevels {
    pub rms: f32,
    pub peak: f32,
}

#[derive(Default)]
pub struct AudioVisualBridge {
    state: AtomicU32,
    request_id: AtomicU64,
    motif_id: AtomicU64,
    envelope: AtomicU32,
    mouth_open: AtomicU32,
    pitch_normalized: AtomicU32,
    noisiness: AtomicU32,
    purr: AtomicU32,
    rms: AtomicU32,
    peak: AtomicU32,
    started_requests: SpscRing<u64, COMMAND_CAPACITY>,
}

impl AudioVisualBridge {
    #[must_use]
    pub fn snapshot(&self) -> AudioVisualFeedback {
        // Pairs with the final Release store in `publish`/`clear`, so an
        // observed state always includes the fields written before it.
        let state = self.state.load(Ordering::Acquire);
        AudioVisualFeedback {
            active: state & 1 != 0,
            syllable_index: ((state >> 8) & 0xff) as u8,
            request_id: self.request_id.load(Ordering::Relaxed),
            motif_id: self.motif_id.load(Ordering::Relaxed),
            envelope: load_f32(&self.envelope),
            mouth_open: load_f32(&self.mouth_open),
            pitch_normalized: load_f32(&self.pitch_normalized),
            noisiness: load_f32(&self.noisiness),
            purr: load_f32(&self.purr),
        }
    }

    #[must_use]
    pub fn levels(&self) -> AudioCallbackLevels {
        AudioCallbackLevels {
            rms: load_f32(&self.rms),
            peak: load_f32(&self.peak),
        }
    }

    /// Consume a durable callback-start event. Unlike the visual envelope, this
    /// cannot be missed when an entire short performance occurs between owner
    /// thread polls.
    pub fn pop_started_request(&self) -> Option<u64> {
        self.started_requests.pop()
    }

    pub(crate) fn publish_started_request(&self, request_id: u64) {
        let _ = self.started_requests.push(request_id);
    }

    pub(crate) fn publish_levels(&self, rms: f32, peak: f32) {
        store_f32(&self.rms, rms.clamp(0.0, 1.0));
        store_f32(&self.peak, peak.clamp(0.0, 1.0));
    }

    pub(crate) fn publish(&self, feedback: AudioVisualFeedback) {
        self.request_id
            .store(feedback.request_id, Ordering::Relaxed);
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
        store_f32(&self.envelope, 0.0);
        store_f32(&self.mouth_open, 0.0);
        store_f32(&self.pitch_normalized, 0.0);
        store_f32(&self.noisiness, 0.0);
        store_f32(&self.purr, 0.0);
        store_f32(&self.rms, 0.0);
        store_f32(&self.peak, 0.0);
        self.request_id.store(0, Ordering::Relaxed);
        self.motif_id.store(0, Ordering::Relaxed);
        self.state.store(0, Ordering::Release);
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
            request_id: 0x00A1_1D10,
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

    #[test]
    fn acquire_snapshot_observes_fields_published_before_active_state() {
        let bridge = Arc::new(AudioVisualBridge::default());
        let feedback = AudioVisualFeedback {
            active: true,
            request_id: 0x51A7_E001,
            motif_id: 0x00A1_1D10,
            syllable_index: 5,
            envelope: 0.83,
            mouth_open: 0.74,
            pitch_normalized: 1.21,
            noisiness: 0.19,
            purr: 1.0,
        };
        let writer = Arc::clone(&bridge);
        let writer = std::thread::spawn(move || writer.publish(feedback));
        let snapshot = loop {
            let snapshot = bridge.snapshot();
            if snapshot.active {
                break snapshot;
            }
            std::hint::spin_loop();
        };
        writer.join().unwrap();

        assert_eq!(snapshot, feedback);
        bridge.clear();
        assert_eq!(bridge.snapshot(), AudioVisualFeedback::default());
    }

    #[test]
    fn callback_start_event_survives_visual_clear_until_consumed() {
        let bridge = AudioVisualBridge::default();
        bridge.publish_started_request(41);
        bridge.clear();
        assert_eq!(bridge.pop_started_request(), Some(41));
        assert_eq!(bridge.pop_started_request(), None);
    }
}
