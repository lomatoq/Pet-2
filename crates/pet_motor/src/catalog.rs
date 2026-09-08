use crate::{BehaviorProgramId, InterruptPolicy, MotorPriority};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PhaseSpec {
    pub name: &'static str,
    pub minimum_seconds: f32,
    pub maximum_seconds: f32,
}

impl PhaseSpec {
    pub const fn new(name: &'static str, minimum_seconds: f32, maximum_seconds: f32) -> Self {
        Self {
            name,
            minimum_seconds,
            maximum_seconds,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ProgramDefinition {
    pub id: BehaviorProgramId,
    pub phases: &'static [PhaseSpec],
    pub cooldown_seconds: f32,
    pub priority: MotorPriority,
    pub interrupt_policy: InterruptPolicy,
}

const ORIENT: [PhaseSpec; 4] = [
    PhaseSpec::new("freeze_30_120ms", 0.03, 0.12),
    PhaseSpec::new("eyes_first", 0.04, 0.12),
    PhaseSpec::new("front_axis_turn", 0.05, 0.16),
    PhaseSpec::new("decide", 0.02, 0.08),
];
const TRAVEL: [PhaseSpec; 7] = [
    PhaseSpec::new("orient", 0.08, 0.18),
    PhaseSpec::new("prepare", 0.07, 0.22),
    PhaseSpec::new("accelerate", 0.12, 0.40),
    PhaseSpec::new("coast", 0.10, 0.70),
    PhaseSpec::new("brake", 0.12, 0.35),
    PhaseSpec::new("arrival_pause", 0.25, 0.90),
    PhaseSpec::new("appraise", 0.20, 0.70),
];
const BRAKE: [PhaseSpec; 5] = [
    PhaseSpec::new("anticipate_brake", 0.04, 0.12),
    PhaseSpec::new("leading_squash", 0.06, 0.18),
    PhaseSpec::new("core_catch_up", 0.06, 0.22),
    PhaseSpec::new("rebound", 0.05, 0.20),
    PhaseSpec::new("stillness", 0.06, 0.18),
];
const ROOST: [PhaseSpec; 3] = [
    PhaseSpec::new("orient", 0.08, 0.18),
    PhaseSpec::new("rank_surfaces", 0.05, 0.16),
    PhaseSpec::new("approach_commit", 0.25, 2.20),
];
const LANDING: [PhaseSpec; 6] = [
    PhaseSpec::new("orient", 0.06, 0.14),
    PhaseSpec::new("preload", 0.07, 0.18),
    PhaseSpec::new("decelerate", 0.10, 0.35),
    PhaseSpec::new("first_contact", 0.06, 0.28),
    PhaseSpec::new("load_transfer", 0.18, 0.55),
    PhaseSpec::new("appraise", 0.10, 0.28),
];
const SIT: [PhaseSpec; 5] = [
    PhaseSpec::new("lower_lift", 0.16, 0.42),
    PhaseSpec::new("spread_contact_patch", 0.18, 0.55),
    PhaseSpec::new("anchor", 0.20, 0.65),
    PhaseSpec::new("micro_adjust", 0.18, 0.65),
    PhaseSpec::new("rest_hold", 0.45, 9.0),
];
const YAWN: [PhaseSpec; 5] = [
    PhaseSpec::new("anticipatory_inhale", 0.30, 0.70),
    PhaseSpec::new("whole_body_stretch", 0.35, 0.85),
    PhaseSpec::new("audible_yawn_peak", 0.24, 0.62),
    PhaseSpec::new("slow_exhale", 0.50, 1.20),
    PhaseSpec::new("blink_settle", 0.22, 0.55),
];
const NREM: [PhaseSpec; 3] = [
    PhaseSpec::new("sleep_onset", 1.0, 3.5),
    PhaseSpec::new("nrem_hold", 30.0, 120.0),
    PhaseSpec::new("micro_arousal_or_continue", 0.30, 1.5),
];
const REM: [PhaseSpec; 5] = [
    PhaseSpec::new("rem_onset", 0.50, 1.5),
    PhaseSpec::new("structured_local_twitches", 1.0, 5.0),
    PhaseSpec::new("dream_murmur", 0.40, 2.0),
    PhaseSpec::new("micro_arousal", 0.30, 1.2),
    PhaseSpec::new("wake_stretch_or_nrem", 0.60, 3.0),
];
const SOLICIT: [PhaseSpec; 6] = [
    PhaseSpec::new("orient", 0.10, 0.25),
    PhaseSpec::new("approach", 0.25, 2.0),
    PhaseSpec::new("stop_short", 0.16, 0.45),
    PhaseSpec::new("present_side", 0.22, 0.65),
    PhaseSpec::new("look_wait", 2.5, 3.5),
    PhaseSpec::new("accept_or_withdraw", 0.25, 1.0),
];
const RUB: [PhaseSpec; 5] = [
    PhaseSpec::new("approach", 0.12, 0.45),
    PhaseSpec::new("first_nudge", 0.10, 0.35),
    PhaseSpec::new("tangential_rub", 0.22, 0.80),
    PhaseSpec::new("pause", 0.12, 0.40),
    PhaseSpec::new("second_rub_or_release", 0.18, 0.75),
];
const SOFT_TOUCH: [PhaseSpec; 4] = [
    PhaseSpec::new("contact_detect", 0.02, 0.08),
    PhaseSpec::new("local_yield", 0.06, 0.35),
    PhaseSpec::new("micro_lean", 0.08, 0.55),
    PhaseSpec::new("recovery_or_hold", 0.10, 1.5),
];
const HOLD: [PhaseSpec; 5] = [
    PhaseSpec::new("hold_detect", 0.05, 0.15),
    PhaseSpec::new("classify_safety", 0.08, 0.22),
    PhaseSpec::new("relax_or_brace", 0.15, 0.65),
    PhaseSpec::new("hold", 0.20, 3.0),
    PhaseSpec::new("release_recover", 0.15, 0.60),
];
const PULL: [PhaseSpec; 6] = [
    PhaseSpec::new("tether_form", 0.06, 0.20),
    PhaseSpec::new("controlled_extension", 0.12, 1.2),
    PhaseSpec::new("release_detect", 0.02, 0.15),
    PhaseSpec::new("snap_limited_return", 0.10, 0.35),
    PhaseSpec::new("secondary_wave", 0.12, 0.50),
    PhaseSpec::new("appraise", 0.10, 0.35),
];
const THREAT: [PhaseSpec; 5] = [
    PhaseSpec::new("compact", 0.06, 0.20),
    PhaseSpec::new("increase_tone", 0.08, 0.28),
    PhaseSpec::new("reduce_flow", 0.08, 0.35),
    PhaseSpec::new("hold_escape_axis", 0.15, 5.5),
    PhaseSpec::new("release_gradually", 0.25, 1.5),
];
const REMERGE: [PhaseSpec; 6] = [
    PhaseSpec::new("attend_fragment", 0.08, 0.30),
    PhaseSpec::new("approach_both_sides", 0.18, 2.0),
    PhaseSpec::new("first_contact", 0.08, 0.50),
    PhaseSpec::new("zipper_coalescence", 0.25, 0.60),
    PhaseSpec::new("rebound_wave", 0.10, 0.45),
    PhaseSpec::new("relief", 0.10, 0.40),
];
const CONTENT: [PhaseSpec; 5] = [
    PhaseSpec::new("open_lobes", 0.25, 0.80),
    PhaseSpec::new("slow_internal_rotation", 0.60, 2.5),
    PhaseSpec::new("short_drift_bout", 0.50, 2.5),
    PhaseSpec::new("pause", 0.70, 4.0),
    PhaseSpec::new("recenter", 0.30, 1.2),
];
const SIGH: [PhaseSpec; 4] = [
    PhaseSpec::new("deep_inhale", 0.35, 0.80),
    PhaseSpec::new("brief_hold", 0.12, 0.35),
    PhaseSpec::new("long_exhale", 0.70, 1.8),
    PhaseSpec::new("rhythm_reset", 0.35, 0.90),
];
const RECHARGE: [PhaseSpec; 5] = [
    PhaseSpec::new("reduce_lift", 0.25, 0.80),
    PhaseSpec::new("seek_support", 0.40, 4.0),
    PhaseSpec::new("compact_or_spread_by_temperature", 0.40, 2.0),
    PhaseSpec::new("slow_breathe", 2.0, 20.0),
    PhaseSpec::new("recover", 0.60, 3.0),
];
const DEN: [PhaseSpec; 5] = [
    PhaseSpec::new("look_den", 0.10, 0.30),
    PhaseSpec::new("approach_bouts", 0.40, 6.0),
    PhaseSpec::new("escort_or_carry", 0.40, 5.0),
    PhaseSpec::new("watch_capture", 0.30, 2.5),
    PhaseSpec::new("appraise", 0.25, 1.0),
];
const FAMILY_REST: [PhaseSpec; 5] = [
    PhaseSpec::new("orient_support", 0.10, 0.35),
    PhaseSpec::new("lower_load", 0.16, 0.60),
    PhaseSpec::new("settle_contact", 0.18, 0.75),
    PhaseSpec::new("hold", 0.35, 4.0),
    PhaseSpec::new("recover", 0.16, 0.60),
];
const FAMILY_MOVE: [PhaseSpec; 5] = [
    PhaseSpec::new("orient", 0.06, 0.20),
    PhaseSpec::new("prepare", 0.08, 0.26),
    PhaseSpec::new("perform", 0.16, 1.2),
    PhaseSpec::new("punctuate", 0.10, 0.50),
    PhaseSpec::new("appraise", 0.12, 0.55),
];
const FAMILY_SOCIAL: [PhaseSpec; 5] = [
    PhaseSpec::new("orient_other", 0.08, 0.25),
    PhaseSpec::new("approach_or_present", 0.16, 1.4),
    PhaseSpec::new("signal", 0.16, 0.75),
    PhaseSpec::new("wait", 2.5, 3.5),
    PhaseSpec::new("ack_or_withdraw", 0.16, 0.70),
];
const FAMILY_TOUCH: [PhaseSpec; 5] = [
    PhaseSpec::new("contact_detect", 0.03, 0.12),
    PhaseSpec::new("local_response", 0.08, 0.45),
    PhaseSpec::new("cooperate_or_guard", 0.12, 0.80),
    PhaseSpec::new("hold", 0.16, 1.5),
    PhaseSpec::new("release_recover", 0.12, 0.65),
];
const FAMILY_PLAY: [PhaseSpec; 5] = [
    PhaseSpec::new("invite_or_orient", 0.08, 0.28),
    PhaseSpec::new("prepare", 0.10, 0.38),
    PhaseSpec::new("play_bout", 0.20, 1.8),
    PhaseSpec::new("pause", 0.12, 0.65),
    PhaseSpec::new("retry_or_settle", 0.16, 0.75),
];
const FAMILY_HOME: [PhaseSpec; 5] = [
    PhaseSpec::new("sense_need_or_home", 0.10, 0.35),
    PhaseSpec::new("orient", 0.08, 0.28),
    PhaseSpec::new("approach_or_sample", 0.18, 1.8),
    PhaseSpec::new("consume_carry_or_rest", 0.25, 2.4),
    PhaseSpec::new("appraise", 0.16, 0.70),
];
const FAMILY_DEFENSE: [PhaseSpec; 5] = [
    PhaseSpec::new("detect", 0.03, 0.12),
    PhaseSpec::new("protect", 0.06, 0.28),
    PhaseSpec::new("hold_boundary", 0.12, 1.5),
    PhaseSpec::new("recheck", 0.10, 0.45),
    PhaseSpec::new("release", 0.18, 0.90),
];
const FAMILY_STATE: [PhaseSpec; 5] = [
    PhaseSpec::new("state_onset", 0.12, 0.45),
    PhaseSpec::new("body_reconfigure", 0.16, 0.75),
    PhaseSpec::new("express_hold", 0.25, 2.5),
    PhaseSpec::new("micro_adjust", 0.12, 0.65),
    PhaseSpec::new("settle", 0.20, 0.90),
];

#[must_use]
pub const fn definition(id: BehaviorProgramId) -> ProgramDefinition {
    use BehaviorProgramId as P;
    let (phases, cooldown_seconds, priority, interrupt_policy) = match id {
        P::MoveOrientReflex => (
            ORIENT.as_slice(),
            0.20,
            MotorPriority::Reactive,
            InterruptPolicy::BlendWithGoalWhenSafe,
        ),
        P::MovePunctuatedTravel => (
            TRAVEL.as_slice(),
            0.25,
            MotorPriority::Voluntary,
            InterruptPolicy::FinishReadablePhase,
        ),
        P::MoveBrakeSquashRecover => (
            BRAKE.as_slice(),
            0.20,
            MotorPriority::Reactive,
            InterruptPolicy::BlendWithGoalWhenSafe,
        ),
        P::RestSurfaceRoostSearch => (
            ROOST.as_slice(),
            1.0,
            MotorPriority::Voluntary,
            InterruptPolicy::FinishReadablePhase,
        ),
        P::RestLandingSoftTouchdown => (
            LANDING.as_slice(),
            0.50,
            MotorPriority::Reactive,
            InterruptPolicy::BlendWithGoalWhenSafe,
        ),
        P::RestSitSettle => (
            SIT.as_slice(),
            1.0,
            MotorPriority::Voluntary,
            InterruptPolicy::FinishReadablePhase,
        ),
        P::RestDrowsyYawn => (
            YAWN.as_slice(),
            25.0,
            MotorPriority::Voluntary,
            InterruptPolicy::FinishReadablePhase,
        ),
        P::RestNremSleep => (
            NREM.as_slice(),
            5.0,
            MotorPriority::Background,
            InterruptPolicy::YieldToExplicitGoal,
        ),
        P::RestRemDreamWake => (
            REM.as_slice(),
            20.0,
            MotorPriority::Voluntary,
            InterruptPolicy::FinishReadablePhase,
        ),
        P::SocialPettingSolicitation => (
            SOLICIT.as_slice(),
            25.0,
            MotorPriority::Voluntary,
            InterruptPolicy::FinishReadablePhase,
        ),
        P::SocialRubNuzzleCursor => (
            RUB.as_slice(),
            8.0,
            MotorPriority::Voluntary,
            InterruptPolicy::FinishReadablePhase,
        ),
        P::TouchSoftTouchYield => (
            SOFT_TOUCH.as_slice(),
            0.05,
            MotorPriority::Reactive,
            InterruptPolicy::BlendWithGoalWhenSafe,
        ),
        P::TouchSustainedHoldRelaxOrResist => (
            HOLD.as_slice(),
            0.25,
            MotorPriority::Reactive,
            InterruptPolicy::BlendWithGoalWhenSafe,
        ),
        P::TouchPullReleaseRebound => (
            PULL.as_slice(),
            0.50,
            MotorPriority::Reactive,
            InterruptPolicy::BlendWithGoalWhenSafe,
        ),
        P::DefenseThreatHardenCompact => (
            THREAT.as_slice(),
            0.50,
            MotorPriority::Emergency,
            InterruptPolicy::Immediate,
        ),
        P::DefenseFragmentTrackAndRemerge => (
            REMERGE.as_slice(),
            1.0,
            MotorPriority::Integrity,
            InterruptPolicy::Immediate,
        ),
        P::StateContentedOpenDrift => (
            CONTENT.as_slice(),
            2.0,
            MotorPriority::Background,
            InterruptPolicy::YieldToExplicitGoal,
        ),
        P::StateRespiratorySighReset => (
            SIGH.as_slice(),
            18.0,
            MotorPriority::Background,
            InterruptPolicy::YieldToExplicitGoal,
        ),
        P::HomeLowEnergyRecharge => (
            RECHARGE.as_slice(),
            5.0,
            MotorPriority::Voluntary,
            InterruptPolicy::FinishReadablePhase,
        ),
        P::HomeDenReturnEscort => (
            DEN.as_slice(),
            5.0,
            MotorPriority::Voluntary,
            InterruptPolicy::FinishReadablePhase,
        ),
        P::RestLeanRest | P::RestNestAdjust => (
            FAMILY_REST.as_slice(),
            2.0,
            MotorPriority::Background,
            InterruptPolicy::YieldToExplicitGoal,
        ),
        P::MoveCuriosityArcApproach
        | P::MoveCautiousApproach
        | P::MoveExcitedDashOvershoot
        | P::MoveInspectPauseScan
        | P::MoveCheckBackSocialReference => (
            FAMILY_MOVE.as_slice(),
            1.0,
            MotorPriority::Voluntary,
            InterruptPolicy::FinishReadablePhase,
        ),
        P::SocialGreetingApproach
        | P::SocialMutualGazePulse
        | P::SocialSlowBlinkAffiliation
        | P::SocialPresentTouchSide
        | P::SocialQuietCompanionship
        | P::SocialAttentionBidWait => (
            FAMILY_SOCIAL.as_slice(),
            5.0,
            MotorPriority::Voluntary,
            InterruptPolicy::FinishReadablePhase,
        ),
        P::TouchStrokeFollow
        | P::TouchLeanIntoStroke
        | P::TouchTickleWriggle
        | P::TouchRhythmicTouchSync
        | P::TouchCircularStirCooperate => (
            FAMILY_TOUCH.as_slice(),
            0.8,
            MotorPriority::Reactive,
            InterruptPolicy::BlendWithGoalWhenSafe,
        ),
        P::PlayPlayBowAnalog
        | P::PlayChaseInviteFeint
        | P::PlayCursorChaseBout
        | P::PlayFakeMissRetry
        | P::PlayOrbCatchEnvelop
        | P::PlayOrbCarryOffer
        | P::PlayHidePeekReveal
        | P::PlaySelfPlayDroplet => (
            FAMILY_PLAY.as_slice(),
            3.0,
            MotorPriority::Voluntary,
            InterruptPolicy::FinishReadablePhase,
        ),
        P::HomeHungerSearchBid
        | P::HomeFoodInspectSample
        | P::HomeFoodAcceptTransport
        | P::HomeFoodRefusePushAway
        | P::HomeDigestionSatiation
        | P::HomeDenNestRest => (
            FAMILY_HOME.as_slice(),
            4.0,
            MotorPriority::Voluntary,
            InterruptPolicy::FinishReadablePhase,
        ),
        P::DefenseStartleOrientFreeze
        | P::DefenseOverpressureBoundary
        | P::DefenseLocalPainGuard
        | P::DefenseStrainBraceAndRelease
        | P::DefenseSafeFragmentDetach
        | P::DefensePostStressShakeOff => (
            FAMILY_DEFENSE.as_slice(),
            1.0,
            MotorPriority::Integrity,
            InterruptPolicy::Immediate,
        ),
        P::StateSadHeavySag
        | P::StateCuriousProbeBud
        | P::StateBoredFidgetSelfStim
        | P::StateSocialPurrCoregulation
        | P::StateSelfGroomRealign
        | P::StateEmotionTransitionSettle => (
            FAMILY_STATE.as_slice(),
            2.0,
            MotorPriority::Background,
            InterruptPolicy::YieldToExplicitGoal,
        ),
    };
    ProgramDefinition {
        id,
        phases,
        cooldown_seconds,
        priority,
        interrupt_policy,
    }
}

#[must_use]
pub fn phase_name(id: BehaviorProgramId, phase_index: u8) -> &'static str {
    definition(id)
        .phases
        .get(usize::from(phase_index))
        .map_or("invalid_phase", |phase| phase.name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_p0_program_has_a_bounded_phase_grammar() {
        assert_eq!(BehaviorProgramId::ALL_P0.len(), 30);
        for id in BehaviorProgramId::ALL_P0 {
            let definition = definition(id);
            assert!(!definition.phases.is_empty(), "{}", id.wire_name());
            assert!(definition.phases.iter().all(|phase| {
                !phase.name.is_empty()
                    && phase.minimum_seconds > 0.0
                    && phase.maximum_seconds >= phase.minimum_seconds
            }));
        }
    }

    #[test]
    fn all_64_catalog_programs_have_bounded_phase_grammars() {
        assert_eq!(BehaviorProgramId::ALL.len(), 64);
        for id in BehaviorProgramId::ALL {
            let definition = definition(id);
            assert!(!definition.phases.is_empty(), "{}", id.wire_name());
            assert!(definition.phases.iter().all(|phase| {
                !phase.name.is_empty()
                    && phase.minimum_seconds > 0.0
                    && phase.maximum_seconds >= phase.minimum_seconds
            }));
        }
    }
}
