use bytemuck::{Pod, Zeroable};
use glam::{Vec2, Vec3, Vec4};
use pet_ecology::{EcologyState, ObjectKind, ObjectLifecycle, stored_orb_hover_offset};
use wgpu::util::DeviceExt;

const MAX_ECOLOGY_INSTANCES: usize = 9;
const STORED_ORB_HOVER_FREQUENCY: f32 = 1.15;
const STORED_ORB_HOVER_MAX_SPEED_PX: f32 = 0.85;
const STORED_ORB_HOVER_MAX_ACCELERATION_PX: f32 = 1.20;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Pod, Zeroable)]
struct EcologyInstance {
    center_radius: [f32; 4],
    color: [f32; 4],
    material: [f32; 4],
}

pub struct EcologyRenderer {
    pipeline: wgpu::RenderPipeline,
    instance_buffer: wgpu::Buffer,
    den_activity: f32,
    den_activity_integral_seconds: f32,
    last_time_seconds: Option<f32>,
    stored_orb_hover_offset_px: Vec2,
    stored_orb_hover_velocity_px: Vec2,
}

impl EcologyRenderer {
    #[must_use]
    pub fn new(device: &wgpu::Device, target_format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("living desktop ecology object shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("ecology_objects.wgsl").into()),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("living desktop ecology object pipeline layout"),
            bind_group_layouts: &[],
            push_constant_ranges: &[],
        });
        let premultiplied = wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                operation: wgpu::BlendOperation::Add,
            },
        };
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("living desktop ecology object pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vertex_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<EcologyInstance>() as u64,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &wgpu::vertex_attr_array![
                        0 => Float32x4,
                        1 => Float32x4,
                        2 => Float32x4
                    ],
                }],
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fragment_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: target_format,
                    blend: Some(premultiplied),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview: None,
            cache: None,
        });
        let instance_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("living desktop ecology object instances"),
            contents: bytemuck::cast_slice(&[EcologyInstance::default(); MAX_ECOLOGY_INSTANCES]),
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        });
        Self {
            pipeline,
            instance_buffer,
            den_activity: 0.0,
            den_activity_integral_seconds: 0.0,
            last_time_seconds: None,
            stored_orb_hover_offset_px: Vec2::ZERO,
            stored_orb_hover_velocity_px: Vec2::ZERO,
        }
    }

    pub fn render(
        &mut self,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        state: &EcologyState,
        desktop_aspect: f32,
        time_seconds: f32,
    ) {
        let aspect = if desktop_aspect.is_finite() {
            desktop_aspect.clamp(0.25, 8.0)
        } else {
            16.0 / 9.0
        };
        let time_seconds = if time_seconds.is_finite() {
            time_seconds
        } else {
            self.last_time_seconds.unwrap_or(0.0)
        };
        let dt = self
            .last_time_seconds
            .map_or(0.0, |last| (time_seconds - last).clamp(0.0, 0.05));
        self.last_time_seconds = Some(time_seconds);

        let nearest_orb_distance_px = state
            .objects
            .iter()
            .filter(|object| {
                object.kind == ObjectKind::Orb && object.lifecycle != ObjectLifecycle::Consumed
            })
            .map(|orb| {
                let delta = Vec2::new(
                    (orb.position.x - state.den.anchor.x) * aspect,
                    orb.position.y - state.den.anchor.y,
                );
                delta.length() * pet_ecology::REFERENCE_DESKTOP_HEIGHT_PX
            })
            .fold(f32::INFINITY, f32::min);
        let target_activity = if nearest_orb_distance_px.is_finite() {
            1.0 - smoothstep(90.0, 180.0, nearest_orb_distance_px)
        } else {
            0.0
        };
        let response_seconds = 0.20;
        let response_alpha = 1.0 - (-dt / response_seconds).exp();
        self.den_activity += (target_activity - self.den_activity) * response_alpha;
        self.den_activity = self.den_activity.clamp(0.0, 1.0);
        self.den_activity_integral_seconds += self.den_activity * dt;

        let stored_hover_target = state
            .objects
            .iter()
            .find(|object| {
                object.kind == ObjectKind::Orb && object.lifecycle == ObjectLifecycle::StoredInDen
            })
            .map_or(Vec2::ZERO, |orb| {
                let offset = stored_orb_hover_offset(orb.id, time_seconds, aspect);
                Vec2::new(offset.x * aspect, offset.y) * pet_ecology::REFERENCE_DESKTOP_HEIGHT_PX
            });
        advance_stored_orb_hover(
            &mut self.stored_orb_hover_offset_px,
            &mut self.stored_orb_hover_velocity_px,
            stored_hover_target,
            dt,
        );
        let stored_hover_offset = Vec2::new(
            self.stored_orb_hover_offset_px.x / (pet_ecology::REFERENCE_DESKTOP_HEIGHT_PX * aspect),
            self.stored_orb_hover_offset_px.y / pet_ecology::REFERENCE_DESKTOP_HEIGHT_PX,
        );

        let mut instances = [EcologyInstance::default(); MAX_ECOLOGY_INSTANCES];
        let mut count = 0_usize;

        let den_radius_y = 105.0 / pet_ecology::REFERENCE_DESKTOP_HEIGHT_PX * 2.0;
        let den_color = Vec4::new(0.88, 0.95, 1.0, self.den_activity_integral_seconds);
        instances[count] = EcologyInstance {
            center_radius: [
                state.den.anchor.x * 2.0 - 1.0,
                1.0 - state.den.anchor.y * 2.0,
                den_radius_y / aspect * state.den.size_scale,
                den_radius_y * state.den.size_scale,
            ],
            color: den_color.to_array(),
            material: [1.0, self.den_activity, state.den.familiarity, time_seconds],
        };
        count += 1;

        for object in &state.objects {
            if count >= MAX_ECOLOGY_INSTANCES || object.lifecycle == ObjectLifecycle::Consumed {
                continue;
            }
            let stored_in_den = object.lifecycle == ObjectLifecycle::StoredInDen;
            let den_distance_px = Vec2::new(
                (object.position.x - state.den.anchor.x) * aspect,
                object.position.y - state.den.anchor.y,
            )
            .length()
                * pet_ecology::REFERENCE_DESKTOP_HEIGHT_PX;
            let den_visual_blend = if object.kind == ObjectKind::Orb {
                if stored_in_den {
                    1.0
                } else {
                    1.0 - smoothstep(24.0, 105.0, den_distance_px)
                }
            } else {
                0.0
            };
            let den_scale = 1.0 - den_visual_blend * 0.14;
            let radius_y = object.radius_px_at_reference / pet_ecology::REFERENCE_DESKTOP_HEIGHT_PX
                * 2.0
                * den_scale;
            let rgb = hsv_to_rgb(object.hue, object.saturation, object.value);
            let hover_offset = if object.kind == ObjectKind::Orb {
                stored_hover_offset
            } else {
                Vec2::ZERO
            };
            instances[count] = EcologyInstance {
                center_radius: [
                    (object.position.x + hover_offset.x) * 2.0 - 1.0,
                    1.0 - (object.position.y + hover_offset.y) * 2.0,
                    radius_y / aspect,
                    radius_y,
                ],
                color: [rgb.x, rgb.y, rgb.z, 0.96 - den_visual_blend * 0.02],
                material: [
                    if object.kind == ObjectKind::Orb {
                        0.0
                    } else {
                        2.0
                    },
                    (object.glow * (1.0 + den_visual_blend * 0.10)).clamp(0.0, 1.0),
                    object.wear,
                    time_seconds + (object.id as u32) as f32 * 0.000_13,
                ],
            };
            count += 1;
        }
        if count == 0 {
            return;
        }
        queue.write_buffer(
            &self.instance_buffer,
            0,
            bytemuck::cast_slice(&instances[..count]),
        );
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("living desktop ecology object pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_vertex_buffer(0, self.instance_buffer.slice(..));
        pass.draw(0..6, 0..count as u32);
    }
}

fn advance_stored_orb_hover(
    offset_px: &mut Vec2,
    velocity_px: &mut Vec2,
    target_px: Vec2,
    dt: f32,
) {
    let dt = if dt.is_finite() {
        dt.clamp(0.0, 0.05)
    } else {
        0.0
    };
    if dt <= 0.0 {
        return;
    }
    let acceleration = ((target_px - *offset_px) * STORED_ORB_HOVER_FREQUENCY.powi(2)
        - *velocity_px * (2.0 * STORED_ORB_HOVER_FREQUENCY))
        .clamp_length_max(STORED_ORB_HOVER_MAX_ACCELERATION_PX);
    *velocity_px =
        (*velocity_px + acceleration * dt).clamp_length_max(STORED_ORB_HOVER_MAX_SPEED_PX);
    *offset_px += *velocity_px * dt;
    if target_px == Vec2::ZERO && offset_px.length() < 0.001 && velocity_px.length() < 0.001 {
        *offset_px = Vec2::ZERO;
        *velocity_px = Vec2::ZERO;
    }
}

fn smoothstep(edge0: f32, edge1: f32, value: f32) -> f32 {
    let t = ((value - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn hsv_to_rgb(hue: f32, saturation: f32, value: f32) -> Vec3 {
    let hue = hue.rem_euclid(1.0) * 6.0;
    let chroma = value * saturation;
    let x = chroma * (1.0 - (hue.rem_euclid(2.0) - 1.0).abs());
    let rgb = match hue as i32 {
        0 => Vec3::new(chroma, x, 0.0),
        1 => Vec3::new(x, chroma, 0.0),
        2 => Vec3::new(0.0, chroma, x),
        3 => Vec3::new(0.0, x, chroma),
        4 => Vec3::new(x, 0.0, chroma),
        _ => Vec3::new(chroma, 0.0, x),
    };
    rgb + Vec3::splat(value - chroma)
}

#[cfg(test)]
mod tests {
    use glam::Vec2;
    use naga::valid::{Capabilities, ValidationFlags, Validator};

    use super::{
        STORED_ORB_HOVER_MAX_ACCELERATION_PX, STORED_ORB_HOVER_MAX_SPEED_PX,
        advance_stored_orb_hover,
    };

    #[test]
    fn ecology_shader_parses_and_validates() {
        let module = naga::front::wgsl::parse_str(include_str!("ecology_objects.wgsl")).unwrap();
        Validator::new(ValidationFlags::all(), Capabilities::all())
            .validate(&module)
            .unwrap();
    }

    #[test]
    fn stored_orb_hover_enters_and_leaves_without_a_position_step() {
        let dt = 1.0 / 60.0;
        let mut offset = Vec2::ZERO;
        let mut velocity = Vec2::ZERO;
        let target = Vec2::new(2.4, -1.5);

        advance_stored_orb_hover(&mut offset, &mut velocity, target, dt);
        assert!(offset.length() <= STORED_ORB_HOVER_MAX_ACCELERATION_PX * dt * dt);

        for _ in 0..600 {
            let previous = offset;
            advance_stored_orb_hover(&mut offset, &mut velocity, target, dt);
            assert!((offset - previous).length() <= STORED_ORB_HOVER_MAX_SPEED_PX * dt + 1.0e-6);
        }

        let before_release = offset;
        advance_stored_orb_hover(&mut offset, &mut velocity, Vec2::ZERO, dt);
        assert!((offset - before_release).length() <= STORED_ORB_HOVER_MAX_SPEED_PX * dt + 1.0e-6);
        for _ in 0..1_200 {
            advance_stored_orb_hover(&mut offset, &mut velocity, Vec2::ZERO, dt);
        }
        assert!(offset.length() < 0.01);
        assert!(velocity.length() < 0.01);
    }
}
