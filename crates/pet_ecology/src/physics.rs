use glam::Vec2;

use crate::{MAX_OBJECT_SPEED, ObjectKind, ObjectLifecycle, WindowAffordanceFrame, WorldObject};

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
    pub orb_trapped: bool,
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
    pub pet_radius_px_at_reference: f32,
}

impl Default for ObjectPhysicsConfig {
    fn default() -> Self {
        Self {
            desktop_aspect: 16.0 / 9.0,
            reference_height_px: 1_152.0,
            pet_radius_px_at_reference: 58.0,
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
    let radius = (object.radius_px_at_reference / reference_height).clamp(0.001, 0.2);
    // Solve in virtual-desktop-height units. Persistence stays normalized
    // 0..1, but an X velocity of one now means the same physical distance as a
    // Y velocity of one even on an ultrawide desktop.
    let mut position = Vec2::new(object.position.x * aspect, object.position.y);
    let minimum = Vec2::splat(radius);
    let maximum = Vec2::new(aspect - radius, 1.0 - radius);
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
                let time = (boundary - position[axis]) / velocity;
                if time >= 0.0 && time <= remaining && time < hit_time {
                    hit_time = time;
                    hit_axis = Some(axis);
                }
            }
        }
        if let Some(axis) = hit_axis {
            position += object.velocity * hit_time;
            position = position.clamp(minimum, maximum);
            object.velocity[axis] = -object.velocity[axis] * object.restitution;
            remaining -= hit_time;
            if hit_time <= 1.0e-7 {
                remaining = (remaining - 1.0e-5).max(0.0);
            }
        } else {
            position += object.velocity * remaining;
            remaining = 0.0;
        }
    }
    position = position.clamp(minimum, maximum);
    object.position = Vec2::new(position.x / aspect, position.y);
    if object.velocity.length_squared() < 1.0e-8 {
        object.velocity = Vec2::ZERO;
        object.lifecycle = ObjectLifecycle::Sleeping;
    } else if object.lifecycle == ObjectLifecycle::Sleeping {
        object.lifecycle = ObjectLifecycle::Free;
    }
}

pub fn step_object_with_windows(
    object: &mut WorldObject,
    config: ObjectPhysicsConfig,
    windows: &WindowAffordanceFrame,
    dt: f32,
    environment: &mut EmbodiedEnvironmentFrame,
) {
    if !matches!(
        object.lifecycle,
        ObjectLifecycle::Free | ObjectLifecycle::Sleeping
    ) {
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
    let radius = (object.radius_px_at_reference / reference_height).clamp(0.001, 0.2);
    let dt = if dt.is_finite() {
        dt.clamp(0.0, 1.0 / 30.0)
    } else {
        0.0
    };
    let start = Vec2::new(object.position.x * aspect, object.position.y);
    step_object(object, config, dt);
    let mut end = Vec2::new(object.position.x * aspect, object.position.y);

    let mut window_contacts = 0_usize;
    for window in windows.as_slice() {
        if !window.is_visible || !window.is_valid() {
            continue;
        }
        let window_velocity = Vec2::new(window.velocity.x * aspect, window.velocity.y);
        let minimum = Vec2::new(window.bounds.minimum.x * aspect, window.bounds.minimum.y)
            - Vec2::splat(radius);
        let maximum = Vec2::new(window.bounds.maximum.x * aspect, window.bounds.maximum.y)
            + Vec2::splat(radius);
        let window_start_offset = window_velocity * dt;
        let relative_start = start + window_start_offset;
        let relative_delta = end - relative_start;
        let mut normal = Vec2::ZERO;
        let mut penetration = 0.0;
        if point_in_rect(relative_start, minimum, maximum) {
            let (resolved, overlap_normal, depth) =
                resolve_inside_rect(relative_start, minimum, maximum);
            end = resolved;
            normal = overlap_normal;
            penetration = depth;
        } else if let Some((time, hit_normal)) =
            swept_point_aabb(relative_start, relative_delta, minimum, maximum)
        {
            end = start.lerp(end, time) + hit_normal * 1.0e-5;
            normal = hit_normal;
        } else if point_in_rect(end, minimum, maximum) {
            let (resolved, overlap_normal, depth) = resolve_inside_rect(end, minimum, maximum);
            end = resolved;
            normal = overlap_normal;
            penetration = depth;
        }
        if normal == Vec2::ZERO {
            continue;
        }
        let relative_velocity = object.velocity - window_velocity;
        let incoming = relative_velocity.dot(normal);
        if incoming < 0.0 {
            let tangent_velocity = relative_velocity - normal * incoming;
            object.velocity = tangent_velocity * 0.92
                - normal * incoming * object.restitution.clamp(0.0, 1.0)
                + window_velocity;
        }
        if object.velocity.length_squared() > MAX_OBJECT_SPEED * MAX_OBJECT_SPEED {
            object.velocity = object.velocity.normalize_or_zero() * MAX_OBJECT_SPEED;
        }
        object.lifecycle = ObjectLifecycle::Free;
        let point = Vec2::new((end.x / aspect).clamp(0.0, 1.0), end.y.clamp(0.0, 1.0));
        environment.push_contact(ExternalContact {
            source: ContactSource::Window,
            point_world: point,
            normal_world: normal,
            penetration_px: penetration * reference_height,
            relative_velocity_px: relative_velocity * reference_height,
            intensity: (relative_velocity.length() * 0.45 + penetration * 12.0).clamp(0.0, 1.0),
        });
        window_contacts += 1;
        if window_contacts >= 4 {
            break;
        }
    }
    object.position = Vec2::new((end.x / aspect).clamp(0.0, 1.0), end.y.clamp(0.0, 1.0));
    if object.kind == ObjectKind::Orb {
        environment.orb_position = Some(object.position);
    }
}

pub fn resolve_object_body_contact(
    object: &mut WorldObject,
    body_position: Vec2,
    body_velocity: Vec2,
    config: ObjectPhysicsConfig,
    environment: &mut EmbodiedEnvironmentFrame,
) -> bool {
    if !matches!(
        object.lifecycle,
        ObjectLifecycle::Free | ObjectLifecycle::Sleeping
    ) || !body_position.is_finite()
        || !body_velocity.is_finite()
    {
        return false;
    }
    let aspect = config.desktop_aspect.clamp(0.25, 8.0);
    let reference_height = config.reference_height_px.max(64.0);
    let body_radius = (config.pet_radius_px_at_reference / reference_height).clamp(0.005, 0.2);
    let object_radius = (object.radius_px_at_reference / reference_height).clamp(0.001, 0.2);
    let body_height_space = Vec2::new(body_position.x * aspect, body_position.y);
    let mut object_height_space = Vec2::new(object.position.x * aspect, object.position.y);
    let delta = object_height_space - body_height_space;
    let distance = delta.length();
    let combined_radius = body_radius + object_radius;
    if distance >= combined_radius {
        return false;
    }
    let preferred_normal = if distance > 1.0e-6 {
        delta / distance
    } else {
        Vec2::X
    };
    let penetration = combined_radius - distance;
    let minimum = Vec2::splat(object_radius);
    let maximum = Vec2::new(aspect - object_radius, 1.0 - object_radius);
    let (resolved_position, normal) = feasible_body_contact_position(
        body_height_space,
        preferred_normal,
        combined_radius + 1.0e-5,
        minimum,
        maximum,
    );
    object_height_space = resolved_position;
    let body_velocity_height_space = Vec2::new(body_velocity.x * aspect, body_velocity.y);
    let relative_velocity = object.velocity - body_velocity_height_space;
    let incoming = relative_velocity.dot(normal);
    if incoming < 0.0 {
        let tangent = relative_velocity - normal * incoming;
        object.velocity = tangent * 0.90 - normal * incoming * object.restitution.clamp(0.0, 1.0)
            + body_velocity_height_space;
    }
    object.velocity = object.velocity.clamp_length_max(MAX_OBJECT_SPEED);
    object.position = Vec2::new(
        (object_height_space.x / aspect).clamp(0.0, 1.0),
        object_height_space.y.clamp(0.0, 1.0),
    );
    object.lifecycle = ObjectLifecycle::Free;
    let normal_world = Vec2::new(normal.x / aspect, normal.y).normalize_or_zero();
    environment.push_contact(ExternalContact {
        source: ContactSource::Orb,
        point_world: (body_position + normal_world * body_radius).clamp(Vec2::ZERO, Vec2::ONE),
        normal_world: -normal_world,
        penetration_px: penetration * reference_height,
        relative_velocity_px: relative_velocity * reference_height,
        intensity: (relative_velocity.length() * 0.45 + penetration * 12.0).clamp(0.0, 1.0),
    });
    true
}

/// Resolve a circle around the body without ever placing the object's center
/// outside its radius-aware desktop bounds. Near a desktop edge the preferred
/// contact normal may be impossible (for example, an orb caught above a pet at
/// the top edge). In that case choose the closest deterministic feasible escape
/// direction so the two contact solvers cannot pin the orb to the border.
fn feasible_body_contact_position(
    body_position: Vec2,
    preferred_normal: Vec2,
    separation: f32,
    minimum: Vec2,
    maximum: Vec2,
) -> (Vec2, Vec2) {
    let preferred_normal = preferred_normal.normalize_or_zero();
    let preferred_normal = if preferred_normal == Vec2::ZERO {
        Vec2::X
    } else {
        preferred_normal
    };
    let direct = body_position + preferred_normal * separation;
    if direct.cmpge(minimum).all() && direct.cmple(maximum).all() {
        return (direct, preferred_normal);
    }

    let diagonal = std::f32::consts::FRAC_1_SQRT_2;
    let directions = [
        Vec2::X,
        Vec2::Y,
        Vec2::NEG_X,
        Vec2::NEG_Y,
        Vec2::new(diagonal, diagonal),
        Vec2::new(-diagonal, diagonal),
        Vec2::new(diagonal, -diagonal),
        Vec2::new(-diagonal, -diagonal),
    ];
    let toward_center = ((minimum + maximum) * 0.5 - body_position).normalize_or_zero();
    let mut best = None;
    for direction in directions {
        let candidate = body_position + direction * separation;
        if !candidate.cmpge(minimum).all() || !candidate.cmple(maximum).all() {
            continue;
        }
        let score = direction.dot(preferred_normal) * 2.0 + direction.dot(toward_center) * 0.35;
        if best.is_none_or(|(_, _, best_score)| score > best_score) {
            best = Some((candidate, direction, score));
        }
    }
    if let Some((position, normal, _)) = best {
        (position, normal)
    } else {
        let position = direct.clamp(minimum, maximum);
        let normal = (position - body_position)
            .try_normalize()
            .unwrap_or(preferred_normal);
        (position, normal)
    }
}

fn point_in_rect(point: Vec2, minimum: Vec2, maximum: Vec2) -> bool {
    point.cmpge(minimum).all() && point.cmple(maximum).all()
}

fn resolve_inside_rect(point: Vec2, minimum: Vec2, maximum: Vec2) -> (Vec2, Vec2, f32) {
    let candidates = [
        (
            point.x - minimum.x,
            Vec2::NEG_X,
            Vec2::new(minimum.x, point.y),
        ),
        (maximum.x - point.x, Vec2::X, Vec2::new(maximum.x, point.y)),
        (
            point.y - minimum.y,
            Vec2::NEG_Y,
            Vec2::new(point.x, minimum.y),
        ),
        (maximum.y - point.y, Vec2::Y, Vec2::new(point.x, maximum.y)),
    ];
    let (depth, normal, resolved) = candidates
        .into_iter()
        .min_by(|left, right| left.0.total_cmp(&right.0))
        .unwrap_or((0.0, Vec2::Y, point));
    (resolved + normal * 1.0e-5, normal, depth.max(0.0))
}

fn swept_point_aabb(start: Vec2, delta: Vec2, minimum: Vec2, maximum: Vec2) -> Option<(f32, Vec2)> {
    let mut enter = 0.0_f32;
    let mut exit = 1.0_f32;
    let mut normal = Vec2::ZERO;
    for axis in 0..2 {
        if delta[axis].abs() <= 1.0e-8 {
            if start[axis] < minimum[axis] || start[axis] > maximum[axis] {
                return None;
            }
            continue;
        }
        let first = (minimum[axis] - start[axis]) / delta[axis];
        let second = (maximum[axis] - start[axis]) / delta[axis];
        let (near, far, near_normal) = if first <= second {
            (
                first,
                second,
                if axis == 0 { Vec2::NEG_X } else { Vec2::NEG_Y },
            )
        } else {
            (second, first, if axis == 0 { Vec2::X } else { Vec2::Y })
        };
        if near > enter {
            enter = near;
            normal = near_normal;
        }
        exit = exit.min(far);
        if enter > exit {
            return None;
        }
    }
    (enter <= 1.0 && exit >= 0.0 && normal != Vec2::ZERO).then_some((enter.clamp(0.0, 1.0), normal))
}

#[cfg(test)]
mod tests {
    use crate::{
        DenState, NormalizedRect, WindowAffordance, WindowAffordanceFrame, WindowId, WorldObject,
    };

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

    #[test]
    fn authored_speed_is_not_multiplied_by_ultrawide_width() {
        let den = DenState::for_seed(8);
        let mut horizontal = WorldObject::canonical_orb(8, den.anchor);
        let mut vertical = horizontal.clone();
        horizontal.position = Vec2::splat(0.5);
        vertical.position = Vec2::splat(0.5);
        horizontal.velocity = Vec2::new(0.1, 0.0);
        vertical.velocity = Vec2::new(0.0, 0.1);
        horizontal.linear_drag = 0.0;
        vertical.linear_drag = 0.0;
        let config = ObjectPhysicsConfig {
            desktop_aspect: 32.0 / 9.0,
            reference_height_px: 1_152.0,
            pet_radius_px_at_reference: 58.0,
        };
        step_object(&mut horizontal, config, 1.0 / 120.0);
        step_object(&mut vertical, config, 1.0 / 120.0);
        let horizontal_px = (horizontal.position.x - 0.5) * config.desktop_aspect * 1_152.0;
        let vertical_px = (vertical.position.y - 0.5) * 1_152.0;
        assert!((horizontal_px - vertical_px).abs() < 0.001);
    }

    #[test]
    fn swept_orb_cannot_tunnel_through_thin_window() {
        let den = DenState::for_seed(9);
        let mut orb = WorldObject::canonical_orb(9, den.anchor);
        orb.position = Vec2::new(0.56, 0.5);
        orb.velocity = Vec2::new(MAX_OBJECT_SPEED, 0.0);
        orb.linear_drag = 0.0;
        let mut windows = WindowAffordanceFrame::default();
        windows.push(WindowAffordance {
            id: WindowId(1),
            bounds: NormalizedRect {
                minimum: Vec2::new(0.60, 0.2),
                maximum: Vec2::new(0.605, 0.8),
            },
            nearest_edge_point: Vec2::new(0.60, 0.5),
            nearest_edge_normal: Vec2::NEG_X,
            is_visible: true,
            ..WindowAffordance::default()
        });
        windows.finish();
        let mut environment = EmbodiedEnvironmentFrame::default();
        step_object_with_windows(
            &mut orb,
            ObjectPhysicsConfig::default(),
            &windows,
            1.0 / 30.0,
            &mut environment,
        );
        assert!(orb.velocity.x < 0.0);
        assert_eq!(environment.contact_count, 1);
        assert_eq!(environment.contacts[0].source, ContactSource::Window);
    }

    #[test]
    fn moving_window_transfers_only_bounded_relative_impulse() {
        let den = DenState::for_seed(10);
        let mut orb = WorldObject::canonical_orb(10, den.anchor);
        orb.position = Vec2::new(0.52, 0.5);
        orb.velocity = Vec2::ZERO;
        let mut windows = WindowAffordanceFrame::default();
        windows.push(WindowAffordance {
            id: WindowId(2),
            bounds: NormalizedRect {
                minimum: Vec2::new(0.50, 0.3),
                maximum: Vec2::new(0.72, 0.7),
            },
            velocity: Vec2::new(-0.4, 0.0),
            nearest_edge_point: Vec2::new(0.50, 0.5),
            nearest_edge_normal: Vec2::NEG_X,
            overlap_pressure: 0.4,
            motion_energy: 0.2,
            is_visible: true,
            ..WindowAffordance::default()
        });
        windows.finish();
        let mut environment = EmbodiedEnvironmentFrame::default();
        step_object_with_windows(
            &mut orb,
            ObjectPhysicsConfig::default(),
            &windows,
            1.0 / 120.0,
            &mut environment,
        );
        assert!(orb.velocity.is_finite());
        assert!(orb.velocity.length() <= MAX_OBJECT_SPEED + 1.0e-5);
        assert_eq!(environment.contact_count, 1);
    }

    #[test]
    fn orb_and_pet_resolve_as_one_local_contact_without_teleporting() {
        let den = DenState::for_seed(11);
        let mut orb = WorldObject::canonical_orb(11, den.anchor);
        orb.position = Vec2::new(0.51, 0.5);
        orb.velocity = Vec2::new(-0.4, 0.0);
        let mut environment = EmbodiedEnvironmentFrame::default();
        let collided = resolve_object_body_contact(
            &mut orb,
            Vec2::splat(0.5),
            Vec2::ZERO,
            ObjectPhysicsConfig::default(),
            &mut environment,
        );
        assert!(collided);
        assert_eq!(environment.contact_count, 1);
        assert_eq!(environment.contacts[0].source, ContactSource::Orb);
        assert!(orb.position.is_finite());
        assert!(orb.velocity.x > 0.0);
        assert!(orb.velocity.length() <= MAX_OBJECT_SPEED + 1.0e-5);
    }

    #[test]
    fn pet_contact_cannot_pin_orb_center_to_desktop_edge() {
        let den = crate::DenState::for_seed(13);
        let mut orb = WorldObject::canonical_orb(13, den.anchor);
        let config = ObjectPhysicsConfig::default();
        let object_radius = orb.radius_px_at_reference / config.reference_height_px;
        let body_radius = config.pet_radius_px_at_reference / config.reference_height_px;
        let body_position = Vec2::new(0.5, 0.01);
        orb.position = Vec2::new(0.5, 0.0);
        orb.velocity = Vec2::ZERO;
        let mut environment = EmbodiedEnvironmentFrame::default();

        assert!(resolve_object_body_contact(
            &mut orb,
            body_position,
            Vec2::ZERO,
            config,
            &mut environment,
        ));

        let aspect = config.desktop_aspect;
        let orb_height_space = Vec2::new(orb.position.x * aspect, orb.position.y);
        let body_height_space = Vec2::new(body_position.x * aspect, body_position.y);
        assert!(orb.position.y >= object_radius);
        assert!(orb.position.y <= 1.0 - object_radius);
        assert!(
            orb_height_space.distance(body_height_space) >= object_radius + body_radius - 1.0e-5
        );
        assert!(orb.position.y > object_radius + 0.01);

        for _ in 0..240 {
            step_object(&mut orb, config, 1.0 / 120.0);
            let _ = resolve_object_body_contact(
                &mut orb,
                body_position,
                Vec2::ZERO,
                config,
                &mut environment,
            );
        }
        assert!(orb.position.y > object_radius + 0.01);
        assert!(orb.position.is_finite());
        assert!(orb.velocity.is_finite());
    }

    #[test]
    fn eight_hertz_adversarial_window_spam_stays_finite_and_contact_bounded() {
        let den = DenState::for_seed(12);
        let mut orb = WorldObject::canonical_orb(12, den.anchor);
        orb.position = Vec2::splat(0.5);
        orb.velocity = Vec2::new(0.7, -0.4);
        let config = ObjectPhysicsConfig::default();
        for tick in 0..7_200_u32 {
            let phase = (tick / 15) as f32 * 0.37;
            let center = Vec2::new(0.5 + phase.sin() * 0.18, 0.5 + phase.cos() * 0.12);
            let mut windows = WindowAffordanceFrame::default();
            for index in 0..6_u64 {
                let offset = (index as f32 - 2.5) * 0.018;
                windows.push(WindowAffordance {
                    id: WindowId(index + 1),
                    bounds: NormalizedRect {
                        minimum: (center + Vec2::new(offset - 0.02, -0.16))
                            .clamp(Vec2::ZERO, Vec2::ONE),
                        maximum: (center + Vec2::new(offset + 0.02, 0.16))
                            .clamp(Vec2::ZERO, Vec2::ONE),
                    },
                    velocity: Vec2::new(phase.cos() * 0.066, -phase.sin() * 0.044),
                    nearest_edge_point: center,
                    nearest_edge_normal: if index % 2 == 0 { Vec2::NEG_X } else { Vec2::X },
                    overlap_pressure: 0.5,
                    motion_energy: 0.4,
                    is_visible: true,
                    ..WindowAffordance::default()
                });
            }
            windows.finish();
            let mut environment = EmbodiedEnvironmentFrame::default();
            step_object_with_windows(&mut orb, config, &windows, 1.0 / 120.0, &mut environment);
            assert!(orb.position.is_finite());
            assert!(orb.velocity.is_finite());
            assert!(orb.velocity.length() <= MAX_OBJECT_SPEED + 1.0e-5);
            assert!(environment.contact_count <= 4);
        }
    }
}
