use thiserror::Error;

/// Validation failures are intentionally specific so a corrupt primary save can
/// be rejected and the host can fall back to its previous atomic snapshot.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum EcologyError {
    #[error("unsupported ecology schema {found}; expected {expected}")]
    UnsupportedSchema { found: u32, expected: u32 },
    #[error("ecology identity seed must be non-zero")]
    InvalidIdentitySeed,
    #[error("ecology contains too many objects")]
    TooManyObjects,
    #[error("ecology contains too many active morsels")]
    TooManyMorsels,
    #[error("ecology must contain exactly one canonical orb")]
    InvalidCanonicalOrbCount,
    #[error("ecology contains a duplicate object id")]
    DuplicateObjectId,
    #[error("ecology next object id collides with a persisted object")]
    InvalidNextObjectId,
    #[error("ecology object contains invalid or out-of-bounds data")]
    InvalidObject,
    #[error("ecology den contains invalid data")]
    InvalidDen,
    #[error("ecology den references an absent or duplicate object")]
    InvalidDenSlot,
    #[error("ecology metabolism contains invalid data")]
    InvalidMetabolism,
    #[error("ecology taste profile contains invalid data")]
    InvalidTaste,
    #[error("ecology skill library contains invalid data")]
    InvalidSkills,
    #[error("ecology contains too many object memories")]
    TooManyObjectMemories,
    #[error("ecology object memory contains invalid data")]
    InvalidObjectMemory,
    #[error("ecology episode statistics contain invalid data")]
    InvalidEpisodeStats,
    #[error("ecology RNG snapshot contains invalid data")]
    InvalidRng,
}
