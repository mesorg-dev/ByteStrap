#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod app;
mod core;
mod i18n;
mod theme;
mod ui;

use eframe::egui;

fn main() -> eframe::Result {
    // Клик по roblox:// в системе (друзья, приватки, плейсы): открываем окно
    // и сразу запускаем игру с этой ссылкой (счётчик сессии и RP работают).
    let deep_link = std::env::args().skip(1).find(|a| a.starts_with("roblox://"));
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
        Box::new(move |cc| {
            let mut app = app::App::new(cc);
            if let Some(url) = deep_link {
                app.play_arg(Some(&url));
            }
            Ok(Box::new(app))
        }),
    )
}
