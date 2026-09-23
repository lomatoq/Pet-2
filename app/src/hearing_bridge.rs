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
    #[serde(default)]
    input_device: Option<String>,
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
    input_devices: Vec<String>,
    input_device: Option<String>,
    path: PathBuf,
    enabled: bool,
    master_gain: f32,
    quiet_seconds: f32,
    test_seconds: f32,
    name_seconds: f32,
    onset_seconds: f32,
    onset_strength: f32,
    name_pending: bool,
    command_pending: Option<CueKind>,
    startle_pending: f32,
    startle_cooldown: f32,
    pub message: String,
    last_cue: String,
    accepted: u64,
    completed_trainings: u64,
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
            .filter(|m| m.len() <= 64_000_000)
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
            "Микрофон выключен. Включи его для реакции на звуки и обучения.".to_owned()
        };
        let enabled = enable || loaded.as_ref().is_some_and(|s| s.enabled);
        let master_gain = loaded.as_ref().map_or(1.0, |s| s.master_gain);
        let input_device = loaded.as_ref().and_then(|s| s.input_device.clone());
        let model = loaded.map_or_else(CueModelV1::new, |s| s.model);
        let mut input = LocalAudioInput::new(
            AudioInputConfig {
                device_name: input_device.clone(),
                ..AudioInputConfig::default()
            },
            model,
        )
        .expect("validated local hearing model");
        if enabled {
            message = match input.start() {
                Ok(()) => "Слушаю. Выбери имя или команду и добавь примеры своего голоса.".into(),
                Err(error) => format!("Microphone unavailable: {error}"),
            };
        }
        pet_audio::AudioEngine::set_master_gain(master_gain);
        Self {
            input_devices: desktop_host::available_input_devices(),
            input_device,
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
            command_pending: None,
            startle_pending: 0.0,
            startle_cooldown: 0.0,
            message,
            last_cue: "none".into(),
            accepted: 0,
            completed_trainings: 0,
            rejected: 0,
            elapsed_since_save: 0.0,
            dirty: enable,
            reference_cadence: OutputReferenceCadence::default(),
        }
    }

    pub fn control(&mut self, action: HearingAction) {
        match action {
            HearingAction::SelectInput { index } => {
                let device = if index == 0 {
                    None
                } else {
                    self.input_devices.get(index as usize - 1).cloned()
                };
                if index != 0 && device.is_none() {
                    return;
                }
                self.message = match self.input.select_device(device.clone()) {
                    Ok(()) => "Микрофон переключён. Проверь индикатор голоса.".into(),
                    Err(error) => format!("Не удалось переключить микрофон: {error}"),
                };
                self.input_device = device;
                self.dirty = true;
            }
            HearingAction::Enable => {
                self.enabled = true;
                self.message = match self.input.start() {
                    Ok(()) => "Слушаю. Скажи что-нибудь — индикатор должен двигаться.".into(),
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
                self.startle_pending = 0.0;
                self.message = "Микрофон выключен.".into();
                self.dirty = true;
            }
            HearingAction::TrainName
            | HearingAction::TrainQuiet
            | HearingAction::TrainOther
            | HearingAction::TrainCommand { .. } => {
                if !self.enabled {
                    self.control(HearingAction::Enable);
                }
                self.test_seconds = 0.0;
                self.command_pending = None;
                pet_audio::AudioEngine::set_master_gain(0.0);
                let cue = match action {
                    HearingAction::TrainName => TrainingCue::Name,
                    HearingAction::TrainQuiet => TrainingCue::Quiet,
                    HearingAction::TrainCommand { cue } => TrainingCue::Command(cue),
                    _ => TrainingCue::Other,
                };
                self.message = match self.input.begin_training(cue) {
                    Ok(()) => format!(
                        "Записываю «{}»: повторы с паузами. Сохраняю каждые пять, продолжаю до 40 или кнопки Стоп.",
                        cue.label()
                    ),
                    Err(error) => format!("Could not start teaching: {error}"),
                };
            }
            HearingAction::CancelTraining => {
                self.message = match self.input.cancel_training() {
                    Ok(()) => "Запись остановлена. Ранее выученные команды сохранены.".into(),
                    Err(error) => format!("Could not cancel teaching: {error}"),
                };
            }
            HearingAction::Test => {
                if !self.enabled {
                    self.control(HearingAction::Enable);
                }
                if self.training()
                    && let Err(error) = self.input.cancel_training()
                {
                    self.message = format!("Could not stop teaching for the test: {error}");
                    return;
                }
                self.test_seconds = 30.0;
                self.accepted = 0;
                self.rejected = 0;
                self.message = "Проверка 30 секунд: говори команды отдельно. Узнанные команды выполняются; посторонние слова должны игнорироваться.".into();
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
            HearingAction::Perform { cue } => match cue {
                CueKind::Quiet => self.quiet(),
                CueKind::Name => {
                    self.name_seconds = 3.0;
                    self.name_pending = true;
                }
                _ => self.command_pending = Some(cue),
            },
            HearingAction::QuietNow => self.quiet(),
            HearingAction::SetVolume { percent } => {
                self.master_gain = f32::from(percent.min(100)) / 100.0;
                self.quiet_seconds = 0.0;
                pet_audio::AudioEngine::set_master_gain(self.master_gain);
                self.message = format!("Voice volume: {}%", percent.min(100));
                self.dirty = true;
            }
            HearingAction::RestoreVolume => {
                self.master_gain = 1.0;
                self.quiet_seconds = 0.0;
                pet_audio::AudioEngine::set_master_gain(1.0);
                self.message = "Обычная громкость восстановлена.".into();
                self.dirty = true;
            }
        }
        self.save_if_dirty();
    }

    fn quiet(&mut self) {
        self.master_gain = 0.25;
        self.quiet_seconds = 20.0;
        pet_audio::AudioEngine::set_master_gain(self.master_gain);
        self.message = "Тише: голос приглушён, новые возгласы приостановлены.".into();
        self.dirty = true;
    }

    pub fn poll(&mut self, dt: f32, own_rms: f32, own_active: bool) {
        let dt = if dt.is_finite() {
            dt.clamp(0.0, 0.25)
        } else {
            0.0
        };
        pet_audio::AudioEngine::set_master_gain(if self.training() {
            0.0
        } else {
            self.master_gain
        });
        let own_active = own_active && !self.training();
        self.startle_cooldown = (self.startle_cooldown - dt).max(0.0);
        self.quiet_seconds = (self.quiet_seconds - dt).max(0.0);
        let was_testing = self.test_seconds > 0.0;
        self.test_seconds = (self.test_seconds - dt).max(0.0);
        if was_testing && self.test_seconds == 0.0 {
            self.message = format!(
                "Проверка завершена: узнано {}, посторонних или неузнанных звуков {}.",
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
                    let (interest, startle) =
                        acoustic_response(rms, self.input.status().noise_floor, own_active);
                    if interest > 0.0 {
                        self.onset_seconds = 0.9;
                        self.onset_strength = interest;
                        if startle > 0.0 && self.startle_cooldown <= 0.0 && !self.training() {
                            self.startle_pending = startle;
                            self.startle_cooldown = 3.0;
                        }
                    }
                }
                AudioPercept::CueAccepted { decision } => {
                    self.accepted += 1;
                    self.last_cue = format!(
                        "{} ({:.0}%)",
                        decision.cue.map_or("—", |cue| cue.label()),
                        decision.confidence * 100.0
                    );
                    match decision.cue {
                        Some(CueKind::Name) if self.name_seconds <= 0.0 => {
                            self.name_seconds = 3.0;
                            self.name_pending = true;
                        }
                        Some(CueKind::Quiet) => self.quiet(),
                        Some(cue) if cue != CueKind::Name => self.command_pending = Some(cue),
                        _ => {}
                    }
                }
                AudioPercept::CueRejected { .. } => {
                    self.rejected += 1;
                    if self.test_seconds > 0.0 {
                        self.message = "Не узнал команду. Произнеси отдельно, с паузой. Можно добавить ещё пять примеров.".into();
                    }
                }
                AudioPercept::TrainingProgress { progress } => {
                    self.message = format!(
                        "«{}»: {} из {}. Сделай паузу, затем повтори.",
                        progress.cue.label(),
                        progress.accepted,
                        progress.required
                    )
                }
                AudioPercept::TrainingComplete { cue, model_ready } => {
                    self.completed_trainings += 1;
                    self.message = format!(
                        "«{}»: примеры сохранены. {}",
                        cue.label(),
                        if self.training() {
                            "Запись продолжается: следующая партия. Нажми Стоп, когда закончишь."
                        } else if model_ready {
                            "Можно проверить или добавить ещё пять."
                        } else {
                            "Добавь пять посторонних слов для различения команд."
                        }
                    );
                    self.dirty = true;
                }
                AudioPercept::TrainingRejected { cue, reason } => {
                    self.message = format!(
                        "«{}»: пример не сохранён. {}",
                        cue.label(),
                        explain_training_error(&reason)
                    )
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
    pub fn take_command(&mut self) -> Option<CueKind> {
        self.command_pending.take()
    }

    pub fn take_startle(&mut self) -> f32 {
        std::mem::take(&mut self.startle_pending)
    }

    pub fn name_attention(&self) -> bool {
        self.name_seconds > 0.0
    }
    pub fn take_name_event(&mut self) -> bool {
        std::mem::take(&mut self.name_pending)
    }
    pub fn sound_interest(&self) -> f32 {
        (self.onset_seconds / 0.9).clamp(0.0, 1.0) * self.onset_strength
    }
    pub fn quiet_boundary(&self) -> bool {
        self.quiet_seconds > 0.0 || self.training()
    }
    pub fn training(&self) -> bool {
        self.input.training_progress().is_some()
    }

    pub fn telemetry(&self) -> serde_json::Value {
        let status = self.input.status();
        let model = self.input.model();
        serde_json::json!({"enabled":self.enabled,"device":status.device_name,"input_devices":self.input_devices,"input_device":self.input_device,
            "message":status.last_error.as_ref().unwrap_or(&self.message),"input_level":status.rms,
            "status":status,"last_cue":self.last_cue,"master_gain":self.master_gain,
            "quiet_seconds":self.quiet_seconds,"test_seconds":self.test_seconds,
            "completed_trainings":self.completed_trainings,"accepted":self.accepted,"rejected":self.rejected,"training":self.input.training_progress(),
            "name_examples":model.name.as_ref().map_or(0, |c| c.examples.len()),
            "other_examples":model.other_examples.len(),
            "quiet_examples":model.quiet.as_ref().map_or(0, |c| c.examples.len()),
            "cues": CueKind::ALL.map(|cue| serde_json::json!({"cue":cue,"label":cue.label(),"examples":model.class(cue).map_or(0, |c| c.examples.len()),"ready":model.is_ready(cue)})),
            "sound_interest": self.sound_interest(),
            "name_ready":model.is_ready(CueKind::Name),"quiet_ready":model.is_ready(CueKind::Quiet)})
    }

    pub fn save_if_dirty(&mut self) {
        if !self.dirty {
            return;
        }
        let save = HearingSave {
            input_device: self.input_device.clone(),
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

// Salience relative to the measured room floor, with an absolute guard so a
// whisper in a quiet room is interesting but never a frightening explosion.
fn acoustic_response(rms: f32, floor: f32, own_active: bool) -> (f32, f32) {
    if own_active || !rms.is_finite() || !floor.is_finite() || rms < 0.003 {
        return (0.0, 0.0);
    }
    let ratio = rms / floor.max(0.0005);
    if ratio < 2.2 {
        return (0.0, 0.0);
    }
    let interest = (0.34 + ratio.log2() * 0.11).clamp(0.34, 0.82);
    let startle = if rms >= 0.045 && ratio >= 5.0 {
        0.85
    } else {
        0.0
    };
    (interest, startle)
}

#[test]
fn sounds_distinguish_whispers_loud_onsets_and_own_voice() {
    let whisper = acoustic_response(0.005, 0.001, false);
    assert!(whisper.0 > 0.44 && whisper.1 == 0.0);
    assert!(acoustic_response(0.10, 0.006, false).1 > 0.55);
    assert_eq!(acoustic_response(0.10, 0.006, true), (0.0, 0.0));
    assert_eq!(acoustic_response(0.006, 0.006, false), (0.0, 0.0));
    assert_eq!(acoustic_response(f32::NAN, 0.001, false), (0.0, 0.0));
}

fn explain_training_error(reason: &str) -> &'static str {
    if reason.contains("quiet") || reason.contains("silent") {
        "Слишком тихо: говори чуть ближе к микрофону."
    } else if reason.contains("duration") {
        "Произнеси одно слово или короткую фразу, затем помолчи секунду."
    } else if reason.contains("output") {
        "Звучал голос питомца. Подожди тишины и повтори."
    } else if reason.contains("separable") {
        "Запись остановлена: фраза неотличима от другой команды или постороннего примера. Выбери другую фразу и начни снова; ранее сохранённые команды не потеряны."
    } else if reason.contains("held-out") || reason.contains("inconsistent") {
        "Один из пяти примеров отличается: заменяю самый непохожий. Четыре остаются в текущей записи; повтори ещё раз с паузой."
    } else {
        "Не удалось выделить голос. Говори обычным голосом, делая паузу после фразы."
    }
}
