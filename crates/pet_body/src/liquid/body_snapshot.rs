use std::collections::BTreeSet;

use glam::Vec2;
use lifecore::{ComponentDetachReason, ComponentLifecycle, stable_hash_bytes};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::{
    LiquidMorphRuntime, MaterialParameters,
    collisions::MaterialStressFrame,
    components::assign_components,
    density::update_density_and_surface,
    particles::{KERNEL_RADIUS, LiquidParticle, MAX_LIQUID_PARTICLES, PARTICLE_SPACING},
    viscoelastic_bonds::{MAX_BONDS, ViscoelasticBond},
};
use crate::PbfTuning;

pub const BODY_MATERIAL_SNAPSHOT_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BodyMaterialSnapshot {
    pub schema_version: u32,
    pub identity_seed: u64,
    pub tuning_schema_version: u32,
    pub tuning_revision: u64,
    pub structural_tuning_hash: u64,
    pub particle_count: usize,
    pub body_origin: Vec2,
    pub particles: Vec<SavedLiquidParticle>,
    pub bonds: Vec<SavedViscoelasticBond>,
    pub components: SavedComponentLifecycle,
    pub elapsed_seconds: f64,
    pub deterministic_sequence: u64,
    pub total_mass_bits_checksum: u64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct SavedLiquidParticle {
    pub rest_position: Vec2,
    pub position: Vec2,
    pub velocity: Vec2,
    pub inverse_mass: f32,
    pub radius: f32,
    pub component_id: u8,
    pub surface_score: f32,
    pub pigment: f32,
    pub emission: f32,
    pub face_weight: f32,
    pub motor_weight: f32,
    pub detached_seconds: f32,
    pub render_position: Vec2,
    pub render_axis_major: Vec2,
    pub render_aspect: f32,
    pub render_surface_score: f32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct SavedViscoelasticBond {
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
    pub break_substeps: u8,
    pub active: bool,
    pub face_lock: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SavedComponentLifecycle {
    pub main_lifecycle: ComponentLifecycle,
    pub necking_seconds: f32,
    pub tracked: Vec<SavedTrackedComponent>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct SavedTrackedComponent {
    pub id: u8,
    pub lifecycle: ComponentLifecycle,
    pub reason: ComponentDetachReason,
    pub age: f32,
    pub offscreen_seconds: f32,
    pub center: Vec2,
    pub velocity: Vec2,
    pub particle_count: u8,
    pub mass_fraction: f32,
    #[serde(default)]
    pub recovery_seconds: f32,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum BodySnapshotError {
    #[error("unsupported body snapshot schema {found}; expected {expected}")]
    UnsupportedSchema { found: u32, expected: u32 },
    #[error("body snapshot belongs to a different identity")]
    IdentityMismatch,
    #[error("body snapshot tuning schema does not match the active profile")]
    TuningSchemaMismatch,
    #[error("body snapshot structural tuning does not match the active profile")]
    StructuralTuningMismatch,
    #[error("body snapshot exceeds a bounded collection limit")]
    CollectionLimit,
    #[error("body snapshot particle count is inconsistent")]
    ParticleCountMismatch,
    #[error("body snapshot contains a non-finite or unsafe value")]
    InvalidValue,
    #[error("body snapshot contains an invalid or duplicate bond")]
    InvalidBond,
    #[error("body snapshot mass checksum is invalid")]
    MassChecksumMismatch,
    #[error("body snapshot component metadata is inconsistent with particles")]
    ComponentMismatch,
}

impl BodyMaterialSnapshot {
    pub fn validate(
        &self,
        expected_identity_seed: u64,
        expected_tuning_schema: u32,
        expected_tuning: PbfTuning,
    ) -> Result<(), BodySnapshotError> {
        if self.schema_version != BODY_MATERIAL_SNAPSHOT_SCHEMA_VERSION {
            return Err(BodySnapshotError::UnsupportedSchema {
                found: self.schema_version,
                expected: BODY_MATERIAL_SNAPSHOT_SCHEMA_VERSION,
            });
        }
        if self.identity_seed != expected_identity_seed {
            return Err(BodySnapshotError::IdentityMismatch);
        }
        if self.tuning_schema_version != expected_tuning_schema
            && !(self.tuning_schema_version == 21 && expected_tuning_schema == 22)
        {
            return Err(BodySnapshotError::TuningSchemaMismatch);
        }
        if self.structural_tuning_hash != liquid_structural_tuning_hash(expected_tuning) {
            return Err(BodySnapshotError::StructuralTuningMismatch);
        }
        if self.particles.len() > MAX_LIQUID_PARTICLES
            || self.bonds.len() > MAX_BONDS
            || self.components.tracked.len() > 3
        {
            return Err(BodySnapshotError::CollectionLimit);
        }
        if self.particle_count == 0
            || self.particle_count != self.particles.len()
            || self.particle_count != expected_tuning.particle_count
        {
            return Err(BodySnapshotError::ParticleCountMismatch);
        }
        if !self.body_origin.is_finite()
            || self.body_origin.length() > 64.0
            || !self.elapsed_seconds.is_finite()
            || !(0.0..=3_600.0).contains(&self.elapsed_seconds)
            || !self.components.necking_seconds.is_finite()
            || !(0.0..=4.0).contains(&self.components.necking_seconds)
        {
            return Err(BodySnapshotError::InvalidValue);
        }
        let valid_particle = |particle: &SavedLiquidParticle| {
            particle.rest_position.is_finite()
                && particle.position.is_finite()
                && particle.velocity.is_finite()
                && particle.render_position.is_finite()
                && particle.render_axis_major.is_finite()
                && particle.position.length() <= 32.0
                && particle.velocity.length() <= 64.0
                && particle.inverse_mass.is_finite()
                && particle.inverse_mass > 0.0
                && particle.inverse_mass <= 64.0
                && [
                    particle.radius,
                    particle.surface_score,
                    particle.pigment,
                    particle.emission,
                    particle.face_weight,
                    particle.motor_weight,
                    particle.detached_seconds,
                    particle.render_aspect,
                    particle.render_surface_score,
                ]
                .into_iter()
                .all(f32::is_finite)
        };
        if !self.particles.iter().all(valid_particle) {
            return Err(BodySnapshotError::InvalidValue);
        }
        if mass_bits_checksum(&self.particles) != self.total_mass_bits_checksum {
            return Err(BodySnapshotError::MassChecksumMismatch);
        }
        let mut pairs = BTreeSet::new();
        for bond in &self.bonds {
            let a = usize::from(bond.a);
            let b = usize::from(bond.b);
            if a >= self.particle_count
                || b >= self.particle_count
                || a >= b
                || !pairs.insert((a, b))
                || ![
                    bond.rest_length,
                    bond.compliance,
                    bond.age,
                    bond.strain,
                    bond.yield_strain,
                    bond.break_strain,
                    bond.relaxation_time,
                    bond.strength,
                    bond.visual_neck,
                    bond.damage,
                ]
                .into_iter()
                .all(f32::is_finite)
                || bond.rest_length <= 0.0
                || bond.rest_length > 4.0
            {
                return Err(BodySnapshotError::InvalidBond);
            }
        }
        for tracked in &self.components.tracked {
            if tracked.id
                == self
                    .particles
                    .iter()
                    .max_by(|a, b| a.face_weight.total_cmp(&b.face_weight))
                    .map_or(0, |particle| particle.component_id)
                || !self
                    .particles
                    .iter()
                    .any(|particle| particle.component_id == tracked.id)
                || !tracked.center.is_finite()
                || !tracked.velocity.is_finite()
                || ![
                    tracked.age,
                    tracked.offscreen_seconds,
                    tracked.mass_fraction,
                    tracked.recovery_seconds,
                ]
                .into_iter()
                .all(f32::is_finite)
                || !(0.0..=2.0).contains(&tracked.recovery_seconds)
            {
                return Err(BodySnapshotError::ComponentMismatch);
            }
        }
        Ok(())
    }
}

#[must_use]
pub fn liquid_structural_tuning_hash(tuning: PbfTuning) -> u64 {
    let mut bytes = Vec::with_capacity(40);
    bytes.extend_from_slice(&(tuning.particle_count as u64).to_le_bytes());
    bytes.extend_from_slice(&tuning.spacing_scale.to_bits().to_le_bytes());
    bytes.extend_from_slice(&tuning.kernel_radius_scale.to_bits().to_le_bytes());
    bytes.extend_from_slice(&tuning.rest_density_scale.to_bits().to_le_bytes());
    bytes.extend_from_slice(&tuning.component_link_radius_scale.to_bits().to_le_bytes());
    bytes.extend_from_slice(&tuning.bond_create_radius_scale.to_bits().to_le_bytes());
    stable_hash_bytes(&bytes)
}

#[must_use]
pub(super) fn mass_bits_checksum(particles: &[SavedLiquidParticle]) -> u64 {
    let mut bytes = Vec::with_capacity(particles.len() * 12);
    for (index, particle) in particles.iter().enumerate() {
        bytes.extend_from_slice(&(index as u64).to_le_bytes());
        bytes.extend_from_slice(&particle.inverse_mass.to_bits().to_le_bytes());
    }
    stable_hash_bytes(&bytes)
}

impl LiquidMorphRuntime {
    #[must_use]
    pub fn body_material_snapshot(
        &self,
        identity_seed: u64,
        tuning_schema_version: u32,
        tuning_revision: u64,
    ) -> BodyMaterialSnapshot {
        let particles = self.particles[..self.particle_count]
            .iter()
            .map(|particle| SavedLiquidParticle {
                rest_position: particle.rest_position,
                position: particle.position,
                velocity: particle.velocity,
                inverse_mass: particle.inverse_mass,
                radius: particle.radius,
                component_id: particle.component_id,
                surface_score: particle.surface_score,
                pigment: particle.pigment,
                emission: particle.emission,
                face_weight: particle.face_weight,
                motor_weight: particle.motor_weight,
                detached_seconds: particle.detached_seconds,
                render_position: particle.render_position,
                render_axis_major: particle.render_axis_major,
                render_aspect: particle.render_aspect,
                render_surface_score: particle.render_surface_score,
            })
            .collect::<Vec<_>>();
        let bonds = self
            .bonds
            .iter()
            .filter(|bond| bond.active)
            .map(|bond| SavedViscoelasticBond {
                a: bond.a,
                b: bond.b,
                rest_length: bond.rest_length,
                compliance: bond.compliance,
                age: bond.age,
                strain: bond.strain,
                yield_strain: bond.yield_strain,
                break_strain: bond.break_strain,
                relaxation_time: bond.relaxation_time,
                strength: bond.strength,
                visual_neck: bond.visual_neck,
                damage: bond.damage,
                break_substeps: bond.break_substeps,
                active: bond.active,
                face_lock: bond.face_lock,
            })
            .collect();
        let total_mass_bits_checksum = mass_bits_checksum(&particles);
        BodyMaterialSnapshot {
            schema_version: BODY_MATERIAL_SNAPSHOT_SCHEMA_VERSION,
            identity_seed,
            tuning_schema_version,
            tuning_revision,
            structural_tuning_hash: liquid_structural_tuning_hash(self.tuning),
            particle_count: self.particle_count,
            body_origin: self.body_origin,
            particles,
            bonds,
            components: self.component_lifecycle.snapshot(),
            elapsed_seconds: f64::from(self.elapsed),
            deterministic_sequence: self.interaction_probe.sequence(),
            total_mass_bits_checksum,
        }
    }

    pub fn restore_body_material_snapshot(
        &mut self,
        snapshot: &BodyMaterialSnapshot,
        expected_identity_seed: u64,
        expected_tuning_schema: u32,
    ) -> Result<(), BodySnapshotError> {
        snapshot.validate(expected_identity_seed, expected_tuning_schema, self.tuning)?;

        self.particles.fill(LiquidParticle::default());
        self.particle_count = snapshot.particle_count;
        for (target, saved) in self.particles.iter_mut().zip(&snapshot.particles) {
            *target = LiquidParticle {
                rest_position: saved.rest_position,
                position: saved.position,
                previous_position: saved.position,
                predicted_position: saved.position,
                velocity: saved.velocity,
                force: Vec2::ZERO,
                inverse_mass: saved.inverse_mass,
                radius: saved.radius,
                density: 0.0,
                lambda: 0.0,
                component_id: saved.component_id,
                surface_score: saved.surface_score,
                pigment: saved.pigment,
                emission: saved.emission,
                face_weight: saved.face_weight,
                motor_weight: saved.motor_weight,
                detached_seconds: saved.detached_seconds,
                render_position: saved.render_position,
                render_axis_major: saved.render_axis_major,
                render_aspect: saved.render_aspect,
                render_surface_score: saved.render_surface_score,
            };
        }
        self.bonds.fill(ViscoelasticBond::default());
        for (target, saved) in self.bonds.iter_mut().zip(&snapshot.bonds) {
            *target = ViscoelasticBond {
                a: saved.a,
                b: saved.b,
                rest_length: saved.rest_length,
                compliance: saved.compliance,
                age: saved.age,
                strain: saved.strain,
                yield_strain: saved.yield_strain,
                break_strain: saved.break_strain,
                relaxation_time: saved.relaxation_time,
                strength: saved.strength,
                visual_neck: saved.visual_neck,
                damage: saved.damage,
                lambda: 0.0,
                break_substeps: saved.break_substeps,
                active: saved.active,
                face_lock: saved.face_lock,
            };
        }
        self.bond_contact_age.fill(0.0);
        self.body_origin = snapshot.body_origin;
        self.elapsed = snapshot.elapsed_seconds as f32;
        self.component_lifecycle.restore(&snapshot.components);
        self.interaction_probe
            .restore_clock(snapshot.deterministic_sequence, snapshot.elapsed_seconds);
        self.topology_guard = super::TopologyGuard::default();
        self.topology_decision = super::TopologyDecision::default();
        self.material_grab = super::MaterialGrab::default();
        self.presentation_recovery_remaining = 0.0;
        self.pending_structural_tuning = None;

        let spacing = PARTICLE_SPACING * self.tuning.spacing_scale;
        let kernel_radius = KERNEL_RADIUS * self.tuning.kernel_radius_scale;
        update_density_and_surface(&mut self.particles, self.particle_count, kernel_radius);
        let component_spacing = super::component_graph_spacing(
            spacing,
            kernel_radius,
            self.tuning.iso_threshold,
            self.cinematic_features,
        );
        self.components = assign_components(
            &mut self.particles,
            self.particle_count,
            component_spacing,
            self.tuning.component_link_radius_scale,
        );
        self.update_diagnostics(
            MaterialParameters::from_tuning(self.tuning),
            MaterialStressFrame::default(),
            0.0,
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{LIQUID_TUNING_SCHEMA_VERSION, PbfTuning};

    #[test]
    fn body_snapshot_restores_full_mass_and_component_lifecycle() {
        let mut original = LiquidMorphRuntime::new(73);
        original.particles[0].position += Vec2::new(0.12, -0.04);
        original.particles[0].velocity = Vec2::new(0.30, -0.20);
        original.body_origin = Vec2::new(0.4, 0.2);
        let snapshot = original.body_material_snapshot(73, LIQUID_TUNING_SCHEMA_VERSION, 9);
        let mut restored = LiquidMorphRuntime::new(73);
        restored
            .restore_body_material_snapshot(&snapshot, 73, LIQUID_TUNING_SCHEMA_VERSION)
            .unwrap();
        let roundtrip = restored.body_material_snapshot(73, LIQUID_TUNING_SCHEMA_VERSION, 9);
        assert_eq!(
            roundtrip.total_mass_bits_checksum,
            snapshot.total_mass_bits_checksum
        );
        assert_eq!(roundtrip.particle_count, snapshot.particle_count);
        assert_eq!(
            restored.particles[0].position,
            original.particles[0].position
        );
        assert_eq!(
            restored.particles[0].previous_position,
            original.particles[0].position
        );
        assert_eq!(restored.particles[0].force, Vec2::ZERO);
        assert_eq!(restored.particles[0].lambda, 0.0);
    }

    #[test]
    fn structural_tuning_mismatch_rejects_the_whole_snapshot() {
        let runtime = LiquidMorphRuntime::new(81);
        let mut snapshot = runtime.body_material_snapshot(81, LIQUID_TUNING_SCHEMA_VERSION, 1);
        snapshot.structural_tuning_hash = liquid_structural_tuning_hash(PbfTuning {
            particle_count: 48,
            ..PbfTuning::default()
        });
        assert_eq!(
            snapshot.validate(81, LIQUID_TUNING_SCHEMA_VERSION, runtime.tuning),
            Err(BodySnapshotError::StructuralTuningMismatch)
        );
    }

    #[test]
    fn corrupt_mass_or_duplicate_bond_is_not_partially_loaded() {
        let runtime = LiquidMorphRuntime::new(91);
        let mut snapshot = runtime.body_material_snapshot(91, LIQUID_TUNING_SCHEMA_VERSION, 1);
        snapshot.particles[0].inverse_mass = 2.0;
        assert_eq!(
            snapshot.validate(91, LIQUID_TUNING_SCHEMA_VERSION, runtime.tuning),
            Err(BodySnapshotError::MassChecksumMismatch)
        );
        let mut duplicate = runtime.body_material_snapshot(91, LIQUID_TUNING_SCHEMA_VERSION, 1);
        duplicate.bonds.push(duplicate.bonds[0]);
        assert_eq!(
            duplicate.validate(91, LIQUID_TUNING_SCHEMA_VERSION, runtime.tuning),
            Err(BodySnapshotError::InvalidBond)
        );
    }
}
