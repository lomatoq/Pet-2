//! Quiet pearl surfaces with transparent space between floating controls.
use egui::Rect;
use std::path::Path;
use winit::{dpi::PhysicalPosition, window::Window};

pub fn reduced_motion() -> bool {
    if std::env::var_os("PET2_REDUCED_MOTION").is_some_and(|v| v == "1") {
        return true;
    }
    #[cfg(windows)]
    {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            SPI_GETCLIENTAREAANIMATION, SystemParametersInfoW,
        };
        let mut enabled: i32 = 1;
        // Read the OS preference only; never modify the user's system settings.
        if unsafe {
            SystemParametersInfoW(
                SPI_GETCLIENTAREAANIMATION,
                0,
                (&mut enabled as *mut i32).cast(),
                0,
            )
        } != 0
        {
            return enabled == 0;
        }
    }
    false
}

pub struct Glass {
    region: Vec<[i32; 5]>,
}
impl Glass {
    pub fn new(_: &Window) -> Option<Self> {
        Some(Self { region: vec![] })
    }
    pub fn update(&mut self, _: &egui::Context, _: &Window) {}
    pub fn set_regions(&mut self, window: &Window, rects: &[(Rect, f32)]) {
        let scale = window.scale_factor() as f32;
        let regions: Vec<_> = rects
            .iter()
            .map(|(r, round)| {
                let r = r.expand(10.0);
                [
                    (r.min.x * scale).floor() as i32,
                    (r.min.y * scale).floor() as i32,
                    (r.max.x * scale).ceil() as i32,
                    (r.max.y * scale).ceil() as i32,
                    ((round + 10.0) * 2.0 * scale) as i32,
                ]
            })
            .collect();
        if regions != self.region {
            apply_region(window, &regions);
            self.region = regions;
        }
    }
}
pub fn position(window: &Window, placement_file: &Path) -> Option<f32> {
    let bytes = std::fs::read(placement_file).ok()?;
    let v = serde_json::from_slice::<serde_json::Value>(&bytes).ok()?;
    let read = |key: &str| v[key].as_f64().map(|x| x as f32).filter(|x| x.is_finite());
    let anchor = [read("x")?, read("y")?];
    let bounds = [read("left")?, read("top")?, read("right")?, read("bottom")?];
    let scale = window.scale_factor() as f32;
    let placement = placement_to_monitor_edge(anchor, bounds, scale);
    let height = (580.0 * scale).round().max(1.0) as u32;
    let requested = winit::dpi::PhysicalSize::new(placement.width, height);
    if window.inner_size() != requested {
        let _ = window.request_inner_size(requested);
    }
    window.set_outer_position(PhysicalPosition::new(placement.position[0], placement.position[1]));
    Some(placement.nest_x)
}
struct MenuPlacement {
    position: [i32; 2],
    width: u32,
    nest_x: f32,
}
fn placement_to_monitor_edge(anchor: [f32; 2], bounds: [f32; 4], scale: f32) -> MenuPlacement {
    // Only widen the 580-point care strip. The six bubbles stay above the nest;
    // their arrival starts beyond the actual monitor edge, even for an interior nest.
    let base_width = (420.0 * scale).min((bounds[2] - bounds[0] - 16.0).max(1.0));
    let left = (anchor[0] - base_width * 0.5).clamp(bounds[0] + 8.0, (bounds[2] - base_width - 8.0).max(bounds[0] + 8.0)).floor();
    let top = (anchor[1] - 645.0 * scale).clamp(bounds[1] + 8.0, (bounds[3] - 580.0 * scale - 8.0).max(bounds[1] + 8.0)).floor();
    let width = (bounds[2] - left).round().max(1.0) as u32;
    let logical_width = width as f32 / scale;
    let half = (logical_width * 0.5).min(210.0);
    let nest_x = ((anchor[0] - left) / scale).clamp(half, (logical_width - half).max(half));
    MenuPlacement { position: [left as i32, top as i32], width, nest_x }
}
#[cfg(windows)]
fn apply_region(window: &Window, rects: &[[i32; 5]]) {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use windows_sys::Win32::Graphics::Gdi::{
        CombineRgn, CreateRectRgn, CreateRoundRectRgn, DeleteObject, RGN_OR, SetWindowRgn,
    };
    let Ok(handle) = window.window_handle() else {
        return;
    };
    let RawWindowHandle::Win32(handle) = handle.as_raw() else {
        return;
    };
    unsafe {
        let union = CreateRectRgn(0, 0, 0, 0);
        if union.is_null() {
            return;
        }
        for r in rects {
            let part = CreateRoundRectRgn(r[0], r[1], r[2] + 1, r[3] + 1, r[4], r[4]);
            if !part.is_null() {
                CombineRgn(union, union, part, RGN_OR);
                DeleteObject(part);
            }
        }
        if SetWindowRgn(handle.hwnd.get() as _, union, 1) == 0 {
            DeleteObject(union);
        }
    }
}
#[cfg(not(windows))]
fn apply_region(_: &Window, _: &[[i32; 5]]) {}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn placement_handles_negative_monitor_origins() {
        let p = placement_to_monitor_edge([-30.0, 950.0], [-1920.0, 0.0, 0.0, 1040.0], 1.0);
        assert_eq!(p.position[0] + p.width as i32, 0);
        assert!(p.position[0] >= -1920 && p.position[1] >= 0 && p.position[1] + 580 < 950);
    }
    #[test]
    fn interior_nest_keeps_local_anchor_while_canvas_reaches_real_edge() {
        for scale in [1.0, 1.5, 2.0] {
            let anchor = [800.0 * scale, 900.0 * scale];
            let p = placement_to_monitor_edge(anchor, [0.0, 0.0, 1920.0 * scale, 1080.0 * scale], scale);
            assert_eq!(p.position[0] + p.width as i32, (1920.0 * scale) as i32);
            assert!((p.nest_x - 210.0).abs() < 1.0 / scale);
            assert!(p.width as f32 > 420.0 * scale);
        }
    }
}
