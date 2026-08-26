use serde::{Deserialize, Serialize};

use crate::{
    DenState, EcologyError, MAX_ACTIVE_MORSELS, MAX_OBJECTS, MetabolicState, MimesisLibrary,
    ObjectId, ObjectKind, ObjectLifecycle, TasteProfile, WorldObject, canonical_orb_id,
};

pub const ECOLOGY_STATE_SCHEMA_VERSION: u32 = 1;
pub const MAX_OBJECT_MEMORIES: usize = 32;
pub const EPISODE_GOAL_COUNT: usize = 27;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ObjectMemory {
    pub object_id: ObjectId,
    pub interaction_count: u32,
    pub positive_outcomes: u32,
    pub negative_outcomes: u32,
    pub prediction_error_ema: f32,
    pub last_seen_seconds: f64,
}

impl ObjectMemory {
    #[must_use]
    fn is_valid(&self) -> bool {
        self.object_id != 0
            && self
                .positive_outcomes
                .saturating_add(self.negative_outcomes)
                <= self.interaction_count
            && self.prediction_error_ema.is_finite()
            && (0.0..=1.0).contains(&self.prediction_error_ema)
            && self.last_seen_seconds.is_finite()
            && self.last_seen_seconds >= 0.0
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct EpisodeStats {
    pub started: [u32; EPISODE_GOAL_COUNT],
    pub completed: [u32; EPISODE_GOAL_COUNT],
    pub aborted: [u32; EPISODE_GOAL_COUNT],
    pub interrupted_by_shutdown: u32,
    pub next_episode_id: u64,
}

impl Default for EpisodeStats {
    fn default() -> Self {
        Self {
            started: [0; EPISODE_GOAL_COUNT],
            completed: [0; EPISODE_GOAL_COUNT],
            aborted: [0; EPISODE_GOAL_COUNT],
            interrupted_by_shutdown: 0,
            next_episode_id: 1,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct SavedEcologyRng {
    pub seed: [u8; 32],
    pub draw_count: u64,
}

impl SavedEcologyRng {
    #[must_use]
    pub fn for_seed(identity_seed: u64) -> Self {
        let mut seed = [0_u8; 32];
        let mut value = identity_seed;
        for chunk in seed.chunks_exact_mut(8) {
            value = super::object::splitmix64(value);
            chunk.copy_from_slice(&value.to_le_bytes());
        }
        Self {
            seed,
            draw_count: 0,
        }
    }

    #[must_use]
    pub fn peek_u64(&self) -> u64 {
        let mut base = u64::from_le_bytes(self.seed[..8].try_into().unwrap_or_default());
        base ^= self.draw_count.rotate_left(17);
        super::object::splitmix64(base)
    }

    pub fn next_u64(&mut self) -> u64 {
        let value = self.peek_u64();
        self.draw_count = self.draw_count.saturating_add(1);
        value
    }

    #[must_use]
    fn is_valid(&self) -> bool {
        self.seed.iter().any(|byte| *byte != 0)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct EcologyState {
    pub schema_version: u32,
    pub identity_seed: u64,
    pub next_object_id: u64,
    pub objects: Vec<WorldObject>,
    pub den: DenState,
    pub metabolism: MetabolicState,
    pub taste: TasteProfile,
    pub skills: MimesisLibrary,
    pub object_memories: Vec<ObjectMemory>,
    pub episode_stats: EpisodeStats,
    pub rng: SavedEcologyRng,
}

impl Default for EcologyState {
    fn default() -> Self {
        Self::new(0x5045_5432_EC01_06A1)
    }
}

impl EcologyState {
    #[must_use]
    pub fn new(identity_seed: u64) -> Self {
        let identity_seed = identity_seed.max(1);
        let den = DenState::for_seed(identity_seed);
        let orb = WorldObject::canonical_orb(identity_seed, den.anchor);
        let mut next_object_id = super::object::splitmix64(orb.id).max(1);
        if next_object_id == orb.id {
            next_object_id = orb.id.wrapping_add(1).max(1);
        }
        Self {
            schema_version: ECOLOGY_STATE_SCHEMA_VERSION,
            identity_seed,
            next_object_id,
            objects: vec![orb],
            den,
            metabolism: MetabolicState::default(),
            taste: TasteProfile::default(),
            skills: MimesisLibrary {
                skills: Vec::new(),
                next_skill_id: 1,
            },
            object_memories: Vec::new(),
            episode_stats: EpisodeStats::default(),
            rng: SavedEcologyRng::for_seed(identity_seed ^ 0xEC01_06A1_5EED_0001),
        }
    }

    #[must_use]
    pub fn snapshot(&self) -> Self {
        self.clone()
    }

    pub fn restore(snapshot: Self) -> Result<Self, EcologyError> {
        snapshot.validate()?;
        Ok(snapshot)
    }

    pub fn validate(&self) -> Result<(), EcologyError> {
        if self.schema_version != ECOLOGY_STATE_SCHEMA_VERSION {
            return Err(EcologyError::UnsupportedSchema {
                found: self.schema_version,
                expected: ECOLOGY_STATE_SCHEMA_VERSION,
            });
        }
        if self.identity_seed == 0 {
            return Err(EcologyError::InvalidIdentitySeed);
        }
        if self.objects.len() > MAX_OBJECTS {
            return Err(EcologyError::TooManyObjects);
        }
        if self
            .objects
            .iter()
            .filter(|object| object.is_active_morsel())
            .count()
            > MAX_ACTIVE_MORSELS
        {
            return Err(EcologyError::TooManyMorsels);
        }
        if self
            .objects
            .iter()
            .filter(|object| object.kind == ObjectKind::Orb)
            .count()
            != 1
            || self
                .objects
                .iter()
                .find(|object| object.kind == ObjectKind::Orb)
                .is_none_or(|orb| orb.id != canonical_orb_id(self.identity_seed))
        {
            return Err(EcologyError::InvalidCanonicalOrbCount);
        }
        for (index, object) in self.objects.iter().enumerate() {
            object.validate()?;
            if self.objects[..index]
                .iter()
                .any(|other| other.id == object.id)
            {
                return Err(EcologyError::DuplicateObjectId);
            }
        }
        if self.next_object_id == 0
            || self
                .objects
                .iter()
                .any(|object| object.id == self.next_object_id)
        {
            return Err(EcologyError::InvalidNextObjectId);
        }
        self.den.validate()?;
        for (index, slot) in self.den.slots.iter().enumerate() {
            if let Some(object_id) = slot
                && (self.den.slots[..index].contains(&Some(*object_id))
                    || self
                        .objects
                        .iter()
                        .find(|object| object.id == *object_id)
                        .is_none_or(|object| object.lifecycle != ObjectLifecycle::StoredInDen))
            {
                return Err(EcologyError::InvalidDenSlot);
            }
        }
        self.metabolism.validate()?;
        self.taste.validate()?;
        self.skills.validate()?;
        if self.object_memories.len() > MAX_OBJECT_MEMORIES {
            return Err(EcologyError::TooManyObjectMemories);
        }
        for (index, memory) in self.object_memories.iter().enumerate() {
            if !memory.is_valid()
                || self.object_memories[..index]
                    .iter()
                    .any(|other| other.object_id == memory.object_id)
            {
                return Err(EcologyError::InvalidObjectMemory);
            }
        }
        if self.episode_stats.next_episode_id == 0 {
            return Err(EcologyError::InvalidEpisodeStats);
        }
        if !self.rng.is_valid() {
            return Err(EcologyError::InvalidRng);
        }
        Ok(())
    }

    pub fn ensure_canonical_orb(&mut self) {
        let expected_id = canonical_orb_id(self.identity_seed);
        if self
            .objects
            .iter()
            .any(|object| object.kind == ObjectKind::Orb && object.id == expected_id)
        {
            return;
        }
        self.objects.retain(|object| object.kind != ObjectKind::Orb);
        if self.objects.len() < MAX_OBJECTS {
            self.objects.push(WorldObject::canonical_orb(
                self.identity_seed,
                self.den.anchor,
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_deterministic_and_has_one_canonical_orb() {
        let first = EcologyState::new(42);
        let second = EcologyState::new(42);
        assert_eq!(first, second);
        first.validate().unwrap();
        assert_eq!(first.objects.len(), 1);
        assert_eq!(first.objects[0].kind, ObjectKind::Orb);
    }

    #[test]
    fn invalid_float_is_rejected() {
        let mut state = EcologyState::new(42);
        state.objects[0].position.x = f32::NAN;
        assert_eq!(state.validate(), Err(EcologyError::InvalidObject));
    }
}
