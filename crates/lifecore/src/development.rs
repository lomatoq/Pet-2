use rand::RngCore;
use serde::{Deserialize, Serialize};

use crate::{
    DevelopmentalEvidence, EmbodiedGestureKind, EpisodeContextV1, Genome, GestureBoundaryEvent,
    InteroceptionSnapshot, LIFECORE_HZ,
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
    #[serde(default)]
    pub nervous_evidence: DevelopmentalEvidence,
    #[serde(default)]
    pub evidence_seconds: f64,
    #[serde(default)]
    pub completed_interaction_episodes: u32,
    #[serde(default)]
    pub observed_gesture_class_mask: u16,
    #[serde(default)]
    pub sleep_consolidations: u32,
    #[serde(default)]
    pub quiet_episodes: u32,
    #[serde(default)]
    pub safe_boundary_episodes: u32,
    #[serde(default)]
    pub last_completed_episode_id: u64,
    #[serde(default)]
    pub last_observed_gesture_episode_id: u64,
}

impl Default for DevelopmentState {
    fn default() -> Self {
        Self {
            stage: DevelopmentStage::Juvenile,
            lifetime: LifetimeStatistics::default(),
            mutation_history: Vec::new(),
            metamorphosis_count: 0,
            nervous_evidence: DevelopmentalEvidence::default(),
            evidence_seconds: 0.0,
            completed_interaction_episodes: 0,
            observed_gesture_class_mask: 0,
            sleep_consolidations: 0,
            quiet_episodes: 0,
            safe_boundary_episodes: 0,
            last_completed_episode_id: 0,
            last_observed_gesture_episode_id: 0,
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
    /// Integrates the six R12 developmental evidence channels at a multi-hour
    /// timescale. Closed-episode counters are edge-triggered by episode id so a
    /// scheduler retry cannot double count experience.
    pub fn integrate_nervous_evidence(
        &mut self,
        snapshot: InteroceptionSnapshot,
        episode: EpisodeContextV1,
        dt: f32,
    ) {
        let dt = if dt.is_finite() {
            dt.clamp(0.0, 60.0)
        } else {
            0.0
        };
        self.evidence_seconds = (self.evidence_seconds + f64::from(dt)).max(0.0);
        let f = snapshot.felt;
        let d = snapshot.derived;
        let e = snapshot.emotions;
        let target = DevelopmentalEvidence {
            flight_mastery: unit(
                0.38 * f.agency_match
                    + 0.30 * f.motor_efficacy
                    + 0.20 * d.confidence
                    + 0.12 * episode.goal_congruent_motor_success,
            ),
            social_security: unit(
                0.40 * f.social_safety
                    + 0.25 * f.contact_pleasantness
                    + 0.20 * episode.safe_social_exchange
                    + 0.15 * (1.0 - e.frustration),
            ),
            exploration_mastery: unit(
                0.40 * f.exploration_readiness
                    + 0.25 * d.curiosity
                    + 0.20 * d.neural_novelty
                    + 0.15 * episode.successful_exploration,
            ),
            rest_adaptation: unit(
                0.48 * episode.rest_quality + 0.28 * f.relief + 0.24 * (1.0 - e.exhaustion),
            ),
            physical_resilience: unit(
                0.42 * f.body_integrity
                    + 0.28 * f.relief
                    + 0.18 * episode.goal_congruent_motor_success
                    + 0.12 * (1.0 - episode.boundary_violation),
            ),
            communication_mastery: unit(
                0.42 * episode.rhythmic_synchrony
                    + 0.28 * episode.safe_social_exchange
                    + 0.20 * episode.reward_positive
                    + 0.10 * (1.0 - episode.ignored_social_bid),
            ),
        };
        // Six hours: experience changes phenotype evidence, never fast mood or
        // per-frame appearance. Promotion still requires the eligibility gate.
        let alpha = 1.0 - (-dt / (6.0 * 60.0 * 60.0)).exp();
        self.nervous_evidence.flight_mastery +=
            (target.flight_mastery - self.nervous_evidence.flight_mastery) * alpha;
        self.nervous_evidence.social_security +=
            (target.social_security - self.nervous_evidence.social_security) * alpha;
        self.nervous_evidence.exploration_mastery +=
            (target.exploration_mastery - self.nervous_evidence.exploration_mastery) * alpha;
        self.nervous_evidence.rest_adaptation +=
            (target.rest_adaptation - self.nervous_evidence.rest_adaptation) * alpha;
        self.nervous_evidence.physical_resilience +=
            (target.physical_resilience - self.nervous_evidence.physical_resilience) * alpha;
        self.nervous_evidence.communication_mastery +=
            (target.communication_mastery - self.nervous_evidence.communication_mastery) * alpha;
        self.nervous_evidence.sanitize();

        if episode.closed
            && episode.episode_id != 0
            && episode.episode_id != self.last_completed_episode_id
        {
            self.completed_interaction_episodes =
                self.completed_interaction_episodes.saturating_add(1);
            if episode.sleeping_or_deep_rest > 0.5 {
                self.quiet_episodes = self.quiet_episodes.saturating_add(1);
            }
            if episode.boundary_violation <= 0.05 {
                self.safe_boundary_episodes = self.safe_boundary_episodes.saturating_add(1);
            }
            self.last_completed_episode_id = episode.episode_id;
        }
    }

    pub fn note_embodied_gesture(
        &mut self,
        episode_id: u64,
        kind: EmbodiedGestureKind,
        boundary: GestureBoundaryEvent,
        ended: bool,
    ) {
        if episode_id == 0 || episode_id == self.last_observed_gesture_episode_id {
            return;
        }
        self.last_observed_gesture_episode_id = episode_id;
        if let Some(bit) = gesture_bit(kind) {
            self.observed_gesture_class_mask |= 1_u16 << bit;
        }
        if ended && boundary == GestureBoundaryEvent::QuietOrSleep {
            self.quiet_episodes = self.quiet_episodes.saturating_add(1);
        }
        if ended && boundary == GestureBoundaryEvent::None {
            self.safe_boundary_episodes = self.safe_boundary_episodes.saturating_add(1);
        }
    }

    pub fn note_sleep_consolidation(&mut self) {
        self.sleep_consolidations = self.sleep_consolidations.saturating_add(1);
    }

    #[must_use]
    pub fn nervous_system_eligible(
        &self,
        interaction_turn_closed: bool,
        body_invariants_pass: bool,
    ) -> bool {
        self.lifetime.ticks_alive >= u64::from(LIFECORE_HZ as u32) * 24 * 60 * 60
            && self.completed_interaction_episodes >= 120
            && self.observed_gesture_class_mask.count_ones() >= 8
            && self.sleep_consolidations >= 5
            && self.quiet_episodes >= 12
            && self.safe_boundary_episodes >= 8
            && interaction_turn_closed
            && body_invariants_pass
    }

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
            || !self.nervous_evidence.is_valid()
            || !self.evidence_seconds.is_finite()
            || self.evidence_seconds < 0.0
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

fn gesture_bit(kind: EmbodiedGestureKind) -> Option<u32> {
    Some(match kind {
        EmbodiedGestureKind::Unknown => return None,
        EmbodiedGestureKind::SoftTouch => 0,
        EmbodiedGestureKind::SlowStretch => 1,
        EmbodiedGestureKind::Tickle => 2,
        EmbodiedGestureKind::RhythmicTouch => 3,
        EmbodiedGestureKind::CircularTwist => 4,
        EmbodiedGestureKind::SharpFlick => 5,
        EmbodiedGestureKind::Hold => 6,
        EmbodiedGestureKind::PullAndRelease => 7,
        EmbodiedGestureKind::FragmentSeparationAttempt => 8,
        EmbodiedGestureKind::SharedPlayInvitation => 9,
        EmbodiedGestureKind::FragmentHelp => 10,
    })
}

fn unit(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        0.0
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

    if development.nervous_system_eligible(true, true) {
        apply_nervous_system_development(genome, &before_genome, development.nervous_evidence);
        changed_traits.push("embodied_nervous_system_evidence".to_owned());
    }
    enforce_r12_generation_caps(genome, &before_genome);

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

fn apply_nervous_system_development(
    genome: &mut Genome,
    before: &Genome,
    evidence: DevelopmentalEvidence,
) {
    let gain = genome.developmental_plasticity * genome.mutation_rate;
    let centered = |value: f32| (2.0 * value.clamp(0.0, 1.0) - 1.0) * gain;
    let flight = centered(evidence.flight_mastery);
    let social_resilience =
        centered(0.55 * evidence.social_security + 0.45 * evidence.physical_resilience);
    let exploration = centered(evidence.exploration_mastery);
    let rest = centered(evidence.rest_adaptation);
    let communication = centered(evidence.communication_mastery);

    genome.body.wing_span += before.body.wing_span * 0.03 * flight;
    genome.body.wing_aspect += 0.03 * flight;
    genome.body.inertia -= 0.04 * flight;

    genome.body.softness += 0.04 * social_resilience;
    genome.body.body_roundness += 0.03 * social_resilience;
    genome.body.glow_color_hsv.z += 0.035 * social_resilience;
    genome.body.pattern_contrast -= 0.025 * social_resilience.max(0.0);

    genome.body.eye_size += 0.01 * exploration;
    genome.body.ear_fin_size += 0.015 * exploration + 0.010 * communication;
    genome.body.pattern_scale *= 1.0 + 0.03 * exploration;
    genome.body.bioluminescence += 0.025 * exploration + 0.025 * rest;
    genome.body.glow_color_hsv.x =
        (genome.body.glow_color_hsv.x + rest * (2.0 / 360.0)).rem_euclid(1.0);

    genome.voice.formant_scale *= 1.0 + 0.02 * communication;
    // Communication changes resonance, never the hard loudness ceiling.
    genome.voice.maximum_loudness = before.voice.maximum_loudness;
}

fn enforce_r12_generation_caps(genome: &mut Genome, before: &Genome) {
    macro_rules! rel {
        ($field:ident, $cap:expr) => {
            genome.body.$field = cap_relative(before.body.$field, genome.body.$field, $cap)
        };
    }
    macro_rules! abs {
        ($field:ident, $cap:expr) => {
            genome.body.$field = cap_absolute(before.body.$field, genome.body.$field, $cap)
        };
    }
    rel!(body_length, 0.02);
    rel!(body_width, 0.02);
    abs!(body_roundness, 0.03);
    abs!(head_ratio, 0.015);
    abs!(eye_size, 0.01);
    rel!(wing_span, 0.03);
    rel!(tail_length, 0.03);
    rel!(limb_length, 0.025);
    abs!(softness, 0.04);
    abs!(visual_mass, 0.035);
    abs!(inertia, 0.04);
    genome.body.primary_color_hsv.x = cap_hue(
        before.body.primary_color_hsv.x,
        genome.body.primary_color_hsv.x,
        2.0 / 360.0,
    );
    genome.body.secondary_color_hsv.x = cap_hue(
        before.body.secondary_color_hsv.x,
        genome.body.secondary_color_hsv.x,
        3.0 / 360.0,
    );
    genome.body.primary_color_hsv.y = cap_absolute(
        before.body.primary_color_hsv.y,
        genome.body.primary_color_hsv.y,
        0.03,
    );
    genome.body.secondary_color_hsv.y = cap_absolute(
        before.body.secondary_color_hsv.y,
        genome.body.secondary_color_hsv.y,
        0.03,
    );
    rel!(pattern_scale, 0.03);
    abs!(pattern_contrast, 0.025);
    abs!(bioluminescence, 0.04);
    genome.voice.base_pitch_hz =
        cap_relative(before.voice.base_pitch_hz, genome.voice.base_pitch_hz, 0.02);
    genome.voice.formant_scale =
        cap_relative(before.voice.formant_scale, genome.voice.formant_scale, 0.02);
    genome.voice.maximum_loudness = before.voice.maximum_loudness;
    genome.identity_seed = before.identity_seed;
    genome.lineage_id = before.lineage_id;
    genome.body.pattern_seed = before.body.pattern_seed;
    genome.voice.voice_seed = before.voice.voice_seed;
    genome.brain.network_seed = before.brain.network_seed;
    genome.brain.learning_rate = before.brain.learning_rate;
}

fn cap_relative(base: f32, value: f32, fraction: f32) -> f32 {
    let cap = base.abs().max(f32::EPSILON) * fraction;
    base + (value - base).clamp(-cap, cap)
}

fn cap_absolute(base: f32, value: f32, cap: f32) -> f32 {
    base + (value - base).clamp(-cap, cap)
}

fn cap_hue(base: f32, value: f32, cap_turns: f32) -> f32 {
    let delta = (value - base + 0.5).rem_euclid(1.0) - 0.5;
    (base + delta.clamp(-cap_turns, cap_turns)).rem_euclid(1.0)
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
            ..DevelopmentState::default()
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

    #[test]
    fn closed_episode_evidence_is_edge_triggered_and_bounded() {
        let mut development = DevelopmentState::default();
        let snapshot = InteroceptionSnapshot {
            felt: crate::FeltStateV1 {
                agency_match: 0.9,
                motor_efficacy: 0.8,
                social_safety: 0.85,
                contact_pleasantness: 0.75,
                body_integrity: 0.95,
                relief: 0.7,
                exploration_readiness: 0.8,
                ..crate::FeltStateV1::default()
            },
            derived: crate::DerivedNervousState {
                confidence: 0.8,
                curiosity: 0.8,
                neural_novelty: 0.7,
                ..crate::DerivedNervousState::default()
            },
            ..InteroceptionSnapshot::default()
        };
        let episode = EpisodeContextV1 {
            episode_id: 41,
            closed: true,
            reward_positive: 0.8,
            successful_exploration: 0.8,
            goal_congruent_motor_success: 0.9,
            safe_social_exchange: 0.9,
            safe_predictable_episode: 0.9,
            rest_quality: 0.7,
            ..EpisodeContextV1::default()
        };

        development.integrate_nervous_evidence(snapshot, episode, 60.0);
        development.integrate_nervous_evidence(snapshot, episode, 60.0);

        assert_eq!(development.completed_interaction_episodes, 1);
        assert_eq!(development.safe_boundary_episodes, 1);
        assert!(development.nervous_evidence.is_valid());
        assert!(development.nervous_evidence.flight_mastery > 0.0);
    }

    #[test]
    fn nervous_development_preserves_locks_and_generation_caps() {
        let mut genome = Genome::from_seed(2026);
        let before = genome.clone();
        let mut development = DevelopmentState::default();
        development.lifetime.ticks_alive = u64::from(LIFECORE_HZ as u32) * 24 * 60 * 60;
        development.completed_interaction_episodes = 120;
        development.observed_gesture_class_mask = 0xff;
        development.sleep_consolidations = 5;
        development.quiet_episodes = 12;
        development.safe_boundary_episodes = 8;
        development.nervous_evidence = DevelopmentalEvidence {
            flight_mastery: 1.0,
            social_security: 1.0,
            exploration_mastery: 1.0,
            rest_adaptation: 1.0,
            physical_resilience: 1.0,
            communication_mastery: 1.0,
        };
        let mut rng = ChaCha8Rng::seed_from_u64(77);

        metamorphose(&mut genome, &mut development, &mut rng);

        assert_eq!(genome.identity_seed, before.identity_seed);
        assert_eq!(genome.lineage_id, before.lineage_id);
        assert_eq!(genome.body.pattern_seed, before.body.pattern_seed);
        assert_eq!(genome.voice.voice_seed, before.voice.voice_seed);
        assert_eq!(genome.brain.network_seed, before.brain.network_seed);
        assert_eq!(genome.brain.learning_rate, before.brain.learning_rate);
        assert_eq!(genome.voice.maximum_loudness, before.voice.maximum_loudness);
        assert!((genome.body.body_length / before.body.body_length - 1.0).abs() <= 0.020_01);
        assert!((genome.body.wing_span / before.body.wing_span - 1.0).abs() <= 0.030_01);
        assert!((genome.body.softness - before.body.softness).abs() <= 0.040_01);
        assert!((genome.voice.formant_scale / before.voice.formant_scale - 1.0).abs() <= 0.020_01);
    }

    #[test]
    fn identical_evidence_and_seed_produce_identical_development_trace() {
        let mut left_genome = Genome::from_seed(88);
        let mut right_genome = left_genome.clone();
        let mut left = DevelopmentState::default();
        left.lifetime.ticks_alive = u64::from(LIFECORE_HZ as u32) * 10 * 24 * 60 * 60;
        left.completed_interaction_episodes = 300;
        left.observed_gesture_class_mask = 0x7ff;
        left.sleep_consolidations = 20;
        left.quiet_episodes = 30;
        left.safe_boundary_episodes = 25;
        left.nervous_evidence = DevelopmentalEvidence {
            flight_mastery: 0.8,
            social_security: 0.7,
            exploration_mastery: 0.9,
            rest_adaptation: 0.65,
            physical_resilience: 0.75,
            communication_mastery: 0.82,
        };
        let mut right = left.clone();
        let mut left_rng = ChaCha8Rng::seed_from_u64(123);
        let mut right_rng = ChaCha8Rng::seed_from_u64(123);

        metamorphose(&mut left_genome, &mut left, &mut left_rng);
        metamorphose(&mut right_genome, &mut right, &mut right_rng);

        assert_eq!(left_genome.stable_hash(), right_genome.stable_hash());
        assert_eq!(left, right);
    }
}
