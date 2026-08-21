use std::{path::PathBuf, time::Instant};

use directories::ProjectDirs;
use lifecore::AppCategory;
use objc2::rc::Retained;
use objc2_app_kit::{
    NSColor, NSFloatingWindowLevel, NSView, NSWindow, NSWindowCollectionBehavior, NSWorkspace,
};
use objc2_core_foundation::{CFArray, CFDictionary, CFNumber, CFString, CFType, CGRect};
use objc2_core_graphics::{
    CGEvent, CGEventSource, CGEventSourceStateID, CGEventType,
    CGRectMakeWithDictionaryRepresentation, CGWindowListCopyWindowInfo, CGWindowListOption,
    kCGNullWindowID, kCGWindowBounds, kCGWindowLayer, kCGWindowNumber, kCGWindowOwnerPID,
};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use winit::window::Window;

use crate::{
    ApplicationInfo, DesktopSnapshot, DesktopSurface, DisplayTopology, HostError,
    PhysicalDesktopPoint, PlatformBackend, PlatformCapabilities, PlatformKind, RectI,
};

pub struct MacOsBackend {
    started: Instant,
    window: Option<Retained<NSWindow>>,
    window_geometry_available: bool,
}

impl Default for MacOsBackend {
    fn default() -> Self {
        Self {
            started: Instant::now(),
            window: None,
            window_geometry_available: true,
        }
    }
}

impl PlatformBackend for MacOsBackend {
    fn kind(&self) -> PlatformKind {
        PlatformKind::MacOs
    }

    fn capabilities(&self) -> PlatformCapabilities {
        PlatformCapabilities {
            global_cursor: true,
            user_idle_time: true,
            active_application: true,
            visible_window_geometry: self.window_geometry_available,
            transparent_overlay: true,
            dynamic_cursor_hittest: true,
            nonactivating_overlay: true,
            multiple_monitors: true,
            microphone: false,
            camera: false,
            screen_capture: false,
        }
    }

    fn initialize(&mut self, window: &Window) -> Result<(), HostError> {
        let handle = window
            .window_handle()
            .map_err(|error| HostError::WindowHandle(error.to_string()))?;
        let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
            return Err(HostError::WindowHandle("expected an AppKit NSView".into()));
        };
        let view: Retained<NSView> = unsafe { Retained::retain(handle.ns_view.as_ptr().cast()) }
            .ok_or_else(|| HostError::WindowHandle("could not retain NSView".into()))?;
        self.window = view.window();
        if self.window.is_none() {
            return Err(HostError::WindowHandle(
                "NSView is not attached to an NSWindow".into(),
            ));
        }
        Ok(())
    }

    fn poll_desktop(&mut self, _topology: &DisplayTopology) -> DesktopSnapshot {
        let cursor = CGEvent::new(None).map(|event| {
            let point = CGEvent::location(Some(&event));
            PhysicalDesktopPoint {
                x: point.x.round() as i32,
                y: point.y.round() as i32,
            }
        });
        let idle = CGEventSource::seconds_since_last_event_type(
            CGEventSourceStateID::CombinedSessionState,
            CGEventType(u32::MAX),
        );
        let workspace = NSWorkspace::sharedWorkspace();
        let frontmost = workspace.frontmostApplication();
        let frontmost_pid = frontmost
            .as_ref()
            .map(|application| application.processIdentifier());
        let active_application = frontmost.map(|application| {
            let identifier = application
                .bundleIdentifier()
                .or_else(|| application.localizedName())
                .map(|name| name.to_string());
            let category = identifier
                .as_deref()
                .map(classify_application)
                .unwrap_or(AppCategory::Unknown);
            ApplicationInfo {
                category,
                transient_identifier: identifier,
            }
        });
        let quartz_windows = quartz_windows();
        self.window_geometry_available = quartz_windows.is_some();
        let quartz_windows = quartz_windows.unwrap_or_default();
        let active_window = frontmost_pid.and_then(|pid| {
            quartz_windows
                .iter()
                .find(|window| window.owner_pid == pid)
                .map(|window| window.surface.bounds)
        });
        let visible_surfaces = quartz_windows
            .into_iter()
            .map(|window| window.surface)
            .collect();
        DesktopSnapshot {
            timestamp: self.started.elapsed().as_secs_f64(),
            topology_revision: _topology.revision,
            cursor,
            idle_seconds: idle.is_finite().then_some(idle as f32),
            active_application,
            active_window,
            visible_surfaces,
        }
    }

    fn apply_overlay_policy(&mut self, _window: &Window) -> Result<(), HostError> {
        let window = self
            .window
            .as_ref()
            .ok_or_else(|| HostError::WindowHandle("backend is not initialized".into()))?;
        window.setOpaque(false);
        window.setBackgroundColor(Some(&NSColor::clearColor()));
        window.setHasShadow(false);
        window.setHidesOnDeactivate(false);
        window.setIgnoresMouseEvents(true);
        window.setLevel(NSFloatingWindowLevel);
        window.setCollectionBehavior(
            NSWindowCollectionBehavior::CanJoinAllSpaces
                | NSWindowCollectionBehavior::FullScreenAuxiliary
                | NSWindowCollectionBehavior::IgnoresCycle,
        );
        Ok(())
    }

    fn set_cursor_hittest(&mut self, window: &Window, enabled: bool) -> Result<(), HostError> {
        window.set_cursor_hittest(enabled).map_err(|error| {
            HostError::Platform(format!("could not update AppKit hit testing: {error}"))
        })?;
        let native = self
            .window
            .as_ref()
            .ok_or_else(|| HostError::WindowHandle("backend is not initialized".into()))?;
        native.setIgnoresMouseEvents(!enabled);
        Ok(())
    }

    fn app_data_directory(&self) -> Result<PathBuf, HostError> {
        ProjectDirs::from("io", "lomatoq", "Pet 2")
            .map(|dirs| dirs.data_local_dir().to_path_buf())
            .ok_or(HostError::AppDataUnavailable)
    }
}

struct QuartzWindow {
    owner_pid: i32,
    surface: DesktopSurface,
}

fn quartz_windows() -> Option<Vec<QuartzWindow>> {
    let array = CGWindowListCopyWindowInfo(
        CGWindowListOption::OptionOnScreenOnly | CGWindowListOption::ExcludeDesktopElements,
        kCGNullWindowID,
    )?;
    let dictionaries: &CFArray<CFDictionary> = unsafe { array.cast_unchecked() };
    let mut windows = Vec::with_capacity(dictionaries.len().min(128));
    for dictionary in dictionaries {
        if let Some(window) = quartz_window(&dictionary) {
            windows.push(window);
        }
    }
    Some(windows)
}

fn quartz_window(dictionary: &CFDictionary) -> Option<QuartzWindow> {
    if dictionary_number(dictionary, unsafe { kCGWindowLayer })? != 0 {
        return None;
    }
    let window_number = dictionary_number(dictionary, unsafe { kCGWindowNumber })?;
    let owner_pid = dictionary_number(dictionary, unsafe { kCGWindowOwnerPID })? as i32;
    let typed: &CFDictionary<CFString, CFType> = unsafe { dictionary.cast_unchecked() };
    let bounds_value = typed.get(unsafe { kCGWindowBounds })?;
    let bounds_dictionary = bounds_value.downcast::<CFDictionary>().ok()?;
    let mut rectangle = CGRect::ZERO;
    if !unsafe { CGRectMakeWithDictionaryRepresentation(Some(&bounds_dictionary), &mut rectangle) }
    {
        return None;
    }
    let bounds = RectI {
        minimum: PhysicalDesktopPoint {
            x: rectangle.origin.x.round() as i32,
            y: rectangle.origin.y.round() as i32,
        },
        maximum: PhysicalDesktopPoint {
            x: (rectangle.origin.x + rectangle.size.width).round() as i32,
            y: (rectangle.origin.y + rectangle.size.height).round() as i32,
        },
    };
    (bounds.width() >= 80 && bounds.height() >= 50).then(|| QuartzWindow {
        owner_pid,
        surface: DesktopSurface {
            transient_id: format!("window:{window_number}"),
            bounds,
        },
    })
}

fn dictionary_number(dictionary: &CFDictionary, key: &CFString) -> Option<i64> {
    let typed: &CFDictionary<CFString, CFType> = unsafe { dictionary.cast_unchecked() };
    typed.get(key)?.downcast::<CFNumber>().ok()?.as_i64()
}

fn classify_application(identifier: &str) -> AppCategory {
    let lower = identifier.to_lowercase();
    if ["xcode", "code", "jetbrains", "office", "notion", "obsidian"]
        .iter()
        .any(|needle| lower.contains(needle))
    {
        AppCategory::FocusedWork
    } else if ["adobe", "blender", "figma", "krita", "resolve"]
        .iter()
        .any(|needle| lower.contains(needle))
    {
        AppCategory::Creative
    } else if ["slack", "discord", "teams", "telegram", "signal"]
        .iter()
        .any(|needle| lower.contains(needle))
    {
        AppCategory::Communication
    } else if ["steam", "spotify", "vlc", "netflix"]
        .iter()
        .any(|needle| lower.contains(needle))
    {
        AppCategory::Entertainment
    } else if lower.starts_with("com.apple.") {
        AppCategory::System
    } else {
        AppCategory::Unknown
    }
}
