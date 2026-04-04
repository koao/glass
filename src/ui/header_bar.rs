use egui::Ui;

use crate::app::{DisplayMode, GlassApp, MonitorState};
use crate::ui::theme;

/// ヘッダーバー描画（安定レイアウト: 全ボタン常時表示）
pub fn draw(ui: &mut Ui, app: &mut GlassApp) {
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        // 操作ボタン（常時3つ表示、状態に応じて有効/無効）
        let is_stopped = app.state == MonitorState::Stopped;
        let is_running = app.state == MonitorState::Running;
        let is_paused = app.state == MonitorState::Paused;

        // ▶ 開始/再開
        let start_label = if is_paused { "▶ 再開" } else { "▶ 開始" };
        if ui
            .add_enabled(is_stopped || is_paused, egui::Button::new(start_label))
            .clicked()
        {
            if is_paused {
                app.resume();
            } else {
                app.start();
            }
        }

        // ⏸ 一時停止
        if ui
            .add_enabled(is_running, egui::Button::new("⏸ 一時停止"))
            .clicked()
        {
            app.pause();
        }

        // ⏹ 停止
        if ui
            .add_enabled(!is_stopped, egui::Button::new("⏹ 停止"))
            .clicked()
        {
            app.stop();
        }

        ui.separator();

        // 表示モード切替
        ui.selectable_value(&mut app.display_mode, DisplayMode::Hex, "HEX");
        ui.selectable_value(&mut app.display_mode, DisplayMode::Ascii, "ASCII");

        ui.separator();

        // IDLE閾値設定
        ui.label("IDLE:");
        ui.add(
            egui::DragValue::new(&mut app.idle_threshold_ms)
                .range(1.0..=1000.0)
                .speed(1.0)
                .suffix(" ms"),
        );

        // エラー表示
        if let Some(err) = &app.last_error {
            ui.add_space(4.0);
            ui.colored_label(theme::STATUS_ERROR, err);
        }

        // 右寄せアイコンボタン
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            // 設定ウィンドウトグル
            if ui.button("⚙").on_hover_text("設定").clicked() {
                app.ui_state.show_settings_window = !app.ui_state.show_settings_window;
            }

            // クリア
            if ui.button("🗑").on_hover_text("クリア").clicked() {
                app.clear_all();
            }

            // 検索トグル
            let search_label = if app.ui_state.show_search_bar {
                "🔍✕"
            } else {
                "🔍"
            };
            if ui
                .button(search_label)
                .on_hover_text("検索 (Ctrl+F)")
                .clicked()
            {
                app.ui_state.show_search_bar = !app.ui_state.show_search_bar;
            }
        });
    });
    ui.add_space(4.0);
}
