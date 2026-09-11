//! R14 semantic orchestration seam.
//!
//! This module intentionally owns no second brain. It turns already-grounded
//! perception evidence plus the organism's current state into one readable
//! semantic intent, then records the outcome for compact social memory.

use lifecore::{
    AppraisedEvent, CompanionEventKind, CompanionIntentFrame, CompanionSocialMemory,
    ExpectedOutcome, IntentTarget, PrimaryIntent, SocialMode,
};

#[derive(Debug, Clone)]
pub struct CompanionRuntime {
    pub memory: CompanionSocialMemory,
    current: CompanionIntentFrame,
    last_event: Option<AppraisedEvent>,
    intent_age: f32,
}

impl Default for CompanionRuntime {
    fn default() -> Self {
        Self {
            memory: CompanionSocialMemory::default(),
            current: CompanionIntentFrame::default(),
            last_event: None,
            intent_age: 0.0,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct CompanionBrainContext {
    pub episode_id: u64,
    pub valence: f32,
    pub arousal: f32,
    pub fatigue: f32,
    pub play_readiness: f32,
    pub curiosity: f32,
    pub frustration: f32,
    pub pain_like: f32,
    pub social_safety: f32,
    pub contact_pleasantness: f32,
    pub agency_match: f32,
    pub user_available: f32,
}

impl CompanionRuntime {
    #[must_use]
    pub fn tick(
        &mut self,
        events: impl IntoIterator<Item = AppraisedEvent>,
        brain: CompanionBrainContext,
        dt: f32,
    ) -> CompanionIntentFrame {
        let dt = finite_dt(dt);
        self.intent_age += dt;
        self.memory.motivation.tick(
            matches!(
                self.current.primary,
                PrimaryIntent::InviteContact | PrimaryIntent::AcceptContact | PrimaryIntent::Nuzzle
            ),
            dt,
        );

        let best = events
            .into_iter()
            .map(AppraisedEvent::sanitized)
            .max_by(|left, right| event_priority(*left).total_cmp(&event_priority(*right)));

        // Measured pain/integrity always outranks external social interpretation.
        if brain.pain_like > 0.24 {
            self.current = frame_for(
                PrimaryIntent::GuardPain,
                IntentTarget::default(),
                brain,
                1.0,
                SocialMode::Protective,
            );
            self.intent_age = 0.0;
            return self.current;
        }

        if let Some(event) = best {
            self.last_event = Some(event);
            let proposed = intent_from_event(event, brain, &self.memory);
            let can_switch = can_switch_intent(self.current, proposed, self.intent_age, event);
            if can_switch {
                self.current = proposed;
                self.intent_age = 0.0;
            }
        } else if self.intent_age > persistence_seconds(self.current) {
            self.current = resting_fallback(brain, &self.memory);
            self.intent_age = 0.0;
        }
        self.current = self.current.sanitized();
        self.current
    }

    pub fn record_outcome(&mut self, outcome: CompanionOutcome) {
        let quality = outcome.quality.clamp(-1.0, 1.0);
        if outcome.clear_direct_user_cause && quality < -0.30 {
            self.memory.update_clear_direct_harm(-quality);
        } else if quality > 0.0 {
            self.memory.update_safe_social_outcome(quality, outcome.duration);
        }
        // Ordinary non-response does not reduce attachment/trust.
    }

    #[must_use]
    pub const fn current(&self) -> CompanionIntentFrame {
        self.current
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct CompanionOutcome {
    pub quality: f32,
    pub duration: f32,
    pub clear_direct_user_cause: bool,
}

fn intent_from_event(
    event: AppraisedEvent,
    brain: CompanionBrainContext,
    memory: &CompanionSocialMemory,
) -> CompanionIntentFrame {
    if event.threat_likelihood > 0.72 {
        return frame_for(
            PrimaryIntent::StartleFreeze,
            event.target,
            brain,
            event.confidence,
            SocialMode::Protective,
        );
    }
    match event.kind {
        CompanionEventKind::Stroke | CompanionEventKind::SoftTouch
            if event.contact_quality > 0.20 && event.directness > 0.80 =>
        {
            frame_for(
                PrimaryIntent::AcceptContact,
                event.target,
                brain,
                event.confidence,
                SocialMode::Affiliative,
            )
        }
        CompanionEventKind::Hold | CompanionEventKind::Pull
            if event.contact_quality < -0.10 =>
        {
            frame_for(
                PrimaryIntent::RejectContact,
                event.target,
                brain,
                event.confidence,
                SocialMode::Protective,
            )
        }
        CompanionEventKind::PlayInvitation
            if event.play_likelihood > 0.55 && brain.user_available > 0.25 =>
        {
            frame_for(
                PrimaryIntent::InvitePlay,
                event.target,
                brain,
                event.confidence,
                SocialMode::Playful,
            )
        }
        CompanionEventKind::PointerApproach | CompanionEventKind::PointerHover => {
            frame_for(
                PrimaryIntent::Orient,
                event.target,
                brain,
                event.confidence,
                SocialMode::SocialOrienting,
            )
        }
        CompanionEventKind::VisualNovelty
        | CompanionEventKind::WindowAppeared
        | CompanionEventKind::WindowMoved
        | CompanionEventKind::WindowResized => frame_for(
            PrimaryIntent::Inspect,
            event.target,
            brain,
            event.confidence,
            SocialMode::Autonomous,
        ),
        CompanionEventKind::SurfaceCollision | CompanionEventKind::Trapped => frame_for(
            PrimaryIntent::EscapePressure,
            event.target,
            brain,
            event.confidence,
            SocialMode::Protective,
        ),
        CompanionEventKind::Freed | CompanionEventKind::BodyRecovery => frame_for(
            PrimaryIntent::SettleAfterStress,
            event.target,
            brain,
            event.confidence,
            SocialMode::Autonomous,
        ),
        CompanionEventKind::UserReturned => {
            let intensity = (memory.familiarity * 0.45
                + memory.attachment * 0.35
                + memory.motivation.reunion_interest * 0.20)
                .clamp(0.0, 1.0);
            let primary = if intensity > 0.62 {
                PrimaryIntent::SocialCheckIn
            } else {
                PrimaryIntent::Orient
            };
            frame_for(primary, event.target, brain, event.confidence, SocialMode::SocialOrienting)
        }
        CompanionEventKind::AppTaskCompleted | CompanionEventKind::ProgressCompleted => {
            // Desktop completion is not automatically a social celebration.
            frame_for(
                PrimaryIntent::Orient,
                event.target,
                brain,
                event.confidence * 0.45,
                SocialMode::Autonomous,
            )
        }
        _ => frame_for(
            PrimaryIntent::Orient,
            event.target,
            brain,
            event.confidence * 0.55,
            SocialMode::Autonomous,
        ),
    }
}

fn frame_for(
    primary: PrimaryIntent,
    target: IntentTarget,
    brain: CompanionBrainContext,
    confidence: f32,
    social_mode: SocialMode,
) -> CompanionIntentFrame {
    CompanionIntentFrame {
        episode_id: brain.episode_id,
        primary,
        target,
        confidence,
        urgency: match primary {
            PrimaryIntent::GuardPain | PrimaryIntent::EscapePressure | PrimaryIntent::StartleFreeze => 0.9,
            PrimaryIntent::RejectContact => 0.7,
            _ => 0.3,
        },
        valence: brain.valence,
        arousal: brain.arousal,
        social_safety: brain.social_safety,
        trust: 0.0,
        attachment: 0.0,
        play_readiness: brain.play_readiness,
        curiosity: brain.curiosity,
        fatigue: brain.fatigue,
        discomfort: brain.pain_like,
        surprise: 0.0,
        frustration: brain.frustration,
        anticipation: if matches!(primary, PrimaryIntent::InvitePlay | PrimaryIntent::Intercept) { 0.7 } else { 0.2 },
        contact_pleasantness: brain.contact_pleasantness,
        agency_match: brain.agency_match,
        expected_outcome: ExpectedOutcome {
            continuation: if matches!(primary, PrimaryIntent::AcceptContact | PrimaryIntent::InvitePlay) { 0.7 } else { 0.2 },
            user_response: if matches!(primary, PrimaryIntent::InviteContact | PrimaryIntent::InvitePlay | PrimaryIntent::SocialCheckIn) { 0.7 } else { 0.1 },
            object_contact: 0.0,
            success: 0.55,
            uncertainty: (1.0 - confidence).clamp(0.0, 1.0),
        },
        social_mode,
        persistence: 0.55,
    }
}

fn resting_fallback(
    brain: CompanionBrainContext,
    memory: &CompanionSocialMemory,
) -> CompanionIntentFrame {
    if brain.fatigue > 0.82 {
        frame_for(
            PrimaryIntent::Rest,
            IntentTarget::default(),
            brain,
            1.0,
            SocialMode::Resting,
        )
    } else if memory.attachment > 0.42 && brain.user_available > 0.15 {
        frame_for(
            PrimaryIntent::QuietCompanionship,
            IntentTarget::default(),
            brain,
            0.65,
            SocialMode::Affiliative,
        )
    } else {
        frame_for(
            PrimaryIntent::IdleContent,
            IntentTarget::default(),
            brain,
            0.7,
            SocialMode::Autonomous,
        )
    }
}

fn can_switch_intent(
    current: CompanionIntentFrame,
    proposed: CompanionIntentFrame,
    age: f32,
    event: AppraisedEvent,
) -> bool {
    if proposed.protective() && !current.protective() {
        return true;
    }
    if current.primary == proposed.primary {
        return false;
    }
    let minimum = if current.protective() { 0.12 } else { 0.22 };
    age >= minimum
        && (proposed.confidence > current.confidence + 0.12
            || event.novelty > 0.72
            || event.directness > 0.90)
}

fn persistence_seconds(frame: CompanionIntentFrame) -> f32 {
    match frame.primary {
        PrimaryIntent::StartleFreeze => 0.12,
        PrimaryIntent::Orient => 0.45,
        PrimaryIntent::Inspect => 1.2,
        PrimaryIntent::AcceptContact | PrimaryIntent::Nuzzle => 0.55,
        PrimaryIntent::InvitePlay | PrimaryIntent::InviteContact => 2.2,
        PrimaryIntent::QuietCompanionship => 12.0,
        PrimaryIntent::Rest => 8.0,
        PrimaryIntent::Sleep => 60.0,
        _ => 1.6,
    }
}

fn event_priority(event: AppraisedEvent) -> f32 {
    (event.threat_likelihood * 1.60
        + event.directness * 0.70
        + event.social_likelihood * 0.40
        + event.play_likelihood * 0.28
        + event.novelty * 0.34
        + event.confidence * 0.55)
        .clamp(0.0, 4.0)
}

fn finite_dt(dt: f32) -> f32 {
    if dt.is_finite() { dt.clamp(0.0, 0.25) } else { 0.0 }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lifecore::{CompanionEventSource, IntentTargetKind};

    #[test]
    fn window_motion_does_not_reduce_attachment() {
        let mut runtime = CompanionRuntime::default();
        runtime.memory.attachment = 0.7;
        let before = runtime.memory.attachment;
        runtime.record_outcome(CompanionOutcome { quality: -0.8, duration: 1.0, clear_direct_user_cause: false });
        assert_eq!(runtime.memory.attachment, before);
    }

    #[test]
    fn safe_stroke_becomes_accept_contact() {
        let mut runtime = CompanionRuntime::default();
        let event = AppraisedEvent {
            source: CompanionEventSource::DirectContact,
            kind: CompanionEventKind::Stroke,
            target: IntentTarget { kind: IntentTargetKind::ContactPoint, confidence: 1.0, ..IntentTarget::default() },
            confidence: 0.95,
            directness: 1.0,
            social_likelihood: 0.9,
            contact_quality: 0.8,
            ..AppraisedEvent::default()
        };
        let frame = runtime.tick([event], CompanionBrainContext { contact_pleasantness: 0.8, social_safety: 0.9, user_available: 1.0, ..CompanionBrainContext::default() }, 0.25);
        assert_eq!(frame.primary, PrimaryIntent::AcceptContact);
    }
}