//! Native desktop integration boundary for Pet 2.
//!
//! Platform APIs and conditional compilation are confined to `platform`. The
//! public contract carries only portable coordinates, capabilities, and sensor data.

mod acoustic_cues;
mod capabilities;
mod contract;
mod coordinates;
mod evolution_control;
mod lab_control;
mod local_audio_input;
mod overlay;
mod platform;
mod sensors;
mod storage;

pub use acoustic_cues::*;
pub use capabilities::*;
pub use contract::*;
pub use coordinates::*;
pub use evolution_control::*;
pub use lab_control::*;
pub use local_audio_input::*;
pub use overlay::*;
pub use platform::{
    create_platform_backend, fallback_display_topology, prepare_overlay_window_attributes,
};
pub use sensors::*;
pub use storage::*;
