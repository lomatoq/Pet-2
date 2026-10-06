//! Explicit correction channel. No images or features are persisted without a
//! named observation, matching instance, fresh request and affirmative consent.
use pet_vision::TrainingFeatures;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::Path,
    time::{Instant, SystemTime, UNIX_EPOCH},
};

fn unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    id: u64,
    instance: u64,
    issued_unix_ms: u64,
    action: String,
    observation_id: Option<u64>,
    label: Option<String>,
    consent_features: bool,
}
#[derive(Serialize)]
struct Example<'a> {
    schema_version: u32,
    request_id: u64,
    observation_id: u64,
    label: &'a str,
    group: String,
    confirmed: bool,
    source: &'static str,
    features: &'a TrainingFeatures,
}
struct Sample {
    id: u64,
    context: u64,
    at: Instant,
    features: TrainingFeatures,
}
pub struct VisionTeacher {
    instance: u64,
    last_request: u64,
    last_poll: Instant,
    sample: Option<Sample>,
    pub message: String,
    pub saved_examples: u32,
}
impl VisionTeacher {
    pub fn new(root: &Path) -> Self {
        let last = fs::read(root.join("vision-teaching-ack.json"))
            .ok()
            .and_then(|x| serde_json::from_slice::<serde_json::Value>(&x).ok())
            .and_then(|v| v["id"].as_u64())
            .unwrap_or(0);
        Self {
            instance: unix_ms(),
            last_request: last,
            last_poll: Instant::now(),
            sample: None,
            message: "No confirmed examples collected".into(),
            saved_examples: fs::read_dir(root.join("vision-teaching")).ok().into_iter()
                .flatten().filter_map(Result::ok).filter(|e| {
                    let name = e.file_name().to_string_lossy().to_string();
                    name.starts_with("sample-") && name.ends_with(".json")
                        && e.file_type().is_ok_and(|t| t.is_file())
                }).count().min(96) as u32,
        }
    }
    pub fn clear(&mut self) {
        self.sample = None;
    }
    pub fn offer(&mut self, id: u64, context: u64, features: Option<TrainingFeatures>) {
        if let Some(features) = features.filter(TrainingFeatures::valid) {
            self.sample = Some(Sample {
                id,
                context,
                at: Instant::now(),
                features,
            });
        }
    }
    pub fn status(&self) -> serde_json::Value {
        serde_json::json!({"instance":self.instance,"sample_id":self.sample.as_ref().map(|s|s.id),"sample_age_seconds":self.sample.as_ref().map(|s|s.at.elapsed().as_secs()),"message":self.message,"saved_examples":self.saved_examples,"storage":"only explicitly confirmed private feature tensors, no automatic screenshot history"})
    }
    pub fn poll(&mut self, root: &Path, enabled: bool) {
        if self.last_poll.elapsed().as_millis() < 500 {
            return;
        }
        self.last_poll = Instant::now();
        if !enabled {
            self.sample = None;
        }
        let Some(r) = fs::read(root.join("vision-teaching-request.json"))
            .ok()
            .filter(|v| v.len() <= 4096)
            .and_then(|v| serde_json::from_slice::<Request>(&v).ok())
        else {
            return;
        };
        if r.id <= self.last_request {
            return;
        }
        self.last_request = r.id;
        let result = self.apply(root, &r, enabled);
        self.message = result
            .as_ref()
            .map_or_else(|e| format!("Not applied: {e}"), |s| s.clone());
        let ack = serde_json::json!({"id":r.id,"instance":self.instance,"applied":result.is_ok(),"message":self.message});
        let _ = super::vision_bridge::atomic_json(&root.join("vision-teaching-ack.json"), &ack);
    }
    fn apply(&mut self, root: &Path, r: &Request, enabled: bool) -> Result<String, String> {
        if r.instance != self.instance || unix_ms().abs_diff(r.issued_unix_ms) > 30_000 {
            return Err("stale session/request".into());
        }
        let directory = root.join("vision-teaching");
        match r.action.as_str() {
            "disable_adapter" => {
                fs::write(root.join("vision-adapter-disabled"), b"disabled by user")
                    .map_err(|e| e.to_string())?;
                return Ok("Visual adapter disabled; base VLM retained".into());
            }
            "enable_adapter" => {
                let p = root.join("vision-adapter-disabled");
                if p.exists() {
                    fs::remove_file(p).map_err(|e| e.to_string())?;
                }
                return Ok("Validated adapter enabled".into());
            }
            "delete_examples" => {
                let training = root.join("vision-training");
                if training.join("training.lock").exists() {
                    return Err("training is using a snapshot; finish the current job before deleting examples".into());
                }
                self.sample = None;
                if directory.exists() {
                    for entry in fs::read_dir(&directory).map_err(|e| e.to_string())? {
                        let entry = entry.map_err(|e| e.to_string())?;
                        let n = entry.file_name().to_string_lossy().to_string();
                        if n.starts_with("sample-")
                            && n.ends_with(".json")
                            && entry.file_type().map_err(|e| e.to_string())?.is_file()
                        {
                            fs::remove_file(entry.path()).map_err(|e| e.to_string())?;
                        }
                    }
                }
                // The trainer makes explicit snapshot copies. Delete those
                // private tensors too, not just the original teaching samples.
                if training.is_dir() {
                    for run in fs::read_dir(&training).map_err(|e| e.to_string())? {
                        let run = run.map_err(|e| e.to_string())?;
                        let name = run.file_name().to_string_lossy().to_string();
                        if !run.file_type().map_err(|e| e.to_string())?.is_dir()
                            || name.len() != 15 || !name.chars().all(|c| c.is_ascii_digit() || c == '-') {
                            continue;
                        }
                        let features = run.path().join("features");
                        if features.is_dir() {
                            for entry in fs::read_dir(&features).map_err(|e| e.to_string())? {
                                let entry = entry.map_err(|e| e.to_string())?;
                                let name = entry.file_name().to_string_lossy().to_string();
                                if name.starts_with("example-") && name.ends_with(".json")
                                    && entry.file_type().map_err(|e| e.to_string())?.is_file() {
                                    fs::remove_file(entry.path()).map_err(|e| e.to_string())?;
                                }
                            }
                        }
                        let dataset = run.path().join("dataset.json");
                        if dataset.is_file() { fs::remove_file(dataset).map_err(|e| e.to_string())?; }
                    }
                }
                self.saved_examples = 0;
                return Ok("Teaching examples and training snapshot tensors deleted; learned adapter weights and pet memories retained".into());
            }
            "promote_adapter" => {
                let training = root.join("vision-training");
                let report: serde_json::Value = serde_json::from_slice(
                    &fs::read(training.join("candidate.json")).map_err(|e| e.to_string())?,
                )
                .map_err(|e| e.to_string())?;
                if report["approved"] != true {
                    return Err("candidate lacks native validation".into());
                }
                let relative = report["candidate_path"]
                    .as_str()
                    .ok_or("missing candidate")?;
                let candidate = training
                    .join(relative)
                    .canonicalize()
                    .map_err(|e| e.to_string())?;
                if !candidate.starts_with(training.canonicalize().map_err(|e| e.to_string())?) {
                    return Err("candidate is outside this profile".into());
                }
                let adapter = pet_vision::ConnectorAdapter::load_checked(
                    &candidate,
                    report["sha256"].as_str().ok_or("missing validation hash")?,
                )
                .map_err(|e| e.to_string())?;
                let previous = root.join("vision-adapter.json");
                if previous.exists() {
                    let archives = root.join("vision-adapter-history");
                    fs::create_dir_all(&archives).map_err(|e| e.to_string())?;
                    fs::copy(&previous, archives.join(format!("{}.json", r.id)))
                        .map_err(|e| e.to_string())?;
                }
                super::vision_bridge::atomic_json(&previous, &adapter)
                    .map_err(|e| e.to_string())?;
                return Ok(
                    "Validated adapter promoted; previous weights archived, base model untouched"
                        .into(),
                );
            }
            "teach" => {}
            _ => return Err("unknown action".into()),
        }
        if !enabled || !r.consent_features {
            return Err("explicit permission to store this example is required".into());
        }
        let s = self.sample.as_ref().ok_or("no current observation")?;
        if r.observation_id != Some(s.id) || s.at.elapsed().as_secs() > 30 {
            return Err("observation changed or expired; no sample saved".into());
        }
        let label = r
            .label
            .as_deref()
            .filter(|s| s.len() == 1 && "ABCDEFGHI".contains(*s))
            .ok_or("invalid category")?;
        fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
        if fs::read_dir(&directory)
            .map_err(|e| e.to_string())?
            .filter_map(Result::ok)
            .count()
            >= 96
        {
            return Err("96-example budget reached; review/delete examples first".into());
        }
        let path = directory.join(format!("sample-{}.json", r.id));
        if path.exists() {
            return Err("duplicate example identity".into());
        }
        let example = Example {
            schema_version: 1,
            request_id: r.id,
            observation_id: s.id,
            label,
            group: format!("{}-{:016x}", self.instance, s.context),
            confirmed: true,
            source: "explicit human correction",
            features: &s.features,
        };
        super::vision_bridge::atomic_json(&path, &example).map_err(|e| e.to_string())?;
        self.saved_examples = self.saved_examples.saturating_add(1);
        Ok("Confirmed example saved locally; it is not a second reward or automatic model promotion".into())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn wrong_instance_and_missing_consent_never_save_a_training_sample() {
        let dir = tempfile::tempdir().unwrap();
        let mut t = VisionTeacher::new(dir.path());
        let r = Request {
            id: 1,
            instance: t.instance + 1,
            issued_unix_ms: unix_ms(),
            action: "teach".into(),
            observation_id: Some(1),
            label: Some("A".into()),
            consent_features: false,
        };
        assert!(t.apply(dir.path(), &r, true).is_err());
        assert!(!dir.path().join("vision-teaching").exists());
    }
    #[test]
    fn rejects_invalid_label_and_stale_observation() {
        let dir = tempfile::tempdir().unwrap();
        let mut t = VisionTeacher::new(dir.path());
        let r = Request {
            id: 1,
            instance: t.instance,
            issued_unix_ms: unix_ms(),
            action: "teach".into(),
            observation_id: Some(999),
            label: Some("run shell".into()),
            consent_features: true,
        };
        assert!(t.apply(dir.path(), &r, true).is_err());
    }

    #[test]
    fn deletion_removes_training_snapshots_and_preserves_learned_weights() {
        let d = tempfile::tempdir().unwrap();
        let root=d.path();
        fs::create_dir_all(root.join("vision-teaching")).unwrap();
        fs::write(root.join("vision-teaching/sample-1.json"),b"{}").unwrap();
        let run=root.join("vision-training/20261006-120000");
        fs::create_dir_all(run.join("features")).unwrap();
        fs::write(run.join("features/example-001.json"),b"{}").unwrap();
        fs::write(run.join("dataset.json"),b"{}").unwrap();
        fs::write(root.join("vision-adapter.json"),b"keep these learned weights").unwrap();
        let mut t=VisionTeacher::new(root);
        assert_eq!(t.saved_examples,1);
        let request=Request{id:2,instance:t.instance,issued_unix_ms:unix_ms(),action:"delete_examples".into(),observation_id:None,label:None,consent_features:false};
        t.apply(root,&request,false).unwrap();
        assert!(!root.join("vision-teaching/sample-1.json").exists());
        assert!(!run.join("features/example-001.json").exists());
        assert!(!run.join("dataset.json").exists());
        assert_eq!(fs::read(root.join("vision-adapter.json")).unwrap(),b"keep these learned weights");
        assert_eq!(t.saved_examples,0);
    }
    #[test]
    fn deletion_does_not_race_a_running_training_job() {
        let d=tempfile::tempdir().unwrap();let root=d.path();
        fs::create_dir_all(root.join("vision-training")).unwrap();
        fs::write(root.join("vision-training/training.lock"),b"123").unwrap();
        let mut t=VisionTeacher::new(root);
        let request=Request{id:2,instance:t.instance,issued_unix_ms:unix_ms(),action:"delete_examples".into(),observation_id:None,label:None,consent_features:false};
        assert!(t.apply(root,&request,false).is_err());
        assert!(root.join("vision-training/training.lock").exists());
    }
}
