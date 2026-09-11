use rand::{RngCore, SeedableRng};
use rand_chacha::ChaCha8Rng;
use serde::{Deserialize, Serialize};

use crate::{
    EcologyError, HabitatObject, MimesisSkill, ObjectId, ObjectKind, ObjectLifecycle,
    PersistentDen, TasteMemoryEntry, MAX_HABITAT_OBJECTS, MAX_MIMESIS_SKILLS, MAX_TASTE_MEMORIES,
};

pub const ECOLOGY_SCHEMA_VERSION: u32 = 1;
pub const EPISODE_GOAL_COUNT: usize = 15;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct EcologyState {
    pub schema_version: u32,
    pub identity_seed: u64,
    pub rng: SavedEcologyRng,
    pub objects: Vec<HabitatObject>,
    pub den: PersistentDen,
    pub taste_memory: Vec<TasteMemoryEntry>,
    pub mimesis_skills: Vec<MimesisSkill>,
    pub episode_stats: EpisodeStats,
    pub next_object_id: u64,
    pub elapsed_seconds: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct EpisodeStats {
    pub started: [u64; EPISODE_GOAL_COUNT],
    pub completed: [u64; EPISODE_GOAL_COUNT],
    pub aborted: [u64; EPISODE_GOAL_COUNT],
    pub interrupted_by_shutdown: u64,
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
    #[allow(clippy::chunks_exact_to_as_chunks)]
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
    pub fn restore(&self) -> ChaCha8Rng {
        let mut rng = ChaCha8Rng::from_seed(self.seed);
        for _ in 0..self.draw_count {
            let _ = rng.next_u64();
        }
        rng
    }
}

impl EcologyState {
    #[must_use]
    pub fn new(identity_seed: u64) -> Self {
        let rng = SavedEcologyRng::for_seed(identity_seed);
        let mut object_rng = rng.restore();
        let orb = HabitatObject::new_orb(ObjectId(1), &mut object_rng);
        Self {
            schema_version: ECOLOGY_SCHEMA_VERSION,
            identity_seed,
            rng,
            objects: vec![orb],
            den: PersistentDen::default(),
            taste_memory: Vec::new(),
            mimesis_skills: Vec::new(),
            episode_stats: EpisodeStats::default(),
            next_object_id: 2,
            elapsed_seconds: 0.0,
        }
    }

    pub fn validate(&self) -> Result<(), EcologyError> {
        if self.schema_version != ECOLOGY_SCHEMA_VERSION {
            return Err(EcologyError::InvalidState(format!(
                "unsupported ecology schema {}",
                self.schema_version
            )));
        }
        if self.objects.len() > MAX_HABITAT_OBJECTS {
            return Err(EcologyError::InvalidState(format!(
                "object capacity exceeded: {}",
                self.objects.len()
            )));
        }
        if self.taste_memory.len() > MAX_TASTE_MEMORIES {
            return Err(EcologyError::InvalidState(format!(
                "taste-memory capacity exceeded: {}",
                self.taste_memory.len()
            )));
        }
        if self.mimesis_skills.len() > MAX_MIMESIS_SKILLS {
            return Err(EcologyError::InvalidState(format!(
                "mimesis-skill capacity exceeded: {}",
                self.mimesis_skills.len()
            )));
        }
        let mut seen = std::collections::HashSet::new();
        for object in &self.objects {
            object.validate()?;
            if !seen.insert(object.id) {
                return Err(EcologyError::InvalidState(format!(
                    "duplicate object id {}",
                    object.id.0
                )));
            }
        }
        let canonical_orbs = self
            .objects
            .iter()
            .filter(|object| object.kind == ObjectKind::Orb)
            .count();
        if canonical_orbs != 1 {
            return Err(EcologyError::InvalidState(format!(
                "expected exactly one canonical orb, found {canonical_orbs}"
            )));
        }
        self.den.validate()?;
        for slot in &self.den.slots {
            if let Some(object_id) = slot.object_id {
                let Some(object) = self.objects.iter().find(|object| object.id == object_id) else {
                    return Err(EcologyError::InvalidState(format!(
                        "den slot references missing object {}",
                        object_id.0
                    )));
                };
                if object.lifecycle != ObjectLifecycle::Stored {
                    return Err(EcologyError::InvalidState(format!(
                        "den slot object {} is not stored",
                        object_id.0
                    )));
                }
            }
        }
        for taste in &self.taste_memory {
            taste.validate()?;
        }
        for skill in &self.mimesis_skills {
            skill.validate()?;
        }
        if self.next_object_id == 0 {
            return Err(EcologyError::InvalidState(
                "next object id must be non-zero".into(),
            ));
        }
        if !self.elapsed_seconds.is_finite() || self.elapsed_seconds < 0.0 {
            return Err(EcologyError::InvalidState(
                "elapsed_seconds must be finite and non-negative".into(),
            ));
        }
        Ok(())
    }
}
