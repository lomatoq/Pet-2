//! Small causal regulator layered around LifeCore's authoritative action choice.
//!
//! This module does not choose or execute actions. It explains and shapes the
//! action that LifeCore already selected, waits for measured consequences, and
//! learns only from successful action-outcome pairs.

use serde::{Deserialize, Serialize};

use crate::{ActionId, DriveKind, ExpectedOutcome, PrimaryIntent};

pub const MAX_ORGANIC_HABITS: usize = 32;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CausalPhase {
    #[default]
    Notice,
    Prepare,
    Act,
    AwaitOutcome,
    IntegrateOutcome,
    Recover,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrganicCauseCode {
    #[default]
    DriveError,
    ExternalNovelty,
    DirectContact,
    HeardName,
    SurfaceOpportunity,
    ActionChanged,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrganicOutcomeCode {
    #[default]
    Pending,
    Success,
    NoResponse,
    Failed,
    Interrupted,
    ContactLost,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct OrganicOutcome {
    pub code: OrganicOutcomeCode,
    /// Confidence that this outcome belongs to the current episode.
    pub confidence: f32,
    /// Measured quality of a successful outcome, in `[0, 1]`.
    pub quality: f32,
}

impl OrganicOutcome {
    #[must_use]
    pub const fn success(quality: f32) -> Self {
        Self {
            code: OrganicOutcomeCode::Success,
            confidence: 1.0,
            quality,
        }
    }

    #[must_use]
    pub const fn no_response() -> Self {
        Self {
            code: OrganicOutcomeCode::NoResponse,
            confidence: 1.0,
            quality: 0.0,
        }
    }

    fn sanitized(mut self) -> Self {
        self.confidence = unit(self.confidence);
        self.quality = unit(self.quality);
        self
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CausalEpisodeTrace {
    pub episode_id: u64,
    pub cause: OrganicCauseCode,
    pub drive: Option<DriveKind>,
    pub drive_error: f32,
    pub selected_action: ActionId,
    pub expected: ExpectedOutcome,
    pub phase: CausalPhase,
    pub phase_elapsed: f32,
    pub total_elapsed: f32,
    pub deadline_seconds: f32,
    pub observed: OrganicOutcomeCode,
    pub prediction_error: f32,
    pub context_key: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct OrganicHabitV1 {
    pub context_key: u64,
    pub action: ActionId,
    pub strength: f32,
    pub expected_quality: f32,
    pub successes: u16,
    pub last_success_seconds: f64,
}

impl OrganicHabitV1 {
    fn sanitize(&mut self) {
        self.strength = unit(self.strength);
        self.expected_quality = unit(self.expected_quality);
        if !self.last_success_seconds.is_finite() {
            self.last_success_seconds = 0.0;
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct OrganicRegulationStateV1 {
    pub schema_version: u16,
    pub activation: f32,
    pub recovery: f32,
    pub next_episode_id: u64,
    pub active_episode: Option<CausalEpisodeTrace>,
    pub habits: Vec<OrganicHabitV1>,
    pub bid_refractory_seconds: f32,
    pub no_response_streak: u8,
}

impl Default for OrganicRegulationStateV1 {
    fn default() -> Self {
        Self {
            schema_version: 1,
            activation: 0.0,
            recovery: 0.0,
            next_episode_id: 1,
            active_episode: None,
            habits: Vec::new(),
            bid_refractory_seconds: 0.0,
            no_response_streak: 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RegulationInput {
    pub dt: f32,
    pub timestamp_seconds: f64,
    pub selected_action: ActionId,
    pub primary_intent: PrimaryIntent,
    pub expected: ExpectedOutcome,
    pub strongest_drive: DriveKind,
    pub drive_error: f32,
    pub cause: Option<OrganicCauseCode>,
    pub external_novelty: f32,
    pub prediction_error: f32,
    pub user_available: f32,
    pub quiet: bool,
    pub direct_contact: f32,
    pub context_key: u64,
    /// The motor has acquired and oriented to its target.
    pub oriented: bool,
    /// The motor has visibly preloaded/prepared the action.
    pub prepared: bool,
    /// The action's visible performance has occurred.
    pub acted: bool,
    pub outcome: Option<OrganicOutcome>,
}

impl Default for RegulationInput {
    fn default() -> Self {
        Self {
            dt: 0.05,
            timestamp_seconds: 0.0,
            selected_action: ActionId::IdleHover,
            primary_intent: PrimaryIntent::IdleContent,
            expected: ExpectedOutcome::default(),
            strongest_drive: DriveKind::Curiosity,
            drive_error: 0.0,
            cause: None,
            external_novelty: 0.0,
            prediction_error: 0.0,
            user_available: 0.0,
            quiet: false,
            direct_contact: 0.0,
            context_key: 0,
            oriented: false,
            prepared: false,
            acted: false,
            outcome: None,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct OutcomeGatedRelief {
    pub drive: Option<DriveKind>,
    pub amount: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RegulationOutput {
    pub trace: Option<CausalEpisodeTrace>,
    pub activation: f32,
    pub recovery: f32,
    pub bid_allowed: bool,
    /// Learned confidence for the already selected action in this context.
    pub selected_habit_strength: f32,
    pub relief: OutcomeGatedRelief,
}

#[derive(Debug, Clone, Default)]
pub struct OrganicRegulator {
    state: OrganicRegulationStateV1,
}

impl OrganicRegulator {
    #[must_use]
    pub fn from_state(mut state: OrganicRegulationStateV1) -> Self {
        state.schema_version = 1;
        state.activation = unit(state.activation);
        state.recovery = unit(state.recovery);
        state.next_episode_id = state.next_episode_id.max(1);
        state.bid_refractory_seconds = finite(state.bid_refractory_seconds).clamp(0.0, 3_600.0);
        state.no_response_streak = state.no_response_streak.min(8);
        for habit in &mut state.habits {
            habit.sanitize();
        }
        state.habits.sort_by(|a, b| {
            b.strength
                .total_cmp(&a.strength)
                .then_with(|| b.last_success_seconds.total_cmp(&a.last_success_seconds))
        });
        state.habits.truncate(MAX_ORGANIC_HABITS);
        if let Some(trace) = &mut state.active_episode {
            sanitize_trace(trace);
        }
        Self { state }
    }

    #[must_use]
    pub fn persistent_state(&self) -> OrganicRegulationStateV1 {
        self.state.clone()
    }

    #[must_use]
    pub fn active_trace(&self) -> Option<&CausalEpisodeTrace> {
        self.state.active_episode.as_ref()
    }

    #[must_use]
    pub fn selected_habit_strength(&self, context_key: u64, action: ActionId) -> f32 {
        self.state
            .habits
            .iter()
            .find(|habit| habit.context_key == context_key && habit.action == action)
            .map_or(0.0, |habit| habit.strength * habit.expected_quality)
    }

    /// Goal-directed value remains a live gate over learned repetition. This
    /// leaves the stored association intact so it can recover when the outcome
    /// becomes useful again.
    #[must_use]
    pub fn habit_value(
        &self,
        context_key: u64,
        action: ActionId,
        current_outcome_value: f32,
    ) -> f32 {
        self.selected_habit_strength(context_key, action) * unit(current_outcome_value)
    }

    #[must_use]
    pub fn tick(&mut self, mut input: RegulationInput) -> RegulationOutput {
        input.dt = finite(input.dt).clamp(0.0, 0.25);
        input.drive_error = unit(input.drive_error);
        input.external_novelty = unit(input.external_novelty);
        input.prediction_error = unit(input.prediction_error);
        input.user_available = unit(input.user_available);
        input.direct_contact = unit(input.direct_contact);
        input.expected = input.expected.sanitized();
        input.outcome = input.outcome.map(OrganicOutcome::sanitized);
        self.state.bid_refractory_seconds = (self.state.bid_refractory_seconds - input.dt).max(0.0);

        let social_bid = is_social_bid(input.selected_action, input.primary_intent);
        let habit_strength = self.habit_value(
            input.context_key,
            input.selected_action,
            input.expected.success,
        );
        let bid_allowed = !input.quiet
            && self.state.bid_refractory_seconds <= 0.0
            && !self.state.active_episode.as_ref().is_some_and(|episode| {
                is_social_action(episode.selected_action)
                    && matches!(
                        episode.phase,
                        CausalPhase::Notice
                            | CausalPhase::Prepare
                            | CausalPhase::Act
                            | CausalPhase::AwaitOutcome
                    )
            });

        let forced_input = 0.45 * input.drive_error
            + 0.30 * input.external_novelty
            + 0.35 * input.prediction_error
            + 0.25 * input.direct_contact
            + 0.10 * habit_strength;
        let safe_rest = if matches!(
            input.primary_intent,
            PrimaryIntent::IdleContent | PrimaryIntent::Rest | PrimaryIntent::QuietCompanionship
        ) {
            1.0
        } else {
            0.0
        };
        let activation_target =
            (forced_input - 0.30 * safe_rest - 0.34 * self.state.recovery).clamp(0.0, 1.0);
        self.state.activation = approach(
            self.state.activation,
            activation_target,
            if activation_target > self.state.activation {
                0.34
            } else {
                1.15
            },
            input.dt,
        );
        self.state.recovery = approach(
            self.state.recovery,
            self.state.activation,
            if self.state.activation > self.state.recovery {
                1.6
            } else {
                3.2
            },
            input.dt,
        );

        if self.state.active_episode.is_none() {
            // Drive magnitude shapes activation, but a new episode needs an
            // edge-triggered cause supplied by the owner (selected-action
            // change, new contact, novelty, name cue, or surface opportunity).
            // This prevents a steady high meter from becoming a timer loop.
            let cause = input.cause;
            let permitted = !social_bid || bid_allowed;
            if let Some(cause) = cause.filter(|_| permitted) {
                let deadline_seconds = if social_bid {
                    // Repeated unanswered bids wait longer, then enter a longer
                    // refractory interval instead of worsening attachment.
                    3.0 + f32::from(self.state.no_response_streak) * 0.75
                } else {
                    2.5
                };
                self.state.active_episode = Some(CausalEpisodeTrace {
                    episode_id: self.state.next_episode_id,
                    cause,
                    drive: Some(input.strongest_drive),
                    drive_error: input.drive_error,
                    selected_action: input.selected_action,
                    expected: input.expected,
                    phase: CausalPhase::Notice,
                    phase_elapsed: 0.0,
                    total_elapsed: 0.0,
                    deadline_seconds,
                    observed: OrganicOutcomeCode::Pending,
                    prediction_error: input.prediction_error,
                    context_key: input.context_key,
                });
                self.state.next_episode_id = self.state.next_episode_id.saturating_add(1).max(1);
            }
        }

        let mut relief = OutcomeGatedRelief::default();
        let mut completed_success: Option<(u64, ActionId, f32, f64)> = None;
        let mut timed_out_social = false;
        if let Some(trace) = &mut self.state.active_episode {
            trace.phase_elapsed += input.dt;
            trace.total_elapsed += input.dt;
            let outcome = input.outcome.filter(|outcome| {
                outcome.confidence >= 0.45
                    && (matches!(trace.phase, CausalPhase::Act | CausalPhase::AwaitOutcome)
                        || matches!(
                            outcome.code,
                            OrganicOutcomeCode::Interrupted | OrganicOutcomeCode::ContactLost
                        ))
            });
            if let Some(outcome) = outcome {
                trace.observed = outcome.code;
                trace.prediction_error = prediction_error(trace.expected, outcome);
                trace.phase = CausalPhase::IntegrateOutcome;
                trace.phase_elapsed = 0.0;
                if outcome.code == OrganicOutcomeCode::Success {
                    relief = OutcomeGatedRelief {
                        drive: trace.drive,
                        amount: (outcome.quality * trace.drive_error * 0.32).clamp(0.0, 0.24),
                    };
                    completed_success = Some((
                        trace.context_key,
                        trace.selected_action,
                        outcome.quality,
                        input.timestamp_seconds,
                    ));
                    self.state.no_response_streak = 0;
                } else if outcome.code == OrganicOutcomeCode::NoResponse {
                    timed_out_social = is_social_action(trace.selected_action);
                }
            } else {
                advance_episode(trace, input);
                if trace.phase == CausalPhase::AwaitOutcome
                    && trace.phase_elapsed >= trace.deadline_seconds
                {
                    trace.observed = OrganicOutcomeCode::NoResponse;
                    trace.prediction_error = trace.expected.user_response;
                    trace.phase = CausalPhase::IntegrateOutcome;
                    trace.phase_elapsed = 0.0;
                    timed_out_social = is_social_action(trace.selected_action);
                }
            }
        }

        if let Some((context, action, quality, now)) = completed_success {
            self.reinforce_success(context, action, quality, now);
        }
        if timed_out_social {
            self.state.no_response_streak = self.state.no_response_streak.saturating_add(1).min(8);
            let streak = f32::from(self.state.no_response_streak);
            self.state.bid_refractory_seconds = (8.0 + streak * streak * 5.0).min(180.0);
        }
        if self
            .state
            .active_episode
            .as_ref()
            .is_some_and(|trace| trace.phase == CausalPhase::Recover && trace.phase_elapsed >= 0.85)
        {
            self.state.active_episode = None;
        }

        RegulationOutput {
            trace: self.state.active_episode.clone(),
            activation: self.state.activation,
            recovery: self.state.recovery,
            bid_allowed,
            selected_habit_strength: habit_strength,
            relief,
        }
    }

    fn reinforce_success(&mut self, context_key: u64, action: ActionId, quality: f32, now: f64) {
        if let Some(habit) = self
            .state
            .habits
            .iter_mut()
            .find(|habit| habit.context_key == context_key && habit.action == action)
        {
            habit.successes = habit.successes.saturating_add(1);
            let rate = (0.22 / f32::from(habit.successes).sqrt()).clamp(0.035, 0.16);
            habit.expected_quality += (quality - habit.expected_quality) * rate;
            // Strength grows slowly and saturates; current outcome value still
            // gates its expression, preserving outcome devaluation.
            habit.strength += (1.0 - habit.strength) * rate * quality;
            habit.last_success_seconds = now;
            habit.sanitize();
            return;
        }
        if self.state.habits.len() >= MAX_ORGANIC_HABITS {
            let replace = self
                .state
                .habits
                .iter()
                .enumerate()
                .min_by(|(_, left), (_, right)| {
                    (left.strength * left.expected_quality)
                        .total_cmp(&(right.strength * right.expected_quality))
                        .then_with(|| {
                            left.last_success_seconds
                                .total_cmp(&right.last_success_seconds)
                        })
                })
                .map(|(index, _)| index)
                .unwrap_or(0);
            self.state.habits.swap_remove(replace);
        }
        self.state.habits.push(OrganicHabitV1 {
            context_key,
            action,
            strength: (quality * 0.12).clamp(0.02, 0.12),
            expected_quality: quality,
            successes: 1,
            last_success_seconds: now,
        });
    }
}

fn advance_episode(trace: &mut CausalEpisodeTrace, input: RegulationInput) {
    match trace.phase {
        CausalPhase::Notice if input.oriented || trace.phase_elapsed >= 0.28 => {
            set_phase(trace, CausalPhase::Prepare);
        }
        CausalPhase::Prepare if input.prepared || trace.phase_elapsed >= 0.38 => {
            set_phase(trace, CausalPhase::Act);
        }
        CausalPhase::Act if input.acted || trace.phase_elapsed >= 0.70 => {
            set_phase(trace, CausalPhase::AwaitOutcome);
        }
        CausalPhase::IntegrateOutcome if trace.phase_elapsed >= 0.24 => {
            set_phase(trace, CausalPhase::Recover);
        }
        _ => {}
    }
}

fn set_phase(trace: &mut CausalEpisodeTrace, phase: CausalPhase) {
    trace.phase = phase;
    trace.phase_elapsed = 0.0;
}

fn prediction_error(expected: ExpectedOutcome, outcome: OrganicOutcome) -> f32 {
    let expected_success = expected.success * (1.0 - expected.uncertainty * 0.5);
    ((if outcome.code == OrganicOutcomeCode::Success {
        outcome.quality
    } else {
        0.0
    }) - expected_success)
        .abs()
        .clamp(0.0, 1.0)
}

fn is_social_bid(action: ActionId, intent: PrimaryIntent) -> bool {
    is_social_action(action)
        || matches!(
            intent,
            PrimaryIntent::InviteContact | PrimaryIntent::InvitePlay | PrimaryIntent::SocialCheckIn
        )
}

fn is_social_action(action: ActionId) -> bool {
    matches!(
        action,
        ActionId::ApproachCursor
            | ActionId::InvitePetting
            | ActionId::InviteCursorChase
            | ActionId::SilentStare
            | ActionId::Chirp
            | ActionId::Purr
    )
}

fn sanitize_trace(trace: &mut CausalEpisodeTrace) {
    trace.episode_id = trace.episode_id.max(1);
    trace.drive_error = unit(trace.drive_error);
    trace.expected = trace.expected.sanitized();
    trace.phase_elapsed = finite(trace.phase_elapsed).clamp(0.0, 3_600.0);
    trace.total_elapsed = finite(trace.total_elapsed).clamp(0.0, 86_400.0);
    trace.deadline_seconds = finite(trace.deadline_seconds).clamp(0.25, 180.0);
    trace.prediction_error = unit(trace.prediction_error);
}

fn approach(current: f32, target: f32, tau: f32, dt: f32) -> f32 {
    if dt <= 0.0 {
        return current;
    }
    let alpha = 1.0 - (-dt / tau.max(0.001)).exp();
    (current + (target - current) * alpha).clamp(0.0, 1.0)
}

fn finite(value: f32) -> f32 {
    if value.is_finite() { value } else { 0.0 }
}

fn unit(value: f32) -> f32 {
    finite(value).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn social_input(dt: f32) -> RegulationInput {
        RegulationInput {
            dt,
            selected_action: ActionId::InvitePetting,
            primary_intent: PrimaryIntent::InviteContact,
            expected: ExpectedOutcome {
                user_response: 0.7,
                success: 0.6,
                uncertainty: 0.2,
                ..ExpectedOutcome::default()
            },
            strongest_drive: DriveKind::Social,
            drive_error: 0.72,
            cause: Some(OrganicCauseCode::DriveError),
            user_available: 0.8,
            context_key: 42,
            ..RegulationInput::default()
        }
    }

    #[test]
    fn deterministic_phase_sequence_is_notice_prepare_act_await_integrate_recover() {
        let mut regulator = OrganicRegulator::default();
        let mut phases = Vec::new();
        for tick in 0..160 {
            let mut input = social_input(0.05);
            input.cause = (tick == 0).then_some(OrganicCauseCode::DriveError);
            input.oriented = tick >= 2;
            input.prepared = tick >= 4;
            input.acted = tick >= 6;
            if tick == 12 {
                input.outcome = Some(OrganicOutcome::success(0.9));
            }
            if let Some(trace) = regulator.tick(input).trace
                && phases.last() != Some(&trace.phase)
            {
                phases.push(trace.phase);
            }
        }
        assert_eq!(
            phases,
            [
                CausalPhase::Notice,
                CausalPhase::Prepare,
                CausalPhase::Act,
                CausalPhase::AwaitOutcome,
                CausalPhase::IntegrateOutcome,
                CausalPhase::Recover,
            ]
        );
    }

    #[test]
    fn unanswered_bids_recover_without_habit_learning_or_negative_internal_value() {
        let mut regulator = OrganicRegulator::default();
        let mut input = social_input(0.05);
        let mut saw_no_response = false;
        for _ in 0..180 {
            let output = regulator.tick(input);
            input.cause = None;
            input.oriented = true;
            input.prepared = true;
            input.acted = true;
            if output
                .trace
                .as_ref()
                .is_some_and(|trace| trace.observed == OrganicOutcomeCode::NoResponse)
            {
                saw_no_response = true;
                assert_eq!(output.relief.amount, 0.0);
            }
        }
        let state = regulator.persistent_state();
        assert!(saw_no_response);
        assert!(state.habits.is_empty());
        assert_eq!(state.no_response_streak, 1);
        assert!(state.bid_refractory_seconds > 0.0);
        assert!(state.activation >= 0.0 && state.recovery >= 0.0);
    }

    #[test]
    fn habits_update_only_on_success_and_devalue_with_outcome_quality() {
        let mut regulator = OrganicRegulator::default();
        let mut input = social_input(0.05);
        for tick in 0..40 {
            input.cause = (tick == 0).then_some(OrganicCauseCode::DriveError);
            input.oriented = true;
            input.prepared = true;
            input.acted = true;
            input.outcome = (tick == 8).then_some(OrganicOutcome::success(0.8));
            let _ = regulator.tick(input);
        }
        let learned = regulator.selected_habit_strength(42, ActionId::InvitePetting);
        assert!(learned > 0.0);
        assert_eq!(regulator.habit_value(42, ActionId::InvitePetting, 0.0), 0.0);
        assert_eq!(
            regulator.habit_value(42, ActionId::InvitePetting, 1.0),
            learned
        );
        let mut state = regulator.persistent_state();
        state.habits[0].expected_quality = 0.0;
        let devalued = OrganicRegulator::from_state(state);
        assert_eq!(
            devalued.selected_habit_strength(42, ActionId::InvitePetting),
            0.0
        );
    }

    #[test]
    fn fixed_time_trace_matches_across_common_tick_rates() {
        fn replay(hz: u32) -> Vec<CausalPhase> {
            let mut regulator = OrganicRegulator::default();
            let mut phases = Vec::new();
            let dt = 1.0 / hz as f32;
            for tick in 0..(hz * 3) {
                let mut input = social_input(dt);
                input.cause = (tick == 0).then_some(OrganicCauseCode::DriveError);
                if let Some(trace) = regulator.tick(input).trace
                    && phases.last() != Some(&trace.phase)
                {
                    phases.push(trace.phase);
                }
            }
            phases
        }
        assert_eq!(replay(30), replay(60));
        assert_eq!(replay(60), replay(120));
    }
}
