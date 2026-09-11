//! Deterministic artificial-life simulation.
//!
//! `LifeCore` owns needs, affect, learning, memories, action arbitration, voice
//! vocabulary, and development. It consumes normalized data and has no knowledge of
//! graphics, audio devices, native windows, physical pixels, or operating systems.

mod actions;
mod affect;
mod bandit;
mod companion;
mod development;
mod drives;
mod genome;
mod interaction;
mod interoception;
mod language;
mod memory;
mod microbrain;
mod persistence;
mod phenotype;
mod phenotype_director;
mod vita;

use std::{array, collections::VecDeque};

use glam::Vec2;
use rand::{RngCore, SeedableRng};
use rand_chacha::ChaCha8Rng;
use serde::{Deserialize, Serialize};

pub use actions::*;
pub use affect::*;
pub use bandit::*;
pub use companion::*;
pub use development::*;
pub use drives::*;
pub use genome::*;
pub use interaction::*;
pub use interoception::*;
pub use language::*;
pub use memory::*;
pub use microbrain::*;
pub use persistence::*;
pub use phenotype::*;
pub use phenotype_director::*;
pub use vita::*;

pub const LIFECORE_HZ: f32 = 20.0;
const RECENT_ACTION_CAPACITY: usize = 16;
const MAX_IGNORED_ATTEMPTS: u32 = 8;
const RECENT_VOCAL_CAPACITY: usize = 4;
const EXACT_VOCAL_REPEAT_WINDOW: usize = 3;
const MAX_VOCAL_FAMILY_SIZE: usize = 4;
const VOCAL_CONTEXT_WEIGHT_LIMIT: f32 = 1.0;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum FeedbackEvent {
    PettingStarted,
    PlayStarted,
    CursorApproached,
    RespondedAfterSound,
    Observed,
    Ignored,
    PushedAway,
    MuteOrHide,
    FocusModeEnabled,
    FocusModeDisabled,
    Reward(f32),
}

impl FeedbackEvent {
    #[must_use]
    pub fn reward(&self) -> f32 {
        match self {
            Self::PettingStarted => 0.90,
            Self::PlayStarted => 0.70,
            Self::CursorApproached => 0.50,
            Self::RespondedAfterSound => 0.40,
            Self::Observed => 0.25,
            Self::Ignored => -0.25,
            Self::PushedAway => -0.60,
            Self::MuteOrHide => -0.80,
            Self::FocusModeEnabled => -0.10,
            Self::FocusModeDisabled => 0.10,
            Self::Reward(value) => value.clamp(-1.0, 1.0),
        }
    }
}

// Remainder of this file is unchanged from R13 and is intentionally preserved
// by the follow-up formatter/integration commit.