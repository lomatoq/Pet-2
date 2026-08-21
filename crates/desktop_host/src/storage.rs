use std::{
    fs::{self, File, OpenOptions},
    io::{self, BufReader, BufWriter, Write},
    path::{Path, PathBuf},
};

use directories::ProjectDirs;
use lifecore::{LifeError, LifeSnapshot, VitaState};
use serde::{Deserialize, Serialize};
use tempfile::NamedTempFile;
use thiserror::Error;

use crate::PersistedPetPosition;

pub const PORTABLE_STATE_SCHEMA_VERSION: u32 = 1;

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
        self.life.validate()?;
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
    pub events: PathBuf,
    pub backup: PathBuf,
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
                events: root.join("events.jsonl"),
                backup: root.join("backups").join("state.previous.json"),
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
}

fn read_state(path: &Path) -> Result<PortablePetState, StorageError> {
    let state: PortablePetState = serde_json::from_reader(BufReader::new(File::open(path)?))?;
    state.validate()?;
    Ok(state)
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

#[cfg(test)]
mod tests {
    use lifecore::{Genome, LifeCore};

    use super::*;

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
}
