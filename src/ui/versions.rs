use eframe::egui::{self, Button, RichText, Sense, Stroke, Ui};
use egui_extras::{Column, TableBuilder};

use crate::app::App;
use crate::core::versions;
use crate::i18n::{fmt, tr};

pub fn show(ui: &mut Ui, app: &mut App) {
    let lang = app.lang;
    ui.add_space(6.0);
    ui.label(
        RichText::new(tr(lang, "Ver_H1"))
            .size(23.0)
            .family(egui::FontFamily::Name("heading".into()))
            .color(crate::theme::FG),
    );
    ui.label(RichText::new(tr(lang, "Ver_Sub")).color(crate::theme::FG_DIM));
    ui.add_space(14.0);

    let installed = app.installed.clone();
    let selected = app.selected_version.clone();
    let busy = app.install.active;
    let list = versions::list(&app.config.base());

    // Тулбар
    ui.horizontal(|ui| {
        let can_manage = !busy && selected.is_some();
        let is_active = selected.as_deref() == installed.as_deref();

        let activate = Button::new(
            RichText::new(tr(lang, "Ver_Activate"))
                .color(crate::theme::BG)
                .strong(),
        )
        .fill(crate::theme::FG)
        .corner_radius(crate::theme::R_SOFT)
        .min_size(egui::vec2(130.0, 34.0));
        if ui
            .add_enabled(can_manage && !is_active, activate)
            .clicked()
        {
            if let Some(g) = &selected {
                app.activate_version(g);
            }
        }

        ui.add_space(8.0);
        let delete = Button::new(RichText::new(tr(lang, "Ver_Delete")))
            .stroke(Stroke::new(1.0, crate::theme::LINE))
            .corner_radius(crate::theme::R_SOFT)
            .min_size(egui::vec2(110.0, 34.0));
        if ui
            .add_enabled(can_manage && !is_active, delete)
            .clicked()
        {
            if let Some(g) = selected.clone() {
                app.delete_version(&g);
            }
        }
    });

    ui.add_space(10.0);

    // Таблица версий
    crate::theme::card_frame().show(ui, |ui| {
        if list.is_empty() {
            ui.label(
                RichText::new(tr(lang, "Ver_Empty")).color(crate::theme::FG_DIMMER),
            );
            return;
        }
        TableBuilder::new(ui)
            .striped(true)
            .column(Column::exact(300.0))
            .column(Column::exact(110.0))
            .column(Column::remainder())
            .header(24.0, |mut header| {
                for key in ["Ver_ColGuid", "Ver_ColSize", "Ver_ColStatus"] {
                    header.col(|ui| {
                        ui.label(
                            RichText::new(tr(lang, key))
                                .size(11.0)
                                .family(egui::FontFamily::Monospace)
                                .color(crate::theme::FG_DIMMER),
                        );
                    });
                }
            })
            .body(|mut body| {
                for v in &list {
                    let is_active = Some(&v.guid) == installed.as_ref();
                    let is_selected = Some(&v.guid) == selected.as_ref();
                    body.row(30.0, |mut row| {
                        row.set_selected(is_selected);
                        row.col(|ui| {
                            let resp = ui.interact(
                                ui.max_rect(),
                                ui.scope_id().with(("ver", v.guid.clone())),
                                Sense::click(),
                            );
                            let color = if is_active {
                                crate::theme::FG
                            } else {
                                crate::theme::FG_DIM
                            };
                            ui.painter().text(
                                ui.max_rect().left_center(),
                                egui::Align2::LEFT_CENTER,
                                &v.guid,
                                egui::FontId::new(12.0, egui::FontFamily::Monospace),
                                color,
                            );
                            if resp.clicked() {
                                app.selected_version = Some(v.guid.clone());
                                app.integrity = None;
                            }
                        });
                        row.col(|ui| {
                            ui.label(
                                RichText::new(versions::format_size(v.size))
                                    .size(12.0)
                                    .family(egui::FontFamily::Monospace)
                                    .color(crate::theme::FG_DIM),
                            );
                        });
                        row.col(|ui| {
                            if is_active {
                                ui.label(
                                    RichText::new(tr(lang, "Ver_Active"))
                                        .size(12.0)
                                        .family(egui::FontFamily::Monospace)
                                        .color(crate::theme::GOOD),
                                );
                            } else if !v.has_player {
                                ui.label(
                                    RichText::new("no player")
                                        .size(12.0)
                                        .family(egui::FontFamily::Monospace)
                                        .color(crate::theme::WARN),
                                );
                            }
                        });
                    });
                }
            });
    });

    ui.add_space(12.0);

    // Целостность
    crate::theme::card_frame().show(ui, |ui| {
        ui.horizontal(|ui| {
            let (status, color) = match &app.integrity {
                None => (tr(lang, "Ver_Check").to_string(), crate::theme::FG_DIM),
                Some(r) if r.ok() => (
                    fmt(lang, "Ver_IntegrityOk", &[]),
                    crate::theme::GOOD,
                ),
                Some(r) => (
                    fmt(lang, "Ver_IntegrityIssues", &[&r.issues().to_string()]),
                    crate::theme::BAD,
                ),
            };
            ui.label(
                RichText::new(status)
                    .family(egui::FontFamily::Monospace)
                    .size(12.0)
                    .color(color),
            );

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let target = selected
                    .clone()
                    .or_else(|| installed.clone());
                let repair = Button::new(
                    RichText::new(tr(lang, "Ver_Repair"))
                        .color(crate::theme::BG)
                        .strong(),
                )
                .fill(crate::theme::FG)
                .corner_radius(crate::theme::R_SOFT)
                .min_size(egui::vec2(140.0, 34.0));
                if ui
                    .add_enabled(!busy && target.is_some(), repair)
                    .clicked()
                {
                    if let Some(g) = target {
                        app.start_repair(g);
                    }
                }
                ui.add_space(8.0);
                let check = Button::new(RichText::new(tr(lang, "Ver_Check")))
                    .stroke(Stroke::new(1.0, crate::theme::LINE))
                    .corner_radius(crate::theme::R_SOFT)
                    .min_size(egui::vec2(160.0, 34.0));
                if ui.add_enabled(!busy, check).clicked() {
                    app.check_integrity();
                }
            });
        });

        if let Some(r) = &app.integrity {
            ui.add_space(8.0);
            ui.label(
                RichText::new(format!("checked: {}", r.checked))
                    .size(11.0)
                    .family(egui::FontFamily::Monospace)
                    .color(crate::theme::FG_DIMMER),
            );
        }
    });
}
