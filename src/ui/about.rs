use eframe::egui::{self, Button, RichText, Stroke, Ui};

use crate::app::{App, UpdateUi};
use crate::i18n::{fmt, tr, Lang};
use crate::theme;

pub fn show(ui: &mut Ui, app: &mut App) {
    let lang = app.lang;
    ui.add_space(6.0);
    ui.label(
        RichText::new(tr(lang, "About_H1"))
            .size(23.0)
            .family(egui::FontFamily::Name("heading".into()))
            .color(theme::FG),
    );
    ui.label(RichText::new(tr(lang, "About_Sub")).color(theme::FG_DIM));
    ui.add_space(14.0);

    theme::card_frame().show(ui, |ui| {
        ui.label(
            RichText::new(format!(
                "{}: {}",
                tr(lang, "About_Version"),
                crate::core::updater::current_version()
            ))
            .family(egui::FontFamily::Monospace)
            .color(theme::FG),
        );
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new(tr(lang, "About_Repo")).color(theme::FG_DIM));
            ui.hyperlink_to(
                "mesorg-dev/ByteStrap",
                "https://github.com/mesorg-dev/ByteStrap",
            );
        });
        ui.horizontal(|ui| {
            ui.label(RichText::new(tr(lang, "About_License")).color(theme::FG_DIM));
            ui.label("GPL-3.0");
        });
    });

    ui.add_space(12.0);

    theme::card_frame().show(ui, |ui| {
        ui.label(
            RichText::new(tr(lang, "About_Lang"))
                .strong()
                .color(theme::FG),
        );
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            for (code, label) in [("ru", "Русский"), ("en", "English")] {
                let selected = lang.code() == code;
                let text = if selected {
                    RichText::new(label).color(theme::BG).strong()
                } else {
                    RichText::new(label).color(theme::FG)
                };
                let mut btn = Button::new(text);
                if selected {
                    btn = btn.fill(theme::FG);
                } else {
                    btn = btn.stroke(Stroke::new(1.0, theme::LINE));
                }
                if ui.add(btn).clicked() {
                    app.set_lang(Lang::resolve(code));
                }
            }
        });
    });

    ui.add_space(12.0);

    theme::card_frame().show(ui, |ui| {
        ui.label(
            RichText::new(tr(lang, "About_CheckUpdates"))
                .strong()
                .color(theme::FG),
        );
        ui.add_space(8.0);

        match &app.update {
            UpdateUi::Idle => {
                if ui
                    .add(
                        Button::new(
                            RichText::new(tr(lang, "About_CheckUpdates")).size(12.0),
                        )
                        .stroke(Stroke::new(1.0, theme::LINE))
                        .corner_radius(theme::R_SOFT)
                        .min_size(egui::vec2(180.0, 32.0)),
                    )
                    .clicked()
                {
                    app.check_updates();
                }
            }
            UpdateUi::Checking => {
                ui.label(
                    RichText::new(tr(lang, "About_Checking"))
                        .family(egui::FontFamily::Monospace)
                        .color(theme::FG_DIM),
                );
            }
            UpdateUi::UpToDate => {
                ui.label(
                    RichText::new(tr(lang, "About_UpToDate"))
                        .family(egui::FontFamily::Monospace)
                        .color(theme::GOOD),
                );
            }
            UpdateUi::Found(rel) => {
                ui.label(
                    RichText::new(fmt(lang, "About_UpdateAvail", &[&rel.tag]))
                        .color(theme::FG),
                );
                if !rel.notes.trim().is_empty() {
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new(rel.notes.trim())
                            .size(11.0)
                            .color(theme::FG_DIMMER),
                    );
                }
                ui.add_space(8.0);
                if ui
                    .add(
                        Button::new(
                            RichText::new(tr(lang, "About_Download"))
                                .size(12.0)
                                .color(theme::BG)
                                .strong(),
                        )
                        .fill(theme::FG)
                        .corner_radius(theme::R_SOFT)
                        .min_size(egui::vec2(180.0, 32.0)),
                    )
                    .clicked()
                {
                    app.start_update(rel.clone());
                }
            }
            UpdateUi::Downloading { done, total } => {
                let pct = if *total > 0 {
                    *done * 100 / *total
                } else {
                    0
                };
                ui.label(
                    RichText::new(fmt(lang, "About_Downloading", &[&pct.to_string()]))
                        .family(egui::FontFamily::Monospace)
                        .color(theme::FG_DIM),
                );
                ui.add_space(6.0);
                let bar = egui::ProgressBar::new(pct as f32 / 100.0)
                    .desired_width(ui.available_width())
                    .fill(theme::FG);
                ui.add(bar);
            }
            UpdateUi::Error(e) => {
                ui.label(
                    RichText::new(fmt(lang, "About_UpdateErr", &[e]))
                        .size(12.0)
                        .family(egui::FontFamily::Monospace)
                        .color(theme::BAD),
                );
                ui.add_space(6.0);
                if ui
                    .add(
                        Button::new(
                            RichText::new(tr(lang, "About_CheckUpdates")).size(12.0),
                        )
                        .stroke(Stroke::new(1.0, theme::LINE))
                        .corner_radius(theme::R_SOFT)
                        .min_size(egui::vec2(180.0, 32.0)),
                    )
                    .clicked()
                {
                    app.check_updates();
                }
            }
        }
    });
}
