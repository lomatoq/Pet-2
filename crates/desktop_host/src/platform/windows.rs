use std::{
    ffi::c_void,
    mem::size_of,
    path::{Path, PathBuf},
    time::Instant,
};

use directories::ProjectDirs;
use lifecore::AppCategory;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use windows_sys::Win32::{
    Foundation::{CloseHandle, HWND, LPARAM, POINT, RECT},
    Graphics::Dwm::{DWMWA_EXTENDED_FRAME_BOUNDS, DwmGetWindowAttribute},
    System::{
        SystemInformation::GetTickCount64,
        Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW},
    },
    UI::{
        Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO},
        WindowsAndMessaging::{
            EnumWindows, GWL_EXSTYLE, GetCursorPos, GetForegroundWindow, GetWindowLongPtrW,
            GetWindowRect, GetWindowThreadProcessId, HWND_TOPMOST, IsWindowVisible,
            SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SetWindowLongPtrW,
            SetWindowPos, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
        },
    },
};
use winit::platform::windows::WindowAttributesExtWindows;
use winit::window::{Window, WindowAttributes};

use crate::{
    ApplicationInfo, DesktopSnapshot, DesktopSurface, DisplayTopology, HostError,
    PhysicalDesktopPoint, PlatformBackend, PlatformCapabilities, PlatformKind, RectI,
};

pub struct WindowsBackend {
    started: Instant,
    overlay_hwnd: Option<HWND>,
}

pub(crate) fn prepare_overlay_window_attributes(attributes: WindowAttributes) -> WindowAttributes {
    // DirectComposition owns presentation for this window, so the legacy DWM
    // redirection bitmap must not sit behind the transparent swapchain.
    attributes.with_no_redirection_bitmap(true)
}

impl Default for WindowsBackend {
    fn default() -> Self {
        Self {
            started: Instant::now(),
            overlay_hwnd: None,
        }
    }
}

impl PlatformBackend for WindowsBackend {
    fn kind(&self) -> PlatformKind {
        PlatformKind::Windows
    }

    fn capabilities(&self) -> PlatformCapabilities {
        PlatformCapabilities {
            global_cursor: true,
            user_idle_time: true,
            active_application: true,
            visible_window_geometry: true,
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
        let RawWindowHandle::Win32(handle) = handle.as_raw() else {
            return Err(HostError::WindowHandle("expected a Win32 HWND".into()));
        };
        self.overlay_hwnd = Some(handle.hwnd.get() as HWND);
        Ok(())
    }

    fn poll_desktop(&mut self, _topology: &DisplayTopology) -> DesktopSnapshot {
        let foreground = unsafe { GetForegroundWindow() };
        let cursor = cursor_position();
        let idle_seconds = idle_seconds();
        let active_window = window_bounds(foreground);
        let active_application = application_for_window(foreground);
        let mut context = EnumerationContext {
            overlay: self.overlay_hwnd,
            surfaces: Vec::with_capacity(32),
        };
        unsafe {
            EnumWindows(
                Some(enum_window),
                &mut context as *mut EnumerationContext as LPARAM,
            );
        }
        DesktopSnapshot {
            timestamp: self.started.elapsed().as_secs_f64(),
            topology_revision: _topology.revision,
            cursor,
            idle_seconds,
            active_application,
            active_window,
            visible_surfaces: context.surfaces,
        }
    }

    fn apply_overlay_policy(&mut self, _window: &Window) -> Result<(), HostError> {
        let hwnd = self
            .overlay_hwnd
            .ok_or_else(|| HostError::WindowHandle("backend is not initialized".into()))?;
        unsafe {
            let mut style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
            style |= WS_EX_NOACTIVATE;
            style |= WS_EX_TOOLWINDOW;
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, style as isize);
            let success = SetWindowPos(
                hwnd,
                HWND_TOPMOST,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_FRAMECHANGED,
            );
            if success == 0 {
                return Err(HostError::Platform("SetWindowPos failed".into()));
            }
        }
        Ok(())
    }

    fn app_data_directory(&self) -> Result<PathBuf, HostError> {
        ProjectDirs::from("io", "lomatoq", "Pet 2")
            .map(|dirs| dirs.data_local_dir().to_path_buf())
            .ok_or(HostError::AppDataUnavailable)
    }
}

struct EnumerationContext {
    overlay: Option<HWND>,
    surfaces: Vec<DesktopSurface>,
}

unsafe extern "system" fn enum_window(hwnd: HWND, parameter: LPARAM) -> i32 {
    let context = unsafe { &mut *(parameter as *mut EnumerationContext) };
    if Some(hwnd) == context.overlay || unsafe { IsWindowVisible(hwnd) } == 0 {
        return 1;
    }
    if let Some(bounds) = window_bounds(hwnd)
        && bounds.width() >= 80
        && bounds.height() >= 50
    {
        context.surfaces.push(DesktopSurface {
            transient_id: format!("window:{hwnd:p}"),
            bounds,
        });
    }
    1
}

fn cursor_position() -> Option<PhysicalDesktopPoint> {
    let mut point = POINT { x: 0, y: 0 };
    (unsafe { GetCursorPos(&mut point) } != 0).then_some(PhysicalDesktopPoint {
        x: point.x,
        y: point.y,
    })
}

fn idle_seconds() -> Option<f32> {
    let mut info = LASTINPUTINFO {
        cbSize: size_of::<LASTINPUTINFO>() as u32,
        dwTime: 0,
    };
    if unsafe { GetLastInputInfo(&mut info) } == 0 {
        return None;
    }
    let elapsed_milliseconds = (unsafe { GetTickCount64() } as u32).wrapping_sub(info.dwTime);
    Some(elapsed_milliseconds as f32 / 1_000.0)
}

fn window_bounds(hwnd: HWND) -> Option<RectI> {
    if hwnd.is_null() {
        return None;
    }
    let mut rect = RECT::default();
    let dwm_result = unsafe {
        DwmGetWindowAttribute(
            hwnd,
            DWMWA_EXTENDED_FRAME_BOUNDS as u32,
            &mut rect as *mut RECT as *mut c_void,
            size_of::<RECT>() as u32,
        )
    };
    if dwm_result < 0 && unsafe { GetWindowRect(hwnd, &mut rect) } == 0 {
        return None;
    }
    let bounds = RectI {
        minimum: PhysicalDesktopPoint {
            x: rect.left,
            y: rect.top,
        },
        maximum: PhysicalDesktopPoint {
            x: rect.right,
            y: rect.bottom,
        },
    };
    bounds.is_valid().then_some(bounds)
}

fn application_for_window(hwnd: HWND) -> Option<ApplicationInfo> {
    if hwnd.is_null() {
        return None;
    }
    let mut process_id = 0_u32;
    unsafe { GetWindowThreadProcessId(hwnd, &mut process_id) };
    if process_id == 0 {
        return None;
    }
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, process_id) };
    if process.is_null() {
        return None;
    }
    let mut buffer = vec![0_u16; 32_768];
    let mut size = buffer.len() as u32;
    let success = unsafe { QueryFullProcessImageNameW(process, 0, buffer.as_mut_ptr(), &mut size) };
    unsafe { CloseHandle(process) };
    if success == 0 {
        return None;
    }
    let path = String::from_utf16_lossy(&buffer[..size as usize]);
    let name = Path::new(&path)
        .file_stem()
        .and_then(|name| name.to_str())
        .unwrap_or("unknown")
        .to_lowercase();
    Some(ApplicationInfo {
        category: classify_application(&name),
        transient_identifier: Some(name),
    })
}

fn classify_application(name: &str) -> AppCategory {
    if [
        "code", "devenv", "idea", "rider", "excel", "word", "obsidian",
    ]
    .iter()
    .any(|needle| name.contains(needle))
    {
        AppCategory::FocusedWork
    } else if ["photoshop", "blender", "figma", "krita", "resolve"]
        .iter()
        .any(|needle| name.contains(needle))
    {
        AppCategory::Creative
    } else if ["discord", "slack", "teams", "telegram", "signal"]
        .iter()
        .any(|needle| name.contains(needle))
    {
        AppCategory::Communication
    } else if ["steam", "spotify", "vlc", "netflix"]
        .iter()
        .any(|needle| name.contains(needle))
    {
        AppCategory::Entertainment
    } else if ["explorer", "settings", "taskmgr"]
        .iter()
        .any(|needle| name.contains(needle))
    {
        AppCategory::System
    } else {
        AppCategory::Unknown
    }
}
