use serde::{Deserialize, Serialize};

use crate::{
    ActionId, DerivedNervousState, EpisodeContextV1, FeltStateV1, SensorFrame, TemperamentGenome,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DriveKind {
    Sleep,
    Social,
    Play,
    Curiosity,
    Comfort,
    Safety,
    Autonomy,
    Novelty,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Drives {
    pub sleep: f32,
    pub social: f32,
    pub play: f32,
    pub curiosity: f32,
    pub comfort: f32,
    pub safety: f32,
    pub autonomy: f32,
    pub novelty: f32,
}

impl Drives {
    #[must_use]
    pub fn initial(temperament: &TemperamentGenome) -> Self {
        Self {
            sleep: 0.12,
            social: 0.22 + temperament.sociability * 0.16,
            play: 0.18 + temperament.playfulness * 0.18,
            curiosity: 0.20 + temperament.curiosity * 0.16,
            comfort: 0.12,
            safety: 0.08,
            autonomy: 0.14 + temperament.autonomy * 0.12,
            novelty: 0.20 + temperament.exploration_rate * 0.15,
        }
        .bounded()
    }

    pub fn update(
        &mut self,
        temperament: &TemperamentGenome,
        sensors: &SensorFrame,
        _current_action: ActionId,
        dt: f32,
    ) {
        self.update_with_rest_quality(temperament, sensors, _current_action, dt, 0.0);
    }

    /// Rest quality must come from completed physical evidence, not a Sleep
    /// nomination. The host consumes that observation once per cognition tick.
    pub fn update_with_rest_quality(
        &mut self,
        temperament: &TemperamentGenome,
        sensors: &SensorFrame,
        _current_action: ActionId,
        dt: f32,
        rest_quality: f32,
    ) {
        // A proposed action is not an experience. Relief enters through
        // measured body evidence or a deduplicated confirmed outcome.
        let hour_angle = std::f32::consts::TAU
            * (sensors.time_of_day_01 - temperament.circadian_phase).rem_euclid(1.0);
        let circadian_sleep = (0.5 - 0.5 * hour_angle.cos()).clamp(0.0, 1.0);
        let user_present = sensors.user_presence.unwrap_or({
            if sensors.user_idle_seconds < 180.0 {
                1.0
            } else {
                0.0
            }
        });
        let user_available = sensors.user_availability.unwrap_or({
            if sensors.user_idle_seconds < 30.0 {
                0.72
            } else if sensors.user_idle_seconds < 180.0 {
                0.32
            } else {
                0.0
            }
        });
        let cursor_threat = ((0.20 - sensors.cursor_distance_to_pet) / 0.20).clamp(0.0, 1.0)
            * sensors.cursor_approach_speed.max(0.0).clamp(0.0, 2.0)
            * (1.0 - temperament.boldness);
        let novelty_input = (sensors.cursor_velocity.length() * 0.25
            + sensors.user_activity_rate * 0.3)
            .clamp(0.0, 1.0);
        let work_intensity = (sensors.user_activity_rate * 0.72
            + if sensors.active_app_category == crate::AppCategory::FocusedWork {
                0.20
            } else {
                0.0
            })
        .clamp(0.0, 1.0);

        // Slow, deterministic endogenous rhythms prevent every need from
        // climbing in lockstep. They are personality-shaped pressure waves,
        // not scheduled behaviors: the policy still decides whether and how a
        // need is expressed in the current desktop context.
        let phase_seconds = sensors.timestamp.rem_euclid(1_200.0) as f32;
        let social_rhythm = 0.78
            + 0.32
                * (phase_seconds / 137.0 + temperament.sociability * 4.1)
                    .sin()
                    .max(-0.65);
        let play_rhythm = 0.76
            + 0.34
                * (phase_seconds / 91.0 + temperament.playfulness * 5.3)
                    .sin()
                    .max(-0.65);
        let curiosity_rhythm = 0.80
            + 0.30
                * (phase_seconds / 173.0 + temperament.curiosity * 3.7)
                    .sin()
                    .max(-0.65);

        let rest = rest_quality.clamp(0.0, 1.0);
        // Non-zero quality has already passed actual sleep/support/safety
        // admission. Poorer admitted sleep recovers slower; it is not partly
        // waking, and cannot accumulate an opposing circadian debt floor.
        let awake = f32::from(rest == 0.0);
        self.sleep += ((0.0025 + circadian_sleep * 0.006) * awake
            - self.sleep * rest / 80.0) * dt;
        self.social += (0.0010 + temperament.sociability * 0.0028)
            * social_rhythm
            * (0.82 + user_present * 0.18)
            * dt;
        self.play += (0.0009 + temperament.playfulness * 0.0031) * play_rhythm * dt;
        self.curiosity += (0.0008 + temperament.curiosity * (0.28 + novelty_input * 0.72) * 0.0032)
            * curiosity_rhythm
            * dt;
        self.comfort += (0.0007
            + work_intensity * 0.0015
            + self.sleep * 0.0006
            + (1.0 - user_available) * user_present * 0.0004)
            * dt;
        self.safety += (cursor_threat * 0.08 - 0.012) * dt;
        self.autonomy +=
            (0.0007 + temperament.autonomy * 0.0015 + work_intensity * user_present * 0.0009) * dt;
        self.novelty += (0.0011 + (1.0 - novelty_input) * 0.0022) * dt;
        *self = self.bounded();
    }

    /// One actual outcome, in deficit units; the host owns attribution and
    /// LifeCore deduplicates event identity before calling this method.
    pub fn relieve_measured(&mut self, drive: DriveKind, amount: f32) {
        if !amount.is_finite() || amount <= 0.0 {
            return;
        }
        let value = match drive {
            DriveKind::Sleep => &mut self.sleep,
            DriveKind::Social => &mut self.social,
            DriveKind::Play => &mut self.play,
            DriveKind::Curiosity => &mut self.curiosity,
            DriveKind::Comfort => &mut self.comfort,
            DriveKind::Safety => &mut self.safety,
            DriveKind::Autonomy => &mut self.autonomy,
            DriveKind::Novelty => &mut self.novelty,
        };
        *value = (*value - amount.min(0.24)).clamp(0.0, 1.0);
    }

    /// Slow homeostatic evidence from the embodied nervous-system loop.
    /// Values are deficits; positive deltas mean the need is less satisfied.
    pub fn integrate_felt_state(
        &mut self,
        felt: FeltStateV1,
        derived: DerivedNervousState,
        episode: EpisodeContextV1,
        dt: f32,
    ) {
        let dt = dt.clamp(0.0, 0.25);
        self.sleep += (0.00035 + 0.00055 * felt.activation + 0.00040 * felt.physical_load)
            * f32::from(episode.sleeping_or_deep_rest <= 0.0)
            * dt;
        self.social += (0.00025 * episode.user_absent + 0.00035 * episode.ignored_social_bid
            - 0.06 * self.social * episode.safe_social_exchange.clamp(0.0, 1.0))
            * dt;
        self.play += (0.00018 * (1.0 - felt.play_readiness) + 0.00022 * felt.boredom
            - 0.0014 * episode.successful_play)
            * dt;
        self.curiosity += (0.00020 * derived.habituation
            + 0.00016 * (1.0 - derived.neural_novelty)
            - 0.0012 * episode.successful_exploration)
            * dt;
        self.comfort +=
            (0.0012 * felt.pain_like + 0.00055 * felt.restraint + 0.00040 * felt.physical_load
                - 0.012 * self.comfort * felt.comfort)
                * dt;
        self.safety += (0.0014 * derived.neural_threat
            + 0.0015 * felt.pain_like
            + 0.00055 * (1.0 - felt.social_safety)
            - 0.0015 * episode.safe_predictable_episode)
            * dt;
        self.autonomy += (0.0012 * felt.restraint + 0.00065 * (1.0 - felt.agency_match)
            - 0.0012 * episode.self_initiated_success)
            * dt;
        self.novelty += (0.00028 * felt.boredom + 0.00018 * derived.habituation
            - 0.0013 * episode.novel_goal_congruent_episode)
            * dt;
        *self = self.bounded_with_recovery();
    }

    #[must_use]
    pub fn homeostatic_cost(&self) -> f32 {
        const IMPORTANCE: DriveVector = DriveVector {
            sleep: 1.15,
            social: 0.95,
            play: 0.72,
            curiosity: 0.68,
            comfort: 0.92,
            safety: 1.35,
            autonomy: 0.58,
            novelty: 0.55,
        };
        self.sleep.powi(2) * IMPORTANCE.sleep
            + self.social.powi(2) * IMPORTANCE.social
            + self.play.powi(2) * IMPORTANCE.play
            + self.curiosity.powi(2) * IMPORTANCE.curiosity
            + self.comfort.powi(2) * IMPORTANCE.comfort
            + self.safety.powi(2) * IMPORTANCE.safety
            + self.autonomy.powi(2) * IMPORTANCE.autonomy
            + self.novelty.powi(2) * IMPORTANCE.novelty
    }

    #[must_use]
    pub fn strongest(&self) -> (DriveKind, f32) {
        [
            (DriveKind::Sleep, self.sleep),
            (DriveKind::Social, self.social),
            (DriveKind::Play, self.play),
            (DriveKind::Curiosity, self.curiosity),
            (DriveKind::Comfort, self.comfort),
            (DriveKind::Safety, self.safety),
            (DriveKind::Autonomy, self.autonomy),
            (DriveKind::Novelty, self.novelty),
        ]
        .into_iter()
        .max_by(|left, right| left.1.total_cmp(&right.1))
        .unwrap_or((DriveKind::Comfort, 0.0))
    }

    #[must_use]
    pub fn relief_value(&self, relief: DriveVector) -> f32 {
        self.sleep * relief.sleep
            + self.social * relief.social
            + self.play * relief.play
            + self.curiosity * relief.curiosity
            + self.comfort * relief.comfort
            + self.safety * relief.safety
            + self.autonomy * relief.autonomy
            + self.novelty * relief.novelty
    }

    #[must_use]
    pub fn is_finite(&self) -> bool {
        [
            self.sleep,
            self.social,
            self.play,
            self.curiosity,
            self.comfort,
            self.safety,
            self.autonomy,
            self.novelty,
        ]
        .into_iter()
        .all(f32::is_finite)
    }

    fn bounded(mut self) -> Self {
        self.sleep = self.sleep.clamp(0.0, 1.0);
        self.social = self.social.clamp(0.0, 1.0);
        self.play = self.play.clamp(0.0, 1.0);
        self.curiosity = self.curiosity.clamp(0.0, 1.0);
        self.comfort = self.comfort.clamp(0.0, 1.0);
        self.safety = self.safety.clamp(0.0, 1.0);
        self.autonomy = self.autonomy.clamp(0.0, 1.0);
        self.novelty = self.novelty.clamp(0.0, 1.0);
        self
    }

    fn bounded_with_recovery(mut self) -> Self {
        for value in [
            &mut self.sleep,
            &mut self.social,
            &mut self.play,
            &mut self.curiosity,
            &mut self.comfort,
            &mut self.safety,
            &mut self.autonomy,
            &mut self.novelty,
        ] {
            *value = value.clamp(0.001, 0.999);
        }
        self
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct DriveVector {
    pub sleep: f32,
    pub social: f32,
    pub play: f32,
    pub curiosity: f32,
    pub comfort: f32,
    pub safety: f32,
    pub autonomy: f32,
    pub novelty: f32,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temperament() -> TemperamentGenome {
        TemperamentGenome {
            sociability: 0.55,
            playfulness: 0.62,
            curiosity: 0.68,
            autonomy: 0.51,
            boldness: 0.52,
            patience: 0.57,
            persistence: 0.54,
            vocality: 0.46,
            adaptability: 0.58,
            attachment_speed: 0.50,
            exploration_rate: 0.61,
            circadian_phase: 0.25,
        }
    }

    #[test]
    fn neutral_presence_does_not_erase_the_mind() {
        let temperament = temperament();
        let mut drives = Drives {
            sleep: 0.10,
            social: 0.18,
            play: 0.16,
            curiosity: 0.17,
            comfort: 0.12,
            safety: 0.0,
            autonomy: 0.14,
            novelty: 0.16,
        };
        let sensors = SensorFrame {
            timestamp: 42.0,
            user_presence: Some(1.0),
            user_availability: Some(0.7),
            ..SensorFrame::default()
        };

        for _ in 0..600 {
            drives.update(&temperament, &sensors, ActionId::IdleHover, 0.1);
        }

        assert!(drives.social > 0.22);
        assert!(drives.play > 0.20);
        assert!(drives.curiosity > 0.20);
        assert!(drives.autonomy > 0.17);
        assert_eq!(drives.safety, 0.0);
    }

    #[test]
    fn matching_experience_satisfies_without_flattening_every_need() {
        let temperament = temperament();
        let mut drives = Drives {
            sleep: 0.24,
            social: 0.44,
            play: 0.61,
            curiosity: 0.52,
            comfort: 0.31,
            safety: 0.10,
            autonomy: 0.49,
            novelty: 0.56,
        };
        let before = drives;
        let sensors = SensorFrame {
            timestamp: 120.0,
            user_presence: Some(1.0),
            user_availability: Some(0.8),
            ..SensorFrame::default()
        };

        for _ in 0..50 {
            drives.update(&temperament, &sensors, ActionId::SelfPlay, 0.1);
        }
        assert!(
            drives.play >= before.play,
            "a blocked proposal cannot satisfy play"
        );
        drives.relieve_measured(DriveKind::Play, 0.14);
        drives.relieve_measured(DriveKind::Autonomy, 0.12);
        assert!(drives.play < before.play - 0.08);
        assert!(drives.autonomy < before.autonomy - 0.07);
        assert!(drives.social >= before.social);
        assert!(drives.sleep >= before.sleep);
        assert!(drives.play > 0.0);
    }

    #[test]
    fn real_rest_recovers_at_circadian_peak_but_sleep_proposal_does_not() {
        let temperament = temperament();
        let sensors = SensorFrame { time_of_day_01: (temperament.circadian_phase + 0.5).fract(), ..Default::default() };
        let mut sleeping = Drives::initial(&temperament);
        sleeping.sleep = 0.99;
        let mut proposal = sleeping;
        let mut partial = sleeping;
        for _ in 0..900 {
            sleeping.update_with_rest_quality(&temperament, &sensors, ActionId::Sleep, 0.1, 1.0);
            partial.update_with_rest_quality(&temperament, &sensors, ActionId::Sleep, 0.1, 0.5);
            proposal.update(&temperament, &sensors, ActionId::Sleep, 0.1);
        }
        assert!(sleeping.sleep < 0.34);
        assert!(partial.sleep > sleeping.sleep);
        assert!(proposal.sleep > 0.99);
    }
}
