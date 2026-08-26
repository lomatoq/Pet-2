use glam::Vec2;
use lifecore::{ActionId, BodyIntent, LocomotionMode, PoseIntent};

use crate::{
    EcologyDecisionTrace, EcologyState, GoalScore, ObjectId, ObjectKind, ObjectLifecycle,
    evaluate_food_utility, retarget_path,
};

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
    pub window_pressure: f32,
    pub window_escape_direction: Vec2,
    pub nearest_window_edge: Option<Vec2>,
    pub window_motion: f32,
    pub orb_trapped: bool,
    pub visual_target: Option<Vec2>,
    pub visual_hue: f32,
    pub visual_strength: f32,
    pub shared_attention: bool,
    pub click_rhythm: Option<crate::RhythmSignature>,
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
    visual_episode_cooldown: f32,
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
        self.visual_episode_cooldown = (self.visual_episode_cooldown - dt).max(0.0);
        let mut frame = frame;
        if self.visual_episode_cooldown > 0.0 {
            frame.visual_strength = 0.0;
            frame.shared_attention = false;
        }
        let mut output = empty_output(brain_intent);
        output.debug.tick = self.tick;
        output.debug.focus_mode_filtered = frame.focus_mode;
        fill_candidate_trace(&mut output.debug, state, frame);

        if frame.window_pressure >= 0.34
            && self
                .active
                .is_some_and(|episode| episode.goal != EpisodeGoal::EscapePressure)
            && let Some(interrupted) = self.active.take()
        {
            state.episode_stats.aborted[interrupted.goal.index()] =
                state.episode_stats.aborted[interrupted.goal.index()].saturating_add(1);
            push_outcome(
                &mut output,
                EcologyOutcome::EpisodeAborted(interrupted.goal, EpisodeReason::WindowPressure),
            );
        }

        if frame.focus_mode
            && self.active.is_some_and(|episode| {
                !matches!(
                    episode.goal,
                    EpisodeGoal::ReturnHome
                        | EpisodeGoal::SleepInDen
                        | EpisodeGoal::EscapePressure
                        | EpisodeGoal::RecoverAfterPressure
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
            output.visual_context = visual_context(state, None, frame);
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
        output.visual_context = visual_context(state, Some(&active), frame);
        match step {
            EpisodeStep::Continue => self.active = Some(active),
            EpisodeStep::Complete => {
                active.phase = EpisodePhase::Complete;
                state.episode_stats.completed[active.goal.index()] =
                    state.episode_stats.completed[active.goal.index()].saturating_add(1);
                push_outcome(&mut output, EcologyOutcome::EpisodeCompleted(active.goal));
                if matches!(
                    active.goal,
                    EpisodeGoal::SharedAttention
                        | EpisodeGoal::ChromaticEcho
                        | EpisodeGoal::Camouflage
                ) {
                    self.visual_episode_cooldown = 1.5;
                }
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
    if frame.window_pressure >= 0.22 {
        return Some((
            EpisodeGoal::EscapePressure,
            EpisodeReason::WindowPressure,
            None,
        ));
    }
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
    if frame.orb_trapped {
        return Some((
            EpisodeGoal::RetrieveOrb,
            EpisodeReason::TrappedObject,
            Some(orb.id),
        ));
    }
    if frame.selected_action == ActionId::MimicClickRhythm && frame.click_rhythm.is_some() {
        return Some((EpisodeGoal::RhythmEcho, EpisodeReason::UserEngaged, None));
    }
    if frame.shared_attention && frame.visual_target.is_some() {
        return Some((
            EpisodeGoal::SharedAttention,
            EpisodeReason::SharedAttentionCue,
            None,
        ));
    }
    if let Some(morsel) = state.objects.iter().find(|object| {
        object.kind == ObjectKind::Morsel
            && matches!(
                object.lifecycle,
                ObjectLifecycle::Free | ObjectLifecycle::Sleeping
            )
            && object.morsel_profile.is_some()
            && (object.preference >= -0.01
                || frame.timestamp - object.last_interaction_seconds >= 45.0)
    }) {
        return Some((
            EpisodeGoal::InspectMorsel,
            EpisodeReason::FoodOpportunity,
            Some(morsel.id),
        ));
    }
    if orb.lifecycle == ObjectLifecycle::GrabbedByUser {
        return Some((
            EpisodeGoal::ChaseOrb,
            EpisodeReason::UserEngaged,
            Some(orb.id),
        ));
    }
    match frame.selected_action {
        ActionId::HappyDisplay => state
            .skills
            .skills
            .iter()
            .filter(|skill| {
                frame.timestamp - skill.last_used_seconds >= crate::MIN_PRACTICE_COOLDOWN_SECONDS
            })
            .max_by(|left, right| left.competence.total_cmp(&right.competence))
            .map(|skill| {
                (
                    EpisodeGoal::PerformSkill,
                    EpisodeReason::PracticeDue,
                    Some(skill.id),
                )
            }),
        ActionId::HideAndSeek if frame.visual_target.is_some() => Some((
            EpisodeGoal::Camouflage,
            EpisodeReason::SharedAttentionCue,
            None,
        )),
        ActionId::ExploreScreen
            if frame.visual_target.is_some() && frame.visual_strength >= 0.28 =>
        {
            Some((
                EpisodeGoal::ChromaticEcho,
                EpisodeReason::SharedAttentionCue,
                None,
            ))
        }
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
        ActionId::LandOnWindow if frame.nearest_window_edge.is_some() => {
            Some((EpisodeGoal::RideWindow, EpisodeReason::ObjectNovelty, None))
        }
        ActionId::ClingToWindowSide if frame.nearest_window_edge.is_some() => Some((
            EpisodeGoal::InspectWindow,
            EpisodeReason::ObjectNovelty,
            None,
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
    let morsel = state.objects.iter().find(|object| {
        object.kind == ObjectKind::Morsel
            && matches!(
                object.lifecycle,
                ObjectLifecycle::Free | ObjectLifecycle::Sleeping
            )
    });
    let practice_skill = state
        .skills
        .skills
        .iter()
        .filter(|skill| {
            frame.timestamp - skill.last_used_seconds >= crate::MIN_PRACTICE_COOLDOWN_SECONDS
        })
        .max_by(|left, right| left.competence.total_cmp(&right.competence));
    let candidates = [
        (
            EpisodeGoal::EscapePressure,
            frame.window_pressure,
            frame.window_pressure >= 0.22,
            EpisodeReason::WindowPressure,
        ),
        (
            EpisodeGoal::RetrieveOrb,
            if frame.orb_trapped { 0.96 } else { 0.0 },
            frame.orb_trapped,
            EpisodeReason::TrappedObject,
        ),
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
        (
            EpisodeGoal::RideWindow,
            if frame.selected_action == ActionId::LandOnWindow {
                0.74 + frame.window_motion * 0.12
            } else {
                0.0
            },
            frame.nearest_window_edge.is_some() && !frame.focus_mode,
            EpisodeReason::ObjectNovelty,
        ),
        (
            EpisodeGoal::InspectWindow,
            if frame.selected_action == ActionId::ClingToWindowSide {
                0.72
            } else {
                0.0
            },
            frame.nearest_window_edge.is_some() && !frame.focus_mode,
            EpisodeReason::ObjectNovelty,
        ),
        (
            EpisodeGoal::InspectMorsel,
            morsel.map_or(0.0, |object| 0.62 + object.novelty * 0.22),
            morsel.is_some() && !frame.focus_mode && frame.window_pressure < 0.22,
            EpisodeReason::FoodOpportunity,
        ),
        (
            EpisodeGoal::SharedAttention,
            if frame.shared_attention { 0.98 } else { 0.0 },
            frame.shared_attention && frame.visual_target.is_some() && !frame.focus_mode,
            EpisodeReason::SharedAttentionCue,
        ),
        (
            EpisodeGoal::ChromaticEcho,
            frame.visual_strength * 0.66,
            frame.visual_target.is_some()
                && frame.selected_action == ActionId::ExploreScreen
                && !frame.focus_mode,
            EpisodeReason::SharedAttentionCue,
        ),
        (
            EpisodeGoal::Camouflage,
            if frame.selected_action == ActionId::HideAndSeek {
                0.72
            } else {
                0.0
            },
            frame.visual_target.is_some() && !frame.focus_mode,
            EpisodeReason::SharedAttentionCue,
        ),
        (
            EpisodeGoal::PerformSkill,
            practice_skill.map_or(0.0, |skill| 0.52 + skill.competence * 0.30),
            practice_skill.is_some()
                && frame.selected_action == ActionId::HappyDisplay
                && !frame.focus_mode,
            EpisodeReason::PracticeDue,
        ),
        (
            EpisodeGoal::RhythmEcho,
            frame
                .click_rhythm
                .map_or(0.0, |rhythm| 0.48 + rhythm.beat_count as f32 / 9.0 * 0.32),
            frame.click_rhythm.is_some()
                && frame.selected_action == ActionId::MimicClickRhythm
                && !frame.focus_mode,
            EpisodeReason::UserEngaged,
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
        EpisodeGoal::RetrieveOrb if active.reason_code == EpisodeReason::TrappedObject => {
            let Some(orb) = active
                .object_id
                .and_then(|id| state.objects.iter().find(|object| object.id == id))
            else {
                return EpisodeStep::Abort(EpisodeReason::SafetyAbort);
            };
            if !frame.orb_trapped && active.elapsed_seconds > 0.10 {
                return EpisodeStep::Complete;
            }
            output.body_intent.target_position = orb.position;
            output.body_intent.gaze_target = Some(orb.position);
            output.body_intent.pose = PoseIntent::Curious;
            output.body_intent.desired_speed = output.body_intent.desired_speed.max(0.36);
            match active.phase {
                EpisodePhase::Orient if active.phase_elapsed_seconds >= 0.20 => {
                    set_phase(active, EpisodePhase::Approach);
                }
                EpisodePhase::Approach if frame.pet_position.distance(orb.position) <= 0.07 => {
                    set_phase(active, EpisodePhase::Manipulate);
                }
                EpisodePhase::Manipulate => {
                    let direction = if frame.window_escape_direction.length_squared() > 1.0e-6 {
                        frame.window_escape_direction.normalize()
                    } else {
                        (orb.position - frame.pet_position)
                            .normalize_or_zero()
                            .lerp(Vec2::X, 0.35)
                            .normalize_or_zero()
                    };
                    let alternating = if active.attempts.is_multiple_of(2) {
                        direction
                    } else {
                        Vec2::new(direction.y, -direction.x)
                    };
                    push_command(
                        output,
                        ObjectCommand::ApplyImpulse {
                            object_id: orb.id,
                            impulse: alternating * 0.16,
                        },
                    );
                    active.attempts = active.attempts.saturating_add(1);
                    set_phase(active, EpisodePhase::Retry);
                }
                EpisodePhase::Retry if active.phase_elapsed_seconds >= 0.60 => {
                    if active.attempts < 2 {
                        set_phase(active, EpisodePhase::Manipulate);
                    } else if frame.user_activity > 0.02 {
                        set_phase(active, EpisodePhase::AskForHelp);
                        output.vocal_trigger = Some(EcologyVocalTrigger::Help);
                    }
                }
                EpisodePhase::AskForHelp => {
                    output.body_intent.gaze_target = if active.phase_elapsed_seconds % 0.9 < 0.45 {
                        Some(orb.position)
                    } else {
                        Some(frame.cursor_position)
                    };
                }
                _ => {}
            }
            output.body_intent.locomotion =
                if matches!(active.phase, EpisodePhase::Approach | EpisodePhase::Orient) {
                    LocomotionMode::Arrive
                } else {
                    LocomotionMode::Hover
                };
            if active.elapsed_seconds >= 5.0 {
                return EpisodeStep::Abort(EpisodeReason::TimedOut);
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
        EpisodeGoal::EscapePressure => {
            let escape = if frame.window_escape_direction.is_finite()
                && frame.window_escape_direction.length_squared() > 1.0e-6
            {
                frame.window_escape_direction.normalize()
            } else if frame.pet_velocity.length_squared() > 1.0e-6 {
                -frame.pet_velocity.normalize()
            } else {
                Vec2::X
            };
            let target =
                (frame.pet_position + escape * 0.18).clamp(Vec2::splat(0.025), Vec2::splat(0.975));
            active.target_position = Some(target);
            output.body_intent.target_position = target;
            output.body_intent.gaze_target = frame.nearest_window_edge;
            output.body_intent.locomotion = LocomotionMode::Seek;
            output.body_intent.pose = PoseIntent::Compact;
            output.body_intent.desired_speed = output.body_intent.desired_speed.max(0.86);
            if frame.window_pressure <= 0.12 && active.elapsed_seconds >= 0.10 {
                state.episode_stats.completed[EpisodeGoal::EscapePressure.index()] =
                    state.episode_stats.completed[EpisodeGoal::EscapePressure.index()]
                        .saturating_add(1);
                state.episode_stats.started[EpisodeGoal::RecoverAfterPressure.index()] =
                    state.episode_stats.started[EpisodeGoal::RecoverAfterPressure.index()]
                        .saturating_add(1);
                push_outcome(
                    output,
                    EcologyOutcome::EpisodeCompleted(EpisodeGoal::EscapePressure),
                );
                push_outcome(
                    output,
                    EcologyOutcome::EpisodeStarted(EpisodeGoal::RecoverAfterPressure),
                );
                active.goal = EpisodeGoal::RecoverAfterPressure;
                active.reason_code = EpisodeReason::WindowPressure;
                set_phase(active, EpisodePhase::Recover);
                active.commitment_remaining = commitment_for(EpisodeGoal::RecoverAfterPressure);
            }
        }
        EpisodeGoal::RecoverAfterPressure => {
            if frame.window_pressure >= 0.34 {
                active.goal = EpisodeGoal::EscapePressure;
                active.reason_code = EpisodeReason::WindowPressure;
                set_phase(active, EpisodePhase::Orient);
                active.commitment_remaining = commitment_for(EpisodeGoal::EscapePressure);
            } else {
                output.body_intent.target_position = frame.pet_position;
                output.body_intent.locomotion = LocomotionMode::Hover;
                output.body_intent.pose = PoseIntent::Neutral;
                output.body_intent.desired_speed = output.body_intent.desired_speed.min(0.22);
                if active.phase_elapsed_seconds >= 0.65 {
                    return EpisodeStep::Complete;
                }
            }
        }
        EpisodeGoal::InspectWindow | EpisodeGoal::RideWindow => {
            let Some(edge) = frame.nearest_window_edge else {
                return EpisodeStep::Abort(EpisodeReason::SafetyAbort);
            };
            active.target_position = Some(edge);
            output.body_intent.target_position = edge;
            output.body_intent.gaze_target = Some(edge);
            output.body_intent.desired_speed = output.body_intent.desired_speed.max(0.30);
            let near_edge = frame.pet_position.distance(edge) <= 0.045;
            if active.goal == EpisodeGoal::RideWindow {
                output.body_intent.locomotion = if near_edge {
                    LocomotionMode::Landing
                } else {
                    LocomotionMode::SurfaceApproach
                };
                output.body_intent.pose = PoseIntent::Landing;
            } else {
                output.body_intent.locomotion = if near_edge {
                    LocomotionMode::EdgeCling
                } else {
                    LocomotionMode::Arrive
                };
                output.body_intent.pose = if near_edge {
                    PoseIntent::Clinging
                } else {
                    PoseIntent::Curious
                };
            }
            if near_edge && active.elapsed_seconds >= 1.2 {
                return EpisodeStep::Complete;
            }
            if active.elapsed_seconds >= 4.5 {
                return EpisodeStep::Abort(EpisodeReason::TimedOut);
            }
        }
        EpisodeGoal::InspectMorsel => {
            let Some((morsel_position, profile)) = active.object_id.and_then(|id| {
                state
                    .objects
                    .iter()
                    .find(|object| object.id == id && object.kind == ObjectKind::Morsel)
                    .and_then(|object| {
                        object
                            .morsel_profile
                            .clone()
                            .map(|profile| (object.position, profile))
                    })
            }) else {
                return EpisodeStep::Abort(EpisodeReason::SafetyAbort);
            };
            output.body_intent.gaze_target = Some(morsel_position);
            output.body_intent.pose = PoseIntent::Curious;
            match active.phase {
                EpisodePhase::Orient if active.phase_elapsed_seconds >= 0.24 => {
                    set_phase(active, EpisodePhase::Approach);
                }
                EpisodePhase::Approach if frame.pet_position.distance(morsel_position) <= 0.055 => {
                    set_phase(active, EpisodePhase::Inspect);
                    output.vocal_trigger = Some(EcologyVocalTrigger::FoodInspect);
                }
                EpisodePhase::Inspect if active.phase_elapsed_seconds >= 0.55 => {
                    set_phase(active, EpisodePhase::Evaluate);
                }
                EpisodePhase::Evaluate => {
                    let utility = evaluate_food_utility(
                        &state.metabolism,
                        &state.taste,
                        &profile,
                        frame.window_pressure,
                    );
                    let next_goal = if utility.total >= 0.08 {
                        EpisodeGoal::EatMorsel
                    } else if utility.total <= -0.10 {
                        EpisodeGoal::RefuseMorsel
                    } else {
                        EpisodeGoal::StoreMorsel
                    };
                    state.episode_stats.completed[EpisodeGoal::InspectMorsel.index()] =
                        state.episode_stats.completed[EpisodeGoal::InspectMorsel.index()]
                            .saturating_add(1);
                    state.episode_stats.started[next_goal.index()] =
                        state.episode_stats.started[next_goal.index()].saturating_add(1);
                    push_outcome(
                        output,
                        EcologyOutcome::EpisodeCompleted(EpisodeGoal::InspectMorsel),
                    );
                    push_outcome(output, EcologyOutcome::EpisodeStarted(next_goal));
                    active.goal = next_goal;
                    active.reason_code = EpisodeReason::FoodOpportunity;
                    active.expected_outcome = expected_outcome_for(next_goal);
                    active.target_position = Some(morsel_position);
                    set_phase(active, EpisodePhase::Execute);
                    active.commitment_remaining = commitment_for(next_goal);
                }
                _ => {}
            }
            output.body_intent.locomotion = if active.phase == EpisodePhase::Approach {
                output.body_intent.target_position = morsel_position;
                output.body_intent.desired_speed = output.body_intent.desired_speed.max(0.28);
                LocomotionMode::Arrive
            } else {
                LocomotionMode::Hover
            };
            output.visual_context.active_target = Some(morsel_position);
        }
        EpisodeGoal::EatMorsel => {
            let Some((morsel_id, profile)) = active.object_id.and_then(|id| {
                state
                    .objects
                    .iter()
                    .find(|object| object.id == id)
                    .and_then(|object| object.morsel_profile.clone().map(|profile| (id, profile)))
            }) else {
                return EpisodeStep::Abort(EpisodeReason::SafetyAbort);
            };
            state.metabolism.consume(&profile);
            state.taste.learn(
                &profile,
                (0.45 + profile.stimulation * 0.25 - state.metabolism.satiation * 0.12)
                    .clamp(-1.0, 1.0),
                0.04,
            );
            push_command(
                output,
                ObjectCommand::Consume {
                    object_id: morsel_id,
                },
            );
            push_outcome(output, EcologyOutcome::MorselConsumed(morsel_id));
            output.body_intent.pose = PoseIntent::Display;
            output.vocal_trigger = Some(EcologyVocalTrigger::FoodAccepted);
            return EpisodeStep::Complete;
        }
        EpisodeGoal::RefuseMorsel => {
            let Some(morsel) = active
                .object_id
                .and_then(|id| state.objects.iter_mut().find(|object| object.id == id))
            else {
                return EpisodeStep::Abort(EpisodeReason::SafetyAbort);
            };
            morsel.preference = (morsel.preference - 0.18).clamp(-1.0, 1.0);
            morsel.novelty = (morsel.novelty - 0.12).clamp(0.0, 1.0);
            morsel.last_interaction_seconds = frame.timestamp.max(0.0);
            let away = (morsel.position - frame.pet_position)
                .normalize_or_zero()
                .lerp(Vec2::new(0.12, -0.08), 0.25)
                .normalize_or_zero();
            push_command(
                output,
                ObjectCommand::ApplyImpulse {
                    object_id: morsel.id,
                    impulse: away * 0.055,
                },
            );
            output.body_intent.pose = PoseIntent::Neutral;
            output.body_intent.gaze_target = Some(frame.cursor_position);
            output.vocal_trigger = Some(EcologyVocalTrigger::FoodRefused);
            return EpisodeStep::Complete;
        }
        EpisodeGoal::StoreMorsel => {
            let Some(morsel_id) = active.object_id else {
                return EpisodeStep::Abort(EpisodeReason::SafetyAbort);
            };
            let Some(slot) = state.den.slots.iter().position(Option::is_none) else {
                active.goal = EpisodeGoal::RefuseMorsel;
                active.reason_code = EpisodeReason::FoodOpportunity;
                set_phase(active, EpisodePhase::Execute);
                return EpisodeStep::Continue;
            };
            output.body_intent.target_position = state.den.anchor;
            output.body_intent.gaze_target = Some(state.den.anchor);
            output.body_intent.locomotion = LocomotionMode::Arrive;
            output.body_intent.pose = PoseIntent::Compact;
            output.body_intent.desired_speed = output.body_intent.desired_speed.max(0.30);
            push_command(
                output,
                ObjectCommand::MoveToward {
                    object_id: morsel_id,
                    target: frame.pet_position,
                    speed: 3.2,
                },
            );
            if frame.pet_position.distance(state.den.anchor) <= 0.04 {
                push_command(
                    output,
                    ObjectCommand::Store {
                        object_id: morsel_id,
                        slot: slot as u8,
                    },
                );
                return EpisodeStep::Complete;
            }
        }
        EpisodeGoal::SharedAttention => {
            let Some(target) = frame.visual_target else {
                return EpisodeStep::Abort(EpisodeReason::SafetyAbort);
            };
            active.target_position = Some(target);
            output.body_intent.gaze_target = if active.elapsed_seconds < 1.8 {
                Some(target)
            } else {
                Some(frame.cursor_position)
            };
            output.body_intent.locomotion = LocomotionMode::Hover;
            output.body_intent.pose = PoseIntent::Curious;
            if active.elapsed_seconds >= 2.35 || !frame.shared_attention {
                return EpisodeStep::Complete;
            }
        }
        EpisodeGoal::PerformSkill | EpisodeGoal::PracticeSkill => {
            let Some(skill_id) = active.object_id else {
                return EpisodeStep::Abort(EpisodeReason::SafetyAbort);
            };
            let Some(skill) = state
                .skills
                .skills
                .iter()
                .find(|skill| skill.id == skill_id)
                .cloned()
            else {
                return EpisodeStep::Abort(EpisodeReason::SafetyAbort);
            };
            let center = *active.target_position.get_or_insert(frame.pet_position);
            let Some(path) = retarget_path(&skill.prototype, center, Vec2::new(0.14, 0.11)) else {
                return EpisodeStep::Abort(EpisodeReason::SafetyAbort);
            };
            let progress = (active.elapsed_seconds / skill.prototype.duration_seconds.max(0.05))
                .clamp(0.0, 1.0);
            let index = (progress * (crate::TRAJECTORY_SAMPLES - 1) as f32).floor() as usize;
            let target = path[index.min(crate::TRAJECTORY_SAMPLES - 1)];
            output.body_intent.target_position = target;
            output.body_intent.gaze_target = Some(target);
            output.body_intent.locomotion = LocomotionMode::Arrive;
            output.body_intent.pose = PoseIntent::Playful;
            output.body_intent.desired_speed = output
                .body_intent
                .desired_speed
                .max((0.24 + skill.competence * 0.24).clamp(0.24, 0.48));
            if active.elapsed_seconds <= dt * 1.5 {
                output.vocal_trigger = Some(EcologyVocalTrigger::SkillAttempt);
            }
            if progress >= 1.0 && frame.pet_position.distance(target) <= 0.055 {
                let error = ((frame.pet_position.distance(target) / 0.11) * 0.72
                    + (1.0 - skill.competence) * 0.28)
                    .clamp(0.0, 1.0);
                if state
                    .skills
                    .record_attempt(skill_id, error, frame.timestamp.max(0.0), None)
                    .is_err()
                {
                    return EpisodeStep::Abort(EpisodeReason::SafetyAbort);
                }
                push_outcome(output, EcologyOutcome::SkillMotorError { skill_id, error });
                if error <= 0.30 {
                    output.vocal_trigger = Some(EcologyVocalTrigger::SkillSuccess);
                }
                return EpisodeStep::Complete;
            }
            if active.elapsed_seconds >= skill.prototype.duration_seconds + 2.0 {
                return EpisodeStep::Abort(EpisodeReason::TimedOut);
            }
        }
        EpisodeGoal::RhythmEcho => {
            let Some(rhythm) = frame.click_rhythm else {
                return EpisodeStep::Abort(EpisodeReason::SafetyAbort);
            };
            output.body_intent.target_position = frame.pet_position;
            output.body_intent.gaze_target = Some(frame.cursor_position);
            output.body_intent.locomotion = LocomotionMode::Hover;
            output.body_intent.pose = PoseIntent::Display;
            if active.elapsed_seconds <= dt * 1.5 {
                output.vocal_trigger = Some(EcologyVocalTrigger::RhythmEcho);
            }
            let duration = rhythm.intervals[..usize::from(rhythm.beat_count).saturating_sub(1)]
                .iter()
                .sum::<f32>()
                / rhythm.tempo_hz.max(0.1);
            if active.elapsed_seconds >= duration.clamp(0.35, 4.0) {
                return EpisodeStep::Complete;
            }
        }
        EpisodeGoal::ChromaticEcho => {
            let Some(target) = frame.visual_target else {
                return EpisodeStep::Abort(EpisodeReason::SafetyAbort);
            };
            active.target_position = Some(target);
            output.body_intent.gaze_target = Some(target);
            output.body_intent.locomotion = LocomotionMode::Hover;
            output.body_intent.pose = PoseIntent::Curious;
            if active.elapsed_seconds >= 1.8 {
                return EpisodeStep::Complete;
            }
        }
        EpisodeGoal::Camouflage => {
            let Some(target) = frame.visual_target else {
                return EpisodeStep::Abort(EpisodeReason::SafetyAbort);
            };
            active.target_position = Some(target);
            output.body_intent.gaze_target = Some(target);
            output.body_intent.locomotion = LocomotionMode::Hover;
            output.body_intent.pose = PoseIntent::Compact;
            output.body_intent.desired_speed = output.body_intent.desired_speed.min(0.18);
            if active.elapsed_seconds >= 2.8 {
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
        EpisodeGoal::EscapePressure => 3.0,
        EpisodeGoal::RecoverAfterPressure => 1.0,
        EpisodeGoal::InspectWindow | EpisodeGoal::RideWindow => 4.5,
        EpisodeGoal::InspectMorsel => 4.0,
        EpisodeGoal::EatMorsel | EpisodeGoal::RefuseMorsel => 1.0,
        EpisodeGoal::StoreMorsel => 8.0,
        EpisodeGoal::SharedAttention => 2.5,
        EpisodeGoal::ChromaticEcho => 2.0,
        EpisodeGoal::Camouflage => 3.0,
        EpisodeGoal::PracticeSkill | EpisodeGoal::PerformSkill => 8.0,
        EpisodeGoal::RhythmEcho => 4.0,
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
        EpisodeGoal::EscapePressure | EpisodeGoal::RecoverAfterPressure => {
            ExpectedOutcome::PressureFalls
        }
        EpisodeGoal::EatMorsel => ExpectedOutcome::MorselAccepted,
        EpisodeGoal::RefuseMorsel => ExpectedOutcome::MorselRefused,
        EpisodeGoal::StoreMorsel => ExpectedOutcome::ObjectReturnsHome,
        EpisodeGoal::PracticeSkill | EpisodeGoal::PerformSkill => ExpectedOutcome::SkillReproduced,
        _ => ExpectedOutcome::None,
    }
}

fn visual_context(
    state: &EcologyState,
    active: Option<&ActivityEpisode>,
    frame: EcologyBehaviorFrame,
) -> EcologyVisualContext {
    let chromatic_blend = active.map_or(0.0, |episode| match episode.goal {
        EpisodeGoal::ChromaticEcho => frame.visual_strength.min(0.35),
        EpisodeGoal::SharedAttention => (frame.visual_strength * 0.55).min(0.24),
        _ => 0.0,
    });
    let camouflage_blend = active.map_or(0.0, |episode| {
        if episode.goal == EpisodeGoal::Camouflage {
            (0.30 + frame.visual_strength * 0.25).min(0.55)
        } else {
            0.0
        }
    });
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
        chromatic_hue: frame.visual_hue.rem_euclid(1.0),
        chromatic_blend,
        camouflage_blend,
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
            window_pressure: 0.0,
            window_escape_direction: Vec2::ZERO,
            nearest_window_edge: None,
            window_motion: 0.0,
            orb_trapped: false,
            visual_target: None,
            visual_hue: 0.0,
            visual_strength: 0.0,
            shared_attention: false,
            click_rhythm: None,
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
            visual_episode_cooldown: 0.0,
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

    #[test]
    fn pressure_preempts_play_then_enters_bounded_recovery() {
        let mut state = EcologyState::new(95);
        let mut director = EpisodeDirector::default();
        let _ = director.tick(
            &mut state,
            behavior_frame(ActionId::SelfPlay),
            representative_intent(),
            0.05,
        );
        let mut pressure = behavior_frame(ActionId::SelfPlay);
        pressure.window_pressure = 0.75;
        pressure.window_escape_direction = Vec2::NEG_X;
        pressure.nearest_window_edge = Some(Vec2::new(0.55, 0.5));
        let output = director.tick(&mut state, pressure, representative_intent(), 0.05);
        assert_eq!(output.debug.active_goal, Some(EpisodeGoal::EscapePressure));
        assert_eq!(output.body_intent.locomotion, LocomotionMode::Seek);
        assert!(output.body_intent.target_position.x < pressure.pet_position.x);

        pressure.window_pressure = 0.0;
        let output = director.tick(&mut state, pressure, representative_intent(), 0.10);
        assert_eq!(
            output.debug.active_goal,
            Some(EpisodeGoal::RecoverAfterPressure)
        );
        assert!(
            output.outcomes[..output.outcome_count]
                .iter()
                .any(|outcome| {
                    *outcome == EcologyOutcome::EpisodeCompleted(EpisodeGoal::EscapePressure)
                })
        );
    }

    #[test]
    fn trapped_orb_requests_help_without_inventing_a_new_object() {
        let mut state = EcologyState::new(96);
        let orb_id = state.objects[0].id;
        let orb_position = state.objects[0].position;
        let mut frame = behavior_frame(ActionId::IdleHover);
        frame.orb_trapped = true;
        frame.pet_position = orb_position;
        let mut director = EpisodeDirector::default();
        let mut help_seen = false;
        for _ in 0..16 {
            let output = director.tick(&mut state, frame, representative_intent(), 0.20);
            help_seen |= output.vocal_trigger == Some(EcologyVocalTrigger::Help);
            if output.debug.active_phase == Some(EpisodePhase::AskForHelp) {
                break;
            }
        }
        assert_eq!(
            director.active_episode().map(|episode| episode.goal),
            Some(EpisodeGoal::RetrieveOrb)
        );
        assert_eq!(
            director.active_episode().map(|episode| episode.phase),
            Some(EpisodePhase::AskForHelp)
        );
        assert!(help_seen);
        assert_eq!(state.objects.len(), 1);
        assert_eq!(state.objects[0].id, orb_id);
    }

    #[test]
    fn focus_mode_suppresses_a_trapped_orb_help_bid() {
        let mut state = EcologyState::new(98);
        let mut frame = behavior_frame(ActionId::IdleHover);
        frame.orb_trapped = true;
        frame.focus_mode = true;
        frame.pet_position = Vec2::splat(0.5);
        let mut director = EpisodeDirector::default();
        let output = director.tick(&mut state, frame, representative_intent(), 0.20);
        assert_eq!(output.debug.active_goal, Some(EpisodeGoal::ReturnHome));
        assert_ne!(output.vocal_trigger, Some(EcologyVocalTrigger::Help));
    }

    #[test]
    fn land_action_uses_the_shared_window_edge() {
        let mut state = EcologyState::new(97);
        let mut frame = behavior_frame(ActionId::LandOnWindow);
        frame.nearest_window_edge = Some(Vec2::new(0.62, 0.44));
        let mut director = EpisodeDirector::default();
        let output = director.tick(&mut state, frame, representative_intent(), 0.05);
        assert_eq!(output.debug.active_goal, Some(EpisodeGoal::RideWindow));
        assert_eq!(
            output.body_intent.target_position,
            frame.nearest_window_edge.unwrap()
        );
        assert_eq!(
            output.body_intent.locomotion,
            LocomotionMode::SurfaceApproach
        );
    }

    fn test_morsel(hue: f32) -> crate::MorselProfile {
        crate::MorselProfile {
            hue,
            saturation: 0.82,
            value: 0.92,
            warmth: 0.64,
            pulse_rate: 0.45,
            stimulation: 0.52,
            cohesion_bias: 0.68,
            novelty: 0.84,
        }
    }

    #[test]
    fn morsel_inspection_can_end_in_eating_and_bounded_taste_learning() {
        let mut state = EcologyState::new(99);
        let profile = test_morsel(0.23);
        let morsel_id = state.spawn_morsel(Vec2::splat(0.5), profile, 1.0).unwrap();
        let mut frame = behavior_frame(ActionId::IdleHover);
        frame.pet_position = Vec2::splat(0.5);
        let mut director = EpisodeDirector::default();
        let mut consumed = false;
        for _ in 0..10 {
            let output = director.tick(&mut state, frame, representative_intent(), 0.25);
            consumed |= output.outcomes[..output.outcome_count]
                .contains(&EcologyOutcome::MorselConsumed(morsel_id));
            if consumed {
                break;
            }
        }
        assert!(consumed);
        assert!(state.metabolism.active_effect.is_some());
        assert!(state.taste.confidence > 0.0);
        assert!(state.taste.confidence <= 1.0);
    }

    #[test]
    fn satiated_pet_refuses_repeated_flavor_calmly() {
        let mut state = EcologyState::new(100);
        let profile = test_morsel(0.31);
        state.metabolism.consume(&profile);
        state.metabolism.satiation = 1.0;
        let morsel_id = state.spawn_morsel(Vec2::splat(0.5), profile, 1.0).unwrap();
        let mut frame = behavior_frame(ActionId::IdleHover);
        frame.pet_position = Vec2::splat(0.5);
        frame.timestamp = 12.0;
        let mut director = EpisodeDirector::default();
        let mut refused = false;
        for _ in 0..10 {
            let output = director.tick(&mut state, frame, representative_intent(), 0.25);
            refused |= output.vocal_trigger == Some(EcologyVocalTrigger::FoodRefused);
            if refused {
                break;
            }
        }
        assert!(refused);
        let morsel = state
            .objects
            .iter()
            .find(|object| object.id == morsel_id)
            .unwrap();
        assert!(morsel.preference < 0.0);
        assert_eq!(
            state.metabolism.reserve.max(crate::METABOLIC_RESERVE_FLOOR),
            state.metabolism.reserve
        );
    }

    #[test]
    fn shared_attention_and_color_adaptation_target_the_active_cell() {
        let mut state = EcologyState::new(101);
        let mut frame = behavior_frame(ActionId::ExploreScreen);
        frame.visual_target = Some(Vec2::new(0.78, 0.22));
        frame.visual_hue = 0.64;
        frame.visual_strength = 0.8;
        frame.shared_attention = true;
        let mut director = EpisodeDirector::default();
        let output = director.tick(&mut state, frame, representative_intent(), 0.05);
        assert_eq!(output.debug.active_goal, Some(EpisodeGoal::SharedAttention));
        assert_eq!(output.body_intent.gaze_target, frame.visual_target);
        assert!(output.visual_context.chromatic_blend <= 0.35);

        let mut camouflage_director = EpisodeDirector::default();
        frame.shared_attention = false;
        frame.selected_action = ActionId::HideAndSeek;
        let output = camouflage_director.tick(&mut state, frame, representative_intent(), 0.05);
        assert_eq!(output.debug.active_goal, Some(EpisodeGoal::Camouflage));
        assert!(output.visual_context.camouflage_blend <= 0.55);
        assert_eq!(output.visual_context.chromatic_hue, frame.visual_hue);
    }

    #[test]
    fn learned_skill_executes_through_bounded_targets_and_records_motor_error() {
        let mut state = EcologyState::new(102);
        let points: Vec<_> = (0..64)
            .map(|index| {
                let phase = index as f32 / 63.0 * std::f32::consts::TAU;
                Vec2::splat(0.5) + Vec2::new(phase.sin(), phase.sin() * phase.cos()) * 0.18
            })
            .collect();
        let signature = crate::ActionSignature::from_trace(&points, 1.0).unwrap();
        let (skill_id, _) = state.skills.observe(signature, 0.0).unwrap();
        let mut frame = behavior_frame(ActionId::HappyDisplay);
        frame.timestamp = 25.0;
        let mut director = EpisodeDirector::default();
        let mut motor_error_seen = false;
        for _ in 0..20 {
            let output = director.tick(&mut state, frame, representative_intent(), 0.10);
            assert!(output.body_intent.target_position.is_finite());
            frame.pet_position = output.body_intent.target_position;
            frame.timestamp += 0.10;
            motor_error_seen |= output.outcomes[..output.outcome_count]
                .iter()
                .any(|outcome| {
                    matches!(
                        outcome,
                        EcologyOutcome::SkillMotorError {
                            skill_id: observed,
                            error,
                        } if *observed == skill_id && (0.0..=1.0).contains(error)
                    )
                });
            if motor_error_seen {
                break;
            }
        }
        assert!(motor_error_seen);
        assert_eq!(state.skills.skills[0].attempts, 1);
    }

    #[test]
    fn click_history_drives_one_grounded_rhythm_echo_episode() {
        let mut state = EcologyState::new(103);
        let mut frame = behavior_frame(ActionId::MimicClickRhythm);
        frame.click_rhythm = crate::RhythmSignature::from_onsets(&[0.0, 0.2, 0.5, 0.7]);
        let mut director = EpisodeDirector::default();
        let output = director.tick(&mut state, frame, representative_intent(), 0.05);
        assert_eq!(output.debug.active_goal, Some(EpisodeGoal::RhythmEcho));
        assert_eq!(output.vocal_trigger, Some(EcologyVocalTrigger::RhythmEcho));
        assert_eq!(output.body_intent.pose, PoseIntent::Display);
    }
}
