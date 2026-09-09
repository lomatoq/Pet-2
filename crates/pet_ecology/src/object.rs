use glam::Vec2;
use serde::{Deserialize, Serialize};

use crate::{EcologyError, MorselProfile};

pub type ObjectId = u64;

pub const MAX_OBJECTS: usize = 8;
pub const MAX_ACTIVE_MORSELS: usize = 3;
pub const MAX_OBJECT_SPEED: f32 = 2.5;
pub const REFERENCE_DESKTOP_HEIGHT_PX: f32 = 1_152.0;

/// The only geometry object physics may consume. Visual glow is deliberately
/// not represented here.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PhysicalInteractionHull {
    pub center: Vec2,
    pub radius_px_at_reference: f32,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ObjectKind {
    Orb,
    Morsel,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ObjectLifecycle {
    Free,
    Sleeping,
    GrabbedByUser,
    CarriedByPet,
    StoredInDen,
    Consumed,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct WorldObject {
    pub id: ObjectId,
    pub kind: ObjectKind,
    pub lifecycle: ObjectLifecycle,
    /// Normalized virtual-desktop position, portable across monitor layouts.
    pub position: Vec2,
    /// Virtual-desktop-height units per second.
    pub velocity: Vec2,
    pub radius_px_at_reference: f32,
    pub mass: f32,
    pub restitution: f32,
    pub linear_drag: f32,
    pub hue: f32,
    pub saturation: f32,
    pub value: f32,
    pub glow: f32,
    pub familiarity: f32,
    pub preference: f32,
    pub novelty: f32,
    pub wear: f32,
    pub home_slot: Option<u8>,
    pub last_interaction_seconds: f64,
    #[serde(default)]
    pub morsel_profile: Option<MorselProfile>,
}

impl WorldObject {
    #[must_use]
    pub const fn physical_hull(&self) -> PhysicalInteractionHull {
        PhysicalInteractionHull {
            center: self.position,
            radius_px_at_reference: self.radius_px_at_reference,
        }
    }

    #[must_use]
    pub fn canonical_orb(identity_seed: u64, den_anchor: Vec2) -> Self {
        let id = canonical_orb_id(identity_seed);
        let jitter = unit_pair(splitmix64(identity_seed ^ 0x4F52_425F_5045_5432));
        let position = (den_anchor + Vec2::new(0.075 + jitter.x * 0.04, -0.03 + jitter.y * 0.06))
            .clamp(Vec2::splat(0.04), Vec2::splat(0.96));
        Self {
            id,
            kind: ObjectKind::Orb,
            lifecycle: ObjectLifecycle::Free,
            position,
            velocity: Vec2::ZERO,
            radius_px_at_reference: 31.0,
            mass: 0.72,
            restitution: 0.76,
            linear_drag: 0.42,
            hue: ((splitmix64(identity_seed) >> 40) as f32 / (1_u32 << 24) as f32).fract(),
            saturation: 0.72,
            value: 0.94,
            glow: 0.68,
            familiarity: 0.18,
            preference: 0.55,
            novelty: 0.82,
            wear: 0.0,
            home_slot: Some(0),
            last_interaction_seconds: 0.0,
            morsel_profile: None,
        }
    }

    #[must_use]
    pub fn morsel(id: ObjectId, position: Vec2, profile: MorselProfile) -> Self {
        Self {
            id: id.max(1),
            kind: ObjectKind::Morsel,
            lifecycle: ObjectLifecycle::Free,
            position: position.clamp(Vec2::splat(0.025), Vec2::splat(0.975)),
            velocity: Vec2::ZERO,
            radius_px_at_reference: 17.0,
            mass: 0.22,
            restitution: 0.42,
            linear_drag: 1.1,
            hue: profile.hue,
            saturation: profile.saturation,
            value: profile.value,
            glow: (0.56 + profile.stimulation * 0.36).clamp(0.0, 1.0),
            familiarity: 0.0,
            preference: 0.0,
            novelty: profile.novelty,
            wear: 0.0,
            home_slot: None,
            last_interaction_seconds: 0.0,
            morsel_profile: Some(profile),
        }
    }

    pub fn validate(&self) -> Result<(), EcologyError> {
        let finite = self.position.is_finite()
            && self.velocity.is_finite()
            && self.radius_px_at_reference.is_finite()
            && self.mass.is_finite()
            && self.restitution.is_finite()
            && self.linear_drag.is_finite()
            && self.hue.is_finite()
            && self.saturation.is_finite()
            && self.value.is_finite()
            && self.glow.is_finite()
            && self.familiarity.is_finite()
            && self.preference.is_finite()
            && self.novelty.is_finite()
            && self.wear.is_finite()
            && self.last_interaction_seconds.is_finite();
        let bounded = self.id != 0
            && self.position.cmpge(Vec2::ZERO).all()
            && self.position.cmple(Vec2::ONE).all()
            && self.velocity.length_squared() <= MAX_OBJECT_SPEED * MAX_OBJECT_SPEED + f32::EPSILON
            && (4.0..=96.0).contains(&self.radius_px_at_reference)
            && (0.05..=8.0).contains(&self.mass)
            && (0.0..=1.0).contains(&self.restitution)
            && (0.0..=8.0).contains(&self.linear_drag)
            && (0.0..=1.0).contains(&self.hue)
            && (0.0..=1.0).contains(&self.saturation)
            && (0.0..=1.0).contains(&self.value)
            && (0.0..=1.0).contains(&self.glow)
            && (0.0..=1.0).contains(&self.familiarity)
            && (-1.0..=1.0).contains(&self.preference)
            && (0.0..=1.0).contains(&self.novelty)
            && (0.0..=1.0).contains(&self.wear)
            && self.home_slot.is_none_or(|slot| slot < 3)
            && self.last_interaction_seconds >= 0.0;
        let profile_valid = match self.kind {
            ObjectKind::Orb => self.morsel_profile.is_none(),
            ObjectKind::Morsel => self
                .morsel_profile
                .as_ref()
                .is_some_and(MorselProfile::is_valid),
        };
        if finite && bounded && profile_valid {
            Ok(())
        } else {
            Err(EcologyError::InvalidObject)
        }
    }

    #[must_use]
    pub fn is_active_morsel(&self) -> bool {
        self.kind == ObjectKind::Morsel && self.lifecycle != ObjectLifecycle::Consumed
    }
}

#[must_use]
pub fn canonical_orb_id(identity_seed: u64) -> ObjectId {
    splitmix64(identity_seed ^ 0xCA11_0B5E_0B15_EED5).max(1)
}

#[must_use]
pub(crate) const fn splitmix64(mut value: u64) -> u64 {
    value = value.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = value;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

fn unit_pair(seed: u64) -> Vec2 {
    let x = (seed >> 40) as f32 / (1_u32 << 24) as f32;
    let y = (splitmix64(seed) >> 40) as f32 / (1_u32 << 24) as f32;
    Vec2::new(x, y)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn orb_physical_hull_is_bit_identical_across_the_full_glow_sweep() {
        let mut orb = WorldObject::canonical_orb(0x0B_BA11, Vec2::splat(0.5));
        let expected = orb.physical_hull();
        assert_eq!(
            expected.radius_px_at_reference.to_bits(),
            31.0_f32.to_bits()
        );

        for step in 0..=100 {
            orb.glow = step as f32 / 100.0;
            let actual = orb.physical_hull();
            assert_eq!(actual.center.x.to_bits(), expected.center.x.to_bits());
            assert_eq!(actual.center.y.to_bits(), expected.center.y.to_bits());
            assert_eq!(
                actual.radius_px_at_reference.to_bits(),
                expected.radius_px_at_reference.to_bits()
            );
        }
    }
}
