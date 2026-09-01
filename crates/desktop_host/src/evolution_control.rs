use std::{collections::BTreeMap, str::FromStr};

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const EVOLUTION_CONFIG_SCHEMA_VERSION: u32 = 1;
pub const EVOLUTION_REPORT_SCHEMA_VERSION: u32 = 2;
pub const EVOLUTION_PROGRESS_SCHEMA_VERSION: u32 = 1;
pub const MAX_EVOLUTION_REPLICATES: u32 = 8;
pub const MAX_EVOLUTION_GENERATIONS: u32 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvolutionPreset {
    OneDayDiagnostic,
    SevenDaySocialization,
    ThirtyDayPersonality,
    BoundarySafety,
    Custom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuietAdvanceMode {
    Exact,
    CalendarOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvolutionOutcomeModel {
    RespectfulSupportive,
    MixedRealistic,
    QuietUser,
    BoundaryValidation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvolutionPolicy {
    Off,
    EligibleMaxOne,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvolutionPersistence {
    #[default]
    DryRun,
    Fork,
    SaveFinal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvolutionProgressStage {
    Starting,
    Replicate,
    Episode,
    Finalizing,
    Completed,
    Failed,
}

/// Atomically replaced while a headless run is alive. This is deliberately a
/// small operational receipt rather than a second copy of the final report.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvolutionProgress {
    pub schema_version: u32,
    pub stage: EvolutionProgressStage,
    pub replicate_index: u32,
    pub replicate_count: u32,
    pub episode_index: u32,
    pub episode_count: u32,
    pub simulated_seconds: f64,
    pub audio_render_count: u32,
    pub learning_update_count: u32,
    pub elapsed_wall_seconds: f64,
    pub eta_seconds: Option<f64>,
    pub last_error: Option<String>,
}

impl EvolutionProgress {
    #[must_use]
    pub fn fraction(&self) -> f32 {
        let total = u64::from(self.replicate_count.max(1))
            .saturating_mul(u64::from(self.episode_count.max(1)));
        let complete = u64::from(self.replicate_index)
            .saturating_mul(u64::from(self.episode_count.max(1)))
            .saturating_add(u64::from(self.episode_index));
        (complete as f64 / total as f64).clamp(0.0, 1.0) as f32
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct EvolutionAudioSummary {
    pub rendered_count: u32,
    pub exported_wav_count: u32,
    pub total_rendered_seconds: f64,
    pub rms_min: f32,
    pub rms_max: f32,
    pub peak_max: f32,
    pub zero_crossing_rate_mean: f32,
    pub wav_artifacts: Vec<String>,
}

pub const RUNTIME_LOAD_ACK_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeLoadStatus {
    Running,
    Stopping,
    Stopped,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeLoadAcknowledgement {
    pub schema_version: u32,
    pub status: RuntimeLoadStatus,
    pub pid: u32,
    pub executable_version: String,
    pub loaded_life_state_hash: u64,
    pub loaded_genome_hash: u64,
    pub updated_unix_ms: u64,
}

impl FromStr for EvolutionPersistence {
    type Err = EvolutionContractError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "dry-run" => Ok(Self::DryRun),
            "fork" => Ok(Self::Fork),
            "save-final" => Ok(Self::SaveFinal),
            _ => Err(EvolutionContractError::InvalidPersistence),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvolutionConfig {
    pub schema_version: u32,
    pub name: String,
    pub preset: EvolutionPreset,
    pub simulated_hours: f64,
    pub episodes_per_day: u32,
    #[serde(default)]
    pub total_episodes: Option<u32>,
    pub replicate_count: u32,
    pub seed: u64,
    pub outcome_model: EvolutionOutcomeModel,
    pub quiet_advance: QuietAdvanceMode,
    pub include_saved_gesture_replays: bool,
    pub sleep_consolidation: bool,
    pub persistent_learning: bool,
    pub evolution_policy: EvolutionPolicy,
    pub maximum_generations: u32,
    pub checkpoint_interval_hours: f64,
    pub persistence: EvolutionPersistence,
}

impl EvolutionConfig {
    pub fn validate(&self) -> Result<(), EvolutionContractError> {
        if self.schema_version != EVOLUTION_CONFIG_SCHEMA_VERSION {
            return Err(EvolutionContractError::UnsupportedConfigSchema);
        }
        if self.name.trim().is_empty() || self.name.chars().count() > 96 {
            return Err(EvolutionContractError::InvalidName);
        }
        if !self.simulated_hours.is_finite() || !(0.01..=720.0).contains(&self.simulated_hours) {
            return Err(EvolutionContractError::InvalidDuration);
        }
        if self.episodes_per_day > 100 {
            return Err(EvolutionContractError::InvalidEpisodeRate);
        }
        if self.total_episodes.is_some_and(|count| count > 3_000) {
            return Err(EvolutionContractError::InvalidEpisodeRate);
        }
        if !(1..=MAX_EVOLUTION_REPLICATES).contains(&self.replicate_count) {
            return Err(EvolutionContractError::InvalidReplicateCount);
        }
        if self.seed == 0 {
            return Err(EvolutionContractError::InvalidSeed);
        }
        if self.maximum_generations > MAX_EVOLUTION_GENERATIONS {
            return Err(EvolutionContractError::InvalidGenerationLimit);
        }
        if !self.checkpoint_interval_hours.is_finite()
            || !(1.0..=168.0).contains(&self.checkpoint_interval_hours)
        {
            return Err(EvolutionContractError::InvalidCheckpointInterval);
        }
        if self.evolution_policy == EvolutionPolicy::Off && self.maximum_generations != 0 {
            return Err(EvolutionContractError::InvalidGenerationPolicy);
        }
        if self.evolution_policy == EvolutionPolicy::EligibleMaxOne && self.maximum_generations > 1
        {
            return Err(EvolutionContractError::InvalidGenerationPolicy);
        }
        if self.preset == EvolutionPreset::BoundarySafety
            && (self.persistent_learning
                || self.evolution_policy != EvolutionPolicy::Off
                || self.maximum_generations != 0)
        {
            return Err(EvolutionContractError::PresetPolicyMismatch);
        }
        Ok(())
    }

    #[must_use]
    pub fn scheduled_episode_count(&self) -> u32 {
        if let Some(count) = self.total_episodes {
            return count;
        }
        ((self.simulated_hours / 24.0) * f64::from(self.episodes_per_day))
            .round()
            .max(0.0) as u32
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvolutionEligibility {
    pub eligible: bool,
    pub simulated_hours_met: bool,
    pub interaction_episodes_met: bool,
    pub gesture_diversity_met: bool,
    pub sleep_consolidations_met: bool,
    pub quiet_episodes_met: bool,
    pub safe_boundary_episodes_met: bool,
    pub mass_and_remerge_passed: bool,
    pub zero_recovery_failures: bool,
    pub interaction_closed: bool,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct EvolutionInvariantSummary {
    pub passed: bool,
    pub mass_conservation_failures: u32,
    pub non_finite_failures: u32,
    pub emergency_recoveries: u64,
    pub topology_budget_failures: u32,
    pub unremerged_components: u32,
    pub duplicate_response_failures: u32,
    pub maximum_detached_mass_fraction: f32,
    pub failures: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvolutionCheckpoint {
    pub simulated_hour: u32,
    pub genome_hash: u64,
    pub life_state_hash: u64,
    pub generation: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvolutionReplicateSummary {
    pub replicate_index: u32,
    pub seed: u64,
    pub completed_episodes: u32,
    pub final_generation: u32,
    pub final_genome_hash: u64,
    pub final_life_state_hash: u64,
    pub interaction_variant_updates: u32,
    pub audio: EvolutionAudioSummary,
    pub telemetry_samples: u64,
    pub gesture_distribution: BTreeMap<String, u32>,
    pub eligibility: EvolutionEligibility,
    pub invariants: EvolutionInvariantSummary,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct EvolutionPerformanceSummary {
    pub wall_seconds: f64,
    pub active_body_steps: u64,
    pub body_step_p50_microseconds: f64,
    pub body_step_p95_microseconds: f64,
    pub body_step_max_microseconds: f64,
    pub episodes_per_wall_second: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvolutionRunReport {
    pub schema_version: u32,
    pub config: EvolutionConfig,
    pub status: String,
    pub clock_mode: QuietAdvanceMode,
    pub approximate_calendar_advance: bool,
    pub base_hz: u32,
    pub perception_hz: u32,
    pub life_hz: u32,
    pub telemetry_hz: u32,
    pub simulated_seconds: f64,
    pub base_ticks: u64,
    pub completed_episodes: u32,
    pub quiet_or_no_response_episodes: u32,
    pub safe_boundary_episodes: u32,
    pub sleep_consolidations: u32,
    pub gesture_distribution: BTreeMap<String, u32>,
    pub interaction_variant_updates: u32,
    pub convention_updates: u32,
    pub audio: EvolutionAudioSummary,
    pub telemetry_samples: u64,
    pub eligibility: EvolutionEligibility,
    pub invariants: EvolutionInvariantSummary,
    pub initial_generation: u32,
    pub final_generation: u32,
    pub initial_genome_hash: u64,
    pub final_genome_hash: u64,
    pub final_life_state_hash: u64,
    pub checkpoints: Vec<EvolutionCheckpoint>,
    pub replicates: Vec<EvolutionReplicateSummary>,
    pub performance: EvolutionPerformanceSummary,
    pub persisted_state: Option<String>,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum EvolutionContractError {
    #[error("unsupported evolution config schema")]
    UnsupportedConfigSchema,
    #[error("evolution run name is empty or too long")]
    InvalidName,
    #[error("simulated duration is outside 0.01..=720 hours")]
    InvalidDuration,
    #[error("episodes per day exceeds the bounded curriculum rate")]
    InvalidEpisodeRate,
    #[error("replicate count is outside 1..=8")]
    InvalidReplicateCount,
    #[error("evolution seed must be non-zero")]
    InvalidSeed,
    #[error("maximum generation count is outside the safe policy")]
    InvalidGenerationLimit,
    #[error("checkpoint interval is outside 1..=168 hours")]
    InvalidCheckpointInterval,
    #[error("generation limit conflicts with evolution policy")]
    InvalidGenerationPolicy,
    #[error("preset conflicts with its safety and persistence policy")]
    PresetPolicyMismatch,
    #[error("persistence must be dry-run, fork, or save-final")]
    InvalidPersistence,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contract_is_closed_bounded_and_uses_integer_episode_counts() {
        let config = EvolutionConfig {
            schema_version: EVOLUTION_CONFIG_SCHEMA_VERSION,
            name: "smoke".into(),
            preset: EvolutionPreset::Custom,
            simulated_hours: 24.0,
            episodes_per_day: 48,
            total_episodes: None,
            replicate_count: 4,
            seed: 42,
            outcome_model: EvolutionOutcomeModel::MixedRealistic,
            quiet_advance: QuietAdvanceMode::Exact,
            include_saved_gesture_replays: false,
            sleep_consolidation: true,
            persistent_learning: true,
            evolution_policy: EvolutionPolicy::Off,
            maximum_generations: 0,
            checkpoint_interval_hours: 24.0,
            persistence: EvolutionPersistence::DryRun,
        };
        config.validate().unwrap();
        assert_eq!(config.scheduled_episode_count(), 48);
        let mut json = serde_json::to_value(&config).unwrap();
        json["raw_trajectory"] = serde_json::json!([[0, 1]]);
        assert!(serde_json::from_value::<EvolutionConfig>(json).is_err());
    }

    #[test]
    fn mandatory_presets_parse_and_validate_as_checked_in_contracts() {
        for source in [
            include_str!("../../../config/evolution/one-day-diagnostic.json"),
            include_str!("../../../config/evolution/seven-day-socialization.json"),
            include_str!("../../../config/evolution/boundary-safety.json"),
        ] {
            let config: EvolutionConfig = serde_json::from_str(source).unwrap();
            config.validate().unwrap();
        }
    }
}
