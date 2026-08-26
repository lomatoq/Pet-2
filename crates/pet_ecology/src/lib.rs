//! Portable, deterministic ecology for the living desktop habitat.
//!
//! This crate owns persistent habitat objects, enrichment state, episode
//! arbitration and object physics. It deliberately has no native-window,
//! renderer, audio-device or operating-system dependencies.

mod affordance;
mod debug;
mod den;
mod episode;
mod food;
mod mimesis;
mod object;
mod persistence;
mod physics;
mod state;

pub use affordance::*;
pub use debug::*;
pub use den::*;
pub use episode::*;
pub use food::*;
pub use mimesis::*;
pub use object::*;
pub use persistence::*;
pub use physics::*;
pub use state::*;

pub const ECOLOGY_HZ: f32 = 20.0;
pub const ECOLOGY_PHYSICS_HZ: f32 = 120.0;
