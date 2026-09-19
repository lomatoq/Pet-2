//! Local hearing orchestration and user-owned teaching. No recordings or transcripts.
use desktop_host::{
    AudioInputConfig, AudioInputState, AudioPercept, CueKind, CueModelV1, HearingAction,
    LocalAudioInput, OutputReferenceFrame, TrainingCue,
};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct HearingSave {
    version: u32,
    enabled: bool,
    master_gain: f32,
    model: CueModelV1,
}

#[derive(Default)]
struct OutputReferenceCadence {
    elapsed: f32,
    was_active: bool,
}

impl OutputReferenceCadence {
    fn due(&mut self, dt: f32, active: bool) -> bool {
        self.elapsed += dt;
        if active != self.was_active || self.elapsed >= 0.1 {
            self.elapsed = 0.0;
            self.was_active = active;
            true
        } else {
            false
        }
    }
}

pub struct HearingBridge {
    input: LocalAudioInput,
    path: PathBuf,
    enabled: bool,
    master_gain: f32,
    quiet_seconds: f32,
    test_seconds: f32,
    name_seconds: f32,
    onset_seconds: f32,
    onset_strength: f32,
    name_pending: bool,
    pub message: String,
    last_cue: String,
    accepted: u64,
    rejected: u64,
    elapsed_since_save: f32,
    dirty: bool,
    reference_cadence: OutputReferenceCadence,
}

impl Drop for HearingBridge {
    fn drop(&mut self) {
        self.input.stop();
        if self.input.take_dirty_model_snapshot().is_some() {
            self.dirty = true;
        }
        self.save_if_dirty();
    }
}

impl HearingBridge {
    pub fn load(root: &Path, enable: bool) -> Self {
        let path = root.join("hearing-state.json");
        let loaded = fs::metadata(&path)
            .ok()
            .filter(|m| m.len() <= 2_000_000)
            .and_then(|_| fs::read(&path).ok())
            .and_then(|bytes| serde_json::from_slice::<HearingSave>(&bytes).ok())
            .filter(|s| {
                s.version == 1
                    && s.master_gain.is_finite()
                    && (0.0..=1.0).contains(&s.master_gain)
                    && s.model.validate().is_ok()
            });
        let mut message = if path.exists() && loaded.is_none() {
            "Could not validate saved hearing model; teach again to replace it.".to_owned()
        } else {
            "Microphone off. Enable hearing to react to sound and teach cues.".to_owned()
        };
        let enabled = enable || loaded.as_ref().is_some_and(|s| s.enabled);
        let master_gain = loaded.as_ref().map_or(1.0, |s| s.master_gain);
        let model = loaded.map_or_else(CueModelV1::new, |s| s.model);
        let mut input = LocalAudioInput::new(AudioInputConfig::default(), model)
            .expect("validated local hearing model");
        if enabled {
            message = match input.start() {
                Ok(()) => {
                    "Listening locally. Teach name, quiet, and other words before testing.".into()
                }
                Err(error) => format!("Microphone unavailable: {error}"),
            };
        }
        pet_audio::AudioEngine::set_master_gain(master_gain);
        Self {
            input,
            path,
            enabled,
            master_gain,
            quiet_seconds: 0.0,
            test_seconds: 0.0,
            name_seconds: 0.0,
            onset_seconds: 0.0,
            onset_strength: 0.0,
            name_pending: false,
            message,
            last_cue: "none".into(),
            accepted: 0,
            rejected: 0,
            elapsed_since_save: 0.0,
            dirty: enable,
            reference_cadence: OutputReferenceCadence::default(),
        }
    }

    pub fn control(&mut self, action: HearingAction) {
        match action {
            HearingAction::Enable => {
                self.enabled = true;
                self.message = match self.input.start() {
                    Ok(()) => "Listening locally; say something to check the input meter.".into(),
                    Err(error) => format!("Microphone unavailable: {error}"),
                };
                self.dirty = true;
            }
            HearingAction::Disable => {
                self.input.stop();
                self.enabled = false;
                self.name_seconds = 0.0;
                self.name_pending = false;
                self.onset_seconds = 0.0;
                self.message = "Microphone off.".into();
                self.dirty = true;
            }
            HearingAction::TrainName | HearingAction::TrainQuiet | HearingAction::TrainOther => {
                if !self.enabled {
                    self.control(HearingAction::Enable);
                }
                self.test_seconds = 0.0;
                let cue = match action {
                    HearingAction::TrainName => TrainingCue::Name,
                    HearingAction::TrainQuiet => TrainingCue::Quiet,
                    _ => TrainingCue::Other,
                };
                self.message = match self.input.begin_training(cue) {
                    Ok(()) => format!(
                        "Teaching {cue:?}: speak separate examples with a pause between them."
                    ),
                    Err(error) => format!("Could not start teaching: {error}"),
                };
            }
            HearingAction::CancelTraining => {
                self.message = match self.input.cancel_training() {
                    Ok(()) => "Teaching cancelled; previous cues retained.".into(),
                    Err(error) => format!("Could not cancel teaching: {error}"),
                };
            }
            HearingAction::Test => {
                if self.training()
                    && let Err(error) = self.input.cancel_training()
                {
                    self.message = format!("Could not stop teaching for the test: {error}");
                    return;
                }
                self.test_seconds = 30.0;
                self.accepted = 0;
                self.rejected = 0;
                self.message = "30-second test: try fresh cue examples and ordinary words. No commands applied during this check.".into();
            }
            HearingAction::Forget => match self.input.forget() {
                Ok(()) => {
                    self.last_cue = "none".into();
                    self.name_pending = false;
                    self.name_seconds = 0.0;
                    self.message = "Learned cue templates deleted.".into();
                    self.dirty = true;
                }
                Err(error) => self.message = format!("Could not forget cues: {error}"),
            },
            HearingAction::QuietNow => self.quiet(),
            HearingAction::RestoreVolume => {
                self.master_gain = 1.0;
                self.quiet_seconds = 0.0;
                pet_audio::AudioEngine::set_master_gain(1.0);
                self.message = "Original voice level restored.".into();
                self.dirty = true;
            }
        }
        self.save_if_dirty();
    }

    fn quiet(&mut self) {
        self.master_gain = 0.25;
        self.quiet_seconds = 20.0;
        pet_audio::AudioEngine::set_master_gain(self.master_gain);
        self.message = "Quiet: voice softened and new calls paused. Restore voice to undo.".into();
        self.dirty = true;
    }

    pub fn poll(&mut self, dt: f32, own_rms: f32, own_active: bool) {
        let dt = if dt.is_finite() {
            dt.clamp(0.0, 0.25)
        } else {
            0.0
        };
        self.quiet_seconds = (self.quiet_seconds - dt).max(0.0);
        let was_testing = self.test_seconds > 0.0;
        self.test_seconds = (self.test_seconds - dt).max(0.0);
        if was_testing && self.test_seconds == 0.0 {
            self.message = format!(
                "Test finished: {} accepted, {} rejected. Check that only intended cues were accepted.",
                self.accepted, self.rejected
            );
        }
        self.name_seconds = (self.name_seconds - dt).max(0.0);
        self.onset_seconds = (self.onset_seconds - dt).max(0.0);
        self.elapsed_since_save += dt;
        if self.enabled
            && self.input.status().state == AudioInputState::Listening
            && self.reference_cadence.due(dt, own_active)
            && let Err(error) = self.input.submit_output_reference(OutputReferenceFrame {
                rms: own_rms,
                active: own_active,
            })
        {
            self.message = format!("Microphone output-reference update: {error}");
        }
        for event in self.input.drain_percepts() {
            match event {
                AudioPercept::Onset { rms } => {
                    if !own_active && rms > 0.008 {
                        self.onset_seconds = 0.65;
                        let floor = self.input.status().noise_floor.max(0.002);
                        self.onset_strength =
                            ((rms / floor).max(1.0).log2() * 0.09).clamp(0.06, 0.42);
                    }
                }
                AudioPercept::CueAccepted { decision } => {
                    self.accepted += 1;
                    self.last_cue =
                        format!("{:?} ({:.0}%)", decision.cue, decision.confidence * 100.0);
                    if self.test_seconds > 0.0 {
                        continue;
                    }
                    match decision.cue {
                        Some(CueKind::Name) if self.name_seconds <= 0.0 => {
                            self.name_seconds = 2.0;
                            self.name_pending = true;
                        }
                        Some(CueKind::Quiet) => self.quiet(),
                        _ => {}
                    }
                }
                AudioPercept::CueRejected { .. } => self.rejected += 1,
                AudioPercept::TrainingProgress { progress } => {
                    self.message = format!(
                        "Teaching {:?}: {}/{} accepted. Pause, then say the next example.",
                        progress.cue, progress.accepted, progress.required
                    )
                }
                AudioPercept::TrainingComplete { cue, model_ready } => {
                    self.message = format!(
                        "{cue:?} examples saved; ready: {model_ready}. Add other words, then test fresh examples."
                    );
                    self.dirty = true;
                }
                AudioPercept::TrainingRejected { cue, reason } => {
                    self.message = format!("{cue:?} example rejected: {reason}")
                }
                _ => {}
            }
        }
        if self.input.take_dirty_model_snapshot().is_some() {
            self.dirty = true;
        }
        if self.elapsed_since_save > 1.0 {
            self.save_if_dirty();
        }
    }

    pub fn input_levels(&self) -> (Option<f32>, Option<f32>) {
        let status = self.input.status();
        if self.enabled && status.state == AudioInputState::Listening {
            (
                Some(status.rms),
                Some(if status.voice_activity { 1.0 } else { 0.0 }),
            )
        } else {
            (None, None)
        }
    }
    pub fn name_attention(&self) -> bool {
        self.name_seconds > 0.0 && !self.quiet_boundary()
    }
    pub fn take_name_event(&mut self) -> bool {
        std::mem::take(&mut self.name_pending)
    }
    pub fn sound_interest(&self) -> f32 {
        (self.onset_seconds / 0.65).clamp(0.0, 1.0) * self.onset_strength
    }
    pub fn quiet_boundary(&self) -> bool {
        self.quiet_seconds > 0.0
    }
    pub fn training(&self) -> bool {
        self.input.training_progress().is_some()
    }

    pub fn telemetry(&self) -> serde_json::Value {
        let status = self.input.status();
        let model = self.input.model();
        serde_json::json!({"enabled":self.enabled,"device":status.device_name,
            "message":status.last_error.as_ref().unwrap_or(&self.message),"input_level":status.rms,
            "status":status,"last_cue":self.last_cue,"master_gain":self.master_gain,
            "quiet_seconds":self.quiet_seconds,"test_seconds":self.test_seconds,
            "accepted":self.accepted,"rejected":self.rejected,"training":self.input.training_progress(),
            "name_ready":model.is_ready(CueKind::Name),"quiet_ready":model.is_ready(CueKind::Quiet)})
    }

    pub fn save_if_dirty(&mut self) {
        if !self.dirty {
            return;
        }
        let save = HearingSave {
            version: 1,
            enabled: self.enabled,
            master_gain: self.master_gain,
            model: self.input.model(),
        };
        let result = (|| -> Result<(), Box<dyn std::error::Error>> {
            save.model.validate()?;
            let bytes = serde_json::to_vec(&save)?;
            if let Some(parent) = self.path.parent() {
                fs::create_dir_all(parent)?;
            }
            let staging = self.path.with_extension("tmp");
            fs::write(&staging, bytes)?;
            fs::rename(staging, &self.path)?;
            Ok(())
        })();
        match result {
            Ok(()) => {
                self.dirty = false;
                self.elapsed_since_save = 0.0;
            }
            Err(error) => self.message = format!("Hearing settings could not be saved: {error}"),
        }
    }
}

#[test]
fn reference_updates_leave_queue_capacity_for_teaching_but_voice_edges_are_immediate() {
    for hz in [30, 60, 120, 240] {
        let mut cadence = OutputReferenceCadence::default();
        let steady_updates = (0..hz)
            .filter(|_| cadence.due(1.0 / hz as f32, false))
            .count();
        assert!((8..=10).contains(&steady_updates));
        assert!(cadence.due(0.001, true));
        assert!(!cadence.due(0.001, true));
        assert!(cadence.due(0.001, false));
    }
}
