use egui::Ui;
use egui_phosphor::regular;

use crate::app::{DisplayMode, GlassApp, MonitorState, ViewTab};
use crate::ui::theme;

/// ヘッダーバー描画（安定レイアウト: 全ボタン常時表示）
pub fn draw(ui: &mut Ui, app: &mut GlassApp) {
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        // 操作ボタン（常時3つ表示、状態に応じて有効/無効）
        let is_stopped = app.state == MonitorState::Stopped;
        let is_running = app.state == MonitorState::Running;
        let is_paused = app.state == MonitorState::Paused;

        // 開始/再開
        let start_label = if is_paused {
            format!("{} {}", regular::PLAY, app.t.resume)
        } else {
            format!("{} {}", regular::PLAY, app.t.start)
        };
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

        // 一時停止
        if ui
            .add_enabled(is_running, egui::Button::new(format!("{} {}", regular::PAUSE, app.t.pause)))
            .clicked()
        {
            app.pause();
        }

        // 停止
        if ui
            .add_enabled(!is_stopped, egui::Button::new(format!("{} {}", regular::STOP, app.t.stop)))
            .clicked()
        {
            app.stop();
        }

        ui.separator();

        // 表示タブ切替
        ui.selectable_value(&mut app.active_tab, ViewTab::Monitor, app.t.tab_monitor);
        ui.selectable_value(&mut app.active_tab, ViewTab::Protocol, app.t.tab_protocol);

        ui.separator();

        // 表示モード切替（モニタタブ時のみ表示）
        if app.active_tab == ViewTab::Monitor {
            ui.selectable_value(&mut app.display_mode, DisplayMode::Hex, "HEX");
            ui.selectable_value(&mut app.display_mode, DisplayMode::Ascii, "ASCII");
        }

        // エラー表示
        if let Some(err) = &app.last_error {
            ui.add_space(4.0);
            ui.colored_label(theme::STATUS_ERROR, err);
        }

        // 右寄せアイコンボタン
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            // 設定ウィンドウトグル（停止中のみ）
            if ui
                .add_enabled(is_stopped, egui::Button::new(regular::GEAR_SIX))
                .on_hover_text(if is_stopped { app.t.settings } else { app.t.settings_stopped_only })
                .clicked()
            {
                app.ui_state.show_settings_window = !app.ui_state.show_settings_window;
            }

            // クリア
            if ui.button(regular::TRASH).on_hover_text(app.t.clear).clicked() {
                app.clear_all();
            }

            // スクリーンショット
            if ui
                .button(regular::CAMERA)
                .on_hover_text(app.t.screenshot)
                .clicked()
            {
                app.ui_state.screenshot_requested = true;
            }

            // ファイル保存（停止中かつデータあり）
            let has_data = app.buffer.byte_count() > 0;
            if ui
                .add_enabled(is_stopped && has_data, egui::Button::new(regular::FLOPPY_DISK))
                .on_hover_text(if is_stopped { app.t.save_file } else { app.t.save_file_stopped_only })
                .clicked()
            {
                app.save_to_file();
            }

            // ファイル読み込み（停止中のみ）
            if ui
                .add_enabled(is_stopped, egui::Button::new(regular::FOLDER_OPEN))
                .on_hover_text(if is_stopped { app.t.load_file } else { app.t.load_file_stopped_only })
                .clicked()
            {
                app.load_from_file();
            }

            // 検索トグル
            if ui
                .button(regular::MAGNIFYING_GLASS)
                .on_hover_text(app.t.search_shortcut)
                .clicked()
            {
                app.ui_state.show_search_bar = !app.ui_state.show_search_bar;
                if !app.ui_state.show_search_bar {
                    app.search.reset();
                }
            }
        });
    });
    ui.add_space(4.0);
}
