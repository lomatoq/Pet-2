//! Native optical replay of recorded production particles, without live pet state.
use glam::Vec2;
use pet_body::{BodyRenderMode, ProceduralBody, Renderer, ReviewBackground};
use std::{path::PathBuf, sync::Arc};
use winit::{
    application::ApplicationHandler,
    dpi::PhysicalSize,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoop},
    window::{Window, WindowId},
};

struct Capture {
    output: PathBuf,
    trace: PathBuf,
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
        std::fs::create_dir_all(&self.output)?;
        let directory = self.trace.parent().ok_or("trace directory required")?;
        let genome: lifecore::Genome =
            serde_json::from_str(&std::fs::read_to_string(directory.join("genome.json"))?)?;
        let profile: pet_body::LiquidTuningProfile =
            serde_json::from_str(&std::fs::read_to_string(directory.join("profile.json"))?)?;
        let mut body = ProceduralBody::generate(&genome)?;
        body.apply_tuning_profile(profile)?;
        body.set_desktop_motion_space(Vec2::new(1920.0, 1080.0), 360.0);
        let window = Arc::new(
            event_loop.create_window(
                Window::default_attributes()
                    .with_title("Pet2 isolated contact replay")
                    .with_visible(false)
                    .with_inner_size(PhysicalSize::new(640, 360)),
            )?,
        );
        let mut renderer = pollster::block_on(Renderer::new(window, &body.mesh))?;
        renderer.set_review_background(ReviewBackground::Black);
        let source = std::fs::read_to_string(&self.trace)?;
        let rows: Vec<serde_json::Value> = source
            .lines()
            .map(serde_json::from_str)
            .collect::<Result<_, _>>()?;
        let contact_time = rows
            .iter()
            .find(|r| r["phase"].is_number())
            .ok_or("no contact")?["t"]
            .as_f64()
            .ok_or("no time")?;
        let mut timing = Vec::new();
        for row in rows {
            let Some(particles) = row["particles"].as_array() else {
                continue;
            };
            let t = row["t"].as_f64().ok_or("invalid time")? as f32;
            let phase = t - contact_time as f32;
            if !(-0.2..=1.5).contains(&phase) {
                continue;
            }
            let mut params = body.render_parameters(&genome, t);
            params.render_mode = BodyRenderMode::ParticlePbf;
            params.face_visible = false;
            params.shadow_opacity = 0.0;
            params.joy_aura = 0.0;
            params.presentation_visibility = 1.0;
            // Match the physical 360px overlay at scale1. The constant is the
            // production ParticlePbf projection; the floor remains at y=310px.
            params.presentation_scale = 1.0;
            let root_y = row["root_y"].as_f64().ok_or("invalid root")? as f32;
            let root_pixel = 310.0 - (1.0 - root_y) * 1080.0;
            params.presentation_offset.y = (180.0 - root_pixel) * (2.0 * 1.255_772_7 / 360.0);
            if particles.len() > params.liquid.particles.len() {
                return Err("too many particles".into());
            }
            params.liquid.particle_count = particles.len();
            params.liquid.bubble_count = 0;
            for (out, p) in params.liquid.particles.iter_mut().zip(particles) {
                *out = pet_body::ParticleRenderState {
                    position: vector(p, "position")?,
                    material_coordinate: vector(p, "material_coordinate")?,
                    axis_major: vector(p, "axis_major")?,
                    velocity: vector(p, "velocity")?,
                    major_radius: scalar(p, "major_radius")?,
                    minor_radius: scalar(p, "minor_radius")?,
                    density: scalar(p, "density")?,
                    optical_thickness: scalar(p, "optical_thickness")?,
                    emission: scalar(p, "emission")?,
                    pigment: scalar(p, "pigment")?,
                    face_weight: scalar(p, "face_weight")?,
                    component_id: p["component_id"].as_u64().ok_or("component ID")? as u8,
                    main_component: p["main_component"]
                        .as_bool()
                        .ok_or("component membership")?,
                };
            }
            // Fixed phase isolates geometry/contact from authored circulation.
            renderer.reset_perceptual_capture_state();
            let frame = renderer.render_capture(params)?;
            let mut bytes = Vec::with_capacity(frame.rgba8.len() + 8);
            bytes.extend(frame.width.to_le_bytes());
            bytes.extend(frame.height.to_le_bytes());
            bytes.extend(frame.rgba8);
            std::fs::write(self.output.join(format!("{:03}.rgba", timing.len())), bytes)?;
            timing.push(serde_json::json!({"time":t,"phase":phase,"floor_y":310,"root_y":root_y}));
        }
        std::fs::write(
            self.output.join("timing.json"),
            serde_json::to_vec_pretty(&timing)?,
        )?;
        Ok(())
    }
}
fn scalar(value: &serde_json::Value, name: &str) -> Result<f32, Box<dyn std::error::Error>> {
    let result = value[name].as_f64().ok_or("missing particle scalar")? as f32;
    if !result.is_finite() {
        return Err("non-finite particle scalar".into());
    }
    Ok(result)
}
fn vector(value: &serde_json::Value, name: &str) -> Result<Vec2, Box<dyn std::error::Error>> {
    let components: [f32; 2] = serde_json::from_value(value[name].clone())?;
    let result = Vec2::from_array(components);
    if !result.is_finite() {
        return Err("non-finite particle vector".into());
    }
    Ok(result)
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut app = Capture {
        output: std::env::args()
            .nth(1)
            .ok_or("output path required")?
            .into(),
        trace: std::env::args()
            .nth(2)
            .ok_or("trace JSONL required")?
            .into(),
        error: None,
    };
    EventLoop::new()?.run_app(&mut app)?;
    app.error.map_or(Ok(()), |e| Err(e.into()))
}
