use serde::{Deserialize, Serialize};

use crate::EcologyError;

pub const METABOLIC_RESERVE_FLOOR: f32 = 0.35;
pub const MAX_FOOD_EFFECT_SECONDS: f32 = 180.0;
pub const MAX_TASTE_LEARNING_RATE: f32 = 0.04;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct MorselProfile {
    pub hue: f32,
    pub saturation: f32,
    pub value: f32,
    pub warmth: f32,
    pub pulse_rate: f32,
    pub stimulation: f32,
    pub cohesion_bias: f32,
    pub novelty: f32,
}

impl MorselProfile {
    #[must_use]
    pub fn is_valid(&self) -> bool {
        [
            self.hue,
            self.saturation,
            self.value,
            self.warmth,
            self.pulse_rate,
            self.stimulation,
            self.cohesion_bias,
            self.novelty,
        ]
        .into_iter()
        .all(|value| value.is_finite() && (0.0..=1.0).contains(&value))
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ConsumedMorselEffect {
    pub hue: f32,
    pub warmth: f32,
    pub stimulation: f32,
    pub cohesion_bias: f32,
    pub remaining_seconds: f32,
}

impl ConsumedMorselEffect {
    #[must_use]
    pub fn is_valid(&self) -> bool {
        [self.hue, self.warmth, self.stimulation, self.cohesion_bias]
            .into_iter()
            .all(|value| value.is_finite() && (0.0..=1.0).contains(&value))
            && self.remaining_seconds.is_finite()
            && (0.0..=MAX_FOOD_EFFECT_SECONDS).contains(&self.remaining_seconds)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct MetabolicState {
    pub reserve: f32,
    pub satiation: f32,
    pub digestion: f32,
    pub active_effect: Option<ConsumedMorselEffect>,
}

impl Default for MetabolicState {
    fn default() -> Self {
        Self {
            reserve: 0.72,
            satiation: 0.32,
            digestion: 0.0,
            active_effect: None,
        }
    }
}

impl MetabolicState {
    /// Empty stomach and depleted reserve both increase eating urgency, while
    /// a full stomach still suppresses intake despite an energy deficit.
    pub fn feeding_appetite(&self) -> f32 {
        let deficit = ((1.0 - self.reserve) / (1.0 - METABOLIC_RESERVE_FLOOR)).clamp(0.0, 1.0);
        ((1.0 - self.satiation).clamp(0.0, 1.0) * (0.65 + 0.35 * deficit)).clamp(0.0, 1.0)
    }

    pub fn validate(&self) -> Result<(), EcologyError> {
        let bounded = self.reserve.is_finite()
            && (METABOLIC_RESERVE_FLOOR..=1.0).contains(&self.reserve)
            && self.satiation.is_finite()
            && (0.0..=1.0).contains(&self.satiation)
            && self.digestion.is_finite()
            && (0.0..=1.0).contains(&self.digestion)
            && self
                .active_effect
                .as_ref()
                .is_none_or(ConsumedMorselEffect::is_valid);
        if bounded {
            Ok(())
        } else {
            Err(EcologyError::InvalidMetabolism)
        }
    }

    /// Offline time restores reserve and settles effects; absence can never
    /// become a starvation or abandonment penalty.
    pub fn apply_offline_seconds(&mut self, seconds: f64) {
        if !seconds.is_finite() || seconds <= 0.0 {
            return;
        }
        let seconds = seconds.min(31_536_000.0) as f32;
        self.reserve = (self.reserve.max(METABOLIC_RESERVE_FLOOR) + seconds * (0.08 / 86_400.0))
            .clamp(METABOLIC_RESERVE_FLOOR, 1.0);
        self.satiation = (self.satiation - seconds * (0.18 / 86_400.0)).clamp(0.0, 1.0);
        self.advance(seconds);
    }

    pub fn advance(&mut self, dt: f32) {
        let dt = if dt.is_finite() {
            dt.clamp(0.0, 60.0)
        } else {
            0.0
        };
        self.reserve = (self.reserve + dt * (0.006 / 60.0)).clamp(METABOLIC_RESERVE_FLOOR, 1.0);
        self.satiation = (self.satiation - dt * (0.012 / 60.0)).clamp(0.0, 1.0);
        if let Some(effect) = &mut self.active_effect {
            effect.remaining_seconds = (effect.remaining_seconds - dt).max(0.0);
            self.digestion = (effect.remaining_seconds / MAX_FOOD_EFFECT_SECONDS).clamp(0.0, 1.0);
            if effect.remaining_seconds <= f32::EPSILON {
                self.active_effect = None;
                self.digestion = 0.0;
            }
        }
    }

    pub fn consume(&mut self, morsel: &MorselProfile) {
        self.consume_portion(morsel, 1.0);
    }

    pub fn consume_portion(&mut self, morsel: &MorselProfile, portion: f32) {
        let portion = if portion.is_finite() {
            portion.clamp(0.0, 1.0)
        } else {
            0.0
        };
        if !morsel.is_valid() {
            return;
        }
        self.reserve = (self.reserve + (0.08 + morsel.value * 0.08) * portion)
            .clamp(METABOLIC_RESERVE_FLOOR, 1.0);
        self.satiation = (self.satiation + 0.28 * portion).clamp(0.0, 1.0);
        self.digestion = 1.0;
        self.active_effect = Some(ConsumedMorselEffect {
            hue: morsel.hue,
            warmth: morsel.warmth,
            stimulation: morsel.stimulation,
            cohesion_bias: morsel.cohesion_bias,
            remaining_seconds: (75.0 + morsel.value * 75.0).min(MAX_FOOD_EFFECT_SECONDS),
        });
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct TasteProfile {
    pub hue_bins: [f32; 12],
    pub warmth_preference: f32,
    pub brightness_preference: f32,
    pub pulse_preference: f32,
    pub confidence: f32,
}

impl Default for TasteProfile {
    fn default() -> Self {
        Self {
            hue_bins: [0.0; 12],
            warmth_preference: 0.0,
            brightness_preference: 0.0,
            pulse_preference: 0.0,
            confidence: 0.0,
        }
    }
}

impl TasteProfile {
    pub fn validate(&self) -> Result<(), EcologyError> {
        let preferences_valid = self
            .hue_bins
            .iter()
            .copied()
            .chain([
                self.warmth_preference,
                self.brightness_preference,
                self.pulse_preference,
            ])
            .all(|value| value.is_finite() && (-1.0..=1.0).contains(&value));
        if preferences_valid
            && self.confidence.is_finite()
            && (0.0..=1.0).contains(&self.confidence)
        {
            Ok(())
        } else {
            Err(EcologyError::InvalidTaste)
        }
    }

    #[must_use]
    pub fn value(&self, morsel: &MorselProfile) -> f32 {
        if !morsel.is_valid() {
            return 0.0;
        }
        let bin = ((morsel.hue.fract() * 12.0).floor() as usize).min(11);
        let warmth = (morsel.warmth * 2.0 - 1.0) * self.warmth_preference;
        let brightness = (morsel.value * 2.0 - 1.0) * self.brightness_preference;
        let pulse = (morsel.pulse_rate * 2.0 - 1.0) * self.pulse_preference;
        (self.hue_bins[bin] * 0.52 + warmth * 0.18 + brightness * 0.18 + pulse * 0.12)
            .clamp(-1.0, 1.0)
    }

    pub fn learn(&mut self, morsel: &MorselProfile, outcome: f32, requested_rate: f32) {
        if !morsel.is_valid() || !outcome.is_finite() {
            return;
        }
        let rate =
            requested_rate.clamp(0.0, MAX_TASTE_LEARNING_RATE) * (1.0 - self.confidence * 0.55);
        let target = outcome.clamp(-1.0, 1.0);
        let bin = ((morsel.hue.fract() * 12.0).floor() as usize).min(11);
        self.hue_bins[bin] = bounded_lerp(self.hue_bins[bin], target, rate);
        self.warmth_preference = bounded_lerp(
            self.warmth_preference,
            target * (morsel.warmth * 2.0 - 1.0),
            rate * 0.55,
        );
        self.brightness_preference = bounded_lerp(
            self.brightness_preference,
            target * (morsel.value * 2.0 - 1.0),
            rate * 0.55,
        );
        self.pulse_preference = bounded_lerp(
            self.pulse_preference,
            target * (morsel.pulse_rate * 2.0 - 1.0),
            rate * 0.45,
        );
        self.confidence = (self.confidence + rate * 0.35).clamp(0.0, 1.0);
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FoodUtility {
    pub reserve_need: f32,
    pub learned_taste: f32,
    pub safe_novelty: f32,
    pub state_match: f32,
    pub satiation_cost: f32,
    pub repetition_cost: f32,
    pub threat_cost: f32,
    pub total: f32,
}

#[must_use]
pub fn evaluate_food_utility(
    metabolism: &MetabolicState,
    taste: &TasteProfile,
    morsel: &MorselProfile,
    threat: f32,
) -> FoodUtility {
    if !morsel.is_valid() {
        return FoodUtility {
            total: -1.0,
            ..FoodUtility::default()
        };
    }
    let reserve_need = ((0.78 - metabolism.reserve) / 0.43).clamp(0.0, 1.0) * 0.42;
    let learned_taste = taste.value(morsel) * (0.18 + taste.confidence * 0.24);
    let safe_novelty = morsel.novelty * 0.18 * (1.0 - threat.clamp(0.0, 1.0));
    let state_match = morsel.stimulation * (1.0 - metabolism.satiation) * 0.12;
    let satiation_cost = metabolism.satiation * 0.46;
    let repetition_cost = metabolism.active_effect.as_ref().map_or(0.0, |effect| {
        let hue_distance = (effect.hue - morsel.hue)
            .abs()
            .min(1.0 - (effect.hue - morsel.hue).abs());
        (1.0 - hue_distance / 0.16).clamp(0.0, 1.0) * 0.34
    });
    let threat_cost = threat.clamp(0.0, 1.0) * 0.62;
    let total = (reserve_need + learned_taste + safe_novelty + state_match
        - satiation_cost
        - repetition_cost
        - threat_cost)
        .clamp(-1.0, 1.0);
    FoodUtility {
        reserve_need,
        learned_taste,
        safe_novelty,
        state_match,
        satiation_cost,
        repetition_cost,
        threat_cost,
        total,
    }
}

/// One appetite/taste decision shared by contact capture and episode selection.
pub fn accepts_morsel(metabolism: &MetabolicState, taste: &TasteProfile,
    morsel: &MorselProfile, threat: f32, crumb: bool) -> bool {
    let utility = evaluate_food_utility(metabolism, taste, morsel, threat).total;
    metabolism.satiation < 0.88 && (if crumb {
        metabolism.feeding_appetite() * 0.8 + utility * 0.2 > 0.10
    } else { utility >= 0.08 })
}

fn bounded_lerp(current: f32, target: f32, amount: f32) -> f32 {
    (current + (target - current) * amount.clamp(0.0, 1.0)).clamp(-1.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn morsel(hue: f32) -> MorselProfile {
        MorselProfile {
            hue,
            saturation: 0.8,
            value: 0.9,
            warmth: 0.6,
            pulse_rate: 0.4,
            stimulation: 0.5,
            cohesion_bias: 0.7,
            novelty: 0.8,
        }
    }

    #[test]
    fn reserve_need_raises_utility_but_never_crosses_the_floor() {
        let taste = TasteProfile::default();
        let mut low = MetabolicState {
            reserve: METABOLIC_RESERVE_FLOOR,
            ..MetabolicState::default()
        };
        let high = MetabolicState::default();
        assert!(
            evaluate_food_utility(&low, &taste, &morsel(0.2), 0.0).total
                > evaluate_food_utility(&high, &taste, &morsel(0.2), 0.0).total
        );
        low.apply_offline_seconds(14.0 * 86_400.0);
        assert!(low.reserve >= METABOLIC_RESERVE_FLOOR);
    }

    #[test]
    fn repeated_flavor_is_less_useful_while_effect_is_active() {
        let taste = TasteProfile::default();
        let mut metabolism = MetabolicState::default();
        let profile = morsel(0.2);
        let before = evaluate_food_utility(&metabolism, &taste, &profile, 0.0).total;
        metabolism.consume(&profile);
        let repeated = evaluate_food_utility(&metabolism, &taste, &profile, 0.0).total;
        assert!(repeated < before);
    }
}

/// A bite is driven by appetite, bolus size and the food's cohesion; no
/// unrelated random gag or fixed animation clip is scheduled.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FeedingBite {
    pub chew_seconds: f32,
    pub chew_hz: f32,
    pub effort: f32,
    pub settle_seconds: f32,
    pub cohesion: f32,
}
impl FeedingBite {
    pub fn new(metabolism: &MetabolicState, food: &MorselProfile, portion: f32) -> Self {
        let appetite = metabolism.feeding_appetite();
        let load = portion.clamp(0.0, 1.0).sqrt();
        // A hasty large cohesive bolus can exceed comfortable processing
        // capacity. It causes a small closed-mouth settling pause, not illness.
        let excess = (load * (0.55 + food.cohesion_bias * 0.45) * appetite - 0.68).max(0.0);
        let settle_seconds = (excess * 1.7).min(0.42);
        Self {
            chew_seconds: 0.45
                + load * 0.85
                + (1.0 - appetite) * 1.1
                + food.cohesion_bias * 0.4
                + settle_seconds,
            chew_hz: 1.4 + appetite * 1.7 - food.cohesion_bias * 0.3,
            effort: 0.04 + load * 0.11 + food.cohesion_bias * 0.04,
            settle_seconds,
            cohesion: food.cohesion_bias,
        }
    }
    pub fn settling(self, elapsed: f32) -> bool {
        self.settle_seconds > 0.0 && (0.28..0.28 + self.settle_seconds).contains(&elapsed)
    }
    pub fn aperture(self, elapsed: f32) -> f32 {
        if elapsed < 0.14 || elapsed > self.chew_seconds - 0.2 || self.settling(elapsed) {
            return 0.0;
        }
        // Remove the settling time from jaw phase: the jaw resumes where it
        // stopped. As the bolus breaks down, its resistance and cycle duration
        // fall; closing remains quicker than opening under cohesive load.
        let t = (elapsed - 0.14 - (elapsed - 0.28).clamp(0.0, self.settle_seconds)).max(0.0);
        let duration = (self.chew_seconds - self.settle_seconds).max(0.1);
        let progress = (t / duration).clamp(0.0, 1.0);
        let cycles = self.chew_hz * (t * 0.78 + 0.22 * t * t / duration);
        let phase = cycles.fract();
        let opening_share = 0.58 + self.cohesion * (1.0 - progress) * 0.12;
        let jaw = if phase < opening_share {
            phase / opening_share
        } else {
            (1.0 - phase) / (1.0 - opening_share)
        };
        let smooth_jaw = jaw * jaw * (3.0 - 2.0 * jaw);
        smooth_jaw * self.effort * (1.0 - progress * 0.7)
    }
}

#[cfg(test)]
mod feeding_tests {
    use super::*;
    #[test]
    fn reserve_and_excess_bolus_causally_control_settling() {
        let food = MorselProfile {
            hue: 0.2,
            saturation: 0.5,
            value: 0.8,
            warmth: 0.5,
            pulse_rate: 0.3,
            stimulation: 0.4,
            cohesion_bias: 1.0,
            novelty: 0.3,
        };
        let depleted = MetabolicState {
            reserve: METABOLIC_RESERVE_FLOOR,
            satiation: 0.02,
            ..Default::default()
        };
        let rested = MetabolicState {
            reserve: 1.0,
            ..depleted.clone()
        };
        let hasty = FeedingBite::new(&depleted, &food, 1.0);
        let easy = FeedingBite::new(&depleted, &food, 0.125);
        let rested = FeedingBite::new(&rested, &food, 1.0);
        assert!(hasty.chew_hz > rested.chew_hz);
        assert!(hasty.settle_seconds > 0.0 && hasty.settle_seconds <= 0.42);
        assert_eq!(easy.settle_seconds, 0.0);
        assert_eq!(rested.settle_seconds, 0.0);
        assert_eq!(hasty.aperture(0.28 + hasty.settle_seconds * 0.5), 0.0);
        for i in 0..500 {
            let a = hasty.aperture(i as f32 * 0.01);
            assert!(a.is_finite() && (0.0..=0.25).contains(&a));
        }
        // Later jaw cycles accelerate as food softens instead of replaying a
        // constant-frequency clip. Compare successive maxima in the easy bite.
        let samples: Vec<f32> = (0..250).map(|i| easy.aperture(i as f32 * 0.01)).collect();
        let peaks: Vec<usize> = (1..samples.len() - 1)
            .filter(|&i| samples[i] > samples[i - 1] && samples[i] > samples[i + 1])
            .collect();
        assert!(peaks.len() >= 3);
        assert!(peaks[2] - peaks[1] < peaks[1] - peaks[0]);
    }

    #[test]
    fn hunger_and_food_load_control_chewing_and_contact_closes_mouth() {
        let food = MorselProfile {
            hue: 0.2,
            saturation: 0.5,
            value: 0.8,
            warmth: 0.5,
            pulse_rate: 0.3,
            stimulation: 0.4,
            cohesion_bias: 0.6,
            novelty: 0.3,
        };
        let hungry = FeedingBite::new(
            &MetabolicState {
                satiation: 0.05,
                ..Default::default()
            },
            &food,
            0.125,
        );
        let full = FeedingBite::new(
            &MetabolicState {
                satiation: 0.85,
                ..Default::default()
            },
            &food,
            0.125,
        );
        let large = FeedingBite::new(
            &MetabolicState {
                satiation: 0.05,
                ..Default::default()
            },
            &food,
            1.0,
        );
        assert!(hungry.chew_seconds < full.chew_seconds && hungry.chew_hz > full.chew_hz);
        assert!(large.chew_seconds > hungry.chew_seconds);
        assert_eq!(hungry.aperture(0.0), 0.0);
        assert_eq!(hungry.aperture(hungry.chew_seconds), 0.0);
        assert!((1..80).any(|i| hungry.aperture(i as f32 * 0.02) > 0.02));
    }
}

/// Contact-to-swallow time: hungry bites are quicker, never instantaneous.
pub fn ingestion_seconds(appetite: f32) -> f32 { 0.12 + (1.0 - appetite.clamp(0.0, 1.0)) * 0.18 }
pub fn ingestion_progress(elapsed: f32, appetite: f32) -> f32 {
    let t = (elapsed / ingestion_seconds(appetite)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}
