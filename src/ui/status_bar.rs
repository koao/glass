use egui::{Ui, RichText, CornerRadius, Stroke};
use egui_phosphor::regular;

use crate::app::{GlassApp, MonitorState};
use crate::ui::theme;

/// ステータスバー描画（表示のみ）
pub fn draw(ui: &mut Ui, app: &GlassApp) {
    ui.horizontal(|ui| {
        // 接続ステータスピル（角丸バッジ）
        draw_status_pill(ui, app);

        ui.separator();

        // 受信バイト数
        ui.label(format!("{}: {} bytes", app.t.received, app.buffer.byte_count()));
        ui.separator();

        // エラー数
        let error_count = app.buffer.error_count();
        if error_count > 0 {
            ui.colored_label(theme::STATUS_ERROR, format!("{}: {}", app.t.errors, error_count));
        } else {
            ui.label(format!("{}: 0", app.t.errors));
        }

        ui.separator();

        // シリアル設定簡易表示
        let parity_char = match app.config.parity {
            crate::serial::config::ParitySetting::None => "N",
            crate::serial::config::ParitySetting::Odd => "O",
            crate::serial::config::ParitySetting::Even => "E",
        };
        let stop = match app.config.stop_bits {
            crate::serial::config::StopBitsSetting::One => "1",
            crate::serial::config::StopBitsSetting::Two => "2",
        };
        let port = if app.config.port_name.is_empty() { app.t.unselected } else { &app.config.port_name };
        let config_text = format!(
            "{} {}bps {}{}{}", port, app.config.baud_rate, app.config.data_bits, parity_char, stop
        );
        ui.label(
            RichText::new(config_text).color(theme::TEXT_MUTED),
        );
    });
}

/// ステータスピル最小幅（テキスト変化でレイアウトが動かないよう固定）
const PILL_MIN_WIDTH: f32 = 220.0;

/// 接続ステータスピル（角丸バッジ・固定幅）
fn draw_status_pill(ui: &mut Ui, app: &GlassApp) {
    let (text, text_color, bg_color) = match app.state {
        MonitorState::Stopped => (
            format!("{} {}", regular::CIRCLE, app.t.status_stopped),
            theme::STATUS_STOPPED,
            theme::PILL_BG_STOPPED,
        ),
        MonitorState::Running => (
            format!("{} {} {}bps {}", regular::CIRCLE, app.config.port_name, app.config.baud_rate, app.t.status_receiving),
            theme::STATUS_RUNNING,
            theme::PILL_BG_RUNNING,
        ),
        MonitorState::Paused => (
            format!("{} {} {}", regular::CIRCLE, app.config.port_name, app.t.status_paused),
            theme::STATUS_PAUSED,
            theme::PILL_BG_PAUSED,
        ),
    };

    egui::Frame::new()
        .fill(bg_color)
        .corner_radius(CornerRadius::same(12))
        .stroke(Stroke::new(1.0, text_color.linear_multiply(0.3)))
        .inner_margin(egui::Margin::symmetric(12, 4))
        .show(ui, |ui| {
            ui.set_min_width(PILL_MIN_WIDTH);
            ui.set_max_width(PILL_MIN_WIDTH);
            ui.vertical_centered(|ui| {
                ui.label(RichText::new(text).color(text_color).size(13.0));
            });
        });
}
