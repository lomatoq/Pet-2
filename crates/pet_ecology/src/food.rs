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
        if !morsel.is_valid() {
            return;
        }
        self.reserve =
            (self.reserve + 0.08 + morsel.value * 0.08).clamp(METABOLIC_RESERVE_FLOOR, 1.0);
        self.satiation = (self.satiation + 0.28).clamp(0.0, 1.0);
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

fn bounded_lerp(current: f32, target: f32, amount: f32) -> f32 {
    (current + (target - current) * amount.clamp(0.0, 1.0)).clamp(-1.0, 1.0)
}
