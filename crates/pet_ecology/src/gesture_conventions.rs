use serde::{Deserialize, Serialize};

use crate::{ActionSignature, EcologyError, signature_distance};

pub const MAX_GESTURE_CONVENTIONS: usize = 12;
pub const MAX_CONVENTION_HISTORY: usize = 8;
pub const GESTURE_CONVENTION_MATCH_DISTANCE: f32 = 0.18;
pub const GESTURE_CONVENTION_MIN_CONFIDENCE: f32 = 0.55;
pub const GESTURE_CONVENTION_MIN_QUALITY: f32 = 0.65;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GestureConventionMeaning {
    SpinGame,
    RhythmMotif,
    LaunchReturn,
    FragmentHelp,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct GestureSignature {
    pub path: ActionSignature,
    pub pressure_mean: f32,
    pub pressure_variance: f32,
    pub strain_peak: f32,
    pub release_speed: f32,
    pub duty_cycle: f32,
    pub rhythm_intervals: [f32; 8],
    pub rhythm_count: u8,
}

impl GestureSignature {
    #[must_use]
    pub fn is_valid(&self) -> bool {
        self.path.is_valid()
            && self.pressure_mean.is_finite()
            && (0.0..=1.0).contains(&self.pressure_mean)
            && self.pressure_variance.is_finite()
            && (0.0..=1.0).contains(&self.pressure_variance)
            && self.strain_peak.is_finite()
            && (0.0..=8.0).contains(&self.strain_peak)
            && self.release_speed.is_finite()
            && (0.0..=4.0).contains(&self.release_speed)
            && self.duty_cycle.is_finite()
            && (0.0..=1.0).contains(&self.duty_cycle)
            && usize::from(self.rhythm_count) <= self.rhythm_intervals.len()
            && self
                .rhythm_intervals
                .iter()
                .enumerate()
                .all(|(index, value)| {
                    value.is_finite()
                        && if index < usize::from(self.rhythm_count) {
                            (0.1..=4.0).contains(value)
                        } else {
                            *value == 0.0
                        }
                })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct GestureConvention {
    pub id: u64,
    pub meaning: GestureConventionMeaning,
    pub prototype: GestureSignature,
    pub confidence: f32,
    pub positive_outcomes: u32,
    pub negative_outcomes: u32,
    pub demonstrations: u32,
    pub last_used_seconds: f64,
    pub version: u64,
}

impl GestureConvention {
    #[must_use]
    fn is_valid(&self) -> bool {
        self.id != 0
            && self.prototype.is_valid()
            && self.confidence.is_finite()
            && (0.0..=1.0).contains(&self.confidence)
            && self.demonstrations > 0
            && self
                .positive_outcomes
                .saturating_add(self.negative_outcomes)
                <= self.demonstrations
            && self.last_used_seconds.is_finite()
            && self.last_used_seconds >= 0.0
            && self.version > 0
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
struct GestureConventionCheckpoint {
    conventions: Vec<GestureConvention>,
    next_id: u64,
    version: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct GestureConventionLibrary {
    pub conventions: Vec<GestureConvention>,
    pub next_id: u64,
    pub version: u64,
    #[serde(default)]
    history: Vec<GestureConventionCheckpoint>,
}

impl Default for GestureConventionLibrary {
    fn default() -> Self {
        Self {
            conventions: Vec::new(),
            next_id: 1,
            version: 1,
            history: Vec::new(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GestureConventionMatch {
    pub id: u64,
    pub meaning: GestureConventionMeaning,
    pub distance: f32,
    pub confidence: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConventionOutcome {
    Positive,
    AmbiguousOrNoResponse,
    ExplicitNegative,
}

impl GestureConventionLibrary {
    pub fn validate(&self) -> Result<(), EcologyError> {
        let valid = self.conventions.len() <= MAX_GESTURE_CONVENTIONS
            && self.history.len() <= MAX_CONVENTION_HISTORY
            && self.next_id != 0
            && self.version > 0
            && self.conventions.iter().all(GestureConvention::is_valid)
            && self.conventions.iter().enumerate().all(|(index, item)| {
                self.conventions[..index]
                    .iter()
                    .all(|other| other.id != item.id)
            })
            && self.conventions.iter().all(|item| item.id != self.next_id)
            && self
                .history
                .iter()
                .all(GestureConventionCheckpoint::is_valid);
        if valid {
            Ok(())
        } else {
            Err(EcologyError::InvalidGestureConventions)
        }
    }

    /// Records a successful shared interaction. A new meaning is learned only
    /// from a strong or explicit success; weak coincidences cannot create a
    /// convention.
    pub fn observe_success(
        &mut self,
        meaning: GestureConventionMeaning,
        signature: GestureSignature,
        timestamp: f64,
        strong_or_explicit: bool,
    ) -> Result<Option<(u64, bool)>, EcologyError> {
        self.observe_success_with_openness(meaning, signature, timestamp, strong_or_explicit, 1.0)
    }

    pub fn observe_success_with_openness(
        &mut self,
        meaning: GestureConventionMeaning,
        signature: GestureSignature,
        timestamp: f64,
        strong_or_explicit: bool,
        learning_openness: f32,
    ) -> Result<Option<(u64, bool)>, EcologyError> {
        if !signature.is_valid() || !valid_timestamp(timestamp) {
            return Err(EcologyError::InvalidGestureConventions);
        }
        let openness = if learning_openness.is_finite() {
            learning_openness.clamp(0.50, 1.25)
        } else {
            1.0
        };
        let nearest = self
            .conventions
            .iter()
            .enumerate()
            .filter(|(_, convention)| convention.meaning == meaning)
            .map(|(index, convention)| {
                (
                    index,
                    gesture_signature_distance(&convention.prototype, &signature),
                )
            })
            .filter(|(_, distance)| *distance <= GESTURE_CONVENTION_MATCH_DISTANCE)
            .min_by(|left, right| left.1.total_cmp(&right.1));

        if let Some((index, _)) = nearest {
            self.checkpoint();
            self.advance_version();
            let convention = &mut self.conventions[index];
            let alpha = (0.28 / (convention.demonstrations.max(1) as f32).sqrt()).clamp(0.05, 0.28);
            blend_gesture_signature(&mut convention.prototype, &signature, alpha);
            convention.confidence += 0.18 * openness * (1.0 - convention.confidence);
            convention.confidence = convention.confidence.clamp(0.0, 1.0);
            convention.positive_outcomes = convention.positive_outcomes.saturating_add(1);
            convention.demonstrations = convention.demonstrations.saturating_add(1);
            convention.last_used_seconds = timestamp;
            convention.version = self.version;
            let id = convention.id;
            self.validate()?;
            return Ok(Some((id, true)));
        }

        if !strong_or_explicit {
            return Ok(None);
        }
        self.checkpoint();
        self.advance_version();
        let id = self.allocate_id();
        let convention = GestureConvention {
            id,
            meaning,
            prototype: signature,
            confidence: 0.25,
            positive_outcomes: 1,
            negative_outcomes: 0,
            demonstrations: 1,
            last_used_seconds: timestamp,
            version: self.version,
        };
        if self.conventions.len() >= MAX_GESTURE_CONVENTIONS {
            let weakest = self
                .conventions
                .iter()
                .enumerate()
                .min_by(|(_, left), (_, right)| {
                    left.confidence
                        .total_cmp(&right.confidence)
                        .then_with(|| left.positive_outcomes.cmp(&right.positive_outcomes))
                        .then_with(|| left.last_used_seconds.total_cmp(&right.last_used_seconds))
                        .then_with(|| left.id.cmp(&right.id))
                })
                .map(|(index, _)| index)
                .ok_or(EcologyError::InvalidGestureConventions)?;
            self.conventions[weakest] = convention;
        } else {
            self.conventions.push(convention);
        }
        self.validate()?;
        Ok(Some((id, false)))
    }

    #[must_use]
    pub fn recognize(
        &self,
        signature: &GestureSignature,
        quality: f32,
        safety_boundary_or_sleep: bool,
    ) -> Option<GestureConventionMatch> {
        if safety_boundary_or_sleep
            || !signature.is_valid()
            || !quality.is_finite()
            || quality < GESTURE_CONVENTION_MIN_QUALITY
        {
            return None;
        }
        self.conventions
            .iter()
            .filter(|convention| convention.confidence >= GESTURE_CONVENTION_MIN_CONFIDENCE)
            .filter_map(|convention| {
                let distance = gesture_signature_distance(&convention.prototype, signature);
                (distance <= GESTURE_CONVENTION_MATCH_DISTANCE).then_some(GestureConventionMatch {
                    id: convention.id,
                    meaning: convention.meaning,
                    distance,
                    confidence: convention.confidence,
                })
            })
            .min_by(|left, right| left.distance.total_cmp(&right.distance))
    }

    #[must_use]
    pub fn nearest_id(
        &self,
        meaning: GestureConventionMeaning,
        signature: &GestureSignature,
    ) -> Option<u64> {
        if !signature.is_valid() {
            return None;
        }
        self.conventions
            .iter()
            .filter(|item| item.meaning == meaning)
            .map(|item| {
                (
                    item.id,
                    gesture_signature_distance(&item.prototype, signature),
                )
            })
            .filter(|(_, distance)| *distance <= GESTURE_CONVENTION_MATCH_DISTANCE)
            .min_by(|left, right| left.1.total_cmp(&right.1))
            .map(|(id, _)| id)
    }

    pub fn record_outcome(
        &mut self,
        id: u64,
        outcome: ConventionOutcome,
        timestamp: f64,
    ) -> Result<(), EcologyError> {
        if !valid_timestamp(timestamp) || !self.conventions.iter().any(|item| item.id == id) {
            return Err(EcologyError::InvalidGestureConventions);
        }
        self.checkpoint();
        self.advance_version();
        let convention = self
            .conventions
            .iter_mut()
            .find(|item| item.id == id)
            .ok_or(EcologyError::InvalidGestureConventions)?;
        match outcome {
            ConventionOutcome::Positive => {
                convention.confidence += 0.18 * (1.0 - convention.confidence);
                convention.positive_outcomes = convention.positive_outcomes.saturating_add(1);
                convention.demonstrations = convention.demonstrations.saturating_add(1);
            }
            ConventionOutcome::AmbiguousOrNoResponse => {
                convention.confidence *= 0.98;
            }
            ConventionOutcome::ExplicitNegative => {
                convention.confidence *= 0.55;
                convention.negative_outcomes = convention.negative_outcomes.saturating_add(1);
                convention.demonstrations = convention.demonstrations.saturating_add(1);
            }
        }
        convention.confidence = convention.confidence.clamp(0.0, 1.0);
        convention.last_used_seconds = timestamp;
        convention.version = self.version;
        self.validate()
    }

    pub fn delete(&mut self, id: u64) -> bool {
        let Some(index) = self.conventions.iter().position(|item| item.id == id) else {
            return false;
        };
        self.checkpoint();
        self.advance_version();
        self.conventions.remove(index);
        true
    }

    pub fn clear(&mut self) -> bool {
        if self.conventions.is_empty() {
            return false;
        }
        self.checkpoint();
        self.advance_version();
        self.conventions.clear();
        true
    }

    pub fn rollback(&mut self) -> bool {
        let Some(checkpoint) = self.history.pop() else {
            return false;
        };
        self.conventions = checkpoint.conventions;
        self.next_id = checkpoint.next_id;
        self.version = checkpoint.version;
        true
    }

    pub fn rollback_to_version(&mut self, version: u64) -> bool {
        if self.version == version {
            return false;
        }
        let Some(index) = self
            .history
            .iter()
            .rposition(|checkpoint| checkpoint.version == version)
        else {
            return false;
        };
        let checkpoint = self.history[index].clone();
        self.history.truncate(index);
        self.conventions = checkpoint.conventions;
        self.next_id = checkpoint.next_id;
        self.version = checkpoint.version;
        true
    }

    #[must_use]
    pub fn history_len(&self) -> usize {
        self.history.len()
    }

    #[must_use]
    pub fn rollback_versions(&self) -> Vec<u64> {
        self.history
            .iter()
            .map(|checkpoint| checkpoint.version)
            .collect()
    }

    fn checkpoint(&mut self) {
        if self.history.len() == MAX_CONVENTION_HISTORY {
            self.history.remove(0);
        }
        self.history.push(GestureConventionCheckpoint {
            conventions: self.conventions.clone(),
            next_id: self.next_id,
            version: self.version,
        });
    }

    fn advance_version(&mut self) {
        self.version = self.version.saturating_add(1).max(1);
    }

    fn allocate_id(&mut self) -> u64 {
        let id = self.next_id.max(1);
        self.next_id = self.next_id.saturating_add(1).max(1);
        while self.conventions.iter().any(|item| item.id == self.next_id) {
            self.next_id = self.next_id.saturating_add(1).max(1);
        }
        id
    }
}

impl GestureConventionCheckpoint {
    fn is_valid(&self) -> bool {
        self.conventions.len() <= MAX_GESTURE_CONVENTIONS
            && self.next_id != 0
            && self.version > 0
            && self.conventions.iter().all(GestureConvention::is_valid)
            && self.conventions.iter().enumerate().all(|(index, item)| {
                self.conventions[..index]
                    .iter()
                    .all(|other| other.id != item.id)
            })
    }
}

#[must_use]
pub fn gesture_signature_distance(left: &GestureSignature, right: &GestureSignature) -> f32 {
    if !left.is_valid() || !right.is_valid() {
        return 1.0;
    }
    let rhythm_count = usize::from(left.rhythm_count.min(right.rhythm_count));
    let rhythm = if rhythm_count == 0 {
        if left.rhythm_count == right.rhythm_count {
            0.0
        } else {
            1.0
        }
    } else {
        let interval_error = left.rhythm_intervals[..rhythm_count]
            .iter()
            .zip(&right.rhythm_intervals[..rhythm_count])
            .map(|(a, b)| ((a - b).abs() / 3.9).clamp(0.0, 1.0))
            .sum::<f32>()
            / rhythm_count as f32;
        (interval_error + (f32::from(left.rhythm_count.abs_diff(right.rhythm_count)) / 8.0))
            .clamp(0.0, 1.0)
    };
    (signature_distance(&left.path, &right.path) * 0.64
        + (left.pressure_mean - right.pressure_mean).abs() * 0.08
        + (left.pressure_variance - right.pressure_variance).abs() * 0.05
        + ((left.strain_peak - right.strain_peak).abs() / 8.0).min(1.0) * 0.06
        + ((left.release_speed - right.release_speed).abs() / 4.0).min(1.0) * 0.07
        + (left.duty_cycle - right.duty_cycle).abs() * 0.04
        + rhythm * 0.06)
        .clamp(0.0, 1.0)
}

fn valid_timestamp(timestamp: f64) -> bool {
    timestamp.is_finite() && timestamp >= 0.0
}

fn blend_gesture_signature(
    target: &mut GestureSignature,
    observation: &GestureSignature,
    amount: f32,
) {
    let amount = amount.clamp(0.05, 0.28);
    for (target, observation) in target.path.path.iter_mut().zip(observation.path.path) {
        *target = target.lerp(observation, amount);
    }
    for (target, observation) in target.path.speed.iter_mut().zip(observation.path.speed) {
        *target += (observation - *target) * amount;
    }
    target.path.duration_seconds +=
        (observation.path.duration_seconds - target.path.duration_seconds) * amount;
    target.path.spin_turns += (observation.path.spin_turns - target.path.spin_turns) * amount;
    target.path.loopness += (observation.path.loopness - target.path.loopness) * amount;
    target.path.closure_error +=
        (observation.path.closure_error - target.path.closure_error) * amount;
    target.pressure_mean += (observation.pressure_mean - target.pressure_mean) * amount;
    target.pressure_variance += (observation.pressure_variance - target.pressure_variance) * amount;
    target.strain_peak += (observation.strain_peak - target.strain_peak) * amount;
    target.release_speed += (observation.release_speed - target.release_speed) * amount;
    target.duty_cycle += (observation.duty_cycle - target.duty_cycle) * amount;
    let rhythm_count = target.rhythm_count.max(observation.rhythm_count).min(8);
    for index in 0..usize::from(rhythm_count) {
        let sample = if index < usize::from(observation.rhythm_count) {
            observation.rhythm_intervals[index]
        } else {
            target.rhythm_intervals[index]
        };
        target.rhythm_intervals[index] += (sample - target.rhythm_intervals[index]) * amount;
    }
    target.rhythm_count = rhythm_count;
    for value in &mut target.rhythm_intervals[usize::from(rhythm_count)..] {
        *value = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use glam::Vec2;

    use super::*;

    fn signature(offset: f32) -> GestureSignature {
        let points = [
            Vec2::new(offset, 0.0),
            Vec2::new(offset + 0.25, 0.2),
            Vec2::new(offset + 0.5, 0.0),
        ];
        GestureSignature {
            path: ActionSignature::from_trace(&points, 0.8).unwrap(),
            pressure_mean: 0.4,
            pressure_variance: 0.04,
            strain_peak: 0.2,
            release_speed: 0.8,
            duty_cycle: 0.72,
            rhythm_intervals: [1.0, 0.8, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
            rhythm_count: 2,
        }
    }

    #[test]
    fn confidence_rules_and_recognition_gate_are_exact() {
        let mut library = GestureConventionLibrary::default();
        assert_eq!(
            library
                .observe_success(
                    GestureConventionMeaning::SpinGame,
                    signature(0.0),
                    1.0,
                    false
                )
                .unwrap(),
            None
        );
        let (id, merged) = library
            .observe_success(
                GestureConventionMeaning::SpinGame,
                signature(0.0),
                2.0,
                true,
            )
            .unwrap()
            .unwrap();
        assert!(!merged);
        assert_eq!(library.conventions[0].confidence, 0.25);
        assert!(library.recognize(&signature(0.0), 1.0, false).is_none());
        for time in 3..6 {
            library
                .observe_success(
                    GestureConventionMeaning::SpinGame,
                    signature(0.0),
                    f64::from(time),
                    true,
                )
                .unwrap();
        }
        assert!(library.conventions[0].confidence >= 0.55);
        assert_eq!(library.recognize(&signature(0.0), 0.64, false), None);
        assert_eq!(library.recognize(&signature(0.0), 1.0, true), None);
        assert_eq!(
            library.recognize(&signature(0.0), 1.0, false).unwrap().id,
            id
        );
    }

    #[test]
    fn negative_ambiguous_delete_clear_and_rollback_are_bounded() {
        let mut library = GestureConventionLibrary::default();
        let (id, _) = library
            .observe_success(
                GestureConventionMeaning::LaunchReturn,
                signature(0.0),
                1.0,
                true,
            )
            .unwrap()
            .unwrap();
        library
            .record_outcome(id, ConventionOutcome::AmbiguousOrNoResponse, 2.0)
            .unwrap();
        assert!((library.conventions[0].confidence - 0.245).abs() < 1.0e-6);
        library
            .record_outcome(id, ConventionOutcome::ExplicitNegative, 3.0)
            .unwrap();
        assert!((library.conventions[0].confidence - 0.13475).abs() < 1.0e-6);
        assert!(library.delete(id));
        assert!(library.conventions.is_empty());
        assert!(library.rollback());
        assert_eq!(library.conventions[0].id, id);
        for time in 4..20 {
            library
                .record_outcome(
                    id,
                    ConventionOutcome::AmbiguousOrNoResponse,
                    f64::from(time),
                )
                .unwrap();
        }
        assert_eq!(library.history_len(), MAX_CONVENTION_HISTORY);
        assert!(library.clear());
        assert!(library.rollback());
        library.validate().unwrap();
    }

    #[test]
    fn gesture_convention_persists_across_restore() {
        let mut state = crate::EcologyState::new(0x0C01_1EC7);
        for time in 1..=4 {
            state
                .gesture_conventions
                .observe_success(
                    GestureConventionMeaning::RhythmMotif,
                    signature(0.0),
                    f64::from(time),
                    true,
                )
                .unwrap();
        }
        let restored = crate::EcologyState::restore(
            serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap(),
        )
        .unwrap();
        assert_eq!(restored.gesture_conventions, state.gesture_conventions);
        assert!(
            restored
                .gesture_conventions
                .recognize(&signature(0.0), 1.0, false)
                .is_some()
        );
    }

    #[test]
    fn gesture_convention_can_be_deleted() {
        let mut library = GestureConventionLibrary::default();
        let (id, _) = library
            .observe_success(
                GestureConventionMeaning::FragmentHelp,
                signature(0.0),
                1.0,
                true,
            )
            .unwrap()
            .unwrap();
        assert!(library.delete(id));
        assert!(library.conventions.is_empty());
        assert_eq!(
            library.nearest_id(GestureConventionMeaning::FragmentHelp, &signature(0.0)),
            None
        );
    }

    #[test]
    fn gesture_convention_rollback_restores_previous_version() {
        let mut library = GestureConventionLibrary::default();
        let (id, _) = library
            .observe_success(
                GestureConventionMeaning::LaunchReturn,
                signature(0.0),
                1.0,
                true,
            )
            .unwrap()
            .unwrap();
        let learned_version = library.version;
        assert!(library.delete(id));
        assert!(library.rollback_to_version(learned_version));
        assert_eq!(library.conventions[0].id, id);
    }

    #[test]
    fn library_is_bounded_and_replaces_low_value_entries() {
        let mut library = GestureConventionLibrary {
            conventions: (0..MAX_GESTURE_CONVENTIONS)
                .map(|index| GestureConvention {
                    id: index as u64 + 1,
                    meaning: GestureConventionMeaning::SpinGame,
                    prototype: signature(0.0),
                    confidence: if index == 0 { 0.05 } else { 0.80 },
                    positive_outcomes: if index == 0 { 0 } else { 1 },
                    negative_outcomes: 0,
                    demonstrations: 1,
                    last_used_seconds: index as f64,
                    version: 1,
                })
                .collect(),
            next_id: MAX_GESTURE_CONVENTIONS as u64 + 1,
            ..GestureConventionLibrary::default()
        };
        library.validate().unwrap();

        let (new_id, merged) = library
            .observe_success(
                GestureConventionMeaning::LaunchReturn,
                signature(0.0),
                20.0,
                true,
            )
            .unwrap()
            .unwrap();

        assert!(!merged);
        assert_eq!(library.conventions.len(), MAX_GESTURE_CONVENTIONS);
        assert!(!library.conventions.iter().any(|item| item.id == 1));
        assert!(library.conventions.iter().any(|item| item.id == new_id));
        library.validate().unwrap();
    }
}
