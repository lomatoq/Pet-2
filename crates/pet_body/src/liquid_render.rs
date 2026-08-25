//! GPU-facing packing for the particle density reconstruction.
//!
//! The simulation remains renderer-agnostic. This module is the narrow contract
//! between the CPU liquid state and the single additive density draw call.

use bytemuck::{Pod, Zeroable};

use crate::{LiquidRenderState, MAX_IDLE_FRAGMENTS, MAX_PARTICLES};

#[repr(C)]
#[derive(Debug, Clone, Copy, Default, Pod, Zeroable)]
pub(crate) struct DensityInstance {
    /// Particle: center.xy, major radius, minor radius.
    /// Unified bubble: center.xy and isotropic radii.
    pub geometry_a: [f32; 4],
    /// Particle: major axis.xy, density scale, optical thickness.
    /// Unified bubble: axis.xy, density scale, optical thickness.
    pub geometry_b: [f32; 4],
    /// Particle: emission, pigment, main-component flag, surface score.
    /// Unified bubble: emission, pigment, zero, zero.
    pub material: [f32; 4],
    /// Particle: immutable material coordinate.xy and velocity.xy.
    /// Unified bubble: parent material coordinate and bud velocity.
    pub material_flow: [f32; 4],
}

pub(crate) fn pack_particles_for_material(
    state: &LiquidRenderState,
    cinematic_unified_fragments: bool,
) -> ([DensityInstance; MAX_PARTICLES], usize) {
    let mut output = [DensityInstance::default(); MAX_PARTICLES];
    let mut count = state.particle_count.min(MAX_PARTICLES);
    for (output, particle) in output[..count].iter_mut().zip(&state.particles[..count]) {
        *output = DensityInstance {
            geometry_a: [
                finite(particle.position.x),
                finite(particle.position.y),
                bounded(particle.major_radius, 0.002, 0.40, 0.12),
                bounded(particle.minor_radius, 0.002, 0.40, 0.12),
            ],
            geometry_b: [
                finite(particle.axis_major.x),
                finite(particle.axis_major.y),
                bounded(particle.density, 0.0, 2.0, 0.0),
                bounded(particle.optical_thickness, 0.0, 3.0, 1.0),
            ],
            material: [
                bounded(particle.emission, 0.0, 2.0, 0.0),
                bounded(particle.pigment, 0.0, 1.0, 0.0),
                if particle.main_component { 1.0 } else { 0.0 },
                bounded(particle.face_weight, 0.0, 1.0, 0.0),
            ],
            material_flow: [
                finite(particle.material_coordinate.x),
                finite(particle.material_coordinate.y),
                finite(particle.velocity.x),
                finite(particle.velocity.y),
            ],
        };
    }
    for bubble in state.bubbles[..state.bubble_count.min(MAX_IDLE_FRAGMENTS)].iter() {
        if count >= MAX_PARTICLES {
            break;
        }
        let speed = bubble.velocity.length();
        let aspect = 1.0 + (speed / 0.10).clamp(0.0, 1.0) * 0.12;
        let area_scale = aspect.sqrt();
        let axis = if speed > 1.0e-5 {
            bubble.velocity / speed
        } else {
            glam::Vec2::X
        };
        let optical_thickness = if cinematic_unified_fragments {
            bounded(bubble.optical_thickness, 0.0, 3.0, 0.92)
        } else {
            // Preserve the incumbent Current/Safe fragment packet exactly.
            0.74
        };
        let material_coordinate = if cinematic_unified_fragments {
            bubble.material_coordinate
        } else {
            glam::Vec2::ZERO
        };
        output[count] = DensityInstance {
            geometry_a: [
                finite(bubble.position.x),
                finite(bubble.position.y),
                bubble.radius * area_scale,
                bubble.radius / area_scale,
            ],
            geometry_b: [axis.x, axis.y, 0.92, optical_thickness],
            material: [bubble.emission, bubble.pigment, 0.0, 0.0],
            material_flow: [
                finite(material_coordinate.x),
                finite(material_coordinate.y),
                finite(bubble.velocity.x),
                finite(bubble.velocity.y),
            ],
        };
        count += 1;
    }
    (output, count)
}

pub(crate) const fn vertex_layout() -> wgpu::VertexBufferLayout<'static> {
    const ATTRIBUTES: [wgpu::VertexAttribute; 4] = wgpu::vertex_attr_array![
        0 => Float32x4,
        1 => Float32x4,
        2 => Float32x4,
        3 => Float32x4
    ];
    wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<DensityInstance>() as wgpu::BufferAddress,
        step_mode: wgpu::VertexStepMode::Instance,
        attributes: &ATTRIBUTES,
    }
}

fn finite(value: f32) -> f32 {
    if value.is_finite() { value } else { 0.0 }
}

fn bounded(value: f32, minimum: f32, maximum: f32, fallback: f32) -> f32 {
    if value.is_finite() {
        value.clamp(minimum, maximum)
    } else {
        fallback
    }
}

#[cfg(test)]
mod tests {
    use std::mem::size_of;

    use super::*;

    #[test]
    fn density_instance_is_four_aligned_vec4s() {
        assert_eq!(size_of::<DensityInstance>(), 64);
    }

    #[test]
    fn particle_packet_keeps_density_thickness_emission_and_pigment_distinct() {
        let mut state = LiquidRenderState {
            particle_count: 1,
            ..LiquidRenderState::default()
        };
        state.particles[0] = crate::ParticleRenderState {
            material_coordinate: glam::Vec2::new(-0.22, 0.31),
            velocity: glam::Vec2::new(0.14, -0.07),
            density: 0.91,
            optical_thickness: 1.23,
            emission: 0.37,
            pigment: 0.68,
            main_component: true,
            ..crate::ParticleRenderState::default()
        };
        let (instances, count) = pack_particles_for_material(&state, false);

        assert_eq!(count, 1);
        assert_eq!(instances[0].geometry_b[2], 0.91);
        assert_eq!(instances[0].geometry_b[3], 1.23);
        assert_eq!(instances[0].material[0], 0.37);
        assert_eq!(instances[0].material[1], 0.68);
        assert_eq!(instances[0].material[2], 1.0);
        assert_eq!(instances[0].material_flow, [-0.22, 0.31, 0.14, -0.07]);
    }

    #[test]
    fn bubble_packet_uses_the_same_density_material_field() {
        let mut state = LiquidRenderState {
            particle_count: 2,
            bubble_count: 1,
            ..LiquidRenderState::default()
        };
        state.bubbles[0] = crate::BubbleRenderState {
            position: glam::Vec2::new(0.2, -0.1),
            radius: 0.041,
            opacity: 0.72,
            emission: 0.36,
            pigment: 0.61,
            material_coordinate: glam::Vec2::new(0.08, -0.17),
            optical_thickness: 1.14,
            ..crate::BubbleRenderState::default()
        };
        let (instances, particle_count) = pack_particles_for_material(&state, true);
        assert_eq!(particle_count, 3);
        assert_eq!(instances[2].geometry_a, [0.2, -0.1, 0.041, 0.041]);
        assert_eq!(instances[2].geometry_b, [1.0, 0.0, 0.92, 1.14]);
        assert_eq!(instances[2].material[..2], [0.36, 0.61]);
        assert_eq!(instances[2].material_flow[..2], [0.08, -0.17]);

        let (safe_instances, safe_count) = pack_particles_for_material(&state, false);
        assert_eq!(safe_count, 3);
        assert_eq!(safe_instances[2].geometry_b[3], 0.74);
        assert_eq!(safe_instances[2].material_flow[..2], [0.0, 0.0]);
    }
}
