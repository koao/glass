use egui::Ui;
use egui_phosphor::regular;

use crate::app::{GlassApp, MonitorState, SettingsTab};
use crate::i18n::Language;
use crate::serial::config::{BAUD_RATES, DATA_BITS, ParitySetting, StopBitsSetting};
use crate::settings::MonitorColors;
use crate::ui::theme;

/// 設定ウィンドウ描画（中央配置・タブ付き）
pub fn draw(ui: &mut Ui, app: &mut GlassApp) {
    let was_open = app.ui_state.show_settings_window;
    let mut open = was_open;

    let screen_rect = ui.ctx().content_rect();
    let window_size = egui::vec2(360.0, 340.0);
    let center = egui::pos2(
        screen_rect.center().x - window_size.x / 2.0,
        screen_rect.center().y - window_size.y / 2.0,
    );

    egui::Window::new(app.t.settings_title)
        .id(egui::Id::new("settings_window"))
        .open(&mut open)
        .resizable(false)
        .collapsible(false)
        .default_pos(center)
        .fixed_size(window_size)
        .show(ui.ctx(), |ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(
                    &mut app.ui_state.settings_tab,
                    SettingsTab::Serial,
                    app.t.tab_serial,
                );
                ui.selectable_value(
                    &mut app.ui_state.settings_tab,
                    SettingsTab::Display,
                    app.t.tab_display,
                );
                ui.selectable_value(
                    &mut app.ui_state.settings_tab,
                    SettingsTab::Colors,
                    app.t.tab_colors,
                );
            });
            ui.separator();

            match app.ui_state.settings_tab {
                SettingsTab::Serial => draw_serial_tab(ui, app),
                SettingsTab::Display => draw_display_tab(ui, app),
                SettingsTab::Colors => draw_colors_tab(ui, app),
            }
        });

    app.ui_state.show_settings_window = open;

    // 設定ウィンドウが閉じられたとき設定を保存
    if was_open && !open {
        app.save_settings();
    }
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

    ui.label(app.t.port_label);
    let port_label = if app.config.port_name.is_empty() {
        app.t.port_select.to_string()
    } else {
        app.config.port_name.clone()
    };
    let port_items: Vec<_> = app
        .available_ports
        .iter()
        .map(|p| (p.clone(), p.clone()))
        .collect();
    let button_width = 28.0;
    let combo_width = ui.available_width() - button_width - 8.0 - ui.spacing().item_spacing.x;
    ui.horizontal(|ui| {
        ui.add_enabled_ui(is_stopped, |ui| {
            egui::ComboBox::from_id_salt("com_port")
                .width(combo_width)
                .selected_text(&port_label)
                .show_ui(ui, |ui| {
                    for (value, label) in &port_items {
                        ui.selectable_value(
                            &mut app.config.port_name,
                            value.clone(),
                            label.as_str(),
                        );
                    }
                });
        });
        if ui
            .button(regular::ARROWS_CLOCKWISE)
            .on_hover_text(app.t.port_refresh)
            .clicked()
        {
            app.refresh_ports();
        }
    });
    ui.add_space(8.0);

    let baud_items: Vec<_> = BAUD_RATES.iter().map(|&r| (r, r.to_string())).collect();
    let baud_text = app.config.baud_rate.to_string();
    combo_row(
        ui,
        "baud_rate",
        app.t.baud_rate,
        is_stopped,
        &mut app.config.baud_rate,
        &baud_items,
        &baud_text,
    );

    let data_items: Vec<_> = DATA_BITS.iter().map(|&b| (b, b.to_string())).collect();
    let data_text = app.config.data_bits.to_string();
    combo_row(
        ui,
        "data_bits",
        app.t.data_bits,
        is_stopped,
        &mut app.config.data_bits,
        &data_items,
        &data_text,
    );

    let parity_items: Vec<_> = ParitySetting::ALL
        .iter()
        .map(|p| (p.clone(), p.label().to_string()))
        .collect();
    let parity_text = app.config.parity.label().to_string();
    combo_row(
        ui,
        "parity",
        app.t.parity,
        is_stopped,
        &mut app.config.parity,
        &parity_items,
        &parity_text,
    );

    let stop_items: Vec<_> = StopBitsSetting::ALL
        .iter()
        .map(|s| (s.clone(), s.label().to_string()))
        .collect();
    let stop_text = app.config.stop_bits.label().to_string();
    combo_row(
        ui,
        "stop_bits",
        app.t.stop_bits,
        is_stopped,
        &mut app.config.stop_bits,
        &stop_items,
        &stop_text,
    );

    ui.add_space(4.0);

    if !is_stopped {
        ui.colored_label(
            theme::TEXT_MUTED,
            format!("{} {}", regular::WARNING, app.t.settings_stopped_msg),
        );
    }
}

/// 表示設定タブ
fn draw_display_tab(ui: &mut Ui, app: &mut GlassApp) {
    ui.add_space(4.0);

    // 言語選択
    ui.label(app.t.language);
    egui::ComboBox::from_id_salt("language")
        .width(ui.available_width() - 8.0)
        .selected_text(app.lang.label())
        .show_ui(ui, |ui| {
            for &lang in Language::ALL {
                if ui
                    .selectable_value(&mut app.lang, lang, lang.label())
                    .changed()
                {
                    app.t = app.lang.texts();
                }
            }
        });
    ui.add_space(8.0);

    ui.label(app.t.idle_threshold);
    ui.add(
        egui::DragValue::new(&mut app.idle_threshold_ms)
            .range(1.0..=1000.0)
            .speed(1.0)
            .suffix(" ms"),
    );
    ui.add_space(4.0);
    ui.colored_label(theme::TEXT_MUTED, app.t.idle_desc);
}

/// 配色設定タブ
fn draw_colors_tab(ui: &mut Ui, app: &mut GlassApp) {
    ui.add_space(4.0);

    color_row(ui, app.t.color_data, &mut app.monitor_colors.data_color);
    color_row(
        ui,
        app.t.color_control,
        &mut app.monitor_colors.control_color,
    );
    color_row(
        ui,
        app.t.color_high_byte,
        &mut app.monitor_colors.high_byte_color,
    );
    color_row(ui, app.t.color_idle_text, &mut app.monitor_colors.idle_text);
    color_row(ui, app.t.color_idle_bg, &mut app.monitor_colors.idle_bg);

    ui.add_space(12.0);
    if ui.button(app.t.color_reset).clicked() {
        app.monitor_colors = MonitorColors::default();
    }
}

/// 色設定行: ラベル＋カラーピッカーボタン
fn color_row(ui: &mut Ui, label: &str, rgb: &mut [u8; 3]) {
    ui.horizontal(|ui| {
        let mut color = egui::Color32::from_rgb(rgb[0], rgb[1], rgb[2]);
        egui::color_picker::color_edit_button_srgba(
            ui,
            &mut color,
            egui::color_picker::Alpha::Opaque,
        );
        if [color.r(), color.g(), color.b()] != *rgb {
            *rgb = [color.r(), color.g(), color.b()];
        }
        ui.label(label);
    });
    ui.add_space(4.0);
}
