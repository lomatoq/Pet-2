//! Deterministic artificial-life simulation.
//!
//! `LifeCore` owns needs, affect, learning, memories, action arbitration, voice
//! vocabulary, and development. It consumes normalized data and has no knowledge of
//! graphics, audio devices, native windows, physical pixels, or operating systems.

mod actions;
mod affect;
mod bandit;
mod development;
mod drives;
mod genome;
mod memory;
mod microbrain;
mod persistence;
mod vita;

use std::{array, collections::VecDeque};

use glam::Vec2;
use rand::{RngCore, SeedableRng};
use rand_chacha::ChaCha8Rng;
use serde::{Deserialize, Serialize};

pub use actions::*;
pub use affect::*;
pub use bandit::*;
pub use development::*;
pub use drives::*;
pub use genome::*;
pub use memory::*;
pub use microbrain::*;
pub use persistence::*;
pub use vita::*;

pub const LIFECORE_HZ: f32 = 20.0;
const RECENT_ACTION_CAPACITY: usize = 16;
const MAX_IGNORED_ATTEMPTS: u32 = 8;
const RECENT_VOCAL_CAPACITY: usize = 4;
const EXACT_VOCAL_REPEAT_WINDOW: usize = 3;
const MAX_VOCAL_FAMILY_SIZE: usize = 4;
const VOCAL_CONTEXT_WEIGHT_LIMIT: f32 = 1.0;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum FeedbackEvent {
    PettingStarted,
    PlayStarted,
    CursorApproached,
    RespondedAfterSound,
    Observed,
    Ignored,
    PushedAway,
    MuteOrHide,
    FocusModeEnabled,
    FocusModeDisabled,
    Reward(f32),
}

impl FeedbackEvent {
    #[must_use]
    pub fn reward(&self) -> f32 {
        match self {
            Self::PettingStarted => 0.90,
            Self::PlayStarted => 0.70,
            Self::CursorApproached => 0.50,
            Self::RespondedAfterSound => 0.40,
            Self::Observed => 0.25,
            Self::Ignored => -0.25,
            Self::PushedAway => -0.60,
            Self::MuteOrHide => -0.80,
            Self::FocusModeEnabled => -1.00,
            Self::FocusModeDisabled => 0.0,
            Self::Reward(value) => value.clamp(-1.0, 1.0),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DebugState {
    pub strongest_drive: DriveKind,
    pub strongest_drive_value: f32,
    pub attention_budget: f32,
    pub selected_motif_id: Option<u64>,
    pub predicted_action_value: f32,
    pub recent_reward: f32,
    pub plastic_weight_range: (f32, f32),
    pub ignored_attempts: u32,
    pub brain_divergence_count: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LifeOutput {
    pub body_intent: BodyIntent,
    pub vocal_request: Option<VocalRequest>,
    pub selected_action: ActionId,
    pub affect: AffectState,
    pub debug: DebugState,
}

pub struct LifeCore {
    pub state: LifeState,
    brain: MicroBrain,
    habits: ContextualBandit,
    memories: MemorySystem,
    rng: ChaCha8Rng,
    rng_seed: [u8; 32],
}

impl LifeCore {
    #[must_use]
    pub fn new(genome: Genome, seed: u64) -> Self {
        let mut seed_rng = ChaCha8Rng::seed_from_u64(seed);
        let mut rng_seed = [0_u8; 32];
        seed_rng.fill_bytes(&mut rng_seed);
        let brain = MicroBrain::new(genome.brain.clone());
        let habits = ContextualBandit::new(&genome.temperament);
        Self {
            state: LifeState::new(genome),
            brain,
            habits,
            memories: MemorySystem::default(),
            rng: ChaCha8Rng::from_seed(rng_seed),
            rng_seed,
        }
    }

    pub fn tick(&mut self, sensors: &SensorFrame, body: &BodyFeedback, dt: f32) -> LifeOutput {
        let dt = finite_dt(dt);
        // Older snapshots could accumulate hundreds of implicit "ignores" because
        // solitary play was misclassified as a bid for user attention. Treat this
        // value as a bounded recent streak so a repaired companion can recover
        // without discarding the rest of its learned state.
        self.state.ignored_attempts = self.state.ignored_attempts.min(MAX_IGNORED_ATTEMPTS);
        self.state.tick_count = self.state.tick_count.saturating_add(1);
        self.state.elapsed_seconds += f64::from(dt);
        self.state.action_elapsed_seconds += dt;
        self.state.last_body_feedback = body.clone();
        self.state.attention_budget.recover(dt);
        for cooldown in &mut self.state.action_cooldowns {
            *cooldown = (*cooldown - dt).max(0.0);
        }
        self.resolve_expired_attention(dt);
        self.resolve_expired_vocal_credit(dt);

        let previous_cost = self.state.drives.homeostatic_cost();
        self.state.drives.update(
            &self.state.genome.temperament,
            sensors,
            self.state.current_action,
            dt,
        );
        let homeostatic_reward =
            (previous_cost - self.state.drives.homeostatic_cost()).clamp(-1.0, 1.0);
        self.state.recent_reward = smooth(self.state.recent_reward, homeostatic_reward, 0.8, dt);

        self.state.affect.update(
            &self.state.drives,
            sensors,
            body,
            &self.state.genome.temperament,
            self.state.recent_reward,
            self.state.ignored_attempts,
            dt,
        );
        self.update_lifetime_statistics(sensors, body, dt);

        let brain_inputs = build_brain_inputs(&self.state, sensors, body);
        let readouts = self.brain.tick(&brain_inputs, dt);
        let context = ContextualBandit::context(
            sensors,
            &self.state.drives,
            &self.state.affect,
            self.state.ignored_attempts,
            self.state.successful_interactions,
        );
        let scores = self.score_actions(sensors, body, &context, &readouts.actions);
        let sampled = sample_softmax(&scores, action_temperature(&self.state), &mut self.rng);
        let switched = self.maybe_switch_action(sampled, &scores, sensors, body, &context);
        let expression = ExpressionState::from_readouts(readouts.expressions, self.state.affect);
        let body_intent = body_intent_for(self.state.current_action, sensors, body, expression);
        let vocal_request = if switched && self.state.current_action.is_vocal() {
            self.select_vocal_request(
                VocalTrigger::Action(self.state.current_action),
                sensors,
                &context,
            )
        } else {
            None
        };
        let (strongest_drive, strongest_drive_value) = self.state.drives.strongest();
        let debug = DebugState {
            strongest_drive,
            strongest_drive_value,
            attention_budget: self.state.attention_budget.current,
            selected_motif_id: self.state.selected_motif_id,
            predicted_action_value: scores[self.state.current_action.index()],
            recent_reward: self.state.recent_reward,
            plastic_weight_range: self.brain.plastic_weight_range(),
            ignored_attempts: self.state.ignored_attempts,
            brain_divergence_count: self.brain.divergence_count,
        };
        LifeOutput {
            body_intent,
            vocal_request,
            selected_action: self.state.current_action,
            affect: self.state.affect,
            debug,
        }
    }

    pub fn apply_feedback(&mut self, event: FeedbackEvent) {
        match event {
            FeedbackEvent::FocusModeEnabled => self.state.focus_mode = true,
            FeedbackEvent::FocusModeDisabled => self.state.focus_mode = false,
            _ => {}
        }
        let reward = event.reward();
        self.state.recent_reward = reward;
        self.brain.apply_reward(reward);
        let (action, context, delay) = self.state.pending_attention.take().map_or(
            (self.state.current_action, [0.0; CONTEXT_SIZE], 0.0),
            |pending| (pending.action, pending.context, pending.elapsed_seconds),
        );
        self.habits.update(action, &context, reward);
        if reward > 0.0 {
            self.state.successful_interactions =
                self.state.successful_interactions.saturating_add(1);
            self.state.ignored_attempts = self.state.ignored_attempts.saturating_sub(1);
        } else if reward < 0.0 {
            self.state.ignored_attempts = self
                .state
                .ignored_attempts
                .saturating_add(1)
                .min(MAX_IGNORED_ATTEMPTS);
        }
        if matches!(
            event,
            FeedbackEvent::MuteOrHide | FeedbackEvent::FocusModeEnabled
        ) {
            self.state.attention_budget.current = 0.0;
        }
        self.apply_vocal_feedback(reward);
        let outcome = outcome_for_reward(reward);
        self.memories.record(EventRecord {
            timestamp: self.state.elapsed_seconds,
            context,
            action,
            outcome,
            reward,
            salience: salience(reward, self.state.affect.attachment, outcome),
        });
        if delay > 0.0 {
            self.memories.user_model.average_response_delay = smooth(
                self.memories.user_model.average_response_delay,
                delay,
                0.15,
                1.0,
            );
        }
    }

    pub fn consolidate_sleep(&mut self) {
        self.memories.consolidate();
        self.brain.consolidate();
        self.repair_vocal_repertoire();
        let mut created = 0;
        let mut mutated_families = Vec::new();
        let candidates: Vec<_> = self
            .state
            .vocal_motifs
            .iter()
            .filter(|motif| motif.expected_reward > 0.18 && motif.success_count > 0)
            .cloned()
            .collect();
        for parent in candidates {
            if created == 2 {
                break;
            }
            let family = motif_family_id(&self.state.vocal_motifs, parent.id);
            if mutated_families.contains(&family) || random_unit(&mut self.rng) > 0.42 {
                continue;
            }
            let child = mutate_motif(&parent, self.rng.next_u64());
            let distinct = self
                .state
                .vocal_motifs
                .iter()
                .all(|existing| motif_distance(existing, &child) > 0.025);
            if distinct {
                self.state.vocal_motifs.push(child);
                mutated_families.push(family);
                created += 1;
            }
        }
        self.repair_vocal_repertoire();
    }

    pub fn trigger_metamorphosis(&mut self) -> DevelopmentResult {
        let result = development::metamorphose(
            &mut self.state.genome,
            &mut self.state.development,
            &mut self.rng,
        );
        self.brain
            .apply_development(&self.state.genome.brain, self.state.genome.mutation_rate);
        self.state.current_action = ActionId::Metamorphosis;
        self.state.action_elapsed_seconds = 0.0;
        result
    }

    #[must_use]
    pub fn snapshot(&self) -> LifeSnapshot {
        LifeSnapshot {
            schema_version: LIFE_SNAPSHOT_SCHEMA_VERSION,
            state: self.state.clone(),
            brain: self.brain.clone(),
            habits: self.habits.clone(),
            memories: self.memories.clone(),
            rng: SavedRngState {
                seed: self.rng_seed,
                stream: self.rng.get_stream(),
                word_position: self.rng.get_word_pos(),
            },
        }
    }

    pub fn restore(snapshot: LifeSnapshot) -> Result<Self, LifeError> {
        snapshot.validate()?;
        let mut rng = ChaCha8Rng::from_seed(snapshot.rng.seed);
        rng.set_stream(snapshot.rng.stream);
        rng.set_word_pos(snapshot.rng.word_position);
        let mut restored = Self {
            state: snapshot.state,
            brain: snapshot.brain,
            habits: snapshot.habits,
            memories: snapshot.memories,
            rng,
            rng_seed: snapshot.rng.seed,
        };
        restored.repair_vocal_repertoire();
        Ok(restored)
    }

    pub fn set_focus_mode(&mut self, enabled: bool) {
        self.state.focus_mode = enabled;
        if enabled {
            self.state.attention_budget.current = 0.0;
        }
    }

    pub fn reset_learning(&mut self) {
        self.habits = ContextualBandit::new(&self.state.genome.temperament);
        self.memories = MemorySystem::default();
        self.brain = MicroBrain::new(self.state.genome.brain.clone());
        self.state.ignored_attempts = 0;
        self.state.successful_interactions = 0;
        self.state.recent_reward = 0.0;
        self.state.vocal_motifs = generate_initial_motifs(&self.state.genome.voice);
        self.state.selected_motif_id = None;
        self.state.recent_vocalizations.clear();
        self.state.pending_vocal_credit = None;
        self.state.pending_vocal_delivery = None;
    }

    /// Ask the mind for an interaction sound without bypassing its learned
    /// repertoire. Hosts should call this after applying the interaction event
    /// that caused the response, then enqueue the returned request.
    pub fn request_vocalization(
        &mut self,
        trigger: VocalTrigger,
        sensors: &SensorFrame,
    ) -> Option<VocalRequest> {
        if !trigger.is_supported() {
            return None;
        }
        let context = ContextualBandit::context(
            sensors,
            &self.state.drives,
            &self.state.affect,
            self.state.ignored_attempts,
            self.state.successful_interactions,
        );
        self.select_vocal_request(trigger, sensors, &context)
    }

    /// Confirm that the callback began rendering this exact performance.
    /// Repertoire recency, use count, and response credit begin here rather than
    /// when a request merely enters a host-side queue.
    pub fn confirm_vocal_request_heard(&mut self, request_id: u64) -> bool {
        if self
            .state
            .pending_vocal_delivery
            .as_ref()
            .is_none_or(|pending| pending.request_id != request_id)
        {
            return false;
        }
        let delivery = self
            .state
            .pending_vocal_delivery
            .take()
            .expect("matching vocal delivery exists");
        for motif in &mut self.state.vocal_motifs {
            motif.novelty = (motif.novelty + (1.0 - motif.novelty) * 0.035).clamp(0.0, 1.0);
        }
        let Some(motif) = self
            .state
            .vocal_motifs
            .iter_mut()
            .find(|motif| motif.id == delivery.motif_id)
        else {
            return false;
        };
        motif.use_count = motif.use_count.saturating_add(1);
        motif.novelty *= 0.62;
        self.state.selected_motif_id = Some(delivery.motif_id);
        if self.state.recent_vocalizations.len() == RECENT_VOCAL_CAPACITY {
            self.state.recent_vocalizations.pop_front();
        }
        self.state
            .recent_vocalizations
            .push_back(RecentVocalization {
                motif_id: delivery.motif_id,
                family_id: delivery.family_id,
            });
        self.state.pending_vocal_credit = Some(PendingVocalCredit {
            motif_id: delivery.motif_id,
            context: delivery.context,
            elapsed_seconds: 0.0,
            response_window_seconds: delivery.response_window_seconds,
            penalize_if_ignored: delivery.penalize_if_ignored,
        });
        true
    }

    /// Cancel a queued performance that never reached the callback. Since no
    /// learning state is committed before `confirm_vocal_request_heard`, this is
    /// a lossless discard rather than a fragile rollback.
    pub fn cancel_vocal_request(&mut self, request_id: u64) {
        if self
            .state
            .pending_vocal_delivery
            .as_ref()
            .is_some_and(|pending| pending.request_id == request_id)
        {
            self.state.pending_vocal_delivery = None;
        }
    }

    /// Audio queues and streams are process-owned. A restored mind keeps learned
    /// history, but must never retain delivery or feedback windows from a stream
    /// that no longer exists.
    pub fn reset_vocal_delivery_session(&mut self) {
        self.state.pending_vocal_delivery = None;
        self.state.pending_vocal_credit = None;
    }

    #[must_use]
    pub fn memory_system(&self) -> &MemorySystem {
        &self.memories
    }

    #[must_use]
    pub fn contextual_bandit(&self) -> &ContextualBandit {
        &self.habits
    }

    fn score_actions(
        &self,
        sensors: &SensorFrame,
        body: &BodyFeedback,
        context: &ContextVector,
        brain_priors: &[f32; ACTION_COUNT],
    ) -> [f32; ACTION_COUNT] {
        array::from_fn(|index| {
            let action = ActionId::ALL[index];
            let definition = action.definition();
            if action == self.state.current_action
                && self.state.action_elapsed_seconds >= definition.maximum_duration
            {
                return -100.0;
            }
            if self.state.focus_mode && !action.is_focus_allowed() {
                return -100.0;
            }
            if !conditions_met(definition.required_conditions, &self.state, sensors, body) {
                return -100.0;
            }
            let expected_relief = self.state.drives.relief_value(definition.drive_relief) * 1.7;
            let learned = self.habits.prediction(action, context);
            let exploration = self.habits.exploration_bonus(action)
                * (0.35 + self.state.genome.temperament.exploration_rate);
            let novelty = (1.0 - repetition_ratio(&self.state.recent_actions, action))
                * self.state.drives.novelty
                * 0.20;
            let continuation = if action == self.state.current_action {
                0.34
            } else {
                0.0
            };
            let cooldown_penalty = if self.state.action_cooldowns[index] > 0.0 {
                6.0 + self.state.action_cooldowns[index] * 0.1
            } else {
                0.0
            };
            let repetition_penalty = repetition_ratio(&self.state.recent_actions, action) * 0.65;
            let interruption_cost = if action != self.state.current_action
                && self.state.action_elapsed_seconds
                    < self.state.current_action.definition().minimum_duration
            {
                self.state.current_action.definition().interruption_cost
            } else {
                0.0
            };
            let annoyance_cost =
                if definition.unsolicited_attention_cost > self.state.attention_budget.current {
                    8.0
                } else {
                    definition.unsolicited_attention_cost
                        * (1.0 + self.state.ignored_attempts as f32 * 0.35)
                };
            brain_priors[index] * 0.62
                + expected_relief
                + learned
                + exploration
                + novelty
                + temperament_bias(action, &self.state)
                + continuation
                - cooldown_penalty
                - repetition_penalty
                - interruption_cost
                - annoyance_cost
        })
    }

    fn maybe_switch_action(
        &mut self,
        sampled: ActionId,
        scores: &[f32; ACTION_COUNT],
        sensors: &SensorFrame,
        body: &BodyFeedback,
        context: &ContextVector,
    ) -> bool {
        let current = self.state.current_action;
        // A captured pointer is a physical manipulation session, not a request
        // to replace the navigation controller on the same tick. Switching from
        // e.g. Sleep/Wander to Hover here produced a large bounded acceleration
        // exactly on mouse-down, which looked like an input hitch even though the
        // frame itself stayed on budget. Feedback and affect are still processed;
        // arbitration resumes normally after release.
        if sensors.pet_dragged {
            return false;
        }
        let current_definition = current.definition();
        let direct_interaction =
            sensors.pointer_pressed || sensors.pet_touched || sensors.pet_dragged;
        let threat = self.state.drives.safety > 0.62
            || body
                .collision
                .as_ref()
                .is_some_and(|collision| collision.intensity > 0.45);
        let minimum_done = self.state.action_elapsed_seconds >= current_definition.minimum_duration;
        let maximum_done = self.state.action_elapsed_seconds >= current_definition.maximum_duration;
        let challenger_wins = scores[sampled.index()] > scores[current.index()] + 0.24;
        if sampled == current
            || (!minimum_done && !direct_interaction && !threat)
            || (!maximum_done && !direct_interaction && !threat && !challenger_wins)
        {
            return false;
        }

        self.state.action_cooldowns[current.index()] =
            current_definition.cooldown * (1.0 + self.state.ignored_attempts.min(5) as f32 * 0.18);
        if self.state.recent_actions.len() == RECENT_ACTION_CAPACITY {
            self.state.recent_actions.pop_front();
        }
        self.state.recent_actions.push_back(current);
        self.state.current_action = sampled;
        self.state.action_elapsed_seconds = 0.0;
        let definition = sampled.definition();
        let user_initiated = direct_interaction;
        if !user_initiated
            && !self
                .state
                .attention_budget
                .spend(definition.unsolicited_attention_cost)
        {
            self.state.current_action = ActionId::SelfPlay;
        }
        if self.state.current_action.is_attention_strategy() && !user_initiated {
            self.state.pending_attention = Some(PendingAttention {
                action: self.state.current_action,
                context: *context,
                elapsed_seconds: 0.0,
                response_window_seconds: 5.0 + self.state.genome.temperament.persistence * 5.0,
            });
        }
        true
    }

    fn select_vocal_request(
        &mut self,
        trigger: VocalTrigger,
        sensors: &SensorFrame,
        context: &ContextVector,
    ) -> Option<VocalRequest> {
        if !trigger.is_supported()
            || self.state.vocal_motifs.is_empty()
            || self.state.pending_vocal_delivery.is_some()
        {
            return None;
        }
        let attachment = self.state.affect.attachment;
        let families: Vec<_> = self
            .state
            .vocal_motifs
            .iter()
            .map(|motif| motif_family_id(&self.state.vocal_motifs, motif.id))
            .collect();
        let recent_exact: Vec<_> = self
            .state
            .recent_vocalizations
            .iter()
            .rev()
            .take(EXACT_VOCAL_REPEAT_WINDOW)
            .map(|recent| recent.motif_id)
            .collect();
        let last_family = self
            .state
            .recent_vocalizations
            .back()
            .map(|recent| recent.family_id);
        let has_alternative_family = last_family.is_some_and(|last| {
            self.state
                .vocal_motifs
                .iter()
                .zip(&families)
                .any(|(motif, family)| *family != last && !recent_exact.contains(&motif.id))
        });
        let mean_use = self
            .state
            .vocal_motifs
            .iter()
            .map(|motif| motif.use_count as f32)
            .sum::<f32>()
            / self.state.vocal_motifs.len().max(1) as f32;
        let mut scores = [f32::NEG_INFINITY; 24];
        for (index, motif) in self.state.vocal_motifs.iter().take(24).enumerate() {
            if recent_exact.contains(&motif.id)
                || (has_alternative_family && Some(families[index]) == last_family)
            {
                scores[index] = -100.0;
                continue;
            }
            let contextual = motif_context_prediction(motif, context);
            let family_recency = self
                .state
                .recent_vocalizations
                .iter()
                .rev()
                .enumerate()
                .filter(|(_, recent)| recent.family_id == families[index])
                .map(|(age, _)| 0.32 / (age + 1) as f32)
                .sum::<f32>();
            let overuse = ((motif.use_count as f32 - mean_use) / (mean_use + 4.0)).max(0.0);
            scores[index] = motif.expected_reward * (0.34 + attachment * 0.22)
                + motif.novelty * (0.22 + (1.0 - attachment) * 0.12)
                + contextual * 0.38
                + motif_style_score(motif, trigger, self.state.affect) * 0.72
                - family_recency
                - overuse * 0.12;
        }
        let chosen = sample_index_softmax(
            &scores[..self.state.vocal_motifs.len().min(24)],
            0.34 + (1.0 - attachment) * 0.32,
            &mut self.rng,
        );
        let performance_seed = self.rng.next_u64();
        // Pitch is identity-sensitive, so it only drifts subtly. Loudness and
        // phrase length may vary much more: those are the clearest organic
        // differences between two utterances of the same learned call.
        let performance_pitch = 1.0 + (random_unit(&mut self.rng) * 2.0 - 1.0) * 0.040;
        let gain_shape = (random_unit(&mut self.rng) + random_unit(&mut self.rng)) * 0.5;
        let duration_shape = (random_unit(&mut self.rng) + random_unit(&mut self.rng)) * 0.5;
        let performance_gain = 0.58 + gain_shape * (1.04 - 0.58);
        let performance_duration = 0.72 + duration_shape * (1.45 - 0.72);
        let performance_tempo = performance_duration.recip();
        let family_id = families[chosen];
        let motif_id = self.state.vocal_motifs[chosen].id;
        let response_window_seconds = if trigger.expects_response() {
            4.5 + self.state.genome.temperament.persistence * 4.5
        } else {
            3.0
        };
        self.state.pending_vocal_delivery = Some(PendingVocalDelivery {
            request_id: performance_seed,
            motif_id,
            family_id,
            context: *context,
            response_window_seconds,
            penalize_if_ignored: trigger.expects_response(),
        });
        let affect = self.state.affect;
        let fatigue = self.state.drives.sleep;
        let (style_pitch, style_tempo, style_gain, purr) = match trigger {
            VocalTrigger::Action(ActionId::Purr) => (0.86, 0.82, 0.74, true),
            VocalTrigger::Action(ActionId::MimicClickRhythm) => (1.02, 1.12, 0.90, false),
            VocalTrigger::Action(ActionId::Chirp) => (1.08, 1.06, 1.0, false),
            VocalTrigger::Action(_) => (1.0, 1.0, 0.88, false),
            VocalTrigger::Touch => (0.98, 1.02, 0.90, false),
            VocalTrigger::ToyOffer => (1.05, 0.96, 0.85, false),
            VocalTrigger::CatchSuccess => (1.12, 1.18, 0.94, false),
            VocalTrigger::MissAndRetry => (0.94, 0.92, 0.78, false),
            VocalTrigger::NeedHelp => (1.08, 0.88, 0.88, false),
            VocalTrigger::FoodInspect => (0.98, 0.92, 0.74, false),
            VocalTrigger::FoodAccepted => (1.06, 1.05, 0.82, false),
            VocalTrigger::FoodRefused => (0.90, 0.86, 0.68, false),
            VocalTrigger::HomeReturn => (0.88, 0.78, 0.65, true),
            VocalTrigger::SkillMastered => (1.15, 1.14, 0.92, false),
            VocalTrigger::RhythmEcho => (1.02, 1.0, 0.90, false),
        };
        Some(VocalRequest {
            motif_id,
            performance_seed,
            gain: self.state.genome.voice.maximum_loudness
                * style_gain
                * (1.0 - affect.stress * 0.45)
                * (1.0 - fatigue * 0.25)
                * performance_gain,
            pan: (sensors.cursor_position.x * 2.0 - 1.0).clamp(-0.8, 0.8),
            pitch_scale: (style_pitch
                * (1.0 + affect.arousal * 0.12 - fatigue * 0.10)
                * performance_pitch)
                .clamp(0.70, 1.38),
            tempo_scale: (style_tempo
                * (1.0 + affect.arousal * 0.18 - fatigue * 0.20)
                * performance_tempo)
                .clamp(0.62, 1.48),
            stress: affect.stress,
            purr,
            rhythm_intervals: if trigger == VocalTrigger::RhythmEcho {
                sensors.recent_click_rhythm.map(|interval| {
                    if interval.is_finite() && interval > 0.0 {
                        interval.clamp(0.1, 4.0)
                    } else {
                        0.0
                    }
                })
            } else {
                [0.0; 8]
            },
        })
    }

    fn resolve_expired_attention(&mut self, dt: f32) {
        let Some(pending) = &mut self.state.pending_attention else {
            return;
        };
        pending.elapsed_seconds += dt;
        if pending.elapsed_seconds < pending.response_window_seconds {
            return;
        }
        let pending = self.state.pending_attention.take().expect("pending exists");
        self.habits.update(pending.action, &pending.context, -0.25);
        self.brain.apply_reward(-0.25);
        self.state.ignored_attempts = self
            .state
            .ignored_attempts
            .saturating_add(1)
            .min(MAX_IGNORED_ATTEMPTS);
        self.state.recent_reward = -0.25;
        self.state.action_cooldowns[pending.action.index()] += pending.action.definition().cooldown
            * (0.5 + self.state.ignored_attempts.min(5) as f32 * 0.25);
        self.memories.record(EventRecord {
            timestamp: self.state.elapsed_seconds,
            context: pending.context,
            action: pending.action,
            outcome: Outcome::Ignored,
            reward: -0.25,
            salience: 0.35,
        });
    }

    fn resolve_expired_vocal_credit(&mut self, dt: f32) {
        let Some(pending) = &mut self.state.pending_vocal_credit else {
            return;
        };
        pending.elapsed_seconds += dt;
        if pending.elapsed_seconds < pending.response_window_seconds {
            return;
        }
        let expired = self
            .state
            .pending_vocal_credit
            .take()
            .expect("pending vocal credit exists");
        if expired.penalize_if_ignored {
            self.update_vocal_motif_from_credit(&expired, -0.18);
        }
    }

    fn apply_vocal_feedback(&mut self, reward: f32) {
        if reward.abs() <= f32::EPSILON {
            return;
        }
        let Some(pending) = self.state.pending_vocal_credit.take() else {
            return;
        };
        self.update_vocal_motif_from_credit(&pending, reward);
    }

    fn update_vocal_motif_from_credit(&mut self, credit: &PendingVocalCredit, reward: f32) {
        if let Some(motif) = self
            .state
            .vocal_motifs
            .iter_mut()
            .find(|motif| motif.id == credit.motif_id)
        {
            let reward = reward.clamp(-1.0, 1.0);
            motif.expected_reward += (reward - motif.expected_reward) * 0.10;
            motif.expected_reward = motif.expected_reward.clamp(-1.0, 1.0);
            let prediction = motif_context_prediction(motif, &credit.context);
            let error = reward - prediction;
            let learning_rate =
                0.018 + self.state.genome.temperament.adaptability.clamp(0.0, 1.0) * 0.032;
            for (weight, value) in motif.context_weights.iter_mut().zip(&credit.context) {
                *weight = (*weight + learning_rate * error * value)
                    .clamp(-VOCAL_CONTEXT_WEIGHT_LIMIT, VOCAL_CONTEXT_WEIGHT_LIMIT);
            }
            if reward > 0.2 {
                motif.success_count = motif.success_count.saturating_add(1);
                motif.novelty = (motif.novelty + 0.06).min(1.0);
            }
        }
    }

    fn repair_vocal_repertoire(&mut self) {
        let canonical = generate_initial_motifs(&self.state.genome.voice);
        for motif in canonical {
            let family_present =
                self.state.vocal_motifs.iter().any(|existing| {
                    motif_family_id(&self.state.vocal_motifs, existing.id) == motif.id
                });
            if !family_present
                && self
                    .state
                    .vocal_motifs
                    .iter()
                    .all(|existing| existing.id != motif.id)
            {
                self.state.vocal_motifs.push(motif);
            }
        }

        let mut family_counts = std::collections::HashMap::<u64, usize>::new();
        for motif in &self.state.vocal_motifs {
            *family_counts
                .entry(motif_family_id(&self.state.vocal_motifs, motif.id))
                .or_default() += 1;
        }
        let needs_prune = self.state.vocal_motifs.len() > 24
            || family_counts
                .values()
                .any(|count| *count > MAX_VOCAL_FAMILY_SIZE);
        if !needs_prune {
            return;
        }

        let all = self.state.vocal_motifs.clone();
        let mut ranked = all.clone();
        ranked.sort_by(|left, right| motif_value(right).total_cmp(&motif_value(left)));
        let mut kept = Vec::with_capacity(24);
        let mut kept_ids = std::collections::HashSet::new();
        let mut kept_families = std::collections::HashMap::<u64, usize>::new();

        // First reserve the strongest member of every extant voice family.
        for motif in &ranked {
            let family = motif_family_id(&all, motif.id);
            if kept_families.contains_key(&family) || kept.len() == 24 {
                continue;
            }
            kept.push(motif.clone());
            kept_ids.insert(motif.id);
            kept_families.insert(family, 1);
        }
        // Then spend the remaining capacity on successful variants, with a hard
        // family cap so one rewarded parent can never erase the whole vocabulary.
        for motif in ranked {
            if kept.len() == 24 || kept_ids.contains(&motif.id) {
                continue;
            }
            let family = motif_family_id(&all, motif.id);
            let count = kept_families.entry(family).or_default();
            if *count >= MAX_VOCAL_FAMILY_SIZE {
                continue;
            }
            *count += 1;
            kept_ids.insert(motif.id);
            kept.push(motif);
        }
        self.state.vocal_motifs = kept;
        if self
            .state
            .selected_motif_id
            .is_some_and(|id| !kept_ids.contains(&id))
        {
            self.state.selected_motif_id = None;
        }
        if self
            .state
            .pending_vocal_credit
            .as_ref()
            .is_some_and(|pending| !kept_ids.contains(&pending.motif_id))
        {
            self.state.pending_vocal_credit = None;
        }
        if self
            .state
            .pending_vocal_delivery
            .as_ref()
            .is_some_and(|pending| !kept_ids.contains(&pending.motif_id))
        {
            self.state.pending_vocal_delivery = None;
        }
    }

    fn update_lifetime_statistics(&mut self, sensors: &SensorFrame, body: &BodyFeedback, dt: f32) {
        let stats = &mut self.state.development.lifetime;
        stats.ticks_alive = stats.ticks_alive.saturating_add(1);
        if matches!(sensors.day_phase, DayPhase::Night) && sensors.user_activity_rate > 0.1 {
            stats.night_activity += 1.0;
        }
        if self.state.current_action.is_vocal() {
            stats.vocal_interactions += 1.0;
        }
        if self.state.current_action == ActionId::PlayCursorChase {
            stats.cursor_chases += 1.0;
        }
        if self.state.focus_mode {
            stats.focus_seconds += dt;
        }
        if self.state.current_action == ActionId::MimicClickRhythm {
            stats.rhythmic_interactions += 1.0;
        }
        if body.locomotion_completed && matches!(self.state.current_action, ActionId::LandOnWindow)
        {
            stats.landings += 1.0;
        }
        stats.ignored_attempts = self.state.ignored_attempts as f32;
    }
}

#[must_use]
pub fn stable_hash_bytes(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

fn build_brain_inputs(
    state: &LifeState,
    sensors: &SensorFrame,
    body: &BodyFeedback,
) -> [f32; SENSOR_INPUT_COUNT] {
    let cursor_delta = sensors.cursor_position - body.world_position;
    [
        cursor_delta.x.clamp(-1.0, 1.0),
        cursor_delta.y.clamp(-1.0, 1.0),
        (sensors.cursor_velocity.length() * 0.25).clamp(0.0, 1.0),
        sensors.cursor_approach_speed.clamp(-1.0, 1.0),
        (1.0 - sensors.cursor_distance_to_pet).clamp(0.0, 1.0),
        bool_value(sensors.pet_hovered),
        bool_value(sensors.pet_touched || body.cursor_contact),
        (sensors.user_idle_seconds / 600.0).clamp(0.0, 1.0),
        sensors.user_activity_rate.clamp(0.0, 1.0),
        sensors.time_of_day_01 * 2.0 - 1.0,
        sensors.audio_rms.unwrap_or(0.0).clamp(0.0, 1.0),
        sensors.voice_activity.unwrap_or(0.0).clamp(0.0, 1.0),
        body.velocity.x.clamp(-1.0, 1.0),
        body.velocity.y.clamp(-1.0, 1.0),
        body.pose_error.clamp(0.0, 1.0),
        bool_value(body.grounded || body.clinging),
        state.drives.sleep,
        state.drives.social,
        state.drives.play,
        state.drives.curiosity,
        state.drives.comfort,
        state.drives.safety,
        state.drives.autonomy,
        state.drives.novelty,
        state.affect.valence,
        state.affect.arousal,
        state.affect.stress,
        state.affect.confidence,
        state.affect.attachment,
        state.affect.frustration,
        sensors.user_presence.unwrap_or(0.5).clamp(0.0, 1.0),
        sensors.user_availability.unwrap_or(0.5).clamp(0.0, 1.0),
    ]
}

fn conditions_met(
    conditions: ActionConditions,
    state: &LifeState,
    sensors: &SensorFrame,
    body: &BodyFeedback,
) -> bool {
    if state.focus_mode && !conditions.allowed_in_focus_mode {
        return false;
    }
    if conditions.needs_cursor && !sensors.cursor_position.is_finite() {
        return false;
    }
    if conditions.needs_cursor_proximity && sensors.cursor_distance_to_pet > 0.28 {
        return false;
    }
    if conditions.needs_cursor_engagement && !cursor_engaged(sensors) {
        return false;
    }
    if conditions.needs_user_available
        && sensors.user_availability.unwrap_or({
            if sensors.user_idle_seconds < 120.0 {
                1.0
            } else {
                0.0
            }
        }) < 0.15
    {
        return false;
    }
    if conditions.needs_surface && sensors.visible_surfaces.is_empty() {
        return false;
    }
    if conditions.threat_only
        && state.drives.safety < 0.35
        && body
            .collision
            .as_ref()
            .map_or(0.0, |collision| collision.intensity)
            < 0.25
    {
        return false;
    }
    if state.drives.sleep < conditions.sleep_drive_minimum {
        return false;
    }
    if state.genome.generation < conditions.generation_minimum {
        return false;
    }
    true
}

fn cursor_engaged(sensors: &SensorFrame) -> bool {
    let close_contact = sensors.cursor_distance_to_pet < 0.12
        && (sensors.pointer_down || sensors.pointer_pressed || sensors.pet_hovered);
    close_contact || sensors.pet_touched || sensors.pet_dragged
}

fn temperament_bias(action: ActionId, state: &LifeState) -> f32 {
    let temperament = &state.genome.temperament;
    match action {
        ActionId::ApproachCursor | ActionId::InvitePetting | ActionId::Purr => {
            temperament.sociability * 0.32 + state.affect.attachment * 0.24
        }
        ActionId::RetreatFromCursor | ActionId::FrustratedRetreat => {
            (1.0 - temperament.boldness) * 0.30 + state.drives.safety * 0.45
        }
        ActionId::ExploreScreen | ActionId::PeekFromEdge | ActionId::ObserveCursor => {
            temperament.curiosity * 0.28 + temperament.exploration_rate * 0.24
        }
        ActionId::InviteCursorChase
        | ActionId::PlayCursorChase
        | ActionId::BringProceduralOrb
        | ActionId::HideAndSeek
        | ActionId::SelfPlay => temperament.playfulness * 0.34,
        ActionId::Chirp | ActionId::MimicClickRhythm => temperament.vocality * 0.35,
        ActionId::SilentStare => temperament.patience * 0.26,
        ActionId::Sleep => state.drives.sleep * 0.42,
        _ => 0.0,
    }
}

fn action_temperature(state: &LifeState) -> f32 {
    let repetition = state
        .recent_actions
        .iter()
        .filter(|action| **action == state.current_action)
        .count() as f32
        / RECENT_ACTION_CAPACITY as f32;
    (0.28
        + state.drives.curiosity * 0.28
        + state.genome.temperament.exploration_rate * 0.30
        + (1.0 - state.affect.confidence) * 0.12
        + repetition * 0.18
        - state.affect.stress * 0.15)
        .clamp(0.16, 1.15)
}

fn sample_softmax(
    scores: &[f32; ACTION_COUNT],
    temperature: f32,
    rng: &mut impl RngCore,
) -> ActionId {
    let index = sample_index_softmax(scores, temperature, rng);
    ActionId::ALL[index]
}

fn sample_index_softmax(scores: &[f32], temperature: f32, rng: &mut impl RngCore) -> usize {
    let maximum = scores.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let mut weights = [0.0_f32; 24];
    let mut total = 0.0;
    for (index, score) in scores.iter().enumerate() {
        let weight = ((*score - maximum) / temperature.max(0.05))
            .clamp(-40.0, 0.0)
            .exp();
        weights[index] = if weight.is_finite() { weight } else { 0.0 };
        total += weights[index];
    }
    if total <= f32::EPSILON {
        return 0;
    }
    let mut cursor = random_unit(rng) * total;
    for (index, weight) in weights[..scores.len()].iter().enumerate() {
        cursor -= *weight;
        if cursor <= 0.0 {
            return index;
        }
    }
    scores.len().saturating_sub(1)
}

fn body_intent_for(
    action: ActionId,
    sensors: &SensorFrame,
    body: &BodyFeedback,
    expression: ExpressionState,
) -> BodyIntent {
    let surface = sensors.visible_surfaces.first();
    let (locomotion, target_position, pose, interaction_target, speed) = match action {
        ActionId::ApproachCursor => (
            LocomotionMode::Arrive,
            sensors.cursor_position,
            PoseIntent::Curious,
            Some(InteractionTarget::Cursor),
            0.14,
        ),
        ActionId::InvitePetting => (
            LocomotionMode::Hover,
            body.world_position,
            PoseIntent::Curious,
            Some(InteractionTarget::User),
            0.06,
        ),
        ActionId::RetreatFromCursor | ActionId::FrustratedRetreat => {
            let away = (body.world_position - sensors.cursor_position).normalize_or_zero();
            (
                LocomotionMode::Flee,
                (body.world_position + away * 0.35).clamp(Vec2::splat(0.05), Vec2::splat(0.95)),
                PoseIntent::Compact,
                None,
                0.58,
            )
        }
        ActionId::InviteCursorChase => (
            LocomotionMode::Hover,
            body.world_position,
            PoseIntent::Playful,
            Some(InteractionTarget::Cursor),
            0.06,
        ),
        ActionId::PlayCursorChase => (
            LocomotionMode::Orbit,
            sensors.cursor_position,
            PoseIntent::Playful,
            Some(InteractionTarget::Cursor),
            0.20,
        ),
        ActionId::LandOnWindow => (
            LocomotionMode::Landing,
            surface.map_or(body.world_position, |surface| {
                Vec2::new(
                    (surface.rect.minimum.x + surface.rect.maximum.x) * 0.5,
                    surface.rect.minimum.y,
                )
            }),
            PoseIntent::Landing,
            surface.map(|surface| InteractionTarget::Surface(surface.id.clone())),
            0.40,
        ),
        ActionId::ClingToWindowSide | ActionId::PeekFromEdge => (
            LocomotionMode::EdgeCling,
            surface.map_or(Vec2::new(0.05, body.world_position.y), |surface| {
                Vec2::new(
                    surface.rect.minimum.x,
                    (surface.rect.minimum.y + surface.rect.maximum.y) * 0.5,
                )
            }),
            PoseIntent::Clinging,
            surface.map(|surface| InteractionTarget::Surface(surface.id.clone())),
            0.32,
        ),
        ActionId::Sleep => (
            LocomotionMode::Sleep,
            body.world_position,
            PoseIntent::Sleeping,
            None,
            0.0,
        ),
        ActionId::Metamorphosis => (
            LocomotionMode::Cocoon,
            body.world_position,
            PoseIntent::Cocoon,
            None,
            0.0,
        ),
        ActionId::SelfPlay => (
            LocomotionMode::Wander,
            deterministic_wander_target(action, sensors.timestamp),
            PoseIntent::Playful,
            None,
            0.08,
        ),
        ActionId::ExploreScreen | ActionId::HideAndSeek => (
            LocomotionMode::Wander,
            deterministic_wander_target(action, sensors.timestamp),
            PoseIntent::Curious,
            None,
            0.11,
        ),
        ActionId::BringProceduralOrb => (
            LocomotionMode::Hover,
            body.world_position,
            PoseIntent::Playful,
            Some(InteractionTarget::ProceduralOrb),
            0.06,
        ),
        ActionId::HappyDisplay => (
            LocomotionMode::Hover,
            body.world_position,
            PoseIntent::Display,
            Some(InteractionTarget::User),
            0.12,
        ),
        _ => (
            LocomotionMode::Hover,
            body.world_position,
            PoseIntent::Neutral,
            None,
            0.12,
        ),
    };
    let direction = (target_position - body.world_position).x.signum();
    BodyIntent {
        locomotion,
        target_position: target_position.clamp(Vec2::ZERO, Vec2::ONE),
        target_surface: surface.map(|surface| surface.id.clone()),
        desired_speed: speed,
        facing_direction: if direction == 0.0 { 1.0 } else { direction },
        gaze_target: Some(sensors.cursor_position.clamp(Vec2::ZERO, Vec2::ONE)),
        pose,
        expression,
        interaction_target,
    }
}

fn deterministic_wander_target(action: ActionId, timestamp: f64) -> Vec2 {
    // Exploration is a behavior bout, not a continuously moving waypoint. Holding a
    // target gives the body time to arrive, observe, and visibly choose again instead
    // of tracing a screen-wide Lissajous curve from corner to corner.
    let bout = (timestamp.max(0.0) / 6.0).floor() as f32;
    let phase = (bout * 1.987 + action.index() as f32 * 1.618).rem_euclid(std::f32::consts::TAU);
    Vec2::new(
        0.5 + phase.sin() * 0.26,
        0.52 + (phase * 0.73 + 1.1).cos() * 0.20,
    )
}

fn repetition_ratio(actions: &VecDeque<ActionId>, action: ActionId) -> f32 {
    if actions.is_empty() {
        return 0.0;
    }
    actions.iter().filter(|entry| **entry == action).count() as f32 / actions.len() as f32
}

fn outcome_for_reward(reward: f32) -> Outcome {
    if reward >= 0.3 {
        Outcome::Success
    } else if reward <= -0.45 {
        Outcome::Rejected
    } else if reward < 0.0 {
        Outcome::Ignored
    } else {
        Outcome::Neutral
    }
}

fn salience(reward: f32, attachment: f32, outcome: Outcome) -> f32 {
    (reward.abs() * 0.62
        + attachment * 0.12
        + if matches!(outcome, Outcome::Rejected) {
            0.22
        } else {
            0.05
        })
    .clamp(0.0, 1.0)
}

fn motif_family_id(motifs: &[VocalMotif], motif_id: u64) -> u64 {
    let mut current = motif_id;
    for _ in 0..=motifs.len() {
        let Some(motif) = motifs.iter().find(|motif| motif.id == current) else {
            return current;
        };
        let Some(parent) = motif.parent_id else {
            return current;
        };
        if parent == current {
            return current;
        }
        current = parent;
    }
    current
}

fn motif_context_prediction(motif: &VocalMotif, context: &ContextVector) -> f32 {
    motif
        .context_weights
        .iter()
        .zip(context)
        .map(|(weight, value)| weight * value)
        .sum::<f32>()
        .clamp(-1.0, 1.0)
}

fn motif_style_score(motif: &VocalMotif, trigger: VocalTrigger, affect: AffectState) -> f32 {
    let count = motif.syllables.len().max(1) as f32;
    let mean =
        |sample: fn(&Syllable) -> f32| motif.syllables.iter().map(sample).sum::<f32>() / count;
    let signature = [
        ((mean(|syllable| syllable.pitch_peak) - 0.45) / 1.75).clamp(0.0, 1.0),
        (mean(|syllable| syllable.duration_ms) / 520.0).clamp(0.0, 1.0),
        (mean(|syllable| syllable.gap_after_ms) / 280.0).clamp(0.0, 1.0),
        mean(|syllable| syllable.click).clamp(0.0, 1.0),
        mean(|syllable| syllable.noisiness).clamp(0.0, 1.0),
        ((motif.syllables.len() as f32 - 1.0) / 5.0).clamp(0.0, 1.0),
        (0.5 + mean(|syllable| syllable.pitch_end - syllable.pitch_start) * 0.35).clamp(0.0, 1.0),
    ];
    let mut target = match trigger {
        VocalTrigger::Action(ActionId::Purr) => [0.20, 0.72, 0.18, 0.05, 0.16, 0.78, 0.42],
        VocalTrigger::Action(ActionId::MimicClickRhythm) => {
            [0.55, 0.30, 0.56, 0.82, 0.30, 0.68, 0.50]
        }
        VocalTrigger::Action(ActionId::Chirp) => [0.72, 0.28, 0.20, 0.20, 0.24, 0.38, 0.74],
        VocalTrigger::Action(_) => [0.50; 7],
        VocalTrigger::Touch => [0.44, 0.34, 0.16, 0.12, 0.18, 0.34, 0.62],
        VocalTrigger::ToyOffer => [0.64, 0.34, 0.30, 0.18, 0.18, 0.52, 0.68],
        VocalTrigger::CatchSuccess => [0.78, 0.24, 0.18, 0.22, 0.18, 0.42, 0.78],
        VocalTrigger::MissAndRetry => [0.46, 0.42, 0.34, 0.16, 0.28, 0.46, 0.42],
        VocalTrigger::NeedHelp => [0.70, 0.46, 0.42, 0.12, 0.26, 0.54, 0.74],
        VocalTrigger::FoodInspect => [0.52, 0.34, 0.26, 0.12, 0.18, 0.38, 0.58],
        VocalTrigger::FoodAccepted => [0.62, 0.30, 0.20, 0.14, 0.14, 0.44, 0.70],
        VocalTrigger::FoodRefused => [0.34, 0.44, 0.32, 0.08, 0.24, 0.34, 0.32],
        VocalTrigger::HomeReturn => [0.24, 0.62, 0.22, 0.05, 0.12, 0.70, 0.40],
        VocalTrigger::SkillMastered => [0.82, 0.26, 0.20, 0.24, 0.18, 0.56, 0.82],
        VocalTrigger::RhythmEcho => [0.55, 0.30, 0.56, 0.82, 0.30, 0.68, 0.50],
    };
    target[0] = (target[0] + affect.arousal * 0.10).clamp(0.0, 1.0);
    target[4] = (target[4] + affect.stress * 0.28).clamp(0.0, 1.0);
    target[6] = (target[6] + affect.valence * 0.16).clamp(0.0, 1.0);
    let weights = [1.0, 0.82, 0.48, 0.72, 0.62, 0.42, 0.68];
    let weighted_distance = signature
        .iter()
        .zip(target)
        .zip(weights)
        .map(|((value, target), weight)| (value - target).powi(2) * weight)
        .sum::<f32>()
        / weights.iter().sum::<f32>();
    (1.0 - weighted_distance.sqrt() * 1.55).clamp(-1.0, 1.0)
}

fn motif_value(motif: &VocalMotif) -> f32 {
    let credited_successes = motif.success_count.min(motif.use_count) as f32;
    motif.expected_reward * 0.7 + motif.novelty * 0.2 + credited_successes * 0.01
}

fn motif_distance(left: &VocalMotif, right: &VocalMotif) -> f32 {
    if left.syllables.len() != right.syllables.len() {
        return 1.0;
    }
    let total = left
        .syllables
        .iter()
        .zip(&right.syllables)
        .map(|(a, b)| {
            (a.duration_ms - b.duration_ms).abs() / 500.0
                + (a.pitch_peak - b.pitch_peak).abs()
                + (a.gap_after_ms - b.gap_after_ms).abs() / 300.0
        })
        .sum::<f32>();
    total / left.syllables.len().max(1) as f32
}

fn finite_dt(dt: f32) -> f32 {
    if dt.is_finite() {
        dt.clamp(0.0, 0.25)
    } else {
        0.0
    }
}

fn random_unit(rng: &mut impl RngCore) -> f32 {
    (f64::from(rng.next_u32()) / f64::from(u32::MAX)) as f32
}

fn bool_value(value: bool) -> f32 {
    if value { 1.0 } else { 0.0 }
}

fn smooth(current: f32, target: f32, speed: f32, dt: f32) -> f32 {
    current + (target - current) * (1.0 - (-speed * dt).exp())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_ticks(core: &mut LifeCore, count: usize, sensors: &SensorFrame) -> Vec<ActionId> {
        let body = BodyFeedback::default();
        (0..count)
            .map(|_| core.tick(sensors, &body, 1.0 / LIFECORE_HZ).selected_action)
            .collect()
    }

    #[test]
    fn same_seed_and_inputs_are_deterministic() {
        let genome = Genome::from_seed(42);
        let mut first = LifeCore::new(genome.clone(), 99);
        let mut second = LifeCore::new(genome, 99);
        let sensors = SensorFrame::default();
        assert_eq!(
            run_ticks(&mut first, 2_000, &sensors),
            run_ticks(&mut second, 2_000, &sensors)
        );
        assert_eq!(first.snapshot(), second.snapshot());
    }

    #[test]
    fn drives_affect_and_plasticity_stay_bounded() {
        let mut core = LifeCore::new(Genome::from_seed(3), 7);
        let mut sensors = SensorFrame {
            cursor_approach_speed: 1.0,
            cursor_distance_to_pet: 0.02,
            ..SensorFrame::default()
        };
        let body = BodyFeedback::default();
        for tick in 0..50_000 {
            sensors.timestamp = tick as f64 / f64::from(LIFECORE_HZ);
            let output = core.tick(&sensors, &body, 1.0 / LIFECORE_HZ);
            assert!(output.affect.is_finite());
            assert!(core.state.drives.is_finite());
            if tick % 17 == 0 {
                core.apply_feedback(FeedbackEvent::Reward(if tick % 34 == 0 {
                    1.0
                } else {
                    -1.0
                }));
            }
        }
        assert!(core.brain.is_valid());
    }

    #[test]
    fn hysteresis_prevents_rapid_switching() {
        let mut core = LifeCore::new(Genome::from_seed(4), 8);
        let actions = run_ticks(&mut core, 400, &SensorFrame::default());
        let switches = actions.windows(2).filter(|pair| pair[0] != pair[1]).count();
        assert!(switches < 40, "unexpectedly high switch count: {switches}");
    }

    #[test]
    fn exploration_target_is_stable_and_center_bounded_for_each_bout() {
        let first = deterministic_wander_target(ActionId::ExploreScreen, 12.1);
        let same_bout = deterministic_wander_target(ActionId::ExploreScreen, 17.9);
        let next_bout = deterministic_wander_target(ActionId::ExploreScreen, 18.1);
        assert_eq!(first, same_bout);
        assert_ne!(first, next_bout);
        for target in [first, next_bout] {
            assert!((0.24..=0.76).contains(&target.x));
            assert!((0.32..=0.72).contains(&target.y));
        }
    }

    #[test]
    fn self_play_is_a_bounded_autonomous_movement_bout() {
        let intent = body_intent_for(
            ActionId::SelfPlay,
            &SensorFrame {
                timestamp: 12.1,
                ..SensorFrame::default()
            },
            &BodyFeedback::default(),
            ExpressionState::default(),
        );
        assert_eq!(intent.locomotion, LocomotionMode::Wander);
        assert!(intent.desired_speed <= 0.08);
        assert!((0.24..=0.76).contains(&intent.target_position.x));
        assert!((0.32..=0.72).contains(&intent.target_position.y));
        assert_eq!(intent.interaction_target, None);
    }

    #[test]
    fn expired_current_action_is_removed_from_candidate_scores() {
        let mut core = LifeCore::new(Genome::from_seed(44), 44);
        core.state.current_action = ActionId::IdleHover;
        core.state.action_elapsed_seconds = ActionId::IdleHover.definition().maximum_duration;
        let scores = core.score_actions(
            &SensorFrame::default(),
            &BodyFeedback::default(),
            &[0.0; CONTEXT_SIZE],
            &[0.0; ACTION_COUNT],
        );
        assert_eq!(scores[ActionId::IdleHover.index()], -100.0);
    }

    #[test]
    fn captured_material_drag_cannot_replace_navigation_action_mid_press() {
        let mut core = LifeCore::new(Genome::from_seed(46), 46);
        core.state.current_action = ActionId::SelfPlay;
        core.state.action_elapsed_seconds = ActionId::SelfPlay.definition().maximum_duration + 1.0;
        let mut scores = [0.0; ACTION_COUNT];
        scores[ActionId::WakeUp.index()] = 10.0;
        let sensors = SensorFrame {
            pointer_down: true,
            pointer_pressed: true,
            pet_touched: true,
            pet_dragged: true,
            ..SensorFrame::default()
        };

        let switched = core.maybe_switch_action(
            ActionId::WakeUp,
            &scores,
            &sensors,
            &BodyFeedback::default(),
            &[0.0; CONTEXT_SIZE],
        );

        assert!(!switched);
        assert_eq!(core.state.current_action, ActionId::SelfPlay);
    }

    #[test]
    fn legacy_ignore_streak_is_repaired_and_stays_bounded() {
        let mut core = LifeCore::new(Genome::from_seed(45), 45);
        core.state.ignored_attempts = 558;
        core.tick(
            &SensorFrame::default(),
            &BodyFeedback::default(),
            1.0 / LIFECORE_HZ,
        );
        assert_eq!(core.state.ignored_attempts, MAX_IGNORED_ATTEMPTS);

        for _ in 0..32 {
            core.apply_feedback(FeedbackEvent::Ignored);
        }
        assert_eq!(core.state.ignored_attempts, MAX_IGNORED_ATTEMPTS);
    }

    #[test]
    fn passive_cursor_does_not_start_cursor_chase() {
        let core = LifeCore::new(Genome::from_seed(41), 41);
        let sensors = SensorFrame {
            cursor_position: Vec2::new(0.8, 0.2),
            cursor_distance_to_pet: 0.42,
            cursor_velocity: Vec2::new(0.9, 0.0),
            ..SensorFrame::default()
        };
        let conditions = ActionId::PlayCursorChase.definition().required_conditions;
        assert!(!conditions_met(
            conditions,
            &core.state,
            &sensors,
            &BodyFeedback::default()
        ));
    }

    #[test]
    fn direct_pet_contact_allows_cursor_chase() {
        let core = LifeCore::new(Genome::from_seed(42), 42);
        let sensors = SensorFrame {
            cursor_distance_to_pet: 0.06,
            pointer_down: true,
            pet_hovered: true,
            ..SensorFrame::default()
        };
        let conditions = ActionId::PlayCursorChase.definition().required_conditions;
        assert!(conditions_met(
            conditions,
            &core.state,
            &sensors,
            &BodyFeedback::default()
        ));
    }

    #[test]
    fn cursor_approach_requires_a_local_affordance() {
        let core = LifeCore::new(Genome::from_seed(43), 43);
        let conditions = ActionId::ApproachCursor.definition().required_conditions;
        let distant = SensorFrame {
            cursor_distance_to_pet: 0.65,
            ..SensorFrame::default()
        };
        let nearby = SensorFrame {
            cursor_distance_to_pet: 0.18,
            ..SensorFrame::default()
        };
        let body = BodyFeedback::default();
        assert!(!conditions_met(conditions, &core.state, &distant, &body));
        assert!(conditions_met(conditions, &core.state, &nearby, &body));
    }

    #[test]
    fn cursor_chase_invitation_stays_put_until_the_user_engages() {
        let body = BodyFeedback {
            world_position: Vec2::new(0.38, 0.56),
            ..BodyFeedback::default()
        };
        let intent = body_intent_for(
            ActionId::InviteCursorChase,
            &SensorFrame::default(),
            &body,
            ExpressionState::default(),
        );
        assert_eq!(intent.locomotion, LocomotionMode::Hover);
        assert_eq!(intent.target_position, body.world_position);
        assert!(intent.desired_speed <= 0.06);
    }

    #[test]
    fn social_bids_signal_in_place_instead_of_crossing_the_desktop() {
        let body = BodyFeedback {
            world_position: Vec2::new(0.38, 0.56),
            ..BodyFeedback::default()
        };
        for action in [ActionId::InvitePetting, ActionId::BringProceduralOrb] {
            let intent = body_intent_for(
                action,
                &SensorFrame::default(),
                &body,
                ExpressionState::default(),
            );
            assert_eq!(intent.locomotion, LocomotionMode::Hover);
            assert_eq!(intent.target_position, body.world_position);
            assert!(intent.desired_speed <= 0.06);
        }
    }

    #[test]
    fn active_cursor_chase_uses_overlay_safe_speed() {
        let intent = body_intent_for(
            ActionId::PlayCursorChase,
            &SensorFrame::default(),
            &BodyFeedback::default(),
            ExpressionState::default(),
        );
        assert_eq!(intent.locomotion, LocomotionMode::Orbit);
        assert!(intent.desired_speed <= 0.20);
    }

    #[test]
    fn focus_mode_blocks_intrusive_actions() {
        let mut core = LifeCore::new(Genome::from_seed(5), 9);
        core.set_focus_mode(true);
        for action in run_ticks(&mut core, 2_000, &SensorFrame::default()) {
            assert!(action.is_focus_allowed(), "focus mode selected {action:?}");
        }
    }

    #[test]
    fn attention_budget_recovers() {
        let mut budget = AttentionBudget {
            current: 0.0,
            ..AttentionBudget::default()
        };
        for _ in 0..2_000 {
            budget.recover(1.0 / LIFECORE_HZ);
        }
        assert!(budget.current > 0.99);
    }

    #[test]
    fn interaction_voice_has_a_hard_anti_repeat_window() {
        let mut core = LifeCore::new(Genome::from_seed(51), 91);
        let sensors = SensorFrame::default();
        let mut heard = Vec::<(u64, u64)>::new();
        for _ in 0..32 {
            let request = core
                .request_vocalization(VocalTrigger::Touch, &sensors)
                .expect("touch has a voice");
            assert!(core.confirm_vocal_request_heard(request.performance_seed));
            let family = motif_family_id(&core.state.vocal_motifs, request.motif_id);
            assert!(
                !heard
                    .iter()
                    .rev()
                    .take(EXACT_VOCAL_REPEAT_WINDOW)
                    .any(|(motif, _)| *motif == request.motif_id),
                "motif repeated inside anti-repeat window"
            );
            if let Some((_, previous_family)) = heard.last() {
                assert_ne!(
                    family, *previous_family,
                    "voice family repeated back-to-back"
                );
            }
            heard.push((request.motif_id, family));
        }
        let unique = heard
            .iter()
            .map(|(motif, _)| *motif)
            .collect::<std::collections::HashSet<_>>()
            .len();
        assert!(unique >= 4, "only {unique} distinct motifs were heard");
    }

    #[test]
    fn vocal_feedback_is_finite_contextual_and_consumed_once() {
        let mut core = LifeCore::new(Genome::from_seed(52), 92);
        let sensors = SensorFrame {
            user_activity_rate: 0.72,
            cursor_distance_to_pet: 0.08,
            ..SensorFrame::default()
        };
        let request = core
            .request_vocalization(VocalTrigger::Touch, &sensors)
            .expect("touch has a voice");
        assert!(core.confirm_vocal_request_heard(request.performance_seed));
        let credit = core
            .state
            .pending_vocal_credit
            .clone()
            .expect("delivered request has pending credit");
        let before = core
            .state
            .vocal_motifs
            .iter()
            .find(|motif| motif.id == request.motif_id)
            .cloned()
            .expect("selected motif exists");
        let prediction_before = motif_context_prediction(&before, &credit.context);

        core.apply_feedback(FeedbackEvent::Reward(0.8));
        let after_once = core
            .state
            .vocal_motifs
            .iter()
            .find(|motif| motif.id == request.motif_id)
            .cloned()
            .expect("selected motif exists");
        assert!(after_once.expected_reward > before.expected_reward);
        assert!(motif_context_prediction(&after_once, &credit.context) > prediction_before);
        assert_eq!(after_once.success_count, before.success_count + 1);
        assert!(core.state.pending_vocal_credit.is_none());

        core.apply_feedback(FeedbackEvent::Reward(0.8));
        let after_unrelated = core
            .state
            .vocal_motifs
            .iter()
            .find(|motif| motif.id == request.motif_id)
            .expect("selected motif exists");
        assert_eq!(
            after_unrelated, &after_once,
            "stale sound received a second reward"
        );
    }

    #[test]
    fn ignored_attention_voice_gets_one_bounded_negative_update() {
        let mut core = LifeCore::new(Genome::from_seed(53), 93);
        let request = core
            .request_vocalization(
                VocalTrigger::Action(ActionId::Chirp),
                &SensorFrame::default(),
            )
            .expect("chirp has a voice");
        assert!(core.confirm_vocal_request_heard(request.performance_seed));
        let before = core
            .state
            .vocal_motifs
            .iter()
            .find(|motif| motif.id == request.motif_id)
            .map(|motif| motif.expected_reward)
            .unwrap();
        core.resolve_expired_vocal_credit(30.0);
        let after = core
            .state
            .vocal_motifs
            .iter()
            .find(|motif| motif.id == request.motif_id)
            .map(|motif| motif.expected_reward)
            .unwrap();
        assert!(after < before);
        assert!(after >= -1.0);
        assert!(core.state.pending_vocal_credit.is_none());
    }

    #[test]
    fn failed_audio_delivery_cannot_train_a_motif() {
        let mut core = LifeCore::new(Genome::from_seed(54), 94);
        let request = core
            .request_vocalization(VocalTrigger::Touch, &SensorFrame::default())
            .expect("touch has a voice");
        let before = core
            .state
            .vocal_motifs
            .iter()
            .find(|motif| motif.id == request.motif_id)
            .cloned()
            .unwrap();
        core.cancel_vocal_request(request.performance_seed);
        core.apply_feedback(FeedbackEvent::Reward(1.0));
        let after = core
            .state
            .vocal_motifs
            .iter()
            .find(|motif| motif.id == request.motif_id)
            .unwrap();
        assert_eq!(after, &before);
        assert!(core.state.recent_vocalizations.is_empty());
        assert!(core.state.pending_vocal_delivery.is_none());
    }

    #[test]
    fn queued_vocal_is_reversible_and_requires_exact_heard_ack() {
        let mut core = LifeCore::new(Genome::from_seed(540), 940);
        core.state.vocal_motifs[0].novelty = 0.2;
        let before_motifs = core.state.vocal_motifs.clone();
        let before_recent = core.state.recent_vocalizations.clone();
        let request = core
            .request_vocalization(VocalTrigger::Touch, &SensorFrame::default())
            .expect("touch selects a queued performance");

        assert_eq!(core.state.vocal_motifs, before_motifs);
        assert_eq!(core.state.recent_vocalizations, before_recent);
        assert!(core.state.pending_vocal_credit.is_none());
        assert!(
            core.request_vocalization(VocalTrigger::Touch, &SensorFrame::default())
                .is_none()
        );
        assert!(!core.confirm_vocal_request_heard(request.performance_seed ^ 1));
        core.cancel_vocal_request(request.performance_seed ^ 1);
        assert!(core.state.pending_vocal_delivery.is_some());

        core.cancel_vocal_request(request.performance_seed);
        assert!(core.state.pending_vocal_delivery.is_none());
        assert_eq!(core.state.vocal_motifs, before_motifs);
        assert_eq!(core.state.recent_vocalizations, before_recent);
        assert!(
            core.request_vocalization(VocalTrigger::Touch, &SensorFrame::default())
                .is_some()
        );
    }

    #[test]
    fn rejected_fifth_vocal_preserves_full_anti_repeat_history() {
        let mut core = LifeCore::new(Genome::from_seed(541), 941);
        for _ in 0..RECENT_VOCAL_CAPACITY {
            let request = core
                .request_vocalization(VocalTrigger::Touch, &SensorFrame::default())
                .unwrap();
            assert!(core.confirm_vocal_request_heard(request.performance_seed));
        }
        let before = core.state.recent_vocalizations.clone();
        let rejected = core
            .request_vocalization(VocalTrigger::Touch, &SensorFrame::default())
            .unwrap();
        core.cancel_vocal_request(rejected.performance_seed);
        assert_eq!(core.state.recent_vocalizations, before);
    }

    #[test]
    fn new_audio_session_drops_only_process_owned_vocal_state() {
        let mut core = LifeCore::new(Genome::from_seed(542), 942);
        let heard = core
            .request_vocalization(VocalTrigger::Touch, &SensorFrame::default())
            .unwrap();
        assert!(core.confirm_vocal_request_heard(heard.performance_seed));
        let history = core.state.recent_vocalizations.clone();
        let queued = core
            .request_vocalization(VocalTrigger::Touch, &SensorFrame::default())
            .unwrap();
        assert_eq!(
            core.state
                .pending_vocal_delivery
                .as_ref()
                .map(|pending| pending.request_id),
            Some(queued.performance_seed)
        );
        assert!(core.state.pending_vocal_credit.is_some());

        core.reset_vocal_delivery_session();

        assert!(core.state.pending_vocal_delivery.is_none());
        assert!(core.state.pending_vocal_credit.is_none());
        assert_eq!(core.state.recent_vocalizations, history);
    }

    #[test]
    fn sleep_consolidation_repairs_and_caps_a_monoculture() {
        let mut core = LifeCore::new(Genome::from_seed(55), 95);
        let mut root = core.state.vocal_motifs[0].clone();
        root.expected_reward = 0.9;
        root.use_count = 20;
        root.success_count = 20;
        let mut collapsed = vec![root.clone()];
        for seed in 1..24 {
            let mut child = mutate_motif(&root, seed);
            child.expected_reward = 0.9;
            child.use_count = 20;
            child.success_count = 20;
            collapsed.push(child);
        }
        core.state.vocal_motifs = collapsed;
        core.consolidate_sleep();

        let mut family_counts = std::collections::HashMap::<u64, usize>::new();
        for motif in &core.state.vocal_motifs {
            *family_counts
                .entry(motif_family_id(&core.state.vocal_motifs, motif.id))
                .or_default() += 1;
        }
        assert!(
            family_counts.len() >= 8,
            "canonical voice families were not restored"
        );
        assert!(
            family_counts
                .values()
                .all(|count| *count <= MAX_VOCAL_FAMILY_SIZE)
        );
        assert!(core.state.vocal_motifs.len() <= 24);
    }

    #[test]
    fn old_schema_one_snapshot_defaults_new_vocal_runtime_state() {
        let core = LifeCore::new(Genome::from_seed(56), 96);
        let snapshot = core.snapshot();
        let current_json = serde_json::to_string(&snapshot.state).unwrap();
        let legacy_json = current_json
            .replace("\"recent_vocalizations\":[],", "")
            .replace("\"pending_vocal_credit\":null,", "")
            .replace("\"pending_vocal_delivery\":null,", "");
        assert_ne!(legacy_json, current_json);
        let legacy_state = serde_json::from_str(&legacy_json).unwrap();
        let legacy = LifeSnapshot {
            state: legacy_state,
            ..snapshot
        };
        let restored = LifeCore::restore(legacy).unwrap();
        assert!(restored.state.recent_vocalizations.is_empty());
        assert!(restored.state.pending_vocal_credit.is_none());
        assert!(restored.state.pending_vocal_delivery.is_none());
        assert_eq!(restored.snapshot().schema_version, 1);
    }

    #[test]
    fn vocal_action_styles_have_distinct_bounded_prosody() {
        let genome = Genome::from_seed(57);
        let mut chirp_core = LifeCore::new(genome.clone(), 97);
        let mut purr_core = LifeCore::new(genome, 97);
        let chirp = chirp_core
            .request_vocalization(
                VocalTrigger::Action(ActionId::Chirp),
                &SensorFrame::default(),
            )
            .unwrap();
        let purr = purr_core
            .request_vocalization(
                VocalTrigger::Action(ActionId::Purr),
                &SensorFrame::default(),
            )
            .unwrap();
        assert!(!chirp.purr);
        assert!(purr.purr);
        assert!(purr.pitch_scale + 0.12 < chirp.pitch_scale);
        for value in [
            chirp.pitch_scale,
            chirp.tempo_scale,
            purr.pitch_scale,
            purr.tempo_scale,
        ] {
            assert!(value.is_finite() && (0.6..=1.5).contains(&value));
        }
    }

    #[test]
    fn ecology_semantics_keep_repertoire_ownership_and_copy_only_reduced_rhythm() {
        let mut core = LifeCore::new(Genome::from_seed(571), 971);
        let sensors = SensorFrame {
            recent_click_rhythm: [0.5, 1.0, 0.75, 1.75, 0.0, 0.0, 0.0, 0.0],
            ..SensorFrame::default()
        };
        let request = core
            .request_vocalization(VocalTrigger::RhythmEcho, &sensors)
            .expect("LifeCore selects a learned motif");
        assert!(
            core.state
                .vocal_motifs
                .iter()
                .any(|motif| motif.id == request.motif_id)
        );
        assert_eq!(request.rhythm_intervals, sensors.recent_click_rhythm);
        assert!(request.performance_seed != 0);
    }

    #[test]
    fn repeated_context_has_audible_but_bounded_rendition_variation() {
        let mut core = LifeCore::new(Genome::from_seed(58), 98);
        let sensors = SensorFrame::default();
        let mut minimum_gain = f32::INFINITY;
        let mut maximum_gain = 0.0_f32;
        let mut minimum_tempo = f32::INFINITY;
        let mut maximum_tempo = 0.0_f32;
        let mut seeds = std::collections::HashSet::new();
        for _ in 0..96 {
            let request = core
                .request_vocalization(VocalTrigger::Touch, &sensors)
                .expect("touch has a voice");
            assert!(request.gain.is_finite() && (0.01..=0.5).contains(&request.gain));
            assert!(request.tempo_scale.is_finite() && (0.5..=1.6).contains(&request.tempo_scale));
            minimum_gain = minimum_gain.min(request.gain);
            maximum_gain = maximum_gain.max(request.gain);
            minimum_tempo = minimum_tempo.min(request.tempo_scale);
            maximum_tempo = maximum_tempo.max(request.tempo_scale);
            assert!(seeds.insert(request.performance_seed));
            assert!(core.confirm_vocal_request_heard(request.performance_seed));
        }
        assert!(maximum_gain / minimum_gain >= 1.35);
        assert!(maximum_tempo / minimum_tempo >= 1.35);
    }

    #[test]
    fn snapshot_round_trip_preserves_future_behavior() {
        let mut original = LifeCore::new(Genome::from_seed(7), 11);
        let sensors = SensorFrame::default();
        run_ticks(&mut original, 300, &sensors);
        original.apply_feedback(FeedbackEvent::PettingStarted);
        let json = serde_json::to_vec(&original.snapshot()).expect("snapshot encodes");
        let snapshot = serde_json::from_slice(&json).expect("snapshot decodes");
        let mut restored = LifeCore::restore(snapshot).expect("snapshot restores");
        assert_eq!(
            run_ticks(&mut original, 500, &sensors),
            run_ticks(&mut restored, 500, &sensors)
        );
        assert_eq!(original.snapshot(), restored.snapshot());
    }

    #[test]
    fn long_simulation_changes_needs_sleeps_and_serializes() {
        let mut core = LifeCore::new(Genome::from_seed(8), 12);
        let mut sensors = SensorFrame::default();
        let body = BodyFeedback::default();
        let initial = core.state.drives;
        let simulation_dt = 0.25;
        let ticks = (24.0 * 60.0 * 60.0 / simulation_dt) as usize;
        let mut saw_sleep = false;
        let mut saw_wake = false;
        let mut action_counts = [0_usize; ACTION_COUNT];
        let mut previous = core.state.current_action;
        for tick in 0..ticks {
            sensors.timestamp = tick as f64 * f64::from(simulation_dt);
            sensors.time_of_day_01 = (sensors.timestamp as f32 / 86_400.0).fract();
            sensors.day_phase = if !(0.20..=0.82).contains(&sensors.time_of_day_01) {
                DayPhase::Night
            } else {
                DayPhase::Day
            };
            let output = core.tick(&sensors, &body, simulation_dt);
            action_counts[output.selected_action.index()] += 1;
            saw_sleep |= output.selected_action == ActionId::Sleep;
            saw_wake |= previous == ActionId::Sleep && output.selected_action != ActionId::Sleep;
            if output.selected_action == ActionId::Sleep && tick % 400 == 0 {
                core.consolidate_sleep();
            }
            if tick % 1_200 == 0 {
                core.apply_feedback(FeedbackEvent::PettingStarted);
            } else if tick % 1_801 == 0 {
                core.apply_feedback(FeedbackEvent::Ignored);
            }
            previous = output.selected_action;
        }
        core.consolidate_sleep();
        assert_ne!(initial, core.state.drives);
        assert!(saw_sleep, "pet never slept in 24 virtual hours");
        assert!(saw_wake, "pet never woke in 24 virtual hours");
        let unique_actions = action_counts.iter().filter(|&&count| count > 0).count();
        let dominant_action = action_counts.iter().copied().max().unwrap_or_default();
        assert!(
            unique_actions >= 4,
            "behavior collapsed to {unique_actions} actions"
        );
        assert!(
            dominant_action < ticks * 9 / 10,
            "one action occupied more than 90% of the simulation"
        );
        assert!(serde_json::to_vec(&core.snapshot()).is_ok());
        assert!(core.memories.is_valid());
        assert!(!core.memories.short_term.is_empty());
        assert!(!core.memories.habits.is_empty());
    }
}
