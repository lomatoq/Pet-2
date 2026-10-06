//! Small symbolic search over executable motor skills. Operator effects are
//! hypotheses for planning; only the exercise runtime can confirm completion.
use crate::grounded_memory::{metric, point};
use crate::{Exercise, ExerciseStep, GameFamily, GroundedMemory, ObjectBelief, Primitive};
use glam::Vec2;
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GoalKind {
    PlaceToy,
    StopToy,
    InspectToy,
    CarryToy,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GoalRequest {
    pub kind: GoalKind,
    pub target: Vec2,
    pub tolerance: f32,
}
#[derive(Clone)]
struct Node {
    bits: u8,
    cost: f32,
    path: Vec<Primitive>,
}
const CONTACT: u8 = 1;
const HELD: u8 = 2;
const AT_TARGET: u8 = 4;
const STILL: u8 = 8;
const INSPECTED: u8 = 16;
impl GroundedMemory {
    pub fn plan_goal(
        &self,
        episode: u64,
        belief: &ObjectBelief,
        request: GoalRequest,
        aspect: f32,
    ) -> Result<Exercise, &'static str> {
        if !point(request.target)
            || !request.tolerance.is_finite()
            || !(0.018..=0.15).contains(&request.tolerance)
            || !belief.visible
            || self.clock - belief.last_seen > 0.3
        {
            return Err("goal requires a fresh observed object and a bounded target");
        }
        let near = ((belief.last_position - request.target) * metric(aspect)).length()
            <= request.tolerance;
        if (request.kind == GoalKind::PlaceToy && near && belief.velocity.length() < 0.04)
            || (request.kind == GoalKind::StopToy && belief.velocity.length() < 0.04)
        {
            return Err("goal is already satisfied; no training event created");
        }
        let initial = if near { AT_TARGET } else { 0 }
            | if belief.velocity.length() < 0.04 {
                STILL
            } else {
                0
            };
        let goal = |bits: u8| match request.kind {
            GoalKind::PlaceToy => {
                bits & AT_TARGET != 0
                    && bits & STILL != 0
                    && bits & HELD == 0
                    && bits & INSPECTED != 0
            }
            GoalKind::StopToy => bits & STILL != 0 && bits & INSPECTED != 0,
            GoalKind::InspectToy => bits & INSPECTED != 0,
            GoalKind::CarryToy => bits & AT_TARGET != 0 && bits & HELD != 0,
        };
        let mut frontier = vec![Node {
            bits: initial,
            cost: 0.0,
            path: Vec::new(),
        }];
        let mut best = [f32::INFINITY; 32];
        best[initial as usize] = 0.0;
        let mut solution = None;
        for _ in 0..64 {
            if frontier.is_empty() {
                break;
            }
            let index = frontier
                .iter()
                .enumerate()
                .min_by(|(_, a), (_, b)| a.cost.total_cmp(&b.cost))
                .unwrap()
                .0;
            let node = frontier.remove(index);
            if goal(node.bits) {
                solution = Some(node.path);
                break;
            }
            if node.path.len() >= 12 {
                continue;
            }
            let b = node.bits;
            let candidates = [
                (Primitive::Inspect, b | INSPECTED, true, 0.5),
                (Primitive::Approach, b | CONTACT, b & INSPECTED != 0, 1.0),
                (Primitive::Grip, b | HELD | CONTACT, b & CONTACT != 0, 1.0),
                (
                    Primitive::Carry,
                    (b | AT_TARGET) & !STILL,
                    b & HELD != 0,
                    2.0 + ((belief.last_position - request.target) * metric(aspect)).length() * 5.0,
                ),
                (Primitive::Release, b & !(HELD | STILL), b & HELD != 0, 0.8),
                (
                    Primitive::Brake,
                    b | STILL,
                    b & CONTACT != 0 && b & HELD == 0,
                    1.5 + (1.0 - belief.confidence()),
                ),
                (
                    Primitive::WaitStill,
                    b | STILL,
                    b & HELD == 0 && b & AT_TARGET != 0,
                    1.0,
                ),
            ];
            for (op, next, allowed, cost) in candidates {
                if !allowed || next == b {
                    continue;
                }
                let candidate = node.cost + cost;
                if candidate >= best[next as usize] {
                    continue;
                }
                best[next as usize] = candidate;
                let mut path = node.path.clone();
                path.push(op);
                frontier.push(Node {
                    bits: next,
                    cost: candidate,
                    path,
                });
            }
        }
        let path = solution.ok_or("no safe plan within the search budget")?;
        if path.is_empty() {
            return Err("goal already satisfied; no learning event is fabricated");
        }
        let mut plan = Self::compile_exercise(
            episode,
            belief.id,
            match request.kind {
                GoalKind::StopToy => GameFamily::Stop,
                GoalKind::InspectToy => GameFamily::OrbitInspect,
                _ => GameFamily::Fetch,
            },
            0,
            belief.last_position,
            request.target,
            aspect,
            self.clock,
        );
        plan.steps = path
            .into_iter()
            .map(|primitive| ExerciseStep {
                primitive,
                target: if matches!(
                    primitive,
                    Primitive::Carry | Primitive::Release | Primitive::WaitStill
                ) {
                    request.target
                } else {
                    belief.last_position
                },
                tolerance: request.tolerance,
                deadline: match primitive {
                    Primitive::Carry => 30.0,
                    Primitive::Approach => 20.0,
                    _ => 6.0,
                },
                speed: 0.24,
            })
            .collect();
        // A place goal includes observed settled outcome even if the symbolic
        // estimate expected the object to stay still through a held transfer.
        if request.kind == GoalKind::PlaceToy
            && plan
                .steps
                .last()
                .is_none_or(|s| s.primitive != Primitive::WaitStill)
        {
            plan.steps.push(ExerciseStep {
                primitive: Primitive::WaitStill,
                target: request.target,
                tolerance: request.tolerance,
                deadline: 6.0,
                speed: 0.0,
            });
        }
        if !plan.valid() {
            return Err("invalid synthesized plan");
        }
        Ok(plan)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::EcologyState;
    #[test]
    fn builds_new_order_from_goal_and_measured_start_not_an_episode_id() {
        let s = EcologyState::new(42);
        let mut m = GroundedMemory::default();
        m.observe(&s.objects, Vec2::splat(0.5), 1.0, 0.05, false);
        let b = &m.beliefs[0];
        let p = m
            .plan_goal(
                1,
                b,
                GoalRequest {
                    kind: GoalKind::PlaceToy,
                    target: Vec2::new(0.2, 0.7),
                    tolerance: 0.04,
                },
                1.0,
            )
            .unwrap();
        let ops = p.steps.iter().map(|s| s.primitive).collect::<Vec<_>>();
        assert!(
            ops.contains(&Primitive::Grip)
                && ops.contains(&Primitive::Carry)
                && ops.contains(&Primitive::Release)
        );
        assert_eq!(ops.last(), Some(&Primitive::WaitStill));
        assert!(p.valid());
        assert_eq!(m.stats.executed, 0);
    }
    #[test]
    fn hidden_objects_require_observation_before_new_plan() {
        let s = EcologyState::new(42);
        let mut m = GroundedMemory::default();
        m.observe(&s.objects, Vec2::splat(0.5), 1.0, 0.05, false);
        m.beliefs[0].visible = false;
        assert!(
            m.plan_goal(
                1,
                &m.beliefs[0],
                GoalRequest {
                    kind: GoalKind::CarryToy,
                    target: Vec2::splat(0.5),
                    tolerance: 0.04
                },
                1.0
            )
            .is_err()
        );
    }
}
