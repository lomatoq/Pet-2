use super::{
    LabControlCommand, LivePetMonitor,
    companion_glass::{self, Glass},
};
use desktop_host::{CueKind, HearingAction as H};
use egui::{Color32, Rect, RichText, Sense, Stroke, pos2, vec2};
use serde_json::Value;
use std::time::Instant;

#[derive(Default)]
pub(super) struct MenuState {
    pub created_unix_ms: u64,
    page: Option<usize>,
    desired_page: Option<usize>,
    panel_presence: f32,
    dismissal: f32,
    dismissal_from: f32,
    dismissal_target: f32,
    dismissal_elapsed: f32,
    selected: usize,
    pub waiting_for_feed: bool,
    waiting_for_cleanup: bool,
    pub close: bool,
    pub hidden: bool,
    pub capture_done: bool,
    pub pending_show: bool,
    pub nest_x: Option<f32>,
    opened: Option<Instant>,
    // Only isolated GPU fixtures supply a deterministic logical clock.
    capture_elapsed: Option<f32>,
    closing: Option<Instant>,
    last_hidden_unix_ms: Option<u64>,
    focus_dismissal_armed: bool,
    pub glass: Option<Glass>,
    pub regions: Vec<(Rect, f32)>,
    volume: Option<u8>,
    last_volume_edit: Option<Instant>,
    restore_volume: u8,
    press: [f32; 6],
    press_velocity: [f32; 6],
    hover: [f32; 6],
    action_started: Option<Instant>,
    action_error: bool,
}
impl MenuState {
    pub fn new(hidden: bool) -> Self {
        Self {
            hidden,
            pending_show: !hidden,
            created_unix_ms: super::unix_time_ms(),
            ..Self::default()
        }
    }

    pub fn can_dismiss_on_focus_loss(&self) -> bool {
        !self.hidden && self.focus_dismissal_armed
    }

    fn dismiss(&mut self) {
        if !self.hidden {
            self.close = true;
            self.pending_show = false;
        }
    }

    pub fn on_native_mouse_button(
        &mut self,
        button: winit::event::MouseButton,
        state: winit::event::ElementState,
    ) {
        // egui-winit drops button events until it has a pointer position (and
        // after CursorLeft). Cancellation needs neither a target nor a position.
        if button == winit::event::MouseButton::Right
            && state == winit::event::ElementState::Pressed
        {
            self.dismiss();
        }
    }

    fn observe_presented_focus(&mut self, focused: bool) {
        // pending_show is cleared only after a successful native present. The
        // first focused frame after that acknowledges startup; no time-based
        // grace period should swallow the user's first outside click.
        self.focus_dismissal_armed |= !self.hidden && !self.pending_show && focused;
    }
    pub fn hide_ready(&mut self) -> bool {
        // Hide only after the rendered tail has finished. A separate wall
        // clock can expire while a delayed frame is still visibly fading.
        let ready = self.close && self.dismissal >= 1.0 && self.panel_presence <= 0.012;
        if ready {
            self.last_hidden_unix_ms = Some(super::unix_time_ms());
        }
        ready
    }

    /// A native secondary press toggles the visible menu. Unlike an explicit
    /// reopen, it must keep a dismissal already initiated by native focus loss.
    pub fn consume_desktop_toggle(&mut self, channel: &desktop_host::CareMenuChannel) -> bool {
        if !channel.consume_open() {
            return false;
        }
        let requested_unix_ms = std::fs::read(channel.placement_path()).ok()
            .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
            .and_then(|placement| placement["secondary_press_unix_ms"].as_u64());
        self.toggle_from_desktop(requested_unix_ms)
    }

    fn toggle_from_desktop(&mut self, requested_unix_ms: Option<u64>) -> bool {
        if !self.hidden {
            self.dismiss();
            return false;
        }
        if requested_unix_ms.zip(self.last_hidden_unix_ms)
            .is_some_and(|(requested, hidden)| requested <= hidden)
        {
            return false;
        }
        self.reopen()
    }

    pub fn reopen(&mut self) -> bool {
        if !self.hidden {
            // Explicit reversal retains the current animation/panel. Native
            // secondary-click toggles use the separate dismissal path above.
            self.close = false;
            self.closing = None;
            return false;
        }
        self.hidden = false;
        self.pending_show = true;
        self.close = false;
        self.closing = None;
        self.waiting_for_feed = false;
        self.waiting_for_cleanup = false;
        self.focus_dismissal_armed = false;
        self.opened = Some(Instant::now());
        self.page = None;
        self.desired_page = None;
        self.panel_presence = 0.0;
        self.dismissal = 0.0;
        self.dismissal_from = 0.0;
        self.dismissal_target = 0.0;
        self.dismissal_elapsed = 0.0;
        self.press = [0.0; 6];
        self.press_velocity = [0.0; 6];
        self.hover = [0.0; 6];
        self.action_started = None;
        self.action_error = false;
        self.capture_done = false;
        true
    }
}
const INK: Color32 = Color32::from_rgb(236, 241, 246);
const SYMBOL: Color32 = Color32::from_rgb(39, 48, 61);
const MUTED: Color32 = Color32::from_rgb(164, 178, 191);
const ACCENT: Color32 = Color32::from_rgb(176, 226, 214);
const LABELS: [&str; 6] = ["Feed", "Teach", "Name", "Voice", "More", "Clean"];

fn action_label(index: usize, latest: &Value) -> &'static str {
    match index {
        0 if latest["details"]["feeding"]["enabled"].as_bool() == Some(true) => "Finish",
        5 if latest["details"]["cleanup"]["enabled"].as_bool() == Some(true) => "Finish",
        _ => LABELS[index],
    }
}

fn accent(index: usize) -> Color32 {
    match index {
        0 => Color32::from_rgb(107, 199, 196),
        1 => Color32::from_rgb(161, 137, 216),
        2 => Color32::from_rgb(185, 140, 208),
        3 => Color32::from_rgb(115, 175, 223),
        5 => Color32::from_rgb(211, 145, 184),
        _ => Color32::from_rgb(151, 163, 204),
    }
}

impl MenuState {
    fn update_dismissal(&mut self, dt: f32) {
        let target = if self.close { 1.0 } else { 0.0 };
        if self.dismissal_target != target {
            self.dismissal_from = self.dismissal;
            self.dismissal_target = target;
            self.dismissal_elapsed = 0.0;
        }
        self.dismissal_elapsed += dt.max(0.0);
        let progress = (self.dismissal_elapsed / MENU_FADE_SECONDS).clamp(0.0, 1.0);
        self.dismissal = self.dismissal_from
            + (target - self.dismissal_from) * menu_opacity_ease(progress);
        if progress >= 1.0 {
            self.dismissal = target;
        }
    }

    fn select_panel(&mut self, panel: Option<usize>) {
        self.desired_page = panel;
        if self.page.is_none() {
            self.page = panel;
            self.panel_presence = 0.0;
        }
    }

    fn update_panel(&mut self, dt: f32) {
        let opening = !self.close && self.page.is_some() && self.page == self.desired_page;
        let target = if opening { 1.0 } else { 0.0 };
        let rate = if opening { 19.0 } else { 25.0 };
        self.panel_presence += (target - self.panel_presence) * (1.0 - (-rate * dt).exp());
        if !opening && self.panel_presence < 0.012 {
            self.panel_presence = 0.0;
            self.page = if self.close { None } else { self.desired_page };
        }
    }

    fn activate(&mut self, index: usize, latest: &Value, ready: bool) -> Option<LabControlCommand> {
        if !ready || self.close || self.waiting_for_feed || self.waiting_for_cleanup {
            return None;
        }
        match index {
            0 => {
                let enabled = latest["details"]["feeding"]["enabled"].as_bool() != Some(true);
                self.waiting_for_feed = enabled;
                self.action_started = Some(Instant::now());
                self.action_error = false;
                self.close = !enabled;
                Some(LabControlCommand::Feeding { enabled })
            }
            5 => {
                let enabled = latest["details"]["cleanup"]["enabled"].as_bool() != Some(true);
                self.waiting_for_cleanup = enabled;
                self.action_started = Some(Instant::now());
                self.action_error = false;
                self.close = !enabled;
                Some(LabControlCommand::Cleanup { enabled })
            }
            _ => {
                self.select_panel(if self.desired_page == Some(index) {
                    None
                } else {
                    Some(index)
                });
                if index == 1 && self.selected == 0 {
                    self.selected = 2;
                }
                None
            }
        }
    }
}
const CUES: [&str; 21] = [
    "Name · Bender",
    "Quiet",
    "Sit",
    "Jump",
    "Circle",
    "Come here",
    "Stay",
    "Dash",
    "Up",
    "Down",
    "Left",
    "Right",
    "Sleep",
    "Wake up",
    "Blink",
    "Look",
    "Bow",
    "Shake",
    "Stretch",
    "Play",
    "Go home",
];
pub(super) fn configure(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    for path in [
        "C:/Windows/Fonts/segoeui.ttf",
        "/System/Library/Fonts/Supplemental/Arial.ttf",
    ] {
        if let Ok(bytes) = std::fs::read(path) {
            fonts
                .font_data
                .insert("companion".into(), egui::FontData::from_owned(bytes).into());
            fonts
                .families
                .entry(egui::FontFamily::Proportional)
                .or_default()
                .insert(0, "companion".into());
            break;
        }
    }
    if let Ok(bytes) = std::fs::read("C:/Windows/Fonts/seguisb.ttf") {
        fonts.font_data.insert(
            "companion-heading".into(),
            egui::FontData::from_owned(bytes).into(),
        );
        fonts.families.insert(
            egui::FontFamily::Name("companion-heading".into()),
            vec!["companion-heading".into(), "companion".into()],
        );
    }
    let heading_family = if fonts.font_data.contains_key("companion-heading") {
        egui::FontFamily::Name("companion-heading".into())
    } else {
        egui::FontFamily::Proportional
    };
    ctx.set_fonts(fonts);
    ctx.tessellation_options_mut(|o| o.round_text_to_pixels = false);
    ctx.set_visuals(egui::Visuals::dark());
    ctx.style_mut(|s| {
        s.animation_time = if companion_glass::reduced_motion() {
            0.0
        } else {
            0.15
        };
        s.text_styles.insert(
            egui::TextStyle::Heading,
            egui::FontId::new(17.0, heading_family),
        );
        s.spacing.item_spacing = vec2(8.0, 10.0);
        s.spacing.button_padding = vec2(14.0, 9.0);
        s.spacing.slider_width = 175.0;
        s.visuals.override_text_color = Some(INK);
        s.visuals.selection.bg_fill = ACCENT;
        s.visuals.selection.stroke = Stroke::new(1.0, SYMBOL);
        s.visuals.widgets.inactive.weak_bg_fill = Color32::from_rgb(38, 46, 56);
        s.visuals.widgets.inactive.bg_fill = Color32::from_rgb(36, 44, 54);
        s.visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, MUTED);
        s.visuals.widgets.hovered.bg_fill = Color32::from_rgb(53, 67, 77);
        s.visuals.widgets.hovered.fg_stroke = Stroke::new(1.0, ACCENT);
        s.visuals.widgets.active.bg_fill = Color32::from_rgb(53, 67, 77);
        s.visuals.widgets.active.fg_stroke = Stroke::new(1.0, INK);
        s.visuals.widgets.inactive.bg_stroke = Stroke::new(0.7, Color32::from_rgb(66, 77, 88));
        s.visuals.widgets.inactive.corner_radius = 12.into();
        s.visuals.widgets.hovered.corner_radius = 12.into();
        s.visuals.widgets.active.corner_radius = 12.into();
        s.visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(44, 55, 65);
        s.visuals.panel_fill = Color32::TRANSPARENT;
        s.visuals.window_fill = Color32::from_rgb(28, 35, 44);
        s.visuals.window_corner_radius = 16.into();
        s.text_styles
            .insert(egui::TextStyle::Body, egui::FontId::proportional(14.0));
        s.text_styles
            .insert(egui::TextStyle::Button, egui::FontId::proportional(14.0));
        s.text_styles
            .insert(egui::TextStyle::Small, egui::FontId::proportional(12.0));
    });
}
struct BubbleMotion {
    center: egui::Pos2,
    alpha: f32,
    settled: f32,
}

fn bubble_target(screen: Rect, index: usize) -> egui::Pos2 {
    // A shallow semicircle above the nest; outer controls curve downward.
    let angle = (152.0 - index as f32 * 24.8).to_radians();
    let radius_x = ((screen.width() - 54.0) * 0.5).min(184.0);
    pos2(
        screen.center().x + radius_x * angle.cos(),
        screen.max.y + 20.0 - 138.0 * angle.sin(),
    )
}

fn bubble_motion(
    screen: Rect,
    index: usize,
    elapsed: f32,
    dismissal: f32,
    reduced: bool,
) -> BubbleMotion {
    bubble_motion_at(
        screen,
        screen.center().x,
        index,
        elapsed,
        dismissal,
        reduced,
    )
}
const MENU_FADE_SECONDS: f32 = 0.25;
const REVEAL_SECONDS: f32 = 0.20;

// Emil Kowalski's strong UI ease-out: cubic-bezier(0.23, 1, 0.32, 1).
// Solve its time coordinate rather than treating the Bezier parameter as time.
// This keeps most of the visible travel in a long, gently decelerating tail.
fn menu_ease_out(time: f32) -> f32 {
    menu_bezier(time, [0.23, 1.0, 0.32, 1.0])
}

// Opacity needs a gentler onset than travel: CSS `ease` starts softly, then
// spends the remainder settling. Strong ease-out here would flash on frame one.
fn menu_opacity_ease(time: f32) -> f32 {
    menu_bezier(time, [0.25, 0.1, 0.25, 1.0])
}

fn menu_bezier(time: f32, control: [f32; 4]) -> f32 {
    if time <= 0.0 {
        return 0.0;
    }
    if time >= 1.0 {
        return 1.0;
    }
    let (mut low, mut high) = (0.0, 1.0);
    for _ in 0..16 {
        let t = (low + high) * 0.5;
        let u = 1.0 - t;
        let x = 3.0 * u * u * t * control[0] + 3.0 * u * t * t * control[2] + t * t * t;
        if x < time {
            low = t;
        } else {
            high = t;
        }
    }
    let t = (low + high) * 0.5;
    let u = 1.0 - t;
    3.0 * u * u * t * control[1] + 3.0 * u * t * t * control[3] + t * t * t
}

fn bubble_motion_at(
    screen: Rect,
    nest_x: f32,
    index: usize,
    elapsed: f32,
    dismissal: f32,
    reduced: bool,
) -> BubbleMotion {
    let layout = Rect::from_center_size(
        pos2(nest_x, screen.center().y),
        vec2(420.0_f32.min(screen.width()), screen.height()),
    );
    let end = bubble_target(layout, index);
    let age = (elapsed - index as f32 * 0.028).max(0.0);
    let reveal = menu_opacity_ease(age / REVEAL_SECONDS);
    if reduced {
        return BubbleMotion {
            center: end,
            alpha: reveal * (1.0 - dismissal),
            settled: 0.0,
        };
    }
    // The existing full edge-to-nest arc is drawer-length travel. Retain its
    // distance allowance; a short popover duration would throw it across wide monitors.
    let duration = (0.50 + (screen.width() - 420.0).max(0.0) * 0.00015).min(1.0);
    let progress = (age / duration).clamp(0.0, 1.0);
    let t = menu_ease_out(progress);
    // One continuous curved arrival from the right edge, no scale pop or bounce.
    let start = pos2(screen.max.x + 34.0, screen.max.y - 230.0);
    let a = pos2(screen.max.x - 70.0, screen.max.y - 300.0);
    let b = end + vec2(70.0, -82.0);
    let u = 1.0 - t;
    let mut center = pos2(
        u * u * u * start.x + 3.0 * u * u * t * a.x + 3.0 * u * t * t * b.x + t * t * t * end.x,
        u * u * u * start.y + 3.0 * u * u * t * a.y + 3.0 * u * t * t * b.y + t * t * t * end.y,
    );
    center.y += 12.0 * dismissal;
    BubbleMotion {
        center,
        alpha: reveal * (1.0 - dismissal),
        settled: t,
    }
}

// Phosphor Fill, MIT. Original SVGs and prefiltered native 1x/2x coverage masks live
// beside this source. A single cached texture per symbol uses linear sampling:
// no small font atlas, bespoke strokes, duplicated shadow, or per-frame upload.
fn icon(p: &egui::Painter, c: egui::Pos2, index: usize, color: Color32, scale: f32) {
    let density = if p.ctx().pixels_per_point() > 1.25 {
        2
    } else {
        1
    };
    let key = egui::Id::new(("care-phosphor-fill-v67", density));
    let icons = if let Some(icons) = p
        .ctx()
        .data_mut(|data| data.get_temp::<[egui::TextureHandle; 6]>(key))
    {
        icons
    } else {
        let coverage: [&[u8]; 6] = if density == 1 {
            [
                include_bytes!("care_icons/bowl-food-1x.alpha"),
                include_bytes!("care_icons/graduation-cap-1x.alpha"),
                include_bytes!("care_icons/identification-badge-1x.alpha"),
                include_bytes!("care_icons/microphone-1x.alpha"),
                include_bytes!("care_icons/sliders-horizontal-1x.alpha"),
                include_bytes!("care_icons/sparkle-1x.alpha"),
            ]
        } else {
            [
                include_bytes!("care_icons/bowl-food-2x.alpha"),
                include_bytes!("care_icons/graduation-cap-2x.alpha"),
                include_bytes!("care_icons/identification-badge-2x.alpha"),
                include_bytes!("care_icons/microphone-2x.alpha"),
                include_bytes!("care_icons/sliders-horizontal-2x.alpha"),
                include_bytes!("care_icons/sparkle-2x.alpha"),
            ]
        };
        let icons = std::array::from_fn(|i| {
            let pixels = coverage[i]
                .iter()
                .map(|&a| Color32::from_white_alpha(a))
                .collect();
            p.ctx().load_texture(
                format!("care-phosphor-{i}"),
                egui::ColorImage::new([28 * density, 28 * density], pixels),
                egui::TextureOptions::LINEAR,
            )
        });
        p.ctx()
            .data_mut(|data| data.insert_temp(key, icons.clone()));
        icons
    };
    // The badge's clip and graduation tassel need different optical centers.
    let offset = match index {
        1 => vec2(0.0, 0.1),
        2 => vec2(0.0, -0.2),
        _ => vec2(0.0, 0.0),
    };
    p.image(
        icons[index].id(),
        Rect::from_center_size(c + offset * scale, vec2(28.0, 28.0) * scale),
        Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
        color,
    );
}

// Shared physical response for the live controls and deterministic capture.
// Both edges retarget the existing value; rapid release/repress never restarts.
fn advance_press(value: &mut f32, velocity: &mut f32, held: bool, dt: f32, reduced: bool) {
    let target = if held { 0.04 } else { 0.0 };
    if reduced {
        *value = 0.0;
        *velocity = 0.0;
        return;
    }
    let rate = if held { 32.0 } else { 26.0 };
    let previous = *value;
    *value += (target - *value) * (1.0 - (-rate * dt).exp());
    *velocity = (*value - previous) / dt.max(0.001);
}

fn panel_surface(p: &egui::Painter, rect: Rect) {
    p.add(
        egui::epaint::RectShape::filled(
            rect.translate(vec2(0.0, 8.0)),
            24,
            Color32::from_black_alpha(42),
        )
        .with_blur_width(26.0),
    );
    p.rect_filled(rect, 24, Color32::from_rgb(27, 34, 43));
    p.rect_stroke(
        rect,
        24,
        Stroke::new(0.7, Color32::from_white_alpha(28)),
        egui::StrokeKind::Inside,
    );
    // One large quiet header field, clipped inside the panel, carries the same
    // mint energy as the buttons without a second white shell or chrome edge.
    let clip = p.with_clip_rect(rect.shrink(1.0));
    clip.add(
        egui::epaint::RectShape::filled(
            Rect::from_min_size(rect.min + vec2(22.0, 14.0), vec2(130.0, 48.0)),
            24,
            Color32::from_rgba_unmultiplied(120, 210, 192, 12),
        )
        .with_blur_width(34.0),
    );
}
pub(super) fn show(
    ctx: &egui::Context,
    state: &mut MenuState,
    monitor: Option<&mut LivePetMonitor>,
) {
    let latest = monitor
        .as_ref()
        .and_then(|m| m.selected_frame())
        .cloned()
        .unwrap_or(Value::Null);
    let ready = monitor.as_ref().is_some_and(|m| m.can_send_control());
    let command = show_frame(ctx, state, &latest, ready);
    if let (Some(monitor), Some(command)) = (monitor, command) {
        monitor.send_control(command);
    }
}

// Shared by the native window, interaction checks, and isolated GPU fixtures.
fn show_frame(
    ctx: &egui::Context,
    state: &mut MenuState,
    latest: &Value,
    ready: bool,
) -> Option<LabControlCommand> {
    let elapsed = state.capture_elapsed.unwrap_or_else(|| {
        state
            .opened
            .get_or_insert_with(Instant::now)
            .elapsed()
            .as_secs_f32()
    });
    state.observe_presented_focus(ctx.input(|i| i.focused));
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)
        || i.pointer.button_pressed(egui::PointerButton::Secondary)) {
        state.dismiss();
    }
    let screen = ctx.content_rect();
    let hearing = &latest["details"]["hearing"];
    if state.waiting_for_feed && latest["details"]["feeding"]["enabled"].as_bool() == Some(true) {
        state.close = true;
    }
    if state.waiting_for_cleanup && latest["details"]["cleanup"]["enabled"].as_bool() == Some(true)
    {
        state.close = true;
    }
    if !state.close
        && (state.waiting_for_feed || state.waiting_for_cleanup)
        && state
            .action_started
            .is_some_and(|t| t.elapsed().as_secs_f32() > 6.0)
    {
        state.waiting_for_feed = false;
        state.waiting_for_cleanup = false;
        state.action_error = true;
    }
    if state.close {
        state.closing.get_or_insert_with(Instant::now);
    }
    let closing = state.closing.map(|t| t.elapsed().as_secs_f32());
    let mut command = None;
    state.regions.clear();
    let dt = ctx.input(|i| i.stable_dt).clamp(0.001, 0.05);
    let reduced =
        !ctx.style().animation_time.is_sign_positive() || ctx.style().animation_time == 0.0;
    state.update_panel(dt);
    if state.capture_elapsed.is_none() {
        state.update_dismissal(dt);
    }
    let nest_x = state.nest_x.unwrap_or(screen.center().x);
    let layout = Rect::from_center_size(
        pos2(nest_x, screen.center().y),
        vec2(420.0_f32.min(screen.width()), screen.height()),
    );
    let base = pos2(nest_x, screen.max.y - 165.0);
    for (index, accessible_label) in LABELS.iter().enumerate() {
        let motion = if state.nest_x.is_some() {
            bubble_motion_at(screen, nest_x, index, elapsed, state.dismissal, reduced)
        } else {
            bubble_motion(screen, index, elapsed, state.dismissal, reduced)
        };
        let alpha = motion.alpha;
        let scale = if reduced {
            1.0
        } else {
            1.0 - state.press[index]
        };
        let rect = Rect::from_center_size(motion.center, vec2(56.0, 56.0));
        let layer = egui::LayerId::new(
            egui::Order::Foreground,
            egui::Id::new(("care-bubble", index)),
        );
        // Keep the complete 56px hit area fixed while only the artwork depresses.
        // The former bottom pivot made a press slide rather than sink inward.
        ctx.set_transform_layer(layer, egui::emath::TSTransform::IDENTITY);
        let waiting = state.waiting_for_feed || state.waiting_for_cleanup;
        let active = state.desired_page == Some(index);
        let label = if (index == 0 && state.waiting_for_feed)
            || (index == 5 && state.waiting_for_cleanup)
        {
            "Starting…"
        } else {
            action_label(index, latest)
        };
        let tint = accent(index);
        let float_weight = if reduced {
            0.0
        } else {
            1.0 - state.hover[index]
        };
        let clock = elapsed * 0.72 + index as f32 * 1.43;
        let float =
            vec2((clock * 0.67).sin() * 1.15, clock.sin() * 2.6) * float_weight * motion.settled;
        let visual_rect = Rect::from_center_size(rect.center() + float, rect.size() * scale);
        let response = egui::Area::new(layer.id)
            .order(egui::Order::Foreground)
            .constrain(false)
            .fixed_pos(rect.min)
            .show(ctx, |ui| {
                if closing.is_some() || !ready || waiting {
                    ui.disable();
                }
                ui.set_opacity(1.0);
                let (_, r) = ui.allocate_exact_size(rect.size(), Sense::click());
                let finishing = label == "Finish";
                let this_waiting = (index == 0 && state.waiting_for_feed)
                    || (index == 5 && state.waiting_for_cleanup);
                ui.painter().add(
                    egui::epaint::RectShape::filled(
                        visual_rect
                            .shrink(3.0)
                            .translate(vec2(0.0, 3.0 * (1.0 - state.press[index] / 0.04))),
                        28,
                        Color32::from_black_alpha((15.0 * alpha) as u8),
                    )
                    .with_blur_width(9.0),
                );
                super::bubble_material::paint(
                    ui.painter(),
                    visual_rect,
                    tint,
                    alpha,
                    if active || finishing {
                        1.0
                    } else {
                        state.hover[index] * 0.55
                    },
                    state.press[index] / 0.04,
                    index,
                );
                let c = visual_rect.center();
                let ink = SYMBOL.gamma_multiply(alpha * if ready { 1.0 } else { 0.80 });
                if finishing {
                    ui.painter()
                        .rect_filled(Rect::from_center_size(c, vec2(13.0, 13.0)), 3, ink);
                } else if this_waiting {
                    for dot in 0..3 {
                        ui.painter().circle_filled(
                            c + vec2((dot as f32 - 1.0) * 5.0, 0.0),
                            1.5,
                            ink,
                        );
                    }
                } else {
                    icon(ui.painter(), c, index, ink, scale);
                }
                if r.has_focus() {
                    ui.painter().rect_stroke(
                        rect.shrink(2.5),
                        25,
                        Stroke::new(1.5, tint.gamma_multiply(alpha)),
                        egui::StrokeKind::Inside,
                    );
                }
                r.widget_info(|| {
                    egui::WidgetInfo::labeled(
                        egui::WidgetType::Button,
                        ready && !waiting,
                        *accessible_label,
                    )
                });
                r.on_hover_text(format!(
                    "{label}{}",
                    if !ready {
                        " · Reconnecting. This action will be available shortly."
                    } else if state.action_error {
                        " · Didn't start. Please try again."
                    } else {
                        match index {
                            0 if label != "Finish" => " · Sprinkle crumbs. Esc to finish.",
                            1 => " · Teach a familiar word",
                            2 => " · Practice the sound of his name",
                            3 => " · Voice and microphone",
                            5 if label != "Finish" => " · Clean traces. Esc to finish.",
                            0 | 5 => " · End this action",
                            _ => " · Birth replay and other actions",
                        }
                    }
                ))
            })
            .inner;
        let held = response.is_pointer_button_down_on();
        let target = if held { 0.04 } else { 0.0 };
        advance_press(
            &mut state.press[index],
            &mut state.press_velocity[index],
            held,
            dt,
            reduced,
        );
        let hover_target = if response.hovered() || response.has_focus() {
            1.0
        } else {
            0.0
        };
        state.hover[index] += (hover_target - state.hover[index]) * (1.0 - (-22.0 * dt).exp());
        if (target - state.press[index]).abs() > 0.0001
            || state.press_velocity[index].abs() > 0.001
            || (hover_target - state.hover[index]).abs() > 0.002
        {
            ctx.request_repaint();
        }
        if alpha > 0.01 {
            state.regions.push((rect, 28.0));
        }
        if response.clicked() && closing.is_none() {
            command = state.activate(index, latest, ready);
        }
    }
    if let Some(page) = state.page {
        let height = match page {
            1 => 374.0,
            2 => 294.0,
            3 => 304.0,
            _ => 256.0,
        };
        let rect = Rect::from_min_size(
            pos2(nest_x - 160.0, base.y - height - 18.0),
            vec2(320.0, height),
        );
        let panel_alpha = state.panel_presence;
        let id = egui::Id::new("care-panel");
        let layer = egui::LayerId::new(egui::Order::Foreground, id);
        let pivot = pos2(bubble_target(layout, page).x, rect.max.y);
        let transform = egui::emath::TSTransform::from_translation(
            pivot.to_vec2()
                + vec2(
                    0.0,
                    if reduced {
                        0.0
                    } else {
                        5.0 * (1.0 - panel_alpha)
                    },
                ),
        ) * egui::emath::TSTransform::from_scaling(if reduced {
            1.0
        } else {
            0.97 + 0.03 * panel_alpha
        }) * egui::emath::TSTransform::from_translation(-pivot.to_vec2());
        ctx.set_transform_layer(layer, transform);
        if panel_alpha > 0.01 {
            state.regions.push((transform * rect, 24.0));
        }
        egui::Area::new(id).order(egui::Order::Foreground).fixed_pos(rect.min).show(ctx,|ui|{
            if closing.is_some() || state.desired_page != Some(page) { ui.disable(); }ui.set_opacity(panel_alpha);ui.set_min_size(rect.size());panel_surface(ui.painter(),rect);
            let mut inside=ui.new_child(egui::UiBuilder::new().max_rect(rect.shrink(20.0)).layout(egui::Layout::top_down(egui::Align::Min)));
            let ui=&mut inside;
            ui.horizontal(|ui|{ui.label(RichText::new(match page {1=>"Learning together",2=>"A name to remember",3=>"Voice & listening",_=>"A little more"}).heading().color(INK));ui.with_layout(egui::Layout::right_to_left(egui::Align::Center),|ui|{if close_button(ui).on_hover_text("Close panel").clicked(){state.select_panel(None);}});});
            ui.label(RichText::new(match page {1=>"A familiar sound. A shared little ritual.",2=>"Your voice makes it familiar.",3=>"Keep it comfortable for both of you.",_=>"Small moments, whenever you like."}).size(13.0).color(MUTED));
            ui.add_space(8.0);

            ui.add_enabled_ui(ready,|ui|match page{
                1|2=>{
                    if page==1 {egui::ComboBox::from_id_salt("care-cue").width(ui.available_width()).selected_text(CUES[state.selected]).show_ui(ui,|ui|{for (i,label) in CUES.iter().enumerate().skip(1){ui.selectable_value(&mut state.selected,i,*label);}});}
                    else{ui.label(RichText::new("Bender / Benny").size(18.0).strong());ui.small("Teach him the sound of his name.");}
                    let index=if page==2{0}else{state.selected};let cue=CueKind::ALL[index];
                    let record=hearing["cues"].as_array().and_then(|a|a.get(index));let count=record.and_then(|r|r["examples"].as_u64()).unwrap_or(0);
                    let training=hearing["training"].is_object();let accepted=hearing["training"]["accepted"].as_u64().unwrap_or(0);
                    ui.small(format!("{count} / 40 examples · {}",if record.is_some_and(|r|r["ready"]==true){"ready to listen"}else{"learning together"}));
                    ui.horizontal(|ui|{let segment_width=(ui.available_width()-32.0)/5.0;for i in 0..5{let (r,_)=ui.allocate_exact_size(vec2(segment_width,5.0),Sense::hover());ui.painter().rect_filled(r,3,if training&&(i as u64)<accepted {ACCENT}else{Color32::from_rgb(61,77,86)});}});
                    ui.label(RichText::new(if training{ "Listening… leave a short pause between phrases." }else{"Say it five times, with a short pause. Existing examples stay saved."}).size(13.0).color(MUTED));
                    if primary(ui,if training{"Stop recording"}else{"Record 5 examples"}){command=Some(LabControlCommand::Hearing{action:if training{H::CancelTraining}else{H::TrainCommand{cue}}});}
                    if page==1 {ui.horizontal(|ui|{if ui.button("Show me").clicked(){command=Some(LabControlCommand::Hearing{action:H::Perform{cue}});}
if ui.button("Try my voice").clicked(){command=Some(LabControlCommand::Hearing{action:H::Test});}});if ui.small_button("Record other words").on_hover_text("Helps distinguish a command from everyday speech").clicked(){command=Some(LabControlCommand::Hearing{action:H::TrainOther});}}
                },
                3=>{let remote=(hearing["master_gain"].as_f64().unwrap_or(1.0)*100.0).round() as u8;if state.last_volume_edit.is_none_or(|t|t.elapsed().as_secs_f32()>1.0){state.volume=Some(remote);}
                    let mut volume=state.volume.unwrap_or(remote);ui.horizontal(|ui|{ui.label("Voice volume");ui.with_layout(egui::Layout::right_to_left(egui::Align::Center),|ui|{ui.label(RichText::new(format!("{volume}%")).color(MUTED));});});
                    ui.horizontal(|ui|{if ui.small_button(if volume==0{"Unmute"}else{"Mute"}).clicked(){if volume>0{state.restore_volume=volume;volume=0;}else{volume=state.restore_volume.max(30);}}ui.add(egui::Slider::new(&mut volume,0..=100).show_value(false));});
                    if Some(volume)!=state.volume{state.volume=Some(volume);state.last_volume_edit=Some(Instant::now());command=Some(LabControlCommand::Hearing{action:H::SetVolume{percent:volume}});}
                    ui.add_space(5.0);let mut enabled=hearing["enabled"].as_bool().unwrap_or(false);if ui.checkbox(&mut enabled,"Listen to my voice").changed(){command=Some(LabControlCommand::Hearing{action:if enabled{H::Enable}else{H::Disable}});}
                    egui::ComboBox::from_id_salt("care-input").width(ui.available_width()).selected_text(hearing["input_device"].as_str().unwrap_or("System microphone")).show_ui(ui,|ui|{if ui.selectable_label(hearing["input_device"].is_null(),"System microphone").clicked(){command=Some(LabControlCommand::Hearing{action:H::SelectInput{index:0}});}
if let Some(devices)=hearing["input_devices"].as_array(){for(i,d)in devices.iter().enumerate(){if let Some(name)=d.as_str()&& ui.selectable_label(hearing["input_device"].as_str()==Some(name),name).clicked(){command=Some(LabControlCommand::Hearing{action:H::SelectInput{index:i as u16+1}});}}}});
                    let rms=hearing["input_level"].as_f64().unwrap_or(0.0) as f32;ui.add(egui::ProgressBar::new((rms.sqrt()*2.2).clamp(0.0,1.0)).desired_height(4.0).fill(ACCENT));ui.small("Learning stays on this computer.");},
                _=>{if primary(ui,"Replay birth"){command=Some(LabControlCommand::ReplayBirth);state.close=true;}ui.label(RichText::new("His age and memories stay the same.").small().color(MUTED));if ui.button("Dash to my cursor").clicked(){command=Some(LabControlCommand::Hearing{action:H::Perform{cue:CueKind::Dash}});state.close=true;}
if ui.button("Close menu").clicked(){state.close=true;}}
            });
        });
    }
    if !reduced && closing.is_none() {
        ctx.request_repaint_after(std::time::Duration::from_micros(16_667));
    }
    // Include actual dropdown/tool-tip areas in the native hit region.
    let layers = ctx.memory(|m| m.areas().visible_layer_ids());
    for layer in layers {
        if layer.order == egui::Order::Foreground || layer.order == egui::Order::Tooltip {
            if layer.id == egui::Id::new("care-panel")
                || (0..6).any(|i| layer.id == egui::Id::new(("care-bubble", i)))
            {
                continue;
            }
            if let Some(area) = egui::AreaState::load(ctx, layer.id) {
                state.regions.push((area.rect(), 10.0));
            }
        }
    }
    if closing.is_some()
        || state.dismissal > 0.001
        || elapsed < 0.70
        || (state.page.is_some() && state.panel_presence < 0.999)
        || state.page != state.desired_page
    {
        ctx.request_repaint();
    }
    command
}

pub(super) fn capture_frame(ctx: &egui::Context, fixture: &str) {
    if fixture.starts_with("material-grid-") {
        let painter = ctx.layer_painter(egui::LayerId::background());
        let row = match fixture {
            "material-grid-light" => 0,
            "material-grid-dark" => 1,
            _ => 2,
        };
        {
            let top = 20.0;
            let bg = Rect::from_min_size(pos2(0.0, top), vec2(420.0, 160.0));
            painter.rect_filled(
                bg,
                0,
                if row == 0 {
                    Color32::from_rgb(246, 243, 249)
                } else {
                    Color32::from_rgb(31, 29, 39)
                },
            );
            if row == 2 {
                for y in 0..8 {
                    for x in 0..21 {
                        if (x + y) % 2 == 0 {
                            painter.rect_filled(
                                Rect::from_min_size(
                                    pos2(x as f32 * 20.0, top + y as f32 * 20.0),
                                    vec2(20.0, 20.0),
                                ),
                                0,
                                Color32::from_rgb(111, 112, 127),
                            );
                        }
                    }
                }
            }
            painter.text(
                pos2(12.0, top + 12.0),
                egui::Align2::LEFT_TOP,
                "idle        hover       active       pressed      fade 50%",
                egui::FontId::proportional(11.0),
                if row == 0 {
                    Color32::from_rgb(64, 60, 78)
                } else {
                    Color32::from_rgb(225, 222, 239)
                },
            );
            for slot in 0..5 {
                let rect = Rect::from_center_size(
                    pos2(45.0 + slot as f32 * 79.0, top + 80.0),
                    vec2(56.0, 56.0) * if slot == 3 { 0.96 } else { 1.0 },
                );
                let alpha = if slot == 4 { 0.5 } else { 1.0 };
                let tint = accent(if slot == 2 { 1 } else { 0 });
                super::bubble_material::paint(
                    &painter,
                    rect,
                    tint,
                    alpha,
                    if slot == 1 || slot == 3 {
                        0.72
                    } else if slot == 2 {
                        1.0
                    } else {
                        0.0
                    },
                    if slot == 3 { 1.0 } else { 0.0 },
                    slot,
                );
                icon(
                    &painter,
                    rect.center(),
                    if slot == 2 { 1 } else { 0 },
                    SYMBOL.gamma_multiply(alpha),
                    if slot == 3 { 0.96 } else { 1.0 },
                );
            }
        }
        return;
    }

    let mut state = MenuState::new(false);
    state.opened = Some(Instant::now() - std::time::Duration::from_secs(2));
    state.capture_elapsed = Some(2.0);
    if ctx.content_rect().width() > 421.0 {
        state.nest_x = Some(210.0);
    }
    state.page = match fixture {
        "learn" => Some(1),
        "name" => Some(2),
        "voice" | "panel-exit" => Some(3),
        "settings" => Some(4),
        _ => None,
    };
    state.desired_page = state.page;
    state.panel_presence = 1.0;
    state.selected = 2;
    let mut frame = serde_json::json!({"details":{"hearing":{"master_gain":0.65,"enabled":true}}});
    let ready = fixture != "offline";
    match fixture {
        "feeding" => frame["details"]["feeding"]["enabled"] = true.into(),
        "pending" => {
            state.waiting_for_feed = true;
            state.action_started = Some(Instant::now());
        }
        "failure" => state.action_error = true,
        "hover" => state.hover[0] = 1.0,
        "pressed" => {
            state.press[0] = 0.04;
            state.hover[0] = 1.0;
        }
        "panel-exit" => {
            state.desired_page = None;
            state.panel_presence = 0.55;
        }
        _ => {}
    }
    if let Some(t) = fixture
        .strip_prefix("press-")
        .and_then(|s| s.parse::<f32>().ok())
    {
        // Hold for 180ms then release; use the exact live response helper.
        let mut elapsed = 0.0_f32;
        while elapsed < t {
            let dt = (t - elapsed).min(1.0 / 120.0);
            advance_press(
                &mut state.press[0],
                &mut state.press_velocity[0],
                elapsed < 0.18,
                dt,
                false,
            );
            elapsed += dt;
        }
        state.hover[0] = 1.0;
    }
    if let Some(t) = fixture
        .strip_prefix("float-")
        .or_else(|| fixture.strip_prefix("drift-"))
        .and_then(|s| s.parse::<f32>().ok())
    {
        state.capture_elapsed = Some(2.0 + t);
    }
    if let Some(t) = fixture
        .strip_prefix("open-")
        .and_then(|s| s.parse::<f32>().ok())
    {
        state.capture_elapsed = Some(t);
    }
    if let Some(t) = fixture
        .strip_prefix("close-")
        .and_then(|s| s.parse::<f32>().ok())
    {
        state.close = true;
        state.update_dismissal(t);
        state.closing = Some(Instant::now() - std::time::Duration::from_secs_f32(t));
    }
    if let Some(t) = fixture.strip_prefix("retoggle-").and_then(|s| s.parse::<f32>().ok()) {
        // Close during arrival, then reopen before the exit finishes. Replay
        // the live transition; don't substitute a separately authored curve.
        let mut elapsed = 0.0_f32;
        while elapsed < t {
            let dt = (t - elapsed).min(1.0 / 120.0);
            state.close = (0.15..0.225).contains(&elapsed);
            state.update_dismissal(dt);
            elapsed += dt;
        }
        state.capture_elapsed = Some(t);
    }
    let _ = show_frame(ctx, &mut state, &frame, ready);
}
fn close_button(ui: &mut egui::Ui) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(vec2(24.0, 24.0), Sense::click());
    let center = rect.center();
    let radius = if response.is_pointer_button_down_on() {
        10.5
    } else {
        12.0
    };
    ui.painter().circle_filled(
        center,
        radius,
        if response.hovered() {
            Color32::from_rgb(65, 80, 91)
        } else {
            Color32::from_rgb(44, 55, 65)
        },
    );
    for sign in [-1.0, 1.0] {
        ui.painter().line_segment(
            [
                center + vec2(-3.0, -3.0 * sign),
                center + vec2(3.0, 3.0 * sign),
            ],
            Stroke::new(1.25, MUTED),
        );
    }
    response
        .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, "Close panel"));
    response
}
fn primary(ui: &mut egui::Ui, label: &str) -> bool {
    ui.add_sized(
        [ui.available_width(), 36.0],
        egui::Button::new(RichText::new(label).color(SYMBOL).strong())
            .fill(ACCENT)
            .corner_radius(12),
    )
    .clicked()
}
#[cfg(test)]
mod tests {
    use super::*;

    fn ui_tick(
        ctx: &egui::Context,
        state: &mut MenuState,
        ready: bool,
        events: Vec<egui::Event>,
    ) -> Option<LabControlCommand> {
        let mut command = None;
        let frame = serde_json::json!({"details":{"feeding":{"enabled":false},"cleanup":{"enabled":false}}});
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(420.0, 580.0))),
                events,
                ..Default::default()
            },
            |ctx| {
                command = show_frame(ctx, state, &frame, ready);
            },
        );
        command
    }

    fn click(
        ctx: &egui::Context,
        state: &mut MenuState,
        ready: bool,
        position: egui::Pos2,
    ) -> Option<LabControlCommand> {
        for _ in 0..3 {
            let _ = ui_tick(ctx, state, ready, vec![]);
        }
        let press = |pressed| egui::Event::PointerButton {
            pos: position,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        let _ = ui_tick(
            ctx,
            state,
            ready,
            vec![egui::Event::PointerMoved(position), press(true)],
        );
        ui_tick(ctx, state, ready, vec![press(false)])
    }

    #[test]
    fn native_feed_and_cleanup_clicks_send_one_command_without_a_panel() {
        for (index, position) in [(0, pos2(50.0, 520.0)), (5, pos2(370.0, 520.0))] {
            let ctx = egui::Context::default();
            configure(&ctx);
            let mut state = MenuState::new(false);
            state.opened = Some(Instant::now() - std::time::Duration::from_secs(1));
            let command =
                click(&ctx, &mut state, true, position).expect("one click must deliver the action");
            assert!(matches!(
                (index, command),
                (0, LabControlCommand::Feeding { enabled: true })
                    | (5, LabControlCommand::Cleanup { enabled: true })
            ));
            assert_eq!(state.page, None);
            assert!(
                click(&ctx, &mut state, true, position).is_none(),
                "pending action must not dispatch twice"
            );
        }
    }

    #[test]
    fn held_control_keeps_full_hit_area_and_release_dispatches_once() {
        let ctx = egui::Context::default();
        configure(&ctx);
        let mut state = MenuState::new(false);
        state.capture_elapsed = Some(2.0);
        for _ in 0..3 {
            let _ = ui_tick(&ctx, &mut state, true, vec![]);
        }
        let initial = state.regions[0];
        let position = initial.0.center() + vec2(25.5, 0.0);
        let pointer = |pressed| egui::Event::PointerButton {
            pos: position,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        let _ = ui_tick(
            &ctx,
            &mut state,
            true,
            vec![egui::Event::PointerMoved(position), pointer(true)],
        );
        for _ in 0..12 {
            let _ = ui_tick(&ctx, &mut state, true, vec![]);
        }
        assert!(
            state.press[0] > 0.03,
            "the live response must reach a visible depression while held"
        );
        assert_eq!(
            state.regions[0], initial,
            "visual depression must not shrink or translate the hit target"
        );
        assert!(matches!(
            ui_tick(&ctx, &mut state, true, vec![pointer(false)]),
            Some(LabControlCommand::Feeding { enabled: true })
        ));
        assert!(ui_tick(&ctx, &mut state, true, vec![]).is_none());
    }

    #[test]
    fn stale_telemetry_does_not_dispatch_or_claim_a_pending_action() {
        let ctx = egui::Context::default();
        configure(&ctx);
        let mut state = MenuState::new(false);
        state.opened = Some(Instant::now() - std::time::Duration::from_secs(1));
        assert!(click(&ctx, &mut state, false, pos2(50.0, 520.0)).is_none());
        assert!(!state.waiting_for_feed && !state.close);
    }

    #[test]
    fn native_secondary_without_a_pointer_position_dismisses_without_an_action() {
        use winit::event::{ElementState, MouseButton};
        let ctx = egui::Context::default();
        configure(&ctx);
        let mut state = MenuState::new(false);
        state.opened = Some(Instant::now() - std::time::Duration::from_secs(2));
        assert!(ui_tick(&ctx, &mut state, true, vec![]).is_none());
        assert!(ctx.input(|i| i.pointer.latest_pos()).is_none());
        for (button, phase) in [
            (MouseButton::Left, ElementState::Pressed),
            (MouseButton::Middle, ElementState::Pressed),
            (MouseButton::Right, ElementState::Released),
        ] {
            state.on_native_mouse_button(button, phase);
            assert!(!state.close);
        }
        // Same production path as WindowEvent::MouseInput; no egui pointer
        // event, coordinate, focus acknowledgement, or action is required.
        state.on_native_mouse_button(MouseButton::Right, ElementState::Pressed);
        assert!(state.close && !state.pending_show);
        assert!(!state.hide_ready(), "native cancellation retains the fade");
        for _ in 0..90 {
            assert!(ui_tick(&ctx, &mut state, false, vec![]).is_none());
        }
        assert!(state.hide_ready());
        assert!(!state.waiting_for_feed && !state.waiting_for_cleanup);
        state.hidden = true;
        state.close = false;
        state.on_native_mouse_button(MouseButton::Right, ElementState::Pressed);
        assert!(!state.close && state.hidden, "late hidden input must not reopen or rearm");
    }

    #[test]
    fn secondary_press_closes_bubbles_and_panels_without_dispatching_an_action() {
        for reduced in [false, true] {
            for page in [None, Some(3)] {
                let ctx = egui::Context::default();
                configure(&ctx);
                ctx.style_mut(|style| style.animation_time = if reduced { 0.0 } else { 0.2 });
                let mut state = MenuState::new(false);
                state.opened = Some(Instant::now() - std::time::Duration::from_secs(2));
                state.select_panel(page);
                for _ in 0..3 {
                    assert!(ui_tick(&ctx, &mut state, true, vec![]).is_none());
                }
                let position = state.regions[0].0.center();
                let pointer = |button, pressed| egui::Event::PointerButton {
                    pos: position, button, pressed, modifiers: Default::default(),
                };
                // Cancel even an already armed primary action. Secondary must
                // take priority over its release in the same native input batch.
                assert!(ui_tick(&ctx, &mut state, true, vec![
                    egui::Event::PointerMoved(position),
                    pointer(egui::PointerButton::Primary, true),
                ]).is_none());
                assert!(ui_tick(&ctx, &mut state, true, vec![
                    pointer(egui::PointerButton::Secondary, true),
                    pointer(egui::PointerButton::Primary, false),
                ]).is_none());
                assert!(state.close);
                assert!(!state.waiting_for_feed && !state.waiting_for_cleanup);
                assert!(!state.hide_ready(), "secondary press must retain the exit fade");
                assert!(ui_tick(&ctx, &mut state, false,
                    vec![pointer(egui::PointerButton::Secondary, false)]).is_none());
                for _ in 0..90 {
                    assert!(ui_tick(&ctx, &mut state, false, vec![]).is_none());
                }
                assert!(state.hide_ready(), "closing must finish, including reduced motion");
            }
        }
    }

    #[test]
    fn desktop_toggle_keeps_focus_loss_dismissal_instead_of_reopening() {
        let data = tempfile::tempdir().unwrap();
        let channel = desktop_host::CareMenuChannel::for_owner(data.path(), "toggle-test").unwrap();
        let mut state = MenuState::new(true);
        channel.request_open().unwrap(); // Legacy placement without a timestamp still works.
        assert!(state.consume_desktop_toggle(&channel));
        assert!(!state.hidden);
        assert!(!state.consume_desktop_toggle(&channel));
        state.pending_show = false;
        state.observe_presented_focus(true);
        assert!(state.can_dismiss_on_focus_loss());
        state.close = true; // Native focus transfer arrives before the IPC request.
        state.update_dismissal(0.08);
        let leaving = state.dismissal;
        channel.request_open().unwrap();
        assert!(!state.consume_desktop_toggle(&channel));
        assert!(state.close && !state.hidden && !state.pending_show);
        assert_eq!(state.dismissal, leaving, "do not restart or reverse the fade");
        state.update_dismissal(0.05);
        assert!(state.dismissal > leaving);
    }

    #[test]
    fn delayed_secondary_request_cannot_reopen_a_completed_focus_loss_fade() {
        let data = tempfile::tempdir().unwrap();
        let channel = desktop_host::CareMenuChannel::for_owner(data.path(), "delayed-test").unwrap();
        let mut state = MenuState::new(false);
        let requested = super::super::unix_time_ms();
        channel.write_placement(&serde_json::json!({"x":42,
            "secondary_press_unix_ms":requested})).unwrap();
        channel.request_open().unwrap();
        // Helper scheduling may delay consumption until focus loss has already
        // completed its fade and the native window has become hidden.
        state.close = true;
        state.update_dismissal(MENU_FADE_SECONDS);
        assert!(state.hide_ready());
        state.hidden = true;
        state.close = false;
        assert!(!state.consume_desktop_toggle(&channel));
        assert!(state.hidden && !state.close);
        // A genuinely new press after dismissal still opens normally.
        channel.write_placement(&serde_json::json!({"x":42,
            "secondary_press_unix_ms":state.last_hidden_unix_ms.unwrap()+1})).unwrap();
        channel.request_open().unwrap();
        assert!(state.consume_desktop_toggle(&channel));
        assert!(!state.hidden && !state.close && state.pending_show);
    }

    #[test]
    fn focus_loss_arms_after_presented_focus_without_a_time_grace_period() {
        let mut state = MenuState::new(true);
        state.observe_presented_focus(false);
        assert!(!state.can_dismiss_on_focus_loss());
        assert!(state.reopen());
        // Initial Focused(false), or early focus before the first presented
        // frame, must not dismiss an opening window.
        state.observe_presented_focus(false);
        assert!(!state.can_dismiss_on_focus_loss());
        state.observe_presented_focus(true);
        assert!(!state.can_dismiss_on_focus_loss());
        state.pending_show = false; // Production clears this only on Presented.
        state.observe_presented_focus(false);
        assert!(!state.can_dismiss_on_focus_loss());
        state.observe_presented_focus(true);
        assert!(state.can_dismiss_on_focus_loss(), "first outside click is now valid");
        state.observe_presented_focus(false);
        assert!(state.can_dismiss_on_focus_loss());
        state.close = true;
        state.update_dismissal(MENU_FADE_SECONDS);
        assert!(state.hide_ready());
        state.hidden = true;
        state.close = false;
        let hidden_at = state.last_hidden_unix_ms;
        for _ in 0..10 {
            assert!(!state.hide_ready());
        }
        assert_eq!(state.last_hidden_unix_ms, hidden_at, "idle must not extend the timestamp guard");
        assert!(state.reopen());
        assert!(!state.can_dismiss_on_focus_loss(), "new opening requires its own presented focus");
    }

    #[test]
    fn panel_retains_current_progress_when_closing_is_reversed() {
        let mut state = MenuState::new(false);
        state.select_panel(Some(3));
        state.update_panel(0.08);
        state.select_panel(None);
        state.update_panel(0.016);
        let leaving = state.panel_presence;
        assert_eq!(state.page, Some(3));
        state.select_panel(Some(3));
        assert_eq!(
            state.panel_presence, leaving,
            "retoggle must not restart the entrance"
        );
        state.update_panel(0.016);
        assert!(state.panel_presence > leaving);
        state.select_panel(Some(1));
        state.update_panel(0.2);
        assert_eq!(state.page, Some(1));
        assert_eq!(state.panel_presence, 0.0);
    }

    #[test]
    fn opening_visible_menu_preserves_panel_and_animation() {
        let mut menu = super::MenuState::new(true);
        assert!(menu.reopen());
        menu.page = Some(3);
        menu.desired_page = Some(3);
        menu.panel_presence = 0.8;
        let opened = menu.opened;
        menu.close = true;
        menu.dismissal = 0.55;
        menu.closing = Some(std::time::Instant::now());
        assert!(!menu.reopen());
        assert_eq!(menu.page, Some(3));
        assert_eq!(menu.opened, opened);
        assert_eq!(menu.desired_page, Some(3));
        assert_eq!(menu.panel_presence, 0.8);
        assert_eq!(
            menu.dismissal, 0.55,
            "reopening must preserve current exit opacity"
        );
        assert!(!menu.close && menu.closing.is_none());
    }
    #[test]
    fn dismissal_waits_for_rendered_tail_and_retargets_current_value() {
        let mut menu = MenuState::new(false);
        menu.close = true;
        menu.closing = Some(Instant::now() - std::time::Duration::from_secs(2));
        assert!(!menu.hide_ready(), "wall time must not cut off an unrendered fade");
        menu.update_dismissal(0.08);
        let leaving = menu.dismissal;
        assert!(leaving > 0.0 && leaving < 1.0);
        assert!(!menu.reopen());
        menu.update_dismissal(0.0);
        assert_eq!(menu.dismissal, leaving, "retarget must not jump opacity");
        menu.update_dismissal(0.05);
        assert!(menu.dismissal < leaving);
        menu.close = true;
        menu.update_dismissal(MENU_FADE_SECONDS);
        assert!(menu.hide_ready());
    }
    #[test]
    fn curved_arrival_has_stable_endpoints_and_separated_controls() {
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(420.0, 580.0));
        for i in 0..6 {
            let first = super::bubble_motion(screen, i, 0.0, 0.0, false);
            assert!(first.center.x > screen.max.x);
            assert_eq!(first.alpha, 0.0);
            let end = super::bubble_motion(screen, i, 0.70, 0.0, false);
            assert!(end.center.distance(super::bubble_target(screen, i)) < 0.001);
            assert_eq!(end.alpha, 1.0);
            let near_end = super::bubble_motion(screen, i, 0.519 + i as f32 * 0.028, 0.0, false);
            assert!(near_end.center.distance(end.center) < 0.002);
            if i > 0 {
                assert!(end.center.distance(super::bubble_target(screen, i - 1)) > 56.0);
            }
            let reduced = super::bubble_motion(screen, i, 0.0, 0.0, true);
            assert_eq!(reduced.center, end.center);
        }
    }
    #[test]
    fn interior_nest_arrival_starts_at_monitor_edge_and_finishes_above_nest() {
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 580.0));
        let layout = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(420.0, 580.0));
        for index in 0..6 {
            let start = super::bubble_motion_at(screen, 210.0, index, 0.0, 0.0, false);
            let end = super::bubble_motion_at(screen, 210.0, index, 1.2, 0.0, false);
            assert_eq!(start.center.x, 1314.0);
            assert!(end.center.distance(super::bubble_target(layout, index)) < 0.001);
        }
    }
}
