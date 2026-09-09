use glam::Vec2;
use lifecore::{ActionId, BehaviorGoalFrame, EmbodiedGestureKind};

use crate::{
    BehaviorContextFrame, BehaviorProgramId, BehaviorTarget, CompletionReason, MotorCause,
    MotorPriority, MotorWorldEvent, MotorWorldGoal, PROGRAM_COUNT, definition, rank_surface,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProgramDecision {
    pub program: BehaviorProgramId,
    pub cause: MotorCause,
    pub priority: MotorPriority,
}

#[must_use]
pub fn choose_program(
    goal: &BehaviorGoalFrame,
    context: &BehaviorContextFrame,
    active_program: Option<BehaviorProgramId>,
    active_readable: bool,
    cooldowns: &[f32; PROGRAM_COUNT],
) -> Option<ProgramDecision> {
    use BehaviorProgramId as P;
    let eligible = |program: P| {
        if program.requests_user_attention()
            && (context.focus_mode || context.boundary_violation >= 0.18)
        {
            return false;
        }
        cooldowns[program.index()] <= 0.0
            || active_program == Some(program)
            // Support acquisition is one continuous behavior.  A completed
            // readable approach/landing bout may be repeated immediately
            // while Sleep is committed; a generic cooldown must not create a
            // locomotion gap before physical support is actually established.
            || (goal.action == ActionId::Sleep
                && matches!(
                    program,
                    P::RestSurfaceRoostSearch
                        | P::RestLandingSoftTouchdown
                        | P::RestSitSettle
                        | P::RestNremSleep
                ))
    };

    let critical_safety_threat = goal.drives.safety > 0.65;
    let physiological_threat = goal.affect.stress > 0.58;
    let external_cursor_speed = context.cursor_velocity.length();
    let touch_response = match context.gesture {
        EmbodiedGestureKind::PullAndRelease
            if !context.gesture_ended && context.gesture_confidence > 0.44 =>
        {
            Some((P::TouchPullReleaseRebound, MotorCause::UserGesture))
        }
        EmbodiedGestureKind::Hold
            if !context.gesture_ended && context.gesture_confidence > 0.44 =>
        {
            Some((P::TouchSustainedHoldRelaxOrResist, MotorCause::UserGesture))
        }
        EmbodiedGestureKind::SlowStretch
            if !context.gesture_ended && context.gesture_confidence > 0.44 =>
        {
            let program = if goal.felt.contact_pleasantness > 0.55 {
                P::TouchLeanIntoStroke
            } else {
                P::TouchStrokeFollow
            };
            Some((program, MotorCause::UserGesture))
        }
        EmbodiedGestureKind::Tickle
            if !context.gesture_ended && context.gesture_confidence > 0.42 =>
        {
            Some((P::TouchTickleWriggle, MotorCause::UserGesture))
        }
        EmbodiedGestureKind::RhythmicTouch
            if !context.gesture_ended && context.gesture_confidence > 0.42 =>
        {
            Some((P::TouchRhythmicTouchSync, MotorCause::UserGesture))
        }
        EmbodiedGestureKind::CircularTwist
            if !context.gesture_ended && context.gesture_confidence > 0.46 =>
        {
            Some((P::TouchCircularStirCooperate, MotorCause::UserGesture))
        }
        EmbodiedGestureKind::SharedPlayInvitation
            if !context.gesture_ended && context.gesture_confidence > 0.48 =>
        {
            Some((P::PlayPlayBowAnalog, MotorCause::UserGesture))
        }
        EmbodiedGestureKind::SoftTouch
            if !context.gesture_ended && context.gesture_confidence > 0.36 =>
        {
            Some((P::TouchSoftTouchYield, MotorCause::UserGesture))
        }
        EmbodiedGestureKind::FragmentHelp
            if !context.gesture_ended && context.gesture_confidence > 0.40 =>
        {
            Some((P::DefenseFragmentTrackAndRemerge, MotorCause::UserGesture))
        }
        // Before a stationary contact has lasted long enough to become a
        // hold, it is a soft touch. This measured fallback keeps deformation
        // from turning a motionless contact into a synthetic pull.
        _ if context.pointer_down
            && context.body.contact.contact_count > 0
            && context.body.contact.duration < 0.45
            && external_cursor_speed < 0.05 =>
        {
            Some((P::TouchSoftTouchYield, MotorCause::UserGesture))
        }
        // A deformable body can make a stationary hold resemble a slow
        // stretch to the categorical classifier. Preserve the causal
        // motor response from measured contact evidence rather than
        // injecting a gesture label or lowering global confidence gates.
        _ if context.pointer_down
            && context.body.contact.contact_count > 0
            && context.body.contact.duration >= 0.45
            && external_cursor_speed < 0.05 =>
        {
            Some((P::TouchSustainedHoldRelaxOrResist, MotorCause::UserGesture))
        }
        _ if context.pointer_down
            && context.pet_dragged
            && context.body.contact.contact_count > 0
            && context.body.contact.duration >= 0.12
            && external_cursor_speed >= 0.05 =>
        {
            Some((P::TouchPullReleaseRebound, MotorCause::UserGesture))
        }
        _ => None,
    };
    let interrupt = if context.body.topology.connected_components > 1
        || context.body.topology.detached_mass_fraction > 0.01
    {
        Some((P::DefenseFragmentTrackAndRemerge, MotorCause::BodyIntegrity))
    } else if context.gesture == EmbodiedGestureKind::FragmentSeparationAttempt
        && !context.gesture_ended
        && context.gesture_confidence > 0.44
    {
        Some((P::DefenseSafeFragmentDetach, MotorCause::UserGesture))
    } else if (context.gesture == EmbodiedGestureKind::SharpFlick
        && !context.gesture_ended
        && context.gesture_confidence > 0.40)
        || goal.felt.startle > 0.55
    {
        Some((P::DefenseStartleOrientFreeze, MotorCause::BodyIntegrity))
    } else if goal.felt.pain_like > 0.24 {
        Some((P::DefenseLocalPainGuard, MotorCause::BodyIntegrity))
    } else if critical_safety_threat {
        Some((P::DefenseThreatHardenCompact, MotorCause::BodyIntegrity))
    } else if let Some(touch_response) = touch_response {
        // Pressure alone cannot bypass the sustained-hold program: that
        // program owns the readable relax-versus-brace decision.  True
        // loss of topology and a critical safety drive still preempt it above.
        Some(touch_response)
    } else if context.boundary_violation > 0.18 {
        Some((P::DefenseOverpressureBoundary, MotorCause::BodyIntegrity))
    } else if physiological_threat {
        Some((P::DefenseThreatHardenCompact, MotorCause::BodyIntegrity))
    } else {
        None
    };
    if let Some((program, cause)) = interrupt
        && (cause == MotorCause::BodyIntegrity || eligible(program))
    {
        return Some(ProgramDecision {
            program,
            cause,
            priority: definition(program).priority,
        });
    }

    if active_program.is_some() && !active_readable {
        return None;
    }

    let world_program = match (context.world_event, context.world_goal) {
        (_, MotorWorldGoal::OfferOrb) => Some(P::PlayOrbCarryOffer),
        (
            MotorWorldEvent::DenFieldEntered
            | MotorWorldEvent::OrbCaptureStarted
            | MotorWorldEvent::OrbCaptureAcceleration
            | MotorWorldEvent::OrbStored
            | MotorWorldEvent::CaptureFailed,
            _,
        )
        | (
            _,
            MotorWorldGoal::ReturnHome
            | MotorWorldGoal::CarryOrbHome
            | MotorWorldGoal::ReturnOrb
            | MotorWorldGoal::RetrieveOrb,
        ) => Some(P::HomeDenReturnEscort),
        // Once LifeCore has actually selected Sleep, the rest controller owns
        // the approach, landing, support verification, and sleep stages.  The
        // den goal remains useful while travelling home, but must not keep
        // restarting the generic escort program after the sleep action begins.
        (_, MotorWorldGoal::SleepInDen) if goal.action != ActionId::Sleep => {
            Some(P::HomeDenReturnEscort)
        }
        _ => None,
    };
    if let Some(program) = world_program
        && eligible(program)
    {
        return Some(ProgramDecision {
            program,
            cause: MotorCause::WorldEvent,
            priority: definition(program).priority,
        });
    }

    if matches!(goal.action, ActionId::Sleep | ActionId::LandOnWindow) {
        let program = if goal.action == ActionId::Sleep && context.has_bottom_screen_edge() {
            // Bottom-edge sleep is one measured physical sequence:
            // fast approach (>8 px) -> soft landing (<=8 px) -> 300 ms real
            // contact dwell -> short loaded settle -> NREM. The self-generated
            // PBF support constraint is deliberately not an acceptance signal.
            if context.screen_edge_supported {
                if context.screen_edge_support_stable_seconds < 0.90 {
                    P::RestSitSettle
                } else {
                    P::RestNremSleep
                }
            } else if context.screen_edge_gap_px <= 8.0 {
                P::RestLandingSoftTouchdown
            } else {
                P::RestSurfaceRoostSearch
            }
        } else if context.somatic.supported {
            if goal.action == ActionId::LandOnWindow && goal.felt.physical_load > 0.60 {
                P::RestLeanRest
            } else if goal.action == ActionId::LandOnWindow && goal.felt.balance < 0.48 {
                P::RestNestAdjust
            } else {
                P::RestSitSettle
            }
        } else if context.somatic.contact_fraction >= 0.16 {
            P::RestSitSettle
        } else if let Some(surface) = rank_surface(
            context,
            goal.action == ActionId::Sleep || goal.drives.sleep > 0.42,
            false,
        ) {
            let support_point = BehaviorTarget::Surface(surface.clone())
                .world_position()
                .unwrap_or(surface.anchor_point)
                .clamp(Vec2::ZERO, Vec2::ONE);
            if support_point.distance(context.body.motion.world_position) > 0.045 {
                P::RestSurfaceRoostSearch
            } else {
                P::RestLandingSoftTouchdown
            }
        } else {
            P::HomeLowEnergyRecharge
        };
        if eligible(program) {
            return Some(ProgramDecision {
                program,
                cause: if matches!(program, P::RestLandingSoftTouchdown | P::RestSitSettle) {
                    MotorCause::SurfaceContact
                } else {
                    MotorCause::BrainAction
                },
                priority: definition(program).priority,
            });
        }
    }

    if context.world_goal == crate::MotorWorldGoal::PetMore {
        let program = if context.pet_touched {
            P::SocialRubNuzzleCursor
        } else {
            P::SocialPettingSolicitation
        };
        if eligible(program) {
            return Some(ProgramDecision {
                program,
                cause: MotorCause::WorldEvent,
                priority: definition(program).priority,
            });
        }
    }
    if goal.action == ActionId::InvitePetting {
        let program =
            if context.body.contact.duration > 0.10 && goal.felt.contact_pleasantness > 0.42 {
                P::SocialRubNuzzleCursor
            } else if goal.attachment > 0.62 {
                P::SocialPresentTouchSide
            } else {
                P::SocialPettingSolicitation
            };
        if eligible(program) {
            return Some(ProgramDecision {
                program,
                cause: MotorCause::BrainAction,
                priority: definition(program).priority,
            });
        }
    }

    // Catalog variants share family-level controllers. Selection stays
    // semantic and deterministic: LifeCore still chooses the ActionId, while
    // this layer chooses an appropriate performance variant from current
    // embodied state without per-program visual calibration.
    let default_variant = match goal.action {
        ActionId::IdleHover
            if context.den_familiarity > 0.74 && goal.felt.sleep_pressure > 0.52 =>
        {
            Some(P::HomeDenNestRest)
        }
        ActionId::IdleHover
            if context.edible_position.is_some()
                && goal.drives.comfort > 0.64
                && goal.affect.valence > 0.20 =>
        {
            Some(P::HomeFoodAcceptTransport)
        }
        ActionId::IdleHover
            if context.edible_position.is_some()
                && goal.drives.comfort > 0.64
                && goal.affect.valence < -0.16 =>
        {
            Some(P::HomeFoodRefusePushAway)
        }
        ActionId::IdleHover if context.edible_position.is_some() && goal.drives.comfort > 0.52 => {
            Some(P::HomeFoodInspectSample)
        }
        ActionId::IdleHover
            if context.world_event == MotorWorldEvent::FoodConsumed
                && goal.drives.comfort < 0.28
                && goal.felt.relief > 0.54 =>
        {
            Some(P::HomeDigestionSatiation)
        }
        ActionId::IdleHover
            if goal.felt.body_ownership < 0.54 && goal.felt.body_integrity > 0.82 =>
        {
            Some(P::StateSelfGroomRealign)
        }
        ActionId::IdleHover if goal.felt.sleep_pressure > 0.62 && goal.felt.activation < 0.42 => {
            Some(P::RestDrowsyYawn)
        }
        ActionId::IdleHover if goal.drives.comfort > 0.68 && goal.felt.activation < 0.44 => {
            Some(P::HomeHungerSearchBid)
        }
        ActionId::IdleHover if goal.felt.boredom > 0.58 => Some(P::StateBoredFidgetSelfStim),
        ActionId::IdleHover if goal.derived.curiosity > 0.68 => Some(P::StateCuriousProbeBud),
        ActionId::IdleHover if goal.affect.valence < -0.16 => Some(P::StateSadHeavySag),
        ActionId::ObserveCursor if context.selected_salience > 0.70 => {
            Some(P::MoveInspectPauseScan)
        }
        ActionId::ObserveCursor => Some(P::MoveCuriosityArcApproach),
        ActionId::ObserveUserActivity if goal.attachment > 0.44 => {
            Some(P::SocialQuietCompanionship)
        }
        ActionId::ObserveUserActivity => Some(P::MoveCheckBackSocialReference),
        ActionId::ApproachCursor if goal.affect.stress > 0.24 => Some(P::MoveCautiousApproach),
        ActionId::ApproachCursor if goal.attachment > 0.52 => Some(P::SocialGreetingApproach),
        ActionId::ApproachCursor => Some(P::MoveCuriosityArcApproach),
        ActionId::RetreatFromCursor => Some(P::MoveCautiousApproach),
        ActionId::ClingToWindowSide => Some(P::DefenseStrainBraceAndRelease),
        ActionId::WakeUp => Some(P::RestRemDreamWake),
        ActionId::ExploreScreen if goal.affect.arousal > 0.62 => Some(P::MoveExcitedDashOvershoot),
        ActionId::ExploreScreen => Some(P::MoveInspectPauseScan),
        ActionId::PeekFromEdge | ActionId::HideAndSeek => Some(P::PlayHidePeekReveal),
        ActionId::InviteCursorChase if goal.felt.play_readiness > 0.62 => {
            Some(P::PlayChaseInviteFeint)
        }
        ActionId::InviteCursorChase => Some(P::PlayPlayBowAnalog),
        ActionId::PlayCursorChase if goal.felt.effort > 0.55 => Some(P::PlayFakeMissRetry),
        ActionId::PlayCursorChase => Some(P::PlayCursorChaseBout),
        ActionId::BringProceduralOrb if context.orb_position.is_some() && !context.orb_stored => {
            Some(P::PlayOrbCatchEnvelop)
        }
        ActionId::BringProceduralOrb => Some(P::PlayOrbCarryOffer),
        ActionId::MimicClickRhythm => Some(P::TouchRhythmicTouchSync),
        ActionId::SilentStare => Some(P::SocialMutualGazePulse),
        ActionId::HappyDisplay if goal.attachment > 0.42 => Some(P::SocialSlowBlinkAffiliation),
        ActionId::FrustratedRetreat if goal.affect.stress > 0.46 => {
            Some(P::DefensePostStressShakeOff)
        }
        ActionId::FrustratedRetreat => Some(P::StateSadHeavySag),
        ActionId::Chirp => Some(P::SocialAttentionBidWait),
        ActionId::Purr => Some(P::StateSocialPurrCoregulation),
        ActionId::SelfPlay if goal.felt.boredom > 0.52 => Some(P::StateBoredFidgetSelfStim),
        ActionId::SelfPlay => Some(P::PlaySelfPlayDroplet),
        ActionId::Metamorphosis => Some(P::StateEmotionTransitionSettle),
        _ => None,
    };
    if let Some(program) = default_variant
        && eligible(program)
    {
        return Some(ProgramDecision {
            program,
            cause: MotorCause::BrainAction,
            priority: definition(program).priority,
        });
    }

    let target_distance = goal
        .body_intent
        .target_position
        .distance(context.body.motion.world_position);
    let locomoting = goal.body_intent.desired_speed > 0.025 && target_distance > 0.022;
    if locomoting && eligible(P::MovePunctuatedTravel) {
        return Some(ProgramDecision {
            program: P::MovePunctuatedTravel,
            cause: MotorCause::BrainAction,
            priority: definition(P::MovePunctuatedTravel).priority,
        });
    }
    if context.body.motion.velocity.length() > 0.12
        && target_distance <= 0.055
        && eligible(P::MoveBrakeSquashRecover)
    {
        return Some(ProgramDecision {
            program: P::MoveBrakeSquashRecover,
            cause: MotorCause::BrainAction,
            priority: definition(P::MoveBrakeSquashRecover).priority,
        });
    }
    if (context.selected_salience > 0.44
        || matches!(
            goal.action,
            ActionId::ObserveCursor | ActionId::ObserveUserActivity
        ))
        && eligible(P::MoveOrientReflex)
    {
        return Some(ProgramDecision {
            program: P::MoveOrientReflex,
            cause: MotorCause::SalientStimulus,
            priority: definition(P::MoveOrientReflex).priority,
        });
    }
    if (goal.felt.activation < 0.30 || goal.felt.physical_load > 0.56 || goal.drives.sleep > 0.66)
        && eligible(P::HomeLowEnergyRecharge)
    {
        return Some(ProgramDecision {
            program: P::HomeLowEnergyRecharge,
            cause: MotorCause::PhysiologicalTransition,
            priority: definition(P::HomeLowEnergyRecharge).priority,
        });
    }
    if goal.affect.valence > 0.20
        && goal.affect.stress < 0.28
        && goal.affect.arousal < 0.58
        && eligible(P::StateContentedOpenDrift)
    {
        return Some(ProgramDecision {
            program: P::StateContentedOpenDrift,
            cause: MotorCause::PhysiologicalTransition,
            priority: definition(P::StateContentedOpenDrift).priority,
        });
    }
    if goal.affect.arousal > 0.52
        && goal.felt.comfort > 0.48
        && eligible(P::StateRespiratorySighReset)
    {
        return Some(ProgramDecision {
            program: P::StateRespiratorySighReset,
            cause: MotorCause::PhysiologicalTransition,
            priority: definition(P::StateRespiratorySighReset).priority,
        });
    }
    None
}

#[must_use]
pub fn lock_target(
    program: BehaviorProgramId,
    goal: &BehaviorGoalFrame,
    context: &BehaviorContextFrame,
    remembered_surface: Option<&crate::SurfaceTarget>,
) -> Option<BehaviorTarget> {
    use BehaviorProgramId as P;
    match program {
        P::RestSurfaceRoostSearch
        | P::RestLandingSoftTouchdown
        | P::RestSitSettle
        | P::RestNremSleep
        | P::RestRemDreamWake => {
            if goal.action == ActionId::Sleep {
                rank_surface(context, true, false).map(BehaviorTarget::Surface)
            } else {
                remembered_surface
                    .cloned()
                    .or_else(|| rank_surface(context, goal.drives.sleep > 0.40, false))
                    .map(BehaviorTarget::Surface)
            }
        }
        P::SocialPettingSolicitation => {
            let offset = context.cursor_position - context.body.motion.world_position;
            let diameter = context.body_diameter.max(Vec2::splat(0.0001));
            let follow = (offset / diameter).clamp_length_max(0.35) * diameter;
            Some(BehaviorTarget::Point(
                (context.body.motion.world_position + follow).clamp(Vec2::ZERO, Vec2::ONE),
            ))
        }
        P::SocialRubNuzzleCursor | P::TouchSoftTouchYield | P::TouchSustainedHoldRelaxOrResist => {
            Some(BehaviorTarget::Cursor(context.cursor_position))
        }
        P::TouchPullReleaseRebound => Some(BehaviorTarget::Cursor(context.cursor_position)),
        P::DefenseFragmentTrackAndRemerge => Some(BehaviorTarget::Component(
            context.body.contact.component_id.unwrap_or(1),
        )),
        P::HomeDenReturnEscort | P::HomeLowEnergyRecharge => context
            .den_anchor
            .map(BehaviorTarget::Den)
            .or(Some(BehaviorTarget::Point(
                goal.body_intent.target_position,
            ))),
        P::MoveOrientReflex => Some(BehaviorTarget::Point(
            goal.body_intent
                .gaze_target
                .unwrap_or(context.cursor_position),
        )),
        P::MovePunctuatedTravel | P::MoveBrakeSquashRecover => {
            Some(BehaviorTarget::Point(goal.body_intent.target_position))
        }
        P::RestLeanRest | P::RestNestAdjust => remembered_surface
            .cloned()
            .or_else(|| rank_surface(context, true, false))
            .map(BehaviorTarget::Surface)
            .or_else(|| context.den_anchor.map(BehaviorTarget::Den)),
        P::MoveCuriosityArcApproach
        | P::MoveCautiousApproach
        | P::MoveExcitedDashOvershoot
        | P::MoveInspectPauseScan
        | P::MoveCheckBackSocialReference => {
            Some(BehaviorTarget::Point(goal.body_intent.target_position))
        }
        P::SocialGreetingApproach
        | P::SocialMutualGazePulse
        | P::SocialSlowBlinkAffiliation
        | P::SocialPresentTouchSide
        | P::SocialQuietCompanionship
        | P::SocialAttentionBidWait => Some(BehaviorTarget::Cursor(context.cursor_position)),
        P::TouchStrokeFollow
        | P::TouchLeanIntoStroke
        | P::TouchTickleWriggle
        | P::TouchRhythmicTouchSync
        | P::TouchCircularStirCooperate => Some(BehaviorTarget::Cursor(context.cursor_position)),
        P::PlayPlayBowAnalog
        | P::PlayChaseInviteFeint
        | P::PlayCursorChaseBout
        | P::PlayFakeMissRetry
        | P::PlayHidePeekReveal => Some(BehaviorTarget::Cursor(context.cursor_position)),
        P::PlayOrbCatchEnvelop | P::PlayOrbCarryOffer | P::PlaySelfPlayDroplet => context
            .orb_position
            .map(BehaviorTarget::Orb)
            .or(Some(BehaviorTarget::Point(
                goal.body_intent.target_position,
            ))),
        P::HomeFoodInspectSample | P::HomeFoodAcceptTransport | P::HomeFoodRefusePushAway => {
            context.edible_position.map(BehaviorTarget::Point)
        }
        P::HomeHungerSearchBid | P::HomeDigestionSatiation | P::HomeDenNestRest => context
            .orb_position
            .map(BehaviorTarget::Orb)
            .or_else(|| context.den_anchor.map(BehaviorTarget::Den))
            .or(Some(BehaviorTarget::Point(
                goal.body_intent.target_position,
            ))),
        P::DefenseSafeFragmentDetach => Some(BehaviorTarget::Component(
            context.body.contact.component_id.unwrap_or(1),
        )),
        P::DefenseStartleOrientFreeze
        | P::DefenseOverpressureBoundary
        | P::DefenseLocalPainGuard
        | P::DefenseStrainBraceAndRelease
        | P::DefensePostStressShakeOff => None,
        P::StateCuriousProbeBud => Some(BehaviorTarget::Cursor(context.cursor_position)),
        P::StateSadHeavySag
        | P::StateBoredFidgetSelfStim
        | P::StateSocialPurrCoregulation
        | P::StateSelfGroomRealign
        | P::StateEmotionTransitionSettle => None,
        _ => None,
    }
}

#[must_use]
pub fn completion_from_context(
    program: BehaviorProgramId,
    context: &BehaviorContextFrame,
) -> CompletionReason {
    use BehaviorProgramId as P;
    match program {
        P::DefenseFragmentTrackAndRemerge if context.body.topology.connected_components <= 1 => {
            CompletionReason::IntegrityRestored
        }
        P::RestLandingSoftTouchdown | P::RestSitSettle if context.support_confirmed() => {
            CompletionReason::SupportConfirmed
        }
        // Social completion is owned by the fresh contextual bid in advance_phase.
        P::HomeDenReturnEscort if context.world_event == MotorWorldEvent::OrbStored => {
            CompletionReason::GoalReached
        }
        _ => CompletionReason::PhaseComplete,
    }
}
