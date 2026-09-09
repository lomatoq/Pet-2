//! Phase-based motor cortex for Pet 2.
//!
//! `lifecore` remains the only owner of motives, action selection, memory, and
//! learning. This crate turns an already-selected semantic goal into one
//! deterministic, target-locked bodily bout with explicit phases, bounded
//! local fields, surface support, and causal telemetry.

mod bus;
mod catalog;
mod contracts;
mod feedback;
mod field;
mod phase;
mod programs;
mod runtime;
mod selector;
mod surface;
mod validation;

pub use bus::*;
pub use catalog::*;
pub use contracts::*;
pub use field::*;
pub use phase::*;
pub use runtime::*;
pub use selector::*;
pub use surface::*;
pub use validation::*;
