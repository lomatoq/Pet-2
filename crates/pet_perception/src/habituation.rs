use lifecore::{AppraisedEvent, CompanionEventKind, CompanionEventSource};

const CAPACITY: usize = 32;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct HabituationKey {
    source: CompanionEventSource,
    kind: CompanionEventKind,
    target_bin: u8,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct HabituationEntry {
    key: HabituationKey,
    exposure: f32,
    last_seen: f64,
    valid: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HabituationTable {
    entries: [HabituationEntry; CAPACITY],
}

impl Default for HabituationTable {
    fn default() -> Self {
        Self {
            entries: [HabituationEntry::default(); CAPACITY],
        }
    }
}

impl HabituationTable {
    #[must_use]
    pub fn apply(&mut self, mut event: AppraisedEvent) -> AppraisedEvent {
        event = event.sanitized();
        let key = key_for(event);
        let now = event.timestamp;
        self.decay(now);
        let index = self
            .entries
            .iter()
            .position(|entry| entry.valid && entry.key == key)
            .unwrap_or_else(|| self.replacement_index());
        let entry = &mut self.entries[index];
        if !entry.valid || entry.key != key {
            *entry = HabituationEntry {
                key,
                exposure: 0.0,
                last_seen: now,
                valid: true,
            };
        }
        let novelty_override = event.novelty > 0.70
            || event.threat_likelihood > 0.45
            || event.source == CompanionEventSource::DirectContact;
        let attenuation = if novelty_override {
            1.0
        } else {
            (1.0 - entry.exposure * 0.78).clamp(0.12, 1.0)
        };
        event.confidence *= attenuation;
        event.social_likelihood *= attenuation.max(0.35);
        event.play_likelihood *= attenuation.max(0.40);
        entry.exposure = (entry.exposure + 0.16 * (1.0 - event.novelty * 0.55)).clamp(0.0, 1.0);
        entry.last_seen = now;
        event
    }

    fn decay(&mut self, now: f64) {
        if !now.is_finite() {
            return;
        }
        for entry in &mut self.entries {
            if !entry.valid {
                continue;
            }
            let elapsed = (now - entry.last_seen).max(0.0) as f32;
            // Roughly hours, not seconds: familiar desktop events should remain familiar.
            entry.exposure *= (-elapsed / 7_200.0).exp();
            if entry.exposure < 0.002 && elapsed > 21_600.0 {
                entry.valid = false;
            }
        }
    }

    fn replacement_index(&self) -> usize {
        self.entries
            .iter()
            .position(|entry| !entry.valid)
            .unwrap_or_else(|| {
                self.entries
                    .iter()
                    .enumerate()
                    .min_by(|a, b| a.1.exposure.total_cmp(&b.1.exposure))
                    .map_or(0, |(index, _)| index)
            })
    }
}

fn key_for(event: AppraisedEvent) -> HabituationKey {
    let target_bin = event.target.position.map_or(0, |point| {
        let x = (point.x.clamp(0.0, 0.999) * 4.0) as u8;
        let y = (point.y.clamp(0.0, 0.999) * 4.0) as u8;
        1 + x + y * 4
    });
    HabituationKey {
        source: event.source,
        kind: event.kind,
        target_bin,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(novelty: f32, threat: f32, timestamp: f64) -> AppraisedEvent {
        AppraisedEvent {
            source: CompanionEventSource::WindowEnvironment,
            kind: CompanionEventKind::WindowMoved,
            confidence: 1.0,
            novelty,
            threat_likelihood: threat,
            timestamp,
            ..AppraisedEvent::default()
        }
    }

    #[test]
    fn repeated_harmless_event_habituates() {
        let mut table = HabituationTable::default();
        let mut last = 1.0;
        for i in 0..8 {
            last = table.apply(event(0.05, 0.0, i as f64)).confidence;
        }
        assert!(last < 0.5);
    }

    #[test]
    fn threat_dishabituates_response() {
        let mut table = HabituationTable::default();
        for i in 0..8 {
            let _ = table.apply(event(0.05, 0.0, i as f64));
        }
        let dangerous = table.apply(event(0.8, 0.8, 9.0));
        assert!(dangerous.confidence > 0.9);
    }
}
