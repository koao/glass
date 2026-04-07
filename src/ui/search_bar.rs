use egui::{Ui, Vec2};
use egui_phosphor::regular;

use crate::app::{GlassApp, MonitorState};
use crate::ui::theme;

/// トグル式検索バー描画
pub fn draw(ui: &mut Ui, app: &mut GlassApp) {
    // ボタン1行分の高さに制限しつつ垂直中央揃え
    let row_height = ui.text_style_height(&egui::TextStyle::Button)
        + ui.spacing().button_padding.y * 2.0
        + ui.spacing().item_spacing.y;
    ui.allocate_ui_with_layout(
        Vec2::new(ui.available_width(), row_height),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.label(app.t.search_label);

            let response = ui.add(
                egui::TextEdit::singleline(&mut app.search.query)
                    .desired_width(200.0)
                    .hint_text(app.t.search_hint),
            );
            let enter_pressed =
                response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));

            let search_clicked = ui.button(app.t.search_button).clicked() || enter_pressed;

            // クリアボタン（検索実行済みの場合に有効）
            if ui
                .add_enabled(
                    app.search.has_searched,
                    egui::Button::new(format!("{} {}", regular::ERASER, app.t.search_clear)),
                )
                .clicked()
            {
                app.search.reset();
            }

            // 受信中は移動ボタン無効
            let is_stopped = app.state == MonitorState::Stopped;
            let has_results = app.search.result_count() > 0;
            let can_navigate = has_results && is_stopped;
            let prev_clicked = ui
                .add_enabled(can_navigate, egui::Button::new(regular::CARET_LEFT))
                .clicked();
            let next_clicked = ui
                .add_enabled(can_navigate, egui::Button::new(regular::CARET_RIGHT))
                .clicked();

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

            if app.search.has_searched {
                let count = app.search.result_count();
                if count > 0 {
                    ui.label(format!("{}/{}", app.search.current_index() + 1, count));
                } else {
                    ui.colored_label(theme::TEXT_MUTED, app.t.no_match);
                }
            }

            // 右寄せ: ヘルプボタン
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .button(format!("{} {}", regular::INFO, app.t.help))
                    .clicked()
                {
                    app.ui_state.show_search_help = !app.ui_state.show_search_help;
                }
            });
        },
    );
}

/// 検索ヘルプウィンドウ描画
pub fn draw_help(ui: &mut Ui, app: &mut GlassApp) {
    if !app.ui_state.show_search_help {
        return;
    }
    egui::Window::new(app.t.search_help_title)
        .id(egui::Id::new("search_help_window"))
        .collapsible(false)
        .resizable(false)
        .default_width(280.0)
        .open(&mut app.ui_state.show_search_help)
        .show(ui.ctx(), |ui| {
            ui.label(app.t.search_help_desc);
            ui.add_space(4.0);

            egui::Grid::new("search_help_grid")
                .num_columns(2)
                .spacing([12.0, 4.0])
                .show(ui, |ui| {
                    ui.strong(app.t.search_help_input);
                    ui.strong(app.t.search_help_meaning);
                    ui.end_row();

                    ui.monospace("$XX");
                    ui.label(app.t.search_help_hex_byte);
                    ui.end_row();

                    ui.monospace(app.t.search_help_other_chars);
                    ui.label(app.t.search_help_ascii_literal);
                    ui.end_row();
                });

            ui.add_space(8.0);
            ui.separator();
            ui.add_space(4.0);
            ui.label(app.t.search_help_examples);
            ui.add_space(2.0);

            egui::Grid::new("search_help_examples")
                .num_columns(2)
                .spacing([12.0, 4.0])
                .show(ui, |ui| {
                    ui.monospace("OK$0D$0A");
                    ui.label("-> OK + CR + LF");
                    ui.end_row();

                    ui.monospace("$02$03");
                    ui.label("-> STX + ETX");
                    ui.end_row();

                    ui.monospace("Hello");
                    ui.label("-> ASCII \"Hello\"");
                    ui.end_row();
                });
        });
}
