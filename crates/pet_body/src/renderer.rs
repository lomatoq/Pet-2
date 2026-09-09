use std::sync::Arc;

use bytemuck::{Pod, Zeroable};
use glam::{Vec2, Vec3};
use thiserror::Error;
use wgpu::util::DeviceExt;
use winit::{dpi::PhysicalSize, window::Window};

use crate::{
    BodyRenderMode, DropletRenderState, LiquidRenderState, MAX_DROPLETS, MAX_PARTICLES,
    MaterialVariant, ProceduralMesh,
    liquid_render::{DensityInstance, pack_particles_for_material, vertex_layout},
};

const SUPERSAMPLE_SCALE: u32 = 2;
const SHADOW_HALF_SCALE: u32 = 2;
const SHADOW_QUARTER_SCALE: u32 = 4;
const SHADOW_QUARTER_THRESHOLD: f32 = 48.0;
const MAX_SHADOW_PAIRED_TAPS: usize = 16;
/// The particle simulation has its own stable local-space extent. Projecting it
/// through the legacy genome mesh made the same tuned blob change size between
/// Body Lab and the desktop Pet, and could push it beyond the overlay bounds.
pub(crate) const PARTICLE_PBF_ORGANISM_SCALE: f32 = 1.255_772_7;
const INTERMEDIATE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
const MAX_INTERNAL_GLOW_ORBS: usize = 8;
const MAX_SOUL_GLOW_LOBES: usize = 6;

fn surface_source_over_blend(premultiplied_output: bool) -> wgpu::BlendState {
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
    iris_hsv: [f32; 4],
    gaze_pupil: [f32; 4],
    lids_brows: [f32; 4],
    brow_mouth: [f32; 4],
    mouth_voice: [f32; 4],
    motion_a: [f32; 4],
    motion_b: [f32; 4],
    pattern: [f32; 4],
    occlusion: [f32; 4],
    physiology_a: [f32; 4],
    physiology_b: [f32; 4],
    flow: [f32; 4],
    eye_detail: [f32; 4],
    visual_detail: [f32; 4],
    droplet_meta: [f32; 4],
    morph_a: [f32; 4],
    morph_b: [f32; 4],
    droplet_position_radius: [[f32; 4]; MAX_DROPLETS],
    droplet_motion_shape: [[f32; 4]; MAX_DROPLETS],
    droplet_bridge: [[f32; 4]; MAX_DROPLETS],
    render_mode: [f32; 4],
    liquid_meta: [f32; 4],
    face_frame_a: [f32; 4],
    face_frame_b: [f32; 4],
    liquid_motion: [f32; 4],
    material_a: [f32; 4],
    material_b: [f32; 4],
    material_c: [f32; 4],
    material_d: [f32; 4],
    material_e: [f32; 4],
    material_f: [f32; 4],
    face_tuning: [f32; 4],
    desktop_capture: [f32; 4],
    desktop_capture_meta: [f32; 4],
    cinematic_a: [f32; 4],
    cinematic_b: [f32; 4],
    cinematic_c: [f32; 4],
    cinematic_d: [f32; 4],
    cinematic_e: [f32; 4],
    cinematic_f: [f32; 4],
    cinematic_g: [f32; 4],
    cinematic_h: [f32; 4],
    cinematic_i: [f32; 4],
    glow_orb_position_radius: [[f32; 4]; MAX_INTERNAL_GLOW_ORBS],
    glow_orb_color_intensity: [[f32; 4]; MAX_INTERNAL_GLOW_ORBS],
    soul_glow_position_radius: [[f32; 4]; MAX_SOUL_GLOW_LOBES],
    face_lids: [[f32; 4]; 2],
    face_brows: [[f32; 4]; 2],
    face_mouth: [f32; 4],
    face_eye: [f32; 4],
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct ComposeGlobals {
    output_mode: [f32; 4],
    shadow: [f32; 4],
    shadow_style: [f32; 4],
    post: [f32; 4],
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct ShadowFilterGlobals {
    direction_meta: [f32; 4],
    downsample: [f32; 4],
    paired_taps: [[f32; 4]; MAX_SHADOW_PAIRED_TAPS / 2],
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

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ReviewBackground {
    #[default]
    Transparent,
    Black,
    White,
    Gray,
    BusyChecker,
    Warm,
    Cold,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum DebugView {
    #[default]
    Material,
    Field,
    Alpha,
    FaceCoverage,
    FlowOrComponent,
    Thickness,
    EdgeDistance,
    MacroNormal,
    StudioReflection,
    Caustics,
}

impl DebugView {
    const fn shader_value(self) -> f32 {
        match self {
            Self::Material => 0.0,
            Self::Field => 1.0,
            Self::Alpha => 2.0,
            Self::FaceCoverage => 3.0,
            Self::FlowOrComponent => 4.0,
            Self::Thickness => 5.0,
            Self::EdgeDistance => 6.0,
            Self::MacroNormal => 7.0,
            Self::StudioReflection => 8.0,
            Self::Caustics => 9.0,
        }
    }
}

impl ReviewBackground {
    const fn shader_value(self) -> f32 {
        match self {
            Self::Transparent => 0.0,
            Self::Black => 1.0,
            Self::White => 2.0,
            Self::Gray => 3.0,
            Self::BusyChecker => 4.0,
            Self::Warm => 5.0,
            Self::Cold => 6.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RenderParameters {
    pub render_mode: BodyRenderMode,
    pub render_scale: u32,
    pub presentation_scale: f32,
    pub presentation_offset: Vec2,
    pub debug_view: DebugView,
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
    pub iris_hsv: Vec3,
    pub override_iris_color: bool,
    pub gaze: Vec2,
    pub vergence: f32,
    pub pupil_size: f32,
    pub pupil_asymmetry: f32,
    pub blink_left: f32,
    pub blink_right: f32,
    pub squint: f32,
    pub brow_raise: f32,
    pub brow_tension: f32,
    pub brow_asymmetry: f32,
    pub geometry: lifecore::FaceGeometry,
    pub eye_aperture: f32,
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
    pub morph_mode2: Vec2,
    pub morph_mode3: Vec2,
    pub morph_mode4: Vec2,
    pub morph_area_scale: f32,
    pub pattern_scale: f32,
    pub pattern_contrast: f32,
    pub pattern_seed: u64,
    pub pulse: f32,
    pub shell_opacity: f32,
    pub inner_density: f32,
    pub translucency: f32,
    pub core_glow: f32,
    pub halo: f32,
    pub iris_activity: f32,
    pub eye_wetness: f32,
    pub flow_strength: f32,
    pub flow_scale: f32,
    pub flow_speed: f32,
    pub flow_phase: f32,
    pub flow_warp: f32,
    pub core_size: f32,
    pub cornea_strength: f32,
    pub iris_scale: f32,
    pub limbal_strength: f32,
    pub iris_fiber_count: f32,
    pub iris_contrast: f32,
    pub droplet_energy: f32,
    pub droplet_cohesion: f32,
    pub droplet_spread: f32,
    pub droplets: [DropletRenderState; MAX_DROPLETS],
    pub liquid: LiquidRenderState,
    pub material_absorption: f32,
    pub material_scattering: f32,
    pub material_thickness: f32,
    pub material_refraction: f32,
    pub material_blur: f32,
    pub material_rim_strength: f32,
    pub material_rim_power: f32,
    pub material_broad_specular: f32,
    pub material_broad_specular_power: f32,
    pub material_tight_specular: f32,
    pub material_tight_specular_power: f32,
    pub material_emission: f32,
    pub material_fresnel_f0: f32,
    pub material_core_level: f32,
    pub material_thickness_gamma: f32,
    pub material_pseudo_depth: f32,
    pub material_normal_scale: f32,
    pub material_light_wrap: f32,
    pub material_ambient_scatter: f32,
    pub material_direct_scatter: f32,
    pub material_transmission_hue_preservation: f32,
    pub material_opacity: f32,
    pub material_variant: MaterialVariant,
    pub material_cinematic_smoothing: f32,
    pub material_internal_orb_count: usize,
    pub material_internal_orb_intensity: f32,
    pub material_internal_orb_size: f32,
    pub material_internal_orb_halo: f32,
    pub material_internal_orb_speed: f32,
    pub material_internal_orb_depth: f32,
    pub material_internal_orb_spread: f32,
    pub material_studio_intensity: f32,
    pub material_studio_base_roughness: f32,
    pub material_studio_coat_roughness: f32,
    pub material_narrow_rim_strength: f32,
    pub material_broad_rim_strength: f32,
    pub material_rim_saturation: f32,
    pub material_edge_light_width: f32,
    pub material_caustic_strength: f32,
    pub material_caustic_scale: f32,
    pub material_caustic_speed: f32,
    pub material_caustic_dispersion: f32,
    pub material_rounded_highlight_strength: f32,
    pub material_highlight_tint: f32,
    pub material_soul_glow_count: usize,
    pub material_soul_glow_strength: f32,
    pub material_soul_glow_size: f32,
    pub material_soul_glow_speed: f32,
    pub material_soul_glow_pulse: f32,
    pub material_soul_glow_feather: f32,
    pub material_bloom_strength: f32,
    pub liquid_iso_threshold: f32,
    pub face_visible: bool,
    pub face_eye_highlight_scale: f32,
    pub face_eye_socket_strength: f32,
    pub face_relief_strength: f32,
    pub face_relief_darkness: f32,
    pub face_relief_coat_strength: f32,
    pub shadow_horizontal_offset: f32,
    pub shadow_vertical_offset: f32,
    pub shadow_feather: f32,
    pub shadow_opacity: f32,
    pub shadow_color: Vec3,
    pub exposure: f32,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapturedFrame {
    pub width: u32,
    pub height: u32,
    /// Tightly packed, straight RGBA8 pixels in top-to-bottom row order.
    pub rgba8: Vec<u8>,
}

#[derive(Debug, Error)]
pub enum RendererCaptureError {
    #[error("the current presentation surface cannot be copied for capture")]
    UnsupportedSurface,
    #[error("the current presentation surface format {0:?} is not an RGBA8/BGRA8 format")]
    UnsupportedFormat(wgpu::TextureFormat),
    #[error("the presentation surface was unavailable during capture")]
    SurfaceUnavailable,
    #[error("the graphics device ran out of memory during capture")]
    OutOfMemory,
    #[error("the capture buffer could not be mapped: {0}")]
    Map(#[from] wgpu::BufferAsyncError),
    #[error("the graphics device could not finish the capture: {0}")]
    Poll(#[from] wgpu::PollError),
    #[error("the capture callback did not complete")]
    CallbackUnavailable,
}

#[derive(Debug, Clone, Copy, Default)]
struct InternalFeatureFrame {
    orb_position_radius: [[f32; 4]; MAX_INTERNAL_GLOW_ORBS],
    orb_color_intensity: [[f32; 4]; MAX_INTERNAL_GLOW_ORBS],
    soul_position_radius: [[f32; 4]; MAX_SOUL_GLOW_LOBES],
}

#[derive(Debug, Default)]
struct InternalFeatureTracker {
    frame: InternalFeatureFrame,
    initialized: bool,
    last_time: f32,
}

impl InternalFeatureTracker {
    fn update(&mut self, parameters: &RenderParameters) -> InternalFeatureFrame {
        let (desired_orbs, desired_colors) = internal_glow_orbs(parameters);
        let desired_soul = soul_glow_lobes(parameters);
        if !self.initialized || parameters.material_variant != MaterialVariant::CinematicJelly {
            self.frame = InternalFeatureFrame {
                orb_position_radius: desired_orbs,
                orb_color_intensity: desired_colors,
                soul_position_radius: desired_soul,
            };
            self.initialized = true;
            self.last_time = parameters.time;
            return self.frame;
        }
        let raw_dt = parameters.time - self.last_time;
        let dt = if raw_dt.is_finite() && (0.0..=0.25).contains(&raw_dt) {
            raw_dt.max(1.0 / 240.0)
        } else {
            1.0 / 60.0
        };
        self.last_time = parameters.time;
        let position_alpha = 1.0 - (-dt / 0.46).exp();
        let scalar_alpha = 1.0 - (-dt / 0.30).exp();
        for index in 0..MAX_INTERNAL_GLOW_ORBS {
            smooth_feature_vec4(
                &mut self.frame.orb_position_radius[index],
                desired_orbs[index],
                position_alpha,
                scalar_alpha,
            );
            for (current, desired) in self.frame.orb_color_intensity[index]
                .iter_mut()
                .zip(desired_colors[index])
            {
                *current += (desired - *current) * scalar_alpha;
            }
        }
        for (current, desired) in self.frame.soul_position_radius.iter_mut().zip(desired_soul) {
            smooth_feature_vec4(current, desired, position_alpha, scalar_alpha);
        }
        self.frame
    }
}

fn smooth_feature_vec4(
    current: &mut [f32; 4],
    desired: [f32; 4],
    position_alpha: f32,
    scalar_alpha: f32,
) {
    let current_position = Vec2::new(current[0], current[1]);
    let desired_position = Vec2::new(desired[0], desired[1]);
    let delta = (desired_position - current_position) * position_alpha;
    let bounded_delta = delta.clamp_length_max(0.015);
    let position = current_position + bounded_delta;
    current[0] = position.x;
    current[1] = position.y;
    current[2] += (desired[2] - current[2]) * scalar_alpha;
    current[3] += (desired[3] - current[3]) * scalar_alpha;
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
    #[error(
        "the {width}x{height} single-surface desktop exceeds the adapter's maximum 2D texture dimension of {maximum_dimension} pixels"
    )]
    SurfaceDimensionsUnsupported {
        width: u32,
        height: u32,
        maximum_dimension: u32,
    },
}

pub struct Renderer {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    pipeline: wgpu::RenderPipeline,
    liquid_particle_pipeline: wgpu::RenderPipeline,
    liquid_filter_pipeline: wgpu::RenderPipeline,
    liquid_surface_pipeline: wgpu::RenderPipeline,
    liquid_surface_safe_pipeline: wgpu::RenderPipeline,
    shadow_downsample_pipeline: wgpu::RenderPipeline,
    shadow_blur_pipeline: wgpu::RenderPipeline,
    compose_pipeline: wgpu::RenderPipeline,
    dual_clear_pipeline: wgpu::RenderPipeline,
    single_clear_pipeline: wgpu::RenderPipeline,
    globals_buffer: wgpu::Buffer,
    compose_globals_buffer: wgpu::Buffer,
    shadow_horizontal_globals_buffer: wgpu::Buffer,
    shadow_vertical_globals_buffer: wgpu::Buffer,
    bind_group_layout: wgpu::BindGroupLayout,
    globals_bind_group: wgpu::BindGroup,
    liquid_surface_bind_group_layout: wgpu::BindGroupLayout,
    liquid_surface_bind_group: wgpu::BindGroup,
    liquid_filter_bind_group_layout: wgpu::BindGroupLayout,
    liquid_filter_bind_group: wgpu::BindGroup,
    shadow_filter_bind_group_layout: wgpu::BindGroupLayout,
    shadow_half_downsample_bind_group: wgpu::BindGroup,
    shadow_half_horizontal_bind_group: wgpu::BindGroup,
    shadow_half_vertical_bind_group: wgpu::BindGroup,
    shadow_quarter_downsample_bind_group: wgpu::BindGroup,
    shadow_quarter_horizontal_bind_group: wgpu::BindGroup,
    shadow_quarter_vertical_bind_group: wgpu::BindGroup,
    compose_bind_group_layout: wgpu::BindGroupLayout,
    compose_half_bind_group: wgpu::BindGroup,
    compose_quarter_bind_group: wgpu::BindGroup,
    background_texture: wgpu::Texture,
    background_size: (u32, u32),
    background_sampler: wgpu::Sampler,
    offscreen_texture: wgpu::Texture,
    offscreen_view: wgpu::TextureView,
    offscreen_sampler: wgpu::Sampler,
    shadow_half_a_texture: wgpu::Texture,
    shadow_half_a_view: wgpu::TextureView,
    shadow_half_b_texture: wgpu::Texture,
    shadow_half_b_view: wgpu::TextureView,
    shadow_quarter_a_texture: wgpu::Texture,
    shadow_quarter_a_view: wgpu::TextureView,
    shadow_quarter_b_texture: wgpu::Texture,
    shadow_quarter_b_view: wgpu::TextureView,
    shadow_sampler: wgpu::Sampler,
    density_texture: wgpu::Texture,
    density_view: wgpu::TextureView,
    material_flow_texture: wgpu::Texture,
    material_flow_view: wgpu::TextureView,
    macro_texture: wgpu::Texture,
    macro_view: wgpu::TextureView,
    density_sampler: wgpu::Sampler,
    _studio_texture: wgpu::Texture,
    studio_view: wgpu::TextureView,
    studio_sampler: wgpu::Sampler,
    particle_instance_buffer: wgpu::Buffer,
    background_valid: bool,
    background_uv_transform: [f32; 4],
    background_freshness: f32,
    studio_material_backdrop: bool,
    organism_scale: f32,
    premultiplied_output: bool,
    review_background: ReviewBackground,
    render_scale: u32,
    maximum_texture_dimension_2d: u32,
    last_rejected_target: Option<[u32; 3]>,
    internal_features: InternalFeatureTracker,
    previous_liquid_scissor: Option<[u32; 4]>,
    density_targets_initialized: bool,
    macro_target_initialized: bool,
    offscreen_target_initialized: bool,
}

impl Renderer {
    pub async fn new(window: Arc<Window>, mesh: &ProceduralMesh) -> Result<Self, RendererError> {
        Self::new_with_render_scale(window, mesh, SUPERSAMPLE_SCALE).await
    }

    pub async fn new_with_render_scale(
        window: Arc<Window>,
        mesh: &ProceduralMesh,
        initial_render_scale: u32,
    ) -> Result<Self, RendererError> {
        let requested_render_scale = initial_render_scale.clamp(1, SUPERSAMPLE_SCALE);
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
        let adapter_limits = adapter.limits();
        let maximum_texture_dimension_2d = adapter_limits.max_texture_dimension_2d;
        let surface_width = size.width.max(1);
        let surface_height = size.height.max(1);
        let Some(initial_render_scale) = supported_render_scale(
            surface_width,
            surface_height,
            requested_render_scale,
            maximum_texture_dimension_2d,
        ) else {
            return Err(RendererError::SurfaceDimensionsUnsupported {
                width: surface_width,
                height: surface_height,
                maximum_dimension: maximum_texture_dimension_2d,
            });
        };
        if initial_render_scale != requested_render_scale {
            eprintln!(
                "renderer target {surface_width}x{surface_height} cannot use {requested_render_scale}x within the adapter's {maximum_texture_dimension_2d}px texture limit; using {initial_render_scale}x"
            );
        }
        // Device limits are fixed for its lifetime. Request the adapter's full
        // advertised 2D extent now so a later monitor-topology resize can use any
        // single-surface size this adapter actually supports.
        let required_limits = wgpu::Limits {
            max_texture_dimension_2d: maximum_texture_dimension_2d,
            ..wgpu::Limits::default()
        };
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("Pet 2 morphic graphics device"),
                required_features: wgpu::Features::empty(),
                required_limits,
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
        let capture_usage = if capabilities.usages.contains(wgpu::TextureUsages::COPY_SRC) {
            wgpu::TextureUsages::COPY_SRC
        } else {
            wgpu::TextureUsages::empty()
        };
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | capture_usage,
            format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode,
            alpha_mode,
            view_formats: vec![],
            // Production now uses one fixed desktop host, so there is no moving
            // HWND camera whose old pose must be suppressed. Two queued frames
            // avoid blocking every 8.33 ms render on acquire/present jitter while
            // keeping latency bounded to one additional presentation interval.
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        let premultiplied_output = alpha_mode == wgpu::CompositeAlphaMode::PreMultiplied;
        // The full-screen fragment shader produces the final frame in the exact alpha
        // convention requested by the surface compositor. Replacing the transparent
        // attachment preserves straight RGB for PostMultiplied surfaces and avoids
        // applying source-over a second time to already-premultiplied output.
        let blend = wgpu::BlendState::REPLACE;

        let globals = globals_for(
            &config,
            RenderParameters::default(),
            organism_scale(mesh),
            premultiplied_output,
            ReviewBackground::Transparent,
            false,
            initial_render_scale,
        );
        let globals_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("morphic pet globals"),
            contents: bytemuck::bytes_of(&globals),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let compose_globals = compose_globals_for(
            &config,
            premultiplied_output,
            ReviewBackground::Transparent,
            RenderParameters::default(),
            initial_render_scale,
        );
        let compose_globals_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("morphic pet resolve globals"),
            contents: bytemuck::bytes_of(&compose_globals),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("morphic pet globals layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let liquid_surface_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("liquid surface density and desktop layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 3,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 4,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 5,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 6,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 7,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 8,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
            });
        let liquid_filter_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("liquid macro reconstruction layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
            });
        let shadow_filter_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("separable shadow filter layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
            });
        let compose_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("morphic pet resolve layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 3,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 4,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
            });
        // The production checker is analytic. Keep only a valid dummy binding until
        // a real background frame is explicitly uploaded instead of reserving a
        // full-desktop BGRA texture that is never sampled.
        let background_texture = create_background_texture(&device, 1, 1);
        let background_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("desktop refraction sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        let offscreen_texture =
            create_offscreen_texture(&device, config.width, config.height, initial_render_scale);
        let offscreen_view = offscreen_texture.create_view(&wgpu::TextureViewDescriptor::default());
        let offscreen_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("morphic pet linear resolve sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        let (shadow_half_a_texture, shadow_half_a_view) = create_shadow_texture(
            &device,
            config.width,
            config.height,
            SHADOW_HALF_SCALE,
            "half A",
        );
        let (shadow_half_b_texture, shadow_half_b_view) = create_shadow_texture(
            &device,
            config.width,
            config.height,
            SHADOW_HALF_SCALE,
            "half B",
        );
        let (shadow_quarter_a_texture, shadow_quarter_a_view) = create_shadow_texture(
            &device,
            config.width,
            config.height,
            SHADOW_QUARTER_SCALE,
            "quarter A",
        );
        let (shadow_quarter_b_texture, shadow_quarter_b_view) = create_shadow_texture(
            &device,
            config.width,
            config.height,
            SHADOW_QUARTER_SCALE,
            "quarter B",
        );
        let shadow_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Gaussian shadow mask sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        let default_shadow_horizontal = shadow_filter_globals(
            config.width,
            config.height,
            SHADOW_HALF_SCALE,
            initial_render_scale,
            RenderParameters::default().shadow_feather,
            true,
        );
        let default_shadow_vertical = shadow_filter_globals(
            config.width,
            config.height,
            SHADOW_HALF_SCALE,
            initial_render_scale,
            RenderParameters::default().shadow_feather,
            false,
        );
        let shadow_horizontal_globals_buffer =
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("horizontal Gaussian shadow globals"),
                contents: bytemuck::bytes_of(&default_shadow_horizontal),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            });
        let shadow_vertical_globals_buffer =
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("vertical Gaussian shadow globals"),
                contents: bytemuck::bytes_of(&default_shadow_vertical),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            });
        let density_texture =
            create_density_texture(&device, config.width, config.height, initial_render_scale);
        let density_view = density_texture.create_view(&wgpu::TextureViewDescriptor::default());
        let material_flow_texture = create_material_flow_texture(
            &device,
            config.width,
            config.height,
            initial_render_scale,
        );
        let material_flow_view =
            material_flow_texture.create_view(&wgpu::TextureViewDescriptor::default());
        let macro_texture = create_macro_texture(&device, config.width, config.height);
        let macro_view = macro_texture.create_view(&wgpu::TextureViewDescriptor::default());
        let density_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("liquid density reconstruction sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        let (studio_texture, studio_view) = create_studio_texture(&device, &queue);
        let studio_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("cinematic studio environment sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        let particle_instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("liquid particle density instances"),
            size: (std::mem::size_of::<DensityInstance>() * MAX_PARTICLES) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let globals_bind_group = create_globals_bind_group(
            &device,
            &bind_group_layout,
            &globals_buffer,
            &background_texture,
            &background_sampler,
        );
        let liquid_surface_bind_group = create_liquid_surface_bind_group(
            &device,
            &liquid_surface_bind_group_layout,
            &globals_buffer,
            &background_texture,
            &background_sampler,
            &density_view,
            &density_sampler,
            &macro_view,
            &material_flow_view,
            &studio_view,
            &studio_sampler,
        );
        let liquid_filter_bind_group = create_liquid_filter_bind_group(
            &device,
            &liquid_filter_bind_group_layout,
            &globals_buffer,
            &density_view,
            &density_sampler,
        );
        let shadow_half_downsample_bind_group = create_shadow_filter_bind_group(
            &device,
            &shadow_filter_bind_group_layout,
            &shadow_horizontal_globals_buffer,
            &offscreen_view,
            &offscreen_sampler,
            "half shadow area downsample",
        );
        let shadow_half_horizontal_bind_group = create_shadow_filter_bind_group(
            &device,
            &shadow_filter_bind_group_layout,
            &shadow_horizontal_globals_buffer,
            &shadow_half_a_view,
            &shadow_sampler,
            "half shadow horizontal blur",
        );
        let shadow_half_vertical_bind_group = create_shadow_filter_bind_group(
            &device,
            &shadow_filter_bind_group_layout,
            &shadow_vertical_globals_buffer,
            &shadow_half_b_view,
            &shadow_sampler,
            "half shadow vertical blur",
        );
        let shadow_quarter_downsample_bind_group = create_shadow_filter_bind_group(
            &device,
            &shadow_filter_bind_group_layout,
            &shadow_horizontal_globals_buffer,
            &offscreen_view,
            &offscreen_sampler,
            "quarter shadow area downsample",
        );
        let shadow_quarter_horizontal_bind_group = create_shadow_filter_bind_group(
            &device,
            &shadow_filter_bind_group_layout,
            &shadow_horizontal_globals_buffer,
            &shadow_quarter_a_view,
            &shadow_sampler,
            "quarter shadow horizontal blur",
        );
        let shadow_quarter_vertical_bind_group = create_shadow_filter_bind_group(
            &device,
            &shadow_filter_bind_group_layout,
            &shadow_vertical_globals_buffer,
            &shadow_quarter_b_view,
            &shadow_sampler,
            "quarter shadow vertical blur",
        );
        let compose_half_bind_group = create_compose_bind_group(
            &device,
            &compose_bind_group_layout,
            &compose_globals_buffer,
            &offscreen_view,
            &offscreen_sampler,
            &shadow_half_a_view,
            &shadow_sampler,
        );
        let compose_quarter_bind_group = create_compose_bind_group(
            &device,
            &compose_bind_group_layout,
            &compose_globals_buffer,
            &offscreen_view,
            &offscreen_sampler,
            &shadow_quarter_a_view,
            &shadow_sampler,
        );
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("morphic procedural pet shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("legacy_analytic.wgsl").into()),
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
                    format: INTERMEDIATE_FORMAT,
                    blend: Some(blend),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview: None,
            cache: None,
        });
        let density_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("anisotropic liquid density accumulation shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("liquid_density.wgsl").into()),
        });
        let density_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("liquid density pipeline layout"),
                bind_group_layouts: &[&bind_group_layout],
                push_constant_ranges: &[],
            });
        let additive = wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
        };
        let liquid_particle_pipeline =
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("liquid particle density accumulation pipeline"),
                layout: Some(&density_pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &density_shader,
                    entry_point: Some("particle_vertex"),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    buffers: &[vertex_layout()],
                },
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    cull_mode: None,
                    ..Default::default()
                },
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &density_shader,
                    entry_point: Some("particle_fragment"),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    targets: &[
                        Some(wgpu::ColorTargetState {
                            format: INTERMEDIATE_FORMAT,
                            blend: Some(additive),
                            write_mask: wgpu::ColorWrites::ALL,
                        }),
                        Some(wgpu::ColorTargetState {
                            format: INTERMEDIATE_FORMAT,
                            blend: Some(additive),
                            write_mask: wgpu::ColorWrites::ALL,
                        }),
                    ],
                }),
                multiview: None,
                cache: None,
            });
        let liquid_filter_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("stable macro liquid reconstruction shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("liquid_filter.wgsl").into()),
        });
        let liquid_filter_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("liquid macro reconstruction pipeline layout"),
                bind_group_layouts: &[&liquid_filter_bind_group_layout],
                push_constant_ranges: &[],
            });
        let liquid_filter_pipeline =
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("half-resolution macro liquid reconstruction pipeline"),
                layout: Some(&liquid_filter_pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &liquid_filter_shader,
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
                    module: &liquid_filter_shader,
                    entry_point: Some("fragment_main"),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: INTERMEDIATE_FORMAT,
                        blend: Some(wgpu::BlendState::REPLACE),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview: None,
                cache: None,
            });
        let liquid_surface_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("implicit living jelly surface shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("liquid_surface.wgsl").into()),
        });
        let liquid_surface_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("implicit liquid surface pipeline layout"),
                bind_group_layouts: &[&liquid_surface_bind_group_layout],
                push_constant_ranges: &[],
            });
        let cinematic_constants = [("CINEMATIC", 1.0)];
        let safe_constants = [("CINEMATIC", 0.0)];
        let create_surface_pipeline = |label, constants| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&liquid_surface_pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &liquid_surface_shader,
                    entry_point: Some("vertex_main"),
                    compilation_options: wgpu::PipelineCompilationOptions {
                        constants,
                        ..Default::default()
                    },
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
                    module: &liquid_surface_shader,
                    entry_point: Some("fragment_main"),
                    compilation_options: wgpu::PipelineCompilationOptions {
                        constants,
                        ..Default::default()
                    },
                    targets: &[Some(wgpu::ColorTargetState {
                        format: INTERMEDIATE_FORMAT,
                        blend: Some(wgpu::BlendState::REPLACE),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview: None,
                cache: None,
            })
        };
        let liquid_surface_pipeline = create_surface_pipeline(
            "HDR cinematic living jelly surface pipeline",
            &cinematic_constants,
        );
        let liquid_surface_safe_pipeline =
            create_surface_pipeline("current-safe liquid surface pipeline", &safe_constants);
        let shadow_filter_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("area-prefiltered separable Gaussian shadow shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shadow_filter.wgsl").into()),
        });
        let shadow_filter_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("separable Gaussian shadow pipeline layout"),
                bind_group_layouts: &[&shadow_filter_bind_group_layout],
                push_constant_ranges: &[],
            });
        let create_shadow_pipeline = |label, entry_point| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&shadow_filter_pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shadow_filter_shader,
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
                    module: &shadow_filter_shader,
                    entry_point: Some(entry_point),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: wgpu::TextureFormat::R16Float,
                        blend: Some(wgpu::BlendState::REPLACE),
                        write_mask: wgpu::ColorWrites::RED,
                    })],
                }),
                multiview: None,
                cache: None,
            })
        };
        let shadow_downsample_pipeline = create_shadow_pipeline(
            "area-prefiltered shadow coverage downsample pipeline",
            "downsample_fragment",
        );
        let shadow_blur_pipeline = create_shadow_pipeline(
            "paired-tap separable Gaussian shadow pipeline",
            "blur_fragment",
        );
        let compose_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("morphic pet supersample resolve shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("compose.wgsl").into()),
        });
        let compose_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("morphic pet resolve pipeline layout"),
                bind_group_layouts: &[&compose_bind_group_layout],
                push_constant_ranges: &[],
            });
        let compose_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("morphic pet 2x linear resolve pipeline"),
            layout: Some(&compose_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &compose_shader,
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
                module: &compose_shader,
                entry_point: Some("fragment_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(surface_source_over_blend(premultiplied_output)),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview: None,
            cache: None,
        });
        let clear_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("scissored intermediate clear shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("clear_intermediate.wgsl").into()),
        });
        let clear_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("scissored intermediate clear pipeline layout"),
                bind_group_layouts: &[],
                push_constant_ranges: &[],
            });
        let clear_target = || {
            Some(wgpu::ColorTargetState {
                format: INTERMEDIATE_FORMAT,
                blend: Some(wgpu::BlendState::REPLACE),
                write_mask: wgpu::ColorWrites::ALL,
            })
        };
        let create_clear_pipeline =
            |label: &'static str,
             entry_point: &'static str,
             targets: &[Option<wgpu::ColorTargetState>]| {
                device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some(label),
                    layout: Some(&clear_pipeline_layout),
                    vertex: wgpu::VertexState {
                        module: &clear_shader,
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
                        module: &clear_shader,
                        entry_point: Some(entry_point),
                        compilation_options: wgpu::PipelineCompilationOptions::default(),
                        targets,
                    }),
                    multiview: None,
                    cache: None,
                })
            };
        let dual_clear_pipeline = create_clear_pipeline(
            "dual HDR scissored clear pipeline",
            "clear_dual",
            &[clear_target(), clear_target()],
        );
        let single_clear_pipeline = create_clear_pipeline(
            "single HDR scissored clear pipeline",
            "clear_single",
            &[clear_target()],
        );
        let background_size = (1, 1);
        Ok(Self {
            surface,
            device,
            queue,
            config,
            pipeline,
            liquid_particle_pipeline,
            liquid_filter_pipeline,
            liquid_surface_pipeline,
            liquid_surface_safe_pipeline,
            shadow_downsample_pipeline,
            shadow_blur_pipeline,
            compose_pipeline,
            dual_clear_pipeline,
            single_clear_pipeline,
            globals_buffer,
            compose_globals_buffer,
            shadow_horizontal_globals_buffer,
            shadow_vertical_globals_buffer,
            bind_group_layout,
            globals_bind_group,
            liquid_surface_bind_group_layout,
            liquid_surface_bind_group,
            liquid_filter_bind_group_layout,
            liquid_filter_bind_group,
            shadow_filter_bind_group_layout,
            shadow_half_downsample_bind_group,
            shadow_half_horizontal_bind_group,
            shadow_half_vertical_bind_group,
            shadow_quarter_downsample_bind_group,
            shadow_quarter_horizontal_bind_group,
            shadow_quarter_vertical_bind_group,
            compose_bind_group_layout,
            compose_half_bind_group,
            compose_quarter_bind_group,
            background_texture,
            background_size,
            background_sampler,
            offscreen_texture,
            offscreen_view,
            offscreen_sampler,
            shadow_half_a_texture,
            shadow_half_a_view,
            shadow_half_b_texture,
            shadow_half_b_view,
            shadow_quarter_a_texture,
            shadow_quarter_a_view,
            shadow_quarter_b_texture,
            shadow_quarter_b_view,
            shadow_sampler,
            density_texture,
            density_view,
            material_flow_texture,
            material_flow_view,
            macro_texture,
            macro_view,
            density_sampler,
            _studio_texture: studio_texture,
            studio_view,
            studio_sampler,
            particle_instance_buffer,
            background_valid: false,
            background_uv_transform: [1.0, 1.0, 0.0, 0.0],
            background_freshness: 0.0,
            studio_material_backdrop: false,
            organism_scale: organism_scale(mesh),
            premultiplied_output,
            review_background: ReviewBackground::Transparent,
            render_scale: initial_render_scale,
            maximum_texture_dimension_2d,
            last_rejected_target: None,
            internal_features: InternalFeatureTracker::default(),
            previous_liquid_scissor: None,
            density_targets_initialized: false,
            macro_target_initialized: false,
            offscreen_target_initialized: false,
        })
    }

    pub fn resize(&mut self, size: PhysicalSize<u32>) {
        if !surface_dimensions_changed(self.config.width, self.config.height, size) {
            return;
        }
        let Some(render_scale) = supported_render_scale(
            size.width,
            size.height,
            self.render_scale,
            self.maximum_texture_dimension_2d,
        ) else {
            let rejection = [size.width, size.height, 0];
            if self.last_rejected_target != Some(rejection) {
                eprintln!(
                    "renderer resize to {}x{} ignored: the single-surface desktop exceeds the adapter's {}px texture limit",
                    size.width, size.height, self.maximum_texture_dimension_2d
                );
            }
            self.last_rejected_target = Some(rejection);
            return;
        };
        if render_scale != self.render_scale {
            self.note_render_scale_fallback(
                size.width,
                size.height,
                self.render_scale,
                render_scale,
            );
            self.render_scale = render_scale;
        } else {
            self.last_rejected_target = None;
        }
        self.config.width = size.width;
        self.config.height = size.height;
        self.surface.configure(&self.device, &self.config);
        self.background_texture = create_background_texture(&self.device, 1, 1);
        self.background_size = (1, 1);
        self.globals_bind_group = create_globals_bind_group(
            &self.device,
            &self.bind_group_layout,
            &self.globals_buffer,
            &self.background_texture,
            &self.background_sampler,
        );
        self.recreate_intermediate_targets();
        self.background_valid = false;
        self.previous_liquid_scissor = None;
    }

    fn note_render_scale_fallback(
        &mut self,
        width: u32,
        height: u32,
        requested_scale: u32,
        resolved_scale: u32,
    ) {
        let rejection = [width, height, requested_scale];
        if self.last_rejected_target != Some(rejection) {
            eprintln!(
                "renderer target {width}x{height} cannot use {requested_scale}x within the adapter's {}px texture limit; using {resolved_scale}x",
                self.maximum_texture_dimension_2d
            );
        }
        self.last_rejected_target = Some(rejection);
    }

    fn recreate_intermediate_targets(&mut self) {
        self.offscreen_texture = create_offscreen_texture(
            &self.device,
            self.config.width,
            self.config.height,
            self.render_scale,
        );
        self.offscreen_view = self
            .offscreen_texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        (self.shadow_half_a_texture, self.shadow_half_a_view) = create_shadow_texture(
            &self.device,
            self.config.width,
            self.config.height,
            SHADOW_HALF_SCALE,
            "half A",
        );
        (self.shadow_half_b_texture, self.shadow_half_b_view) = create_shadow_texture(
            &self.device,
            self.config.width,
            self.config.height,
            SHADOW_HALF_SCALE,
            "half B",
        );
        (self.shadow_quarter_a_texture, self.shadow_quarter_a_view) = create_shadow_texture(
            &self.device,
            self.config.width,
            self.config.height,
            SHADOW_QUARTER_SCALE,
            "quarter A",
        );
        (self.shadow_quarter_b_texture, self.shadow_quarter_b_view) = create_shadow_texture(
            &self.device,
            self.config.width,
            self.config.height,
            SHADOW_QUARTER_SCALE,
            "quarter B",
        );
        self.density_texture = create_density_texture(
            &self.device,
            self.config.width,
            self.config.height,
            self.render_scale,
        );
        self.density_view = self
            .density_texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        self.material_flow_texture = create_material_flow_texture(
            &self.device,
            self.config.width,
            self.config.height,
            self.render_scale,
        );
        self.material_flow_view = self
            .material_flow_texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        self.macro_texture =
            create_macro_texture(&self.device, self.config.width, self.config.height);
        self.macro_view = self
            .macro_texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        self.liquid_surface_bind_group = create_liquid_surface_bind_group(
            &self.device,
            &self.liquid_surface_bind_group_layout,
            &self.globals_buffer,
            &self.background_texture,
            &self.background_sampler,
            &self.density_view,
            &self.density_sampler,
            &self.macro_view,
            &self.material_flow_view,
            &self.studio_view,
            &self.studio_sampler,
        );
        self.liquid_filter_bind_group = create_liquid_filter_bind_group(
            &self.device,
            &self.liquid_filter_bind_group_layout,
            &self.globals_buffer,
            &self.density_view,
            &self.density_sampler,
        );
        self.shadow_half_downsample_bind_group = create_shadow_filter_bind_group(
            &self.device,
            &self.shadow_filter_bind_group_layout,
            &self.shadow_horizontal_globals_buffer,
            &self.offscreen_view,
            &self.offscreen_sampler,
            "half shadow area downsample",
        );
        self.shadow_half_horizontal_bind_group = create_shadow_filter_bind_group(
            &self.device,
            &self.shadow_filter_bind_group_layout,
            &self.shadow_horizontal_globals_buffer,
            &self.shadow_half_a_view,
            &self.shadow_sampler,
            "half shadow horizontal blur",
        );
        self.shadow_half_vertical_bind_group = create_shadow_filter_bind_group(
            &self.device,
            &self.shadow_filter_bind_group_layout,
            &self.shadow_vertical_globals_buffer,
            &self.shadow_half_b_view,
            &self.shadow_sampler,
            "half shadow vertical blur",
        );
        self.shadow_quarter_downsample_bind_group = create_shadow_filter_bind_group(
            &self.device,
            &self.shadow_filter_bind_group_layout,
            &self.shadow_horizontal_globals_buffer,
            &self.offscreen_view,
            &self.offscreen_sampler,
            "quarter shadow area downsample",
        );
        self.shadow_quarter_horizontal_bind_group = create_shadow_filter_bind_group(
            &self.device,
            &self.shadow_filter_bind_group_layout,
            &self.shadow_horizontal_globals_buffer,
            &self.shadow_quarter_a_view,
            &self.shadow_sampler,
            "quarter shadow horizontal blur",
        );
        self.shadow_quarter_vertical_bind_group = create_shadow_filter_bind_group(
            &self.device,
            &self.shadow_filter_bind_group_layout,
            &self.shadow_vertical_globals_buffer,
            &self.shadow_quarter_b_view,
            &self.shadow_sampler,
            "quarter shadow vertical blur",
        );
        self.compose_half_bind_group = create_compose_bind_group(
            &self.device,
            &self.compose_bind_group_layout,
            &self.compose_globals_buffer,
            &self.offscreen_view,
            &self.offscreen_sampler,
            &self.shadow_half_a_view,
            &self.shadow_sampler,
        );
        self.compose_quarter_bind_group = create_compose_bind_group(
            &self.device,
            &self.compose_bind_group_layout,
            &self.compose_globals_buffer,
            &self.offscreen_view,
            &self.offscreen_sampler,
            &self.shadow_quarter_a_view,
            &self.shadow_sampler,
        );
        self.density_targets_initialized = false;
        self.macro_target_initialized = false;
        self.offscreen_target_initialized = false;
        self.previous_liquid_scissor = None;
    }

    pub fn replace_mesh(&mut self, mesh: &ProceduralMesh) {
        self.organism_scale = organism_scale(mesh);
    }

    pub fn set_review_background(&mut self, background: ReviewBackground) {
        self.review_background = background;
    }

    /// Uses the same deterministic checker-backed material formation as Body Lab
    /// while leaving the final compositor transparent. This deliberately avoids
    /// synchronous desktop capture in the production overlay.
    pub fn set_studio_material_backdrop(&mut self, enabled: bool) {
        self.studio_material_backdrop = enabled;
        if enabled {
            self.background_valid = false;
        }
    }

    /// Clears renderer-only temporal history between offline perceptual
    /// fixtures without altering body physics or authored tuning.
    pub fn reset_perceptual_capture_state(&mut self) {
        self.internal_features = InternalFeatureTracker::default();
        self.previous_liquid_scissor = None;
        self.density_targets_initialized = false;
        self.macro_target_initialized = false;
        self.offscreen_target_initialized = false;
    }

    /// Maps current overlay UVs into the physical desktop rectangle represented by
    /// the latest asynchronous capture.
    pub fn set_background_uv_transform(&mut self, scale: Vec2, offset: Vec2) {
        if scale.is_finite() && offset.is_finite() {
            self.background_uv_transform = [scale.x, scale.y, offset.x, offset.y];
        }
    }

    pub fn set_background_freshness(&mut self, freshness: f32) {
        if freshness.is_finite() {
            self.background_freshness = freshness.clamp(0.0, 1.0);
        }
    }

    #[must_use]
    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }

    #[must_use]
    pub fn queue(&self) -> &wgpu::Queue {
        &self.queue
    }

    #[must_use]
    pub fn surface_format(&self) -> wgpu::TextureFormat {
        self.config.format
    }

    /// Reports the alpha convention required by the native composition surface.
    /// Overlay passes must use the same convention as the final body resolve or
    /// transparent desktop composition will diverge from the opaque Lab oracle.
    #[must_use]
    pub fn premultiplied_output(&self) -> bool {
        self.premultiplied_output
    }

    /// Upload a top-down BGRA8 desktop crop without intermediate conversion.
    /// Returns false when the frame does not match the current overlay surface.
    pub fn update_background(
        &mut self,
        width: u32,
        height: u32,
        bytes_per_row: u32,
        bgra8: &[u8],
    ) -> bool {
        if width == 0
            || height == 0
            || bytes_per_row < width.saturating_mul(4)
            || bgra8.len()
                < bytes_per_row as usize * height.saturating_sub(1) as usize + width as usize * 4
        {
            return false;
        }
        if self.background_size != (width, height) {
            self.background_texture = create_background_texture(&self.device, width, height);
            self.background_size = (width, height);
            self.globals_bind_group = create_globals_bind_group(
                &self.device,
                &self.bind_group_layout,
                &self.globals_buffer,
                &self.background_texture,
                &self.background_sampler,
            );
            self.liquid_surface_bind_group = create_liquid_surface_bind_group(
                &self.device,
                &self.liquid_surface_bind_group_layout,
                &self.globals_buffer,
                &self.background_texture,
                &self.background_sampler,
                &self.density_view,
                &self.density_sampler,
                &self.macro_view,
                &self.material_flow_view,
                &self.studio_view,
                &self.studio_sampler,
            );
        }
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.background_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            bgra8,
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
        self.background_valid = true;
        true
    }

    pub fn render(&mut self, parameters: RenderParameters) -> RenderOutcome {
        self.render_with_overlay(parameters, |_, _, _, _| {})
    }

    /// Renders through the exact production GPU pipelines and synchronously
    /// reads back the final composited surface. This deliberately lives outside
    /// the real-time path and is intended for deterministic perceptual fixtures.
    pub fn render_capture(
        &mut self,
        parameters: RenderParameters,
    ) -> Result<CapturedFrame, RendererCaptureError> {
        self.render_capture_with_overlay(parameters, |_, _, _, _| {})
    }

    pub fn render_capture_with_overlay<F>(
        &mut self,
        parameters: RenderParameters,
        overlay: F,
    ) -> Result<CapturedFrame, RendererCaptureError>
    where
        F: FnOnce(&wgpu::Device, &wgpu::Queue, &mut wgpu::CommandEncoder, &wgpu::TextureView),
    {
        if !self.config.usage.contains(wgpu::TextureUsages::COPY_SRC) {
            return Err(RendererCaptureError::UnsupportedSurface);
        }
        let (outcome, capture) = self.render_internal(parameters, |_, _, _, _| {}, overlay, true);
        match outcome {
            RenderOutcome::Presented => {
                capture.expect("a presented capture request always produces a capture result")
            }
            RenderOutcome::Skipped => Err(RendererCaptureError::SurfaceUnavailable),
            RenderOutcome::OutOfMemory => Err(RendererCaptureError::OutOfMemory),
        }
    }

    pub fn render_with_overlay<F>(
        &mut self,
        parameters: RenderParameters,
        overlay: F,
    ) -> RenderOutcome
    where
        F: FnOnce(&wgpu::Device, &wgpu::Queue, &mut wgpu::CommandEncoder, &wgpu::TextureView),
    {
        self.render_internal(parameters, |_, _, _, _| {}, overlay, false)
            .0
    }

    /// Composites one callback below the organism and one above it. This keeps
    /// world landmarks such as the den behind the body while portable ecology
    /// objects remain available as foreground interaction props.
    pub fn render_with_layers<B, F>(
        &mut self,
        parameters: RenderParameters,
        background: B,
        foreground: F,
    ) -> RenderOutcome
    where
        B: FnOnce(&wgpu::Device, &wgpu::Queue, &mut wgpu::CommandEncoder, &wgpu::TextureView),
        F: FnOnce(&wgpu::Device, &wgpu::Queue, &mut wgpu::CommandEncoder, &wgpu::TextureView),
    {
        self.render_internal(parameters, background, foreground, false)
            .0
    }

    fn render_internal<B, F>(
        &mut self,
        parameters: RenderParameters,
        background: B,
        foreground: F,
        capture: bool,
    ) -> (
        RenderOutcome,
        Option<Result<CapturedFrame, RendererCaptureError>>,
    )
    where
        B: FnOnce(&wgpu::Device, &wgpu::Queue, &mut wgpu::CommandEncoder, &wgpu::TextureView),
        F: FnOnce(&wgpu::Device, &wgpu::Queue, &mut wgpu::CommandEncoder, &wgpu::TextureView),
    {
        let requested_render_scale = parameters.render_scale.clamp(1, SUPERSAMPLE_SCALE);
        let render_scale = supported_render_scale(
            self.config.width,
            self.config.height,
            requested_render_scale,
            self.maximum_texture_dimension_2d,
        )
        .expect("an already configured native surface remains supported");
        if render_scale != requested_render_scale {
            self.note_render_scale_fallback(
                self.config.width,
                self.config.height,
                requested_render_scale,
                render_scale,
            );
        } else {
            self.last_rejected_target = None;
        }
        if render_scale != self.render_scale {
            self.render_scale = render_scale;
            self.recreate_intermediate_targets();
        }
        let use_liquid = parameters.render_mode == BodyRenderMode::ParticlePbf
            && parameters.liquid.particle_count > 0;
        let use_cinematic =
            use_liquid && parameters.material_variant == MaterialVariant::CinematicJelly;
        // Every visible liquid fragment now contributes to the same implicit
        // density/material field. Current/Safe already used this path; Cinematic
        // no longer pays for or diverges through a separate bubble pass.
        let (particle_instances, particle_count) =
            pack_particles_for_material(&parameters.liquid, use_cinematic);
        let internal_features = self.internal_features.update(&parameters);
        let current_scissor = if use_liquid {
            liquid_scissor_rect(&self.config, parameters, self.organism_scale)
        } else {
            [0, 0, self.config.width, self.config.height]
        };
        let scissor = self
            .previous_liquid_scissor
            .map_or(current_scissor, |previous| {
                union_scissor(previous, current_scissor)
            });
        if use_liquid {
            self.queue.write_buffer(
                &self.particle_instance_buffer,
                0,
                bytemuck::cast_slice(&particle_instances[..particle_count]),
            );
        }
        self.queue.write_buffer(
            &self.globals_buffer,
            0,
            bytemuck::bytes_of(&globals_for_resolved(
                &self.config,
                parameters,
                self.organism_scale,
                self.premultiplied_output,
                self.review_background,
                self.background_valid,
                self.render_scale,
                Some(internal_features),
                self.studio_material_backdrop,
                self.background_uv_transform,
                self.background_freshness,
            )),
        );
        self.queue.write_buffer(
            &self.compose_globals_buffer,
            0,
            bytemuck::bytes_of(&compose_globals_for(
                &self.config,
                self.premultiplied_output,
                self.review_background,
                parameters,
                self.render_scale,
            )),
        );
        let shadow_scale = shadow_downsample_scale(parameters.shadow_feather);
        self.queue.write_buffer(
            &self.shadow_horizontal_globals_buffer,
            0,
            bytemuck::bytes_of(&shadow_filter_globals(
                self.config.width,
                self.config.height,
                shadow_scale,
                self.render_scale,
                parameters.shadow_feather,
                true,
            )),
        );
        self.queue.write_buffer(
            &self.shadow_vertical_globals_buffer,
            0,
            bytemuck::bytes_of(&shadow_filter_globals(
                self.config.width,
                self.config.height,
                shadow_scale,
                self.render_scale,
                parameters.shadow_feather,
                false,
            )),
        );
        let frame = match self.surface.get_current_texture() {
            Ok(frame) => frame,
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                self.surface.configure(&self.device, &self.config);
                return (RenderOutcome::Skipped, None);
            }
            Err(wgpu::SurfaceError::OutOfMemory) => return (RenderOutcome::OutOfMemory, None),
            Err(_) => return (RenderOutcome::Skipped, None),
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
            let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("transparent surface layer clear"),
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
        }
        if use_liquid {
            let scaled = scale_scissor(
                scissor,
                self.render_scale,
                self.config.width,
                self.config.height,
            );
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("2x unified additive liquid density pass"),
                color_attachments: &[
                    Some(wgpu::RenderPassColorAttachment {
                        view: &self.density_view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: if self.density_targets_initialized {
                                wgpu::LoadOp::Load
                            } else {
                                wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT)
                            },
                            store: wgpu::StoreOp::Store,
                        },
                    }),
                    Some(wgpu::RenderPassColorAttachment {
                        view: &self.material_flow_view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: if self.density_targets_initialized {
                                wgpu::LoadOp::Load
                            } else {
                                wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT)
                            },
                            store: wgpu::StoreOp::Store,
                        },
                    }),
                ],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_scissor_rect(scaled[0], scaled[1], scaled[2], scaled[3]);
            if self.density_targets_initialized {
                pass.set_pipeline(&self.dual_clear_pipeline);
                pass.draw(0..3, 0..1);
            }
            pass.set_bind_group(0, &self.globals_bind_group, &[]);
            pass.set_pipeline(&self.liquid_particle_pipeline);
            pass.set_vertex_buffer(0, self.particle_instance_buffer.slice(..));
            pass.draw(0..6, 0..particle_count as u32);
        }
        if use_cinematic {
            let macro_scissor = half_scissor(scissor, self.config.width, self.config.height);
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("half-native stable macro liquid reconstruction pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.macro_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: if self.macro_target_initialized {
                            wgpu::LoadOp::Load
                        } else {
                            wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT)
                        },
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_scissor_rect(
                macro_scissor[0],
                macro_scissor[1],
                macro_scissor[2],
                macro_scissor[3],
            );
            if self.macro_target_initialized {
                pass.set_pipeline(&self.single_clear_pipeline);
                pass.draw(0..3, 0..1);
            }
            pass.set_pipeline(&self.liquid_filter_pipeline);
            pass.set_bind_group(0, &self.liquid_filter_bind_group, &[]);
            pass.draw(0..3, 0..1);
        }
        {
            let scaled = scale_scissor(
                scissor,
                self.render_scale,
                self.config.width,
                self.config.height,
            );
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("2x linear morphic pet material pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.offscreen_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: if self.offscreen_target_initialized {
                            wgpu::LoadOp::Load
                        } else {
                            wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT)
                        },
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_scissor_rect(scaled[0], scaled[1], scaled[2], scaled[3]);
            if self.offscreen_target_initialized {
                pass.set_pipeline(&self.single_clear_pipeline);
                pass.draw(0..3, 0..1);
            }
            if use_liquid {
                pass.set_pipeline(if use_cinematic {
                    &self.liquid_surface_pipeline
                } else {
                    &self.liquid_surface_safe_pipeline
                });
                pass.set_bind_group(0, &self.liquid_surface_bind_group, &[]);
            } else {
                pass.set_pipeline(&self.pipeline);
                pass.set_bind_group(0, &self.globals_bind_group, &[]);
            }
            pass.draw(0..3, 0..1);
        }
        {
            let (
                mask_a_view,
                mask_b_view,
                downsample_bind_group,
                horizontal_bind_group,
                vertical_bind_group,
            ) = if shadow_scale == SHADOW_QUARTER_SCALE {
                (
                    &self.shadow_quarter_a_view,
                    &self.shadow_quarter_b_view,
                    &self.shadow_quarter_downsample_bind_group,
                    &self.shadow_quarter_horizontal_bind_group,
                    &self.shadow_quarter_vertical_bind_group,
                )
            } else {
                (
                    &self.shadow_half_a_view,
                    &self.shadow_half_b_view,
                    &self.shadow_half_downsample_bind_group,
                    &self.shadow_half_horizontal_bind_group,
                    &self.shadow_half_vertical_bind_group,
                )
            };
            let mask_scissor =
                downsample_scissor(scissor, shadow_scale, self.config.width, self.config.height);
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("area-prefiltered shadow coverage pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: mask_a_view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            // Masks are compact R16 targets. Clearing them is
                            // cheaper than allowing an old moving silhouette to
                            // leak into the current Gaussian support.
                            load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });
                pass.set_scissor_rect(
                    mask_scissor[0],
                    mask_scissor[1],
                    mask_scissor[2],
                    mask_scissor[3],
                );
                pass.set_pipeline(&self.shadow_downsample_pipeline);
                pass.set_bind_group(0, downsample_bind_group, &[]);
                pass.draw(0..3, 0..1);
            }
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("horizontal paired-tap Gaussian shadow pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: mask_b_view,
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
                pass.set_scissor_rect(
                    mask_scissor[0],
                    mask_scissor[1],
                    mask_scissor[2],
                    mask_scissor[3],
                );
                pass.set_pipeline(&self.shadow_blur_pipeline);
                pass.set_bind_group(0, horizontal_bind_group, &[]);
                pass.draw(0..3, 0..1);
            }
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("vertical paired-tap Gaussian shadow pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: mask_a_view,
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
                pass.set_scissor_rect(
                    mask_scissor[0],
                    mask_scissor[1],
                    mask_scissor[2],
                    mask_scissor[3],
                );
                pass.set_pipeline(&self.shadow_blur_pipeline);
                pass.set_bind_group(0, vertical_bind_group, &[]);
                pass.draw(0..3, 0..1);
            }
        }
        background(&self.device, &self.queue, &mut encoder, &view);
        {
            let compose_scissor = if self.review_background == ReviewBackground::Transparent {
                scissor
            } else {
                [0, 0, self.config.width, self.config.height]
            };
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("native morphic pet resolve and footprint shadow pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
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
            pass.set_pipeline(&self.compose_pipeline);
            pass.set_bind_group(
                0,
                if shadow_scale == SHADOW_QUARTER_SCALE {
                    &self.compose_quarter_bind_group
                } else {
                    &self.compose_half_bind_group
                },
                &[],
            );
            pass.set_scissor_rect(
                compose_scissor[0],
                compose_scissor[1],
                compose_scissor[2],
                compose_scissor[3],
            );
            pass.draw(0..3, 0..1);
        }
        foreground(&self.device, &self.queue, &mut encoder, &view);
        let capture_format = capture.then_some(match self.config.format {
            wgpu::TextureFormat::Rgba8Unorm | wgpu::TextureFormat::Rgba8UnormSrgb => Ok(false),
            wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb => Ok(true),
            format => Err(RendererCaptureError::UnsupportedFormat(format)),
        });
        let unpadded_bytes_per_row = self.config.width.saturating_mul(4);
        let alignment = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let padded_bytes_per_row = unpadded_bytes_per_row.div_ceil(alignment) * alignment;
        let capture_buffer = capture_format
            .as_ref()
            .and_then(|format| format.as_ref().ok())
            .map(|_| {
                self.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("morphic pet perceptual capture readback"),
                    size: u64::from(padded_bytes_per_row) * u64::from(self.config.height),
                    usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                    mapped_at_creation: false,
                })
            });
        if let Some(buffer) = &capture_buffer {
            encoder.copy_texture_to_buffer(
                wgpu::TexelCopyTextureInfo {
                    texture: &frame.texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::TexelCopyBufferInfo {
                    buffer,
                    layout: wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(padded_bytes_per_row),
                        rows_per_image: Some(self.config.height),
                    },
                },
                wgpu::Extent3d {
                    width: self.config.width,
                    height: self.config.height,
                    depth_or_array_layers: 1,
                },
            );
        }
        let submission = self.queue.submit(Some(encoder.finish()));
        if use_liquid {
            self.density_targets_initialized = true;
        }
        if use_cinematic {
            self.macro_target_initialized = true;
        }
        self.offscreen_target_initialized = true;
        self.previous_liquid_scissor = use_liquid.then_some(current_scissor);
        frame.present();
        let capture_result = match (capture_format, capture_buffer) {
            (None, None) => None,
            (Some(Err(error)), None) => Some(Err(error)),
            (Some(Ok(bgra)), Some(buffer)) => Some((|| {
                let slice = buffer.slice(..);
                let (sender, receiver) = std::sync::mpsc::sync_channel(1);
                slice.map_async(wgpu::MapMode::Read, move |result| {
                    let _ = sender.send(result);
                });
                self.device.poll(wgpu::PollType::Wait {
                    submission_index: Some(submission),
                    timeout: Some(std::time::Duration::from_secs(10)),
                })?;
                receiver
                    .recv()
                    .map_err(|_| RendererCaptureError::CallbackUnavailable)??;
                let mapped = slice.get_mapped_range();
                let width = self.config.width;
                let height = self.config.height;
                let mut rgba8 = vec![0_u8; width as usize * height as usize * 4];
                for row in 0..height as usize {
                    let source = &mapped[row * padded_bytes_per_row as usize
                        ..row * padded_bytes_per_row as usize + unpadded_bytes_per_row as usize];
                    let target = &mut rgba8[row * unpadded_bytes_per_row as usize
                        ..(row + 1) * unpadded_bytes_per_row as usize];
                    target.copy_from_slice(source);
                    if bgra {
                        for pixel in target.chunks_exact_mut(4) {
                            pixel.swap(0, 2);
                        }
                    }
                }
                drop(mapped);
                buffer.unmap();
                Ok(CapturedFrame {
                    width,
                    height,
                    rgba8,
                })
            })()),
            _ => unreachable!("capture format and buffer construction stay paired"),
        };
        (RenderOutcome::Presented, capture_result)
    }
}

fn surface_dimensions_changed(
    current_width: u32,
    current_height: u32,
    requested: PhysicalSize<u32>,
) -> bool {
    requested.width > 0
        && requested.height > 0
        && (requested.width != current_width || requested.height != current_height)
}

fn supported_render_scale(
    width: u32,
    height: u32,
    requested_scale: u32,
    maximum_texture_dimension_2d: u32,
) -> Option<u32> {
    if width == 0 || height == 0 || maximum_texture_dimension_2d == 0 {
        return None;
    }
    let requested_scale = requested_scale.clamp(1, SUPERSAMPLE_SCALE);
    (1..=requested_scale).rev().find(|scale| {
        u64::from(width) * u64::from(*scale) <= u64::from(maximum_texture_dimension_2d)
            && u64::from(height) * u64::from(*scale) <= u64::from(maximum_texture_dimension_2d)
    })
}

fn liquid_scissor_rect(
    config: &wgpu::SurfaceConfiguration,
    parameters: RenderParameters,
    mesh_scale: f32,
) -> [u32; 4] {
    let organism_scale = projection_scale(mesh_scale, parameters.render_mode)
        * bounded(parameters.presentation_scale, 0.50, 3.0, 1.0);
    let aspect = config.width as f32 / config.height.max(1) as f32;
    let mut minimum = Vec2::splat(f32::INFINITY);
    let mut maximum = Vec2::splat(f32::NEG_INFINITY);
    let to_pixel = |local: Vec2| {
        let point = (local + parameters.presentation_offset) / organism_scale.max(1.0e-5);
        Vec2::new(
            (point.x / aspect + 1.0) * 0.5 * config.width as f32,
            (1.0 - point.y) * 0.5 * config.height as f32,
        )
    };
    let pixels_per_local = config.height as f32 / (2.0 * organism_scale.max(1.0e-5));
    for particle in &parameters.liquid.particles[..parameters.liquid.particle_count] {
        let center = to_pixel(particle.position);
        let extent =
            Vec2::splat(particle.major_radius.max(particle.minor_radius) * pixels_per_local);
        minimum = minimum.min(center - extent);
        maximum = maximum.max(center + extent);
    }
    for bubble in &parameters.liquid.bubbles[..parameters.liquid.bubble_count] {
        let center = to_pixel(bubble.position);
        let extent = Vec2::splat(bubble.radius * pixels_per_local);
        minimum = minimum.min(center - extent);
        maximum = maximum.max(center + extent);
    }
    if !minimum.is_finite() || !maximum.is_finite() {
        return [0, 0, config.width.max(1), config.height.max(1)];
    }
    let organism_minimum = minimum;
    let organism_maximum = maximum;
    let rim_padding = 12.0 + parameters.material_edge_light_width.clamp(2.0, 32.0);
    minimum -= Vec2::splat(rim_padding);
    maximum += Vec2::splat(rim_padding);

    // The macOS-style drop shadow remains the organism's full alpha silhouette,
    // merely offset and feathered behind it. Keep its old and new extents in the
    // scissor so motion never leaves stale translucent pixels.
    let shadow_offset = Vec2::new(
        parameters.shadow_horizontal_offset.clamp(-96.0, 96.0),
        parameters.shadow_vertical_offset.clamp(-96.0, 96.0),
    );
    let feather = parameters.shadow_feather.clamp(2.0, 128.0);
    let shadow_scale = shadow_downsample_scale(feather) as f32;
    let shadow_padding = (feather / shadow_scale).ceil() * shadow_scale;
    let (shadow_minimum, shadow_maximum) = shadow_work_bounds(
        organism_minimum,
        organism_maximum,
        shadow_offset,
        shadow_padding,
    );
    minimum = minimum.min(shadow_minimum);
    maximum = maximum.max(shadow_maximum);
    let x0 = minimum
        .x
        .floor()
        .clamp(0.0, config.width.saturating_sub(1) as f32) as u32;
    let y0 = minimum
        .y
        .floor()
        .clamp(0.0, config.height.saturating_sub(1) as f32) as u32;
    let x1 = maximum.x.ceil().clamp((x0 + 1) as f32, config.width as f32) as u32;
    let y1 = maximum
        .y
        .ceil()
        .clamp((y0 + 1) as f32, config.height as f32) as u32;
    [x0, y0, x1 - x0, y1 - y0]
}

fn shadow_work_bounds(
    organism_minimum: Vec2,
    organism_maximum: Vec2,
    shadow_offset: Vec2,
    shadow_padding: f32,
) -> (Vec2, Vec2) {
    let padding = Vec2::splat(shadow_padding.max(0.0));
    let mask_minimum = organism_minimum - padding;
    let mask_maximum = organism_maximum + padding;
    let compose_minimum = mask_minimum + shadow_offset;
    let compose_maximum = mask_maximum + shadow_offset;
    (
        mask_minimum.min(compose_minimum),
        mask_maximum.max(compose_maximum),
    )
}

fn union_scissor(first: [u32; 4], second: [u32; 4]) -> [u32; 4] {
    let x0 = first[0].min(second[0]);
    let y0 = first[1].min(second[1]);
    let x1 = first[0]
        .saturating_add(first[2])
        .max(second[0].saturating_add(second[2]));
    let y1 = first[1]
        .saturating_add(first[3])
        .max(second[1].saturating_add(second[3]));
    [
        x0,
        y0,
        x1.saturating_sub(x0).max(1),
        y1.saturating_sub(y0).max(1),
    ]
}

fn scale_scissor(scissor: [u32; 4], scale: u32, width: u32, height: u32) -> [u32; 4] {
    let scale = scale.max(1);
    let target_width = width.max(1).saturating_mul(scale);
    let target_height = height.max(1).saturating_mul(scale);
    let x = scissor[0].saturating_mul(scale).min(target_width - 1);
    let y = scissor[1].saturating_mul(scale).min(target_height - 1);
    let w = scissor[2]
        .saturating_mul(scale)
        .min(target_width - x)
        .max(1);
    let h = scissor[3]
        .saturating_mul(scale)
        .min(target_height - y)
        .max(1);
    [x, y, w, h]
}

fn half_scissor(scissor: [u32; 4], width: u32, height: u32) -> [u32; 4] {
    let target_width = width.max(1).div_ceil(2);
    let target_height = height.max(1).div_ceil(2);
    let x = (scissor[0] / 2).min(target_width - 1);
    let y = (scissor[1] / 2).min(target_height - 1);
    let x1 = scissor[0]
        .saturating_add(scissor[2])
        .div_ceil(2)
        .min(target_width);
    let y1 = scissor[1]
        .saturating_add(scissor[3])
        .div_ceil(2)
        .min(target_height);
    [
        x,
        y,
        x1.saturating_sub(x).max(1),
        y1.saturating_sub(y).max(1),
    ]
}

fn downsample_scissor(scissor: [u32; 4], downsample: u32, width: u32, height: u32) -> [u32; 4] {
    let downsample = downsample.max(1);
    let target_width = width.max(1).div_ceil(downsample);
    let target_height = height.max(1).div_ceil(downsample);
    let x = (scissor[0] / downsample).min(target_width - 1);
    let y = (scissor[1] / downsample).min(target_height - 1);
    let x1 = scissor[0]
        .saturating_add(scissor[2])
        .div_ceil(downsample)
        .min(target_width);
    let y1 = scissor[1]
        .saturating_add(scissor[3])
        .div_ceil(downsample)
        .min(target_height);
    [
        x,
        y,
        x1.saturating_sub(x).max(1),
        y1.saturating_sub(y).max(1),
    ]
}

fn create_background_texture(device: &wgpu::Device, width: u32, height: u32) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("desktop refraction texture"),
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

fn create_offscreen_texture(
    device: &wgpu::Device,
    width: u32,
    height: u32,
    render_scale: u32,
) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("2x linear morphic pet intermediate"),
        size: wgpu::Extent3d {
            width: width.max(1).saturating_mul(render_scale),
            height: height.max(1).saturating_mul(render_scale),
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: INTERMEDIATE_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    })
}

fn create_shadow_texture(
    device: &wgpu::Device,
    width: u32,
    height: u32,
    downsample: u32,
    suffix: &str,
) -> (wgpu::Texture, wgpu::TextureView) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(&format!("{suffix} R16 Gaussian shadow mask")),
        size: wgpu::Extent3d {
            width: width.max(1).div_ceil(downsample.max(1)),
            height: height.max(1).div_ceil(downsample.max(1)),
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::R16Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    (texture, view)
}

fn create_density_texture(
    device: &wgpu::Device,
    width: u32,
    height: u32,
    render_scale: u32,
) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("2x HDR liquid density field"),
        size: wgpu::Extent3d {
            width: width.max(1).saturating_mul(render_scale),
            height: height.max(1).saturating_mul(render_scale),
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: INTERMEDIATE_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    })
}

fn create_material_flow_texture(
    device: &wgpu::Device,
    width: u32,
    height: u32,
    render_scale: u32,
) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("2x advected liquid material field"),
        size: wgpu::Extent3d {
            width: width.max(1).saturating_mul(render_scale),
            height: height.max(1).saturating_mul(render_scale),
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: INTERMEDIATE_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    })
}

fn create_macro_texture(device: &wgpu::Device, width: u32, height: u32) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("half-native stable macro liquid field"),
        size: wgpu::Extent3d {
            width: width.max(1).div_ceil(2),
            height: height.max(1).div_ceil(2),
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: INTERMEDIATE_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    })
}

fn create_studio_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
) -> (wgpu::Texture, wgpu::TextureView) {
    const SIZE: u32 = 192;
    let mut pixels = vec![0_u8; (SIZE * SIZE * 4) as usize];
    for y in 0..SIZE {
        for x in 0..SIZE {
            let uv = Vec2::new(
                (x as f32 + 0.5) / SIZE as f32,
                (y as f32 + 0.5) / SIZE as f32,
            );
            let big = rounded_box_light(uv, Vec2::new(0.72, 0.27), Vec2::new(0.082, 0.175), 0.048);
            let fill = rounded_box_light(uv, Vec2::new(0.32, 0.20), Vec2::splat(0.058), 0.034);
            let horizon = (1.0 - ((uv.y - 0.58) / 0.24).abs())
                .clamp(0.0, 1.0)
                .powf(3.0);
            let vignette = (1.0 - (uv - Vec2::splat(0.5)).length() * 0.44).clamp(0.45, 1.0);
            let color = (Vec3::new(0.018, 0.026, 0.036)
                + Vec3::new(1.00, 1.06, 1.12) * big * 1.08
                + Vec3::new(0.78, 0.90, 1.00) * fill * 0.72
                + Vec3::new(0.12, 0.17, 0.22) * horizon * 0.22)
                * vignette;
            let offset = ((y * SIZE + x) * 4) as usize;
            pixels[offset] = linear_to_srgb_u8(color.x);
            pixels[offset + 1] = linear_to_srgb_u8(color.y);
            pixels[offset + 2] = linear_to_srgb_u8(color.z);
            pixels[offset + 3] = 255;
        }
    }
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("procedural rounded-softbox studio environment"),
        size: wgpu::Extent3d {
            width: SIZE,
            height: SIZE,
            depth_or_array_layers: 1,
        },
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
        &pixels,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(SIZE * 4),
            rows_per_image: Some(SIZE),
        },
        wgpu::Extent3d {
            width: SIZE,
            height: SIZE,
            depth_or_array_layers: 1,
        },
    );
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    (texture, view)
}

fn rounded_box_light(uv: Vec2, center: Vec2, half_size: Vec2, radius: f32) -> f32 {
    let q = (uv - center).abs() - half_size + Vec2::splat(radius);
    let distance = q.max(Vec2::ZERO).length() + q.x.max(q.y).min(0.0) - radius;
    smoothstep(0.032, -0.016, distance)
}

fn smoothstep(edge0: f32, edge1: f32, value: f32) -> f32 {
    let t = ((value - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn linear_to_srgb_u8(value: f32) -> u8 {
    let value = value.clamp(0.0, 1.0);
    let encoded = if value <= 0.003_130_8 {
        value * 12.92
    } else {
        1.055 * value.powf(1.0 / 2.4) - 0.055
    };
    (encoded * 255.0 + 0.5) as u8
}

fn create_globals_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    globals_buffer: &wgpu::Buffer,
    background_texture: &wgpu::Texture,
    background_sampler: &wgpu::Sampler,
) -> wgpu::BindGroup {
    let background_view = background_texture.create_view(&wgpu::TextureViewDescriptor::default());
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("morphic pet globals and desktop background"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: globals_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(&background_view),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::Sampler(background_sampler),
            },
        ],
    })
}

#[allow(clippy::too_many_arguments)]
fn create_liquid_surface_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    globals_buffer: &wgpu::Buffer,
    background_texture: &wgpu::Texture,
    background_sampler: &wgpu::Sampler,
    density_view: &wgpu::TextureView,
    density_sampler: &wgpu::Sampler,
    macro_view: &wgpu::TextureView,
    material_flow_view: &wgpu::TextureView,
    studio_view: &wgpu::TextureView,
    studio_sampler: &wgpu::Sampler,
) -> wgpu::BindGroup {
    let background_view = background_texture.create_view(&wgpu::TextureViewDescriptor::default());
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("implicit liquid surface resources"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: globals_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(&background_view),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::Sampler(background_sampler),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::TextureView(density_view),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: wgpu::BindingResource::Sampler(density_sampler),
            },
            wgpu::BindGroupEntry {
                binding: 5,
                resource: wgpu::BindingResource::TextureView(macro_view),
            },
            wgpu::BindGroupEntry {
                binding: 6,
                resource: wgpu::BindingResource::TextureView(material_flow_view),
            },
            wgpu::BindGroupEntry {
                binding: 7,
                resource: wgpu::BindingResource::TextureView(studio_view),
            },
            wgpu::BindGroupEntry {
                binding: 8,
                resource: wgpu::BindingResource::Sampler(studio_sampler),
            },
        ],
    })
}

fn create_liquid_filter_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    globals_buffer: &wgpu::Buffer,
    density_view: &wgpu::TextureView,
    density_sampler: &wgpu::Sampler,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("liquid macro reconstruction resources"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: globals_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(density_view),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::Sampler(density_sampler),
            },
        ],
    })
}

fn create_shadow_filter_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    globals_buffer: &wgpu::Buffer,
    source_view: &wgpu::TextureView,
    source_sampler: &wgpu::Sampler,
    label: &str,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some(label),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: globals_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(source_view),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::Sampler(source_sampler),
            },
        ],
    })
}

fn create_compose_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    compose_globals_buffer: &wgpu::Buffer,
    offscreen_view: &wgpu::TextureView,
    offscreen_sampler: &wgpu::Sampler,
    shadow_view: &wgpu::TextureView,
    shadow_sampler: &wgpu::Sampler,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("morphic pet resolve resources"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: compose_globals_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(offscreen_view),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::Sampler(offscreen_sampler),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::TextureView(shadow_view),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: wgpu::BindingResource::Sampler(shadow_sampler),
            },
        ],
    })
}

fn shadow_downsample_scale(feather: f32) -> u32 {
    if feather.clamp(2.0, 128.0) > SHADOW_QUARTER_THRESHOLD {
        SHADOW_QUARTER_SCALE
    } else {
        SHADOW_HALF_SCALE
    }
}

fn shadow_filter_globals(
    width: u32,
    height: u32,
    downsample: u32,
    render_scale: u32,
    feather: f32,
    horizontal: bool,
) -> ShadowFilterGlobals {
    let downsample = downsample.clamp(SHADOW_HALF_SCALE, SHADOW_QUARTER_SCALE);
    // Body Lab's feather is the truncated 3-sigma radius in native pixels.
    let sigma = (feather.clamp(2.0, 128.0) / (3.0 * downsample as f32)).max(0.25);
    let radius = (sigma * 3.0).ceil().clamp(1.0, 32.0) as usize;
    let pair_count = radius.div_ceil(2).min(MAX_SHADOW_PAIRED_TAPS);
    let mut paired_taps = [[0.0; 4]; MAX_SHADOW_PAIRED_TAPS / 2];
    let mut normalization = 1.0_f32;

    for pair_index in 0..pair_count {
        let first_index = pair_index * 2 + 1;
        let first_weight = (-0.5 * (first_index as f32 / sigma).powi(2)).exp();
        let second_index = first_index + 1;
        let second_weight = if second_index <= radius {
            (-0.5 * (second_index as f32 / sigma).powi(2)).exp()
        } else {
            0.0
        };
        let combined_weight = first_weight + second_weight;
        let fractional_offset = if combined_weight > f32::EPSILON {
            (first_index as f32 * first_weight + second_index as f32 * second_weight)
                / combined_weight
        } else {
            first_index as f32
        };
        let packed = &mut paired_taps[pair_index / 2];
        let lane = (pair_index % 2) * 2;
        packed[lane] = fractional_offset;
        packed[lane + 1] = combined_weight;
        normalization += combined_weight * 2.0;
    }

    let inverse_normalization = normalization.recip();
    for packed in &mut paired_taps {
        packed[1] *= inverse_normalization;
        packed[3] *= inverse_normalization;
    }
    let target_width = width.max(1).div_ceil(downsample) as f32;
    let target_height = height.max(1).div_ceil(downsample) as f32;
    ShadowFilterGlobals {
        direction_meta: [
            if horizontal {
                target_width.recip()
            } else {
                0.0
            },
            if horizontal {
                0.0
            } else {
                target_height.recip()
            },
            inverse_normalization,
            pair_count as f32,
        ],
        // The source material target can be native or 2x. The shader needs the
        // complete source-to-mask footprint to area-prefilter either path.
        downsample: [
            downsample as f32,
            downsample.saturating_mul(render_scale.max(1)) as f32,
            0.0,
            0.0,
        ],
        paired_taps,
    }
}

fn compose_globals_for(
    config: &wgpu::SurfaceConfiguration,
    premultiplied_output: bool,
    review_background: ReviewBackground,
    parameters: RenderParameters,
    render_scale: u32,
) -> ComposeGlobals {
    ComposeGlobals {
        output_mode: [
            if premultiplied_output { 1.0 } else { 0.0 },
            review_background.shader_value(),
            1.0 / config.width.max(1) as f32,
            1.0 / config.height.max(1) as f32,
        ],
        // Preserve the incumbent full-silhouette drop shadow. The lab only authors
        // offset, feather, opacity, and tint around this stable macOS-like model.
        shadow: [
            bounded(parameters.shadow_horizontal_offset, -96.0, 96.0, 0.0),
            bounded(parameters.shadow_vertical_offset, -96.0, 96.0, 9.0),
            bounded(parameters.shadow_opacity, 0.0, 0.50, 0.050),
            bounded(parameters.exposure, 0.25, 3.0, 1.0),
        ],
        shadow_style: [
            bounded(parameters.shadow_feather, 2.0, 128.0, 22.0),
            bounded(parameters.shadow_color.x, 0.0, 1.0, 0.008),
            bounded(parameters.shadow_color.y, 0.0, 1.0, 0.012),
            bounded(parameters.shadow_color.z, 0.0, 1.0, 0.020),
        ],
        post: [
            if parameters.material_variant == MaterialVariant::CinematicJelly {
                bounded(parameters.material_bloom_strength, 0.0, 3.0, 0.55)
            } else {
                0.0
            },
            render_scale.clamp(1, SUPERSAMPLE_SCALE) as f32,
            0.0,
            0.0,
        ],
    }
}

impl Default for RenderParameters {
    fn default() -> Self {
        Self {
            render_mode: BodyRenderMode::AnalyticJelly,
            render_scale: SUPERSAMPLE_SCALE,
            presentation_scale: 1.0,
            presentation_offset: Vec2::ZERO,
            debug_view: DebugView::Material,
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
            iris_hsv: Vec3::new(0.055, 0.82, 0.48),
            override_iris_color: false,
            gaze: Vec2::ZERO,
            vergence: 0.0,
            pupil_size: 0.52,
            pupil_asymmetry: 0.0,
            blink_left: 0.0,
            blink_right: 0.0,
            squint: 0.0,
            brow_raise: 0.0,
            brow_tension: 0.0,
            brow_asymmetry: 0.0,
            geometry: lifecore::FaceGeometry::default(),
            eye_aperture: 1.0,
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
            morph_mode2: Vec2::ZERO,
            morph_mode3: Vec2::ZERO,
            morph_mode4: Vec2::ZERO,
            morph_area_scale: 1.0,
            pattern_scale: 2.0,
            pattern_contrast: 0.35,
            pattern_seed: 0,
            pulse: 0.0,
            shell_opacity: 0.96,
            inner_density: 0.76,
            translucency: 0.28,
            core_glow: 0.08,
            halo: 0.04,
            iris_activity: 0.24,
            eye_wetness: 0.62,
            flow_strength: 0.20,
            flow_scale: 2.0,
            flow_speed: 0.14,
            flow_phase: 0.0,
            flow_warp: 0.08,
            core_size: 0.36,
            cornea_strength: 0.62,
            iris_scale: 0.62,
            limbal_strength: 0.44,
            iris_fiber_count: 32.0,
            iris_contrast: 0.20,
            droplet_energy: 0.22,
            droplet_cohesion: 0.65,
            droplet_spread: 0.76,
            droplets: [DropletRenderState::default(); MAX_DROPLETS],
            liquid: LiquidRenderState::default(),
            material_absorption: 1.0,
            material_scattering: 0.72,
            material_thickness: 1.0,
            material_refraction: 1.0,
            material_blur: 1.15,
            material_rim_strength: 0.90,
            material_rim_power: 2.8,
            material_broad_specular: 0.38,
            material_broad_specular_power: 12.0,
            material_tight_specular: 0.95,
            material_tight_specular_power: 64.0,
            material_emission: 0.16,
            material_fresnel_f0: 0.055,
            material_core_level: 1.35,
            material_thickness_gamma: 0.78,
            material_pseudo_depth: 0.30,
            material_normal_scale: 2.20,
            material_light_wrap: 0.55,
            material_ambient_scatter: 0.22,
            material_direct_scatter: 0.55,
            material_transmission_hue_preservation: 0.72,
            material_opacity: 0.92,
            material_variant: MaterialVariant::CinematicJelly,
            material_cinematic_smoothing: 0.68,
            material_internal_orb_count: 8,
            material_internal_orb_intensity: 0.92,
            material_internal_orb_size: 0.045,
            material_internal_orb_halo: 1.20,
            material_internal_orb_speed: 0.32,
            material_internal_orb_depth: 0.62,
            material_internal_orb_spread: 0.72,
            material_studio_intensity: 1.45,
            material_studio_base_roughness: 0.32,
            material_studio_coat_roughness: 0.09,
            material_narrow_rim_strength: 1.0,
            material_broad_rim_strength: 0.62,
            material_rim_saturation: 1.15,
            material_edge_light_width: 18.0,
            material_caustic_strength: 0.28,
            material_caustic_scale: 3.4,
            material_caustic_speed: 0.32,
            material_caustic_dispersion: 0.45,
            material_rounded_highlight_strength: 0.85,
            material_highlight_tint: 0.16,
            material_soul_glow_count: 4,
            material_soul_glow_strength: 0.20,
            material_soul_glow_size: 0.13,
            material_soul_glow_speed: 0.10,
            material_soul_glow_pulse: 0.18,
            material_soul_glow_feather: 0.68,
            material_bloom_strength: 0.55,
            liquid_iso_threshold: 0.34,
            face_visible: true,
            face_eye_highlight_scale: 1.45,
            face_eye_socket_strength: 0.18,
            face_relief_strength: 0.55,
            face_relief_darkness: 0.72,
            face_relief_coat_strength: 1.20,
            shadow_horizontal_offset: 0.0,
            shadow_vertical_offset: 9.0,
            shadow_feather: 22.0,
            shadow_opacity: 0.050,
            shadow_color: Vec3::new(0.008, 0.012, 0.020),
            exposure: 1.0,
            occlusion_mode: OcclusionMode::Front,
            occlusion_edge: 0.0,
            occlusion_softness: 0.02,
        }
    }
}

pub(crate) fn organism_scale(mesh: &ProceduralMesh) -> f32 {
    let extent = mesh.maximum - mesh.minimum;
    ((extent.x.max(extent.y) * 0.72).clamp(0.78, 1.35) * 1.12).min(1.51)
}

pub(crate) const fn projection_scale(mesh_scale: f32, render_mode: BodyRenderMode) -> f32 {
    match render_mode {
        BodyRenderMode::ParticlePbf => PARTICLE_PBF_ORGANISM_SCALE,
        BodyRenderMode::AnalyticJelly => mesh_scale,
    }
}

fn globals_for(
    config: &wgpu::SurfaceConfiguration,
    parameters: RenderParameters,
    organism_scale: f32,
    premultiplied_output: bool,
    review_background: ReviewBackground,
    background_valid: bool,
    render_scale: u32,
) -> Globals {
    globals_for_resolved(
        config,
        parameters,
        organism_scale,
        premultiplied_output,
        review_background,
        background_valid,
        render_scale,
        None,
        false,
        [1.0, 1.0, 0.0, 0.0],
        1.0,
    )
}

#[allow(clippy::too_many_arguments)]
fn globals_for_resolved(
    config: &wgpu::SurfaceConfiguration,
    parameters: RenderParameters,
    organism_scale: f32,
    premultiplied_output: bool,
    review_background: ReviewBackground,
    background_valid: bool,
    render_scale: u32,
    internal_features: Option<InternalFeatureFrame>,
    studio_material_backdrop: bool,
    background_uv_transform: [f32; 4],
    background_freshness: f32,
) -> Globals {
    let organism_scale = projection_scale(organism_scale, parameters.render_mode)
        * bounded(parameters.presentation_scale, 0.50, 3.0, 1.0);
    let aspect = config.width as f32 / config.height.max(1) as f32;
    let features = internal_features.unwrap_or_else(|| {
        let (orb_position_radius, orb_color_intensity) = internal_glow_orbs(&parameters);
        InternalFeatureFrame {
            orb_position_radius,
            orb_color_intensity,
            soul_position_radius: soul_glow_lobes(&parameters),
        }
    });
    Globals {
        face_lids: parameters.geometry.sanitized().lids,
        face_brows: parameters.geometry.sanitized().brows,
        face_mouth: parameters.geometry.sanitized().mouth,
        face_eye: [
            bounded(parameters.eye_aperture, 0.0, 1.0, 1.0),
            0.0,
            0.0,
            0.0,
        ],
        viewport_time: [
            aspect,
            parameters.time,
            parameters.arousal.clamp(0.0, 1.0),
            parameters.glow.clamp(0.0, 1.0),
        ],
        body_shape: if parameters.render_mode == BodyRenderMode::ParticlePbf {
            [
                parameters.body_width.clamp(0.35, 1.2),
                parameters.body_length.clamp(0.55, 1.5),
                bounded(parameters.presentation_offset.x, -8.0, 8.0, 0.0),
                bounded(parameters.presentation_offset.y, -8.0, 8.0, 0.0),
            ]
        } else {
            [
                parameters.body_width.clamp(0.35, 1.2),
                parameters.body_length.clamp(0.55, 1.5),
                parameters.body_roundness.clamp(0.2, 1.0),
                parameters.head_ratio.clamp(0.28, 0.78),
            ]
        },
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
        iris_hsv: parameters
            .iris_hsv
            .extend(if parameters.override_iris_color {
                1.0
            } else {
                0.0
            })
            .to_array(),
        gaze_pupil: [
            bounded(parameters.gaze.x, -1.0, 1.0, 0.0),
            bounded(parameters.gaze.y, -1.0, 1.0, 0.0),
            bounded(parameters.vergence, 0.0, 0.2, 0.0),
            bounded(parameters.pupil_size, 0.15, 0.95, 0.52),
        ],
        lids_brows: [
            bounded(parameters.blink_left, 0.0, 1.0, 0.0),
            bounded(parameters.blink_right, 0.0, 1.0, 0.0),
            bounded(parameters.squint, 0.0, 1.0, 0.0),
            bounded(parameters.brow_raise, -1.0, 1.0, 0.0),
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
        physiology_a: [
            bounded(parameters.pulse, -0.035, 0.035, 0.0),
            bounded(parameters.shell_opacity, 0.90, 0.995, 0.96),
            bounded(parameters.inner_density, 0.55, 0.92, 0.76),
            bounded(parameters.translucency, 0.12, 0.55, 0.28),
        ],
        physiology_b: [
            bounded(parameters.core_glow, 0.02, 0.72, 0.08),
            bounded(parameters.halo, 0.0, 0.30, 0.04),
            bounded(parameters.iris_activity, 0.0, 1.0, 0.24),
            bounded(parameters.eye_wetness, 0.0, 1.0, 0.62),
        ],
        flow: [
            bounded(parameters.flow_strength, 0.0, 2.0, 0.20),
            bounded(parameters.flow_speed, 0.0, 0.60, 0.14),
            finite_or(parameters.flow_phase, 0.0).rem_euclid(std::f32::consts::TAU),
            bounded(parameters.flow_warp, 0.0, 0.18, 0.08),
        ],
        eye_detail: [
            bounded(parameters.iris_scale, 0.58, 0.68, 0.62),
            bounded(parameters.limbal_strength, 0.0, 0.8, 0.44),
            bounded(parameters.iris_fiber_count, 16.0, 52.0, 32.0),
            bounded(parameters.iris_contrast, 0.0, 0.42, 0.20),
        ],
        visual_detail: [
            bounded(parameters.core_size, 0.24, 0.52, 0.36),
            bounded(parameters.cornea_strength, 0.0, 1.0, 0.62),
            bounded(parameters.flow_scale, 0.8, 3.4, 2.0),
            0.0,
        ],
        droplet_meta: [
            parameters
                .droplets
                .iter()
                .filter(|droplet| droplet.activity > 0.001)
                .count() as f32,
            bounded(parameters.droplet_energy, 0.0, 1.0, 0.22),
            bounded(parameters.droplet_cohesion, 0.0, 1.0, 0.65),
            bounded(parameters.droplet_spread, 0.0, 1.4, 0.76),
        ],
        morph_a: [
            bounded(parameters.morph_mode2.x, -0.105, 0.105, 0.0),
            bounded(parameters.morph_mode2.y, -0.105, 0.105, 0.0),
            bounded(parameters.morph_mode3.x, -0.050, 0.050, 0.0),
            bounded(parameters.morph_mode3.y, -0.050, 0.050, 0.0),
        ],
        morph_b: [
            bounded(parameters.morph_mode4.x, -0.026, 0.026, 0.0),
            bounded(parameters.morph_mode4.y, -0.026, 0.026, 0.0),
            bounded(parameters.morph_area_scale, 0.92, 1.0, 1.0),
            0.0,
        ],
        droplet_position_radius: parameters.droplets.map(|droplet| {
            [
                bounded(droplet.position.x, -0.90, 0.90, 0.0),
                bounded(droplet.position.y, -0.90, 0.90, 0.0),
                bounded(droplet.radius, 0.0, 0.095, 0.0),
                bounded(droplet.attachment, 0.0, 1.0, 0.0),
            ]
        }),
        droplet_motion_shape: parameters.droplets.map(|droplet| {
            [
                bounded(droplet.stretch, 0.0, 1.0, 0.0),
                if droplet.rotation.is_finite() {
                    droplet.rotation
                } else {
                    0.0
                },
                if droplet.phase.is_finite() {
                    droplet.phase.rem_euclid(std::f32::consts::TAU)
                } else {
                    0.0
                },
                bounded(droplet.activity, 0.0, 1.0, 0.0),
            ]
        }),
        droplet_bridge: parameters.droplets.map(|droplet| {
            [
                bounded(droplet.anchor.x, -0.90, 0.90, 0.0),
                bounded(droplet.anchor.y, -0.90, 0.90, 0.0),
                bounded(droplet.neck_radius, 0.0, 0.095, 0.0),
                bounded(droplet.bridge, 0.0, 1.0, 0.0),
            ]
        }),
        render_mode: [
            if premultiplied_output { 1.0 } else { 0.0 },
            review_background.shader_value(),
            if studio_material_backdrop {
                2.0
            } else if background_valid {
                1.0
            } else {
                0.0
            },
            1.0 / config.height.max(1).saturating_mul(render_scale.max(1)) as f32,
        ],
        liquid_meta: [
            parameters.liquid.particle_count.min(MAX_PARTICLES) as f32,
            render_scale.max(1) as f32,
            // A lower iso-level reconstructs the shared field between particles;
            // the old high threshold exposed kernel scallops as separate lumps.
            bounded(parameters.liquid_iso_threshold, 0.05, 1.20, 0.18),
            1.0 / config.width.max(1).saturating_mul(render_scale.max(1)) as f32,
        ],
        face_frame_a: [
            finite_or(parameters.liquid.face_frame.origin.x, 0.0),
            finite_or(parameters.liquid.face_frame.origin.y, 0.13),
            finite_or(parameters.liquid.face_frame.axis_x.x, 1.0),
            finite_or(parameters.liquid.face_frame.axis_x.y, 0.0),
        ],
        face_frame_b: [
            finite_or(parameters.liquid.face_frame.axis_y.x, 0.0),
            finite_or(parameters.liquid.face_frame.axis_y.y, 1.0),
            bounded(parameters.liquid.face_frame.scale.x, 0.55, 1.60, 1.0),
            bounded(parameters.liquid.face_frame.scale.y, 0.55, 1.60, 1.0),
        ],
        liquid_motion: [
            parameters
                .liquid
                .particles
                .iter()
                .take(parameters.liquid.particle_count.min(MAX_PARTICLES))
                .map(|particle| particle.velocity.x)
                .sum::<f32>()
                / parameters.liquid.particle_count.max(1) as f32,
            parameters
                .liquid
                .particles
                .iter()
                .take(parameters.liquid.particle_count.min(MAX_PARTICLES))
                .map(|particle| particle.velocity.y)
                .sum::<f32>()
                / parameters.liquid.particle_count.max(1) as f32,
            bounded(parameters.liquid.diagnostics.maximum_speed, 0.0, 8.0, 0.0),
            bounded(
                parameters.liquid.diagnostics.detached_mass / 72.0,
                0.0,
                1.0,
                0.0,
            ),
        ],
        material_a: [
            bounded(parameters.material_absorption, 0.0, 4.0, 1.0),
            bounded(parameters.material_scattering, 0.0, 4.0, 0.72),
            bounded(parameters.material_thickness, 0.10, 4.0, 1.0),
            bounded(parameters.translucency, 0.0, 1.0, 0.72),
        ],
        material_b: [
            bounded(parameters.material_refraction, 0.0, 3.0, 1.0),
            bounded(parameters.material_blur, 0.0, 5.0, 1.15),
            bounded(parameters.material_rim_strength, 0.0, 4.0, 0.90),
            bounded(parameters.material_rim_power, 0.5, 12.0, 2.8),
        ],
        material_c: [
            bounded(parameters.material_broad_specular, 0.0, 1.0, 0.38),
            bounded(parameters.material_broad_specular_power, 4.0, 64.0, 12.0),
            bounded(parameters.material_tight_specular, 0.0, 1.5, 0.95),
            bounded(parameters.material_tight_specular_power, 24.0, 192.0, 64.0),
        ],
        material_d: [
            bounded(parameters.material_emission, 0.0, 2.0, 0.16),
            bounded(parameters.material_opacity, 0.05, 1.0, 0.92),
            if parameters.face_visible { 1.0 } else { 0.0 },
            bounded(parameters.material_fresnel_f0, 0.01, 0.12, 0.055),
        ],
        material_e: [
            bounded(parameters.material_core_level, 0.55, 3.0, 1.35),
            bounded(parameters.material_thickness_gamma, 0.25, 2.5, 0.78),
            bounded(parameters.material_pseudo_depth, 0.05, 0.60, 0.30),
            bounded(parameters.material_normal_scale, 0.20, 4.0, 2.20),
        ],
        material_f: [
            bounded(parameters.material_light_wrap, 0.0, 1.5, 0.55),
            bounded(parameters.material_ambient_scatter, 0.0, 1.5, 0.22),
            bounded(parameters.material_direct_scatter, 0.0, 2.0, 0.55),
            bounded(
                parameters.material_transmission_hue_preservation,
                0.0,
                1.0,
                0.72,
            ),
        ],
        face_tuning: [
            if parameters.face_visible { 1.0 } else { 0.0 },
            parameters.debug_view.shader_value(),
            0.0,
            0.0,
        ],
        desktop_capture: background_uv_transform,
        desktop_capture_meta: [background_freshness.clamp(0.0, 1.0), 0.0, 0.0, 0.0],
        cinematic_a: [
            match parameters.material_variant {
                MaterialVariant::CurrentSafe => 0.0,
                MaterialVariant::CinematicJelly => 1.0,
            },
            bounded(parameters.material_cinematic_smoothing, 0.0, 1.0, 0.68),
            parameters
                .material_internal_orb_count
                .min(MAX_INTERNAL_GLOW_ORBS) as f32,
            bounded(parameters.material_internal_orb_halo, 0.0, 3.0, 1.20),
        ],
        cinematic_b: [
            bounded(parameters.material_internal_orb_size, 0.012, 0.10, 0.045),
            bounded(parameters.material_internal_orb_intensity, 0.0, 4.0, 0.92),
            bounded(parameters.material_internal_orb_speed, 0.0, 2.0, 0.32),
            bounded(parameters.material_internal_orb_depth, 0.0, 1.0, 0.62),
        ],
        cinematic_c: [
            bounded(parameters.material_studio_intensity, 0.0, 4.0, 1.45),
            bounded(parameters.material_studio_base_roughness, 0.05, 1.0, 0.32),
            bounded(parameters.material_studio_coat_roughness, 0.02, 0.50, 0.09),
            bounded(parameters.material_edge_light_width, 2.0, 32.0, 18.0),
        ],
        cinematic_d: [
            bounded(parameters.material_caustic_strength, 0.0, 4.0, 0.28),
            bounded(parameters.material_caustic_scale, 0.5, 12.0, 3.4),
            bounded(parameters.material_caustic_speed, 0.0, 2.0, 0.32),
            bounded(parameters.material_bloom_strength, 0.0, 3.0, 0.55),
        ],
        cinematic_e: [
            bounded(parameters.material_narrow_rim_strength, 0.0, 3.0, 1.0),
            bounded(parameters.material_broad_rim_strength, 0.0, 3.0, 0.62),
            bounded(parameters.material_caustic_dispersion, 0.0, 1.0, 0.45),
            bounded(
                parameters.material_rounded_highlight_strength,
                0.0,
                3.0,
                0.85,
            ),
        ],
        cinematic_f: [
            bounded(parameters.material_highlight_tint, 0.0, 0.5, 0.16),
            parameters.material_soul_glow_count.min(MAX_SOUL_GLOW_LOBES) as f32,
            bounded(parameters.material_soul_glow_strength, 0.0, 2.0, 0.20),
            bounded(parameters.material_soul_glow_feather, 0.20, 1.50, 0.68),
        ],
        cinematic_g: [
            bounded(parameters.material_soul_glow_size, 0.04, 0.30, 0.13),
            bounded(parameters.material_soul_glow_speed, 0.0, 0.50, 0.10),
            bounded(parameters.material_soul_glow_pulse, 0.0, 0.60, 0.18),
            bounded(parameters.face_eye_highlight_scale, 0.50, 2.50, 1.45),
        ],
        cinematic_h: [
            bounded(parameters.face_eye_socket_strength, 0.0, 1.0, 0.18),
            bounded(parameters.face_relief_strength, 0.0, 1.5, 0.55),
            bounded(parameters.material_rim_saturation, 0.0, 1.6, 1.15),
            0.0,
        ],
        cinematic_i: [
            bounded(parameters.face_relief_darkness, 0.35, 0.95, 0.72),
            bounded(parameters.face_relief_coat_strength, 0.0, 2.5, 1.20),
            bounded(parameters.exposure, 0.25, 3.0, 1.0),
            bounded(parameters.pupil_asymmetry, -0.14, 0.14, 0.0),
        ],
        glow_orb_position_radius: features.orb_position_radius,
        glow_orb_color_intensity: features.orb_color_intensity,
        soul_glow_position_radius: features.soul_position_radius,
    }
}

fn internal_glow_orbs(
    parameters: &RenderParameters,
) -> (
    [[f32; 4]; MAX_INTERNAL_GLOW_ORBS],
    [[f32; 4]; MAX_INTERNAL_GLOW_ORBS],
) {
    let mut positions = [[0.0; 4]; MAX_INTERNAL_GLOW_ORBS];
    let mut colors = [[0.0; 4]; MAX_INTERNAL_GLOW_ORBS];
    if parameters.material_variant != MaterialVariant::CinematicJelly {
        return (positions, colors);
    }

    let particle_count = parameters.liquid.particle_count.min(MAX_PARTICLES);
    if particle_count == 0 {
        return (positions, colors);
    }
    let mut center = Vec2::ZERO;
    let mut material_center = Vec2::ZERO;
    for particle in parameters
        .liquid
        .particles
        .iter()
        .take(particle_count)
        .filter(|particle| particle.main_component)
    {
        center += particle.position;
        material_center += particle.material_coordinate;
    }
    let main_count = parameters.liquid.particles[..particle_count]
        .iter()
        .filter(|particle| particle.main_component)
        .count();
    if main_count == 0 {
        return (positions, colors);
    }
    center /= main_count as f32;
    material_center /= main_count as f32;
    let material_radius = parameters.liquid.particles[..particle_count]
        .iter()
        .filter(|particle| particle.main_component)
        .map(|particle| particle.material_coordinate.distance(material_center))
        .fold(0.0_f32, f32::max)
        .max(0.05);

    let count = parameters
        .material_internal_orb_count
        .min(MAX_INTERNAL_GLOW_ORBS);
    let speed = parameters.material_internal_orb_speed.clamp(0.0, 2.0)
        * (0.72 + parameters.flow_speed.clamp(0.0, 0.6) * 0.8);
    let mood_energy = (0.72
        + parameters.arousal.clamp(0.0, 1.0) * 0.38
        + parameters.core_glow.clamp(0.0, 0.72) * 0.55)
        .clamp(0.65, 1.45);

    for index in 0..count {
        let salt = index as u64;
        let phase = hash_unit(
            parameters.pattern_seed,
            salt.wrapping_mul(7).wrapping_add(1),
        ) * std::f32::consts::TAU;
        let t = parameters.time * speed * std::f32::consts::TAU + phase;
        // Stratified polar targets live in immutable material space. At the
        // default spread their bands cover roughly 30–70% of the body radius,
        // instead of selecting the center-first prefix of the particle pack.
        let spread = parameters.material_internal_orb_spread.clamp(0.0, 1.0);
        let radial_min = 0.08 + spread * 0.30;
        let radial_max = 0.16 + spread * 0.75;
        let stratum = (index as f32
            + 0.28
            + hash_unit(parameters.pattern_seed, salt.wrapping_mul(13) + 0x31) * 0.44)
            / count.max(1) as f32;
        let radial_fraction =
            (radial_min + (radial_max - radial_min) * stratum.fract()).clamp(0.04, 0.94);
        let golden_angle = 2.399_963_1_f32;
        let target_angle = index as f32 * golden_angle
            + hash_unit(parameters.pattern_seed, salt.wrapping_mul(13) + 0x47)
                * std::f32::consts::TAU;
        let material_target =
            material_center + Vec2::from_angle(target_angle) * material_radius * radial_fraction;

        let mut nearest_indices = [0_usize; 4];
        let mut nearest_distances = [f32::INFINITY; 4];
        let anchor_count = main_count.min(4);
        for (candidate_index, particle) in parameters.liquid.particles[..particle_count]
            .iter()
            .enumerate()
            .filter(|(_, particle)| particle.main_component)
        {
            let distance = particle
                .material_coordinate
                .distance_squared(material_target);
            let slot = nearest_distances
                .iter()
                .take(anchor_count)
                .position(|nearest| distance < *nearest)
                .unwrap_or(anchor_count);
            if slot < anchor_count {
                for shift in (slot + 1..anchor_count).rev() {
                    nearest_distances[shift] = nearest_distances[shift - 1];
                    nearest_indices[shift] = nearest_indices[shift - 1];
                }
                nearest_distances[slot] = distance;
                nearest_indices[slot] = candidate_index;
            }
        }
        let mut material_position = Vec2::ZERO;
        let mut weight_sum = 0.0;
        for anchor_slot in 0..anchor_count {
            let anchor = parameters.liquid.particles[nearest_indices[anchor_slot]];
            let base_weight = 1.0 / (nearest_distances[anchor_slot] + 0.0004);
            let drift_weight = 1.0
                + 0.12
                    * (t * (0.17 + anchor_slot as f32 * 0.041) + phase + anchor_slot as f32 * 1.73)
                        .sin();
            let weight = base_weight * drift_weight;
            material_position += anchor.position * weight;
            weight_sum += weight;
        }
        material_position /= weight_sum.max(0.0001);
        let radial = material_position - center;
        let tangent = Vec2::new(-radial.y, radial.x).normalize_or_zero();
        let drift = tangent
            * (0.010 + 0.016 * hash_unit(parameters.pattern_seed, salt + 19))
            * (t * 0.61 + phase).sin();
        let inset = 0.91 + 0.07 * (t * 0.29 + phase).sin().mul_add(0.5, 0.5);
        let position = center + radial * inset + drift;
        let radius_jitter = 0.32
            + hash_unit(
                parameters.pattern_seed,
                salt.wrapping_mul(11).wrapping_add(5),
            ) * 0.86;
        let scale_pulse = 1.0 + 0.12 * (t * (0.43 + index as f32 * 0.037) + phase).sin();
        let radius = parameters.material_internal_orb_size.clamp(0.012, 0.10)
            * radius_jitter
            * 0.72
            * scale_pulse;
        let depth_seed = hash_unit(
            parameters.pattern_seed,
            salt.wrapping_mul(11).wrapping_add(6),
        );
        let depth = (0.12
            + parameters.material_internal_orb_depth.clamp(0.0, 1.0) * (0.18 + depth_seed * 0.70))
            .clamp(0.08, 0.94);
        let life = 0.70
            + 0.30
                * (t * (0.19 + 0.04 * index as f32) + phase)
                    .sin()
                    .mul_add(0.5, 0.5);
        let intensity_jitter = 0.72
            + hash_unit(
                parameters.pattern_seed,
                salt.wrapping_mul(11).wrapping_add(7),
            ) * 0.55;
        let intensity = parameters.material_internal_orb_intensity.clamp(0.0, 4.0)
            * intensity_jitter
            * life
            * mood_energy;
        let color_mix = (0.48
            + hash_unit(
                parameters.pattern_seed,
                salt.wrapping_mul(11).wrapping_add(8),
            ) * 0.44
            + parameters.glow.clamp(0.0, 1.0) * 0.08)
            .clamp(0.0, 1.0);
        let pearl = (0.05
            + hash_unit(
                parameters.pattern_seed,
                salt.wrapping_mul(11).wrapping_add(9),
            ) * 0.20
            + parameters.arousal.clamp(0.0, 1.0) * 0.08)
            .clamp(0.0, 0.36);

        positions[index] = [position.x, position.y, radius, depth];
        colors[index] = [color_mix, pearl, intensity, life];
    }
    (positions, colors)
}

fn soul_glow_lobes(parameters: &RenderParameters) -> [[f32; 4]; MAX_SOUL_GLOW_LOBES] {
    let mut lobes = [[0.0; 4]; MAX_SOUL_GLOW_LOBES];
    if parameters.material_variant != MaterialVariant::CinematicJelly {
        return lobes;
    }
    let particle_count = parameters.liquid.particle_count.min(MAX_PARTICLES);
    if !parameters.liquid.particles[..particle_count]
        .iter()
        .any(|particle| particle.main_component)
    {
        return lobes;
    }
    let frame = parameters.liquid.face_frame;
    let axis_x = frame.axis_x.normalize_or_zero();
    let axis_x = if axis_x.length_squared() > 0.5 {
        axis_x
    } else {
        Vec2::X
    };
    let axis_y = frame.axis_y.normalize_or_zero();
    let axis_y = if axis_y.length_squared() > 0.5 {
        axis_y
    } else {
        Vec2::Y
    };
    // The filtered face/material frame belongs to the largest component and is
    // deliberately frozen at low support. Keeping both core and orbit size in
    // that frame prevents a component-list reclassification from teleporting a
    // soul lobe; the liquid runtime moves this frame continuously toward the new
    // main component instead.
    let core = frame.origin;
    let extent = Vec2::new(0.18, 0.15) * frame.scale.clamp(Vec2::splat(0.55), Vec2::splat(1.50));
    let lobe_count = parameters.material_soul_glow_count.min(MAX_SOUL_GLOW_LOBES);
    let speed = parameters.material_soul_glow_speed.clamp(0.0, 0.50);
    for (index, lobe) in lobes.iter_mut().enumerate().take(lobe_count) {
        let salt = index as u64;
        let phase = hash_unit(parameters.pattern_seed, salt.wrapping_mul(13) + 0x51)
            * std::f32::consts::TAU;
        let time = parameters.time * speed * std::f32::consts::TAU;
        let orbit_x = (time * (0.78 + index as f32 * 0.031) + phase).sin()
            * extent.x
            * (0.22 + hash_unit(parameters.pattern_seed, salt + 0x63) * 0.22);
        let orbit_y = (time * (0.70 + index as f32 * 0.027) + phase * 1.31).cos()
            * extent.y
            * (0.18 + hash_unit(parameters.pattern_seed, salt + 0x75) * 0.20);
        let radius_jitter = 0.72 + hash_unit(parameters.pattern_seed, salt + 0x87) * 0.46;
        let pulse = 1.0
            + parameters.material_soul_glow_pulse.clamp(0.0, 0.60)
                * (time * (0.88 + index as f32 * 0.041) + phase * 0.79).sin();
        let position = core + axis_x * orbit_x + axis_y * orbit_y;
        *lobe = [
            position.x,
            position.y,
            parameters.material_soul_glow_size.clamp(0.04, 0.30) * radius_jitter,
            pulse.max(0.15),
        ];
    }
    lobes
}

fn hash_unit(seed: u64, salt: u64) -> f32 {
    let mut value = seed ^ salt.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    value ^= value >> 30;
    value = value.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value ^= value >> 27;
    value = value.wrapping_mul(0x94D0_49BB_1331_11EB);
    value ^= value >> 31;
    ((value >> 40) as u32) as f32 / ((1_u32 << 24) - 1) as f32
}

fn finite_or(value: f32, fallback: f32) -> f32 {
    if value.is_finite() { value } else { fallback }
}

fn bounded(value: f32, minimum: f32, maximum: f32, fallback: f32) -> f32 {
    finite_or(value, fallback).clamp(minimum, maximum)
}

#[cfg(test)]
mod tests {
    use std::mem::{align_of, size_of};

    use super::*;

    #[test]
    fn organism_resolve_uses_source_over_for_background_layers() {
        let premultiplied = surface_source_over_blend(true);
        assert_eq!(premultiplied.color.src_factor, wgpu::BlendFactor::One);
        assert_eq!(
            premultiplied.color.dst_factor,
            wgpu::BlendFactor::OneMinusSrcAlpha
        );

        let straight = surface_source_over_blend(false);
        assert_eq!(straight.color.src_factor, wgpu::BlendFactor::SrcAlpha);
        assert_eq!(
            straight.color.dst_factor,
            wgpu::BlendFactor::OneMinusSrcAlpha
        );
    }

    #[test]
    fn identical_or_zero_surface_resize_does_not_reallocate_targets() {
        assert!(!surface_dimensions_changed(
            3_440,
            1_440,
            PhysicalSize::new(3_440, 1_440),
        ));
        assert!(!surface_dimensions_changed(
            3_440,
            1_440,
            PhysicalSize::new(0, 1_440),
        ));
        assert!(surface_dimensions_changed(
            3_440,
            1_440,
            PhysicalSize::new(3_840, 2_160),
        ));
    }

    #[test]
    fn render_scale_falls_back_without_exceeding_the_adapter_texture_limit() {
        assert_eq!(supported_render_scale(3_440, 1_440, 2, 8_192), Some(2));
        assert_eq!(supported_render_scale(5_120, 2_880, 2, 8_192), Some(1));
        assert_eq!(supported_render_scale(11_520, 2_160, 2, 8_192), None);
        assert_eq!(supported_render_scale(11_520, 2_160, 1, 16_384), Some(1));
        assert_eq!(supported_render_scale(u32::MAX, 1, 2, 16_384), None);
        assert_eq!(supported_render_scale(0, 2_160, 2, 16_384), None);
    }

    #[test]
    fn globals_layout_is_wgsl_uniform_safe() {
        assert_eq!(align_of::<Globals>(), 4);
        let shader = naga::front::wgsl::parse_str(include_str!("liquid_surface.wgsl")).unwrap();
        let shader_size = shader
            .types
            .iter()
            .find_map(|(_, ty)| {
                if ty.name.as_deref() == Some("Globals") {
                    if let naga::TypeInner::Struct { span, .. } = ty.inner {
                        Some(span as usize)
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
            .unwrap();
        assert_eq!(size_of::<Globals>(), shader_size);
        assert_eq!(size_of::<Globals>() % 16, 0);
        assert_eq!(size_of::<ShadowFilterGlobals>(), 10 * 16);
        assert_eq!(size_of::<ShadowFilterGlobals>() % 16, 0);
    }

    #[test]
    fn shadow_filter_kernel_is_normalized_bounded_and_axis_separable() {
        for feather in [2.0, 22.0, 48.0, 64.0, 128.0] {
            let downsample = shadow_downsample_scale(feather);
            let horizontal = shadow_filter_globals(1919, 1079, downsample, 2, feather, true);
            let vertical = shadow_filter_globals(1919, 1079, downsample, 2, feather, false);
            for globals in [horizontal, vertical] {
                let pair_count = globals.direction_meta[3] as usize;
                assert!((1..=MAX_SHADOW_PAIRED_TAPS).contains(&pair_count));
                let mut sum = globals.direction_meta[2];
                let mut previous_offset = 0.0;
                for pair_index in 0..pair_count {
                    let packed = globals.paired_taps[pair_index / 2];
                    let lane = (pair_index % 2) * 2;
                    let offset = packed[lane];
                    let weight = packed[lane + 1];
                    assert!(offset.is_finite() && offset > previous_offset);
                    assert!(weight.is_finite() && weight > 0.0);
                    previous_offset = offset;
                    sum += weight * 2.0;
                }
                assert!((sum - 1.0).abs() <= 2.0e-6, "feather={feather} sum={sum}");
            }
            assert!(horizontal.direction_meta[0] > 0.0);
            assert_eq!(horizontal.direction_meta[1], 0.0);
            assert_eq!(vertical.direction_meta[0], 0.0);
            assert!(vertical.direction_meta[1] > 0.0);
        }
    }

    #[test]
    fn wide_shadow_selects_quarter_mask_without_exceeding_kernel_budget() {
        assert_eq!(shadow_downsample_scale(48.0), SHADOW_HALF_SCALE);
        assert_eq!(shadow_downsample_scale(48.01), SHADOW_QUARTER_SCALE);
        let widest = shadow_filter_globals(3840, 2160, SHADOW_QUARTER_SCALE, 2, 128.0, true);
        assert_eq!(widest.direction_meta[3] as usize, MAX_SHADOW_PAIRED_TAPS);
        assert_eq!(widest.downsample[0], SHADOW_QUARTER_SCALE as f32);
        assert_eq!(widest.downsample[1], 8.0);
    }

    #[test]
    fn offset_shadow_work_bounds_cover_unshifted_mask_and_shifted_halo() {
        let organism_minimum = Vec2::new(300.0, 250.0);
        let organism_maximum = Vec2::new(500.0, 450.0);
        let padding = 128.0;
        let mask_minimum = organism_minimum - Vec2::splat(padding);
        let mask_maximum = organism_maximum + Vec2::splat(padding);

        for offset in [
            Vec2::new(96.0, 96.0),
            Vec2::new(-96.0, -96.0),
            Vec2::new(96.0, -96.0),
            Vec2::new(-96.0, 96.0),
        ] {
            let (minimum, maximum) =
                shadow_work_bounds(organism_minimum, organism_maximum, offset, padding);
            let compose_minimum = mask_minimum + offset;
            let compose_maximum = mask_maximum + offset;

            assert_eq!(minimum, mask_minimum.min(compose_minimum));
            assert_eq!(maximum, mask_maximum.max(compose_maximum));
        }
    }

    #[test]
    fn cinematic_sclera_maps_to_neutral_perceptual_white() {
        fn linear_to_srgb(value: f32) -> f32 {
            if value <= 0.003_130_8 {
                value * 12.92
            } else {
                1.055 * value.powf(1.0 / 2.4) - 0.055
            }
        }
        let hdr_white = 2.75_f32;
        let white_point = 4.0_f32;
        let mapped = hdr_white * (1.0 + hdr_white / white_point.powi(2)) / (1.0 + hdr_white);
        let srgb = linear_to_srgb(mapped);
        assert!((0.93..=0.96).contains(&srgb), "mapped sclera={srgb}");
        let rgb = Vec3::splat(srgb);
        assert!(rgb.max_element() - rgb.min_element() <= 0.02);
    }

    #[test]
    fn internal_feature_crossfade_clamps_split_merge_motion_per_tick() {
        let mut feature = [-0.18, 0.22, 0.04, 0.8];
        let desired = [0.41, -0.36, 0.08, 0.2];
        let before = Vec2::new(feature[0], feature[1]);
        smooth_feature_vec4(&mut feature, desired, 1.0, 0.25);
        let after = Vec2::new(feature[0], feature[1]);
        assert!(after.distance(before) <= 0.015_001);
        assert!(feature[2] > 0.04 && feature[2] < 0.08);
    }

    #[test]
    fn render_parameters_are_clamped_before_reaching_wgsl() {
        let globals = globals_for(
            &wgpu::SurfaceConfiguration {
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                format: wgpu::TextureFormat::Bgra8UnormSrgb,
                width: 512,
                height: 512,
                present_mode: wgpu::PresentMode::Fifo,
                desired_maximum_frame_latency: 2,
                alpha_mode: wgpu::CompositeAlphaMode::PreMultiplied,
                view_formats: vec![],
            },
            RenderParameters {
                pupil_size: 8.0,
                shell_opacity: f32::NAN,
                inner_density: -8.0,
                droplets: [DropletRenderState {
                    position: Vec2::splat(8.0),
                    radius: 2.0,
                    attachment: 2.0,
                    activity: -1.0,
                    ..DropletRenderState::default()
                }; MAX_DROPLETS],
                ..RenderParameters::default()
            },
            1.0,
            true,
            ReviewBackground::Transparent,
            false,
            SUPERSAMPLE_SCALE,
        );
        assert_eq!(globals.gaze_pupil[3], 0.95);
        assert_eq!(globals.physiology_a[1], 0.96);
        assert_eq!(globals.physiology_a[2], 0.55);
        assert_eq!(globals.droplet_position_radius[0][0], 0.90);
        assert_eq!(globals.droplet_position_radius[0][2], 0.095);
        assert_eq!(globals.droplet_position_radius[0][3], 1.0);
        assert_eq!(globals.droplet_motion_shape[0][3], 0.0);
        assert!(
            bytemuck::cast_slice::<Globals, f32>(std::slice::from_ref(&globals))
                .iter()
                .all(|value| value.is_finite())
        );
    }

    #[test]
    fn production_studio_backdrop_and_local_presentation_reach_particle_shader() {
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: wgpu::TextureFormat::Bgra8UnormSrgb,
            width: 768,
            height: 768,
            present_mode: wgpu::PresentMode::Fifo,
            desired_maximum_frame_latency: 2,
            alpha_mode: wgpu::CompositeAlphaMode::PreMultiplied,
            view_formats: vec![],
        };
        let globals = globals_for_resolved(
            &config,
            RenderParameters {
                render_mode: BodyRenderMode::ParticlePbf,
                presentation_offset: Vec2::new(0.31, -0.24),
                ..RenderParameters::default()
            },
            1.0,
            true,
            ReviewBackground::Transparent,
            false,
            SUPERSAMPLE_SCALE,
            None,
            true,
            [1.0, 1.0, 0.0, 0.0],
            1.0,
        );
        assert_eq!(globals.body_shape[2..], [0.31, -0.24]);
        assert_eq!(globals.render_mode[1], 0.0);
        assert_eq!(globals.render_mode[2], 2.0);
    }

    #[test]
    fn mac_style_drop_shadow_controls_reach_compositor_without_shape_projection() {
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: wgpu::TextureFormat::Bgra8UnormSrgb,
            width: 800,
            height: 600,
            present_mode: wgpu::PresentMode::Fifo,
            desired_maximum_frame_latency: 2,
            alpha_mode: wgpu::CompositeAlphaMode::PreMultiplied,
            view_formats: vec![],
        };
        let globals = compose_globals_for(
            &config,
            true,
            ReviewBackground::BusyChecker,
            RenderParameters {
                shadow_horizontal_offset: -13.0,
                shadow_vertical_offset: 17.0,
                shadow_feather: 61.0,
                shadow_opacity: 0.14,
                shadow_color: Vec3::new(0.03, 0.04, 0.06),
                exposure: 1.12,
                ..RenderParameters::default()
            },
            SUPERSAMPLE_SCALE,
        );

        assert_eq!(globals.shadow, [-13.0, 17.0, 0.14, 1.12]);
        assert_eq!(globals.shadow_style, [61.0, 0.03, 0.04, 0.06]);
    }

    #[test]
    fn compositor_resolve_tracks_native_and_supersampled_targets() {
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: wgpu::TextureFormat::Bgra8UnormSrgb,
            width: 800,
            height: 600,
            present_mode: wgpu::PresentMode::Fifo,
            desired_maximum_frame_latency: 2,
            alpha_mode: wgpu::CompositeAlphaMode::PreMultiplied,
            view_formats: vec![],
        };
        let parameters = RenderParameters::default();
        let native =
            compose_globals_for(&config, true, ReviewBackground::Transparent, parameters, 1);
        let supersampled = compose_globals_for(
            &config,
            true,
            ReviewBackground::Transparent,
            parameters,
            SUPERSAMPLE_SCALE,
        );

        assert_eq!(native.post[1], 1.0);
        assert_eq!(supersampled.post[1], SUPERSAMPLE_SCALE as f32);
    }

    #[test]
    fn eyelid_uniforms_use_zero_as_open_and_one_as_closed() {
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: wgpu::TextureFormat::Bgra8UnormSrgb,
            width: 512,
            height: 512,
            present_mode: wgpu::PresentMode::Fifo,
            desired_maximum_frame_latency: 2,
            alpha_mode: wgpu::CompositeAlphaMode::PreMultiplied,
            view_formats: vec![],
        };
        let open = globals_for(
            &config,
            RenderParameters::default(),
            1.0,
            true,
            ReviewBackground::Transparent,
            false,
            SUPERSAMPLE_SCALE,
        );
        let closed = globals_for(
            &config,
            RenderParameters {
                blink_left: 1.0,
                blink_right: 1.0,
                ..RenderParameters::default()
            },
            1.0,
            true,
            ReviewBackground::Transparent,
            false,
            SUPERSAMPLE_SCALE,
        );
        assert_eq!(open.lids_brows[..2], [0.0, 0.0]);
        assert_eq!(closed.lids_brows[..2], [1.0, 1.0]);
    }

    #[test]
    fn gel_material_controls_reach_distinct_shader_uniforms() {
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: wgpu::TextureFormat::Bgra8UnormSrgb,
            width: 512,
            height: 512,
            present_mode: wgpu::PresentMode::Fifo,
            desired_maximum_frame_latency: 2,
            alpha_mode: wgpu::CompositeAlphaMode::PreMultiplied,
            view_formats: vec![],
        };
        let globals = globals_for(
            &config,
            RenderParameters {
                material_absorption: 0.31,
                material_scattering: 0.47,
                material_thickness: 1.19,
                translucency: 0.63,
                material_refraction: 1.27,
                material_blur: 2.4,
                material_rim_strength: 0.83,
                material_rim_power: 4.2,
                material_broad_specular: 0.33,
                material_broad_specular_power: 29.0,
                material_tight_specular: 0.72,
                material_tight_specular_power: 121.0,
                material_emission: 0.44,
                material_opacity: 0.87,
                material_fresnel_f0: 0.041,
                material_core_level: 1.42,
                material_thickness_gamma: 0.83,
                material_pseudo_depth: 0.27,
                material_normal_scale: 1.61,
                material_light_wrap: 0.57,
                material_ambient_scatter: 0.23,
                material_direct_scatter: 0.71,
                material_transmission_hue_preservation: 0.79,
                material_variant: MaterialVariant::CinematicJelly,
                material_cinematic_smoothing: 0.77,
                material_internal_orb_count: 5,
                material_internal_orb_intensity: 1.73,
                material_internal_orb_size: 0.081,
                material_internal_orb_halo: 1.42,
                material_internal_orb_speed: 0.41,
                material_internal_orb_depth: 0.69,
                material_studio_intensity: 1.66,
                material_studio_base_roughness: 0.38,
                material_studio_coat_roughness: 0.11,
                material_edge_light_width: 17.0,
                material_caustic_strength: 0.82,
                material_caustic_scale: 4.6,
                material_caustic_speed: 0.23,
                material_narrow_rim_strength: 1.21,
                material_broad_rim_strength: 0.67,
                material_rim_saturation: 1.27,
                material_caustic_dispersion: 0.49,
                material_rounded_highlight_strength: 0.91,
                material_highlight_tint: 0.17,
                material_soul_glow_count: 4,
                material_soul_glow_strength: 0.24,
                material_soul_glow_size: 0.14,
                material_soul_glow_speed: 0.11,
                material_soul_glow_pulse: 0.19,
                material_soul_glow_feather: 0.71,
                face_eye_highlight_scale: 1.46,
                face_eye_socket_strength: 0.21,
                face_relief_strength: 0.58,
                material_bloom_strength: 0.71,
                flow_strength: 1.17,
                halo: 0.19,
                ..RenderParameters::default()
            },
            1.0,
            true,
            ReviewBackground::Transparent,
            false,
            SUPERSAMPLE_SCALE,
        );

        assert_eq!(globals.material_a, [0.31, 0.47, 1.19, 0.63]);
        assert_eq!(globals.material_b, [1.27, 2.4, 0.83, 4.2]);
        assert_eq!(globals.material_c, [0.33, 29.0, 0.72, 121.0]);
        assert_eq!(globals.material_d, [0.44, 0.87, 1.0, 0.041]);
        assert_eq!(globals.material_e, [1.42, 0.83, 0.27, 1.61]);
        assert_eq!(globals.material_f, [0.57, 0.23, 0.71, 0.79]);
        assert_eq!(globals.cinematic_a, [1.0, 0.77, 5.0, 1.42]);
        assert_eq!(globals.cinematic_b, [0.081, 1.73, 0.41, 0.69]);
        assert_eq!(globals.cinematic_c, [1.66, 0.38, 0.11, 17.0]);
        assert_eq!(globals.cinematic_d, [0.82, 4.6, 0.23, 0.71]);
        assert_eq!(globals.cinematic_e, [1.21, 0.67, 0.49, 0.91]);
        assert_eq!(globals.cinematic_f, [0.17, 4.0, 0.24, 0.71]);
        assert_eq!(globals.cinematic_g, [0.14, 0.11, 0.19, 1.46]);
        assert_eq!(globals.cinematic_h, [0.21, 0.58, 1.27, 0.0]);
        assert_eq!(globals.flow[0], 1.17);
        assert_eq!(globals.physiology_b[1], 0.19);
    }

    #[test]
    fn internal_glow_orbs_are_deterministic_and_material_anchored() {
        let mut liquid = LiquidRenderState {
            particle_count: 4,
            ..LiquidRenderState::default()
        };
        for (index, position) in [
            Vec2::new(-0.20, -0.10),
            Vec2::new(0.20, -0.10),
            Vec2::new(-0.18, 0.20),
            Vec2::new(0.18, 0.20),
        ]
        .into_iter()
        .enumerate()
        {
            liquid.particles[index].position = position;
            liquid.particles[index].material_coordinate = position;
            liquid.particles[index].main_component = true;
        }
        let parameters = RenderParameters {
            liquid,
            time: 1.25,
            pattern_seed: 77,
            material_variant: MaterialVariant::CinematicJelly,
            material_internal_orb_count: 4,
            ..RenderParameters::default()
        };
        let first = internal_glow_orbs(&parameters);
        let replay = internal_glow_orbs(&parameters);
        assert_eq!(first, replay);
        assert!(
            first.0[..4]
                .iter()
                .all(|orb| orb.iter().all(|v| v.is_finite()))
        );
        assert!(first.0[..4].iter().all(|orb| orb[2] > 0.0));
        assert!(first.1[..4].iter().all(|orb| orb[2] > 0.0));

        let safe = internal_glow_orbs(&RenderParameters {
            material_variant: MaterialVariant::CurrentSafe,
            ..parameters
        });
        assert_eq!(safe, ([[0.0; 4]; 8], [[0.0; 4]; 8]));
    }

    #[test]
    fn internal_orb_spread_moves_the_radial_distribution_monotonically() {
        let mut liquid = LiquidRenderState {
            particle_count: 32,
            ..LiquidRenderState::default()
        };
        for index in 0..liquid.particle_count {
            let band = (index % 8) as f32 / 7.0;
            let angle = index as f32 * 2.399_963_1;
            let position = Vec2::from_angle(angle) * (0.05 + band * 0.27);
            liquid.particles[index].position = position;
            liquid.particles[index].material_coordinate = position;
            liquid.particles[index].main_component = true;
        }
        let base = RenderParameters {
            liquid,
            time: 0.0,
            pattern_seed: 0x51AD,
            material_variant: MaterialVariant::CinematicJelly,
            material_internal_orb_count: 8,
            ..RenderParameters::default()
        };
        let mean_radius = |spread| {
            let orbs = internal_glow_orbs(&RenderParameters {
                material_internal_orb_spread: spread,
                ..base
            });
            orbs.0[..8]
                .iter()
                .map(|orb| Vec2::new(orb[0], orb[1]).length())
                .sum::<f32>()
                / 8.0
        };
        let compact = mean_radius(0.0);
        let default = mean_radius(0.72);
        let wide = mean_radius(1.0);
        assert!(
            compact < default && default < wide,
            "{compact}, {default}, {wide}"
        );
        assert!((0.30 * 0.32..=0.70 * 0.32).contains(&default));
    }

    #[test]
    fn analytic_studio_roughness_broadens_lobes_and_lowers_peaks() {
        let base_contract = |roughness: f32| {
            let width = 0.13 + 0.32 * roughness;
            let peak = 1.42 + (0.48 - 1.42) * roughness.sqrt();
            (width, peak)
        };
        let coat_contract = |roughness: f32| {
            let width = 0.045 + 0.19 * roughness;
            let peak = 1.85 + (0.74 - 1.85) * (roughness * 2.0).sqrt();
            (width, peak)
        };
        for contract in [base_contract, coat_contract] {
            let smooth = contract(0.08);
            let rough = contract(0.42);
            assert!(rough.0 > smooth.0);
            assert!(rough.1 < smooth.1);
        }
    }

    #[test]
    fn soul_glow_is_deterministic_material_anchored_and_safe_gated() {
        let mut liquid = LiquidRenderState {
            particle_count: 4,
            ..LiquidRenderState::default()
        };
        liquid.face_frame.origin = Vec2::new(0.01, 0.08);
        for (index, position) in [
            Vec2::new(-0.18, -0.11),
            Vec2::new(0.19, -0.09),
            Vec2::new(-0.16, 0.20),
            Vec2::new(0.17, 0.18),
        ]
        .into_iter()
        .enumerate()
        {
            liquid.particles[index].position = position;
            liquid.particles[index].main_component = true;
        }
        let parameters = RenderParameters {
            liquid,
            time: 4.75,
            pattern_seed: 911,
            material_variant: MaterialVariant::CinematicJelly,
            material_soul_glow_count: 4,
            ..RenderParameters::default()
        };
        let first = soul_glow_lobes(&parameters);
        assert_eq!(first, soul_glow_lobes(&parameters));
        assert!(first[..4].iter().all(|lobe| {
            lobe.iter().all(|value| value.is_finite()) && lobe[2] > 0.0 && lobe[3] > 0.0
        }));
        let next_tick = soul_glow_lobes(&RenderParameters {
            time: parameters.time + 1.0 / 120.0,
            ..parameters
        });
        assert!(
            first[..4]
                .iter()
                .zip(&next_tick[..4])
                .all(|(a, b)| { Vec2::new(a[0], a[1]).distance(Vec2::new(b[0], b[1])) <= 0.015 })
        );
        let mut split_liquid = parameters.liquid;
        split_liquid.particles[1].main_component = false;
        split_liquid.particles[3].main_component = false;
        let after_component_split = soul_glow_lobes(&RenderParameters {
            liquid: split_liquid,
            ..parameters
        });
        assert!(
            first[..4]
                .iter()
                .zip(&after_component_split[..4])
                .all(|(a, b)| { Vec2::new(a[0], a[1]).distance(Vec2::new(b[0], b[1])) <= 0.015 })
        );
        let safe = soul_glow_lobes(&RenderParameters {
            material_variant: MaterialVariant::CurrentSafe,
            ..parameters
        });
        assert_eq!(safe, [[0.0; 4]; MAX_SOUL_GLOW_LOBES]);
    }
}
