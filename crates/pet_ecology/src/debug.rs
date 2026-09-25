use crate::{EPISODE_GOAL_COUNT, EpisodeGoal, EpisodePhase, EpisodeReason};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct GoalScore {
    pub goal: Option<EpisodeGoal>,
    pub score: f32,
    pub eligible: bool,
    pub reason: EpisodeReason,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EcologyDecisionTrace {
    pub tick: u64,
    pub orb_motivation: Option<crate::OrbMotivation>,
    pub active_goal: Option<EpisodeGoal>,
    pub active_phase: Option<EpisodePhase>,
    pub selected_reason: EpisodeReason,
    pub scores: [GoalScore; EPISODE_GOAL_COUNT],
    pub score_count: usize,
    pub focus_mode_filtered: bool,
}

impl Default for EcologyDecisionTrace {
    fn default() -> Self {
        Self {
            tick: 0,
            orb_motivation: None,
            active_goal: None,
            active_phase: None,
            selected_reason: EpisodeReason::NoEligibleEpisode,
            scores: [GoalScore::default(); EPISODE_GOAL_COUNT],
            score_count: 0,
            focus_mode_filtered: false,
        }
    }
}
