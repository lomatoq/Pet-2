use serde::{Deserialize, Serialize};

use crate::{BodyFeedback, Drives, FeltStateV1, SensorFrame, TemperamentGenome};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct AffectState {
    pub valence: f32,
    pub arousal: f32,
    pub stress: f32,
    pub confidence: f32,
    pub attachment: f32,
    pub frustration: f32,
}

impl Default for AffectState {
    fn default() -> Self {
        Self {
            valence: 0.15,
            arousal: 0.30,
            stress: 0.08,
            confidence: 0.48,
            attachment: 0.05,
            frustration: 0.04,
        }
    }
}

impl AffectState {
    #[allow(clippy::too_many_arguments)]
    pub fn update(
        &mut self,
        drives: &Drives,
        sensors: &SensorFrame,
        body: &BodyFeedback,
        temperament: &TemperamentGenome,
        recent_reward: f32,
        ignored_attempts: u32,
        dt: f32,
    ) {
        let presence = sensors.user_presence.unwrap_or({
            if sensors.user_idle_seconds < 180.0 {
                1.0
            } else {
                0.0
            }
        });
        let threat = drives.safety.max(
            body.collision
                .as_ref()
                .map_or(0.0, |collision| collision.intensity),
        );
        let mean_need = drives.homeostatic_cost() / 7.9;
        let ignored = (ignored_attempts as f32 / 5.0).clamp(0.0, 1.0);
        let pose_stress = body.pose_error.clamp(0.0, 1.0);

        let valence_target =
            (0.35 + recent_reward * 0.55 - mean_need * 0.7 - threat * 0.35).clamp(-1.0, 1.0);
        let arousal_target = (0.12
            + sensors.user_activity_rate * 0.32
            + sensors.cursor_velocity.length() * 0.12
            + drives.play * 0.25
            + threat * 0.42)
            .clamp(0.0, 1.0);
        let stress_target = (threat * 0.72 + pose_stress * 0.25 + ignored * 0.24
            - temperament.boldness * 0.18)
            .clamp(0.0, 1.0);
        let confidence_target = (0.38 + recent_reward * 0.28 + temperament.boldness * 0.28
            - ignored * 0.32
            - threat * 0.30)
            .clamp(0.0, 1.0);
        let attachment_target = (self.attachment
            + presence * temperament.attachment_speed * recent_reward.max(0.0) * 0.02)
            .clamp(0.0, 1.0);
        let frustration_target = (ignored * (0.5 + temperament.persistence * 0.35)
            + (-recent_reward).max(0.0) * 0.35)
            .clamp(0.0, 1.0);

        self.valence = smooth(self.valence, valence_target, 2.2, dt).clamp(-1.0, 1.0);
        self.arousal = smooth(self.arousal, arousal_target, 2.8, dt).clamp(0.0, 1.0);
        self.stress = smooth(self.stress, stress_target, 3.6, dt).clamp(0.0, 1.0);
        self.confidence = smooth(self.confidence, confidence_target, 1.4, dt).clamp(0.0, 1.0);
        self.attachment = smooth(self.attachment, attachment_target, 0.28, dt).clamp(0.0, 1.0);
        self.frustration = smooth(self.frustration, frustration_target, 1.8, dt).clamp(0.0, 1.0);
    }

    /// Fast bounded somatic evidence. This supplements, rather than replaces,
    /// the existing drive/event affect model.
    pub fn integrate_felt_state(&mut self, felt: FeltStateV1, dt: f32) {
        let threat = (0.45 * felt.pain_like
            + 0.30 * felt.physical_load
            + 0.25 * (1.0 - felt.body_integrity))
            .clamp(0.0, 1.0);
        let valence_evidence = (0.30 * felt.comfort + 0.35 * felt.relief
            - 0.55 * felt.pain_like
            - 0.35 * felt.restraint)
            .clamp(-1.0, 1.0);
        let confidence_evidence = (felt.motor_efficacy * felt.agency_match).clamp(0.0, 1.0);
        let frustration_evidence = (felt.restraint * (1.0 - felt.agency_match)
            + (1.0 - felt.motor_efficacy) * 0.25)
            .clamp(0.0, 1.0);
        let attachment_evidence = (felt.contact_pleasantness * felt.social_safety).clamp(0.0, 1.0);
        self.valence = smooth(self.valence, valence_evidence, 3.5, dt).clamp(-1.0, 1.0);
        self.stress = smooth(self.stress, threat, 8.0, dt).clamp(0.0, 1.0);
        self.confidence = smooth(self.confidence, confidence_evidence, 2.0, dt).clamp(0.0, 1.0);
        self.frustration = smooth(self.frustration, frustration_evidence, 4.0, dt).clamp(0.0, 1.0);
        self.attachment = smooth(self.attachment, attachment_evidence, 0.18, dt).clamp(0.0, 1.0);
    }

    #[must_use]
    pub fn is_finite(&self) -> bool {
        [
            self.valence,
            self.arousal,
            self.stress,
            self.confidence,
            self.attachment,
            self.frustration,
        ]
        .into_iter()
        .all(f32::is_finite)
    }
}

fn smooth(current: f32, target: f32, speed: f32, dt: f32) -> f32 {
    current + (target - current) * (1.0 - (-speed * dt).exp())
}
