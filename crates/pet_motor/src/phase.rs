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

/// Keeps sleep holding time physiological while making every performed body
/// action twice as fast and the whole pre-sleep landing chain four times faster.
#[must_use]
pub fn phase_clock_scale(program: BehaviorProgramId, phase_name: &str) -> f32 {
    use BehaviorProgramId as P;
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
    let performed_dt = dt * phase_clock_scale(active.program, spec.name);
    active.phase_time += performed_dt;
    active.total_time += performed_dt;
    active.minimum_readability_reached |= active.phase_time >= spec.minimum_seconds;
    let evidence_complete = phase_evidence_complete(active, spec.name, context);
    if active.phase_time < spec.minimum_seconds
        || (!evidence_complete && active.phase_time < spec.maximum_seconds)
    {
        return PhaseAdvance::Hold;
    }
    if usize::from(active.phase.index) + 1 >= definition.phases.len() {
        return PhaseAdvance::Finished(super::completion_from_context(active.program, context));
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
    let program = active.program;
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
