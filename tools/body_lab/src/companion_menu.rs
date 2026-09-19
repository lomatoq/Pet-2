use super::{LabControlCommand, LivePetMonitor};
use desktop_host::HearingAction as H;
use egui::{Color32, RichText};
use serde_json::Value;

#[derive(Default)]
pub(super) struct MenuState {
    page: u8,
    step: u8,
    quiet: bool,
    training_baseline: u64,
    waiting_for_feed: bool,
    pub close: bool,
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
    egui::CentralPanel::default().frame(egui::Frame::default()
        .fill(Color32::from_rgb(20, 24, 34)).inner_margin(26.0)).show(ctx, |ui| {
        ui.label(RichText::new("ПЕРСИК").size(13.0).color(Color32::from_rgb(248, 184, 119)));
        ui.heading(if state.page == 0 { "Давай пообщаемся" } else { "Учимся слышать" });
        let Some(monitor) = monitor else { ui.label("Не удалось найти питомца."); return; };
        let latest = monitor.selected_frame().cloned().unwrap_or(Value::Null);
        let ready = monitor.can_send_control();
        if !ready { ui.label("Подключаюсь к Персику…"); }
        if state.waiting_for_feed && latest.pointer("/details/feeding/enabled").and_then(Value::as_bool) == Some(true) {
            state.close = true;
        }
        let hearing = latest.pointer("/details/hearing").cloned().unwrap_or(Value::Null);
        let enabled = hearing["enabled"].as_bool().unwrap_or(false);
        let rms = hearing["input_level"].as_f64().unwrap_or(0.0) as f32;
        ui.add_space(6.0);
        ui.add(egui::ProgressBar::new((rms.sqrt() * 2.2).clamp(0.0, 1.0)).text(if enabled { "Микрофон · уровень звука" } else { "Микрофон выключен" }));
        if enabled {
            ui.small(hearing["device"].as_str().unwrap_or("Ожидание устройства…"));
            if let Some(error) = hearing.pointer("/status/last_error").and_then(Value::as_str) {
                ui.colored_label(Color32::LIGHT_RED, error);
            }
        }
        let mut command = None;
        egui::ScrollArea::vertical().max_height(ui.available_height() - 50.0).show(ui, |ui| {
        ui.add_enabled_ui(ready, |ui| {
            if state.page == 0 {
                ui.add_space(8.0);
                if ui.button("Кормить · светящиеся крошки").clicked() {
                    command = Some(LabControlCommand::Feeding { enabled: true });
                    state.waiting_for_feed = true;
                }
                ui.small("Кликай по рабочему столу — появятся маленькие шарики еды. Правый клик или Esc завершит кормление. Режим длится до 90 секунд.");
                if ui.button("Научить · имя и «тише»").clicked() { state.page = 1; }
                ui.small("Пять примеров, другие слова и проверка. Всё обрабатывается на этом компьютере.");
                ui.separator();
                if ui.button(if enabled { "Выключить микрофон" } else { "Включить реакцию на звуки" }).clicked() {
                    command = Some(LabControlCommand::Hearing { action: if enabled { H::Disable } else { H::Enable } });
                }
                ui.small("Обычный звук привлекает внимание, резкий громкий звук вызывает короткий отскок. Для этого учить слова не нужно.");
            } else {
                ui.horizontal(|ui| {
                    if ui.button("‹ Назад").clicked() { state.page = 0; }
                    ui.label(format!("Шаг {} из 4", state.step + 1));
                });
                if state.step == 0 {
                    ui.selectable_value(&mut state.quiet, false, "Своё имя");
                    ui.selectable_value(&mut state.quiet, true, "Сигнал «тише»");
                    ui.label("Сядь рядом с микрофоном. Говори обычным голосом, не кричи. После каждого примера помолчи примерно секунду.");
                    ui.label("Нажми Play, затем произнеси выбранное имя или слово пять раз с паузами. Счётчик покажет принятые примеры.");
                    if ui.button("▶ Play · начать запись примеров").clicked() {
                        command = Some(LabControlCommand::Hearing { action: if state.quiet { H::TrainQuiet } else { H::TrainName } });
                        state.training_baseline = hearing["completed_trainings"].as_u64().unwrap_or(0);
                        state.step = 1;
                    }
                } else if state.step == 1 || state.step == 2 {
                    let other = state.step == 2;
                    let count = if hearing["training"].is_object() {
                        hearing["training"]["accepted"].as_u64().unwrap_or(0)
                    } else { hearing[if other { "other_examples" } else if state.quiet { "quiet_examples" } else { "name_examples" }].as_u64().unwrap_or(0) };
                    ui.heading(if other { "Теперь другие слова" } else { "Произнеси выбранное слово" });
                    ui.label(if other { "Пять разных слов, кроме имени и «тише»: например, «окно», «чашка», «лампа», «книга», «сегодня». Делай паузы." } else { "Повтори пять раз с небольшими естественными отличиями интонации. Не произноси все примеры слитно." });
                    ui.add(egui::ProgressBar::new((count as f32 / 5.0).min(1.0)).text(format!("{} / 5 примеров", count.min(5))));
                    if let Some(message) = hearing["message"].as_str() { ui.small(message); }
                    let finished = count >= 5 && !hearing["training"].is_object()
                        && hearing["completed_trainings"].as_u64().unwrap_or(0) > state.training_baseline;
                    if ui.add_enabled(finished, egui::Button::new(if other { "▶ Проверить" } else { "Дальше · другие слова" })).clicked() {
                        command = Some(LabControlCommand::Hearing { action: if other { H::Test } else { H::TrainOther } });
                        state.training_baseline = hearing["completed_trainings"].as_u64().unwrap_or(0);
                        state.step += 1;
                    }
                    if ui.button("Повторить этот шаг").clicked() {
                        state.training_baseline = hearing["completed_trainings"].as_u64().unwrap_or(0);
                        command = Some(LabControlCommand::Hearing { action: if other { H::TrainOther } else if state.quiet { H::TrainQuiet } else { H::TrainName } });
                    }
                } else {
                    ui.label("Назови его свежим примером, затем произнеси постороннее слово. Имя должно узнаваться, постороннее слово — отклоняться.");
                    ui.label(format!("Узнано: {} · отклонено: {}", hearing["accepted"], hearing["rejected"]));
                    ui.label(format!("Последний сигнал: {}", hearing["last_cue"].as_str().unwrap_or("—")));
                    ui.small("Имя вызывает внимание. Во время проверки «тише» только распознаётся; после неё снижает голос. Модель рассчитана на твой голос и комнату.");
                    if ui.button("Повторить проверку").clicked() { command = Some(LabControlCommand::Hearing { action: H::Test }); }
                    if ui.button("Готово").clicked() { state.page = 0; state.step = 0; }
                }
                if state.step > 0 && state.step < 3 && ui.button("Остановить обучение").clicked() {
                    command = Some(LabControlCommand::Hearing { action: H::CancelTraining });
                    state.step = 0;
                }
            }
        });
        });
        if let Some(command) = command { monitor.send_control(command); }
        ui.add_space(10.0);
        ui.small("Кормление насыщает и подкрепляет опыт после того, как крошка съедена. Сырые записи голоса не сохраняются.");
    });
}
