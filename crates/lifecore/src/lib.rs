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

pub const LIFECORE_HZ: f32 = 20.0;
const RECENT_ACTION_CAPACITY: usize = 16;

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
        self.state.tick_count = self.state.tick_count.saturating_add(1);
        self.state.elapsed_seconds += f64::from(dt);
        self.state.action_elapsed_seconds += dt;
        self.state.last_body_feedback = body.clone();
        self.state.attention_budget.recover(dt);
        for cooldown in &mut self.state.action_cooldowns {
            *cooldown = (*cooldown - dt).max(0.0);
        }
        self.resolve_expired_attention(dt);

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
            self.select_vocal_request(sensors, &context)
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
            self.state.ignored_attempts = self.state.ignored_attempts.saturating_add(1);
        }
        if matches!(
            event,
            FeedbackEvent::MuteOrHide | FeedbackEvent::FocusModeEnabled
        ) {
            self.state.attention_budget.current = 0.0;
        }
        self.update_selected_motif_reward(reward);
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
        let mut created = 0;
        let candidates: Vec<_> = self
            .state
            .vocal_motifs
            .iter()
            .filter(|motif| motif.expected_reward > 0.18 && motif.success_count > 0)
            .cloned()
            .collect();
        for parent in candidates {
            if created == 2 || random_unit(&mut self.rng) > 0.42 {
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
                created += 1;
            }
        }
        if self.state.vocal_motifs.len() > 24 {
            self.state
                .vocal_motifs
                .sort_by(|left, right| motif_value(right).total_cmp(&motif_value(left)));
            self.state.vocal_motifs.truncate(24);
        }
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
        Ok(Self {
            state: snapshot.state,
            brain: snapshot.brain,
            habits: snapshot.habits,
            memories: snapshot.memories,
            rng,
            rng_seed: snapshot.rng.seed,
        })
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
        sensors: &SensorFrame,
        context: &ContextVector,
    ) -> Option<VocalRequest> {
        if self.state.vocal_motifs.is_empty() {
            return None;
        }
        let attachment = self.state.affect.attachment;
        let mut scores = [f32::NEG_INFINITY; 24];
        for (index, motif) in self.state.vocal_motifs.iter().take(24).enumerate() {
            let contextual = motif
                .context_weights
                .iter()
                .zip(context)
                .map(|(weight, value)| weight * value)
                .sum::<f32>();
            scores[index] = motif.expected_reward * (0.65 + attachment * 0.55)
                + motif.novelty * (1.0 - attachment) * 0.28
                + contextual * 0.22
                - motif.use_count as f32 * 0.003;
        }
        let chosen = sample_index_softmax(
            &scores[..self.state.vocal_motifs.len().min(24)],
            0.32 + (1.0 - attachment) * 0.42,
            &mut self.rng,
        );
        let motif = &mut self.state.vocal_motifs[chosen];
        motif.use_count = motif.use_count.saturating_add(1);
        motif.novelty *= 0.92;
        self.state.selected_motif_id = Some(motif.id);
        let affect = self.state.affect;
        let fatigue = self.state.drives.sleep;
        Some(VocalRequest {
            motif_id: motif.id,
            gain: self.state.genome.voice.maximum_loudness
                * (1.0 - affect.stress * 0.45)
                * (1.0 - fatigue * 0.25),
            pan: (sensors.cursor_position.x * 2.0 - 1.0).clamp(-0.8, 0.8),
            pitch_scale: (1.0 + affect.arousal * 0.12 - fatigue * 0.10).clamp(0.75, 1.30),
            tempo_scale: (1.0 + affect.arousal * 0.18 - fatigue * 0.20).clamp(0.65, 1.4),
            stress: affect.stress,
            purr: self.state.current_action == ActionId::Purr,
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
        self.state.ignored_attempts = self.state.ignored_attempts.saturating_add(1);
        self.state.recent_reward = -0.25;
        self.state.action_cooldowns[pending.action.index()] += pending.action.definition().cooldown
            * (0.5 + self.state.ignored_attempts as f32 * 0.25);
        self.memories.record(EventRecord {
            timestamp: self.state.elapsed_seconds,
            context: pending.context,
            action: pending.action,
            outcome: Outcome::Ignored,
            reward: -0.25,
            salience: 0.35,
        });
    }

    fn update_selected_motif_reward(&mut self, reward: f32) {
        let Some(selected) = self.state.selected_motif_id else {
            return;
        };
        if let Some(motif) = self
            .state
            .vocal_motifs
            .iter_mut()
            .find(|motif| motif.id == selected)
        {
            motif.expected_reward += (reward - motif.expected_reward) * 0.18;
            motif.expected_reward = motif.expected_reward.clamp(-1.0, 1.0);
            if reward > 0.2 {
                motif.success_count = motif.success_count.saturating_add(1);
            }
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
        ActionId::ApproachCursor | ActionId::InvitePetting => (
            LocomotionMode::Arrive,
            sensors.cursor_position,
            PoseIntent::Curious,
            Some(InteractionTarget::Cursor),
            0.42,
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
        ActionId::PlayCursorChase | ActionId::InviteCursorChase => (
            LocomotionMode::Orbit,
            sensors.cursor_position,
            PoseIntent::Playful,
            Some(InteractionTarget::Cursor),
            0.72,
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
        ActionId::ExploreScreen | ActionId::HideAndSeek => (
            LocomotionMode::Wander,
            deterministic_wander_target(action, sensors.timestamp),
            PoseIntent::Curious,
            None,
            0.48,
        ),
        ActionId::BringProceduralOrb => (
            LocomotionMode::Seek,
            sensors.cursor_position.lerp(Vec2::splat(0.5), 0.35),
            PoseIntent::Playful,
            Some(InteractionTarget::ProceduralOrb),
            0.55,
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
    let phase = (timestamp as f32 * 0.07 + action.index() as f32 * 1.618).rem_euclid(6.0);
    Vec2::new(
        0.5 + phase.sin() * 0.42,
        0.5 + (phase * 0.73 + 1.1).cos() * 0.38,
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

fn motif_value(motif: &VocalMotif) -> f32 {
    motif.expected_reward * 0.7 + motif.novelty * 0.2 + motif.success_count as f32 * 0.01
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
