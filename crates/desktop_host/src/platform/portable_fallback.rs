use std::{path::PathBuf, time::Instant};

use directories::ProjectDirs;
use winit::window::Window;

use crate::{
    DesktopSnapshot, DisplayTopology, HostError, PlatformBackend, PlatformCapabilities,
    PlatformKind,
};

pub struct PortableFallback {
    started: Instant,
}

impl Default for PortableFallback {
    fn default() -> Self {
        Self {
            started: Instant::now(),
        }
    }
}

impl PlatformBackend for PortableFallback {
    fn kind(&self) -> PlatformKind {
        PlatformKind::PortableFallback
    }

    fn capabilities(&self) -> PlatformCapabilities {
        PlatformCapabilities::portable_fallback()
    }

    fn initialize(&mut self, _window: &Window) -> Result<(), HostError> {
        Ok(())
    }

    fn poll_desktop(&mut self, _topology: &DisplayTopology) -> DesktopSnapshot {
        DesktopSnapshot::unavailable(self.started.elapsed().as_secs_f64(), _topology.revision)
    }

    fn apply_overlay_policy(&mut self, _window: &Window) -> Result<(), HostError> {
        Ok(())
    }

    fn app_data_directory(&self) -> Result<PathBuf, HostError> {
        ProjectDirs::from("io", "lomatoq", "Pet 2")
            .map(|dirs| dirs.data_local_dir().to_path_buf())
            .ok_or(HostError::AppDataUnavailable)
    }
}
