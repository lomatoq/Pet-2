use glam::Vec2;
use serde::{Deserialize, Serialize};

use crate::{EcologyError, ObjectId};

pub const DEN_SLOT_COUNT: usize = 3;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DenEdge {
    Left,
    Right,
    Top,
    Bottom,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct DenState {
    pub edge: DenEdge,
    pub anchor: Vec2,
    pub size_scale: f32,
    pub slots: [Option<ObjectId>; DEN_SLOT_COUNT],
    pub visits: u32,
    pub comfort_value: f32,
    pub familiarity: f32,
    pub last_relocated_seconds: f64,
}

impl DenState {
    #[must_use]
    pub fn for_seed(identity_seed: u64) -> Self {
        let right = super::object::splitmix64(identity_seed ^ 0x4445_4E5F_4544_4745) & 1 != 0;
        let vertical =
            0.72 + ((super::object::splitmix64(identity_seed) >> 48) as f32 / 65_535.0) * 0.16;
        Self {
            edge: if right { DenEdge::Right } else { DenEdge::Left },
            anchor: Vec2::new(if right { 0.965 } else { 0.035 }, vertical),
            size_scale: 1.0,
            slots: [None; DEN_SLOT_COUNT],
            visits: 0,
            comfort_value: 0.55,
            familiarity: 0.25,
            last_relocated_seconds: 0.0,
        }
    }

    pub fn validate(&self) -> Result<(), EcologyError> {
        let finite = self.anchor.is_finite()
            && self.size_scale.is_finite()
            && self.comfort_value.is_finite()
            && self.familiarity.is_finite()
            && self.last_relocated_seconds.is_finite();
        let bounded = self.anchor.cmpge(Vec2::ZERO).all()
            && self.anchor.cmple(Vec2::ONE).all()
            && (0.55..=1.8).contains(&self.size_scale)
            && (0.0..=1.0).contains(&self.comfort_value)
            && (0.0..=1.0).contains(&self.familiarity)
            && self.last_relocated_seconds >= 0.0;
        if finite && bounded {
            Ok(())
        } else {
            Err(EcologyError::InvalidDen)
        }
    }

    pub fn remap_invalid_anchor(&mut self) {
        if !self.anchor.is_finite() {
            self.anchor = match self.edge {
                DenEdge::Left => Vec2::new(0.035, 0.80),
                DenEdge::Right => Vec2::new(0.965, 0.80),
                DenEdge::Top => Vec2::new(0.82, 0.035),
                DenEdge::Bottom => Vec2::new(0.82, 0.965),
            };
        }
        self.anchor = self.anchor.clamp(Vec2::splat(0.025), Vec2::splat(0.975));
        match self.edge {
            DenEdge::Left => self.anchor.x = 0.035,
            DenEdge::Right => self.anchor.x = 0.965,
            DenEdge::Top => self.anchor.y = 0.035,
            DenEdge::Bottom => self.anchor.y = 0.965,
        }
    }
}
