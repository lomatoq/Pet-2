//! Opaque pearl surfaces with transparent space between floating controls.
use egui::{Color32, Rect, pos2};
use std::path::Path;
use winit::{dpi::PhysicalPosition, window::Window};

pub struct Glass {
    region: Vec<[i32; 5]>,
}
impl Glass {
    pub fn new(_: &Window) -> Option<Self> {
        Some(Self { region: vec![] })
    }
    pub fn update(&mut self, _: &egui::Context, _: &Window) {}
    pub fn texture(&self) -> Option<egui::TextureId> {
        None
    }
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
pub fn position(window: &Window, root: &Path) {
    let Ok(bytes) = std::fs::read(root.join("companion-menu-placement.json")) else {
        return;
    };
    let Ok(v) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
        return;
    };
    let read = |key: &str| v[key].as_f64().map(|x| x as f32).filter(|x| x.is_finite());
    let (Some(x), Some(y), Some(left), Some(top), Some(right), Some(bottom)) = (
        read("x"),
        read("y"),
        read("left"),
        read("top"),
        read("right"),
        read("bottom"),
    ) else {
        return;
    };
    let size = window.inner_size();
    let scale = window.scale_factor() as f32;
    let p = clamp_position(
        [x, y],
        [left, top, right, bottom],
        [size.width as f32, size.height as f32],
        scale,
    );
    window.set_outer_position(PhysicalPosition::new(p[0], p[1]));
}
fn clamp_position(anchor: [f32; 2], bounds: [f32; 4], size: [f32; 2], scale: f32) -> [i32; 2] {
    [
        (anchor[0] - size[0] * 0.5).clamp(
            bounds[0] + 8.0,
            (bounds[2] - size[0] - 8.0).max(bounds[0] + 8.0),
        ) as i32,
        (anchor[1] - size[1] - 65.0 * scale).clamp(
            bounds[1] + 8.0,
            (bounds[3] - size[1] - 8.0).max(bounds[1] + 8.0),
        ) as i32,
    ]
}
pub fn surface(
    painter: &egui::Painter,
    rect: Rect,
    radius: u8,
    texture: Option<egui::TextureId>,
    screen: Rect,
    alpha: f32,
    active: bool,
) {
    painter.add(
        egui::epaint::RectShape::filled(
            rect.translate(egui::vec2(0.0, 5.0)),
            radius,
            Color32::from_black_alpha((26.0 * alpha) as u8),
        )
        .with_blur_width(18.0),
    );
    if let Some(id) = texture {
        let uv = Rect::from_min_max(
            pos2(rect.min.x / screen.width(), rect.min.y / screen.height()),
            pos2(rect.max.x / screen.width(), rect.max.y / screen.height()),
        );
        painter.add(
            egui::epaint::RectShape::filled(
                rect,
                radius,
                Color32::from_white_alpha((230.0 * alpha) as u8),
            )
            .with_texture(id, uv),
        );
    }
    let tint = if active {
        [231, 221, 249]
    } else {
        [247, 244, 255]
    };
    painter.rect_filled(
        rect,
        radius,
        Color32::from_rgba_unmultiplied(tint[0], tint[1], tint[2], (255.0 * alpha) as u8),
    );
    painter.rect_stroke(
        rect,
        radius,
        egui::Stroke::new(1.0, Color32::from_white_alpha((185.0 * alpha) as u8)),
        egui::StrokeKind::Inside,
    );
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
        let p = clamp_position(
            [-30.0, 950.0],
            [-1920.0, 0.0, 0.0, 1040.0],
            [420.0, 580.0],
            1.0,
        );
        assert!(p[0] + 420 <= 0 && p[0] >= -1920 && p[1] >= 0 && p[1] + 580 < 950);
    }
}
