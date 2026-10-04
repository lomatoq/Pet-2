//! Short physiological continuity across bouts, separate from lifelong identity.
//! Evidence is consumed once by LifeCore; nominated actions never earn recovery.
use serde::{Deserialize, Serialize};

use crate::{ActionId, EpisodeContextV1, InteroceptionSnapshot};

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ActivityRegulationState {
    pub sleep_depth: f32,
    pub wake_inertia: f32,
    pub exertion: f32,
    pub contact_warmth: f32,
    #[serde(skip)]
    was_resting: bool,
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct ActivityEvidence {
    rest: f32,
    gentle_contact: f32,
    effort: f32,
    threat: f32,
}

impl ActivityEvidence {
    pub(crate) fn measured(snapshot: InteroceptionSnapshot, episode: EpisodeContextV1) -> Self {
        let f = snapshot.felt;
        let threat = unit(
            f.pain_like
                .max(f.restraint)
                .max(f.startle)
                .max(episode.boundary_violation),
        );
        Self {
            rest: unit(episode.sleeping_or_deep_rest),
            gentle_contact: unit(f.contact_pleasantness.max(0.0))
                * unit(episode.safe_social_exchange)
                * unit(f.social_safety)
                * (1.0 - threat),
            effort: unit(f.physical_load)
                * unit(f.agency_match)
                * (1.0 - unit(episode.sleeping_or_deep_rest)),
            threat,
        }
    }
}

impl ActivityRegulationState {
    pub(crate) fn clear_observation(&mut self) {
        self.was_resting = false;
    }

    pub(crate) fn recover_unobserved(&mut self, seconds: f32) {
        self.clear_observation();
        self.sleep_depth *= (-seconds / 30.0).exp();
        self.wake_inertia *= (-seconds / 15.0).exp();
        self.exertion *= (-seconds / 40.0).exp();
        self.contact_warmth *= (-seconds / 18.0).exp();
    }

    pub(crate) fn tick(&mut self, evidence: ActivityEvidence, dt: f32) {
        self.advance(Some(evidence), dt);
    }

    pub(crate) fn elapse(&mut self, dt: f32) {
        self.advance(None, dt);
    }

    fn advance(&mut self, observed: Option<ActivityEvidence>, dt: f32) {
        let dt = if dt.is_finite() {
            dt.clamp(0.0, 0.25)
        } else {
            0.0
        };
        if let Some(evidence) = observed {
            let resting = evidence.rest > 0.1;
            if self.was_resting && !resting {
                // Only a fresh observation of rest ending creates this effect.
                self.wake_inertia = self.wake_inertia.max(self.sleep_depth * 0.85);
            }
            self.was_resting = resting;
            self.sleep_depth = follow(
                self.sleep_depth,
                evidence.rest,
                if resting { 8.0 } else { 2.0 },
                dt,
            );
        }
        // A missing observation is unknown, not an awake/sleep transition.
        // Somatic aftereffects still decay with elapsed time without new credit.
        let evidence = observed.unwrap_or_default();
        self.wake_inertia = follow(self.wake_inertia, 0.0, 15.0, dt);
        self.exertion = follow(
            self.exertion,
            evidence.effort,
            if evidence.effort > self.exertion {
                12.0
            } else {
                40.0
            },
            dt,
        );
        self.contact_warmth = follow(
            self.contact_warmth,
            evidence.gentle_contact,
            if evidence.threat > 0.2 {
                0.35
            } else if evidence.gentle_contact > self.contact_warmth {
                2.0
            } else {
                18.0
            },
            dt,
        );
    }

    #[must_use]
    pub fn drowsiness(self, sleep_pressure: f32) -> f32 {
        unit(unit(sleep_pressure).powi(2) * 0.75 + self.wake_inertia * 0.65 + self.exertion * 0.25)
    }

    #[must_use]
    pub fn vigor(self, sleep_pressure: f32) -> f32 {
        1.0 - self.drowsiness(sleep_pressure)
    }

    /// An opportunity cost, not a second action selector or hard pose gate.
    pub(crate) fn action_bias(self, action: ActionId, sleep: f32) -> f32 {
        use ActionId as A;
        let tired = self.drowsiness(sleep);
        match action {
            A::PlayCursorChase
            | A::InviteCursorChase
            | A::BringProceduralOrb
            | A::HideAndSeek
            | A::SelfPlay
            | A::HappyDisplay => -0.9 * tired,
            A::ExploreScreen | A::ClingToWindowSide => -0.45 * tired,
            A::InvitePetting | A::Chirp => -0.35 * self.contact_warmth,
            A::IdleHover | A::ObserveUserActivity | A::SilentStare => {
                0.22 * tired + 0.24 * self.contact_warmth
            }
            A::Sleep => 0.40 * tired,
            // Acute escape and protective behavior retain their full authority.
            _ => 0.0,
        }
    }

    #[must_use]
    pub fn is_valid(self) -> bool {
        [
            self.sleep_depth,
            self.wake_inertia,
            self.exertion,
            self.contact_warmth,
        ]
        .into_iter()
        .all(|v| v.is_finite() && (0.0..=1.0).contains(&v))
    }
}

fn unit(v: f32) -> f32 {
    if v.is_finite() {
        v.clamp(0.0, 1.0)
    } else {
        0.0
    }
}
fn follow(current: f32, target: f32, tau: f32, dt: f32) -> f32 {
    unit(current + (target - current) * (1.0 - (-dt / tau).exp()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn measured_rest_creates_then_releases_wake_inertia() {
        let mut state = ActivityRegulationState::default();
        for _ in 0..200 {
            state.tick(
                ActivityEvidence {
                    rest: 1.0,
                    ..Default::default()
                },
                0.1,
            );
        }
        state.tick(ActivityEvidence::default(), 0.1);
        assert!(state.wake_inertia > 0.7);
        let initial = state.vigor(0.2);
        for _ in 0..600 {
            state.tick(ActivityEvidence::default(), 0.1);
        }
        assert!(state.vigor(0.2) > initial + 0.4);
        let mut unexecuted = ActivityRegulationState::default();
        for _ in 0..200 {
            unexecuted.tick(ActivityEvidence::default(), 0.1);
        }
        assert_eq!(unexecuted.wake_inertia, 0.0);
    }

    #[test]
    fn warmth_survives_release_but_absence_never_creates_it() {
        let mut state = ActivityRegulationState::default();
        for _ in 0..60 {
            state.tick(
                ActivityEvidence {
                    gentle_contact: 0.8,
                    ..Default::default()
                },
                0.1,
            );
        }
        let released = state.contact_warmth;
        for _ in 0..20 {
            state.tick(ActivityEvidence::default(), 0.1);
        }
        assert!(state.contact_warmth > released * 0.8);
        for _ in 0..1200 {
            state.tick(ActivityEvidence::default(), 0.1);
        }
        assert!(state.contact_warmth < 0.002);
        assert!(state.is_valid());
    }

    #[test]
    fn forceful_contact_cannot_create_warmth_and_clears_aftereffect() {
        let mut snapshot = InteroceptionSnapshot::default();
        snapshot.felt.contact_pleasantness = 1.0;
        snapshot.felt.social_safety = 1.0;
        snapshot.felt.restraint = 1.0;
        let evidence = ActivityEvidence::measured(
            snapshot,
            EpisodeContextV1 {
                safe_social_exchange: 1.0,
                ..Default::default()
            },
        );
        assert_eq!(evidence.gentle_contact, 0.0);
        let mut state = ActivityRegulationState {
            contact_warmth: 0.8,
            ..Default::default()
        };
        for _ in 0..20 {
            state.tick(evidence, 0.1);
        }
        assert!(state.contact_warmth < 0.01);
    }

    #[test]
    fn independent_timestep_partitions_match_and_defense_is_unbiased() {
        let evidence = ActivityEvidence {
            gentle_contact: 0.8,
            effort: 0.6,
            ..Default::default()
        };
        let mut a = ActivityRegulationState::default();
        let mut b = a;
        for _ in 0..200 {
            a.tick(evidence, 0.05);
        }
        for _ in 0..100 {
            b.tick(evidence, 0.1);
        }
        assert!((a.contact_warmth - b.contact_warmth).abs() < 1e-5);
        assert!((a.exertion - b.exertion).abs() < 1e-5);
        assert_eq!(a.action_bias(ActionId::RetreatFromCursor, 1.0), 0.0);
    }

    #[test]
    fn missing_evidence_and_resume_cannot_invent_a_rest_departure() {
        let mut state = ActivityRegulationState::default();
        for _ in 0..200 {
            state.tick(
                ActivityEvidence {
                    rest: 1.0,
                    ..Default::default()
                },
                0.1,
            );
        }
        for _ in 0..20 {
            state.elapse(0.1);
        }
        assert_eq!(state.wake_inertia, 0.0);
        let encoded = serde_json::to_string(&state).unwrap();
        let mut restored: ActivityRegulationState = serde_json::from_str(&encoded).unwrap();
        restored.elapse(0.1);
        restored.tick(ActivityEvidence::default(), 0.1);
        assert_eq!(
            restored.wake_inertia, 0.0,
            "a stale session latch created a wake"
        );
        state.tick(ActivityEvidence::default(), 0.1);
        assert!(
            state.wake_inertia > 0.7,
            "fresh measured departure was lost"
        );
        state.contact_warmth = 0.8;
        state.recover_unobserved(600.0);
        assert!(state.contact_warmth < 0.001);
        assert!(state.wake_inertia < 0.001);
    }
}
