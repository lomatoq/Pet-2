use glam::Vec2;

use super::particles::{LiquidParticle, MAX_LIQUID_PARTICLES};

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ComponentSummary {
    pub component_count: usize,
    pub main_component: u8,
    pub main_mass: f32,
    pub detached_mass: f32,
    pub main_com: Vec2,
}

/// A render-space topology observation. These labels describe the filtered
/// anisotropic iso-field only; they must never feed forces or solver ownership.
#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RenderComponentClassification {
    pub summary: ComponentSummary,
    pub component_ids: [u8; MAX_LIQUID_PARTICLES],
}

pub fn assign_components(
    particles: &mut [LiquidParticle; MAX_LIQUID_PARTICLES],
    count: usize,
    spacing: f32,
    link_radius_scale: f32,
) -> ComponentSummary {
    let count = count.min(MAX_LIQUID_PARTICLES);
    let snapshot = *particles;
    if count == 0 {
        return ComponentSummary::default();
    }
    let mut temporary_groups = [u8::MAX; MAX_LIQUID_PARTICLES];
    let mut masses = [0.0_f32; MAX_LIQUID_PARTICLES];
    let mut centers = [Vec2::ZERO; MAX_LIQUID_PARTICLES];
    let mut component_count = 0_usize;
    let threshold = spacing * link_radius_scale.clamp(1.0, 1.75);
    let split_threshold_squared = (threshold * 1.08).powi(2);
    let join_threshold_squared = (threshold * 0.92).powi(2);

    for seed in 0..count {
        if temporary_groups[seed] != u8::MAX {
            continue;
        }
        let group = component_count as u8;
        component_count += 1;
        let mut queue = [0_usize; MAX_LIQUID_PARTICLES];
        let mut read = 0;
        let mut write = 1;
        queue[0] = seed;
        temporary_groups[seed] = group;
        while read < write {
            let current = queue[read];
            read += 1;
            let mass = snapshot[current].inverse_mass.max(1.0e-5).recip();
            masses[usize::from(group)] += mass;
            centers[usize::from(group)] += snapshot[current].position * mass;
            for other in 0..count {
                if temporary_groups[other] != u8::MAX {
                    continue;
                }
                let threshold_squared =
                    if snapshot[current].component_id == snapshot[other].component_id {
                        split_threshold_squared
                    } else {
                        join_threshold_squared
                    };
                if snapshot[current]
                    .position
                    .distance_squared(snapshot[other].position)
                    >= threshold_squared
                {
                    continue;
                }
                temporary_groups[other] = group;
                queue[write] = other;
                write += 1;
            }
        }
    }

    // Presentation follows one immutable material carrier rather than whichever
    // blob happens to be largest this frame. A strict comparison keeps the lowest
    // particle index as the deterministic tie-break.
    let mut face_carrier = 0_usize;
    for index in 1..count {
        if snapshot[index].face_weight > snapshot[face_carrier].face_weight {
            face_carrier = index;
        }
    }
    let carrier_group = usize::from(temporary_groups[face_carrier]);

    // Match fresh graph groups to previous persistent IDs by material overlap.
    // Reserve the face carrier's ID first, so a merge with a larger component can
    // never re-parent the presentation anchor.
    let mut group_ids = [u8::MAX; MAX_LIQUID_PARTICLES];
    let mut used_ids = [false; u8::MAX as usize + 1];
    let carrier_id = snapshot[face_carrier].component_id;
    group_ids[carrier_group] = carrier_id;
    used_ids[usize::from(carrier_id)] = true;
    for (group, group_id) in group_ids[..component_count].iter_mut().enumerate() {
        if *group_id != u8::MAX {
            continue;
        }
        let mut overlap = [0.0_f32; u8::MAX as usize + 1];
        for index in 0..count {
            if usize::from(temporary_groups[index]) == group {
                let mass = snapshot[index].inverse_mass.max(1.0e-5).recip();
                overlap[usize::from(snapshot[index].component_id)] += mass;
            }
        }
        let mut best_id = None;
        let mut best_mass = 0.0_f32;
        for (id, overlap_mass) in overlap.into_iter().enumerate() {
            if !used_ids[id] && overlap_mass > best_mass {
                best_id = Some(id as u8);
                best_mass = overlap_mass;
            }
        }
        let persistent_id = best_id.unwrap_or_else(|| {
            used_ids
                .iter()
                .position(|used| !*used)
                .unwrap_or(group)
                .min(u8::MAX as usize) as u8
        });
        *group_id = persistent_id;
        used_ids[usize::from(persistent_id)] = true;
    }

    for index in 0..count {
        particles[index].component_id = group_ids[usize::from(temporary_groups[index])];
    }
    let total_mass = masses[..component_count].iter().sum::<f32>();
    let main_mass = masses[carrier_group];
    let main_com = centers[carrier_group] / main_mass.max(1.0e-5);
    ComponentSummary {
        component_count,
        main_component: group_ids[carrier_group],
        main_mass,
        detached_mass: (total_mass - main_mass).max(0.0),
        main_com,
    }
}

/// Classifies particles using the same anisotropic compact kernels that are
/// presented by the density shader. This is intentionally used after filtered
/// render proxies are current, so a manual tear becomes a component split on
/// the same frame that the visible iso-field disconnects.
#[cfg(test)]
pub fn classify_render_components(
    particles: &[LiquidParticle; MAX_LIQUID_PARTICLES],
    count: usize,
    kernel_radius: f32,
    iso_threshold: f32,
) -> RenderComponentClassification {
    let snapshot = *particles;
    if count == 0 {
        return RenderComponentClassification {
            summary: ComponentSummary::default(),
            component_ids: [u8::MAX; MAX_LIQUID_PARTICLES],
        };
    }
    let mut assigned = [u8::MAX; MAX_LIQUID_PARTICLES];
    let mut masses = [0.0_f32; MAX_LIQUID_PARTICLES];
    let mut centers = [Vec2::ZERO; MAX_LIQUID_PARTICLES];
    let mut component_count = 0_usize;

    for seed in 0..count {
        if assigned[seed] != u8::MAX {
            continue;
        }
        let component = component_count as u8;
        component_count += 1;
        let mut queue = [0_usize; MAX_LIQUID_PARTICLES];
        let mut read = 0;
        let mut write = 1;
        queue[0] = seed;
        assigned[seed] = component;
        while read < write {
            let current = queue[read];
            read += 1;
            let mass = snapshot[current].inverse_mass.max(1.0e-5).recip();
            masses[usize::from(component)] += mass;
            centers[usize::from(component)] += snapshot[current].position * mass;
            for other in 0..count {
                if assigned[other] != u8::MAX
                    || !render_kernels_connected(
                        snapshot[current],
                        snapshot[other],
                        kernel_radius,
                        iso_threshold,
                    )
                {
                    continue;
                }
                assigned[other] = component;
                queue[write] = other;
                write += 1;
            }
        }
    }

    let mut face_carrier = 0_usize;
    for index in 1..count {
        if snapshot[index].face_weight > snapshot[face_carrier].face_weight {
            face_carrier = index;
        }
    }
    let main_component = usize::from(assigned[face_carrier]);
    let total_mass = masses[..component_count].iter().sum::<f32>();
    let main_mass = masses[main_component];
    let main_com = centers[main_component] / main_mass.max(1.0e-5);
    RenderComponentClassification {
        summary: ComponentSummary {
            component_count,
            main_component: main_component as u8,
            main_mass,
            detached_mass: (total_mass - main_mass).max(0.0),
            main_com,
        },
        component_ids: assigned,
    }
}

#[cfg(test)]
fn render_kernels_connected(
    first: LiquidParticle,
    second: LiquidParticle,
    kernel_radius: f32,
    iso_threshold: f32,
) -> bool {
    if first
        .render_position
        .distance_squared(second.render_position)
        <= 1.0e-10
    {
        return true;
    }
    for sample in 1..8 {
        let point = first
            .render_position
            .lerp(second.render_position, sample as f32 / 8.0);
        let density = render_kernel_density(point, first, kernel_radius)
            + render_kernel_density(point, second, kernel_radius);
        if density < iso_threshold {
            return false;
        }
    }
    true
}

#[cfg(test)]
fn render_kernel_density(point: Vec2, particle: LiquidParticle, kernel_radius: f32) -> f32 {
    let mut axis = particle.render_axis_major.normalize_or_zero();
    if axis.length_squared() < 0.5 {
        axis = Vec2::X;
    }
    let perpendicular = Vec2::new(-axis.y, axis.x);
    let aspect = particle.render_aspect.max(1.0);
    let area_scale = aspect.sqrt();
    let delta = point - particle.render_position;
    let coordinate = Vec2::new(
        delta.dot(axis) / (kernel_radius * area_scale).max(1.0e-5),
        delta.dot(perpendicular) / (kernel_radius / area_scale).max(1.0e-5),
    );
    let radius_squared = coordinate.length_squared();
    if radius_squared >= 1.0 {
        0.0
    } else {
        (1.0 - radius_squared).powi(3)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::liquid::particles::PARTICLE_SPACING;

    #[test]
    fn face_carrier_component_wins_even_when_another_blob_is_larger() {
        let mut particles = [LiquidParticle::default(); MAX_LIQUID_PARTICLES];
        particles[0].position = Vec2::ZERO;
        particles[0].inverse_mass = 1.0;
        particles[1].position = Vec2::new(PARTICLE_SPACING, 0.0);
        particles[1].inverse_mass = 1.0;
        particles[2].position = Vec2::new(PARTICLE_SPACING * 2.0, 0.0);
        particles[2].inverse_mass = 1.0;
        particles[3].position = Vec2::new(0.65, 0.0);
        particles[3].face_weight = 1.0;
        particles[3].inverse_mass = 1.0;
        particles[3].force = Vec2::new(4.0, -2.0);
        let force_before = particles[3].force;

        let summary = assign_components(&mut particles, 4, PARTICLE_SPACING, 1.30);
        assert_eq!(summary.component_count, 2);
        assert_eq!(summary.main_component, particles[3].component_id);
        assert_eq!(summary.main_mass, 1.0);
        assert_eq!(summary.detached_mass, 3.0);
        assert_eq!(
            particles[3].force, force_before,
            "diagnostics changed force"
        );
    }

    #[test]
    fn face_carrier_tie_uses_the_lowest_particle_index() {
        let mut particles = [LiquidParticle::default(); MAX_LIQUID_PARTICLES];
        particles[0].position = Vec2::ZERO;
        particles[0].face_weight = 1.0;
        particles[0].inverse_mass = 1.0;
        particles[1].position = Vec2::new(0.65, 0.0);
        particles[1].face_weight = 1.0;
        particles[1].inverse_mass = 1.0;
        let summary = assign_components(&mut particles, 2, PARTICLE_SPACING, 1.30);
        assert_eq!(summary.main_component, particles[0].component_id);
    }

    #[test]
    fn component_links_use_split_join_hysteresis() {
        let distance = PARTICLE_SPACING * 1.30;
        let mut previously_joined = [LiquidParticle::default(); MAX_LIQUID_PARTICLES];
        previously_joined[0].position = Vec2::ZERO;
        previously_joined[0].inverse_mass = 1.0;
        previously_joined[0].component_id = 7;
        previously_joined[1].position = Vec2::new(distance, 0.0);
        previously_joined[1].inverse_mass = 1.0;
        previously_joined[1].component_id = 7;
        let joined = assign_components(&mut previously_joined, 2, PARTICLE_SPACING, 1.30);
        assert_eq!(joined.component_count, 1);
        assert_eq!(previously_joined[0].component_id, 7);
        assert_eq!(previously_joined[1].component_id, 7);

        let mut previously_split = previously_joined;
        previously_split[0].component_id = 7;
        previously_split[1].component_id = 11;
        let split = assign_components(&mut previously_split, 2, PARTICLE_SPACING, 1.30);
        assert_eq!(split.component_count, 2);
        assert_eq!(previously_split[0].component_id, 7);
        assert_eq!(previously_split[1].component_id, 11);
    }

    #[test]
    fn merge_preserves_face_carrier_persistent_id() {
        let mut particles = [LiquidParticle::default(); MAX_LIQUID_PARTICLES];
        particles[0].position = Vec2::ZERO;
        particles[0].inverse_mass = 1.0;
        particles[0].face_weight = 1.0;
        particles[0].component_id = 13;
        particles[1].position = Vec2::new(PARTICLE_SPACING * 0.7, 0.0);
        particles[1].inverse_mass = 1.0;
        particles[1].component_id = 2;
        particles[2].position = Vec2::new(PARTICLE_SPACING * 1.4, 0.0);
        particles[2].inverse_mass = 1.0;
        particles[2].component_id = 2;

        let summary = assign_components(&mut particles, 3, PARTICLE_SPACING, 1.30);
        assert_eq!(summary.component_count, 1);
        assert_eq!(summary.main_component, 13);
        assert!(
            particles[..3]
                .iter()
                .all(|particle| particle.component_id == 13)
        );
    }

    #[test]
    fn rendered_components_honor_full_authored_anisotropy() {
        let mut particles = [LiquidParticle::default(); MAX_LIQUID_PARTICLES];
        for (particle, x) in particles[..2].iter_mut().zip([-0.215_f32, 0.215]) {
            particle.position = Vec2::new(x, 0.0);
            particle.render_position = particle.position;
            particle.render_axis_major = Vec2::X;
            particle.render_aspect = 4.5;
            particle.inverse_mass = 1.0;
        }
        let original_ids = [particles[0].component_id, particles[1].component_id];
        let classification = classify_render_components(&particles, 2, 0.16, 0.30);
        assert_eq!(classification.summary.component_count, 1);
        assert_eq!(classification.summary.main_mass, 2.0);
        assert_eq!(
            [particles[0].component_id, particles[1].component_id],
            original_ids,
            "render topology leaked back into the physical graph"
        );
    }
}
