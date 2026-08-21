use desktop_host::DesktopVisualSample;
use lifecore::{
    BodyFeedback, BodyIntent, FeedbackEvent, LifeState, SensorFrame, VitaMind, VitaOutput,
    VitaPerceptFrame, VitaState,
};
use pet_perception::{PerceptionRuntime, VisualFeatureFrame};

/// Owns VITA's persistent mind and its transient privacy-preserving perception state.
/// Raw device events are reduced to timestamps or scalar features before entering here.
pub struct VitaRuntime {
    mind: VitaMind,
    perception: PerceptionRuntime,
    percept: VitaPerceptFrame,
}

impl VitaRuntime {
    #[must_use]
    pub fn new(identity_seed: u64, restored: Option<VitaState>) -> Self {
        Self {
            mind: restored.map_or_else(
                || VitaMind::new(identity_seed),
                |state| VitaMind::restore(identity_seed, state),
            ),
            perception: PerceptionRuntime::default(),
            percept: VitaPerceptFrame::default(),
        }
    }

    pub fn reset_learning(&mut self, identity_seed: u64) {
        *self = Self::new(identity_seed, None);
    }

    pub fn observe(&mut self, sensors: &SensorFrame, body: &BodyFeedback, dt: f32) {
        self.percept = self.perception.update(sensors, body, dt);
    }

    #[must_use]
    pub fn think(
        &mut self,
        life: &LifeState,
        sensors: &SensorFrame,
        body: &BodyFeedback,
        base_intent: &BodyIntent,
        dt: f32,
    ) -> VitaOutput {
        self.mind
            .tick(&self.percept, sensors, life, body, base_intent, dt)
    }

    pub fn apply_feedback(&mut self, event: &FeedbackEvent) {
        self.mind.apply_feedback(event);
    }

    pub fn note_metamorphosis(&mut self) {
        self.mind.note_metamorphosis();
    }

    /// Records only that a key event occurred; key identity and text never enter VITA.
    pub fn note_key_activity(&mut self, timestamp: f64) {
        self.perception.note_key_activity(timestamp);
    }

    pub fn note_click(&mut self, timestamp: f64) {
        self.perception.note_click(timestamp);
    }

    pub fn note_scroll(&mut self, normalized_delta: f32) {
        self.perception.note_scroll(normalized_delta);
    }

    pub fn set_visual_features(&mut self, sample: DesktopVisualSample) {
        self.perception.set_visual_features(VisualFeatureFrame {
            mean_luminance: sample.mean_luminance,
            local_luminance: sample.local_luminance,
            contrast: sample.contrast,
            colorfulness: sample.colorfulness,
            warmth: sample.warmth,
            dominant_hue: sample.dominant_hue,
            motion_energy: sample.motion_energy,
            edge_density: sample.edge_density,
            sudden_change: sample.sudden_change,
        });
    }

    #[must_use]
    pub fn snapshot(&self) -> VitaState {
        self.mind.snapshot()
    }

    #[must_use]
    pub fn state(&self) -> &VitaState {
        &self.mind.state
    }

    #[must_use]
    pub fn percept(&self) -> &VitaPerceptFrame {
        &self.percept
    }
}

#[cfg(test)]
mod tests {
    use glam::Vec2;
    use lifecore::{Genome, LifeCore};

    use super::*;

    #[test]
    fn bridge_restores_persistent_mind_but_not_raw_history() {
        let core = LifeCore::new(Genome::from_seed(17), 19);
        let mut runtime = VitaRuntime::new(core.state.genome.identity_seed, None);
        runtime.note_key_activity(1.0);
        runtime.observe(
            &SensorFrame {
                timestamp: 1.0,
                cursor_position: Vec2::new(0.7, 0.4),
                ..SensorFrame::default()
            },
            &BodyFeedback::default(),
            1.0 / 60.0,
        );
        let restored = VitaRuntime::new(core.state.genome.identity_seed, Some(runtime.snapshot()));
        assert_eq!(runtime.state(), restored.state());
        assert_eq!(restored.percept(), &VitaPerceptFrame::default());
    }
}
