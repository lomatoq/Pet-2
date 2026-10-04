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
    evidence_age: f32,
}

impl Default for CompanionRuntime {
    fn default() -> Self {
        Self {
            memory: CompanionSocialMemory::default(),
            current: CompanionIntentFrame::default(),
            last_event: None,
            intent_age: 0.0,
            evidence_age: 0.0,
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
    pub fn from_memory(memory: CompanionSocialMemory) -> Self {
        Self {
            memory: memory.sanitized(),
            ..Self::default()
        }
    }

    #[must_use]
    pub fn tick(
        &mut self,
        events: impl IntoIterator<Item = AppraisedEvent>,
        brain: CompanionBrainContext,
        dt: f32,
    ) -> CompanionIntentFrame {
        let dt = finite_dt(dt);
        self.intent_age = (self.intent_age + dt).min(3600.0);
        self.evidence_age = (self.evidence_age + dt).min(3600.0);
        self.memory.motivation.tick(
            matches!(
                self.current.primary,
                PrimaryIntent::InviteContact | PrimaryIntent::AcceptContact | PrimaryIntent::Nuzzle
            ),
            dt,
        );

        // Context-only events and replayed observations cannot renew the cause
        // of an old expression, or hide a lower-priority actionable observation.
        let best = events
            .into_iter()
            .filter(|event| event.timestamp.is_finite() && event.timestamp >= 0.0)
            .map(AppraisedEvent::sanitized)
            .filter(|event| {
                self.last_event.is_none_or(|previous| {
                    event.timestamp >= previous.timestamp && *event != previous
                })
            })
            .filter_map(|event| {
                intent_from_event(event, brain, &self.memory).map(|proposed| (event, proposed))
            })
            .max_by(|(left, _), (right, _)| {
                event_priority(*left).total_cmp(&event_priority(*right))
            });

        // Measured pain/integrity always outranks external social interpretation.
        if brain.pain_like > 0.24 {
            if self.current.primary != PrimaryIntent::GuardPain {
                self.intent_age = 0.0;
            }
            self.current = frame_for(
                PrimaryIntent::GuardPain,
                IntentTarget::default(),
                brain,
                1.0,
                SocialMode::Protective,
            );
            self.evidence_age = 0.0;
        } else {
            if let Some((event, proposed)) = best
                && (self.current.primary == proposed.primary
                    || can_switch_intent(self.current, proposed, self.intent_age, event)
                    || self.evidence_age >= persistence_seconds(self.current))
            {
                // Follow a moving referent and changing appraisal within the
                // same bout. Only a different action resets the bout clock.
                if self.current.primary != proposed.primary {
                    self.intent_age = 0.0;
                }
                self.current = proposed;
                self.last_event = Some(event);
                self.evidence_age = 0.0;
            }
            if self.evidence_age >= persistence_seconds(self.current) {
                let fallback = resting_fallback(brain, &self.memory);
                if self.current.primary != fallback.primary {
                    self.intent_age = 0.0;
                }
                self.current = fallback;
            } else {
                // Refresh fast organism state even without a new event. The
                // retained observation fades independently of unrelated input.
                let retained = self.last_event.and_then(|event| {
                    intent_from_event(event, brain, &self.memory)
                        .filter(|frame| frame.primary == self.current.primary)
                        .map(|frame| (event, frame))
                });
                self.current = retained.map_or_else(
                    || {
                        frame_for(
                            self.current.primary,
                            self.current.target,
                            brain,
                            self.current.confidence,
                            self.current.social_mode,
                        )
                    },
                    |(event, frame)| {
                        let retention = (-self.evidence_age / persistence_seconds(frame)).exp();
                        appraise_event(frame, event, brain, retention)
                    },
                );
            }
        }
        self.current.trust = self.memory.trust;
        self.current.attachment = self.memory.attachment;
        self.current.social_safety =
            (self.current.social_safety * 0.65 + self.memory.social_safety * 0.35).clamp(0.0, 1.0);
        self.current = self.current.sanitized();
        self.current
    }

    pub fn record_outcome(&mut self, outcome: CompanionOutcome) {
        let quality = outcome.quality.clamp(-1.0, 1.0);
        if outcome.clear_direct_user_cause && quality < -0.30 {
            self.memory.update_clear_direct_harm(-quality);
        } else if quality > 0.0 {
            self.memory
                .update_safe_social_outcome(quality, outcome.duration);
        }
        // Ordinary non-response does not reduce attachment/trust.
    }

    /// Synchronize expression with an already-grounded execution decision.
    /// The caller owns goal/target evidence and refusal/focus admission. This
    /// seam does not select actions, start social bids or update affiliation.
    pub fn synchronize_execution(
        &mut self,
        primary: PrimaryIntent,
        target: IntentTarget,
        expected: ExpectedOutcome,
    ) -> bool {
        let mut executed = self.current;
        executed.primary = primary;
        let requested_protective = CompanionIntentFrame {
            primary,
            ..Default::default()
        }
        .protective();
        if self.current.protective() && !requested_protective {
            return false;
        }
        if primary != self.current.primary {
            self.intent_age = 0.0;
        }
        executed.target = target.sanitized();
        executed.confidence = executed.target.confidence;
        executed.expected_outcome = expected.sanitized();
        executed.anticipation = executed
            .expected_outcome
            .continuation
            .max(executed.expected_outcome.user_response)
            .max(executed.expected_outcome.object_contact);
        executed.social_mode = match primary {
            PrimaryIntent::Rest | PrimaryIntent::Sleep => SocialMode::Resting,
            PrimaryIntent::InviteContact => SocialMode::ContactSeeking,
            PrimaryIntent::AcceptContact
            | PrimaryIntent::Nuzzle
            | PrimaryIntent::QuietCompanionship => SocialMode::Affiliative,
            PrimaryIntent::InvitePlay
            | PrimaryIntent::Chase
            | PrimaryIntent::Intercept
            | PrimaryIntent::Catch
            | PrimaryIntent::OfferObject => SocialMode::Playful,
            PrimaryIntent::SocialCheckIn => SocialMode::SocialOrienting,
            _ if executed.protective() => SocialMode::Protective,
            _ => SocialMode::Autonomous,
        };
        self.current = executed.sanitized();
        self.evidence_age = 0.0;
        true
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
) -> Option<CompanionIntentFrame> {
    if event.kind == CompanionEventKind::None || event.confidence <= 0.01 {
        return None;
    }
    if event.threat_likelihood > 0.72 {
        return Some(frame_for(
            PrimaryIntent::StartleFreeze,
            event.target,
            brain,
            event.confidence,
            SocialMode::Protective,
        ));
    }
    let frame = match event.kind {
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
        CompanionEventKind::Hold | CompanionEventKind::Pull if event.contact_quality < -0.10 => {
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
        CompanionEventKind::PointerApproach | CompanionEventKind::PointerHover => frame_for(
            PrimaryIntent::Orient,
            event.target,
            brain,
            event.confidence,
            SocialMode::SocialOrienting,
        ),
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
            frame_for(
                primary,
                event.target,
                brain,
                event.confidence,
                SocialMode::SocialOrienting,
            )
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
        // Aggregate activity/idle state informs the brain context; it is not
        // evidence of a new receiver-directed act or a new spatial referent.
        CompanionEventKind::UserActive
        | CompanionEventKind::UserIdleShort
        | CompanionEventKind::UserIdleLong => return None,
        _ if event.target.kind != lifecore::IntentTargetKind::None || event.directness > 0.80 => {
            frame_for(
                PrimaryIntent::Orient,
                event.target,
                brain,
                event.confidence * 0.55,
                SocialMode::Autonomous,
            )
        }
        _ => return None,
    };
    Some(frame)
}

fn frame_for(
    primary: PrimaryIntent,
    target: IntentTarget,
    brain: CompanionBrainContext,
    confidence: f32,
    social_mode: SocialMode,
) -> CompanionIntentFrame {
    let confidence = unit(confidence);
    let available = unit(brain.user_available);
    let capacity = 1.0 - unit(brain.fatigue);
    let agency = unit(brain.agency_match);
    let contact = unit(brain.contact_pleasantness);
    let play = unit(brain.play_readiness);
    // These are bounded appraisal proxies, not calibrated probabilities or
    // invented knowledge of a user's response. They change with current state.
    let continuation = match primary {
        PrimaryIntent::AcceptContact | PrimaryIntent::Nuzzle => contact * capacity,
        PrimaryIntent::InvitePlay => play * capacity * available,
        _ => 0.0,
    } * confidence;
    let user_response = if matches!(
        primary,
        PrimaryIntent::InviteContact | PrimaryIntent::InvitePlay | PrimaryIntent::SocialCheckIn
    ) {
        available * capacity * confidence
    } else {
        0.0
    };
    let object_contact = if target.kind == lifecore::IntentTargetKind::Orb
        && matches!(
            primary,
            PrimaryIntent::Approach
                | PrimaryIntent::Chase
                | PrimaryIntent::Intercept
                | PrimaryIntent::Catch
                | PrimaryIntent::Carry
                | PrimaryIntent::OfferObject
        ) {
        confidence * agency * capacity
    } else {
        0.0
    };
    CompanionIntentFrame {
        episode_id: brain.episode_id,
        primary,
        target,
        confidence,
        urgency: match primary {
            PrimaryIntent::GuardPain
            | PrimaryIntent::EscapePressure
            | PrimaryIntent::StartleFreeze => 0.9,
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
        anticipation: continuation.max(user_response).max(object_contact),
        contact_pleasantness: brain.contact_pleasantness,
        agency_match: brain.agency_match,
        expected_outcome: ExpectedOutcome {
            continuation,
            user_response,
            object_contact,
            success: confidence * agency * (1.0 - unit(brain.frustration)),
            uncertainty: (1.0 - confidence).clamp(0.0, 1.0),
        },
        social_mode,
        persistence: 0.55,
    }
    .sanitized()
}

fn appraise_event(
    mut frame: CompanionIntentFrame,
    event: AppraisedEvent,
    brain: CompanionBrainContext,
    retention: f32,
) -> CompanionIntentFrame {
    let retention = unit(retention);
    let confidence = frame.confidence * retention;
    let unexpected = 1.0 - event.expectedness;
    frame.confidence = confidence;
    frame.target.confidence *= retention;
    frame.surprise = event.confidence
        * (event.prediction_error * 0.70 + event.novelty * 0.30)
        * unexpected
        * retention;
    frame.expected_outcome.continuation *= retention;
    if matches!(
        frame.primary,
        PrimaryIntent::AcceptContact | PrimaryIntent::Nuzzle
    ) {
        // Direct pleasant contact is evidence even when the slow user model has
        // not yet inferred availability. It does not create a new social bid.
        frame.expected_outcome.continuation = frame
            .expected_outcome
            .continuation
            .max(unit(event.contact_quality) * (1.0 - unit(brain.fatigue)) * frame.confidence);
    }
    frame.expected_outcome.user_response *= retention;
    frame.expected_outcome.object_contact *= retention;
    frame.expected_outcome.success = confidence
        * (event.controllability * 0.60 + unit(brain.agency_match) * 0.40)
        * (1.0 - event.threat_likelihood)
        * (1.0 - unit(brain.frustration));
    frame.expected_outcome.uncertainty =
        1.0 - confidence * (1.0 - event.prediction_error * unexpected);
    frame.anticipation = frame
        .expected_outcome
        .continuation
        .max(frame.expected_outcome.user_response)
        .max(frame.expected_outcome.object_contact);
    frame.sanitized()
}

fn unit(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        0.0
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
    if dt.is_finite() {
        dt.clamp(0.0, 0.25)
    } else {
        0.0
    }
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
        runtime.record_outcome(CompanionOutcome {
            quality: -0.8,
            duration: 1.0,
            clear_direct_user_cause: false,
        });
        assert_eq!(runtime.memory.attachment, before);
    }

    #[test]
    fn safe_stroke_becomes_accept_contact() {
        let mut runtime = CompanionRuntime::default();
        let event = AppraisedEvent {
            source: CompanionEventSource::DirectContact,
            kind: CompanionEventKind::Stroke,
            target: IntentTarget {
                kind: IntentTargetKind::ContactPoint,
                confidence: 1.0,
                ..IntentTarget::default()
            },
            confidence: 0.95,
            directness: 1.0,
            social_likelihood: 0.9,
            contact_quality: 0.8,
            ..AppraisedEvent::default()
        };
        let frame = runtime.tick(
            [event],
            CompanionBrainContext {
                contact_pleasantness: 0.8,
                social_safety: 0.9,
                user_available: 1.0,
                ..CompanionBrainContext::default()
            },
            0.25,
        );
        assert_eq!(frame.primary, PrimaryIntent::AcceptContact);
    }

    fn observation(kind: CompanionEventKind, timestamp: f64) -> AppraisedEvent {
        AppraisedEvent {
            kind,
            timestamp,
            target: IntentTarget {
                kind: IntentTargetKind::Cursor,
                position: Some(glam::Vec2::new(0.3, 0.4)),
                confidence: 0.9,
                ..Default::default()
            },
            confidence: 0.9,
            directness: 1.0,
            expectedness: 0.6,
            controllability: 0.7,
            ..Default::default()
        }
    }

    #[test]
    fn moving_referent_and_fast_state_refresh_without_restarting_bout() {
        let mut runtime = CompanionRuntime::default();
        let mut event = observation(CompanionEventKind::PlayInvitation, 1.0);
        event.play_likelihood = 0.9;
        let brain = CompanionBrainContext {
            user_available: 1.0,
            play_readiness: 0.8,
            agency_match: 0.9,
            ..Default::default()
        };
        let first = runtime.tick([event], brain, 0.25);
        event.timestamp = 1.25;
        event.target.position = Some(glam::Vec2::new(0.6, 0.7));
        let updated = runtime.tick(
            [event],
            CompanionBrainContext {
                valence: -0.4,
                arousal: 0.75,
                fatigue: 0.7,
                episode_id: 23,
                ..brain
            },
            0.25,
        );
        assert_eq!(updated.primary, PrimaryIntent::InvitePlay);
        assert_eq!(updated.target.position, event.target.position);
        assert_eq!(updated.episode_id, 23);
        assert_eq!(updated.valence, -0.4);
        assert_eq!(updated.arousal, 0.75);
        assert_eq!(updated.fatigue, 0.7);
        assert!(updated.expected_outcome.continuation < first.expected_outcome.continuation);
        assert!(updated.expected_outcome.user_response < first.expected_outcome.user_response);
        assert_eq!(runtime.intent_age, 0.25);
    }

    #[test]
    fn replayed_event_expires_even_while_input_keeps_arriving() {
        let mut runtime = CompanionRuntime::default();
        let event = observation(CompanionEventKind::VisualNovelty, 5.0);
        let _ = runtime.tick([event], Default::default(), 0.25);
        for _ in 0..12 {
            let _ = runtime.tick([event], Default::default(), 0.25);
        }
        assert_eq!(runtime.current.primary, PrimaryIntent::IdleContent);
        assert_eq!(runtime.current.target.kind, IntentTargetKind::None);
        assert_eq!(runtime.current.surprise, 0.0);
    }

    #[test]
    fn context_activity_cannot_pin_contact_or_mask_an_actionable_event() {
        let mut runtime = CompanionRuntime::default();
        let mut contact = observation(CompanionEventKind::Stroke, 1.0);
        contact.contact_quality = 0.9;
        let brain = CompanionBrainContext {
            fatigue: 0.9,
            ..Default::default()
        };
        let _ = runtime.tick([contact], brain, 0.25);
        for step in 0..8 {
            let activity = AppraisedEvent {
                kind: CompanionEventKind::UserActive,
                timestamp: 2.0 + f64::from(step),
                confidence: 1.0,
                novelty: 1.0,
                directness: 1.0,
                ..Default::default()
            };
            let _ = runtime.tick([activity], brain, 0.25);
        }
        assert_eq!(runtime.current.primary, PrimaryIntent::Rest);
        let noise = AppraisedEvent {
            kind: CompanionEventKind::UserActive,
            timestamp: 12.0,
            confidence: 1.0,
            novelty: 1.0,
            directness: 1.0,
            ..Default::default()
        };
        let actual = observation(CompanionEventKind::WindowMoved, 12.0);
        let result = runtime.tick([noise, actual], Default::default(), 0.25);
        assert_eq!(result.primary, PrimaryIntent::Inspect);
    }

    #[test]
    fn ignored_different_event_does_not_renew_old_evidence() {
        let mut runtime = CompanionRuntime::default();
        let event = observation(CompanionEventKind::Stroke, 1.0);
        let mut contact = event;
        contact.contact_quality = 0.8;
        let _ = runtime.tick([contact], Default::default(), 0.25);
        let mut hover = observation(CompanionEventKind::PointerHover, 1.25);
        hover.confidence = 0.1;
        hover.directness = 0.2;
        for _ in 0..8 {
            let _ = runtime.tick([hover], Default::default(), 0.125);
            hover.timestamp += 0.125;
        }
        assert_ne!(runtime.current.primary, PrimaryIntent::AcceptContact);
    }

    #[test]
    fn event_surprise_decays_while_current_affect_stays_live() {
        let mut runtime = CompanionRuntime::default();
        let mut event = observation(CompanionEventKind::VisualNovelty, 1.0);
        event.prediction_error = 0.95;
        event.novelty = 0.9;
        event.expectedness = 0.05;
        let first = runtime.tick([event], Default::default(), 0.25);
        let later = runtime.tick(
            [],
            CompanionBrainContext {
                arousal: 0.8,
                frustration: 0.7,
                ..Default::default()
            },
            0.25,
        );
        assert!(first.surprise > 0.7);
        assert!(later.surprise < first.surprise);
        assert!(later.confidence < first.confidence);
        assert_eq!(later.arousal, 0.8);
        assert_eq!(later.frustration, 0.7);
        assert!(later.expected_outcome.uncertainty > first.expected_outcome.uncertainty);
        let mut expected = event;
        expected.expectedness = 0.99;
        expected.timestamp += 0.5;
        let familiar = runtime.tick([expected], Default::default(), 0.25);
        assert!(familiar.surprise < later.surprise);
    }

    #[test]
    fn pain_remains_authoritative_and_keeps_affiliation() {
        let mut runtime = CompanionRuntime::default();
        runtime.memory.attachment = 0.72;
        runtime.memory.trust = 0.81;
        let brain = CompanionBrainContext {
            pain_like: 0.8,
            ..Default::default()
        };
        let _ = runtime.tick([], brain, 0.25);
        let frame = runtime.tick(
            [observation(CompanionEventKind::PointerHover, 2.0)],
            brain,
            0.25,
        );
        assert_eq!(frame.primary, PrimaryIntent::GuardPain);
        assert_eq!(frame.attachment, runtime.memory.attachment);
        assert_eq!(frame.trust, runtime.memory.trust);
        assert_eq!(runtime.intent_age, 0.25);
        assert!(!runtime.synchronize_execution(
            PrimaryIntent::InvitePlay,
            IntentTarget::default(),
            ExpectedOutcome::default()
        ));
    }

    #[test]
    fn execution_sync_preserves_state_and_bout_but_updates_shared_meaning() {
        let mut runtime = CompanionRuntime::default();
        let brain = CompanionBrainContext {
            valence: 0.5,
            arousal: 0.7,
            fatigue: 0.2,
            ..Default::default()
        };
        let _ = runtime.tick([], brain, 0.25);
        let before = runtime.current();
        let target = IntentTarget {
            kind: IntentTargetKind::Orb,
            position: Some(glam::Vec2::new(0.7, 0.4)),
            confidence: 0.8,
            ..Default::default()
        };
        let expected = ExpectedOutcome {
            continuation: 0.4,
            user_response: 0.7,
            success: 0.5,
            uncertainty: 0.2,
            ..Default::default()
        };
        assert!(runtime.synchronize_execution(PrimaryIntent::OfferObject, target, expected));
        assert_eq!(runtime.current.expected_outcome, expected);
        assert_eq!(runtime.current.target, target);
        assert_eq!(runtime.current.valence, before.valence);
        assert_eq!(runtime.current.arousal, before.arousal);
        assert_eq!(runtime.current.attachment, before.attachment);
        let _ = runtime.tick([], brain, 0.125);
        assert!(runtime.synchronize_execution(PrimaryIntent::OfferObject, target, expected));
        assert_eq!(runtime.intent_age, 0.125);
        assert_eq!(runtime.current.anticipation, 0.7);
    }

    #[test]
    fn expiry_uses_elapsed_time_at_multiple_tick_rates() {
        for hz in [20, 60, 120] {
            let mut runtime = CompanionRuntime::default();
            let event = observation(CompanionEventKind::VisualNovelty, 1.0);
            let dt = 1.0 / hz as f32;
            let _ = runtime.tick([event], Default::default(), dt);
            for _ in 0..(hz * 2) {
                let _ = runtime.tick([event], Default::default(), dt);
            }
            assert_eq!(
                runtime.current.primary,
                PrimaryIntent::IdleContent,
                "hz={hz}"
            );
        }
    }
}
