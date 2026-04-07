//! トリガ設定ウィンドウ

use eframe::egui;

use crate::app::GlassApp;

pub fn draw(ctx: &egui::Context, app: &mut GlassApp) {
    if !app.ui_state.show_trigger_window {
        return;
    }
    let mut open = true;
    egui::Window::new(app.t.trigger_settings)
        .open(&mut open)
        .resizable(false)
        .collapsible(false)
        .auto_sized()
        .show(ctx, |ui| {
            ui.set_max_width(300.0);
            ui.label(app.t.trigger_pattern_label);
            let mut text = app.trigger.pattern_text.clone();
            let resp = ui.add(
                egui::TextEdit::singleline(&mut text)
                    .hint_text(app.t.trigger_pattern_hint)
                    .desired_width(300.0),
            );
            if resp.changed() {
                app.trigger.set_pattern_text(text);
            }

            ui.add_space(6.0);
            ui.label(app.t.trigger_post_delay_label);
            ui.horizontal(|ui| {
                let mut n = app.trigger.post_match_delay_ms;
                if ui
                    .add(egui::DragValue::new(&mut n).range(0..=600_000).speed(10.0))
                    .changed()
                {
                    app.trigger.post_match_delay_ms = n;
                }
                ui.label(app.t.trigger_post_delay_unit);
            });

            ui.add_space(6.0);
            ui.separator();
            ui.label(app.t.trigger_oneshot_note);
        });
    if !open {
        app.ui_state.show_trigger_window = false;
    }
}
