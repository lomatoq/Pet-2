//! Procedural voice synthesis shared by every platform.

mod engine;
mod envelope;
mod filter;
mod motif;
mod mutation;
mod oscillator;
mod ring;

pub use engine::*;
pub use motif::*;
pub use mutation::*;
pub use ring::SpscRing;
