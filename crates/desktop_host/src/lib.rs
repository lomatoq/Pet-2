//! Native desktop integration boundary for Pet 2.
//!
//! Platform APIs and conditional compilation are confined to `platform`. The
//! public contract carries only portable coordinates, capabilities, and sensor data.

mod capabilities;
mod contract;
mod coordinates;
mod lab_control;
mod overlay;
mod platform;
mod sensors;
mod storage;

pub use capabilities::*;
pub use contract::*;
pub use coordinates::*;
pub use lab_control::*;
pub use overlay::*;
pub use platform::{create_platform_backend, prepare_overlay_window_attributes};
pub use sensors::*;
pub use storage::*;
