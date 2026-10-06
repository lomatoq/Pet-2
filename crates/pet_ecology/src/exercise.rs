//! Composable, bounded exercises run INSIDE the existing episode director.
//! Primitives declare measurable completion predicates; a timer is not success.
use crate::grounded_memory::{GroundedMemory, metric, point};
use crate::{EcologyBehaviorFrame, ObjectCommand, ObjectId, ObjectLifecycle, WorldObject};
use glam::Vec2;
use lifecore::{BodyIntent, InteractionTarget, LocomotionMode, PoseIntent};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GameFamily {
    Roll,
    Stop,
    Shuttle,
    Fetch,
    OrbitInspect,
    Rebound,
}
impl GameFamily {
    pub const ALL: [Self; 6] = [
        Self::Roll,
        Self::Stop,
        Self::Shuttle,
        Self::Fetch,
        Self::OrbitInspect,
        Self::Rebound,
    ];
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TrialEstimate {
    pub successes: u32,
    pub attempts: u32,
    pub error_ema: f32,
}
impl TrialEstimate {
    pub fn lower_bound(&self) -> f32 {
        let n = self.attempts as f32;
        if n == 0.0 {
            return 0.0;
        }
        let p = self.successes as f32 / n;
        let z = 1.64_f32;
        ((p + z * z / (2.0 * n) - z * (p * (1.0 - p) / n + z * z / (4.0 * n * n)).sqrt())
            / (1.0 + z * z / n))
            .max(0.0)
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SkillRecord {
    pub family: GameFamily,
    pub level: u8,
    pub trials: [TrialEstimate; 5],
    pub calibration: [f32; 5],
    pub last_practiced: f64,
    pub last_episode: u64,
}
impl SkillRecord {
    pub fn valid(&self) -> bool {
        self.level < 5
            && self
                .calibration
                .iter()
                .all(|x| x.is_finite() && (0.7..=2.5).contains(x))
            && self.last_practiced.is_finite()
            && self.last_practiced >= 0.0
            && self.trials.iter().all(|t| {
                t.successes <= t.attempts
                    && t.error_ema.is_finite()
                    && (0.0..=1.0).contains(&t.error_ema)
            })
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Primitive {
    Inspect,
    Approach,
    Push,
    Brake,
    Grip,
    Carry,
    Release,
    ObserveMotion,
    ObserveRebound,
    WaitStill,
    Visit,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ExerciseStep {
    pub primitive: Primitive,
    pub target: Vec2,
    pub tolerance: f32,
    pub deadline: f32,
    pub speed: f32,
}
impl ExerciseStep {
    fn new(primitive: Primitive, target: Vec2, tolerance: f32, deadline: f32, speed: f32) -> Self {
        Self {
            primitive,
            target,
            tolerance,
            deadline,
            speed,
        }
    }
    fn valid(&self) -> bool {
        point(self.target)
            && self.tolerance.is_finite()
            && (0.005..=0.3).contains(&self.tolerance)
            && self.deadline.is_finite()
            && (0.1..=40.0).contains(&self.deadline)
            && self.speed.is_finite()
            && (0.0..=0.8).contains(&self.speed)
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Exercise {
    pub episode_id: u64,
    pub object_id: ObjectId,
    pub family: GameFamily,
    pub level: u8,
    pub steps: Vec<ExerciseStep>,
    pub index: usize,
    pub elapsed: f32,
    pub step_seconds: f32,
    pub started_at: f64,
    pub expires_at: f64,
    pub awaiting_command: bool,
    pub last_receipt: Option<(u64, bool)>,
    pub executed_steps: u32,
    #[serde(default)]
    pub confirmed_interventions: u32,
    pub start_position: Vec2,
    pub reference_origin: Vec2,
    pub reference_aspect: f32,
    pub held_seconds: f32,
    pub revalidate: bool,
    pub retries: u8,
    pub replans: u8,
    pub launch_velocity: Vec2,
    #[serde(default)]
    pub rebound_seen: bool,
    pub last_error: f32,
}
impl Exercise {
    pub fn valid(&self) -> bool {
        self.object_id != 0
            && self.episode_id != 0
            && self.level < 5
            && !self.steps.is_empty()
            && self.steps.len() <= 24
            && self.index <= self.steps.len()
            && self.steps.iter().all(ExerciseStep::valid)
            && [
                self.elapsed,
                self.step_seconds,
                self.held_seconds,
                self.last_error,
            ]
            .iter()
            .all(|x| x.is_finite() && *x >= 0.0)
            && self.started_at.is_finite()
            && self.started_at >= 0.0
            && self.expires_at.is_finite()
            && self.expires_at >= self.started_at
            && point(self.start_position)
            && point(self.reference_origin)
            && self.reference_aspect.is_finite()
            && (0.25..=8.0).contains(&self.reference_aspect)
            && self.retries <= 3
            && self.replans <= 3
            && self.launch_velocity.is_finite()
    }
    pub fn prepare_resume(&mut self) {
        self.revalidate = true;
        self.awaiting_command = false;
        self.last_receipt = None;
        self.step_seconds = 0.0;
        self.held_seconds = 0.0;
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExerciseStatus {
    Running,
    Succeeded,
    Failed,
}
pub struct ExerciseOutput {
    pub intent: BodyIntent,
    pub command: Option<ObjectCommand>,
    pub status: ExerciseStatus,
    pub progress: f32,
}

impl GroundedMemory {
    pub fn suspend_exercise(&mut self, explicit_refusal: bool) {
        if explicit_refusal {
            self.exercise = None;
            self.suspended.clear();
            self.cooldown = self.cooldown.max(45.0);
            return;
        }
        if let Some(mut x) = self.exercise.take() {
            x.prepare_resume();
            if self.suspended.len() == 4 {
                self.suspended.remove(0);
            }
            self.suspended.push(x);
        }
    }
    /// Authored JSON can compose the same safe primitives in any order. Plans
    /// cannot contain shell commands, clicks, arbitrary code or unbounded loops.
    pub fn request_exercise(&mut self, mut plan: Exercise) -> Result<(), &'static str> {
        if !plan.valid() {
            return Err("invalid or oversized exercise");
        }
        plan.index = 0;
        plan.elapsed = 0.0;
        plan.executed_steps = 0;
        plan.confirmed_interventions = 0;
        plan.retries = 0;
        plan.replans = 0;
        plan.last_error = 0.0;
        plan.launch_velocity = Vec2::ZERO;
        plan.rebound_seen = false;
        plan.prepare_resume();
        plan.started_at = self.clock;
        plan.expires_at = self.clock + 120.0;
        self.suspend_exercise(false);
        self.suspended.push(plan);
        if self.suspended.len() > 4 {
            self.suspended.remove(0);
        }
        Ok(())
    }
    fn record_trial(&mut self, x: &Exercise, success: bool) {
        if success && (x.executed_steps == 0
            || (x.family != GameFamily::OrbitInspect && x.confirmed_interventions == 0)) {
            // Reaching an already satisfied predicate is not new motor mastery.
            return;
        }
        if !self.skills.iter().any(|s| s.family == x.family) {
            self.skills.push(SkillRecord {
                family: x.family,
                level: 0,
                trials: [TrialEstimate::default(); 5],
                calibration: [1.0; 5],
                last_practiced: 0.0,
                last_episode: 0,
            });
        }
        let s = self
            .skills
            .iter_mut()
            .find(|s| s.family == x.family)
            .unwrap();
        if s.last_episode == x.episode_id {
            return;
        }
        s.last_episode = x.episode_id;
        s.last_practiced = self.clock;
        if success && x.family == GameFamily::Roll {
            if let Some(push) = x.steps.iter().find(|p| p.primitive == Primitive::Push) {
                let nominal = 0.14 + f32::from(x.level) * 0.035;
                let measured = (push.speed / nominal).clamp(0.7, 2.5);
                s.calibration[x.level as usize] +=
                    (measured - s.calibration[x.level as usize]) * 0.35;
            }
        }
        let t = &mut s.trials[usize::from(x.level)];
        t.attempts = t.attempts.saturating_add(1);
        if success {
            t.successes = t.successes.saturating_add(1)
        }
        t.error_ema += (x.last_error.min(1.0) - t.error_ema) * 0.2;
        if t.attempts >= 8 && t.lower_bound() >= 0.55 && s.level == x.level && s.level < 4 {
            s.level += 1;
        }
        // Reduced confidence after a context change allows relearning. Lifetime
        // counts remain diagnostics; a repeatedly failing hard level steps down.
        if !success
            && x.last_error > 0.3
            && s.level > 0
            && t.attempts >= 4
            && u64::from(t.successes) * 3 < u64::from(t.attempts)
        {
            s.level -= 1;
        }
        if success {
            self.remember_program(x);
        }
        if success {
            self.stats.plan_successes = self.stats.plan_successes.saturating_add(1)
        } else {
            self.stats.plan_failures = self.stats.plan_failures.saturating_add(1)
        }
    }
    pub fn curriculum(
        &self,
        episode: u64,
        object: ObjectId,
        origin: Vec2,
        home: Vec2,
        aspect: f32,
        contact: bool,
        velocity: Vec2,
    ) -> Exercise {
        if let Some(recalled) = self.recall_program(episode, object, origin, aspect) {
            return recalled;
        }
        let family = GameFamily::ALL
            .into_iter()
            .max_by(|a, b| {
                let score = |family: GameFamily| {
                    let s = self.skills.iter().find(|s| s.family == family);
                    let (n, value, last) = s
                        .map(|s| {
                            (
                                s.trials[s.level as usize].attempts,
                                s.trials[s.level as usize].lower_bound(),
                                s.last_practiced,
                            )
                        })
                        .unwrap_or((0, 0.0, -100.0));
                    let fit = match family {
                        GameFamily::Stop => {
                            if velocity.length() > 0.12 {
                                0.4
                            } else {
                                -0.45
                            }
                        }
                        GameFamily::Fetch => {
                            if contact {
                                0.15
                            } else {
                                0.0
                            }
                        }
                        GameFamily::Roll => 0.16,
                        GameFamily::Rebound => {
                            if origin.x.min(1.0 - origin.x) * aspect < 0.14 {
                                0.24
                            } else {
                                -1.0
                            }
                        }
                        _ => 0.0,
                    };
                    0.4 / (1.0 + n as f32).sqrt()
                        + value * 0.12
                        + ((self.clock - last) / 90.0).clamp(0.0, 1.0) as f32 * 0.25
                        + fit
                };
                score(*a).total_cmp(&score(*b))
            })
            .unwrap_or(GameFamily::Roll);
        let level = self
            .skills
            .iter()
            .find(|s| s.family == family)
            .map_or(0, |s| s.level);
        let mut plan = Self::compile_exercise(
            episode, object, family, level, origin, home, aspect, self.clock,
        );
        if family == GameFamily::Roll {
            let gain = self
                .skills
                .iter()
                .find(|s| s.family == family)
                .map_or(1.0, |s| s.calibration[level as usize]);
            for step in &mut plan.steps {
                if step.primitive == Primitive::Push {
                    step.speed = (step.speed * gain).min(0.55);
                }
            }
        }
        plan
    }
    pub fn compile_exercise(
        episode: u64,
        object: ObjectId,
        family: GameFamily,
        level: u8,
        origin: Vec2,
        home: Vec2,
        aspect: f32,
        now: f64,
    ) -> Exercise {
        let level = level.min(4);
        let scale = metric(aspect);
        let side = if origin.x < 0.5 { 1.0 } else { -1.0 };
        let distance = 0.07 + f32::from(level) * 0.028;
        let target = (origin + Vec2::new(side * distance, 0.0) / scale)
            .clamp(Vec2::splat(0.04), Vec2::splat(0.96));
        let tolerance = (0.052 - f32::from(level) * 0.006).max(0.018);
        let mut steps = vec![
            ExerciseStep::new(Primitive::Inspect, origin, 0.03, 2.0, 0.0),
            ExerciseStep::new(Primitive::Approach, origin, 0.055, 20.0, 0.28),
        ];
        match family {
            GameFamily::Rebound => {
                let wall = Vec2::new(if origin.x < 0.5 { 0.0 } else { 1.0 }, origin.y);
                steps.push(ExerciseStep::new(
                    Primitive::Push,
                    wall,
                    tolerance,
                    8.0,
                    0.22,
                ));
                steps.push(ExerciseStep::new(
                    Primitive::ObserveRebound,
                    origin,
                    tolerance,
                    5.0,
                    0.0,
                ));
            }
            GameFamily::Roll => {
                steps.push(ExerciseStep::new(
                    Primitive::Push,
                    target,
                    tolerance,
                    6.0,
                    0.14 + f32::from(level) * 0.035,
                ));
                steps.push(ExerciseStep::new(
                    Primitive::ObserveMotion,
                    target,
                    tolerance,
                    4.0,
                    0.0,
                ));
            }
            GameFamily::Stop => {
                steps.push(ExerciseStep::new(Primitive::Brake, origin, 0.05, 8.0, 0.4));
                steps.push(ExerciseStep::new(
                    Primitive::WaitStill,
                    origin,
                    0.065,
                    2.0,
                    0.0,
                ));
            }
            GameFamily::Shuttle | GameFamily::Fetch => {
                let destination = if family == GameFamily::Fetch {
                    home
                } else {
                    target
                };
                steps.push(ExerciseStep::new(Primitive::Grip, origin, 0.055, 6.0, 0.25));
                steps.push(ExerciseStep::new(
                    Primitive::Carry,
                    destination,
                    tolerance,
                    25.0,
                    0.22,
                ));
                steps.push(ExerciseStep::new(
                    Primitive::Release,
                    destination,
                    tolerance,
                    3.0,
                    0.0,
                ));
                steps.push(ExerciseStep::new(
                    Primitive::WaitStill,
                    destination,
                    tolerance + 0.025,
                    4.0,
                    0.0,
                ));
                if level > 1 && family == GameFamily::Shuttle {
                    steps.push(ExerciseStep::new(
                        Primitive::Approach,
                        destination,
                        0.055,
                        15.0,
                        0.28,
                    ));
                    steps.push(ExerciseStep::new(
                        Primitive::Grip,
                        destination,
                        0.055,
                        5.0,
                        0.2,
                    ));
                    steps.push(ExerciseStep::new(
                        Primitive::Carry,
                        origin,
                        tolerance,
                        25.0,
                        0.24,
                    ));
                    steps.push(ExerciseStep::new(
                        Primitive::Release,
                        origin,
                        tolerance,
                        3.0,
                        0.0,
                    ));
                }
            }
            GameFamily::OrbitInspect => {
                for i in 0..(2 + usize::from(level)) {
                    let angle = (i as f32 + 0.5) * 2.4;
                    let p = (origin
                        + Vec2::new(angle.cos(), angle.sin()) * (0.09 + f32::from(level) * 0.01)
                            / scale)
                        .clamp(Vec2::splat(0.04), Vec2::splat(0.96));
                    steps.push(ExerciseStep::new(Primitive::Visit, p, 0.035, 10.0, 0.2));
                }
            }
        }
        Exercise {
            episode_id: episode,
            object_id: object,
            family,
            level,
            steps,
            index: 0,
            elapsed: 0.0,
            step_seconds: 0.0,
            started_at: now,
            expires_at: now + 120.0,
            awaiting_command: false,
            last_receipt: None,
            executed_steps: 0,
            confirmed_interventions: 0,
            start_position: origin,
            reference_origin: origin,
            reference_aspect: aspect.clamp(0.25, 8.0),
            held_seconds: 0.0,
            revalidate: false,
            retries: 0,
            replans: 0,
            launch_velocity: Vec2::ZERO,
            rebound_seen: false,
            last_error: 0.0,
        }
    }
    pub fn run_exercise(
        &mut self,
        frame: EcologyBehaviorFrame,
        episode: u64,
        orb: &WorldObject,
        home: Vec2,
        base: BodyIntent,
        dt: f32,
    ) -> Option<ExerciseOutput> {
        if frame.focus_mode
            || frame.sleeping
            || frame.window_pressure >= 0.22
            || orb.lifecycle == ObjectLifecycle::GrabbedByUser
        {
            self.suspend_exercise(false);
            return None;
        }
        if orb.lifecycle == ObjectLifecycle::StoredInDen
            && self.exercise.as_ref().is_some_and(|x| {
                x.object_id == orb.id
                    && x.family == GameFamily::Fetch
                    && !x.revalidate
                    && x.confirmed_interventions >= 1
                    && x.executed_steps >= 2
                    && x.index + 1 == x.steps.len()
                    && x.steps.get(x.index).is_some_and(|step|
                        step.primitive == Primitive::WaitStill
                        && ((step.target - home) * metric(frame.desktop_aspect)).length() <= step.tolerance)
            })
        {
            let mut x = self.exercise.take().unwrap();
            x.index = x.steps.len();
            self.record_trial(&x, true);
            self.cooldown = 12.0;
            return Some(ExerciseOutput {
                intent: base,
                command: None,
                status: ExerciseStatus::Succeeded,
                progress: 1.0,
            });
        }
        let belief = self
            .beliefs
            .iter()
            .find(|b| b.id == orb.id && b.visible && self.clock - b.last_seen < 0.2)?
            .clone();
        if self.exercise.is_none() {
            if self.cooldown > 0.0 {
                return None;
            }
            self.suspended
                .retain(|x| self.clock < x.expires_at && x.valid());
            if let Some(index) = self.suspended.iter().rposition(|x| x.object_id == orb.id) {
                let mut x = self.suspended.remove(index);
                x.episode_id = episode;
                x.prepare_resume();
                self.exercise = Some(x);
                self.stats.continuations = self.stats.continuations.saturating_add(1);
            } else {
                self.exercise = Some(self.curriculum(
                    episode,
                    orb.id,
                    belief.last_position,
                    home,
                    frame.desktop_aspect,
                    frame.orb_physical.contact,
                    belief.velocity,
                ));
            }
        }
        let mut x = self.exercise.take()?;
        if x.object_id != orb.id || !x.valid() {
            return None;
        }
        let dt = if dt.is_finite() {
            dt.clamp(0.0, 0.25)
        } else {
            0.0
        };
        x.elapsed += dt;
        x.step_seconds += dt;
        let scale = metric(frame.desktop_aspect);
        let object = belief.last_position;
        let mut intent = base;
        intent.gaze_target = Some(object);
        intent.interaction_target = Some(InteractionTarget::ProceduralOrb);
        intent.pose = PoseIntent::Playful;
        intent.locomotion = LocomotionMode::Arrive;
        intent.target_surface = None;
        intent.target_position = frame.pet_position;
        intent.desired_speed = 0.0;
        let mut output = ExerciseOutput {
            intent,
            command: None,
            status: ExerciseStatus::Running,
            progress: x.index as f32 / x.steps.len() as f32,
        };
        if self.clock > x.expires_at || x.elapsed > 100.0 {
            x.last_error = 1.0;
            self.record_trial(&x, false);
            self.cooldown = 20.0;
            output.status = ExerciseStatus::Failed;
            return Some(output);
        }
        if x.revalidate {
            if x.step_seconds < 0.6 {
                self.exercise = Some(x);
                return Some(output);
            }
            x.revalidate = false;
            x.step_seconds = 0.0;
            // Never silently repeat an in-flight one-shot after restart.
            if x.index < x.steps.len()
                && matches!(
                    x.steps[x.index].primitive,
                    Primitive::Push | Primitive::Release
                )
            {
                x.steps.insert(
                    x.index,
                    ExerciseStep::new(Primitive::Approach, object, 0.055, 12.0, 0.25),
                );
                if x.steps.len() > 24 {
                    x.last_error = 1.0;
                    self.record_trial(&x, false);
                    self.cooldown = 20.0;
                    output.status = ExerciseStatus::Failed;
                    return Some(output);
                }
            }
        }
        if x.index == x.steps.len() {
            let success = x.executed_steps > 0;
            self.record_trial(&x, success);
            self.cooldown = 12.0;
            output.status = if success {
                ExerciseStatus::Succeeded
            } else {
                ExerciseStatus::Failed
            };
            return Some(output);
        }
        let step = x.steps[x.index].clone();
        let contact = frame.orb_physical.contact;
        let held = orb.lifecycle == ObjectLifecycle::CarriedByPet;
        let distance = ((step.target - object) * scale).length();
        let own_distance = ((step.target - frame.pet_position) * scale).length();
        let mut done = false;
        match step.primitive {
            Primitive::Inspect => {
                done = x.step_seconds >= 0.45 && belief.observations >= 3;
                if done {
                    x.executed_steps = x.executed_steps.saturating_add(1);
                }
            }
            Primitive::Approach => {
                output.intent.target_position =
                    belief.predict_position(self.clock + 0.12, frame.desktop_aspect);
                output.intent.desired_speed =
                    step.speed.max((belief.velocity.length() + 0.12).min(0.65));
                output.intent.locomotion = LocomotionMode::Seek;
                done = contact || held;
            }
            Primitive::Visit => {
                output.intent.target_position = step.target;
                output.intent.desired_speed = step.speed;
                done = own_distance < step.tolerance;
                if done {
                    x.executed_steps = x.executed_steps.saturating_add(1);
                }
            }
            Primitive::Push | Primitive::Brake => {
                output.intent.target_position = object;
                output.intent.desired_speed = 0.28;
                if let Some((event, accepted)) = x.last_receipt.take() {
                    done = accepted;
                    if accepted {
                        x.executed_steps += 1;
                        x.launch_velocity = self
                            .episodes
                            .iter()
                            .find(|e| e.id == event)
                            .map_or(Vec2::ZERO, |e| e.observed_velocity);
                        if step.primitive == Primitive::Brake {
                            done = belief.velocity.length() < 0.06;
                            if done && x.index + 1 < x.steps.len() {
                                x.steps[x.index + 1].target = object;
                            }
                        }
                    } else {
                        x.retries += 1;
                    }
                }
                if !done && contact && !x.awaiting_command && x.step_seconds > 0.18 {
                    let desired = if step.primitive == Primitive::Brake {
                        Vec2::ZERO
                    } else {
                        ((step.target - object) * scale).normalize_or(Vec2::X) * step.speed
                    };
                    let impulse = belief.impulse_for_velocity(desired, belief.velocity);
                    output.command = Some(ObjectCommand::ApplyImpulse {
                        object_id: orb.id,
                        impulse,
                    });
                    x.awaiting_command = true;
                    x.start_position = object;
                }
            }
            Primitive::Grip => {
                output.intent.target_position = object;
                output.intent.desired_speed = 0.22;
                done = held;
                if contact && !held && !x.awaiting_command {
                    output.command = Some(ObjectCommand::MoveToward {
                        object_id: orb.id,
                        target: frame.orb_physical.socket_position,
                        speed: 2.0,
                    });
                    x.awaiting_command = true;
                }
                if let Some((_, accepted)) = x.last_receipt.take() {
                    if accepted {
                        x.executed_steps += 1
                    } else {
                        x.retries += 1;
                    }
                }
            }
            Primitive::Carry => {
                if held {
                    output.intent.target_position = (step.target
                        - (frame.orb_physical.socket_position - frame.pet_position))
                        .clamp(Vec2::ZERO, Vec2::ONE);
                    output.intent.desired_speed = step.speed;
                    output.command = Some(ObjectCommand::MoveToward {
                        object_id: orb.id,
                        target: frame.orb_physical.socket_position,
                        speed: 2.5,
                    });
                    done = distance < step.tolerance;
                } else {
                    x.last_error = 1.0;
                    x.step_seconds = step.deadline + 0.01;
                }
            }
            Primitive::Release => {
                if let Some((_, accepted)) = x.last_receipt.take() {
                    done = accepted && !held;
                    if accepted {
                        x.executed_steps += 1
                    } else {
                        x.retries += 1;
                    }
                }
                if held && !x.awaiting_command {
                    output.command = Some(ObjectCommand::Release {
                        object_id: orb.id,
                        velocity: Vec2::ZERO,
                    });
                    x.awaiting_command = true;
                } else if !held && !x.awaiting_command {
                    done = x.executed_steps > 0;
                }
            }
            Primitive::ObserveRebound => {
                // A short real bounce may have settled by the end of the
                // presentation hold. Retain observed reversal, not predicted contact.
                x.rebound_seen |= x.confirmed_interventions > 0
                    && x.launch_velocity.x.abs() > 0.02
                    && belief.velocity.x * x.launch_velocity.x < -0.0002;
                done = x.step_seconds >= 0.15 && x.rebound_seen;
                x.last_error = if done { 0.0 } else { 0.5 };
            }
            Primitive::ObserveMotion => {
                output.intent.target_position = frame.pet_position;
                output.intent.desired_speed = 0.0;
                let displacement = ((object - x.start_position) * scale).length();
                x.last_error = (distance / (step.tolerance * 3.0)).clamp(0.0, 1.0);
                done = x.step_seconds >= 0.20
                    && distance <= step.tolerance
                    && displacement > 0.006
                    && x.executed_steps > 0;
            }
            Primitive::WaitStill => {
                if !held && distance <= step.tolerance && belief.velocity.length() < 0.055 {
                    x.held_seconds += dt
                } else {
                    x.held_seconds = 0.0
                }
                done = x.held_seconds >= 0.3 && x.executed_steps > 0;
                x.last_error = (distance / (step.tolerance * 3.0)).clamp(0.0, 1.0);
            }
        }
        if !done
            && matches!(step.primitive, Primitive::ObserveMotion | Primitive::ObserveRebound)
            && x.step_seconds >= 0.55
            && belief.velocity.length() < 0.035
            && x.replans < 3
            && x.index > 0
            && x.steps[x.index - 1].primitive == Primitive::Push
        {
            // A measured shortfall changes the next attempt, not the goal or its
            // success criterion. Approach/contact is rechecked before every tap.
            x.replans += 1;
            x.index -= 1;
            x.steps[x.index].speed = (x.steps[x.index].speed * 1.5).min(0.55);
            x.step_seconds = 0.0;
            x.awaiting_command = false;
            x.last_receipt = None;
            self.exercise = Some(x);
            return Some(output);
        }
        if done {
            x.index += 1;
            x.step_seconds = 0.0;
            x.held_seconds = 0.0;
            x.awaiting_command = false;
            x.last_receipt = None;
            x.retries = 0;
        } else if x.step_seconds > step.deadline || x.retries >= 3 {
            x.last_error = x.last_error.max(0.5);
            self.record_trial(&x, false);
            self.cooldown = 18.0;
            output.status = ExerciseStatus::Failed;
            return Some(output);
        }
        if matches!(
            step.primitive,
            Primitive::Carry | Primitive::Visit | Primitive::Push
        ) {
            output.intent.gaze_target = Some(if x.step_seconds < 0.18 {
                step.target
            } else {
                object
            });
            if x.step_seconds < 0.14 {
                output.intent.desired_speed = 0.0;
                output.intent.target_position = frame.pet_position;
            }
        }
        self.exercise = Some(x);
        Some(output)
    }
}
