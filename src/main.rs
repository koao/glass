#![windows_subsystem = "windows"]

mod app;
mod model;
mod serial;
mod settings;
mod ui;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1024.0, 600.0])
            .with_title("Glass"),
        ..Default::default()
    };
    eframe::run_native("Glass", options, Box::new(|cc| Ok(Box::new(app::GlassApp::new(cc)))))
}
