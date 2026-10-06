//! Neuro material review: actual production material, no desktop capture or save edits.
use glam::Vec2;
use lifecore::{
    AffectState, BodyIntent, ExpressionState, Genome, LocomotionMode, PoseIntent, SensorFrame,
};
use pet_body::{
    EnergyExpression, LiquidTuningProfile, ProceduralBody, Renderer, ReviewBackground,
    VisualMindInput, VoiceVisualState,
};
use std::{path::PathBuf, sync::Arc};
use winit::{
    application::ApplicationHandler,
    dpi::PhysicalSize,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoop},
    window::{Window, WindowId},
};
struct Review {
    profile: PathBuf,
    out: PathBuf,
    error: Option<String>,
}
impl ApplicationHandler for Review {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if let Err(e) = self.capture(event_loop) {
            self.error = Some(e.to_string());
        }
        event_loop.exit();
    }
    fn window_event(&mut self, _: &ActiveEventLoop, _: WindowId, _: WindowEvent) {}
}
impl Review {
    fn capture(&self, event_loop: &ActiveEventLoop) -> Result<(), Box<dyn std::error::Error>> {
        std::fs::create_dir_all(&self.out)?;
        let profile: LiquidTuningProfile =
            serde_json::from_str(&std::fs::read_to_string(&self.profile)?)?;
        let genome = Genome::from_seed(profile.seed);
        let mut body = ProceduralBody::generate(&genome)?;
        body.apply_tuning_profile(profile)?;
        body.set_desktop_motion_space(Vec2::new(768.0, 512.0), 512.0);
        body.set_render_aspect(1.5);
        body.simulation.feedback.world_position = Vec2::splat(0.5);
        let window = Arc::new(
            event_loop.create_window(
                Window::default_attributes()
                    .with_title("Pet2 isolated material review")
                    .with_visible(false)
                    .with_inner_size(PhysicalSize::new(768, 512)),
            )?,
        );
        let mut renderer = pollster::block_on(Renderer::new(window, &body.mesh))?;
        let intent = BodyIntent {
            locomotion: LocomotionMode::Arrive,
            target_position: Vec2::splat(0.5),
            target_surface: None,
            desired_speed: 0.0,
            facing_direction: 1.0,
            gaze_target: None,
            pose: PoseIntent::Neutral,
            expression: ExpressionState::default(),
            interaction_target: None,
        };
        let mind = VisualMindInput {
            arousal: 0.15,
            curiosity: 0.0,
            fatigue: 0.0,
            ..Default::default()
        };
        let affect = AffectState {
            arousal: 0.15,
            stress: 0.0,
            ..Default::default()
        };
        let sensors = SensorFrame::default();
        let mut metrics = Vec::new();
        for tick in 0..721 {
            let dt = 1.0 / 120.0;
            body.fixed_update(&genome, &intent, &sensors, dt);
            body.embodied_update(
                &intent,
                &sensors,
                affect,
                mind,
                VoiceVisualState::default(),
                dt,
            );
            body.presentation_update(dt);
            if [120, 240, 480, 720].contains(&tick) {
                renderer.set_review_background(ReviewBackground::Black);
                let frame = renderer.render_capture(body.render_parameters(&genome, 0.15))?;
                save(&self.out.join(format!("physical-{tick}.rgba")), frame)?;
                let d = body.embodiment.liquid.diagnostics();
                metrics.push(serde_json::json!({"tick":tick,"finite":d.finite,"main_mass":d.main_mass,
                    "components":d.component_count,"max_speed":d.maximum_speed,"stretch":d.stretch_ratio,"failsafe":d.failsafe_hits}));
            }
        }
        // Freeze ALL geometry. Only the production EnergyExpression advances.
        let base = body.render_parameters(&genome, 0.15);
        for (name, bg) in [
            ("black", ReviewBackground::Black),
            ("white", ReviewBackground::White),
            ("busy", ReviewBackground::BusyChecker),
        ] {
            renderer.set_review_background(bg);
            let mut energy = EnergyExpression::default();
            for tick in 0..721 {
                energy.update(affect, mind, ExpressionState::default(), false, 1.0 / 120.0);
                if [0, 120, 360, 720].contains(&tick) {
                    let mut parameters = base;
                    parameters.energy = energy.appearance;
                    save(
                        &self.out.join(format!("frozen-{name}-{tick}.rgba")),
                        renderer.render_capture(parameters)?,
                    )?;
                }
            }
        }
        // Geometry is frozen; these are authored optical/face fixtures, not an
        // autonomous behavior test. Every frame uses the real production GPU path.
        let mut optical = Vec::new();
        for (label, pose, affect, sleeping) in [
            (
                "calm",
                lifecore::FacePose::Awake,
                AffectState {
                    arousal: 0.15,
                    stress: 0.0,
                    ..Default::default()
                },
                false,
            ),
            (
                "joy",
                lifecore::FacePose::Playful,
                AffectState {
                    valence: 0.9,
                    arousal: 0.65,
                    stress: 0.0,
                    frustration: 0.0,
                    ..Default::default()
                },
                false,
            ),
            (
                "anger",
                lifecore::FacePose::Boundary,
                AffectState {
                    valence: -0.8,
                    arousal: 0.85,
                    frustration: 0.9,
                    stress: 0.35,
                    ..Default::default()
                },
                false,
            ),
            (
                "sleep",
                lifecore::FacePose::Tired,
                AffectState {
                    arousal: 0.05,
                    stress: 0.0,
                    frustration: 0.0,
                    ..Default::default()
                },
                true,
            ),
        ] {
            let face = pose.expression();
            let mut energy = EnergyExpression::default();
            for _ in 0..1440 {
                energy.update(affect, mind, face, sleeping, 1.0 / 120.0);
            }
            let mut parameters = base;
            parameters.energy = energy.appearance;
            parameters.geometry = face.geometry;
            parameters.brow_raise = face.brow_raise;
            parameters.brow_tension = face.brow_tension;
            parameters.brow_asymmetry = face.brow_asymmetry;
            parameters.mouth_curve = face.mouth_curve;
            parameters.mouth_open = face.mouth_open;
            parameters.mouth_tension = face.mouth_tension;
            parameters.eye_aperture = face.eye_aperture;
            if sleeping {
                parameters.blink_left = 1.0;
                parameters.blink_right = 1.0;
            }
            optical.push(
                serde_json::json!({"label":label,"energy":format!("{:?}",energy.appearance)}),
            );
            for width in [768, 384, 256] {
                renderer.resize(PhysicalSize::new(width, width * 2 / 3));
                for (bg_name, bg) in [
                    ("black", ReviewBackground::Black),
                    ("white", ReviewBackground::White),
                    ("busy", ReviewBackground::BusyChecker),
                ] {
                    renderer.set_review_background(bg);
                    save(
                        &self
                            .out
                            .join(format!("state-{label}-{width}-{bg_name}.rgba")),
                        renderer.render_capture(parameters)?,
                    )?;
                }
            }
            if matches!(label, "joy" | "anger") {
                renderer.resize(PhysicalSize::new(384, 256));
                renderer.set_review_background(ReviewBackground::Black);
                for frame in 0..180 {
                    energy.update(affect, mind, face, sleeping, 1.0 / 30.0);
                    parameters.energy = energy.appearance;
                    save(
                        &self.out.join(format!("emotion-{label}-{frame:03}.rgba")),
                        renderer.render_capture(parameters)?,
                    )?;
                }
            }
        }
        renderer.resize(PhysicalSize::new(384, 256));
        renderer.set_review_background(ReviewBackground::Black);
        let mut parameters = base;
        parameters.energy.dynamics.y = 1.4;
        // TAU is approached from both sides with all other inputs held fixed.
        for (name, phase) in [
            ("before", std::f32::consts::TAU - 0.0001),
            ("after", 0.0001),
        ] {
            parameters.energy.dynamics.x = phase;
            save(
                &self.out.join(format!("wrap-{name}.rgba")),
                renderer.render_capture(parameters)?,
            )?;
        }
        let mut energy = EnergyExpression::default();
        let mut milliseconds = Vec::new();
        for frame in 0..180 {
            // A sampled six-second sequence, not wall-clock or browser RAF.
            energy.update(affect, mind, ExpressionState::default(), false, 1.0 / 30.0);
            parameters.energy = energy.appearance;
            let started = std::time::Instant::now();
            let image = renderer.render_capture(parameters)?;
            milliseconds.push(started.elapsed().as_secs_f64() * 1000.0);
            save(&self.out.join(format!("sequence-{frame:03}.rgba")), image)?;
        }
        parameters.chromatic_motion = Vec2::new(0.8, 0.0);
        save(
            &self.out.join("travel-right.rgba"),
            renderer.render_capture(parameters)?,
        )?;
        std::fs::write(
            self.out.join("optical.json"),
            serde_json::to_vec_pretty(&serde_json::json!({
                "fixtures":optical,"sequence_frames":180,"sequence_dt":1.0/30.0,
                "capture_roundtrip_ms":milliseconds,"timing_scope":"CPU submission + complete GPU pipeline + blocking readback, not GPU timestamp or app FPS"
            }))?,
        )?;
        std::fs::write(
            self.out.join("physics.json"),
            serde_json::to_vec_pretty(&metrics)?,
        )?;
        Ok(())
    }
}
fn save(path: &std::path::Path, frame: pet_body::CapturedFrame) -> Result<(), std::io::Error> {
    let mut data = Vec::new();
    data.extend(frame.width.to_le_bytes());
    data.extend(frame.height.to_le_bytes());
    data.extend(frame.rgba8);
    std::fs::write(path, data)
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 2 {
        return Err("usage: neuro_review PROFILE_JSON OUTPUT_DIR".into());
    }
    let mut review = Review {
        profile: args[0].clone().into(),
        out: args[1].clone().into(),
        error: None,
    };
    EventLoop::new()?.run_app(&mut review)?;
    review.error.map_or(Ok(()), |e| Err(e.into()))
}
