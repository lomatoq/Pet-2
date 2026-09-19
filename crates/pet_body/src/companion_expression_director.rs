use glam::Vec2;
use lifecore::{CompanionIntentFrame, PrimaryIntent};

use crate::gaze_controller::GazeMode as CompanionGazeMode;
use crate::{
    BlinkContext, BlinkController, BlinkOutput, BlinkOwner, BlinkReason, BlinkRequest,
    GazeController, GazePlan,
};

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct FaceTarget {
    pub eye_aperture: f32,
    pub squint: f32,
    pub pupil_size: f32,
    pub pupil_focus: f32,
    pub brow_raise: f32,
    pub brow_tension: f32,
    pub brow_asymmetry: f32,
    pub mouth_curve: f32,
    pub mouth_open: f32,
    pub mouth_tension: f32,
    pub mouth_compression: f32,
    pub mouth_asymmetry: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BodyStyleTarget {
    pub compactness: f32,
    pub lean: Vec2,
    pub buoyancy: f32,
    pub viscosity_multiplier: f32,
    pub surface_tension_multiplier: f32,
    pub contact_yield: f32,
    pub internal_pulse: f32,
    pub glow: f32,
}

impl Default for BodyStyleTarget {
    fn default() -> Self {
        Self {
            compactness: 0.5,
            lean: Vec2::ZERO,
            buoyancy: 0.5,
            viscosity_multiplier: 1.0,
            surface_tension_multiplier: 1.0,
            contact_yield: 0.5,
            internal_pulse: 0.05,
            glow: 0.25,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ExpressionOwner {
    #[default]
    Physiology,
    Affect,
    Contact,
    MotorIntent,
    Integrity,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ExpressionEvidence {
    pub body_position: Vec2,
    pub contact_point: Option<Vec2>,
    pub contact_pressure: f32,
    pub contact_strain: f32,
    pub body_speed: f32,
    pub motor_error: f32,
    pub audio_mouth_open: f32,
    pub audio_active: bool,
    pub target_velocity: Vec2,
    pub protective_reflex: bool,
    pub pain_like: f32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct CompanionExpressionTarget {
    pub face: FaceTarget,
    pub body: BodyStyleTarget,
    pub gaze: Option<Vec2>,
    pub blink_left: f32,
    pub blink_right: f32,
    pub blink_owner: BlinkOwner,
    pub blink_reason: BlinkReason,
    pub owner: ExpressionOwner,
}

#[derive(Debug, Clone)]
pub struct CompanionExpressionDirector {
    gaze: GazeController,
    blink: BlinkController,
    expressive_side: f32,
    brow_asymmetry: f32,
    mouth_asymmetry: f32,
}

impl CompanionExpressionDirector {
    /// Submit a causal accent to the single blink owner. Existing priority and
    /// social refractory rules decide whether it is accepted.
    pub fn request_blink(&mut self, request: BlinkRequest) {
        self.blink.request(request);
    }

    /// Advances only the eyelid motor envelope. The host calls this at body
    /// cadence after the slower causal `tick` has selected any new event.
    #[must_use]
    pub fn present_blink(&mut self, dt: f32) -> BlinkOutput {
        self.blink.present(dt)
    }

    #[must_use]
    pub fn new(seed: u64) -> Self {
        Self {
            gaze: GazeController::default(),
            blink: BlinkController::new(seed),
            // A stable individual preference, not a newly sampled side per bout.
            expressive_side: if seed.count_ones().is_multiple_of(2) {
                1.0
            } else {
                -1.0
            },
            brow_asymmetry: 0.0,
            mouth_asymmetry: 0.0,
        }
    }

    #[must_use]
    pub fn tick(
        &mut self,
        intent: CompanionIntentFrame,
        evidence: ExpressionEvidence,
        dt: f32,
    ) -> CompanionExpressionTarget {
        let intent = intent.sanitized();
        let mut face = face_prototype(intent.primary);
        let mut body = body_prototype(intent.primary);
        let mut owner = ExpressionOwner::MotorIntent;

        // Quiet supported rest is not sleep. Lid droop must follow measured
        // fatigue rather than the Rest label alone.
        if intent.primary == PrimaryIntent::Rest {
            let tired = ((intent.fatigue - 0.35) / 0.50).clamp(0.0, 1.0);
            face.eye_aperture = 0.96 - tired * 0.34;
        }

        // Appraisal modulation is deliberately small: it enriches the semantic
        // intent rather than replacing it with a generic emotion mask.
        face.pupil_size =
            (face.pupil_size + intent.arousal * 0.12 + intent.surprise * 0.10).clamp(0.0, 1.0);
        face.brow_raise =
            (face.brow_raise + intent.curiosity * 0.08 + intent.surprise * 0.12).clamp(-1.0, 1.0);
        face.brow_tension = (face.brow_tension
            + intent.frustration * 0.10
            + evidence.motor_error.clamp(0.0, 1.0) * 0.08)
            .clamp(0.0, 1.0);
        body.internal_pulse = (body.internal_pulse + intent.arousal * 0.08).clamp(0.0, 1.0);
        body.glow = (body.glow + intent.attachment * 0.05 + intent.arousal * 0.04).clamp(0.0, 0.65);

        // Coherent action families feed the real runtime, not just Lab fixtures.
        // These are sustained targets; ExpressionRuntime owns their transitions.
        match intent.primary {
            PrimaryIntent::Inspect | PrimaryIntent::Explore | PrimaryIntent::SearchObject => {
                face.mouth_open = 0.04 + intent.curiosity * 0.10;
                face.mouth_compression = (1.0 - intent.confidence) * 0.12;
                face.brow_raise += intent.curiosity * 0.12;
            }
            PrimaryIntent::InvitePlay | PrimaryIntent::Chase | PrimaryIntent::Intercept => {
                let delight = intent.confidence * 0.55 + intent.play_readiness * 0.45;
                face.mouth_curve = 0.28 + delight * 0.28;
                face.mouth_open = 0.08 + delight * 0.16;
                face.squint = 0.08 + delight * 0.10;
                face.eye_aperture = 0.94 - delight * 0.06;
            }
            PrimaryIntent::RecoverFromMiss => {
                face.mouth_curve = -0.12 - intent.frustration * 0.22;
                face.mouth_compression = 0.12 + intent.frustration * 0.25;
                face.mouth_tension = 0.10 + intent.frustration * 0.20;
                face.eye_aperture = 0.88 - intent.frustration * 0.12;
            }
            _ => {}
        }

        if matches!(
            intent.primary,
            PrimaryIntent::AcceptContact | PrimaryIntent::Nuzzle
        ) && intent.contact_pleasantness > 0.20
        {
            owner = ExpressionOwner::Contact;
            let pleasant = intent.contact_pleasantness.clamp(0.0, 1.0);
            face.eye_aperture = (face.eye_aperture - pleasant * 0.12).clamp(0.35, 1.0);
            face.squint = (face.squint + pleasant * 0.08).clamp(0.0, 0.35);
            face.brow_tension *= 1.0 - pleasant * 0.60;
            body.contact_yield = (body.contact_yield + pleasant * 0.18).clamp(0.0, 1.0);
            body.viscosity_multiplier =
                (body.viscosity_multiplier - pleasant * 0.10).clamp(0.75, 1.35);
            if let Some(point) = evidence.contact_point.filter(|point| point.is_finite()) {
                let axis = (point - evidence.body_position).normalize_or_zero();
                body.lean += axis * 0.05 * pleasant;
            }
        }

        // Physical integrity owns the final semantic face. It may never be
        // overwritten by positive valence or attachment.
        let danger = evidence.protective_reflex
            || evidence.pain_like > 0.22
            || evidence.contact_strain > 0.65
            || intent.protective();
        if danger {
            owner = ExpressionOwner::Integrity;
            let pain = evidence
                .pain_like
                .max(evidence.contact_strain)
                .max(intent.discomfort)
                .clamp(0.0, 1.0);
            face.mouth_curve = face.mouth_curve.min(-0.02 - pain * 0.10);
            face.mouth_tension = face.mouth_tension.max(0.20 + pain * 0.42);
            face.mouth_compression = face.mouth_compression.max(0.12 + pain * 0.28);
            face.brow_tension = face.brow_tension.max(0.28 + pain * 0.40);
            face.brow_raise = face.brow_raise.min(0.08);
            body.compactness = body.compactness.max(0.68 + pain * 0.20);
            body.contact_yield = body.contact_yield.min(0.18);
            body.viscosity_multiplier = body.viscosity_multiplier.max(1.12);
            body.surface_tension_multiplier = body.surface_tension_multiplier.max(1.10);
        }

        // Independent expressive channels follow a cause, then retain motor
        // continuity across intent/bout changes. No oscillator or per-frame RNG.
        let (brow_target, mouth_target) = if danger || intent.primary == PrimaryIntent::Sleep {
            (0.0, 0.0)
        } else {
            match intent.primary {
                PrimaryIntent::Inspect | PrimaryIntent::Explore | PrimaryIntent::SearchObject => {
                    (0.28 + intent.curiosity * 0.14, -0.08)
                }
                PrimaryIntent::InvitePlay | PrimaryIntent::Chase | PrimaryIntent::Intercept => {
                    (0.16, 0.24)
                }
                PrimaryIntent::Celebrate => (0.12, 0.20),
                PrimaryIntent::RecoverFromMiss => (0.42, -0.14),
                PrimaryIntent::AcceptContact | PrimaryIntent::Nuzzle => {
                    (0.06, 0.08 * intent.contact_pleasantness)
                }
                _ => (0.0, 0.0),
            }
        };
        let safe_dt = finite(dt, 0.0).clamp(0.0, 0.25);
        let brow_tau = if danger { 0.08 } else { 0.32 };
        let mouth_tau = if danger { 0.08 } else { 0.46 };
        self.brow_asymmetry += (brow_target * self.expressive_side - self.brow_asymmetry)
            * (1.0 - (-safe_dt / brow_tau).exp());
        self.mouth_asymmetry += (mouth_target * self.expressive_side - self.mouth_asymmetry)
            * (1.0 - (-safe_dt / mouth_tau).exp());
        face.brow_asymmetry = self.brow_asymmetry;
        face.mouth_asymmetry = self.mouth_asymmetry;

        // Actual audio is the only high-priority mouth aperture owner while phonating.
        if evidence.audio_active {
            face.mouth_open = evidence.audio_mouth_open.clamp(0.0, 1.0);
        }

        let gaze_mode = gaze_mode_for(intent.primary, intent.expected_outcome.uncertainty);
        // A cached contact location must not steal attention after contact ends,
        // or during unrelated travel/play. Tracking that stale point fought the
        // current semantic target whenever contact classification flickered.
        let contact_target = evidence.contact_point.filter(|point| {
            point.is_finite()
                && evidence.contact_pressure > 0.02
                && (danger
                    || matches!(
                        intent.primary,
                        PrimaryIntent::AcceptContact
                            | PrimaryIntent::Nuzzle
                            | PrimaryIntent::InviteContact
                    ))
        });
        let gaze_plan = GazePlan {
            primary_target: contact_target
                .or(intent.target.position)
                .or(Some(evidence.body_position)),
            secondary_target: intent.target.position,
            target_velocity: evidence.target_velocity,
            mode: gaze_mode,
            acquire_tau: if matches!(intent.primary, PrimaryIntent::StartleFreeze) {
                0.035
            } else {
                0.07
            },
            release_tau: 0.18,
            dwell_min: 0.20,
            dwell_max: 1.10,
            lead_seconds: if gaze_mode == CompanionGazeMode::PredictiveIntercept {
                0.12
            } else {
                0.0
            },
            micro_saccade_amplitude: if intent.confidence > 0.6 { 0.004 } else { 0.0 },
            confidence: intent.confidence,
        };
        let gaze = self.gaze.tick(gaze_plan, dt);
        face.pupil_focus = gaze.pupil_focus;

        self.blink.update_context(
            dt,
            BlinkContext {
                sleeping: intent.primary == PrimaryIntent::Sleep,
                protective: danger,
                fatigue: intent.fatigue,
                fixation_transition: gaze.fixation_started,
                awaiting_important_outcome: intent.expected_outcome.user_response > 0.55
                    && intent.anticipation > 0.35,
            },
        );
        let blink = self.blink.sample();

        sanitize_face(&mut face);
        sanitize_body(&mut body);
        CompanionExpressionTarget {
            face,
            body,
            gaze: gaze.target,
            blink_left: blink.left,
            blink_right: blink.right,
            blink_owner: blink.owner,
            blink_reason: blink.reason,
            owner,
        }
    }
}

fn gaze_mode_for(intent: PrimaryIntent, uncertainty: f32) -> CompanionGazeMode {
    if uncertainty > 0.45
        && !matches!(
            intent,
            PrimaryIntent::Sleep
                | PrimaryIntent::Avoid
                | PrimaryIntent::GuardPain
                | PrimaryIntent::RejectContact
        )
    {
        return CompanionGazeMode::SocialReference;
    }
    match intent {
        PrimaryIntent::Sleep => CompanionGazeMode::Sleep,
        PrimaryIntent::Intercept | PrimaryIntent::Chase | PrimaryIntent::Catch => {
            CompanionGazeMode::PredictiveIntercept
        }
        PrimaryIntent::AcceptContact | PrimaryIntent::Nuzzle | PrimaryIntent::InviteContact => {
            CompanionGazeMode::ContactMonitor
        }
        PrimaryIntent::QuietCompanionship | PrimaryIntent::SocialCheckIn => {
            CompanionGazeMode::MutualGaze
        }
        PrimaryIntent::Avoid | PrimaryIntent::GuardPain | PrimaryIntent::RejectContact => {
            CompanionGazeMode::AvoidantCheck
        }
        PrimaryIntent::Inspect | PrimaryIntent::Explore | PrimaryIntent::SearchObject => {
            CompanionGazeMode::Inspect
        }
        _ => CompanionGazeMode::Track,
    }
}

fn face_prototype(intent: PrimaryIntent) -> FaceTarget {
    let mut face = FaceTarget {
        eye_aperture: 0.98,
        squint: 0.0,
        pupil_size: 0.50,
        pupil_focus: 0.55,
        brow_raise: 0.02,
        brow_tension: 0.02,
        brow_asymmetry: 0.0,
        mouth_curve: 0.03,
        mouth_open: 0.0,
        mouth_tension: 0.02,
        mouth_compression: 0.0,
        mouth_asymmetry: 0.0,
    };
    match intent {
        PrimaryIntent::Inspect | PrimaryIntent::Explore | PrimaryIntent::SearchObject => {
            face.eye_aperture = 1.0;
            face.pupil_size = 0.60;
            face.brow_raise = 0.22;
        }
        PrimaryIntent::InviteContact | PrimaryIntent::AcceptContact | PrimaryIntent::Nuzzle => {
            face.eye_aperture = 0.80;
            face.squint = 0.11;
            face.brow_tension = 0.0;
            face.mouth_curve = 0.10;
        }
        PrimaryIntent::InvitePlay | PrimaryIntent::Chase | PrimaryIntent::Intercept => {
            face.eye_aperture = 1.0;
            face.pupil_size = 0.67;
            face.brow_raise = 0.20;
            face.mouth_curve = 0.12;
        }
        PrimaryIntent::RecoverFromMiss => {
            face.brow_raise = 0.24;
            face.mouth_curve = -0.02;
            face.mouth_compression = 0.08;
        }
        PrimaryIntent::Celebrate => {
            face.eye_aperture = 0.96;
            face.pupil_size = 0.62;
            face.brow_raise = 0.18;
            face.mouth_curve = 0.20;
        }
        PrimaryIntent::StartleFreeze => {
            face.eye_aperture = 1.0;
            face.pupil_size = 0.75;
            face.brow_raise = 0.34;
            face.brow_tension = 0.12;
            face.mouth_open = 0.10;
        }
        PrimaryIntent::GuardPain | PrimaryIntent::RejectContact | PrimaryIntent::EscapePressure => {
            face.eye_aperture = 0.80;
            face.squint = 0.15;
            face.brow_tension = 0.42;
            face.mouth_curve = -0.08;
            face.mouth_tension = 0.34;
            face.mouth_compression = 0.25;
        }
        PrimaryIntent::Sleep => {
            face.eye_aperture = 0.0;
            face.squint = 0.0;
            face.brow_raise = 0.0;
            face.brow_tension = 0.0;
            face.mouth_curve = 0.0;
        }
        PrimaryIntent::Rest => {
            face.eye_aperture = 0.62;
            face.pupil_size = 0.45;
        }
        _ => {}
    }
    face
}

fn body_prototype(intent: PrimaryIntent) -> BodyStyleTarget {
    let mut body = BodyStyleTarget::default();
    match intent {
        PrimaryIntent::InviteContact | PrimaryIntent::AcceptContact | PrimaryIntent::Nuzzle => {
            body.compactness = 0.48;
            body.viscosity_multiplier = 0.90;
            body.surface_tension_multiplier = 0.91;
            body.contact_yield = 0.84;
            body.glow = 0.38;
        }
        PrimaryIntent::InvitePlay | PrimaryIntent::Chase | PrimaryIntent::Intercept => {
            body.compactness = 0.34;
            body.buoyancy = 0.70;
            body.viscosity_multiplier = 0.86;
            body.surface_tension_multiplier = 0.91;
            body.internal_pulse = 0.26;
            body.glow = 0.44;
        }
        PrimaryIntent::Inspect | PrimaryIntent::Explore => {
            body.compactness = 0.44;
            body.buoyancy = 0.58;
            body.contact_yield = 0.50;
            body.internal_pulse = 0.14;
        }
        PrimaryIntent::GuardPain | PrimaryIntent::RejectContact | PrimaryIntent::EscapePressure => {
            body.compactness = 0.78;
            body.viscosity_multiplier = 1.20;
            body.surface_tension_multiplier = 1.18;
            body.contact_yield = 0.12;
            body.glow = 0.22;
        }
        PrimaryIntent::Rest => {
            body.compactness = 0.68;
            body.buoyancy = 0.34;
            body.viscosity_multiplier = 1.16;
            body.glow = 0.20;
        }
        PrimaryIntent::Sleep => {
            body.compactness = 0.82;
            body.buoyancy = 0.18;
            body.viscosity_multiplier = 1.30;
            body.surface_tension_multiplier = 1.12;
            body.internal_pulse = 0.015;
            body.glow = 0.12;
        }
        _ => {}
    }
    body
}

fn sanitize_face(face: &mut FaceTarget) {
    face.eye_aperture = unit(face.eye_aperture);
    face.squint = unit(face.squint);
    face.pupil_size = unit(face.pupil_size);
    face.pupil_focus = unit(face.pupil_focus);
    face.brow_raise = signed(face.brow_raise);
    face.brow_tension = unit(face.brow_tension);
    face.brow_asymmetry = signed(face.brow_asymmetry);
    face.mouth_curve = signed(face.mouth_curve);
    face.mouth_open = unit(face.mouth_open);
    face.mouth_tension = unit(face.mouth_tension);
    face.mouth_compression = unit(face.mouth_compression);
    face.mouth_asymmetry = signed(face.mouth_asymmetry);
}

fn sanitize_body(body: &mut BodyStyleTarget) {
    body.compactness = unit(body.compactness);
    body.lean = if body.lean.is_finite() {
        body.lean.clamp(Vec2::splat(-1.0), Vec2::ONE)
    } else {
        Vec2::ZERO
    };
    body.buoyancy = unit(body.buoyancy);
    body.viscosity_multiplier = finite(body.viscosity_multiplier, 1.0).clamp(0.75, 1.35);
    body.surface_tension_multiplier =
        finite(body.surface_tension_multiplier, 1.0).clamp(0.75, 1.35);
    body.contact_yield = unit(body.contact_yield);
    body.internal_pulse = unit(body.internal_pulse);
    body.glow = unit(body.glow);
}

fn unit(value: f32) -> f32 {
    finite(value, 0.0).clamp(0.0, 1.0)
}
fn signed(value: f32) -> f32 {
    finite(value, 0.0).clamp(-1.0, 1.0)
}
fn finite(value: f32, fallback: f32) -> f32 {
    if value.is_finite() { value } else { fallback }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn intent(primary: PrimaryIntent) -> CompanionIntentFrame {
        CompanionIntentFrame {
            primary,
            confidence: 0.9,
            curiosity: 0.8,
            ..Default::default()
        }
    }

    #[test]
    fn quiet_rest_keeps_awake_eyes_and_fatigue_still_droops_them() {
        let mut director = CompanionExpressionDirector::new(42);
        let mut resting = intent(PrimaryIntent::Rest);
        resting.fatigue = 0.1;
        let awake = director.tick(resting, ExpressionEvidence::default(), 0.05);
        resting.fatigue = 0.9;
        let tired = director.tick(resting, ExpressionEvidence::default(), 0.05);
        assert!(awake.face.eye_aperture >= 0.95);
        assert_eq!(awake.face.squint, 0.0);
        assert!(awake.face.eye_aperture - tired.face.eye_aperture > 0.30);
        let sleeping = director.tick(
            intent(PrimaryIntent::Sleep),
            ExpressionEvidence::default(),
            0.05,
        );
        assert_eq!(sleeping.face.eye_aperture, 0.0);
    }

    #[test]
    fn actual_intent_families_drive_distinct_coherent_faces() {
        let mut director = CompanionExpressionDirector::new(42);
        let mut inspect = intent(PrimaryIntent::Inspect);
        inspect.curiosity = 0.8;
        inspect.confidence = 0.3;
        let questioning = director.tick(inspect, Default::default(), 0.05).face;
        let mut play = intent(PrimaryIntent::InvitePlay);
        play.confidence = 0.9;
        play.play_readiness = 0.9;
        let delighted = director.tick(play, Default::default(), 0.05).face;
        let mut miss = intent(PrimaryIntent::RecoverFromMiss);
        miss.frustration = 0.8;
        let frustrated = director.tick(miss, Default::default(), 0.05).face;
        assert!(delighted.mouth_curve > 0.5 && delighted.squint > 0.15);
        assert!(delighted.mouth_open > questioning.mouth_open + 0.08);
        assert!(frustrated.mouth_curve < -0.25 && frustrated.mouth_compression > 0.3);
        assert!(questioning.eye_aperture > frustrated.eye_aperture + 0.15);
        assert!(questioning.brow_raise > delighted.brow_raise);
    }

    #[test]
    fn contextual_asymmetry_is_readable_seed_stable_and_not_a_blink_wink() {
        let mut a = CompanionExpressionDirector::new(42);
        let mut replay = CompanionExpressionDirector::new(42);
        let mut opposite = CompanionExpressionDirector::new(43);
        for frame in 0..900 {
            let mut input = intent(PrimaryIntent::Inspect);
            input.episode_id = frame; // Bout metadata cannot flip the face.
            let face = a.tick(input, Default::default(), 1.0 / 60.0);
            assert_eq!(face, replay.tick(input, Default::default(), 1.0 / 60.0));
            let other = opposite.tick(input, Default::default(), 1.0 / 60.0);
            assert!((face.face.brow_asymmetry + other.face.brow_asymmetry).abs() < 1.0e-6);
            assert_eq!(face.blink_left > 0.001, face.blink_right > 0.0009);
            if frame > 180 {
                assert!(face.face.brow_asymmetry.abs() > 0.35);
                assert!(face.face.mouth_asymmetry.abs() > 0.07);
            }
        }
    }

    #[test]
    fn playful_mouth_and_questioning_brow_transition_without_a_sign_snap() {
        let mut director = CompanionExpressionDirector::new(42);
        let mut previous = CompanionExpressionTarget::default();
        for _ in 0..180 {
            previous = director.tick(
                intent(PrimaryIntent::InvitePlay),
                Default::default(),
                1.0 / 60.0,
            );
        }
        assert!(previous.face.mouth_asymmetry.abs() > 0.23);
        assert!(previous.face.mouth_curve > 0.0);
        for _ in 0..180 {
            let current = director.tick(
                intent(PrimaryIntent::RecoverFromMiss),
                Default::default(),
                1.0 / 60.0,
            );
            assert!((current.face.mouth_asymmetry - previous.face.mouth_asymmetry).abs() < 0.02);
            assert!((current.face.brow_asymmetry - previous.face.brow_asymmetry).abs() < 0.02);
            previous = current;
        }
        assert!(previous.face.brow_asymmetry.abs() > 0.41);
        assert!(previous.face.mouth_asymmetry.abs() > 0.13);
        assert!(previous.face.mouth_curve <= 0.0);
        for _ in 0..300 {
            previous = director.tick(intent(PrimaryIntent::Rest), Default::default(), 1.0 / 60.0);
        }
        assert!(previous.face.brow_asymmetry.abs() < 0.001);
        assert!(previous.face.mouth_asymmetry.abs() < 0.001);
    }

    #[test]
    fn danger_owns_face_and_removes_playful_residuals() {
        let mut director = CompanionExpressionDirector::new(42);
        for _ in 0..180 {
            let _ = director.tick(
                intent(PrimaryIntent::InvitePlay),
                Default::default(),
                1.0 / 60.0,
            );
        }
        for _ in 0..30 {
            let face = director.tick(
                intent(PrimaryIntent::InvitePlay),
                ExpressionEvidence {
                    pain_like: 0.8,
                    ..Default::default()
                },
                1.0 / 60.0,
            );
            assert_eq!(face.owner, ExpressionOwner::Integrity);
            assert!(face.face.mouth_curve < 0.0);
        }
        assert!(director.mouth_asymmetry.abs() < 0.001);
        assert!(director.brow_asymmetry.abs() < 0.001);
    }

    #[test]
    fn quiet_companionship_label_does_not_generate_social_blinks() {
        for hz in [30, 60, 120] {
            let mut director = CompanionExpressionDirector::new(42);
            let mut quiet = intent(PrimaryIntent::QuietCompanionship);
            quiet.attachment = 0.95;
            quiet.arousal = 0.1;
            let mut social_frames = 0;
            for body_tick in 0..hz * 90 {
                if body_tick % (hz / 20).max(1) == 0 {
                    let _ = director.tick(quiet, ExpressionEvidence::default(), 0.05);
                }
                let blink = director.present_blink(1.0 / hz as f32);
                social_frames += usize::from(blink.owner == BlinkOwner::Social);
            }
            assert_eq!(social_frames, 0, "hz={hz}");
        }
    }

    #[test]
    fn explicit_social_response_is_one_complete_blink() {
        let mut director = CompanionExpressionDirector::new(42);
        director.request_blink(BlinkRequest {
            owner: BlinkOwner::Social,
            strength: 0.82,
            duration: 0.52,
        });
        let mut starts = 0;
        let mut active = false;
        for _ in 0..120 {
            let blink = director.present_blink(1.0 / 120.0);
            let next = blink.owner == BlinkOwner::Social && blink.left > 0.05;
            starts += usize::from(next && !active);
            active = next;
        }
        assert_eq!(starts, 1);
    }
}
