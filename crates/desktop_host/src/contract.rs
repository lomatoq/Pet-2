use std::path::PathBuf;

use glam::Vec2;
use thiserror::Error;
use winit::window::Window;

use crate::{DesktopSnapshot, DesktopVisualSample, DisplayTopology, PlatformCapabilities};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlatformKind {
    Windows,
    MacOs,
    PortableFallback,
}

#[derive(Debug, Error)]
pub enum HostError {
    #[error("native window handle is unavailable: {0}")]
    WindowHandle(String),
    #[error("platform operation failed: {0}")]
    Platform(String),
    #[error("application data directory is unavailable")]
    AppDataUnavailable,
}

pub trait PlatformBackend {
    fn kind(&self) -> PlatformKind;
    fn capabilities(&self) -> PlatformCapabilities;
    fn initialize(&mut self, window: &Window) -> Result<(), HostError>;
    fn poll_desktop(&mut self, topology: &DisplayTopology) -> DesktopSnapshot;
    fn poll_visual_features(
        &mut self,
        _topology: &DisplayTopology,
        _pet_position: Vec2,
    ) -> Option<DesktopVisualSample> {
        None
    }
    fn apply_overlay_policy(&mut self, window: &Window) -> Result<(), HostError>;
    fn set_cursor_hittest(&mut self, window: &Window, enabled: bool) -> Result<(), HostError> {
        window.set_cursor_hittest(enabled).map_err(|error| {
            HostError::Platform(format!("could not update cursor hit testing: {error}"))
        })
    }
    fn app_data_directory(&self) -> Result<PathBuf, HostError>;
    fn on_suspend(&mut self) {}
    fn on_resume(&mut self) {}
    fn shutdown(&mut self) {}
}
