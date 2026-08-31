use std::collections::VecDeque;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    ACTION_COUNT, ActionId, AffectState, BodyFeedback, ContextualBandit, DevelopmentState, Drives,
    Genome, MemorySystem, MicroBrain, PendingAttention, VocalMotif, generate_initial_motifs,
};

pub const LIFE_SNAPSHOT_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecentVocalization {
    pub motif_id: u64,
    pub family_id: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PendingVocalCredit {
    pub motif_id: u64,
    pub context: [f32; 16],
    pub elapsed_seconds: f32,
    pub response_window_seconds: f32,
    pub penalize_if_ignored: bool,
}

/// A performance selected by the mind but not yet confirmed audible by the
/// host. Selection is deliberately reversible: repertoire history and outcome
/// credit only change after the audio callback reports this exact request.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PendingVocalDelivery {
    pub request_id: u64,
    pub motif_id: u64,
    pub family_id: u64,
    pub context: [f32; 16],
    pub response_window_seconds: f32,
    pub penalize_if_ignored: bool,
}

impl PendingVocalDelivery {
    #[must_use]
    fn is_valid(&self) -> bool {
        self.context.iter().all(|value| value.is_finite())
            && self.response_window_seconds.is_finite()
            && (0.25..=30.0).contains(&self.response_window_seconds)
    }
}

impl PendingVocalCredit {
    #[must_use]
    fn is_valid(&self) -> bool {
        self.context.iter().all(|value| value.is_finite())
            && self.elapsed_seconds.is_finite()
            && self.elapsed_seconds >= 0.0
            && self.response_window_seconds.is_finite()
            && (0.25..=30.0).contains(&self.response_window_seconds)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AttentionBudget {
    pub current: f32,
    pub maximum: f32,
    pub recovery_per_second: f32,
}

impl Default for AttentionBudget {
    fn default() -> Self {
        Self {
            current: 1.0,
            maximum: 1.0,
            recovery_per_second: 1.0 / 90.0,
        }
    }
}

impl AttentionBudget {
    pub fn recover(&mut self, dt: f32) {
        self.current = (self.current + self.recovery_per_second * dt).clamp(0.0, self.maximum);
    }

    pub fn spend(&mut self, amount: f32) -> bool {
        if amount <= 0.0 {
            return true;
        }
        if self.current + f32::EPSILON < amount {
            return false;
        }
        self.current = (self.current - amount).max(0.0);
        true
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LifeState {
    pub genome: Genome,
    pub drives: Drives,
    pub affect: AffectState,
    pub current_action: ActionId,
    pub action_elapsed_seconds: f32,
    pub action_cooldowns: [f32; ACTION_COUNT],
    pub recent_actions: VecDeque<ActionId>,
    pub attention_budget: AttentionBudget,
    pub pending_attention: Option<PendingAttention>,
    pub ignored_attempts: u32,
    pub successful_interactions: u32,
    pub recent_reward: f32,
    pub focus_mode: bool,
    pub vocal_motifs: Vec<VocalMotif>,
    pub selected_motif_id: Option<u64>,
    #[serde(default)]
    pub recent_vocalizations: VecDeque<RecentVocalization>,
    #[serde(default)]
    pub pending_vocal_credit: Option<PendingVocalCredit>,
    #[serde(default)]
    pub pending_vocal_delivery: Option<PendingVocalDelivery>,
    pub development: DevelopmentState,
    pub last_body_feedback: BodyFeedback,
    pub tick_count: u64,
    pub elapsed_seconds: f64,
}

impl LifeState {
    #[must_use]
    pub fn new(genome: Genome) -> Self {
        let drives = Drives::initial(&genome.temperament);
        let vocal_motifs = generate_initial_motifs(&genome.voice);
        Self {
            genome,
            drives,
            affect: AffectState::default(),
            current_action: ActionId::IdleHover,
            action_elapsed_seconds: 0.0,
            action_cooldowns: [0.0; ACTION_COUNT],
            recent_actions: VecDeque::with_capacity(16),
            attention_budget: AttentionBudget::default(),
            pending_attention: None,
            ignored_attempts: 0,
            successful_interactions: 0,
            recent_reward: 0.0,
            focus_mode: false,
            vocal_motifs,
            selected_motif_id: None,
            recent_vocalizations: VecDeque::new(),
            pending_vocal_credit: None,
            pending_vocal_delivery: None,
            development: DevelopmentState::default(),
            last_body_feedback: BodyFeedback::default(),
            tick_count: 0,
            elapsed_seconds: 0.0,
        }
    }

    #[must_use]
    pub fn is_valid(&self) -> bool {
        self.genome.is_valid()
            && self.development.is_valid_for_genome(&self.genome)
            && self.drives.is_finite()
            && self.affect.is_finite()
            && self.action_elapsed_seconds.is_finite()
            && self.action_cooldowns.iter().all(|value| value.is_finite())
            && self.attention_budget.current.is_finite()
            && self.attention_budget.maximum.is_finite()
            && self.attention_budget.current >= 0.0
            && self.attention_budget.current <= self.attention_budget.maximum + f32::EPSILON
            && self.vocal_motifs.len() <= 24
            && self.vocal_motifs.iter().all(|motif| {
                !motif.syllables.is_empty()
                    && motif.syllables.len() <= 6
                    && motif.expected_reward.is_finite()
                    && motif.novelty.is_finite()
                    && motif
                        .context_weights
                        .iter()
                        .all(|weight| weight.is_finite())
            })
            && self.recent_vocalizations.len() <= 4
            && self.pending_vocal_credit.as_ref().is_none_or(|credit| {
                credit.is_valid()
                    && self
                        .vocal_motifs
                        .iter()
                        .any(|motif| motif.id == credit.motif_id)
            })
            && self.pending_vocal_delivery.as_ref().is_none_or(|delivery| {
                delivery.is_valid()
                    && self
                        .vocal_motifs
                        .iter()
                        .any(|motif| motif.id == delivery.motif_id)
            })
            && self.last_body_feedback.world_position.is_finite()
            && self.last_body_feedback.velocity.is_finite()
            && self.last_body_feedback.acceleration.is_finite()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedRngState {
    pub seed: [u8; 32],
    pub stream: u64,
    pub word_position: u128,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LifeSnapshot {
    pub schema_version: u32,
    pub state: LifeState,
    pub brain: MicroBrain,
    pub habits: ContextualBandit,
    pub memories: MemorySystem,
    pub rng: SavedRngState,
}

impl LifeSnapshot {
    pub fn validate(&self) -> Result<(), LifeError> {
        if self.schema_version != LIFE_SNAPSHOT_SCHEMA_VERSION {
            return Err(LifeError::UnsupportedSchema {
                found: self.schema_version,
                expected: LIFE_SNAPSHOT_SCHEMA_VERSION,
            });
        }
        if !self.state.is_valid() {
            return Err(LifeError::InvalidState(
                "LifeState bounds or finite check failed",
            ));
        }
        if !self.brain.is_valid() {
            return Err(LifeError::InvalidState(
                "MicroBrain structure or bounds are invalid",
            ));
        }
        if !self.habits.is_valid() {
            return Err(LifeError::InvalidState(
                "ContextualBandit structure is invalid",
            ));
        }
        if !self.memories.is_valid() {
            return Err(LifeError::InvalidState(
                "Memory limits or values are invalid",
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum LifeError {
    #[error("unsupported LifeSnapshot schema {found}; expected {expected}")]
    UnsupportedSchema { found: u32, expected: u32 },
    #[error("invalid LifeSnapshot: {0}")]
    InvalidState(&'static str),
}
