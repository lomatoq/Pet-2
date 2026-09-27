//! Small spatial memory: curiosity revisits places, not a wall-clock animation.
use glam::Vec2;
use serde::{Deserialize, Serialize};

use crate::{AffectState, BodyFeedback, Drives, TemperamentGenome};

const PLACES: usize = 12;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ExplorationMemory {
    /// Slow familiarity survives absence. Recency and failed routes recover.
    pub familiarity: [f32; PLACES],
    pub recency: [f32; PLACES],
    pub obstruction: [f32; PLACES],
    pub observed_seconds: f64,
    target_index: Option<usize>,
    best_distance: f32,
    stalled_seconds: f32,
    inspected_seconds: f32,
}

impl ExplorationMemory {
    /// Native relaunch may change monitor topology and physical spawn position.
    /// Keep learned places, but start a new route from the observed body.
    /// Core snapshot replay deliberately does not call this method.
    pub fn resume_after_absence(&mut self) {
        self.target_index = None;
        self.pause();
    }

    pub fn is_valid(&self) -> bool {
        self.familiarity
            .iter()
            .chain(&self.recency)
            .chain(&self.obstruction)
            .all(|x| x.is_finite() && (0.0..=1.0).contains(x))
            && self.observed_seconds.is_finite()
            && self.observed_seconds >= 0.0
            && self.target_index.is_none_or(|i| i < PLACES)
            && [
                self.best_distance,
                self.stalled_seconds,
                self.inspected_seconds,
            ]
            .iter()
            .all(|x| x.is_finite() && *x >= 0.0)
    }

    /// Calendar recovery is not observation and does not manufacture visits.
    pub fn recover(&mut self, seconds: f64) {
        if !seconds.is_finite() || seconds <= 0.0 {
            return;
        }
        let recent_decay = (-seconds / 240.0).exp() as f32;
        let obstruction_decay = (-seconds / 90.0).exp() as f32;
        for value in &mut self.recency {
            *value *= recent_decay;
        }
        for value in &mut self.obstruction {
            *value *= obstruction_decay;
        }
    }

    pub fn pause(&mut self) {
        // Resume the same destination, but do not count another activity as
        // evidence that this route is blocked.
        self.stalled_seconds = 0.0;
        self.inspected_seconds = 0.0;
        self.best_distance = 2.0;
    }

    pub fn observe(&mut self, position: Vec2, seed: u64, dt: f32) {
        if !position.is_finite() || !dt.is_finite() || dt <= 0.0 {
            return;
        }
        let dt = dt.min(0.25);
        self.recover(f64::from(dt));
        self.observed_seconds += f64::from(dt);
        let nearest = (0..PLACES)
            .min_by(|a, b| {
                anchor(*a, seed)
                    .distance_squared(position)
                    .total_cmp(&anchor(*b, seed).distance_squared(position))
            })
            .unwrap_or(0);
        // Evidence comes from actual occupancy, regardless of selected action.
        self.familiarity[nearest] +=
            (1.0 - self.familiarity[nearest]) * (1.0 - (-dt / 180.0).exp());
        self.recency[nearest] += (1.0 - self.recency[nearest]) * (1.0 - (-dt / 5.0).exp());
    }

    pub fn target(
        &mut self,
        body: &BodyFeedback,
        seed: u64,
        traits: &TemperamentGenome,
        drives: &Drives,
        affect: AffectState,
        dt: f32,
    ) -> Vec2 {
        let dt = if dt.is_finite() {
            dt.clamp(0.0, 0.25)
        } else {
            0.0
        };
        if let Some(index) = self.target_index {
            let distance = body.world_position.distance(anchor(index, seed));
            if distance < 0.045 {
                self.inspected_seconds += dt;
                self.stalled_seconds = 0.0;
                // Inspection accumulates only while physically present.
                let inspection = 0.6 + 1.6 * traits.patience + drives.sleep;
                if self.inspected_seconds >= inspection {
                    self.target_index = None;
                }
            } else {
                self.inspected_seconds = 0.0;
                if distance < self.best_distance - 0.003 {
                    self.best_distance = distance;
                    self.stalled_seconds = 0.0;
                } else {
                    self.stalled_seconds += dt;
                }
                // Watchdog on lack of progress, never on total flight duration.
                if self.stalled_seconds > 3.0 + 6.0 * traits.persistence {
                    self.obstruction[index] = (self.obstruction[index] + 0.6).min(1.0);
                    self.target_index = None;
                }
            }
        }
        if self.target_index.is_none() {
            let curiosity = (0.25 + drives.curiosity + drives.novelty * 0.45)
                * (0.4 + traits.curiosity * 0.6)
                * (1.0 - affect.stress * 0.75);
            let comfort = drives.comfort + drives.sleep * 0.7 + affect.stress;
            let index = (0..PLACES)
                .max_by(|a, b| {
                    let score = |i: usize| {
                        let distance = body.world_position.distance(anchor(i, seed));
                        curiosity * (1.0 - self.familiarity[i] * 0.65)
                            + comfort * self.familiarity[i]
                            - self.recency[i] * (0.45 + curiosity)
                            - self.obstruction[i] * 2.0
                            - distance * (0.15 + drives.sleep * 0.65 + affect.stress * 0.5)
                            + (anchor(i, seed).x * 31.7 + anchor(i, seed).y * 13.1).sin() * 0.025
                    };
                    score(*a).total_cmp(&score(*b))
                })
                .unwrap_or(0);
            self.target_index = Some(index);
            self.best_distance = body.world_position.distance(anchor(index, seed));
            self.stalled_seconds = 0.0;
            self.inspected_seconds = 0.0;
        }
        anchor(self.target_index.unwrap_or(0), seed)
    }
}

fn anchor(index: usize, seed: u64) -> Vec2 {
    // Fixed identity jitter uses no action RNG; the platform still projects
    // these normalized interior goals onto its legal desktop/work area.
    let mut hash = seed.wrapping_add((index as u64 + 1).wrapping_mul(0x9E3779B97F4A7C15));
    hash = (hash ^ (hash >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
    hash = (hash ^ (hash >> 27)).wrapping_mul(0x94D049BB133111EB);
    hash ^= hash >> 31;
    let jitter = |bits: u64| (bits as u16 as f32 / 65535.0 - 0.5) * 0.045;
    Vec2::new(
        0.14 + (index % 4) as f32 * 0.24 + jitter(hash),
        0.22 + (index / 4) as f32 * 0.27 + jitter(hash >> 16),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Genome;

    #[test]
    fn native_resume_clears_route_without_inventing_or_erasing_experience() {
        let genome = Genome::from_seed(4);
        let drives = Drives::initial(&genome.temperament);
        let mut memory = ExplorationMemory::default();
        memory.observe(Vec2::splat(0.5), 4, 0.25);
        memory.target(
            &BodyFeedback::default(),
            4,
            &genome.temperament,
            &drives,
            AffectState::default(),
            0.05,
        );
        let evidence = (memory.familiarity, memory.recency, memory.observed_seconds);
        memory.resume_after_absence();
        assert_eq!(memory.target_index, None);
        assert_eq!(
            (memory.familiarity, memory.recency, memory.observed_seconds),
            evidence
        );
    }

    #[test]
    fn ten_active_minutes_visit_multiple_places_and_replay_same_next_destination() {
        for seed in [4, 31, 72] {
            let genome = Genome::from_seed(seed);
            let drives = Drives::initial(&genome.temperament);
            let mut memory = ExplorationMemory::default();
            let mut body = BodyFeedback::default();
            let mut destinations = std::collections::BTreeSet::new();
            for _ in 0..12_000 {
                memory.observe(body.world_position, seed, 0.05);
                let target = memory.target(
                    &body,
                    seed,
                    &genome.temperament,
                    &drives,
                    AffectState::default(),
                    0.05,
                );
                destinations.insert(memory.target_index.unwrap());
                let delta = target - body.world_position;
                body.world_position += delta.clamp_length_max(0.06 * 0.05);
            }
            assert!(destinations.len() >= 4, "seed {seed}: {destinations:?}");
            assert!(
                memory
                    .familiarity
                    .iter()
                    .filter(|value| **value > 0.05)
                    .count()
                    >= 4
            );
            let mut replay: ExplorationMemory =
                serde_json::from_str(&serde_json::to_string(&memory).unwrap()).unwrap();
            assert_eq!(
                memory.target(
                    &body,
                    seed,
                    &genome.temperament,
                    &drives,
                    AffectState::default(),
                    0.05
                ),
                replay.target(
                    &body,
                    seed,
                    &genome.temperament,
                    &drives,
                    AffectState::default(),
                    0.05
                )
            );
        }
    }

    #[test]
    fn healthy_progress_holds_target_beyond_old_six_second_boundary() {
        let genome = Genome::from_seed(41);
        let drives = Drives::initial(&genome.temperament);
        let mut memory = ExplorationMemory::default();
        let mut body = BodyFeedback::default();
        let target = memory.target(
            &body,
            41,
            &genome.temperament,
            &drives,
            AffectState::default(),
            0.05,
        );
        for _ in 0..200 {
            body.world_position += (target - body.world_position).normalize_or_zero() * 0.00025;
            assert_eq!(
                memory.target(
                    &body,
                    41,
                    &genome.temperament,
                    &drives,
                    AffectState::default(),
                    0.05
                ),
                target
            );
        }
    }

    #[test]
    fn blocked_route_is_abandoned_but_not_blacklisted_forever() {
        let genome = Genome::from_seed(9);
        let drives = Drives::initial(&genome.temperament);
        let mut memory = ExplorationMemory::default();
        let body = BodyFeedback::default();
        let first = memory.target(
            &body,
            9,
            &genome.temperament,
            &drives,
            AffectState::default(),
            0.05,
        );
        for _ in 0..200 {
            memory.target(
                &body,
                9,
                &genome.temperament,
                &drives,
                AffectState::default(),
                0.05,
            );
        }
        assert_ne!(anchor(memory.target_index.unwrap(), 9), first);
        assert!(memory.obstruction.iter().any(|x| *x > 0.0));
        memory.recover(3600.0);
        assert!(memory.obstruction.iter().all(|x| *x < 0.001));
    }

    #[test]
    fn curiosity_seeks_less_familiar_places_while_stress_favors_known_place() {
        let mut genome = Genome::from_seed(7);
        genome.temperament.curiosity = 1.0;
        let mut drives = Drives::initial(&genome.temperament);
        drives.curiosity = 1.0;
        drives.novelty = 1.0;
        drives.comfort = 0.0;
        drives.sleep = 0.0;
        let mut memory = ExplorationMemory::default();
        memory.familiarity[0] = 1.0;
        let body = BodyFeedback::default();
        memory.target(
            &body,
            7,
            &genome.temperament,
            &drives,
            AffectState::default(),
            0.05,
        );
        assert_ne!(memory.target_index, Some(0));
        memory.target_index = None;
        drives.comfort = 1.0;
        let affect = AffectState {
            stress: 1.0,
            ..AffectState::default()
        };
        memory.target(&body, 7, &genome.temperament, &drives, affect, 0.05);
        assert_eq!(memory.target_index, Some(0));
    }

    #[test]
    fn thirty_day_restarts_preserve_identity_and_only_credit_actual_observation() {
        for seed in [4, 31, 5784121873664838231] {
            let mut memory = ExplorationMemory::default();
            for _ in 0..30 {
                // Ten real active minutes at 20 Hz, then 23h50m calendar-only.
                for _ in 0..12_000 {
                    memory.observe(anchor(3, seed), seed, 0.05);
                }
                let familiar = memory.familiarity;
                memory.recover(85_800.0);
                assert_eq!(memory.familiarity, familiar);
                memory = serde_json::from_str(&serde_json::to_string(&memory).unwrap()).unwrap();
                assert!(memory.is_valid());
            }
            assert!((memory.observed_seconds - 18_000.0).abs() < 0.01);
            assert!(memory.familiarity[3] > 0.99);
            assert!(
                memory
                    .familiarity
                    .iter()
                    .enumerate()
                    .all(|(i, f)| i == 3 || *f == 0.0)
            );
        }
        assert_ne!(anchor(3, 4), anchor(3, 31));
        assert!(
            serde_json::from_str::<ExplorationMemory>("{}")
                .unwrap()
                .is_valid()
        );
    }
}
