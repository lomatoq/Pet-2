use glam::Vec2;
use lifecore::{EmbodiedGestureKind, InteractionReasonCode, stable_hash_bytes};
use pet_body::{BodyMaterialSnapshot, BodySnapshotError, PbfTuning};
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const POINTER_INTERACTION_REPLAY_SCHEMA_VERSION: u32 = 1;
pub const MAX_POINTER_REPLAY_SECONDS: u32 = 30;
pub const MAX_POINTER_REPLAY_SAMPLES: usize = 3_600;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PointerInteractionReplay {
    pub schema_version: u32,
    pub base_commit: String,
    pub identity_seed: u64,
    pub tuning_schema_version: u32,
    pub tuning_revision: u64,
    pub fixed_hz: u32,
    pub initial_body_snapshot: BodyMaterialSnapshot,
    pub samples: Vec<PointerReplaySample>,
    pub expected: PointerReplayExpectations,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct PointerReplaySample {
    pub tick: u64,
    pub position_body_local: Vec2,
    pub down: bool,
    pub hovered: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComponentTimelineCheckpoint {
    pub tick: u64,
    pub component_count: u8,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PointerReplayExpectations {
    pub component_timeline: Vec<ComponentTimelineCheckpoint>,
    pub detached_ticks: Vec<u64>,
    pub remerge_ticks: Vec<u64>,
    pub mass_checksum: u64,
    pub selected_gesture: EmbodiedGestureKind,
    pub response_reason: InteractionReasonCode,
    pub response_variant: u8,
    pub final_state_hash: u64,
}

#[derive(Debug, Error)]
pub enum PointerReplayError {
    #[error("unsupported pointer replay schema {found}; expected {expected}")]
    UnsupportedSchema { found: u32, expected: u32 },
    #[error("pointer replay metadata is invalid")]
    InvalidMetadata,
    #[error("pointer replay exceeds the bounded 30 second / 3600 sample limit")]
    TooLong,
    #[error("pointer replay samples are invalid or not strictly ordered")]
    InvalidSamples,
    #[error("pointer replay expectations are not bounded or ordered")]
    InvalidExpectations,
    #[error(transparent)]
    Body(#[from] BodySnapshotError),
}

impl PointerInteractionReplay {
    pub fn validate(&self, tuning: PbfTuning) -> Result<(), PointerReplayError> {
        if self.schema_version != POINTER_INTERACTION_REPLAY_SCHEMA_VERSION {
            return Err(PointerReplayError::UnsupportedSchema {
                found: self.schema_version,
                expected: POINTER_INTERACTION_REPLAY_SCHEMA_VERSION,
            });
        }
        if self.base_commit.trim().is_empty()
            || self.base_commit.len() > 64
            || !(30..=120).contains(&self.fixed_hz)
            || self.identity_seed != self.initial_body_snapshot.identity_seed
            || self.tuning_schema_version != self.initial_body_snapshot.tuning_schema_version
            || self.tuning_revision != self.initial_body_snapshot.tuning_revision
        {
            return Err(PointerReplayError::InvalidMetadata);
        }
        if self.samples.len() > MAX_POINTER_REPLAY_SAMPLES
            || self.samples.last().is_some_and(|sample| {
                sample.tick > u64::from(MAX_POINTER_REPLAY_SECONDS * self.fixed_hz)
            })
        {
            return Err(PointerReplayError::TooLong);
        }
        let mut previous = None;
        for sample in &self.samples {
            if previous.is_some_and(|tick| sample.tick <= tick)
                || !sample.position_body_local.is_finite()
                || sample.position_body_local.abs().max_element() > 4.0
            {
                return Err(PointerReplayError::InvalidSamples);
            }
            previous = Some(sample.tick);
        }
        if self.expected.component_timeline.len() > MAX_POINTER_REPLAY_SAMPLES
            || self.expected.detached_ticks.len() > 64
            || self.expected.remerge_ticks.len() > 64
            || !strict_ticks(
                self.expected
                    .component_timeline
                    .iter()
                    .map(|checkpoint| checkpoint.tick),
            )
            || !strict_ticks(self.expected.detached_ticks.iter().copied())
            || !strict_ticks(self.expected.remerge_ticks.iter().copied())
            || self
                .expected
                .component_timeline
                .iter()
                .any(|checkpoint| checkpoint.component_count == 0 || checkpoint.component_count > 4)
        {
            return Err(PointerReplayError::InvalidExpectations);
        }
        self.initial_body_snapshot.validate(
            self.identity_seed,
            self.tuning_schema_version,
            tuning,
        )?;
        if self.expected.mass_checksum != self.initial_body_snapshot.total_mass_bits_checksum {
            return Err(PointerReplayError::InvalidExpectations);
        }
        Ok(())
    }

    #[must_use]
    pub fn semantic_hash(&self) -> u64 {
        let mut bytes = Vec::with_capacity(self.samples.len() * 24 + 128);
        bytes.extend_from_slice(&self.schema_version.to_le_bytes());
        bytes.extend_from_slice(&self.identity_seed.to_le_bytes());
        bytes.extend_from_slice(&self.tuning_revision.to_le_bytes());
        for sample in &self.samples {
            bytes.extend_from_slice(&sample.tick.to_le_bytes());
            bytes.extend_from_slice(&sample.position_body_local.x.to_bits().to_le_bytes());
            bytes.extend_from_slice(&sample.position_body_local.y.to_bits().to_le_bytes());
            bytes.push(u8::from(sample.down));
            bytes.push(u8::from(sample.hovered));
        }
        stable_hash_bytes(&bytes)
    }
}

fn strict_ticks(mut ticks: impl Iterator<Item = u64>) -> bool {
    let Some(mut previous) = ticks.next() else {
        return true;
    };
    for tick in ticks {
        if tick <= previous {
            return false;
        }
        previous = tick;
    }
    true
}

#[derive(Debug, Clone, Default)]
pub struct PointerReplayRecorder {
    samples: Vec<PointerReplaySample>,
}

impl PointerReplayRecorder {
    pub fn push(&mut self, sample: PointerReplaySample) -> Result<(), PointerReplayError> {
        if self.samples.len() >= MAX_POINTER_REPLAY_SAMPLES
            || self
                .samples
                .last()
                .is_some_and(|previous| sample.tick <= previous.tick)
            || !sample.position_body_local.is_finite()
            || sample.position_body_local.abs().max_element() > 4.0
        {
            return Err(PointerReplayError::InvalidSamples);
        }
        self.samples.push(sample);
        Ok(())
    }

    #[must_use]
    pub fn finish(self) -> Vec<PointerReplaySample> {
        self.samples
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pet_body::{LIQUID_TUNING_SCHEMA_VERSION, LiquidMorphRuntime};

    fn replay() -> PointerInteractionReplay {
        let body = LiquidMorphRuntime::new(17);
        let snapshot = body.body_material_snapshot(17, LIQUID_TUNING_SCHEMA_VERSION, 3);
        PointerInteractionReplay {
            schema_version: POINTER_INTERACTION_REPLAY_SCHEMA_VERSION,
            base_commit: "test-commit".to_owned(),
            identity_seed: 17,
            tuning_schema_version: LIQUID_TUNING_SCHEMA_VERSION,
            tuning_revision: 3,
            fixed_hz: 120,
            samples: vec![
                PointerReplaySample {
                    tick: 1,
                    position_body_local: Vec2::ZERO,
                    down: true,
                    hovered: true,
                },
                PointerReplaySample {
                    tick: 2,
                    position_body_local: Vec2::new(0.1, 0.0),
                    down: false,
                    hovered: true,
                },
            ],
            expected: PointerReplayExpectations {
                mass_checksum: snapshot.total_mass_bits_checksum,
                ..PointerReplayExpectations::default()
            },
            initial_body_snapshot: snapshot,
        }
    }

    #[test]
    fn pointer_replay_round_trip_is_semantically_exact() {
        let replay = replay();
        replay.validate(PbfTuning::default()).unwrap();
        let json = serde_json::to_vec(&replay).unwrap();
        let restored: PointerInteractionReplay = serde_json::from_slice(&json).unwrap();
        assert_eq!(restored, replay);
        assert_eq!(restored.semantic_hash(), replay.semantic_hash());
    }

    #[test]
    fn pointer_replay_rejects_unbounded_or_derived_input() {
        let mut replay = replay();
        replay.samples[1].tick = replay.samples[0].tick;
        assert!(matches!(
            replay.validate(PbfTuning::default()),
            Err(PointerReplayError::InvalidSamples)
        ));
    }
}
