//! The native click route and its own native helper share one launch channel.
use desktop_host::CareMenuChannel;
use std::{
    io,
    path::Path,
    process::{Child, Command},
};

pub(crate) struct CareMenuRuntime {
    pub(crate) channel: CareMenuChannel,
    pub(crate) child: Option<Child>,
}

impl CareMenuRuntime {
    pub(crate) fn new(root: &Path) -> Self {
        let mut runtime = Self {
            channel: CareMenuChannel::new_launch(root),
            child: None,
        };
        if let Err(error) = runtime.spawn(true) {
            eprintln!("care menu startup: {error}");
        }
        runtime
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
        self.channel.write_placement(placement)?;
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
