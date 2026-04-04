use egui::Ui;

use crate::app::{DisplayMode, GlassApp, MonitorState};
use crate::serial::config::{BAUD_RATES, DATA_BITS, ParitySetting, StopBitsSetting};
use crate::ui::search::SearchMode;

/// ツールバー描画
pub fn draw(ui: &mut Ui, app: &mut GlassApp) {
    let is_stopped = app.state == MonitorState::Stopped;

    // 1行目: シリアル設定
    ui.horizontal(|ui| {
        ui.label("Port:");
        let port_label = if app.config.port_name.is_empty() {
            "選択してください".to_string()
        } else {
            app.config.port_name.clone()
        };
        ui.add_enabled_ui(is_stopped, |ui| {
            egui::ComboBox::from_id_salt("com_port")
                .width(120.0)
                .selected_text(&port_label)
                .show_ui(ui, |ui| {
                    for port in &app.available_ports {
                        ui.selectable_value(&mut app.config.port_name, port.clone(), port);
                    }
                });
        });
        if ui.button("🔄").on_hover_text("ポート一覧を更新").clicked() {
            app.refresh_ports();
        }

        ui.separator();

        ui.label("Baud:");
        ui.add_enabled_ui(is_stopped, |ui| {
            egui::ComboBox::from_id_salt("baud_rate")
                .width(80.0)
                .selected_text(app.config.baud_rate.to_string())
                .show_ui(ui, |ui| {
                    for &rate in BAUD_RATES {
                        ui.selectable_value(&mut app.config.baud_rate, rate, rate.to_string());
                    }
                });
        });

        ui.label("Data:");
        ui.add_enabled_ui(is_stopped, |ui| {
            egui::ComboBox::from_id_salt("data_bits")
                .width(50.0)
                .selected_text(app.config.data_bits.to_string())
                .show_ui(ui, |ui| {
                    for &bits in DATA_BITS {
                        ui.selectable_value(&mut app.config.data_bits, bits, bits.to_string());
                    }
                });
        });

        ui.label("Parity:");
        ui.add_enabled_ui(is_stopped, |ui| {
            egui::ComboBox::from_id_salt("parity")
                .width(70.0)
                .selected_text(app.config.parity.label())
                .show_ui(ui, |ui| {
                    for p in ParitySetting::ALL {
                        ui.selectable_value(&mut app.config.parity, p.clone(), p.label());
                    }
                });
        });

        ui.label("Stop:");
        ui.add_enabled_ui(is_stopped, |ui| {
            egui::ComboBox::from_id_salt("stop_bits")
                .width(50.0)
                .selected_text(app.config.stop_bits.label())
                .show_ui(ui, |ui| {
                    for s in StopBitsSetting::ALL {
                        ui.selectable_value(&mut app.config.stop_bits, s.clone(), s.label());
                    }
                });
        });
    });

    // 2行目: 制御ボタン + 表示設定
    ui.horizontal(|ui| {
        match app.state {
            MonitorState::Stopped => {
                if ui.button("▶ 開始").clicked() {
                    app.start();
                }
            }
            MonitorState::Running => {
                if ui.button("⏸ 一時停止").clicked() {
                    app.pause();
                }
                if ui.button("⏹ 停止").clicked() {
                    app.stop();
                }
            }
            MonitorState::Paused => {
                if ui.button("▶ 再開").clicked() {
                    app.resume();
                }
                if ui.button("⏹ 停止").clicked() {
                    app.stop();
                }
            }
        }

        if ui.button("🗑 クリア").clicked() {
            app.clear_all();
        }

        ui.separator();

        ui.selectable_value(&mut app.display_mode, DisplayMode::Hex, "HEX");
        ui.selectable_value(&mut app.display_mode, DisplayMode::Ascii, "ASCII");

        ui.separator();

        ui.label("IDLE:");
        ui.add(
            egui::DragValue::new(&mut app.idle_threshold_ms)
                .range(1.0..=1000.0)
                .speed(1.0)
                .suffix(" ms"),
        );
    });

    // 3行目: 検索バー
    ui.horizontal(|ui| {
        ui.label("検索:");
        ui.selectable_value(&mut app.search.mode, SearchMode::Hex, "HEX");
        ui.selectable_value(&mut app.search.mode, SearchMode::Ascii, "ASCII");

        let response = ui.add(egui::TextEdit::singleline(&mut app.search.query).desired_width(200.0));
        let enter_pressed = response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));

        if ui.button("検索").clicked() || enter_pressed {
            let entries = app.buffer.entries().to_vec();
            app.search.search(&entries);
        }

        let has_results = app.search.result_count() > 0;
        if ui.add_enabled(has_results, egui::Button::new("◀")).clicked() {
            let entries = app.buffer.entries().to_vec();
            app.search.prev(&entries);
        }
        if ui.add_enabled(has_results, egui::Button::new("▶")).clicked() {
            let entries = app.buffer.entries().to_vec();
            app.search.next(&entries);
        }

        if !app.search.query.is_empty() {
            let count = app.search.result_count();
            if count > 0 {
                ui.label(format!("{}/{}", app.search.current + 1, count));
            } else {
                ui.colored_label(egui::Color32::GRAY, "一致なし");
            }
        }
    });

    // エラー表示
    if let Some(err) = &app.last_error {
        ui.colored_label(egui::Color32::RED, err);
    }
}
