//! Semantic projection of the scene that actually owns execution.
//! The policy proposal remains a proposal; face, motor and outcome credit
//! consume the same current scene instead of explaining another action.
use crate::vita_runtime::VitaRuntime;
use lifecore::{
    ActionId, BodyIntent, ExpectedOutcome, IntentTarget, IntentTargetKind, PrimaryIntent,
};
use pet_ecology::{ActivityEpisode, EcologyState, EpisodeGoal, EpisodePhase};

pub fn synchronize(
    vita: &mut VitaRuntime,
    episode: Option<&ActivityEpisode>,
    state: &EcologyState,
    intent: &BodyIntent,
    proposed: ActionId,
) -> ActionId {
    let Some(episode) = episode else {
        return proposed;
    };
    let (primary, kind, action) = semantics(episode.goal, episode.phase, proposed);
    let object_position = episode
        .object_id
        .and_then(|id| {
            state
                .objects
                .iter()
                .find(|o| o.id == id)
                .map(|o| o.position)
        })
        .or(episode.target_position);
    let position = match kind {
        IntentTargetKind::Den => Some(state.den.anchor),
        IntentTargetKind::Surface => episode.target_position.or(Some(intent.target_position)),
        _ => object_position.or(Some(intent.target_position)),
    };
    let user_response = f32::from(matches!(
        episode.goal,
        EpisodeGoal::OfferOrb | EpisodeGoal::SharedAttention
    ));
    let accepted = vita.synchronize_execution_scene(
        primary,
        IntentTarget {
            kind,
            position,
            confidence: episode.prediction_confidence.clamp(0.25, 1.0),
            ..Default::default()
        },
        ExpectedOutcome {
            continuation: 0.85,
            user_response: user_response * 0.65,
            object_contact: f32::from(episode.object_id.is_some()) * 0.8,
            success: episode.prediction_confidence.clamp(0.0, 1.0),
            uncertainty: 1.0 - episode.prediction_confidence.clamp(0.0, 1.0),
        },
    );
    if accepted {
        action
    } else {
        match vita.companion_intent().primary {
            PrimaryIntent::StartleFreeze | PrimaryIntent::GuardPain => ActionId::IdleHover,
            PrimaryIntent::EscapePressure | PrimaryIntent::Avoid | PrimaryIntent::RejectContact => {
                ActionId::RetreatFromCursor
            }
            _ => action,
        }
    }
}

pub fn executed_action(episode: Option<&ActivityEpisode>, proposed: ActionId) -> ActionId {
    episode.map_or(proposed, |e| semantics(e.goal, e.phase, proposed).2)
}

pub fn credit_object_contacts(
    life: &mut lifecore::LifeCore,
    episode: Option<&ActivityEpisode>,
    outcomes: &[pet_ecology::EcologyOutcome],
) {
    let Some(episode) = episode else {
        return;
    };
    if !matches!(
        episode.goal,
        EpisodeGoal::ChaseOrb | EpisodeGoal::InterceptOrb | EpisodeGoal::SoloOrbPlay
    ) {
        return;
    }
    for outcome in outcomes {
        if let pet_ecology::EcologyOutcome::ObjectContact(id) = outcome {
            if episode.object_id != Some(*id) {
                continue;
            }
            let identity = lifecore::stable_hash_bytes(
                format!("physical-play:{}:{id}", episode.id).as_bytes(),
            );
            life.apply_measured_relief(identity, lifecore::DriveKind::Play, 0.10);
        }
    }
}

fn semantics(
    goal: EpisodeGoal,
    phase: EpisodePhase,
    proposed: ActionId,
) -> (PrimaryIntent, IntentTargetKind, ActionId) {
    use EpisodeGoal as G;
    use IntentTargetKind as T;
    use PrimaryIntent as P;
    match goal {
        G::EscapePressure => (P::EscapePressure, T::Surface, ActionId::RetreatFromCursor),
        G::RecoverAfterPressure => (P::SettleAfterStress, T::Surface, ActionId::IdleHover),
        G::ReturnHome => (P::ReturnHome, T::Den, ActionId::LandOnWindow),
        G::SleepInDen => (P::Sleep, T::Den, ActionId::Sleep),
        G::ExitDen => (P::Wake, T::Den, ActionId::WakeUp),
        G::PeekFromDen => (P::Peek, T::Den, ActionId::PeekFromEdge),
        G::CarryOrbHome => (P::Carry, T::Den, ActionId::BringProceduralOrb),
        G::ReturnOrb => (P::Carry, T::Cursor, ActionId::BringProceduralOrb),
        G::RetrieveOrb | G::SeekOrb => (P::SearchObject, T::Orb, ActionId::SelfPlay),
        G::OfferOrb => (P::OfferObject, T::Orb, ActionId::BringProceduralOrb),
        G::ChaseOrb => (P::Chase, T::Orb, ActionId::PlayCursorChase),
        G::InterceptOrb => (P::Intercept, T::Orb, ActionId::PlayCursorChase),
        G::SoloOrbPlay => (P::InvitePlay, T::Orb, ActionId::SelfPlay),
        G::HideOrb => (P::Hide, T::Orb, ActionId::HideAndSeek),
        G::InspectMorsel | G::EatMorsel | G::StoreMorsel => (
            if matches!(
                phase,
                EpisodePhase::Manipulate | EpisodePhase::Evaluate | EpisodePhase::Execute
            ) || goal == G::EatMorsel {
                P::EatAccept
            } else {
                P::EatInspect
            },
            T::ScreenRegion,
            ActionId::IdleHover,
        ),
        G::RefuseMorsel => (P::EatReject, T::ScreenRegion, ActionId::IdleHover),
        G::InspectWindow => (P::Inspect, T::Surface, ActionId::ClingToWindowSide),
        G::RideWindow => (P::Rest, T::Surface, ActionId::LandOnWindow),
        G::SharedAttention => (P::Inspect, T::ScreenRegion, ActionId::ExploreScreen),
        G::ChromaticEcho => (P::Inspect, T::ScreenRegion, ActionId::ExploreScreen),
        G::Camouflage => (P::Hide, T::ScreenRegion, ActionId::HideAndSeek),
        G::PracticeSkill | G::PerformSkill | G::RhythmEcho => (P::Mimic, T::None, proposed),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scene_owner_replaces_unrelated_proposal() {
        assert_eq!(
            semantics(
                EpisodeGoal::SleepInDen,
                EpisodePhase::Execute,
                ActionId::SelfPlay
            )
            .2,
            ActionId::Sleep
        );
        assert_eq!(
            semantics(
                EpisodeGoal::InspectMorsel,
                EpisodePhase::Inspect,
                ActionId::BringProceduralOrb
            )
            .0,
            PrimaryIntent::EatInspect
        );
        assert_eq!(
            semantics(
                EpisodeGoal::ChaseOrb,
                EpisodePhase::Approach,
                ActionId::Sleep
            )
            .0,
            PrimaryIntent::Chase
        );
    }

    #[test]
    fn verified_bite_recovery_keeps_the_eating_scene_until_chewing_finishes() {
        assert_eq!(semantics(EpisodeGoal::EatMorsel, EpisodePhase::Recover, ActionId::Sleep).0, PrimaryIntent::EatAccept);
    }
}
