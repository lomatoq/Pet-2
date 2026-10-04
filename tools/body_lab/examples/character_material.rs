//! Production renderer material review, using copied user fixtures only.
use pet_body::{BodyMaterialSnapshot, BodyRenderMode, ProceduralBody, Renderer, ReviewBackground};
use std::{path::PathBuf, sync::Arc, time::Instant};
use winit::{
    application::ApplicationHandler,
    dpi::PhysicalSize,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoop},
    window::{Window, WindowId},
};

struct Capture {
    output: PathBuf,
    fixture: PathBuf,
    error: Option<String>,
}
#[derive(serde::Deserialize)]
struct SavedState {
    life: SavedLife,
}
#[derive(serde::Deserialize)]
struct SavedLife {
    state: SavedOrganism,
}
#[derive(serde::Deserialize)]
struct SavedOrganism {
    genome: lifecore::Genome,
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
        let state: SavedState =
            serde_json::from_str(&std::fs::read_to_string(self.fixture.join("state.json"))?)?;
        let genome = state.life.state.genome;
        let mut profile: pet_body::LiquidTuningProfile = serde_json::from_str(
            &std::fs::read_to_string(self.fixture.join("liquid-tuning.json"))?,
        )?;
        // Explicit review ablations; the user profile on disk is never changed.
        match std::env::args().nth(3).as_deref() {
            Some("satin") => profile.material.studio_coat_roughness = 0.50,
            Some("uncoated") => profile.material.studio_intensity = 0.0,
            None | Some("coated") => {}
            Some(_) => return Err("material case must be coated, satin or uncoated".into()),
        }
        let snapshot: BodyMaterialSnapshot = serde_json::from_str(&std::fs::read_to_string(
            self.fixture.join("body-state.json"),
        )?)?;
        let window = Arc::new(
            event_loop.create_window(
                Window::default_attributes()
                    .with_title("Pet2 isolated material review")
                    .with_visible(false)
                    .with_inner_size(PhysicalSize::new(960, 640)),
            )?,
        );
        let mut body = ProceduralBody::generate(&genome)?;
        body.apply_tuning_profile(profile.clone())?;
        let mut renderer = pollster::block_on(Renderer::new(window, &body.mesh))?;
        let mut timings = Vec::new();
        for (shape, restored) in [("awake", false), ("resting", true)] {
            body = ProceduralBody::generate(&genome)?;
            body.apply_tuning_profile(profile.clone())?;
            if restored {
                body.restore_body_material_snapshot(&snapshot)?;
            }
            for (background_name, background) in [
                ("dark", ReviewBackground::Black),
                ("light", ReviewBackground::White),
                ("busy", ReviewBackground::BusyChecker),
            ] {
                renderer.set_review_background(background);
                renderer.reset_perceptual_capture_state();
                let mut params = body.render_parameters(&genome, 0.3);
                params.render_mode = BodyRenderMode::ParticlePbf;
                params.presentation_visibility = 1.0;
                params.blink_left = if restored { 1.0 } else { 0.0 };
                params.blink_right = params.blink_left;
                for _ in 0..5 {
                    renderer.render_capture(params)?;
                }
                let started = Instant::now();
                for _ in 0..12 {
                    renderer.render_capture(params)?;
                }
                timings.push(
                    serde_json::json!({"shape": shape, "background": background_name,
                    "capture_ms_per_frame": started.elapsed().as_secs_f64()*1000.0/12.0,
                    "note": "CPU submission plus GPU readback; not a GPU timer"}),
                );
                let frame = renderer.render_capture(params)?;
                let mut bytes = Vec::with_capacity(frame.rgba8.len() + 8);
                bytes.extend(frame.width.to_le_bytes());
                bytes.extend(frame.height.to_le_bytes());
                bytes.extend(frame.rgba8);
                std::fs::write(
                    self.output.join(format!("{shape}-{background_name}.rgba")),
                    bytes,
                )?;
            }
        }
        let probes = self.output.join("probes");
        std::fs::create_dir_all(&probes)?;
        renderer.set_review_background(ReviewBackground::Black);
        for (shape, restored) in [("awake", false), ("resting", true)] {
            body = ProceduralBody::generate(&genome)?;
            body.apply_tuning_profile(profile.clone())?;
            if restored {
                body.restore_body_material_snapshot(&snapshot)?;
            }
            for probe in [
                "normal",
                "reflection",
                "no-coat",
                "high-curvature",
                "small",
                "motion",
                "gaze",
                "half-blink",
                "frown",
            ] {
                let mut params = body.render_parameters(&genome, 0.3);
                params.render_mode = BodyRenderMode::ParticlePbf;
                params.presentation_visibility = 1.0;
                params.material_bloom_strength = 0.0;
                params.shadow_opacity = 0.0;
                params.joy_aura = 0.0;
                params.blink_left = if restored { 1.0 } else { 0.0 };
                params.blink_right = params.blink_left;
                match probe {
                    "normal" => params.debug_view = pet_body::DebugView::MacroNormal,
                    "reflection" => params.debug_view = pet_body::DebugView::StudioReflection,
                    "no-coat" => params.material_studio_intensity = 0.0,
                    "high-curvature" => params.material_normal_scale = 4.0,
                    "small" => params.presentation_scale = 1.40,
                    "motion" => {
                        params.chromatic_motion = glam::Vec2::new(0.65, 0.10);
                        params.gaze.x = 0.65;
                    }
                    "gaze" => params.gaze.x = 0.65,
                    "half-blink" => {
                        params.blink_left = 0.55;
                        params.blink_right = 0.55;
                    }
                    "frown" => {
                        // Material stress probe, not a full affect/motor replay.
                        params.brow_tension = 0.80;
                        params.mouth_curve = -0.65;
                    }
                    _ => unreachable!(),
                }
                renderer.reset_perceptual_capture_state();
                let frame = renderer.render_capture(params)?;
                let mut bytes = Vec::with_capacity(frame.rgba8.len() + 8);
                bytes.extend(frame.width.to_le_bytes());
                bytes.extend(frame.height.to_le_bytes());
                bytes.extend(frame.rgba8);
                std::fs::write(probes.join(format!("{shape}-{probe}.rgba")), bytes)?;
            }
        }
        // Exercise the production velocity filter and unit conversion. No
        // chromatic_motion assignment: desktop feedback drives render params.
        let moving = self.output.join("velocity-motion");
        std::fs::create_dir_all(&moving)?;
        std::fs::create_dir_all(moving.join("wake-off"))?;
        body = ProceduralBody::generate(&genome)?;
        body.apply_tuning_profile(profile.clone())?;
        let desktop = glam::Vec2::new(3440.0, 1440.0);
        body.set_desktop_motion_space(desktop, 640.0);
        body.simulation.feedback.world_position = glam::Vec2::splat(0.5);
        let mut intent = lifecore::BodyIntent {
            locomotion: lifecore::LocomotionMode::Hover,
            target_position: glam::Vec2::splat(0.5),
            target_surface: None,
            desired_speed: 0.0,
            facing_direction: 1.0,
            gaze_target: None,
            pose: lifecore::PoseIntent::Neutral,
            expression: lifecore::ExpressionState::default(),
            interaction_target: None,
        };
        let mut motion_log = Vec::new();
        for (name, pixels_per_second) in [
            ("00-still", glam::Vec2::ZERO),
            ("01-right-slow", glam::Vec2::new(65.0, 0.0)),
            ("02-right", glam::Vec2::new(160.0, 0.0)),
            ("03-down", glam::Vec2::new(0.0, 160.0)),
            ("04-left-reversal", glam::Vec2::new(-160.0, 0.0)),
            ("05-stop", glam::Vec2::ZERO),
        ] {
            let velocity = pixels_per_second / desktop;
            intent.facing_direction = if velocity.x < 0.0 { -1.0 } else { 1.0 };
            for _ in 0..60 {
                body.simulation.feedback.velocity = velocity;
                body.simulation.feedback.world_position += velocity / 120.0;
                body.embodied_update(
                    &intent,
                    &lifecore::SensorFrame::default(),
                    lifecore::AffectState::default(),
                    pet_body::VisualMindInput::default(),
                    pet_body::VoiceVisualState::default(),
                    1.0 / 120.0,
                );
            }
            let mut params = body.render_parameters(&genome, 0.3);
            params.render_mode = BodyRenderMode::ParticlePbf;
            params.presentation_visibility = 1.0;
            renderer.set_review_background(ReviewBackground::Black);
            renderer.reset_perceptual_capture_state();
            let frame = renderer.render_capture(params)?;
            let mut bytes = Vec::with_capacity(frame.rgba8.len() + 8);
            bytes.extend(frame.width.to_le_bytes());
            bytes.extend(frame.height.to_le_bytes());
            bytes.extend(frame.rgba8);
            std::fs::write(moving.join(format!("{name}.rgba")), bytes)?;
            let mut wake_off = params;
            wake_off.chromatic_motion = glam::Vec2::ZERO;
            renderer.reset_perceptual_capture_state();
            let frame = renderer.render_capture(wake_off)?;
            let mut bytes = Vec::with_capacity(frame.rgba8.len() + 8);
            bytes.extend(frame.width.to_le_bytes());
            bytes.extend(frame.height.to_le_bytes());
            bytes.extend(frame.rgba8);
            std::fs::write(moving.join("wake-off").join(format!("{name}.rgba")), bytes)?;
            motion_log.push(serde_json::json!({"case":name,
                "physical_px_per_second":pixels_per_second.to_array(),
                "normalized_feedback":velocity.to_array(),
                "rendered_optical_velocity":params.chromatic_motion.to_array(),
                "scope":"synthetic physical path through production filter/body/native renderer"}));
        }
        std::fs::write(
            moving.join("velocity.json"),
            serde_json::to_vec_pretty(&motion_log)?,
        )?;
        std::fs::write(
            self.output.join("capture-timings.json"),
            serde_json::to_vec_pretty(&timings)?,
        )?;
        Ok(())
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut app = Capture {
        output: std::env::args()
            .nth(1)
            .ok_or("output path required")?
            .into(),
        fixture: std::env::args()
            .nth(2)
            .ok_or("copied fixture path required")?
            .into(),
        error: None,
    };
    EventLoop::new()?.run_app(&mut app)?;
    app.error.map_or(Ok(()), |e| Err(e.into()))
}
