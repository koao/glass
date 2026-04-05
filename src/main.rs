#![windows_subsystem = "windows"]

mod app;
mod i18n;
mod model;
mod protocol;
mod serial;
mod settings;
mod ui;

fn load_icon() -> eframe::egui::IconData {
    let icon_bytes = include_bytes!("../assets/icon.ico");
    let img = image::load_from_memory(icon_bytes)
        .expect("アイコンの読み込みに失敗しました")
        .into_rgba8();
    let (width, height) = img.dimensions();
    eframe::egui::IconData {
        rgba: img.into_raw(),
        width,
        height,
    }
}

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1024.0, 600.0])
            .with_min_inner_size(app::MIN_WINDOW_SIZE)
            .with_title("Glass")
            .with_icon(std::sync::Arc::new(load_icon())),
        ..Default::default()
    };
    eframe::run_native("Glass", options, Box::new(|cc| Ok(Box::new(app::GlassApp::new(cc)))))
}
