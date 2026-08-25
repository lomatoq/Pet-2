use super::particles::{LiquidParticle, MAX_LIQUID_PARTICLES, two_particles_mut};

pub const MAX_BONDS: usize = 384;
const MAX_BONDS_PER_PARTICLE: usize = 6;

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ViscoelasticBond {
    pub a: u8,
    pub b: u8,
    pub rest_length: f32,
    pub compliance: f32,
    pub age: f32,
    pub strain: f32,
    pub yield_strain: f32,
    pub break_strain: f32,
    pub relaxation_time: f32,
    pub strength: f32,
    pub visual_neck: f32,
    pub damage: f32,
    pub lambda: f32,
    pub break_substeps: u8,
    pub active: bool,
    pub face_lock: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BondMaterial {
    pub compliance: f32,
    pub yield_strain: f32,
    pub break_strain: f32,
    pub relaxation_time: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BondUpdateParameters {
    pub material: BondMaterial,
    pub spacing: f32,
    pub create_radius_scale: f32,
    pub create_speed_limit: f32,
    pub dt: f32,
}

pub fn initialize_bonds(
    particles: &[LiquidParticle; MAX_LIQUID_PARTICLES],
    count: usize,
    spacing: f32,
    create_radius_scale: f32,
) -> [ViscoelasticBond; MAX_BONDS] {
    let mut bonds = [ViscoelasticBond::default(); MAX_BONDS];
    let mut degrees = [0_u8; MAX_LIQUID_PARTICLES];
    let mut slot = 0;
    for a in 0..count {
        for b in (a + 1)..count {
            if slot >= MAX_BONDS {
                return bonds;
            }
            let distance = particles[a].position.distance(particles[b].position);
            if distance > spacing * create_radius_scale.clamp(1.0, 1.75)
                || usize::from(degrees[a]) >= MAX_BONDS_PER_PARTICLE
                || usize::from(degrees[b]) >= MAX_BONDS_PER_PARTICLE
            {
                continue;
            }
            bonds[slot] = make_bond(particles, a, b, distance, BondMaterial::default(), spacing);
            degrees[a] += 1;
            degrees[b] += 1;
            slot += 1;
        }
    }
    bonds
}

impl Default for BondMaterial {
    fn default() -> Self {
        Self {
            compliance: 1.4e-4,
            yield_strain: 0.18,
            break_strain: 0.82,
            relaxation_time: 0.72,
        }
    }
}

pub fn solve_bonds(
    particles: &mut [LiquidParticle; MAX_LIQUID_PARTICLES],
    bonds: &mut [ViscoelasticBond; MAX_BONDS],
    material: BondMaterial,
    iterations: usize,
    dt: f32,
) {
    for bond in bonds.iter_mut().filter(|bond| bond.active) {
        bond.lambda = 0.0;
    }
    for _ in 0..iterations {
        for bond in bonds.iter_mut().filter(|bond| bond.active) {
            let a = usize::from(bond.a);
            let b = usize::from(bond.b);
            let delta = particles[b].predicted_position - particles[a].predicted_position;
            let distance = delta.length();
            if distance <= 1.0e-6 {
                continue;
            }
            let constraint = distance - bond.rest_length;
            let compliance = if bond.face_lock {
                material.compliance * 0.36
            } else {
                material.compliance
            };
            let alpha = compliance.max(1.0e-8) / (dt * dt).max(1.0e-8);
            let inverse_mass_sum = particles[a].inverse_mass + particles[b].inverse_mass + alpha;
            let delta_lambda = (-constraint - alpha * bond.lambda) / inverse_mass_sum;
            bond.lambda += delta_lambda;
            let correction = delta / distance * delta_lambda.clamp(-0.018, 0.018);
            let (particle_a, particle_b) = two_particles_mut(particles, a, b);
            particle_a.predicted_position -= correction * particle_a.inverse_mass;
            particle_b.predicted_position += correction * particle_b.inverse_mass;
        }
    }
}

pub fn update_bonds(
    particles: &[LiquidParticle; MAX_LIQUID_PARTICLES],
    count: usize,
    bonds: &mut [ViscoelasticBond; MAX_BONDS],
    contact_age: &mut [f32],
    parameters: BondUpdateParameters,
) {
    let BondUpdateParameters {
        material,
        spacing,
        create_radius_scale,
        create_speed_limit,
        dt,
    } = parameters;
    for bond in bonds.iter_mut().filter(|bond| bond.active) {
        let distance = particles[usize::from(bond.a)]
            .position
            .distance(particles[usize::from(bond.b)].position);
        bond.age += dt;
        bond.strain = (distance / bond.rest_length.max(1.0e-5) - 1.0).max(-0.9);
        bond.compliance = material.compliance;
        bond.yield_strain = material.yield_strain;
        bond.break_strain = if bond.face_lock {
            material.break_strain * 1.8
        } else {
            material.break_strain
        };
        bond.relaxation_time = material.relaxation_time;
        if bond.strain.abs() > bond.yield_strain {
            let relaxation = 1.0 - (-dt / bond.relaxation_time.max(0.05)).exp();
            bond.rest_length += (distance - bond.rest_length) * relaxation * 0.72;
        }
        if bond.strain > bond.break_strain && !bond.face_lock {
            bond.break_substeps = bond.break_substeps.saturating_add(1);
        } else {
            bond.break_substeps = bond.break_substeps.saturating_sub(1);
        }
        if bond.break_substeps >= 3 {
            bond.active = false;
            bond.strength = 0.0;
            bond.visual_neck = 0.0;
            let a = usize::from(bond.a).min(usize::from(bond.b));
            let b = usize::from(bond.a).max(usize::from(bond.b));
            contact_age[a * MAX_LIQUID_PARTICLES + b] = -0.30;
            continue;
        }
        let normalized_strain = (bond.strain.max(0.0) / bond.break_strain.max(0.1)).clamp(0.0, 1.0);
        bond.visual_neck = (1.0 - normalized_strain).sqrt();
        bond.strength = (1.0 - normalized_strain * 0.65).clamp(0.0, 1.0);
    }

    let mut degrees = [0_u8; MAX_LIQUID_PARTICLES];
    for bond in bonds.iter().filter(|bond| bond.active) {
        degrees[usize::from(bond.a)] = degrees[usize::from(bond.a)].saturating_add(1);
        degrees[usize::from(bond.b)] = degrees[usize::from(bond.b)].saturating_add(1);
    }
    for a in 0..count {
        for b in (a + 1)..count {
            let age_index = a * MAX_LIQUID_PARTICLES + b;
            let distance = particles[a].position.distance(particles[b].position);
            let relative_speed = (particles[a].velocity - particles[b].velocity).length();
            let stable_contact = distance < spacing * create_radius_scale.clamp(1.0, 1.75)
                && relative_speed < create_speed_limit.clamp(0.05, 3.0);
            contact_age[age_index] = if stable_contact {
                (contact_age[age_index] + dt).min(1.0)
            } else {
                0.0
            };
            if contact_age[age_index] < 0.12
                || usize::from(degrees[a]) >= MAX_BONDS_PER_PARTICLE
                || usize::from(degrees[b]) >= MAX_BONDS_PER_PARTICLE
                || has_active_bond(bonds, a, b)
            {
                continue;
            }
            if let Some(slot) = bonds.iter_mut().find(|bond| !bond.active) {
                *slot = make_bond(particles, a, b, distance, material, spacing);
                degrees[a] += 1;
                degrees[b] += 1;
                contact_age[age_index] = 0.0;
            }
        }
    }
}

#[must_use]
pub fn active_bond_count(bonds: &[ViscoelasticBond; MAX_BONDS]) -> usize {
    bonds.iter().filter(|bond| bond.active).count()
}

fn has_active_bond(bonds: &[ViscoelasticBond; MAX_BONDS], a: usize, b: usize) -> bool {
    bonds.iter().any(|bond| {
        bond.active
            && ((usize::from(bond.a) == a && usize::from(bond.b) == b)
                || (usize::from(bond.a) == b && usize::from(bond.b) == a))
    })
}

fn make_bond(
    particles: &[LiquidParticle; MAX_LIQUID_PARTICLES],
    a: usize,
    b: usize,
    distance: f32,
    material: BondMaterial,
    spacing: f32,
) -> ViscoelasticBond {
    let face_lock = particles[a].face_weight > 0.22 && particles[b].face_weight > 0.22;
    ViscoelasticBond {
        a: a as u8,
        b: b as u8,
        rest_length: distance.max(spacing * 0.58),
        compliance: material.compliance,
        yield_strain: material.yield_strain,
        break_strain: material.break_strain,
        relaxation_time: material.relaxation_time,
        strength: 1.0,
        visual_neck: 1.0,
        active: true,
        face_lock,
        ..ViscoelasticBond::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::liquid::particles::{PARTICLE_SPACING, initialize_particles};

    #[test]
    fn initial_network_is_bounded_and_face_patch_is_reinforced() {
        let (particles, count) = initialize_particles(7);
        let bonds = initialize_bonds(&particles, count, PARTICLE_SPACING, 1.30);
        assert!(active_bond_count(&bonds) > count);
        assert!(active_bond_count(&bonds) <= MAX_BONDS);
        assert!(bonds.iter().any(|bond| bond.active && bond.face_lock));
    }

    #[test]
    fn stretched_neck_thins_then_breaks_without_a_phantom_tether() {
        let mut particles = [LiquidParticle::default(); MAX_LIQUID_PARTICLES];
        particles[0].position = glam::Vec2::ZERO;
        particles[1].position = glam::Vec2::new(PARTICLE_SPACING * 2.2, 0.0);
        particles[0].inverse_mass = 1.0;
        particles[1].inverse_mass = 1.0;
        let material = BondMaterial {
            break_strain: 0.60,
            ..BondMaterial::default()
        };
        let mut bonds = [ViscoelasticBond::default(); MAX_BONDS];
        bonds[0] = ViscoelasticBond {
            a: 0,
            b: 1,
            rest_length: PARTICLE_SPACING,
            strength: 1.0,
            visual_neck: 1.0,
            active: true,
            ..ViscoelasticBond::default()
        };
        let mut contact_age = vec![0.0; MAX_LIQUID_PARTICLES * MAX_LIQUID_PARTICLES];
        update_bonds(
            &particles,
            2,
            &mut bonds,
            &mut contact_age,
            BondUpdateParameters {
                material,
                spacing: PARTICLE_SPACING,
                create_radius_scale: 1.30,
                create_speed_limit: 0.48,
                dt: 1.0 / 120.0,
            },
        );
        assert!(bonds[0].visual_neck < 0.55);
        for _ in 0..2 {
            update_bonds(
                &particles,
                2,
                &mut bonds,
                &mut contact_age,
                BondUpdateParameters {
                    material,
                    spacing: PARTICLE_SPACING,
                    create_radius_scale: 1.30,
                    create_speed_limit: 0.48,
                    dt: 1.0 / 120.0,
                },
            );
        }
        assert!(!bonds[0].active);
        assert_eq!(bonds[0].visual_neck, 0.0);
        assert_eq!(bonds[0].strength, 0.0);
    }
}
