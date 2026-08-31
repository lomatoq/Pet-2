use rand::RngCore;
use serde::{Deserialize, Serialize};

use crate::{
    Genome,
    genome::{range, signed_unit},
};

/// Full before/after genomes are small enough to keep a useful rolling lineage
/// without turning the main life snapshot into an unbounded event store.
pub const MAX_MUTATION_HISTORY: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DevelopmentStage {
    Juvenile,
    Young,
    Mature,
    Evolved,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct LifetimeStatistics {
    pub ticks_alive: u64,
    pub night_activity: f32,
    pub vocal_interactions: f32,
    pub cursor_chases: f32,
    pub focus_seconds: f32,
    pub rhythmic_interactions: f32,
    pub landings: f32,
    pub ignored_attempts: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MutationRecord {
    pub generation: u32,
    pub stage: DevelopmentStage,
    pub changed_traits: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub before_genome: Option<Genome>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after_genome: Option<Genome>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_genome_hash: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub child_genome_hash: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lifetime_snapshot: Option<LifetimeStatistics>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DevelopmentState {
    pub stage: DevelopmentStage,
    pub lifetime: LifetimeStatistics,
    pub mutation_history: Vec<MutationRecord>,
    pub metamorphosis_count: u32,
}

impl Default for DevelopmentState {
    fn default() -> Self {
        Self {
            stage: DevelopmentStage::Juvenile,
            lifetime: LifetimeStatistics::default(),
            mutation_history: Vec::new(),
            metamorphosis_count: 0,
        }
    }
}

impl LifetimeStatistics {
    #[must_use]
    pub fn is_valid(&self) -> bool {
        [
            self.night_activity,
            self.vocal_interactions,
            self.cursor_chases,
            self.focus_seconds,
            self.rhythmic_interactions,
            self.landings,
            self.ignored_attempts,
        ]
        .into_iter()
        .all(|value| value.is_finite() && value >= 0.0)
    }
}

impl MutationRecord {
    fn is_fully_legacy(&self) -> bool {
        self.before_genome.is_none()
            && self.after_genome.is_none()
            && self.parent_genome_hash.is_none()
            && self.child_genome_hash.is_none()
            && self.lifetime_snapshot.is_none()
    }
}

impl DevelopmentState {
    /// Bounds checkpoint-less v1 history, then enriches only the one checkpoint
    /// that legacy v1 can identify exactly: the final mutation record when it
    /// names the currently persisted genome. Parent genomes and missing
    /// generations are unknowable and stay absent.
    pub(crate) fn repair_legacy_current_generation(&mut self, current: &Genome) {
        // v1 did not bound this vector. Preserve its newest usable window only
        // when every record is demonstrably from that checkpoint-less schema.
        if self.mutation_history.len() > MAX_MUTATION_HISTORY
            && self
                .mutation_history
                .iter()
                .all(MutationRecord::is_fully_legacy)
        {
            let excess = self.mutation_history.len() - MAX_MUTATION_HISTORY;
            self.mutation_history.drain(..excess);
        }
        let Some(record) = self.mutation_history.last_mut() else {
            return;
        };
        if record.is_fully_legacy()
            && record.generation == current.generation
            && record.stage == self.stage
        {
            record.after_genome = Some(current.clone());
            record.child_genome_hash = Some(current.stable_hash());
        }
    }

    #[must_use]
    pub fn is_valid_for_genome(&self, current: &Genome) -> bool {
        let oversized_legacy_history = self.mutation_history.len() > MAX_MUTATION_HISTORY
            && self
                .mutation_history
                .iter()
                .all(MutationRecord::is_fully_legacy);
        if !self.lifetime.is_valid()
            || (self.mutation_history.len() > MAX_MUTATION_HISTORY && !oversized_legacy_history)
            || self.metamorphosis_count != current.generation
            || self.stage != stage_for_count(self.metamorphosis_count)
        {
            return false;
        }

        let mut previous_generation = None;
        let mut previous_after: Option<&Genome> = None;
        let mut contains_checkpoint = false;
        for record in &self.mutation_history {
            if record.generation == 0
                || record.generation > current.generation
                || record.stage != stage_for_count(record.generation)
                || previous_generation.is_some_and(|generation| record.generation <= generation)
                || record
                    .lifetime_snapshot
                    .as_ref()
                    .is_some_and(|lifetime| !lifetime.is_valid())
            {
                return false;
            }
            previous_generation = Some(record.generation);

            let before_pair = record.before_genome.as_ref().zip(record.parent_genome_hash);
            let after_pair = record.after_genome.as_ref().zip(record.child_genome_hash);
            if record.before_genome.is_some() != record.parent_genome_hash.is_some()
                || record.after_genome.is_some() != record.child_genome_hash.is_some()
            {
                return false;
            }

            match (before_pair, after_pair) {
                (None, None) => {
                    if record.lifetime_snapshot.is_some() {
                        return false;
                    }
                    previous_after = None;
                }
                (None, Some((after, child_hash))) => {
                    // A legacy repair knows the exact child/current genome but
                    // cannot infer its parent. It remains a valid anchor after
                    // later, fully checkpointed generations are appended.
                    if !after.is_valid()
                        || !same_lineage(after, current)
                        || after.generation != record.generation
                        || child_hash != after.stable_hash()
                    {
                        return false;
                    }
                    contains_checkpoint = true;
                    previous_after = Some(after);
                }
                (Some(_), None) => return false,
                (Some((before, parent_hash)), Some((after, child_hash))) => {
                    if !before.is_valid()
                        || !after.is_valid()
                        || !same_lineage(before, current)
                        || !same_lineage(after, current)
                        || parent_hash != before.stable_hash()
                        || child_hash != after.stable_hash()
                        || before.generation.saturating_add(1) != after.generation
                        || after.generation != record.generation
                        || previous_after.is_some_and(|previous| previous != before)
                    {
                        return false;
                    }
                    contains_checkpoint = true;
                    previous_after = Some(after);
                }
            }
        }

        if contains_checkpoint {
            let Some(latest) = self.mutation_history.last() else {
                return false;
            };
            if latest.generation != current.generation
                || latest
                    .after_genome
                    .as_ref()
                    .is_none_or(|genome| genome != current)
            {
                return false;
            }
        }
        true
    }

    fn push_mutation(&mut self, record: MutationRecord) {
        self.mutation_history.push(record);
        let excess = self
            .mutation_history
            .len()
            .saturating_sub(MAX_MUTATION_HISTORY);
        if excess > 0 {
            self.mutation_history.drain(..excess);
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DevelopmentResult {
    pub previous_generation: u32,
    pub new_generation: u32,
    pub stage: DevelopmentStage,
    pub changed_traits: Vec<String>,
}

pub fn metamorphose(
    genome: &mut Genome,
    development: &mut DevelopmentState,
    rng: &mut impl RngCore,
) -> DevelopmentResult {
    let before_genome = genome.clone();
    let lifetime_snapshot = development.lifetime.clone();
    let previous_generation = genome.generation;
    let plasticity = genome.developmental_plasticity;
    let mutation_scale = genome.mutation_rate * (0.55 + plasticity * 0.65);
    let lifetime = &development.lifetime;
    let total = lifetime.ticks_alive.max(1) as f32;
    let night_pressure = (lifetime.night_activity / total).clamp(0.0, 1.0);
    let vocal_pressure = (lifetime.vocal_interactions / total * 20.0).clamp(0.0, 1.0);
    let chase_pressure = (lifetime.cursor_chases / total * 20.0).clamp(0.0, 1.0);
    let focus_pressure = (lifetime.focus_seconds / (total * 0.05)).clamp(0.0, 1.0);
    let landing_pressure = (lifetime.landings / total * 25.0).clamp(0.0, 1.0);
    let ignored_pressure = (lifetime.ignored_attempts / total * 20.0).clamp(0.0, 1.0);
    let mut changed_traits = Vec::new();

    if chance(rng, 0.35 + night_pressure * 0.45) {
        genome.body.bioluminescence += signed_bias(rng, night_pressure) * mutation_scale;
        changed_traits.push("bioluminescence".to_owned());
    }
    if chance(rng, 0.30 + vocal_pressure * 0.45) {
        genome.voice.pitch_range_octaves += signed_bias(rng, vocal_pressure) * mutation_scale * 0.8;
        genome.body.ear_fin_size += signed_bias(rng, vocal_pressure) * mutation_scale * 0.18;
        changed_traits.push("voice_range_and_ear_fins".to_owned());
    }
    if chance(rng, 0.28 + chase_pressure * 0.50) {
        genome.body.tail_length += signed_bias(rng, chase_pressure) * mutation_scale * 0.35;
        genome.body.inertia -= chase_pressure * mutation_scale * 0.12;
        changed_traits.push("maneuverability_and_tail".to_owned());
    }
    if chance(rng, 0.25 + focus_pressure * 0.45) {
        genome.temperament.autonomy += signed_bias(rng, focus_pressure) * mutation_scale * 0.25;
        changed_traits.push("autonomy".to_owned());
    }
    if chance(rng, 0.25 + landing_pressure * 0.45) {
        genome.body.limb_length += signed_bias(rng, landing_pressure) * mutation_scale * 0.12;
        genome.body.limb_thickness += landing_pressure * mutation_scale * 0.05;
        changed_traits.push("landing_limbs".to_owned());
    }
    if chance(rng, 0.22 + ignored_pressure * 0.45) {
        genome.temperament.patience += ignored_pressure * mutation_scale * 0.18;
        genome.temperament.vocality -= ignored_pressure * mutation_scale * 0.10;
        changed_traits.push("quiet_patience".to_owned());
    }
    if changed_traits.len() < 3 {
        genome.body.pattern_scale *= 1.0 + signed_unit(rng) * mutation_scale;
        genome.voice.formant_scale *= 1.0 + signed_unit(rng) * mutation_scale * 0.45;
        genome.brain.time_constant_scale *= 1.0 + signed_unit(rng) * mutation_scale * 0.25;
        changed_traits.push("pattern_voice_brain_subtle_shift".to_owned());
    }

    genome.generation = genome.generation.saturating_add(1);
    genome.clamp_all();
    development.metamorphosis_count = development.metamorphosis_count.saturating_add(1);
    development.stage = stage_for_count(development.metamorphosis_count);
    let after_genome = genome.clone();
    development.push_mutation(MutationRecord {
        generation: genome.generation,
        stage: development.stage,
        changed_traits: changed_traits.clone(),
        parent_genome_hash: Some(before_genome.stable_hash()),
        child_genome_hash: Some(after_genome.stable_hash()),
        before_genome: Some(before_genome),
        after_genome: Some(after_genome),
        lifetime_snapshot: Some(lifetime_snapshot),
    });

    DevelopmentResult {
        previous_generation,
        new_generation: genome.generation,
        stage: development.stage,
        changed_traits,
    }
}

fn chance(rng: &mut impl RngCore, probability: f32) -> bool {
    range(rng, 0.0, 1.0) < probability.clamp(0.0, 1.0)
}

fn signed_bias(rng: &mut impl RngCore, pressure: f32) -> f32 {
    signed_unit(rng) * 0.45 + pressure * 0.55
}

const fn stage_for_count(metamorphosis_count: u32) -> DevelopmentStage {
    match metamorphosis_count {
        0 => DevelopmentStage::Juvenile,
        1 => DevelopmentStage::Young,
        2 => DevelopmentStage::Mature,
        _ => DevelopmentStage::Evolved,
    }
}

fn same_lineage(left: &Genome, right: &Genome) -> bool {
    left.identity_seed == right.identity_seed && left.lineage_id == right.lineage_id
}

#[cfg(test)]
mod tests {
    use rand::SeedableRng;
    use rand_chacha::ChaCha8Rng;

    use super::*;

    #[test]
    fn metamorphosis_records_exact_before_and_after_checkpoints() {
        let mut genome = Genome::from_seed(42);
        let before = genome.clone();
        let mut development = DevelopmentState::default();
        development.lifetime.ticks_alive = 1_200;
        development.lifetime.cursor_chases = 12.0;
        let lifetime = development.lifetime.clone();
        let mut rng = ChaCha8Rng::seed_from_u64(99);

        let result = metamorphose(&mut genome, &mut development, &mut rng);

        assert_eq!(result.previous_generation, 0);
        assert_eq!(result.new_generation, 1);
        assert_eq!(before.identity_seed, genome.identity_seed);
        assert_eq!(before.lineage_id, genome.lineage_id);
        let record = development.mutation_history.last().unwrap();
        assert_eq!(record.before_genome.as_ref(), Some(&before));
        assert_eq!(record.after_genome.as_ref(), Some(&genome));
        assert_eq!(record.parent_genome_hash, Some(before.stable_hash()));
        assert_eq!(record.child_genome_hash, Some(genome.stable_hash()));
        assert_eq!(record.lifetime_snapshot.as_ref(), Some(&lifetime));
        assert!(development.is_valid_for_genome(&genome));
    }

    #[test]
    fn history_is_bounded_and_retains_a_contiguous_latest_window() {
        let mut genome = Genome::from_seed(7);
        let mut development = DevelopmentState::default();
        let mut rng = ChaCha8Rng::seed_from_u64(11);
        for _ in 0..MAX_MUTATION_HISTORY + 3 {
            metamorphose(&mut genome, &mut development, &mut rng);
        }

        assert_eq!(development.mutation_history.len(), MAX_MUTATION_HISTORY);
        assert_eq!(development.mutation_history[0].generation, 4);
        assert_eq!(
            development.mutation_history.last().unwrap().generation,
            genome.generation
        );
        assert!(development.is_valid_for_genome(&genome));
    }

    #[test]
    fn legacy_record_deserializes_without_fabricated_checkpoint_data() {
        let record: MutationRecord = serde_json::from_str(
            r#"{"generation":1,"stage":"young","changed_traits":["autonomy"]}"#,
        )
        .unwrap();
        assert!(record.is_fully_legacy());
    }

    #[test]
    fn oversized_v1_history_migrates_to_the_latest_bounded_window() {
        let mut current = Genome::from_seed(13);
        current.generation = 67;
        let mut development = DevelopmentState {
            stage: DevelopmentStage::Evolved,
            lifetime: LifetimeStatistics::default(),
            mutation_history: (1..=67)
                .map(|generation| MutationRecord {
                    generation,
                    stage: stage_for_count(generation),
                    changed_traits: vec!["legacy".to_owned()],
                    before_genome: None,
                    after_genome: None,
                    parent_genome_hash: None,
                    child_genome_hash: None,
                    lifetime_snapshot: None,
                })
                .collect(),
            metamorphosis_count: 67,
        };

        // Validation admits this one legacy shape so restore can migrate it.
        assert!(development.is_valid_for_genome(&current));
        development.repair_legacy_current_generation(&current);

        assert_eq!(development.mutation_history.len(), MAX_MUTATION_HISTORY);
        assert_eq!(development.mutation_history[0].generation, 4);
        assert_eq!(
            development
                .mutation_history
                .last()
                .unwrap()
                .after_genome
                .as_ref(),
            Some(&current)
        );
        assert!(development.is_valid_for_genome(&current));
    }

    #[test]
    fn validation_rejects_wrong_hash_order_and_foreign_lineage() {
        let mut genome = Genome::from_seed(17);
        let mut development = DevelopmentState::default();
        let mut rng = ChaCha8Rng::seed_from_u64(19);
        metamorphose(&mut genome, &mut development, &mut rng);

        let mut wrong_hash = development.clone();
        wrong_hash.mutation_history[0].child_genome_hash = Some(1);
        assert!(!wrong_hash.is_valid_for_genome(&genome));

        let mut wrong_order = development.clone();
        wrong_order.mutation_history[0].generation = 2;
        assert!(!wrong_order.is_valid_for_genome(&genome));

        let mut foreign = development;
        foreign.mutation_history[0]
            .after_genome
            .as_mut()
            .unwrap()
            .lineage_id ^= 1;
        foreign.mutation_history[0].child_genome_hash = Some(
            foreign.mutation_history[0]
                .after_genome
                .as_ref()
                .unwrap()
                .stable_hash(),
        );
        assert!(!foreign.is_valid_for_genome(&genome));
    }
}
