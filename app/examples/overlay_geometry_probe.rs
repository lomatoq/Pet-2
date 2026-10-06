//! A bounded, hidden native window fixture. Never loads or changes a saved pet.
#[path = "../src/overlay_geometry.rs"]
mod overlay_geometry;
use desktop_host::{PhysicalDesktopPoint, RectI};
use std::time::{Duration, Instant};
use winit::{
    application::ApplicationHandler,
    dpi::{PhysicalPosition, PhysicalSize},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    window::Window,
};

struct Probe {
    window: Option<Window>,
    bounds: RectI,
    started: Instant,
    stage: usize,
    injected: bool,
    rows: Vec<serde_json::Value>,
    passed: bool,
}
impl ApplicationHandler for Probe {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let monitors: Vec<_> = event_loop.available_monitors().collect();
        assert!(!monitors.is_empty(), "native display unavailable");
        self.bounds = RectI {
            minimum: PhysicalDesktopPoint {
                x: monitors.iter().map(|m| m.position().x).min().unwrap(),
                y: monitors.iter().map(|m| m.position().y).min().unwrap(),
            },
            maximum: PhysicalDesktopPoint {
                x: monitors
                    .iter()
                    .map(|m| m.position().x + m.size().width as i32)
                    .max()
                    .unwrap(),
                y: monitors
                    .iter()
                    .map(|m| m.position().y + m.size().height as i32)
                    .max()
                    .unwrap(),
            },
        };
        self.window = Some(
            event_loop
                .create_window(
                    Window::default_attributes()
                        .with_title("Pet2 Geometry Probe")
                        .with_visible(false)
                        .with_decorations(false)
                        .with_position(PhysicalPosition::new(
                            self.bounds.minimum.x,
                            self.bounds.minimum.y,
                        ))
                        .with_inner_size(PhysicalSize::new(
                            self.bounds.width() as u32,
                            self.bounds.height() as u32,
                        )),
                )
                .expect("owned geometry fixture"),
        );
    }
    fn window_event(
        &mut self,
        _: &ActiveEventLoop,
        _: winit::window::WindowId,
        _: winit::event::WindowEvent,
    ) {
    }
    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if self.started.elapsed() > Duration::from_secs(10) {
            event_loop.exit();
            return;
        }
        let window = self.window.as_ref().unwrap();
        if !self.injected {
            match self.stage {
                0 => window.set_outer_position(PhysicalPosition::new(
                    self.bounds.minimum.x + 113,
                    self.bounds.minimum.y + 71,
                )),
                1 => {
                    let _ = window.request_inner_size(PhysicalSize::new(640, 480));
                }
                2 => {
                    let _ = window
                        .request_inner_size(PhysicalSize::new(self.bounds.width() as u32, 240));
                }
                _ => {
                    self.passed = true;
                    event_loop.exit();
                    return;
                }
            }
            self.injected = true;
            self.rows.push(serde_json::json!({"stage": self.stage, "mismatch_detected": !overlay_geometry::window_matches(window, self.bounds)}));
        } else if overlay_geometry::window_matches(window, self.bounds) {
            self.rows[self.stage]["restored"] = true.into();
            self.stage += 1;
            self.injected = false;
        } else {
            self.rows[self.stage]["mismatch_detected"] = true.into();
            overlay_geometry::request_restore(window, self.bounds);
        }
        event_loop.set_control_flow(ControlFlow::WaitUntil(
            Instant::now() + Duration::from_millis(50),
        ));
    }
}
fn main() {
    let mut probe = Probe {
        window: None,
        bounds: RectI::default(),
        started: Instant::now(),
        stage: 0,
        injected: false,
        rows: vec![],
        passed: false,
    };
    EventLoop::new().unwrap().run_app(&mut probe).unwrap();
    let passed = probe.passed
        && probe.rows.len() == 3
        && probe
            .rows
            .iter()
            .all(|r| r["mismatch_detected"] == true && r["restored"] == true);
    println!(
        "{}",
        serde_json::json!({"passed":passed,"cases":probe.rows,"scope":"owned hidden native window; saved pet untouched"})
    );
    if !passed {
        std::process::exit(1);
    }
}
