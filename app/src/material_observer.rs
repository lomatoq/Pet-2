//! Bounded numeric evidence for rare material failures, available without the Lab.
use pet_body::LiquidDiagnostics;
use serde::Serialize;
use std::collections::VecDeque;
#[derive(Clone, Copy, Debug, Serialize)]
pub struct Sample {
    pub seconds: f64,
    pub components: usize,
    pub main_mass: f32,
    pub detached_mass: f32,
    pub density_error: f32,
    pub speed: f32,
    pub stretch: f32,
    pub compression: f32,
    pub failsafe_hits: u64,
    pub recoveries: u64,
    pub finite: bool,
}
#[derive(Default)]
pub struct MaterialObserver {
    sample_elapsed: f32,
    report_elapsed: f32,
    cooldown: f32,
    last_failsafe: u64,
    last_recovery: u64,
    last_components: usize,
    last_detached_mass: f32,
    history: VecDeque<Sample>,
}
pub struct Observation {
    pub anomaly: bool,
    pub sample: Sample,
    pub history: Vec<Sample>,
}
impl MaterialObserver {
    pub fn observe(
        &mut self,
        d: LiquidDiagnostics,
        seconds: f64,
        dt: f32,
        externally_manipulated: bool,
    ) -> Option<Observation> {
        self.sample_elapsed += dt;
        self.report_elapsed += dt;
        self.cooldown = (self.cooldown - dt).max(0.0);
        let solver_failure = !d.finite
            || d.failsafe_hits > self.last_failsafe
            || d.recovery_count > self.last_recovery;
        let spontaneous_loss = !externally_manipulated
            && (d.component_count > 1
                || d.detached_mass > 0.0
                || d.maximum_speed > 2.0
                || d.stretch_ratio > 3.5
                || d.maximum_compression > 0.6);
        // A velocity warning must not hide a later topology failure during
        // its deduplication window. Record each new loss immediately.
        let new_fragmentation = !externally_manipulated
            && ((d.component_count > 1 && d.component_count > self.last_components)
                || d.detached_mass > self.last_detached_mass + 0.5);
        let anomaly = solver_failure
            || new_fragmentation
            || (spontaneous_loss && self.cooldown <= 0.0);
        self.last_failsafe = d.failsafe_hits;
        self.last_recovery = d.recovery_count;
        self.last_components = d.component_count;
        self.last_detached_mass = d.detached_mass;
        if self.sample_elapsed < 0.2 && !anomaly {
            return None;
        }
        self.sample_elapsed = 0.0;
        let sample = Sample {
            seconds,
            components: d.component_count,
            main_mass: d.main_mass,
            detached_mass: d.detached_mass,
            density_error: d.density_error,
            speed: d.maximum_speed,
            stretch: d.stretch_ratio,
            compression: d.maximum_compression,
            failsafe_hits: d.failsafe_hits,
            recoveries: d.recovery_count,
            finite: d.finite,
        };
        if self.history.len() == 24 {
            self.history.pop_front();
        }
        self.history.push_back(sample);
        if !anomaly && self.report_elapsed < 2.0 {
            return None;
        }
        self.report_elapsed = 0.0;
        if anomaly {
            self.cooldown = 5.0;
        }
        Some(Observation {
            anomaly,
            sample,
            history: if anomaly {
                self.history.iter().copied().collect()
            } else {
                Vec::new()
            },
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn healthy() -> LiquidDiagnostics {
        LiquidDiagnostics {
            finite: true,
            particle_count: 96,
            component_count: 1,
            main_mass: 96.0,
            stretch_ratio: 1.0,
            ..Default::default()
        }
    }
    #[test]
    fn catches_a_one_tick_failure_between_periodic_samples_and_retains_preceding_context() {
        let mut o = MaterialObserver::default();
        for i in 0..120 {
            o.observe(healthy(), i as f64 / 120.0, 1.0 / 120.0, false);
        }
        let mut d = healthy();
        d.failsafe_hits = 1;
        let event = o.observe(d, 1.001, 1.0 / 120.0, false).unwrap();
        assert!(event.anomaly);
        assert!(event.history.len() > 1);
        assert!(o.observe(d, 1.01, 1.0 / 120.0, false).is_none());
    }
    #[test]
    fn deliberate_manipulation_does_not_masquerade_as_spontaneous_fragmentation() {
        let mut d = healthy();
        d.detached_mass = 30.0;
        d.main_mass = 66.0;
        let mut o = MaterialObserver::default();
        assert!(o.observe(d, 0.0, 0.2, true).is_none());
        assert!(o.observe(d, 0.2, 0.2, false).unwrap().anomaly);
    }
    #[test]
    fn catches_a_small_detachment_even_when_motion_is_quiet() {
        let mut d = healthy();
        d.component_count = 2;
        d.detached_mass = 1.0;
        d.main_mass = 95.0;
        let event = MaterialObserver::default()
            .observe(d, 0.0, 1.0 / 120.0, false)
            .unwrap();
        assert!(event.anomaly);
        assert_eq!(event.sample.components, 2);
    }
    #[test]
    fn velocity_warning_cooldown_cannot_hide_a_later_detachment() {
        let mut observer = MaterialObserver::default();
        let mut d = healthy();
        d.maximum_speed = 2.5;
        assert!(observer.observe(d, 0.0, 1.0 / 120.0, false).unwrap().anomaly);
        d.maximum_speed = 0.1;
        d.component_count = 2;
        d.detached_mass = 1.0;
        assert!(observer.observe(d, 0.01, 1.0 / 120.0, false).unwrap().anomaly);
    }
}
