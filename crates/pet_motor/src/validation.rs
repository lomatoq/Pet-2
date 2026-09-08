use glam::Vec2;
use lifecore::{
    ActionId, AffectState, BehaviorGoalFrame, BodyIntent, DerivedNervousState, Drives,
    EmbodiedGestureKind, FeltStateV1, Genome, LocomotionMode, PoseIntent, SurfaceId,
};
use serde::{Deserialize, Serialize};

use crate::{
    BehaviorContextFrame, BehaviorPerformanceRuntime, BehaviorProgramId, BoundedMaterialActuation,
    CompletionReason, InternalPhysiologyActuation, MotorCause, MotorPoseIntent, PROGRAM_COUNT,
    PhaseAdvance, ProgramFamily, SurfaceCandidate, advance_phase, choose_program, definition,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CatalogValidationRecord {
    pub program: String,
    pub family: ProgramFamily,
    pub phase_count: usize,
    pub selector_reachable: bool,
    pub activation_cause: Option<MotorCause>,
    pub activation_probe: String,
    pub phase_grammar_finished: bool,
    pub completion_reason: CompletionReason,
    pub non_cosmetic_actuation: bool,
    pub causal_trace_present: bool,
    pub maximum_local_fields: usize,
    pub maximum_surface_attachments: usize,
    pub bounded: bool,
    pub passed: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CatalogValidationSummary {
    pub schema_version: String,
    pub catalog_program_count: usize,
    pub passed_program_count: usize,
    pub failed_programs: Vec<String>,
    pub maximum_local_fields_observed: usize,
    pub maximum_surface_attachments_observed: usize,
    pub records: Vec<CatalogValidationRecord>,
}

impl CatalogValidationSummary {
    #[must_use]
    pub fn passed(&self) -> bool {
        self.catalog_program_count == 64
            && self.passed_program_count == self.catalog_program_count
            && self.failed_programs.is_empty()
    }
}

/// Deterministic, renderer-free acceptance sweep for all catalog programs.
/// It validates executable actuation and thresholds; it intentionally does not
/// pretend to replace a human aesthetics review.
#[must_use]
pub fn validate_catalog(seed: u64) -> CatalogValidationSummary {
    let goal = validation_goal();
    let context = validation_context();
    let mut records = Vec::with_capacity(BehaviorProgramId::ALL.len());

    for program in BehaviorProgramId::ALL {
        let (activation_goal, activation_context, activation_probe) = activation_fixture(program);
        let decision = choose_program(
            &activation_goal,
            &activation_context,
            None,
            true,
            &[0.0; PROGRAM_COUNT],
        );
        let selector_reachable = decision.is_some_and(|decision| decision.program == program);
        let activation_cause = decision.map(|decision| decision.cause);
        let mut runtime = BehaviorPerformanceRuntime::new(seed ^ program.index() as u64);
        runtime.start(program, MotorCause::BrainAction, &goal, &context);
        let packet = runtime.tick(&goal, &context, 0.01);
        let non_cosmetic = packet.locomotion.pose != MotorPoseIntent::Neutral
            || packet.locomotion.speed_multiplier != 1.0
            || packet.material != BoundedMaterialActuation::default()
            || packet.internal != InternalPhysiologyActuation::default()
            || packet.field_count() > 0
            || packet.support.is_some();
        let bounded = packet_is_bounded(&packet);
        let causal_trace_present = runtime
            .traces()
            .last()
            .is_some_and(|trace| trace.program == program && !trace.phase_name.is_empty());

        let mut active = runtime.active().expect("validation program starts").clone();
        let mut completion_reason = CompletionReason::None;
        let mut phase_grammar_finished = false;
        for _ in 0..4_096 {
            if let PhaseAdvance::Finished(reason) = advance_phase(&mut active, &context, 0.25) {
                completion_reason = reason;
                phase_grammar_finished = true;
                break;
            }
        }
        let maximum_local_fields = packet.field_count();
        let maximum_surface_attachments = usize::from(packet.support.is_some());
        let passed = selector_reachable
            && non_cosmetic
            && bounded
            && causal_trace_present
            && phase_grammar_finished
            && definition(program).phases.len() == active.phase.index as usize + 1;
        records.push(CatalogValidationRecord {
            program: program.wire_name().to_owned(),
            family: program.family(),
            phase_count: definition(program).phases.len(),
            selector_reachable,
            activation_cause,
            activation_probe: activation_probe.to_owned(),
            phase_grammar_finished,
            completion_reason,
            non_cosmetic_actuation: non_cosmetic,
            causal_trace_present,
            maximum_local_fields,
            maximum_surface_attachments,
            bounded,
            passed,
        });
    }

    let failed_programs = records
        .iter()
        .filter(|record| !record.passed)
        .map(|record| record.program.clone())
        .collect::<Vec<_>>();
    CatalogValidationSummary {
        schema_version: "pet2.motor_catalog_acceptance.v2".to_owned(),
        catalog_program_count: records.len(),
        passed_program_count: records.iter().filter(|record| record.passed).count(),
        failed_programs,
        maximum_local_fields_observed: records
            .iter()
            .map(|record| record.maximum_local_fields)
            .max()
            .unwrap_or(0),
        maximum_surface_attachments_observed: records
            .iter()
            .map(|record| record.maximum_surface_attachments)
            .max()
            .unwrap_or(0),
        records,
    }
}

/// Produces one deterministic just-over-threshold probe for every selector
/// branch. These are semantic/numeric fixtures, not visual calibrations.
fn activation_fixture(
    program: BehaviorProgramId,
) -> (BehaviorGoalFrame, BehaviorContextFrame, &'static str) {
    use BehaviorProgramId as P;

    let mut goal = validation_goal();
    goal.action = ActionId::IdleHover;
    goal.body_intent.target_position = Vec2::splat(0.50);
    goal.body_intent.gaze_target = None;
    goal.body_intent.desired_speed = 0.0;
    goal.drives.sleep = 0.12;
    goal.drives.comfort = 0.12;
    goal.drives.safety = 0.08;
    goal.affect = AffectState::default();
    goal.felt = FeltStateV1 {
        activation: 0.62,
        agency_match: 0.82,
        social_safety: 0.72,
        body_integrity: 1.0,
        body_ownership: 1.0,
        balance: 1.0,
        comfort: 0.68,
        ..FeltStateV1::default()
    };
    goal.derived = DerivedNervousState::default();
    goal.attachment = 0.30;

    let mut context = BehaviorContextFrame::default();
    context.body.motion.world_position = Vec2::splat(0.50);
    context.cursor_position = Vec2::new(0.62, 0.44);
    context.den_anchor = Some(Vec2::new(0.08, 0.80));

    let evidence = match program {
        P::MoveOrientReflex => {
            context.selected_salience = 0.50;
            "selected_salience=0.50 (>0.44)"
        }
        P::MovePunctuatedTravel => {
            goal.body_intent.target_position = Vec2::new(0.80, 0.50);
            goal.body_intent.desired_speed = 0.10;
            "desired_speed=0.10 (>0.025), target_distance=0.30 (>0.022)"
        }
        P::MoveBrakeSquashRecover => {
            goal.body_intent.target_position = Vec2::new(0.53, 0.50);
            context.body.motion.velocity = Vec2::new(0.14, 0.0);
            "velocity=0.14 (>0.12), target_distance=0.03 (<=0.055)"
        }
        P::MoveCuriosityArcApproach => {
            goal.action = ActionId::ObserveCursor;
            context.selected_salience = 0.60;
            "action=observe_cursor, selected_salience=0.60 (<=0.70)"
        }
        P::MoveCautiousApproach => {
            goal.action = ActionId::ApproachCursor;
            goal.affect.stress = 0.30;
            "action=approach_cursor, stress=0.30 (>0.24)"
        }
        P::MoveExcitedDashOvershoot => {
            goal.action = ActionId::ExploreScreen;
            goal.affect.arousal = 0.70;
            "action=explore_screen, arousal=0.70 (>0.62)"
        }
        P::MoveInspectPauseScan => {
            goal.action = ActionId::ExploreScreen;
            "action=explore_screen, arousal<=0.62"
        }
        P::MoveCheckBackSocialReference => {
            goal.action = ActionId::ObserveUserActivity;
            goal.attachment = 0.30;
            "action=observe_user_activity, attachment=0.30 (<=0.44)"
        }
        P::RestSurfaceRoostSearch => {
            sleep_edge_fixture(&mut goal, &mut context, 20.0, 0.0, false);
            "action=sleep, measured_edge_gap=20px (>8px)"
        }
        P::RestLandingSoftTouchdown => {
            sleep_edge_fixture(&mut goal, &mut context, 4.0, 0.0, false);
            "action=sleep, measured_edge_gap=4px (<=8px), support=false"
        }
        P::RestSitSettle => {
            sleep_edge_fixture(&mut goal, &mut context, 1.0, 0.50, true);
            "action=sleep, measured_support=true, dwell=0.50s (<0.90s)"
        }
        P::RestNremSleep => {
            sleep_edge_fixture(&mut goal, &mut context, 1.0, 1.0, true);
            "action=sleep, measured_support=true, dwell=1.00s (>=0.90s)"
        }
        P::RestDrowsyYawn => {
            goal.felt.sleep_pressure = 0.70;
            goal.felt.activation = 0.30;
            "idle, sleep_pressure=0.70 (>0.62), activation=0.30 (<0.42)"
        }
        P::RestRemDreamWake => {
            goal.action = ActionId::WakeUp;
            "action=wake_up"
        }
        P::RestLeanRest => {
            goal.action = ActionId::LandOnWindow;
            goal.felt.physical_load = 0.70;
            context.somatic.supported = true;
            "land_on_window, support=true, physical_load=0.70 (>0.60)"
        }
        P::RestNestAdjust => {
            goal.action = ActionId::LandOnWindow;
            goal.felt.balance = 0.30;
            context.somatic.supported = true;
            "land_on_window, support=true, balance=0.30 (<0.48)"
        }
        P::SocialPettingSolicitation => {
            goal.action = ActionId::InvitePetting;
            "action=invite_petting, no contact, attachment<=0.62"
        }
        P::SocialRubNuzzleCursor => {
            goal.action = ActionId::InvitePetting;
            goal.felt.contact_pleasantness = 0.70;
            context.body.contact.duration = 0.20;
            "invite_petting, contact=0.20s (>0.10), pleasantness=0.70 (>0.42)"
        }
        P::SocialGreetingApproach => {
            goal.action = ActionId::ApproachCursor;
            goal.attachment = 0.70;
            "approach_cursor, attachment=0.70 (>0.52), stress<=0.24"
        }
        P::SocialMutualGazePulse => {
            goal.action = ActionId::SilentStare;
            "action=silent_stare"
        }
        P::SocialSlowBlinkAffiliation => {
            goal.action = ActionId::HappyDisplay;
            goal.attachment = 0.50;
            "action=happy_display, attachment=0.50 (>0.42)"
        }
        P::SocialPresentTouchSide => {
            goal.action = ActionId::InvitePetting;
            goal.attachment = 0.70;
            "invite_petting, no contact, attachment=0.70 (>0.62)"
        }
        P::SocialQuietCompanionship => {
            goal.action = ActionId::ObserveUserActivity;
            goal.attachment = 0.50;
            "observe_user_activity, attachment=0.50 (>0.44)"
        }
        P::SocialAttentionBidWait => {
            goal.action = ActionId::Chirp;
            "action=chirp"
        }
        P::TouchSoftTouchYield => gesture_fixture(
            &mut context,
            EmbodiedGestureKind::SoftTouch,
            "gesture=soft_touch, confidence=0.80 (>0.36)",
        ),
        P::TouchSustainedHoldRelaxOrResist => gesture_fixture(
            &mut context,
            EmbodiedGestureKind::Hold,
            "gesture=hold, confidence=0.80 (>0.44)",
        ),
        P::TouchPullReleaseRebound => gesture_fixture(
            &mut context,
            EmbodiedGestureKind::PullAndRelease,
            "gesture=pull_and_release, confidence=0.80 (>0.44)",
        ),
        P::TouchStrokeFollow => gesture_fixture(
            &mut context,
            EmbodiedGestureKind::SlowStretch,
            "gesture=slow_stretch, confidence=0.80, pleasantness<=0.55",
        ),
        P::TouchLeanIntoStroke => {
            goal.felt.contact_pleasantness = 0.80;
            gesture_fixture(
                &mut context,
                EmbodiedGestureKind::SlowStretch,
                "slow_stretch, confidence=0.80, pleasantness=0.80 (>0.55)",
            )
        }
        P::TouchTickleWriggle => gesture_fixture(
            &mut context,
            EmbodiedGestureKind::Tickle,
            "gesture=tickle, confidence=0.80 (>0.42)",
        ),
        P::TouchRhythmicTouchSync => {
            goal.action = ActionId::MimicClickRhythm;
            gesture_fixture(
                &mut context,
                EmbodiedGestureKind::RhythmicTouch,
                "gesture=rhythmic_touch, confidence=0.80 (>0.42)",
            )
        }
        P::TouchCircularStirCooperate => gesture_fixture(
            &mut context,
            EmbodiedGestureKind::CircularTwist,
            "gesture=circular_twist, confidence=0.80 (>0.46)",
        ),
        P::PlayPlayBowAnalog => {
            goal.action = ActionId::InviteCursorChase;
            goal.felt.play_readiness = 0.40;
            "invite_cursor_chase, play_readiness=0.40 (<=0.62)"
        }
        P::PlayChaseInviteFeint => {
            goal.action = ActionId::InviteCursorChase;
            goal.felt.play_readiness = 0.80;
            "invite_cursor_chase, play_readiness=0.80 (>0.62)"
        }
        P::PlayCursorChaseBout => {
            goal.action = ActionId::PlayCursorChase;
            goal.felt.effort = 0.30;
            "play_cursor_chase, effort=0.30 (<=0.55)"
        }
        P::PlayFakeMissRetry => {
            goal.action = ActionId::PlayCursorChase;
            goal.felt.effort = 0.70;
            "play_cursor_chase, effort=0.70 (>0.55)"
        }
        P::PlayOrbCatchEnvelop => {
            goal.action = ActionId::BringProceduralOrb;
            context.orb_position = Some(Vec2::new(0.66, 0.62));
            "bring_procedural_orb, orb present and not stored"
        }
        P::PlayOrbCarryOffer => {
            goal.action = ActionId::BringProceduralOrb;
            "bring_procedural_orb, no free orb"
        }
        P::PlayHidePeekReveal => {
            goal.action = ActionId::PeekFromEdge;
            "action=peek_from_edge"
        }
        P::PlaySelfPlayDroplet => {
            goal.action = ActionId::SelfPlay;
            goal.felt.boredom = 0.30;
            "self_play, boredom=0.30 (<=0.52)"
        }
        P::HomeHungerSearchBid => {
            goal.drives.comfort = 0.75;
            goal.felt.activation = 0.30;
            "idle, comfort_need=0.75 (>0.68), activation=0.30 (<0.44)"
        }
        P::HomeFoodInspectSample => {
            goal.drives.comfort = 0.60;
            context.edible_position = Some(Vec2::new(0.66, 0.62));
            "idle, edible_morsel present, comfort_need=0.60 (>0.52)"
        }
        P::HomeFoodAcceptTransport => {
            goal.drives.comfort = 0.72;
            goal.affect.valence = 0.30;
            context.edible_position = Some(Vec2::new(0.66, 0.62));
            "morsel present, comfort_need=0.72 (>0.64), valence=0.30 (>0.20)"
        }
        P::HomeFoodRefusePushAway => {
            goal.drives.comfort = 0.72;
            goal.affect.valence = -0.30;
            context.edible_position = Some(Vec2::new(0.66, 0.62));
            "morsel present, comfort_need=0.72, valence=-0.30 (<-0.16)"
        }
        P::HomeDigestionSatiation => {
            context.world_event = crate::MotorWorldEvent::FoodConsumed;
            goal.drives.comfort = 0.20;
            goal.felt.relief = 0.70;
            "idle, comfort_need=0.20 (<0.28), relief=0.70 (>0.54)"
        }
        P::HomeLowEnergyRecharge => {
            goal.felt.activation = 0.20;
            "activation=0.20 (<0.30)"
        }
        P::HomeDenReturnEscort => {
            context.world_goal = crate::MotorWorldGoal::ReturnHome;
            "world_goal=return_home"
        }
        P::HomeDenNestRest => {
            context.den_familiarity = 0.80;
            goal.felt.sleep_pressure = 0.60;
            "den_familiarity=0.80 (>0.74), sleep_pressure=0.60 (>0.52)"
        }
        P::DefenseThreatHardenCompact => {
            goal.drives.safety = 0.70;
            "safety_need=0.70 (>0.65)"
        }
        P::DefenseFragmentTrackAndRemerge => {
            context.body.topology.connected_components = 2;
            "connected_components=2 (>1)"
        }
        P::DefenseStartleOrientFreeze => {
            goal.felt.startle = 0.70;
            "startle=0.70 (>0.55)"
        }
        P::DefenseOverpressureBoundary => {
            context.boundary_violation = 0.30;
            "boundary_violation=0.30 (>0.18)"
        }
        P::DefenseLocalPainGuard => {
            goal.felt.pain_like = 0.30;
            "pain_like=0.30 (>0.24)"
        }
        P::DefenseStrainBraceAndRelease => {
            goal.action = ActionId::ClingToWindowSide;
            "action=cling_to_window_side"
        }
        P::DefenseSafeFragmentDetach => gesture_fixture(
            &mut context,
            EmbodiedGestureKind::FragmentSeparationAttempt,
            "fragment_separation_attempt, confidence=0.80 (>0.44)",
        ),
        P::DefensePostStressShakeOff => {
            goal.action = ActionId::FrustratedRetreat;
            goal.affect.stress = 0.50;
            "frustrated_retreat, stress=0.50 (>0.46 and <=0.58 interrupt)"
        }
        P::StateContentedOpenDrift => {
            goal.affect.valence = 0.40;
            goal.affect.arousal = 0.30;
            "valence=0.40 (>0.20), stress<0.28, arousal=0.30 (<0.58)"
        }
        P::StateRespiratorySighReset => {
            goal.affect.arousal = 0.70;
            goal.felt.comfort = 0.60;
            "arousal=0.70 (>0.52), felt_comfort=0.60 (>0.48)"
        }
        P::StateSadHeavySag => {
            goal.affect.valence = -0.30;
            "idle, valence=-0.30 (<-0.16)"
        }
        P::StateCuriousProbeBud => {
            goal.derived.curiosity = 0.80;
            "idle, derived_curiosity=0.80 (>0.68)"
        }
        P::StateBoredFidgetSelfStim => {
            goal.felt.boredom = 0.70;
            "idle, boredom=0.70 (>0.58)"
        }
        P::StateSocialPurrCoregulation => {
            goal.action = ActionId::Purr;
            "action=purr"
        }
        P::StateSelfGroomRealign => {
            goal.felt.body_ownership = 0.40;
            "body_ownership=0.40 (<0.54), body_integrity>0.82"
        }
        P::StateEmotionTransitionSettle => {
            goal.action = ActionId::Metamorphosis;
            "action=metamorphosis"
        }
    };
    (goal, context, evidence)
}

fn gesture_fixture(
    context: &mut BehaviorContextFrame,
    gesture: EmbodiedGestureKind,
    evidence: &'static str,
) -> &'static str {
    context.gesture = gesture;
    context.gesture_confidence = 0.80;
    context.gesture_ended = false;
    evidence
}

fn sleep_edge_fixture(
    goal: &mut BehaviorGoalFrame,
    context: &mut BehaviorContextFrame,
    gap_px: f32,
    dwell_seconds: f32,
    supported: bool,
) {
    goal.action = ActionId::Sleep;
    context.screen_edge_gap_px = gap_px;
    context.screen_edge_normal_velocity_px_s = 0.0;
    context.screen_edge_support_stable_seconds = dwell_seconds;
    context.screen_edge_supported = supported;
    context.surfaces.push(SurfaceCandidate {
        surface_id: SurfaceId("screen:bottom_edge".into()),
        minimum: Vec2::new(0.0, 0.999),
        maximum: Vec2::ONE,
        velocity: Vec2::ZERO,
        familiarity: 1.0,
        recent_failed_landings: 0,
    });
}

fn validation_goal() -> BehaviorGoalFrame {
    let genome = Genome::from_seed(0x6400_CAFE);
    BehaviorGoalFrame {
        action: ActionId::IdleHover,
        body_intent: BodyIntent {
            locomotion: LocomotionMode::Arrive,
            target_position: Vec2::new(0.72, 0.58),
            target_surface: None,
            desired_speed: 0.30,
            facing_direction: 1.0,
            gaze_target: Some(Vec2::new(0.72, 0.58)),
            pose: PoseIntent::Curious,
            expression: Default::default(),
            interaction_target: None,
        },
        affect: AffectState::default(),
        drives: Drives::initial(&genome.temperament),
        felt: FeltStateV1 {
            activation: 0.62,
            agency_match: 0.82,
            social_safety: 0.72,
            body_integrity: 1.0,
            comfort: 0.68,
            ..FeltStateV1::default()
        },
        derived: DerivedNervousState::default(),
        attachment: 0.42,
        recent_outcome: None,
    }
}

fn validation_context() -> BehaviorContextFrame {
    let mut context = BehaviorContextFrame::default();
    context.body.motion.world_position = Vec2::new(0.50, 0.949);
    context.body_bottom_extent = 0.05;
    context.cursor_position = Vec2::new(0.62, 0.44);
    context.cursor_velocity = Vec2::new(0.12, 0.03);
    context.den_anchor = Some(Vec2::new(0.08, 0.80));
    context.orb_position = Some(Vec2::new(0.66, 0.62));
    context.screen_edge_gap_px = 1.0;
    context.screen_edge_normal_velocity_px_s = 0.0;
    context.screen_edge_support_stable_seconds = 1.0;
    context.screen_edge_supported = true;
    context.somatic.contact_fraction = 0.40;
    context.somatic.support_stability = 1.0;
    context.somatic.supported = true;
    context.pointer_released = true;
    context.gesture_ended = true;
    context.world_event = crate::MotorWorldEvent::OrbStored;
    context.surfaces.push(SurfaceCandidate {
        surface_id: SurfaceId("screen:bottom_edge".into()),
        minimum: Vec2::new(0.0, 0.999),
        maximum: Vec2::ONE,
        velocity: Vec2::ZERO,
        familiarity: 1.0,
        recent_failed_landings: 0,
    });
    context
}

fn packet_is_bounded(packet: &crate::SomaticActuationPacket) -> bool {
    packet.field_count() <= crate::LOCAL_FIELD_BUDGET
        && usize::from(packet.support.is_some()) <= crate::SURFACE_ATTACHMENT_BUDGET
        && (0.0..=1.5).contains(&packet.locomotion.speed_multiplier)
        && (0.0..=1.5).contains(&packet.locomotion.acceleration_limit)
        && (0.60..=1.40).contains(&packet.material.density_compliance_multiplier)
        && (0.65..=1.50).contains(&packet.material.viscosity_multiplier)
        && (0.75..=1.35).contains(&packet.material.surface_tension_multiplier)
        && packet.fields.iter().flatten().all(|field| {
            field.center.is_finite()
                && field.axis.is_finite()
                && (0.04..=1.25).contains(&field.radius)
                && (-1.0..=1.0).contains(&field.strength)
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_catalog_acceptance_passes_64_of_64() {
        let report = validate_catalog(0x6400_2026);
        assert!(report.passed(), "failed: {:?}", report.failed_programs);
        assert!(report.maximum_local_fields_observed <= crate::LOCAL_FIELD_BUDGET);
        assert!(report.maximum_surface_attachments_observed <= 1);
    }
}
