use glam::Vec2;

pub const MAX_WINDOW_AFFORDANCES: usize = 16;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WindowId(pub u64);

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct NormalizedRect {
    pub minimum: Vec2,
    pub maximum: Vec2,
}

impl NormalizedRect {
    #[must_use]
    pub fn is_valid(self) -> bool {
        self.minimum.is_finite()
            && self.maximum.is_finite()
            && self.minimum.cmpge(Vec2::ZERO).all()
            && self.maximum.cmple(Vec2::ONE).all()
            && self.maximum.cmpge(self.minimum).all()
    }

    #[must_use]
    pub fn center(self) -> Vec2 {
        (self.minimum + self.maximum) * 0.5
    }
}

/// One privacy-safe window representation shared by perception, object physics
/// and navigation. It deliberately contains no title, process name or handle.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WindowAffordance {
    pub id: WindowId,
    pub bounds: NormalizedRect,
    pub velocity: Vec2,
    pub nearest_edge_point: Vec2,
    pub nearest_edge_normal: Vec2,
    pub overlap_pressure: f32,
    pub popup_pressure: f32,
    pub motion_energy: f32,
    pub is_visible: bool,
}

impl WindowAffordance {
    #[must_use]
    pub fn is_valid(self) -> bool {
        self.bounds.is_valid()
            && self.velocity.is_finite()
            && self.nearest_edge_point.is_finite()
            && self.nearest_edge_normal.is_finite()
            && [
                self.overlap_pressure,
                self.popup_pressure,
                self.motion_energy,
            ]
            .into_iter()
            .all(|value| value.is_finite() && (0.0..=1.0).contains(&value))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct WindowAffordanceFrame {
    pub windows: [WindowAffordance; MAX_WINDOW_AFFORDANCES],
    pub count: usize,
    pub pressure: f32,
    pub escape_direction: Vec2,
    pub motion_energy: f32,
}

impl Default for WindowAffordanceFrame {
    fn default() -> Self {
        Self {
            windows: [WindowAffordance::default(); MAX_WINDOW_AFFORDANCES],
            count: 0,
            pressure: 0.0,
            escape_direction: Vec2::ZERO,
            motion_energy: 0.0,
        }
    }
}

impl WindowAffordanceFrame {
    pub fn push(&mut self, window: WindowAffordance) {
        if window.is_valid() && self.count < MAX_WINDOW_AFFORDANCES {
            self.windows[self.count] = window;
            self.count += 1;
            self.pressure = self.pressure.max(window.overlap_pressure);
            self.motion_energy = self.motion_energy.max(window.motion_energy);
            self.escape_direction += window.nearest_edge_normal * window.overlap_pressure;
        }
    }

    pub fn finish(&mut self) {
        self.windows[..self.count].sort_by_key(|window| window.id.0);
        self.pressure = self.pressure.clamp(0.0, 1.0);
        self.motion_energy = self.motion_energy.clamp(0.0, 1.0);
        self.escape_direction = self.escape_direction.normalize_or_zero();
    }

    #[must_use]
    pub fn as_slice(&self) -> &[WindowAffordance] {
        &self.windows[..self.count]
    }
}
