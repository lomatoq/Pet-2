use glam::Vec2;
use lifecore::{
    AffectState, BodyIntent, FacePose, Genome, LocomotionMode, PoseIntent, SensorFrame,
};
use pet_body::{ProceduralBody, VisualMindInput, VoiceVisualState};

fn rendered(pose: FacePose, fps: u32) -> pet_body::RenderParameters {
    rendered_expression(pose.expression(), fps)
}

fn rendered_expression(
    expression: lifecore::ExpressionState,
    fps: u32,
) -> pet_body::RenderParameters {
    rendered_expression_looking(expression, fps, Vec2::new(0.6, 0.5))
}

fn rendered_expression_looking(
    expression: lifecore::ExpressionState,
    fps: u32,
    target: Vec2,
) -> pet_body::RenderParameters {
    let genome = Genome::from_seed(42);
    let mut body = Box::new(ProceduralBody::generate(&genome).unwrap());
    let intent = BodyIntent {
        locomotion: LocomotionMode::Hover,
        target_position: Vec2::splat(0.5),
        target_surface: None,
        desired_speed: 0.0,
        facing_direction: 1.0,
        gaze_target: Some(target),
        pose: PoseIntent::Neutral,
        expression,
        interaction_target: None,
    };
    for frame in 0..fps * 2 {
        for _ in 0..120 / fps {
            body.embodied_update(
                &intent,
                &SensorFrame::default(),
                AffectState::default(),
                VisualMindInput::default(),
                VoiceVisualState::default(),
                1.0 / 120.0,
            );
        }
        body.presentation_update(1.0 / fps as f32);
        assert!(body.embodiment.pose.mouth_open.is_finite(), "frame {frame}");
    }
    body.render_parameters(&genome, 0.0)
}

#[test]
fn owned_sleep_check_reaches_visible_final_lid_gap_without_waking_body() {
    let genome = Genome::from_seed(42);
    let mut body = Box::new(ProceduralBody::generate(&genome).unwrap());
    body.embodiment.managed_blink = true;
    let mut expression = FacePose::Tired.expression();
    expression.eye_aperture = 1.0;
    expression.blink_left = 0.28;
    expression.blink_right = 1.0;
    let intent = BodyIntent {
        locomotion: LocomotionMode::Hover,
        target_position: Vec2::splat(0.5),
        target_surface: None,
        desired_speed: 0.0,
        facing_direction: 1.0,
        gaze_target: None,
        pose: PoseIntent::Sleeping,
        expression,
        interaction_target: None,
    };
    for _ in 0..60 {
        body.embodied_update(
            &intent,
            &SensorFrame::default(),
            AffectState::default(),
            VisualMindInput::default(),
            VoiceVisualState::default(),
            1.0 / 60.0,
        );
        body.presentation_update(1.0 / 60.0);
    }
    let rendered = body.render_parameters(&genome, 0.0);
    let gap = |side: usize, blink: f32| {
        let lid = rendered.geometry.lids[side];
        // Same center-gap reconstruction as liquid_surface.wgsl; scalar
        // aperture and blink both contribute and cannot be tested separately.
        1.44 + (lid[0] + lid[1]) * 0.45
            - lid[3] * 0.08
            - lid[2] * 0.95
            - blink * 2.37
            - body.embodiment.pose.squint * 0.36
            - (1.0 - body.embodiment.pose.eye_aperture) * 1.7
    };
    assert!(gap(0, body.embodiment.pose.blink_left) > 0.2);
    assert!(gap(1, body.embodiment.pose.blink_right) < 0.0);
    assert_eq!(body.embodiment.pose.gaze, Vec2::ZERO);
}

#[test]
fn expressive_lids_do_not_magnify_eye_disks_at_any_presentation_rate() {
    for hz in [30, 60, 120] {
        let neutral = rendered(FacePose::Awake, hz);
        let startle = rendered(FacePose::Startled, hz);
        assert_eq!(startle.eye_scales, [Vec2::ONE; 2]);
        assert_eq!(startle.eye_size, neutral.eye_size);
        let upper_center = |r: &pet_body::RenderParameters| {
            let lid = r.geometry.lids[0];
            0.58 + (lid[0] + lid[1]) * 0.45 - lid[3] * 0.08
        };
        assert!(
            upper_center(&neutral) < 0.62,
            "neutral must reserve actual lid headroom"
        );
        assert!(
            upper_center(&startle) > upper_center(&neutral) + 0.25,
            "startle uncovers the fixed eye instead of scaling it"
        );
        assert!(startle.geometry.lids[0][0] > neutral.geometry.lids[0][0] + 0.25);
        let mut question = FacePose::Awake.expression();
        question.brow_asymmetry = 0.65;
        let one_side = rendered_expression(question, hz);
        assert_eq!(one_side.eye_scales, [Vec2::ONE; 2]);
        assert!(one_side.geometry.lids[0][0] - one_side.geometry.lids[1][0] > 0.15);
        let mut strain = FacePose::Awake.expression();
        strain.effort = 0.9;
        let strained = rendered_expression(strain, hz);
        assert_eq!(strained.eye_scales, neutral.eye_scales);
        assert_eq!(strained.gaze, neutral.gaze);
    }
}

#[test]
fn angry_shout_requires_actual_audio_and_releases_continuously() {
    let genome = Genome::from_seed(42);
    let mut body = ProceduralBody::generate(&genome).unwrap();
    let mut life = lifecore::LifeCore::new(genome.clone(), 42);
    let mut intent = life
        .tick(&SensorFrame::default(), &Default::default(), 0.05)
        .body_intent;
    intent.expression = FacePose::Boundary.expression();
    let mut previous_open = 0.0_f32;
    for phase in 0..3 {
        let voice = VoiceVisualState {
            active: phase == 1,
            shout: 1.0, // Stale feedback must not shout when playback is inactive.
            mouth_open: 0.98,
            envelope: 1.0,
            ..Default::default()
        };
        for _ in 0..120 {
            body.embodied_update(
                &intent,
                &SensorFrame::default(),
                AffectState::default(),
                VisualMindInput::default(),
                voice,
                1.0 / 120.0,
            );
            body.presentation_update(1.0 / 120.0);
            let output = body.render_parameters(&genome, 0.0);
            assert!((output.mouth_open - previous_open).abs() < 0.18);
            previous_open = output.mouth_open;
        }
        let output = body.render_parameters(&genome, 0.0);
        if phase == 1 {
            assert!(output.mouth_shout > 0.98);
            assert!(output.mouth_open > 0.95);
            assert!(output.geometry.mouth[0] > 1.58);
            assert!(output.geometry.mouth[3] < 0.02);
        } else {
            assert!(output.mouth_shout < 0.002);
            assert!(output.mouth_open < 0.02);
            assert!(output.geometry.mouth[3] > 0.7);
        }
    }
}

#[test]
fn rendered_lids_follow_vertical_fixation_and_fade_independently_during_blinks() {
    for hz in [30, 60, 120] {
        let expression = FacePose::Awake.expression();
        let up = rendered_expression_looking(expression, hz, Vec2::new(0.5, 0.25));
        let down = rendered_expression_looking(expression, hz, Vec2::new(0.5, 0.75));
        assert!(up.geometry.lids[0][0] > down.geometry.lids[0][0] + 0.20);
        assert!(up.geometry.lids[0][2] > down.geometry.lids[0][2] + 0.025);
        let mut blink = expression;
        blink.blink_left = 1.0;
        let closed = rendered_expression_looking(blink, hz, Vec2::new(0.5, 0.25));
        assert!(closed.geometry.lids[0][0].abs() < 0.001);
        assert!(closed.geometry.lids[1][0] > 0.10);
        assert!(closed.blink_left > 0.995);
        assert_eq!(closed.geometry.mouth, up.geometry.mouth);
    }
}

#[test]
fn facial_rig_correctives_separate_question_concern_brace_and_open_jaw() {
    let mut skeptical = FacePose::Awake.expression();
    skeptical.brow_asymmetry = 0.6;
    skeptical.mouth_asymmetry = -0.3;
    skeptical.mouth_compression = 0.35;
    let question = rendered_expression(skeptical, 60);
    let inner_difference = question.geometry.brows[0][0] - question.geometry.brows[1][0];
    let outer_difference = question.geometry.brows[0][1] - question.geometry.brows[1][1];
    assert!(inner_difference > 0.8);
    assert!(
        outer_difference.abs() < inner_difference * 0.25,
        "question must bend sections independently, not translate a rigid brow"
    );

    let mut concern = FacePose::Awake.expression();
    concern.mouth_curve = -0.65;
    concern.brow_tension = 0.05;
    let worried = rendered_expression(concern, 60);
    let mut effort = concern;
    effort.brow_tension = 0.9;
    effort.mouth_tension = 0.85;
    effort.mouth_compression = 0.8;
    effort.effort = 0.9;
    let braced = rendered_expression(effort, 60);
    assert!(worried.geometry.brows[0][0] > braced.geometry.brows[0][0] + 0.5);
    assert!(worried.geometry.brows[0][1] < braced.geometry.brows[0][1] - 0.15);
    assert!(braced.geometry.mouth[3] > worried.geometry.mouth[3] + 0.7);

    let mut playful = FacePose::Playful.expression();
    playful.mouth_asymmetry = 0.4;
    let crooked = rendered_expression(playful, 60);
    let surprise = rendered(FacePose::Startled, 60);
    assert!(
        crooked.geometry.lids[0][2] > surprise.geometry.lids[0][2] + 0.30,
        "playful mouth must recruit lower-lid/cheek support, not stare blankly"
    );
    let mut yawn = FacePose::Awake.expression();
    yawn.mouth_open = 0.95;
    yawn.geometry.mouth[0] = 0.85;
    let jaw = rendered_expression(yawn, 60);
    assert!(crooked.geometry.mouth[0] > surprise.geometry.mouth[0] * 2.0);
    assert!((crooked.geometry.mouth[1] - crooked.geometry.mouth[2]).abs() > 0.5);
    assert!(jaw.mouth_open > surprise.mouth_open + 0.2);
    assert!(jaw.geometry.mouth[0] > surprise.geometry.mouth[0] + 0.15);
}

#[test]
fn causal_mouth_articulation_reaches_final_render_geometry() {
    for fps in [30, 60, 120] {
        let mut open = FacePose::Awake.expression();
        open.mouth_open = 0.8;
        let rounded = rendered_expression(open, fps);
        let mut joyful = open;
        joyful.mouth_curve = 0.8;
        joyful.mouth_asymmetry = 0.35;
        let smile = rendered_expression(joyful, fps);
        assert!(smile.geometry.mouth[0] > rounded.geometry.mouth[0] * 1.25);
        assert!(smile.geometry.mouth[1] - smile.geometry.mouth[2] > 0.5);
        // Unequal corner participation, not a rigid symmetric tilt.
        assert!((smile.geometry.mouth[1] + smile.geometry.mouth[2]).abs() > 0.04);
        joyful.mouth_asymmetry = -0.35;
        let mirrored = rendered_expression(joyful, fps);
        assert!((smile.geometry.mouth[1] - mirrored.geometry.mouth[2]).abs() < 0.002);
        assert!((smile.geometry.mouth[2] - mirrored.geometry.mouth[1]).abs() < 0.002);
        open.mouth_compression = 0.8;
        let pressed = rendered_expression(open, fps);
        assert!(pressed.geometry.mouth[3] > 0.79);
        assert!(pressed.geometry.mouth[0] < rounded.geometry.mouth[0]);
        assert_eq!(pressed.geometry.lids, rounded.geometry.lids);
    }
}

#[test]
fn held_physiological_blink_never_reopens_between_blink_envelopes() {
    let mut expression = FacePose::Awake.expression();
    expression.blink_left = 1.0;
    expression.blink_right = 1.0;
    let output = rendered_expression(expression, 60);
    assert!(output.blink_left > 0.995 && output.blink_right > 0.995);
}

#[test]
fn managed_open_eyes_do_not_receive_legacy_blinks() {
    let genome = Genome::from_seed(42);
    let mut body = ProceduralBody::generate(&genome).unwrap();
    body.embodiment.managed_blink = true;
    let mut life = lifecore::LifeCore::new(genome, 42);
    let mut intent = life
        .tick(&SensorFrame::default(), &body.simulation.feedback, 0.05)
        .body_intent;
    intent.expression = FacePose::Awake.expression();
    for _ in 0..720 {
        body.embodied_update(
            &intent,
            &SensorFrame::default(),
            AffectState::default(),
            VisualMindInput::default(),
            VoiceVisualState::default(),
            1.0 / 60.0,
        );
        assert!(body.embodiment.pose.blink_left < 0.01);
        assert!(body.embodiment.pose.blink_right < 0.01);
    }
}

#[test]
fn silent_expression_opening_survives_the_entire_cpu_pipeline() {
    let confused = rendered(FacePose::Confused, 60);
    let startled = rendered(FacePose::Startled, 60);
    let neutral = rendered(FacePose::Awake, 60);
    assert!(confused.mouth_open > 0.28);
    assert!(startled.mouth_open > 0.60);
    assert!(neutral.mouth_open < 0.01);
    assert!(confused.geometry.mouth[0] < neutral.geometry.mouth[0] * 0.6);
}

#[test]
fn affection_raises_lower_lids_while_tiredness_drops_upper_lids() {
    let affection = rendered(FacePose::Affectionate, 60);
    let tired = rendered(FacePose::Tired, 60);
    assert!(affection.geometry.lids[0][2] > 0.5);
    assert_eq!(tired.geometry.lids[0][2], 0.0);
    assert!(tired.eye_aperture < affection.eye_aperture - 0.3);
    assert!(tired.mouth_curve < 0.0 && affection.mouth_curve > 0.5);
}

#[test]
fn fixed_step_geometry_is_independent_of_presentation_rate() {
    for pose in FacePose::ALL {
        let reference = rendered(pose, 120);
        for fps in [30, 60] {
            let value = rendered(pose, fps);
            assert_eq!(value.geometry, reference.geometry);
            assert!((value.mouth_open - reference.mouth_open).abs() < 1.0e-6);
        }
    }
}
