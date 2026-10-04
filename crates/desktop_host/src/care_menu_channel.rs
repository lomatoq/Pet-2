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
