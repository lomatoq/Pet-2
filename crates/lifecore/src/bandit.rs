use serde::{Deserialize, Serialize};

use crate::{ActionId, AffectState, Drives, SensorFrame, TemperamentGenome};

pub const CONTEXT_SIZE: usize = 16;
pub const ATTENTION_STRATEGY_COUNT: usize = 10;
pub type ContextVector = [f32; CONTEXT_SIZE];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContextualBandit {
    pub weights: Vec<ContextVector>,
    pub attempts: [u32; ATTENTION_STRATEGY_COUNT],
    pub total_updates: u32,
    pub learning_rate: f32,
}

impl ContextualBandit {
    #[must_use]
    pub fn new(temperament: &TemperamentGenome) -> Self {
        Self {
            weights: vec![[0.0; CONTEXT_SIZE]; ATTENTION_STRATEGY_COUNT],
            attempts: [0; ATTENTION_STRATEGY_COUNT],
            total_updates: 0,
            learning_rate: 0.015 + temperament.adaptability * 0.055,
        }
    }

    #[must_use]
    pub fn context(
        sensors: &SensorFrame,
        drives: &Drives,
        affect: &AffectState,
        ignored_attempts: u32,
        successful_interactions: u32,
    ) -> ContextVector {
        let available = sensors.user_availability.unwrap_or({
            if sensors.user_idle_seconds < 90.0 {
                1.0
            } else {
                0.25
            }
        });
        [
            sensors.time_of_day_01.clamp(0.0, 1.0),
            (sensors.user_idle_seconds / 600.0).clamp(0.0, 1.0),
            sensors.user_activity_rate.clamp(0.0, 1.0),
            sensors.active_app_category.index() as f32 / 5.0,
            available.clamp(0.0, 1.0),
            drives.social,
            drives.play,
            affect.attachment,
            (ignored_attempts as f32 / 6.0).clamp(0.0, 1.0),
            (successful_interactions as f32 / 20.0).clamp(0.0, 1.0),
            sensors.cursor_distance_to_pet.clamp(0.0, 1.0),
            affect.valence * 0.5 + 0.5,
            affect.arousal,
            affect.stress,
            affect.confidence,
            1.0,
        ]
    }

    #[must_use]
    pub fn prediction(&self, action: ActionId, context: &ContextVector) -> f32 {
        strategy_index(action).map_or(0.0, |index| dot(&self.weights[index], context))
    }

    #[must_use]
    pub fn exploration_bonus(&self, action: ActionId) -> f32 {
        strategy_index(action).map_or(0.0, |index| {
            let total = self.total_updates.max(1) as f32;
            let attempts = self.attempts[index].max(1) as f32;
            ((total.ln() + 1.0) / attempts).sqrt().clamp(0.0, 2.0) * 0.12
        })
    }

    pub fn update(&mut self, action: ActionId, context: &ContextVector, reward: f32) {
        let Some(index) = strategy_index(action) else {
            return;
        };
        let prediction = dot(&self.weights[index], context);
        let error = reward.clamp(-1.0, 1.0) - prediction;
        for (weight, value) in self.weights[index].iter_mut().zip(context) {
            *weight = (*weight + self.learning_rate * error * value).clamp(-2.0, 2.0);
        }
        self.attempts[index] = self.attempts[index].saturating_add(1);
        self.total_updates = self.total_updates.saturating_add(1);
    }

    #[must_use]
    pub fn is_valid(&self) -> bool {
        self.weights.len() == ATTENTION_STRATEGY_COUNT
            && self
                .weights
                .iter()
                .flatten()
                .all(|weight| weight.is_finite() && weight.abs() <= 2.001)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PendingAttention {
    pub action: ActionId,
    pub context: ContextVector,
    pub elapsed_seconds: f32,
    pub response_window_seconds: f32,
}

#[must_use]
pub fn strategy_index(action: ActionId) -> Option<usize> {
    match action {
        ActionId::SilentStare => Some(0),
        ActionId::ApproachCursor => Some(1),
        ActionId::LandOnWindow => Some(2),
        ActionId::PeekFromEdge => Some(3),
        ActionId::BringProceduralOrb => Some(4),
        ActionId::Chirp => Some(5),
        ActionId::MimicClickRhythm => Some(6),
        ActionId::SelfPlay => Some(7),
        ActionId::HideAndSeek => Some(8),
        ActionId::FrustratedRetreat => Some(9),
        _ => None,
    }
}

fn dot(left: &ContextVector, right: &ContextVector) -> f32 {
    left.iter()
        .zip(right)
        .map(|(a, b)| a * b)
        .sum::<f32>()
        .clamp(-2.0, 2.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reward_moves_prediction_in_expected_direction() {
        let temperament = crate::Genome::from_seed(3).temperament;
        let mut bandit = ContextualBandit::new(&temperament);
        let context = std::array::from_fn(|index| if index % 2 == 0 { 0.8 } else { 0.2 });
        let before = bandit.prediction(ActionId::Chirp, &context);
        bandit.update(ActionId::Chirp, &context, 0.9);
        let after_positive = bandit.prediction(ActionId::Chirp, &context);
        assert!(after_positive > before);
        for _ in 0..20 {
            bandit.update(ActionId::Chirp, &context, -0.8);
        }
        assert!(bandit.prediction(ActionId::Chirp, &context) < after_positive);
    }
}
