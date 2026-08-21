use std::collections::VecDeque;

use serde::{Deserialize, Serialize};

use crate::{ActionId, AppCategory, ContextVector};

const SHORT_TERM_CAPACITY: usize = 512;
const EPISODIC_CAPACITY: usize = 256;
const HABIT_CAPACITY: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Success,
    Ignored,
    Rejected,
    Neutral,
    Interrupted,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EventRecord {
    pub timestamp: f64,
    pub context: ContextVector,
    pub action: ActionId,
    pub outcome: Outcome,
    pub reward: f32,
    pub salience: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EpisodicMemory {
    pub representative: EventRecord,
    pub occurrence_count: u32,
    pub mean_reward: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AppInteractionStats {
    pub category: AppCategory,
    pub attempts: u32,
    pub successes: u32,
    pub mean_reward: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActionPreference {
    pub action: ActionId,
    pub mean_reward: f32,
    pub observations: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UserModel {
    pub usual_active_hours: [f32; 24],
    pub interaction_by_app: Vec<AppInteractionStats>,
    pub preferred_attention_strategies: Vec<ActionPreference>,
    pub average_response_delay: f32,
    pub preferred_sound_intensity: f32,
    pub typical_activity_rate: f32,
}

impl Default for UserModel {
    fn default() -> Self {
        Self {
            usual_active_hours: [0.0; 24],
            interaction_by_app: Vec::new(),
            preferred_attention_strategies: Vec::new(),
            average_response_delay: 2.0,
            preferred_sound_intensity: 0.5,
            typical_activity_rate: 0.25,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Habit {
    pub actions: Vec<ActionId>,
    pub success_score: f32,
    pub use_count: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MemorySystem {
    pub short_term: VecDeque<EventRecord>,
    pub episodic: Vec<EpisodicMemory>,
    pub user_model: UserModel,
    pub habits: Vec<Habit>,
}

impl Default for MemorySystem {
    fn default() -> Self {
        Self {
            short_term: VecDeque::with_capacity(SHORT_TERM_CAPACITY),
            episodic: Vec::with_capacity(EPISODIC_CAPACITY),
            user_model: UserModel::default(),
            habits: Vec::with_capacity(HABIT_CAPACITY),
        }
    }
}

impl MemorySystem {
    pub fn record(&mut self, event: EventRecord) {
        if self.short_term.len() == SHORT_TERM_CAPACITY {
            self.short_term.pop_front();
        }
        if event.salience >= 0.32 {
            self.merge_episode(&event);
        }
        self.short_term.push_back(event);
    }

    pub fn consolidate(&mut self) {
        let mut active_hours = [0.0_f32; 24];
        for event in &self.short_term {
            let hour = ((event.timestamp / 3600.0).rem_euclid(24.0)) as usize;
            active_hours[hour] += event.salience.max(0.05);
            update_preference(
                &mut self.user_model.preferred_attention_strategies,
                event.action,
                event.reward,
            );
        }
        let maximum = active_hours
            .iter()
            .copied()
            .fold(0.0_f32, f32::max)
            .max(1.0);
        for (stored, observed) in self
            .user_model
            .usual_active_hours
            .iter_mut()
            .zip(active_hours)
        {
            *stored = (*stored * 0.82 + observed / maximum * 0.18).clamp(0.0, 1.0);
        }
        self.discover_habit();
        self.episodic.sort_by(|left, right| {
            right
                .representative
                .salience
                .total_cmp(&left.representative.salience)
        });
        self.episodic.truncate(EPISODIC_CAPACITY);
    }

    #[must_use]
    pub fn is_valid(&self) -> bool {
        self.short_term.len() <= SHORT_TERM_CAPACITY
            && self.episodic.len() <= EPISODIC_CAPACITY
            && self.habits.len() <= HABIT_CAPACITY
            && self.short_term.iter().all(|event| {
                event.reward.is_finite()
                    && event.salience.is_finite()
                    && event.context.iter().all(|value| value.is_finite())
            })
    }

    fn merge_episode(&mut self, event: &EventRecord) {
        if let Some(existing) = self.episodic.iter_mut().find(|memory| {
            memory.representative.action == event.action
                && memory.representative.outcome == event.outcome
                && context_distance(&memory.representative.context, &event.context) < 0.42
        }) {
            existing.occurrence_count = existing.occurrence_count.saturating_add(1);
            let count = existing.occurrence_count as f32;
            existing.mean_reward += (event.reward - existing.mean_reward) / count;
            existing.representative.salience = existing
                .representative
                .salience
                .max(event.salience)
                .clamp(0.0, 1.0);
            return;
        }
        if self.episodic.len() == EPISODIC_CAPACITY
            && let Some((index, _)) = self.episodic.iter().enumerate().min_by(|left, right| {
                left.1
                    .representative
                    .salience
                    .total_cmp(&right.1.representative.salience)
            })
        {
            self.episodic.swap_remove(index);
        }
        self.episodic.push(EpisodicMemory {
            representative: event.clone(),
            occurrence_count: 1,
            mean_reward: event.reward,
        });
    }

    fn discover_habit(&mut self) {
        let successful: Vec<_> = self
            .short_term
            .iter()
            .filter(|event| event.reward > 0.35)
            .rev()
            .take(4)
            .map(|event| event.action)
            .collect();
        if successful.len() < 3 {
            return;
        }
        let mut actions = successful;
        actions.reverse();
        if let Some(habit) = self
            .habits
            .iter_mut()
            .find(|habit| habit.actions == actions)
        {
            habit.success_score = (habit.success_score * 0.8 + 0.2).clamp(0.0, 1.0);
            habit.use_count = habit.use_count.saturating_add(1);
            return;
        }
        if self.habits.len() == HABIT_CAPACITY {
            self.habits
                .sort_by(|left, right| right.success_score.total_cmp(&left.success_score));
            self.habits.pop();
        }
        self.habits.push(Habit {
            actions,
            success_score: 0.55,
            use_count: 1,
        });
    }
}

fn update_preference(preferences: &mut Vec<ActionPreference>, action: ActionId, reward: f32) {
    if !action.is_attention_strategy() {
        return;
    }
    if let Some(preference) = preferences.iter_mut().find(|entry| entry.action == action) {
        preference.observations = preference.observations.saturating_add(1);
        preference.mean_reward +=
            (reward - preference.mean_reward) / preference.observations as f32;
        return;
    }
    preferences.push(ActionPreference {
        action,
        mean_reward: reward,
        observations: 1,
    });
}

fn context_distance(left: &ContextVector, right: &ContextVector) -> f32 {
    left.iter()
        .zip(right)
        .map(|(a, b)| (a - b).powi(2))
        .sum::<f32>()
        .sqrt()
}
