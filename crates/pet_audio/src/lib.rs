//! Procedural voice synthesis shared by every platform.

mod engine;
mod envelope;
mod filter;
mod motif;
mod mutation;
mod oscillator;
mod ring;
mod visual_feedback;

pub use engine::*;
pub use motif::*;
pub use mutation::*;
pub use ring::SpscRing;
pub use visual_feedback::{
    AudioVisualBridge, AudioVisualFeedback, global_visual_bridge, global_visual_feedback,
};
