//! Isolated snapshots of the exact native menu, without opening a pet session.
use super::companion_menu;
use egui::{Context, RawInput, Rect, pos2, vec2};
use egui_wgpu::{Renderer as EguiRenderer, RendererOptions, ScreenDescriptor, wgpu};
use glam::Vec2;
use lifecore::Genome;
use pet_body::{ProceduralBody, Renderer, ReviewBackground};
use std::{fs, path::PathBuf, sync::Arc};
use winit::{
    application::ApplicationHandler,
    dpi::PhysicalSize,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoop},
    window::{Window, WindowId},
};

pub fn run(output: PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    fs::create_dir_all(&output)?;
    let mut capture = Capture {
        output,
        error: None,
    };
    EventLoop::new()?.run_app(&mut capture)?;
    capture.error.map_or(Ok(()), |e| Err(e.into()))
}
struct Capture {
    output: PathBuf,
    error: Option<String>,
}
impl ApplicationHandler for Capture {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if let Err(e) = self.capture(event_loop) {
            self.error = Some(e.to_string());
        }
        event_loop.exit();
    }
    fn window_event(&mut self, _: &ActiveEventLoop, _: WindowId, _: WindowEvent) {}
}
impl Capture {
    fn capture(&self, event_loop: &ActiveEventLoop) -> Result<(), Box<dyn std::error::Error>> {
        // Optional pixel-density fixture; never changes the live OS DPI setting.
        let capture_scale = if std::env::var_os("PET2_MENU_CAPTURE_SCALE").is_some_and(|v| v == "2")
        {
            2_u32
        } else {
            1_u32
        };
        let wide = std::env::var_os("PET2_MENU_CAPTURE_WIDE").is_some_and(|v| v == "1");
        let logical_width = if wide { 1280 } else { 420 };
        let width = logical_width * capture_scale;
        let height = 580 * capture_scale;
        let window = Arc::new(
            event_loop.create_window(
                Window::default_attributes()
                    .with_title("Pet2 · isolated native care menu capture")
                    .with_visible(false)
                    .with_inner_size(PhysicalSize::new(width, height)),
            )?,
        );
        let genome = Genome::from_seed(42);
        let body = ProceduralBody::generate(&genome)?;
        let mut renderer = pollster::block_on(Renderer::new(window, &body.mesh))?;
        renderer.set_review_background(ReviewBackground::Transparent);
        let mut parameters = body.render_parameters(&genome, 0.35);
        parameters.render_mode = pet_body::BodyRenderMode::ParticlePbf;
        parameters.presentation_offset = Vec2::splat(8.0);
        let ctx = Context::default();
        companion_menu::configure(&ctx);
        ctx.set_pixels_per_point(capture_scale as f32);
        let mut egui = EguiRenderer::new(
            renderer.device(),
            renderer.surface_format(),
            RendererOptions::default(),
        );
        super::bubble_material::register(&mut egui, renderer.device(), renderer.surface_format());
        let mut fixtures: Vec<String> = [
            "controls",
            "float-0",
            "float-2",
            "float-4",
            "float-6",
            "float-8",
            "float-10",
            "learn",
            "name",
            "name-no-input",
            "voice",
            "settings",
            "feeding",
            "pending",
            "offline",
            "failure",
            "hover",
            "pressed",
            "panel-exit",
            "open-0.04",
            "open-0.10",
            "open-0.20",
            "close-0.04",
            "close-0.10",
            "close-0.20",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect();
        fixtures.extend((0..48).map(|i| format!("open-{:.6}", i as f32 / 60.0)));
        fixtures.extend((0..12).map(|i| format!("drift-{:.6}", i as f32 / 60.0)));
        fixtures.extend((0..14).map(|i| format!("close-{:.6}", i as f32 / 60.0)));
        fixtures.extend((0..40).map(|i| format!("press-{:.6}", i as f32 / 60.0)));
        if capture_scale > 1 || wide {
            fixtures.retain(|name| {
                matches!(
                    name.as_str(),
                    "controls"
                        | "learn"
                        | "voice"
                        | "open-0.300000"
                        | "drift-0.000000"
                        | "drift-0.016667"
                        | "drift-0.033333"
                )
            });
        }
        if std::env::var_os("PET2_MENU_MATERIAL_MATRIX").is_some() {
            fixtures = vec![
                "material-grid-light".into(),
                "material-grid-dark".into(),
                "material-grid-checker".into(),
                "controls".into(),
                "learn".into(),
                "name".into(),
                "voice".into(),
                "settings".into(),
                "pending".into(),
                "offline".into(),
                "hover".into(),
                "pressed".into(),
                "feeding".into(),
                "close-0.100000".into(),
                "drift-0.000000".into(),
                "drift-0.016667".into(),
                "drift-0.033333".into(),
            ];
            fixtures.extend((0..32).map(|i| format!("press-{:.6}", i as f32 / 60.0)));
        }
        if std::env::var_os("PET2_MENU_MOTION_ONLY").is_some() {
            fixtures = (0..76).map(|i| format!("open-{:.6}", i as f32 / 60.0)).collect();
            fixtures.extend((0..22).map(|i| format!("close-{:.6}", i as f32 / 60.0)));
            fixtures.extend((0..55).map(|i| format!("retoggle-{:.6}", i as f32 / 60.0)));
        }
        if std::env::var_os("PET2_MENU_TEACHING_ONLY").is_some() {
            fixtures = ["name", "name-no-input", "learn", "voice"]
                .into_iter().map(str::to_owned).collect();
        }
        let started = std::time::Instant::now();
        let count = fixtures.len();
        for name in fixtures {
            let mut output = None;
            // Let egui resolve Area sizes before accepting a snapshot.
            for frame in 0..60 {
                let raw = RawInput {
                    screen_rect: Some(Rect::from_min_size(
                        pos2(0.0, 0.0),
                        vec2(logical_width as f32, 580.0),
                    )),
                    time: Some(frame as f64 / 60.0),
                    ..Default::default()
                };
                let current = ctx.run(raw, |ctx| companion_menu::capture_frame(ctx, &name));
                for (id, delta) in &current.textures_delta.set {
                    egui.update_texture(renderer.device(), renderer.queue(), *id, delta);
                }
                output = Some(current);
            }
            let output = output.unwrap();
            let jobs = ctx.tessellate(output.shapes, output.pixels_per_point);
            let screen = ScreenDescriptor {
                size_in_pixels: [width, height],
                pixels_per_point: capture_scale as f32,
            };
            let frame = renderer.render_capture_with_overlay(
                parameters,
                |device, queue, encoder, view| {
                    let callbacks = egui.update_buffers(device, queue, encoder, &jobs, &screen);
                    assert!(callbacks.is_empty());
                    let pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("native menu fixture"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view,
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
                    egui.render(&mut pass.forget_lifetime(), &jobs, &screen);
                },
            )?;
            let mut bytes = Vec::new();
            bytes.extend(frame.width.to_le_bytes());
            bytes.extend(frame.height.to_le_bytes());
            bytes.extend(frame.rgba8);
            fs::write(self.output.join(format!("{name}.rgba")), bytes)?;
        }
        fs::write(self.output.join("capture-timing.json"),serde_json::json!({"frames":count,"elapsed_ms":started.elapsed().as_secs_f64()*1000.0,"format":format!("{:?}",renderer.surface_format()),"pixels_per_point":capture_scale,"method":"native full renderer+readback+egui 60 settle passes; not live FPS or isolated shader timing"}).to_string())?;
        Ok(())
    }
}
