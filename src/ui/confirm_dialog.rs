use egui::{Align2, Color32, RichText, Vec2};
use egui_phosphor::regular;

use crate::app::GlassApp;

/// モーダルオーバーレイ＋中央ダイアログの共通描画
pub fn draw_modal<R>(
    ctx: &egui::Context,
    id: &str,
    title: impl Into<String>,
    body: impl FnOnce(&mut egui::Ui) -> R,
) -> Option<R> {
    // 半透明オーバーレイ
    egui::Area::new(egui::Id::new(format!("{}_overlay", id)))
        .anchor(Align2::LEFT_TOP, Vec2::ZERO)
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            let screen_rect = ctx.input(|i| i.viewport_rect());
            ui.painter().rect_filled(
                screen_rect,
                0.0,
                Color32::from_black_alpha(160),
            );
        });

    let mut result = None;
    egui::Area::new(egui::Id::new(id))
        .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            egui::Frame::window(ui.style())
                .inner_margin(16.0)
                .show(ui, |ui| {
                    ui.set_min_width(300.0);
                    ui.label(
                        RichText::new(format!("{} {}", regular::WARNING, title.into()))
                            .strong()
                            .size(16.0),
                    );
                    ui.separator();
                    ui.add_space(8.0);
                    result = Some(body(ui));
                    ui.add_space(12.0);
                });
        });
    result
}

/// クリア確認ダイアログ描画（モーダル）
pub fn draw(ui: &mut egui::Ui, app: &mut GlassApp) {
    if !app.ui_state.show_clear_confirm {
        return;
    }

    let mut action = None;
    draw_modal(ui.ctx(), "confirm_dialog", app.t.clear, |ui| {
        ui.label(app.t.confirm_clear);
        ui.add_space(4.0);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .add(egui::Button::new(app.t.confirm_yes).min_size(Vec2::new(80.0, 28.0)))
                .clicked()
            {
                action = Some(true);
            }
            if ui
                .add(egui::Button::new(app.t.confirm_no).min_size(Vec2::new(80.0, 28.0)))
                .clicked()
            {
                action = Some(false);
            }
        });
    });

    if let Some(confirmed) = action {
        app.ui_state.show_clear_confirm = false;
        if confirmed {
            app.clear_all();
        }
    }
}
