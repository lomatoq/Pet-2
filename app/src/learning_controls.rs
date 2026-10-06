//! Explicit, expiring exercise requests and bounded causal telemetry.
//! Files carry data only; no generated code or OS commands can be executed.
use pet_ecology::{EcologyState, Exercise, GameFamily, GroundedMemory, ObjectKind};
use serde::Deserialize;
use std::{
    fs,
    path::{Path, PathBuf},
    time::{Instant, SystemTime, UNIX_EPOCH},
};
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |x| x.as_millis() as u64)
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    id: u64,
    instance: u64,
    issued_unix_ms: u64,
    action: String,
    family: Option<GameFamily>,
    level: Option<u8>,
    program: Option<pet_ecology::ProgramSpec>,
    goal: Option<pet_ecology::GoalRequest>,
}
pub struct LearningControls {
    root: PathBuf,
    instance: u64,
    last_id: u64,
    poll_at: Instant,
    pub message: String,
}
impl LearningControls {
    pub fn new(root: &Path) -> Self {
        let last_id = fs::read(root.join("grounded-control-ack.json"))
            .ok()
            .and_then(|x| serde_json::from_slice::<serde_json::Value>(&x).ok())
            .and_then(|v| v["id"].as_u64())
            .unwrap_or(0);
        Self {
            root: root.to_path_buf(),
            instance: now(),
            last_id,
            poll_at: Instant::now() - std::time::Duration::from_secs(2),
            message: "Learning from executed actions; no artificial experience".into(),
        }
    }
    pub fn poll(&mut self, state: &mut EcologyState, aspect: f32) {
        if self.poll_at.elapsed().as_millis() < 750 {
            return;
        }
        self.poll_at = Instant::now();
        if let Some(r) = fs::read(self.root.join("grounded-control.json"))
            .ok()
            .filter(|v| v.len() <= 32768)
            .and_then(|v| serde_json::from_slice::<Request>(&v).ok())
        {
            if r.id > self.last_id {
                self.last_id = r.id;
                let result = self.apply(&r, state, aspect);
                self.message = result
                    .as_ref()
                    .map_or_else(|e| format!("Not applied: {e}"), |s| s.clone());
                let _ = super::vision_bridge::atomic_json(
                    &self.root.join("grounded-control-ack.json"),
                    &serde_json::json!({"id":r.id,"applied":result.is_ok(),"message":self.message}),
                );
            }
        }
        let m = &state.grounded;
        let status = serde_json::json!({"schema_version":1,"instance":self.instance,"updated_unix_ms":now(),"message":self.message,"stats":m.stats,"skills":m.skills,"learned_programs":m.programs,"exercise":m.exercise,"suspended":m.suspended.len(),"cooldown_seconds":m.cooldown,
            "beliefs":m.beliefs,"recent_experience":m.episodes.iter().rev().take(8).collect::<Vec<_>>(),
            "scope":"persistent object beliefs, measured action effects and composed exercises in the pet habitat; not omniscient desktop reasoning"});
        let _ = super::vision_bridge::atomic_json(&self.root.join("grounded-status.json"), &status);
    }
    fn apply(&self, r: &Request, state: &mut EcologyState, aspect: f32) -> Result<String, String> {
        if r.instance != self.instance || now().abs_diff(r.issued_unix_ms) > 30_000 {
            return Err("stale request or runtime instance".into());
        }
        match r.action.as_str() {
            "cancel" => {
                state.grounded.suspend_exercise(true);
                Ok("Exercise cancelled; learned evidence retained".into())
            }
            "practice" | "program" | "goal" => {
                let orb = state
                    .objects
                    .iter()
                    .find(|o| o.kind == ObjectKind::Orb)
                    .ok_or("No physical toy")?;
                let mut program = if r.action == "practice" {
                    let family = r.family.ok_or("No game family")?;
                    let level = r.level.unwrap_or(0);
                    if level > 4 {
                        return Err("difficulty must be 0 through 4".into());
                    }
                    GroundedMemory::compile_exercise(
                        state.episode_stats.next_episode_id,
                        orb.id,
                        family,
                        level,
                        orb.position,
                        state.den.anchor,
                        aspect,
                        state.grounded.clock,
                    )
                } else if r.action == "goal" {
                    let belief = state
                        .grounded
                        .beliefs
                        .iter()
                        .find(|b| b.id == orb.id)
                        .ok_or("Toy not observed yet")?;
                    state
                        .grounded
                        .plan_goal(
                            state.episode_stats.next_episode_id,
                            belief,
                            r.goal.clone().ok_or("No goal")?,
                            aspect,
                        )
                        .map_err(str::to_string)?
                } else {
                    r.program
                        .clone()
                        .ok_or("No program")?
                        .compile(
                            state.episode_stats.next_episode_id,
                            orb.id,
                            orb.position,
                            aspect,
                            state.grounded.clock,
                        )
                        .map_err(str::to_string)?
                };
                // Bind user programs to the existing toy; never spawn or claim a
                // phantom object. Actual admission still belongs to the director.
                if program.object_id != orb.id {
                    return Err("program names an unknown object".into());
                }
                program.episode_id = state.episode_stats.next_episode_id;
                state
                    .grounded
                    .request_exercise(program)
                    .map_err(str::to_string)?;
                state.grounded.cooldown = 0.0;
                Ok(
                    "Exercise queued; sleep, focus, safety and user ownership remain authoritative"
                        .into(),
                )
            }
            _ => Err("unknown operation".into()),
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn old_instance_cannot_start_a_task() {
        let d = tempfile::tempdir().unwrap();
        let c = LearningControls::new(d.path());
        let mut s = EcologyState::new(42);
        let r = Request {
            id: 1,
            instance: c.instance + 1,
            issued_unix_ms: now(),
            action: "practice".into(),
            family: Some(GameFamily::Roll),
            level: Some(0),
            program: None,
            goal: None,
        };
        assert!(c.apply(&r, &mut s, 1.0).is_err());
        assert!(s.grounded.suspended.is_empty());
    }
    #[test]
    fn bounded_user_task_is_queued_without_fake_rewards() {
        let d = tempfile::tempdir().unwrap();
        let c = LearningControls::new(d.path());
        let mut s = EcologyState::new(42);
        let r = Request {
            id: 1,
            instance: c.instance,
            issued_unix_ms: now(),
            action: "practice".into(),
            family: Some(GameFamily::Fetch),
            level: Some(2),
            program: None,
            goal: None,
        };
        assert!(c.apply(&r, &mut s, 1.0).is_ok());
        assert_eq!(s.grounded.suspended.len(), 1);
        assert_eq!(s.grounded.stats.executed, 0);
        assert_eq!(s.grounded.stats.plan_successes, 0);
        assert!(s.validate().is_ok());
    }
}
