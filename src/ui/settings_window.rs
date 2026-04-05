use egui::Ui;
use egui_phosphor::regular;

use crate::app::{GlassApp, MonitorState, SettingsTab};
use crate::serial::config::{BAUD_RATES, DATA_BITS, ParitySetting, StopBitsSetting};
use crate::ui::theme;

/// 設定ウィンドウ描画（中央配置・タブ付き）
pub fn draw(ui: &mut Ui, app: &mut GlassApp) {
    let mut open = app.ui_state.show_settings_window;

    let screen_rect = ui.ctx().content_rect();
    let window_size = egui::vec2(360.0, 340.0);
    let center = egui::pos2(
        screen_rect.center().x - window_size.x / 2.0,
        screen_rect.center().y - window_size.y / 2.0,
    );

    egui::Window::new("設定")
        .open(&mut open)
        .resizable(false)
        .collapsible(false)
        .default_pos(center)
        .default_size(window_size)
        .show(ui.ctx(), |ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(
                    &mut app.ui_state.settings_tab,
                    SettingsTab::Serial,
                    "シリアルポート",
                );
                ui.selectable_value(
                    &mut app.ui_state.settings_tab,
                    SettingsTab::Display,
                    "表示",
                );
            });
            ui.separator();

            match app.ui_state.settings_tab {
                SettingsTab::Serial => draw_serial_tab(ui, app),
                SettingsTab::Display => draw_display_tab(ui, app),
            }
        });

    app.ui_state.show_settings_window = open;
}

/// ラベル＋フル幅ComboBoxを描画するヘルパー
fn combo_row<T: PartialEq + Clone>(
    ui: &mut Ui,
    id: &str,
    label: &str,
    enabled: bool,
    current: &mut T,
    items: &[(T, String)],
    selected_text: &str,
) {
    ui.label(label);
    ui.add_enabled_ui(enabled, |ui| {
        egui::ComboBox::from_id_salt(id)
            .width(ui.available_width() - 8.0)
            .selected_text(selected_text)
            .show_ui(ui, |ui| {
                for (value, label) in items {
                    ui.selectable_value(current, value.clone(), label.as_str());
                }
            });
    });
    ui.add_space(8.0);
}

/// シリアルポート設定タブ
fn draw_serial_tab(ui: &mut Ui, app: &mut GlassApp) {
    let is_stopped = app.state == MonitorState::Stopped;

    ui.add_space(4.0);

    ui.horizontal(|ui| {
        ui.label("ポート:");
        if ui.button(regular::ARROWS_CLOCKWISE).on_hover_text("ポート一覧を更新").clicked() {
            app.refresh_ports();
        }
    });
    let port_label = if app.config.port_name.is_empty() {
        "選択してください".to_string()
    } else {
        app.config.port_name.clone()
    };
    let port_items: Vec<_> = app
        .available_ports
        .iter()
        .map(|p| (p.clone(), p.clone()))
        .collect();
    combo_row(
        ui, "com_port", "", is_stopped,
        &mut app.config.port_name, &port_items, &port_label,
    );

    let baud_items: Vec<_> = BAUD_RATES.iter().map(|&r| (r, r.to_string())).collect();
    let baud_text = app.config.baud_rate.to_string();
    combo_row(
        ui, "baud_rate", "ボーレート:", is_stopped,
        &mut app.config.baud_rate, &baud_items, &baud_text,
    );

    let data_items: Vec<_> = DATA_BITS.iter().map(|&b| (b, b.to_string())).collect();
    let data_text = app.config.data_bits.to_string();
    combo_row(
        ui, "data_bits", "データビット:", is_stopped,
        &mut app.config.data_bits, &data_items, &data_text,
    );

    let parity_items: Vec<_> = ParitySetting::ALL.iter().map(|p| (p.clone(), p.label().to_string())).collect();
    let parity_text = app.config.parity.label().to_string();
    combo_row(
        ui, "parity", "パリティ:", is_stopped,
        &mut app.config.parity, &parity_items, &parity_text,
    );

    let stop_items: Vec<_> = StopBitsSetting::ALL.iter().map(|s| (s.clone(), s.label().to_string())).collect();
    let stop_text = app.config.stop_bits.label().to_string();
    combo_row(
        ui, "stop_bits", "ストップビット:", is_stopped,
        &mut app.config.stop_bits, &stop_items, &stop_text,
    );

    ui.add_space(4.0);

    if !is_stopped {
        ui.colored_label(theme::TEXT_MUTED, format!("{} 設定変更は停止中のみ可能", regular::WARNING));
    }
}

/// 表示設定タブ
fn draw_display_tab(ui: &mut Ui, app: &mut GlassApp) {
    ui.add_space(4.0);

    ui.label("IDLE閾値:");
    ui.add(
        egui::DragValue::new(&mut app.idle_threshold_ms)
            .range(1.0..=1000.0)
            .speed(1.0)
            .suffix(" ms"),
    );
    ui.add_space(4.0);
    ui.colored_label(
        theme::TEXT_MUTED,
        "バイト間の無通信時間がこの値を超えるとIDLEマーカーを表示",
    );
}
