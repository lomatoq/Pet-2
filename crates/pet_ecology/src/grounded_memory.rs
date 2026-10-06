//! Grounded, object-specific experience. Only the application execution boundary
//! can acknowledge an intervention; selecting an action is never success.
use crate::exercise::{Exercise, SkillRecord};
use crate::{MAX_OBJECT_SPEED, ObjectCommand, ObjectId, ObjectKind, ObjectLifecycle, WorldObject};
use glam::Vec2;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

pub const MAX_BELIEFS: usize = 32;
pub const MAX_CAUSAL_EPISODES: usize = 128;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Intervention {
    Push,
    Grip,
    Release,
    Store,
    Retrieve,
    Consume,
}
impl Intervention {
    pub fn from_command(c: ObjectCommand) -> Option<(Self, ObjectId, Vec2)> {
        Some(match c {
            ObjectCommand::ApplyImpulse { object_id, impulse } => (Self::Push, object_id, impulse),
            ObjectCommand::MoveToward {
                object_id, target, ..
            } => (Self::Grip, object_id, target),
            ObjectCommand::Release {
                object_id,
                velocity,
            } => (Self::Release, object_id, velocity),
            ObjectCommand::Store { object_id, .. } => (Self::Store, object_id, Vec2::ZERO),
            ObjectCommand::Retrieve { object_id, target } => (Self::Retrieve, object_id, target),
            ObjectCommand::Consume { object_id } => (Self::Consume, object_id, Vec2::ZERO),
            ObjectCommand::None => return None,
        })
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CausalOutcome {
    AwaitingObservation,
    Observed,
    Blocked,
    Confounded,
    Unobserved,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CausalEpisode {
    pub id: u64,
    pub episode_id: u64,
    pub object_id: ObjectId,
    pub action: Intervention,
    pub before: Vec2,
    pub intended: Vec2,
    pub predicted: Vec2,
    pub after: Option<Vec2>,
    pub observed_velocity: Vec2,
    pub at: f64,
    pub outcome: CausalOutcome,
    pub prediction_error: Option<f32>,
    pub applied: bool,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ObjectBelief {
    pub id: ObjectId,
    pub kind: ObjectKind,
    pub last_position: Vec2,
    pub velocity: Vec2,
    pub last_seen: f64,
    pub visible: bool,
    pub uncertainty: f32,
    pub container: bool,
    pub observations: u64,
    /// Columns of the learned local impulse -> velocity Jacobian. No private
    /// physics mass is exposed to the learner or the exercise planner.
    pub impulse_columns: [Vec2; 2],
    pub impulse_samples: u32,
    pub effect_error: f32,
    pub motion_bias: Vec2,
    pub motion_samples: u32,
}
impl ObjectBelief {
    pub fn effect(&self, impulse: Vec2) -> Vec2 {
        self.impulse_columns[0] * impulse.x + self.impulse_columns[1] * impulse.y
    }
    pub fn confidence(&self) -> f32 {
        self.impulse_samples as f32
            / (self.impulse_samples as f32 + 6.0)
            / (1.0 + self.effect_error * 8.0)
    }
    pub fn predict_position(&self, now: f64, aspect: f32) -> Vec2 {
        let age = (now - self.last_seen).clamp(0.0, 0.6) as f32;
        (self.last_position
            + (self.velocity * age + self.motion_bias * (age / 0.15)) / metric(aspect))
        .clamp(Vec2::ZERO, Vec2::ONE)
    }
    pub fn impulse_for_velocity(&self, desired: Vec2, current: Vec2) -> Vec2 {
        let delta = (desired - current).clamp_length_max(0.45);
        let a = self.impulse_columns[0];
        let b = self.impulse_columns[1];
        let det = a.perp_dot(b);
        let learned = if det.abs() > 0.05 {
            Vec2::new(delta.perp_dot(b), a.perp_dot(delta)) / det
        } else {
            delta / 1.4
        };
        // Bound exploration. Learned weights may modulate effort, never bypass
        // physical contact, user ownership or hard motion limits.
        (delta / 1.4)
            .lerp(learned, self.confidence())
            .clamp_length_max(0.18)
    }
    fn valid(&self) -> bool {
        self.id != 0
            && point(self.last_position)
            && self.velocity.is_finite()
            && self.velocity.length() <= MAX_OBJECT_SPEED + 0.001
            && self.last_seen.is_finite()
            && self.last_seen >= 0.0
            && unit(self.uncertainty)
            && self
                .impulse_columns
                .iter()
                .all(|v| v.is_finite() && v.abs().max_element() <= 20.0)
            && unit(self.effect_error)
            && self.motion_bias.is_finite()
            && self.motion_bias.length() <= 0.0401
    }
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct GroundedStats {
    pub proposals: u64,
    pub executed: u64,
    pub blocked: u64,
    pub observed_outcomes: u64,
    pub confounded: u64,
    pub continuations: u64,
    pub plan_successes: u64,
    pub plan_failures: u64,
    pub awake_seconds: f64,
    pub protected_seconds: f64,
    pub travelled_height_units: f64,
    pub learning_updates: u64,
    pub last_prediction_error: f32,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GroundedMemory {
    pub schema_version: u32,
    pub clock: f64,
    pub next_event: u64,
    pub beliefs: Vec<ObjectBelief>,
    pub episodes: VecDeque<CausalEpisode>,
    pub skills: Vec<SkillRecord>,
    pub programs: Vec<crate::LearnedProgram>,
    pub exercise: Option<Exercise>,
    pub suspended: Vec<Exercise>,
    pub cooldown: f32,
    pub stats: GroundedStats,
    #[serde(skip)]
    pending: Vec<Pending>,
    #[serde(skip)]
    last_pet_position: Option<Vec2>,
}
impl Default for GroundedMemory {
    fn default() -> Self {
        Self {
            schema_version: 1,
            clock: 0.0,
            next_event: 1,
            beliefs: Vec::new(),
            episodes: VecDeque::new(),
            skills: Vec::new(),
            programs: Vec::new(),
            exercise: None,
            suspended: Vec::new(),
            cooldown: 0.0,
            stats: GroundedStats::default(),
            pending: Vec::new(),
            last_pet_position: None,
        }
    }
}
#[derive(Clone, Debug, PartialEq)]
struct Pending {
    event: u64,
    object: ObjectId,
    start: Vec2,
    velocity: Vec2,
    due: f64,
    intervention_at: f64,
}
pub(crate) fn metric(aspect: f32) -> Vec2 {
    Vec2::new(
        if aspect.is_finite() {
            aspect.clamp(0.25, 8.0)
        } else {
            1.0
        },
        1.0,
    )
}
pub(crate) fn point(p: Vec2) -> bool {
    p.is_finite() && p.cmpge(Vec2::ZERO).all() && p.cmple(Vec2::ONE).all()
}
fn unit(x: f32) -> bool {
    x.is_finite() && (0.0..=1.0).contains(&x)
}
impl GroundedMemory {
    /// The host supplies what is perceptually available. Hidden/contained objects
    /// retain a belief; their current hidden coordinates are never copied here.
    pub fn observe(
        &mut self,
        objects: &[WorldObject],
        pet: Vec2,
        aspect: f32,
        dt: f32,
        protected: bool,
    ) {
        if !dt.is_finite() || dt <= 0.0 || !point(pet) {
            return;
        }
        let dt = dt.min(0.25);
        self.clock += f64::from(dt);
        self.cooldown = (self.cooldown - dt).max(0.0);
        if protected {
            self.stats.protected_seconds += f64::from(dt)
        } else {
            self.stats.awake_seconds += f64::from(dt)
        }
        if let Some(old) = self.last_pet_position {
            self.stats.travelled_height_units +=
                f64::from(((pet - old) * metric(aspect)).length().min(0.1));
        }
        self.last_pet_position = Some(pet);
        for b in &mut self.beliefs {
            b.visible = false;
            b.uncertainty = (b.uncertainty + dt * 0.09).min(1.0);
        }
        for o in objects.iter().filter(|o| o.validate().is_ok()) {
            let visible = !matches!(
                o.lifecycle,
                ObjectLifecycle::Consumed | ObjectLifecycle::StoredInDen
            );
            if !visible {
                if let Some(b) = self.beliefs.iter_mut().find(|b| b.id == o.id) {
                    b.container = o.lifecycle == ObjectLifecycle::StoredInDen;
                }
                continue;
            }
            if !self.beliefs.iter().any(|b| b.id == o.id) {
                if self.beliefs.len() == MAX_BELIEFS {
                    let oldest = self
                        .beliefs
                        .iter()
                        .enumerate()
                        .min_by(|(_, a), (_, b)| a.last_seen.total_cmp(&b.last_seen))
                        .map(|(i, _)| i)
                        .unwrap_or(0);
                    self.beliefs.remove(oldest);
                }
                self.beliefs.push(ObjectBelief {
                    id: o.id,
                    kind: o.kind,
                    last_position: o.position,
                    velocity: o.velocity,
                    last_seen: self.clock,
                    visible: true,
                    uncertainty: 0.0,
                    container: false,
                    observations: 0,
                    impulse_columns: [Vec2::X * 1.4, Vec2::Y * 1.4],
                    impulse_samples: 0,
                    effect_error: 0.0,
                    motion_bias: Vec2::ZERO,
                    motion_samples: 0,
                });
            }
            let b = self.beliefs.iter_mut().find(|b| b.id == o.id).unwrap();
            b.last_position = o.position;
            b.velocity = o.velocity;
            b.last_seen = self.clock;
            b.visible = true;
            b.container = false;
            b.uncertainty = 0.0;
            b.observations = b.observations.saturating_add(1);
        }
        let pending = std::mem::take(&mut self.pending);
        for p in pending {
            let observed = objects.iter().find(|o| o.id == p.object);
            let interrupted = observed.is_none_or(|o| {
                matches!(
                    o.lifecycle,
                    ObjectLifecycle::GrabbedByUser
                        | ObjectLifecycle::CarriedByPet
                        | ObjectLifecycle::Consumed
                        | ObjectLifecycle::StoredInDen
                ) || o.last_interaction_seconds > p.intervention_at + 0.0001
            });
            if interrupted {
                self.mark(p.event, CausalOutcome::Confounded, None, None);
                self.stats.confounded = self.stats.confounded.saturating_add(1);
            } else if self.clock >= p.due {
                let o = observed.unwrap();
                let duration = (self.clock - (p.due - 0.15)) as f32;
                let delta = (o.position - p.start) * metric(aspect);
                if delta.length() > 0.45 || duration > 0.30 {
                    self.mark(p.event, CausalOutcome::Unobserved, None, None);
                    continue;
                }
                let error = self
                    .episodes
                    .iter()
                    .find(|e| e.id == p.event)
                    .map(|e| {
                        ((o.position - e.predicted) * metric(aspect))
                            .length()
                            .min(1.0)
                    })
                    .unwrap_or(0.0);
                self.mark(
                    p.event,
                    CausalOutcome::Observed,
                    Some(o.position),
                    Some(error),
                );
                self.stats.observed_outcomes = self.stats.observed_outcomes.saturating_add(1);
                self.stats.last_prediction_error = error;
                if let Some(b) = self.beliefs.iter_mut().find(|b| b.id == p.object) {
                    let residual = (delta - p.velocity * duration).clamp_length_max(0.04);
                    b.motion_bias = b.motion_bias.lerp(residual, 0.12).clamp_length_max(0.04);
                    b.motion_samples = b.motion_samples.saturating_add(1);
                }
            } else {
                self.pending.push(p);
            }
        }
    }
    fn mark(
        &mut self,
        event: u64,
        outcome: CausalOutcome,
        after: Option<Vec2>,
        error: Option<f32>,
    ) {
        if let Some(e) = self.episodes.iter_mut().find(|e| e.id == event) {
            e.outcome = outcome;
            e.after = after;
            e.prediction_error = error;
        }
    }
    /// This entry is called AFTER attempting the real command, with observed
    /// pre/post state. Never call it merely because an animation requested force.
    pub fn receipt(
        &mut self,
        command: ObjectCommand,
        before: Option<&WorldObject>,
        after: Option<&WorldObject>,
        episode: u64,
        aspect: f32,
        applied: bool,
    ) -> Option<u64> {
        let (action, id, input) = Intervention::from_command(command)?;
        if !input.is_finite() {
            return None;
        }
        let before = before?;
        if before.id != id || before.validate().is_err() {
            return None;
        }
        // Evidence must refer to the same finite, validated object. A removed
        // morsel is the sole valid absent post-state; all other absence is unknown.
        let valid_after = after.is_some_and(|o| o.id == id && o.validate().is_ok())
            || (action == Intervention::Consume && before.kind == ObjectKind::Morsel && after.is_none());
        let applied = applied && valid_after && before.lifecycle != ObjectLifecycle::GrabbedByUser;
        self.stats.proposals = self.stats.proposals.saturating_add(1);
        if applied {
            self.stats.executed = self.stats.executed.saturating_add(1)
        } else {
            self.stats.blocked = self.stats.blocked.saturating_add(1)
        }
        // Sustaining a grip is not a new event. Still accounted as an executed
        // controller command above; do not fabricate dozens of learning trials.
        if matches!(action, Intervention::Grip)
            && before.lifecycle == ObjectLifecycle::CarriedByPet
            && applied
        {
            return None;
        }
        let event = self.next_event;
        self.next_event = self.next_event.saturating_add(1);
        if applied {
            for p in &self.pending {
                if p.object == id
                    && let Some(e) = self.episodes.iter_mut().find(|e| e.id == p.event)
                {
                    e.outcome = CausalOutcome::Confounded;
                    self.stats.confounded = self.stats.confounded.saturating_add(1);
                }
            }
            self.pending.retain(|p| p.object != id);
        }
        let v = after.map_or(before.velocity, |o| o.velocity);
        let belief = self.beliefs.iter().find(|b| b.id == id);
        let prediction_velocity = match action {
            Intervention::Push => before.velocity + belief.map_or(input * 1.4, |b| b.effect(input)),
            Intervention::Release => input,
            _ => v,
        };
        let predicted = (before.position
            + (prediction_velocity * 0.15 + belief.map_or(Vec2::ZERO, |b| b.motion_bias))
                / metric(aspect))
        .clamp(Vec2::ZERO, Vec2::ONE);
        if self.episodes.len() == MAX_CAUSAL_EPISODES {
            self.episodes.pop_front();
        }
        let wait = applied && matches!(action, Intervention::Push | Intervention::Release);
        self.episodes.push_back(CausalEpisode {
            id: event,
            episode_id: episode,
            object_id: id,
            action,
            before: before.position,
            intended: input,
            predicted,
            after: if wait {
                None
            } else {
                after.map(|o| o.position)
            },
            observed_velocity: v,
            at: self.clock,
            outcome: if !applied {
                CausalOutcome::Blocked
            } else if wait {
                CausalOutcome::AwaitingObservation
            } else {
                CausalOutcome::Observed
            },
            prediction_error: None,
            applied,
        });
        if applied && !wait {
            self.stats.observed_outcomes = self.stats.observed_outcomes.saturating_add(1);
        }
        if wait {
            self.pending.push(Pending {
                event,
                object: id,
                start: before.position,
                velocity: v,
                due: self.clock + 0.15,
                intervention_at: before.last_interaction_seconds,
            });
        }
        if applied
            && action == Intervention::Push
            && input.length() > 0.003
            && v.length() < MAX_OBJECT_SPEED - 0.001
            && let Some(b) = self.beliefs.iter_mut().find(|b| b.id == id)
        {
                let error = v - before.velocity - b.effect(input);
                let rate = 0.22 / (0.002 + input.length_squared());
                b.impulse_columns[0] = (b.impulse_columns[0] + error * input.x * rate)
                    .clamp(Vec2::splat(-20.0), Vec2::splat(20.0));
                b.impulse_columns[1] = (b.impulse_columns[1] + error * input.y * rate)
                    .clamp(Vec2::splat(-20.0), Vec2::splat(20.0));
                b.effect_error += (error.length().min(1.0) - b.effect_error) * 0.15;
                b.impulse_samples = b.impulse_samples.saturating_add(1);
                self.stats.learning_updates = self.stats.learning_updates.saturating_add(1);
        }
        if let Some(x) = &mut self.exercise
            && x.object_id == id && x.awaiting_command {
                if applied && (action != Intervention::Push || input.length() > 0.003) {
                    x.confirmed_interventions = x.confirmed_interventions.saturating_add(1);
                }
                x.last_receipt = Some((event, applied));
                x.awaiting_command = false;
        }
        Some(event)
    }
    pub fn after_native_restart(&mut self) {
        self.pending.clear();
        self.last_pet_position = None;
        for b in &mut self.beliefs {
            b.visible = false;
            b.uncertainty = b.uncertainty.max(0.4);
        }
        for e in &mut self.episodes {
            if e.outcome == CausalOutcome::AwaitingObservation {
                e.outcome = CausalOutcome::Unobserved;
            }
        }
        if let Some(mut x) = self.exercise.take() {
            x.prepare_resume();
            if self.suspended.len() < 4 {
                self.suspended.push(x);
            }
        }
    }
    pub fn valid(&self) -> bool {
        self.schema_version == 1
            && self.clock.is_finite()
            && self.clock >= 0.0
            && self.next_event > 0
            && self.beliefs.len() <= MAX_BELIEFS
            && self
                .beliefs
                .iter()
                .enumerate()
                .all(|(i, b)| b.valid() && !self.beliefs[..i].iter().any(|a| a.id == b.id))
            && self.episodes.len() <= MAX_CAUSAL_EPISODES
            && self.episodes.iter().all(|e| {
                e.id > 0
                    && e.id < self.next_event
                    && e.object_id > 0
                    && point(e.before)
                    && point(e.predicted)
                    && e.intended.is_finite()
                    && e.observed_velocity.is_finite()
                    && e.at.is_finite()
                    && e.at >= 0.0
                    && e.after.is_none_or(point)
                    && e.prediction_error.is_none_or(unit)
            })
            && self.cooldown.is_finite()
            && (0.0..=300.0).contains(&self.cooldown)
            && self.skills.len() <= 30
            && self.skills.iter().all(SkillRecord::valid)
            && self.programs.len() <= 12
            && self.programs.iter().all(crate::LearnedProgram::valid)
            && self.exercise.as_ref().is_none_or(Exercise::valid)
            && self.suspended.len() <= 4
            && self.suspended.iter().all(Exercise::valid)
            && [
                self.stats.awake_seconds,
                self.stats.protected_seconds,
                self.stats.travelled_height_units,
            ]
            .iter()
            .all(|x| x.is_finite() && *x >= 0.0)
            && unit(self.stats.last_prediction_error)
    }
}
