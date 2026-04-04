use egui::{Color32, Ui};

use crate::app::{GlassApp, MonitorState};

/// ステータスバー描画
pub fn draw(ui: &mut Ui, app: &GlassApp) {
    ui.horizontal(|ui| {
        let (status_text, status_color) = match app.state {
            MonitorState::Stopped => ("停止", Color32::GRAY),
            MonitorState::Running => ("受信中", Color32::GREEN),
            MonitorState::Paused => ("一時停止", Color32::YELLOW),
        };
        ui.colored_label(status_color, status_text);
        ui.separator();
        ui.label(format!("受信: {} bytes", app.buffer.byte_count()));
        ui.separator();

        let error_count = app.buffer.error_count();
        if error_count > 0 {
            ui.colored_label(Color32::RED, format!("エラー: {}", error_count));
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
