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
mod repertoire_catalog;
mod repertoire_runtime;
mod runtime;
mod selector;
mod surface;
mod validation;
mod virtual_interoception;
mod virtual_throat;

pub use bus::*;
pub use catalog::*;
pub use contracts::*;
pub use field::*;
pub use phase::*;
pub use repertoire_catalog::*;
pub use repertoire_runtime::*;
pub use runtime::*;
pub use selector::*;
pub use surface::*;
pub use validation::*;
pub use virtual_interoception::*;
pub use virtual_throat::*;
