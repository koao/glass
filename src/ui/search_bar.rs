use egui::Ui;

use crate::app::GlassApp;
use crate::ui::search::SearchMode;
use crate::ui::theme;

/// トグル式検索バー描画
pub fn draw(ui: &mut Ui, app: &mut GlassApp) {
    ui.horizontal(|ui| {
        ui.label("検索:");
        ui.selectable_value(&mut app.search.mode, SearchMode::Hex, "HEX");
        ui.selectable_value(&mut app.search.mode, SearchMode::Ascii, "ASCII");

        let response = ui.add(
            egui::TextEdit::singleline(&mut app.search.query).desired_width(200.0),
        );
        let enter_pressed =
            response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));

        let search_clicked = ui.button("検索").clicked() || enter_pressed;
        let has_results = app.search.result_count() > 0;
        let prev_clicked = ui.add_enabled(has_results, egui::Button::new("◀")).clicked();
        let next_clicked = ui.add_enabled(has_results, egui::Button::new("▶")).clicked();

        // バッファのクローンは操作があった場合のみ1回
        if search_clicked || prev_clicked || next_clicked {
            let entries = app.buffer.entries().to_vec();
            if search_clicked {
                app.search.search(&entries);
            } else if prev_clicked {
                app.search.prev(&entries);
            } else {
                app.search.next(&entries);
            }
        }

        if !app.search.query.is_empty() {
            let count = app.search.result_count();
            if count > 0 {
                ui.label(format!("{}/{}", app.search.current + 1, count));
            } else {
                ui.colored_label(theme::TEXT_MUTED, "一致なし");
            }
        }

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("✕").on_hover_text("閉じる (Esc)").clicked() {
                app.ui_state.show_search_bar = false;
            }
        });
    });
}
