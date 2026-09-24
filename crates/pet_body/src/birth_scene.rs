//! Authored capsule layers and cushion nest. Simulation stays in the liquid solver.
use bytemuck::{Pod, Zeroable};
#[path = "birth_vfx.rs"]
mod vfx;
use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Sprite {
    rect: [f32; 4],   // center / half extent, in pixels
    effect: [f32; 4], // time, sprite kind, reserved
    style: [f32; 4],  // angle, opacity, inverse viewport
    color: [f32; 4],
}

pub struct BirthScene {
    front: wgpu::RenderPipeline,
    behind: wgpu::RenderPipeline,
    additive: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    textures: Vec<wgpu::BindGroup>,
    buffer: wgpu::Buffer,
    den_front_buffer: wgpu::Buffer,
    den_reveal: [f32; 3],
}

const IMAGES: [&[u8]; 6] = [
    include_bytes!("../../../assets/nest/nest-pearl-v28.rgba"),
    include_bytes!("../../../assets/birth/lower_back.rgba"),
    include_bytes!("../../../assets/birth/upper_back.rgba"),
    include_bytes!("../../../assets/birth/orb.rgba"),
    include_bytes!("../../../assets/birth/lower_front.rgba"),
    include_bytes!("../../../assets/birth/upper_front.rgba"),
];

impl BirthScene {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("First Light authored sprites"),
            source: wgpu::ShaderSource::Wgsl(include_str!("birth_scene.wgsl").into()),
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("First Light texture"),
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
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[&layout],
            push_constant_ranges: &[],
        });
        let pipeline = |behind: bool, additive: bool| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("First Light sprite composition"),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vertex_main"),
                    compilation_options: Default::default(),
                    buffers: &[wgpu::VertexBufferLayout {
                        array_stride: 64,
                        step_mode: wgpu::VertexStepMode::Instance,
                        attributes: &wgpu::vertex_attr_array![0 => Float32x4, 1 => Float32x4, 2 => Float32x4, 3 => Float32x4],
                    }],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fragment_main"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend: Some(wgpu::BlendState {
                            color: wgpu::BlendComponent {
                                src_factor: if behind {
                                    wgpu::BlendFactor::OneMinusDstAlpha
                                } else {
                                    wgpu::BlendFactor::One
                                },
                                dst_factor: if behind || additive {
                                    wgpu::BlendFactor::One
                                } else {
                                    wgpu::BlendFactor::OneMinusSrcAlpha
                                },
                                operation: wgpu::BlendOperation::Add,
                            },
                            alpha: wgpu::BlendComponent {
                                src_factor: wgpu::BlendFactor::One,
                                dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                                operation: wgpu::BlendOperation::Add,
                            },
                        }),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: Default::default(),
                depth_stencil: None,
                multisample: Default::default(),
                multiview: None,
                cache: None,
            })
        };
        let front = pipeline(false, false);
        let behind = pipeline(true, false);
        let additive = pipeline(false, true);
        let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("First Light sprites"),
            contents: &[0; std::mem::size_of::<Sprite>() * 2048],
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        });
        let den_front_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("nest front cushion sprite"),
            contents: &[0; std::mem::size_of::<Sprite>()],
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        });
        Self {
            front,
            behind,
            additive,
            layout,
            textures: Vec::new(),
            buffer,
            den_front_buffer,
            den_reveal: [0.0, 1.0, 0.0],
        }
    }

    /// Translation, alpha and lower clipping edge shared by both nest layers.
    pub fn set_den_reveal(&mut self, offset_y: f32, alpha: f32, clip_bottom: f32) {
        self.den_reveal = [offset_y, alpha.clamp(0.0, 1.0), clip_bottom];
    }
    fn revealed_den(&self, mut sprite: Sprite) -> Sprite {
        sprite.rect[1] += self.den_reveal[0];
        sprite.style[1] *= self.den_reveal[1];
        sprite.effect[2] = self.den_reveal[2];
        sprite
    }

    fn upload(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        if !self.textures.is_empty() {
            return;
        }
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        for bytes in IMAGES {
            let width = u32::from_le_bytes(bytes[..4].try_into().unwrap());
            let height = u32::from_le_bytes(bytes[4..8].try_into().unwrap());
            let size = wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            };
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("authored RGBA layer"),
                size,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                &bytes[8..],
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(width * 4),
                    rows_per_image: Some(height),
                },
                size,
            );
            let view = texture.create_view(&Default::default());
            self.textures
                .push(device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: None,
                    layout: &self.layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(&view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::Sampler(&sampler),
                        },
                    ],
                }));
        }
    }

    /// The den anchor is the resting creature's center. The back layer is behind
    /// the liquid; `render_den_front` later occludes its lower volume.
    #[allow(clippy::too_many_arguments)]
    pub fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        viewport: [u32; 2],
        den: [f32; 2],
        den_scale: f32,
        birth: Option<(f32, [f32; 4])>,
    ) {
        self.upload(device, queue);
        let w = viewport[0].max(1) as f32;
        let h = viewport[1].max(1) as f32;
        let mut sprites = Vec::new();
        sprites.push(self.revealed_den(den_sprite(viewport, den, den_scale, false)));
        if let Some((t, monitor)) = birth {
            let pose = vfx::Pose::new(t, monitor);
            for (x, y, sw, sh, side) in [
                (-546.98, 197.43, 1088.39, 787.0, 1.0),
                (-548.0, -928.0, 1089.0, 666.0, -1.0),
                (-626.9, -676.3, 1249.0, 1256.2, 0.0),
                (-544.0, 365.0, 1086.0, 618.0, 1.0),
                (-546.0, -929.0, 1087.0, 566.0, -1.0),
            ] {
                let position = pose.world(glam::Vec2::new(x + sw * 0.5, y + sh * 0.5), side);
                let open = ((t - 7.79) / 0.92).clamp(0.0, 1.0);
                let opacity = if side == 0.0 {
                    (1.0 - smooth(0.06, 0.98, open)).powf(1.15)
                } else {
                    1.0 - smooth(9.2, 11.2, t)
                };
                let scale = if side == 0.0 {
                    pose.orb_scale * 1.36
                } else {
                    glam::Vec2::ONE
                };
                sprites.push(Sprite {
                    rect: [
                        position.x,
                        position.y,
                        sw * pose.scale * scale.x * 0.5,
                        sh * pose.scale * scale.y * 0.5,
                    ],
                    effect: [t, if side == 0.0 { 1.0 } else { 0.0 }, 0.0, 0.0],
                    style: [pose.angle(side), opacity, 1.0 / w, 1.0 / h],
                    color: [1.0; 4],
                });
            }
            // Three time-offset holder silhouettes from the reference release trail.
            let launch_age = t - 7.65;
            if (0.0..0.72).contains(&launch_age) {
                for sample in (1..=3).rev() {
                    let at = (t - sample as f32 * 0.016).max(7.65);
                    let previous = vfx::Pose::new(at, monitor);
                    let alpha =
                        0.06 * (1.0 - sample as f32 / 4.0) * (1.0 - smooth(0.28, 0.72, launch_age));
                    for (x, y, sw, sh, side, texture) in [
                        (-546.98, 197.43, 1088.39, 787.0, 1.0, 1.0),
                        (-548.0, -928.0, 1089.0, 666.0, -1.0, 2.0),
                    ] {
                        let position =
                            previous.world(glam::Vec2::new(x + sw * 0.5, y + sh * 0.5), side);
                        sprites.push(Sprite {
                            rect: [
                                position.x,
                                position.y,
                                sw * previous.scale * 0.5,
                                sh * previous.scale * 0.5,
                            ],
                            effect: [t, 0.0, 0.0, texture],
                            style: [previous.angle(side), alpha, 1.0 / w, 1.0 / h],
                            color: [1.0, 1.0, 1.0, 0.0],
                        });
                    }
                }
            }
            vfx::append(&mut sprites, t, monitor, viewport);
        }
        queue.write_buffer(&self.buffer, 0, bytemuck::cast_slice(&sprites));
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("nest and birth capsule"),
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
        pass.set_vertex_buffer(0, self.buffer.slice(..));
        let authored_count = sprites.len().min(6);
        let order = (authored_count..sprites.len())
            .filter(|i| sprites[*i].color[3] < 0.5)
            .chain(0..authored_count)
            .chain((authored_count..sprites.len()).filter(|i| sprites[*i].color[3] >= 0.5));
        for i in order {
            pass.set_pipeline(
                if sprites[i].effect[1] >= 2.0 && sprites[i].effect[1] < 6.0 {
                    &self.additive
                } else if i == 0 || sprites[i].color[3] < 0.5 {
                    &self.behind
                } else {
                    &self.front
                },
            );
            let texture = if i >= 6 && sprites[i].effect[1] == 0.0 {
                sprites[i].effect[3] as usize
            } else {
                i.min(5)
            };
            pass.set_bind_group(0, &self.textures[texture], &[]);
            pass.draw(0..6, i as u32..i as u32 + 1);
        }
    }

    /// Draw after both the liquid and ecology objects. Both nest layers sample
    /// the same authored pixels and transform, so resize cannot open a seam.
    #[allow(clippy::too_many_arguments)]
    pub fn render_den_front(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        viewport: [u32; 2],
        den: [f32; 2],
        den_scale: f32,
    ) {
        self.upload(device, queue);
        let sprite = self.revealed_den(den_sprite(viewport, den, den_scale, true));
        // A distinct buffer matters: queue writes happen before submitted render
        // passes and must not replace the capsule instances prepared above.
        queue.write_buffer(&self.den_front_buffer, 0, bytemuck::bytes_of(&sprite));
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("nest foreground cushion occlusion"),
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
        pass.set_pipeline(&self.front);
        pass.set_bind_group(0, &self.textures[0], &[]);
        pass.set_vertex_buffer(0, self.den_front_buffer.slice(..));
        pass.draw(0..6, 0..1);
    }
}

fn den_width_pixels(viewport: [u32; 2], scale: f32) -> f32 {
    (310.0 * scale)
        .min(viewport[0].max(1) as f32 * 0.8)
        .min(viewport[1].max(1) as f32 * 0.65)
}

/// Physical seat below the den anchor, shared by contact and toy placement.
#[must_use]
pub fn den_seat_depth_pixels(viewport: [u32; 2], scale: f32) -> f32 {
    den_width_pixels(viewport, scale) * 0.20
}

fn den_sprite(viewport: [u32; 2], den: [f32; 2], scale: f32, front: bool) -> Sprite {
    let w = viewport[0].max(1) as f32;
    let h = viewport[1].max(1) as f32;
    let width = den_width_pixels(viewport, scale);
    Sprite {
        // The visible base is at source y=846/1024. Its screen position remains
        // anchor + .292*width, matching the taskbar placement of earlier builds.
        rect: [
            den[0] * w,
            den[1] * h + width * 0.074_552_1,
            width * 0.5,
            width / 3.0,
        ],
        effect: [0.0, if front { -1.0 } else { -2.0 }, 0.0, 0.0],
        style: [0.0, 1.0, 1.0 / w, 1.0 / h],
        color: [1.0; 4],
    }
}
pub fn smooth(a: f32, b: f32, t: f32) -> f32 {
    let x = ((t - a) / (b - a)).clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}
/// Calendar growth: two infant days, then gradual growth through day seven.
pub fn growth_scale(age_seconds: f64) -> f32 {
    0.82 + 0.18 * smooth(2.0, 7.0, (age_seconds.max(0.0) / 86400.0) as f32)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn nest_layers_share_transform_and_visible_base_reaches_taskbar() {
        for viewport in [[1920, 1080], [3440, 1440], [640, 480], [800, 1280]] {
            let width = den_width_pixels(viewport, 1.0);
            let anchor = [0.5, 1.0 - 0.292 * width / viewport[1] as f32];
            let back = den_sprite(viewport, anchor, 1.0, false);
            let front = den_sprite(viewport, anchor, 1.0, true);
            assert_eq!(back.rect, front.rect);
            let base = back.rect[1] + (846.0 / 1024.0 - 0.5) * back.rect[3] * 2.0;
            assert!((base - viewport[1] as f32).abs() < 0.5);
            let rim = back.rect[1] + (660.0 / 1024.0 - 0.5) * back.rect[3] * 2.0;
            let seat = anchor[1] * viewport[1] as f32 + den_seat_depth_pixels(viewport, 1.0);
            assert!(seat > rim && seat - rim < width * 0.04);
        }
    }

    #[test]
    fn growth_is_bounded_and_continuous() {
        assert_eq!(growth_scale(0.0), 0.82);
        assert_eq!(growth_scale(86400.0), 0.82);
        assert!((growth_scale(4.5 * 86400.0) - 0.91).abs() < 0.0001);
        assert_eq!(growth_scale(8.0 * 86400.0), 1.0);
        for day in 0..8 {
            assert!(
                growth_scale(f64::from(day + 1) * 86400.0)
                    >= growth_scale(f64::from(day) * 86400.0)
            );
        }
    }
    #[test]
    fn sprite_shader_is_valid() {
        let module = naga::front::wgsl::parse_str(include_str!("birth_scene.wgsl")).unwrap();
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .unwrap();
    }
}
