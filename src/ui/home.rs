use eframe::egui::{
    self, Align, Button, Frame, Layout, ProgressBar, RichText, Sense, Stroke, Ui, Vec2,
};

use crate::app::App;
use crate::core::deploy::Phase;
use crate::i18n::{fmt, tr};

fn h1(ui: &mut Ui, text: &str) {
    ui.add_space(6.0);
    ui.label(
        RichText::new(text)
            .size(23.0)
            .family(egui::FontFamily::Name("heading".into()))
            .color(crate::theme::FG),
    );
}

pub fn show(ui: &mut Ui, app: &mut App) {
    let lang = app.lang;
    h1(ui, tr(lang, "Home_H1"));
    ui.label(RichText::new(tr(lang, "Home_Sub")).color(crate::theme::FG_DIM));
    ui.add_space(14.0);

    let busy = app.install.active;
    let status_text = if busy {
        app.status.clone()
    } else if let Some(err) = &app.install.error {
        fmt(lang, "Home_Status_Error", &[err])
    } else if app.installed.is_none() {
        tr(lang, "Home_Status_NoClient").to_string()
    } else if app.update_available() {
        match &app.latest {
            Some(v) => fmt(lang, "Home_Status_Update", &[&v.version]),
            None => tr(lang, "Home_Status_Ready").to_string(),
        }
    } else {
        tr(lang, "Home_Status_Ready").to_string()
    };

    let dot_color = if busy {
        crate::theme::WARN
    } else if app.install.error.is_some() {
        crate::theme::BAD
    } else if app.installed.is_some() {
        crate::theme::GOOD
    } else {
        crate::theme::FG_DIMMER
    };

    Frame::NONE
        .fill(crate::theme::PANEL)
        .stroke(Stroke::new(1.0, crate::theme::LINE))
        .corner_radius(crate::theme::R_CARD)
        .inner_margin(egui::Margin::same(20))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                let (r, _) = ui.allocate_exact_size(Vec2::splat(12.0), Sense::hover());
                ui.painter().circle_filled(r.center(), 6.0, dot_color);
                ui.label(
                    RichText::new(&status_text)
                        .color(crate::theme::FG_DIM)
                        .family(egui::FontFamily::Monospace)
                        .size(12.0),
                );

                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if busy {
                        let cancel = Button::new(RichText::new(tr(lang, "Btn_Cancel")).weak())
                            .stroke(Stroke::new(1.0, crate::theme::LINE))
                            .corner_radius(crate::theme::R_SOFT)
                            .min_size(egui::vec2(110.0, 44.0));
                        if ui.add(cancel).clicked() {
                            app.cancel_install();
                        }
                    } else if app.installed.is_some() && app.update_available() {
                        let update = Button::new(
                            RichText::new(tr(lang, "Home_Update"))
                                .color(crate::theme::BG)
                                .strong(),
                        )
                        .fill(crate::theme::FG)
                        .corner_radius(crate::theme::R_SOFT)
                        .min_size(egui::vec2(150.0, 44.0));
                        if ui.add(update).clicked() {
                            app.start_install(false);
                        }
                        ui.add_space(8.0);
                        let play = Button::new(RichText::new(tr(lang, "Home_Play")))
                            .stroke(Stroke::new(1.0, crate::theme::FG_DIMMER))
                            .corner_radius(crate::theme::R_SOFT)
                            .min_size(egui::vec2(150.0, 44.0));
                        if ui.add(play).clicked() {
                            app.play();
                        }
                    } else {
                        let (label, action_install) = if app.installed.is_some() {
                            (tr(lang, "Home_Play"), false)
                        } else {
                            (tr(lang, "Home_Install"), true)
                        };
                        let primary =
                            Button::new(RichText::new(label).color(crate::theme::BG).strong())
                                .fill(crate::theme::FG)
                                .corner_radius(crate::theme::R_SOFT)
                                .min_size(egui::vec2(190.0, 44.0));
                        if ui.add(primary).clicked() {
                            if action_install {
                                app.start_install(false);
                            } else {
                                app.play();
                            }
                        }
                    }
                });
            });

            if busy {
                ui.add_space(12.0);
                let frac = if app.install.total > 0 {
                    (app.install.done as f64 / app.install.total as f64).clamp(0.0, 1.0) as f32
                } else {
                    0.0
                };
                let text = if app.install.phase == Phase::Download && app.install.total > 0 {
                    let done_mb = app.install.done as f64 / 1_048_576.0;
                    let total_mb = app.install.total as f64 / 1_048_576.0;
                    Some(fmt(
                        lang,
                        "Home_MbOf",
                        &[&format!("{done_mb:.1}"), &format!("{total_mb:.0}")],
                    ))
                } else {
                    None
                };
                let bar = ProgressBar::new(frac)
                    .desired_height(6.0)
                    .fill(crate::theme::GOOD)
                    .animate(true);
                let bar = match text {
                    Some(t) => bar.text(t),
                    None => bar,
                };
                ui.add(bar);
            }

            ui.add_space(14.0);
            ui.separator();
            ui.add_space(10.0);

            let version_text = match (&app.latest, &app.installed) {
                (Some(lat), _) => fmt(lang, "Home_Version", &[&lat.version]),
                (None, Some(inst)) => fmt(lang, "Home_Version", &[inst]),
                _ => tr(lang, "Home_VersionNone").to_string(),
            };
            ui.label(
                RichText::new(version_text)
                    .family(egui::FontFamily::Monospace)
                    .size(12.0)
                    .color(crate::theme::FG_DIMMER),
            );
        });

    ui.add_space(12.0);

    let flags_count = app.flags.len().to_string();
    let entries = [("MODS", "0"), ("FLAGS", flags_count.as_str()), ("HOURS", "0")];

    Frame::NONE
        .fill(crate::theme::PANEL)
        .stroke(Stroke::new(1.0, crate::theme::LINE))
        .corner_radius(crate::theme::R_CARD)
        .inner_margin(egui::Margin::same(20))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                for (i, (label, value)) in entries.into_iter().enumerate() {
                    ui.vertical(|ui| {
                        ui.label(
                            RichText::new(value)
                                .size(26.0)
                                .family(egui::FontFamily::Name("heading".into()))
                                .color(crate::theme::FG),
                        );
                        ui.label(
                            RichText::new(label)
                                .size(11.0)
                                .family(egui::FontFamily::Monospace)
                                .color(crate::theme::FG_DIMMER),
                        );
                    });
                    if i < 2 {
                        ui.add_space(48.0);
                    }
                }
            });
        });
}
