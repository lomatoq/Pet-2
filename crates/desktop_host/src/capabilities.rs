use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlatformCapabilities {
    pub global_cursor: bool,
    pub user_idle_time: bool,
    pub active_application: bool,
    pub visible_window_geometry: bool,
    pub transparent_overlay: bool,
    pub dynamic_cursor_hittest: bool,
    pub nonactivating_overlay: bool,
    pub multiple_monitors: bool,
    pub microphone: bool,
    pub camera: bool,
    pub screen_capture: bool,
}

impl PlatformCapabilities {
    #[must_use]
    pub const fn portable_fallback() -> Self {
        Self {
            global_cursor: false,
            user_idle_time: false,
            active_application: false,
            visible_window_geometry: false,
            transparent_overlay: true,
            dynamic_cursor_hittest: true,
            nonactivating_overlay: true,
            multiple_monitors: true,
            microphone: false,
            camera: false,
            screen_capture: false,
        }
    }
}
