use std::{
    ffi::c_void,
    mem::size_of,
    path::{Path, PathBuf},
    time::Instant,
};

use directories::ProjectDirs;
use glam::Vec2;
use lifecore::AppCategory;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use windows_sys::Win32::{
    Foundation::{CloseHandle, HWND, LPARAM, POINT, RECT},
    Graphics::{
        Dwm::{DWMWA_EXTENDED_FRAME_BOUNDS, DwmGetWindowAttribute},
        Gdi::{GetDC, GetPixel, HDC, ReleaseDC},
    },
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
    ApplicationInfo, DesktopSnapshot, DesktopSurface, DesktopVisualSample, DisplayTopology,
    HostError, PhysicalDesktopPoint, PlatformBackend, PlatformCapabilities, PlatformKind, RectI,
};

pub struct WindowsBackend {
    started: Instant,
    overlay_hwnd: Option<HWND>,
    previous_visual: Vec<[f32; 3]>,
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
            previous_visual: Vec::with_capacity(64),
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
            screen_capture: true,
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

    fn poll_visual_features(
        &mut self,
        topology: &DisplayTopology,
        pet_position: Vec2,
    ) -> Option<DesktopVisualSample> {
        sample_desktop_visual(topology, pet_position, &mut self.previous_visual)
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

fn sample_desktop_visual(
    topology: &DisplayTopology,
    pet_position: Vec2,
    previous: &mut Vec<[f32; 3]>,
) -> Option<DesktopVisualSample> {
    let virtual_bounds = topology.virtual_physical_bounds;
    if !virtual_bounds.is_valid() {
        return None;
    }
    let foreground = unsafe { GetForegroundWindow() };
    let global_bounds = window_bounds(foreground)
        .and_then(|bounds| intersect_rect(bounds, virtual_bounds))
        .unwrap_or(virtual_bounds);
    let pet_center = PhysicalDesktopPoint {
        x: virtual_bounds.minimum.x
            + (pet_position.x.clamp(0.0, 1.0) * virtual_bounds.width() as f32).round() as i32,
        y: virtual_bounds.minimum.y
            + (pet_position.y.clamp(0.0, 1.0) * virtual_bounds.height() as f32).round() as i32,
    };
    let local_bounds = intersect_rect(
        RectI {
            minimum: PhysicalDesktopPoint {
                x: pet_center.x - 96,
                y: pet_center.y - 96,
            },
            maximum: PhysicalDesktopPoint {
                x: pet_center.x + 97,
                y: pet_center.y + 97,
            },
        },
        virtual_bounds,
    )
    .unwrap_or(global_bounds);

    let device_context = unsafe { GetDC(std::ptr::null_mut()) };
    if device_context.is_null() {
        return None;
    }
    let mut samples = Vec::with_capacity(60);
    sample_grid(device_context, global_bounds, 7, 5, &mut samples);
    let global_count = samples.len();
    sample_grid(device_context, local_bounds, 5, 5, &mut samples);
    unsafe {
        ReleaseDC(std::ptr::null_mut(), device_context);
    }
    if global_count == 0 || samples.len() == global_count {
        return None;
    }

    let (mean_luminance, contrast, colorfulness, warmth, dominant_hue, edge_density) =
        summarize_samples(&samples[..global_count]);
    let local_luminance = samples[global_count..]
        .iter()
        .map(|sample| luminance(*sample))
        .sum::<f32>()
        / (samples.len() - global_count) as f32;
    let motion_energy = if previous.len() == samples.len() {
        samples
            .iter()
            .zip(previous.iter())
            .map(|(current, old)| {
                ((current[0] - old[0]).abs()
                    + (current[1] - old[1]).abs()
                    + (current[2] - old[2]).abs())
                    / 3.0
            })
            .sum::<f32>()
            / samples.len() as f32
    } else {
        0.0
    };
    let previous_mean = if previous.is_empty() {
        mean_luminance
    } else {
        previous
            .iter()
            .map(|sample| luminance(*sample))
            .sum::<f32>()
            / previous.len() as f32
    };
    let sudden_change =
        ((mean_luminance - previous_mean).abs() * 1.8 + motion_energy * 1.25).clamp(0.0, 1.0);
    *previous = samples;

    let sample = DesktopVisualSample {
        mean_luminance,
        local_luminance,
        contrast,
        colorfulness,
        warmth,
        dominant_hue,
        motion_energy: (motion_energy * 2.6).clamp(0.0, 1.0),
        edge_density,
        sudden_change,
    };
    sample.is_finite().then_some(sample)
}

fn sample_grid(
    device_context: HDC,
    bounds: RectI,
    columns: i32,
    rows: i32,
    output: &mut Vec<[f32; 3]>,
) {
    if !bounds.is_valid() || columns <= 0 || rows <= 0 {
        return;
    }
    for row in 0..rows {
        for column in 0..columns {
            let x = bounds.minimum.x
                + (((column as f32 + 0.5) / columns as f32) * bounds.width() as f32).round() as i32;
            let y = bounds.minimum.y
                + (((row as f32 + 0.5) / rows as f32) * bounds.height() as f32).round() as i32;
            let color = unsafe { GetPixel(device_context, x, y) };
            if color == u32::MAX {
                continue;
            }
            output.push([
                (color & 0xff) as f32 / 255.0,
                ((color >> 8) & 0xff) as f32 / 255.0,
                ((color >> 16) & 0xff) as f32 / 255.0,
            ]);
        }
    }
}

fn summarize_samples(samples: &[[f32; 3]]) -> (f32, f32, f32, f32, f32, f32) {
    if samples.is_empty() {
        return (0.5, 0.0, 0.0, 0.5, 0.0, 0.0);
    }
    let luminances: Vec<_> = samples.iter().map(|sample| luminance(*sample)).collect();
    let mean = luminances.iter().sum::<f32>() / luminances.len() as f32;
    let contrast = (luminances
        .iter()
        .map(|value| (*value - mean).powi(2))
        .sum::<f32>()
        / luminances.len() as f32)
        .sqrt()
        .mul_add(2.2, 0.0)
        .clamp(0.0, 1.0);
    let colorfulness = samples
        .iter()
        .map(|sample| {
            sample.iter().copied().fold(f32::NEG_INFINITY, f32::max)
                - sample.iter().copied().fold(f32::INFINITY, f32::min)
        })
        .sum::<f32>()
        / samples.len() as f32;
    let warmth = (0.5
        + samples
            .iter()
            .map(|sample| sample[0] - sample[2])
            .sum::<f32>()
            / samples.len() as f32
            * 0.5)
        .clamp(0.0, 1.0);
    let mut hue_vector = Vec2::ZERO;
    for sample in samples {
        let (hue, saturation) = hue_and_saturation(*sample);
        let angle = hue * std::f32::consts::TAU;
        hue_vector += Vec2::new(angle.cos(), angle.sin()) * saturation;
    }
    let dominant_hue = if hue_vector.length_squared() > 1.0e-6 {
        hue_vector
            .y
            .atan2(hue_vector.x)
            .rem_euclid(std::f32::consts::TAU)
            / std::f32::consts::TAU
    } else {
        0.0
    };
    let edge_density = if luminances.len() < 2 {
        0.0
    } else {
        (luminances
            .windows(2)
            .map(|pair| (pair[1] - pair[0]).abs())
            .sum::<f32>()
            / (luminances.len() - 1) as f32
            * 3.2)
            .clamp(0.0, 1.0)
    };
    (
        mean.clamp(0.0, 1.0),
        contrast,
        colorfulness.clamp(0.0, 1.0),
        warmth,
        dominant_hue,
        edge_density,
    )
}

fn luminance(sample: [f32; 3]) -> f32 {
    sample[0] * 0.2126 + sample[1] * 0.7152 + sample[2] * 0.0722
}

fn hue_and_saturation(sample: [f32; 3]) -> (f32, f32) {
    let maximum = sample.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let minimum = sample.iter().copied().fold(f32::INFINITY, f32::min);
    let delta = maximum - minimum;
    if delta <= 1.0e-6 || maximum <= 1.0e-6 {
        return (0.0, 0.0);
    }
    let hue = if maximum == sample[0] {
        ((sample[1] - sample[2]) / delta).rem_euclid(6.0)
    } else if maximum == sample[1] {
        (sample[2] - sample[0]) / delta + 2.0
    } else {
        (sample[0] - sample[1]) / delta + 4.0
    } / 6.0;
    (hue.rem_euclid(1.0), (delta / maximum).clamp(0.0, 1.0))
}

fn intersect_rect(left: RectI, right: RectI) -> Option<RectI> {
    let intersection = RectI {
        minimum: PhysicalDesktopPoint {
            x: left.minimum.x.max(right.minimum.x),
            y: left.minimum.y.max(right.minimum.y),
        },
        maximum: PhysicalDesktopPoint {
            x: left.maximum.x.min(right.maximum.x),
            y: left.maximum.y.min(right.maximum.y),
        },
    };
    intersection.is_valid().then_some(intersection)
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
