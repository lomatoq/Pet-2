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
        let eyes_only = std::env::args().any(|arg| arg == "--eye-emotions-only");
        let care_only = std::env::args().any(|arg| arg == "--self-care-only");
        let mood_only = std::env::args().any(|arg| arg == "--mood-colors-only");
        for scale in [1.0_f32, 1.5, 2.0] {
            if (care_only || mood_only) && scale != 1.0 {continue;}
            let pixels = (512.0 * scale) as u32;
            let _ = window.request_inner_size(PhysicalSize::new(pixels, pixels));
            renderer.resize(PhysicalSize::new(pixels, pixels));
            for (background_name, background) in [
                ("black", ReviewBackground::Black),
                ("white", ReviewBackground::White),
                ("busy", ReviewBackground::BusyChecker),
            ] {
                let mut fixtures: Vec<_> = FacePose::ALL
                    .into_iter()
                    .map(|pose| (format!("{pose:?}"), pose.expression(), false))
                    .collect();
                let mut playful = FacePose::Playful.expression();
                playful.mouth_asymmetry = 0.4;
                let mut skeptical = FacePose::Awake.expression();
                skeptical.brow_asymmetry = 0.6;
                skeptical.mouth_asymmetry = -0.3;
                skeptical.mouth_compression = 0.35;
                let mut concern = FacePose::Awake.expression();
                concern.mouth_curve = -0.65;
                concern.brow_tension = 0.05;
                let mut effort = concern;
                effort.brow_tension = 0.9;
                effort.mouth_tension = 0.85;
                effort.mouth_compression = 0.8;
                effort.effort = 0.9;
                let mut yawn = FacePose::Awake.expression();
                yawn.mouth_open = 0.95;
                yawn.geometry.mouth[0] = 0.85;
                let mut angry_shout = FacePose::Boundary.expression();
                angry_shout.mouth_curve = -0.08;
                let mut excited = FacePose::Playful.expression();
                excited.brow_raise = 0.65;
                excited.mouth_open = 0.55;
                excited.brow_asymmetry = 0.18;
                let mut goofy = playful;
                goofy.brow_asymmetry = -0.40;
                goofy.geometry.lids[0][2] = 0.46;
                goofy.geometry.lids[1][0] = 0.28;
                for (name, expression) in [
                    ("MoodInterest", FacePose::Curious.expression()),
                    ("MoodAngry", FacePose::Boundary.expression()),
                    ("MoodSleepy", FacePose::Tired.expression()),
                    ("MoodExcited", excited),
                    ("MoodOdd", FacePose::Confused.expression()),
                    ("MoodGoofy", goofy),
                    ("RigPlayful", playful),
                    ("RigSkeptical", skeptical),
                    ("RigConcern", concern),
                    ("RigEffort", effort),
                    ("RigStartle", FacePose::Startled.expression()),
                    ("RigYawn", yawn),
                    ("RigAngryShout", angry_shout),
                ] {
                    fixtures.push((name.to_owned(), expression, true));
                }
                if eyes_only {
                    fixtures.clear();
                    for (name, curve) in [("Neutral",0.0),("JoyMild",0.3),("JoyStrong",0.95),("SadMild",-0.25),("SadStrong",-0.85)] {
                        let mut expression=FacePose::Awake.expression();
                        expression.mouth_curve=curve;
                        expression.brow_tension=0.0;
                        expression.brow_raise=0.0;
                        expression.squint=0.0;
                        expression.blink_left=0.0;
                        expression.blink_right=0.0;
                        fixtures.push((name.to_owned(),expression,true));
                    }
                    fixtures.push(("AngerControl".into(),FacePose::Boundary.expression(),true));
                    fixtures.push(("SurpriseControl".into(),FacePose::Startled.expression(),true));
                    for (name,curve,tension,raise) in [
                        ("SmallFrown",-0.10,0.0,0.0),
                        ("TenseFrown",-0.42,0.60,0.05),
                        ("AlarmFrown",-0.42,0.68,0.70),
                        ("JoyAlert",0.48,0.06,0.70),
                    ] {
                        let mut expression=FacePose::Awake.expression();
                        expression.mouth_curve=curve;
                        expression.brow_tension=tension;
                        expression.brow_raise=raise;
                        expression.mouth_open=0.35;
                        expression.blink_left=0.0;
                        expression.blink_right=0.0;
                        fixtures.push((name.into(),expression,true));
                    }
                }
                if care_only {
                    fixtures.clear();
                    for name in ["CareNeutral","CareGroom","CareGroomShort","CareGroomLeft","CareScratch","CareNuzzle","CareDecline","CareFeedingPriority"] {
                        fixtures.push((name.into(),FacePose::Awake.expression(),true));
                    }
                    for (name,primary) in [
                        ("GeneralJoy",lifecore::PrimaryIntent::Celebrate),
                        ("GeneralConcern",lifecore::PrimaryIntent::RecoverFromMiss),
                        ("GeneralBoundary",lifecore::PrimaryIntent::RejectContact),
                        ("GeneralStartle",lifecore::PrimaryIntent::StartleFreeze),
                        ("GeneralCurious",lifecore::PrimaryIntent::Inspect),
                        ("GeneralRest",lifecore::PrimaryIntent::Rest),
                    ] {
                        let mut director=pet_body::CompanionExpressionDirector::new(42);
                        let motor=lifecore::CompanionIntentFrame {primary,confidence:0.8,
                            curiosity:0.8,play_readiness:0.8,frustration:0.8,surprise:0.8,
                            ..Default::default()};
                        let mut target=pet_body::CompanionExpressionTarget::default();
                        for _ in 0..30 {target=director.tick(motor,Default::default(),0.05);}
                        let f=target.face;
                        let expression=lifecore::ExpressionState {eye_aperture:f.eye_aperture,
                            squint:f.squint,pupil_size:f.pupil_size,pupil_focus:f.pupil_focus,
                            brow_raise:f.brow_raise,brow_tension:f.brow_tension,brow_asymmetry:f.brow_asymmetry,
                            mouth_curve:f.mouth_curve,mouth_open:f.mouth_open,mouth_tension:f.mouth_tension,
                            mouth_compression:f.mouth_compression,mouth_asymmetry:f.mouth_asymmetry,
                            ..Default::default()};
                        fixtures.push((name.into(),expression,true));
                    }
                }
                if mood_only {
                    fixtures.clear();
                    for (name,curve,tension,raise) in [
                        ("Calm",0.0,0.0,0.0), ("Delight",0.95,0.0,0.0),
                        ("DelightOnset",0.95,0.0,0.0), ("Content",0.45,0.0,0.0),
                        ("Concern",-0.7,0.0,0.0), ("Tense",-0.4,0.8,0.0),
                        ("Curious",0.05,0.0,0.8),
                    ] {
                        let mut face=FacePose::Awake.expression();
                        face.mouth_curve=curve;face.brow_tension=tension;face.brow_raise=raise;
                        fixtures.push((name.into(),face,true));
                    }
                }
                for (fixture, expression, managed) in fixtures {
                    let mut body = Box::new(ProceduralBody::generate(&genome)?);
                    body.apply_tuning_profile(profile.clone())?;
                    // Rig fixtures test representational capacity, not autonomous selection.
                    // Preserve legacy pose captures unchanged for incumbent comparisons.
                    body.embodiment.managed_blink = managed;
                    body.set_render_aspect(1.0);
                    body.set_desktop_motion_space(Vec2::splat(pixels as f32), pixels as f32);
                    // Fixed pixel reference: approximately 190 px diameter at 100%.
                    body.set_presentation_scale(0.8);
                    let care = match fixture.as_str() {
                        "CareGroomShort" | "CareGroomLeft" => lifecore::SelfCareMotorFrame {
                            kind:lifecore::SelfCareKind::Groom,strength:0.95,side:-1.0,
                            tongue_extension:if fixture=="CareGroomShort" {0.3} else {0.95},
                            mouth_open:0.28,squint:0.25,face_lowering:0.7,face_turn:-0.25,
                            body_lean:-0.3,..Default::default()},
                        "CareGroom" | "CareFeedingPriority" => lifecore::SelfCareMotorFrame {
                            kind:lifecore::SelfCareKind::Groom,strength:0.95,side:1.0,
                            tongue_extension:0.95,mouth_open:0.28,squint:0.25,
                            face_lowering:0.7,face_turn:0.25,body_lean:0.3,..Default::default()},
                        "CareScratch" => lifecore::SelfCareMotorFrame {
                            kind:lifecore::SelfCareKind::Scratch,strength:0.9,side:-1.0,
                            squint:0.75,mouth_asymmetry:-0.7,body_lean:-0.8,body_pulse:0.8,
                            face_turn:-0.6,..Default::default()},
                        "CareNuzzle" => lifecore::SelfCareMotorFrame {
                            kind:lifecore::SelfCareKind::Nuzzle,strength:0.9,side:1.0,
                            squint:0.65,face_turn:0.6,body_lean:0.7,..Default::default()},
                        "CareDecline" => lifecore::SelfCareMotorFrame {
                            kind:lifecore::SelfCareKind::Decline,strength:0.9,side:-1.0,
                            squint:0.4,face_turn:-0.65,body_lean:-0.5,reluctance:0.85,..Default::default()},
                        _ => lifecore::SelfCareMotorFrame::default(),
                    };
                    body.set_self_care_motor(care);
                    body.set_feeding_expression_active(fixture=="CareFeedingPriority");
                    let intent = BodyIntent {
                        locomotion: LocomotionMode::Hover,
                        target_position: Vec2::splat(0.5),
                        target_surface: None,
                        desired_speed: 0.0,
                        facing_direction: 1.0,
                        gaze_target: Some(Vec2::new(0.55, 0.5)),
                        pose: PoseIntent::Neutral,
                        expression,
                        interaction_target: None,
                    };
                    for _ in 0..if mood_only {if fixture=="DelightOnset" {60} else {720}} else {180} {
                        body.embodied_update(
                            &intent,
                            &SensorFrame::default(),
                            AffectState::default(),
                            VisualMindInput::default(),
                            if fixture == "RigAngryShout" {
                                VoiceVisualState {
                                    active: true,
                                    envelope: 1.0,
                                    mouth_open: 0.98,
                                    shout: 1.0,
                                    ..Default::default()
                                }
                            } else {
                                VoiceVisualState::default()
                            },
                            1.0 / 120.0,
                        );
                    }
                    body.presentation_update(1.0 / 60.0);
                    renderer.set_review_background(background);
                    renderer.reset_perceptual_capture_state();
                    let parameters = body.render_parameters(&genome, 0.0);
                    let frame = renderer.render_capture(parameters)?;
                    let name =
                        format!("{fixture}-{background_name}-{}.ppm", (scale * 100.0) as u32);
                    let mut file = fs::File::create(self.output.join(&name))?;
                    write!(file, "P6\n{} {}\n255\n", frame.width, frame.height)?;
                    let rgb: Vec<u8> = frame
                        .rgba8
                        .chunks_exact(4)
                        .flat_map(|px| px[..3].iter().copied())
                        .collect();
                    file.write_all(&rgb)?;
                    records.push(serde_json::json!({"file": name, "fixture": fixture, "managed_blink": managed,
                        "desired": intent.expression, "smoothed": body.expression.current,
                        "renderer_geometry": parameters.geometry, "renderer_mouth_open": parameters.mouth_open,
                        "renderer_eye_scales": parameters.eye_scales,
                        "self_care": care, "tongue_extension":parameters.tongue_extension,
                        "body_tilt":parameters.tilt,"body_compression":parameters.compression,
                        "renderer_blink": [parameters.blink_left, parameters.blink_right]}));
                }
            }
        }
        if eyes_only || care_only || mood_only {
            fs::write(self.output.join("channels.json"),serde_json::to_vec_pretty(&records)?)?;
            return Ok(());
        }
        let _ = window.request_inner_size(PhysicalSize::new(512, 512));
        renderer.resize(PhysicalSize::new(512, 512));
        // Temporal acceptance, not just attractive expression endpoints.
        // Neutral speech -> held alarm -> recovery, fixed seed and fixed gaze.
        let mut timeline = Box::new(ProceduralBody::generate(&genome)?);
        timeline.apply_tuning_profile(profile.clone())?;
        timeline.embodiment.managed_blink = true;
        timeline.set_render_aspect(1.0);
        timeline.set_presentation_scale(0.8);
        renderer.set_review_background(ReviewBackground::White);
        renderer.reset_perceptual_capture_state();
        for frame_index in 0..120 {
            let mut expression = if (30..60).contains(&frame_index) {
                FacePose::Startled.expression()
            } else {
                FacePose::Awake.expression()
            };
            if frame_index < 30 {
                expression.mouth_open = (frame_index as f32 * 0.7).sin().abs() * 0.5;
            }
            let intent = BodyIntent {
                locomotion: LocomotionMode::Hover,
                target_position: Vec2::splat(0.5),
                target_surface: None,
                desired_speed: 0.0,
                facing_direction: 1.0,
                gaze_target: Some(Vec2::splat(0.5)),
                pose: PoseIntent::Neutral,
                expression,
                interaction_target: None,
            };
            for _ in 0..4 {
                timeline.embodied_update(
                    &intent,
                    &SensorFrame::default(),
                    AffectState::default(),
                    VisualMindInput::default(),
                    VoiceVisualState::default(),
                    1.0 / 120.0,
                );
            }
            timeline.presentation_update(1.0 / 30.0);
            let parameters = timeline.render_parameters(&genome, frame_index as f32 / 30.0);
            let frame = renderer.render_capture(parameters)?;
            records.push(serde_json::json!({
                "timeline_frame": frame_index,
                "time_seconds": frame_index as f32 / 30.0,
                "renderer_eye_scales": parameters.eye_scales,
                "renderer_geometry": parameters.geometry,
                "renderer_eye_aperture": parameters.eye_aperture,
                "renderer_blink": [parameters.blink_left, parameters.blink_right],
                "renderer_mouth_open": parameters.mouth_open,
            }));
            let mut file =
                fs::File::create(self.output.join(format!("Timeline-{frame_index:03}.ppm")))?;
            write!(file, "P6\n{} {}\n255\n", frame.width, frame.height)?;
            let rgb: Vec<_> = frame
                .rgba8
                .chunks_exact(4)
                .flat_map(|p| p[..3].iter().copied())
                .collect();
            file.write_all(&rgb)?;
        }
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
        // Formation oracle: a rejected desktop texture must have zero influence
        // on the procedural den, even if old pixels contrast maximally.
        let mut den_state = pet_ecology::EcologyState::new(42);
        den_state.objects.clear();
        den_state.den.anchor = Vec2::splat(0.5);
        let mut den_frames = Vec::new();
        for old_color in [[255_u8, 0, 0, 255], [0_u8, 0, 255, 255]] {
            let mut den_renderer = pet_body::EcologyRenderer::new(
                renderer.device(),
                renderer.surface_format(),
                renderer.premultiplied_output(),
            );
            den_renderer.set_den_tuning(profile.den);
            den_renderer.update_background(
                renderer.device(),
                renderer.queue(),
                1,
                1,
                4,
                &old_color,
            );
            den_renderer.set_background_freshness(0.0);
            den_renderer.prepare(renderer.queue(), &den_state, 1.0, 2.0);
            renderer.set_review_background(ReviewBackground::White);
            renderer.reset_perceptual_capture_state();
            let frame = renderer.render_capture_with_overlay(
                initial.render_parameters(&genome, 0.0),
                |_, _, encoder, view| den_renderer.render_prepared_den(encoder, view),
            )?;
            den_frames.push(frame.rgba8);
        }
        if den_frames[0] != den_frames[1] {
            return Err("procedural den retained influence from stale captured pixels".into());
        }
        records.push(
            serde_json::json!({"den_stale_texture_oracle": "passed", "compared_pixels": 512 * 512}),
        );
        fs::write(
            self.output.join("channels.json"),
            serde_json::to_vec_pretty(&records)?,
        )?;
        Ok(())
    }
}
