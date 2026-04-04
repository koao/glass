use egui::{Ui, RichText, CornerRadius, Stroke};

use crate::app::{GlassApp, MonitorState};
use crate::ui::theme;

/// ステータスバー描画（表示のみ）
pub fn draw(ui: &mut Ui, app: &GlassApp) {
    ui.horizontal(|ui| {
        // 接続ステータスピル（角丸バッジ）
        draw_status_pill(ui, app);

        ui.separator();

        // 受信バイト数
        ui.label(format!("受信: {} bytes", app.buffer.byte_count()));
        ui.separator();

        // エラー数
        let error_count = app.buffer.error_count();
        if error_count > 0 {
            ui.colored_label(theme::STATUS_ERROR, format!("エラー: {}", error_count));
        } else {
            ui.label("エラー: 0");
        }
    });
}

/// ステータスピル最小幅（テキスト変化でレイアウトが動かないよう固定）
const PILL_MIN_WIDTH: f32 = 220.0;

/// 接続ステータスピル（角丸バッジ・固定幅）
fn draw_status_pill(ui: &mut Ui, app: &GlassApp) {
    let (text, text_color, bg_color) = match app.state {
        MonitorState::Stopped => (
            "● 停止".to_string(),
            theme::STATUS_STOPPED,
            theme::PILL_BG_STOPPED,
        ),
        MonitorState::Running => (
            format!("● {} {}bps 受信中", app.config.port_name, app.config.baud_rate),
            theme::STATUS_RUNNING,
            theme::PILL_BG_RUNNING,
        ),
        MonitorState::Paused => (
            format!("● {} 一時停止", app.config.port_name),
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
