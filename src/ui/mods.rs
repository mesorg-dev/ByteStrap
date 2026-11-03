use eframe::egui::{self, RichText, Ui};

use crate::app::{App, Confirm};
use crate::core::mods::FileStatus;
use crate::i18n::{fmt, tr};
use crate::theme;

fn outline_btn(label: &str) -> eframe::egui::Button<'static> {
    eframe::egui::Button::new(RichText::new(label.to_owned()).size(12.0).color(theme::FG))
        .stroke(eframe::egui::Stroke::new(1.0, theme::LINE))
        .corner_radius(theme::R_SOFT)
        .min_size(egui::vec2(0.0, 30.0))
}

fn primary_btn(label: &str) -> eframe::egui::Button<'static> {
    eframe::egui::Button::new(RichText::new(label.to_owned()).size(12.0).color(theme::BG).strong())
        .fill(theme::FG)
        .corner_radius(theme::R_SOFT)
        .min_size(egui::vec2(0.0, 30.0))
}

pub fn show(ui: &mut Ui, app: &mut App) {
    let lang = app.lang;
    ui.add_space(6.0);
    ui.label(
        RichText::new(tr(lang, "Mods_H1"))
            .size(23.0)
            .family(egui::FontFamily::Name("heading".into()))
            .color(theme::FG),
    );
    ui.label(RichText::new(tr(lang, "Mods_Sub")).color(theme::FG_DIM));
    ui.add_space(14.0);

    if !app.mods_scanned {
        app.refresh_mods();
    }

    // ===== Диалог отката =====
    let mut answer: Option<bool> = None;
    if let Some(Confirm::RevertMods) = &app.confirm {
        egui::Window::new("ByteStrap")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ui.ctx(), |ui| {
                ui.set_min_width(360.0);
                ui.label(
                    RichText::new(tr(lang, "Mods_RevertConfirm"))
                        .size(13.0)
                        .color(theme::FG),
                );
                ui.add_space(10.0);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button(tr(lang, "Btn_No")).clicked() {
                        answer = Some(false);
                    }
                    if ui.add(primary_btn(tr(lang, "Btn_Yes"))).clicked() {
                        answer = Some(true);
                    }
                });
            });
    }

    // ===== Действия =====
    theme::card_frame().show(ui, |ui| {
        ui.label(
            RichText::new(tr(lang, "Mods_Where")).strong().color(theme::FG),
        );
        ui.add_space(4.0);
        ui.label(
            RichText::new(app.config.base().join("Mods").to_string_lossy())
                .size(11.0)
                .family(egui::FontFamily::Monospace)
                .color(theme::FG_DIMMER),
        );
        ui.add_space(10.0);

        ui.horizontal_wrapped(|ui| {
            if ui.add(outline_btn(tr(lang, "Mods_OpenFolder"))).clicked() {
                #[cfg(windows)]
                {
                    let p = app.config.base().join("Mods");
                    let _ = std::fs::create_dir_all(&p);
                    let _ = std::process::Command::new("explorer").arg(p).spawn();
                }
            }
            if ui.add(outline_btn(tr(lang, "Mods_Refresh"))).clicked() {
                app.refresh_mods();
                app.mods_msg = None;
            }

            let has_client = app.installed.is_some();
            if ui
                .add_enabled(has_client, primary_btn(tr(lang, "Mods_Apply")))
                .clicked()
            {
                app.start_mods_apply();
            }

            let can_revert = has_client && !app.mods_files.is_empty();
            if ui
                .add_enabled(can_revert, outline_btn(tr(lang, "Mods_Revert")))
                .clicked()
            {
                app.confirm = Some(Confirm::RevertMods);
            }
        });

        if let Some((text, error)) = &app.mods_msg {
            let color = if *error { theme::BAD } else { theme::GOOD };
            ui.add_space(8.0);
            ui.label(
                RichText::new(text)
                    .size(12.0)
                    .family(egui::FontFamily::Monospace)
                    .color(color),
            );
        }
    });

    ui.add_space(12.0);

    // ===== Список файлов =====
    theme::card_frame().show(ui, |ui| {
        if app.mods_files.is_empty() {
            ui.label(RichText::new(tr(lang, "Mods_Empty")).color(theme::FG_DIMMER));
            return;
        }

        ui.label(
            RichText::new(fmt(lang, "Mods_CountFmt", &[&app.mods_files.len().to_string()]))
                .size(11.0)
                .family(egui::FontFamily::Monospace)
                .color(theme::FG_DIMMER),
        );
        ui.add_space(6.0);

        let guid = app.installed.clone().unwrap_or_default();
        let base = app.config.base();
        egui::ScrollArea::vertical()
            .max_height(340.0)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for f in &app.mods_files {
                    let status = if guid.is_empty() {
                        FileStatus::Missing
                    } else {
                        crate::core::mods::file_status(&base, &guid, &f.rel)
                    };
                    let (status_text, color) = match status {
                        FileStatus::Applied => (tr(lang, "Mods_StApplied"), theme::GOOD),
                        FileStatus::Missing => (tr(lang, "Mods_StMissing"), theme::WARN),
                        FileStatus::Clean => (tr(lang, "Mods_StClean"), theme::FG_DIMMER),
                    };
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(&f.rel)
                                .size(12.0)
                                .family(egui::FontFamily::Monospace)
                                .color(theme::FG),
                        );
                        ui.with_layout(
                            egui::Layout::right_to_left(egui::Align::Center),
                            |ui| {
                                ui.label(
                                    RichText::new(status_text)
                                        .size(11.0)
                                        .family(egui::FontFamily::Monospace)
                                        .color(color),
                                );
                                ui.label(
                                    RichText::new(format!("{} B", f.size))
                                        .size(11.0)
                                        .family(egui::FontFamily::Monospace)
                                        .color(theme::FG_DIMMER),
                                );
                            },
                        );
                    });
                }
            });
    });

    // ===== Обработка диалога =====
    if let Some(yes) = answer {
        let confirm = app.confirm.take();
        if yes && matches!(confirm, Some(Confirm::RevertMods)) {
            app.start_mods_revert();
        }
    }
}
