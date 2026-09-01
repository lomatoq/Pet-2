use glam::Vec2;
use lifecore::{
    BodyComponentObservation, BodyMaterialState, ComponentLifecycle, EmbodiedInteractionFrame,
    MAX_TRACKED_BODY_COMPONENTS, PointerMaterialContact,
};

use crate::InteractionTuning;

use super::{
    collisions::MaterialGrabReadback,
    components::ComponentSummary,
    particles::{LiquidParticle, MAX_LIQUID_PARTICLES},
};

const REFERENCE_BODY_RADIUS: f32 = 0.39;
const SLOSH_REFERENCE_SPEED: f32 = 0.80;

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(super) struct LiquidInteractionProbe {
    sequence: u64,
    timestamp: f64,
    contact_seconds: f32,
    pressure_impulse: f32,
    previous_deformation_energy: f32,
    filtered_pressure: f32,
    filtered_strain: f32,
    filtered_slosh: f32,
    latest: EmbodiedInteractionFrame,
}

impl LiquidInteractionProbe {
    #[must_use]
    pub(super) const fn sequence(&self) -> u64 {
        self.sequence
    }

    pub(super) fn restore_clock(&mut self, sequence: u64, timestamp: f64) {
        *self = Self::default();
        self.sequence = sequence;
        self.timestamp = if timestamp.is_finite() {
            timestamp.clamp(0.0, 3_600.0)
        } else {
            0.0
        };
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn update(
        &mut self,
        particles: &[LiquidParticle; MAX_LIQUID_PARTICLES],
        count: usize,
        components: ComponentSummary,
        grab: MaterialGrabReadback,
        body_origin: Vec2,
        body_world_position: Vec2,
        world_to_body_scale: Vec2,
        pointer_world: Vec2,
        maximum_strain: f32,
        density_error: f32,
        tuning: InteractionTuning,
        topology_attempt_rejected: bool,
        dt: f32,
    ) {
        let count = count.min(MAX_LIQUID_PARTICLES);
        let dt = if dt.is_finite() {
            dt.clamp(0.0, 0.05)
        } else {
            0.0
        };
        if dt <= 0.0 || count == 0 {
            return;
        }
        self.sequence = self.sequence.saturating_add(1);
        self.timestamp += f64::from(dt);

        let active = tuning.enabled
            && grab.held
            && grab.area_fraction >= tuning.contact_weight_floor.max(1.0e-5);
        if active {
            self.contact_seconds = (self.contact_seconds + dt).min(60.0);
        } else {
            self.contact_seconds = 0.0;
        }
        let smoothing = 1.0 - (-std::f32::consts::TAU * tuning.signal_smoothing_hz * dt).exp();
        self.filtered_pressure += (grab.effective_pressure - self.filtered_pressure) * smoothing;
        self.filtered_strain += (maximum_strain.max(0.0) - self.filtered_strain) * smoothing;

        let mut observations = [BodyComponentObservation::default(); MAX_TRACKED_BODY_COMPONENTS];
        let mut observation_count = 0_usize;
        let mut slosh = 0.0_f32;
        let mut deformation = 0.0_f32;
        let mut observed_mass = 0.0_f32;
        if components.component_count > 0 {
            let main = component_observation(
                particles,
                count,
                components.main_component,
                components.main_com,
                body_origin,
                body_world_position,
                world_to_body_scale,
                count,
                true,
            );
            slosh = slosh.max(
                (main.internal_speed / SLOSH_REFERENCE_SPEED)
                    .powi(2)
                    .clamp(0.0, 1.0),
            );
            deformation = deformation.max(main.observation.deformation);
            observed_mass += main.mass;
            observations[0] = main.observation;
            observation_count = 1;

            let mut next_id = 0_u16;
            while observation_count < MAX_TRACKED_BODY_COMPONENTS && next_id <= u8::MAX as u16 {
                let id = next_id as u8;
                next_id += 1;
                if id == components.main_component
                    || !particles[..count]
                        .iter()
                        .any(|particle| particle.component_id == id)
                {
                    continue;
                }
                let component = component_observation(
                    particles,
                    count,
                    id,
                    components.main_com,
                    body_origin,
                    body_world_position,
                    world_to_body_scale,
                    count,
                    false,
                );
                slosh = slosh.max(
                    (component.internal_speed / SLOSH_REFERENCE_SPEED)
                        .powi(2)
                        .clamp(0.0, 1.0),
                );
                deformation = deformation.max(component.observation.deformation);
                observed_mass += component.mass;
                observations[observation_count] = component.observation;
                observation_count += 1;
            }
        }
        self.filtered_slosh += (slosh - self.filtered_slosh) * smoothing;

        let total_mass = particles[..count]
            .iter()
            .map(|particle| particle.inverse_mass.max(1.0e-5).recip())
            .sum::<f32>();
        let detached_mass_fraction = components.detached_mass / total_mass.max(1.0e-5);
        let component_budget = f32::from(tuning.maximum_detached_components.max(1));
        let used_component_budget = components.component_count.saturating_sub(1) as f32;
        let mass_budget = tuning.maximum_detached_mass_fraction.max(1.0e-5);
        let topology_budget_remaining = (1.0
            - (used_component_budget / component_budget).max(detached_mass_fraction / mass_budget))
        .clamp(0.0, 1.0);
        let topology_budget_exhausted = topology_attempt_rejected
            || components.component_count > usize::from(tuning.maximum_detached_components) + 1
            || detached_mass_fraction > tuning.maximum_detached_mass_fraction + 1.0e-6;
        let maximum_strain = self.filtered_strain.max(0.0);
        let neck_tension = ((maximum_strain - tuning.stretch_strain_min)
            / tuning.boundary_strain.max(0.05))
        .clamp(0.0, 1.0);
        let neck_thickness = (1.0 - neck_tension).sqrt();
        let pressure_work = self.filtered_pressure
            * (grab.filtered_velocity - grab.material_velocity).length()
            / REFERENCE_BODY_RADIUS.clamp(0.0, 1.0);
        let normalized_deformation = deformation / (1.0 + deformation);
        let deformation_energy = (maximum_strain.powi(2) * 0.30
            + density_error.abs().min(1.0) * 0.15
            + self.filtered_slosh * 0.25
            + pressure_work * 0.15
            + normalized_deformation * 0.15)
            .clamp(0.0, 1.0);
        let deformation_rate =
            ((deformation_energy - self.previous_deformation_energy) / dt).clamp(-32.0, 32.0);
        self.previous_deformation_energy = deformation_energy;
        if active {
            self.pressure_impulse = (self.pressure_impulse + self.filtered_pressure * dt).min(60.0);
        } else if !grab.active {
            self.pressure_impulse = 0.0;
        }

        let velocity_scale = REFERENCE_BODY_RADIUS.recip();
        let mut frame = EmbodiedInteractionFrame {
            sequence: self.sequence,
            timestamp: self.timestamp,
            contact: PointerMaterialContact {
                active,
                point_local: grab.contact_center - components.main_com,
                point_world: pointer_world,
                normal_local: grab.normal,
                area_fraction: grab.area_fraction,
                effective_pressure: self.filtered_pressure,
                pointer_speed: grab.filtered_velocity.length() * velocity_scale,
                pointer_acceleration: grab.filtered_acceleration.length() * velocity_scale,
                pointer_velocity_local: grab.filtered_velocity * velocity_scale,
                pointer_acceleration_local: grab.filtered_acceleration * velocity_scale,
                material_velocity_local: grab.material_velocity * velocity_scale,
                relative_velocity_local: (grab.filtered_velocity - grab.material_velocity)
                    * velocity_scale,
                contact_seconds: self.contact_seconds,
                pressure_impulse: self.pressure_impulse.max(grab.pressure_impulse),
            },
            material: BodyMaterialState {
                deformation_energy,
                deformation_rate,
                maximum_strain,
                neck_tension,
                neck_thickness,
                slosh_energy: self.filtered_slosh,
                internal_relative_speed: slosh.sqrt() * SLOSH_REFERENCE_SPEED,
                detached_mass_fraction,
                component_count: components.component_count.min(MAX_TRACKED_BODY_COMPONENTS) as u8,
                mass_conservation_error: ((total_mass - observed_mass).abs()
                    / total_mass.max(1.0e-5))
                .min(1.0),
                topology_budget_remaining,
                topology_budget_exhausted,
            },
            components: observations,
            component_observation_count: observation_count as u8,
            detached_event: None,
            remerge_event: None,
            recovery_event: None,
        };
        // If more than four components already exist, the frame remains bounded
        // while exposing a failed hard invariant instead of leaking arrays.
        if components.component_count > MAX_TRACKED_BODY_COMPONENTS {
            frame.material.mass_conservation_error = 1.0;
        }
        frame.sanitize();
        self.latest = frame;
    }

    #[must_use]
    pub(super) fn latest(&self) -> EmbodiedInteractionFrame {
        self.latest
    }

    pub(super) fn latest_mut(&mut self) -> &mut EmbodiedInteractionFrame {
        &mut self.latest
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct ComponentMetrics {
    observation: BodyComponentObservation,
    mass: f32,
    internal_speed: f32,
}

#[allow(clippy::too_many_arguments)]
fn component_observation(
    particles: &[LiquidParticle; MAX_LIQUID_PARTICLES],
    count: usize,
    id: u8,
    main_com: Vec2,
    body_origin: Vec2,
    body_world_position: Vec2,
    world_to_body_scale: Vec2,
    total_count: usize,
    main: bool,
) -> ComponentMetrics {
    let mut particle_count = 0_usize;
    let mut mass = 0.0_f32;
    let mut center = Vec2::ZERO;
    let mut velocity = Vec2::ZERO;
    for particle in &particles[..count] {
        if particle.component_id != id {
            continue;
        }
        let particle_mass = particle.inverse_mass.max(1.0e-5).recip();
        particle_count += 1;
        mass += particle_mass;
        center += particle.position * particle_mass;
        velocity += particle.velocity * particle_mass;
    }
    center /= mass.max(1.0e-5);
    velocity /= mass.max(1.0e-5);

    let mut covariance_xx = 0.0_f32;
    let mut covariance_xy = 0.0_f32;
    let mut covariance_yy = 0.0_f32;
    let mut angular_numerator = 0.0_f32;
    let mut angular_denominator = 0.0_f32;
    for particle in &particles[..count] {
        if particle.component_id != id {
            continue;
        }
        let particle_mass = particle.inverse_mass.max(1.0e-5).recip();
        let arm = particle.position - center;
        let relative_velocity = particle.velocity - velocity;
        covariance_xx += arm.x * arm.x * particle_mass;
        covariance_xy += arm.x * arm.y * particle_mass;
        covariance_yy += arm.y * arm.y * particle_mass;
        angular_numerator += arm.perp_dot(relative_velocity) * particle_mass;
        angular_denominator += arm.length_squared() * particle_mass;
    }
    covariance_xx /= mass.max(1.0e-5);
    covariance_xy /= mass.max(1.0e-5);
    covariance_yy /= mass.max(1.0e-5);
    let angular_velocity = angular_numerator / angular_denominator.max(1.0e-5);
    let mut internal_energy = 0.0_f32;
    for particle in &particles[..count] {
        if particle.component_id != id {
            continue;
        }
        let particle_mass = particle.inverse_mass.max(1.0e-5).recip();
        let arm = particle.position - center;
        let rigid_velocity = velocity + Vec2::new(-arm.y, arm.x) * angular_velocity;
        internal_energy += particle.velocity.distance_squared(rigid_velocity) * particle_mass;
    }
    let internal_speed = (internal_energy / mass.max(1.0e-5)).sqrt();
    let trace = covariance_xx + covariance_yy;
    let discriminant =
        ((covariance_xx - covariance_yy).powi(2) + 4.0 * covariance_xy.powi(2)).sqrt();
    let lambda_max = ((trace + discriminant) * 0.5).max(0.0);
    let lambda_min = ((trace - discriminant) * 0.5).max(1.0e-5);
    let deformation = ((lambda_max / lambda_min).sqrt() - 1.0).clamp(0.0, 8.0);
    let center_world = body_world_position
        + Vec2::new(
            safe_divide(center.x - body_origin.x, world_to_body_scale.x),
            safe_divide(center.y - body_origin.y, world_to_body_scale.y),
        );
    ComponentMetrics {
        observation: BodyComponentObservation {
            component_id: id,
            lifecycle: if main {
                ComponentLifecycle::Attached
            } else {
                ComponentLifecycle::Detached
            },
            particle_count: particle_count.min(u8::MAX as usize) as u8,
            mass_fraction: mass / total_count.max(1) as f32,
            center_local: center - main_com,
            center_world,
            velocity_local: velocity / REFERENCE_BODY_RADIUS,
            angular_velocity,
            deformation,
            distance_to_main: center.distance(main_com) / REFERENCE_BODY_RADIUS,
            ..BodyComponentObservation::default()
        },
        mass,
        internal_speed,
    }
}

fn safe_divide(numerator: f32, denominator: f32) -> f32 {
    if numerator.is_finite() && denominator.is_finite() && denominator.abs() > 1.0e-6 {
        numerator / denominator
    } else {
        0.0
    }
}
