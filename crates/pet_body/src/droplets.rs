use std::{array, f32::consts::TAU};

use glam::Vec2;
use lifecore::BodyFeedback;

use crate::{
    DerivedVisualTraits, DropletTuning, ModalDeformation, VisualMindInput, VisualPhysiologyPose,
    visual_traits::unit_from_hash,
};

pub const MAX_DROPLETS: usize = 8;
const MIN_ACTIVE_DROPLETS: usize = 6;
const MAX_DROPLET_RADIUS: f32 = 0.095;
const MAX_RELATIVE_VELOCITY: f32 = 6.0;

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct DropletMotion {
    /// Body displacement since the previous simulation tick, in shader-local space.
    pub displacement: Vec2,
    /// Unrotated presentation displacement. Free liquid fragments subtract this
    /// value so their desktop position no longer inherits body translation.
    pub presentation_displacement: Vec2,
    /// Body velocity expressed in the same shader-local space as droplet positions.
    pub velocity: Vec2,
    /// Body acceleration expressed in shader-local units per second squared.
    pub acceleration: Vec2,
    /// Normalized desktop units to shader-local units for interaction geometry.
    pub world_to_body_scale: Vec2,
}

impl DropletMotion {
    /// Screen-aligned kinematics for the ParticlePbf liquid. Its particles are
    /// already physically leaned and are presented directly by the density
    /// shader, so applying analytic pose rotation/squash here would put forces
    /// in a different frame than the material.
    #[must_use]
    pub fn from_screen_space(
        feedback: &BodyFeedback,
        normalized_displacement: Vec2,
        world_to_body_scale: Vec2,
    ) -> Self {
        let safe_scale =
            finite_vec2(world_to_body_scale).clamp(Vec2::splat(-64.0), Vec2::splat(64.0));
        let displacement = finite_vec2(normalized_displacement) * safe_scale;
        Self {
            displacement,
            presentation_displacement: displacement,
            velocity: finite_vec2(feedback.velocity) * safe_scale,
            acceleration: finite_vec2(feedback.acceleration) * safe_scale,
            world_to_body_scale: safe_scale,
        }
        .sanitized()
    }

    #[must_use]
    pub fn from_normalized_desktop(
        feedback: &BodyFeedback,
        normalized_displacement: Vec2,
        world_to_body_scale: Vec2,
        body_tilt: f32,
        body_squash: Vec2,
    ) -> Self {
        let safe_scale =
            finite_vec2(world_to_body_scale).clamp(Vec2::splat(-64.0), Vec2::splat(64.0));
        let safe_squash = finite_vec2(body_squash)
            .abs()
            .clamp(Vec2::splat(0.35), Vec2::splat(2.0));
        let tilt = if body_tilt.is_finite() {
            body_tilt
        } else {
            0.0
        };
        Self {
            displacement: rotate(finite_vec2(normalized_displacement) * safe_scale, -tilt)
                / safe_squash,
            presentation_displacement: finite_vec2(normalized_displacement) * safe_scale,
            velocity: rotate(finite_vec2(feedback.velocity) * safe_scale, -tilt) / safe_squash,
            acceleration: rotate(finite_vec2(feedback.acceleration) * safe_scale, -tilt)
                / safe_squash,
            world_to_body_scale: safe_scale,
        }
        .sanitized()
    }

    fn sanitized(self) -> Self {
        Self {
            displacement: finite_vec2(self.displacement).clamp_length_max(0.48),
            presentation_displacement: finite_vec2(self.presentation_displacement)
                .clamp_length_max(0.48),
            velocity: finite_vec2(self.velocity).clamp_length_max(16.0),
            acceleration: finite_vec2(self.acceleration).clamp_length_max(64.0),
            world_to_body_scale: finite_vec2(self.world_to_body_scale)
                .clamp(Vec2::splat(-64.0), Vec2::splat(64.0)),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum DropletLifecycle {
    #[default]
    Attached,
    Budding,
    Detached,
    Returning,
}

impl DropletLifecycle {
    fn is_mobile(self) -> bool {
        !matches!(self, Self::Attached)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct DropletState {
    pub position: Vec2,
    pub velocity: Vec2,
    pub target: Vec2,
    pub anchor: Vec2,
    pub radius: f32,
    pub shell_angle: f32,
    pub inertia_weight: f32,
    pub response: f32,
    pub lifecycle: DropletLifecycle,
    pub lifecycle_elapsed: f32,
    pub lifecycle_duration: f32,
    /// Zero is a separate parcel; one is fully adhered to the body surface.
    pub attachment: f32,
    pub stretch: f32,
    pub rotation: f32,
    pub phase: f32,
    pub activity: f32,
    /// Radius of the viscoelastic neck joining this parcel to its shell anchor.
    pub neck_radius: f32,
    /// Visibility/strength of the bridge. Kept separate from body attachment.
    pub bridge: f32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct DropletRenderState {
    pub position: Vec2,
    pub radius: f32,
    pub attachment: f32,
    pub stretch: f32,
    pub rotation: f32,
    pub phase: f32,
    pub activity: f32,
    pub anchor: Vec2,
    pub neck_radius: f32,
    pub bridge: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DropletRuntime {
    pub droplets: [DropletState; MAX_DROPLETS],
    pub active_count: usize,
    elapsed: f32,
    lag_direction: Vec2,
    /// Accumulated body translation in the parcel simulation frame.
    body_origin: Vec2,
    tuning: DropletTuning,
}

impl DropletRuntime {
    #[must_use]
    pub fn new(seed: u64, traits: &DerivedVisualTraits) -> Self {
        let active_count = usize::from(
            traits
                .droplet_count
                .clamp(MIN_ACTIVE_DROPLETS as u8, MAX_DROPLETS as u8),
        );
        let droplets = array::from_fn(|index| {
            let unit = |offset: u64| unit_from_hash(seed ^ (index as u64 + offset).rotate_left(11));
            // Even angular ownership makes a corona of attached lobes instead of a
            // one-dimensional chain. The offset avoids placing a lobe directly over
            // the center of the face on common 6- and 8-parcel identities.
            let shell_angle =
                -0.35 + TAU * (index as f32 / active_count as f32) + (unit(10) - 0.5) * 0.10;
            let rank = (index + seed as usize % active_count) % active_count;
            // A designed mass hierarchy reads as liquid breakup. Near-equal radii
            // read as a procedural particle ring regardless of shader quality.
            let size_scale = match rank {
                0 => 1.34,
                1 => lerp(0.98, 1.10, unit(1)),
                2 => lerp(0.72, 0.84, unit(1)),
                _ => lerp(0.42, 0.62, unit(1)),
            };
            let inertia_weight = lerp(0.22, 0.82, unit(20));
            let radius = (traits.droplet_size * size_scale).min(MAX_DROPLET_RADIUS);
            let response = lerp(0.68, 1.34, unit(30));
            let direction = Vec2::new(shell_angle.cos(), shell_angle.sin());
            let tangent = Vec2::new(-direction.y, direction.x);
            let phase = unit(40) * TAU;
            let lifecycle = match rank {
                1 => DropletLifecycle::Budding,
                2 => DropletLifecycle::Detached,
                _ => DropletLifecycle::Attached,
            };
            let radial_scale = match lifecycle {
                DropletLifecycle::Attached => 0.96,
                DropletLifecycle::Budding => 1.04,
                DropletLifecycle::Detached => 1.16,
                DropletLifecycle::Returning => 1.0,
            };
            let position = ellipse_surface(direction) * radial_scale
                + tangent
                    * if lifecycle == DropletLifecycle::Detached {
                        radius * 0.55
                    } else {
                        0.0
                    };
            let lifecycle_duration = lifecycle_duration(lifecycle, index, phase);
            DropletState {
                position,
                target: position,
                anchor: ellipse_surface(direction) * 0.96,
                radius,
                shell_angle,
                inertia_weight,
                response,
                lifecycle,
                lifecycle_elapsed: if lifecycle == DropletLifecycle::Budding {
                    lifecycle_duration * 0.42
                } else {
                    0.0
                },
                lifecycle_duration,
                attachment: match lifecycle {
                    DropletLifecycle::Attached => 1.0,
                    DropletLifecycle::Budding => 0.58,
                    DropletLifecycle::Detached | DropletLifecycle::Returning => 0.0,
                },
                rotation: shell_angle,
                phase,
                activity: if index < active_count { 1.0 } else { 0.0 },
                neck_radius: if lifecycle == DropletLifecycle::Budding {
                    radius * 0.24
                } else {
                    0.0
                },
                bridge: if lifecycle == DropletLifecycle::Budding {
                    1.0
                } else {
                    0.0
                },
                ..DropletState::default()
            }
        });
        Self {
            droplets,
            active_count,
            elapsed: 0.0,
            lag_direction: Vec2::NEG_Y,
            body_origin: Vec2::ZERO,
            tuning: DropletTuning::default(),
        }
    }

    pub fn set_tuning(&mut self, tuning: DropletTuning) {
        let size_ratio = tuning.size_scale / self.tuning.size_scale.max(0.001);
        for droplet in &mut self.droplets {
            droplet.radius = (droplet.radius * size_ratio).clamp(0.006, MAX_DROPLET_RADIUS);
        }
        self.active_count = tuning.count.clamp(1, MAX_DROPLETS);
        self.tuning = tuning;
    }

    #[must_use]
    pub fn tuning(&self) -> DropletTuning {
        self.tuning
    }

    pub fn update(
        &mut self,
        traits: &DerivedVisualTraits,
        physiology: VisualPhysiologyPose,
        mind: VisualMindInput,
        motion: DropletMotion,
        gaze: Vec2,
        dt: f32,
    ) {
        self.update_with_morph(
            traits,
            physiology,
            mind,
            motion,
            ModalDeformation::default(),
            gaze,
            dt,
        );
    }

    #[allow(clippy::too_many_arguments)]
    pub fn update_with_morph(
        &mut self,
        _traits: &DerivedVisualTraits,
        physiology: VisualPhysiologyPose,
        mut mind: VisualMindInput,
        motion: DropletMotion,
        morph: ModalDeformation,
        gaze: Vec2,
        dt: f32,
    ) {
        mind.sanitize();
        let dt = if dt.is_finite() {
            dt.clamp(0.0, 0.05)
        } else {
            0.0
        };
        self.elapsed = (self.elapsed + dt).rem_euclid(3_600.0);
        let motion = motion.sanitized();
        self.body_origin += motion.displacement;
        if self.body_origin.length() > 16.0 {
            let rebase = self.body_origin;
            for droplet in &mut self.droplets {
                droplet.position -= rebase;
                droplet.target -= rebase;
                droplet.anchor -= rebase;
            }
            self.body_origin = Vec2::ZERO;
        }
        let body_velocity = motion.velocity;
        let body_acceleration = motion.acceleration;
        let speed = body_velocity.length();
        let motion_amount = (speed / 1.35).clamp(0.0, 1.0);
        let desired_lag_direction = if speed > 0.012 {
            -body_velocity / speed
        } else if body_acceleration.length() > 0.08 {
            -body_acceleration.normalize()
        } else {
            self.lag_direction
        };
        let direction_blend = 1.0 - (-6.5 * dt).exp();
        self.lag_direction = self
            .lag_direction
            .lerp(desired_lag_direction, direction_blend)
            .normalize_or_zero();
        if self.lag_direction.length_squared() < 0.5 {
            self.lag_direction = Vec2::NEG_Y;
        }
        let sleepy_reduction = usize::from(mind.fatigue > 0.88);
        let visual_count = self.active_count.saturating_sub(sleepy_reduction).max(1);
        let mut mobile_count = self.droplets[..self.active_count]
            .iter()
            .filter(|droplet| droplet.lifecycle.is_mobile())
            .count();
        for (index, droplet) in self.droplets.iter_mut().enumerate() {
            let shell_direction = Vec2::new(droplet.shell_angle.cos(), droplet.shell_angle.sin());
            let shell_tangent = Vec2::new(-shell_direction.y, shell_direction.x);
            let local_anchor = morph.deform_point(ellipse_surface(shell_direction) * 0.96);
            let attached_anchor = self.body_origin + local_anchor;
            droplet.anchor = attached_anchor;

            if index < self.active_count {
                droplet.lifecycle_elapsed += dt * lerp(0.82, 1.18, physiology.droplet_energy);
                let lifecycle_finished = droplet.lifecycle_elapsed >= droplet.lifecycle_duration;
                let return_close = droplet.lifecycle == DropletLifecycle::Returning
                    && droplet.lifecycle_elapsed > 0.20
                    && droplet.position.distance(attached_anchor) < droplet.radius * 0.85;
                match droplet.lifecycle {
                    DropletLifecycle::Attached
                        if lifecycle_finished
                            && mobile_count < self.tuning.maximum_mobile.min(self.active_count) =>
                    {
                        set_lifecycle(droplet, DropletLifecycle::Budding, index);
                        mobile_count += 1;
                    }
                    DropletLifecycle::Attached if lifecycle_finished => {
                        // Wait near the threshold instead of synchronizing all queued
                        // parcels on the first frame a mobile slot becomes available.
                        droplet.lifecycle_elapsed = droplet.lifecycle_duration * 0.78;
                    }
                    DropletLifecycle::Budding if lifecycle_finished => {
                        set_lifecycle(droplet, DropletLifecycle::Detached, index);
                    }
                    DropletLifecycle::Detached if lifecycle_finished => {
                        set_lifecycle(droplet, DropletLifecycle::Returning, index);
                    }
                    DropletLifecycle::Returning if lifecycle_finished || return_close => {
                        set_lifecycle(droplet, DropletLifecycle::Attached, index);
                        mobile_count = mobile_count.saturating_sub(1);
                    }
                    _ => {}
                }
            }

            // Positions live in an accumulated world frame. Translation is therefore
            // real inertia instead of an authored fraction subtracted in local space.
            let separation_after_motion = droplet.position.distance(attached_anchor);
            let return_threshold = (0.22 + droplet.radius * 1.35) * self.tuning.detach_distance;
            if matches!(
                droplet.lifecycle,
                DropletLifecycle::Budding | DropletLifecycle::Detached
            ) && separation_after_motion > return_threshold
            {
                set_lifecycle(droplet, DropletLifecycle::Returning, index);
            }
            let activity_target = if index < visual_count { 1.0 } else { 0.0 };
            droplet.activity = smooth(droplet.activity, activity_target, 2.8, dt);

            let lifecycle_progress =
                (droplet.lifecycle_elapsed / droplet.lifecycle_duration.max(0.001)).clamp(0.0, 1.0);
            let attachment_target = match droplet.lifecycle {
                DropletLifecycle::Attached => 1.0,
                DropletLifecycle::Budding => 1.0 - smoothstep01(lifecycle_progress),
                DropletLifecycle::Detached => 0.0,
                DropletLifecycle::Returning => {
                    let merge_range = droplet.radius * 3.2 * self.tuning.merge_distance;
                    (1.0 - droplet.position.distance(attached_anchor) / merge_range).clamp(0.0, 1.0)
                }
            };
            droplet.attachment = smooth(droplet.attachment, attachment_target, 8.0, dt);

            let wobble_phase = self.elapsed * (0.16 + droplet.response * 0.17) + droplet.phase;
            let membrane_wobble = shell_tangent
                * wobble_phase.sin()
                * (0.0025 + mind.curiosity * 0.0035 + mind.stress * 0.0020)
                + shell_direction
                    * (wobble_phase * 0.71).cos()
                    * physiology.droplet_energy
                    * 0.0025;
            let outward_excursion = match droplet.lifecycle {
                DropletLifecycle::Attached => 0.0,
                DropletLifecycle::Budding => {
                    droplet.radius * lerp(0.15, 2.10, smoothstep01(lifecycle_progress))
                }
                DropletLifecycle::Detached => {
                    droplet.radius * (2.15 + (wobble_phase * 0.63).sin() * 0.38)
                }
                DropletLifecycle::Returning => 0.0,
            };
            let free_orbit = if droplet.lifecycle == DropletLifecycle::Detached {
                shell_tangent
                    * (wobble_phase * 0.79).sin()
                    * droplet.radius
                    * lerp(0.65, 1.75, physiology.droplet_spread / 1.22)
                    * self.tuning.spread_scale
                    + self.lag_direction
                        * motion_amount
                        * droplet.radius
                        * 0.42
                        * self.tuning.inertia_scale
            } else {
                Vec2::ZERO
            };
            let idle_curiosity = gaze.clamp_length_max(1.0)
                * mind.curiosity
                * (1.0 - motion_amount)
                * 0.012
                * lerp(0.45, 0.85, droplet.inertia_weight);
            let local_target = (local_anchor
                + membrane_wobble
                + shell_direction * outward_excursion
                + free_orbit
                + idle_curiosity * droplet.attachment)
                .clamp_length_max(self.tuning.maximum_distance);
            droplet.target = self.body_origin + local_target;

            let lifecycle_stiffness = match droplet.lifecycle {
                DropletLifecycle::Attached => 1.0,
                DropletLifecycle::Budding => 0.52,
                DropletLifecycle::Detached => 0.075,
                DropletLifecycle::Returning => 1.28 * self.tuning.return_strength,
            };
            let lifecycle_drag = match droplet.lifecycle {
                DropletLifecycle::Attached => 1.0,
                DropletLifecycle::Budding => 0.74,
                DropletLifecycle::Detached => 0.18,
                DropletLifecycle::Returning => 0.82,
            };
            let stiffness = self.tuning.elasticity
                * droplet.response
                * lerp(1.18, 1.02, droplet.inertia_weight)
                * lerp(1.08, 0.78, self.tuning.lag / 0.24)
                * lerp(0.88, 1.12, physiology.droplet_cohesion)
                * lifecycle_stiffness;
            let damping = self.tuning.drag
                * lerp(0.98, 0.72, motion_amount)
                * lerp(1.04, 0.92, droplet.inertia_weight)
                * lerp(1.0, 1.18, mind.fatigue)
                * lifecycle_drag;
            // This one-sided reel only bounds exhausted excursions; world-frame
            // momentum, not this force, creates the visible lag.
            let tether_offset = droplet.position - attached_anchor;
            let tether_distance = tether_offset.length();
            let tether_slack = match droplet.lifecycle {
                DropletLifecycle::Attached => 0.022,
                DropletLifecycle::Budding => 0.072,
                DropletLifecycle::Detached => 0.20 + droplet.radius * 0.90,
                DropletLifecycle::Returning => 0.052,
            };
            let tether_extension = (tether_distance - tether_slack).max(0.0);
            let tether_direction = tether_offset.normalize_or_zero();
            let tether_strength = match droplet.lifecycle {
                DropletLifecycle::Attached => 82.0,
                DropletLifecycle::Budding => 48.0,
                DropletLifecycle::Detached => 13.0,
                DropletLifecycle::Returning => 108.0 * self.tuning.return_strength,
            } * self.tuning.tether_strength;
            let nonlinear_reel = tether_direction
                * tether_extension
                * tether_strength
                * (1.0 + tether_extension * 7.0);
            let anchor_velocity = match droplet.lifecycle {
                DropletLifecycle::Attached => body_velocity,
                DropletLifecycle::Budding => body_velocity * 0.82,
                DropletLifecycle::Detached => Vec2::ZERO,
                DropletLifecycle::Returning => body_velocity * 0.72,
            };
            let relative_velocity = droplet.velocity - anchor_velocity;
            let acceleration = (droplet.target - droplet.position) * stiffness
                - relative_velocity * damping
                - nonlinear_reel;
            droplet.velocity =
                (droplet.velocity + acceleration * dt).clamp_length_max(MAX_RELATIVE_VELOCITY);
            droplet.position += droplet.velocity * dt;
            let local_position = droplet.position - self.body_origin;
            if local_position.length() > self.tuning.maximum_distance {
                set_lifecycle(droplet, DropletLifecycle::Returning, index);
                droplet.position = self.body_origin
                    + local_position.normalize_or_zero() * self.tuning.maximum_distance;
                droplet.velocity *= 0.45;
            }
            let shape_velocity = droplet.velocity - body_velocity;
            let shape_speed = shape_velocity.length();
            let lifecycle_stretch = match droplet.lifecycle {
                DropletLifecycle::Budding => lifecycle_progress * 0.16,
                DropletLifecycle::Detached => 0.06,
                _ => 0.0,
            };
            droplet.stretch = smooth(
                droplet.stretch,
                ((shape_speed * 0.055 + lifecycle_stretch) * self.tuning.stretch_scale)
                    .clamp(0.0, 0.92),
                lerp(4.0, 7.5, droplet.response / 1.34),
                dt,
            );
            if shape_speed > 0.035 {
                let desired_rotation = shape_velocity.y.atan2(shape_velocity.x);
                let turn = shortest_angle_delta(droplet.rotation, desired_rotation);
                let maximum_turn = lerp(1.45, 3.05, droplet.response / 1.34) * dt;
                droplet.rotation += turn.clamp(-maximum_turn, maximum_turn);
            }
            droplet.rotation = droplet.rotation.rem_euclid(TAU);
            let bridge_span = droplet.position.distance(attached_anchor);
            let (bridge_target, neck_target) = match droplet.lifecycle {
                DropletLifecycle::Attached | DropletLifecycle::Detached => (0.0, 0.0),
                DropletLifecycle::Budding => {
                    let thinning = (1.0 - smoothstep01(lifecycle_progress)).sqrt();
                    let stretch_thinning = (1.0 + bridge_span / droplet.radius.max(0.008) * 0.45)
                        .sqrt()
                        .recip();
                    (1.0, droplet.radius * 0.34 * thinning * stretch_thinning)
                }
                DropletLifecycle::Returning => {
                    let merge_range = droplet.radius * 3.5 * self.tuning.merge_distance;
                    let coalescence = (1.0 - bridge_span / merge_range.max(0.001)).clamp(0.0, 1.0);
                    (
                        smoothstep01(coalescence),
                        droplet.radius * 0.31 * coalescence.sqrt(),
                    )
                }
            };
            droplet.bridge = smooth(droplet.bridge, bridge_target, 12.0, dt);
            droplet.neck_radius = smooth(
                droplet.neck_radius,
                neck_target * self.tuning.bridge_radius_scale,
                15.0,
                dt,
            );
            droplet.phase = (droplet.phase
                + dt * droplet.response * lerp(0.11, 0.47, physiology.droplet_energy))
            .rem_euclid(TAU);
        }
    }

    #[must_use]
    pub fn render_states(&self) -> [DropletRenderState; MAX_DROPLETS] {
        self.droplets.map(|droplet| DropletRenderState {
            position: droplet.position - self.body_origin,
            radius: droplet.radius.clamp(0.0, MAX_DROPLET_RADIUS),
            attachment: droplet.attachment.clamp(0.0, 1.0),
            stretch: droplet.stretch.clamp(0.0, 1.0),
            rotation: droplet.rotation,
            phase: droplet.phase,
            activity: droplet.activity.clamp(0.0, 1.0),
            anchor: droplet.anchor - self.body_origin,
            neck_radius: droplet.neck_radius.clamp(0.0, MAX_DROPLET_RADIUS),
            bridge: droplet.bridge.clamp(0.0, 1.0),
        })
    }

    #[must_use]
    pub fn centroid(&self) -> Vec2 {
        let count = self.active_count.clamp(1, MAX_DROPLETS);
        self.droplets[..count]
            .iter()
            .map(|droplet| droplet.position - self.body_origin)
            .sum::<Vec2>()
            / count as f32
    }

    #[must_use]
    pub fn is_valid(&self) -> bool {
        (1..=MAX_DROPLETS).contains(&self.active_count)
            && self.droplets.iter().all(|droplet| {
                droplet.position.is_finite()
                    && droplet.velocity.is_finite()
                    && droplet.target.is_finite()
                    && (droplet.position - self.body_origin).length()
                        <= self.tuning.maximum_distance + 0.000_1
                    && droplet.velocity.length() <= MAX_RELATIVE_VELOCITY + 0.000_1
                    && droplet.radius.is_finite()
                    && droplet.lifecycle_duration.is_finite()
                    && droplet.lifecycle_elapsed.is_finite()
                    && droplet.attachment.is_finite()
                    && (0.0..=1.0).contains(&droplet.attachment)
                    && droplet.stretch.is_finite()
                    && droplet.rotation.is_finite()
                    && droplet.phase.is_finite()
                    && droplet.activity.is_finite()
                    && droplet.anchor.is_finite()
                    && droplet.neck_radius.is_finite()
                    && droplet.bridge.is_finite()
            })
    }
}

fn set_lifecycle(droplet: &mut DropletState, lifecycle: DropletLifecycle, index: usize) {
    droplet.lifecycle = lifecycle;
    droplet.lifecycle_elapsed = 0.0;
    droplet.lifecycle_duration = lifecycle_duration(lifecycle, index, droplet.phase);
}

fn lifecycle_duration(lifecycle: DropletLifecycle, index: usize, phase: f32) -> f32 {
    let variation = 0.5 + 0.5 * (phase * 1.73 + index as f32 * 2.19).sin();
    match lifecycle {
        DropletLifecycle::Attached => lerp(1.6, 5.2, variation),
        DropletLifecycle::Budding => lerp(0.48, 0.96, variation),
        DropletLifecycle::Detached => lerp(1.05, 2.75, variation),
        DropletLifecycle::Returning => lerp(0.72, 1.55, variation),
    }
}

fn shortest_angle_delta(current: f32, target: f32) -> f32 {
    (target - current + std::f32::consts::PI).rem_euclid(TAU) - std::f32::consts::PI
}

fn smoothstep01(value: f32) -> f32 {
    let value = value.clamp(0.0, 1.0);
    value * value * (3.0 - 2.0 * value)
}

fn ellipse_surface(direction: Vec2) -> Vec2 {
    let direction = direction.normalize_or_zero();
    let denominator = ((direction.x / 0.35).powi(2) + (direction.y / 0.43).powi(2)).sqrt();
    if denominator > 0.000_1 {
        direction / denominator
    } else {
        Vec2::new(0.0, -0.43)
    }
}

fn rotate(vector: Vec2, angle: f32) -> Vec2 {
    let (sine, cosine) = angle.sin_cos();
    Vec2::new(
        vector.x * cosine - vector.y * sine,
        vector.x * sine + vector.y * cosine,
    )
}

fn finite_vec2(value: Vec2) -> Vec2 {
    if value.is_finite() { value } else { Vec2::ZERO }
}

fn lerp(start: f32, end: f32, amount: f32) -> f32 {
    start + (end - start) * amount
}

fn smooth(current: f32, target: f32, speed: f32, dt: f32) -> f32 {
    current + (target - current) * (1.0 - (-speed * dt).exp())
}

#[cfg(test)]
mod tests {
    use lifecore::Genome;

    use super::*;

    #[test]
    fn particle_liquid_kinematics_remain_screen_aligned_under_pose_changes() {
        let feedback = BodyFeedback {
            velocity: Vec2::new(0.7, -0.2),
            acceleration: Vec2::new(1.3, 0.4),
            ..BodyFeedback::default()
        };
        let scale = Vec2::new(2.0, -2.0);
        let upright = DropletMotion::from_screen_space(&feedback, Vec2::new(0.01, -0.02), scale);
        let repeated = DropletMotion::from_screen_space(&feedback, Vec2::new(0.01, -0.02), scale);
        assert_eq!(upright, repeated);
        assert!(upright.acceleration.x > 0.0);
        assert_eq!(upright.displacement, upright.presentation_displacement);

        let analytic = DropletMotion::from_normalized_desktop(
            &feedback,
            Vec2::new(0.01, -0.02),
            scale,
            0.16,
            Vec2::new(0.72, 1.24),
        );
        assert_ne!(analytic.acceleration, upright.acceleration);
    }
    use crate::VisualPhysiologyRuntime;

    fn local_motion(feedback: &BodyFeedback, dt: f32) -> DropletMotion {
        DropletMotion::from_normalized_desktop(
            feedback,
            feedback.velocity * dt,
            Vec2::ONE,
            0.0,
            Vec2::ONE,
        )
    }

    #[test]
    fn droplets_replay_deterministically_and_stay_bounded_for_24_virtual_hours() {
        let genome = Genome::from_seed(42);
        let traits = DerivedVisualTraits::from_genome(&genome);
        let mut physiology = VisualPhysiologyRuntime::new(genome.identity_seed, &traits);
        let mut first = DropletRuntime::new(
            genome.identity_seed ^ genome.body.pattern_seed.rotate_left(17),
            &traits,
        );
        let mut second = first.clone();
        let mind = VisualMindInput {
            arousal: 0.8,
            curiosity: 0.7,
            novelty: 0.5,
            ..VisualMindInput::default()
        };
        let feedback = BodyFeedback {
            velocity: Vec2::new(0.8, -0.25),
            acceleration: Vec2::new(-0.4, 0.2),
            ..BodyFeedback::default()
        };
        let dt = 0.05;
        for _ in 0..(24.0 * 60.0 * 60.0 / dt) as usize {
            physiology.update(&traits, mind, dt);
            first.update(
                &traits,
                physiology.pose,
                mind,
                local_motion(&feedback, dt),
                Vec2::new(0.6, -0.2),
                dt,
            );
            second.update(
                &traits,
                physiology.pose,
                mind,
                local_motion(&feedback, dt),
                Vec2::new(0.6, -0.2),
                dt,
            );
        }
        assert_eq!(first, second);
        assert!(first.is_valid());
    }

    #[test]
    fn fast_motion_creates_lag_stretch_and_sleep_reduces_activity() {
        let genome = Genome::from_seed(7);
        let traits = DerivedVisualTraits::from_genome(&genome);
        let mut physiology = VisualPhysiologyRuntime::new(genome.identity_seed, &traits);
        let mut runtime = DropletRuntime::new(genome.identity_seed, &traits);
        let excited = VisualMindInput {
            arousal: 1.0,
            curiosity: 1.0,
            ..VisualMindInput::default()
        };
        let moving = BodyFeedback {
            velocity: Vec2::new(1.0, 0.0),
            acceleration: Vec2::new(0.8, 0.0),
            ..BodyFeedback::default()
        };
        for _ in 0..240 {
            physiology.update(&traits, excited, 1.0 / 120.0);
            runtime.update(
                &traits,
                physiology.pose,
                excited,
                local_motion(&moving, 1.0 / 120.0),
                Vec2::X,
                1.0 / 120.0,
            );
        }
        let stretched = runtime
            .droplets
            .iter()
            .map(|droplet| droplet.stretch)
            .fold(0.0_f32, f32::max);
        assert!(stretched > 0.05);

        let sleepy = VisualMindInput {
            fatigue: 1.0,
            ..VisualMindInput::default()
        };
        let energetic = physiology.pose.droplet_energy;
        for _ in 0..480 {
            physiology.update(&traits, sleepy, 1.0 / 120.0);
        }
        assert!(physiology.pose.droplet_energy < energetic);
    }

    #[test]
    fn calm_lifecycle_is_asynchronous_and_never_mobilizes_the_whole_shell() {
        let genome = Genome::from_seed(19);
        let traits = DerivedVisualTraits::from_genome(&genome);
        let physiology = VisualPhysiologyRuntime::new(genome.identity_seed, &traits);
        let mut runtime = DropletRuntime::new(genome.identity_seed, &traits);
        let mut saw_detached = false;
        let mut saw_attachment_range = false;
        for _ in 0..2_400 {
            runtime.update(
                &traits,
                physiology.pose,
                VisualMindInput::default(),
                DropletMotion::default(),
                Vec2::ZERO,
                1.0 / 120.0,
            );
            let active = &runtime.droplets[..runtime.active_count];
            let mobile = active
                .iter()
                .filter(|droplet| droplet.lifecycle.is_mobile())
                .count();
            assert!(mobile <= 2, "mobile parcels {mobile}");
            saw_detached |= active
                .iter()
                .any(|droplet| droplet.lifecycle == DropletLifecycle::Detached);
            let minimum_attachment = active
                .iter()
                .map(|droplet| droplet.attachment)
                .fold(1.0_f32, f32::min);
            let maximum_attachment = active
                .iter()
                .map(|droplet| droplet.attachment)
                .fold(0.0_f32, f32::max);
            saw_attachment_range |= maximum_attachment - minimum_attachment > 0.55;
        }
        assert!(saw_detached);
        assert!(saw_attachment_range);
        assert!(runtime.is_valid());
    }

    #[test]
    fn parcel_sizes_have_a_clear_hero_medium_satellite_hierarchy() {
        let genome = Genome::from_seed(191);
        let traits = DerivedVisualTraits::from_genome(&genome);
        let runtime = DropletRuntime::new(genome.identity_seed, &traits);
        let active = &runtime.droplets[..runtime.active_count];
        let minimum = active
            .iter()
            .map(|droplet| droplet.radius)
            .fold(f32::INFINITY, f32::min);
        let maximum = active
            .iter()
            .map(|droplet| droplet.radius)
            .fold(0.0_f32, f32::max);
        assert!(maximum / minimum > 2.0, "radius range {minimum}..{maximum}");
    }

    #[test]
    fn moving_body_keeps_inertial_lobes_distributed_around_the_shell() {
        let genome = Genome::from_seed(23);
        let traits = DerivedVisualTraits::from_genome(&genome);
        let mut physiology = VisualPhysiologyRuntime::new(genome.identity_seed, &traits);
        let mut runtime = DropletRuntime::new(genome.identity_seed, &traits);
        let mind = VisualMindInput {
            arousal: 0.7,
            ..VisualMindInput::default()
        };
        let moving = BodyFeedback {
            velocity: Vec2::X,
            ..BodyFeedback::default()
        };
        for _ in 0..360 {
            physiology.update(&traits, mind, 1.0 / 120.0);
            runtime.update(
                &traits,
                physiology.pose,
                mind,
                local_motion(&moving, 1.0 / 120.0),
                Vec2::ZERO,
                1.0 / 120.0,
            );
        }
        let active = &runtime.droplets[..runtime.active_count];
        let centroid = active
            .iter()
            .map(|droplet| droplet.position - runtime.body_origin)
            .sum::<Vec2>()
            / active.len() as f32;
        let minimum = active
            .iter()
            .map(|droplet| droplet.position - runtime.body_origin)
            .reduce(Vec2::min)
            .expect("active droplets");
        let maximum = active
            .iter()
            .map(|droplet| droplet.position - runtime.body_origin)
            .reduce(Vec2::max)
            .expect("active droplets");
        assert!(centroid.length() < 0.14, "corona centroid {centroid:?}");
        assert!(
            minimum.x < -0.25 && maximum.x > 0.18,
            "horizontal corona bounds {minimum:?}..{maximum:?}"
        );
        assert!(
            minimum.y < -0.30 && maximum.y > 0.28,
            "vertical corona bounds {minimum:?}..{maximum:?}"
        );
        assert!(runtime.is_valid());
    }

    #[test]
    fn normalized_desktop_motion_is_mapped_into_shader_body_space() {
        let feedback = BodyFeedback {
            velocity: Vec2::new(0.10, 0.20),
            acceleration: Vec2::new(-0.25, 0.50),
            ..BodyFeedback::default()
        };
        let motion = DropletMotion::from_normalized_desktop(
            &feedback,
            Vec2::new(0.01, -0.02),
            Vec2::new(12.0, -7.0),
            0.0,
            Vec2::ONE,
        );
        assert!(motion.displacement.distance(Vec2::new(0.12, 0.14)) < 0.000_1);
        assert!(motion.velocity.distance(Vec2::new(1.2, -1.4)) < 0.000_1);
        assert!(motion.acceleration.distance(Vec2::new(-3.0, -3.5)) < 0.000_1);
    }

    #[test]
    fn attachment_state_controls_body_frame_inertia() {
        let genome = Genome::from_seed(31);
        let traits = DerivedVisualTraits::from_genome(&genome);
        let physiology = VisualPhysiologyRuntime::new(genome.identity_seed, &traits);
        let mut runtime = DropletRuntime::new(genome.identity_seed, &traits);
        set_lifecycle(&mut runtime.droplets[0], DropletLifecycle::Attached, 0);
        set_lifecycle(&mut runtime.droplets[1], DropletLifecycle::Detached, 1);
        runtime.droplets[0].lifecycle_duration = 1_000.0;
        runtime.droplets[1].lifecycle_duration = 1_000.0;
        for index in 0..2 {
            let direction = Vec2::new(
                runtime.droplets[index].shell_angle.cos(),
                runtime.droplets[index].shell_angle.sin(),
            );
            let anchor = ellipse_surface(direction) * 0.96;
            runtime.droplets[index].position = anchor;
            runtime.droplets[index].target = anchor;
            runtime.droplets[index].velocity = Vec2::ZERO;
        }
        let before = runtime.droplets.map(|droplet| droplet.position);
        runtime.update(
            &traits,
            physiology.pose,
            VisualMindInput::default(),
            DropletMotion {
                displacement: Vec2::new(0.10, 0.0),
                presentation_displacement: Vec2::new(0.10, 0.0),
                velocity: Vec2::new(1.2, 0.0),
                acceleration: Vec2::ZERO,
                world_to_body_scale: Vec2::ONE,
            },
            Vec2::ZERO,
            1.0 / 120.0,
        );
        let attached_world_motion = runtime.droplets[0].position.distance(before[0]);
        let detached_world_motion = runtime.droplets[1].position.distance(before[1]);
        let attached_local_lag = (runtime.droplets[0].position - runtime.body_origin)
            .distance(runtime.droplets[0].anchor - runtime.body_origin);
        let detached_local_lag = (runtime.droplets[1].position - runtime.body_origin)
            .distance(runtime.droplets[1].anchor - runtime.body_origin);
        assert!(runtime.body_origin.distance(Vec2::new(0.10, 0.0)) < 0.000_1);
        assert!(attached_world_motion > detached_world_motion);
        assert!(detached_local_lag > attached_local_lag);
    }

    #[test]
    fn sudden_motion_reels_an_overstretched_parcel_back_without_a_tether_plateau() {
        let genome = Genome::from_seed(317);
        let traits = DerivedVisualTraits::from_genome(&genome);
        let physiology = VisualPhysiologyRuntime::new(genome.identity_seed, &traits);
        let mut runtime = DropletRuntime::new(genome.identity_seed, &traits);
        let parcel = &mut runtime.droplets[0];
        set_lifecycle(parcel, DropletLifecycle::Detached, 0);
        parcel.lifecycle_duration = 1_000.0;
        parcel.inertia_weight = 1.0;
        let direction = Vec2::new(parcel.shell_angle.cos(), parcel.shell_angle.sin());
        let anchor = ellipse_surface(direction) * 0.96;
        parcel.position = anchor;
        parcel.target = anchor;
        parcel.velocity = Vec2::ZERO;
        parcel.attachment = 0.0;

        runtime.update(
            &traits,
            physiology.pose,
            VisualMindInput::default(),
            DropletMotion {
                displacement: Vec2::new(0.48, 0.0),
                presentation_displacement: Vec2::new(0.48, 0.0),
                velocity: Vec2::new(10.0, 0.0),
                acceleration: Vec2::new(32.0, 0.0),
                world_to_body_scale: Vec2::ONE,
            },
            Vec2::ZERO,
            1.0 / 120.0,
        );
        let peak_separation = runtime.droplets[0]
            .position
            .distance(runtime.droplets[0].anchor);
        assert_eq!(
            runtime.droplets[0].lifecycle,
            DropletLifecycle::Returning,
            "an exhausted inertia excursion should begin coalescing immediately"
        );
        assert!(
            peak_separation > 0.12,
            "impulse separation {peak_separation}"
        );

        for _ in 0..180 {
            runtime.update(
                &traits,
                physiology.pose,
                VisualMindInput::default(),
                DropletMotion::default(),
                Vec2::ZERO,
                1.0 / 120.0,
            );
        }
        let final_separation = runtime.droplets[0]
            .position
            .distance(runtime.droplets[0].anchor);
        assert!(
            final_separation < peak_separation * 0.40,
            "soft reel should collapse the separation instead of parking at a clamp: {peak_separation} -> {final_separation}"
        );
        assert!(runtime.is_valid());
    }

    #[test]
    fn parcel_orientation_has_a_hard_angular_speed_limit() {
        let genome = Genome::from_seed(313);
        let traits = DerivedVisualTraits::from_genome(&genome);
        let physiology = VisualPhysiologyRuntime::new(genome.identity_seed, &traits);
        let mut runtime = DropletRuntime::new(genome.identity_seed, &traits);
        set_lifecycle(&mut runtime.droplets[0], DropletLifecycle::Detached, 0);
        runtime.droplets[0].lifecycle_duration = 1_000.0;
        let dt = 1.0 / 120.0;
        for frame in 0..240 {
            let before = runtime.droplets[0].rotation;
            let direction = if frame % 2 == 0 { 1.0 } else { -1.0 };
            runtime.update(
                &traits,
                physiology.pose,
                VisualMindInput::default(),
                DropletMotion {
                    velocity: Vec2::new(direction * 8.0, 0.0),
                    ..DropletMotion::default()
                },
                Vec2::ZERO,
                dt,
            );
            let turn = shortest_angle_delta(before, runtime.droplets[0].rotation).abs();
            assert!(turn <= 3.05 * dt + 0.000_1, "angular step {turn}");
        }
    }
}
