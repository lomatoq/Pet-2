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
        !self.hidden && self.opened.is_some_and(|t| t.elapsed().as_secs_f32() > 0.7)
    }
    pub fn hide_ready(&mut self) -> bool {
        self.close
            && self
                .closing
                .get_or_insert_with(Instant::now)
                .elapsed()
                .as_secs_f32()
                >= 0.29
    }

    pub fn reopen(&mut self) -> bool {
        if !self.hidden {
            // A right click on the nest can first transfer native focus away.
            // Cancel that pending dismissal without resetting the visible UI.
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
        self.opened = Some(Instant::now());
        self.page = None;
        self.desired_page = None;
        self.panel_presence = 0.0;
        self.dismissal = 0.0;
        self.press = [0.0; 6];
        self.press_velocity = [0.0; 6];
        self.hover = [0.0; 6];
        self.action_started = None;
        self.action_error = false;
        self.capture_done = false;
        true
    }
}
const INK: Color32 = Color32::from_rgb(47, 42, 58);
const MUTED: Color32 = Color32::from_rgb(110, 103, 122);
const ACCENT: Color32 = Color32::from_rgb(105, 79, 141);
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
        0 => Color32::from_rgb(62, 122, 109),
        1 => Color32::from_rgb(102, 85, 160),
        2 => Color32::from_rgb(130, 99, 145),
        3 => Color32::from_rgb(62, 113, 157),
        5 => Color32::from_rgb(155, 90, 123),
        _ => Color32::from_rgb(89, 98, 125),
    }
}

impl MenuState {
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
    fonts.font_data.insert(
        "phosphor".into(),
        egui::FontData::from_static(include_bytes!("../assets/phosphor/Pet2-Phosphor.ttf")).into(),
    );
    fonts.families.insert(
        egui::FontFamily::Name("phosphor".into()),
        vec!["phosphor".into()],
    );
    ctx.set_fonts(fonts);
    ctx.tessellation_options_mut(|o| o.round_text_to_pixels = false);
    ctx.set_visuals(egui::Visuals::light());
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
        s.spacing.button_padding = vec2(12.0, 8.0);
        s.spacing.slider_width = 175.0;
        s.visuals.override_text_color = Some(INK);
        s.visuals.selection.bg_fill = ACCENT;
        s.visuals.selection.stroke = Stroke::new(1.0, Color32::WHITE);
        s.visuals.widgets.inactive.weak_bg_fill = Color32::from_rgb(239, 234, 245);
        s.visuals.widgets.inactive.bg_fill = Color32::from_rgb(243, 238, 248);
        s.visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, MUTED);
        s.visuals.widgets.hovered.bg_fill = Color32::from_rgb(234, 225, 246);
        s.visuals.widgets.hovered.fg_stroke = Stroke::new(1.0, ACCENT);
        s.visuals.widgets.active.bg_fill = Color32::from_rgb(234, 225, 246);
        s.visuals.widgets.active.fg_stroke = Stroke::new(1.0, INK);
        s.visuals.widgets.inactive.bg_stroke = Stroke::new(0.7, Color32::from_rgb(223, 215, 232));
        s.visuals.widgets.inactive.corner_radius = 10.into();
        s.visuals.widgets.hovered.corner_radius = 10.into();
        s.visuals.widgets.active.corner_radius = 10.into();
        s.visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(236, 230, 250);
        s.visuals.panel_fill = Color32::TRANSPARENT;
        s.visuals.window_fill = Color32::from_rgb(251, 249, 253);
        s.visuals.window_corner_radius = 16.into();
        s.text_styles
            .insert(egui::TextStyle::Body, egui::FontId::proportional(13.0));
        s.text_styles
            .insert(egui::TextStyle::Button, egui::FontId::proportional(13.0));
        s.text_styles
            .insert(egui::TextStyle::Small, egui::FontId::proportional(11.0));
    });
}
// Original Phosphor regular glyphs, embedded so they never depend on installed fonts.
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
    bubble_motion_at(screen, screen.center().x, index, elapsed, dismissal, reduced)
}
fn bubble_motion_at(screen: Rect, nest_x: f32, index: usize, elapsed: f32, dismissal: f32, reduced: bool) -> BubbleMotion {
    let layout = Rect::from_center_size(pos2(nest_x, screen.center().y), vec2(420.0_f32.min(screen.width()), screen.height()));
    let end = bubble_target(layout, index);
    if reduced {
        return BubbleMotion {
            center: end,
            alpha: 1.0 - dismissal,
            settled: 0.0,
        };
    }
    let duration = (0.52 + (screen.width() - 420.0).max(0.0) * 0.00015).min(1.0);
    let progress = ((elapsed - index as f32 * 0.028) / duration).clamp(0.0, 1.0);
    // Minimum-jerk timing: zero velocity and acceleration at both ends.
    let smooth = |t: f32| t * t * t * (10.0 + t * (-15.0 + 6.0 * t));
    let t = smooth(progress);
    // One continuous curved arrival from the right edge, no scale pop or bounce.
    let start = pos2(screen.max.x + 34.0, screen.max.y - 230.0);
    let a = pos2(screen.max.x - 70.0, screen.max.y - 300.0);
    let b = end + vec2(70.0, -82.0);
    let u = 1.0 - t;
    let mut center = pos2(
        u * u * u * start.x + 3.0 * u * u * t * a.x + 3.0 * u * t * t * b.x + t * t * t * end.x,
        u * u * u * start.y + 3.0 * u * u * t * a.y + 3.0 * u * t * t * b.y + t * t * t * end.y,
    );
    let exit = smooth(dismissal);
    center.y += 18.0 * exit;
    let fade = (progress / 0.15).clamp(0.0, 1.0);
    let fade = fade * fade * (3.0 - 2.0 * fade);
    BubbleMotion {
        center,
        alpha: fade * (1.0 - exit),
        settled: progress.powi(8),
    }
}

fn icon(p: &egui::Painter, c: egui::Pos2, index: usize, color: Color32) {
    let glyphs = [
        "\u{e6ca}", "\u{e62c}", "\u{e478}", "\u{e326}", "\u{e434}", "\u{e6a2}",
    ];
    let font = egui::FontId::new(26.0, egui::FontFamily::Name("phosphor".into()));
    // Small lower shadow seats the crisp symbol in the lens, without outlining it.
    p.text(
        c + vec2(0.0, 0.7),
        egui::Align2::CENTER_CENTER,
        glyphs[index],
        font.clone(),
        Color32::from_black_alpha((color.a() as f32 * 0.24) as u8),
    );
    p.text(c, egui::Align2::CENTER_CENTER, glyphs[index], font, color);
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
        state.opened.get_or_insert_with(Instant::now).elapsed().as_secs_f32()
    });
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        state.close = true;
    }
    let screen = ctx.content_rect();
    let texture = state.glass.as_ref().and_then(Glass::texture);
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
    let target_dismissal = if state.close { 1.0 } else { 0.0 };
    let dismissal_duration = if state.close { 0.21 } else { 0.24 };
    let delta = target_dismissal - state.dismissal;
    if state.capture_elapsed.is_none() {
        state.dismissal += delta.signum() * delta.abs().min(dt / dismissal_duration);
    }
    if state.dismissal > 0.99 {
        state.dismissal = 1.0;
    }
    if state.dismissal < 0.001 {
        state.dismissal = 0.0;
    }
    let nest_x = state.nest_x.unwrap_or(screen.center().x);
    let layout = Rect::from_center_size(pos2(nest_x, screen.center().y), vec2(420.0_f32.min(screen.width()), screen.height()));
    let base = pos2(nest_x, screen.max.y - 165.0);
    for (index, accessible_label) in LABELS.iter().enumerate() {
        let motion = if state.nest_x.is_some() { bubble_motion_at(screen, nest_x, index, elapsed, state.dismissal, reduced) } else { bubble_motion(screen, index, elapsed, state.dismissal, reduced) };
        let alpha = motion.alpha;
        let scale = 1.0 + 0.012 * state.hover[index] - state.press[index];
        let rect = Rect::from_center_size(motion.center, vec2(56.0, 56.0));
        let layer = egui::LayerId::new(
            egui::Order::Foreground,
            egui::Id::new(("care-bubble", index)),
        );
        let pivot = rect.center_bottom();
        let transform = egui::emath::TSTransform::from_translation(pivot.to_vec2())
            * egui::emath::TSTransform::from_scaling(scale)
            * egui::emath::TSTransform::from_translation(-pivot.to_vec2());
        ctx.set_transform_layer(layer, transform);
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
        // Preserve the material; floating never drives its optical phase.
        let phase = index as f32 * 1.43;
        let float_weight = if reduced {
            0.0
        } else {
            1.0 - state.hover[index]
        };
        let clock = elapsed * 0.72 + index as f32 * 1.43;
        let float =
            vec2((clock * 0.67).sin() * 1.15, clock.sin() * 2.6) * float_weight * motion.settled;
        let visual_rect = rect.translate(float);
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
                companion_glass::bubble_shadow(ui.painter(), visual_rect, alpha);
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
                    phase,
                    index,
                );
                let c = visual_rect.center();
                let ink = Color32::from_rgb(248, 249, 255)
                    .gamma_multiply(alpha * if ready { 1.0 } else { 0.80 });
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
                    icon(ui.painter(), c, index, ink);
                }
                if active {
                    ui.painter().circle_filled(c + vec2(0.0, 19.0), 1.75, ink);
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
        let target = if response.is_pointer_button_down_on() {
            0.035
        } else {
            0.0
        };
        if target > 0.0 {
            state.press[index] = target;
            state.press_velocity[index] = 0.0;
        } else {
            for _ in 0..4 {
                let h = dt / 4.0;
                state.press_velocity[index] += (400.0 * (target - state.press[index])
                    - 40.0 * state.press_velocity[index])
                    * h;
                state.press[index] += state.press_velocity[index] * h;
            }
        }
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
            state.regions.push((transform * rect, 28.0 * scale));
        }
        if response.clicked() && closing.is_none() {
            command = state.activate(index, latest, ready);
        }
    }
    if let Some(page) = state.page {
        let height = match page {
            1 => 346.0,
            2 => 268.0,
            3 => 282.0,
            _ => 210.0,
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
            if closing.is_some() || state.desired_page != Some(page) { ui.disable(); }ui.set_opacity(panel_alpha);ui.set_min_size(rect.size());companion_glass::surface(ui.painter(),rect,24,texture,screen,1.0,false);
            let mut inside=ui.new_child(egui::UiBuilder::new().max_rect(rect.shrink(20.0)).layout(egui::Layout::top_down(egui::Align::Min)));
            let ui=&mut inside;
            ui.horizontal(|ui|{ui.label(RichText::new(LABELS[page]).heading().color(INK));ui.with_layout(egui::Layout::right_to_left(egui::Align::Center),|ui|{if close_button(ui).on_hover_text("Close panel").clicked(){state.select_panel(None);}});});ui.add_space(4.0);

            ui.add_enabled_ui(ready,|ui|match page{
                1|2=>{
                    if page==1 {egui::ComboBox::from_id_salt("care-cue").width(ui.available_width()).selected_text(CUES[state.selected]).show_ui(ui,|ui|{for (i,label) in CUES.iter().enumerate().skip(1){ui.selectable_value(&mut state.selected,i,*label);}});}
                    else{ui.label(RichText::new("Bender / Benny").size(18.0).strong());ui.small("Teach him the sound of his name.");}
                    let index=if page==2{0}else{state.selected};let cue=CueKind::ALL[index];
                    let record=hearing["cues"].as_array().and_then(|a|a.get(index));let count=record.and_then(|r|r["examples"].as_u64()).unwrap_or(0);
                    let training=hearing["training"].is_object();let accepted=hearing["training"]["accepted"].as_u64().unwrap_or(0);
                    ui.small(format!("{count} / 40 examples · {}",if record.is_some_and(|r|r["ready"]==true){"ready to listen"}else{"learning together"}));
                    ui.horizontal(|ui|{let segment_width=(ui.available_width()-32.0)/5.0;for i in 0..5{let (r,_)=ui.allocate_exact_size(vec2(segment_width,5.0),Sense::hover());ui.painter().rect_filled(r,3,if training&&(i as u64)<accepted {ACCENT}else{Color32::from_rgb(218,205,237)});}});
                    ui.label(RichText::new(if training{ "Listening… leave a short pause between phrases." }else{"Say it five times, with a short pause. Existing examples stay saved."}).size(12.0).color(MUTED));
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
    let mut state = MenuState::new(false);
    state.opened = Some(Instant::now() - std::time::Duration::from_secs(2));
    state.capture_elapsed = Some(2.0);
    if ctx.content_rect().width() > 421.0 { state.nest_x = Some(210.0); }
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
        "pressed" => state.press[0] = 0.035,
        "panel-exit" => {
            state.desired_page = None;
            state.panel_presence = 0.55;
        }
        _ => {}
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
        state.dismissal = (t / 0.21).clamp(0.0, 1.0);
        state.closing = Some(Instant::now() - std::time::Duration::from_secs_f32(t));
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
            Color32::from_rgb(224, 213, 242)
        } else {
            Color32::from_rgb(236, 230, 250)
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
        egui::Button::new(RichText::new(label).color(Color32::WHITE))
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
    fn stale_telemetry_does_not_dispatch_or_claim_a_pending_action() {
        let ctx = egui::Context::default();
        configure(&ctx);
        let mut state = MenuState::new(false);
        state.opened = Some(Instant::now() - std::time::Duration::from_secs(1));
        assert!(click(&ctx, &mut state, false, pos2(50.0, 520.0)).is_none());
        assert!(!state.waiting_for_feed && !state.close);
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
