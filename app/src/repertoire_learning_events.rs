//! Observations of explicit social feedback and the existing convention learner.
//! No rewards, illnesses, user emotions, or periodic gestures are invented here.
use glam::Vec2;
use lifecore::FeedbackEvent;
use pet_motor::RepertoireEvent;

pub const LEARNING_REPERTOIRE_IDS: &[u16] = &[167, 168, 197, 200];

#[derive(Default)]
pub struct RepertoireLearningEvents {
    pending: Vec<RepertoireEvent>,
    last_clarification: Option<f64>,
    last_learning_ack: Option<f64>,
    absent: bool,
}

impl RepertoireLearningEvents {
    fn push(&mut self, id: u16, target: Option<Vec2>) {
        if self.pending.len() < 4 && !self.pending.iter().any(|event| event.id == id) {
            self.pending.push(RepertoireEvent {
                id,
                confidence: 0.9,
                target: target.filter(|p| p.is_finite()),
                side: 0.0,
            });
        }
    }

    pub fn feedback(&mut self, event: &FeedbackEvent, inviting: bool) {
        // Silence is not rejection. Existing cancellation owns retreat/safety.
        if inviting && matches!(event, FeedbackEvent::PushedAway | FeedbackEvent::MuteOrHide) {
            self.push(167, None);
        }
    }

    pub fn ambiguous_gesture(&mut self, now: f64, target: Vec2) {
        if !now.is_finite()
            || self
                .last_clarification
                .is_some_and(|last| now - last < 20.0)
        {
            return;
        }
        self.last_clarification = Some(now);
        self.push(168, Some(target));
    }

    pub fn confirmed_convention_update(&mut self, now: f64, target: Vec2) {
        if !now.is_finite() || self.last_learning_ack.is_some_and(|last| now - last < 15.0) {
            return;
        }
        self.last_learning_ack = Some(now);
        self.push(197, Some(target));
    }

    pub fn observe_activity(&mut self, idle_seconds: f32, scene_target: Option<Vec2>) {
        if !idle_seconds.is_finite() || idle_seconds < 0.0 {
            return;
        }
        if idle_seconds >= 1800.0 {
            self.absent = true;
        }
        if self.absent && idle_seconds < 2.0 {
            self.absent = false;
            // A single scene check; no wake command, social demand or penalty.
            self.push(200, scene_target);
        }
    }

    pub fn drain(&mut self) -> Vec<RepertoireEvent> {
        std::mem::take(&mut self.pending)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn silence_is_not_refusal_and_events_are_bounded() {
        let mut observer = RepertoireLearningEvents::default();
        observer.feedback(&FeedbackEvent::Ignored, true);
        observer.feedback(&FeedbackEvent::PushedAway, false);
        assert!(observer.drain().is_empty());
        for _ in 0..1000 {
            observer.feedback(&FeedbackEvent::PushedAway, true);
        }
        assert_eq!(observer.drain().len(), 1);
    }
    #[test]
    fn no_periodic_clarification_or_absence_performance() {
        let mut observer = RepertoireLearningEvents::default();
        observer.ambiguous_gesture(1.0, Vec2::ZERO);
        assert_eq!(observer.drain()[0].id, 168);
        for tick in 0..300 {
            observer.ambiguous_gesture(1.0 + f64::from(tick) * 0.05, Vec2::ZERO);
            observer.observe_activity(4000.0, None);
            assert!(observer.drain().is_empty());
        }
        observer.observe_activity(0.0, Some(Vec2::X));
        assert_eq!(observer.drain()[0].id, 200);
        for _ in 0..1000 {
            observer.observe_activity(0.0, Some(Vec2::X));
            assert!(observer.drain().is_empty());
        }
    }
}
