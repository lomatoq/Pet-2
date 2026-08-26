use glam::Vec2;

use crate::{MAX_OBJECT_SPEED, ObjectLifecycle, WorldObject};

pub const MAX_EXTERNAL_CONTACTS: usize = 8;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ContactSource {
    Orb,
    Window,
    #[default]
    DesktopBoundary,
    Den,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ExternalContact {
    pub source: ContactSource,
    pub point_world: Vec2,
    pub normal_world: Vec2,
    pub penetration_px: f32,
    pub relative_velocity_px: Vec2,
    pub intensity: f32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct EmbodiedEnvironmentFrame {
    pub contacts: [ExternalContact; MAX_EXTERNAL_CONTACTS],
    pub contact_count: usize,
    pub orb_position: Option<Vec2>,
    pub den_anchor: Option<Vec2>,
    pub pressure: f32,
    pub escape_direction: Vec2,
}

impl EmbodiedEnvironmentFrame {
    pub fn push_contact(&mut self, contact: ExternalContact) {
        if self.contact_count < MAX_EXTERNAL_CONTACTS {
            self.contacts[self.contact_count] = contact;
            self.contact_count += 1;
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ObjectPhysicsConfig {
    pub desktop_aspect: f32,
    pub reference_height_px: f32,
}

impl Default for ObjectPhysicsConfig {
    fn default() -> Self {
        Self {
            desktop_aspect: 16.0 / 9.0,
            reference_height_px: 1_152.0,
        }
    }
}

/// Fixed-step portable object solver. Coordinates are normalized by virtual
/// desktop height so ultrawide width does not multiply authored speed.
pub fn step_object(object: &mut WorldObject, config: ObjectPhysicsConfig, dt: f32) {
    if !matches!(
        object.lifecycle,
        ObjectLifecycle::Free | ObjectLifecycle::Sleeping
    ) {
        return;
    }
    let dt = if dt.is_finite() {
        dt.clamp(0.0, 1.0 / 30.0)
    } else {
        0.0
    };
    if dt <= 0.0 {
        return;
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
    let radius_y = (object.radius_px_at_reference / reference_height).clamp(0.001, 0.2);
    let radius_x = radius_y / aspect;
    let minimum = Vec2::new(radius_x, radius_y);
    let maximum = Vec2::ONE - minimum;
    let damping = (-object.linear_drag.max(0.0) * dt).exp();
    object.velocity *= damping;
    if object.velocity.length_squared() > MAX_OBJECT_SPEED * MAX_OBJECT_SPEED {
        object.velocity = object.velocity.normalize_or_zero() * MAX_OBJECT_SPEED;
    }

    // Analytic time-of-impact iterations prevent tunneling even after a maximum
    // legal drag release. Axis ties resolve X then Y deterministically.
    let mut remaining = dt;
    for _ in 0..4 {
        if remaining <= f32::EPSILON {
            break;
        }
        let mut hit_time = remaining + 1.0;
        let mut hit_axis = None;
        for axis in 0..2 {
            let velocity = object.velocity[axis];
            let boundary = if velocity < 0.0 {
                minimum[axis]
            } else {
                maximum[axis]
            };
            if velocity.abs() > 1.0e-7 {
                let time = (boundary - object.position[axis]) / velocity;
                if time >= 0.0 && time <= remaining && time < hit_time {
                    hit_time = time;
                    hit_axis = Some(axis);
                }
            }
        }
        if let Some(axis) = hit_axis {
            object.position += object.velocity * hit_time;
            object.position = object.position.clamp(minimum, maximum);
            object.velocity[axis] = -object.velocity[axis] * object.restitution;
            remaining -= hit_time;
            if hit_time <= 1.0e-7 {
                remaining = (remaining - 1.0e-5).max(0.0);
            }
        } else {
            object.position += object.velocity * remaining;
            remaining = 0.0;
        }
    }
    object.position = object.position.clamp(minimum, maximum);
    if object.velocity.length_squared() < 1.0e-8 {
        object.velocity = Vec2::ZERO;
        object.lifecycle = ObjectLifecycle::Sleeping;
    } else if object.lifecycle == ObjectLifecycle::Sleeping {
        object.lifecycle = ObjectLifecycle::Free;
    }
}

#[cfg(test)]
mod tests {
    use crate::{DenState, WorldObject};

    use super::*;

    #[test]
    fn maximum_speed_throw_does_not_tunnel_through_desktop() {
        let den = DenState::for_seed(7);
        let mut orb = WorldObject::canonical_orb(7, den.anchor);
        orb.position = Vec2::new(0.5, 0.5);
        orb.velocity = Vec2::new(MAX_OBJECT_SPEED, MAX_OBJECT_SPEED);
        for _ in 0..600 {
            step_object(&mut orb, ObjectPhysicsConfig::default(), 1.0 / 120.0);
            assert!(orb.position.cmpge(Vec2::ZERO).all());
            assert!(orb.position.cmple(Vec2::ONE).all());
            assert!(orb.position.is_finite());
            assert!(orb.velocity.is_finite());
        }
    }
}
