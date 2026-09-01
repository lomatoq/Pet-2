#[cfg(target_os = "macos")]
mod macos;
#[cfg(not(any(target_os = "windows", target_os = "macos")))]
mod portable_fallback;
#[cfg(any(target_os = "windows", target_os = "macos"))]
mod visual_sampling;
#[cfg(target_os = "windows")]
mod windows;

use crate::{DisplayTopology, PlatformBackend};
use winit::window::WindowAttributes;

#[must_use]
pub fn prepare_overlay_window_attributes(attributes: WindowAttributes) -> WindowAttributes {
    #[cfg(target_os = "windows")]
    {
        windows::prepare_overlay_window_attributes(attributes)
    }
    #[cfg(not(target_os = "windows"))]
    {
        attributes
    }
}

#[must_use]
pub fn create_platform_backend() -> Box<dyn PlatformBackend> {
    #[cfg(target_os = "windows")]
    {
        Box::new(windows::WindowsBackend::default())
    }
    #[cfg(target_os = "macos")]
    {
        Box::new(macos::MacOsBackend::default())
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        Box::new(portable_fallback::PortableFallback::default())
    }
}

/// Last-resort startup topology for platforms where the window event loop can
/// briefly report no monitors (observed on macOS during background relaunch).
/// Normal winit topology remains authoritative whenever it is valid.
#[must_use]
pub fn fallback_display_topology(revision: u64) -> Option<DisplayTopology> {
    #[cfg(target_os = "macos")]
    {
        macos::fallback_display_topology(revision)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = revision;
        None
    }
}
