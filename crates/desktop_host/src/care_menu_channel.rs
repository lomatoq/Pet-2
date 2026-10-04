//! Care UI ownership belongs to one running Pet, not to every historical binary
//! sharing its save directory. Ordinary console/save paths remain unchanged.
use std::{
    io,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Clone, Debug)]
pub struct CareMenuChannel {
    root: PathBuf,
    owner: Option<String>,
}

impl CareMenuChannel {
    pub fn legacy(root: &Path) -> Self {
        Self {
            root: root.to_owned(),
            owner: None,
        }
    }

    pub fn for_owner(root: &Path, owner: &str) -> io::Result<Self> {
        if owner.is_empty()
            || owner.len() > 64
            || !owner
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid care menu owner",
            ));
        }
        Ok(Self {
            root: root.to_owned(),
            owner: Some(owner.to_owned()),
        })
    }

    pub fn new_launch(root: &Path) -> Self {
        let epoch = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        Self::for_owner(root, &format!("{:x}-{epoch:x}", std::process::id()))
            .expect("generated care owner is valid")
    }

    pub fn owner(&self) -> Option<&str> {
        self.owner.as_deref()
    }

    fn path(&self, suffix: &str) -> PathBuf {
        self.root.join(match &self.owner {
            Some(owner) => format!("companion-menu-{owner}-{suffix}"),
            None => format!("companion-menu-{suffix}"),
        })
    }

    pub fn open_path(&self) -> PathBuf {
        self.path("open")
    }
    pub fn placement_path(&self) -> PathBuf {
        self.path("placement.json")
    }

    pub fn control_store(&self) -> crate::StateStore {
        let mut store = crate::StateStore::at(self.root.clone());
        if self.owner.is_some() {
            store.paths.lab_control = self.path("control.json");
            store.paths.lab_control_backup = self.path("control.bak.json");
        }
        store
    }

    pub fn request_open(&self) -> io::Result<()> {
        std::fs::write(self.open_path(), b"open")
    }
    pub fn consume_open(&self) -> bool {
        std::fs::remove_file(self.open_path()).is_ok()
    }

    pub fn write_placement(&self, placement: &serde_json::Value) -> io::Result<()> {
        let pending = self.path("placement.next");
        std::fs::write(&pending, serde_json::to_vec(placement)?)?;
        std::fs::rename(pending, self.placement_path())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn old_menu_and_other_pet_launch_cannot_consume_current_request() {
        let data = tempfile::tempdir().unwrap();
        let legacy = CareMenuChannel::legacy(data.path());
        let old = CareMenuChannel::for_owner(data.path(), "old-pet").unwrap();
        let current = CareMenuChannel::for_owner(data.path(), "current-pet").unwrap();
        current
            .write_placement(&serde_json::json!({"x":42}))
            .unwrap();
        current.request_open().unwrap();
        assert!(!legacy.consume_open());
        assert!(!old.consume_open());
        assert!(current.consume_open());
        assert!(!current.consume_open());
        assert!(!legacy.placement_path().exists());
        assert_eq!(
            std::fs::read(current.placement_path()).unwrap(),
            b"{\"x\":42}"
        );
    }
    #[test]
    fn reopened_menu_updates_its_anchor_without_overwriting_legacy_placement() {
        let data = tempfile::tempdir().unwrap();
        let legacy = CareMenuChannel::legacy(data.path());
        let current = CareMenuChannel::for_owner(data.path(), "current-pet").unwrap();
        legacy
            .write_placement(&serde_json::json!({"x":10}))
            .unwrap();
        for x in [42, 99] {
            current
                .write_placement(&serde_json::json!({"x":x}))
                .unwrap();
            current.request_open().unwrap();
            assert!(current.consume_open());
            let placement: serde_json::Value =
                serde_json::from_slice(&std::fs::read(current.placement_path()).unwrap()).unwrap();
            assert_eq!(placement["x"], x);
        }
        assert_eq!(
            std::fs::read(legacy.placement_path()).unwrap(),
            b"{\"x\":10}"
        );
    }
    #[test]
    fn legacy_renewal_cannot_replace_care_open_close_or_feeding() {
        use crate::{LAB_CONTROL_SCHEMA_VERSION, LabControlCommand, LabControlEnvelope};
        let data = tempfile::tempdir().unwrap();
        let legacy = CareMenuChannel::legacy(data.path()).control_store();
        let care = CareMenuChannel::for_owner(data.path(), "current-pet")
            .unwrap()
            .control_store();
        let envelope = |id, command| LabControlEnvelope {
            schema_version: LAB_CONTROL_SCHEMA_VERSION,
            command_id: id,
            issued_unix_ms: 1_791_116_000_000,
            expires_after_ms: 5_000,
            session_token: Some("0123456789abcdef0123456789abcdef".into()),
            command,
        };
        for (id, command) in [
            (
                2,
                LabControlCommand::OpenSession {
                    protocol_version: crate::LAB_SESSION_PROTOCOL_VERSION,
                    lease_seconds: 10,
                },
            ),
            (4, LabControlCommand::CloseSession),
            (6, LabControlCommand::Feeding { enabled: true }),
        ] {
            care.save_lab_control(&envelope(id, command)).unwrap();
            legacy
                .save_lab_control(&envelope(
                    id + 1,
                    LabControlCommand::RenewSession { lease_seconds: 10 },
                ))
                .unwrap();
            assert_eq!(
                care.load_lab_control().unwrap().unwrap().command_id,
                id,
                "legacy writer replaced care request"
            );
            assert_eq!(
                legacy.load_lab_control().unwrap().unwrap().command_id,
                id + 1
            );
        }
        assert_ne!(care.paths.lab_control, legacy.paths.lab_control);
        assert_ne!(
            care.paths.lab_control_backup,
            legacy.paths.lab_control_backup
        );
        assert_eq!(care.paths.telemetry, legacy.paths.telemetry);
        assert_eq!(care.paths.state, legacy.paths.state);
    }
    #[test]
    fn owner_cannot_escape_directory_and_launches_are_distinct() {
        let data = tempfile::tempdir().unwrap();
        for invalid in ["", "../pet", "a/b", "a\\b", "a:b", "hello world"] {
            assert!(CareMenuChannel::for_owner(data.path(), invalid).is_err());
        }
        assert_ne!(
            CareMenuChannel::new_launch(data.path()).owner(),
            CareMenuChannel::new_launch(data.path()).owner()
        );
        assert_eq!(
            CareMenuChannel::legacy(data.path()).open_path(),
            data.path().join("companion-menu-open")
        );
    }
}
