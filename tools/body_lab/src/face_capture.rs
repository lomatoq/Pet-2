//! Deterministic production capture, isolated from all persistent pet state.
use glam::Vec2;
use lifecore::{
    AffectState, BodyIntent, FacePose, Genome, LocomotionMode, PoseIntent, SensorFrame,
};
use pet_body::{
    LiquidTuningProfile, ProceduralBody, Renderer, ReviewBackground, VisualMindInput,
    VoiceVisualState,
};
use std::{fs, io::Write, path::PathBuf, sync::Arc};
use winit::{
    application::ApplicationHandler,
    dpi::PhysicalSize,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoop},
    window::{Window, WindowId},
};

pub fn run(output: PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    fs::create_dir_all(&output)?;
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
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if let Err(error) = self.capture(event_loop) {
            self.error = Some(error.to_string());
        }
        event_loop.exit();
    }
    fn window_event(&mut self, _: &ActiveEventLoop, _: WindowId, _: WindowEvent) {}
}

impl Capture {
    fn capture(&self, event_loop: &ActiveEventLoop) -> Result<(), Box<dyn std::error::Error>> {
        let window = Arc::new(
            event_loop.create_window(
                Window::default_attributes()
                    .with_title("Pet Lab · production face capture")
                    .with_inner_size(PhysicalSize::new(512, 512)),
            )?,
        );
        let genome = Genome::from_seed(42);
        let profile: LiquidTuningProfile = serde_json::from_str(include_str!(
            "../../../config/embodiment/active-liquid-profile-r11.json"
        ))?;
        let initial = ProceduralBody::generate(&genome)?;
        let mut renderer = pollster::block_on(Renderer::new(window.clone(), &initial.mesh))?;
        let mut records = Vec::new();
        for scale in [1.0_f32, 1.5, 2.0] {
            let pixels = (512.0 * scale) as u32;
            let _ = window.request_inner_size(PhysicalSize::new(pixels, pixels));
            renderer.resize(PhysicalSize::new(pixels, pixels));
            for (background_name, background) in [
                ("black", ReviewBackground::Black),
                ("white", ReviewBackground::White),
                ("busy", ReviewBackground::BusyChecker),
            ] {
                for pose in FacePose::ALL {
                    let mut body = Box::new(ProceduralBody::generate(&genome)?);
                    body.apply_tuning_profile(profile.clone())?;
                    body.set_render_aspect(1.0);
                    body.set_desktop_motion_space(Vec2::splat(pixels as f32), pixels as f32);
                    // Fixed pixel reference: approximately 190 px diameter at 100%.
                    body.set_presentation_scale(0.8);
                    let intent = BodyIntent {
                        locomotion: LocomotionMode::Hover,
                        target_position: Vec2::splat(0.5),
                        target_surface: None,
                        desired_speed: 0.0,
                        facing_direction: 1.0,
                        gaze_target: Some(Vec2::new(0.55, 0.5)),
                        pose: PoseIntent::Neutral,
                        expression: pose.expression(),
                        interaction_target: None,
                    };
                    for _ in 0..180 {
                        body.embodied_update(
                            &intent,
                            &SensorFrame::default(),
                            AffectState::default(),
                            VisualMindInput::default(),
                            VoiceVisualState::default(),
                            1.0 / 120.0,
                        );
                    }
                    body.presentation_update(1.0 / 60.0);
                    renderer.set_review_background(background);
                    renderer.reset_perceptual_capture_state();
                    let parameters = body.render_parameters(&genome, 0.0);
                    let frame = renderer.render_capture(parameters)?;
                    let name = format!("{pose:?}-{background_name}-{}.ppm", (scale * 100.0) as u32);
                    let mut file = fs::File::create(self.output.join(&name))?;
                    write!(file, "P6\n{} {}\n255\n", frame.width, frame.height)?;
                    let rgb: Vec<u8> = frame
                        .rgba8
                        .chunks_exact(4)
                        .flat_map(|px| px[..3].iter().copied())
                        .collect();
                    file.write_all(&rgb)?;
                    records.push(serde_json::json!({"file": name, "pose": pose,
                        "desired": intent.expression, "smoothed": body.expression.current,
                        "renderer_geometry": parameters.geometry, "renderer_mouth_open": parameters.mouth_open,
                        "renderer_blink": [parameters.blink_left, parameters.blink_right]}));
                }
            }
        }
        let _ = window.request_inner_size(PhysicalSize::new(512, 512));
        renderer.resize(PhysicalSize::new(512, 512));
        for mode in [
            pet_motor::ShapeMode::Neutral,
            pet_motor::ShapeMode::Reach,
            pet_motor::ShapeMode::Present,
            pet_motor::ShapeMode::Guard,
            pet_motor::ShapeMode::Settle,
            pet_motor::ShapeMode::Recoil,
        ] {
            let mut body = Box::new(ProceduralBody::generate(&genome)?);
            let mut shape_profile = profile.clone();
            shape_profile.face.visible = false;
            shape_profile.material.halo = 0.0;
            shape_profile.material.soul_glow_strength = 0.0;
            body.apply_tuning_profile(shape_profile)?;
            body.set_render_aspect(1.0);
            body.set_desktop_motion_space(Vec2::splat(512.0), 512.0);
            body.set_presentation_scale(0.8);
            let intent = BodyIntent {
                locomotion: LocomotionMode::Hover,
                target_position: Vec2::splat(0.5),
                target_surface: None,
                desired_speed: 0.0,
                facing_direction: 1.0,
                gaze_target: None,
                pose: PoseIntent::Neutral,
                expression: FacePose::Awake.expression(),
                interaction_target: None,
            };
            for tick in 0..360 {
                body.set_somatic_actuation(pet_motor::SomaticActuationPacket {
                    shape: pet_motor::ShapeIntent {
                        mode,
                        axis: Vec2::X,
                        strength: (tick as f32 / 60.0).min(1.0),
                    },
                    ..Default::default()
                });
                body.embodied_update(
                    &intent,
                    &SensorFrame::default(),
                    AffectState::default(),
                    VisualMindInput::default(),
                    VoiceVisualState::default(),
                    1.0 / 120.0,
                );
            }
            body.presentation_update(1.0 / 60.0);
            renderer.set_review_background(ReviewBackground::White);
            renderer.reset_perceptual_capture_state();
            let frame = renderer.render_capture(body.render_parameters(&genome, 0.0))?;
            let name = format!("shape-{mode:?}.ppm");
            let mut file = fs::File::create(self.output.join(&name))?;
            write!(file, "P6\n{} {}\n255\n", frame.width, frame.height)?;
            let rgb: Vec<u8> = frame
                .rgba8
                .chunks_exact(4)
                .flat_map(|p| p[..3].iter().copied())
                .collect();
            file.write_all(&rgb)?;
            records.push(serde_json::json!({"file":name,"physical_diagnostics":format!("{:?}",body.embodiment.liquid.diagnostics())}));
        }
        fs::write(
            self.output.join("channels.json"),
            serde_json::to_vec_pretty(&records)?,
        )?;
        Ok(())
    }
}
