//! Reproducible native GPU digestion fixtures; never touches the user's save.
use glam::Vec2;
use pet_body::{EcologyRenderer, ProceduralBody, Renderer, ReviewBackground};
use pet_ecology::{DigestionFrame, EcologyState, MorselProfile};
use std::{path::PathBuf, sync::Arc};
use winit::{
    application::ApplicationHandler,
    dpi::PhysicalSize,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoop},
    window::{Window, WindowId},
};
pub fn run(output: PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    std::fs::create_dir_all(&output)?;
    let mut app = Capture {
        output,
        error: None,
    };
    EventLoop::new()?.run_app(&mut app)?;
    app.error.map_or(Ok(()), |e| Err(e.into()))
}
struct Capture {
    output: PathBuf,
    error: Option<String>,
}
impl ApplicationHandler for Capture {
    fn resumed(&mut self, e: &ActiveEventLoop) {
        if let Err(error) = self.render(e) {
            self.error = Some(error.to_string());
        }
        e.exit();
    }
    fn window_event(&mut self, _: &ActiveEventLoop, _: WindowId, _: WindowEvent) {}
}
impl Capture {
    fn render(&self, e: &ActiveEventLoop) -> Result<(), Box<dyn std::error::Error>> {
        let window = Arc::new(
            e.create_window(
                Window::default_attributes()
                    .with_title("Pet 2 digestion verification")
                    .with_visible(false)
                    .with_inner_size(PhysicalSize::new(1280, 800)),
            )?,
        );
        let body = ProceduralBody::generate(&lifecore::Genome::from_seed(42))?;
        let mut renderer = pollster::block_on(Renderer::new(window, &body.mesh))?;
        let mut objects = EcologyRenderer::new(
            renderer.device(),
            renderer.surface_format(),
            renderer.premultiplied_output(),
        );
        let mut state = EcologyState::new(42);
        state.objects.clear();
        let mut frames = Vec::new();
        for (i, moisture) in [0.05, 0.5, 0.95].into_iter().enumerate() {
            let mut local = EcologyState::new(42);
            let food = MorselProfile {
                hue: 0.9,
                saturation: 0.7,
                value: 0.8,
                warmth: moisture,
                pulse_rate: 0.5,
                stimulation: 0.7,
                cohesion_bias: 1.0 - moisture,
                novelty: 0.7,
            };
            for _ in 0..4 {
                local.metabolism.consume(&food);
            }
            let frame = DigestionFrame {
                outlet: Vec2::new(0.22 + i as f32 * 0.28, 0.7 - 3.0 / 800.0),
                mouth: Vec2::new(0.22 + i as f32 * 0.28, 0.6),
                floor: 0.7,
                settled: true,
                aspect: 1.6,
                ..Default::default()
            };
            for _ in 0..18000 {
                local.metabolism.tract.advance(1.0 / 60.0);
                local
                    .waste
                    .step(&mut local.metabolism.tract, frame, 1.0 / 60.0);
            }
            state.waste.chains.extend(local.waste.chains);
            frames.push(frame);
        }
        std::fs::write(
            self.output.join("waste-fixture.json"),
            serde_json::to_vec_pretty(&state.waste)?,
        )?;
        for (name, background, clean_time) in [
            ("white", ReviewBackground::White, 0),
            ("dark", ReviewBackground::Black, 0),
            ("cleanup", ReviewBackground::White, 14),
        ] {
            renderer.set_review_background(background);
            if clean_time > 0 {
                let cursor = state.waste.chains[0].nodes[0].position;
                for _ in 0..clean_time {
                    state.waste.step(
                        &mut state.metabolism.tract,
                        DigestionFrame {
                            cursor: Some(cursor),
                            ..frames[0]
                        },
                        1.0 / 60.0,
                    );
                }
            }
            objects.prepare(renderer.queue(), &state, 1.6, 300.0);
            let mut params = body.render_parameters(&lifecore::Genome::from_seed(42),0.3);
            params.presentation_visibility = 0.0;
            let frame = renderer.render_capture_with_overlay(params, |_, _, encoder, view| {
                objects.render_foreground_objects(encoder, view);
            })?;
            let mut bytes = Vec::new();
            bytes.extend(frame.width.to_le_bytes());
            bytes.extend(frame.height.to_le_bytes());
            bytes.extend(frame.rgba8);
            std::fs::write(self.output.join(format!("{name}.rgba")), bytes)?;
        }
        Ok(())
    }
}
