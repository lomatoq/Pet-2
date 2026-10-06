use glam::Vec2;
use lifecore::{BodyFeedback, SensorFrame};
use pet_ecology::EmbodiedEnvironmentFrame;

use super::{
    kernels::wendland_c2_kernel,
    particles::{KERNEL_RADIUS, LiquidParticle, MAX_LIQUID_PARTICLES},
};

#[derive(Debug, Clone, PartialEq)]
pub struct MaterialGrab {
    held: bool,
    initialized: bool,
    target_center: Vec2,
    filtered_center: Vec2,
    filtered_velocity: Vec2,
    previous_filtered_velocity: Vec2,
    filtered_acceleration: Vec2,
    onset_center: Vec2,
    reference_origin: Vec2,
    onset_positions: [Vec2; MAX_LIQUID_PARTICLES],
    onset_weights: [f32; MAX_LIQUID_PARTICLES],
    captured_count: usize,
    patch_initialized: bool,
    amplitude: f32,
    field_axis: Vec2,
    differential_forces: [Vec2; MAX_LIQUID_PARTICLES],
    cursor_distance: f32,
    field_coverage: f32,
    effective_area_fraction: f32,
    effective_pressure: f32,
    weighted_contact_center: Vec2,
    weighted_material_velocity: Vec2,
    pressure_impulse: f32,
}

impl Default for MaterialGrab {
    fn default() -> Self {
        Self {
            held: false,
            initialized: false,
            target_center: Vec2::ZERO,
            filtered_center: Vec2::ZERO,
            filtered_velocity: Vec2::ZERO,
            previous_filtered_velocity: Vec2::ZERO,
            filtered_acceleration: Vec2::ZERO,
            onset_center: Vec2::ZERO,
            reference_origin: Vec2::ZERO,
            onset_positions: [Vec2::ZERO; MAX_LIQUID_PARTICLES],
            onset_weights: [0.0; MAX_LIQUID_PARTICLES],
            captured_count: 0,
            patch_initialized: false,
            amplitude: 0.0,
            field_axis: Vec2::ZERO,
            differential_forces: [Vec2::ZERO; MAX_LIQUID_PARTICLES],
            cursor_distance: 0.0,
            field_coverage: 0.0,
            effective_area_fraction: 0.0,
            effective_pressure: 0.0,
            weighted_contact_center: Vec2::ZERO,
            weighted_material_velocity: Vec2::ZERO,
            pressure_impulse: 0.0,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(super) struct MaterialGrabReadback {
    pub active: bool,
    pub held: bool,
    pub filtered_velocity: Vec2,
    pub filtered_acceleration: Vec2,
    pub contact_center: Vec2,
    pub material_velocity: Vec2,
    pub normal: Vec2,
    pub area_fraction: f32,
    pub effective_pressure: f32,
    pub pressure_impulse: f32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(super) struct MaterialStressFrame {
    pub center: Vec2,
    pub magnitude: f32,
    pub cursor_distance: f32,
    pub com_follow_ratio: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GrabParameters {
    /// Compact support of the captured material patch in body-local units.
    pub support_radius: f32,
    /// Maximum acceleration applied by the compliant material grasp.
    pub peak_acceleration: f32,
    pub response_hz: f32,
    pub max_speed_radii_per_second: f32,
    pub max_acceleration_radii_per_second_squared: f32,
    /// Time to reach approximately 95% of full amplitude.
    pub fade_in_seconds: f32,
    /// Time to decay to approximately 5% amplitude.
    pub fade_out_seconds: f32,
}

impl Default for GrabParameters {
    fn default() -> Self {
        Self {
            support_radius: KERNEL_RADIUS * 1.75,
            peak_acceleration: 6.0,
            response_hz: 18.0,
            max_speed_radii_per_second: 8.0,
            max_acceleration_radii_per_second_squared: 80.0,
            fade_in_seconds: 0.020,
            fade_out_seconds: 0.040,
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn apply_interaction_forces(
    particles: &mut [LiquidParticle; MAX_LIQUID_PARTICLES],
    count: usize,
    grab: &mut MaterialGrab,
    grab_parameters: GrabParameters,
    body_origin: Vec2,
    body_world_position: Vec2,
    world_to_body_scale: Vec2,
    _local_containment_bounds: Option<(Vec2, Vec2)>,
    sensors: &SensorFrame,
    feedback: &BodyFeedback,
    _scroll_velocity: f32,
    _main_component: u8,
    _main_com: Vec2,
    dt: f32,
) {
    let count = count.min(MAX_LIQUID_PARTICLES);
    let cursor_local = (sensors.cursor_position - body_world_position) * world_to_body_scale;
    let cursor_world = body_origin + cursor_local;
    grab.update_pointer(
        sensors.pet_dragged,
        cursor_world,
        body_origin,
        grab_parameters,
        dt,
    );
    grab.apply_field(particles, count, body_origin, grab_parameters);

    // A plain click and scroll contribute no hidden pointer pressure or velocity
    // target. Window collisions remain an independent environmental load.
    if let Some(collision) = &feedback.collision {
        let local_normal = (collision.normal * world_to_body_scale.signum()).normalize_or_zero();
        for particle in &mut particles[..count] {
            let exposure = (particle.position - body_origin)
                .normalize_or_zero()
                .dot(-local_normal)
                .max(0.0);
            particle.force += local_normal * collision.intensity * exposure * 9.0;
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ObjectContactForce {
    pub applied_world: Vec2,
    pub impact_world: Vec2,
}

pub fn apply_external_contact_forces(
    particles: &mut [LiquidParticle; MAX_LIQUID_PARTICLES],
    count: usize,
    body_origin: Vec2,
    body_world_position: Vec2,
    world_to_body_scale: Vec2,
    environment: &EmbodiedEnvironmentFrame,
    support_radius: f32,
) -> ObjectContactForce {
    let count = count.min(MAX_LIQUID_PARTICLES);
    let support_radius = support_radius.max(KERNEL_RADIUS).max(1.0e-4);
    let mut applied_object_force_world = ObjectContactForce::default();
    for contact in
        &environment.contacts[..environment.contact_count.min(environment.contacts.len())]
    {
        if !contact.point_world.is_finite()
            || !contact.normal_world.is_finite()
            || contact.normal_world.length_squared() <= 1.0e-8
        {
            continue;
        }
        let point_local =
            body_origin + (contact.point_world - body_world_position) * world_to_body_scale;
        let normal_local =
            (contact.normal_world * world_to_body_scale.signum()).normalize_or_zero();
        if contact.source == pet_ecology::ContactSource::Orb {
            // This is already reciprocal J / ecology_dt, not an impulse to
            // replay each PBF substep. Particle.force is local acceleration;
            // the ordinary solver integrates it using its own substep dt².
            if !contact.body_force_world.is_finite()
                || !world_to_body_scale.is_finite()
                || world_to_body_scale.abs().min_element() < 1.0e-4
            {
                continue;
            }
            let total_mass: f32 = particles[..count]
                .iter()
                .filter(|p| p.inverse_mass.is_finite() && p.inverse_mass > 0.0)
                .map(|p| 1.0 / p.inverse_mass)
                .sum();
            let mut weights = [0.0; MAX_LIQUID_PARTICLES];
            for (weight, p) in weights[..count].iter_mut().zip(&particles[..count]) {
                let u = (1.0 - p.position.distance(point_local) / support_radius).clamp(0.0, 1.0);
                *weight = u * u * (3.0 - 2.0 * u);
            }
            let weighted_mass: f32 = particles[..count]
                .iter()
                .zip(&weights)
                .filter(|(p, _)| p.inverse_mass.is_finite() && p.inverse_mass > 0.0)
                .map(|(p, w)| w / p.inverse_mass)
                .sum();
            if weighted_mass < 1.0e-6 || total_mass < 1.0e-6 {
                continue;
            }
            // The physical toy body has mass3, independently of tessellation.
            let local_force = contact.body_force_world * world_to_body_scale;
            for (particle, weight) in particles[..count].iter_mut().zip(weights) {
                if particle.inverse_mass <= 0.0 || !particle.inverse_mass.is_finite() {
                    continue;
                }
                let acceleration = (local_force * (weight * total_mass / (3.0 * weighted_mass)))
                    .clamp_length_max(36.0);
                particle.force += acceleration;
                let toy_particle_mass = 3.0 / (particle.inverse_mass * total_mass);
                let delivered = acceleration / world_to_body_scale * toy_particle_mass;
                applied_object_force_world.applied_world += delivered;
                // The ecology hold contact has ZERO relative velocity, even
                // for a heavy orb. Load alone must never become startle.
                if contact.relative_velocity_px.is_finite()
                    && contact.relative_velocity_px.length() > 10.0
                {
                    applied_object_force_world.impact_world += delivered;
                }
            }
            // No intensity/penetration heuristic stacked onto true orb mass.
            continue;
        }
        let impact = if contact.relative_velocity_px.is_finite() {
            (contact.relative_velocity_px.length() / 1_200.0).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let penetration = if contact.penetration_px.is_finite() {
            (contact.penetration_px / 96.0).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let intensity = if contact.intensity.is_finite() {
            contact.intensity.clamp(0.0, 1.0)
        } else {
            0.0
        };
        let acceleration = (intensity * 5.0 + impact * 4.0 + penetration * 3.0).clamp(0.0, 12.0);
        for particle in &mut particles[..count] {
            let distance = particle.position.distance(point_local);
            let influence = (1.0 - distance / support_radius).clamp(0.0, 1.0);
            if influence > 0.0 {
                particle.force += normal_local * acceleration * influence * influence;
            }
        }
    }
    applied_object_force_world
}

impl MaterialGrab {
    fn update_pointer(
        &mut self,
        held: bool,
        target_center: Vec2,
        body_origin: Vec2,
        parameters: GrabParameters,
        dt: f32,
    ) {
        let dt = dt.clamp(0.0, 0.10);
        // The captured material patch lives in the same local frame as the
        // filtered cursor, including a translated carrier origin.
        if self.initialized {
            let shift = body_origin - self.reference_origin;
            self.onset_center += shift;
            self.filtered_center += shift;
            self.target_center += shift;
            if self.patch_initialized {
                for position in &mut self.onset_positions[..self.captured_count] {
                    *position += shift;
                }
            }
        }
        self.reference_origin = body_origin;
        if held {
            // A new press begins exactly at the hit-tested visible material. Only
            // subsequent cursor samples need filtering; this avoids sweeping a
            // stale field through the body when a new gesture begins.
            if !self.initialized || !self.held {
                self.filtered_center = target_center;
                self.onset_center = target_center;
                self.patch_initialized = false;
                self.filtered_velocity = Vec2::ZERO;
                self.previous_filtered_velocity = Vec2::ZERO;
                self.filtered_acceleration = Vec2::ZERO;
                self.pressure_impulse = 0.0;
                self.initialized = true;
            }
            self.target_center = target_center;
        }
        self.held = held;

        if self.initialized && dt > 0.0 {
            self.previous_filtered_velocity = self.filtered_velocity;
            let support = parameters.support_radius.max(1.0e-4);
            let max_speed = support * parameters.max_speed_radii_per_second.max(0.0);
            let max_acceleration = support
                * parameters
                    .max_acceleration_radii_per_second_squared
                    .max(0.0);
            let (position, velocity) = step_critically_damped(
                self.filtered_center,
                self.filtered_velocity,
                self.target_center,
                parameters.response_hz.max(0.01),
                max_speed,
                max_acceleration,
                dt,
            );
            self.filtered_center = position;
            self.filtered_velocity = velocity;
            self.filtered_acceleration =
                (self.filtered_velocity - self.previous_filtered_velocity) / dt;
        }

        let target_amplitude = if held { 1.0 } else { 0.0 };
        let transition_seconds = if held {
            parameters.fade_in_seconds
        } else {
            parameters.fade_out_seconds
        };
        self.amplitude =
            exponential_transition(self.amplitude, target_amplitude, transition_seconds, dt);
        if held {
            self.pressure_impulse =
                (self.pressure_impulse + self.effective_pressure * dt).min(60.0);
        }
        if !held && self.amplitude < 1.0e-5 {
            self.amplitude = 0.0;
            self.filtered_velocity = Vec2::ZERO;
            self.filtered_acceleration = Vec2::ZERO;
        }

        let axis = self.filtered_center - body_origin;
        self.field_axis = axis.normalize_or_zero();
        self.cursor_distance = axis.length();
    }

    fn apply_field(
        &mut self,
        particles: &mut [LiquidParticle; MAX_LIQUID_PARTICLES],
        count: usize,
        fallback_center: Vec2,
        parameters: GrabParameters,
    ) {
        self.differential_forces.fill(Vec2::ZERO);
        self.field_coverage = 0.0;
        self.effective_area_fraction = 0.0;
        self.effective_pressure = 0.0;
        self.weighted_contact_center = fallback_center;
        self.weighted_material_velocity = Vec2::ZERO;
        if self.amplitude <= 0.0 || count == 0 {
            return;
        }

        let support = parameters.support_radius.max(1.0e-4);
        let peak_acceleration = parameters.peak_acceleration.max(0.0);
        if !self.patch_initialized || self.captured_count != count {
            // Indices are material identities while the locked solver count is
            // stable. A reconstruction/count change starts a fresh observation
            // rather than applying old offsets to different material.
            self.onset_center = self.filtered_center;
            self.captured_count = count;
            for (index, particle) in particles[..count].iter().enumerate() {
                self.onset_positions[index] = particle.position;
                self.onset_weights[index] =
                    wendland_c2_kernel(particle.position - self.onset_center, support);
            }
            self.patch_initialized = true;
        }
        let displacement = self.filtered_center - self.onset_center;
        // Recruitment follows actual pull distance, not holding time. A plain
        // press does not damp breathing or drive particles toward one point.
        let load = (displacement.length() / (support * 0.15)).clamp(0.0, 1.0);
        let recruitment = load * load * (3.0 - 2.0 * load);
        let stiffness = peak_acceleration / support;
        let mut captured_center = Vec2::ZERO;
        let mut captured_weight = 0.0;
        for (particle, weight) in particles[..count].iter().zip(&self.onset_weights) {
            captured_center += particle.position * *weight;
            captured_weight += *weight;
        }
        let patch_center = if captured_weight > 1.0e-5 {
            captured_center / captured_weight
        } else {
            self.onset_center
        };
        let mut weight_sum = 0.0;
        let mut support_center = Vec2::ZERO;
        let mut support_velocity = Vec2::ZERO;
        let mut support_weight_sum = 0.0;
        let mut support_weight_squared_sum = 0.0;
        for (index, particle) in particles[..count].iter_mut().enumerate() {
            // A captured grasp remains attached to this material neighborhood
            // while the virtual cursor runs ahead. Dropping locality around a
            // distant cursor would release it before a neck could form. No new
            // distant material can enter the onset-selected patch.
            let distance = patch_center.distance(particle.position);
            let edge = ((distance / support - 0.8) / 0.2).clamp(0.0, 1.0);
            let current_locality = 1.0 - edge * edge * (3.0 - 2.0 * edge);
            let support_weight = self.onset_weights[index] * current_locality;
            support_weight_sum += support_weight;
            support_weight_squared_sum += support_weight * support_weight;
            support_center += particle.position * support_weight;
            support_velocity += particle.velocity * support_weight;
            // Preserve onset offsets within the patch: the target is a local
            // translation, never a sink that compresses all mass to the cursor.
            // Forces are compliant; the density solver still owns all positions.
            let target = self.onset_positions[index] + displacement;
            // The compact weight sets material stiffness, not a reduced force
            // ceiling. A genuinely loaded surface patch can reach the existing
            // ceiling even when its onset sample is away from the kernel peak.
            let local_stiffness = stiffness * support_weight;
            let local_damping = 2.0 * local_stiffness.sqrt();
            let spring = (target - particle.position) * local_stiffness;
            let damper = (self.filtered_velocity - particle.velocity) * local_damping;
            let force = (spring + damper).clamp_length_max(peak_acceleration)
                * (recruitment * self.amplitude);
            self.differential_forces[index] = force;
            particle.force += force;
            weight_sum += force.length();
        }
        self.field_coverage = weight_sum / (count as f32 * peak_acceleration.max(1.0e-5));
        let effective_particle_count =
            support_weight_sum * support_weight_sum / support_weight_squared_sum.max(1.0e-5);
        self.effective_area_fraction =
            (effective_particle_count / count.max(1) as f32).clamp(0.0, 1.0);
        let force_density = weight_sum / effective_particle_count.max(1.0);
        self.effective_pressure = (force_density / peak_acceleration.max(1.0e-5)).clamp(0.0, 1.0);
        // Contact observations use the actual retained material neighborhood
        // even at zero mechanical work and with the virtual cursor farther away.
        if support_weight_sum > 1.0e-5 {
            self.weighted_contact_center = support_center / support_weight_sum;
            self.weighted_material_velocity = support_velocity / support_weight_sum;
        }
        self.field_axis = (self.filtered_center - self.weighted_contact_center).normalize_or_zero();
    }

    #[must_use]
    pub(super) fn material_stress(
        &self,
        particles: &[LiquidParticle; MAX_LIQUID_PARTICLES],
        count: usize,
        _main_component: u8,
        fallback_center: Vec2,
    ) -> MaterialStressFrame {
        let count = count.min(MAX_LIQUID_PARTICLES);
        if self.amplitude <= 0.0 || count == 0 {
            return MaterialStressFrame {
                center: fallback_center,
                cursor_distance: self.cursor_distance,
                com_follow_ratio: self.field_coverage,
                ..MaterialStressFrame::default()
            };
        }

        let mut center = Vec2::ZERO;
        let mut weight_sum = 0.0;
        for (index, particle) in particles[..count].iter().enumerate() {
            let force_weight = self.differential_forces[index].length();
            center += particle.position * force_weight;
            weight_sum += force_weight;
        }
        MaterialStressFrame {
            center: if weight_sum > 1.0e-5 {
                center / weight_sum
            } else {
                fallback_center
            },
            magnitude: self.field_coverage.clamp(0.0, 1.0),
            cursor_distance: self.cursor_distance,
            com_follow_ratio: self.field_coverage,
        }
    }

    #[must_use]
    pub(super) fn is_active(&self) -> bool {
        self.held || self.amplitude > 1.0e-3
    }

    #[must_use]
    pub(super) fn release_mobility(&self) -> f32 {
        if self.held {
            0.0
        } else {
            (1.0 - self.amplitude).clamp(0.0, 1.0)
        }
    }

    #[must_use]
    pub(super) fn drag_axis(&self) -> Vec2 {
        if self.is_active() {
            self.field_axis
        } else {
            Vec2::ZERO
        }
    }

    #[must_use]
    pub(super) fn readback(&self) -> MaterialGrabReadback {
        MaterialGrabReadback {
            active: self.is_active() && self.effective_area_fraction > 0.0,
            held: self.held,
            filtered_velocity: self.filtered_velocity,
            filtered_acceleration: self.filtered_acceleration,
            contact_center: self.weighted_contact_center,
            material_velocity: self.weighted_material_velocity,
            normal: self.field_axis,
            area_fraction: self.effective_area_fraction,
            effective_pressure: self.effective_pressure,
            pressure_impulse: self.pressure_impulse,
        }
    }
}

fn exponential_transition(current: f32, target: f32, duration: f32, dt: f32) -> f32 {
    if duration <= f32::EPSILON {
        return target;
    }
    // Three time constants reach 95.0% of the requested transition.
    let decay = (-3.0 * dt.max(0.0) / duration).exp();
    target + (current - target) * decay
}

fn step_critically_damped(
    position: Vec2,
    velocity: Vec2,
    target: Vec2,
    response_hz: f32,
    maximum_speed: f32,
    maximum_acceleration: f32,
    dt: f32,
) -> (Vec2, Vec2) {
    if dt <= 0.0 {
        return (position, velocity);
    }
    let omega = std::f32::consts::TAU * response_hz.max(0.01);
    let displacement = position - target;
    let helper = velocity + displacement * omega;
    let decay = (-omega * dt).exp();
    let analytic_position = target + (displacement + helper * dt) * decay;
    let analytic_velocity = (velocity - helper * (omega * dt)) * decay;

    let acceleration_limit = maximum_acceleration.max(0.0) * dt;
    let delta_velocity = (analytic_velocity - velocity).clamp_length_max(acceleration_limit);
    let limited_velocity = (velocity + delta_velocity).clamp_length_max(maximum_speed.max(0.0));
    let speed_was_limited = limited_velocity.distance(analytic_velocity) > 1.0e-6;
    let analytic_step = analytic_position - position;
    let step_was_limited = analytic_step.length() > maximum_speed.max(0.0) * dt + 1.0e-6;

    if !speed_was_limited && !step_was_limited {
        (analytic_position, analytic_velocity)
    } else {
        let step = ((velocity + limited_velocity) * (0.5 * dt))
            .clamp_length_max(maximum_speed.max(0.0) * dt);
        (position + step, limited_velocity)
    }
}

#[cfg(test)]
mod tests {
    use pet_ecology::{ContactSource, ExternalContact};

    use super::*;

    #[test]
    fn return_mobility_follows_actual_release_amplitude_and_resets_on_regrab() {
        let mut grab = MaterialGrab::default();
        let parameters = GrabParameters::default();
        for _ in 0..30 {
            grab.update_pointer(true, Vec2::ZERO, Vec2::ZERO, parameters, 1.0 / 120.0);
        }
        assert_eq!(grab.release_mobility(), 0.0);
        grab.update_pointer(false, Vec2::ZERO, Vec2::ZERO, parameters, 1.0 / 120.0);
        let first = grab.release_mobility();
        assert!(first > 0.0 && first < 1.0);
        grab.update_pointer(false, Vec2::ZERO, Vec2::ZERO, parameters, 1.0 / 120.0);
        assert!(grab.release_mobility() > first);
        grab.update_pointer(true, Vec2::ZERO, Vec2::ZERO, parameters, 1.0 / 120.0);
        assert_eq!(grab.release_mobility(), 0.0);
    }

    #[test]
    fn reciprocal_orb_force_is_mass_normalized_and_static_load_is_not_impact() {
        for count in [12, 48, 96] {
            let mut particles = [LiquidParticle::default(); MAX_LIQUID_PARTICLES];
            for p in &mut particles[..count] {
                p.inverse_mass = 1.0;
            }
            let mut environment = EmbodiedEnvironmentFrame::default();
            environment.push_contact(ExternalContact {
                source: ContactSource::Orb,
                point_world: Vec2::splat(0.5),
                normal_world: Vec2::Y,
                body_force_world: Vec2::Y * 0.6,
                ..Default::default()
            });
            let applied = apply_external_contact_forces(
                &mut particles,
                count,
                Vec2::ZERO,
                Vec2::splat(0.5),
                Vec2::new(2.0, -2.0),
                &environment,
                0.3,
            );
            assert!((applied.applied_world.y - 0.6).abs() < 1.0e-5);
            assert_eq!(applied.impact_world, Vec2::ZERO);
            for p in &particles[..count] {
                assert!((p.force.y + 0.4).abs() < 1.0e-5);
            }
        }
    }

    #[test]
    fn external_contact_is_local_bounded_and_does_not_push_distant_material() {
        let mut particles = [LiquidParticle::default(); MAX_LIQUID_PARTICLES];
        particles[0].position = Vec2::ZERO;
        particles[1].position = Vec2::new(0.8, 0.0);
        let mut environment = EmbodiedEnvironmentFrame::default();
        environment.push_contact(ExternalContact {
            source: ContactSource::Window,
            point_world: Vec2::splat(0.5),
            normal_world: Vec2::X,
            penetration_px: 24.0,
            relative_velocity_px: Vec2::new(-360.0, 0.0),
            intensity: 0.8,
            body_force_world: Vec2::ZERO,
        });
        apply_external_contact_forces(
            &mut particles,
            2,
            Vec2::ZERO,
            Vec2::splat(0.5),
            Vec2::ONE,
            &environment,
            0.24,
        );
        assert!(particles[0].force.x > 0.0);
        assert!(particles[0].force.length() <= 12.0 + 1.0e-5);
        assert_eq!(particles[1].force, Vec2::ZERO);
    }

    #[test]
    fn pointer_field_ignores_raw_pointer_velocity_and_component_labels() {
        let mut still = [LiquidParticle::default(); MAX_LIQUID_PARTICLES];
        still[0].position = Vec2::new(-0.05, 0.0);
        still[0].component_id = 0;
        still[1].position = Vec2::new(0.05, 0.0);
        still[1].component_id = 9;
        let mut moving = still;
        let sensor_frame = |velocity: f32| SensorFrame {
            cursor_position: Vec2::splat(0.5),
            cursor_velocity: Vec2::new(velocity, 0.0),
            pointer_down: true,
            pet_dragged: true,
            ..SensorFrame::default()
        };
        let mut still_grab = MaterialGrab::default();
        let mut moving_grab = MaterialGrab::default();
        for (particles, grab, velocity) in [
            (&mut still, &mut still_grab, 0.0),
            (&mut moving, &mut moving_grab, 50.0),
        ] {
            apply_interaction_forces(
                particles,
                2,
                grab,
                GrabParameters::default(),
                Vec2::ZERO,
                Vec2::splat(0.5),
                Vec2::ONE,
                None,
                &sensor_frame(velocity),
                &BodyFeedback::default(),
                0.0,
                0,
                Vec2::ZERO,
                1.0 / 120.0,
            );
        }
        assert_eq!(still[0].force, moving[0].force);
        assert_eq!(still[1].force, moving[1].force);
        assert_eq!(still[0].force, Vec2::ZERO);
        assert_eq!(still[1].force, Vec2::ZERO);
    }

    #[test]
    fn filtered_center_respects_speed_and_acceleration_limits() {
        let parameters = GrabParameters::default();
        let dt = 1.0 / 120.0;
        let mut grab = MaterialGrab::default();
        grab.update_pointer(true, Vec2::ZERO, Vec2::ZERO, parameters, dt);
        let previous_position = grab.filtered_center;
        let previous_velocity = grab.filtered_velocity;
        grab.update_pointer(true, Vec2::new(2.0, 0.0), Vec2::ZERO, parameters, dt);

        let max_speed = parameters.support_radius * parameters.max_speed_radii_per_second;
        let max_acceleration =
            parameters.support_radius * parameters.max_acceleration_radii_per_second_squared;
        assert!(grab.filtered_center.distance(previous_position) <= max_speed * dt + 1.0e-6);
        assert!(grab.filtered_velocity.length() <= max_speed + 1.0e-6);
        assert!(
            grab.filtered_velocity.distance(previous_velocity) <= max_acceleration * dt + 1.0e-6
        );
    }

    #[test]
    fn stationary_press_does_zero_mechanical_work_but_observes_contact() {
        let parameters = GrabParameters::default();
        let mut particles = [LiquidParticle::default(); MAX_LIQUID_PARTICLES];
        for (index, particle) in particles[..5].iter_mut().enumerate() {
            particle.position = Vec2::new(-0.08 + index as f32 * 0.04, 0.01);
            particle.velocity = Vec2::new(0.03, -0.02);
        }
        let mut grab = MaterialGrab::default();
        for _ in 0..1200 {
            grab.update_pointer(
                true,
                Vec2::new(0.04, 0.02),
                Vec2::ZERO,
                parameters,
                1.0 / 120.0,
            );
            grab.apply_field(&mut particles, 5, Vec2::ZERO, parameters);
            assert!(particles[..5].iter().all(|p| p.force == Vec2::ZERO));
            assert_eq!(grab.effective_pressure, 0.0);
        }
        let observation = grab.readback();
        assert!(observation.active && observation.held && observation.area_fraction > 0.0);
        assert!(observation.contact_center.is_finite());
        assert!(
            observation
                .material_velocity
                .distance(Vec2::new(0.03, -0.02))
                < 1.0e-6
        );
    }

    #[test]
    fn captured_grasp_keeps_local_tension_without_selecting_new_distant_material() {
        let parameters = GrabParameters::default();
        let mut particles = [LiquidParticle::default(); MAX_LIQUID_PARTICLES];
        for (index, particle) in particles[..7].iter_mut().enumerate() {
            particle.position = Vec2::new(-0.10 + index as f32 * 0.04, 0.02);
        }
        particles[7].position = Vec2::new(4.0, 0.0);
        let mut grab = MaterialGrab::default();
        grab.update_pointer(true, Vec2::ZERO, Vec2::ZERO, parameters, 1.0 / 120.0);
        grab.apply_field(&mut particles, 8, Vec2::ZERO, parameters);
        for _ in 0..60 {
            grab.update_pointer(
                true,
                Vec2::new(0.08, 0.0),
                Vec2::ZERO,
                parameters,
                1.0 / 120.0,
            );
        }
        grab.apply_field(&mut particles, 8, Vec2::ZERO, parameters);
        assert!(particles[..7].iter().any(|p| p.force.length() > 0.01));
        assert!(
            particles[..8].iter().all(|p| p.force.is_finite()
                && p.force.length() <= parameters.peak_acceleration + 1.0e-5)
        );
        assert_eq!(particles[7].force, Vec2::ZERO);
        for _ in 0..600 {
            grab.update_pointer(
                true,
                Vec2::new(2.0, 0.0),
                Vec2::ZERO,
                parameters,
                1.0 / 120.0,
            );
        }
        for particle in &mut particles[..8] {
            particle.force = Vec2::ZERO;
        }
        grab.apply_field(&mut particles, 8, Vec2::ZERO, parameters);
        assert!(particles[..7].iter().any(|p| p.force.length() > 0.01));
        assert!(
            particles[..8].iter().all(|p| p.force.is_finite()
                && p.force.length() <= parameters.peak_acceleration + 1.0e-5)
        );
        assert_eq!(particles[7].force, Vec2::ZERO);
        assert!(grab.effective_pressure > 0.01);
        let loaded_pressure = grab.effective_pressure;
        for _ in 0..120 {
            grab.update_pointer(
                false,
                Vec2::new(2.0, 0.0),
                Vec2::ZERO,
                parameters,
                1.0 / 120.0,
            );
            for particle in &mut particles[..8] {
                particle.force = Vec2::ZERO;
            }
            grab.apply_field(&mut particles, 8, Vec2::ZERO, parameters);
        }
        assert!(grab.effective_pressure < loaded_pressure * 0.001);
    }

    #[test]
    fn onset_patch_moves_with_its_local_origin_and_regrab_cancels_old_load() {
        let parameters = GrabParameters::default();
        let mut particles = [LiquidParticle::default(); MAX_LIQUID_PARTICLES];
        particles[0].position = Vec2::new(0.04, 0.02);
        let mut grab = MaterialGrab::default();
        grab.update_pointer(true, Vec2::ZERO, Vec2::ZERO, parameters, 1.0 / 120.0);
        let shift = Vec2::new(0.3, -0.2);
        particles[0].position += shift;
        grab.update_pointer(true, shift, shift, parameters, 1.0 / 120.0);
        grab.apply_field(&mut particles, 1, shift, parameters);
        assert_eq!(particles[0].force, Vec2::ZERO);
        grab.update_pointer(
            true,
            shift + Vec2::new(0.08, 0.0),
            shift,
            parameters,
            1.0 / 120.0,
        );
        grab.update_pointer(false, shift, shift, parameters, 1.0 / 120.0);
        assert!(grab.amplitude > 0.0);
        grab.update_pointer(
            true,
            shift + Vec2::new(-0.08, 0.0),
            shift,
            parameters,
            1.0 / 120.0,
        );
        particles[0].force = Vec2::ZERO;
        grab.apply_field(&mut particles, 1, shift, parameters);
        assert_eq!(particles[0].force, Vec2::ZERO);
        assert_eq!(grab.filtered_velocity, Vec2::ZERO);
    }

    #[test]
    fn entrained_patch_keeps_offsets_and_stops_loading_when_it_follows_the_cursor() {
        let parameters = GrabParameters::default();
        let mut particles = [LiquidParticle::default(); MAX_LIQUID_PARTICLES];
        for (index, particle) in particles[..5].iter_mut().enumerate() {
            particle.position = Vec2::new(-0.06 + index as f32 * 0.03, 0.01);
        }
        let mut grab = MaterialGrab::default();
        grab.update_pointer(true, Vec2::ZERO, Vec2::ZERO, parameters, 1.0 / 120.0);
        grab.apply_field(&mut particles, 5, Vec2::ZERO, parameters);
        let translation = Vec2::new(0.06, 0.0);
        for _ in 0..120 {
            grab.update_pointer(true, translation, Vec2::ZERO, parameters, 1.0 / 120.0);
        }
        grab.apply_field(&mut particles, 5, Vec2::ZERO, parameters);
        assert!(grab.effective_pressure > 0.01);
        // A correctly entrained patch has moved together without collapse.
        // Such positions are a unit fixture; native replay checks the solver.
        for particle in &mut particles[..5] {
            particle.position += translation;
            particle.velocity = grab.filtered_velocity;
            particle.force = Vec2::ZERO;
        }
        grab.apply_field(&mut particles, 5, Vec2::ZERO, parameters);
        assert!(grab.effective_pressure < 1.0e-6);
        assert!(particles[..5].iter().all(|p| p.force.length() < 1.0e-5));
    }

    #[test]
    fn amplitude_fades_in_and_out_without_a_release_impulse() {
        let parameters = GrabParameters::default();
        let dt = 1.0 / 120.0;
        let mut grab = MaterialGrab::default();
        grab.update_pointer(true, Vec2::ZERO, Vec2::ZERO, parameters, dt);
        let first = grab.amplitude;
        grab.update_pointer(true, Vec2::ZERO, Vec2::ZERO, parameters, dt);
        assert!(grab.amplitude > first && grab.amplitude < 1.0);

        grab.update_pointer(false, Vec2::ZERO, Vec2::ZERO, parameters, dt);
        let first_release = grab.amplitude;
        assert!(first_release > 0.0);
        for _ in 0..16 {
            grab.update_pointer(false, Vec2::ZERO, Vec2::ZERO, parameters, dt);
        }
        assert!(grab.amplitude < first_release * 0.01);
        assert!(!grab.held);
    }
}
