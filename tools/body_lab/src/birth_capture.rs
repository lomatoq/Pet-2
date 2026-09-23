//! Reproducible GPU fixtures for the native birth scene, without user state.
use glam::{Vec2, Vec3};
use lifecore::{
    AffectState, BodyIntent, FacePose, Genome, LocomotionMode, PoseIntent, SensorFrame,
};
use pet_body::{
    LiquidTuningProfile, ProceduralBody, Renderer, ReviewBackground, VisualMindInput,
    VoiceVisualState,
};
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
        if let Err(error) = self.render(event_loop) {
            self.error = Some(error.to_string());
        }
        event_loop.exit();
    }
    fn window_event(&mut self, _: &ActiveEventLoop, _: WindowId, _: WindowEvent) {}
}
impl Capture {
    fn render(&self, event_loop: &ActiveEventLoop) -> Result<(), Box<dyn std::error::Error>> {
        let window = Arc::new(
            event_loop.create_window(
                Window::default_attributes()
                    .with_title("Pet 2 · First Light verification")
                    .with_inner_size(PhysicalSize::new(1280, 800)),
            )?,
        );
        let genome = Genome::from_seed(42);
        let mut body = ProceduralBody::generate(&genome)?;
        let profile: LiquidTuningProfile = serde_json::from_str(include_str!(
            "../../../config/embodiment/active-liquid-profile-r11.json"
        ))?;
        body.apply_tuning_profile(profile.clone())?;
        let mut renderer = pollster::block_on(Renderer::new(window.clone(), &body.mesh))?;
        renderer.set_review_background(ReviewBackground::Transparent);
        let mut scene =
            pet_body::birth_scene::BirthScene::new(renderer.device(), renderer.surface_format());
        let mut ecology_renderer = pet_body::EcologyRenderer::new(
            renderer.device(),
            renderer.surface_format(),
            renderer.premultiplied_output(),
        );
        for (name, width, height, time, pose) in [
            ("trails", 1280, 800, 1.9, FacePose::Awake),
            ("wink", 640, 480, 15.0, FacePose::Awake),
            ("neutral", 640, 480, 15.0, FacePose::Awake),
            ("seated", 640, 480, 15.0, FacePose::Awake),
            ("cradle-side-left", 640, 480, 15.0, FacePose::Awake),
            ("cradle-below", 640, 480, 15.0, FacePose::Awake),
            ("seated-large", 1280, 800, 15.0, FacePose::Awake),
            ("shouting", 640, 480, 15.0, FacePose::Awake),
            ("flattened", 640, 480, 15.0, FacePose::Awake),
            ("speaking", 640, 480, 15.0, FacePose::Awake),
            ("mouth-parted", 640, 480, 15.0, FacePose::Awake),
            ("squint", 640, 480, 15.0, FacePose::Awake),
            ("asymmetric", 640, 480, 15.0, FacePose::Confused),
            ("capsule-arrival", 1280, 800, 4.0, FacePose::Awake),
            ("capsule-opening", 1280, 800, 8.1, FacePose::Awake),
            ("capsule-burst", 1280, 800, 8.45, FacePose::Awake),
            ("lamps", 1280, 800, 7.30, FacePose::Awake),
            ("asleep", 640, 480, 15.0, FacePose::Tired),
            ("look-left", 640, 480, 15.0, FacePose::Awake),
            ("look-right", 640, 480, 15.0, FacePose::Awake),
            ("look-up", 640, 480, 15.0, FacePose::Awake),
            ("happy", 640, 480, 15.0, FacePose::Playful),
            ("newborn", 1280, 800, 10.0, FacePose::Awake),
            ("portrait", 800, 1280, 4.0, FacePose::Awake),
            ("small-screen", 640, 480, 4.0, FacePose::Awake),
            ("angry", 640, 480, 12.0, FacePose::Boundary),
            ("flight", 1280, 800, 15.0, FacePose::Awake),
            ("flight-turn", 1280, 800, 15.0, FacePose::Awake),
            ("flight-arc-early", 1280, 800, 15.0, FacePose::Awake),
            ("flight-arc-middle", 1280, 800, 15.0, FacePose::Awake),
            ("flight-arc-late", 1280, 800, 15.0, FacePose::Awake),
            ("sad", 640, 480, 12.0, FacePose::Awake),
            ("scared", 640, 480, 12.0, FacePose::Startled),
        ] {
            body = ProceduralBody::generate(&genome)?;
            body.apply_tuning_profile(profile.clone())?;
            let size = PhysicalSize::new(width, height);
            let _ = window.request_inner_size(size);
            renderer.resize(size);
            body.set_render_aspect(width as f32 / height as f32);
            body.embodiment.managed_blink = true;
            let mut expression = pose.expression();
            if name == "sad" {
                expression.mouth_curve = -0.8;
                expression.brow_tension = 0.04;
            }
            let intent = BodyIntent {
                target_position: Vec2::splat(0.5),
                target_surface: None,
                facing_direction: 1.0,
                locomotion: LocomotionMode::Hover,
                desired_speed: 0.0,
                gaze_target: None,
                pose: PoseIntent::Neutral,
                expression,
                interaction_target: None,
            };
            body.set_presentation_scale(1.845 * height as f32 / 1152.0 / 0.82);
            body.set_desktop_motion_space(Vec2::new(width as f32, height as f32), height as f32);
            body.embodiment.set_motion_response_scale(2.15);
            body.embodiment.set_motion_acceleration_limit(4.20);
            for step in 0..if name.starts_with("flight-arc") {
                match name {
                    "flight-arc-early" => 260,
                    "flight-arc-middle" => 290,
                    _ => 340,
                }
            } else if name.starts_with("flight")
                || name == "flattened"
                || name.starts_with("seated")
            {
                480
            } else {
                180
            } {
                if name.starts_with("flight-arc") {
                    let angle = ((step as f32 - 240.0) / 120.0).max(0.0) * 3.0;
                    let axis = Vec2::from_angle(angle);
                    body.simulation.feedback.velocity = axis * 0.15;
                    body.simulation.feedback.acceleration = if step < 240 {
                        Vec2::ZERO
                    } else {
                        axis.perp() * 0.45
                    };
                } else if name.starts_with("flight") {
                    let turn = if name == "flight-turn" && step >= 240 {
                        -1.0
                    } else {
                        1.0
                    };
                    body.simulation.feedback.velocity = Vec2::new(0.15 * turn, -0.04);
                    body.simulation.feedback.acceleration =
                        if step < 20 || (name == "flight-turn" && (240..260).contains(&step)) {
                            Vec2::new(0.45 * turn, 0.0)
                        } else {
                            Vec2::ZERO
                        };
                } else {
                    body.simulation.feedback.velocity = Vec2::ZERO;
                    body.simulation.feedback.acceleration = Vec2::ZERO;
                }

                if name == "flattened" || name.starts_with("seated") {
                    let floor = if name.starts_with("seated") {
                        0.5 + pet_body::birth_scene::den_seat_depth_pixels([width, height], 1.0)
                            / height as f32
                    } else {
                        0.78
                    };
                    body.simulation.feedback.world_position = Vec2::new(
                        0.5,
                        floor
                            - body.liquid_contact_bounds_pixels(height as f32).maximum.y
                                / height as f32,
                    );
                    let packet = pet_motor::SomaticActuationPacket {
                        support: Some(pet_motor::SurfaceAttachmentCommand {
                            surface_id: lifecore::SurfaceId("den:cushion".into()),
                            anchor_point: Vec2::new(0.5, floor),
                            normal: Vec2::NEG_Y,
                            tangent: Vec2::X,
                            target_contact_fraction: 0.40,
                            normal_compliance: 0.25,
                            tangent_friction: 0.6,
                            adhesion: 0.20,
                            load_fraction: 0.40,
                            break_force: 0.75,
                            release_half_life: 0.18,
                        }),
                        ..Default::default()
                    };
                    body.set_somatic_actuation(packet);
                    if name.starts_with("seated") {
                        let w = (310.0_f32)
                            .min(width as f32 * 0.8)
                            .min(height as f32 * 0.65);
                        body.set_cradle_bounds_pixels(
                            Some((
                                Vec2::new(width as f32 * 0.5 - w * 0.285, floor * height as f32),
                                Vec2::new(width as f32 * 0.5 + w * 0.285, 0.0),
                            )),
                            body.simulation.feedback.world_position
                                * Vec2::new(width as f32, height as f32),
                            height as f32,
                        );
                    }
                }
                body.embodied_update(
                    &intent,
                    &SensorFrame::default(),
                    AffectState::default(),
                    VisualMindInput::default(),
                    if name == "shouting" {
                        VoiceVisualState {
                            active: true,
                            shout: 0.95,
                            envelope: 0.95,
                            ..Default::default()
                        }
                    } else {
                        VoiceVisualState::default()
                    },
                    1.0 / 120.0,
                );
                body.presentation_update(1.0 / 120.0);
            }
            body.set_presentation_scale(1.845 * height as f32 / 1152.0 / 0.82);
            if name.starts_with("seated") {
                body.set_presentation_offset_pixels(
                    (body.simulation.feedback.world_position - Vec2::splat(0.5))
                        * Vec2::new(width as f32, height as f32),
                    height as f32,
                );
            }
            if name.starts_with("cradle-") {
                body.set_presentation_offset_pixels(
                    if name == "cradle-side-left" {
                        Vec2::new(-118.0, 30.0)
                    } else {
                        Vec2::new(0.0, 102.0)
                    },
                    height as f32,
                );
            }
            let mut params = body.render_parameters(&genome, 0.3);
            params.birth_roundness = if time < 11.2 { 1.0 } else { 0.0 };
            if name == "speaking" {
                params.mouth_open = 0.95;
                params.audio_envelope = 0.90;
                params.mouth_shout = 0.8;
            }
            if name == "mouth-parted" {
                params.mouth_open = 0.25;
            }
            if name == "squint" {
                params.eye_aperture = 0.45;
                params.squint = 0.55;
            }
            if name == "wink" {
                params.blink_left = 1.0;
                params.blink_right = 0.0;
            }
            if name == "asleep" {
                params.blink_left = 1.0;
                params.blink_right = 1.0;
            }
            if name == "look-left" {
                params.gaze = Vec2::new(-0.85, 0.0);
            }
            if name == "look-right" {
                params.gaze = Vec2::new(0.85, 0.0);
            }
            if name == "look-up" {
                params.gaze = Vec2::new(0.0, 0.85);
            }
            if time < 11.2 {
                params.mouth_curve = 0.28;
                params.brow_tension = 0.0;
                params.brow_raise = 0.15;
                params.gaze = Vec2::ZERO;
                params.blink_left = 0.0;
                params.blink_right = 0.0;
            }
            params.exposure = 1.0;
            params.material_bloom_strength = 0.05;
            params.shadow_color = Vec3::new(0.008, 0.012, 0.020);
            params.shadow_opacity = 0.16;
            params.shadow_feather = 32.0;
            params.shadow_horizontal_offset = 0.0;
            params.shadow_vertical_offset = 0.0;
            params.render_scale = 1;
            params.presentation_visibility = pet_body::birth_scene::smooth(7.96, 8.65, time);
            let den = if name.starts_with("seated") || name.starts_with("cradle-") {
                [0.5, 0.5]
            } else {
                [
                    (170.0 / width as f32).min(0.5),
                    1.0 - 310.0 * 0.292 / height as f32,
                ]
            };
            let mut ecology = pet_ecology::EcologyState::new(42);
            ecology.den.anchor = Vec2::from_array(den);
            let orb = &mut ecology.objects[0];
            orb.lifecycle = pet_ecology::ObjectLifecycle::StoredInDen;
            orb.position = ecology.den.anchor
                + Vec2::new(
                    80.0 / width as f32,
                    pet_body::birth_scene::den_seat_depth_pixels([width, height], 1.0)
                        / height as f32
                        - orb.radius_px_at_reference / 1080.0,
                );
            ecology_renderer.prepare(
                renderer.queue(),
                &ecology,
                width as f32 / height as f32,
                15.0,
            );
            let scene = std::cell::RefCell::new(&mut scene);
            let inside = name.starts_with("seated");
            let outside = name.starts_with("cradle-");
            let frame = renderer.render_capture_with_layers(
                params,
                |device, queue, encoder, view| {
                    if inside || outside {
                        ecology_renderer.render_background_objects(encoder, view);
                    }
                    if outside {
                        let mut scene = scene.borrow_mut();
                        scene.render(
                            device,
                            queue,
                            encoder,
                            view,
                            [width, height],
                            den,
                            1.0,
                            None,
                        );
                        scene.render_den_front(
                            device,
                            queue,
                            encoder,
                            view,
                            [width, height],
                            den,
                            1.0,
                        );
                    }
                },
                |device, queue, encoder, view| {
                    if !outside {
                        let mut scene = scene.borrow_mut();
                        scene.render(
                            device,
                            queue,
                            encoder,
                            view,
                            [width, height],
                            den,
                            1.0,
                            if inside {
                                None
                            } else {
                                Some((time, [0.0, 0.0, width as f32, height as f32]))
                            },
                        );
                        scene.render_den_front(
                            device,
                            queue,
                            encoder,
                            view,
                            [width, height],
                            den,
                            1.0,
                        );
                    }
                    if inside || outside {
                        ecology_renderer.render_foreground_objects(encoder, view);
                    }
                },
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
