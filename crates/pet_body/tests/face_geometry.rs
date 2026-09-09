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
    let genome = Genome::from_seed(42);
    let mut body = Box::new(ProceduralBody::generate(&genome).unwrap());
    let intent = BodyIntent {
        locomotion: LocomotionMode::Hover,
        target_position: Vec2::splat(0.5),
        target_surface: None,
        desired_speed: 0.0,
        facing_direction: 1.0,
        gaze_target: Some(Vec2::new(0.6, 0.5)),
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
fn held_physiological_blink_never_reopens_between_blink_envelopes() {
    let mut expression = FacePose::Awake.expression();
    expression.blink_left = 1.0;
    expression.blink_right = 1.0;
    let output = rendered_expression(expression, 60);
    assert!(output.blink_left > 0.995 && output.blink_right > 0.995);
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
