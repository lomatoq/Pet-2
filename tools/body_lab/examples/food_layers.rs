//! Reproducible native GPU digestion fixtures; never touches the user's save.
use glam::Vec2;
use pet_body::{EcologyRenderer, ProceduralBody, Renderer, ReviewBackground};
use pet_ecology::{EcologyState, MorselProfile};
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
        let genome = lifecore::Genome::from_seed(42);
        let mut body = ProceduralBody::generate(&genome)?;
        let profile: pet_body::LiquidTuningProfile = serde_json::from_str(
            &std::fs::read_to_string(std::env::args().nth(2).ok_or("profile path required")?)?,
        )?;
        body.apply_tuning_profile(profile)?;
        let mut renderer = pollster::block_on(Renderer::new(window, &body.mesh))?;
        renderer.set_review_background(ReviewBackground::Transparent);
        let mut objects = EcologyRenderer::new(
            renderer.device(),
            renderer.surface_format(),
            renderer.premultiplied_output(),
        );
        let mut state = EcologyState::new(42);
        state.objects.clear();
        state.spawn_morsel(
            Vec2::splat(0.5),
            MorselProfile {
                hue: 0.10,
                saturation: 0.9,
                value: 0.9,
                warmth: 0.5,
                pulse_rate: 0.5,
                stimulation: 0.5,
                cohesion_bias: 0.5,
                novelty: 0.5,
            },
            0.0,
        );
        for (name, lifecycle, visible) in [
            ("body-only", pet_ecology::ObjectLifecycle::Consumed, 1.0),
            ("food-only", pet_ecology::ObjectLifecycle::Sleeping, 0.0),
            (
                "floor-behind-body",
                pet_ecology::ObjectLifecycle::Sleeping,
                1.0,
            ),
            (
                "held-at-mouth",
                pet_ecology::ObjectLifecycle::CarriedByPet,
                1.0,
            ),
        ] {
            state.objects[0].lifecycle = lifecycle;
            objects.prepare(renderer.queue(), &state, 1.6, 0.0);
            let mut params = body.render_parameters(&genome, 0.3);
            params.render_mode = pet_body::BodyRenderMode::ParticlePbf;
            params.presentation_visibility = visible;
            let frame = renderer.render_capture_with_layers(
                params,
                |_, _, encoder, view| objects.render_background_objects(encoder, view),
                |_, _, encoder, view| objects.render_foreground_objects(encoder, view),
            )?;
            let mut bytes = Vec::new();
            bytes.extend(frame.width.to_le_bytes());
            bytes.extend(frame.height.to_le_bytes());
            bytes.extend(frame.rgba8);
            std::fs::write(self.output.join(format!("{name}.rgba")), bytes)?;
        }
        Ok(())
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    run(std::env::args()
        .nth(1)
        .ok_or("output path required")?
        .into())
}
