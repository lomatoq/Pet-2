//! Ecology clip coordinates and native input share one virtual-desktop canvas.
//! A moved/resized overlay cannot be accepted as a new world coordinate system.
use desktop_host::RectI;
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::window::Window;

/// Restore the authored side in physical working-area coordinates. Keeping a
/// valid but old normalized x left the den floating away from its saved edge.
pub(crate) fn docked_den_anchor(
    old: glam::Vec2,
    working_area: RectI,
    edge: pet_ecology::DenEdge,
    den_width: f32,
) -> glam::Vec2 {
    let inset_x = (den_width * 0.5 + 15.0)
        .max(170.0)
        .min(working_area.width() as f32 * 0.5);
    let left = working_area.minimum.x as f32 + inset_x;
    let right = working_area.maximum.x as f32 - inset_x;
    let x = match edge {
        pet_ecology::DenEdge::Left => left,
        pet_ecology::DenEdge::Right => right,
        _ => old.x.clamp(left, right),
    };
    glam::Vec2::new(x, working_area.maximum.y as f32 - den_width * 0.292)
}

pub(crate) fn window_matches(window: &Window, desktop: RectI) -> bool {
    window.is_minimized() != Some(true)
        && window
            .outer_position()
            .is_ok_and(|origin| matches(desktop, origin, window.inner_size()))
}

pub(crate) fn request_restore(window: &Window, desktop: RectI) {
    if !desktop.is_valid() || window.is_minimized() == Some(true) {
        return;
    }
    let origin = PhysicalPosition::new(desktop.minimum.x, desktop.minimum.y);
    let size = PhysicalSize::new(desktop.width() as u32, desktop.height() as u32);
    if window.outer_position().is_ok_and(|actual| actual != origin) {
        window.set_outer_position(origin);
    }
    if window.inner_size() != size {
        let _ = window.request_inner_size(size);
    }
}

pub(crate) fn matches(
    desktop: RectI,
    origin: PhysicalPosition<i32>,
    size: PhysicalSize<u32>,
) -> bool {
    desktop.is_valid()
        && origin.x == desktop.minimum.x
        && origin.y == desktop.minimum.y
        && size.width == desktop.width() as u32
        && size.height == desktop.height() as u32
}

#[cfg(test)]
mod tests {
    use super::*;
    use desktop_host::PhysicalDesktopPoint;

    fn desktop() -> RectI {
        RectI {
            minimum: PhysicalDesktopPoint { x: -1920, y: -900 },
            maximum: PhysicalDesktopPoint { x: 3440, y: 1440 },
        }
    }

    #[test]
    fn den_returns_to_its_saved_side_and_keeps_the_floor_contact() {
        let area = desktop();
        let old = glam::Vec2::new(2869.0, 1200.0);
        let right = docked_den_anchor(old, area, pet_ecology::DenEdge::Right, 310.0);
        let left = docked_den_anchor(old, area, pet_ecology::DenEdge::Left, 310.0);
        assert_eq!(right.x, 3270.0);
        assert_eq!(left.x, -1750.0);
        assert!((right.y - (1440.0 - 310.0 * 0.292)).abs() < 0.001);
        assert_eq!(left.y, right.y);
        let scaled = docked_den_anchor(old, area, pet_ecology::DenEdge::Right, 558.0);
        assert_eq!(scaled.x, 3146.0);
        assert!(scaled.x + 558.0 * 0.5 < area.maximum.x as f32);
        let bottom = docked_den_anchor(old, area, pet_ecology::DenEdge::Bottom, 310.0);
        assert_eq!(bottom.x, old.x);
    }

    #[test]
    fn restored_canvas_keeps_the_den_pixels_and_hit_target_in_the_same_place() {
        let d = desktop();
        let origin = PhysicalPosition::new(d.minimum.x, d.minimum.y);
        let size = PhysicalSize::new(d.width() as u32, d.height() as u32);
        assert!(matches(d, origin, size));
        // The old path compensated the creature offset after a move, but
        // continued drawing the den relative to the displaced native window.
        assert!(!matches(d, PhysicalPosition::new(0, 0), size));
        let den = glam::Vec2::new(0.8339844, 0.8196354);
        let physical = glam::Vec2::new(origin.x as f32, origin.y as f32)
            + den * glam::Vec2::new(size.width as f32, size.height as f32);
        assert!((physical.x - (d.minimum.x as f32 + den.x * d.width() as f32)).abs() < 0.001);
        assert!((physical.y - (d.minimum.y as f32 + den.y * d.height() as f32)).abs() < 0.001);
    }

    #[test]
    fn changed_window_aspect_cannot_squash_a_world_space_orb() {
        let d = desktop();
        let origin = PhysicalPosition::new(d.minimum.x, d.minimum.y);
        let aspect = d.width() as f32 / d.height() as f32;
        let radius_y = 0.06;
        let radius_x = radius_y / aspect;
        for size in [
            PhysicalSize::new(1920, 1080),
            PhysicalSize::new(5360, 1080),
            PhysicalSize::new(0, 0),
        ] {
            assert!(!matches(d, origin, size));
        }
        let size = PhysicalSize::new(d.width() as u32, d.height() as u32);
        assert!(matches(d, origin, size));
        assert!((radius_x * size.width as f32 - radius_y * size.height as f32).abs() < 0.001);
    }

    #[test]
    fn empty_and_minimized_canvases_are_never_world_geometry() {
        assert!(!matches(
            RectI::default(),
            PhysicalPosition::new(0, 0),
            PhysicalSize::new(1, 1)
        ));
        assert!(!matches(
            desktop(),
            PhysicalPosition::new(-1920, -900),
            PhysicalSize::new(0, 0)
        ));
    }
}
