use std::sync::Arc;

use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec2};
use thiserror::Error;
use wgpu::util::DeviceExt;
use winit::{dpi::PhysicalSize, window::Window};

use crate::{MeshVertex, ProceduralMesh};

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct Globals {
    view_projection: [[f32; 4]; 4],
    time_arousal_blink_glow: [f32; 4],
    pattern: [f32; 4],
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RenderParameters {
    pub time: f32,
    pub arousal: f32,
    pub blink: f32,
    pub glow: f32,
    pub pattern_scale: f32,
    pub pattern_contrast: f32,
    pub pattern_seed: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderOutcome {
    Presented,
    Skipped,
    OutOfMemory,
}

#[derive(Debug, Error)]
pub enum RendererError {
    #[error("could not create a GPU surface: {0}")]
    Surface(#[from] wgpu::CreateSurfaceError),
    #[error("no compatible graphics adapter was found: {0}")]
    NoAdapter(#[from] wgpu::RequestAdapterError),
    #[error("could not create the graphics device: {0}")]
    Device(#[from] wgpu::RequestDeviceError),
    #[error("the window surface exposes no supported formats")]
    NoSurfaceFormat,
}

pub struct Renderer {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    index_count: u32,
    globals_buffer: wgpu::Buffer,
    globals_bind_group: wgpu::BindGroup,
    depth: wgpu::TextureView,
    mesh_view: MeshView,
}

#[derive(Debug, Clone, Copy)]
struct MeshView {
    center: Vec2,
    half_extent: Vec2,
}

impl MeshView {
    fn from_mesh(mesh: &ProceduralMesh) -> Self {
        let minimum = mesh.minimum.truncate();
        let maximum = mesh.maximum.truncate();
        Self {
            center: (minimum + maximum) * 0.5,
            half_extent: ((maximum - minimum) * 0.5 * 1.14).max(Vec2::splat(0.05)),
        }
    }
}

impl Renderer {
    pub async fn new(window: Arc<Window>, mesh: &ProceduralMesh) -> Result<Self, RendererError> {
        let size = window.inner_size();
        let mut instance_descriptor = wgpu::InstanceDescriptor::default();
        instance_descriptor.backend_options.dx12.presentation_system =
            wgpu_types::Dx12SwapchainKind::DxgiFromVisual;
        let instance = wgpu::Instance::new(&instance_descriptor);
        let surface = instance.create_surface(window)?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::LowPower,
                force_fallback_adapter: false,
                compatible_surface: Some(&surface),
            })
            .await?;
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("Pet 2 graphics device"),
                required_features: wgpu::Features::empty(),
                // The overlay may briefly inherit a large physical extent while Windows
                // applies a per-monitor DPI transition. The portable renderer still uses
                // conservative default WebGPU limits, whose 2D texture cap accommodates it.
                required_limits: wgpu::Limits::default(),
                experimental_features: wgpu::ExperimentalFeatures::disabled(),
                memory_hints: wgpu::MemoryHints::MemoryUsage,
                trace: wgpu::Trace::Off,
            })
            .await?;
        let capabilities = surface.get_capabilities(&adapter);
        let format = capabilities
            .formats
            .iter()
            .copied()
            .find(wgpu::TextureFormat::is_srgb)
            .or_else(|| capabilities.formats.first().copied())
            .ok_or(RendererError::NoSurfaceFormat)?;
        let alpha_mode = if capabilities
            .alpha_modes
            .contains(&wgpu::CompositeAlphaMode::PreMultiplied)
        {
            wgpu::CompositeAlphaMode::PreMultiplied
        } else if capabilities
            .alpha_modes
            .contains(&wgpu::CompositeAlphaMode::PostMultiplied)
        {
            wgpu::CompositeAlphaMode::PostMultiplied
        } else {
            wgpu::CompositeAlphaMode::Auto
        };
        let present_mode = if capabilities
            .present_modes
            .contains(&wgpu::PresentMode::Fifo)
        {
            wgpu::PresentMode::Fifo
        } else {
            capabilities.present_modes[0]
        };
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode,
            alpha_mode,
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("procedural pet vertices"),
            contents: bytemuck::cast_slice(&mesh.vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("procedural pet indices"),
            contents: bytemuck::cast_slice(&mesh.indices),
            usage: wgpu::BufferUsages::INDEX,
        });
        let mesh_view = MeshView::from_mesh(mesh);
        let globals = globals_for(&config, RenderParameters::default(), mesh_view);
        let globals_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("pet render globals"),
            contents: bytemuck::bytes_of(&globals),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("pet globals layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let globals_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("pet globals"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: globals_buffer.as_entire_binding(),
            }],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("procedural pet shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("pet.wgsl").into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("pet pipeline layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("procedural pet pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vertex_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[MeshVertex::buffer_layout()],
            },
            primitive: wgpu::PrimitiveState {
                cull_mode: Some(wgpu::Face::Back),
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth24Plus,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::LessEqual,
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fragment_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview: None,
            cache: None,
        });
        let depth = create_depth(&device, &config);
        Ok(Self {
            surface,
            device,
            queue,
            config,
            pipeline,
            vertex_buffer,
            index_buffer,
            index_count: mesh.indices.len() as u32,
            globals_buffer,
            globals_bind_group,
            depth,
            mesh_view,
        })
    }

    pub fn resize(&mut self, size: PhysicalSize<u32>) {
        if size.width == 0 || size.height == 0 {
            return;
        }
        self.config.width = size.width;
        self.config.height = size.height;
        self.surface.configure(&self.device, &self.config);
        self.depth = create_depth(&self.device, &self.config);
    }

    pub fn replace_mesh(&mut self, mesh: &ProceduralMesh) {
        self.vertex_buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("procedural pet vertices"),
                contents: bytemuck::cast_slice(&mesh.vertices),
                usage: wgpu::BufferUsages::VERTEX,
            });
        self.index_buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("procedural pet indices"),
                contents: bytemuck::cast_slice(&mesh.indices),
                usage: wgpu::BufferUsages::INDEX,
            });
        self.index_count = mesh.indices.len() as u32;
        self.mesh_view = MeshView::from_mesh(mesh);
    }

    pub fn render(&mut self, parameters: RenderParameters) -> RenderOutcome {
        self.queue.write_buffer(
            &self.globals_buffer,
            0,
            bytemuck::bytes_of(&globals_for(&self.config, parameters, self.mesh_view)),
        );
        let frame = match self.surface.get_current_texture() {
            Ok(frame) => frame,
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                self.surface.configure(&self.device, &self.config);
                return RenderOutcome::Skipped;
            }
            Err(wgpu::SurfaceError::OutOfMemory) => return RenderOutcome::OutOfMemory,
            Err(_) => return RenderOutcome::Skipped,
        };
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("pet render encoder"),
            });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("transparent pet pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.globals_bind_group, &[]);
            pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
            pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..self.index_count, 0, 0..1);
        }
        self.queue.submit(Some(encoder.finish()));
        frame.present();
        RenderOutcome::Presented
    }
}

impl Default for RenderParameters {
    fn default() -> Self {
        Self {
            time: 0.0,
            arousal: 0.3,
            blink: 0.0,
            glow: 0.2,
            pattern_scale: 2.0,
            pattern_contrast: 0.5,
            pattern_seed: 0,
        }
    }
}

fn globals_for(
    config: &wgpu::SurfaceConfiguration,
    parameters: RenderParameters,
    mesh_view: MeshView,
) -> Globals {
    let aspect = config.width as f32 / config.height.max(1) as f32;
    let half_height = mesh_view
        .half_extent
        .y
        .max(mesh_view.half_extent.x / aspect.max(0.01));
    let half_width = half_height * aspect;
    let projection = Mat4::orthographic_rh(
        mesh_view.center.x - half_width,
        mesh_view.center.x + half_width,
        mesh_view.center.y - half_height,
        mesh_view.center.y + half_height,
        -3.0,
        3.0,
    );
    Globals {
        view_projection: projection.to_cols_array_2d(),
        time_arousal_blink_glow: [
            parameters.time,
            parameters.arousal.clamp(0.0, 1.0),
            parameters.blink.clamp(0.0, 1.0),
            parameters.glow.clamp(0.0, 1.0),
        ],
        pattern: [
            parameters.pattern_scale.clamp(0.2, 12.0),
            parameters.pattern_contrast.clamp(0.0, 1.0),
            parameters.pattern_seed as u32 as f32 / u32::MAX as f32 * 17.0,
            0.0,
        ],
    }
}

fn create_depth(device: &wgpu::Device, config: &wgpu::SurfaceConfiguration) -> wgpu::TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("pet depth texture"),
            size: wgpu::Extent3d {
                width: config.width.max(1),
                height: config.height.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth24Plus,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        })
        .create_view(&wgpu::TextureViewDescriptor::default())
}
