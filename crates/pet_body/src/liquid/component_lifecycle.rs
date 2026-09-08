use glam::Vec2;
use lifecore::{
    BodyComponentObservation, ComponentDetachReason, ComponentLifecycle, EmbodiedInteractionFrame,
    MAX_TRACKED_BODY_COMPONENTS,
};

use crate::InteractionTuning;

use super::{
    body_snapshot::{SavedComponentLifecycle, SavedTrackedComponent},
    components::ComponentSummary,
    particles::{LiquidParticle, MAX_LIQUID_PARTICLES},
    topology_guard::HARD_MAX_FRAGMENT_LIFETIME_SECONDS,
};

#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct TrackedComponent {
    id: u8,
    lifecycle: ComponentLifecycle,
    reason: ComponentDetachReason,
    age: f32,
    offscreen_seconds: f32,
    center: Vec2,
    velocity: Vec2,
    particle_count: u8,
    mass_fraction: f32,
    recovery_seconds: f32,
    active: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(super) struct ComponentLifecycleTracker {
    tracked: [TrackedComponent; MAX_TRACKED_BODY_COMPONENTS - 1],
    main_lifecycle: ComponentLifecycle,
    necking_seconds: f32,
    detached_event: Option<u8>,
    remerge_event: Option<u8>,
    recovery_event: Option<u8>,
    recovered_ghost: Option<TrackedComponent>,
}

impl ComponentLifecycleTracker {
    pub(super) fn snapshot(&self) -> SavedComponentLifecycle {
        SavedComponentLifecycle {
            main_lifecycle: self.main_lifecycle,
            necking_seconds: self.necking_seconds,
            tracked: self
                .tracked
                .iter()
                .filter(|tracked| tracked.active)
                .map(|tracked| SavedTrackedComponent {
                    id: tracked.id,
                    lifecycle: tracked.lifecycle,
                    reason: tracked.reason,
                    age: tracked.age,
                    offscreen_seconds: tracked.offscreen_seconds,
                    center: tracked.center,
                    velocity: tracked.velocity,
                    particle_count: tracked.particle_count,
                    mass_fraction: tracked.mass_fraction,
                    recovery_seconds: tracked.recovery_seconds,
                })
                .collect(),
        }
    }

    pub(super) fn restore(&mut self, saved: &SavedComponentLifecycle) {
        *self = Self::default();
        self.main_lifecycle = saved.main_lifecycle;
        self.necking_seconds = saved.necking_seconds;
        for (slot, tracked) in self.tracked.iter_mut().zip(&saved.tracked) {
            *slot = TrackedComponent {
                id: tracked.id,
                lifecycle: tracked.lifecycle,
                reason: tracked.reason,
                age: tracked.age,
                offscreen_seconds: tracked.offscreen_seconds,
                center: tracked.center,
                velocity: tracked.velocity,
                particle_count: tracked.particle_count,
                mass_fraction: tracked.mass_fraction,
                recovery_seconds: tracked.recovery_seconds,
                active: true,
            };
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn update(
        &mut self,
        particles: &[LiquidParticle; MAX_LIQUID_PARTICLES],
        count: usize,
        components: ComponentSummary,
        contact_active: bool,
        maximum_strain: f32,
        local_containment_bounds: Option<(Vec2, Vec2)>,
        tuning: InteractionTuning,
        dt: f32,
    ) {
        self.detached_event = None;
        self.remerge_event = None;
        self.recovery_event = None;
        self.recovered_ghost = None;
        let count = count.min(MAX_LIQUID_PARTICLES);
        let dt = dt.clamp(0.0, 0.05);

        if components.component_count <= 1 {
            self.necking_seconds = if maximum_strain >= tuning.stretch_strain_min {
                (self.necking_seconds + dt).min(tuning.split_hold_seconds * 2.0)
            } else {
                (self.necking_seconds - dt * 2.0).max(0.0)
            };
            self.main_lifecycle = if self.necking_seconds >= tuning.split_hold_seconds {
                ComponentLifecycle::Necking
            } else {
                ComponentLifecycle::Attached
            };
        } else {
            self.necking_seconds = 0.0;
            self.main_lifecycle = ComponentLifecycle::Attached;
        }

        let total_mass = particles[..count]
            .iter()
            .map(|particle| particle.inverse_mass.max(1.0e-5).recip())
            .sum::<f32>()
            .max(1.0e-5);
        let main_velocity = component_velocity(particles, count, components.main_component);
        let previous = self.tracked;
        let mut seen = [false; MAX_TRACKED_BODY_COMPONENTS - 1];

        for id_value in 0..=u8::MAX {
            let id = id_value;
            if id == components.main_component
                || !particles[..count]
                    .iter()
                    .any(|particle| particle.component_id == id)
            {
                continue;
            }
            let Some(slot) = self
                .tracked
                .iter()
                .position(|tracked| tracked.active && tracked.id == id)
                .or_else(|| self.tracked.iter().position(|tracked| !tracked.active))
            else {
                break;
            };
            seen[slot] = true;
            let metrics = component_metrics(particles, count, id, total_mass);
            let is_new = !previous[slot].active || previous[slot].id != id;
            let tracked = &mut self.tracked[slot];
            if is_new {
                *tracked = TrackedComponent {
                    id,
                    lifecycle: ComponentLifecycle::Detached,
                    reason: if contact_active {
                        ComponentDetachReason::PointerStrain
                    } else {
                        ComponentDetachReason::BodyInertia
                    },
                    active: true,
                    ..TrackedComponent::default()
                };
                self.detached_event.get_or_insert(id);
            }
            tracked.age = (tracked.age + dt).min(3_600.0);
            tracked.center = metrics.center;
            tracked.velocity = metrics.velocity;
            tracked.particle_count = metrics.particle_count;
            tracked.mass_fraction = metrics.mass_fraction;
            let outside = local_containment_bounds.map_or(
                metrics.center.distance(components.main_com) > 1.75,
                |(minimum, maximum)| {
                    metrics.center.x < minimum.x
                        || metrics.center.y < minimum.y
                        || metrics.center.x > maximum.x
                        || metrics.center.y > maximum.y
                },
            );
            tracked.offscreen_seconds = if outside {
                (tracked.offscreen_seconds + dt).min(60.0)
            } else {
                0.0
            };
            let relative_center = metrics.center - components.main_com;
            let relative_velocity = metrics.velocity - main_velocity;
            let returning = relative_center.dot(relative_velocity) < -0.01;
            let merging = relative_center.length() < 0.24;
            tracked.lifecycle = if merging {
                ComponentLifecycle::Merging
            } else if tracked.age
                >= tuning
                    .fragment_lifetime_seconds
                    .min(HARD_MAX_FRAGMENT_LIFETIME_SECONDS)
                || tracked.offscreen_seconds >= tuning.offscreen_recovery_delay_seconds
            {
                if previous[slot].lifecycle != ComponentLifecycle::DeterministicRecovery {
                    self.recovery_event.get_or_insert(id);
                }
                ComponentLifecycle::DeterministicRecovery
            } else if returning {
                ComponentLifecycle::Returning
            } else {
                ComponentLifecycle::Detached
            };
            tracked.recovery_seconds =
                if tracked.lifecycle == ComponentLifecycle::DeterministicRecovery {
                    (tracked.recovery_seconds + dt).min(2.0)
                } else {
                    (tracked.recovery_seconds - dt * 2.0).max(0.0)
                };
        }

        for (slot, was_seen) in seen.iter().copied().enumerate() {
            if self.tracked[slot].active && !was_seen {
                let mut recovered = self.tracked[slot];
                recovered.lifecycle = ComponentLifecycle::Recovered;
                self.remerge_event.get_or_insert(recovered.id);
                if self.tracked[slot].lifecycle == ComponentLifecycle::DeterministicRecovery {
                    self.recovery_event.get_or_insert(recovered.id);
                }
                self.recovered_ghost = Some(recovered);
                self.tracked[slot] = TrackedComponent::default();
            }
        }
    }

    pub(super) fn apply_recovery_field(
        &self,
        particles: &mut [LiquidParticle; MAX_LIQUID_PARTICLES],
        count: usize,
        main_com: Vec2,
        local_containment_bounds: Option<(Vec2, Vec2)>,
        tuning: InteractionTuning,
    ) {
        for tracked in self
            .tracked
            .iter()
            .filter(|tracked| tracked.active && tracked.recovery_seconds > 0.0)
        {
            let recovery_phase = (tracked.recovery_seconds / 0.45).clamp(0.0, 1.0);
            let smooth_envelope = recovery_phase * recovery_phase * (3.0 - 2.0 * recovery_phase);
            let field_multiplier =
                1.0 + (tuning.recovery_field_boost.clamp(1.0, 2.0) - 1.0) * smooth_envelope;
            let safe_target = local_containment_bounds.map_or(main_com, |(minimum, maximum)| {
                let margin = Vec2::splat(0.08).min((maximum - minimum).max(Vec2::ZERO) * 0.20);
                let safe_minimum = minimum + margin;
                let safe_maximum = maximum - margin;
                if tracked.center.cmpge(minimum).all() && tracked.center.cmple(maximum).all() {
                    main_com.clamp(safe_minimum, safe_maximum)
                } else {
                    tracked.center.clamp(safe_minimum, safe_maximum)
                }
            });
            let delta = safe_target - tracked.center;
            let acceleration = delta.normalize_or_zero() * (4.0 * field_multiplier).clamp(4.0, 8.0);
            for particle in particles[..count.min(MAX_LIQUID_PARTICLES)]
                .iter_mut()
                .filter(|particle| particle.component_id == tracked.id)
            {
                particle.force += acceleration;
            }
        }
    }

    pub(super) fn decorate(&self, frame: &mut EmbodiedInteractionFrame) {
        let main_component_id = frame.components[0].component_id;
        for observation in &mut frame.components[..usize::from(frame.component_observation_count)] {
            if observation.component_id == main_component_id {
                observation.lifecycle = self.main_lifecycle;
                continue;
            }
            if let Some(tracked) = self
                .tracked
                .iter()
                .find(|tracked| tracked.active && tracked.id == observation.component_id)
            {
                apply_tracking(observation, *tracked);
            }
        }
        if let Some(ghost) = self.recovered_ghost {
            let index = usize::from(frame.component_observation_count);
            if index < MAX_TRACKED_BODY_COMPONENTS {
                frame.components[index] = BodyComponentObservation {
                    component_id: ghost.id,
                    lifecycle: ComponentLifecycle::Recovered,
                    detach_reason: ghost.reason,
                    particle_count: 0,
                    mass_fraction: 0.0,
                    center_local: ghost.center - frame.components[0].center_local,
                    velocity_local: ghost.velocity,
                    age_seconds: ghost.age,
                    offscreen_seconds: ghost.offscreen_seconds,
                    ..BodyComponentObservation::default()
                };
                frame.component_observation_count += 1;
            }
        }
        frame.detached_event = self.detached_event;
        frame.remerge_event = self.remerge_event;
        frame.recovery_event = self.recovery_event;
        frame.sanitize();
    }

    #[must_use]
    pub(super) fn is_stable(&self) -> bool {
        self.tracked.iter().all(|tracked| !tracked.active)
            && self.main_lifecycle != ComponentLifecycle::Necking
    }
}

fn apply_tracking(observation: &mut BodyComponentObservation, tracked: TrackedComponent) {
    observation.lifecycle = tracked.lifecycle;
    observation.detach_reason = tracked.reason;
    observation.age_seconds = tracked.age;
    observation.offscreen_seconds = tracked.offscreen_seconds;
}

#[derive(Debug, Clone, Copy, Default)]
struct ComponentMetrics {
    center: Vec2,
    velocity: Vec2,
    particle_count: u8,
    mass_fraction: f32,
}

fn component_metrics(
    particles: &[LiquidParticle; MAX_LIQUID_PARTICLES],
    count: usize,
    id: u8,
    total_mass: f32,
) -> ComponentMetrics {
    let mut center = Vec2::ZERO;
    let mut velocity = Vec2::ZERO;
    let mut mass = 0.0_f32;
    let mut particle_count = 0_usize;
    for particle in &particles[..count] {
        if particle.component_id != id {
            continue;
        }
        let particle_mass = particle.inverse_mass.max(1.0e-5).recip();
        center += particle.position * particle_mass;
        velocity += particle.velocity * particle_mass;
        mass += particle_mass;
        particle_count += 1;
    }
    ComponentMetrics {
        center: center / mass.max(1.0e-5),
        velocity: velocity / mass.max(1.0e-5),
        particle_count: particle_count.min(u8::MAX as usize) as u8,
        mass_fraction: mass / total_mass,
    }
}

fn component_velocity(
    particles: &[LiquidParticle; MAX_LIQUID_PARTICLES],
    count: usize,
    id: u8,
) -> Vec2 {
    let mut velocity = Vec2::ZERO;
    let mut mass = 0.0_f32;
    for particle in &particles[..count] {
        if particle.component_id == id {
            let particle_mass = particle.inverse_mass.max(1.0e-5).recip();
            velocity += particle.velocity * particle_mass;
            mass += particle_mass;
        }
    }
    velocity / mass.max(1.0e-5)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn component_id_survives_motion_and_retires_after_remerge() {
        let mut particles = [LiquidParticle::default(); MAX_LIQUID_PARTICLES];
        for particle in &mut particles[..4] {
            particle.inverse_mass = 1.0;
        }
        particles[0].component_id = 0;
        particles[1].component_id = 0;
        particles[2].component_id = 1;
        particles[3].component_id = 1;
        particles[2].position = Vec2::new(0.5, 0.0);
        particles[3].position = Vec2::new(0.55, 0.0);
        let mut tracker = ComponentLifecycleTracker::default();
        let split = ComponentSummary {
            component_count: 2,
            main_component: 0,
            main_mass: 2.0,
            detached_mass: 2.0,
            main_com: Vec2::ZERO,
        };
        tracker.update(
            &particles,
            4,
            split,
            true,
            0.8,
            None,
            InteractionTuning::default(),
            1.0 / 120.0,
        );
        assert_eq!(tracker.detached_event, Some(1));
        tracker.update(
            &particles,
            4,
            split,
            true,
            0.8,
            None,
            InteractionTuning::default(),
            1.0 / 120.0,
        );
        assert_eq!(tracker.detached_event, None);

        for particle in &mut particles[..4] {
            particle.component_id = 0;
        }
        tracker.update(
            &particles,
            4,
            ComponentSummary {
                component_count: 1,
                main_component: 0,
                main_mass: 4.0,
                detached_mass: 0.0,
                main_com: Vec2::ZERO,
            },
            false,
            0.0,
            None,
            InteractionTuning::default(),
            1.0 / 120.0,
        );
        assert_eq!(tracker.remerge_event, Some(1));
    }

    #[test]
    fn offscreen_component_recovers_deterministically() {
        fn replay() -> (SavedComponentLifecycle, bool) {
            let mut particles = [LiquidParticle::default(); MAX_LIQUID_PARTICLES];
            for particle in &mut particles[..8] {
                particle.inverse_mass = 1.0;
            }
            for (index, particle) in particles[..4].iter_mut().enumerate() {
                particle.component_id = 0;
                particle.position = Vec2::new(index as f32 * 0.04, 0.0);
            }
            for (index, particle) in particles[4..8].iter_mut().enumerate() {
                particle.component_id = 1;
                particle.position = Vec2::new(2.0 + index as f32 * 0.04, 0.0);
            }
            let components = ComponentSummary {
                component_count: 2,
                main_component: 0,
                main_mass: 4.0,
                detached_mass: 4.0,
                main_com: Vec2::new(0.06, 0.0),
            };
            let tuning = InteractionTuning {
                offscreen_recovery_delay_seconds: 0.25,
                ..InteractionTuning::default()
            };
            let mut tracker = ComponentLifecycleTracker::default();
            let mut saw_recovery = false;
            for _ in 0..32 {
                tracker.update(
                    &particles,
                    8,
                    components,
                    false,
                    0.0,
                    Some((Vec2::splat(-1.0), Vec2::splat(1.0))),
                    tuning,
                    1.0 / 120.0,
                );
                saw_recovery |= tracker.recovery_event == Some(1);
            }
            (tracker.snapshot(), saw_recovery)
        }

        let first = replay();
        let second = replay();
        assert_eq!(first, second);
        assert!(first.1);
        assert_eq!(
            first.0.tracked[0].lifecycle,
            ComponentLifecycle::DeterministicRecovery
        );
        assert!(first.0.tracked[0].recovery_seconds > 0.0);
    }
}
