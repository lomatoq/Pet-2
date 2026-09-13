//! Procedural voice synthesis shared by every platform.

mod body_bridge;
mod body_resonance;
mod breath;
mod endpoint_watcher;
mod engine;
mod filter;
mod glottis;
mod motif;
mod mutation;
mod noise;
mod nonphonated;
mod phonation;
mod prosody;
mod ring;
mod tract;
mod visual_feedback;
mod voice_diagnostics;

pub use body_bridge::*;
pub use endpoint_watcher::*;
pub use engine::*;
pub use motif::*;
pub use mutation::*;
pub use nonphonated::*;
pub use phonation::PhonationRegime;
pub use ring::SpscRing;
pub use visual_feedback::{
    AudioCallbackLevels, AudioVisualBridge, AudioVisualFeedback, global_visual_bridge,
    global_visual_feedback,
};
pub use voice_diagnostics::VoiceDiagnostics;
