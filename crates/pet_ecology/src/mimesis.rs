use glam::Vec2;
use serde::{Deserialize, Serialize};

use crate::EcologyError;

pub const TRAJECTORY_SAMPLES: usize = 32;
pub const SPEED_SAMPLES: usize = 16;
pub const MAX_LEARNED_SKILLS: usize = 16;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ActionSignature {
    pub path: [Vec2; TRAJECTORY_SAMPLES],
    pub speed: [f32; SPEED_SAMPLES],
    pub duration_seconds: f32,
    pub spin_turns: f32,
    pub loopness: f32,
    pub closure_error: f32,
    pub scale_invariant: bool,
}

impl Default for ActionSignature {
    fn default() -> Self {
        Self {
            path: [Vec2::ZERO; TRAJECTORY_SAMPLES],
            speed: [0.0; SPEED_SAMPLES],
            duration_seconds: 0.0,
            spin_turns: 0.0,
            loopness: 0.0,
            closure_error: 0.0,
            scale_invariant: true,
        }
    }
}

impl ActionSignature {
    #[must_use]
    pub fn is_valid(&self) -> bool {
        self.path.iter().all(|point| point.is_finite())
            && self
                .speed
                .iter()
                .all(|value| value.is_finite() && (0.0..=4.0).contains(value))
            && self.duration_seconds.is_finite()
            && (0.05..=6.0).contains(&self.duration_seconds)
            && self.spin_turns.is_finite()
            && (-8.0..=8.0).contains(&self.spin_turns)
            && self.loopness.is_finite()
            && (0.0..=1.0).contains(&self.loopness)
            && self.closure_error.is_finite()
            && (0.0..=2.0).contains(&self.closure_error)
    }

    /// Constructs the shared observation/execution representation from a
    /// bounded trace. Arc-length resampling makes it insensitive to event rate.
    #[must_use]
    pub fn from_trace(points: &[Vec2], duration_seconds: f32) -> Option<Self> {
        if points.len() < 2
            || points.iter().any(|point| !point.is_finite())
            || !duration_seconds.is_finite()
            || !(0.05..=6.0).contains(&duration_seconds)
        {
            return None;
        }
        let mut cumulative = Vec::with_capacity(points.len());
        cumulative.push(0.0_f32);
        for pair in points.windows(2) {
            cumulative.push(cumulative.last().copied().unwrap_or(0.0) + pair[0].distance(pair[1]));
        }
        let total = cumulative.last().copied().unwrap_or(0.0);
        if total < 1.0e-5 || !total.is_finite() {
            return None;
        }
        let mut path = [Vec2::ZERO; TRAJECTORY_SAMPLES];
        let mut segment = 0;
        for (index, output) in path.iter_mut().enumerate() {
            let target = total * index as f32 / (TRAJECTORY_SAMPLES - 1) as f32;
            while segment + 1 < cumulative.len() - 1 && cumulative[segment + 1] < target {
                segment += 1;
            }
            let start_distance = cumulative[segment];
            let end_distance = cumulative[segment + 1];
            let alpha = if end_distance > start_distance {
                (target - start_distance) / (end_distance - start_distance)
            } else {
                0.0
            };
            *output = points[segment].lerp(points[segment + 1], alpha.clamp(0.0, 1.0));
        }
        let origin = path[0];
        for point in &mut path {
            *point -= origin;
        }
        let extent = path
            .iter()
            .fold(Vec2::ZERO, |extent, point| extent.max(point.abs()));
        let scale = extent.max_element().max(1.0e-4);
        for point in &mut path {
            *point /= scale;
        }
        let mut speed = [0.0; SPEED_SAMPLES];
        for (index, value) in speed.iter_mut().enumerate() {
            let path_index = index * (TRAJECTORY_SAMPLES - 1) / (SPEED_SAMPLES - 1);
            let previous = path_index.saturating_sub(1);
            let next = (path_index + 1).min(TRAJECTORY_SAMPLES - 1);
            let local_distance = path[previous].distance(path[next]);
            let local_dt = duration_seconds * (next - previous).max(1) as f32
                / (TRAJECTORY_SAMPLES - 1) as f32;
            *value = (local_distance / local_dt.max(1.0e-4)).clamp(0.0, 4.0);
        }
        let mut signed_turn = 0.0_f32;
        for triple in path.windows(3) {
            let a = triple[1] - triple[0];
            let b = triple[2] - triple[1];
            if a.length_squared() > 1.0e-8 && b.length_squared() > 1.0e-8 {
                signed_turn += a.perp_dot(b).atan2(a.dot(b));
            }
        }
        let closure_error = path[TRAJECTORY_SAMPLES - 1].length().clamp(0.0, 2.0);
        Some(Self {
            path,
            speed,
            duration_seconds,
            spin_turns: (signed_turn / std::f32::consts::TAU).clamp(-8.0, 8.0),
            loopness: (1.0 - closure_error).clamp(0.0, 1.0),
            closure_error,
            scale_invariant: true,
        })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct LearnedSkill {
    pub id: u64,
    pub prototype: ActionSignature,
    pub competence: f32,
    pub uncertainty: f32,
    pub motor_error_ema: f32,
    pub social_value: f32,
    pub demonstrations: u32,
    pub attempts: u32,
    pub successes: u32,
    pub last_used_seconds: f64,
}

impl LearnedSkill {
    #[must_use]
    pub fn is_valid(&self) -> bool {
        self.id != 0
            && self.prototype.is_valid()
            && [self.competence, self.uncertainty, self.motor_error_ema]
                .into_iter()
                .all(|value| value.is_finite() && (0.0..=1.0).contains(&value))
            && self.social_value.is_finite()
            && (-1.0..=1.0).contains(&self.social_value)
            && self.successes <= self.attempts
            && self.last_used_seconds.is_finite()
            && self.last_used_seconds >= 0.0
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct MimesisLibrary {
    pub skills: Vec<LearnedSkill>,
    pub next_skill_id: u64,
}

impl MimesisLibrary {
    pub fn validate(&self) -> Result<(), EcologyError> {
        let valid = self.skills.len() <= MAX_LEARNED_SKILLS
            && self.next_skill_id != 0
            && self.skills.iter().all(LearnedSkill::is_valid)
            && self.skills.iter().enumerate().all(|(index, skill)| {
                self.skills[..index]
                    .iter()
                    .all(|other| other.id != skill.id)
            })
            && self
                .skills
                .iter()
                .all(|skill| skill.id != self.next_skill_id);
        if valid {
            Ok(())
        } else {
            Err(EcologyError::InvalidSkills)
        }
    }
}

#[must_use]
pub fn signature_distance(a: &ActionSignature, b: &ActionSignature) -> f32 {
    if !a.is_valid() || !b.is_valid() {
        return 1.0;
    }
    let path = a
        .path
        .iter()
        .zip(&b.path)
        .map(|(left, right)| left.distance(*right))
        .sum::<f32>()
        / TRAJECTORY_SAMPLES as f32;
    let speed = a
        .speed
        .iter()
        .zip(&b.speed)
        .map(|(left, right)| (left - right).abs())
        .sum::<f32>()
        / SPEED_SAMPLES as f32;
    (path * 0.58
        + speed * 0.18
        + (a.loopness - b.loopness).abs() * 0.10
        + (a.closure_error - b.closure_error).abs() * 0.08
        + ((a.spin_turns - b.spin_turns).abs() / 4.0).min(1.0) * 0.06)
        .clamp(0.0, 1.0)
}
