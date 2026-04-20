use egui::Ui;
use egui_phosphor::regular;

use crate::app::{DisplayMode, GlassApp, MonitorState, ViewTab};
use crate::ui::menu::{self, MenuItem};

/// 幅がこの値未満ならタブと表示モードを 2 行目に折り返す。
/// 1 行目 (操作/トリガ/送信/クリア) + 右寄せ (検索/設定/メニュー) が重ならない最低幅の目安。
const WRAP_THRESHOLD: f32 = 1250.0;

/// ヘッダーバー描画（幅が狭い時はタブ/表示モードを 2 行目に折り返す）
pub fn draw(ui: &mut Ui, app: &mut GlassApp) {
    let compact = ui.available_width() < WRAP_THRESHOLD;

    ui.add_space(8.0);
    ui.horizontal(|ui| {
        draw_action_buttons(ui, app);
        ui.separator();
        draw_trigger_buttons(ui, app);
        ui.separator();
        draw_send_button(ui, app);
        // クリアは送信の右隣
        draw_clear_button(ui, app);

        if !compact {
            ui.separator();
            draw_tabs_and_display_mode(ui, app);
        }

        // 右寄せボタン群
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            draw_right_aligned_buttons(ui, app);
        });
    });

    // 幅不足時はタブ/表示モードを 2 行目に表示
    if compact {
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            draw_tabs_and_display_mode(ui, app);
        });
    }

    ui.add_space(4.0);
}

/// 開始 / 一時停止 / 停止
fn draw_action_buttons(ui: &mut Ui, app: &mut GlassApp) {
    let is_stopped = app.state.is_idle();
    let is_running = app.state == MonitorState::Running;
    let is_paused = app.state == MonitorState::Paused;

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

    if ui
        .add_enabled(
            is_running,
            egui::Button::new(format!("{} {}", regular::PAUSE, app.t.pause)),
        )
        .clicked()
    {
        app.pause();
    }

    if ui
        .add_enabled(
            !is_stopped,
            egui::Button::new(format!("{} {}", regular::STOP, app.t.stop)),
        )
        .clicked()
    {
        app.stop();
    }
}

/// トリガ ON/OFF + 設定アイコン
fn draw_trigger_buttons(ui: &mut Ui, app: &mut GlassApp) {
    let trigger_label = format!("{} {}", regular::LIGHTNING, app.t.trigger);
    let has_pattern = !app.trigger.is_pattern_empty();
    let toggle = ui
        .add_enabled(
            has_pattern,
            egui::Button::selectable(app.trigger.armed, trigger_label),
        )
        .on_disabled_hover_text(app.t.trigger_no_pattern);
    if toggle.clicked() {
        if app.trigger.armed {
            app.trigger.disarm();
        } else {
            let len = app.buffer.entries().len();
            app.trigger.arm_from(len);
        }
    }
    if ui
        .button(regular::GEAR_FINE.to_string())
        .on_hover_text(app.t.trigger_settings)
        .clicked()
    {
        app.ui_state.show_trigger_window = !app.ui_state.show_trigger_window;
    }
}

/// 送信パネルトグル
fn draw_send_button(ui: &mut Ui, app: &mut GlassApp) {
    if ui
        .button(format!("{} {}", regular::PAPER_PLANE_TILT, app.t.send))
        .on_hover_text(app.t.send_panel_shortcut)
        .clicked()
    {
        app.ui_state.show_send_panel = !app.ui_state.show_send_panel;
    }
}

/// クリア (確認ダイアログ経由)
fn draw_clear_button(ui: &mut Ui, app: &mut GlassApp) {
    if ui
        .button(format!("{} {}", regular::TRASH, app.t.clear))
        .clicked()
    {
        app.show_clear_confirm();
    }
}

/// モニタ/プロトコルタブ + HEX/ASCII モード
fn draw_tabs_and_display_mode(ui: &mut Ui, app: &mut GlassApp) {
    ui.selectable_value(&mut app.active_tab, ViewTab::Monitor, app.t.tab_monitor);
    ui.selectable_value(&mut app.active_tab, ViewTab::Protocol, app.t.tab_protocol);

    if app.active_tab == ViewTab::Monitor {
        ui.separator();
        ui.selectable_value(&mut app.display_mode, DisplayMode::Hex, "HEX");
        ui.selectable_value(&mut app.display_mode, DisplayMode::Ascii, "ASCII");
    }
}

/// 右寄せ: メニュー / 設定 / 検索
fn draw_right_aligned_buttons(ui: &mut Ui, app: &mut GlassApp) {
    let is_stopped = app.state.is_idle();
    let has_data = app.buffer.byte_count() > 0;

    ui.menu_button(format!("{} {}", regular::LIST, app.t.menu), |ui| {
        let items = [
            MenuItem::new(app.t.load_file)
                .icon(regular::FOLDER_OPEN)
                .shortcut("Ctrl+O")
                .enabled(is_stopped),
            MenuItem::new(app.t.save_file)
                .icon(regular::FLOPPY_DISK)
                .shortcut("Ctrl+S")
                .enabled(is_stopped && has_data),
            MenuItem::new(app.t.screenshot)
                .icon(regular::CAMERA)
                .shortcut("Ctrl+Shift+S"),
        ];
        if let Some(idx) = menu::show(ui, &items) {
            match idx {
                0 => app.load_from_file(),
                1 => app.save_to_file(),
                2 => app.ui_state.screenshot_requested = true,
                _ => {}
            }
            ui.close();
        }
    });

    if ui
        .add_enabled(
            is_stopped,
            egui::Button::new(format!("{} {}", regular::GEAR_SIX, app.t.settings)),
        )
        .on_disabled_hover_text(app.t.settings_stopped_only)
        .clicked()
    {
        app.ui_state.show_settings_window = !app.ui_state.show_settings_window;
    }

    if ui
        .button(format!(
            "{} {}",
            regular::MAGNIFYING_GLASS,
            app.t.search_button
        ))
        .on_hover_text(app.t.search_shortcut)
        .clicked()
    {
        app.toggle_search();
    }
}
