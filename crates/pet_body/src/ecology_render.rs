use bytemuck::{Pod, Zeroable};
use glam::{Vec2, Vec3, Vec4};
use pet_ecology::{EcologyState, ObjectKind, ObjectLifecycle, stored_orb_hover_offset};
use wgpu::util::DeviceExt;

use crate::DenVisualTuning;

const MAX_ECOLOGY_INSTANCES: usize = 9;
const STORED_ORB_HOVER_FREQUENCY: f32 = 1.15;
const STORED_ORB_HOVER_MAX_SPEED_PX: f32 = 0.85;
const STORED_ORB_HOVER_MAX_ACCELERATION_PX: f32 = 1.20;

fn ecology_source_over_blend(premultiplied_output: bool) -> wgpu::BlendState {
    wgpu::BlendState {
        color: wgpu::BlendComponent {
            src_factor: if premultiplied_output {
                wgpu::BlendFactor::One
            } else {
                wgpu::BlendFactor::SrcAlpha
            },
            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
            operation: wgpu::BlendOperation::Add,
        },
        alpha: wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::One,
            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
            operation: wgpu::BlendOperation::Add,
        },
    }
}

/// Region whose captured pixels must remain the last known clean desktop
/// while the den itself is visible. This prevents the topmost transparent
/// overlay from becoming its own optical input without hiding the Pet from
/// screenshots or relying on display-affinity support.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EcologyCaptureExclusion {
    pub center_normalized: Vec2,
    pub radius_pixels: f32,
    pub feather_pixels: f32,
}

impl EcologyCaptureExclusion {
    #[must_use]
    pub fn new(center_normalized: Vec2, radius_pixels: f32, feather_pixels: f32) -> Option<Self> {
        let value = Self {
            center_normalized,
            radius_pixels,
            feather_pixels,
        };
        (center_normalized.is_finite()
            && radius_pixels.is_finite()
            && feather_pixels.is_finite()
            && radius_pixels > 0.0
            && feather_pixels >= 0.0)
            .then_some(value)
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Pod, Zeroable)]
struct EcologyInstance {
    center_radius: [f32; 4],
    color: [f32; 4],
    material: [f32; 4],
    den_surface: [f32; 4],
    den_optics: [f32; 4],
    den_particles: [f32; 4],
    den_noise: [f32; 4],
    den_material: [f32; 4],
    den_mask: [f32; 4],
}

pub struct EcologyRenderer {
    pipeline: wgpu::RenderPipeline,
    reference_background_pipeline: wgpu::RenderPipeline,
    instance_buffer: wgpu::Buffer,
    background_bind_group_layout: wgpu::BindGroupLayout,
    background_bind_group: wgpu::BindGroup,
    background_texture: wgpu::Texture,
    background_sampler: wgpu::Sampler,
    background_size: (u32, u32),
    background_bytes_per_row: u32,
    smoothed_background_bgra: Vec<u8>,
    background_freshness: f32,
    den_activity: f32,
    den_activity_integral_seconds: f32,
    last_time_seconds: Option<f32>,
    stored_orb_hover_offset_px: Vec2,
    stored_orb_hover_velocity_px: Vec2,
    den_tuning: DenVisualTuning,
    prepared_den_count: u32,
    prepared_count: u32,
}

impl EcologyRenderer {
    #[must_use]
    pub fn new(
        device: &wgpu::Device,
        target_format: wgpu::TextureFormat,
        premultiplied_output: bool,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("living desktop ecology object shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("ecology_objects.wgsl").into()),
        });
        let background_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("living desktop ecology background layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
            });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("living desktop ecology object pipeline layout"),
            bind_group_layouts: &[&background_bind_group_layout],
            push_constant_ranges: &[],
        });
        let source_over = ecology_source_over_blend(premultiplied_output);
        let output_constants = [(
            "PREMULTIPLIED_OUTPUT",
            if premultiplied_output { 1.0 } else { 0.0 },
        )];
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("living desktop ecology object pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vertex_main"),
                compilation_options: wgpu::PipelineCompilationOptions {
                    constants: &output_constants,
                    ..Default::default()
                },
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<EcologyInstance>() as u64,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &wgpu::vertex_attr_array![
                        0 => Float32x4,
                        1 => Float32x4,
                        2 => Float32x4,
                        3 => Float32x4,
                        4 => Float32x4,
                        5 => Float32x4,
                        6 => Float32x4,
                        7 => Float32x4,
                        8 => Float32x4
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
                compilation_options: wgpu::PipelineCompilationOptions {
                    constants: &output_constants,
                    ..Default::default()
                },
                targets: &[Some(wgpu::ColorTargetState {
                    format: target_format,
                    blend: Some(source_over),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview: None,
            cache: None,
        });
        let reference_background_pipeline =
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("living desktop ecology reference background pipeline"),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("reference_background_vertex"),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    buffers: &[],
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
                    entry_point: Some("reference_background_fragment"),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: target_format,
                        blend: None,
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
        let background_texture = create_ecology_background_texture(device, 1, 1);
        let background_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("living desktop ecology background sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        let background_bind_group = create_ecology_background_bind_group(
            device,
            &background_bind_group_layout,
            &background_texture,
            &background_sampler,
        );
        Self {
            pipeline,
            reference_background_pipeline,
            instance_buffer,
            background_bind_group_layout,
            background_bind_group,
            background_texture,
            background_sampler,
            background_size: (1, 1),
            background_bytes_per_row: 4,
            smoothed_background_bgra: Vec::new(),
            background_freshness: 0.0,
            den_activity: 0.0,
            den_activity_integral_seconds: 0.0,
            last_time_seconds: None,
            stored_orb_hover_offset_px: Vec2::ZERO,
            stored_orb_hover_velocity_px: Vec2::ZERO,
            den_tuning: DenVisualTuning::default(),
            prepared_den_count: 0,
            prepared_count: 0,
        }
    }

    pub fn set_den_tuning(&mut self, tuning: DenVisualTuning) {
        self.den_tuning = tuning;
    }

    /// Uploads the latest top-down BGRA8 crop captured behind the transparent
    /// overlay. Pixels remain process-local and are never persisted.
    pub fn update_background(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        width: u32,
        height: u32,
        bytes_per_row: u32,
        bgra8: &[u8],
    ) -> bool {
        self.update_background_excluding(device, queue, width, height, bytes_per_row, bgra8, None)
    }

    /// Uploads a desktop frame while retaining the clean history underneath a
    /// visible den. The first frame always seeds the history verbatim; callers
    /// delay den rendering until that seed has arrived.
    pub fn update_background_excluding(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        width: u32,
        height: u32,
        bytes_per_row: u32,
        bgra8: &[u8],
        exclusion: Option<EcologyCaptureExclusion>,
    ) -> bool {
        if width == 0
            || height == 0
            || bytes_per_row < width.saturating_mul(4)
            || bgra8.len() < bytes_per_row as usize * height as usize
        {
            return false;
        }
        if self.background_size != (width, height) {
            self.background_texture = create_ecology_background_texture(device, width, height);
            self.background_size = (width, height);
            self.background_bind_group = create_ecology_background_bind_group(
                device,
                &self.background_bind_group_layout,
                &self.background_texture,
                &self.background_sampler,
            );
        }
        let expected_length = bytes_per_row as usize * height as usize;
        if self.background_size != (width, height)
            || self.background_bytes_per_row != bytes_per_row
            || self.smoothed_background_bgra.len() != expected_length
        {
            self.smoothed_background_bgra = bgra8[..expected_length].to_vec();
        } else {
            temporally_stabilize_background(
                &mut self.smoothed_background_bgra,
                bgra8,
                width,
                height,
                bytes_per_row,
                exclusion,
            );
        }
        self.background_bytes_per_row = bytes_per_row;
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.background_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &self.smoothed_background_bgra,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(bytes_per_row),
                rows_per_image: Some(height),
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        self.background_freshness = 1.0;
        true
    }

    pub fn set_background_freshness(&mut self, freshness: f32) {
        if freshness.is_finite() {
            self.background_freshness = freshness.clamp(0.0, 1.0);
        }
    }

    /// Draws the exact texture that the den optics sample. This is intended for
    /// opaque review surfaces such as Pet Lab, where it guarantees that an A/B
    /// refraction test bends the pixels visibly present beneath the den. The
    /// production transparent desktop overlay deliberately does not call it.
    pub fn render_reference_background(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
    ) {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("living desktop ecology reference background pass"),
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
        pass.set_pipeline(&self.reference_background_pipeline);
        pass.set_bind_group(0, &self.background_bind_group, &[]);
        pass.draw(0..3, 0..1);
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
        self.prepare(queue, state, desktop_aspect, time_seconds);
        self.render_prepared_den(encoder, target);
        self.render_prepared_objects(encoder, target);
    }

    /// Uploads one deterministic ecology instance list. The den occupies the
    /// first range and portable objects the second, allowing the body compositor
    /// to sit between them without duplicating simulation or buffer writes.
    pub fn prepare(
        &mut self,
        queue: &wgpu::Queue,
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
                object.kind == ObjectKind::Orb && orb_drives_den_activity(object.lifecycle)
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
        let response_seconds = den_activity_response_seconds(
            self.den_activity,
            target_activity,
            self.den_tuning.orb_attack_seconds,
            self.den_tuning.orb_release_seconds,
        );
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
        // The den's material may move inward, but its silhouette is a fixed
        // landmark. Scaling the instance with a breathing oscillator made the
        // transparent feather sweep across desktop pixels and read as an
        // alternating dark/light rim even after height displacement was
        // removed from the mask.
        let den_scale = stable_den_scale(state.den.size_scale);
        let den_color = Vec4::new(
            self.background_freshness,
            self.den_tuning.ripple_inward_speed,
            self.den_tuning.orb_speedup_fraction,
            self.den_activity_integral_seconds,
        );
        instances[count] = EcologyInstance {
            center_radius: [
                state.den.anchor.x * 2.0 - 1.0,
                1.0 - state.den.anchor.y * 2.0,
                den_radius_y / aspect * den_scale,
                den_radius_y * den_scale,
            ],
            color: den_color.to_array(),
            material: [1.0, self.den_activity, state.den.familiarity, time_seconds],
            den_surface: [
                self.den_tuning.noise_size,
                self.den_tuning.noise_strength,
                self.den_tuning.inward_speed,
                self.den_tuning.ripple_strength,
            ],
            den_optics: [
                self.den_tuning.displacement_strength,
                self.den_tuning.refraction_strength,
                self.den_tuning.dispersion_strength,
                self.den_tuning.glow_strength,
            ],
            den_particles: [
                self.den_tuning.ripple_opacity,
                self.den_tuning.particle_brightness,
                f32::from(self.den_tuning.particle_count),
                self.den_tuning.displacement_radius,
            ],
            den_noise: [
                self.den_tuning.noise_detail_scale,
                self.den_tuning.noise_detail_mix,
                self.den_tuning.noise_warp,
                self.den_tuning.noise_band_width,
            ],
            den_material: [
                self.den_tuning.refraction_opacity,
                self.den_tuning.tint_strength,
                self.den_tuning.caustic_strength,
                self.den_tuning.displacement_noise_mix,
            ],
            den_mask: [
                self.den_tuning.center_mask_radius,
                self.den_tuning.center_mask_feather,
                self.den_tuning.center_mask_opacity,
                self.den_tuning.displacement_blur,
            ],
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
                den_surface: [0.0; 4],
                den_optics: [0.0; 4],
                den_particles: [0.0; 4],
                den_noise: [0.0; 4],
                den_material: [0.0; 4],
                den_mask: [0.0; 4],
            };
            count += 1;
        }
        if count == 0 {
            self.prepared_den_count = 0;
            self.prepared_count = 0;
            return;
        }
        queue.write_buffer(
            &self.instance_buffer,
            0,
            bytemuck::cast_slice(&instances[..count]),
        );
        self.prepared_den_count = 1;
        self.prepared_count = count as u32;
    }

    pub fn render_prepared_den(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
    ) {
        self.render_prepared_range(encoder, target, 0..self.prepared_den_count);
    }

    pub fn render_prepared_objects(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
    ) {
        self.render_prepared_range(
            encoder,
            target,
            self.prepared_den_count..self.prepared_count,
        );
    }

    fn render_prepared_range(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        instances: std::ops::Range<u32>,
    ) {
        if instances.is_empty() {
            return;
        }
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
        pass.set_bind_group(0, &self.background_bind_group, &[]);
        pass.set_vertex_buffer(0, self.instance_buffer.slice(..));
        pass.draw(0..6, instances);
    }
}

fn temporally_stabilize_background(
    history: &mut [u8],
    incoming: &[u8],
    width: u32,
    height: u32,
    bytes_per_row: u32,
    exclusion: Option<EcologyCaptureExclusion>,
) {
    // A one-pole 1:1 causal blend removes capture timing jitter without the
    // visible multi-frame drag of the former 3:1 history at 30 Hz. Capture now
    // targets 60 Hz; the procedural optical field remains render-rate smooth.
    // Only live BGRA pixels are touched; row padding remains irrelevant.
    let exclusion = exclusion.map(|region| {
        let center = region.center_normalized.clamp(Vec2::ZERO, Vec2::ONE)
            * Vec2::new(width as f32, height as f32);
        let radius = region.radius_pixels.max(0.0);
        let outer_radius = (radius + region.feather_pixels).max(radius + 0.001);
        (center, radius, outer_radius)
    });
    for row in 0..height as usize {
        let row_start = row * bytes_per_row as usize;
        let live_row_end = row_start + width as usize * 4;
        let history_row = &mut history[row_start..live_row_end];
        let incoming_row = &incoming[row_start..live_row_end];
        let Some((center, radius, outer_radius)) = exclusion else {
            blend_background_bytes(history_row, incoming_row);
            continue;
        };

        let delta_y = row as f32 + 0.5 - center.y;
        if delta_y.abs() >= outer_radius {
            blend_background_bytes(history_row, incoming_row);
            continue;
        }
        let half_span = (outer_radius * outer_radius - delta_y * delta_y).sqrt();
        let first_masked =
            ((center.x - half_span - 0.5).floor() as i32).clamp(0, width as i32) as usize;
        let masked_end =
            ((center.x + half_span - 0.5).ceil() as i32 + 1).clamp(0, width as i32) as usize;
        blend_background_bytes(
            &mut history_row[..first_masked * 4],
            &incoming_row[..first_masked * 4],
        );
        blend_background_bytes(
            &mut history_row[masked_end * 4..],
            &incoming_row[masked_end * 4..],
        );

        for column in first_masked..masked_end {
            let delta_x = column as f32 + 0.5 - center.x;
            let distance_squared = delta_x * delta_x + delta_y * delta_y;
            if distance_squared <= radius * radius {
                continue;
            }
            let keep_history =
                1.0 - smoothstep(radius, outer_radius, distance_squared.max(0.0).sqrt());
            let index = column * 4;
            for channel in 0..4 {
                let previous = history_row[index + channel];
                let next = averaged_background_channel(previous, incoming_row[index + channel]);
                history_row[index + channel] = (f32::from(next) * (1.0 - keep_history)
                    + f32::from(previous) * keep_history)
                    .round()
                    .clamp(0.0, 255.0) as u8;
            }
        }
    }
}

#[inline]
fn averaged_background_channel(previous: u8, incoming: u8) -> u8 {
    // Preserve the existing causal blend's half-up behavior. The extra unit
    // prevents a dark one-value fixed point while still saturating cleanly.
    ((u16::from(previous) + u16::from(incoming)) / 2 + 1).min(255) as u8
}

#[inline]
fn blend_background_bytes(history: &mut [u8], incoming: &[u8]) {
    debug_assert_eq!(history.len(), incoming.len());
    for (previous, incoming) in history.iter_mut().zip(incoming) {
        *previous = averaged_background_channel(*previous, *incoming);
    }
}

fn create_ecology_background_texture(
    device: &wgpu::Device,
    width: u32,
    height: u32,
) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("living desktop ecology background texture"),
        size: wgpu::Extent3d {
            width: width.max(1),
            height: height.max(1),
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Bgra8UnormSrgb,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    })
}

fn create_ecology_background_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    texture: &wgpu::Texture,
    sampler: &wgpu::Sampler,
) -> wgpu::BindGroup {
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("living desktop ecology background bind group"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
        ],
    })
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

fn stable_den_scale(size_scale: f32) -> f32 {
    if size_scale.is_finite() {
        size_scale.clamp(0.50, 2.0)
    } else {
        1.0
    }
}

fn den_activity_response_seconds(
    current: f32,
    target: f32,
    attack_seconds: f32,
    release_seconds: f32,
) -> f32 {
    if target > current {
        attack_seconds
    } else {
        release_seconds
    }
}

fn orb_drives_den_activity(lifecycle: ObjectLifecycle) -> bool {
    !matches!(
        lifecycle,
        ObjectLifecycle::StoredInDen | ObjectLifecycle::Consumed
    )
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
        EcologyCaptureExclusion, STORED_ORB_HOVER_MAX_ACCELERATION_PX,
        STORED_ORB_HOVER_MAX_SPEED_PX, advance_stored_orb_hover, den_activity_response_seconds,
        ecology_source_over_blend, orb_drives_den_activity, stable_den_scale,
        temporally_stabilize_background,
    };
    use pet_ecology::ObjectLifecycle;

    fn minimum_reconstruction_alpha(background: glam::Vec3, target: glam::Vec3) -> f32 {
        let epsilon = glam::Vec3::splat(0.0001);
        let darker = ((background - target) / background.max(epsilon)).max(glam::Vec3::ZERO);
        let lighter = ((target - background) / (glam::Vec3::ONE - background).max(epsilon))
            .max(glam::Vec3::ZERO);
        darker
            .max_element()
            .max(lighter.max_element())
            .clamp(0.0, 1.0)
    }

    #[test]
    fn ecology_shader_parses_and_validates() {
        let source = include_str!("ecology_objects.wgsl");
        assert!(source.contains("fn den_concentric_ripple"));
        assert!(source.contains("fn den_inward_ring"));
        assert!(source.contains("fn den_converging_energy_noise"));
        assert!(source.contains("vec2<f32>(-1.50, -1.50)"));
        assert!(source.contains("let width_fraction = saturate(noise.w * 0.01)"));
        assert!(source.contains("let angular_cells = exp2(mix(3.321928, -1.514573"));
        assert!(source.contains("let sample_point = direction * angular_cells"));
        assert!(source.contains("return den_energy_sample(sample_point, direction, noise)"));
        assert!(!source.contains("side_a"));
        assert!(!source.contains("side_b"));
        assert!(!source.contains("den_noise_shape"));
        assert!(!source.contains("isotropic_filter"));
        assert!(!source.contains("broad_field"));
        assert!(source.contains("override PREMULTIPLIED_OUTPUT"));
        assert!(source.contains("let ring_radius = mix(maximum_radius * 1.35"));
        assert!(source.contains("let ripple_crest = concentric"));
        assert!(!source.contains("sin((warped_radius"));
        assert!(source.contains("let travelling_mask = outer_mask * inward_sink"));
        assert!(!source.contains("stable_interior_mask"));
        assert!(!source.contains("dynamic_field_mask"));
        assert!(!source.contains("smoothstep(0.72, 1.12, r)"));
        assert!(source.contains("let displaced_local = local + gradient * displacement_strength"));
        assert!(!source.contains("* optics.x * field_mask"));
        assert!(source.contains("let optical_edge_fade = smoothstep(0.0, 1.0, outer_mask)"));
        assert!(source.contains("let optical_carrier = 0.28 + saturate(abs(height)) * 0.72"));
        assert!(!source.contains("let optical_shift_local = normal * height"));
        assert!(source.contains("* optics.x * optical_edge_fade"));
        assert!(source.contains("* optical_edge_fade;"));
        assert!(source.contains("let refract_alpha = min(field_mask"));
        assert!(source.contains("let reactive_activity = activity * 0.22"));
        assert!(source.contains("let orb_speedup_fraction = saturate(input.color.b)"));
        assert!(source.contains("activity_integral * 0.34 * orb_speedup_fraction"));
        assert!(source.contains("activity_integral * 0.42 * orb_speedup_fraction"));
        assert!(!source.contains("time * mix(0.34"));
        assert!(!source.contains("time * input.color.g * mix"));
        assert!(
            source.contains("let particle_time = time + activity_integral * orb_speedup_fraction")
        );
        assert!(source.contains("let refraction_opacity = mix(0.052, 0.58, capture_freshness)"));
        assert!(source.contains("fn minimum_reconstruction_alpha"));
        assert!(source.contains("let desired_background = mix(background_reference, refracted"));
        assert!(source.contains("let reconstructed_source = max("));
        assert!(source.contains("return vec4<f32>(output_rgb, output_alpha)"));
        assert!(!source.contains("spectral_carrier"));
        assert!(!source.contains("dispersion_tint"));
        assert!(!source.contains("desktop_safe_refracted"));
        assert!(source.contains("let rgb = max(optical_rgb, background_reference * alpha)"));
        assert!(!source.contains("displacement_radius * 0.78 + edge_displacement"));
        assert!(source.contains("displacement_radius * 0.20"));
        assert!(source.contains("displacement_radius * 0.27"));
        let module = naga::front::wgsl::parse_str(source).unwrap();
        Validator::new(ValidationFlags::all(), Capabilities::all())
            .validate(&module)
            .unwrap();
    }

    #[test]
    fn ecology_blend_matches_the_native_alpha_convention() {
        let premultiplied = ecology_source_over_blend(true);
        assert_eq!(premultiplied.color.src_factor, wgpu::BlendFactor::One);
        assert_eq!(
            premultiplied.color.dst_factor,
            wgpu::BlendFactor::OneMinusSrcAlpha
        );

        let straight = ecology_source_over_blend(false);
        assert_eq!(straight.color.src_factor, wgpu::BlendFactor::SrcAlpha);
        assert_eq!(
            straight.color.dst_factor,
            wgpu::BlendFactor::OneMinusSrcAlpha
        );
    }

    #[test]
    fn minimum_alpha_reconstruction_is_empty_for_an_unchanged_desktop_pixel() {
        let backdrops = [
            glam::Vec3::ZERO,
            glam::Vec3::splat(0.02),
            glam::Vec3::new(0.12, 0.48, 0.91),
            glam::Vec3::new(0.94, 0.72, 0.30),
            glam::Vec3::ONE,
        ];
        for backdrop in backdrops {
            assert_eq!(minimum_reconstruction_alpha(backdrop, backdrop), 0.0);
        }
    }

    #[test]
    fn minimum_alpha_source_over_exactly_reconstructs_displaced_rgb_edges() {
        let cases = [
            (
                glam::Vec3::new(0.40, 0.41, 0.42),
                glam::Vec3::new(0.72, 0.18, 0.55),
            ),
            (
                glam::Vec3::new(0.92, 0.12, 0.48),
                glam::Vec3::new(0.08, 0.74, 0.22),
            ),
            (glam::Vec3::ZERO, glam::Vec3::new(0.20, 0.60, 1.0)),
            (glam::Vec3::ONE, glam::Vec3::new(0.80, 0.25, 0.10)),
        ];
        for (background, target) in cases {
            let alpha = minimum_reconstruction_alpha(background, target);
            let source = (target - background * (1.0 - alpha)).max(glam::Vec3::ZERO);
            let composed = source + background * (1.0 - alpha);
            assert!((composed - target).abs().max_element() < 1.0e-5);
            assert!(source.cmpge(glam::Vec3::ZERO).all());
            assert!(source.cmple(glam::Vec3::splat(alpha + 1.0e-5)).all());
        }
    }

    #[test]
    fn orb_reactivity_is_a_subtle_bounded_lift_instead_of_a_second_speed_mode() {
        let reactive_activity = 0.22_f32;
        let speedup = 0.15_f32;
        let idle_noise_rate = 0.34_f32;
        let active_noise_rate = idle_noise_rate * (1.0 + speedup);
        let idle_ripple_rate = 0.42_f32;
        let active_ripple_rate = idle_ripple_rate * (1.0 + speedup);
        let active_particle_rate = 1.0_f32 + speedup;
        let idle_carrier = 0.115_f32;
        let active_carrier = 0.115 + (0.150 - 0.115) * reactive_activity;

        assert!(((active_noise_rate / idle_noise_rate) - 1.15).abs() < 1.0e-6);
        assert!(((active_ripple_rate / idle_ripple_rate) - 1.15).abs() < 1.0e-6);
        assert!((active_particle_rate - 1.15).abs() < 1.0e-6);
        assert!((1.05..=1.10).contains(&(active_carrier / idle_carrier)));
        assert_eq!(den_activity_response_seconds(0.0, 1.0, 0.55, 1.20), 0.55);
        assert_eq!(den_activity_response_seconds(1.0, 0.0, 0.55, 1.20), 1.20);
        let first_attack_step = 1.0 - (-(1.0_f32 / 60.0) / 0.55).exp();
        assert!(first_attack_step < 0.04);

        // The phase itself cannot depend on the instantaneous activity value:
        // toggling activity at a large uptime therefore has exactly zero jump.
        let uptime = 18_000.0_f32;
        let integrated_activity = 4.25_f32;
        let before = uptime * idle_ripple_rate + integrated_activity * idle_ripple_rate * speedup;
        let after = uptime * idle_ripple_rate + integrated_activity * idle_ripple_rate * speedup;
        assert_eq!(before, after);
    }

    #[test]
    fn stored_or_consumed_orb_releases_den_activity() {
        assert!(orb_drives_den_activity(ObjectLifecycle::Free));
        assert!(orb_drives_den_activity(ObjectLifecycle::Sleeping));
        assert!(orb_drives_den_activity(ObjectLifecycle::GrabbedByUser));
        assert!(orb_drives_den_activity(ObjectLifecycle::CarriedByPet));
        assert!(!orb_drives_den_activity(ObjectLifecycle::StoredInDen));
        assert!(!orb_drives_den_activity(ObjectLifecycle::Consumed));
    }

    #[test]
    fn primary_ripple_crest_moves_inward_and_is_absorbed_at_twenty_percent() {
        let crest_radius = |progress: f32| 1.35 + (0.20 - 1.35) * progress;
        assert!(crest_radius(0.68) < crest_radius(0.12));
        assert!((crest_radius(1.0) - 0.20).abs() < 1.0e-6);

        let sink = |radius: f32, maximum: f32| {
            let t = ((radius - maximum * 0.20) / (maximum * 0.07)).clamp(0.0, 1.0);
            t * t * (3.0 - 2.0 * t)
        };
        assert_eq!(sink(0.20, 1.0), 0.0);
        assert!(sink(0.235, 1.0) > 0.0);
        assert_eq!(sink(0.27, 1.0), 1.0);
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

    #[test]
    fn den_outer_scale_is_authored_and_never_time_modulated() {
        assert_eq!(stable_den_scale(0.82), 0.82);
        assert_eq!(stable_den_scale(1.17), 1.17);
        assert_eq!(stable_den_scale(f32::NAN), 1.0);
        assert_eq!(stable_den_scale(4.0), 2.0);
    }

    #[test]
    fn captured_background_changes_are_temporally_bounded_and_converge() {
        let mut history = vec![0_u8; 512];
        let incoming = vec![255_u8; 512];
        temporally_stabilize_background(&mut history, &incoming, 2, 2, 256, None);
        assert!(history[..8].iter().all(|channel| *channel == 128));
        assert!(history[256..264].iter().all(|channel| *channel == 128));
        assert!(history[8..256].iter().all(|channel| *channel == 0));
        assert!(history[264..].iter().all(|channel| *channel == 0));
        for _ in 0..24 {
            temporally_stabilize_background(&mut history, &incoming, 2, 2, 256, None);
        }
        assert!(history[..8].iter().all(|channel| *channel >= 253));
        assert!(history[256..264].iter().all(|channel| *channel >= 253));
    }

    #[test]
    fn den_capture_exclusion_keeps_clean_center_while_outside_updates() {
        let mut history = vec![10_u8; 4 * 4 * 4];
        let incoming = vec![250_u8; 4 * 4 * 4];
        let exclusion = EcologyCaptureExclusion::new(Vec2::splat(0.5), 1.2, 0.4);

        temporally_stabilize_background(&mut history, &incoming, 4, 4, 16, exclusion);

        let center_pixel = (1 * 4 + 1) * 4;
        let corner_pixel = 0;
        assert!(
            history[center_pixel..center_pixel + 4]
                .iter()
                .all(|channel| *channel == 10)
        );
        assert!(
            history[corner_pixel..corner_pixel + 4]
                .iter()
                .all(|channel| *channel == 131)
        );
    }
}
