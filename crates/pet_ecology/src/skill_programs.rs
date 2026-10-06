//! Successful compositions become reusable skills, not just another named pose.
//! Targets are transferred in normalized local space and revalidated on replay.
use crate::grounded_memory::point;
use crate::{Exercise, ExerciseStep, GameFamily, GroundedMemory, Primitive};
use glam::Vec2;
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LearnedProgram {
    pub signature: u64,
    pub family: GameFamily,
    pub level: u8,
    pub origin: Vec2,
    pub aspect: f32,
    pub steps: Vec<ExerciseStep>,
    pub successes: u32,
    pub last_episode: u64,
    pub last_used: f64,
}
impl LearnedProgram {
    pub fn valid(&self) -> bool {
        self.signature > 0
            && self.level < 5
            && point(self.origin)
            && self.aspect.is_finite()
            && (0.25..=8.0).contains(&self.aspect)
            && self.successes > 0
            && self.last_episode > 0
            && self.last_used.is_finite()
            && self.last_used >= 0.0
            && self.steps.len() >= 2
            && self.steps.len() <= 24
            && self.steps.iter().all(|s| {
                s.target.is_finite()
                    && point(s.target)
                    && s.tolerance.is_finite()
                    && (0.005..=0.3).contains(&s.tolerance)
                    && s.deadline.is_finite()
                    && (0.1..=40.0).contains(&s.deadline)
                    && s.speed.is_finite()
                    && (0.0..=0.8).contains(&s.speed)
            })
    }
}
fn code(p: Primitive) -> u8 {
    match p {
        Primitive::Inspect => 1,
        Primitive::Approach => 2,
        Primitive::Push => 3,
        Primitive::Brake => 4,
        Primitive::Grip => 5,
        Primitive::Carry => 6,
        Primitive::Release => 7,
        Primitive::ObserveMotion => 8,
        Primitive::ObserveRebound => 9,
        Primitive::WaitStill => 10,
        Primitive::Visit => 11,
    }
}
impl GroundedMemory {
    pub(crate) fn remember_program(&mut self, x: &Exercise) {
        if x.executed_steps == 0 || x.steps.len() < 2 || x.index != x.steps.len() {
            return;
        }
        let mut signature = 14695981039346656037_u64;
        let mut mix = |value: u64| {
            signature ^= value;
            signature = signature.wrapping_mul(1099511628211);
        };
        mix(x.family as u64);
        for step in &x.steps {
            mix(u64::from(code(step.primitive)));
            // Two different local spatial programs must not collapse merely
            // because they use the same primitive names. Stable 2% bins ignore
            // subpixel drift; effort calibration remains a learned parameter.
            let relative = (step.target - x.reference_origin)
                * crate::grounded_memory::metric(x.reference_aspect);
            for component in [relative.x, relative.y] {
                mix((component * 50.0).round() as i32 as u32 as u64);
            }
        }
        let signature = signature.max(1);
        if let Some(p) = self.programs.iter_mut().find(|p| p.signature == signature) {
            if p.last_episode == x.episode_id {
                return;
            }
            p.successes = p.successes.saturating_add(1);
            p.last_episode = x.episode_id;
            p.last_used = self.clock;
            p.steps = x.steps.clone();
            p.origin = x.reference_origin;
            p.aspect = x.reference_aspect;
            p.level = x.level;
            return;
        }
        if self.programs.len() == 12 {
            let i = self
                .programs
                .iter()
                .enumerate()
                .min_by_key(|(_, p)| p.successes)
                .map(|(i, _)| i)
                .unwrap_or(0);
            self.programs.remove(i);
        }
        self.programs.push(LearnedProgram {
            signature,
            family: x.family,
            level: x.level,
            origin: x.reference_origin,
            aspect: x.reference_aspect,
            steps: x.steps.clone(),
            successes: 1,
            last_episode: x.episode_id,
            last_used: self.clock,
        });
    }
    pub(crate) fn recall_program(&self, mut plan: Exercise) -> Option<Exercise> {
        let p = self
            .programs
            .iter()
            .filter(|p| p.successes >= 2 && p.family == plan.family && p.level == plan.level)
            .max_by_key(|p| p.successes)?;
        let origin = plan.reference_origin;
        let aspect = plan.reference_aspect;
        let home = plan
            .steps
            .iter()
            .find(|s| s.primitive == Primitive::Carry)
            .map(|s| s.target);
        plan.steps = p
            .steps
            .iter()
            .cloned()
            .map(|mut s| {
                s.target = if matches!(
                    s.primitive,
                    Primitive::Inspect | Primitive::Approach | Primitive::Grip | Primitive::Brake
                ) {
                    origin
                } else if plan.family == GameFamily::Fetch
                    && matches!(
                        s.primitive,
                        Primitive::Carry | Primitive::Release | Primitive::WaitStill
                    )
                    && let Some(home) = home
                {
                    // Fetch means the current den, not a memorized offset that
                    // changes destination when the toy or den has moved.
                    home
                } else {
                    (origin
                        + (s.target - p.origin) * crate::grounded_memory::metric(p.aspect)
                            / crate::grounded_memory::metric(aspect))
                    .clamp(Vec2::splat(0.04), Vec2::splat(0.96))
                };
                s
            })
            .collect();
        plan.prepare_resume();
        Some(plan)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_completed_executed_compositions_enter_memory_and_duplicates_do_not_reinforce() {
        let mut m = GroundedMemory::default();
        let mut x = GroundedMemory::compile_exercise(
            1,
            2,
            GameFamily::Shuttle,
            0,
            Vec2::splat(0.5),
            Vec2::splat(0.7),
            1.0,
            0.0,
        );
        m.remember_program(&x);
        assert!(m.programs.is_empty());
        x.index = x.steps.len();
        x.executed_steps = 3;
        m.remember_program(&x);
        m.remember_program(&x);
        assert_eq!(m.programs[0].successes, 1);
        x.episode_id = 2;
        m.remember_program(&x);
        m.clock = 100.0;
        let candidate = GroundedMemory::compile_exercise(
            3,
            2,
            GameFamily::Shuttle,
            0,
            Vec2::splat(0.3),
            Vec2::splat(0.8),
            1.0,
            m.clock,
        );
        let p = m.recall_program(candidate).unwrap();
        assert!(p.valid());
        assert_eq!(p.steps[0].target, Vec2::splat(0.3));
        assert_eq!(p.executed_steps, 0);
        assert!(m.valid());
    }

    #[test]
    fn familiar_program_does_not_preempt_an_unlearned_family_or_depend_on_waiting() {
        let mut m = GroundedMemory::default();
        let mut x = GroundedMemory::compile_exercise(
            1,
            2,
            GameFamily::Roll,
            0,
            Vec2::splat(0.5),
            Vec2::splat(0.9),
            2.4,
            0.0,
        );
        x.index = x.steps.len();
        x.executed_steps = 3;
        m.remember_program(&x);
        x.episode_id = 2;
        m.remember_program(&x);
        let mut trials = [crate::TrialEstimate::default(); 5];
        trials[0] = crate::TrialEstimate {
            successes: 2,
            attempts: 2,
            error_ema: 0.0,
        };
        m.skills.push(crate::SkillRecord {
            family: GameFamily::Roll,
            level: 0,
            trials,
            calibration: [1.0; 5],
            last_practiced: 0.0,
            last_episode: 2,
        });
        let choose = |m: &GroundedMemory| {
            m.curriculum(
                3,
                2,
                Vec2::splat(0.5),
                Vec2::splat(0.9),
                2.4,
                false,
                Vec2::ZERO,
            )
        };
        let first = choose(&m);
        assert_ne!(first.family, GameFamily::Roll);
        m.clock = 1_000_000.0;
        assert_eq!(first.family, choose(&m).family);
        assert_eq!(m.stats.plan_successes, 0);
    }

    #[test]
    fn recalled_fetch_targets_current_home_after_den_and_orb_relocation() {
        let mut m = GroundedMemory::default();
        let mut x = GroundedMemory::compile_exercise(
            1,
            2,
            GameFamily::Fetch,
            0,
            Vec2::new(0.4, 0.5),
            Vec2::new(0.8, 0.8),
            1.5,
            0.0,
        );
        x.index = x.steps.len();
        x.executed_steps = 4;
        m.remember_program(&x);
        x.episode_id = 2;
        m.remember_program(&x);
        let home = Vec2::new(0.95, 0.9);
        let candidate = GroundedMemory::compile_exercise(
            3,
            2,
            GameFamily::Fetch,
            0,
            Vec2::new(0.2, 0.3),
            home,
            2.4,
            0.0,
        );
        let recalled = m.recall_program(candidate).unwrap();
        for step in &recalled.steps {
            if matches!(
                step.primitive,
                Primitive::Carry | Primitive::Release | Primitive::WaitStill
            ) {
                assert_eq!(step.target, home);
            }
        }
        assert!(recalled.valid());
    }
}
