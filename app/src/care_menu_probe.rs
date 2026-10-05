//! Bounded native care-route integration fixture. It creates no desktop organism.
//! The actual Pet executable owns/spawns its sibling menu, receives native right
//! clicks, and runs the same storage/admission/session transitions as production.
use super::{
    LabCommandAdmission, LabInterventionState, LabSessionState, apply_feeding_mode,
    apply_lab_session_command, cancel_care_placement, unix_time_millis,
};
use crate::care_menu_runtime::CareMenuRuntime;
use desktop_host::{
    LabControlCommand, RUNTIME_LOAD_ACK_SCHEMA_VERSION, RuntimeLoadAcknowledgement,
    RuntimeLoadStatus, StateStore,
};
use std::{
    error::Error,
    sync::Arc,
    time::{Duration, Instant, SystemTime},
};
use winit::{
    application::ApplicationHandler,
    event::{ElementState, MouseButton, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    window::{Window, WindowId},
};

pub(crate) fn run(store: StateStore) -> Result<(), Box<dyn Error>> {
    let event_loop = EventLoop::new()?;
    let mut probe = Probe {
        menu: CareMenuRuntime::new(&store.paths.root),
        store,
        window: None,
        session: LabSessionState::default(),
        admission: LabInterventionState::new(unix_time_millis(SystemTime::now())),
        start: Instant::now(),
        next_frame: Instant::now(),
        sequence: 0,
        right_clicks: 0,
        commands: vec![],
        care_commands: vec![],
        command_events: vec![],
        feeding_seconds: 0.0,
        cleanup_mode: false,
    };
    event_loop.run_app(&mut probe)?;
    if let Some(child) = probe.menu.child.as_mut() {
        let _ = child.kill();
        let _ = child.wait();
    }
    Ok(())
}

struct Probe {
    store: StateStore,
    menu: CareMenuRuntime,
    window: Option<Arc<Window>>,
    session: LabSessionState,
    admission: LabInterventionState,
    start: Instant,
    next_frame: Instant,
    sequence: u64,
    right_clicks: u64,
    commands: Vec<String>,
    care_commands: Vec<String>,
    command_events: Vec<serde_json::Value>,
    feeding_seconds: f32,
    cleanup_mode: bool,
}
impl Probe {
    fn report(&self) {
        let report = serde_json::json!({"pid":std::process::id(),"helper_pid":self.menu.child.as_ref().map(|c|c.id()),
            "owner":self.menu.channel.owner(),"right_clicks":self.right_clicks,"commands":self.commands,"care_commands":self.care_commands,"command_events":self.command_events,"care_ack":self.menu.control_ack(),
            "session_active":self.session.active(Instant::now()),"session_id":self.session.session_id,
            "feeding_enabled":self.feeding_seconds>0.0});
        let pending = self.store.paths.root.join("care-probe-report.next");
        let _ = std::fs::write(&pending, report.to_string());
        let _ = std::fs::rename(
            pending,
            self.store.paths.root.join("care-probe-report.json"),
        );
    }
}
impl ApplicationHandler for Probe {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        self.window = Some(Arc::new(
            event_loop
                .create_window(
                    Window::default_attributes()
                        .with_title("Pet2 Care Route Probe")
                        .with_visible(false),
                )
                .expect("probe window"),
        ));
        self.report();
    }
    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        if self.window.as_ref().is_none_or(|w| w.id() != id) {
            return;
        }
        match event {
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Right,
                ..
            } => {
                self.right_clicks += 1;
                if !cancel_care_placement(&mut self.feeding_seconds, &mut self.cleanup_mode) {
                    self.menu.open(&serde_json::json!({"x":800,"y":900,"left":0,"top":0,"right":1920,"bottom":1080})).expect("native right-click care request");
                }
                self.report();
            }
            WindowEvent::CloseRequested => event_loop.exit(),
            _ => {}
        }
    }
    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let now = Instant::now();
        if now < self.next_frame {
            event_loop.set_control_flow(ControlFlow::WaitUntil(self.next_frame));
            return;
        }
        self.next_frame = now + Duration::from_millis(20);
        if self.start.elapsed() > Duration::from_secs(30) {
            event_loop.exit();
            return;
        }
        let wall = unix_time_millis(SystemTime::now());
        let _ = self.store.save_runtime_ack(&RuntimeLoadAcknowledgement {
            schema_version: RUNTIME_LOAD_ACK_SCHEMA_VERSION,
            status: RuntimeLoadStatus::Running,
            pid: std::process::id(),
            executable_version: env!("CARGO_PKG_VERSION").into(),
            loaded_life_state_hash: 42,
            loaded_genome_hash: 7,
            updated_unix_ms: wall,
        });
        for (care, store) in [
            (false, self.store.clone()),
            (true, self.menu.control_store.clone()),
        ] {
            if let Ok(Some(envelope)) = store.load_lab_control() {
                let ledger = if care {
                    &mut self.menu.admission
                } else {
                    &mut self.admission
                };
                if ledger.admit_command(&envelope, wall) != LabCommandAdmission::Execute {
                    continue;
                }
                let token = envelope.session_token.as_deref();
                let changed = if let Some(changed) =
                    apply_lab_session_command(&mut self.session, &envelope.command, token, now)
                {
                    changed
                } else if let LabControlCommand::Feeding { enabled } = envelope.command
                    && self.session.accepts(token, now)
                {
                    apply_feeding_mode(enabled, &mut self.feeding_seconds, &mut self.cleanup_mode);
                    true
                } else {
                    false
                };
                ledger.note_applied(changed);
                let name = super::lab_command_name(&envelope.command).to_owned();
                self.command_events.push(
                    serde_json::json!({"source":if care {"care"} else {"generic"},
                    "command":name,"applied":changed,"command_id":envelope.command_id,
                    "session_id":token.map(desktop_host::lab_session_id)}),
                );
                self.commands.push(name.clone());
                if care {
                    self.care_commands.push(name);
                }
                self.report();
            }
        }
        self.session.expire(now);
        self.sequence += 1;
        if self.sequence.is_multiple_of(10) {
            let event = serde_json::json!({"kind":"debug_state","monotonic_seconds":self.start.elapsed().as_secs_f64(),"details":{
                "runtime_session_id":1,"sequence":self.sequence,"lab_session":{"active":self.session.active(now),"session_id":self.session.session_id},
                "lab_interventions":{"last_command_id":self.admission.last_seen_command_id},"care_control":self.menu.control_ack(),
                "hearing":{"master_gain":0.65,"enabled":true},"feeding":{"enabled":self.feeding_seconds>0.0},"cleanup":{"enabled":false}}});
            use std::io::Write;
            if let Ok(mut file) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&self.store.paths.telemetry)
            {
                let _ = writeln!(file, "{event}");
            }
        }
        event_loop.set_control_flow(ControlFlow::WaitUntil(self.next_frame));
    }
}
