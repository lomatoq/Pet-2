use std::{
    fs,
    io::Write,
    path::Path,
    time::{Instant, SystemTime, UNIX_EPOCH},
};

pub struct BirthRuntime {
    born_unix_seconds: u64,
    pub started: Option<Instant>,
}
impl BirthRuntime {
    pub fn load(directory: &Path, seed: u64) -> Self {
        let now = now_seconds();
        let path = directory.join(format!("birth-{seed}.txt"));
        let existing = fs::read_to_string(&path)
            .ok()
            .and_then(|s| s.trim().parse::<u64>().ok());
        let born = existing.unwrap_or(now).min(now);
        if existing.is_none() {
            let result = fs::create_dir_all(directory).and_then(|()| {
                let mut file = fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&path)?;
                writeln!(file, "{born}")?;
                file.sync_all()
            });
            if let Err(error) = result {
                eprintln!("could not persist birth calendar: {error}");
            }
        }
        Self {
            born_unix_seconds: born,
            started: existing.is_none().then(Instant::now),
        }
    }
    pub fn age_seconds(&self) -> f64 {
        now_seconds().saturating_sub(self.born_unix_seconds) as f64
    }
    pub fn replay(&mut self) {
        self.started = Some(Instant::now());
    }
    pub fn elapsed(&mut self) -> Option<f32> {
        let elapsed = self.started?.elapsed().as_secs_f32();
        if elapsed >= 14.0 {
            self.started = None;
            None
        } else {
            Some(elapsed)
        }
    }
}
fn now_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn restart_and_replay_preserve_birth_calendar() {
        let dir = std::env::temp_dir().join(format!(
            "pet2-birth-test-{}-{}",
            std::process::id(),
            now_seconds()
        ));
        let first = BirthRuntime::load(&dir, 42);
        assert!(first.started.is_some());
        let mut second = BirthRuntime::load(&dir, 42);
        assert!(second.started.is_none());
        second.replay();
        assert_eq!(first.born_unix_seconds, second.born_unix_seconds);
        assert!(second.started.is_some());
        fs::remove_file(dir.join("birth-42.txt")).unwrap();
        fs::remove_dir(dir).unwrap();
    }
}
