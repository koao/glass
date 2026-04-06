use egui::Vec2;

use crate::app::GlassApp;
use crate::ui::confirm_dialog::draw_modal;

/// エラーダイアログ描画（モーダル）
pub fn draw(ui: &mut egui::Ui, app: &mut GlassApp) {
    let Some(message) = &app.ui_state.error_message else {
        return;
    };

    let mut close = false;
    let message = message.clone();
    draw_modal(ui.ctx(), "error_dialog", app.t.err_dialog_title, |ui| {
        ui.label(&message);
        ui.add_space(4.0);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .add(egui::Button::new(app.t.ok).min_size(Vec2::new(80.0, 28.0)))
                .clicked()
            {
                close = true;
            }
        });
    });

    if close {
        app.ui_state.error_message = None;
    }
}
