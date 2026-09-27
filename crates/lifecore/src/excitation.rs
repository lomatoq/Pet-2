//! Bounded excitement bouts driven by observed play and actual motor outcomes.
//! This is a virtual-animal control model, not a biological hormone model.
use serde::{Deserialize, Serialize};

use crate::{AffectState, Drives, TemperamentGenome};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExcitationTactic {
    #[default]
    Dash,
    Orbit,
    Zigzag,
    Roll,
    WallBounce,
}

impl ExcitationTactic {
    pub const ALL: [Self; 5] = [
        Self::Dash,
        Self::Orbit,
        Self::Zigzag,
        Self::Roll,
        Self::WallBounce,
    ];
    pub const fn index(self) -> usize {
        self as usize
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExcitationCue {
    GentleStroke,
    QuickPlay,
    OrbGame,
    NameCall,
}

impl ExcitationCue {
    pub const fn index(self) -> usize {
        self as usize
    }
}

/// Confirmed causal observation supplied by the host, never invented on a tick.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ExcitationFeedback {
    pub event_id: u64,
    pub cue: Option<ExcitationCue>,
    pub tactic: Option<ExcitationTactic>,
    pub arousal_delta: f32,
    pub pleasantness: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ExcitationInput {
    pub play_engagement: f32,
    pub calming_contact: f32,
    pub energy: f32,
    pub physical_load: f32,
    /// Eating, sleep, focus, protection, dragging and other exclusive owners.
    pub blocked: bool,
    /// The prior output was actually accepted and executed by the motor owner.
    pub executing: bool,
    pub wall_available: bool,
    pub allow_roll: bool,
    pub cue: Option<ExcitationCue>,
    pub cue_strength: f32,
    pub feedback: Option<ExcitationFeedback>,
}

impl Default for ExcitationInput {
    fn default() -> Self {
        Self {
            play_engagement: 0.0,
            calming_contact: 0.0,
            energy: 1.0,
            physical_load: 0.0,
            blocked: false,
            executing: false,
            wall_available: false,
            allow_roll: false,
            cue: None,
            cue_strength: 0.0,
            feedback: None,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct ExcitationMotorFrame {
    pub intensity: f32,
    pub tactic: ExcitationTactic,
    pub phase: f32,
    pub recovery: f32,
    pub speed_scale: f32,
    pub turn_bias: f32,
    pub arc_width: f32,
    pub bounce_softness: f32,
    /// Optional face/body orientation request, never a direct physics impulse.
    pub roll_radians: f32,
    /// Slow, bounded colour modulation; not a flashing light.
    pub chroma_pulse: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ExcitationState {
    pub activation: f32,
    pub recovery: f32,
    pub intensity: f32,
    pub phase: f32,
    pub active: bool,
    pub tactic: ExcitationTactic,
    pub episode_count: u64,
    pub executed_seconds: f64,
    pub preferences: [f32; 5],
    pub preference_evidence: [f32; 5],
    pub cue_effects: [f32; 4],
    pub cue_evidence: [f32; 4],
    pub recent_use: [f32; 5],
    recent_feedback: [u64; 16],
    feedback_cursor: usize,
    variant: f32,
    direction: f32,
}

impl Default for ExcitationState {
    fn default() -> Self {
        Self {
            activation: 0.0,
            recovery: 0.0,
            intensity: 0.0,
            phase: 0.0,
            active: false,
            tactic: ExcitationTactic::Dash,
            episode_count: 0,
            executed_seconds: 0.0,
            preferences: [0.0; 5],
            preference_evidence: [0.0; 5],
            cue_effects: [-0.65, 0.65, 0.5, 0.0],
            cue_evidence: [0.0; 4],
            recent_use: [0.0; 5],
            recent_feedback: [0; 16],
            feedback_cursor: 0,
            variant: 0.5,
            direction: 1.0,
        }
    }
}

impl ExcitationState {
    pub fn is_valid(&self) -> bool {
        [
            self.activation,
            self.recovery,
            self.intensity,
            self.phase,
            self.variant,
        ]
        .iter()
        .chain(&self.preference_evidence)
        .chain(&self.cue_evidence)
        .chain(&self.recent_use)
        .all(|v| v.is_finite() && (0.0..=1.0).contains(v))
            && self
                .preferences
                .iter()
                .chain(&self.cue_effects)
                .chain([&self.direction])
                .all(|v| v.is_finite() && (-1.0..=1.0).contains(v))
            && self.executed_seconds.is_finite()
            && self.executed_seconds >= 0.0
            && self.feedback_cursor < 16
    }

    pub fn resume_after_absence(&mut self) {
        self.active = false;
        self.intensity = 0.0;
        self.phase = 0.0;
        // No delayed burst or exhaustion debt on launch. Learned taste remains.
        self.activation = 0.0;
        self.recovery = 0.0;
    }

    pub fn observe_feedback(&mut self, feedback: ExcitationFeedback) {
        if feedback.event_id == 0
            || self.recent_feedback.contains(&feedback.event_id)
            || !feedback.arousal_delta.is_finite()
            || !feedback.pleasantness.is_finite()
        {
            return;
        }
        self.recent_feedback[self.feedback_cursor] = feedback.event_id;
        self.feedback_cursor = (self.feedback_cursor + 1) % self.recent_feedback.len();
        if let Some(cue) = feedback.cue {
            let index = cue.index();
            // Finite memory remains reversible after years of prior experience.
            let rate = 0.08 + (1.0 - self.cue_evidence[index]) * 0.10;
            self.cue_effects[index] +=
                (feedback.arousal_delta.clamp(-1.0, 1.0) - self.cue_effects[index]) * rate;
            self.cue_evidence[index] += (1.0 - self.cue_evidence[index]) * 0.12;
        }
        if let Some(tactic) = feedback.tactic {
            let index = tactic.index();
            let rate = 0.08 + (1.0 - self.preference_evidence[index]) * 0.12;
            self.preferences[index] +=
                (feedback.pleasantness.clamp(-1.0, 1.0) - self.preferences[index]) * rate;
            self.preference_evidence[index] += (1.0 - self.preference_evidence[index]) * 0.12;
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn tick(
        &mut self,
        input: ExcitationInput,
        traits: &TemperamentGenome,
        drives: &mut Drives,
        affect: AffectState,
        identity_seed: u64,
        dt: f32,
    ) -> ExcitationMotorFrame {
        let dt = if dt.is_finite() {
            dt.clamp(0.0, 0.25)
        } else {
            0.0
        };
        if let Some(feedback) = input.feedback {
            self.observe_feedback(feedback);
        }
        if dt <= 0.0 {
            return ExcitationMotorFrame::default();
        }
        let unit = |v: f32| {
            if v.is_finite() {
                v.clamp(0.0, 1.0)
            } else {
                0.0
            }
        };
        let energy = unit(input.energy);
        let load = unit(input.physical_load);
        let calm = unit(input.calming_contact);
        let play = unit(input.play_engagement);
        let fatigue = drives.sleep.max(load * 0.65);
        let vigor = energy * (1.0 - fatigue) * (1.0 - affect.stress.clamp(0.0, 1.0));
        let cue =
            input.cue.map_or(0.0, |cue| self.cue_effects[cue.index()]) * unit(input.cue_strength);
        let excitement = play * (0.18 + traits.playfulness * 0.2 + affect.valence.max(0.0) * 0.12)
            + cue.max(0.0) * 0.25;
        let calm_rate = calm * (0.35 + traits.sociability * 0.25) + (-cue).max(0.0) * 0.3;
        let drain = 0.055 + calm_rate + self.recovery * 0.35;
        let rise = if input.blocked {
            0.0
        } else {
            excitement * vigor
        };
        // Exact exponential integration of charge/recovery avoids cadence drift.
        let equilibrium = rise / (rise + drain).max(0.0001);
        self.activation =
            equilibrium + (self.activation - equilibrium) * (-(rise + drain) * dt).exp();
        self.recovery *= (-dt * (0.025 + calm * 0.035)).exp();
        for recent in &mut self.recent_use {
            *recent *= (-dt / 90.0).exp();
        }
        let tactic_unavailable = self.active
            && ((self.tactic == ExcitationTactic::Roll && !input.allow_roll)
                || (self.tactic == ExcitationTactic::WallBounce && !input.wall_available));
        if input.blocked || tactic_unavailable || energy < 0.15 || drives.safety > 0.55 {
            self.active = false;
            self.intensity = 0.0;
            self.phase = 0.0;
            return ExcitationMotorFrame {
                recovery: self.recovery,
                ..Default::default()
            };
        }
        let threshold = (0.36 + fatigue * 0.3 + self.recovery * 0.4
            - traits.playfulness * 0.10
            - traits.boldness * 0.05)
            .clamp(0.20, 0.85);
        if !self.active && self.activation > threshold && calm < 0.7 {
            self.active = true;
            self.episode_count = self.episode_count.saturating_add(1);
            self.phase = 0.0;
            let mut hash = identity_seed ^ self.episode_count.wrapping_mul(0x9E3779B97F4A7C15);
            hash = (hash ^ (hash >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
            hash ^= hash >> 27;
            self.variant = (hash as u32) as f32 / u32::MAX as f32;
            self.direction = if hash & 1 == 0 { -1.0 } else { 1.0 };
            self.tactic = ExcitationTactic::ALL
                .into_iter()
                .filter(|tactic| {
                    (*tactic != ExcitationTactic::Roll || input.allow_roll)
                        && (*tactic != ExcitationTactic::WallBounce || input.wall_available)
                })
                .max_by(|a, b| {
                    self.tactic_score(*a, traits)
                        .total_cmp(&self.tactic_score(*b, traits))
                })
                .unwrap_or_default();
        }
        if self.active && (self.activation < threshold * 0.45 || self.recovery > 0.85 || calm > 0.7)
        {
            self.active = false;
        }
        let requested = if self.active {
            let recruited = (self.activation / threshold.max(0.1)).min(2.0);
            (recruited * recruited * 0.42 * vigor.sqrt() * (1.0 - self.recovery * 0.6)).min(0.95)
        } else {
            0.0
        };
        self.intensity += (requested - self.intensity) * (1.0 - (-dt / 0.32).exp());
        if input.executing && self.intensity > 0.02 {
            let effort = self.intensity * (0.06 + load * 0.05) * dt;
            self.phase = (self.phase + dt * (0.12 + self.variant * 0.13 + self.intensity * 0.12))
                .rem_euclid(1.0);
            self.executed_seconds += f64::from(dt);
            self.recovery = (self.recovery + effort).min(1.0);
            self.activation = (self.activation - effort * 0.7).max(0.0);
            drives.sleep = (drives.sleep + effort * 0.018).min(1.0);
            drives.play = (drives.play - effort * 0.045).max(0.0);
            self.recent_use[self.tactic.index()] =
                (self.recent_use[self.tactic.index()] + dt * 0.12).min(1.0);
        }
        self.frame()
    }

    fn tactic_score(&self, tactic: ExcitationTactic, traits: &TemperamentGenome) -> f32 {
        let bias = match tactic {
            ExcitationTactic::Dash => traits.boldness * 0.25,
            ExcitationTactic::Orbit => traits.patience * 0.25,
            ExcitationTactic::Zigzag => traits.curiosity * 0.25,
            ExcitationTactic::Roll => traits.playfulness * 0.25,
            ExcitationTactic::WallBounce => traits.persistence * 0.25,
        };
        self.preferences[tactic.index()] * (0.3 + self.preference_evidence[tactic.index()] * 0.5)
            - self.recent_use[tactic.index()] * 0.4
            + bias
            + (self.variant * 17.0 + tactic.index() as f32 * 2.7).sin() * 0.055
    }

    fn frame(&self) -> ExcitationMotorFrame {
        let wave = (self.phase * std::f32::consts::TAU).sin();
        ExcitationMotorFrame {
            intensity: self.intensity,
            tactic: self.tactic,
            phase: self.phase,
            recovery: self.recovery,
            speed_scale: 1.0 + self.intensity * 0.55,
            turn_bias: self.direction * (0.35 + self.variant * 0.45),
            arc_width: 0.3 + self.variant * 0.5,
            bounce_softness: 0.55 + self.variant * 0.35,
            roll_radians: if self.tactic == ExcitationTactic::Roll {
                self.direction * std::f32::consts::PI * wave * (self.intensity / 0.6).min(1.0)
            } else {
                0.0
            },
            chroma_pulse: (0.5 + wave * 0.5) * self.intensity * 0.10,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Genome;

    #[test]
    fn preference_changes_actual_selected_movement_and_capabilities_remain_authoritative() {
        let genome = Genome::from_seed(4);
        let mut drives = Drives::initial(&genome.temperament);
        let mut state = ExcitationState::default();
        for event_id in 1..60 {
            state.observe_feedback(ExcitationFeedback {
                event_id,
                cue: None,
                tactic: Some(ExcitationTactic::Orbit),
                arousal_delta: 0.0,
                pleasantness: 1.0,
            });
        }
        state.activation = 0.8;
        let frame = state.tick(
            warm(),
            &genome.temperament,
            &mut drives,
            Default::default(),
            4,
            0.05,
        );
        assert_eq!(frame.tactic, ExcitationTactic::Orbit);
        for event_id in 60..120 {
            state.observe_feedback(ExcitationFeedback {
                event_id,
                cue: None,
                tactic: Some(ExcitationTactic::Orbit),
                arousal_delta: 0.0,
                pleasantness: -1.0,
            });
        }
        state.active = false;
        let frame = state.tick(
            warm(),
            &genome.temperament,
            &mut drives,
            Default::default(),
            4,
            0.05,
        );
        assert_ne!(frame.tactic, ExcitationTactic::Orbit);
        assert!(!matches!(
            frame.tactic,
            ExcitationTactic::Roll | ExcitationTactic::WallBounce
        ));
        state.tactic = ExcitationTactic::WallBounce;
        state.active = true;
        assert_eq!(
            state
                .tick(
                    warm(),
                    &genome.temperament,
                    &mut drives,
                    Default::default(),
                    4,
                    0.05
                )
                .intensity,
            0.0
        );
    }

    #[test]
    fn learned_cue_effect_changes_future_excitation_and_restores_quiet() {
        let genome = Genome::from_seed(4);
        let drives = Drives::initial(&genome.temperament);
        let mut state = ExcitationState::default();
        for event_id in 1..70 {
            state.observe_feedback(ExcitationFeedback {
                event_id,
                cue: Some(ExcitationCue::GentleStroke),
                tactic: None,
                arousal_delta: 1.0,
                pleasantness: 0.7,
            });
        }
        let mut excited = state.clone();
        let mut excited_drives = drives;
        for _ in 0..600 {
            excited.tick(
                ExcitationInput {
                    cue: Some(ExcitationCue::GentleStroke),
                    cue_strength: 1.0,
                    ..Default::default()
                },
                &genome.temperament,
                &mut excited_drives,
                Default::default(),
                4,
                0.05,
            );
        }
        assert!(excited.intensity > 0.3);
        for event_id in 70..140 {
            state.observe_feedback(ExcitationFeedback {
                event_id,
                cue: Some(ExcitationCue::GentleStroke),
                tactic: None,
                arousal_delta: -1.0,
                pleasantness: 0.7,
            });
        }
        state.activation = excited.activation;
        let mut calm_drives = drives;
        for _ in 0..600 {
            state.tick(
                ExcitationInput {
                    cue: Some(ExcitationCue::GentleStroke),
                    cue_strength: 1.0,
                    ..Default::default()
                },
                &genome.temperament,
                &mut calm_drives,
                Default::default(),
                4,
                0.05,
            );
        }
        assert!(state.activation < 0.01 && state.intensity < 0.01);
    }

    fn warm() -> ExcitationInput {
        ExcitationInput {
            play_engagement: 1.0,
            cue: Some(ExcitationCue::OrbGame),
            cue_strength: 0.8,
            executing: true,
            ..Default::default()
        }
    }

    #[test]
    fn idle_never_creates_a_burst_and_blocked_owner_spends_no_energy() {
        let genome = Genome::from_seed(4);
        let mut drives = Drives::initial(&genome.temperament);
        let mut state = ExcitationState::default();
        for _ in 0..12000 {
            assert_eq!(
                state
                    .tick(
                        Default::default(),
                        &genome.temperament,
                        &mut drives,
                        Default::default(),
                        4,
                        0.05
                    )
                    .intensity,
                0.0
            );
        }
        let sleep = drives.sleep;
        for _ in 0..2000 {
            state.tick(
                ExcitationInput {
                    blocked: true,
                    ..warm()
                },
                &genome.temperament,
                &mut drives,
                Default::default(),
                4,
                0.05,
            );
        }
        assert_eq!(sleep, drives.sleep);
        assert_eq!(state.executed_seconds, 0.0);
    }

    #[test]
    fn warm_play_recruits_bouts_and_calming_recovers_without_replaying() {
        let genome = Genome::from_seed(4);
        let mut drives = Drives::initial(&genome.temperament);
        let mut state = ExcitationState::default();
        let mut peak = 0.0_f32;
        for _ in 0..2400 {
            peak = peak.max(
                state
                    .tick(
                        warm(),
                        &genome.temperament,
                        &mut drives,
                        Default::default(),
                        4,
                        0.05,
                    )
                    .intensity,
            );
        }
        assert!(peak > 0.3 && state.executed_seconds > 3.0 && state.recovery > 0.0);
        let count = state.episode_count;
        for _ in 0..2400 {
            state.tick(
                ExcitationInput {
                    calming_contact: 1.0,
                    ..Default::default()
                },
                &genome.temperament,
                &mut drives,
                Default::default(),
                4,
                0.05,
            );
        }
        assert!(state.intensity < 0.001 && state.recovery < 0.001);
        assert_eq!(state.episode_count, count);
    }

    #[test]
    fn unexecuted_request_does_not_advance_phase_or_give_relief() {
        let genome = Genome::from_seed(4);
        let mut drives = Drives::initial(&genome.temperament);
        let before = drives;
        let mut state = ExcitationState::default();
        for _ in 0..1000 {
            state.tick(
                ExcitationInput {
                    executing: false,
                    ..warm()
                },
                &genome.temperament,
                &mut drives,
                Default::default(),
                4,
                0.05,
            );
        }
        assert!(state.intensity > 0.1);
        assert_eq!(state.phase, 0.0);
        assert_eq!(state.executed_seconds, 0.0);
        assert_eq!(drives, before);
    }

    #[test]
    fn actual_feedback_is_deduplicated_persisted_and_reversible() {
        let mut state = ExcitationState::default();
        let feedback = ExcitationFeedback {
            event_id: 1,
            cue: Some(ExcitationCue::GentleStroke),
            tactic: Some(ExcitationTactic::Orbit),
            arousal_delta: 0.8,
            pleasantness: 0.9,
        };
        state.observe_feedback(feedback);
        let once = state.clone();
        state.observe_feedback(feedback);
        assert_eq!(state, once);
        for event_id in 2..80 {
            state.observe_feedback(ExcitationFeedback {
                event_id,
                ..feedback
            });
        }
        assert!(state.cue_effects[0] > 0.7 && state.preferences[1] > 0.8);
        let mut restored: ExcitationState =
            serde_json::from_str(&serde_json::to_string(&state).unwrap()).unwrap();
        restored.resume_after_absence();
        assert_eq!(restored.preferences, state.preferences);
        for event_id in 80..160 {
            restored.observe_feedback(ExcitationFeedback {
                event_id,
                arousal_delta: -0.8,
                pleasantness: -0.7,
                ..feedback
            });
        }
        assert!(restored.cue_effects[0] < -0.7 && restored.preferences[1] < -0.6);
        assert!(restored.is_valid());
        assert!(
            serde_json::from_str::<ExcitationState>("{}")
                .unwrap()
                .is_valid()
        );
    }

    #[test]
    fn active_simulation_is_stable_across_behavior_cadences() {
        let run = |dt: f32| {
            let genome = Genome::from_seed(13);
            let mut drives = Drives::initial(&genome.temperament);
            let mut state = ExcitationState::default();
            for _ in 0..(40.0 / dt).round() as usize {
                state.tick(
                    warm(),
                    &genome.temperament,
                    &mut drives,
                    Default::default(),
                    13,
                    dt,
                );
            }
            (state, drives)
        };
        let (a, da) = run(0.05);
        let (b, db) = run(0.01);
        assert!((a.activation - b.activation).abs() < 0.03);
        assert!((a.recovery - b.recovery).abs() < 0.03);
        assert!((da.sleep - db.sleep).abs() < 0.005);
        assert!(a.is_valid() && b.is_valid());
    }
}
