use glam::Vec2;
use lifecore::BodyIntent;

use crate::{EcologyDecisionTrace, ObjectId};

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
}
