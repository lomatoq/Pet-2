//! Minimal user-facing task language. Runtime progress/receipts are never input.
use crate::{Exercise, ExerciseStep, GameFamily, GroundedMemory, ObjectId, Primitive};
use glam::Vec2;
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProgramSpec {
    pub steps: Vec<StepSpec>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StepSpec {
    pub primitive: Primitive,
    pub target: Option<Vec2>,
    pub tolerance: Option<f32>,
    pub seconds: Option<f32>,
    pub speed: Option<f32>,
}
impl ProgramSpec {
    pub fn compile(
        self,
        episode: u64,
        object: ObjectId,
        origin: Vec2,
        aspect: f32,
        now: f64,
    ) -> Result<Exercise, &'static str> {
        if self.steps.is_empty() || self.steps.len() > 24 {
            return Err("a program needs 1 through 24 bounded steps");
        }
        let mut p = GroundedMemory::compile_exercise(
            episode,
            object,
            GameFamily::OrbitInspect,
            0,
            origin,
            origin,
            aspect,
            now,
        );
        p.steps = self
            .steps
            .into_iter()
            .map(|s| ExerciseStep {
                primitive: s.primitive,
                target: s.target.unwrap_or(origin),
                tolerance: s.tolerance.unwrap_or(0.05),
                deadline: s.seconds.unwrap_or(match s.primitive {
                    Primitive::Approach | Primitive::Carry | Primitive::Visit => 20.0,
                    _ => 5.0,
                }),
                speed: s.speed.unwrap_or(0.24),
            })
            .collect();
        if !p.valid() {
            return Err("invalid target, speed, tolerance or deadline");
        }
        Ok(p)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn simple_composition_has_no_authority_to_import_completed_steps() {
        let p:ProgramSpec=serde_json::from_str(r#"{"steps":[{"primitive":"inspect"},{"primitive":"approach"},{"primitive":"grip"},{"primitive":"carry","target":[0.4,0.7]},{"primitive":"release"},{"primitive":"wait_still","target":[0.4,0.7]}]}"#).unwrap();
        let x = p.compile(1, 2, Vec2::splat(0.5), 1.0, 0.0).unwrap();
        assert_eq!(x.index, 0);
        assert_eq!(x.executed_steps, 0);
        assert!(x.valid());
        assert!(
            serde_json::from_str::<ProgramSpec>(r#"{"steps":[],"executed_steps":100}"#).is_err()
        );
    }
}
