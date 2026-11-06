#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

use bytestrap::shell;
use eframe::egui;

fn main() -> eframe::Result {
    // roblox:// deep-link: открыть окно шелла и сразу запустить игру.
    let deep_link = std::env::args().skip(1).find(|a| a.starts_with("roblox://"));

    let cfg = shell::config::ShellConfig::load();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([cfg.win_w, cfg.win_h])
            .with_min_inner_size([980.0, 620.0])
            .with_decorations(false)
            .with_app_id("ByteStrapShell"),
        ..Default::default()
    };

    eframe::run_native(
        "ByteStrapShell",
        options,
        Box::new(move |cc| {
            let mut app = shell::Shell::new(cc);
            if let Some(url) = deep_link {
                app.set_deep_link(&url);
            }
            Ok(Box::new(app))
        }),
    )
}
