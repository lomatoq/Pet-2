//! Runtime-only execution interlock. Bowel mass remains in DigestiveTract;
//! this coordinator grants no relief and never infers sleep from eyelids.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToiletingPhase {
    #[default]
    Idle,
    Blocked,
    Wake,
    Approach,
    Settle,
    Eliminate,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ToiletingEvidence {
    pub needed: bool,
    pub allowed: bool,
    pub sleeping: bool,
    pub waking: bool,
    pub at_site: bool,
    pub supported: bool,
    pub slow: bool,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct ToiletingExecution {
    pub phase: ToiletingPhase,
    pub awake_seconds: f32,
    pub settled_seconds: f32,
    pub reserve_awake: bool,
    pub may_approach: bool,
    pub may_eliminate: bool,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ToiletingCoordinator {
    execution: ToiletingExecution,
}

impl ToiletingCoordinator {
    pub fn step(&mut self, e: ToiletingEvidence, dt: f32) -> ToiletingExecution {
        let dt = if dt.is_finite() { dt.clamp(0.0, 0.05) } else { 0.0 };
        if !e.needed {
            self.execution = ToiletingExecution::default();
            return self.execution;
        }
        let x = &mut self.execution;
        x.reserve_awake = e.allowed;
        x.may_approach = false;
        x.may_eliminate = false;
        if !e.allowed || e.sleeping || e.waking {
            x.awake_seconds = 0.0;
            x.settled_seconds = 0.0;
            x.phase = if e.allowed { ToiletingPhase::Wake } else { ToiletingPhase::Blocked };
            return *x;
        }
        x.awake_seconds = (x.awake_seconds + dt).min(1.0);
        if x.awake_seconds < 0.25 {
            x.phase = ToiletingPhase::Wake;
            x.settled_seconds = 0.0;
            return *x;
        }
        x.may_approach = true;
        if !e.at_site {
            x.phase = ToiletingPhase::Approach;
            x.settled_seconds = 0.0;
        } else if !e.supported || !e.slow {
            x.phase = ToiletingPhase::Settle;
            x.settled_seconds = 0.0;
        } else {
            x.settled_seconds = (x.settled_seconds + dt).min(1.0);
            x.may_eliminate = x.settled_seconds >= 0.35;
            x.phase = if x.may_eliminate { ToiletingPhase::Eliminate } else { ToiletingPhase::Settle };
        }
        *x
    }

    pub const fn execution(&self) -> ToiletingExecution { self.execution }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn ready() -> ToiletingEvidence {
        ToiletingEvidence { needed: true, allowed: true, at_site: true, supported: true, slow: true, ..Default::default() }
    }
    #[test]
    fn sleeping_and_waking_never_grant_navigation_or_elimination() {
        for waking in [false, true] {
            let mut c = ToiletingCoordinator::default();
            for _ in 0..1200 {
                let x = c.step(ToiletingEvidence { sleeping: !waking, waking, ..ready() }, 1.0 / 120.0);
                assert!(x.reserve_awake && !x.may_approach && !x.may_eliminate);
            }
        }
    }
    #[test]
    fn awake_and_supported_evidence_must_be_contiguous_after_interruption() {
        let mut c = ToiletingCoordinator::default();
        for _ in 0..120 { c.step(ready(), 1.0 / 120.0); }
        assert!(c.execution().may_eliminate);
        for e in [ToiletingEvidence { allowed: false, ..ready() }, ToiletingEvidence { supported: false, ..ready() }, ToiletingEvidence { slow: false, ..ready() }] {
            assert!(!c.step(e, 1.0 / 120.0).may_eliminate);
            assert!(!c.step(ready(), 1.0 / 120.0).may_eliminate);
            for _ in 0..120 { c.step(ready(), 1.0 / 120.0); }
            assert!(c.execution().may_eliminate);
        }
    }
    #[test]
    fn no_need_and_invalid_time_cannot_manufacture_a_bout() {
        let mut c = ToiletingCoordinator::default();
        assert_eq!(c.step(ToiletingEvidence::default(), 10.0).phase, ToiletingPhase::Idle);
        for dt in [f32::NAN, f32::INFINITY, -1.0] {
            assert!(!c.step(ready(), dt).may_approach);
        }
    }
}
