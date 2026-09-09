use std::path::PathBuf;

use glam::Vec2;
use thiserror::Error;
use winit::window::Window;

use crate::{DesktopSnapshot, DesktopVisualFrame, DisplayTopology, PlatformCapabilities, RectI};

/// An owned, top-down BGRA8 snapshot produced off the realtime/render thread.
#[derive(Debug, Clone)]
pub struct DesktopBackgroundFrame {
    pub width: u32,
    pub height: u32,
    pub bytes_per_row: u32,
    pub bgra8: Vec<u8>,
    pub physical_rect: RectI,
    /// The captured rectangle in overlay-local normalized coordinates, encoded
    /// as `[minimum_x, minimum_y, width, height]`. The den shader uses this to
    /// map its screen-global UVs into the compact capture texture.
    pub normalized_region: [f32; 4],
    pub sequence: u64,
    pub timestamp: f64,
    pub mean_luminance: f32,
    pub contrast: f32,
}

/// A compact overlay-local region requested for desktop-background capture.
/// Keeping the maximum texture dimension explicit prevents a small optical
/// effect from uploading a virtual-desktop-sized image every render frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DesktopBackgroundCaptureRegion {
    pub minimum_normalized: Vec2,
    pub maximum_normalized: Vec2,
    pub maximum_output_dimension: u32,
}

impl DesktopBackgroundCaptureRegion {
    #[must_use]
    pub fn new(
        minimum_normalized: Vec2,
        maximum_normalized: Vec2,
        maximum_output_dimension: u32,
    ) -> Option<Self> {
        let value = Self {
            minimum_normalized,
            maximum_normalized,
            maximum_output_dimension,
        };
        (minimum_normalized.is_finite()
            && maximum_normalized.is_finite()
            && minimum_normalized.cmpge(Vec2::ZERO).all()
            && maximum_normalized.cmple(Vec2::ONE).all()
            && maximum_normalized.cmpgt(minimum_normalized).all()
            && maximum_output_dimension > 0)
            .then_some(value)
    }
}

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
    ) -> Option<DesktopVisualFrame> {
        None
    }
    fn capture_overlay_background(
        &mut self,
        _window: &Window,
        _region: DesktopBackgroundCaptureRegion,
    ) -> Option<DesktopBackgroundFrame> {
        None
    }
    /// True when every captured background excludes the organism at source.
    /// Such frames can refresh the lens directly without retaining stale pixels.
    fn overlay_background_excludes_pet(&self) -> bool {
        false
    }
    /// Temporarily removes the overlay from OS-level desktop capture. Production
    /// uses this only while seeding the den's clean refraction history, then
    /// immediately restores normal capture behavior.
    fn set_overlay_capture_excluded(
        &mut self,
        _window: &Window,
        _excluded: bool,
    ) -> Result<(), HostError> {
        Ok(())
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
