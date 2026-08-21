use rand::RngCore;
use serde::{Deserialize, Serialize};

use crate::{
    Genome,
    genome::{range, signed_unit},
};

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
    development.stage = match development.metamorphosis_count {
        0 => DevelopmentStage::Juvenile,
        1 => DevelopmentStage::Young,
        2 => DevelopmentStage::Mature,
        _ => DevelopmentStage::Evolved,
    };
    development.mutation_history.push(MutationRecord {
        generation: genome.generation,
        stage: development.stage,
        changed_traits: changed_traits.clone(),
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
