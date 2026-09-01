use serde::{Deserialize, Serialize};

use crate::{
    ActionId, AffectState, BodyFeedback, Drives, InteractionGazeTarget, InteractionReasonCode,
    InteractionResponsePlan, LifeState, SensorFrame, VocalRequest, VocalTrigger,
};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SocialIntent {
    #[default]
    Notice,
    Contact,
    Acknowledge,
    Invite,
    Query,
    Effort,
    Boundary,
    Alarm,
    Relief,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VoiceGesture {
    #[default]
    WarmChuff,
    PurrHum,
    MewWhine,
    LowRumble,
    ClippedPulse,
    ReliefExhale,
}

impl VoiceGesture {
    #[must_use]
    pub const fn for_trigger(trigger: VocalTrigger) -> Self {
        match trigger {
            VocalTrigger::Action(ActionId::Purr) | VocalTrigger::HomeReturn => Self::PurrHum,
            VocalTrigger::CalmBoundary | VocalTrigger::FoodRefused => Self::LowRumble,
            VocalTrigger::PhysicalStartle | VocalTrigger::ComponentDetached => Self::ClippedPulse,
            VocalTrigger::NeedHelp | VocalTrigger::Action(ActionId::Chirp) => Self::MewWhine,
            VocalTrigger::ComponentRemerged
            | VocalTrigger::FragmentHelped
            | VocalTrigger::FoodAccepted => Self::ReliefExhale,
            VocalTrigger::Action(_)
            | VocalTrigger::ToyOffer
            | VocalTrigger::CatchSuccess
            | VocalTrigger::MissAndRetry
            | VocalTrigger::FoodInspect
            | VocalTrigger::SkillMastered
            | VocalTrigger::RhythmEcho
            | VocalTrigger::VisualNotice
            | VocalTrigger::SoftTouch
            | VocalTrigger::PlayfulRelease => Self::WarmChuff,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct LivingStateFrame {
    pub affect: AffectState,
    pub drives: Drives,
    pub current_action: ActionId,
    pub sleeping: bool,
    pub focus_mode: bool,
    pub quiet_preferred: bool,
}

impl LivingStateFrame {
    #[must_use]
    pub fn from_life(state: &LifeState) -> Self {
        let sleeping = state.current_action == ActionId::Sleep;
        Self {
            affect: state.affect,
            drives: state.drives,
            current_action: state.current_action,
            sleeping,
            focus_mode: state.focus_mode,
            quiet_preferred: sleeping || state.focus_mode || state.drives.sleep > 0.82,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct PhysicalExpressionContext {
    pub contact: bool,
    pub pressure: f32,
    pub deformation: f32,
    pub strain: f32,
    pub slosh_energy: f32,
    pub topology_budget_used: f32,
    pub detached_fraction: f32,
    pub speed: f32,
    pub acceleration: f32,
    pub airborne: bool,
}

impl PhysicalExpressionContext {
    #[must_use]
    pub fn from_frames(sensors: &SensorFrame, body: &BodyFeedback) -> Self {
        let physical = sensors.embodied_interaction;
        Self {
            contact: physical.contact.active,
            pressure: physical.contact.effective_pressure.clamp(0.0, 1.0),
            deformation: physical.material.deformation_energy.clamp(0.0, 1.0),
            strain: physical.material.maximum_strain.clamp(0.0, 1.0),
            slosh_energy: physical.material.slosh_energy.clamp(0.0, 1.0),
            topology_budget_used: (1.0 - physical.material.topology_budget_remaining)
                .clamp(0.0, 1.0),
            detached_fraction: physical.material.detached_mass_fraction.clamp(0.0, 1.0),
            speed: body.velocity.length().clamp(0.0, 1.0),
            acceleration: (body.acceleration.length() * 0.25).clamp(0.0, 1.0),
            airborne: !body.grounded && !body.clinging,
        }
    }

    #[must_use]
    pub fn load(self) -> f32 {
        (self.pressure * 0.30
            + self.deformation * 0.22
            + self.strain * 0.24
            + self.topology_budget_used * 0.16
            + self.detached_fraction * 0.40)
            .clamp(0.0, 1.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CreaturePhrase {
    pub phrase_id: u64,
    pub episode_id: u64,
    pub intent: SocialIntent,
    pub voice_gesture: Option<VoiceGesture>,
    pub plan: InteractionResponsePlan,
    pub physical: PhysicalExpressionContext,
    pub priority: u8,
    pub quiet_suppressed: bool,
}

#[derive(Debug, Clone, Copy)]
struct ActivePhrase {
    phrase: CreaturePhrase,
    remaining_seconds: f32,
}

/// Transient coordinator. Persistent affect, repertoire and learning remain in
/// `LifeCore`; this object owns only short phrase locks and cross-channel shape.
#[derive(Debug, Default)]
pub struct ExpressionDirector {
    active: Option<ActivePhrase>,
}

impl ExpressionDirector {
    pub fn tick(&mut self, dt: f32) {
        let Some(active) = &mut self.active else {
            return;
        };
        active.remaining_seconds = (active.remaining_seconds - dt.max(0.0)).max(0.0);
        if active.remaining_seconds <= f32::EPSILON {
            self.active = None;
        }
    }

    #[must_use]
    pub fn direct(
        &mut self,
        mut plan: InteractionResponsePlan,
        living: LivingStateFrame,
        physical: PhysicalExpressionContext,
    ) -> CreaturePhrase {
        let intent = social_intent(plan.reason);
        let priority = intent_priority(intent);
        if let Some(active) = self.active
            && active.remaining_seconds > 0.0
            && active.phrase.episode_id == plan.episode_id
        {
            return active.phrase;
        }

        let load = physical.load();
        let (
            eye_aperture,
            eye_scale,
            brow_asymmetry,
            mouth_curve,
            mouth_compression,
            effort,
            relief,
        ) = expression_shape(intent);
        plan.expression.eye_aperture = eye_aperture;
        plan.expression.eye_scale = eye_scale;
        plan.expression.brow_asymmetry = brow_asymmetry;
        plan.expression.mouth_curve = mouth_curve;
        plan.expression.mouth_compression = (mouth_compression + load * 0.24).clamp(0.0, 1.0);
        plan.expression.mouth_asymmetry = if physical.airborne {
            (physical.speed - 0.35).clamp(0.0, 0.35)
        } else {
            0.0
        };
        plan.expression.effort = effort.max(load);
        plan.expression.relief = relief * (1.0 - load);
        plan.expression.amplitude =
            (0.72 + living.affect.arousal * 0.22 + load * 0.28).clamp(0.55, 1.18);
        plan.body.local_pulse = plan
            .body
            .local_pulse
            .max((physical.slosh_energy * 0.012).clamp(0.0, 0.018));
        plan.body.recoil = plan.body.recoil.max((load * 0.055).clamp(0.0, 0.07));
        if intent == SocialIntent::Boundary {
            plan.body.resistance = plan.body.resistance.max(0.62);
            plan.body.cooperation = 0.0;
            plan.gaze = InteractionGazeTarget::Away;
        } else if intent == SocialIntent::Alarm {
            plan.gaze = InteractionGazeTarget::Cursor;
        } else if intent == SocialIntent::Relief {
            plan.gaze = InteractionGazeTarget::Viewer;
        }
        plan.onset_seconds = plan.onset_seconds.clamp(0.04, 0.18);
        plan.hold_seconds = (0.80 + living.affect.attachment * 0.45 + load * 0.35).clamp(0.80, 2.0);
        plan.release_seconds = plan.release_seconds.clamp(0.18, 0.60);
        let quiet_suppressed = living.quiet_preferred && priority < 220;
        let voice_gesture = plan.voice_trigger.map(VoiceGesture::for_trigger);
        if quiet_suppressed {
            plan.voice_trigger = None;
        }
        plan.sanitize();
        let phrase = CreaturePhrase {
            phrase_id: plan.response_id,
            episode_id: plan.episode_id,
            intent,
            voice_gesture: (!quiet_suppressed).then_some(voice_gesture).flatten(),
            plan,
            physical,
            priority,
            quiet_suppressed,
        };
        self.active = Some(ActivePhrase {
            phrase,
            remaining_seconds: phrase.plan.onset_seconds
                + phrase.plan.hold_seconds
                + phrase.plan.release_seconds,
        });
        phrase
    }
}

#[derive(Debug, Default)]
pub struct VocalArbiter {
    next_allowed_seconds: f64,
    last_request_id: Option<u64>,
}

impl VocalArbiter {
    #[must_use]
    pub fn admit(
        &mut self,
        request: VocalRequest,
        now_seconds: f64,
        quiet_preferred: bool,
    ) -> Option<VocalRequest> {
        if (quiet_preferred && request.priority < 220)
            || now_seconds < self.next_allowed_seconds
            || self.last_request_id == Some(request.performance_seed)
        {
            return None;
        }
        let hold = match request.gesture {
            VoiceGesture::PurrHum => 0.55,
            VoiceGesture::LowRumble => 0.44,
            VoiceGesture::ReliefExhale => 0.36,
            VoiceGesture::MewWhine => 0.32,
            VoiceGesture::WarmChuff | VoiceGesture::ClippedPulse => 0.24,
        };
        self.next_allowed_seconds = now_seconds + hold;
        self.last_request_id = Some(request.performance_seed);
        Some(request)
    }
}

#[must_use]
pub const fn social_intent(reason: InteractionReasonCode) -> SocialIntent {
    match reason {
        InteractionReasonCode::CuriousInspection => SocialIntent::Query,
        InteractionReasonCode::GentleContact => SocialIntent::Contact,
        InteractionReasonCode::PlayfulCooperation | InteractionReasonCode::SharedRitual => {
            SocialIntent::Invite
        }
        InteractionReasonCode::PhysicalStartle | InteractionReasonCode::ComponentDetached => {
            SocialIntent::Alarm
        }
        InteractionReasonCode::EffortfulResistance => SocialIntent::Effort,
        InteractionReasonCode::CalmBoundary => SocialIntent::Boundary,
        InteractionReasonCode::RhythmRecognition => SocialIntent::Acknowledge,
        InteractionReasonCode::ComponentRecovery | InteractionReasonCode::SuccessfulReunion => {
            SocialIntent::Relief
        }
        InteractionReasonCode::QuietAcknowledgement => SocialIntent::Notice,
    }
}

const fn intent_priority(intent: SocialIntent) -> u8 {
    match intent {
        SocialIntent::Alarm => 255,
        SocialIntent::Boundary => 240,
        SocialIntent::Effort => 210,
        SocialIntent::Relief => 190,
        SocialIntent::Invite => 170,
        SocialIntent::Query => 150,
        SocialIntent::Contact => 130,
        SocialIntent::Acknowledge => 110,
        SocialIntent::Notice => 90,
    }
}

const fn expression_shape(intent: SocialIntent) -> (f32, f32, f32, f32, f32, f32, f32) {
    match intent {
        SocialIntent::Notice => (0.92, 1.08, 0.08, 0.08, 0.08, 0.08, 0.10),
        SocialIntent::Contact => (0.82, 1.04, 0.02, 0.34, 0.06, 0.04, 0.28),
        SocialIntent::Acknowledge => (0.88, 1.02, -0.10, 0.20, 0.10, 0.10, 0.20),
        SocialIntent::Invite => (0.96, 1.13, 0.16, 0.52, 0.03, 0.10, 0.18),
        SocialIntent::Query => (0.94, 1.14, 0.42, 0.02, 0.14, 0.16, 0.02),
        SocialIntent::Effort => (0.64, 0.94, -0.12, -0.22, 0.68, 0.88, 0.0),
        SocialIntent::Boundary => (0.52, 0.92, 0.0, -0.32, 0.86, 0.70, 0.0),
        SocialIntent::Alarm => (1.0, 1.18, 0.24, -0.28, 0.48, 1.0, 0.0),
        SocialIntent::Relief => (0.76, 1.0, -0.04, 0.40, 0.02, 0.0, 0.92),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{InteractionExpressionTarget, VocalTrigger};

    fn plan(reason: InteractionReasonCode, trigger: VocalTrigger) -> InteractionResponsePlan {
        InteractionResponsePlan {
            response_id: 7,
            episode_id: 9,
            reason,
            expression: InteractionExpressionTarget::default(),
            voice_trigger: Some(trigger),
            ..InteractionResponsePlan::default()
        }
    }

    #[test]
    fn physical_load_is_visible_and_phrase_timing_meets_contract() {
        let mut director = ExpressionDirector::default();
        let mut life = LifeState::new(crate::Genome::from_seed(8));
        life.affect.attachment = 0.7;
        let phrase = director.direct(
            plan(
                InteractionReasonCode::EffortfulResistance,
                VocalTrigger::CalmBoundary,
            ),
            LivingStateFrame::from_life(&life),
            PhysicalExpressionContext {
                strain: 0.9,
                deformation: 0.8,
                airborne: true,
                speed: 0.8,
                ..PhysicalExpressionContext::default()
            },
        );
        assert!(phrase.plan.onset_seconds <= 0.2);
        assert!((0.8..=2.0).contains(&phrase.plan.hold_seconds));
        assert!(phrase.plan.expression.effort >= 0.8);
        assert!(phrase.plan.expression.mouth_asymmetry > 0.0);
    }

    #[test]
    fn quiet_state_suppresses_low_priority_voice_but_not_boundary() {
        let mut director = ExpressionDirector::default();
        let mut life = LifeState::new(crate::Genome::from_seed(8));
        life.current_action = ActionId::Sleep;
        let quiet = director.direct(
            plan(
                InteractionReasonCode::GentleContact,
                VocalTrigger::SoftTouch,
            ),
            LivingStateFrame::from_life(&life),
            PhysicalExpressionContext::default(),
        );
        assert!(quiet.quiet_suppressed);
        assert!(quiet.plan.voice_trigger.is_none());

        director.tick(4.0);
        let boundary = director.direct(
            plan(
                InteractionReasonCode::CalmBoundary,
                VocalTrigger::CalmBoundary,
            ),
            LivingStateFrame::from_life(&life),
            PhysicalExpressionContext::default(),
        );
        assert!(!boundary.quiet_suppressed);
        assert_eq!(boundary.voice_gesture, Some(VoiceGesture::LowRumble));
    }

    #[test]
    fn vocal_arbiter_has_one_gate_and_quiet_wins() {
        let request = VocalRequest {
            motif_id: 1,
            performance_seed: 2,
            gain: 0.2,
            pan: 0.0,
            pitch_scale: 1.0,
            tempo_scale: 1.0,
            stress: 0.0,
            purr: false,
            gesture: VoiceGesture::WarmChuff,
            priority: 150,
            rhythm_intervals: [0.0; 8],
        };
        let mut arbiter = VocalArbiter::default();
        assert!(arbiter.admit(request.clone(), 1.0, true).is_none());
        let mut boundary = request;
        boundary.performance_seed = 3;
        boundary.gesture = VoiceGesture::LowRumble;
        boundary.priority = 240;
        assert!(arbiter.admit(boundary, 1.0, true).is_some());
    }
}
