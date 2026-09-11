use glam::Vec2;
use lifecore::{CompanionIntentFrame, PrimaryIntent};

use crate::{BlinkController, BlinkOwner, BlinkRequest, GazeController, GazeMode, GazePlan};

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
    pub owner: ExpressionOwner,
}

pub struct CompanionExpressionDirector {
    gaze: GazeController,
    blink: BlinkController,
}

impl CompanionExpressionDirector {
    #[must_use]
    pub fn new(seed: u64) -> Self {
        Self {
            gaze: GazeController::default(),
            blink: BlinkController::new(seed),
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

        // Appraisal modulation is deliberately small: it enriches the semantic
        // intent rather than replacing it with a generic emotion mask.
        face.pupil_size = (face.pupil_size + intent.arousal * 0.12 + intent.surprise * 0.10)
            .clamp(0.0, 1.0);
        face.brow_raise = (face.brow_raise + intent.curiosity * 0.08 + intent.surprise * 0.12)
            .clamp(-1.0, 1.0);
        face.brow_tension = (face.brow_tension
            + intent.frustration * 0.10
            + evidence.motor_error.clamp(0.0, 1.0) * 0.08)
            .clamp(0.0, 1.0);
        body.internal_pulse = (body.internal_pulse + intent.arousal * 0.08).clamp(0.0, 1.0);
        body.glow = (body.glow + intent.attachment * 0.05 + intent.arousal * 0.04).clamp(0.0, 0.65);

        if matches!(intent.primary, PrimaryIntent::AcceptContact | PrimaryIntent::Nuzzle)
            && intent.contact_pleasantness > 0.20
        {
            owner = ExpressionOwner::Contact;
            let pleasant = intent.contact_pleasantness.clamp(0.0, 1.0);
            face.eye_aperture = (face.eye_aperture - pleasant * 0.12).clamp(0.35, 1.0);
            face.squint = (face.squint + pleasant * 0.08).clamp(0.0, 0.35);
            face.brow_tension *= 1.0 - pleasant * 0.60;
            body.contact_yield = (body.contact_yield + pleasant * 0.18).clamp(0.0, 1.0);
            body.viscosity_multiplier = (body.viscosity_multiplier - pleasant * 0.10).clamp(0.75, 1.35);
            if let Some(point) = evidence.contact_point.filter(Vec2::is_finite) {
                let axis = (point - intent.target.position.unwrap_or(Vec2::splat(0.5))).normalize_or_zero();
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

        // Actual audio is the only high-priority mouth aperture owner while phonating.
        if evidence.audio_active {
            face.mouth_open = evidence.audio_mouth_open.clamp(0.0, 1.0);
        }

        let gaze_mode = gaze_mode_for(intent.primary, intent.expected_outcome.uncertainty);
        let gaze_plan = GazePlan {
            primary_target: evidence.contact_point.or(intent.target.position),
            secondary_target: intent.target.position,
            target_velocity: evidence.target_velocity,
            mode: gaze_mode,
            acquire_tau: if matches!(intent.primary, PrimaryIntent::StartleFreeze) { 0.035 } else { 0.07 },
            release_tau: 0.18,
            dwell_min: 0.20,
            dwell_max: 1.10,
            lead_seconds: if gaze_mode == GazeMode::PredictiveIntercept { 0.12 } else { 0.0 },
            micro_saccade_amplitude: if intent.confidence > 0.6 { 0.004 } else { 0.0 },
            confidence: intent.confidence,
        };
        let gaze = self.gaze.tick(gaze_plan, dt);
        face.pupil_focus = gaze.pupil_focus;

        if matches!(intent.primary, PrimaryIntent::Sleep) {
            self.blink.request(BlinkRequest { owner: BlinkOwner::Sleep, strength: 1.0, duration: 2.5 });
        } else if matches!(intent.primary, PrimaryIntent::QuietCompanionship)
            && intent.attachment > 0.55
            && intent.arousal < 0.45
            && intent.confidence > 0.7
        {
            // The runtime should gate this with a social event/refractory condition;
            // this request is safe because BlinkController arbitrates/refracts it.
            self.blink.request(BlinkRequest { owner: BlinkOwner::Social, strength: 0.82, duration: 0.52 });
        }
        let blink = self.blink.tick(dt, intent.primary == PrimaryIntent::Sleep, danger, intent.fatigue);

        sanitize_face(&mut face);
        sanitize_body(&mut body);
        CompanionExpressionTarget {
            face,
            body,
            gaze: gaze.target,
            blink_left: blink.left,
            blink_right: blink.right,
            owner,
        }
    }
}

fn gaze_mode_for(intent: PrimaryIntent, uncertainty: f32) -> GazeMode {
    if uncertainty > 0.45 {
        return GazeMode::SocialReference;
    }
    match intent {
        PrimaryIntent::Sleep => GazeMode::Sleep,
        PrimaryIntent::Intercept | PrimaryIntent::Chase | PrimaryIntent::Catch => GazeMode::PredictiveIntercept,
        PrimaryIntent::AcceptContact | PrimaryIntent::Nuzzle | PrimaryIntent::InviteContact => GazeMode::ContactMonitor,
        PrimaryIntent::QuietCompanionship | PrimaryIntent::SocialCheckIn => GazeMode::MutualGaze,
        PrimaryIntent::Avoid | PrimaryIntent::GuardPain | PrimaryIntent::RejectContact => GazeMode::AvoidantCheck,
        PrimaryIntent::Inspect | PrimaryIntent::Explore | PrimaryIntent::SearchObject => GazeMode::Inspect,
        _ => GazeMode::Track,
    }
}

fn face_prototype(intent: PrimaryIntent) -> FaceTarget {
    let mut face = FaceTarget {
        eye_aperture: 0.90,
        squint: 0.04,
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
            face.brow_asymmetry = 0.08;
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
            face.brow_asymmetry = 0.08;
            face.mouth_curve = 0.12;
        }
        PrimaryIntent::RecoverFromMiss => {
            face.brow_raise = 0.24;
            face.brow_asymmetry = 0.12;
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
    body.lean = if body.lean.is_finite() { body.lean.clamp(Vec2::splat(-1.0), Vec2::ONE) } else { Vec2::ZERO };
    body.buoyancy = unit(body.buoyancy);
    body.viscosity_multiplier = finite(body.viscosity_multiplier, 1.0).clamp(0.75, 1.35);
    body.surface_tension_multiplier = finite(body.surface_tension_multiplier, 1.0).clamp(0.75, 1.35);
    body.contact_yield = unit(body.contact_yield);
    body.internal_pulse = unit(body.internal_pulse);
    body.glow = unit(body.glow);
}

fn unit(value: f32) -> f32 { finite(value, 0.0).clamp(0.0, 1.0) }
fn signed(value: f32) -> f32 { finite(value, 0.0).clamp(-1.0, 1.0) }
fn finite(value: f32, fallback: f32) -> f32 { if value.is_finite() { value } else { fallback } }
