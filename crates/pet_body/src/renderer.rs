use std::sync::Arc;

use bytemuck::{Pod, Zeroable};
use glam::{Vec2, Vec3};
use thiserror::Error;
use wgpu::util::DeviceExt;
use winit::{dpi::PhysicalSize, window::Window};

use crate::ProceduralMesh;

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct Globals {
    viewport_time: [f32; 4],
    body_shape: [f32; 4],
    face_shape: [f32; 4],
    appendages: [f32; 4],
    primary_hsv: [f32; 4],
    secondary_hsv: [f32; 4],
    glow_hsv: [f32; 4],
    gaze_pupil: [f32; 4],
    lids_brows: [f32; 4],
    brow_mouth: [f32; 4],
    mouth_voice: [f32; 4],
    motion_a: [f32; 4],
    motion_b: [f32; 4],
    pattern: [f32; 4],
    occlusion: [f32; 4],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OcclusionMode {
    Front,
    PeekFromLeft,
    PeekFromRight,
    PeekFromTop,
    BehindSurface,
}

impl OcclusionMode {
    const fn shader_value(self) -> f32 {
        match self {
            Self::Front => 0.0,
            Self::PeekFromLeft => 1.0,
            Self::PeekFromRight => 2.0,
            Self::PeekFromTop => 3.0,
            Self::BehindSurface => 4.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RenderParameters {
    pub time: f32,
    pub arousal: f32,
    pub glow: f32,
    pub body_length: f32,
    pub body_width: f32,
    pub body_roundness: f32,
    pub head_ratio: f32,
    pub eye_size: f32,
    pub eye_spacing: f32,
    pub pupil_ratio: f32,
    pub softness: f32,
    pub tail_length: f32,
    pub tail_thickness: f32,
    pub ear_fin_size: f32,
    pub primary_hsv: Vec3,
    pub secondary_hsv: Vec3,
    pub glow_hsv: Vec3,
    pub gaze: Vec2,
    pub vergence: f32,
    pub pupil_size: f32,
    pub blink_left: f32,
    pub blink_right: f32,
    pub squint: f32,
    pub brow_raise: f32,
    pub brow_tension: f32,
    pub brow_asymmetry: f32,
    pub mouth_open: f32,
    pub mouth_curve: f32,
    pub mouth_tension: f32,
    pub cheek_glow: f32,
    pub audio_envelope: f32,
    pub purr: f32,
    pub squash: Vec2,
    pub tilt: f32,
    pub head_lag: Vec2,
    pub tail_lag: Vec2,
    pub breath: f32,
    pub compression: f32,
    pub pattern_scale: f32,
    pub pattern_contrast: f32,
    pub pattern_seed: u64,
    pub occlusion_mode: OcclusionMode,
    pub occlusion_edge: f32,
    pub occlusion_softness: f32,
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
    globals_buffer: wgpu::Buffer,
    globals_bind_group: wgpu::BindGroup,
    organism_scale: f32,
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
                label: Some("Pet 2 morphic graphics device"),
                required_features: wgpu::Features::empty(),
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

        let globals = globals_for(&config, RenderParameters::default(), organism_scale(mesh));
        let globals_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("morphic pet globals"),
            contents: bytemuck::bytes_of(&globals),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("morphic pet globals layout"),
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
            label: Some("morphic pet globals"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: globals_buffer.as_entire_binding(),
            }],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("morphic procedural pet shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("pet.wgsl").into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("morphic pet pipeline layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("morphic pet pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vertex_main"),
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
        Ok(Self {
            surface,
            device,
            queue,
            config,
            pipeline,
            globals_buffer,
            globals_bind_group,
            organism_scale: organism_scale(mesh),
        })
    }

    pub fn resize(&mut self, size: PhysicalSize<u32>) {
        if size.width == 0 || size.height == 0 {
            return;
        }
        self.config.width = size.width;
        self.config.height = size.height;
        self.surface.configure(&self.device, &self.config);
    }

    pub fn replace_mesh(&mut self, mesh: &ProceduralMesh) {
        self.organism_scale = organism_scale(mesh);
    }

    pub fn render(&mut self, parameters: RenderParameters) -> RenderOutcome {
        self.queue.write_buffer(
            &self.globals_buffer,
            0,
            bytemuck::bytes_of(&globals_for(&self.config, parameters, self.organism_scale)),
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
                label: Some("morphic pet render encoder"),
            });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("transparent morphic pet pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.globals_bind_group, &[]);
            pass.draw(0..3, 0..1);
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
            glow: 0.2,
            body_length: 1.0,
            body_width: 0.72,
            body_roundness: 0.8,
            head_ratio: 0.50,
            eye_size: 0.16,
            eye_spacing: 0.30,
            pupil_ratio: 0.52,
            softness: 0.7,
            tail_length: 0.8,
            tail_thickness: 0.10,
            ear_fin_size: 0.18,
            primary_hsv: Vec3::new(0.52, 0.58, 0.86),
            secondary_hsv: Vec3::new(0.62, 0.42, 0.92),
            glow_hsv: Vec3::new(0.12, 0.62, 1.0),
            gaze: Vec2::ZERO,
            vergence: 0.0,
            pupil_size: 0.52,
            blink_left: 0.0,
            blink_right: 0.0,
            squint: 0.0,
            brow_raise: 0.0,
            brow_tension: 0.0,
            brow_asymmetry: 0.0,
            mouth_open: 0.0,
            mouth_curve: 0.1,
            mouth_tension: 0.0,
            cheek_glow: 0.0,
            audio_envelope: 0.0,
            purr: 0.0,
            squash: Vec2::ONE,
            tilt: 0.0,
            head_lag: Vec2::ZERO,
            tail_lag: Vec2::ZERO,
            breath: 0.5,
            compression: 0.0,
            pattern_scale: 2.0,
            pattern_contrast: 0.35,
            pattern_seed: 0,
            occlusion_mode: OcclusionMode::Front,
            occlusion_edge: 0.0,
            occlusion_softness: 0.02,
        }
    }
}

fn organism_scale(mesh: &ProceduralMesh) -> f32 {
    let extent = mesh.maximum - mesh.minimum;
    (extent.x.max(extent.y) * 0.72).clamp(0.78, 1.35)
}

fn globals_for(
    config: &wgpu::SurfaceConfiguration,
    parameters: RenderParameters,
    organism_scale: f32,
) -> Globals {
    let aspect = config.width as f32 / config.height.max(1) as f32;
    Globals {
        viewport_time: [
            aspect,
            parameters.time,
            parameters.arousal.clamp(0.0, 1.0),
            parameters.glow.clamp(0.0, 1.0),
        ],
        body_shape: [
            parameters.body_width.clamp(0.35, 1.2),
            parameters.body_length.clamp(0.55, 1.5),
            parameters.body_roundness.clamp(0.2, 1.0),
            parameters.head_ratio.clamp(0.28, 0.78),
        ],
        face_shape: [
            parameters.eye_size.clamp(0.06, 0.30),
            parameters.eye_spacing.clamp(0.14, 0.52),
            parameters.pupil_ratio.clamp(0.20, 0.88),
            parameters.softness.clamp(0.0, 1.0),
        ],
        appendages: [
            parameters.tail_length.clamp(0.2, 1.8),
            parameters.tail_thickness.clamp(0.025, 0.24),
            parameters.ear_fin_size.clamp(0.0, 0.42),
            organism_scale,
        ],
        primary_hsv: parameters.primary_hsv.extend(1.0).to_array(),
        secondary_hsv: parameters.secondary_hsv.extend(1.0).to_array(),
        glow_hsv: parameters.glow_hsv.extend(1.0).to_array(),
        gaze_pupil: [
            parameters.gaze.x.clamp(-1.0, 1.0),
            parameters.gaze.y.clamp(-1.0, 1.0),
            parameters.vergence.clamp(0.0, 0.2),
            parameters.pupil_size.clamp(0.15, 0.95),
        ],
        lids_brows: [
            parameters.blink_left.clamp(0.0, 1.0),
            parameters.blink_right.clamp(0.0, 1.0),
            parameters.squint.clamp(0.0, 1.0),
            parameters.brow_raise.clamp(-1.0, 1.0),
        ],
        brow_mouth: [
            parameters.brow_tension.clamp(0.0, 1.0),
            parameters.brow_asymmetry.clamp(-1.0, 1.0),
            parameters.mouth_open.clamp(0.0, 1.0),
            parameters.mouth_curve.clamp(-1.0, 1.0),
        ],
        mouth_voice: [
            parameters.mouth_tension.clamp(0.0, 1.0),
            parameters.cheek_glow.clamp(0.0, 1.0),
            parameters.audio_envelope.clamp(0.0, 1.0),
            parameters.purr.clamp(0.0, 1.0),
        ],
        motion_a: [
            parameters.squash.x.clamp(0.55, 1.65),
            parameters.squash.y.clamp(0.55, 1.65),
            parameters.tilt.clamp(-0.65, 0.65),
            parameters.head_lag.x.clamp(-0.18, 0.18),
        ],
        motion_b: [
            parameters.head_lag.y.clamp(-0.18, 0.18),
            parameters.tail_lag.x.clamp(-0.35, 0.35),
            parameters.tail_lag.y.clamp(-0.35, 0.35),
            parameters.breath.clamp(0.0, 1.0),
        ],
        pattern: [
            parameters.pattern_scale.clamp(0.2, 12.0),
            parameters.pattern_contrast.clamp(0.0, 1.0),
            parameters.pattern_seed as u32 as f32 / u32::MAX as f32 * 17.0,
            parameters.compression.clamp(-0.2, 0.6),
        ],
        occlusion: [
            parameters.occlusion_mode.shader_value(),
            parameters.occlusion_edge.clamp(-1.0, 1.0),
            parameters.occlusion_softness.clamp(0.001, 0.18),
            0.0,
        ],
    }
}
