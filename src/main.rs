#![windows_subsystem = "windows"]

mod app;
mod i18n;
mod logging;
mod model;
mod protocol;
mod serial;
mod settings;
mod trigger;
mod ui;
mod util;

fn load_icon() -> eframe::egui::IconData {
    let icon_bytes = include_bytes!("../assets/icon.ico");
    match image::load_from_memory(icon_bytes) {
        Ok(img) => {
            let img = img.into_rgba8();
            let (width, height) = img.dimensions();
            eframe::egui::IconData {
                rgba: img.into_raw(),
                width,
                height,
            }
        }
        Err(e) => {
            tracing::error!(error = %e, "アイコンの読み込みに失敗しました");
            eframe::egui::IconData {
                rgba: Vec::new(),
                width: 0,
                height: 0,
            }
        }
    }
}

fn main() -> eframe::Result {
    let _log_guard = logging::init();

    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1024.0, 600.0])
            .with_min_inner_size(app::MIN_WINDOW_SIZE)
            .with_title("Glass")
            .with_icon(std::sync::Arc::new(load_icon())),
        ..Default::default()
    };
    let result = eframe::run_native(
        "Glass",
        options,
        Box::new(|cc| Ok(Box::new(app::GlassApp::new(cc)))),
    );
    if let Err(e) = &result {
        tracing::error!(error = %e, "eframe::run_native が異常終了しました");
    }
    result
}
