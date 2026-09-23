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
    selected: usize,
    pub waiting_for_feed: bool,
    pub close: bool,
    pub hidden: bool,
    pub capture_done: bool,
    opened: Option<Instant>,
    closing: Option<Instant>,
    page_opened: Option<Instant>,
    pub glass: Option<Glass>,
    pub regions: Vec<(Rect, f32)>,
    volume: Option<u8>,
    last_volume_edit: Option<Instant>,
    restore_volume: u8,
    press: [f32; 5],
    press_velocity: [f32; 5],
}
impl MenuState {
    pub fn new(hidden: bool) -> Self {
        Self {
            hidden,
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
                >= 0.66
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
        self.close = false;
        self.closing = None;
        self.waiting_for_feed = false;
        self.opened = Some(Instant::now());
        self.page = None;
        self.capture_done = false;
        true
    }
}
const INK: Color32 = Color32::from_rgb(48, 43, 66);
const MUTED: Color32 = Color32::from_rgb(113, 107, 133);
const ACCENT: Color32 = Color32::from_rgb(114, 82, 165);
const LABELS: [&str; 5] = ["Food", "Learn", "Name", "Voice", "Settings"];
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
    ctx.set_fonts(fonts);
    ctx.set_visuals(egui::Visuals::light());
    ctx.style_mut(|s| {
        s.spacing.item_spacing = vec2(8.0, 10.0);
        s.spacing.button_padding = vec2(12.0, 8.0);
        s.spacing.slider_width = 175.0;
        s.visuals.override_text_color = Some(INK);
        s.visuals.selection.bg_fill = ACCENT;
        s.visuals.selection.stroke = Stroke::new(1.0, Color32::WHITE);
        s.visuals.widgets.inactive.weak_bg_fill = Color32::from_rgb(234, 227, 249);
        s.visuals.widgets.inactive.corner_radius = 10.into();
        s.visuals.widgets.hovered.corner_radius = 10.into();
        s.visuals.widgets.active.corner_radius = 10.into();
        s.visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(236, 230, 250);
        s.visuals.panel_fill = Color32::TRANSPARENT;
        s.visuals.window_fill = Color32::from_rgb(247, 244, 255);
        s.visuals.window_corner_radius = 16.into();
        s.text_styles
            .insert(egui::TextStyle::Body, egui::FontId::proportional(13.0));
        s.text_styles
            .insert(egui::TextStyle::Button, egui::FontId::proportional(13.0));
        s.text_styles
            .insert(egui::TextStyle::Small, egui::FontId::proportional(11.0));
    });
}
fn ease(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}
fn icon(p: &egui::Painter, c: egui::Pos2, index: usize, color: Color32) {
    let st = Stroke::new(1.55, color);
    let line = |a: [f32; 2], b: [f32; 2]| {
        p.line_segment([c + vec2(a[0], a[1]), c + vec2(b[0], b[1])], st);
    };
    match index {
        0 => {
            p.circle_stroke(c, 7.0, st);
            for a in [[-3.0, -3.0], [3.0, 0.0], [-1.0, 4.0]] {
                p.circle_filled(c + vec2(a[0], a[1]), 1.2, color);
            }
        }
        1 => {
            p.add(egui::Shape::closed_line(
                vec![
                    c + vec2(-9.0, -2.0),
                    c + vec2(0.0, -7.0),
                    c + vec2(9.0, -2.0),
                    c + vec2(0.0, 3.0),
                ],
                st,
            ));
            line([-5.0, 2.0], [-5.0, 6.0]);
            line([-5.0, 6.0], [5.0, 6.0]);
            line([5.0, 6.0], [5.0, 2.0]);
        }
        2 => {
            p.add(egui::Shape::closed_line(
                vec![
                    c + vec2(-7.0, -6.0),
                    c + vec2(1.0, -6.0),
                    c + vec2(8.0, 1.0),
                    c + vec2(0.0, 8.0),
                    c + vec2(-7.0, 1.0),
                ],
                st,
            ));
            p.circle_filled(c + vec2(-3.0, -2.0), 1.5, color);
        }
        3 => {
            p.rect_stroke(
                Rect::from_center_size(c + vec2(0.0, -2.0), vec2(7.0, 12.0)),
                4,
                st,
                egui::StrokeKind::Middle,
            );
            line([-7.0, -1.0], [-7.0, 3.0]);
            line([-7.0, 3.0], [0.0, 7.0]);
            line([0.0, 7.0], [7.0, 3.0]);
            line([7.0, 3.0], [7.0, -1.0]);
            line([0.0, 7.0], [0.0, 10.0]);
        }
        _ => {
            p.circle_stroke(c, 5.0, st);
            p.circle_stroke(c, 1.8, st);
            for i in 0..8 {
                let a = i as f32 * std::f32::consts::TAU / 8.0;
                line(
                    [a.cos() * 6.0, a.sin() * 6.0],
                    [a.cos() * 8.0, a.sin() * 8.0],
                );
            }
        }
    }
}
pub(super) fn show(
    ctx: &egui::Context,
    state: &mut MenuState,
    monitor: Option<&mut LivePetMonitor>,
) {
    let elapsed = state
        .opened
        .get_or_insert_with(Instant::now)
        .elapsed()
        .as_secs_f32();
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        state.close = true;
    }
    let screen = ctx.content_rect();
    let texture = state.glass.as_ref().and_then(Glass::texture);
    let latest = monitor
        .as_ref()
        .and_then(|m| m.selected_frame())
        .cloned()
        .unwrap_or(Value::Null);
    let ready = monitor.as_ref().is_some_and(|m| m.can_send_control());
    let hearing = &latest["details"]["hearing"];
    if state.waiting_for_feed && latest["details"]["feeding"]["enabled"].as_bool() == Some(true) {
        state.close = true;
    }
    if state.close {
        state.closing.get_or_insert_with(Instant::now);
    }
    let closing = state.closing.map(|t| t.elapsed().as_secs_f32());
    let mut command = None;
    state.regions.clear();
    let base = pos2((screen.width() - 320.0) * 0.5, screen.height() - 134.0);
    let specs = [
        (vec2(20.0, 0.0), 94.0),
        (vec2(130.0, 0.0), 110.0),
        (vec2(0.0, 62.0), 102.0),
        (vec2(118.0, 62.0), 106.0),
        (vec2(240.0, 62.0), 48.0),
    ];
    for (index, (offset, width)) in specs.iter().enumerate() {
        let phase = ((elapsed - [0.16, 0.20, 0.0, 0.04, 0.08][index]) / 0.42).clamp(0.0, 1.0);
        let leaving = closing.map_or(0.0, |t| {
            ease((t - [0.06, 0.10, 0.16, 0.20, 0.24][index]) / 0.38)
        });
        let alpha = ease(phase) * (1.0 - leaving);
        let scale =
            (0.84 + 0.16 * ease(phase)) * (1.0 - 0.14 * leaving) * (1.0 - state.press[index]);
        let rect = Rect::from_min_size(
            base + *offset + vec2(0.0, 10.0 * (1.0 - phase)),
            vec2(*width, 48.0),
        );
        let layer = egui::LayerId::new(
            egui::Order::Foreground,
            egui::Id::new(("care-bubble", index)),
        );
        let pivot = rect.center_bottom();
        ctx.set_transform_layer(
            layer,
            egui::emath::TSTransform::from_translation(pivot.to_vec2())
                * egui::emath::TSTransform::from_scaling(scale)
                * egui::emath::TSTransform::from_translation(-pivot.to_vec2()),
        );
        let response = egui::Area::new(layer.id)
            .order(egui::Order::Foreground)
            .fixed_pos(rect.min)
            .show(ctx, |ui| {
                if closing.is_some() {
                    ui.disable();
                    ui.set_opacity(1.0);
                }
                let (_, r) = ui.allocate_exact_size(rect.size(), Sense::click());
                companion_glass::surface(
                    ui.painter(),
                    rect,
                    24,
                    texture,
                    screen,
                    alpha,
                    state.page == Some(index),
                );
                if r.hovered() {
                    ui.painter().rect_filled(
                        rect.shrink(1.0),
                        24,
                        Color32::from_white_alpha((35.0 * alpha) as u8),
                    );
                }
                let center = rect.min + vec2(24.0, 24.0);
                if index < 4 {
                    ui.painter().circle_filled(
                        center,
                        15.0,
                        Color32::from_white_alpha((145.0 * alpha) as u8),
                    );
                }
                icon(ui.painter(), center, index, ACCENT.gamma_multiply(alpha));
                if index < 4 {
                    ui.painter().text(
                        rect.min + vec2(46.0, 24.0),
                        egui::Align2::LEFT_CENTER,
                        LABELS[index],
                        egui::FontId::proportional(13.0),
                        INK.gamma_multiply(alpha),
                    );
                }
                r.widget_info(|| {
                    egui::WidgetInfo::labeled(egui::WidgetType::Button, true, LABELS[index])
                });
                r
            })
            .inner;
        let target = if response.is_pointer_button_down_on() {
            0.065
        } else {
            0.0
        };
        let dt = ctx.input(|i| i.stable_dt).clamp(0.001, 0.033);
        // Substeps keep the lightly damped return stable across missed frames.
        for _ in 0..4 {
            let h = dt / 4.0;
            state.press_velocity[index] +=
                (310.0 * (target - state.press[index]) - 27.0 * state.press_velocity[index]) * h;
            state.press[index] += state.press_velocity[index] * h;
        }
        if (target - state.press[index]).abs() > 0.0001 || state.press_velocity[index].abs() > 0.001
        {
            ctx.request_repaint();
        }
        state.regions.push((rect, 24.0));
        if response.clicked() && closing.is_none() {
            state.page = if state.page == Some(index) {
                None
            } else {
                Some(index)
            };
            if index == 1 && state.selected == 0 {
                state.selected = 2;
            }
            state.page_opened = Some(Instant::now());
        }
    }
    if let Some(page) = state.page {
        let height = match page {
            0 => 232.0,
            1 => 346.0,
            2 => 268.0,
            3 => 282.0,
            _ => 210.0,
        };
        let rect = Rect::from_min_size(
            pos2((screen.width() - 300.0) * 0.5, base.y - height - 18.0),
            vec2(300.0, height),
        );
        state.regions.push((rect, 24.0));
        let phase = (state
            .page_opened
            .get_or_insert_with(Instant::now)
            .elapsed()
            .as_secs_f32()
            / 0.22)
            .min(1.0);
        let panel_alpha = ease(phase) * (1.0 - closing.map_or(0.0, |t| ease(t / 0.32)));
        let id = egui::Id::new("care-panel");
        let layer = egui::LayerId::new(egui::Order::Foreground, id);
        let pivot = rect.center_bottom();
        ctx.set_transform_layer(
            layer,
            egui::emath::TSTransform::from_translation(pivot.to_vec2())
                * egui::emath::TSTransform::from_scaling(0.92 + 0.08 * panel_alpha)
                * egui::emath::TSTransform::from_translation(-pivot.to_vec2()),
        );
        egui::Area::new(id).order(egui::Order::Foreground).fixed_pos(rect.min).show(ctx,|ui|{
            if closing.is_some() { ui.disable(); }ui.set_opacity(panel_alpha);ui.set_min_size(rect.size());companion_glass::surface(ui.painter(),rect,24,texture,screen,1.0,false);
            let mut inside=ui.new_child(egui::UiBuilder::new().max_rect(rect.shrink(20.0)).layout(egui::Layout::top_down(egui::Align::Min)));
            let ui=&mut inside;
            ui.horizontal(|ui|{ui.label(RichText::new(LABELS[page]).size(16.0).strong());ui.with_layout(egui::Layout::right_to_left(egui::Align::Center),|ui|{if close_button(ui).on_hover_text("Close panel").clicked(){state.page=None;}});});ui.add_space(4.0);
            if !ready{ui.label(RichText::new("Connecting to Bender…").small().color(MUTED));}
            ui.add_enabled_ui(ready,|ui|match page{
                0=>{ui.label(RichText::new("Glowing crumbs").strong());ui.label(RichText::new("A little treat, right from your cursor.").color(MUTED));ui.add_space(5.0);if primary(ui,"Feed Bender"){command=Some(LabControlCommand::Feeding{enabled:true});state.waiting_for_feed=true;}
if ui.button("Stop feeding & clear crumbs").clicked(){command=Some(LabControlCommand::Feeding{enabled:false});}ui.small("Press Esc or right-click to stop.");},
                1|2=>{
                    if page==1 {egui::ComboBox::from_id_salt("care-cue").width(245.0).selected_text(CUES[state.selected]).show_ui(ui,|ui|{for (i,label) in CUES.iter().enumerate().skip(1){ui.selectable_value(&mut state.selected,i,*label);}});}
                    else{ui.label(RichText::new("Bender / Benny").size(18.0).strong());ui.small("Teach him the sound of his name.");}
                    let index=if page==2{0}else{state.selected};let cue=CueKind::ALL[index];
                    let record=hearing["cues"].as_array().and_then(|a|a.get(index));let count=record.and_then(|r|r["examples"].as_u64()).unwrap_or(0);
                    let training=hearing["training"].is_object();let accepted=hearing["training"]["accepted"].as_u64().unwrap_or(0);
                    ui.small(format!("{count} / 40 examples · {}",if record.is_some_and(|r|r["ready"]==true){"ready to listen"}else{"learning together"}));
                    ui.horizontal(|ui|{for i in 0..5{let (r,_)=ui.allocate_exact_size(vec2(44.0,5.0),Sense::hover());ui.painter().rect_filled(r,3,if training&&(i as u64)<accepted {ACCENT}else{Color32::from_rgb(218,205,237)});}});
                    ui.label(RichText::new(if training{ "Listening… leave a short pause between phrases." }else{"Say it five times, with a short pause. Existing examples stay saved."}).size(12.0).color(MUTED));
                    if primary(ui,if training{"Stop recording"}else{"Record 5 examples"}){command=Some(LabControlCommand::Hearing{action:if training{H::CancelTraining}else{H::TrainCommand{cue}}});}
                    if page==1 {ui.horizontal(|ui|{if ui.button("Show me").clicked(){command=Some(LabControlCommand::Hearing{action:H::Perform{cue}});}
if ui.button("Try my voice").clicked(){command=Some(LabControlCommand::Hearing{action:H::Test});}});if ui.small_button("Record other words").on_hover_text("Helps distinguish a command from everyday speech").clicked(){command=Some(LabControlCommand::Hearing{action:H::TrainOther});}}
                },
                3=>{let remote=(hearing["master_gain"].as_f64().unwrap_or(1.0)*100.0).round() as u8;if state.last_volume_edit.is_none_or(|t|t.elapsed().as_secs_f32()>1.0){state.volume=Some(remote);}
                    let mut volume=state.volume.unwrap_or(remote);ui.horizontal(|ui|{ui.label("Voice volume");ui.label(RichText::new(format!("{volume}%")).color(MUTED));});
                    ui.horizontal(|ui|{if ui.small_button(if volume==0{"Unmute"}else{"Mute"}).clicked(){if volume>0{state.restore_volume=volume;volume=0;}else{volume=state.restore_volume.max(30);}}ui.add(egui::Slider::new(&mut volume,0..=100).show_value(false));});
                    if Some(volume)!=state.volume{state.volume=Some(volume);state.last_volume_edit=Some(Instant::now());command=Some(LabControlCommand::Hearing{action:H::SetVolume{percent:volume}});}
                    ui.add_space(5.0);let mut enabled=hearing["enabled"].as_bool().unwrap_or(false);if ui.checkbox(&mut enabled,"Listen to my voice").changed(){command=Some(LabControlCommand::Hearing{action:if enabled{H::Enable}else{H::Disable}});}
                    egui::ComboBox::from_id_salt("care-input").width(245.0).selected_text(hearing["input_device"].as_str().unwrap_or("System microphone")).show_ui(ui,|ui|{if ui.selectable_label(hearing["input_device"].is_null(),"System microphone").clicked(){command=Some(LabControlCommand::Hearing{action:H::SelectInput{index:0}});}
if let Some(devices)=hearing["input_devices"].as_array(){for(i,d)in devices.iter().enumerate(){if let Some(name)=d.as_str()&& ui.selectable_label(hearing["input_device"].as_str()==Some(name),name).clicked(){command=Some(LabControlCommand::Hearing{action:H::SelectInput{index:i as u16+1}});}}}});
                    let rms=hearing["input_level"].as_f64().unwrap_or(0.0) as f32;ui.add(egui::ProgressBar::new((rms.sqrt()*2.2).clamp(0.0,1.0)).desired_height(4.0).fill(ACCENT));ui.small("Learning stays on this computer.");},
                _=>{if primary(ui,"Replay birth"){command=Some(LabControlCommand::ReplayBirth);state.close=true;}ui.label(RichText::new("His age and memories stay the same.").small().color(MUTED));if ui.button("Dash to my cursor").clicked(){command=Some(LabControlCommand::Hearing{action:H::Perform{cue:CueKind::Dash}});state.close=true;}
if ui.button("Close menu").clicked(){state.close=true;}}
            });
        });
    }
    // Include actual dropdown/tool-tip areas in the native hit region.
    let layers = ctx.memory(|m| m.areas().visible_layer_ids());
    for layer in layers {
        if layer.order == egui::Order::Foreground || layer.order == egui::Order::Tooltip {
            if layer.id == egui::Id::new("care-panel")
                || (0..5).any(|i| layer.id == egui::Id::new(("care-bubble", i)))
            {
                continue;
            }
            if let Some(area) = egui::AreaState::load(ctx, layer.id) {
                state.regions.push((area.rect(), 10.0));
            }
        }
    }
    if closing.is_some()
        || elapsed < 0.7
        || state
            .page_opened
            .is_some_and(|t| t.elapsed().as_secs_f32() < 0.3)
    {
        ctx.request_repaint();
    }
    if let (Some(monitor), Some(command)) = (monitor, command) {
        monitor.send_control(command);
    }
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
    #[test]
    fn opening_visible_menu_preserves_panel_and_animation() {
        let mut menu = super::MenuState::new(true);
        assert!(menu.reopen());
        menu.page = Some(3);
        menu.page_opened = Some(std::time::Instant::now());
        let opened = menu.opened;
        let page_opened = menu.page_opened;
        menu.close = true;
        menu.closing = Some(std::time::Instant::now());
        assert!(!menu.reopen());
        assert_eq!(menu.page, Some(3));
        assert_eq!(menu.opened, opened);
        assert_eq!(menu.page_opened, page_opened);
        assert!(!menu.close && menu.closing.is_none());
    }
    #[test]
    fn fade_has_stationary_endpoints_and_never_reverses() {
        assert_eq!(super::ease(-1.0), 0.0);
        assert_eq!(super::ease(2.0), 1.0);
        assert!(super::ease(0.01) < 0.00002);
        for i in 0..100 {
            assert!(super::ease((i + 1) as f32 / 100.0) >= super::ease(i as f32 / 100.0));
        }
    }
}
