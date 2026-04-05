use egui::{Align2, Color32, RichText, Vec2};
use egui_phosphor::regular;

use crate::app::GlassApp;

/// エラーダイアログ描画（モーダル）
pub fn draw(ui: &mut egui::Ui, app: &mut GlassApp) {
    let Some(message) = &app.ui_state.error_message else {
        return;
    };

    let mut close = false;
    egui::Area::new(egui::Id::new("error_dialog_overlay"))
        .anchor(Align2::LEFT_TOP, Vec2::ZERO)
        .order(egui::Order::Foreground)
        .show(ui.ctx(), |ui| {
            let screen_rect = ui.ctx().input(|i| i.viewport_rect());
            ui.painter().rect_filled(
                screen_rect,
                0.0,
                Color32::from_black_alpha(160),
            );
        });

    egui::Area::new(egui::Id::new("error_dialog"))
        .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
        .order(egui::Order::Foreground)
        .show(ui.ctx(), |ui| {
            egui::Frame::window(ui.style())
                .inner_margin(16.0)
                .show(ui, |ui| {
                    ui.set_min_width(300.0);

                    // タイトル行
                    ui.label(
                        RichText::new(format!("{} {}", regular::WARNING, app.t.err_dialog_title))
                            .strong()
                            .size(16.0),
                    );

                    ui.separator();
                    ui.add_space(8.0);
                    ui.label(message.as_str());
                    ui.add_space(12.0);

                    // OKボタン
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .add(
                                egui::Button::new(app.t.ok)
                                    .min_size(Vec2::new(80.0, 28.0)),
                            )
                            .clicked()
                        {
                            close = true;
                        }
                    });
                });
        });

    if close {
        app.ui_state.error_message = None;
    }
}
