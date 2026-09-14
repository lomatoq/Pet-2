//! Fictional throat effort, sourced from measured synthesizer output and finite
//! virtual sensory doses. Not illness, real inhalation or microphone inference.
use crate::RepertoireEffect;
use serde::Serialize;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub enum VirtualThroatPhase {
    #[default]
    Idle,
    Notice,
    Swallow,
    Clear,
    Recover,
}

#[derive(Debug, Clone, Copy, Default, Serialize)]
pub struct VirtualThroatStatus {
    pub phase: VirtualThroatPhase,
    pub virtual_dose: f32,
    pub measured_voice_effort: f32,
    pub cooldown_seconds: f32,
}

#[derive(Debug, Clone, Default)]
pub struct VirtualThroatState {
    status: VirtualThroatStatus,
    elapsed: f32,
    previous_irritation: f32,
    needs_clear: bool,
    blocked_seconds: f32,
}

impl VirtualThroatState {
    pub fn status(&self) -> VirtualThroatStatus {
        self.status
    }

    pub fn tick(
        &mut self,
        voice_active: bool,
        measured_effort: f32,
        virtual_irritation: f32,
        gesture_allowed: bool,
        dt: f32,
    ) -> RepertoireEffect {
        let dt = if dt.is_finite() {
            dt.clamp(0.0, 0.25)
        } else {
            0.0
        };
        let effort = if measured_effort.is_finite() && voice_active {
            measured_effort.clamp(0.0, 1.0)
        } else {
            0.0
        };
        let irritation = if virtual_irritation.is_finite() {
            virtual_irritation.clamp(0.0, 1.0)
        } else {
            0.0
        };
        let fresh_dose = (irritation - self.previous_irritation).max(0.0);
        self.previous_irritation = irritation;
        self.status.measured_voice_effort = effort;
        self.status.cooldown_seconds = (self.status.cooldown_seconds - dt).max(0.0);
        self.status.virtual_dose = (self.status.virtual_dose * (-dt / 18.0).exp()
            + fresh_dose * 0.65
            + effort * effort * dt * 0.09)
            .clamp(0.0, 1.0);
        if !gesture_allowed || voice_active {
            // No hidden swallow while the mouth is owned by real speech.
            // A delayed need decays normally; it does not force a voice stop.
            self.blocked_seconds += dt;
            if self.blocked_seconds > 3.0 {
                self.status.phase = VirtualThroatPhase::Idle;
                self.elapsed = 0.0;
            }
            return RepertoireEffect::ZERO;
        }
        self.blocked_seconds = 0.0;
        if self.status.phase == VirtualThroatPhase::Idle {
            if self.status.cooldown_seconds > 0.0 || self.status.virtual_dose < 0.18 {
                return RepertoireEffect::ZERO;
            }
            self.needs_clear = self.status.virtual_dose > 0.36;
            self.status.phase = VirtualThroatPhase::Notice;
            self.elapsed = 0.0;
        }
        self.elapsed += dt;
        let duration = match self.status.phase {
            VirtualThroatPhase::Notice => 0.35,
            VirtualThroatPhase::Swallow => 0.65,
            VirtualThroatPhase::Clear => 0.45,
            VirtualThroatPhase::Recover => 1.1,
            VirtualThroatPhase::Idle => 1.0,
        };
        let t = (self.elapsed / duration).clamp(0.0, 1.0);
        let pulse = (4.0 * t * (1.0 - t)).powi(2);
        let mut effect = RepertoireEffect::ZERO;
        match self.status.phase {
            VirtualThroatPhase::Notice => {
                effect.mouth_asymmetry = 0.10 * pulse;
                effect.breath = -0.08 * pulse;
            }
            VirtualThroatPhase::Swallow => {
                effect.mouth_open = -0.12 * pulse;
                effect.mouth_curve = -0.06 * pulse;
                effect.breath = -0.16 * pulse;
            }
            VirtualThroatPhase::Clear => {
                effect.mouth_open = 0.13 * pulse;
                effect.mouth_asymmetry = 0.08 * pulse;
                effect.breath = 0.22 * pulse;
            }
            VirtualThroatPhase::Recover => {
                effect.mouth_curve = 0.04 * pulse;
                effect.breath = 0.10 * pulse;
            }
            VirtualThroatPhase::Idle => {}
        }
        if self.elapsed >= duration {
            self.elapsed = 0.0;
            self.status.phase = match self.status.phase {
                VirtualThroatPhase::Notice => VirtualThroatPhase::Swallow,
                VirtualThroatPhase::Swallow if self.needs_clear => VirtualThroatPhase::Clear,
                VirtualThroatPhase::Swallow | VirtualThroatPhase::Clear => {
                    VirtualThroatPhase::Recover
                }
                _ => VirtualThroatPhase::Idle,
            };
            if self.status.phase == VirtualThroatPhase::Idle {
                self.status.virtual_dose *= 0.08;
                self.status.cooldown_seconds = 45.0;
            }
        }
        effect
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn measured_voice_bout_causes_one_finite_recovery_not_periodic_illness() {
        let mut a = VirtualThroatState::default();
        for _ in 0..160 {
            assert_eq!(a.tick(true, 0.9, 0.0, false, 0.05), RepertoireEffect::ZERO);
        }
        let mut b = a.clone();
        let mut phases = Vec::new();
        for _ in 0..2400 {
            let effect = a.tick(false, 0.0, 0.0, true, 0.05);
            assert_eq!(effect, b.tick(false, 0.0, 0.0, true, 0.05));
            if phases.last() != Some(&a.status.phase) {
                phases.push(a.status.phase);
            }
        }
        assert!(phases.contains(&VirtualThroatPhase::Swallow));
        assert!(phases.contains(&VirtualThroatPhase::Clear));
        assert_eq!(
            phases
                .iter()
                .filter(|&&p| p == VirtualThroatPhase::Notice)
                .count(),
            1
        );
        assert_eq!(a.status.phase, VirtualThroatPhase::Idle);
        assert!(a.status.virtual_dose < 0.001);
    }
    #[test]
    fn static_virtual_level_does_not_repeatedly_create_doses_and_busy_is_silent() {
        let mut s = VirtualThroatState::default();
        assert_eq!(s.tick(false, 0.0, 0.4, false, 0.05), RepertoireEffect::ZERO);
        for _ in 0..100 {
            assert_eq!(s.tick(false, 0.0, 0.4, false, 0.05), RepertoireEffect::ZERO);
        }
        let mut visible = 0;
        for _ in 0..3000 {
            visible += usize::from(s.tick(false, 0.0, 0.4, true, 0.05) != RepertoireEffect::ZERO);
        }
        assert!(visible > 0 && visible < 60);
        assert_eq!(s.status.phase, VirtualThroatPhase::Idle);
    }
}
