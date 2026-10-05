# Native menu window
The current menu is an opaque decorated 460x570 winit window. There is no website shell, header, sidebar or router.
Source tools/body_lab/src/main.rs:595:630
```rust
impl ApplicationHandler for BodyLab {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.runtime.is_some() {
            return;
        }
        let window = match event_loop.create_window(
            Window::default_attributes()
                .with_window_icon(Some(desktop_host::application_icon()))
                .with_title(if self.pet_menu {
                    "Персик — забота и обучение"
                } else {
                    "Pet2 Dev Console"
                })
                .with_inner_size(if self.pet_menu {
                    LogicalSize::new(460.0, 570.0)
                } else {
                    LogicalSize::new(1_180.0, 880.0)
                })
                .with_min_inner_size(if self.pet_menu {
                    LogicalSize::new(420.0, 530.0)
                } else {
                    LogicalSize::new(880.0, 640.0)
                })
                .with_position(LogicalPosition::new(32.0, 48.0))
                .with_resizable(true),
        ) {
            Ok(window) => Arc::new(window),
            Err(error) => {
                eprintln!("could not create Pet2 Dev Console window: {error}");
                event_loop.exit();
                return;
            }
        };
        let mut body = match generate_lab_body(&self.genome) {
            Ok(body) => body,
            Err(error) => {
```
Render entry tools/body_lab/src/main.rs:1665:1695
```rust
fn render_main(runtime: &mut LabRuntime, genome: &Genome, event_loop: &ActiveEventLoop) {
    let diagnostics = runtime.body.embodiment.liquid.diagnostics();
    let raw_input = runtime.egui_state.take_egui_input(runtime.window.as_ref());
    let egui_context = runtime.egui_context.clone();
    let mut selected_panel = runtime.panel;
    let output = egui_context.run(raw_input, |context| {
        if let Some(menu) = &mut runtime.pet_menu {
            companion_menu::show(context, menu, runtime.live_monitor.as_mut());
            return;
        }
        dev_console_navigation(context, &mut selected_panel, runtime.live_monitor.as_ref());
        match selected_panel {
            DevPanel::Character => {
                runtime.ui.show(
                    context,
                    diagnostics,
                    runtime.body.embodiment.pose.pupil_size,
                    runtime.body.embodiment.pose.pupil_asymmetry,
                    runtime.fps,
                    runtime.p95_frame_ms,
                );
            }
            panel => {
                if let Some(monitor) = runtime.live_monitor.as_mut() {
                    show_live_panel(context, monitor, panel);
                } else {
                    show_live_pet_unavailable(context);
                }
            }
        }
    });
```
