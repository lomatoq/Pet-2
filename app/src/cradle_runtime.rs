//! Stateful admission to a one-way open cradle. Passing underneath is not sitting.
use glam::Vec2;

#[derive(Clone, Copy, Debug)]
pub struct CradleGeometry {
    pub anchor: Vec2,
    pub half_width: f32,
    pub floor: f32,
}
impl CradleGeometry {
    pub fn new(anchor: Vec2, viewport: [u32; 2], scale: f32) -> Self {
        let width = (310.0 * scale)
            .min(viewport[0] as f32 * 0.8)
            .min(viewport[1] as f32 * 0.65);
        Self {
            anchor,
            half_width: width * 0.285,
            floor: anchor.y + pet_body::birth_scene::den_seat_depth_pixels(viewport, scale),
        }
    }
    pub fn contains_target(self, target: Vec2) -> bool {
        (target.x - self.anchor.x).abs() < self.half_width
            && target.y > self.anchor.y - 85.0
            && target.y < self.floor + 30.0
    }
}

#[derive(Default)]
pub struct CradleSeat {
    pub inside: bool,
    descending: bool,
}
impl CradleSeat {
    /// Broad phase for the one-way physical cushion, independent of a sit
    /// intention or the occlusion/admission latch. Passing underneath remains
    /// free; the liquid solver decides the exact first surface contact.
    pub fn surface_available(
        &self,
        g: CradleGeometry,
        center: Vec2,
        hull_min: Vec2,
        hull_max: Vec2,
    ) -> bool {
        self.inside
            || (self.fits_entrance(g, center, hull_min, hull_max)
                && center.y < g.floor - 12.0
                && center.y + hull_max.y <= g.floor + 3.0)
    }

    pub fn scene_target(&self, g: CradleGeometry, target: Vec2, sleeping_in_den: bool) -> Vec2 {
        if self.inside && sleeping_in_den {
            g.anchor
        } else {
            target
        }
    }
    #[allow(clippy::too_many_arguments)]
    pub fn update(
        &mut self,
        g: CradleGeometry,
        center: &mut Vec2,
        velocity: &mut Vec2,
        hull_min: Vec2,
        hull_max: Vec2,
        dragged: bool,
        wants_home: bool,
    ) {
        let fits_entrance = self.fits_entrance(g, *center, hull_min, hull_max);
        let bottom = center.y + hull_max.y;
        if self.inside {
            // Leave over the lip, never through the closed floor/side walls.
            if bottom < g.anchor.y - 24.0 {
                self.inside = false;
                return;
            }
        } else if !dragged
            && wants_home
            && fits_entrance
            && bottom >= g.floor - 28.0
            && bottom <= g.floor + 3.0
            && center.y < g.floor - 12.0
            && velocity.y >= -35.0
            && velocity.length() < 190.0
        {
            self.inside = true;
        }
        if self.inside {
            let min_x = g.anchor.x - g.half_width - hull_min.x + 2.0;
            let max_x = g.anchor.x + g.half_width - hull_max.x - 2.0;
            {
                // A stretched hull still has a constrained material center.
                // The liquid solver compresses its flanks against the walls.
                let x = if min_x <= max_x {
                    center.x.clamp(min_x, max_x)
                } else {
                    (min_x + max_x) * 0.5
                };
                if (x - center.x).abs() > 0.001 {
                    velocity.x = 0.0;
                    center.x = x;
                }
            }
            let max_y = g.floor - hull_max.y;
            if center.y > max_y {
                center.y = max_y;
                velocity.y = velocity.y.min(0.0);
            }
        }
    }
    fn fits_entrance(&self, g: CradleGeometry, center: Vec2, min: Vec2, max: Vec2) -> bool {
        // Admit a centered liquid mass through the open lip; the solver then
        // compresses its flanks. Requiring the undeformed hull to fit can deadlock.
        let material_center = center.x + (min.x + max.x) * 0.5;
        let half = (max.x - min.x) * 0.5;
        half < g.half_width * 1.35
            && (material_center - g.anchor.x).abs() <= (g.half_width - half).max(8.0)
    }
    pub fn navigation(
        &mut self,
        g: CradleGeometry,
        center: Vec2,
        hull_min: Vec2,
        hull_max: Vec2,
        target: Vec2,
    ) -> Option<Vec2> {
        let wants_home = g.contains_target(target);
        if !wants_home || self.inside {
            self.descending = false;
        }
        if self.inside && !wants_home {
            return Some(Vec2::new(g.anchor.x, g.anchor.y - 32.0 - hull_max.y));
        }
        if !self.inside && wants_home {
            let fits = self.fits_entrance(g, center, hull_min, hull_max);
            // Keep the descent committed across small changes of liquid hull.
            // Otherwise the entrance check alternates between climb and descend.
            if fits && center.y + hull_max.y <= g.floor + 3.0 {
                self.descending = true;
            }
            if (center.x + (hull_min.x + hull_max.x) * 0.5 - g.anchor.x).abs() > g.half_width * 0.65
                || center.y > g.floor
            {
                self.descending = false;
            }
            if !self.descending {
                return Some(Vec2::new(g.anchor.x, g.anchor.y - 32.0 - hull_max.y));
            }
            return Some(Vec2::new(g.anchor.x, g.floor - hull_max.y));
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn geometry() -> CradleGeometry {
        CradleGeometry::new(Vec2::new(300.0, 300.0), [1920, 1080], 1.0)
    }
    #[test]
    fn cushion_collision_precedes_admission_and_rejects_underneath() {
        let g = geometry();
        let seat = CradleSeat::default();
        let min = Vec2::splat(-60.0);
        let max = Vec2::splat(60.0);
        assert!(!seat.inside);
        // 50px above the wall: too early for the 28px admission latch, but
        // physics must already know the actual geometric plane on approach.
        assert!(seat.surface_available(g, Vec2::new(g.anchor.x, g.floor - 110.0), min, max));
        assert!(!seat.surface_available(g, Vec2::new(g.anchor.x, g.floor + 10.0), min, max));
        assert!(!seat.surface_available(
            g,
            Vec2::new(g.anchor.x + g.half_width * 2.0, g.floor - 65.0),
            min,
            max
        ));
    }
    #[test]
    fn changing_liquid_hull_does_not_reverse_committed_descent() {
        let g = geometry();
        let mut seat = CradleSeat::default();
        let c = Vec2::new(g.anchor.x, g.floor - 72.0);
        seat.navigation(g, c, Vec2::splat(-65.0), Vec2::splat(65.0), g.anchor);
        let next = seat
            .navigation(
                g,
                c + Vec2::new(9.0, 0.0),
                Vec2::splat(-84.0),
                Vec2::splat(84.0),
                g.anchor,
            )
            .unwrap();
        assert!((next.y - (g.floor - 84.0)).abs() < 0.001);
    }
    #[test]
    fn sleeping_scene_does_not_turn_a_local_guard_fallback_into_an_exit() {
        let g = geometry();
        let mut seat = CradleSeat {
            inside: true,
            ..Default::default()
        };
        let center = Vec2::new(g.anchor.x, g.floor - 62.0);
        let fallback_floor_target = Vec2::new(g.anchor.x - 140.0, 1000.0);
        let target = seat.scene_target(g, fallback_floor_target, true);
        assert!(
            seat.navigation(g, center, Vec2::splat(-60.0), Vec2::splat(60.0), target)
                .is_none()
        );
        // Actual wake/exit ownership still traverses the open lip.
        let target = seat.scene_target(g, fallback_floor_target, false);
        assert!(
            seat.navigation(g, center, Vec2::splat(-60.0), Vec2::splat(60.0), target)
                .unwrap()
                .y
                < center.y
        );
    }
    #[test]
    fn outside_and_underneath_never_become_occluded_or_clamped() {
        let g = geometry();
        let mut seat = CradleSeat::default();
        for point in [Vec2::new(195.0, 320.0), Vec2::new(300.0, 410.0)] {
            let mut center = point;
            let mut velocity = Vec2::new(40.0, 0.0);
            seat.update(
                g,
                &mut center,
                &mut velocity,
                Vec2::splat(-65.0),
                Vec2::splat(65.0),
                false,
                true,
            );
            assert!(!seat.inside);
            assert_eq!(center, point);
        }
    }
    #[test]
    fn a_centered_soft_body_can_enter_before_its_flanks_compress() {
        let g = geometry();
        let mut seat = CradleSeat::default();
        let mut center = Vec2::new(g.anchor.x, g.floor - 100.0);
        let mut velocity = Vec2::new(0.0, 15.0);
        let min = Vec2::splat(-99.0);
        let max = Vec2::splat(99.0);
        let target = seat.navigation(g, center, min, max, g.anchor).unwrap();
        assert!(
            target.y > g.anchor.y - 100.0,
            "must descend rather than hover above the lip"
        );
        seat.update(g, &mut center, &mut velocity, min, max, false, true);
        assert!(seat.inside);
    }
    #[test]
    fn admitted_body_cannot_leave_through_floor_or_sides_but_can_take_off() {
        let g = geometry();
        let mut seat = CradleSeat::default();
        let mut center = Vec2::new(300.0, g.floor - 62.0);
        let mut velocity = Vec2::new(0.0, 8.0);
        let min = Vec2::splat(-60.0);
        let max = Vec2::splat(60.0);
        seat.update(g, &mut center, &mut velocity, min, max, false, true);
        assert!(seat.inside);
        for _ in 0..300 {
            center += Vec2::new(1.0, 2.0);
            velocity = Vec2::new(100.0, 100.0);
            seat.update(g, &mut center, &mut velocity, min, max, false, true);
            assert!(center.y + max.y <= g.floor + 0.001);
            assert!(center.x + max.x <= g.anchor.x + g.half_width);
        }
        let exit = seat
            .navigation(g, center, min, max, Vec2::new(50.0, 50.0))
            .unwrap();
        center = exit;
        seat.update(g, &mut center, &mut velocity, min, max, false, false);
        assert!(!seat.inside);
    }
    #[test]
    fn dragging_keeps_occlusion_and_walls_until_entire_body_clears_lip() {
        let g = geometry();
        let mut seat = CradleSeat {
            inside: true,
            ..Default::default()
        };
        for half in [60.0, 105.0] {
            for dx in [-180.0, 180.0] {
                let mut center = Vec2::new(g.anchor.x + dx, g.floor);
                let mut velocity = Vec2::new(dx, 250.0);
                seat.update(
                    g,
                    &mut center,
                    &mut velocity,
                    Vec2::splat(-half),
                    Vec2::splat(half),
                    true,
                    false,
                );
                assert!(seat.inside, "grabbing must not change the render layer");
                assert!(center.y + half <= g.floor);
                assert!((center.x - g.anchor.x).abs() <= (g.half_width - half).max(0.0));
                assert_eq!(velocity, Vec2::ZERO);
            }
        }
        let mut center = Vec2::new(g.anchor.x, g.anchor.y - 24.0 - 60.0 - 1.0);
        let mut velocity = Vec2::new(0.0, -100.0);
        seat.update(
            g,
            &mut center,
            &mut velocity,
            Vec2::splat(-60.0),
            Vec2::splat(60.0),
            true,
            false,
        );
        assert!(!seat.inside);
        center = Vec2::new(g.anchor.x, g.floor - 60.0);
        velocity = Vec2::ZERO;
        seat.update(
            g,
            &mut center,
            &mut velocity,
            Vec2::splat(-60.0),
            Vec2::splat(60.0),
            true,
            true,
        );
        assert!(
            !seat.inside,
            "an outside grab must not be latched by a home target"
        );
    }
}
