//! The native click route and its own native helper share one launch channel.
use desktop_host::{CareMenuChannel, StateStore};
use std::{
    io,
    path::Path,
    process::{Child, Command},
};

pub(crate) struct CareMenuRuntime {
    pub(crate) channel: CareMenuChannel,
    pub(crate) child: Option<Child>,
    pub(crate) control_store: StateStore,
    pub(crate) admission: super::LabInterventionState,
}

impl CareMenuRuntime {
    pub(crate) fn new(root: &Path) -> Self {
        let channel = CareMenuChannel::new_launch(root);
        let mut runtime = Self {
            control_store: channel.control_store(),
            admission: super::LabInterventionState::new(super::unix_time_millis(
                std::time::SystemTime::now(),
            )),
            channel,
            child: None,
        };
        if let Err(error) = runtime.spawn(true) {
            eprintln!("care menu startup: {error}");
        }
        runtime
    }

    pub(crate) fn control_ack(&self) -> serde_json::Value {
        serde_json::json!({"owner":self.channel.owner(),
            "last_command_id":self.admission.last_seen_command_id,
            "last_command_status":self.admission.last_command_status.as_str()})
    }

    fn spawn(&mut self, idle: bool) -> io::Result<()> {
        let folder = std::env::current_exe()?
            .parent()
            .ok_or_else(|| io::Error::other("Pet executable has no folder"))?
            .to_owned();
        let helper = [
            folder.join("Pet2 Dev Console.exe"),
            folder.join("body_lab.exe"),
        ]
        .into_iter()
        .find(|p| p.is_file())
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "sibling care helper is missing"))?;
        self.child = Some(
            Command::new(helper)
                .arg(if idle {
                    "--pet-menu-idle"
                } else {
                    "--pet-menu"
                })
                .arg("--data-dir")
                .arg(self.channel.open_path().parent().expect("channel has root"))
                .arg("--menu-owner")
                .arg(self.channel.owner().expect("Pet has launch owner"))
                .spawn()?,
        );
        Ok(())
    }

    pub(crate) fn open(&mut self, placement: &serde_json::Value) -> io::Result<()> {
        let mut placement = placement.clone();
        // Keep the input time across native focus loss and helper scheduling.
        // A close request issued while visible must not reopen a completed fade.
        placement["secondary_press_unix_ms"] = serde_json::json!(super::unix_time_millis(
            std::time::SystemTime::now(),
        ));
        self.channel.write_placement(&placement)?;
        let running = match self.child.as_mut() {
            Some(child) => child.try_wait()?.is_none(),
            None => false,
        };
        if !running {
            self.spawn(true)?;
        }
        // Always signal this launch's channel, including startup/spawn races.
        // A legacy helper cannot consume this request or placement.
        self.channel.request_open()
    }
}
