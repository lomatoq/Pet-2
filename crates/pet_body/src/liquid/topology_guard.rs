use glam::Vec2;

use crate::InteractionTuning;

use super::particles::{LiquidParticle, MAX_LIQUID_PARTICLES};

pub const HARD_MAX_DETACHED_COMPONENTS: usize = 3;
pub const HARD_MAX_TRACKED_COMPONENTS: usize = 4;
pub const HARD_MAX_DETACHED_MASS_FRACTION: f32 = 0.25;
pub const HARD_MIN_FRAGMENT_PARTICLES: usize = 3;
pub const HARD_MAX_FRAGMENT_LIFETIME_SECONDS: f32 = 15.0;

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(super) struct TopologyDecision {
    pub predicted_component_count: usize,
    pub detached_mass_fraction: f32,
    pub minimum_fragment_particles: usize,
    pub budget_exhausted: bool,
    pub correction_count: u8,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(super) struct TopologyGuard {
    latest: TopologyDecision,
}

impl TopologyGuard {
    pub(super) fn observe(
        &mut self,
        particles: &[LiquidParticle; MAX_LIQUID_PARTICLES],
        count: usize,
        link_distance: f32,
        tuning: InteractionTuning,
    ) -> TopologyDecision {
        let count = count.min(MAX_LIQUID_PARTICLES);
        if count == 0 {
            self.latest = TopologyDecision::default();
            return self.latest;
        }
        let prediction = predict(particles, count, link_distance);
        let maximum_detached_components =
            usize::from(tuning.maximum_detached_components).clamp(1, HARD_MAX_DETACHED_COMPONENTS);
        let maximum_detached_mass_fraction = tuning
            .maximum_detached_mass_fraction
            .clamp(0.05, HARD_MAX_DETACHED_MASS_FRACTION);
        let minimum_fragment_particles =
            usize::from(tuning.minimum_fragment_particles).clamp(HARD_MIN_FRAGMENT_PARTICLES, 12);
        self.latest = TopologyDecision {
            predicted_component_count: prediction.component_count,
            detached_mass_fraction: prediction.detached_mass_fraction,
            minimum_fragment_particles: prediction.minimum_detached_particles,
            budget_exhausted: prediction.component_count > maximum_detached_components + 1
                || prediction.detached_mass_fraction > maximum_detached_mass_fraction + 1.0e-6
                || (prediction.component_count > 1
                    && prediction.minimum_detached_particles < minimum_fragment_particles),
            correction_count: 0,
        };
        self.latest
    }

    pub(super) fn enforce(
        &mut self,
        particles: &mut [LiquidParticle; MAX_LIQUID_PARTICLES],
        count: usize,
        link_distance: f32,
        tuning: InteractionTuning,
    ) -> TopologyDecision {
        let count = count.min(MAX_LIQUID_PARTICLES);
        if count == 0 {
            self.latest = TopologyDecision::default();
            return self.latest;
        }
        let mut prediction = predict(particles, count, link_distance);
        let maximum_detached_components =
            usize::from(tuning.maximum_detached_components).clamp(1, HARD_MAX_DETACHED_COMPONENTS);
        let maximum_detached_mass_fraction = tuning
            .maximum_detached_mass_fraction
            .clamp(0.05, HARD_MAX_DETACHED_MASS_FRACTION);
        let minimum_fragment_particles =
            usize::from(tuning.minimum_fragment_particles).clamp(HARD_MIN_FRAGMENT_PARTICLES, 12);
        let maximum_total_components =
            (maximum_detached_components + 1).min(HARD_MAX_TRACKED_COMPONENTS);
        let invalid = prediction.component_count > maximum_total_components
            || prediction.detached_mass_fraction > maximum_detached_mass_fraction + 1.0e-6
            || (prediction.component_count > 1
                && prediction.minimum_detached_particles < minimum_fragment_particles);
        let mut correction_count = 0_u8;
        if invalid {
            // Reject the invalid predicted transition at the constraint boundary.
            // `position` is the last accepted physical state, but rolling every
            // particle back to it also destroys the velocity of the unaffected
            // body when velocity is reconstructed after constraints. Repeated
            // rejection under a held pointer then looks like a total-body
            // freeze. Keep the largest predicted continuation of each already
            // accepted component and roll back only its newly split branches.
            // This preserves ordinary rigid/deformation motion and the motion of
            // previously accepted detached components while still making an
            // oversized or undersized new split non-authoritative.
            let accepted = predict_current_positions(particles, count, link_distance);
            correction_count = reject_new_split_branches(particles, count, &accepted, &prediction);
            prediction = predict(particles, count, link_distance);

            // A migrated/snapshot state may already violate the new invariant.
            // Repair that legacy state with bounded constraint projections while
            // deterministic recovery supplies the longer-range return field.
            for _ in 0..2 {
                if prediction.component_count <= maximum_total_components
                    && prediction.detached_mass_fraction <= maximum_detached_mass_fraction + 1.0e-6
                    && (prediction.component_count == 1
                        || prediction.minimum_detached_particles >= minimum_fragment_particles)
                {
                    break;
                }
                correction_count = correction_count.saturating_add(constrain_to_main(
                    particles,
                    count,
                    &prediction,
                    link_distance,
                ));
                prediction = predict(particles, count, link_distance);
            }
        }
        self.latest = TopologyDecision {
            predicted_component_count: prediction.component_count,
            detached_mass_fraction: prediction.detached_mass_fraction,
            minimum_fragment_particles: prediction.minimum_detached_particles,
            budget_exhausted: invalid,
            correction_count,
        };
        self.latest
    }
}

fn reject_new_split_branches(
    particles: &mut [LiquidParticle; MAX_LIQUID_PARTICLES],
    count: usize,
    accepted: &Prediction,
    predicted: &Prediction,
) -> u8 {
    let mut corrections = 0_u8;
    for accepted_group in 0..accepted.component_count {
        let mut overlap = [0_u8; MAX_LIQUID_PARTICLES];
        for index in 0..count {
            if usize::from(accepted.labels[index]) == accepted_group {
                let predicted_group = usize::from(predicted.labels[index]);
                overlap[predicted_group] = overlap[predicted_group].saturating_add(1);
            }
        }
        // A tie is intentionally resolved by the stable prediction label. The
        // result is deterministic and, unlike face-carrier ownership, cannot
        // select a tiny pinched patch as the motion-authoritative side.
        let continuation = overlap[..predicted.component_count]
            .iter()
            .copied()
            .enumerate()
            .max_by(|(left_group, left_count), (right_group, right_count)| {
                left_count
                    .cmp(right_count)
                    .then_with(|| right_group.cmp(left_group))
            })
            .map_or(0, |(group, _)| group as u8);
        for index in 0..count {
            if usize::from(accepted.labels[index]) == accepted_group
                && predicted.labels[index] != continuation
                && particles[index].predicted_position != particles[index].position
            {
                particles[index].predicted_position = particles[index].position;
                corrections = corrections.saturating_add(1);
            }
        }
    }
    corrections
}

#[derive(Debug, Clone, Copy)]
struct Prediction {
    labels: [u8; MAX_LIQUID_PARTICLES],
    component_count: usize,
    main_group: u8,
    detached_mass_fraction: f32,
    minimum_detached_particles: usize,
}

fn predict(
    particles: &[LiquidParticle; MAX_LIQUID_PARTICLES],
    count: usize,
    link_distance: f32,
) -> Prediction {
    predict_with_positions(particles, count, link_distance, |particle| {
        particle.predicted_position
    })
}

fn predict_current_positions(
    particles: &[LiquidParticle; MAX_LIQUID_PARTICLES],
    count: usize,
    link_distance: f32,
) -> Prediction {
    predict_with_positions(particles, count, link_distance, |particle| {
        particle.position
    })
}

fn predict_with_positions(
    particles: &[LiquidParticle; MAX_LIQUID_PARTICLES],
    count: usize,
    link_distance: f32,
    position_of: impl Fn(&LiquidParticle) -> Vec2,
) -> Prediction {
    let mut labels = [u8::MAX; MAX_LIQUID_PARTICLES];
    let mut sizes = [0_usize; MAX_LIQUID_PARTICLES];
    let mut masses = [0.0_f32; MAX_LIQUID_PARTICLES];
    let mut component_count = 0_usize;
    let threshold_squared = link_distance.max(1.0e-5).powi(2);
    for seed in 0..count {
        if labels[seed] != u8::MAX {
            continue;
        }
        let group = component_count as u8;
        component_count += 1;
        let mut queue = [0_usize; MAX_LIQUID_PARTICLES];
        let mut read = 0_usize;
        let mut write = 1_usize;
        queue[0] = seed;
        labels[seed] = group;
        while read < write {
            let current = queue[read];
            read += 1;
            sizes[usize::from(group)] += 1;
            masses[usize::from(group)] += particles[current].inverse_mass.max(1.0e-5).recip();
            for other in 0..count {
                if labels[other] != u8::MAX
                    || position_of(&particles[current])
                        .distance_squared(position_of(&particles[other]))
                        >= threshold_squared
                {
                    continue;
                }
                labels[other] = group;
                queue[write] = other;
                write += 1;
            }
        }
    }
    let face_carrier = (1..count).fold(0_usize, |best, index| {
        if particles[index].face_weight > particles[best].face_weight {
            index
        } else {
            best
        }
    });
    let main_group = labels[face_carrier];
    let total_mass = masses[..component_count].iter().sum::<f32>();
    let detached_mass = masses[..component_count]
        .iter()
        .enumerate()
        .filter(|(group, _)| *group != usize::from(main_group))
        .map(|(_, mass)| mass)
        .sum::<f32>();
    let minimum_detached_particles = sizes[..component_count]
        .iter()
        .enumerate()
        .filter(|(group, _)| *group != usize::from(main_group))
        .map(|(_, size)| *size)
        .min()
        .unwrap_or(count);
    Prediction {
        labels,
        component_count,
        main_group,
        detached_mass_fraction: detached_mass / total_mass.max(1.0e-5),
        minimum_detached_particles,
    }
}

fn constrain_to_main(
    particles: &mut [LiquidParticle; MAX_LIQUID_PARTICLES],
    count: usize,
    prediction: &Prediction,
    link_distance: f32,
) -> u8 {
    let mut corrections = 0_u8;
    for group in 0..prediction.component_count {
        if group == usize::from(prediction.main_group) {
            continue;
        }
        let mut nearest = None;
        let mut nearest_distance_squared = f32::INFINITY;
        for detached in 0..count {
            if usize::from(prediction.labels[detached]) != group {
                continue;
            }
            for main in 0..count {
                if prediction.labels[main] != prediction.main_group {
                    continue;
                }
                let distance_squared = particles[detached]
                    .predicted_position
                    .distance_squared(particles[main].predicted_position);
                if distance_squared < nearest_distance_squared {
                    nearest_distance_squared = distance_squared;
                    nearest = Some((detached, main));
                }
            }
        }
        let Some((detached, main)) = nearest else {
            continue;
        };
        let delta = particles[detached].predicted_position - particles[main].predicted_position;
        let distance = delta.length();
        if distance <= 1.0e-6 {
            continue;
        }
        let excess = (distance - link_distance * 0.90).max(0.0);
        let correction = delta / distance * (excess * 0.5).min(0.018);
        particles[detached].predicted_position -= correction;
        particles[main].predicted_position += correction;
        corrections = corrections.saturating_add(1);
    }
    corrections
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn topology_budget_never_deletes_or_spawns_particles() {
        let mut particles = [LiquidParticle::default(); MAX_LIQUID_PARTICLES];
        for (index, particle) in particles[..8].iter_mut().enumerate() {
            particle.inverse_mass = 1.0;
            particle.face_weight = if index == 0 { 1.0 } else { 0.0 };
            particle.predicted_position = Vec2::new(index as f32 * 0.4, 0.0);
        }
        let before = particles[..8]
            .iter()
            .map(|particle| particle.inverse_mass.recip())
            .sum::<f32>();
        let mut guard = TopologyGuard::default();
        let decision = guard.enforce(&mut particles, 8, 0.10, InteractionTuning::default());
        let after = particles[..8]
            .iter()
            .map(|particle| particle.inverse_mass.recip())
            .sum::<f32>();
        assert_eq!(before, after);
        assert!(decision.budget_exhausted);
        assert!(decision.correction_count > 0);
    }

    #[test]
    fn topology_budget_prevents_a_fourth_detached_component() {
        let mut particles = [LiquidParticle::default(); MAX_LIQUID_PARTICLES];
        let count = 32;
        for (index, particle) in particles[..count].iter_mut().enumerate() {
            particle.inverse_mass = 1.0;
            particle.face_weight = if index == 0 { 1.0 } else { 0.0 };
            // The accepted state is one connected strand. The prediction asks
            // for one main body plus four four-particle fragments.
            particle.position = Vec2::new(index as f32 * 0.04, 0.0);
            particle.predicted_position = if index < 16 {
                Vec2::new(index as f32 * 0.04, 0.0)
            } else {
                let fragment = (index - 16) / 4;
                let within = (index - 16) % 4;
                Vec2::new(2.0 + fragment as f32 * 0.5 + within as f32 * 0.04, 0.0)
            };
        }
        let tuning = InteractionTuning {
            maximum_detached_components: 3,
            maximum_detached_mass_fraction: HARD_MAX_DETACHED_MASS_FRACTION,
            minimum_fragment_particles: 4,
            ..InteractionTuning::default()
        };
        let mut guard = TopologyGuard::default();

        let predicted = guard.observe(&particles, count, 0.10, tuning);
        assert!(predicted.predicted_component_count > 4);
        let accepted = guard.enforce(&mut particles, count, 0.10, tuning);

        assert!(accepted.budget_exhausted);
        assert!(accepted.predicted_component_count <= HARD_MAX_TRACKED_COMPONENTS);
        assert!(
            particles[..count]
                .iter()
                .all(|particle| particle.predicted_position == particle.position)
        );
    }

    #[test]
    fn minimum_fragment_size_is_enforced_before_break() {
        let mut particles = [LiquidParticle::default(); MAX_LIQUID_PARTICLES];
        for (index, particle) in particles[..6].iter_mut().enumerate() {
            particle.inverse_mass = 1.0;
            particle.face_weight = if index == 0 { 1.0 } else { 0.0 };
            particle.predicted_position = if index < 5 {
                Vec2::new(index as f32 * 0.05, 0.0)
            } else {
                Vec2::new(0.32, 0.0)
            };
        }
        let mut guard = TopologyGuard::default();
        let decision = guard.enforce(&mut particles, 6, 0.10, InteractionTuning::default());
        assert!(decision.budget_exhausted);
        assert!(decision.correction_count > 0);
    }

    #[test]
    fn rejected_tiny_pinch_preserves_unrelated_body_motion() {
        for face_carrier in [0, 7] {
            let mut particles = [LiquidParticle::default(); MAX_LIQUID_PARTICLES];
            let translation = Vec2::new(0.012, 0.006);
            for (index, particle) in particles[..8].iter_mut().enumerate() {
                particle.inverse_mass = 1.0;
                particle.face_weight = if index == face_carrier { 1.0 } else { 0.0 };
                particle.position = Vec2::new(index as f32 * 0.04, 0.0);
                particle.predicted_position = particle.position + translation;
            }
            // The pointer attempts to pull one particle out as an illegal
            // singleton. This used to roll all eight predictions back and made
            // the complete body reconstruct exactly zero velocity every tick.
            particles[7].predicted_position = Vec2::new(0.72, 0.0);
            let accepted_main_predictions = particles[..7]
                .iter()
                .map(|particle| particle.predicted_position)
                .collect::<Vec<_>>();
            let mut guard = TopologyGuard::default();

            let decision = guard.enforce(&mut particles, 8, 0.10, InteractionTuning::default());

            assert!(decision.budget_exhausted);
            assert_eq!(decision.predicted_component_count, 1);
            assert_eq!(decision.correction_count, 1);
            for (particle, expected) in particles[..7].iter().zip(accepted_main_predictions) {
                assert_eq!(particle.predicted_position, expected);
                assert_ne!(particle.predicted_position, particle.position);
            }
            assert_eq!(particles[7].predicted_position, particles[7].position);
        }
    }

    #[test]
    fn rejected_pinch_does_not_freeze_an_existing_detached_component() {
        let mut particles = [LiquidParticle::default(); MAX_LIQUID_PARTICLES];
        let main_translation = Vec2::new(0.012, 0.004);
        let detached_translation = Vec2::new(-0.006, 0.009);
        for (index, particle) in particles[..24].iter_mut().enumerate() {
            particle.inverse_mass = 1.0;
            particle.face_weight = if index == 0 { 1.0 } else { 0.0 };
            particle.position = if index < 20 {
                Vec2::new(index as f32 * 0.04, 0.0)
            } else {
                Vec2::new(1.0 + (index - 20) as f32 * 0.04, 0.0)
            };
            particle.predicted_position = particle.position
                + if index < 20 {
                    main_translation
                } else {
                    detached_translation
                };
        }
        // A second, illegal one-particle split is attempted from the main
        // component while a valid four-particle fragment is already moving.
        particles[19].predicted_position = Vec2::new(2.0, 0.0);
        let expected_main = particles[..19]
            .iter()
            .map(|particle| particle.predicted_position)
            .collect::<Vec<_>>();
        let expected_detached = particles[20..24]
            .iter()
            .map(|particle| particle.predicted_position)
            .collect::<Vec<_>>();
        let mut guard = TopologyGuard::default();

        let decision = guard.enforce(&mut particles, 24, 0.10, InteractionTuning::default());

        assert!(decision.budget_exhausted);
        assert_eq!(decision.predicted_component_count, 2);
        assert_eq!(decision.correction_count, 1);
        for (particle, expected) in particles[..19].iter().zip(expected_main) {
            assert_eq!(particle.predicted_position, expected);
        }
        assert_eq!(particles[19].predicted_position, particles[19].position);
        for (particle, expected) in particles[20..24].iter().zip(expected_detached) {
            assert_eq!(particle.predicted_position, expected);
            assert_ne!(particle.predicted_position, particle.position);
        }
    }
}
