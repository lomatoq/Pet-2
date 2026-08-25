use glam::Vec2;
use lifecore::{BodyFeedback, SensorFrame};

use super::{
    kernels::{wendland_c2_gradient, wendland_c2_kernel},
    particles::{KERNEL_RADIUS, LiquidParticle, MAX_LIQUID_PARTICLES},
};

#[derive(Debug, Clone, PartialEq)]
pub struct MaterialGrab {
    held: bool,
    initialized: bool,
    target_center: Vec2,
    filtered_center: Vec2,
    filtered_velocity: Vec2,
    amplitude: f32,
    field_axis: Vec2,
    differential_forces: [Vec2; MAX_LIQUID_PARTICLES],
    cursor_distance: f32,
    field_coverage: f32,
}

impl Default for MaterialGrab {
    fn default() -> Self {
        Self {
            held: false,
            initialized: false,
            target_center: Vec2::ZERO,
            filtered_center: Vec2::ZERO,
            filtered_velocity: Vec2::ZERO,
            amplitude: 0.0,
            field_axis: Vec2::ZERO,
            differential_forces: [Vec2::ZERO; MAX_LIQUID_PARTICLES],
            cursor_distance: 0.0,
            field_coverage: 0.0,
        }
    }
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
    /// Compact support of the pointer potential in body-local units.
    pub support_radius: f32,
    /// Maximum acceleration anywhere in the Wendland gradient.
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
        if held {
            // A new press begins exactly at the hit-tested visible material. Only
            // subsequent cursor samples need filtering; this avoids sweeping a
            // stale field through the body when a new gesture begins.
            if !self.initialized || (!self.held && self.amplitude <= 1.0e-4) {
                self.filtered_center = target_center;
                self.filtered_velocity = Vec2::ZERO;
                self.initialized = true;
            }
            self.target_center = target_center;
        }
        self.held = held;

        if self.initialized && dt > 0.0 {
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
        }

        let target_amplitude = if held { 1.0 } else { 0.0 };
        let transition_seconds = if held {
            parameters.fade_in_seconds
        } else {
            parameters.fade_out_seconds
        };
        self.amplitude =
            exponential_transition(self.amplitude, target_amplitude, transition_seconds, dt);
        if !held && self.amplitude < 1.0e-5 {
            self.amplitude = 0.0;
            self.filtered_velocity = Vec2::ZERO;
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
        if self.amplitude <= 0.0 || count == 0 {
            return;
        }

        let support = parameters.support_radius.max(1.0e-4);
        let peak_acceleration = parameters.peak_acceleration.max(0.0);
        let mut weighted_center = Vec2::ZERO;
        let mut weight_sum = 0.0;
        for (index, particle) in particles[..count].iter_mut().enumerate() {
            let force = pointer_gradient(
                self.filtered_center - particle.position,
                support,
                peak_acceleration,
            ) * self.amplitude;
            self.differential_forces[index] = force;
            particle.force += force;
            let weight = force.length();
            weighted_center += particle.position * weight;
            weight_sum += weight;
        }
        self.field_coverage = weight_sum / (count as f32 * peak_acceleration.max(1.0e-5));
        if weight_sum <= 1.0e-5 {
            self.field_axis = (self.filtered_center - fallback_center).normalize_or_zero();
        } else {
            self.field_axis =
                (self.filtered_center - weighted_center / weight_sum).normalize_or_zero();
        }
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
    pub(super) fn drag_axis(&self) -> Vec2 {
        if self.is_active() {
            self.field_axis
        } else {
            Vec2::ZERO
        }
    }
}

fn pointer_gradient(delta: Vec2, support_radius: f32, peak_acceleration: f32) -> Vec2 {
    let support_radius = support_radius.max(1.0e-5);
    if wendland_c2_kernel(delta, support_radius) <= 0.0 {
        return Vec2::ZERO;
    }
    // |dW/dx| reaches (540/256)/h at q=1/4. Normalize to give the strength
    // control the exact meaning "peak acceleration".
    const WENDLAND_GRADIENT_PEAK: f32 = 540.0 / 256.0;
    -wendland_c2_gradient(delta, support_radius)
        * (peak_acceleration.max(0.0) * support_radius / WENDLAND_GRADIENT_PEAK)
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
    use super::*;

    #[test]
    fn wendland_pointer_gradient_is_compact_smooth_and_peak_calibrated() {
        let support = 0.24;
        assert_eq!(pointer_gradient(Vec2::ZERO, support, 6.0), Vec2::ZERO);
        assert_eq!(
            pointer_gradient(Vec2::new(support, 0.0), support, 6.0),
            Vec2::ZERO
        );
        let peak = pointer_gradient(Vec2::new(support * 0.25, 0.0), support, 6.0);
        assert!((peak.length() - 6.0).abs() < 1.0e-5);
        assert!(peak.x > 0.0);
        assert_eq!(
            peak,
            -pointer_gradient(Vec2::new(-support * 0.25, 0.0), support, 6.0)
        );
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
        assert!(still[0].force.x > 0.0);
        assert!(still[1].force.x < 0.0);
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
