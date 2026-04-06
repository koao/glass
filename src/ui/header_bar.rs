use egui::Ui;
use egui_phosphor::regular;

use crate::app::{DisplayMode, GlassApp, MonitorState, ViewTab};

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
            ui.separator();
        }

        // クリア（確認ダイアログ経由）— タブ/モード切替の右隣
        if ui.button(format!("{} {}", regular::TRASH, app.t.clear)).clicked() {
            app.ui_state.show_clear_confirm = true;
        }

        // 右寄せボタン
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            // メニュー（一番右）
            let has_data = app.buffer.byte_count() > 0;
            ui.menu_button(format!("{} {}", regular::LIST, app.t.menu), |ui| {
                ui.spacing_mut().item_spacing.y = 8.0;
                // ファイル読み込み (Ctrl+O)
                if ui
                    .add_enabled(is_stopped, egui::Button::new(
                        format!("{}  {}    Ctrl+O", regular::FOLDER_OPEN, app.t.load_file)
                    ))
                    .clicked()
                {
                    app.load_from_file();
                    ui.close();
                }
                // ファイル保存 (Ctrl+S)
                if ui
                    .add_enabled(is_stopped && has_data, egui::Button::new(
                        format!("{}  {}    Ctrl+S", regular::FLOPPY_DISK, app.t.save_file)
                    ))
                    .clicked()
                {
                    app.save_to_file();
                    ui.close();
                }
                // スクリーンショット (Ctrl+Shift+S)
                if ui
                    .button(format!("{}  {}    Ctrl+Shift+S", regular::CAMERA, app.t.screenshot))
                    .clicked()
                {
                    app.ui_state.screenshot_requested = true;
                    ui.close();
                }
            });

            // 設定ウィンドウトグル（停止中のみ）
            if ui
                .add_enabled(is_stopped, egui::Button::new(
                    format!("{} {}", regular::GEAR_SIX, app.t.settings)
                ))
                .on_disabled_hover_text(app.t.settings_stopped_only)
                .clicked()
            {
                app.ui_state.show_settings_window = !app.ui_state.show_settings_window;
            }

            // 検索トグル（タブに応じてモニタ/プロトコル検索を切替）
            if ui
                .button(format!("{} {}", regular::MAGNIFYING_GLASS, app.t.search_button))
                .on_hover_text(app.t.search_shortcut)
                .clicked()
            {
                app.toggle_search();
            }
        });
    });
    ui.add_space(4.0);
}
