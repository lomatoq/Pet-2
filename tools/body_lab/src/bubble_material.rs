//! Soft opal energy membranes for six controls in egui's existing pass.
//!
//! Broad subsurface light and centered depression preserve filled-symbol contrast.
//! Icons are painted above this material by the caller. GPU objects are
//! initialized once; each immutable slot receives one small uniform update.
use egui::{Color32, PaintCallbackInfo, Painter, Rect};
use egui_wgpu::{Callback, CallbackResources, CallbackTrait, ScreenDescriptor, wgpu};
use std::num::NonZeroU64;
use wgpu::util::DeviceExt;

const SLOTS: usize = 6;
const UNIFORM_BYTES: usize = 64;

struct Slot {
    uniform: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
}
struct Resources {
    pipeline: wgpu::RenderPipeline,
    slots: [Slot; SLOTS],
    format: wgpu::TextureFormat,
}

/// Register after constructing each egui renderer, using that renderer's actual
/// target format. Repeated registration with the same target is a no-op.
/// Default egui MSAA (one sample) and no depth attachment are required.
pub fn register(
    renderer: &mut egui_wgpu::Renderer,
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
) {
    if renderer
        .callback_resources
        .get::<Resources>()
        .is_some_and(|r| r.format == format)
    {
        return;
    }
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("companion bubble analytic material"),
        source: wgpu::ShaderSource::Wgsl(include_str!("bubble_material.wgsl").into()),
    });
    let bind_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("companion bubble uniform layout"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: NonZeroU64::new(UNIFORM_BYTES as u64),
            },
            count: None,
        }],
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("companion bubble pipeline layout"),
        bind_group_layouts: &[&bind_layout],
        push_constant_ranges: &[],
    });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("companion bubble material pipeline"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &module,
            entry_point: Some("vertex_main"),
            compilation_options: Default::default(),
            buffers: &[],
        },
        fragment: Some(wgpu::FragmentState {
            module: &module,
            entry_point: Some("fragment_main"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview: None,
        cache: None,
    });
    let initial = Uniforms::new(Color32::WHITE, 0.0, 0.0, 0.0, [1.0; 2], format).bytes();
    let slots = std::array::from_fn(|_| {
        let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("companion bubble immutable slot"),
            contents: &initial,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("companion bubble immutable bind group"),
            layout: &bind_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform.as_entire_binding(),
            }],
        });
        Slot {
            uniform,
            bind_group,
        }
    });
    renderer.callback_resources.insert(Resources {
        pipeline,
        slots,
        format,
    });
}

#[derive(Clone, Copy)]
struct Uniforms {
    tint_alpha: [f32; 4],
    shape: [f32; 4],
    output: [f32; 4],
    quad: [f32; 4],
}
fn finite_or(value: f32, fallback: f32) -> f32 {
    if value.is_finite() { value } else { fallback }
}
fn linear_channel(channel: u8) -> f32 {
    let c = f32::from(channel) / 255.0;
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}
impl Uniforms {
    fn new(
        tint: Color32,
        alpha: f32,
        emphasis: f32,
        phase: f32,
        size: [f32; 2],
        format: wgpu::TextureFormat,
    ) -> Self {
        let [r, g, b, _] = tint.to_srgba_unmultiplied();
        Self {
            tint_alpha: [
                linear_channel(r),
                linear_channel(g),
                linear_channel(b),
                finite_or(alpha, 0.0).clamp(0.0, 1.0),
            ],
            shape: [
                finite_or(size[0], 1.0).max(1.0),
                finite_or(size[1], 1.0).max(1.0),
                finite_or(emphasis, 0.0).clamp(0.0, 1.0),
                finite_or(phase, 0.0).rem_euclid(std::f32::consts::TAU),
            ],
            // Linear output for sRGB attachments; encode for ordinary Unorm.
            output: [if format.is_srgb() { 0.0 } else { 1.0 }, 0.0, 0.0, 0.0],
            quad: [0.0, 0.0, 1.0, 1.0],
        }
    }
    // Explicit stack encoding avoids unsafe casts or a new dependency. WGSL is
    // four vec4s, and WebGPU uniform scalars use little-endian IEEE754 words.
    fn bytes(self) -> [u8; UNIFORM_BYTES] {
        let mut bytes = [0; UNIFORM_BYTES];
        for (i, value) in self
            .tint_alpha
            .into_iter()
            .chain(self.shape)
            .chain(self.output)
            .chain(self.quad)
            .enumerate()
        {
            bytes[i * 4..i * 4 + 4].copy_from_slice(&value.to_le_bytes());
        }
        bytes
    }
}

struct BubbleCallback {
    tint: Color32,
    alpha: f32,
    emphasis: f32,
    phase: f32,
    depression: f32,
    size: [f32; 2],
    rect: Rect,
    slot: usize,
}
impl CallbackTrait for BubbleCallback {
    fn prepare(
        &self,
        _device: &wgpu::Device,
        queue: &wgpu::Queue,
        screen: &ScreenDescriptor,
        _encoder: &mut wgpu::CommandEncoder,
        resources: &mut CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        if let Some(resources) = resources.get::<Resources>() {
            let mut uniform = Uniforms::new(
                self.tint,
                self.alpha,
                self.emphasis,
                self.phase,
                self.size,
                resources.format,
            );
            let width = screen.size_in_pixels[0].max(1) as f32 / screen.pixels_per_point;
            let height = screen.size_in_pixels[1].max(1) as f32 / screen.pixels_per_point;
            uniform.quad = [
                2.0 * self.rect.center().x / width - 1.0,
                1.0 - 2.0 * self.rect.center().y / height,
                self.rect.width() / width,
                self.rect.height() / height,
            ];
            uniform.output[1] = finite_or(self.depression, 0.0).clamp(0.0, 1.0);
            let bytes = uniform.bytes();
            queue.write_buffer(&resources.slots[self.slot].uniform, 0, &bytes);
        }
        Vec::new()
    }
    fn paint(
        &self,
        info: PaintCallbackInfo,
        pass: &mut wgpu::RenderPass<'static>,
        resources: &CallbackResources,
    ) {
        if let Some(resources) = resources.get::<Resources>() {
            // Egui's courtesy viewport rounds to integer pixels, causing slow
            // floating to jump. Draw an exact subpixel quad in the full viewport;
            // egui's existing scissor still clips it, including edge arrivals.
            pass.set_viewport(
                0.0,
                0.0,
                info.screen_size_px[0] as f32,
                info.screen_size_px[1] as f32,
                0.0,
                1.0,
            );
            pass.set_pipeline(&resources.pipeline);
            pass.set_bind_group(0, &resources.slots[self.slot].bind_group, &[]);
            pass.draw(0..6, 0..1);
        }
    }
}

/// Add one GPU lens callback. Use one stable slot (0..6) per button, at most
/// once per egui frame; prepare happens for all callbacks before any are drawn.
/// Rect includes the transparent AA margin. Paint the icon afterward.
pub fn paint(
    painter: &Painter,
    rect: Rect,
    tint: Color32,
    alpha: f32,
    emphasis: f32,
    depression: f32,
    slot: usize,
) {
    if slot >= SLOTS
        || !rect.is_finite()
        || !rect.is_positive()
        || !alpha.is_finite()
        || alpha <= 0.001
    {
        return;
    }
    painter.add(Callback::new_paint_callback(
        rect,
        BubbleCallback {
            tint,
            alpha,
            emphasis,
            phase: slot as f32 * 1.43,
            depression,
            size: [rect.width(), rect.height()],
            rect: painter
                .ctx()
                .layer_transform_to_global(painter.layer_id())
                .map_or(rect, |t| t * rect),
            slot,
        },
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shader_validates_and_matches_four_vec4_uniform_encoding() {
        let shader =
            wgpu::naga::front::wgsl::parse_str(include_str!("bubble_material.wgsl")).unwrap();
        wgpu::naga::valid::Validator::new(
            wgpu::naga::valid::ValidationFlags::all(),
            wgpu::naga::valid::Capabilities::all(),
        )
        .validate(&shader)
        .unwrap();
        let span = shader
            .types
            .iter()
            .find_map(|(_, ty)| {
                if ty.name.as_deref() == Some("Uniforms") {
                    if let wgpu::naga::TypeInner::Struct { span, .. } = ty.inner {
                        Some(span as usize)
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
            .unwrap();
        assert_eq!(span, UNIFORM_BYTES);
    }
    #[test]
    fn uniforms_are_finite_bounded_and_four_vec4_words() {
        let uniform = Uniforms::new(
            Color32::from_rgb(60, 98, 86),
            f32::NAN,
            8.0,
            f32::INFINITY,
            [f32::NAN, -10.0],
            wgpu::TextureFormat::Bgra8UnormSrgb,
        );
        assert_eq!(uniform.tint_alpha[3], 0.0);
        assert_eq!(uniform.shape, [1.0, 1.0, 1.0, 0.0]);
        assert_eq!(uniform.output[0], 0.0);
        assert_eq!(uniform.bytes().len(), 64);
        assert!(
            uniform
                .tint_alpha
                .iter()
                .all(|v| v.is_finite() && (0.0..=1.0).contains(v))
        );
        let gamma = Uniforms::new(
            Color32::WHITE,
            1.0,
            0.0,
            0.0,
            [56.0; 2],
            wgpu::TextureFormat::Bgra8Unorm,
        );
        assert_eq!(gamma.output[0], 1.0);
        assert_eq!(
            f32::from_le_bytes(gamma.bytes()[12..16].try_into().unwrap()),
            1.0
        );
    }

    #[test]
    #[ignore = "requires a real wgpu adapter; run explicitly for material validation"]
    fn gpu_validates_both_target_encodings_and_cached_six_slots() {
        pollster::block_on(async {
            let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
            let adapter = instance
                .request_adapter(&wgpu::RequestAdapterOptions::default())
                .await
                .unwrap();
            let (device, queue) = adapter
                .request_device(&wgpu::DeviceDescriptor::default())
                .await
                .unwrap();
            eprintln!("bubble validation adapter: {:?}", adapter.get_info());
            for format in [
                wgpu::TextureFormat::Bgra8Unorm,
                wgpu::TextureFormat::Bgra8UnormSrgb,
            ] {
                device.push_error_scope(wgpu::ErrorFilter::Validation);
                let mut renderer = egui_wgpu::Renderer::new(&device, format, Default::default());
                register(&mut renderer, &device, format);
                let slots = renderer
                    .callback_resources
                    .get::<Resources>()
                    .unwrap()
                    .slots
                    .as_ptr();
                register(&mut renderer, &device, format);
                assert_eq!(
                    renderer
                        .callback_resources
                        .get::<Resources>()
                        .unwrap()
                        .slots
                        .as_ptr(),
                    slots
                );
                let mut encoder = device.create_command_encoder(&Default::default());
                for slot in 0..SLOTS {
                    let cb = BubbleCallback {
                        tint: Color32::WHITE,
                        alpha: 1.0,
                        emphasis: slot as f32 / 6.0,
                        phase: slot as f32,
                        depression: slot as f32 / 5.0,
                        size: [56.0; 2],
                        rect: Rect::from_min_size(
                            egui::pos2(22.125, 450.375),
                            egui::vec2(56.0, 56.0),
                        ),
                        slot,
                    };
                    assert!(
                        cb.prepare(
                            &device,
                            &queue,
                            &ScreenDescriptor {
                                size_in_pixels: [420, 580],
                                pixels_per_point: 1.0
                            },
                            &mut encoder,
                            &mut renderer.callback_resources
                        )
                        .is_empty()
                    );
                }
                assert!(device.pop_error_scope().await.is_none());
            }
        });
    }
}
