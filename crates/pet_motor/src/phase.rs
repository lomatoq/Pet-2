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
    if program == P::MoveInspectPauseScan
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
    let performed_dt = dt * phase_clock_scale(active.program, spec.name);
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
    if phase_name == "hold" && program.family() == crate::ProgramFamily::TouchManipulation {
        return !context.pointer_down || context.gesture_ended;
    }
    match (program, phase_name) {
        (P::MovePunctuatedTravel, "coast") => {
            context
                .somatic
                .motor_error
                .min(context.body.efference_copy.intended_velocity.length())
                < 0.08
        }
        (P::MovePunctuatedTravel | P::MoveBrakeSquashRecover, "brake" | "stillness") => {
            context.body.motion.velocity.length() < 0.045
        }
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
