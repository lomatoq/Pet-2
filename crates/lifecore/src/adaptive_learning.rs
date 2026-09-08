//! Small, bounded learners. Only confirmed observations count as experience;
//! retrieval and rehearsal never manufacture another user response.

use std::collections::VecDeque;

use glam::Vec2;
use serde::{Deserialize, Serialize};

use crate::{
    ActionId, BodyFeedbackV2, BodyIntent, CommunicativeIntent, EmbodiedGestureEvent,
    EmbodiedGestureKind, InteractionGazeTarget, InteractionResponsePlan, LifeState, LocomotionMode,
    ReceiverEffect, VocalTrigger,
};

const FEATURES: usize = 8;
const RESPONSES: usize = 3;
const HISTORY: usize = 32;
type Features = [f32; FEATURES];

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct ResponseExperience {
    pub response_id: u64,
    context: Features,
    strategy: usize,
    reward: f32,
    at: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ResponseLearning {
    weights: [Features; RESPONSES],
    pub observations: [u32; RESPONSES],
    history: VecDeque<ResponseExperience>,
    pub last_outcome_id: u64,
    #[serde(skip)]
    pending: Option<(ResponseExperience, bool)>,
}

impl Default for ResponseLearning {
    fn default() -> Self {
        Self {
            weights: [[0.0; FEATURES]; RESPONSES],
            observations: [0; RESPONSES],
            history: VecDeque::new(),
            last_outcome_id: 0,
            pending: None,
        }
    }
}

impl ResponseLearning {
    pub fn supports(event: &EmbodiedGestureEvent) -> bool {
        event.boundary == crate::GestureBoundaryEvent::None
            && matches!(
                event.classification.kind,
                EmbodiedGestureKind::SoftTouch
                    | EmbodiedGestureKind::Tickle
                    | EmbodiedGestureKind::RhythmicTouch
            )
    }

    pub fn context(event: &EmbodiedGestureEvent, life: &LifeState, work: f32) -> Features {
        [
            1.0,
            work.clamp(0.0, 1.0),
            life.drives.sleep,
            life.affect.stress,
            life.drives.social,
            life.drives.play,
            f32::from(event.classification.kind == EmbodiedGestureKind::SoftTouch),
            event.classification.confidence.clamp(0.0, 1.0),
        ]
    }

    /// 0: listen quietly; 1: acknowledge contact; 2: invite another turn.
    pub fn choose(&self, context: Features, now: f64, random: f32) -> usize {
        if context[1] > 0.62 || context[2] >= 0.72 || context[3] > 0.75 {
            return 0;
        }
        let mut scores = [0.0; RESPONSES];
        for (strategy, score) in scores.iter_mut().enumerate() {
            let learned = dot(&self.weights[strategy], &context);
            let mut evidence = 0.0;
            let mut total = 0.0;
            let mut refused = false;
            for memory in self.history.iter().rev().filter(|m| m.strategy == strategy) {
                let distance = distance(&context, &memory.context);
                let age = (now - memory.at).max(0.0);
                if distance < 0.20 {
                    let weight = (1.0 - distance / 0.20) * (-age / 3_600.0).exp() as f32;
                    evidence += weight * memory.reward;
                    total += weight;
                    refused |= memory.reward < 0.0 && age < 30.0;
                }
            }
            // Retrieval interpolates an estimate; it is not another reward.
            let estimate = if total > 0.1 {
                learned * 0.75 + evidence / total * 0.25
            } else {
                learned
            };
            let prior = match strategy {
                0 => context[1] * 0.30 + context[2] * 0.20,
                1 => 0.12,
                _ => context[5] * 0.12 - context[1] * 0.35,
            };
            *score = if refused && strategy != 0 {
                -20.0
            } else {
                estimate + prior
            };
        }
        let max = scores.into_iter().fold(f32::NEG_INFINITY, f32::max);
        let temperature = 0.16;
        let weights = scores.map(|v| ((v - max) / temperature).exp());
        let mut remaining = random.clamp(0.0, 0.999_999) * weights.iter().sum::<f32>();
        for (index, weight) in weights.into_iter().enumerate() {
            remaining -= weight;
            if remaining <= 0.0 {
                return index;
            }
        }
        RESPONSES - 1
    }

    pub fn propose(&mut self, response_id: u64, context: Features, strategy: usize, now: f64) {
        self.pending = Some((
            ResponseExperience {
                response_id,
                context,
                strategy,
                at: now,
                reward: 0.0,
            },
            false,
        ));
    }

    pub fn acknowledge(&mut self, response_id: u64) {
        if let Some((experience, executed)) = &mut self.pending
            && experience.response_id == response_id
        {
            *executed = true;
        }
    }

    pub fn cancel(&mut self) {
        self.pending = None;
    }

    pub fn outcome(&mut self, response_id: u64, reward: Option<f32>, openness: f32, now: f64) {
        let Some((mut experience, executed)) = self.pending else {
            return;
        };
        if experience.response_id != response_id {
            return;
        }
        self.pending = None;
        if !executed || response_id <= self.last_outcome_id || now - experience.at > 30.0 {
            return;
        }
        self.last_outcome_id = response_id;
        let Some(reward) = reward.filter(|r| r.is_finite() && *r != 0.0) else {
            return;
        };
        experience.reward = reward.clamp(-1.0, 1.0);
        experience.at = now;
        let error =
            experience.reward - dot(&self.weights[experience.strategy], &experience.context);
        let rate = 0.04 * openness.clamp(0.5, 1.0);
        let norm = dot(&experience.context, &experience.context).max(1.0);
        for (w, x) in self.weights[experience.strategy]
            .iter_mut()
            .zip(experience.context)
        {
            *w = (*w + rate * error * x / norm).clamp(-2.0, 2.0);
        }
        self.observations[experience.strategy] =
            self.observations[experience.strategy].saturating_add(1);
        if self.history.len() == HISTORY {
            self.history.pop_front();
        }
        self.history.push_back(experience);
    }

    pub fn apply_strategy(plan: &mut InteractionResponsePlan, strategy: usize) {
        match strategy {
            0 => {
                plan.reason = crate::InteractionReasonCode::QuietAcknowledgement;
                plan.expression.relief = 0.0;
                plan.voice_trigger = None;
                plan.body.local_pulse = 0.0;
                plan.body.lean = 0.0;
                plan.gaze = InteractionGazeTarget::ContactPoint;
                plan.communicative_intent = CommunicativeIntent::AcknowledgeContact;
                plan.expected_receiver_effect = ReceiverEffect::Notice;
                plan.hold_seconds = 0.18;
            }
            2 => {
                plan.reason = crate::InteractionReasonCode::SharedRitual;
                plan.expression.relief = 0.0;
                plan.communicative_intent = CommunicativeIntent::InviteRepeat;
                plan.expected_receiver_effect = ReceiverEffect::RepeatMotif;
                plan.voice_trigger = Some(VocalTrigger::PlayfulRelease);
                plan.gaze = InteractionGazeTarget::Viewer;
                plan.body.local_pulse = 0.02;
                plan.await_user_seconds = 1.8;
            }
            _ => {}
        }
        plan.sanitize();
    }

    fn is_valid(&self) -> bool {
        self.weights
            .iter()
            .flatten()
            .all(|x| x.is_finite() && x.abs() <= 2.0)
            && self.history.len() <= HISTORY
            && self.history.iter().all(|e| {
                e.strategy < RESPONSES
                    && valid_features(&e.context)
                    && e.reward.is_finite()
                    && e.reward.abs() <= 1.0
                    && e.at.is_finite()
            })
    }
}

const OUTPUTS: usize = 4;
type Target = [f32; OUTPUTS];

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
struct Transition {
    input: Features,
    target: Target,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
struct BodyRegime {
    weights: [Features; OUTPUTS],
    previous_weights: [Features; OUTPUTS],
    training: VecDeque<Transition>,
    validation: VecDeque<Transition>,
    observations: u64,
    last_replay: u64,
    best_error: f32,
    validation_error: f32,
    baseline_error: f32,
    progress: f32,
    progress_streak: u8,
}

impl Default for BodyRegime {
    fn default() -> Self {
        let mut weights = [[0.0; FEATURES]; OUTPUTS];
        weights[0][1] = 0.8;
        weights[0][3] = 0.2;
        weights[1][2] = 0.8;
        weights[1][4] = 0.2;
        weights[2][6] = 1.0;
        weights[3][7] = 1.0;
        Self {
            weights,
            previous_weights: weights,
            training: VecDeque::new(),
            validation: VecDeque::new(),
            observations: 0,
            last_replay: 0,
            best_error: 4.0,
            validation_error: 4.0,
            baseline_error: 4.0,
            progress: 0.0,
            progress_streak: 0,
        }
    }
}

impl BodyRegime {
    fn predict_with(weights: &[Features; OUTPUTS], input: &Features) -> Target {
        std::array::from_fn(|i| dot(&weights[i], input).clamp(-1.0, 1.0))
    }

    fn optimize(&mut self, sample: Transition) {
        let predicted = Self::predict_with(&self.weights, &sample.input);
        let norm = dot(&sample.input, &sample.input).max(1.0);
        for (i, row) in self.weights.iter_mut().enumerate() {
            for (w, x) in row.iter_mut().zip(sample.input) {
                *w = (*w + 0.08 * (sample.target[i] - predicted[i]) * x / norm).clamp(-2.0, 2.0);
            }
        }
    }

    fn observe(&mut self, sample: Transition) {
        self.observations = self.observations.saturating_add(1);
        if self.observations.is_multiple_of(5) {
            if self.validation.len() == 16 {
                self.validation.pop_front();
            }
            self.validation.push_back(sample);
        } else {
            if self.training.len() == 128 {
                self.training.pop_front();
            }
            self.training.push_back(sample);
            self.optimize(sample);
        }
        if self.observations.is_multiple_of(32) && self.validation.len() >= 8 {
            let n = self.validation.len() as f32;
            let mut delta = 0.0;
            let mut delta_square = 0.0;
            let mut error = 0.0;
            let mut baseline = 0.0;
            for sample in &self.validation {
                let next = mse(
                    Self::predict_with(&self.weights, &sample.input),
                    sample.target,
                );
                let old = mse(
                    Self::predict_with(&self.previous_weights, &sample.input),
                    sample.target,
                );
                delta += old - next;
                delta_square += (old - next).powi(2);
                error += next;
                // Persistence is a deliberately strong resting-body baseline.
                baseline += mse(
                    [
                        sample.input[1],
                        sample.input[2],
                        sample.input[6],
                        sample.input[7],
                    ],
                    sample.target,
                );
            }
            self.validation_error = error / n;
            self.baseline_error = baseline / n;
            let mean = delta / n;
            let uncertainty = ((delta_square / n - mean * mean).max(0.0) / n).sqrt();
            let improves =
                mean > 2.0 * uncertainty + 0.000_01 && self.validation_error < self.best_error;
            self.progress_streak = if improves {
                self.progress_streak.saturating_add(1)
            } else {
                0
            };
            self.progress = if self.progress_streak >= 2 {
                (mean * 20.0).min(0.08)
            } else {
                0.0
            };
            self.best_error = self.best_error.min(self.validation_error);
            self.previous_weights = self.weights;
        }
    }

    fn rehearse(&mut self) -> u32 {
        if self.observations < self.last_replay + 32 {
            return 0;
        }
        self.last_replay = self.observations;
        // Eight uniform updates, no synthetic targets, no validation examples.
        let count = self.training.len().min(8);
        let samples: Vec<_> = (0..count)
            .map(|i| self.training[i * self.training.len() / count])
            .collect();
        for sample in &samples {
            self.optimize(*sample);
        }
        samples.len() as u32
    }

    fn is_valid(&self) -> bool {
        self.training.len() <= 128
            && self.validation.len() <= 16
            && self
                .weights
                .iter()
                .chain(&self.previous_weights)
                .flatten()
                .all(|x| x.is_finite() && x.abs() <= 2.0)
            && self.training.iter().chain(&self.validation).all(|s| {
                valid_features(&s.input) && s.target.iter().all(|x| x.is_finite() && x.abs() <= 1.0)
            })
            && [
                self.best_error,
                self.validation_error,
                self.baseline_error,
                self.progress,
            ]
            .iter()
            .all(|x| x.is_finite() && *x >= 0.0 && *x <= 4.0)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct BodyLearningDiagnostics {
    pub observations: u64,
    pub replay_updates: u64,
    pub prediction_error: f32,
    pub confidence: f32,
    pub learning_progress: f32,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BodyLearning {
    regimes: [BodyRegime; 2],
    pub diagnostics: BodyLearningDiagnostics,
    #[serde(skip)]
    pending: Option<(Features, usize, f64, u64)>,
    #[serde(skip)]
    blocked_seconds: f32,
    #[serde(skip)]
    fresh_for: f32,
}

impl BodyLearning {
    pub fn begin(&mut self, frame: BodyFeedbackV2, command: Vec2, timestamp: f64) {
        let command = command.clamp_length_max(1.0);
        let input = [
            1.0,
            frame.motion.velocity.x.clamp(-1.0, 1.0),
            frame.motion.velocity.y.clamp(-1.0, 1.0),
            command.x,
            command.y,
            frame.contact.pressure,
            frame.shape.deformation_energy,
            frame.shape.maximum_strain,
        ];
        self.pending = (valid_features(&input) && timestamp.is_finite()).then_some((
            input,
            usize::from(frame.contact.pressure > 0.02),
            timestamp,
            frame.frame_id,
        ));
    }

    pub fn complete(&mut self, frame: BodyFeedbackV2, timestamp: f64) {
        let Some((input, regime, started, frame_id)) = self.pending.take() else {
            return;
        };
        let dt = timestamp - started;
        if frame.frame_id <= frame_id || !(0.035..=0.075).contains(&dt) {
            self.diagnostics.confidence = 0.0;
            self.blocked_seconds = 0.0;
            return;
        }
        let target = [
            frame.motion.velocity.x.clamp(-1.0, 1.0),
            frame.motion.velocity.y.clamp(-1.0, 1.0),
            frame.shape.deformation_energy,
            frame.shape.maximum_strain,
        ];
        if !target.iter().all(|x| x.is_finite() && x.abs() <= 1.0) {
            return;
        }
        let model = &mut self.regimes[regime];
        let prediction = BodyRegime::predict_with(&model.weights, &input);
        self.diagnostics.prediction_error = mse(prediction, target);
        model.observe(Transition { input, target });
        self.diagnostics.observations = self.diagnostics.observations.saturating_add(1);
        let reliable = model.observations >= 160
            && model.validation_error <= model.baseline_error * 0.9 + 0.000_01
            && model.validation_error < 0.01
            && self.diagnostics.prediction_error < 0.02;
        self.diagnostics.confidence = if reliable { 0.8 } else { 0.0 };
        self.diagnostics.learning_progress = if regime == 0 { model.progress } else { 0.0 };
        self.fresh_for = 0.15;
        let moving_command = Vec2::new(input[3], input[4]).length() > 0.04;
        let stalled = Vec2::new(target[0], target[1]).length() < 0.01;
        self.blocked_seconds =
            if reliable && moving_command && stalled && frame.contact.pressure > 0.02 {
                (self.blocked_seconds + dt as f32).min(2.0)
            } else {
                0.0
            };
    }

    pub fn tick(&mut self, dt: f32) {
        self.fresh_for = (self.fresh_for - dt).max(0.0);
        if self.fresh_for == 0.0 {
            self.diagnostics.confidence = 0.0;
            self.diagnostics.learning_progress = 0.0;
            self.blocked_seconds = 0.0;
        }
    }

    pub fn adapt_intent(&self, intent: &mut BodyIntent, contact: bool) {
        if contact
            && self.blocked_seconds >= 0.4
            && matches!(
                intent.locomotion,
                LocomotionMode::Seek
                    | LocomotionMode::Arrive
                    | LocomotionMode::Wander
                    | LocomotionMode::Orbit
            )
        {
            // Reduce futile repeated effort; never reverse a safety escape.
            intent.desired_speed *= 0.75;
        }
    }

    pub fn curiosity_bonus(&self, action: ActionId, work: bool) -> f32 {
        if work || self.fresh_for == 0.0 {
            return 0.0;
        }
        if matches!(action, ActionId::SelfPlay | ActionId::ExploreScreen) {
            self.diagnostics.learning_progress
        } else {
            0.0
        }
    }

    pub fn clear_transients(&mut self) {
        self.pending = None;
        self.blocked_seconds = 0.0;
        self.fresh_for = 0.0;
        self.diagnostics.confidence = 0.0;
        self.diagnostics.learning_progress = 0.0;
    }

    pub fn rehearse(&mut self) {
        for regime in &mut self.regimes {
            self.diagnostics.replay_updates += u64::from(regime.rehearse());
        }
    }

    fn is_valid(&self) -> bool {
        self.regimes.iter().all(BodyRegime::is_valid)
            && [
                self.diagnostics.prediction_error,
                self.diagnostics.confidence,
                self.diagnostics.learning_progress,
            ]
            .iter()
            .all(|x| x.is_finite() && *x >= 0.0 && *x <= 4.0)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AdaptiveLearning {
    pub responses: ResponseLearning,
    pub body: BodyLearning,
}

impl AdaptiveLearning {
    pub fn is_valid(&self) -> bool {
        self.responses.is_valid() && self.body.is_valid()
    }
}

fn valid_features(x: &Features) -> bool {
    x.iter().all(|v| v.is_finite() && v.abs() <= 1.0)
}
fn dot(a: &Features, b: &Features) -> f32 {
    a.iter().zip(b).map(|(a, b)| a * b).sum()
}
fn distance(a: &Features, b: &Features) -> f32 {
    a.iter().zip(b).map(|(a, b)| (a - b).powi(2)).sum::<f32>() / FEATURES as f32
}
fn mse(a: Target, b: Target) -> f32 {
    a.into_iter()
        .zip(b)
        .map(|(a, b)| (a - b).powi(2))
        .sum::<f32>()
        / OUTPUTS as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context(gentle: bool) -> Features {
        [1.0, 0.0, 0.2, 0.1, 0.5, 0.6, f32::from(gentle), 0.9]
    }

    fn reward(policy: &mut ResponseLearning, id: u64, x: Features, strategy: usize, value: f32) {
        policy.propose(id, x, strategy, id as f64);
        policy.acknowledge(id);
        policy.outcome(id, Some(value), 1.0, id as f64);
    }

    #[test]
    fn outcomes_require_execution_and_are_consumed_once() {
        let mut p = ResponseLearning::default();
        p.propose(1, context(true), 1, 0.0);
        p.outcome(1, Some(1.0), 1.0, 1.0);
        assert_eq!(p.observations, [0; 3]);
        reward(&mut p, 2, context(true), 1, -1.0);
        let learned = p.clone();
        p.outcome(2, Some(1.0), 1.0, 3.0);
        assert_eq!(p, learned);
        p.propose(3, context(true), 2, 4.0);
        p.acknowledge(3);
        p.outcome(3, None, 1.0, 5.0);
        assert_eq!(p.observations, [0, 1, 0]);
    }

    #[test]
    fn learns_opposite_contexts_and_preserves_focus_guard() {
        let mut p = ResponseLearning::default();
        let mut id = 1;
        for _ in 0..160 {
            for gentle in [true, false] {
                for strategy in 0..3 {
                    let preferred = if gentle { 1 } else { 2 };
                    reward(
                        &mut p,
                        id,
                        context(gentle),
                        strategy,
                        if strategy == preferred { 1.0 } else { -0.5 },
                    );
                    id += 1;
                }
            }
        }
        // Evaluate after short refusal memory has expired, with unseen social drive.
        for gentle in [true, false] {
            let mut x = context(gentle);
            x[4] = 0.55;
            let correct = (0..100)
                .filter(|i| p.choose(x, 10_000.0, *i as f32 / 100.0) == if gentle { 1 } else { 2 })
                .count();
            assert!(correct > 75, "context {gentle}: {correct}% correct");
            x[1] = 1.0;
            assert!((0..100).all(|i| p.choose(x, 10_000.0, i as f32 / 100.0) == 0));
        }
    }

    #[test]
    fn recent_refusal_changes_next_attempt_without_another_reward() {
        let mut p = ResponseLearning::default();
        reward(&mut p, 1, context(true), 2, -1.0);
        let count = p.observations;
        assert!((0..100).all(|i| p.choose(context(true), 2.0, i as f32 / 100.0) != 2));
        assert_eq!(p.observations, count);
        // Contextually different play is not a permanent global prohibition.
        assert!((0..100).any(|i| p.choose(context(false), 100.0, i as f32 / 100.0) == 2));
    }

    fn sample(i: usize) -> Transition {
        let a = (i as f32 * 0.37).sin();
        let b = (i as f32 * 0.71).cos();
        Transition {
            input: [1.0, 0.0, 0.0, a, b, 0.3, 0.0, 0.0],
            target: [0.6 * a, 0.4 * b, 0.09, 0.06],
        }
    }

    #[test]
    fn body_model_predicts_unseen_action_effects_and_rehearsal_has_bounded_cost() {
        let mut model = BodyRegime::default();
        for i in 0..800 {
            model.observe(sample(i));
        }
        let error: f32 = (1000..1100)
            .map(|i| {
                let s = sample(i);
                mse(BodyRegime::predict_with(&model.weights, &s.input), s.target)
            })
            .sum::<f32>()
            / 100.0;
        assert!(error < 0.001, "unseen prediction error {error}");
        let before = model.observations;
        let validation = model.validation.clone();
        assert_eq!(model.rehearse(), 8);
        let rehearsed = model.clone();
        assert_eq!(model.rehearse(), 0);
        assert_eq!(model, rehearsed);
        assert_eq!(model.observations, before);
        assert_eq!(model.validation, validation);
        assert!(model.is_valid());
    }

    #[test]
    fn unpredictable_noise_does_not_create_sustained_curiosity() {
        let mut model = BodyRegime::default();
        let mut random = 0x1234_5678_u32;
        let mut bonuses = 0;
        for i in 0..3000 {
            let mut s = sample(i);
            for y in &mut s.target {
                random ^= random << 13;
                random ^= random >> 17;
                random ^= random << 5;
                *y = random as f32 / u32::MAX as f32 * 2.0 - 1.0;
            }
            model.observe(s);
            if i > 2000 && model.progress > 0.0 {
                bonuses += 1;
            }
        }
        assert!(
            bonuses < 100,
            "persistent noise bonus on {bonuses} transitions"
        );
    }

    #[test]
    fn duplicate_or_late_body_frames_never_train() {
        let mut model = BodyLearning::default();
        let frame = BodyFeedbackV2 {
            frame_id: 2,
            ..Default::default()
        };
        let intent = BodyIntent {
            locomotion: LocomotionMode::Seek,
            target_position: Vec2::ONE,
            target_surface: None,
            desired_speed: 0.1,
            facing_direction: 1.0,
            gaze_target: None,
            pose: crate::PoseIntent::Neutral,
            expression: crate::ExpressionState::default(),
            interaction_target: None,
        };
        model.begin(frame, Vec2::splat(intent.desired_speed * 0.5), 0.0);
        model.complete(frame, 0.05);
        assert_eq!(model.diagnostics.observations, 0);
        model.begin(frame, Vec2::splat(intent.desired_speed * 0.5), 0.0);
        model.complete(
            BodyFeedbackV2 {
                frame_id: 3,
                ..frame
            },
            0.2,
        );
        assert_eq!(model.diagnostics.observations, 0);
        model.begin(frame, Vec2::splat(intent.desired_speed * 0.5), 0.0);
        model.complete(
            BodyFeedbackV2 {
                frame_id: 3,
                ..frame
            },
            0.05,
        );
        model.complete(
            BodyFeedbackV2 {
                frame_id: 4,
                ..frame
            },
            0.10,
        );
        assert_eq!(model.diagnostics.observations, 1);
    }

    #[test]
    fn learnable_effects_get_progress_but_known_effects_do_not() {
        let mut learning = BodyRegime::default();
        let mut known = BodyRegime::default();
        let mut peak = 0.0_f32;
        for i in 0..256 {
            learning.observe(sample(i));
            known.observe(Transition {
                input: [1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
                target: [0.0; OUTPUTS],
            });
            peak = peak.max(learning.progress);
        }
        assert!(
            peak > 0.0,
            "learnable dynamics should earn a bounded progress signal"
        );
        assert_eq!(known.progress, 0.0);
        assert!(peak <= 0.08);
    }

    #[test]
    fn learned_hold_reduces_futile_effort_and_release_restores_it() {
        let mut model = BodyLearning::default();
        let mut frame = BodyFeedbackV2 {
            frame_id: 1,
            ..Default::default()
        };
        frame.contact.pressure = 0.8;
        let intent = BodyIntent {
            locomotion: LocomotionMode::Seek,
            target_position: Vec2::ONE,
            target_surface: None,
            desired_speed: 0.1,
            facing_direction: 1.0,
            gaze_target: None,
            pose: crate::PoseIntent::Neutral,
            expression: crate::ExpressionState::default(),
            interaction_target: None,
        };
        for i in 0..500 {
            model.begin(
                frame,
                Vec2::splat(intent.desired_speed * 0.5),
                i as f64 * 0.05,
            );
            frame.frame_id += 1;
            model.complete(frame, (i + 1) as f64 * 0.05);
        }
        let mut adapted = intent.clone();
        model.adapt_intent(&mut adapted, true);
        assert!(adapted.desired_speed < intent.desired_speed);
        let mut released = intent.clone();
        model.adapt_intent(&mut released, false);
        assert_eq!(released, intent);
        let mut escape = intent;
        escape.locomotion = LocomotionMode::Flee;
        let before = escape.clone();
        model.adapt_intent(&mut escape, true);
        assert_eq!(escape, before);
    }

    #[test]
    fn restart_preserves_learning_but_discards_unexecuted_credit() {
        let mut learning = AdaptiveLearning::default();
        reward(&mut learning.responses, 1, context(true), 1, 1.0);
        learning.responses.propose(2, context(false), 2, 2.0);
        let mut restored: AdaptiveLearning =
            serde_json::from_slice(&serde_json::to_vec(&learning).unwrap()).unwrap();
        restored.responses.acknowledge(2);
        restored.responses.outcome(2, Some(1.0), 1.0, 3.0);
        assert_eq!(restored.responses.observations, [0, 1, 0]);
        assert!(restored.is_valid());
    }
}
