use std::f32::consts::TAU;

use glam::Vec2;

pub const MAX_LIQUID_PARTICLES: usize = 96;
#[cfg(test)]
pub const DEFAULT_LIQUID_PARTICLES: usize = 96;
pub const PARTICLE_SPACING: f32 = 0.072;
pub const KERNEL_RADIUS: f32 = 0.145;

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct LiquidParticle {
    /// Immutable material coordinate used by the face-frame fit.
    pub rest_position: Vec2,
    pub position: Vec2,
    pub previous_position: Vec2,
    pub predicted_position: Vec2,
    pub velocity: Vec2,
    pub force: Vec2,
    pub inverse_mass: f32,
    pub radius: f32,
    pub density: f32,
    pub lambda: f32,
    pub component_id: u8,
    pub surface_score: f32,
    pub pigment: f32,
    pub emission: f32,
    pub face_weight: f32,
    pub motor_weight: f32,
    pub detached_seconds: f32,
    /// Temporally filtered reconstruction state. Physics remains unsmoothed;
    /// only the splat renderer consumes these fields.
    pub render_position: Vec2,
    pub render_axis_major: Vec2,
    pub render_aspect: f32,
    pub render_surface_score: f32,
}

impl LiquidParticle {
    #[must_use]
    pub fn is_finite(self) -> bool {
        self.rest_position.is_finite()
            && self.position.is_finite()
            && self.previous_position.is_finite()
            && self.predicted_position.is_finite()
            && self.velocity.is_finite()
            && self.force.is_finite()
            && self.render_position.is_finite()
            && self.render_axis_major.is_finite()
            && [
                self.inverse_mass,
                self.radius,
                self.density,
                self.lambda,
                self.surface_score,
                self.pigment,
                self.emission,
                self.face_weight,
                self.motor_weight,
                self.detached_seconds,
                self.render_aspect,
                self.render_surface_score,
            ]
            .into_iter()
            .all(f32::is_finite)
    }
}

#[must_use]
#[cfg(test)]
pub fn initialize_particles(seed: u64) -> ([LiquidParticle; MAX_LIQUID_PARTICLES], usize) {
    initialize_particles_with(seed, DEFAULT_LIQUID_PARTICLES, 1.0)
}

#[must_use]
pub fn initialize_particles_with(
    seed: u64,
    requested_count: usize,
    spacing_scale: f32,
) -> ([LiquidParticle; MAX_LIQUID_PARTICLES], usize) {
    let spacing = PARTICLE_SPACING * spacing_scale.clamp(0.65, 1.45);
    let mut candidates = Vec::with_capacity(160);
    let vertical_spacing = spacing * 3.0_f32.sqrt() * 0.5;
    for row in -8_i32..=8 {
        for column in -8_i32..=8 {
            let offset = if row & 1 == 0 { 0.0 } else { 0.5 };
            let point = Vec2::new(
                (column as f32 + offset) * spacing,
                row as f32 * vertical_spacing,
            );
            let elliptical_radius = (point / Vec2::new(0.35, 0.43)).length_squared();
            if elliptical_radius <= 1.0 {
                candidates.push((elliptical_radius, point));
            }
        }
    }
    candidates.sort_by(|(radius_a, point_a), (radius_b, point_b)| {
        radius_a
            .total_cmp(radius_b)
            .then_with(|| point_a.y.total_cmp(&point_b.y))
            .then_with(|| point_a.x.total_cmp(&point_b.x))
    });

    let phase = (seed as u32 as f32 / u32::MAX as f32 - 0.5) * 0.025;
    let mut particles = [LiquidParticle::default(); MAX_LIQUID_PARTICLES];
    let count = requested_count
        .clamp(24, MAX_LIQUID_PARTICLES)
        .min(candidates.len());
    for (index, (_, mut point)) in candidates.into_iter().take(count).enumerate() {
        point.x += phase * (point.y * 5.0).sin();
        // Break the visible six-fold lattice without sacrificing deterministic
        // near-uniform coverage. A very small blue-noise-like material jitter is
        // enough because PBF subsequently relaxes the pack.
        let jitter_angle = hash_unit(seed, index as u64, 0x91) * TAU;
        let jitter_radius = hash_unit(seed, index as u64, 0xD7).sqrt() * spacing * 0.055;
        point += Vec2::from_angle(jitter_angle) * jitter_radius;
        let normalized_radius = (point / Vec2::new(0.35, 0.43)).length();
        if normalized_radius > 0.995 {
            point *= 0.995 / normalized_radius;
        }
        let normalized = point / Vec2::new(0.35, 0.43);
        let radial = normalized.length().clamp(0.0, 1.0);
        let core = (1.0 - radial).powf(1.45);
        let face_delta = (point - Vec2::new(0.0, 0.135)) / Vec2::new(0.18, 0.14);
        let face_weight = (-face_delta.length_squared() * 1.35).exp();
        particles[index] = LiquidParticle {
            rest_position: point,
            position: point,
            previous_position: point,
            predicted_position: point,
            inverse_mass: 1.0,
            radius: spacing * 0.49,
            pigment: (0.40 + core * 0.55).clamp(0.0, 1.0),
            emission: (core * 0.92 + face_weight * 0.08).clamp(0.0, 1.0),
            face_weight: face_weight.clamp(0.0, 1.0),
            motor_weight: (0.22 + core * 0.78).clamp(0.0, 1.0),
            render_position: point,
            render_axis_major: Vec2::X,
            render_aspect: 1.0,
            ..LiquidParticle::default()
        };
    }
    (particles, count)
}

fn hash_unit(seed: u64, index: u64, salt: u64) -> f32 {
    let mut value =
        seed ^ index.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ salt.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value ^= value >> 30;
    value = value.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value ^= value >> 27;
    value = value.wrapping_mul(0x94D0_49BB_1331_11EB);
    value ^= value >> 31;
    ((value >> 40) as u32) as f32 / 0x00FF_FFFF as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initial_pack_is_deterministic_compact_and_has_a_face_patch() {
        let (first, count) = initialize_particles(42);
        let (second, replay_count) = initialize_particles(42);
        assert_eq!(count, DEFAULT_LIQUID_PARTICLES);
        assert_eq!(count, replay_count);
        assert_eq!(first, second);
        assert!(first[..count].iter().all(|particle| particle.is_finite()));
        assert!(
            first[..count]
                .iter()
                .filter(|particle| particle.face_weight > 0.35)
                .count()
                >= 8
        );
        assert!(
            first[..count]
                .iter()
                .all(|particle| { (particle.position / Vec2::new(0.35, 0.43)).length() <= 1.01 })
        );
    }
}
