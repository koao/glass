use egui::RichText;

use crate::app::{ConfirmAction, DialogKind, GlassApp};

/// ダイアログ描画（egui::Modal ベース）
pub fn draw(ctx: &egui::Context, app: &mut GlassApp) {
    let Some(kind) = &app.ui_state.dialog else {
        return;
    };

    // クロージャ内で app を借用しないよう、事前に必要な値を取り出す
    let (title, message, on_confirm) = match kind {
        DialogKind::Confirm {
            title,
            message,
            on_confirm,
        } => (title.as_str(), message.as_str(), Some(*on_confirm)),
        DialogKind::Info { title, message } => (title.as_str(), message.as_str(), None),
    };
    let btn_yes = app.t.confirm_yes;
    let btn_no = app.t.confirm_no;
    let btn_ok = app.t.ok;

    let modal = egui::Modal::new(egui::Id::new("app_dialog"))
        .frame(egui::Frame::popup(ctx.global_style().as_ref()).inner_margin(24.0));

    let mut close = false;
    let mut confirmed = false;

    let resp = modal.show(ctx, |ui| {
        ui.set_min_width(320.0);

        // --- タイトル + メッセージ ---
        ui.add_space(4.0);
        ui.label(RichText::new(title).strong().size(18.0));
        ui.add_space(8.0);
        ui.label(RichText::new(message).size(14.0));
        ui.add_space(16.0);

        ui.separator();
        ui.add_space(8.0);

        // --- ボタン（右寄せ） ---
        let btn_min = egui::vec2(80.0, 0.0);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if on_confirm.is_some() {
                if ui
                    .add(egui::Button::new(btn_yes).min_size(btn_min))
                    .clicked()
                {
                    confirmed = true;
                    close = true;
                }
                if ui
                    .add(egui::Button::new(btn_no).min_size(btn_min))
                    .clicked()
                {
                    close = true;
                }
            } else if ui
                .add(egui::Button::new(btn_ok).min_size(btn_min))
                .clicked()
            {
                close = true;
            }
        });
    });

    if resp.should_close() || close {
        if confirmed && let Some(action) = on_confirm {
            match action {
                ConfirmAction::ClearAll => app.clear_all(),
            }
        }
        app.ui_state.dialog = None;
    }
}
