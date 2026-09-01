use glam::Vec2;
use serde::{Deserialize, Serialize};

use crate::{
    EcologyError, MAX_OBJECT_SPEED, ObjectId, ObjectKind, ObjectLifecycle, ObjectPhysicsConfig,
    WorldObject,
};

pub const DEN_SLOT_COUNT: usize = 3;
pub const DEN_ATTRACTION_RADIUS_PX: f32 = 190.0;
pub const DEN_CAPTURE_RADIUS_PX: f32 = 7.0;
const DEN_ATTRACTION_FULL_STRENGTH_RADIUS_PX: f32 = 50.0;
const DEN_ATTRACTION_MAX_SPEED: f32 = 0.58;
const STORED_ORB_HOVER_X_PX: f32 = 6.0;
const STORED_ORB_HOVER_Y_PX: f32 = 4.0;

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

/// Applies the den's local, reversible attraction field to the canonical orb.
///
/// The caller remains responsible for assigning a den slot when this returns
/// `true`. User and pet authority always win because non-free lifecycles are
/// ignored. Motion is solved in desktop-height space so the authored radius and
/// speed remain physically consistent on ultrawide desktops.
pub fn step_den_attraction(
    object: &mut WorldObject,
    den_anchor: Vec2,
    config: ObjectPhysicsConfig,
    dt: f32,
) -> bool {
    if object.kind != ObjectKind::Orb
        || !matches!(
            object.lifecycle,
            ObjectLifecycle::Free | ObjectLifecycle::Sleeping
        )
        || !object.position.is_finite()
        || !object.velocity.is_finite()
        || !den_anchor.is_finite()
    {
        return false;
    }
    let dt = if dt.is_finite() {
        dt.clamp(0.0, 1.0 / 30.0)
    } else {
        0.0
    };
    if dt <= 0.0 {
        return false;
    }
    let aspect = if config.desktop_aspect.is_finite() {
        config.desktop_aspect.clamp(0.25, 8.0)
    } else {
        16.0 / 9.0
    };
    let reference_height = if config.reference_height_px.is_finite() {
        config.reference_height_px.max(64.0)
    } else {
        1_152.0
    };
    let position = Vec2::new(object.position.x * aspect, object.position.y);
    let anchor = Vec2::new(den_anchor.x * aspect, den_anchor.y);
    let toward_center = anchor - position;
    let distance = toward_center.length();
    let distance_px = distance * reference_height;
    if distance_px > DEN_ATTRACTION_RADIUS_PX {
        return false;
    }
    if distance_px <= DEN_CAPTURE_RADIUS_PX {
        object.position = den_anchor;
        object.velocity = Vec2::ZERO;
        return true;
    }

    let proximity = 1.0
        - smoothstep(
            DEN_ATTRACTION_FULL_STRENGTH_RADIUS_PX,
            DEN_ATTRACTION_RADIUS_PX,
            distance_px,
        );
    let inward = toward_center / distance.max(f32::EPSILON);
    let tangent_sign = if object.id & 1 == 0 { 1.0 } else { -1.0 };
    let tangent = Vec2::new(-inward.y, inward.x) * tangent_sign;
    let curve = (distance_px / DEN_ATTRACTION_RADIUS_PX).clamp(0.0, 1.0) * 0.12;
    let direction = (inward + tangent * curve).normalize_or_zero();
    let desired_speed = (distance * 4.2).min(DEN_ATTRACTION_MAX_SPEED);
    let desired_velocity = direction * desired_speed;
    let response = 4.0 + proximity * 9.0;
    let alpha = (1.0 - (-response * dt).exp()) * proximity;
    object.velocity = object
        .velocity
        .lerp(desired_velocity, alpha)
        .clamp_length_max(MAX_OBJECT_SPEED);
    // The orb has screen gravity, so a velocity servo alone settles slightly
    // below the center. A small bounded positional correction removes that
    // equilibrium without teleporting or taking authority from direct grabs.
    let position_response = 2.2 + proximity * 3.8;
    let position_alpha = (1.0 - (-position_response * dt).exp()) * proximity;
    let curved_pull = toward_center + tangent * distance * curve * 0.35;
    let radius = (object.radius_px_at_reference / reference_height).clamp(0.001, 0.2);
    let minimum = Vec2::splat(radius);
    let maximum = Vec2::new(aspect - radius, 1.0 - radius);
    let pulled_position = (position + curved_pull * position_alpha).clamp(minimum, maximum);
    object.position = Vec2::new(pulled_position.x / aspect, pulled_position.y);
    if (anchor - pulled_position).length() * reference_height <= DEN_CAPTURE_RADIUS_PX {
        object.position = den_anchor;
        object.velocity = Vec2::ZERO;
        return true;
    }
    if alpha > 0.0 {
        object.lifecycle = ObjectLifecycle::Free;
    }
    false
}

/// World-space offset used by both rendering and pointer extraction. Sharing
/// the exact curve prevents a stored orb from snapping when hover authority is
/// handed from the den presentation to the user's drag controller.
#[must_use]
pub fn stored_orb_hover_offset(
    object_id: ObjectId,
    time_seconds: f32,
    desktop_aspect: f32,
) -> Vec2 {
    let time_seconds = if time_seconds.is_finite() {
        time_seconds
    } else {
        0.0
    };
    let aspect = if desktop_aspect.is_finite() {
        desktop_aspect.clamp(0.25, 8.0)
    } else {
        16.0 / 9.0
    };
    let seed_phase = (object_id as u32) as f32 * 0.000_13 * std::f32::consts::TAU;
    let hover_x =
        (time_seconds * std::f32::consts::TAU / 3.83 + seed_phase).sin() * STORED_ORB_HOVER_X_PX;
    let hover_y = (time_seconds * std::f32::consts::TAU / 5.17 + seed_phase * 1.7).sin()
        * STORED_ORB_HOVER_Y_PX;
    Vec2::new(
        hover_x / (crate::REFERENCE_DESKTOP_HEIGHT_PX * aspect),
        -hover_y / crate::REFERENCE_DESKTOP_HEIGHT_PX,
    )
}

fn smoothstep(edge0: f32, edge1: f32, value: f32) -> f32 {
    let t = ((value - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::step_object;

    #[test]
    fn nearby_free_orb_is_smoothly_captured_at_the_exact_den_center() {
        let den = DenState::for_seed(42);
        let config = ObjectPhysicsConfig::default();
        let aspect = config.desktop_aspect;
        let mut orb = WorldObject::canonical_orb(42, den.anchor);
        orb.position = den.anchor + Vec2::new(130.0 / config.reference_height_px / aspect, -0.01);
        orb.velocity = Vec2::ZERO;
        orb.lifecycle = ObjectLifecycle::Free;

        let mut captured = false;
        for _ in 0..1_200 {
            step_object(&mut orb, config, 1.0 / 120.0);
            if step_den_attraction(&mut orb, den.anchor, config, 1.0 / 120.0) {
                captured = true;
                break;
            }
            assert!(orb.velocity.length() <= MAX_OBJECT_SPEED + 1.0e-5);
        }

        assert!(captured);
        assert_eq!(orb.position, den.anchor);
        assert_eq!(orb.velocity, Vec2::ZERO);
    }

    #[test]
    fn user_and_pet_authority_disable_den_attraction() {
        let den = DenState::for_seed(43);
        let config = ObjectPhysicsConfig::default();
        for lifecycle in [
            ObjectLifecycle::GrabbedByUser,
            ObjectLifecycle::CarriedByPet,
            ObjectLifecycle::StoredInDen,
        ] {
            let mut orb = WorldObject::canonical_orb(43, den.anchor);
            orb.position = den.anchor + Vec2::new(0.01, 0.0);
            orb.velocity = Vec2::new(0.3, -0.2);
            orb.lifecycle = lifecycle;
            let before = orb.clone();

            assert!(!step_den_attraction(
                &mut orb,
                den.anchor,
                config,
                1.0 / 120.0,
            ));
            assert_eq!(orb, before);
        }
    }
}
