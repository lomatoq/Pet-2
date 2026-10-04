use glam::Vec2;

use crate::InteractionTuning;

use super::components::pair_link_distance_squared;
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

    #[cfg(test)]
    pub(super) fn enforce(
        &mut self,
        particles: &mut [LiquidParticle; MAX_LIQUID_PARTICLES],
        count: usize,
        link_distance: f32,
        tuning: InteractionTuning,
    ) -> TopologyDecision {
        self.enforce_with_budding(particles, count, link_distance, tuning, false)
    }

    pub(super) fn enforce_with_budding(
        &mut self,
        particles: &mut [LiquidParticle; MAX_LIQUID_PARTICLES],
        count: usize,
        link_distance: f32,
        tuning: InteractionTuning,
        allow_intentional_bud: bool,
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
            let legacy_invalid = accepted.component_count > maximum_total_components
                || accepted.detached_mass_fraction > maximum_detached_mass_fraction + 1.0e-6
                || (accepted.component_count > 1
                    && accepted.minimum_detached_particles < minimum_fragment_particles);
            correction_count = reject_new_split_branches(particles, count, &accepted, &prediction);
            prediction = predict(particles, count, link_distance);

            // A migrated/snapshot state may already violate the new invariant.
            // Repair that legacy state with bounded constraint projections while
            // deterministic recovery supplies the longer-range return field.
            for iteration in 0..24 {
                if allow_intentional_bud && iteration >= 2 {
                    break;
                }
                if legacy_invalid && iteration >= 2 {
                    break;
                }
                if prediction.component_count <= maximum_total_components
                    && prediction.detached_mass_fraction <= maximum_detached_mass_fraction + 1.0e-6
                    && (prediction.component_count == 1
                        || prediction.minimum_detached_particles >= minimum_fragment_particles)
                {
                    break;
                }
                if !legacy_invalid {
                    correction_count = correction_count.saturating_add(if allow_intentional_bud {
                        project_intentional_bud(particles, count, &prediction, link_distance)
                    } else {
                        project_new_transition(particles, count, &prediction, link_distance)
                    });
                    prediction = predict(particles, count, link_distance);
                    continue;
                }
                let repair_graph = predict_with_positions(
                    particles,
                    count,
                    link_distance,
                    |p| p.predicted_position,
                    true,
                );
                correction_count = correction_count.saturating_add(constrain_to_main(
                    particles,
                    count,
                    &repair_graph,
                    link_distance,
                ));
                prediction = predict(particles, count, link_distance);
            }
            let valid = |p: &Prediction| {
                p.component_count <= maximum_total_components
                    && p.detached_mass_fraction <= maximum_detached_mass_fraction + 1.0e-6
                    && (p.component_count == 1
                        || p.minimum_detached_particles >= minimum_fragment_particles)
            };
            if !legacy_invalid && !valid(&prediction) {
                let candidate = *particles;
                let mut motion = [Vec2::ZERO; MAX_LIQUID_PARTICLES];
                let mut mass = [0.0_f32; MAX_LIQUID_PARTICLES];
                let mut pinned = [false; MAX_LIQUID_PARTICLES];
                for (i, p) in candidate[..count].iter().enumerate() {
                    let group = usize::from(accepted.labels[i]);
                    let m = p.inverse_mass.max(1.0e-5).recip();
                    motion[group] += (p.predicted_position - p.position) * m;
                    mass[group] += m;
                    pinned[group] |= p.inverse_mass <= 0.0;
                }
                for group in 0..accepted.component_count {
                    motion[group] = if pinned[group] {
                        Vec2::ZERO
                    } else {
                        motion[group] / mass[group].max(1.0e-5)
                    };
                }
                // Admit the largest sampled valid deformation around conserved
                // component COM transport. At alpha=0 every accepted edge is
                // rigidly unchanged; Jensen's inequality bounds kinetic energy.
                // This preserves translation instead of freezing the whole body.
                for sample in (0..12).rev() {
                    let alpha = sample as f32 / 12.0;
                    for (i, p) in particles[..count].iter_mut().enumerate() {
                        let mean = motion[usize::from(accepted.labels[i])];
                        let relative =
                            candidate[i].predicted_position - candidate[i].position - mean;
                        p.predicted_position = if p.inverse_mass <= 0.0 {
                            p.position
                        } else {
                            p.position + mean + relative * alpha
                        };
                    }
                    prediction = predict(particles, count, link_distance);
                    if valid(&prediction) {
                        break;
                    }
                }
                if !valid(&prediction) {
                    // A rigid floating-point translation can move an exact
                    // threshold edge by an ulp. Check representable transport
                    // too; never commit an unverified invalid alpha=0 sample.
                    for sample in (0..12).rev() {
                        let beta = sample as f32 / 12.0;
                        for (i, p) in particles[..count].iter_mut().enumerate() {
                            p.predicted_position =
                                p.position + motion[usize::from(accepted.labels[i])] * beta;
                        }
                        prediction = predict(particles, count, link_distance);
                        if valid(&prediction) {
                            break;
                        }
                    }
                }
                correction_count = correction_count.saturating_add(1);
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
        for (index, particle) in particles.iter_mut().enumerate().take(count) {
            if usize::from(accepted.labels[index]) == accepted_group
                && predicted.labels[index] != continuation
                && particle.predicted_position != particle.position
            {
                particle.predicted_position = particle.position;
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
    predict_with_positions(
        particles,
        count,
        link_distance,
        |particle| particle.predicted_position,
        false,
    )
}

fn predict_current_positions(
    particles: &[LiquidParticle; MAX_LIQUID_PARTICLES],
    count: usize,
    link_distance: f32,
) -> Prediction {
    predict_with_positions(
        particles,
        count,
        link_distance,
        |particle| particle.position,
        false,
    )
}

fn predict_with_positions(
    particles: &[LiquidParticle; MAX_LIQUID_PARTICLES],
    count: usize,
    link_distance: f32,
    position_of: impl Fn(&LiquidParticle) -> Vec2,
    include_current_edges: bool,
) -> Prediction {
    let mut labels = [u8::MAX; MAX_LIQUID_PARTICLES];
    let mut sizes = [0_usize; MAX_LIQUID_PARTICLES];
    let mut masses = [0.0_f32; MAX_LIQUID_PARTICLES];
    let mut component_count = 0_usize;
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
                let threshold_squared = pair_link_distance_squared(
                    &particles[current],
                    &particles[other],
                    link_distance,
                );
                if labels[other] != u8::MAX
                    || (position_of(&particles[current])
                        .distance_squared(position_of(&particles[other]))
                        >= threshold_squared
                        && (!include_current_edges
                            || particles[current]
                                .position
                                .distance_squared(particles[other].position)
                                >= threshold_squared))
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
    // Repair an already invalid configuration, rather than applying a new
    // motor impulse. Select all pairs from one immutable prediction and move
    // whole connected components: pulling just their nearest particles can
    // break another neck inside the main body while "repairing" a fragment.
    let mut pairs = [(0_usize, 0_usize); MAX_LIQUID_PARTICLES];
    let mut pair_count = 0;
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
        pairs[pair_count] = (detached, main);
        pair_count += 1;
    }
    let mut masses = [0.0_f32; MAX_LIQUID_PARTICLES];
    let mut pinned = [false; MAX_LIQUID_PARTICLES];
    for (index, particle) in particles[..count].iter().enumerate() {
        let group = usize::from(prediction.labels[index]);
        if particle.inverse_mass > 0.0 {
            masses[group] += particle.inverse_mass.recip();
        } else {
            pinned[group] = true;
        }
    }
    let mut corrections = [Vec2::ZERO; MAX_LIQUID_PARTICLES];
    let mut correction_count = 0_u8;
    for &(detached, main) in &pairs[..pair_count] {
        let delta = particles[detached].predicted_position - particles[main].predicted_position;
        let distance = delta.length();
        if distance <= 1.0e-6 {
            continue;
        }
        let bridge_distance =
            pair_link_distance_squared(&particles[detached], &particles[main], link_distance)
                .sqrt();
        let excess = (distance - bridge_distance * 0.90).max(0.0);
        let detached_group = usize::from(prediction.labels[detached]);
        let main_group = usize::from(prediction.main_group);
        let group_weight = |group: usize| {
            if pinned[group] {
                0.0
            } else {
                masses[group].max(1.0e-6).recip()
            }
        };
        let detached_weight = group_weight(detached_group);
        let main_weight = group_weight(main_group);
        let weight_sum = detached_weight + main_weight;
        if excess <= 0.0 || weight_sum <= 1.0e-6 {
            continue;
        }
        // One normalization bounds the sum of the shared main translations.
        // Component inverse masses preserve the mass center; rigid translation
        // preserves every internal graph edge, so repair cannot split an
        // admitted component. The bound is the old equal-mass pair closure.
        let correction = delta / distance * excess.min(0.036) / pair_count.max(1) as f32;
        corrections[detached_group] -= correction * (detached_weight / weight_sum);
        corrections[main_group] += correction * (main_weight / weight_sum);
        correction_count = correction_count.saturating_add(1);
    }
    for (index, particle) in particles[..count].iter_mut().enumerate() {
        let correction = corrections[usize::from(prediction.labels[index])];
        // Pre-stabilize both endpoints of the time difference. Correcting only
        // the prediction manufactures kinetic energy when velocity is derived
        // as (prediction - position) / dt; repeating it then tears more necks.
        particle.position += correction;
        particle.previous_position += correction;
        particle.predicted_position += correction;
    }
    correction_count
}

// Preserve the authored cooperative-pull neck response. This active material
// mode is explicitly requested by measured interaction, never by anger/panic.
// It still passes the same checked admissibility gate before a split can commit.
fn project_intentional_bud(
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
        let threshold =
            pair_link_distance_squared(&particles[detached], &particles[main], link_distance)
                .sqrt();
        let excess = (distance - threshold * 0.90).max(0.0);
        let correction = delta / distance * (excess * 0.5).min(0.018);
        particles[detached].predicted_position -= correction;
        particles[main].predicted_position += correction;
        corrections = corrections.saturating_add(1);
    }
    corrections
}

// A newly rejected transition from a valid state has stored kinetic motion,
// not a legacy positional error. Project the actual broken accepted edges and
// remove their separating motion; never pull an arbitrary already closing pair
// toward a shorter, invented rest length. Each inverse-mass pair projection is
// dissipative in reconstructed velocity (closure <= separating displacement).
fn project_new_transition(
    particles: &mut [LiquidParticle; MAX_LIQUID_PARTICLES],
    count: usize,
    _prediction: &Prediction,
    link_distance: f32,
) -> u8 {
    let mut corrections = 0_u8;
    for first in 0..count {
        for second in first + 1..count {
            let link_squared =
                pair_link_distance_squared(&particles[first], &particles[second], link_distance);
            if particles[first]
                .position
                .distance_squared(particles[second].position)
                >= link_squared
            {
                continue;
            }
            let delta = particles[first].predicted_position - particles[second].predicted_position;
            let distance = delta.length();
            let edge_distance = link_squared.sqrt();
            if distance < edge_distance || distance <= 1.0e-6 {
                continue;
            }
            let normal = delta / distance;
            let separating_motion = (particles[first].predicted_position
                - particles[first].position
                - (particles[second].predicted_position - particles[second].position))
                .dot(normal)
                .max(0.0);
            let first_weight = particles[first].inverse_mass.max(0.0);
            let second_weight = particles[second].inverse_mass.max(0.0);
            let weight_sum = first_weight + second_weight;
            if weight_sum <= 1.0e-6 {
                continue;
            }
            let closure = (distance - edge_distance * (1.0 - 5.0e-6))
                .max(0.0)
                .min(separating_motion);
            if closure <= 0.0 {
                continue;
            }
            let correction = normal * closure / weight_sum;
            particles[first].predicted_position -= correction * first_weight;
            particles[second].predicted_position += correction * second_weight;
            corrections = corrections.saturating_add(1);
        }
    }
    corrections
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guard_and_committed_components_share_exact_split_join_decisions() {
        for (same_id, gap, expected) in [(true, 0.26, 1), (false, 0.22, 1), (false, 0.24, 2)] {
            let mut particles = [LiquidParticle::default(); MAX_LIQUID_PARTICLES];
            for (i, p) in particles[..8].iter_mut().enumerate() {
                p.inverse_mass = 1.0;
                p.face_weight = if i == 0 { 1.0 } else { 0.0 };
                p.component_id = if i < 4 || same_id { 0 } else { 1 };
                p.position = Vec2::new(
                    if i < 4 {
                        i as f32 * 0.01
                    } else {
                        0.03 + gap + (i - 4) as f32 * 0.01
                    },
                    0.0,
                );
                p.predicted_position = p.position;
            }
            let guard = predict(&particles, 8, 0.25);
            let committed =
                super::super::components::assign_components(&mut particles, 8, 0.25 / 1.3, 1.3);
            assert_eq!(guard.component_count, expected);
            assert_eq!(guard.component_count, committed.component_count);
            assert!((guard.detached_mass_fraction - committed.detached_mass / 8.0).abs() < 1.0e-6);
        }
    }

    #[test]
    fn new_transition_edge_projection_cannot_increase_kinetic_energy() {
        let mut particles = [LiquidParticle::default(); MAX_LIQUID_PARTICLES];
        for (i, p) in particles[..3].iter_mut().enumerate() {
            p.inverse_mass = if i == 0 { 0.5 } else { 1.0 };
            p.position = Vec2::new(i as f32 * 0.09, 0.0);
            p.predicted_position = p.position + Vec2::new((i as f32 - 1.0) * 0.06, 0.01);
        }
        let energy = |ps: &[LiquidParticle]| {
            ps.iter()
                .map(|p| (p.predicted_position - p.position).length_squared() / p.inverse_mass)
                .sum::<f32>()
        };
        let momentum = |ps: &[LiquidParticle]| {
            ps.iter()
                .map(|p| (p.predicted_position - p.position) / p.inverse_mass)
                .sum::<Vec2>()
        };
        let before = energy(&particles[..3]);
        let before_momentum = momentum(&particles[..3]);
        let prediction = predict(&particles, 3, 0.1);
        assert!(project_new_transition(&mut particles, 3, &prediction, 0.1) > 0);
        assert!(energy(&particles[..3]) <= before + 1.0e-6);
        assert!(momentum(&particles[..3]).distance(before_momentum) < 1.0e-6);
    }

    #[test]
    fn repair_union_preserves_necks_in_both_time_configurations() {
        let mut particles = [LiquidParticle::default(); MAX_LIQUID_PARTICLES];
        for (index, particle) in particles[..6].iter_mut().enumerate() {
            particle.inverse_mass = 1.0;
            particle.face_weight = if index == 0 { 1.0 } else { 0.0 };
            particle.position = Vec2::new(
                if index < 4 {
                    index as f32 * 0.04
                } else {
                    0.9 + (index - 4) as f32 * 0.04
                },
                0.0,
            );
            particle.predicted_position = particle.position
                + if (2..4).contains(&index) {
                    Vec2::new(0.22, 0.0)
                } else {
                    Vec2::ZERO
                };
            particle.previous_position = particle.position;
        }
        let before = particles;
        let old_graph = predict_current_positions(&particles, 6, 0.1);
        let predicted_graph = predict(&particles, 6, 0.1);
        assert_eq!(old_graph.component_count, 2);
        assert_eq!(predicted_graph.component_count, 3);
        let repair_graph =
            predict_with_positions(&particles, 6, 0.1, |p| p.predicted_position, true);
        assert_eq!(repair_graph.component_count, 2);
        assert_eq!(constrain_to_main(&mut particles, 6, &repair_graph, 0.1), 1);
        for i in 0..6 {
            for j in 0..6 {
                if before[i].position.distance(before[j].position) < 0.1 {
                    assert!(particles[i].position.distance(particles[j].position) < 0.1);
                }
                if before[i]
                    .predicted_position
                    .distance(before[j].predicted_position)
                    < 0.1
                {
                    assert!(
                        particles[i]
                            .predicted_position
                            .distance(particles[j].predicted_position)
                            < 0.1
                    );
                }
            }
            assert!(
                (particles[i].predicted_position - particles[i].position)
                    .distance(before[i].predicted_position - before[i].position)
                    < 1.0e-6
            );
        }
    }

    #[test]
    fn shared_endpoint_repair_preserves_velocity_and_mass_center() {
        let mut particles = [LiquidParticle::default(); MAX_LIQUID_PARTICLES];
        // Four separated fragments share their nearest main endpoint. This is
        // the actual remerge topology that accumulated the old per-link cap.
        for (index, particle) in particles[..8].iter_mut().enumerate() {
            particle.inverse_mass = if index == 0 { 0.5 } else { 1.0 };
            particle.face_weight = if index == 0 { 1.0 } else { 0.0 };
            particle.position = if index < 4 {
                Vec2::new(-0.03 * index as f32, 0.0)
            } else {
                Vec2::new(0.3, (index - 4) as f32 * 0.2 - 0.3)
            };
            particle.previous_position = particle.position - Vec2::new(0.004, -0.002);
            particle.predicted_position = particle.position + Vec2::new(0.01, 0.006);
        }
        let before = particles;
        let prediction = predict(&particles, 8, 0.1);
        assert_eq!(prediction.component_count, 5);
        assert_eq!(constrain_to_main(&mut particles, 8, &prediction, 0.1), 4);
        let mut mass_weighted_correction = Vec2::ZERO;
        for (particle, old) in particles[..8].iter().zip(&before[..8]) {
            let correction = particle.position - old.position;
            assert!(correction.length() <= 0.036 + 1.0e-6);
            assert!(
                (particle.predicted_position - particle.position)
                    .distance(old.predicted_position - old.position)
                    < 1.0e-6
            );
            assert!(
                (particle.position - particle.previous_position)
                    .distance(old.position - old.previous_position)
                    < 1.0e-6
            );
            mass_weighted_correction += correction / particle.inverse_mass;
        }
        assert!(mass_weighted_correction.length() < 1.0e-6);
        assert!(predict(&particles, 8, 0.1).component_count <= prediction.component_count);
    }

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
