use std::{
    fs::{self, File, OpenOptions},
    io::{self, BufReader, BufWriter, Write},
    path::{Path, PathBuf},
};

use directories::ProjectDirs;
use lifecore::{LifeError, LifeSnapshot, VitaState};
use pet_ecology::{EcologyError, EcologyState};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use tempfile::NamedTempFile;
use thiserror::Error;

use crate::{LabControlEnvelope, LabControlValidationError, PersistedPetPosition};

pub const PORTABLE_STATE_SCHEMA_VERSION: u32 = 1;
pub const TELEMETRY_LOG_MAX_BYTES: u64 = 8 * 1024 * 1024;
pub const LAB_CONTROL_MAX_FILE_BYTES: u64 = 4 * 1024;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PortablePetState {
    pub schema_version: u32,
    pub life: LifeSnapshot,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vita: Option<VitaState>,
    pub position: PersistedPetPosition,
}

impl PortablePetState {
    pub fn validate(&self) -> Result<(), StorageError> {
        if self.schema_version != PORTABLE_STATE_SCHEMA_VERSION {
            return Err(StorageError::UnsupportedSchema {
                found: self.schema_version,
                expected: PORTABLE_STATE_SCHEMA_VERSION,
            });
        }
        self.life.validate_with_additive_voice_repair()?;
        if let Some(vita) = &self.vita
            && !vita.is_valid()
        {
            return Err(StorageError::InvalidVitaState);
        }
        let normalized = self.position.normalized;
        if !normalized.x.is_finite()
            || !normalized.y.is_finite()
            || !(0.0..=1.0).contains(&normalized.x)
            || !(0.0..=1.0).contains(&normalized.y)
        {
            return Err(StorageError::InvalidPosition);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UserProfile {
    pub schema_version: u32,
    pub muted: bool,
    pub volume: f32,
    pub focus_mode: bool,
    pub debug_overlay: bool,
}

impl Default for UserProfile {
    fn default() -> Self {
        Self {
            schema_version: 1,
            muted: false,
            volume: 0.7,
            focus_mode: false,
            debug_overlay: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EventLogEntry {
    pub monotonic_seconds: f64,
    pub kind: String,
    pub details: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoragePaths {
    pub root: PathBuf,
    pub state: PathBuf,
    pub profile: PathBuf,
    pub liquid_tuning: PathBuf,
    pub liquid_tuning_status: PathBuf,
    pub morph_brain: PathBuf,
    pub ecology_state: PathBuf,
    pub body_state: PathBuf,
    pub body_backup: PathBuf,
    pub interaction_replays: PathBuf,
    pub evolution_runs: PathBuf,
    pub events: PathBuf,
    pub telemetry: PathBuf,
    pub telemetry_previous: PathBuf,
    pub lab_control: PathBuf,
    pub lab_control_backup: PathBuf,
    pub runtime_load_ack: PathBuf,
    pub runtime_load_ack_backup: PathBuf,
    pub backup: PathBuf,
    pub ecology_backup: PathBuf,
}

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("application data directory is unavailable")]
    AppDataUnavailable,
    #[error("storage I/O failed: {0}")]
    Io(#[from] io::Error),
    #[error("JSON serialization failed: {0}")]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Life(#[from] LifeError),
    #[error("unsupported portable state schema {found}; expected {expected}")]
    UnsupportedSchema { found: u32, expected: u32 },
    #[error("portable state contains an invalid normalized position")]
    InvalidPosition,
    #[error("portable state contains an invalid VITA mind")]
    InvalidVitaState,
    #[error("body state failed semantic validation")]
    InvalidBodyState,
    #[error("telemetry record is {record_bytes} bytes, exceeding the {max_bytes}-byte log cap")]
    TelemetryRecordTooLarge { record_bytes: u64, max_bytes: u64 },
    #[error(transparent)]
    InvalidLabControl(#[from] LabControlValidationError),
    #[error("Lab control file is {file_bytes} bytes, exceeding the {max_bytes}-byte cap")]
    LabControlFileTooLarge { file_bytes: u64, max_bytes: u64 },
    #[error(transparent)]
    Ecology(#[from] EcologyError),
}

#[derive(Debug, Clone)]
pub struct StateStore {
    pub paths: StoragePaths,
}

impl StateStore {
    pub fn discover() -> Result<Self, StorageError> {
        let dirs =
            ProjectDirs::from("io", "lomatoq", "Pet 2").ok_or(StorageError::AppDataUnavailable)?;
        Ok(Self::at(dirs.data_local_dir()))
    }

    #[must_use]
    pub fn at(root: impl AsRef<Path>) -> Self {
        let root = root.as_ref().to_path_buf();
        Self {
            paths: StoragePaths {
                state: root.join("state.json"),
                profile: root.join("profile.json"),
                liquid_tuning: root.join("liquid-tuning.json"),
                liquid_tuning_status: root.join("liquid-tuning-applied.json"),
                morph_brain: root.join("morph-brain.json"),
                ecology_state: root.join("ecology-state.json"),
                body_state: root.join("body-state.json"),
                body_backup: root.join("backups").join("body-state.previous.json"),
                interaction_replays: root.join("interaction-replays"),
                evolution_runs: root.join("evolution-runs"),
                events: root.join("events.jsonl"),
                telemetry: root.join("telemetry.jsonl"),
                telemetry_previous: root.join("telemetry.previous.jsonl"),
                lab_control: root.join("lab-control.json"),
                lab_control_backup: root.join("backups").join("lab-control.previous.json"),
                runtime_load_ack: root.join("runtime-load-ack.json"),
                runtime_load_ack_backup: root
                    .join("backups")
                    .join("runtime-load-ack.previous.json"),
                backup: root.join("backups").join("state.previous.json"),
                ecology_backup: root.join("backups").join("ecology-state.previous.json"),
                root,
            },
        }
    }

    pub fn load_state(&self) -> Result<Option<PortablePetState>, StorageError> {
        if !self.paths.state.exists() {
            return Ok(None);
        }
        match read_state(&self.paths.state) {
            Ok(state) => Ok(Some(state)),
            Err(primary_error) if self.paths.backup.exists() => read_state(&self.paths.backup)
                .map(Some)
                .map_err(|_| primary_error),
            Err(error) => Err(error),
        }
    }

    pub fn save_state(&self, state: &PortablePetState) -> Result<(), StorageError> {
        state.validate()?;
        atomic_json(&self.paths.state, &self.paths.backup, state)
    }

    pub fn import_state(&self, path: impl AsRef<Path>) -> Result<PortablePetState, StorageError> {
        let state: PortablePetState = serde_json::from_reader(BufReader::new(File::open(path)?))?;
        state.validate()?;
        Ok(state)
    }

    pub fn export_state(
        &self,
        state: &PortablePetState,
        path: impl AsRef<Path>,
    ) -> Result<(), StorageError> {
        state.validate()?;
        let path = path.as_ref();
        let backup = path.with_extension("previous.json");
        atomic_json(path, &backup, state)
    }

    pub fn load_profile(&self) -> Result<UserProfile, StorageError> {
        if !self.paths.profile.exists() {
            return Ok(UserProfile::default());
        }
        let mut profile: UserProfile =
            serde_json::from_reader(BufReader::new(File::open(&self.paths.profile)?))?;
        profile.volume = profile.volume.clamp(0.0, 1.0);
        Ok(profile)
    }

    pub fn save_profile(&self, profile: &UserProfile) -> Result<(), StorageError> {
        atomic_json(
            &self.paths.profile,
            &self
                .paths
                .root
                .join("backups")
                .join("profile.previous.json"),
            profile,
        )
    }

    pub fn load_liquid_tuning<T: DeserializeOwned>(&self) -> Result<Option<T>, StorageError> {
        if !self.paths.liquid_tuning.exists() {
            return Ok(None);
        }
        serde_json::from_reader(BufReader::new(File::open(&self.paths.liquid_tuning)?))
            .map(Some)
            .map_err(StorageError::from)
    }

    pub fn save_liquid_tuning<T: Serialize>(&self, profile: &T) -> Result<(), StorageError> {
        atomic_json(
            &self.paths.liquid_tuning,
            &self
                .paths
                .root
                .join("backups")
                .join("liquid-tuning.previous.json"),
            profile,
        )
    }

    pub fn load_liquid_tuning_status<T: DeserializeOwned>(
        &self,
    ) -> Result<Option<T>, StorageError> {
        if !self.paths.liquid_tuning_status.exists() {
            return Ok(None);
        }
        serde_json::from_reader(BufReader::new(File::open(
            &self.paths.liquid_tuning_status,
        )?))
        .map(Some)
        .map_err(StorageError::from)
    }

    pub fn save_liquid_tuning_status<T: Serialize>(&self, status: &T) -> Result<(), StorageError> {
        atomic_json(
            &self.paths.liquid_tuning_status,
            &self
                .paths
                .root
                .join("backups")
                .join("liquid-tuning-applied.previous.json"),
            status,
        )
    }

    pub fn load_morph_brain<T: DeserializeOwned>(&self) -> Result<Option<T>, StorageError> {
        let backup = self
            .paths
            .root
            .join("backups")
            .join("morph-brain.previous.json");
        if !self.paths.morph_brain.exists() {
            return if backup.exists() {
                read_json(&backup).map(Some)
            } else {
                Ok(None)
            };
        }
        match read_json(&self.paths.morph_brain) {
            Ok(state) => Ok(Some(state)),
            Err(primary_error) if backup.exists() => {
                read_json(&backup).map(Some).map_err(|_| primary_error)
            }
            Err(error) => Err(error),
        }
    }

    pub fn save_morph_brain<T: Serialize>(&self, state: &T) -> Result<(), StorageError> {
        atomic_json(
            &self.paths.morph_brain,
            &self
                .paths
                .root
                .join("backups")
                .join("morph-brain.previous.json"),
            state,
        )
    }

    pub fn load_body_state<T: DeserializeOwned>(&self) -> Result<Option<T>, StorageError> {
        self.load_body_state_validated(|_| true)
    }

    pub fn load_body_state_validated<T: DeserializeOwned>(
        &self,
        validate: impl Fn(&T) -> bool,
    ) -> Result<Option<T>, StorageError> {
        let load_valid = |path: &Path| -> Result<T, StorageError> {
            let state = read_json(path)?;
            if validate(&state) {
                Ok(state)
            } else {
                Err(StorageError::InvalidBodyState)
            }
        };
        if !self.paths.body_state.exists() {
            return if self.paths.body_backup.exists() {
                load_valid(&self.paths.body_backup).map(Some)
            } else {
                Ok(None)
            };
        }
        match load_valid(&self.paths.body_state) {
            Ok(state) => Ok(Some(state)),
            Err(primary_error) if self.paths.body_backup.exists() => {
                load_valid(&self.paths.body_backup)
                    .map(Some)
                    .map_err(|_| primary_error)
            }
            Err(error) => Err(error),
        }
    }

    pub fn save_body_state<T: Serialize>(&self, state: &T) -> Result<(), StorageError> {
        atomic_json(&self.paths.body_state, &self.paths.body_backup, state)
    }

    pub fn load_ecology_state(&self) -> Result<Option<EcologyState>, StorageError> {
        if !self.paths.ecology_state.exists() {
            return if self.paths.ecology_backup.exists() {
                read_ecology_state(&self.paths.ecology_backup).map(Some)
            } else {
                Ok(None)
            };
        }
        match read_ecology_state(&self.paths.ecology_state) {
            Ok(state) => Ok(Some(state)),
            Err(primary_error) if self.paths.ecology_backup.exists() => {
                read_ecology_state(&self.paths.ecology_backup)
                    .map(Some)
                    .map_err(|_| primary_error)
            }
            Err(error) => Err(error),
        }
    }

    pub fn save_ecology_state(&self, state: &EcologyState) -> Result<(), StorageError> {
        state.validate()?;
        atomic_json(&self.paths.ecology_state, &self.paths.ecology_backup, state)
    }

    /// Loads only the primary command slot. A stale backup must never be
    /// replayed after a partially written or externally corrupted command.
    pub fn load_lab_control(&self) -> Result<Option<LabControlEnvelope>, StorageError> {
        if !self.paths.lab_control.exists() {
            return Ok(None);
        }
        let file_bytes = fs::metadata(&self.paths.lab_control)?.len();
        if file_bytes > LAB_CONTROL_MAX_FILE_BYTES {
            return Err(StorageError::LabControlFileTooLarge {
                file_bytes,
                max_bytes: LAB_CONTROL_MAX_FILE_BYTES,
            });
        }
        let control: LabControlEnvelope = read_json(&self.paths.lab_control)?;
        control.validate()?;
        Ok(Some(control))
    }

    /// Replaces the one-command slot; this intentionally does not append a
    /// queue that could grow without a bound or replay old interventions.
    pub fn save_lab_control(&self, control: &LabControlEnvelope) -> Result<(), StorageError> {
        control.validate()?;
        atomic_json(
            &self.paths.lab_control,
            &self.paths.lab_control_backup,
            control,
        )
    }

    pub fn load_runtime_ack<T: DeserializeOwned>(&self) -> Result<Option<T>, StorageError> {
        if !self.paths.runtime_load_ack.exists() {
            return Ok(None);
        }
        read_json(&self.paths.runtime_load_ack).map(Some)
    }

    pub fn save_runtime_ack<T: Serialize>(&self, acknowledgement: &T) -> Result<(), StorageError> {
        atomic_json(
            &self.paths.runtime_load_ack,
            &self.paths.runtime_load_ack_backup,
            acknowledgement,
        )
    }

    pub fn append_event(&self, event: &EventLogEntry) -> Result<(), StorageError> {
        fs::create_dir_all(&self.paths.root)?;
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.paths.events)?;
        serde_json::to_writer(&mut file, event)?;
        file.write_all(b"\n")?;
        file.flush()?;
        Ok(())
    }

    pub fn append_telemetry(&self, event: &EventLogEntry) -> Result<(), StorageError> {
        self.append_telemetry_with_cap(event, TELEMETRY_LOG_MAX_BYTES)
    }

    fn append_telemetry_with_cap(
        &self,
        event: &EventLogEntry,
        max_bytes: u64,
    ) -> Result<(), StorageError> {
        let line = encode_json_line(event)?;
        let record_bytes = line.len() as u64;
        if record_bytes > max_bytes {
            return Err(StorageError::TelemetryRecordTooLarge {
                record_bytes,
                max_bytes,
            });
        }

        fs::create_dir_all(&self.paths.root)?;
        let current_bytes = match fs::metadata(&self.paths.telemetry) {
            Ok(metadata) => metadata.len(),
            Err(error) if error.kind() == io::ErrorKind::NotFound => 0,
            Err(error) => return Err(error.into()),
        };
        if current_bytes > 0 && current_bytes.saturating_add(record_bytes) > max_bytes {
            if self.paths.telemetry_previous.exists() {
                fs::remove_file(&self.paths.telemetry_previous)?;
            }
            fs::rename(&self.paths.telemetry, &self.paths.telemetry_previous)?;
        }

        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.paths.telemetry)?;
        file.write_all(&line)?;
        file.flush()?;
        Ok(())
    }
}

fn encode_json_line<T: Serialize>(value: &T) -> Result<Vec<u8>, StorageError> {
    let mut line = serde_json::to_vec(value)?;
    line.push(b'\n');
    Ok(line)
}

fn read_state(path: &Path) -> Result<PortablePetState, StorageError> {
    let state: PortablePetState = serde_json::from_reader(BufReader::new(File::open(path)?))?;
    state.validate()?;
    Ok(state)
}

fn read_ecology_state(path: &Path) -> Result<EcologyState, StorageError> {
    let state: EcologyState = serde_json::from_reader(BufReader::new(File::open(path)?))?;
    Ok(EcologyState::restore(state)?)
}

fn atomic_json<T: Serialize>(path: &Path, backup: &Path, value: &T) -> Result<(), StorageError> {
    let parent = path.parent().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "state path has no parent directory",
        )
    })?;
    fs::create_dir_all(parent)?;
    if let Some(backup_parent) = backup.parent() {
        fs::create_dir_all(backup_parent)?;
    }
    let mut temporary = NamedTempFile::new_in(parent)?;
    {
        let mut writer = BufWriter::new(temporary.as_file_mut());
        serde_json::to_writer_pretty(&mut writer, value)?;
        writer.write_all(b"\n")?;
        writer.flush()?;
    }
    temporary.as_file_mut().sync_all()?;
    if path.exists() {
        if backup.exists() {
            fs::remove_file(backup)?;
        }
        fs::rename(path, backup)?;
    }
    let temporary_path = temporary.into_temp_path();
    if let Err(error) = fs::rename(&temporary_path, path) {
        if backup.exists() && !path.exists() {
            let _ = fs::rename(backup, path);
        }
        return Err(StorageError::Io(error));
    }
    Ok(())
}

fn read_json<T: DeserializeOwned>(path: &Path) -> Result<T, StorageError> {
    serde_json::from_reader(BufReader::new(File::open(path)?)).map_err(StorageError::from)
}

#[cfg(test)]
mod tests {
    use lifecore::{Genome, LifeCore};

    use super::*;

    fn telemetry_event(sequence: u64, payload_bytes: usize) -> EventLogEntry {
        EventLogEntry {
            monotonic_seconds: sequence as f64,
            kind: "debug_state".into(),
            details: serde_json::json!({
                "sequence": sequence,
                "payload": "x".repeat(payload_bytes),
            }),
        }
    }

    fn lab_control(command_id: u64, command: crate::LabControlCommand) -> LabControlEnvelope {
        LabControlEnvelope {
            schema_version: crate::LAB_CONTROL_SCHEMA_VERSION,
            command_id,
            issued_unix_ms: 1_728_000_000_000,
            expires_after_ms: 5_000,
            command,
        }
    }

    fn read_json_lines(path: &Path) -> Vec<EventLogEntry> {
        let bytes = fs::read(path).unwrap();
        assert!(
            !bytes.is_empty(),
            "{} is unexpectedly empty",
            path.display()
        );
        assert_eq!(
            bytes.last(),
            Some(&b'\n'),
            "{} ends with a partial JSONL record",
            path.display()
        );
        bytes
            .split(|byte| *byte == b'\n')
            .filter(|line| !line.is_empty())
            .map(|line| serde_json::from_slice(line).unwrap())
            .collect()
    }

    fn telemetry_sequences(path: &Path) -> Vec<u64> {
        read_json_lines(path)
            .into_iter()
            .map(|event| event.details["sequence"].as_u64().unwrap())
            .collect()
    }

    #[test]
    fn state_roundtrip_is_valid_and_creates_backup() {
        let directory = tempfile::tempdir().unwrap();
        let store = StateStore::at(directory.path());
        let life = LifeCore::new(Genome::from_seed(9), 9);
        let state = PortablePetState {
            schema_version: PORTABLE_STATE_SCHEMA_VERSION,
            life: life.snapshot(),
            vita: None,
            position: PersistedPetPosition::default(),
        };
        store.save_state(&state).unwrap();
        assert_eq!(store.load_state().unwrap(), Some(state.clone()));
        store.save_state(&state).unwrap();
        assert!(store.paths.backup.exists());
    }

    #[test]
    fn legacy_snapshot_repairs_voice_anatomy_and_gestures() {
        let directory = tempfile::tempdir().unwrap();
        let store = StateStore::at(directory.path());
        let life = LifeCore::new(Genome::from_seed(0x001E_6AC7), 0x001E_6AC7);
        let mut legacy_life = life.snapshot();
        let expected_pitch = legacy_life.state.genome.voice.base_pitch_hz;
        let expected_motif_ids = legacy_life
            .state
            .vocal_motifs
            .iter()
            .map(|motif| motif.id)
            .collect::<Vec<_>>();
        let expected_habits = legacy_life.habits.clone();
        let expected_memories = legacy_life.memories.clone();
        legacy_life.state.genome.voice.anatomy = lifecore::VoiceAnatomy::default();
        for motif in &mut legacy_life.state.vocal_motifs {
            for syllable in &mut motif.syllables {
                syllable.gesture = lifecore::VocalGesture::default();
            }
        }
        let portable = PortablePetState {
            schema_version: PORTABLE_STATE_SCHEMA_VERSION,
            life: legacy_life,
            vita: None,
            position: PersistedPetPosition::default(),
        };
        fs::create_dir_all(&store.paths.root).unwrap();
        fs::write(
            &store.paths.state,
            serde_json::to_vec_pretty(&portable).unwrap(),
        )
        .unwrap();

        let loaded = store
            .load_state()
            .expect("legacy storage validation accepts additive voice migration")
            .expect("legacy state exists");
        let restored = LifeCore::restore(loaded.life).expect("legacy voice repairs in place");
        let restored_snapshot = restored.snapshot();
        assert_eq!(restored.state.genome.voice.anatomy.schema, 1);
        assert_eq!(restored.state.genome.voice.base_pitch_hz, expected_pitch);
        assert_eq!(
            restored
                .state
                .vocal_motifs
                .iter()
                .map(|motif| motif.id)
                .collect::<Vec<_>>(),
            expected_motif_ids
        );
        assert_eq!(restored_snapshot.habits, expected_habits);
        assert_eq!(restored_snapshot.memories, expected_memories);
        assert!(
            restored
                .state
                .vocal_motifs
                .iter()
                .flat_map(|motif| &motif.syllables)
                .any(|syllable| syllable.gesture != lifecore::VocalGesture::default())
        );
    }

    #[test]
    fn liquid_tuning_roundtrip_is_atomic_and_keeps_previous_backup() {
        let directory = tempfile::tempdir().unwrap();
        let store = StateStore::at(directory.path());
        let first = serde_json::json!({"schema_version": 1, "name": "first"});
        let second = serde_json::json!({"schema_version": 1, "name": "second"});

        store.save_liquid_tuning(&first).unwrap();
        assert_eq!(
            store
                .load_liquid_tuning::<serde_json::Value>()
                .unwrap()
                .unwrap(),
            first
        );
        store.save_liquid_tuning(&second).unwrap();

        assert_eq!(
            store
                .load_liquid_tuning::<serde_json::Value>()
                .unwrap()
                .unwrap(),
            second
        );
        let backup: serde_json::Value = serde_json::from_reader(BufReader::new(
            File::open(
                directory
                    .path()
                    .join("backups")
                    .join("liquid-tuning.previous.json"),
            )
            .unwrap(),
        ))
        .unwrap();
        assert_eq!(backup, first);
    }

    #[test]
    fn liquid_tuning_status_roundtrip_is_separate_from_the_authored_profile() {
        let directory = tempfile::tempdir().unwrap();
        let store = StateStore::at(directory.path());
        let profile = serde_json::json!({"profile_revision": 7});
        let status = serde_json::json!({"profile_revision": 7, "applied": true});

        store.save_liquid_tuning(&profile).unwrap();
        store.save_liquid_tuning_status(&status).unwrap();

        assert_eq!(
            store
                .load_liquid_tuning_status::<serde_json::Value>()
                .unwrap(),
            Some(status)
        );
        assert_eq!(
            store.load_liquid_tuning::<serde_json::Value>().unwrap(),
            Some(profile)
        );
    }

    #[test]
    fn rejects_non_finite_or_out_of_range_position() {
        let life = LifeCore::new(Genome::from_seed(3), 3);
        let state = PortablePetState {
            schema_version: PORTABLE_STATE_SCHEMA_VERSION,
            life: life.snapshot(),
            vita: None,
            position: PersistedPetPosition {
                monitor: None,
                normalized: crate::NormalizedDesktopPoint { x: 1.2, y: 0.5 },
            },
        };
        assert!(matches!(
            state.validate(),
            Err(StorageError::InvalidPosition)
        ));
    }

    #[test]
    fn corrupt_primary_recovers_previous_valid_snapshot() {
        let directory = tempfile::tempdir().unwrap();
        let store = StateStore::at(directory.path());
        let life = LifeCore::new(Genome::from_seed(12), 12);
        let state = PortablePetState {
            schema_version: PORTABLE_STATE_SCHEMA_VERSION,
            life: life.snapshot(),
            vita: None,
            position: PersistedPetPosition::default(),
        };
        store.save_state(&state).unwrap();
        store.save_state(&state).unwrap();
        std::fs::write(&store.paths.state, b"{broken").unwrap();
        assert_eq!(store.load_state().unwrap(), Some(state));
    }

    #[test]
    fn corrupt_morph_primary_recovers_previous_learning_snapshot() {
        let directory = tempfile::tempdir().unwrap();
        let store = StateStore::at(directory.path());
        let first = serde_json::json!({"schema": 1, "weights": [0.1, 0.2]});
        let second = serde_json::json!({"schema": 1, "weights": [0.3, 0.4]});

        store.save_morph_brain(&first).unwrap();
        store.save_morph_brain(&second).unwrap();
        std::fs::write(&store.paths.morph_brain, b"{broken").unwrap();

        assert_eq!(
            store
                .load_morph_brain::<serde_json::Value>()
                .unwrap()
                .unwrap(),
            first
        );
    }

    #[test]
    fn lab_control_round_trip_atomically_replaces_the_single_command_slot() {
        let directory = tempfile::tempdir().unwrap();
        let store = StateStore::at(directory.path());
        assert_eq!(
            store.paths.lab_control,
            directory.path().join("lab-control.json")
        );
        assert_eq!(store.load_lab_control().unwrap(), None);

        let first = lab_control(
            101,
            crate::LabControlCommand::CueAttention {
                position: [0.25, 0.75],
                duration_seconds: 2.0,
            },
        );
        let latest = lab_control(
            102,
            crate::LabControlCommand::DrivePulse {
                drive: crate::LabDrive::Curiosity,
                delta: 0.6,
                duration_seconds: 4.0,
            },
        );

        store.save_lab_control(&first).unwrap();
        assert_eq!(store.load_lab_control().unwrap(), Some(first.clone()));
        store.save_lab_control(&latest).unwrap();

        assert_eq!(store.load_lab_control().unwrap(), Some(latest));
        assert_eq!(
            read_json::<LabControlEnvelope>(&store.paths.lab_control_backup).unwrap(),
            first
        );
        assert_eq!(
            fs::read_dir(directory.path())
                .unwrap()
                .filter_map(Result::ok)
                .filter(|entry| entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with("lab-control"))
                .count(),
            1
        );
    }

    #[test]
    fn invalid_lab_control_is_rejected_without_replacing_the_current_command() {
        let directory = tempfile::tempdir().unwrap();
        let store = StateStore::at(directory.path());
        let valid = lab_control(7, crate::LabControlCommand::FocusMode { enabled: true });
        store.save_lab_control(&valid).unwrap();
        let original = fs::read(&store.paths.lab_control).unwrap();

        let mut invalid = valid;
        invalid.expires_after_ms = crate::LAB_CONTROL_MAX_EXPIRY_MS + 1;
        assert!(matches!(
            store.save_lab_control(&invalid),
            Err(StorageError::InvalidLabControl(
                LabControlValidationError::InvalidExpiry
            ))
        ));
        assert_eq!(fs::read(&store.paths.lab_control).unwrap(), original);
        assert!(!store.paths.lab_control_backup.exists());
    }

    #[test]
    fn lab_control_load_never_falls_back_to_a_stale_backup() {
        let directory = tempfile::tempdir().unwrap();
        let store = StateStore::at(directory.path());
        let first = lab_control(1, crate::LabControlCommand::ClearDrivePulses);
        let second = lab_control(2, crate::LabControlCommand::Reward { value: 0.5 });
        store.save_lab_control(&first).unwrap();
        store.save_lab_control(&second).unwrap();
        assert!(store.paths.lab_control_backup.exists());

        fs::write(&store.paths.lab_control, b"{broken").unwrap();
        assert!(matches!(
            store.load_lab_control(),
            Err(StorageError::Json(_))
        ));
    }

    #[test]
    fn lab_control_load_validates_schema_and_caps_external_file_size() {
        let directory = tempfile::tempdir().unwrap();
        let store = StateStore::at(directory.path());
        fs::create_dir_all(&store.paths.root).unwrap();

        let mut invalid_schema =
            lab_control(1, crate::LabControlCommand::FocusMode { enabled: false });
        invalid_schema.schema_version = crate::LAB_CONTROL_SCHEMA_VERSION + 1;
        fs::write(
            &store.paths.lab_control,
            serde_json::to_vec(&invalid_schema).unwrap(),
        )
        .unwrap();
        assert!(matches!(
            store.load_lab_control(),
            Err(StorageError::InvalidLabControl(
                LabControlValidationError::UnsupportedSchema
            ))
        ));

        fs::write(
            &store.paths.lab_control,
            vec![b' '; LAB_CONTROL_MAX_FILE_BYTES as usize + 1],
        )
        .unwrap();
        assert!(matches!(
            store.load_lab_control(),
            Err(StorageError::LabControlFileTooLarge { .. })
        ));
    }

    #[test]
    fn telemetry_rotation_is_bounded_parseable_and_keeps_only_one_previous_file() {
        let directory = tempfile::tempdir().unwrap();
        let store = StateStore::at(directory.path());
        assert_eq!(
            store.paths.telemetry,
            directory.path().join("telemetry.jsonl")
        );
        assert_eq!(
            store.paths.telemetry_previous,
            directory.path().join("telemetry.previous.jsonl")
        );

        let semantic = EventLogEntry {
            monotonic_seconds: 1.0,
            kind: "user_feedback".into(),
            details: serde_json::json!({"reward": 1.0}),
        };
        store.append_event(&semantic).unwrap();
        let semantic_bytes = fs::read(&store.paths.events).unwrap();

        let record_bytes = encode_json_line(&telemetry_event(0, 32)).unwrap().len() as u64;
        let max_bytes = record_bytes * 2;
        for sequence in 0..7 {
            store
                .append_telemetry_with_cap(&telemetry_event(sequence, 32), max_bytes)
                .unwrap();
        }

        assert!(fs::metadata(&store.paths.telemetry).unwrap().len() <= max_bytes);
        assert!(fs::metadata(&store.paths.telemetry_previous).unwrap().len() <= max_bytes);
        assert_eq!(telemetry_sequences(&store.paths.telemetry_previous), [4, 5]);
        assert_eq!(telemetry_sequences(&store.paths.telemetry), [6]);
        assert_eq!(fs::read(&store.paths.events).unwrap(), semantic_bytes);

        let mut telemetry_files = fs::read_dir(directory.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|name| name.starts_with("telemetry"))
            .collect::<Vec<_>>();
        telemetry_files.sort();
        assert_eq!(
            telemetry_files,
            ["telemetry.jsonl", "telemetry.previous.jsonl"]
        );
    }

    #[test]
    fn telemetry_rotates_before_append_without_splitting_the_tail_record() {
        let directory = tempfile::tempdir().unwrap();
        let store = StateStore::at(directory.path());
        let first = telemetry_event(1, 48);
        let second = telemetry_event(2, 48);
        let first_bytes = encode_json_line(&first).unwrap().len() as u64;
        let second_bytes = encode_json_line(&second).unwrap().len() as u64;
        let max_bytes = first_bytes + second_bytes - 1;

        store.append_telemetry_with_cap(&first, max_bytes).unwrap();
        store.append_telemetry_with_cap(&second, max_bytes).unwrap();

        assert_eq!(telemetry_sequences(&store.paths.telemetry_previous), [1]);
        assert_eq!(telemetry_sequences(&store.paths.telemetry), [2]);
        assert!(fs::metadata(&store.paths.telemetry).unwrap().len() <= max_bytes);
        assert!(fs::metadata(&store.paths.telemetry_previous).unwrap().len() <= max_bytes);
    }

    #[test]
    fn oversized_telemetry_record_is_rejected_without_rotation_or_partial_append() {
        let directory = tempfile::tempdir().unwrap();
        let store = StateStore::at(directory.path());
        let first = telemetry_event(1, 8);
        let max_bytes = encode_json_line(&first).unwrap().len() as u64;
        store.append_telemetry_with_cap(&first, max_bytes).unwrap();
        let original = fs::read(&store.paths.telemetry).unwrap();

        let oversized = telemetry_event(2, 512);
        assert!(matches!(
            store.append_telemetry_with_cap(&oversized, max_bytes),
            Err(StorageError::TelemetryRecordTooLarge { .. })
        ));
        assert_eq!(fs::read(&store.paths.telemetry).unwrap(), original);
        assert!(!store.paths.telemetry_previous.exists());
        assert_eq!(telemetry_sequences(&store.paths.telemetry), [1]);
    }

    #[test]
    fn ecology_roundtrip_is_separate_and_recovers_previous_valid_snapshot() {
        let directory = tempfile::tempdir().unwrap();
        let store = StateStore::at(directory.path());
        let first = EcologyState::new(101);
        let mut second = first.clone();
        second.den.visits = 4;

        store.save_ecology_state(&first).unwrap();
        store.save_ecology_state(&second).unwrap();
        assert_eq!(store.load_ecology_state().unwrap(), Some(second));
        assert!(store.paths.ecology_backup.exists());

        std::fs::write(&store.paths.ecology_state, b"{broken").unwrap();
        assert_eq!(store.load_ecology_state().unwrap(), Some(first));
    }
}
