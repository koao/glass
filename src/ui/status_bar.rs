use egui::Ui;

use crate::app::{GlassApp, MonitorState};
use crate::ui::theme;

/// ステータスバー描画
pub fn draw(ui: &mut Ui, app: &GlassApp) {
    ui.horizontal(|ui| {
        let (status_text, status_color) = match app.state {
            MonitorState::Stopped => ("停止", theme::STATUS_STOPPED),
            MonitorState::Running => ("受信中", theme::STATUS_RUNNING),
            MonitorState::Paused => ("一時停止", theme::STATUS_PAUSED),
        };
        ui.colored_label(status_color, status_text);
        ui.separator();
        ui.label(format!("受信: {} bytes", app.buffer.byte_count()));
        ui.separator();

        let error_count = app.buffer.error_count();
        if error_count > 0 {
            ui.colored_label(theme::STATUS_ERROR, format!("エラー: {}", error_count));
        } else {
            ui.label("エラー: 0");
        }

        if !app.config.port_name.is_empty() && app.state != MonitorState::Stopped {
            ui.separator();
            ui.label(format!(
                "{} @ {}bps",
                app.config.port_name, app.config.baud_rate
            ));
        }
    });
}
