use glam::Vec2;
use lifecore::{ActionId, BodyIntent, LocomotionMode, PoseIntent};

use crate::{EcologyDecisionTrace, EcologyState, GoalScore, ObjectId, ObjectKind, ObjectLifecycle};

pub const MAX_OBJECT_COMMANDS: usize = 8;
pub const MAX_OUTCOMES: usize = 8;

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EpisodeGoal {
    OfferOrb,
    ChaseOrb,
    InterceptOrb,
    RetrieveOrb,
    ReturnOrb,
    CarryOrbHome,
    SoloOrbPlay,
    HideOrb,
    SeekOrb,
    ReturnHome,
    ExitDen,
    SleepInDen,
    PeekFromDen,
    InspectWindow,
    RideWindow,
    EscapePressure,
    RecoverAfterPressure,
    InspectMorsel,
    EatMorsel,
    RefuseMorsel,
    StoreMorsel,
    SharedAttention,
    ChromaticEcho,
    Camouflage,
    PracticeSkill,
    PerformSkill,
    RhythmEcho,
}

impl EpisodeGoal {
    pub const ALL: [Self; 27] = [
        Self::OfferOrb,
        Self::ChaseOrb,
        Self::InterceptOrb,
        Self::RetrieveOrb,
        Self::ReturnOrb,
        Self::CarryOrbHome,
        Self::SoloOrbPlay,
        Self::HideOrb,
        Self::SeekOrb,
        Self::ReturnHome,
        Self::ExitDen,
        Self::SleepInDen,
        Self::PeekFromDen,
        Self::InspectWindow,
        Self::RideWindow,
        Self::EscapePressure,
        Self::RecoverAfterPressure,
        Self::InspectMorsel,
        Self::EatMorsel,
        Self::RefuseMorsel,
        Self::StoreMorsel,
        Self::SharedAttention,
        Self::ChromaticEcho,
        Self::Camouflage,
        Self::PracticeSkill,
        Self::PerformSkill,
        Self::RhythmEcho,
    ];

    #[must_use]
    pub const fn index(self) -> usize {
        self as usize
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EpisodePhase {
    Orient,
    Approach,
    Inspect,
    Prepare,
    Manipulate,
    WaitForUser,
    Execute,
    Evaluate,
    Retry,
    AskForHelp,
    Celebrate,
    Recover,
    ReturnHome,
    Complete,
    Aborted,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum EpisodeReason {
    #[default]
    NoEligibleEpisode,
    ContinueCommitment,
    BrainRequestedOrb,
    ObjectNovelty,
    UserEngaged,
    ReturnToDen,
    FocusModeRetreat,
    WindowPressure,
    TrappedObject,
    FoodOpportunity,
    SharedAttentionCue,
    PracticeDue,
    ExplicitTeachMode,
    SafetyAbort,
    TimedOut,
    InterruptedByShutdown,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ExpectedOutcome {
    #[default]
    None,
    UserTouchesObject,
    ObjectMoves,
    ObjectReturnsHome,
    PressureFalls,
    MorselAccepted,
    MorselRefused,
    AttentionShared,
    SkillReproduced,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ActivityEpisode {
    pub id: u64,
    pub goal: EpisodeGoal,
    pub phase: EpisodePhase,
    pub object_id: Option<ObjectId>,
    pub target_position: Option<Vec2>,
    pub reason_code: EpisodeReason,
    pub elapsed_seconds: f32,
    pub phase_elapsed_seconds: f32,
    pub commitment_remaining: f32,
    pub attempts: u8,
    pub prediction_confidence: f32,
    pub expected_outcome: ExpectedOutcome,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum ObjectCommand {
    #[default]
    None,
    ApplyImpulse {
        object_id: ObjectId,
        impulse: Vec2,
    },
    MoveToward {
        object_id: ObjectId,
        target: Vec2,
        speed: f32,
    },
    Release {
        object_id: ObjectId,
        velocity: Vec2,
    },
    Store {
        object_id: ObjectId,
        slot: u8,
    },
    Retrieve {
        object_id: ObjectId,
        target: Vec2,
    },
    Consume {
        object_id: ObjectId,
    },
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct EcologyVisualContext {
    pub orb_position: Option<Vec2>,
    pub den_anchor: Option<Vec2>,
    pub active_target: Option<Vec2>,
    pub episode_energy: f32,
    pub chromatic_hue: f32,
    pub chromatic_blend: f32,
    pub camouflage_blend: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EcologyVocalTrigger {
    Offer,
    Help,
    Retrieve,
    FoodInspect,
    FoodAccepted,
    FoodRefused,
    SkillAttempt,
    SkillSuccess,
    RhythmEcho,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum EcologyOutcome {
    #[default]
    None,
    EpisodeStarted(EpisodeGoal),
    EpisodeCompleted(EpisodeGoal),
    EpisodeAborted(EpisodeGoal, EpisodeReason),
    ObjectContact(ObjectId),
    ObjectStored(ObjectId),
    MorselConsumed(ObjectId),
    SkillMotorError {
        skill_id: u64,
        error: f32,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct EcologyOutput {
    pub body_intent: BodyIntent,
    pub object_commands: [ObjectCommand; MAX_OBJECT_COMMANDS],
    pub object_command_count: usize,
    pub visual_context: EcologyVisualContext,
    pub vocal_trigger: Option<EcologyVocalTrigger>,
    pub outcomes: [EcologyOutcome; MAX_OUTCOMES],
    pub outcome_count: usize,
    pub debug: EcologyDecisionTrace,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EcologyBehaviorFrame {
    pub selected_action: ActionId,
    pub pet_position: Vec2,
    pub pet_velocity: Vec2,
    pub cursor_position: Vec2,
    pub pointer_down: bool,
    pub user_activity: f32,
    pub focus_mode: bool,
    pub sleeping: bool,
    pub timestamp: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EpisodeStep {
    Continue,
    Complete,
    Abort(EpisodeReason),
}

/// The sole ecology behavior writer. With no active episode this boundary is a
/// strict pass-through and therefore preserves every existing brain mode.
#[derive(Clone, Debug, Default)]
pub struct EpisodeDirector {
    active: Option<ActivityEpisode>,
    tick: u64,
}

impl EpisodeDirector {
    #[must_use]
    pub const fn active_episode(&self) -> Option<&ActivityEpisode> {
        self.active.as_ref()
    }

    pub fn interrupt_for_shutdown(&mut self) -> Option<ActivityEpisode> {
        self.active.take().map(|mut episode| {
            episode.phase = EpisodePhase::Aborted;
            episode.reason_code = EpisodeReason::InterruptedByShutdown;
            episode
        })
    }

    /// Inactive Wave-0 seam. This function performs no heap allocation and
    /// returns the supplied intent unchanged, field for field.
    #[must_use]
    pub fn tick_passthrough(
        &mut self,
        brain_intent: BodyIntent,
        focus_mode: bool,
    ) -> EcologyOutput {
        self.tick = self.tick.saturating_add(1);
        let mut debug = EcologyDecisionTrace {
            tick: self.tick,
            focus_mode_filtered: focus_mode,
            ..EcologyDecisionTrace::default()
        };
        if let Some(active) = self.active {
            debug.active_goal = Some(active.goal);
            debug.active_phase = Some(active.phase);
            debug.selected_reason = EpisodeReason::ContinueCommitment;
        }
        EcologyOutput {
            body_intent: brain_intent,
            object_commands: [ObjectCommand::None; MAX_OBJECT_COMMANDS],
            object_command_count: 0,
            visual_context: EcologyVisualContext::default(),
            vocal_trigger: None,
            outcomes: [EcologyOutcome::None; MAX_OUTCOMES],
            outcome_count: 0,
            debug,
        }
    }

    #[must_use]
    pub fn tick(
        &mut self,
        state: &mut EcologyState,
        frame: EcologyBehaviorFrame,
        brain_intent: BodyIntent,
        dt: f32,
    ) -> EcologyOutput {
        self.tick = self.tick.saturating_add(1);
        let dt = if dt.is_finite() {
            dt.clamp(0.0, 0.25)
        } else {
            0.0
        };
        let mut output = empty_output(brain_intent);
        output.debug.tick = self.tick;
        output.debug.focus_mode_filtered = frame.focus_mode;
        fill_candidate_trace(&mut output.debug, state, frame);

        if frame.focus_mode
            && self.active.is_some_and(|episode| {
                !matches!(
                    episode.goal,
                    EpisodeGoal::ReturnHome | EpisodeGoal::SleepInDen
                )
            })
            && let Some(interrupted) = self.active.take()
        {
            state.episode_stats.aborted[interrupted.goal.index()] =
                state.episode_stats.aborted[interrupted.goal.index()].saturating_add(1);
            push_outcome(
                &mut output,
                EcologyOutcome::EpisodeAborted(interrupted.goal, EpisodeReason::FocusModeRetreat),
            );
        }

        if self.active.is_none()
            && let Some((goal, reason, object_id)) = select_episode(state, frame)
        {
            let id = state.episode_stats.next_episode_id;
            state.episode_stats.next_episode_id = id.saturating_add(1).max(1);
            state.episode_stats.started[goal.index()] =
                state.episode_stats.started[goal.index()].saturating_add(1);
            self.active = Some(ActivityEpisode {
                id,
                goal,
                phase: EpisodePhase::Orient,
                object_id,
                target_position: None,
                reason_code: reason,
                elapsed_seconds: 0.0,
                phase_elapsed_seconds: 0.0,
                commitment_remaining: commitment_for(goal),
                attempts: 0,
                prediction_confidence: 0.52,
                expected_outcome: expected_outcome_for(goal),
            });
            push_outcome(&mut output, EcologyOutcome::EpisodeStarted(goal));
            output.debug.selected_reason = reason;
        }

        let Some(mut active) = self.active.take() else {
            output.visual_context = visual_context(state, None);
            return output;
        };
        active.elapsed_seconds += dt;
        active.phase_elapsed_seconds += dt;
        active.commitment_remaining = (active.commitment_remaining - dt).max(0.0);
        let step = drive_episode(state, frame, &mut active, &mut output, dt);
        output.debug.active_goal = Some(active.goal);
        output.debug.active_phase = Some(active.phase);
        if output.debug.selected_reason == EpisodeReason::NoEligibleEpisode {
            output.debug.selected_reason = EpisodeReason::ContinueCommitment;
        }
        output.visual_context = visual_context(state, Some(&active));
        match step {
            EpisodeStep::Continue => self.active = Some(active),
            EpisodeStep::Complete => {
                active.phase = EpisodePhase::Complete;
                state.episode_stats.completed[active.goal.index()] =
                    state.episode_stats.completed[active.goal.index()].saturating_add(1);
                push_outcome(&mut output, EcologyOutcome::EpisodeCompleted(active.goal));
            }
            EpisodeStep::Abort(reason) => {
                active.phase = EpisodePhase::Aborted;
                state.episode_stats.aborted[active.goal.index()] =
                    state.episode_stats.aborted[active.goal.index()].saturating_add(1);
                push_outcome(
                    &mut output,
                    EcologyOutcome::EpisodeAborted(active.goal, reason),
                );
            }
        }
        output
    }
}

fn empty_output(body_intent: BodyIntent) -> EcologyOutput {
    EcologyOutput {
        body_intent,
        object_commands: [ObjectCommand::None; MAX_OBJECT_COMMANDS],
        object_command_count: 0,
        visual_context: EcologyVisualContext::default(),
        vocal_trigger: None,
        outcomes: [EcologyOutcome::None; MAX_OUTCOMES],
        outcome_count: 0,
        debug: EcologyDecisionTrace::default(),
    }
}

fn select_episode(
    state: &EcologyState,
    frame: EcologyBehaviorFrame,
) -> Option<(EpisodeGoal, EpisodeReason, Option<ObjectId>)> {
    let orb = state
        .objects
        .iter()
        .find(|object| object.kind == ObjectKind::Orb)?;
    if frame.focus_mode {
        return (frame.pet_position.distance(state.den.anchor) > 0.045).then_some((
            EpisodeGoal::ReturnHome,
            EpisodeReason::FocusModeRetreat,
            None,
        ));
    }
    if frame.sleeping || frame.selected_action == ActionId::Sleep {
        return Some((EpisodeGoal::SleepInDen, EpisodeReason::ReturnToDen, None));
    }
    if orb.lifecycle == ObjectLifecycle::GrabbedByUser {
        return Some((
            EpisodeGoal::ChaseOrb,
            EpisodeReason::UserEngaged,
            Some(orb.id),
        ));
    }
    match frame.selected_action {
        ActionId::BringProceduralOrb => Some((
            if orb.lifecycle == ObjectLifecycle::StoredInDen {
                EpisodeGoal::RetrieveOrb
            } else {
                EpisodeGoal::OfferOrb
            },
            EpisodeReason::BrainRequestedOrb,
            Some(orb.id),
        )),
        ActionId::PlayCursorChase | ActionId::InviteCursorChase => Some((
            EpisodeGoal::ChaseOrb,
            EpisodeReason::UserEngaged,
            Some(orb.id),
        )),
        ActionId::SelfPlay if orb.novelty > 0.12 => Some((
            EpisodeGoal::SoloOrbPlay,
            EpisodeReason::ObjectNovelty,
            Some(orb.id),
        )),
        _ => None,
    }
}

fn fill_candidate_trace(
    trace: &mut EcologyDecisionTrace,
    state: &EcologyState,
    frame: EcologyBehaviorFrame,
) {
    let orb = state
        .objects
        .iter()
        .find(|object| object.kind == ObjectKind::Orb);
    let candidates = [
        (
            EpisodeGoal::ReturnHome,
            if frame.focus_mode { 1.0 } else { 0.0 },
            frame.focus_mode,
            EpisodeReason::FocusModeRetreat,
        ),
        (
            EpisodeGoal::SleepInDen,
            if frame.sleeping { 0.95 } else { 0.0 },
            frame.sleeping,
            EpisodeReason::ReturnToDen,
        ),
        (
            EpisodeGoal::OfferOrb,
            if frame.selected_action == ActionId::BringProceduralOrb {
                0.86
            } else {
                0.0
            },
            orb.is_some() && !frame.focus_mode,
            EpisodeReason::BrainRequestedOrb,
        ),
        (
            EpisodeGoal::ChaseOrb,
            if matches!(
                frame.selected_action,
                ActionId::PlayCursorChase | ActionId::InviteCursorChase
            ) {
                0.78
            } else {
                0.0
            },
            orb.is_some() && !frame.focus_mode,
            EpisodeReason::UserEngaged,
        ),
        (
            EpisodeGoal::SoloOrbPlay,
            if frame.selected_action == ActionId::SelfPlay {
                0.70
            } else {
                0.0
            },
            orb.is_some() && !frame.focus_mode,
            EpisodeReason::ObjectNovelty,
        ),
    ];
    for (index, (goal, score, eligible, reason)) in candidates.into_iter().enumerate() {
        trace.scores[index] = GoalScore {
            goal: Some(goal),
            score,
            eligible,
            reason,
        };
    }
    trace.score_count = candidates.len();
}

fn drive_episode(
    state: &mut EcologyState,
    frame: EcologyBehaviorFrame,
    active: &mut ActivityEpisode,
    output: &mut EcologyOutput,
    dt: f32,
) -> EpisodeStep {
    if !frame.pet_position.is_finite() || !frame.cursor_position.is_finite() {
        return EpisodeStep::Abort(EpisodeReason::SafetyAbort);
    }
    match active.goal {
        EpisodeGoal::ReturnHome | EpisodeGoal::SleepInDen => {
            output.body_intent.target_position = state.den.anchor;
            output.body_intent.gaze_target = Some(state.den.anchor);
            output.body_intent.desired_speed = output.body_intent.desired_speed.max(0.34);
            output.body_intent.locomotion = if active.goal == EpisodeGoal::SleepInDen
                && frame.pet_position.distance(state.den.anchor) <= 0.04
            {
                LocomotionMode::Sleep
            } else {
                LocomotionMode::Arrive
            };
            output.body_intent.pose = if active.goal == EpisodeGoal::SleepInDen {
                PoseIntent::Compact
            } else {
                PoseIntent::Neutral
            };
            if active.goal == EpisodeGoal::ReturnHome
                && frame.pet_position.distance(state.den.anchor) <= 0.035
            {
                state.den.visits = state.den.visits.saturating_add(1);
                state.den.familiarity = (state.den.familiarity + 0.006).clamp(0.0, 1.0);
                return EpisodeStep::Complete;
            }
            if active.goal == EpisodeGoal::SleepInDen
                && frame.pet_position.distance(state.den.anchor) <= 0.04
            {
                state.den.comfort_value = (state.den.comfort_value + dt * 0.002).clamp(0.0, 1.0);
                if !frame.sleeping && active.elapsed_seconds > 0.5 {
                    return EpisodeStep::Complete;
                }
            }
        }
        EpisodeGoal::OfferOrb => {
            let Some(orb) = active
                .object_id
                .and_then(|id| state.objects.iter().find(|object| object.id == id))
            else {
                return EpisodeStep::Abort(EpisodeReason::SafetyAbort);
            };
            if orb.lifecycle == ObjectLifecycle::GrabbedByUser {
                active.phase = EpisodePhase::Celebrate;
                output.body_intent.pose = PoseIntent::Display;
                output.vocal_trigger = Some(EcologyVocalTrigger::Offer);
                return EpisodeStep::Complete;
            }
            match active.phase {
                EpisodePhase::Orient if active.phase_elapsed_seconds >= 0.28 => {
                    set_phase(active, EpisodePhase::Approach);
                }
                EpisodePhase::Approach if frame.pet_position.distance(orb.position) <= 0.055 => {
                    set_phase(active, EpisodePhase::Manipulate);
                }
                EpisodePhase::Manipulate if active.phase_elapsed_seconds >= 0.35 => {
                    set_phase(active, EpisodePhase::WaitForUser);
                    output.vocal_trigger = Some(EcologyVocalTrigger::Offer);
                }
                EpisodePhase::WaitForUser if active.phase_elapsed_seconds >= 5.5 => {
                    state.episode_stats.started[EpisodeGoal::CarryOrbHome.index()] =
                        state.episode_stats.started[EpisodeGoal::CarryOrbHome.index()]
                            .saturating_add(1);
                    active.goal = EpisodeGoal::CarryOrbHome;
                    active.phase = EpisodePhase::ReturnHome;
                    active.phase_elapsed_seconds = 0.0;
                    active.commitment_remaining = commitment_for(EpisodeGoal::CarryOrbHome);
                    active.reason_code = EpisodeReason::TimedOut;
                }
                _ => {}
            }
            output.body_intent.gaze_target = Some(orb.position);
            output.body_intent.pose = PoseIntent::Playful;
            if active.phase == EpisodePhase::Approach {
                output.body_intent.target_position = orb.position;
                output.body_intent.locomotion = LocomotionMode::Arrive;
                output.body_intent.desired_speed = output.body_intent.desired_speed.max(0.42);
            } else if matches!(
                active.phase,
                EpisodePhase::Manipulate | EpisodePhase::WaitForUser
            ) {
                let toward_user = (frame.cursor_position - frame.pet_position).normalize_or_zero();
                let offer_target =
                    (frame.pet_position + toward_user * 0.045).clamp(Vec2::ZERO, Vec2::ONE);
                push_command(
                    output,
                    ObjectCommand::MoveToward {
                        object_id: orb.id,
                        target: offer_target,
                        speed: 0.9,
                    },
                );
                output.body_intent.target_position = frame.pet_position;
                output.body_intent.locomotion = LocomotionMode::Hover;
                output.body_intent.gaze_target = Some(frame.cursor_position);
            }
        }
        EpisodeGoal::ChaseOrb | EpisodeGoal::SoloOrbPlay => {
            let Some(orb) = active
                .object_id
                .and_then(|id| state.objects.iter().find(|object| object.id == id))
            else {
                return EpisodeStep::Abort(EpisodeReason::SafetyAbort);
            };
            output.body_intent.target_position = orb.position;
            output.body_intent.gaze_target = Some(orb.position);
            output.body_intent.locomotion = if active.goal == EpisodeGoal::ChaseOrb {
                LocomotionMode::Seek
            } else {
                LocomotionMode::Orbit
            };
            output.body_intent.pose = PoseIntent::Playful;
            output.body_intent.desired_speed = output.body_intent.desired_speed.max(0.48);
            if frame.pet_position.distance(orb.position) <= 0.048
                && orb.lifecycle != ObjectLifecycle::GrabbedByUser
            {
                let direction = (orb.position - frame.pet_position)
                    .normalize_or_zero()
                    .lerp(Vec2::new(0.22, -0.16), 0.24)
                    .normalize_or_zero();
                push_command(
                    output,
                    ObjectCommand::ApplyImpulse {
                        object_id: orb.id,
                        impulse: direction * (0.15 + frame.user_activity * 0.08),
                    },
                );
                active.attempts = active.attempts.saturating_add(1);
                active.phase = EpisodePhase::Execute;
            }
            if active.elapsed_seconds >= 4.5 || active.attempts >= 3 {
                return EpisodeStep::Complete;
            }
        }
        EpisodeGoal::CarryOrbHome => {
            let Some(orb_id) = active.object_id else {
                return EpisodeStep::Abort(EpisodeReason::SafetyAbort);
            };
            output.body_intent.target_position = state.den.anchor;
            output.body_intent.gaze_target = Some(state.den.anchor);
            output.body_intent.locomotion = LocomotionMode::Arrive;
            output.body_intent.pose = PoseIntent::Compact;
            output.body_intent.desired_speed = output.body_intent.desired_speed.max(0.38);
            push_command(
                output,
                ObjectCommand::MoveToward {
                    object_id: orb_id,
                    target: frame.pet_position,
                    speed: 4.0,
                },
            );
            if frame.pet_position.distance(state.den.anchor) <= 0.04 {
                let slot = state
                    .den
                    .slots
                    .iter()
                    .position(Option::is_none)
                    .unwrap_or(0) as u8;
                push_command(
                    output,
                    ObjectCommand::Store {
                        object_id: orb_id,
                        slot,
                    },
                );
                state.den.visits = state.den.visits.saturating_add(1);
                state.den.familiarity = (state.den.familiarity + 0.012).clamp(0.0, 1.0);
                return EpisodeStep::Complete;
            }
        }
        EpisodeGoal::RetrieveOrb => {
            let Some(orb_id) = active.object_id else {
                return EpisodeStep::Abort(EpisodeReason::SafetyAbort);
            };
            output.body_intent.target_position = state.den.anchor;
            output.body_intent.gaze_target = Some(state.den.anchor);
            output.body_intent.locomotion = LocomotionMode::Arrive;
            output.body_intent.pose = PoseIntent::Curious;
            output.body_intent.desired_speed = output.body_intent.desired_speed.max(0.34);
            if frame.pet_position.distance(state.den.anchor) <= 0.045 {
                let direction = (frame.cursor_position - state.den.anchor).normalize_or_zero();
                let target = (state.den.anchor + direction * 0.055).clamp(Vec2::ZERO, Vec2::ONE);
                push_command(
                    output,
                    ObjectCommand::Retrieve {
                        object_id: orb_id,
                        target,
                    },
                );
                return EpisodeStep::Complete;
            }
        }
        _ => return EpisodeStep::Abort(EpisodeReason::SafetyAbort),
    }
    active.commitment_remaining = active.commitment_remaining.max(dt);
    EpisodeStep::Continue
}

fn set_phase(active: &mut ActivityEpisode, phase: EpisodePhase) {
    active.phase = phase;
    active.phase_elapsed_seconds = 0.0;
}

fn commitment_for(goal: EpisodeGoal) -> f32 {
    match goal {
        EpisodeGoal::OfferOrb => 6.5,
        EpisodeGoal::ChaseOrb | EpisodeGoal::SoloOrbPlay => 4.8,
        EpisodeGoal::CarryOrbHome | EpisodeGoal::RetrieveOrb => 8.0,
        EpisodeGoal::ReturnHome | EpisodeGoal::SleepInDen => 8.0,
        _ => 3.0,
    }
}

fn expected_outcome_for(goal: EpisodeGoal) -> ExpectedOutcome {
    match goal {
        EpisodeGoal::OfferOrb => ExpectedOutcome::UserTouchesObject,
        EpisodeGoal::ChaseOrb | EpisodeGoal::SoloOrbPlay => ExpectedOutcome::ObjectMoves,
        EpisodeGoal::ReturnHome | EpisodeGoal::SleepInDen | EpisodeGoal::CarryOrbHome => {
            ExpectedOutcome::ObjectReturnsHome
        }
        EpisodeGoal::RetrieveOrb => ExpectedOutcome::ObjectMoves,
        _ => ExpectedOutcome::None,
    }
}

fn visual_context(state: &EcologyState, active: Option<&ActivityEpisode>) -> EcologyVisualContext {
    EcologyVisualContext {
        orb_position: state
            .objects
            .iter()
            .find(|object| object.kind == ObjectKind::Orb)
            .map(|object| object.position),
        den_anchor: Some(state.den.anchor),
        active_target: active.and_then(|episode| episode.target_position),
        episode_energy: active.map_or(0.0, |episode| {
            (episode.commitment_remaining / commitment_for(episode.goal)).clamp(0.0, 1.0)
        }),
        ..EcologyVisualContext::default()
    }
}

fn push_command(output: &mut EcologyOutput, command: ObjectCommand) {
    if output.object_command_count < MAX_OBJECT_COMMANDS {
        output.object_commands[output.object_command_count] = command;
        output.object_command_count += 1;
    }
}

fn push_outcome(output: &mut EcologyOutput, outcome: EcologyOutcome) {
    if output.outcome_count < MAX_OUTCOMES {
        output.outcomes[output.outcome_count] = outcome;
        output.outcome_count += 1;
    }
}

#[cfg(test)]
mod tests {
    use lifecore::{
        BodyIntent, ExpressionState, InteractionTarget, LocomotionMode, PoseIntent, SurfaceId,
    };

    use super::*;

    fn representative_intent() -> BodyIntent {
        BodyIntent {
            locomotion: LocomotionMode::Seek,
            target_position: Vec2::new(0.13, 0.87),
            target_surface: Some(SurfaceId("test-surface".into())),
            desired_speed: 0.73,
            facing_direction: -0.4,
            gaze_target: Some(Vec2::new(0.92, 0.08)),
            pose: PoseIntent::Playful,
            expression: ExpressionState::default(),
            interaction_target: Some(InteractionTarget::User),
        }
    }

    #[test]
    fn inactive_director_is_exact_intent_passthrough() {
        let expected = representative_intent();
        let actual = EpisodeDirector::default().tick_passthrough(expected.clone(), false);
        assert_eq!(actual.body_intent, expected);
        assert_eq!(actual.object_command_count, 0);
        assert_eq!(actual.outcome_count, 0);
        assert_eq!(
            actual.debug.selected_reason,
            EpisodeReason::NoEligibleEpisode
        );
    }

    fn behavior_frame(action: ActionId) -> EcologyBehaviorFrame {
        EcologyBehaviorFrame {
            selected_action: action,
            pet_position: Vec2::splat(0.5),
            pet_velocity: Vec2::ZERO,
            cursor_position: Vec2::new(0.72, 0.44),
            pointer_down: false,
            user_activity: 0.5,
            focus_mode: false,
            sleeping: false,
            timestamp: 1.0,
        }
    }

    #[test]
    fn bring_orb_action_starts_a_real_offer_episode() {
        let mut state = EcologyState::new(91);
        let mut director = EpisodeDirector::default();
        let output = director.tick(
            &mut state,
            behavior_frame(ActionId::BringProceduralOrb),
            representative_intent(),
            0.05,
        );
        assert_eq!(
            director.active_episode().unwrap().goal,
            EpisodeGoal::OfferOrb
        );
        assert_eq!(output.debug.active_goal, Some(EpisodeGoal::OfferOrb));
        assert_eq!(
            state.episode_stats.started[EpisodeGoal::OfferOrb.index()],
            1
        );
    }

    #[test]
    fn focus_mode_interrupts_play_and_routes_home() {
        let mut state = EcologyState::new(92);
        let mut director = EpisodeDirector::default();
        let _ = director.tick(
            &mut state,
            behavior_frame(ActionId::SelfPlay),
            representative_intent(),
            0.05,
        );
        let mut focus = behavior_frame(ActionId::IdleHover);
        focus.focus_mode = true;
        focus.pet_position = Vec2::splat(0.5);
        let output = director.tick(&mut state, focus, representative_intent(), 0.05);
        assert_eq!(
            director.active_episode().unwrap().goal,
            EpisodeGoal::ReturnHome
        );
        assert_eq!(output.body_intent.target_position, state.den.anchor);
        assert_eq!(
            state.episode_stats.aborted[EpisodeGoal::SoloOrbPlay.index()],
            1
        );
    }

    #[test]
    fn carried_orb_is_stored_when_pet_reaches_den() {
        let mut state = EcologyState::new(93);
        let orb_id = state.objects[0].id;
        let mut director = EpisodeDirector {
            active: Some(ActivityEpisode {
                id: 1,
                goal: EpisodeGoal::CarryOrbHome,
                phase: EpisodePhase::ReturnHome,
                object_id: Some(orb_id),
                target_position: Some(state.den.anchor),
                reason_code: EpisodeReason::TimedOut,
                elapsed_seconds: 1.0,
                phase_elapsed_seconds: 1.0,
                commitment_remaining: 3.0,
                attempts: 0,
                prediction_confidence: 0.5,
                expected_outcome: ExpectedOutcome::ObjectReturnsHome,
            }),
            tick: 0,
        };
        let mut frame = behavior_frame(ActionId::BringProceduralOrb);
        frame.pet_position = state.den.anchor;
        let output = director.tick(&mut state, frame, representative_intent(), 0.05);
        assert!(output.object_commands[..output.object_command_count]
            .iter()
            .any(|command| matches!(command, ObjectCommand::Store { object_id, .. } if *object_id == orb_id)));
        assert!(director.active_episode().is_none());
    }

    #[test]
    fn stored_orb_selects_retrieve_instead_of_offer() {
        let mut state = EcologyState::new(94);
        let orb_id = state.objects[0].id;
        state.objects[0].lifecycle = ObjectLifecycle::StoredInDen;
        state.objects[0].position = state.den.anchor;
        state.den.slots[0] = Some(orb_id);
        let mut frame = behavior_frame(ActionId::BringProceduralOrb);
        frame.pet_position = state.den.anchor;
        let mut director = EpisodeDirector::default();
        let output = director.tick(&mut state, frame, representative_intent(), 0.05);
        assert!(output.object_commands[..output.object_command_count]
            .iter()
            .any(|command| matches!(command, ObjectCommand::Retrieve { object_id, .. } if *object_id == orb_id)));
        assert_eq!(output.debug.active_goal, Some(EpisodeGoal::RetrieveOrb));
    }
}
