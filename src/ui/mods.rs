use eframe::egui::{self, RichText, Ui};

use crate::app::{App, Confirm, ProfileDlg};
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
    let base = app.config.base();

    // ===== Диалоги =====
    let confirm_body: Option<String> = match &app.confirm {
        Some(Confirm::RevertMods) => Some(tr(lang, "Mods_RevertConfirm").to_string()),
        Some(Confirm::LoadModProfile(n)) => Some(fmt(lang, "Mods_LoadConfirm", &[n])),
        Some(Confirm::DeleteModProfile(n)) => Some(fmt(lang, "Prof_DelConfirm", &[n])),
        _ => None,
    };
    let mut answer: Option<bool> = None;
    if let Some(body) = confirm_body {
        egui::Window::new("ByteStrap")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ui.ctx(), |ui| {
                ui.set_min_width(360.0);
                ui.label(RichText::new(body).size(13.0).color(theme::FG));
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

    // ===== Профили =====
    ui.add_space(12.0);
    ui.label(
        RichText::new(tr(lang, "Mods_Profiles"))
            .size(11.0)
            .family(egui::FontFamily::Monospace)
            .color(theme::FG_DIMMER),
    );
    ui.add_space(4.0);

    let profiles = crate::core::mods::list_profiles(&base);

    ui.horizontal(|ui| {
        let sel_valid = profiles.iter().any(|n| n == &app.profile_sel);
        let sel_text = if sel_valid {
            app.profile_sel.clone()
        } else if !app.config.active_mod_profile.is_empty()
            && profiles
                .iter()
                .any(|n| n.eq_ignore_ascii_case(&app.config.active_mod_profile))
        {
            app.config.active_mod_profile.clone()
        } else {
            tr(lang, "Prof_Select").to_string()
        };
        egui::ComboBox::from_id_salt("mod_profile_sel")
            .selected_text(RichText::new(sel_text).size(12.0).color(theme::FG))
            .width(240.0)
            .show_ui(ui, |ui| {
                for n in &profiles {
                    if ui.selectable_label(app.profile_sel == *n, n).clicked() {
                        app.profile_sel = n.clone();
                    }
                }
            });

        if ui
            .add_enabled(sel_valid, primary_btn(tr(lang, "Prof_Load")))
            .clicked()
        {
            let name = app.profile_sel.clone();
            if app.mods_files.is_empty() {
                app.start_mod_profile_load(name);
            } else {
                app.confirm = Some(Confirm::LoadModProfile(name));
            }
        }
        if ui.add(outline_btn(tr(lang, "Prof_SaveAs"))).clicked() {
            app.mod_profile_name = if sel_valid {
                app.profile_sel.clone()
            } else {
                app.config.active_mod_profile.clone()
            };
            app.profile_dlg = Some(ProfileDlg::Mods);
        }
        if ui
            .add_enabled(sel_valid, outline_btn(tr(lang, "Prof_Delete")))
            .clicked()
        {
            app.confirm = Some(Confirm::DeleteModProfile(app.profile_sel.clone()));
        }
    });
    if profiles.is_empty() {
        ui.label(
            RichText::new(tr(lang, "Mods_ProfilesEmpty")).color(theme::FG_DIMMER),
        );
    }

    // ===== Диалог «Сохранить как…» =====
    let mut save_as: Option<String> = None;
    let mut dlg_cancel = false;
    if app.profile_dlg == Some(ProfileDlg::Mods) {
        let exists = {
            let n = app.mod_profile_name.trim();
            !n.is_empty() && profiles.iter().any(|p| p == n)
        };
        egui::Window::new("ByteStrap")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ui.ctx(), |ui| {
                ui.set_min_width(380.0);
                ui.label(
                    RichText::new(tr(lang, "Prof_SaveAsDesc"))
                        .size(13.0)
                        .color(theme::FG),
                );
                ui.add_space(8.0);
                let mut out = egui::TextEdit::singleline(&mut app.mod_profile_name)
                    .hint_text(tr(lang, "Flags_ProfileName"))
                    .desired_width(340.0)
                    .margin(egui::vec2(8.0, 5.0))
                    .show(ui);
                out.response.request_focus();
                if out.response.gained_focus() {
                    let max = egui::text::CCursor::new(out.galley.text().chars().count());
                    out.state
                        .cursor
                        .set_char_range(Some(egui::text::CCursorRange::two(
                            egui::text::CCursor::new(0),
                            max,
                        )));
                    out.state.store(ui.ctx(), out.response.id);
                }
                ui.add_space(10.0);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button(tr(lang, "Btn_Cancel")).clicked() {
                        dlg_cancel = true;
                    }
                    let label = if exists {
                        tr(lang, "Prof_Overwrite")
                    } else {
                        tr(lang, "Prof_Save")
                    };
                    if ui.add(primary_btn(label)).clicked() {
                        save_as = Some(app.mod_profile_name.trim().to_string());
                    }
                });
            });
    }
    if let Some(name) = save_as {
        if name.is_empty() {
            app.mods_msg = Some((tr(lang, "Prof_NeedName").to_string(), false));
        } else {
            match crate::core::mods::save_profile(&base, &name) {
                Ok(()) => {
                    app.profile_dlg = None;
                    app.mod_profile_name.clear();
                    app.profile_sel = name.clone();
                    app.mods_msg = Some((fmt(lang, "Prof_SavedFmt", &[&name]), false));
                }
                Err(e) => app.mods_msg = Some((e.to_string(), true)),
            }
        }
    }
    if dlg_cancel {
        app.profile_dlg = None;
        app.mod_profile_name.clear();
    }

    // ===== Обработка диалога =====
    if let Some(yes) = answer {
        let confirm = app.confirm.take();
        if yes {
            match confirm {
                Some(Confirm::RevertMods) => app.start_mods_revert(),
                Some(Confirm::LoadModProfile(name)) => app.start_mod_profile_load(name),
                Some(Confirm::DeleteModProfile(name)) => {
                    match crate::core::mods::delete_profile(&base, &name) {
                        Ok(()) => {
                            if app.mod_profile_name.trim().eq_ignore_ascii_case(&name) {
                                app.mod_profile_name.clear();
                            }
                            if app.profile_sel.eq_ignore_ascii_case(&name) {
                                app.profile_sel.clear();
                            }
                            if app.config.active_mod_profile.eq_ignore_ascii_case(&name) {
                                app.config.active_mod_profile.clear();
                                app.config.save();
                            }
                            app.mods_msg =
                                Some((fmt(lang, "Prof_DeletedFmt", &[&name]), false));
                        }
                        Err(e) => app.mods_msg = Some((e.to_string(), true)),
                    }
                }
                _ => {}
            }
        }
    }
}
