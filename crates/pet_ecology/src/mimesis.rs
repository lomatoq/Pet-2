use glam::Vec2;
use serde::{Deserialize, Serialize};

use crate::EcologyError;

pub const TRAJECTORY_SAMPLES: usize = 32;
pub const SPEED_SAMPLES: usize = 16;
pub const MAX_LEARNED_SKILLS: usize = 16;
pub const MAX_PRACTICE_ATTEMPTS_PER_BOUT: u8 = 3;
pub const MIN_PRACTICE_COOLDOWN_SECONDS: f64 = 20.0;

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

    pub fn observe(
        &mut self,
        signature: ActionSignature,
        timestamp: f64,
    ) -> Result<(u64, bool), EcologyError> {
        if !signature.is_valid() || !timestamp.is_finite() || timestamp < 0.0 {
            return Err(EcologyError::InvalidSkills);
        }
        if let Some((index, _)) = self
            .skills
            .iter()
            .enumerate()
            .map(|(index, skill)| (index, signature_distance(&skill.prototype, &signature)))
            .filter(|(_, distance)| *distance <= 0.16)
            .min_by(|left, right| left.1.total_cmp(&right.1))
        {
            let skill = &mut self.skills[index];
            let blend = (0.34 / (skill.demonstrations.max(1) as f32).sqrt()).clamp(0.08, 0.34);
            blend_signature(&mut skill.prototype, &signature, blend);
            skill.demonstrations = skill.demonstrations.saturating_add(1);
            skill.uncertainty = (skill.uncertainty * 0.82 + 0.18 * 0.20).clamp(0.0, 1.0);
            skill.last_used_seconds = timestamp;
            let id = skill.id;
            self.validate()?;
            return Ok((id, true));
        }

        let id = self.next_skill_id.max(1);
        self.next_skill_id = self.next_skill_id.saturating_add(1).max(1);
        let candidate = LearnedSkill {
            id,
            prototype: signature,
            competence: 0.12,
            uncertainty: 0.72,
            motor_error_ema: 0.78,
            social_value: 0.0,
            demonstrations: 1,
            attempts: 0,
            successes: 0,
            last_used_seconds: timestamp,
        };
        if self.skills.len() < MAX_LEARNED_SKILLS {
            self.skills.push(candidate);
        } else if let Some((replace, _)) = self
            .skills
            .iter()
            .enumerate()
            .map(|(index, skill)| {
                (
                    index,
                    skill.competence * 0.55
                        + skill.social_value.max(0.0) * 0.25
                        + skill.demonstrations.min(8) as f32 / 8.0 * 0.20,
                )
            })
            .min_by(|left, right| left.1.total_cmp(&right.1))
        {
            self.skills[replace] = candidate;
        }
        self.validate()?;
        Ok((id, false))
    }

    pub fn record_attempt(
        &mut self,
        skill_id: u64,
        motor_error: f32,
        timestamp: f64,
        reproduced: Option<&ActionSignature>,
    ) -> Result<(), EcologyError> {
        if !motor_error.is_finite() || !timestamp.is_finite() || timestamp < 0.0 {
            return Err(EcologyError::InvalidSkills);
        }
        let Some(skill) = self.skills.iter_mut().find(|skill| skill.id == skill_id) else {
            return Err(EcologyError::InvalidSkills);
        };
        let error = motor_error.clamp(0.0, 1.0);
        let previous_prediction = skill.motor_error_ema;
        skill.attempts = skill.attempts.saturating_add(1);
        if error <= 0.30 {
            skill.successes = skill.successes.saturating_add(1);
        }
        skill.motor_error_ema = skill.motor_error_ema * 0.78 + error * 0.22;
        skill.competence = (skill.competence * 0.82 + (1.0 - error) * 0.18).clamp(0.0, 1.0);
        skill.uncertainty =
            (skill.uncertainty * 0.84 + (previous_prediction - error).abs() * 0.16).clamp(0.0, 1.0);
        if error <= 0.36
            && let Some(reproduced) = reproduced.filter(|signature| signature.is_valid())
        {
            blend_signature(&mut skill.prototype, reproduced, 0.04);
        }
        skill.last_used_seconds = timestamp;
        self.validate()
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

fn blend_signature(target: &mut ActionSignature, observation: &ActionSignature, amount: f32) {
    let amount = amount.clamp(0.0, 0.25);
    for (target, observation) in target.path.iter_mut().zip(observation.path) {
        *target = target.lerp(observation, amount);
    }
    for (target, observation) in target.speed.iter_mut().zip(observation.speed) {
        *target += (observation - *target) * amount;
    }
    target.duration_seconds += (observation.duration_seconds - target.duration_seconds) * amount;
    target.spin_turns += (observation.spin_turns - target.spin_turns) * amount;
    target.loopness += (observation.loopness - target.loopness) * amount;
    target.closure_error += (observation.closure_error - target.closure_error) * amount;
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MotorErrorComponents {
    pub mean_path_distance: f32,
    pub speed_envelope_error: f32,
    pub curvature_error: f32,
    pub closure_error: f32,
    pub spin_error: f32,
    pub settle_time_error: f32,
    pub collision_penalty: f32,
}

#[must_use]
pub fn motor_error(components: MotorErrorComponents) -> f32 {
    (components.mean_path_distance.clamp(0.0, 1.0) * 0.42
        + components.speed_envelope_error.clamp(0.0, 1.0) * 0.20
        + components.curvature_error.clamp(0.0, 1.0) * 0.16
        + components.closure_error.clamp(0.0, 1.0) * 0.10
        + components.spin_error.clamp(0.0, 1.0) * 0.07
        + components.settle_time_error.clamp(0.0, 1.0) * 0.05
        + components.collision_penalty.clamp(0.0, 1.0))
    .clamp(0.0, 1.0)
}

#[must_use]
pub fn retarget_path(
    signature: &ActionSignature,
    center: Vec2,
    half_extent: Vec2,
) -> Option<[Vec2; TRAJECTORY_SAMPLES]> {
    if !signature.is_valid() || !center.is_finite() || !half_extent.is_finite() {
        return None;
    }
    let safe_center = center.clamp(Vec2::splat(0.08), Vec2::splat(0.92));
    let safe_extent = half_extent.clamp(Vec2::splat(0.025), Vec2::splat(0.28));
    let bounds_minimum = Vec2::splat(0.025);
    let bounds_maximum = Vec2::splat(0.975);
    Some(std::array::from_fn(|index| {
        (safe_center + signature.path[index] * safe_extent).clamp(bounds_minimum, bounds_maximum)
    }))
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RhythmSignature {
    pub intervals: [f32; 8],
    pub beat_count: u8,
    pub tempo_hz: f32,
    pub accent: [f32; 8],
    pub pause_ratio: f32,
}

impl RhythmSignature {
    #[must_use]
    pub fn from_onsets(onsets: &[f64]) -> Option<Self> {
        if !(2..=9).contains(&onsets.len())
            || onsets.iter().any(|time| !time.is_finite())
            || onsets.windows(2).any(|pair| pair[1] <= pair[0])
        {
            return None;
        }
        let mut raw = [0.0_f32; 8];
        let interval_count = onsets.len() - 1;
        for (index, pair) in onsets.windows(2).enumerate() {
            raw[index] = (pair[1] - pair[0]) as f32;
        }
        let mean = raw[..interval_count].iter().sum::<f32>() / interval_count as f32;
        if mean <= 1.0e-4 || !mean.is_finite() {
            return None;
        }
        let mut intervals = [0.0_f32; 8];
        let mut accent = [0.0_f32; 8];
        for index in 0..interval_count {
            intervals[index] = (raw[index] / mean).clamp(0.1, 4.0);
            accent[index] = (1.0 / intervals[index]).clamp(0.0, 1.0);
        }
        let longest = raw[..interval_count]
            .iter()
            .copied()
            .fold(0.0_f32, f32::max);
        Some(Self {
            intervals,
            beat_count: onsets.len() as u8,
            tempo_hz: (1.0 / mean).clamp(0.1, 12.0),
            accent,
            pause_ratio: (longest / mean / 4.0).clamp(0.0, 1.0),
        })
    }

    #[must_use]
    pub fn interval_correlation(self, other: Self) -> f32 {
        let count = usize::from(self.beat_count.min(other.beat_count)).saturating_sub(1);
        if count < 2 {
            return 0.0;
        }
        correlation(&self.intervals[..count], &other.intervals[..count])
    }
}

fn correlation(left: &[f32], right: &[f32]) -> f32 {
    let mean_left = left.iter().sum::<f32>() / left.len() as f32;
    let mean_right = right.iter().sum::<f32>() / right.len() as f32;
    let mut numerator = 0.0;
    let mut left_energy = 0.0;
    let mut right_energy = 0.0;
    for (left, right) in left.iter().zip(right) {
        let left = *left - mean_left;
        let right = *right - mean_right;
        numerator += left * right;
        left_energy += left * left;
        right_energy += right * right;
    }
    if left_energy <= 1.0e-8 || right_energy <= 1.0e-8 {
        if left
            .iter()
            .zip(right)
            .all(|(left, right)| (*left - *right).abs() <= 1.0e-5)
        {
            1.0
        } else {
            0.0
        }
    } else {
        (numerator / (left_energy * right_energy).sqrt()).clamp(-1.0, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn figure_eight(samples: usize, center: Vec2, scale: Vec2) -> Vec<Vec2> {
        (0..samples)
            .map(|index| {
                let phase = index as f32 / (samples - 1) as f32 * std::f32::consts::TAU;
                center + Vec2::new(phase.sin(), phase.sin() * phase.cos()) * scale
            })
            .collect()
    }

    #[test]
    fn near_duplicate_demonstrations_merge_instead_of_appending() {
        let first =
            ActionSignature::from_trace(&figure_eight(80, Vec2::splat(0.5), Vec2::splat(0.2)), 2.0)
                .unwrap();
        let second = ActionSignature::from_trace(
            &figure_eight(37, Vec2::new(0.3, 0.7), Vec2::splat(0.12)),
            2.0,
        )
        .unwrap();
        let mut library = MimesisLibrary {
            skills: Vec::new(),
            next_skill_id: 1,
        };
        let (id, merged) = library.observe(first, 1.0).unwrap();
        assert!(!merged);
        assert_eq!(library.observe(second, 2.0).unwrap(), (id, true));
        assert_eq!(library.skills.len(), 1);
        assert_eq!(library.skills[0].demonstrations, 2);
    }

    #[test]
    fn ten_safe_attempts_reduce_fixed_fixture_motor_error_by_at_least_a_quarter() {
        let signature =
            ActionSignature::from_trace(&figure_eight(64, Vec2::splat(0.5), Vec2::splat(0.2)), 2.4)
                .unwrap();
        let mut library = MimesisLibrary {
            skills: Vec::new(),
            next_skill_id: 1,
        };
        let (id, _) = library.observe(signature, 1.0).unwrap();
        let initial = library.skills[0].motor_error_ema;
        for attempt in 0..10 {
            let error = 0.58 - attempt as f32 * 0.035;
            library
                .record_attempt(id, error, 2.0 + attempt as f64, None)
                .unwrap();
        }
        assert!(library.skills[0].motor_error_ema <= initial * 0.75);
        assert!(library.skills[0].competence > 0.12);
    }

    #[test]
    fn figure_eight_transfers_to_a_new_region_without_topology_loss_or_jump() {
        let signature = ActionSignature::from_trace(
            &figure_eight(72, Vec2::splat(0.5), Vec2::splat(0.18)),
            2.2,
        )
        .unwrap();
        let transferred =
            retarget_path(&signature, Vec2::new(0.76, 0.28), Vec2::new(0.14, 0.10)).unwrap();
        assert!(transferred.iter().all(|point| {
            point.is_finite() && point.cmpge(Vec2::ZERO).all() && point.cmple(Vec2::ONE).all()
        }));
        assert!(
            transferred
                .windows(2)
                .all(|pair| pair[0].distance(pair[1]) < 0.09)
        );
        let replay = ActionSignature::from_trace(&transferred, 2.2).unwrap();
        assert!(signature_distance(&signature, &replay) < 0.12);
    }

    #[test]
    fn normalized_rhythm_preserves_interval_correlation() {
        let demonstration = RhythmSignature::from_onsets(&[0.0, 0.2, 0.5, 0.7, 1.1]).unwrap();
        let replay = RhythmSignature::from_onsets(&[2.0, 2.3, 2.75, 3.05, 3.65]).unwrap();
        assert!(demonstration.interval_correlation(replay) >= 0.99);
    }

    #[test]
    fn weighted_motor_error_is_finite_and_bounded() {
        let error = motor_error(MotorErrorComponents {
            mean_path_distance: 0.4,
            speed_envelope_error: 0.3,
            curvature_error: 0.2,
            closure_error: 0.1,
            spin_error: 0.2,
            settle_time_error: 0.1,
            collision_penalty: 0.15,
        });
        assert!((0.0..=1.0).contains(&error));
        assert!(error > 0.0);
    }
}
