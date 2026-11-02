#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod app;
mod core;
mod i18n;
mod theme;
mod ui;

use eframe::egui;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1120.0, 700.0])
            .with_min_inner_size([980.0, 620.0])
            .with_decorations(false)
            .with_app_id("ByteStrap"),
        ..Default::default()
    };
    eframe::run_native(
        "ByteStrap",
        options,
        Box::new(|cc| Ok(Box::new(app::App::new(cc)))),
    )
}
