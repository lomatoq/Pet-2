# Current tokens
Segoe UI, fallback Arial. Body/button 17px, spacing 10x12px, button padding16x10px, panel inset22px. Panel RGB20,24,34; accent RGB248,184,119. egui default widgets. This is current ground truth only; requested replacement adopts the new pearl design system. No CSS/Tailwind.
```rust
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


```
