use glam::Vec2;

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
