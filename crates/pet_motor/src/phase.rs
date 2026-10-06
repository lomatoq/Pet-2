use crate::{
    ActivePerformance, BehaviorContextFrame, BehaviorProgramId, CompletionReason, definition,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PhaseAdvance {
    Hold,
    Advanced,
    Finished(CompletionReason),
}

/// Global body-action clock. Face presentation has its own real-time clock in
/// `pet_body::embodiment` and therefore is deliberately not accelerated here.
pub const BODY_ACTION_TEMPO: f32 = 2.0;

/// Physical preparation can accelerate; social and perceptual holds use seconds.
#[must_use]
pub fn phase_clock_scale(program: BehaviorProgramId, phase_name: &str) -> f32 {
    use BehaviorProgramId as P;
    if program == P::MovePunctuatedTravel {
        return 1.0;
    }
    if ordinary_orientation_phase(program, phase_name)
        || program == P::DefenseStartleOrientFreeze
        || program == P::MoveInspectPauseScan
        || response_phase(phase_name)
        || matches!(
            phase_name,
            "ack_or_withdraw"
                | "accept_or_withdraw"
                | "inspect"
                | "listen"
                | "appraise"
                | "decide"
                | "recheck"
                | "hold_boundary"
                | "release_gradually"
                | "hold"
                | "recovery_or_hold"
                | "release_recover"
        )
    {
        return 1.0;
    }
    match program {
        P::RestSurfaceRoostSearch
        | P::RestLandingSoftTouchdown
        | P::RestSitSettle
        | P::RestDrowsyYawn => 4.0,
        P::RestNremSleep if phase_name == "sleep_onset" => 4.0,
        P::RestNremSleep if phase_name == "nrem_hold" => 1.0,
        _ => BODY_ACTION_TEMPO,
    }
}

pub fn advance_phase(
    active: &mut ActivePerformance,
    context: &BehaviorContextFrame,
    dt: f32,
) -> PhaseAdvance {
    let definition = definition(active.program);
    let Some(spec) = definition.phases.get(usize::from(active.phase.index)) else {
        return PhaseAdvance::Finished(CompletionReason::Invalidated);
    };
    let dt = if dt.is_finite() {
        dt.clamp(0.0, 0.25)
    } else {
        0.0
    };
    if active.program == BehaviorProgramId::MovePunctuatedTravel {
        // Elapsed time is telemetry only. The same physical state must produce
        // the same transition at 30, 60 or 144 Hz, or after an arbitrarily long
        // obstruction. Goal changes/integrity reflexes can still interrupt it.
        active.total_time += dt;
        let Some(target) = active
            .locked_target
            .as_ref()
            .and_then(crate::BehaviorTarget::world_position)
            .filter(|target| target.is_finite())
        else {
            return PhaseAdvance::Finished(CompletionReason::Invalidated);
        };
        let diameter = context.body_diameter.max(glam::Vec2::splat(0.001));
        let distance = ((target - context.body.motion.world_position) / diameter).length();
        let speed = (context.body.motion.velocity / diameter).length();
        let requested = (context.body.efference_copy.intended_velocity / diameter).length();
        let strain = context
            .somatic
            .maximum_strain
            .max(context.body.shape.maximum_strain);
        let acquired = context.orientation.is_none_or(|eye| {
            eye.target_position.distance(target) <= 0.035 && eye.gaze_error <= 0.08
        });
        let velocity_fraction = if requested > 0.01 {
            (speed / requested).clamp(0.0, 1.0)
        } else {
            1.0
        };
        let (progress, ready) = match spec.name {
            "orient" => (
                if acquired { 1.0 } else { 0.0 },
                acquired || !orientation_acquisition_required(active.program, spec.name, context),
            ),
            "prepare" => (
                (1.0 - strain).clamp(0.0, 1.0),
                strain < 0.65 && !context.body.topology.recovery_active,
            ),
            "accelerate" => (
                velocity_fraction,
                velocity_fraction >= 0.70 || distance <= 0.75,
            ),
            "coast" => (1.0 / (1.0 + distance), distance <= 0.75),
            "brake" => (
                (1.0 - speed / 0.8).clamp(0.0, 1.0),
                speed <= 0.12 && distance <= 0.75,
            ),
            "arrival_pause" => (
                (1.0 - context.body.fluid.settle_error).clamp(0.0, 1.0),
                context.body.fluid.settle_error < 0.15
                    && context.body.fluid.slosh_energy < 0.20
                    && strain < 0.30,
            ),
            "appraise" => (if acquired { 1.0 } else { 0.0 }, acquired),
            _ => return PhaseAdvance::Finished(CompletionReason::Invalidated),
        };
        // Keep the existing packet/trace format; progress now describes physical
        // completion instead of elapsed / authored duration.
        active.phase_time = progress * spec.maximum_seconds;
        active.minimum_readability_reached = true;
        if !ready {
            return PhaseAdvance::Hold;
        }
        if usize::from(active.phase.index) + 1 >= definition.phases.len() {
            return PhaseAdvance::Finished(super::completion_from_context(active.program, context));
        }
        active.phase.index += 1;
        active.phase_time = 0.0;
        return PhaseAdvance::Advanced;
    }
    let waiting = response_phase(spec.name);
    if waiting {
        let bid = active.social_bid.get_or_insert_with(|| crate::SocialBid {
            bid_id: active.bout_id,
            object_id: context.orb_id,
            status: crate::BidStatus::Waiting,
            target: active.locked_target.clone(),
            expected_response: match active.program {
                BehaviorProgramId::PlayOrbCarryOffer => crate::ExpectedResponse::ToyMove,
                BehaviorProgramId::SocialAttentionBidWait => crate::ExpectedResponse::Attention,
                _ => crate::ExpectedResponse::Touch,
            },
            started_at: context.timestamp_seconds,
            started_frame: context.frame_id,
            response_received: false,
            previous_touch: context.pet_touched,
            previous_toy_held: context.orb_user_held,
        });
        let fresh =
            context.frame_id > bid.started_frame && context.timestamp_seconds > bid.started_at;
        let response = match bid.expected_response {
            crate::ExpectedResponse::Touch => {
                context.pet_touched && !bid.previous_touch && context.boundary_violation < 0.18
            }
            crate::ExpectedResponse::ToyMove => {
                context.orb_user_held
                    && !bid.previous_toy_held
                    && bid.object_id.is_some()
                    && bid.object_id == context.orb_id
            }
            crate::ExpectedResponse::Help => {
                context.locomotion_completed && context.somatic.motor_error < 0.08
            }
            crate::ExpectedResponse::Attention => {
                context.cursor_velocity.length() > 0.02
                    && context
                        .cursor_position
                        .distance(context.body.motion.world_position)
                        < 0.12
            }
        };
        bid.previous_touch = context.pet_touched;
        bid.previous_toy_held = context.orb_user_held;
        bid.response_received |= fresh && response && context.boundary_violation < 0.18;
        if bid.response_received {
            bid.status = crate::BidStatus::Accepted;
        }
        if context.focus_mode || context.boundary_violation >= 0.18 {
            bid.status = crate::BidStatus::Cancelled;
            return PhaseAdvance::Finished(CompletionReason::GracefulWithdrawal);
        }
        if bid.expected_response == crate::ExpectedResponse::ToyMove
            && (context.orb_position.is_none() || bid.object_id != context.orb_id)
        {
            bid.status = crate::BidStatus::Invalidated;
            return PhaseAdvance::Finished(CompletionReason::Invalidated);
        }
    }
    let clock = if ordinary_orientation_phase(active.program, spec.name)
        && !orientation_acquisition_required(active.program, spec.name, context)
    {
        // Pursuit, actual touch and urgent boundary responses retain the
        // original physical tempo even when their program is generic travel.
        BODY_ACTION_TEMPO
    } else {
        phase_clock_scale(active.program, spec.name)
    };
    let performed_dt = dt * clock;
    active.phase_time += performed_dt;
    if waiting && let Some(bid) = &mut active.social_bid {
        let elapsed = (context.timestamp_seconds - bid.started_at) as f32;
        if elapsed.is_finite() {
            active.phase_time = active.phase_time.max(elapsed.max(0.0));
        }
        if active.phase_time >= spec.maximum_seconds && !bid.response_received {
            bid.status = crate::BidStatus::TimedOut;
        }
    }
    active.total_time += performed_dt;
    active.minimum_readability_reached |= active.phase_time >= spec.minimum_seconds;
    let evidence_complete = phase_evidence_complete(active, spec.name, context);
    if (!waiting && active.phase_time < spec.minimum_seconds)
        || (!evidence_complete && active.phase_time < spec.maximum_seconds)
    {
        return PhaseAdvance::Hold;
    }
    if !evidence_complete
        && orientation_acquisition_required(active.program, spec.name, context)
        && context.orientation.is_some()
    {
        // Acquisition failure is observable failure, not permission to perform
        // toward an object the body never actually looked at.
        return PhaseAdvance::Finished(CompletionReason::TimedOut);
    }
    if usize::from(active.phase.index) + 1 >= definition.phases.len() {
        let reason = active.social_bid.as_ref().map_or_else(
            || super::completion_from_context(active.program, context),
            |bid| {
                if bid.response_received {
                    CompletionReason::UserResponded
                } else {
                    CompletionReason::GracefulWithdrawal
                }
            },
        );
        return PhaseAdvance::Finished(reason);
    }
    active.phase.index = active.phase.index.saturating_add(1);
    active.phase_time = 0.0;
    active.minimum_readability_reached = false;
    PhaseAdvance::Advanced
}

#[must_use]
pub fn phase_progress(active: &ActivePerformance) -> f32 {
    let spec = &definition(active.program).phases[usize::from(active.phase.index)];
    (active.phase_time / spec.maximum_seconds.max(1.0e-4)).clamp(0.0, 1.0)
}

fn phase_evidence_complete(
    active: &ActivePerformance,
    phase_name: &str,
    context: &BehaviorContextFrame,
) -> bool {
    use BehaviorProgramId as P;
    if response_phase(phase_name) {
        return active
            .social_bid
            .as_ref()
            .is_some_and(|bid| bid.response_received);
    }
    let program = active.program;
    if orientation_acquisition_required(program, phase_name, context)
        && let Some(evidence) = context.orientation
    {
        let target_matches = active
            .locked_target
            .as_ref()
            .and_then(crate::BehaviorTarget::world_position)
            .is_none_or(|target| {
                evidence.target_position.is_finite()
                    && evidence.target_position.distance(target) <= 0.035
            });
        return target_matches
            && evidence.gaze_error.is_finite()
            && evidence.gaze_error <= 0.08
            && evidence.acquired_seconds.is_finite()
            && evidence.acquired_seconds >= 0.04;
    }
    if phase_name == "hold" && program.family() == crate::ProgramFamily::TouchManipulation {
        return !context.pointer_down || context.gesture_ended;
    }
    match (program, phase_name) {
        (P::MoveBrakeSquashRecover, "stillness") => context.body.motion.velocity.length() < 0.045,
        (P::RestSurfaceRoostSearch, "approach_commit") => {
            if bottom_screen_edge(active) {
                context.screen_edge_gap_px <= 8.0
            } else {
                context
                    .somatic
                    .motor_error
                    .min(context.body.efference_copy.intended_velocity.length())
                    < 0.07
            }
        }
        (P::RestLandingSoftTouchdown, "first_contact") => {
            if bottom_screen_edge(active) {
                context.screen_edge_gap_px.abs() <= 4.0
            } else {
                context.somatic.contact_fraction >= 0.08
            }
        }
        (P::RestLandingSoftTouchdown, "load_transfer") => {
            if bottom_screen_edge(active) {
                context.screen_edge_supported
            } else {
                context.somatic.contact_fraction >= 0.22
                    && context.somatic.support_stability >= 0.55
            }
        }
        (P::RestSitSettle, "anchor" | "rest_hold") => context.support_confirmed(),
        (P::TouchSoftTouchYield, "recovery_or_hold") => {
            context.gesture_ended || context.pointer_released
        }
        (P::TouchSustainedHoldRelaxOrResist, "release_recover") => {
            context.gesture_ended || !context.pointer_down
        }
        (P::TouchPullReleaseRebound, "release_detect") => context.pointer_released,
        (P::DefenseFragmentTrackAndRemerge, "zipper_coalescence") => {
            context.body.topology.connected_components <= 1
        }
        (P::HomeDenReturnEscort, "watch_capture") => {
            context.world_event == crate::MotorWorldEvent::OrbStored
        }
        _ => true,
    }
}

/// Only voluntary perceptual preparation consumes real time and measured gaze.
/// Defensive startle/withdrawal, contact reflex and interception keep fast paths.
#[must_use]
pub fn ordinary_orientation_phase(program: BehaviorProgramId, phase_name: &str) -> bool {
    use BehaviorProgramId as P;
    match program {
        P::MoveOrientReflex => matches!(phase_name, "eyes_first" | "front_axis_turn"),
        P::MovePunctuatedTravel
        | P::MoveCuriosityArcApproach
        | P::MoveCautiousApproach
        | P::MoveInspectPauseScan
        | P::MoveCheckBackSocialReference => matches!(phase_name, "orient" | "prepare"),
        _ => false,
    }
}

/// Shared by phase evidence, motor defaults and final app gaze ownership. A
/// predictive pursuit or direct physical interaction is not ordinary inspection;
/// comparing its moving/led gaze to a frozen waypoint would create false failure.
#[must_use]
pub fn orientation_acquisition_required(
    program: BehaviorProgramId,
    phase_name: &str,
    context: &BehaviorContextFrame,
) -> bool {
    ordinary_orientation_phase(program, phase_name)
        && !context.pet_dragged
        && !context.pet_touched
        && context.boundary_violation < 0.18
        && !matches!(
            context.companion_intent,
            lifecore::PrimaryIntent::Chase
                | lifecore::PrimaryIntent::Intercept
                | lifecore::PrimaryIntent::Catch
        )
}

fn bottom_screen_edge(active: &ActivePerformance) -> bool {
    matches!(
        active.locked_target.as_ref(),
        Some(crate::BehaviorTarget::Surface(surface))
            if surface.surface_id.0 == "screen:bottom_edge"
    )
}

/// Human-dependent holds must never inherit physical performance tempo.
#[must_use]
pub fn response_phase(name: &str) -> bool {
    matches!(
        name,
        "look_wait" | "wait" | "user_turn" | "listen" | "ambiguity_wait"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec2;

    fn orient_performance() -> ActivePerformance {
        ActivePerformance {
            social_bid: None,
            bout_id: 1,
            program: BehaviorProgramId::MoveOrientReflex,
            phase: crate::PhaseId {
                program: BehaviorProgramId::MoveOrientReflex,
                index: 1,
            },
            phase_time: 0.0,
            total_time: 0.0,
            locked_target: Some(crate::BehaviorTarget::Point(Vec2::new(0.8, 0.3))),
            sampled_style: crate::BoutStyle::default(),
            minimum_readability_reached: false,
            interruption_request: None,
            source_action: lifecore::ActionId::IdleHover,
            cause: crate::MotorCause::BrainAction,
        }
    }

    #[test]
    fn travel_coasts_until_measured_arrival_regardless_of_elapsed_time() {
        let mut active = orient_performance();
        active.program = BehaviorProgramId::MovePunctuatedTravel;
        active.phase = crate::PhaseId {
            program: active.program,
            index: 3,
        };
        let mut context = BehaviorContextFrame::default();
        context.body.motion.world_position = Vec2::new(0.1, 0.3);
        // Both zero motor error and zero intended velocity used to trigger
        // an arrival far from the target. Neither is spatial evidence.
        for _ in 0..25 {
            assert_eq!(
                advance_phase(&mut active, &context, 0.1),
                PhaseAdvance::Hold
            );
        }
        let mut blocked = active.clone();
        blocked.phase_time = 20_000.0;
        blocked.total_time = 20_000.0;
        assert_eq!(
            advance_phase(&mut blocked, &context, 0.01),
            PhaseAdvance::Hold
        );
        context.body.motion.world_position = Vec2::new(0.8, 0.3);
        assert_eq!(
            advance_phase(&mut active, &context, 0.01),
            PhaseAdvance::Advanced
        );
        assert_eq!(
            crate::phase_name(active.program, active.phase.index),
            "brake"
        );
        context.body.motion.velocity = Vec2::new(0.4, 0.0);
        active.phase_time = 20_000.0;
        assert_eq!(
            advance_phase(&mut active, &context, 0.25),
            PhaseAdvance::Hold
        );
        context.body.motion.velocity = Vec2::ZERO;
        assert_eq!(
            advance_phase(&mut active, &context, 0.001),
            PhaseAdvance::Advanced
        );
        context.body.fluid.settle_error = 0.5;
        assert_eq!(
            advance_phase(&mut active, &context, 0.25),
            PhaseAdvance::Hold
        );
        context.body.fluid.settle_error = 0.0;
        assert_eq!(
            advance_phase(&mut active, &context, 0.001),
            PhaseAdvance::Advanced
        );
    }

    #[test]
    fn ordinary_preparation_waits_for_measured_same_target_and_can_timeout() {
        let mut active = orient_performance();
        let mut context = BehaviorContextFrame {
            orientation: Some(crate::OrientationEvidence {
                target_position: Vec2::new(0.8, 0.3),
                gaze_error: 0.4,
                acquired_seconds: 0.0,
            }),
            ..Default::default()
        };
        assert_eq!(
            advance_phase(&mut active, &context, 0.05),
            PhaseAdvance::Hold
        );
        assert_eq!(
            advance_phase(&mut active, &context, 0.05),
            PhaseAdvance::Hold
        );
        context.orientation.as_mut().unwrap().gaze_error = 0.02;
        context.orientation.as_mut().unwrap().acquired_seconds = 0.08;
        context.orientation.as_mut().unwrap().target_position = Vec2::new(0.2, 0.7);
        assert_eq!(
            advance_phase(&mut active, &context, 0.05),
            PhaseAdvance::Hold
        );
        context.orientation.as_mut().unwrap().target_position = Vec2::new(0.8, 0.3);
        assert_eq!(
            advance_phase(&mut active, &context, 0.05),
            PhaseAdvance::Advanced
        );
        println!(
            "orientation trace: hold error -> hold error -> hold wrong target -> advance acquired at 0.20s"
        );
        active = orient_performance();
        context.orientation.as_mut().unwrap().gaze_error = 0.4;
        for _ in 0..14 {
            assert_eq!(
                advance_phase(&mut active, &context, 0.05),
                PhaseAdvance::Hold
            );
        }
        assert_eq!(
            advance_phase(&mut active, &context, 0.05),
            PhaseAdvance::Finished(CompletionReason::TimedOut)
        );
        println!("orientation failure: TimedOut at 0.75s, no blind performance");
        assert_eq!(
            phase_clock_scale(BehaviorProgramId::MovePunctuatedTravel, "prepare"),
            1.0
        );
        assert_eq!(
            phase_clock_scale(BehaviorProgramId::DefenseStartleOrientFreeze, "eyes_first"),
            1.0
        );
        assert_eq!(
            phase_clock_scale(BehaviorProgramId::DefenseThreatHardenCompact, "prepare"),
            BODY_ACTION_TEMPO
        );
    }

    #[test]
    fn predictive_pursuit_and_contact_do_not_wait_for_frozen_inspection_target() {
        for intent in [
            lifecore::PrimaryIntent::Chase,
            lifecore::PrimaryIntent::Intercept,
            lifecore::PrimaryIntent::Catch,
        ] {
            let context = BehaviorContextFrame {
                companion_intent: intent,
                orientation: Some(crate::OrientationEvidence {
                    target_position: Vec2::new(0.1, 0.8),
                    gaze_error: 0.9,
                    acquired_seconds: 0.0,
                }),
                ..Default::default()
            };
            let mut active = orient_performance();
            assert!(!orientation_acquisition_required(
                active.program,
                "eyes_first",
                &context
            ));
            assert_eq!(
                advance_phase(&mut active, &context, 0.02),
                PhaseAdvance::Advanced
            );
            assert!((active.total_time - 0.04).abs() < 1.0e-6);
        }
        for (touched, dragged, boundary) in
            [(true, false, 0.0), (false, true, 0.0), (false, false, 0.3)]
        {
            let context = BehaviorContextFrame {
                pet_touched: touched,
                pet_dragged: dragged,
                boundary_violation: boundary,
                ..Default::default()
            };
            assert!(!orientation_acquisition_required(
                BehaviorProgramId::MovePunctuatedTravel,
                "prepare",
                &context
            ));
        }
        let context = BehaviorContextFrame {
            companion_intent: lifecore::PrimaryIntent::Inspect,
            ..Default::default()
        };
        assert!(orientation_acquisition_required(
            BehaviorProgramId::MoveInspectPauseScan,
            "orient",
            &context
        ));
    }
}
