use glam::Vec2;
use lifecore::{CompanionIntentFrame, FaceGeometry, PrimaryIntent};

use crate::gaze_controller::GazeMode as CompanionGazeMode;
use crate::{
    BlinkContext, BlinkController, BlinkOutput, BlinkOwner, BlinkReason, BlinkRequest,
    GazeController, GazePlan,
};

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct FaceTarget {
    /// Contextual deviations from neutral geometry. They compose with the
    /// phenotype rig; expression, blink and speech keep independent ownership.
    pub geometry: FaceGeometry,
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

impl FaceTarget {
    pub fn apply_geometry(self, geometry: &mut FaceGeometry) {
        let neutral = FaceGeometry::default();
        for ((out, value), base) in geometry
            .lids
            .iter_mut()
            .flatten()
            .chain(geometry.brows.iter_mut().flatten())
            .chain(geometry.mouth.iter_mut())
            .zip(
                self.geometry
                    .lids
                    .iter()
                    .flatten()
                    .chain(self.geometry.brows.iter().flatten())
                    .chain(self.geometry.mouth.iter()),
            )
            .zip(
                neutral
                    .lids
                    .iter()
                    .flatten()
                    .chain(neutral.brows.iter().flatten())
                    .chain(neutral.mouth.iter()),
            )
        {
            *out += value - base;
        }
        *geometry = geometry.sanitized();
    }
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
    /// Short-lived performance memory, never a pose timer or a random variant.
    appraisal_adaptation: f32,
    expected_success: f32,
    contact_warmth: f32,
    direction: f32,
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
            appraisal_adaptation: 0.0,
            expected_success: 0.0,
            contact_warmth: 0.0,
            direction: 0.0,
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
        let safe_dt = finite(dt, 0.0).clamp(0.0, 0.25);
        let danger = evidence.protective_reflex
            || evidence.pain_like > 0.22
            || evidence.contact_strain > 0.65
            || intent.protective();
        let sleeping = intent.primary == PrimaryIntent::Sleep;
        let safe_contact = !danger
            && !sleeping
            && evidence.contact_pressure > 0.02
            && matches!(
                intent.primary,
                PrimaryIntent::AcceptContact | PrimaryIntent::Nuzzle
            );
        let active_contact = evidence.contact_point.filter(|point| {
            point.is_finite() && evidence.contact_pressure > 0.02 && (safe_contact || danger)
        });
        let direction = active_contact
            .or(intent.target.position)
            .filter(|_| evidence.body_position.is_finite())
            .map(|point| {
                let delta = point - evidence.body_position;
                delta.x / (delta.length() + 0.035)
            })
            .unwrap_or(self.expressive_side * 0.30);
        self.direction = follow(self.direction, direction, safe_dt, 0.28);
        let appraisal = if sleeping || danger {
            0.0
        } else {
            intent.curiosity * 0.55 + intent.surprise * 0.30 + intent.anticipation * 0.15
        };
        self.appraisal_adaptation = follow(self.appraisal_adaptation, appraisal, safe_dt, 2.8);
        let orienting = (appraisal - self.appraisal_adaptation).max(0.0);
        let expected_before = self.expected_success;
        self.expected_success = follow(
            self.expected_success,
            intent.expected_outcome.success * intent.anticipation,
            safe_dt,
            1.6,
        );
        let warmth_target = if safe_contact {
            intent.contact_pleasantness.max(0.0)
        } else {
            0.0
        };
        self.contact_warmth = follow(
            self.contact_warmth,
            warmth_target,
            safe_dt,
            if warmth_target > self.contact_warmth {
                0.65
            } else {
                2.8
            },
        );
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
                let uncertainty =
                    (1.0 - intent.confidence).max(intent.expected_outcome.uncertainty);
                face.eye_aperture =
                    0.96 + 0.04 * intent.confidence - 0.25 * intent.fatigue - 0.035 * uncertainty;
                face.mouth_open = 0.025 + intent.curiosity * (0.04 + orienting * 0.15);
                face.mouth_curve = 0.02 + intent.confidence * 0.10 - uncertainty * 0.07;
                face.mouth_compression = uncertainty * 0.20;
                face.brow_raise =
                    0.14 + intent.curiosity * 0.25 + uncertainty * 0.12 + orienting * 0.18;
                face.brow_tension += uncertainty * 0.06;
                face.geometry.mouth[0] = 0.90 + intent.confidence * 0.15;
                for (i, lid) in face.geometry.lids.iter_mut().enumerate() {
                    let side = if i == 0 { -1.0 } else { 1.0 };
                    lid[0] = -uncertainty * 0.10 * (1.0 + side * self.direction);
                    lid[1] = -intent.fatigue * 0.14;
                    lid[3] = intent.curiosity * 0.12;
                }
            }
            PrimaryIntent::InvitePlay | PrimaryIntent::Chase | PrimaryIntent::Intercept => {
                let delight = (intent.confidence * 0.55 + intent.play_readiness * 0.45)
                    * (1.0 - intent.fatigue * 0.48);
                face.mouth_curve = 0.28 + delight * 0.28;
                face.mouth_open = 0.08 + delight * 0.16;
                face.squint = 0.08 + delight * 0.10;
                face.eye_aperture = 0.94 - delight * 0.06 - intent.fatigue * 0.19;
                face.geometry.mouth[0] = 0.92 + delight * 0.25;
                face.geometry.lids = [[0.0, 0.0, delight * 0.16, 0.08]; 2];
            }
            PrimaryIntent::Celebrate => {
                let delight = (intent.valence.max(0.0) * 0.32
                    + intent.arousal * 0.26
                    + intent.confidence * 0.22
                    + intent.play_readiness * 0.20)
                    * (1.0 - intent.fatigue * 0.40);
                face.mouth_curve = 0.30 + delight * 0.50;
                face.mouth_open = 0.035 + delight * delight * 0.34;
                face.squint = 0.045 + delight * 0.23;
                face.brow_raise = 0.07 + delight * 0.24 + orienting * 0.10;
                face.eye_aperture = 0.96 - delight * 0.15 - intent.fatigue * 0.18;
                face.geometry.mouth[0] = 0.90 + delight * 0.32;
                face.geometry.lids = [[0.0, 0.0, delight * 0.20, 0.12]; 2];
            }
            PrimaryIntent::RecoverFromMiss => {
                // A lost expectation first reads as a question, then softens.
                // Frustration can tense the lips; it is not defensive anger.
                face.mouth_curve = -0.06 - intent.frustration * 0.25 - expected_before * 0.05;
                face.mouth_compression = 0.08 + intent.frustration * 0.30 + expected_before * 0.12;
                face.mouth_tension = 0.10 + intent.frustration * 0.20;
                face.eye_aperture = 0.88 - intent.frustration * 0.12;
                face.brow_raise = 0.12 + expected_before * 0.35 + intent.surprise * 0.15;
                face.brow_tension = 0.02 + intent.frustration * 0.08;
                face.geometry.brows = [[0.16 + expected_before * 0.20, 0.0, 0.30, 1.0]; 2];
                face.geometry.mouth[0] = 0.94 - intent.frustration * 0.13;
            }
            _ => {}
        }

        if !danger && !sleeping {
            // The received touch leaves a brief affiliative after-effect. The
            // cached point alone is not contact and cannot manufacture warmth.
            face.mouth_curve += self.contact_warmth * 0.18;
            face.brow_tension *= 1.0 - self.contact_warmth * 0.60;
            face.squint += self.contact_warmth * 0.10;
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
            if safe_contact {
                face.geometry.mouth[0] = 0.95 + self.contact_warmth * 0.14;
                for (i, lid) in face.geometry.lids.iter_mut().enumerate() {
                    let side = if i == 0 { -1.0 } else { 1.0 };
                    lid[2] = pleasant * (0.10 + 0.08 * (1.0 + side * self.direction));
                    lid[0] = -pleasant * 0.04 * (1.0 + side * self.direction);
                }
            }
        }

        // Physical integrity owns the final semantic face. It may never be
        // overwritten by positive valence or attachment.
        if danger {
            face.geometry = FaceGeometry::default();
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
            if matches!(
                intent.primary,
                PrimaryIntent::StartleFreeze | PrimaryIntent::EscapePressure
            ) {
                // Alertness raises the brows; it must not become an angry knit
                // merely because protective ownership won arbitration.
                face.brow_raise = face.brow_raise.max(0.60 + intent.surprise * 0.24);
                face.brow_tension = face.brow_tension.min(0.20);
                face.eye_aperture = 1.0;
                face.mouth_open = face.mouth_open.max(0.28 + intent.surprise * 0.24);
            } else {
                face.brow_raise = face.brow_raise.min(0.08);
            }
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
        let brow_tau = if danger { 0.08 } else { 0.32 };
        let mouth_tau = if danger { 0.08 } else { 0.46 };
        self.brow_asymmetry += (brow_target * self.direction - self.brow_asymmetry)
            * (1.0 - (-safe_dt / brow_tau).exp());
        self.mouth_asymmetry += (mouth_target * self.direction - self.mouth_asymmetry)
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
    // Urgent interception, contact and defense own attention even when the
    // outcome is uncertain. Social referencing is only for recipient-directed bids.
    if uncertainty > 0.45
        && matches!(
            intent,
            PrimaryIntent::OfferObject | PrimaryIntent::InvitePlay
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
        geometry: FaceGeometry::default(),
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
            face.mouth_curve = 0.70;
            face.squint = 0.12;
            face.mouth_open = 0.22;
        }
        PrimaryIntent::StartleFreeze => {
            face.eye_aperture = 1.0;
            face.pupil_size = 0.75;
            face.brow_raise = 0.74;
            face.brow_tension = 0.12;
            face.mouth_open = 0.48;
        }
        PrimaryIntent::GuardPain | PrimaryIntent::RejectContact | PrimaryIntent::EscapePressure => {
            face.eye_aperture = 0.80;
            face.squint = 0.22;
            face.brow_tension = if intent == PrimaryIntent::RejectContact {
                0.64
            } else {
                0.32
            };
            face.mouth_curve = -0.30;
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
    face.geometry = face.geometry.sanitized();
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

fn follow(current: f32, target: f32, dt: f32, tau: f32) -> f32 {
    current + (target - current) * (1.0 - (-dt / tau).exp())
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
    fn protective_alert_and_boundary_remain_distinct_and_joy_recruits_a_readable_smile() {
        let mut director = CompanionExpressionDirector::new(42);
        let mut alert = intent(PrimaryIntent::StartleFreeze);
        alert.surprise = 0.9;
        let alarm = director.tick(
            alert,
            ExpressionEvidence {
                protective_reflex: true,
                ..Default::default()
            },
            0.05,
        );
        let boundary = director.tick(
            intent(PrimaryIntent::RejectContact),
            ExpressionEvidence {
                protective_reflex: true,
                ..Default::default()
            },
            0.05,
        );
        let joy = director.tick(
            CompanionIntentFrame {
                valence: 0.9,
                arousal: 0.8,
                play_readiness: 0.9,
                ..intent(PrimaryIntent::Celebrate)
            },
            Default::default(),
            0.05,
        );
        assert!(alarm.face.brow_raise > 0.65 && alarm.face.mouth_open > 0.4);
        assert!(alarm.face.brow_tension < 0.25);
        assert!(boundary.face.brow_tension > 0.6 && boundary.face.mouth_curve < 0.0);
        assert!(joy.face.mouth_curve > 0.6 && joy.face.squint > 0.1);
    }

    #[test]
    fn uncertainty_does_not_steal_interception_or_protective_attention() {
        for intent in [
            PrimaryIntent::Chase,
            PrimaryIntent::Intercept,
            PrimaryIntent::Catch,
        ] {
            assert_eq!(
                gaze_mode_for(intent, 1.0),
                CompanionGazeMode::PredictiveIntercept
            );
        }
        assert_eq!(
            gaze_mode_for(PrimaryIntent::Sleep, 1.0),
            CompanionGazeMode::Sleep
        );
        assert_eq!(
            gaze_mode_for(PrimaryIntent::RejectContact, 1.0),
            CompanionGazeMode::AvoidantCheck
        );
        assert_eq!(
            gaze_mode_for(PrimaryIntent::Explore, 1.0),
            CompanionGazeMode::Inspect
        );
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
                // With no spatial evidence, identity is a quiet bias rather
                // than a permanently raised eyebrow on the same side.
                assert!((0.10..0.13).contains(&face.face.brow_asymmetry.abs()));
                assert!((0.02..0.03).contains(&face.face.mouth_asymmetry.abs()));
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
        assert!(previous.face.mouth_asymmetry.abs() > 0.07);
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
        assert!(previous.face.brow_asymmetry.abs() > 0.12);
        assert!(previous.face.mouth_asymmetry.abs() > 0.04);
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

    fn held(input: CompanionIntentFrame, evidence: ExpressionEvidence, seconds: f32) -> FaceTarget {
        let mut director = CompanionExpressionDirector::new(42);
        let mut face = FaceTarget::default();
        for _ in 0..(seconds * 60.0) as usize {
            face = director.tick(input, evidence, 1.0 / 60.0).face;
        }
        face
    }

    #[test]
    fn same_intent_has_contextual_repertoire_instead_of_seeded_pose_variants() {
        let fresh = intent(PrimaryIntent::Inspect);
        let alert = held(fresh, Default::default(), 2.0);
        let tired = held(
            CompanionIntentFrame {
                fatigue: 0.85,
                ..fresh
            },
            Default::default(),
            2.0,
        );
        let cautious = held(
            CompanionIntentFrame {
                confidence: 0.1,
                ..fresh
            },
            Default::default(),
            2.0,
        );
        assert!(alert.eye_aperture - tired.eye_aperture > 0.20);
        assert!(cautious.mouth_compression - alert.mouth_compression > 0.15);
        assert!(cautious.brow_raise - alert.brow_raise > 0.09);
        assert!(cautious.geometry.maximum_error(alert.geometry) > 0.10);
        let small = held(
            CompanionIntentFrame {
                primary: PrimaryIntent::Celebrate,
                confidence: 0.35,
                valence: 0.3,
                arousal: 0.15,
                ..Default::default()
            },
            Default::default(),
            2.0,
        );
        let big = held(
            CompanionIntentFrame {
                primary: PrimaryIntent::Celebrate,
                confidence: 0.95,
                valence: 0.95,
                arousal: 0.9,
                play_readiness: 0.9,
                ..Default::default()
            },
            Default::default(),
            2.0,
        );
        assert!(big.mouth_curve - small.mouth_curve > 0.30);
        assert!(big.mouth_open - small.mouth_open > 0.20);
        assert!(big.geometry.maximum_error(small.geometry) > 0.15);
    }

    #[test]
    fn contact_side_changes_local_lids_and_safe_touch_leaves_a_short_aftereffect() {
        let input = CompanionIntentFrame {
            contact_pleasantness: 0.9,
            ..intent(PrimaryIntent::AcceptContact)
        };
        let evidence = |side| ExpressionEvidence {
            body_position: Vec2::splat(0.5),
            contact_point: Some(Vec2::new(side, 0.5)),
            contact_pressure: 0.12,
            ..Default::default()
        };
        let left = held(input, evidence(0.2), 2.0);
        let right = held(input, evidence(0.8), 2.0);
        assert!(left.geometry.lids[0][2] - right.geometry.lids[0][2] > 0.12);
        assert!((left.geometry.lids[0][2] - right.geometry.lids[1][2]).abs() < 1e-5);
        let mut director = CompanionExpressionDirector::new(42);
        for _ in 0..120 {
            let _ = director.tick(input, evidence(0.2), 1.0 / 60.0);
        }
        let quiet = intent(PrimaryIntent::QuietCompanionship);
        let after = director.tick(quiet, Default::default(), 1.0 / 60.0).face;
        let untouched = held(quiet, Default::default(), 1.0);
        assert!(after.mouth_curve - untouched.mouth_curve > 0.14);
        for _ in 0..1200 {
            let _ = director.tick(quiet, evidence(0.2), 1.0 / 60.0);
        }
        assert!(
            director.contact_warmth < 0.001,
            "stale/non-social contact cannot renew warmth"
        );
    }

    #[test]
    fn expectation_history_changes_miss_without_turning_it_into_anger() {
        let mut expected = CompanionExpressionDirector::new(42);
        let mut waiting = intent(PrimaryIntent::Intercept);
        waiting.anticipation = 0.95;
        waiting.expected_outcome.success = 0.95;
        for _ in 0..180 {
            let _ = expected.tick(waiting, Default::default(), 1.0 / 60.0);
        }
        let miss = CompanionIntentFrame {
            frustration: 0.45,
            ..intent(PrimaryIntent::RecoverFromMiss)
        };
        let surprised = expected.tick(miss, Default::default(), 1.0 / 60.0).face;
        let unanticipated = held(miss, Default::default(), 1.0);
        assert!(surprised.brow_raise - unanticipated.brow_raise > 0.20);
        assert!(surprised.brow_tension < 0.10);
        assert!(surprised.mouth_curve < 0.0);
        let boundary = held(
            intent(PrimaryIntent::RejectContact),
            Default::default(),
            1.0,
        );
        assert!(boundary.brow_tension - surprised.brow_tension > 0.50);
    }

    #[test]
    fn held_curiosity_adapts_without_repeating_or_restarting_from_episode_metadata() {
        let mut director = CompanionExpressionDirector::new(42);
        let mut input = intent(PrimaryIntent::Inspect);
        let onset = director.tick(input, Default::default(), 0.05).face;
        let mut settled = onset;
        for tick in 0..1200 {
            input.episode_id = tick;
            settled = director.tick(input, Default::default(), 1.0 / 60.0).face;
        }
        assert!(onset.brow_raise - settled.brow_raise > 0.065);
        let a = settled;
        for _ in 0..1200 {
            settled = director.tick(input, Default::default(), 1.0 / 60.0).face;
        }
        assert!((a.brow_raise - settled.brow_raise).abs() < 0.001);
        assert!((a.mouth_open - settled.mouth_open).abs() < 0.001);
    }

    #[test]
    fn contextual_layers_preserve_sleep_integrity_and_audio_ownership() {
        let mut director = CompanionExpressionDirector::new(42);
        let input = CompanionIntentFrame {
            contact_pleasantness: 0.95,
            ..intent(PrimaryIntent::Nuzzle)
        };
        for _ in 0..120 {
            let _ = director.tick(
                input,
                ExpressionEvidence {
                    contact_pressure: 0.2,
                    ..Default::default()
                },
                1.0 / 60.0,
            );
        }
        let sleepy = director.tick(intent(PrimaryIntent::Sleep), Default::default(), 0.05);
        assert_eq!(sleepy.face.eye_aperture, 0.0);
        assert_eq!(sleepy.face.geometry, FaceGeometry::default());
        let pain = director.tick(
            input,
            ExpressionEvidence {
                pain_like: 0.8,
                audio_active: true,
                audio_mouth_open: 0.73,
                ..Default::default()
            },
            0.05,
        );
        assert_eq!(pain.owner, ExpressionOwner::Integrity);
        assert!(pain.face.mouth_curve < 0.0);
        assert_eq!(pain.face.geometry, FaceGeometry::default());
        assert_eq!(pain.face.mouth_open, 0.73);
    }

    #[test]
    fn contextual_memory_and_geometry_are_cadence_stable_and_bounded() {
        let run = |hz: u32| {
            let mut director = CompanionExpressionDirector::new(42);
            let mut result = FaceTarget::default();
            for frame in 0..hz * 6 {
                let mut input = if frame < hz * 2 {
                    intent(PrimaryIntent::Inspect)
                } else if frame < hz * 4 {
                    intent(PrimaryIntent::Nuzzle)
                } else {
                    intent(PrimaryIntent::QuietCompanionship)
                };
                input.contact_pleasantness = 0.9;
                input.target.position = Some(Vec2::new(0.8, 0.5));
                let evidence = ExpressionEvidence {
                    body_position: Vec2::splat(0.5),
                    contact_pressure: 0.15,
                    contact_point: Some(Vec2::new(0.2, 0.5)),
                    ..Default::default()
                };
                result = director.tick(input, evidence, 1.0 / hz as f32).face;
                assert_eq!(result.geometry, result.geometry.sanitized());
            }
            result
        };
        let reference = run(120);
        for hz in [20, 30, 60] {
            let face = run(hz);
            assert!((face.mouth_curve - reference.mouth_curve).abs() < 0.001);
            assert!(face.geometry.maximum_error(reference.geometry) < 0.001);
        }
        let mut geometry = FaceGeometry::default();
        held(intent(PrimaryIntent::Inspect), Default::default(), 2.0).apply_geometry(&mut geometry);
        assert_eq!(geometry, geometry.sanitized());
    }

    #[test]
    fn shader_clock_cannot_drive_an_independent_face_motor() {
        let genome = lifecore::Genome::from_seed(42);
        let mut body = Box::new(crate::ProceduralBody::generate(&genome).unwrap());
        body.embodiment.pose.eye_aperture = 0.83;
        body.embodiment.pose.mouth_open = 0.38;
        body.embodiment.pose.mouth_curve = 0.45;
        body.embodiment.pose.pupil_size = 0.62;
        body.embodiment.pose.gaze = Vec2::new(0.11, -0.08);
        let baseline = body.render_parameters(&genome, 0.8);
        for seconds in [0.13, 0.72, 1.2, 4.6, 8.8, 60.0] {
            body.animation.time = seconds;
            let later = body.render_parameters(&genome, 0.8);
            assert_eq!(later.gaze, baseline.gaze);
            assert_eq!(later.pupil_size, baseline.pupil_size);
            assert_eq!(later.mouth_open, baseline.mouth_open);
            assert_eq!(later.mouth_curve, baseline.mouth_curve);
            assert_eq!(later.mouth_tension, baseline.mouth_tension);
            assert_eq!(later.geometry, baseline.geometry);
        }
    }
}
