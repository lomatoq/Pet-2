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
        current_action: ActionId,
        dt: f32,
    ) {
        let relief = current_action.definition().drive_relief;
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
        let cursor_threat = ((0.20 - sensors.cursor_distance_to_pet) / 0.20).clamp(0.0, 1.0)
            * sensors.cursor_approach_speed.max(0.0).clamp(0.0, 2.0)
            * (1.0 - temperament.boldness);
        let novelty_input = (sensors.cursor_velocity.length() * 0.25
            + sensors.user_activity_rate * 0.3)
            .clamp(0.0, 1.0);

        self.sleep += (0.0025 + circadian_sleep * 0.006) * dt - relief.sleep * dt;
        self.social +=
            (0.0015 + temperament.sociability * 0.003) * user_present * dt - relief.social * dt;
        self.play += (0.001 + temperament.playfulness * 0.0035) * dt - relief.play * dt;
        self.curiosity +=
            (0.001 + temperament.curiosity * novelty_input * 0.004) * dt - relief.curiosity * dt;
        self.comfort += (0.001 + sensors.user_activity_rate * 0.0015) * dt - relief.comfort * dt;
        self.safety += (cursor_threat * 0.08 - 0.012) * dt - relief.safety * dt;
        self.autonomy += (0.001 + (1.0 - temperament.autonomy) * user_present * 0.001) * dt
            - relief.autonomy * dt;
        self.novelty += (0.0014 + (1.0 - novelty_input) * 0.002) * dt - relief.novelty * dt;
        *self = self.bounded();
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
        self.sleep += (0.00035 + 0.00055 * felt.activation + 0.00040 * felt.physical_load
            - 0.0018 * episode.sleeping_or_deep_rest)
            * dt;
        self.social += (0.00025 * episode.user_absent + 0.00035 * episode.ignored_social_bid
            - 0.0016 * episode.safe_social_exchange)
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
                - 0.0013 * felt.comfort)
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
