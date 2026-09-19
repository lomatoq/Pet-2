use super::{LabControlCommand, LivePetMonitor};
use desktop_host::{CueKind, HearingAction as H};
use egui::{Color32, RichText};
use serde_json::Value;
#[derive(Default)]
pub(super) struct MenuState {
    page: u8,
    selected: usize,
    pub waiting_for_feed: bool,
    pub close: bool,
    pub hidden: bool,
    pub capture_done: bool,
}
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
    ctx.style_mut(|s| {
        s.spacing.item_spacing = egui::vec2(10.0, 12.0);
        s.spacing.button_padding = egui::vec2(16.0, 10.0);
        s.text_styles
            .insert(egui::TextStyle::Body, egui::FontId::proportional(17.0));
        s.text_styles
            .insert(egui::TextStyle::Button, egui::FontId::proportional(17.0));
    });
}

pub(super) fn show(
    ctx: &egui::Context,
    state: &mut MenuState,
    monitor: Option<&mut LivePetMonitor>,
) {
    egui::CentralPanel::default().frame(egui::Frame::default().fill(Color32::from_rgb(20,24,34)).inner_margin(22.0)).show(ctx, |ui| {
        ui.label(RichText::new("BENNY · ОБЩЕНИЕ").color(Color32::from_rgb(248,184,119)));
        let Some(monitor) = monitor else { ui.label("Не удалось найти питомца."); return; };
        let latest = monitor.selected_frame().cloned().unwrap_or(Value::Null);
        let ready = monitor.can_send_control();
        let hearing = &latest["details"]["hearing"];
        let training = hearing["training"].is_object();
        let enabled = hearing["enabled"].as_bool().unwrap_or(false);
        let rms = hearing["input_level"].as_f64().unwrap_or(0.0) as f32;
        if state.waiting_for_feed && latest["details"]["feeding"]["enabled"].as_bool() == Some(true) { state.close = true; }
        ui.add(egui::ProgressBar::new((rms.sqrt()*2.2).clamp(0.0,1.0)).text(if enabled { "Микрофон · уровень голоса" } else { "Микрофон выключен" }));
        if !ready { ui.label("Подключаюсь к питомцу…"); }
        let mut command = None;
        egui::ScrollArea::vertical().show(ui, |ui| { ui.add_enabled_ui(ready, |ui| {
            if state.page == 0 {
                ui.heading("Давай пообщаемся");
                if ui.button("Кормить · светящиеся крошки").clicked() { command = Some(LabControlCommand::Feeding { enabled:true }); state.waiting_for_feed=true; }
                ui.small("Esc или правый клик — отменить кормление. Старые крошки убираются, новые продолжают сыпаться.");
                if ui.button("Отменить кормление · убрать крошки").clicked() { command=Some(LabControlCommand::Feeding { enabled:false }); }
                if ui.button("Научить · имя и 20 команд").clicked() { state.page=1; }
                if ui.button("⚡ Быстро перелететь к курсору").clicked() { command=Some(LabControlCommand::Hearing { action:H::Perform { cue:CueKind::Dash } }); }
                ui.small("После нажатия перенеси курсор: через секунду он полетит к нему. Можно показать действие до обучения голосу.");
                egui::ComboBox::from_id_salt("hearing_input").selected_text(hearing["input_device"].as_str().unwrap_or("Системный микрофон")).show_ui(ui, |ui| {
                    if ui.selectable_label(hearing["input_device"].is_null(), "Системный микрофон").clicked() { command=Some(LabControlCommand::Hearing { action:H::SelectInput { index:0 } }); }
                    if let Some(devices) = hearing["input_devices"].as_array() {
                        for (i, device) in devices.iter().enumerate() {
                            if let Some(name) = device.as_str()
                                && ui.selectable_label(hearing["input_device"].as_str() == Some(name), name).clicked() {
                                command = Some(LabControlCommand::Hearing { action: H::SelectInput { index: i as u16 + 1 } });
                            }
                        }
                    }
                });
                if hearing["status"]["native_sample_rate"].as_u64().is_some_and(|r| r <= 24000) {
                    ui.small("Микрофон работает в речевом режиме. Для чистого звука Bluetooth-наушников выбери отдельный микрофон или выключи слушание.");
                }
                if ui.button(if enabled { "Выключить микрофон" } else { "Включить микрофон" }).clicked() { command=Some(LabControlCommand::Hearing { action:if enabled { H::Disable } else { H::Enable } }); }
            } else {
                if ui.button("‹ Назад").clicked() { state.page=0; }
                ui.heading("Учим по одной команде");
                egui::ComboBox::from_id_salt("teach-cue").selected_text(CueKind::ALL[state.selected].label()).show_ui(ui, |ui| {
                    for (i,cue) in CueKind::ALL.iter().enumerate() { ui.selectable_value(&mut state.selected,i,cue.label()); }
                });
                let cue = CueKind::ALL[state.selected];
                let record = hearing["cues"].as_array().and_then(|a| a.get(state.selected));
                let count = record.and_then(|r| r["examples"].as_u64()).unwrap_or(0);
                let learned = record.and_then(|r| r["ready"].as_bool()).unwrap_or(false);
                ui.label(format!("Сохранено {count} / 40 · {}",if learned { "можно произносить" } else { "ещё учимся" }));
                ui.label(if cue==CueKind::Name { "Говори Бендер или Бенни. Добавляй повторы с разной привычной интонацией и расстоянием. Имя отдельно вызывает сильное внимание." } else { "Произноси выбранную фразу. Команда работает сама по себе — имя перед ней не нужно. Для «Бендер, сделай круг» обучай и такую фразу отдельно в этой же команде." });
                ui.small("Одна партия — 5 повторов с секундной паузой. Старые примеры сохраняются. Можно добавить 30–40 примеров постепенно. Во время записи питомец молчит.");
                if ui.add_enabled(!training,egui::Button::new("▶ Добавлять примеры")).clicked() { command=Some(LabControlCommand::Hearing { action:H::TrainCommand { cue } }); }
                if training {
                    let n=hearing["training"]["accepted"].as_u64().unwrap_or(0);
                    ui.add(egui::ProgressBar::new(n as f32/5.0).text(format!("Принято {n} из 5")));
                    if ui.button("Остановить запись").clicked() { command=Some(LabControlCommand::Hearing { action:H::CancelTraining }); }
                }
                ui.separator();
                let others=hearing["other_examples"].as_u64().unwrap_or(0);
                ui.label(format!("Посторонние слова: {others} примеров"));
                ui.small("Необязательно: помогают отличать команды от обычной речи. Например: «лампа», «чашка», «сегодня», «окно», «книга». Не произноси здесь имя или команды.");
                if ui.add_enabled(!training,egui::Button::new("Записывать посторонние слова")).clicked() { command=Some(LabControlCommand::Hearing { action:H::TrainOther }); }
                ui.horizontal_wrapped(|ui| {
                    if ui.add_enabled(!training,egui::Button::new("Показать действие")).clicked() { command=Some(LabControlCommand::Hearing { action:H::Perform { cue } }); }
                    if ui.add_enabled(!training,egui::Button::new("Проверить голосом")).clicked() { command=Some(LabControlCommand::Hearing { action:H::Test }); }
                });
                ui.small("В проверке команда выполняется. Неузнанный звук — не поломка: добавь примеры или говори с паузой. Это обучение звучанию фразы, не распознавание произвольной речи.");
                ui.label(format!("Последний сигнал: {}", hearing["last_cue"].as_str().unwrap_or("—")));
            }
            if let Some(message)=hearing["message"].as_str() { ui.separator(); ui.label(message); }
        }); });
        if let Some(command)=command { monitor.send_control(command); }
    });
}
