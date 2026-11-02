use std::collections::BTreeMap;

use eframe::egui::{self, Button, RichText, Sense, Stroke, TextEdit, Ui};
use egui_extras::{Column, TableBuilder};

use crate::app::{App, Confirm, ImportQueue};
use crate::core::flags::{self, FlagError};
use crate::i18n::{fmt, tr, Lang};
use crate::theme;

/// (RU, EN, flag, value)
const PRESETS: [(&str, &str, &str, &str); 16] = [
    ("FPS 60 (сток)", "FPS 60 (stock)", "DFIntTaskSchedulerTargetFps", "60"),
    ("FPS 144", "FPS 144", "DFIntTaskSchedulerTargetFps", "144"),
    ("FPS 240", "FPS 240", "DFIntTaskSchedulerTargetFps", "240"),
    ("Свет: Future", "Light: Future", "FFlagDebugForceFutureIsBrightPhase3", "true"),
    ("Свет: ShadowMap", "Light: ShadowMap", "FFlagDebugForceFutureIsBrightPhase2", "true"),
    ("Свет: Voxel (быстро)", "Light: Voxel (fast)", "DFFlagDebugRenderForceTechnologyVoxel", "true"),
    ("Без блюма", "No bloom", "FFlagRenderNoLowFrmBloom", "true"),
    ("Без пост-эффектов", "No post FX", "FFlagDisablePostFx", "true"),
    ("MSAA x4", "MSAA x4", "FIntDebugForceMSAASamples", "4"),
    ("MSAA x8", "MSAA x8", "FIntDebugForceMSAASamples", "8"),
    ("Детали далеко", "Far details", "DFIntCSGLevelOfDetailSwitchingDistance", "5000"),
    ("Без травы", "No grass", "FIntFRMMaxGrassDistance", "0"),
    ("Дальний зум", "Far zoom", "FIntCameraMaxZoomDistance", "1000"),
    ("Без DPI-скейла", "No DPI scale", "DFFlagDisableDPIScale", "true"),
    ("Рендер: Vulkan", "Render: Vulkan", "FFlagDebugGraphicsPreferVulkan", "true"),
    ("Пинг в лог", "Ping to log", "DFFlagDebugPrintDataPingBreakDown", "true"),
];

fn outline_btn(label: &str) -> Button<'static> {
    Button::new(RichText::new(label.to_owned()).size(12.0).color(theme::FG))
        .stroke(Stroke::new(1.0, theme::LINE))
        .corner_radius(theme::R_SOFT)
        .min_size(egui::vec2(0.0, 30.0))
}

fn primary_btn(label: &str) -> Button<'static> {
    Button::new(RichText::new(label.to_owned()).size(12.0).color(theme::BG).strong())
        .fill(theme::FG)
        .corner_radius(theme::R_SOFT)
        .min_size(egui::vec2(0.0, 30.0))
}

fn msg(app: &mut App, lang: Lang, key: &'static str, args: &[&str], error: bool) {
    app.flag_msg = Some((fmt(lang, key, args), error));
}

fn set_flag(flags: &mut Vec<(String, String)>, name: &str, value: &str) {
    match flags.iter_mut().find(|(n, _)| n == name) {
        Some(row) => row.1 = value.to_string(),
        None => flags.push((name.to_string(), value.to_string())),
    }
}

fn toggle_preset(app: &mut App, flag: &str, value: &str) {
    let pos = app
        .flags
        .iter()
        .position(|(n, v)| n.trim().eq_ignore_ascii_case(flag) && v.trim() == value);
    match pos {
        Some(i) => {
            app.flags.remove(i);
            if let Some(sel) = app.flags_selected {
                if sel == i {
                    app.flags_selected = None;
                } else if sel > i {
                    app.flags_selected = Some(sel - 1);
                }
            }
        }
        None => match app
            .flags
            .iter_mut()
            .find(|(n, _)| n.trim().eq_ignore_ascii_case(flag))
        {
            Some(row) => row.1 = value.to_string(),
            None => app.flags.push((flag.to_string(), value.to_string())),
        },
    }
    app.flags_dirty = true;
}

fn error_text(lang: Lang, name: &str, detail: &str, err: FlagError) -> String {
    let info = flags::lookup(name);
    let hint = info.map(flags::hint).unwrap_or_default();
    let min = info
        .and_then(|i| i.min)
        .map(|m| m.to_string())
        .unwrap_or_default();
    let max = info
        .and_then(|i| i.max)
        .map(|m| m.to_string())
        .unwrap_or_default();
    let allowed = info
        .map(|i| i.allowed.join(", "))
        .unwrap_or_default();
    let text = match err {
        FlagError::NotBoolean => fmt(lang, "FlagErr_NotBoolFmt", &[detail]),
        FlagError::NotInteger => fmt(lang, "FlagErr_NotIntFmt", &[detail]),
        FlagError::BelowMin => fmt(lang, "FlagErr_MinFmt", &[detail, &min]),
        FlagError::AboveMax => fmt(lang, "FlagErr_MaxFmt", &[detail, &max]),
        FlagError::NotAllowed => fmt(lang, "FlagErr_AllowedFmt", &[detail, &allowed]),
    };
    if hint.is_empty() {
        text
    } else {
        format!("{text}  ({hint})")
    }
}

fn version_dir(app: &App, guid: &str) -> std::path::PathBuf {
    app.config.base().join("Versions").join(guid)
}

/// Наложение импорта: новые — сразу, совпадающие — пропуск,
/// конфликты — в очередь окна спроса.
#[cfg_attr(not(windows), allow(dead_code))]
fn apply_import(app: &mut App, lang: Lang, text: &str) {
    let incoming = match flags::parse_import(text) {
        Ok(v) => v,
        Err(e) => {
            msg(
                app,
                lang,
                "Flags_ImportErr",
                &[&e.to_string()],
                true,
            );
            return;
        }
    };
    if incoming.is_empty() {
        msg(app, lang, "Flags_EmptyImport", &[], false);
        return;
    }

    let mut conflicts = std::collections::VecDeque::new();
    let mut applied = 0usize;
    for (k, v) in incoming {
        match app.flags.iter().position(|(n, _)| *n == k) {
            None => {
                app.flags.push((k, v));
                applied += 1;
            }
            Some(i) => {
                if app.flags[i].1 != v {
                    conflicts.push_back((k, v));
                }
            }
        }
    }

    if conflicts.is_empty() {
        app.flags_dirty = true;
        msg(app, lang, "Flags_Imported", &[&applied.to_string(), "0"], false);
    } else {
        app.import = Some(ImportQueue {
            conflicts,
            replace_all: false,
            keep_rest: false,
            applied,
            skipped: 0,
        });
    }
}

/// Дожим очереди конфликтов (после решения или при пустой очереди).
fn drain_import(app: &mut App, lang: Lang) {
    let mut finished: Option<(usize, usize)> = None;
    if let Some(q) = &mut app.import {
        loop {
            if q.keep_rest {
                q.skipped += q.conflicts.len();
                q.conflicts.clear();
            } else if q.replace_all {
                match q.conflicts.pop_front() {
                    Some((n, v)) => {
                        set_flag(&mut app.flags, &n, &v);
                        q.applied += 1;
                        continue;
                    }
                    None => {}
                }
            }
            if q.conflicts.is_empty() {
                finished = Some((q.applied, q.skipped));
            }
            break;
        }
    }
    if let Some((a, s)) = finished {
        app.import = None;
        app.flags_dirty = true;
        msg(
            app,
            lang,
            "Flags_Imported",
            &[&a.to_string(), &s.to_string()],
            false,
        );
    }
}

#[cfg(windows)]
fn clipboard_json() -> Option<String> {
    let mut c = arboard::Clipboard::new().ok()?;
    let t = c.get_text().ok()?;
    let trimmed = t.trim().to_string();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

#[cfg(windows)]
fn pick_import_file(lang: Lang) -> Option<String> {
    let path = rfd::FileDialog::new()
        .set_title(tr(lang, "Dlg_FlagsImport"))
        .add_filter("JSON", &["json"])
        .add_filter(tr(lang, "Dlg_AllFiles"), &["*"])
        .pick_file()?;
    std::fs::read_to_string(path).ok()
}

#[cfg(windows)]
fn pick_export_file() -> Option<std::path::PathBuf> {
    rfd::FileDialog::new()
        .set_title("Export")
        .add_filter("JSON", &["json"])
        .set_file_name("fastflags.json")
        .save_file()
}

pub fn show(ui: &mut Ui, app: &mut App) {
    let lang = app.lang;
    ui.add_space(6.0);
    ui.label(
        RichText::new(tr(lang, "Flags_H1"))
            .size(23.0)
            .family(egui::FontFamily::Name("heading".into()))
            .color(theme::FG),
    );
    ui.label(RichText::new(tr(lang, "Flags_Sub")).color(theme::FG_DIM));
    ui.add_space(14.0);

    let Some(target) = app.installed.clone() else {
        ui.label(RichText::new(tr(lang, "Home_Status_NoClient")).color(theme::FG_DIMMER));
        return;
    };

    // (Пере)загрузка таблицы при смене активной версии.
    if app.flags_loaded_for.as_ref() != Some(&target) {
        let dir = version_dir(app, &target);
        app.flags = flags::load(&dir).into_iter().collect();
        app.flags_loaded_for = Some(target.clone());
        app.flags_dirty = false;
        app.flags_selected = None;
    }

    let dir = version_dir(app, &target);

    // ===== Окна подтверждений =====
    let mut answer: Option<bool> = None;
    if let Some(c) = &app.confirm {
        let text = match c {
            Confirm::ClearAll => tr(lang, "Flags_ClearConfirm").to_string(),
            Confirm::ApplyProfile(n) => fmt(lang, "Prof_ApplyConfirm", &[n]),
            Confirm::DeleteProfile(n) => fmt(lang, "Prof_DelConfirm", &[n]),
        };
        egui::Window::new("ByteStrap")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ui.ctx(), |ui| {
                ui.set_min_width(320.0);
                ui.label(RichText::new(text).size(13.0).color(theme::FG));
                ui.add_space(10.0);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button(tr(lang, "Btn_No")).clicked() {
                        answer = Some(false);
                    }
                    if ui
                        .add(primary_btn(tr(lang, "Btn_Yes")))
                        .clicked()
                    {
                        answer = Some(true);
                    }
                });
            });
    }

    // ===== Окно конфликта импорта =====
    enum Decision {
        Keep,
        Replace,
        ReplaceAll,
        Cancel,
    }
    let mut decision: Option<Decision> = None;
    if let Some(q) = &app.import {
        if let Some((name, incoming)) = q.conflicts.front() {
            let old = app
                .flags
                .iter()
                .find(|(n, _)| n == name)
                .map(|(_, v)| v.clone())
                .unwrap_or_default();
            let body = fmt(
                lang,
                "Flags_ConflictMsg",
                &[name.as_str(), old.as_str(), incoming.as_str()],
            );
            egui::Window::new(tr(lang, "Flags_ConflictTitle"))
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, -40.0])
                .show(ui.ctx(), |ui| {
                    ui.set_min_width(380.0);
                    ui.label(RichText::new(body).size(13.0).color(theme::FG));
                    ui.add_space(12.0);
                    ui.horizontal_wrapped(|ui| {
                        if ui
                            .add(primary_btn(tr(lang, "Flags_ConflictReplaceAll")))
                            .clicked()
                        {
                            decision = Some(Decision::ReplaceAll);
                        }
                        if ui
                            .add(primary_btn(tr(lang, "Flags_ConflictReplace")))
                            .clicked()
                        {
                            decision = Some(Decision::Replace);
                        }
                        if ui
                            .add(outline_btn(tr(lang, "Flags_ConflictKeep")))
                            .clicked()
                        {
                            decision = Some(Decision::Keep);
                        }
                        if ui
                            .add(outline_btn(tr(lang, "Btn_Cancel")))
                            .clicked()
                        {
                            decision = Some(Decision::Cancel);
                        }
                    });
                });
        }
    }

    // ===== Тулбар =====
    ui.horizontal(|ui| {
        ui.add(
            TextEdit::singleline(&mut app.flags_search)
                .hint_text(tr(lang, "Flags_Search"))
                .desired_width(220.0)
                .margin(egui::vec2(8.0, 5.0)),
        );
        if ui.add(outline_btn(tr(lang, "Flags_Add"))).clicked() {
            app.flags.push(("NewFlag".to_string(), "true".to_string()));
            app.flags_selected = Some(app.flags.len() - 1);
            app.flags_dirty = true;
            app.flag_msg = None;
        }

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.add(outline_btn(tr(lang, "Flags_Clear"))).clicked() {
                if !app.flags.is_empty() {
                    app.confirm = Some(Confirm::ClearAll);
                }
            }
            ui.add_space(6.0);
            let can_delete = app.flags_selected.is_some();
            if ui
                .add_enabled(can_delete, outline_btn(tr(lang, "Flags_Delete")))
                .clicked()
            {
                if let Some(i) = app.flags_selected.take() {
                    if i < app.flags.len() {
                        app.flags.remove(i);
                        app.flags_dirty = true;
                    }
                }
            }
            ui.add_space(6.0);

            #[cfg(windows)]
            if ui.add(outline_btn(tr(lang, "Flags_Export"))).clicked() {
                let json = flags::export_json(&app.flags);
                let count = app.flags.len().to_string();
                if let Some(path) = pick_export_file() {
                    match std::fs::write(path, json) {
                        Ok(()) => msg(app, lang, "Flags_ExportedFmt", &[&count], false),
                        Err(e) => msg(app, lang, "Flags_ExportErr", &[&e.to_string()], true),
                    }
                }
            }
            #[cfg(windows)]
            if ui.add(outline_btn(tr(lang, "Flags_Paste"))).clicked() {
                match clipboard_json() {
                    Some(t) => apply_import(app, lang, &t),
                    None => msg(app, lang, "Flags_NoClipboard", &[], false),
                }
            }
            #[cfg(windows)]
            if ui.add(outline_btn(tr(lang, "Flags_Import"))).clicked() {
                if let Some(text) = pick_import_file(lang) {
                    apply_import(app, lang, &text);
                }
            }
            ui.add_space(6.0);

            if app.flags_dirty {
                let (r, _) = ui.allocate_exact_size(egui::vec2(8.0, 8.0), Sense::hover());
                ui.painter()
                    .circle_filled(r.left_center(), 3.0, theme::BAD);
            }
            let save = if app.flags_dirty {
                primary_btn(tr(lang, "Flags_Save"))
            } else {
                outline_btn(tr(lang, "Flags_Save"))
            };
            if ui
                .add_enabled(app.flags_dirty, save.min_size(egui::vec2(110.0, 30.0)))
                .clicked()
            {
                let map: BTreeMap<String, String> = app.flags.iter().cloned().collect();
                match flags::save(&dir, &map) {
                    Ok(()) => {
                        app.flags_dirty = false;
                        if !app.config.active_flag_profile.is_empty() {
                            app.config.active_flag_profile.clear();
                            app.config.save();
                        }
                        msg(app, lang, "Flags_Saved", &[], false);
                    }
                    Err(e) => msg(app, lang, "Flags_SaveErr", &[&e.to_string()], true),
                }
            }
        });
    });

    // ===== Сообщение =====
    if let Some((text, error)) = &app.flag_msg {
        let color = if *error { theme::BAD } else { theme::GOOD };
        ui.add_space(4.0);
        ui.label(RichText::new(text).size(12.0).family(egui::FontFamily::Monospace).color(color));
    }
    ui.add_space(8.0);

    // ===== Пресеты =====
    ui.label(
        RichText::new(tr(lang, "Flags_Presets"))
            .size(11.0)
            .family(egui::FontFamily::Monospace)
            .color(theme::FG_DIMMER),
    );
    ui.add_space(4.0);
    ui.horizontal_wrapped(|ui| {
        for (ru, en, flag, value) in PRESETS {
            let label = if lang == Lang::Ru { ru } else { en };
            let active = app.flags.iter().any(|(n, v)| {
                n.trim().eq_ignore_ascii_case(flag) && v.trim() == value
            });
            let btn = if active {
                primary_btn(label).min_size(egui::vec2(0.0, 28.0))
            } else {
                outline_btn(label).min_size(egui::vec2(0.0, 28.0))
            };
            if ui.add(btn).clicked() {
                toggle_preset(app, flag, value);
                app.flag_msg = None;
            }
        }
    });
    ui.add_space(10.0);

    // ===== Таблица =====
    let query = app.flags_search.trim().to_lowercase();
    let idxs: Vec<usize> = app
        .flags
        .iter()
        .enumerate()
        .filter(|(_, (n, _))| query.is_empty() || n.to_lowercase().contains(&query))
        .map(|(i, _)| i)
        .collect();

    crate::theme::card_frame().show(ui, |ui| {
        if app.flags.is_empty() {
            ui.label(
                RichText::new(tr(lang, "Empty_Flags")).color(theme::FG_DIMMER),
            );
            return;
        }
        if idxs.is_empty() {
            ui.label(RichText::new(tr(lang, "Search_Empty")).color(theme::FG_DIMMER));
            return;
        }

        TableBuilder::new(ui)
            .vscroll(false)
            .striped(true)
            .column(Column::exact(300.0))
            .column(Column::exact(180.0))
            .column(Column::remainder())
            .header(22.0, |mut header| {
                for key in ["Flags_ColName", "Flags_ColValue", "Flags_ColDesc"] {
                    header.col(|ui| {
                        ui.label(
                            RichText::new(tr(lang, key))
                                .size(11.0)
                                .family(egui::FontFamily::Monospace)
                                .color(theme::FG_DIMMER),
                        );
                    });
                }
            })
            .body(|mut body| {
                for &i in &idxs {
                    let is_selected = app.flags_selected == Some(i);
                    body.row(26.0, |mut row| {
                        row.set_selected(is_selected);
                        row.col(|ui| {
                            let mut name = app.flags[i].0.clone();
                            let resp = ui.add(
                                TextEdit::singleline(&mut name)
                                    .desired_width(f32::INFINITY)
                                    .frame(egui::Frame::NONE)
                                    .margin(egui::vec2(4.0, 2.0)),
                            );
                            if resp.changed() {
                                app.flags[i].0 = name;
                                app.flags_dirty = true;
                            }
                            if resp.has_focus() {
                                app.flags_selected = Some(i);
                            }
                        });
                        row.col(|ui| {
                            let old = app.flags[i].1.clone();
                            let mut value = old.clone();
                            let resp = ui.add(
                                TextEdit::singleline(&mut value)
                                    .desired_width(f32::INFINITY)
                                    .frame(egui::Frame::NONE)
                                    .margin(egui::vec2(4.0, 2.0)),
                            );
                            if resp.changed() {
                                let name = app.flags[i].0.trim().to_string();
                                match flags::validate(&name, value.trim()) {
                                    Ok(()) => {
                                        app.flags[i].1 = value;
                                        app.flags_dirty = true;
                                        if matches!(app.flag_msg, Some((_, true))) {
                                            app.flag_msg = None;
                                        }
                                    }
                                    Err(e) => {
                                        let detail = value.trim().to_string();
                                        let text = error_text(lang, &name, &detail, e);
                                        app.flag_msg = Some((text, true));
                                    }
                                }
                            }
                            if resp.has_focus() {
                                app.flags_selected = Some(i);
                            }
                        });
                        row.col(|ui| {
                            let name = app.flags[i].0.trim();
                            let desc = flags::lookup(name)
                                .map(|f| format!("{} · {}", f.category, f.desc))
                                .unwrap_or_default();
                            ui.label(
                                RichText::new(desc)
                                    .size(11.0)
                                    .family(egui::FontFamily::Monospace)
                                    .color(theme::FG_DIMMER),
                            );
                        });
                    });
                }
            });
    });

    // Описание выделенного флага
    if let Some(i) = app.flags_selected {
        if let Some((name, _)) = app.flags.get(i) {
            if let Some(info) = flags::lookup(name.trim()) {
                ui.add_space(6.0);
                ui.label(
                    RichText::new(format!(
                        "{} · {} · {}",
                        info.category,
                        flags::hint(info),
                        info.desc
                    ))
                    .size(11.0)
                    .family(egui::FontFamily::Monospace)
                    .color(theme::FG_DIM),
                );
            }
        }
    }

    ui.add_space(12.0);

    // ===== Профили =====
    ui.label(
        RichText::new(tr(lang, "Flags_Profiles"))
            .size(11.0)
            .family(egui::FontFamily::Monospace)
            .color(theme::FG_DIMMER),
    );
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.add(
            TextEdit::singleline(&mut app.flag_profile_name)
                .hint_text(tr(lang, "Flags_ProfileName"))
                .desired_width(220.0)
                .margin(egui::vec2(8.0, 5.0)),
        );
        if ui
            .add(outline_btn(tr(lang, "Flags_DeleteProfile")))
            .clicked()
        {
            let name = app.flag_profile_name.trim().to_string();
            if name.is_empty() {
                msg(app, lang, "Prof_NeedName", &[], false);
            } else {
                let profiles = flags::load_profiles(&app.config.base());
                if profiles.profiles.contains_key(&name) {
                    app.confirm = Some(Confirm::DeleteProfile(name));
                } else {
                    msg(app, lang, "Prof_NoSuchFmt", &[&name], false);
                }
            }
        }
        if ui
            .add(primary_btn(tr(lang, "Flags_SaveProfile")))
            .clicked()
        {
            let name = app.flag_profile_name.trim().to_string();
            if name.is_empty() {
                msg(app, lang, "Prof_NeedName", &[], false);
            } else if app.flags.is_empty() {
                msg(app, lang, "Prof_EmptyTable", &[], false);
            } else {
                let mut profiles = flags::load_profiles(&app.config.base());
                profiles
                    .profiles
                    .insert(name.clone(), flags::flags_to_profile(&app.flags));
                match flags::save_profiles(&app.config.base(), &profiles) {
                    Ok(()) => {
                        app.flag_profile_name.clear();
                        msg(app, lang, "Prof_SavedFmt", &[&name], false);
                    }
                    Err(e) => msg(app, lang, "Flags_SaveErr", &[&e.to_string()], true),
                }
            }
        }
    });
    ui.add_space(6.0);

    let profiles = flags::load_profiles(&app.config.base());
    if profiles.profiles.is_empty() {
        ui.label(
            RichText::new(tr(lang, "Flags_ProfilesEmpty")).color(theme::FG_DIMMER),
        );
    } else {
        ui.horizontal_wrapped(|ui| {
            let names: Vec<String> = profiles.profiles.keys().cloned().collect();
            for name in names {
                let active = app.config.active_flag_profile.eq_ignore_ascii_case(&name);
                let btn = if active {
                    primary_btn(&name).min_size(egui::vec2(0.0, 28.0))
                } else {
                    outline_btn(&name).min_size(egui::vec2(0.0, 28.0))
                };
                if ui.add(btn).clicked() {
                    if app.flags.is_empty() {
                        apply_profile(app, lang, &name, &profiles, &dir);
                    } else {
                        app.confirm = Some(Confirm::ApplyProfile(name));
                    }
                }
            }
        });
    }

    // ===== Обработка решений =====
    if let Some(yes) = answer {
        let confirm = app.confirm.take();
        if yes {
            if let Some(c) = confirm {
                match c {
                    Confirm::ClearAll => {
                        app.flags.clear();
                        app.flags_selected = None;
                        app.flags_dirty = true;
                        msg(app, lang, "Flags_Cleared", &[], false);
                    }
                    Confirm::ApplyProfile(name) => {
                        let profiles = flags::load_profiles(&app.config.base());
                        apply_profile(app, lang, &name, &profiles, &dir);
                    }
                    Confirm::DeleteProfile(name) => {
                        let mut profiles = flags::load_profiles(&app.config.base());
                        profiles.profiles.remove(&name);
                        match flags::save_profiles(&app.config.base(), &profiles) {
                            Ok(()) => {
                                if app.flag_profile_name.trim().eq_ignore_ascii_case(&name) {
                                    app.flag_profile_name.clear();
                                }
                                if app
                                    .config
                                    .active_flag_profile
                                    .eq_ignore_ascii_case(&name)
                                {
                                    app.config.active_flag_profile.clear();
                                    app.config.save();
                                }
                                msg(app, lang, "Prof_DeletedFmt", &[&name], false);
                            }
                            Err(e) => msg(app, lang, "Flags_SaveErr", &[&e.to_string()], true),
                        }
                    }
                }
            }
        }
    }

    if let Some(d) = decision {
        match d {
            Decision::Keep => {
                if let Some(q) = &mut app.import {
                    q.keep_rest = true;
                }
            }
            Decision::Replace => {
                if let Some(q) = &mut app.import {
                    if let Some((n, v)) = q.conflicts.pop_front() {
                        set_flag(&mut app.flags, &n, &v);
                        q.applied += 1;
                    }
                }
            }
            Decision::ReplaceAll => {
                if let Some(q) = &mut app.import {
                    q.replace_all = true;
                    if let Some((n, v)) = q.conflicts.pop_front() {
                        set_flag(&mut app.flags, &n, &v);
                        q.applied += 1;
                    }
                }
            }
            Decision::Cancel => {
                app.import = None;
            }
        }
    }
    drain_import(app, lang);
}

/// Применить профиль: таблица + сразу в ClientAppSettings.
fn apply_profile(
    app: &mut App,
    lang: Lang,
    name: &str,
    profiles: &flags::Profiles,
    dir: &std::path::Path,
) {
    let Some(profile) = profiles.profiles.get(name) else {
        msg(app, lang, "Prof_NoSuchFmt", &[name], false);
        return;
    };
    app.flags = flags::profile_to_flags(profile);
    app.flags_selected = None;
    match flags::save(
        dir,
        &app.flags.iter().cloned().collect::<BTreeMap<_, _>>(),
    ) {
        Ok(()) => {
            app.flags_dirty = false;
            if !app.config.active_flag_profile.eq_ignore_ascii_case(name) {
                app.config.active_flag_profile = name.to_string();
                app.config.save();
            }
            msg(
                app,
                lang,
                "Prof_AppliedFmt",
                &[name, &app.flags.len().to_string()],
                false,
            );
        }
        Err(e) => msg(app, lang, "Flags_SaveErr", &[&e.to_string()], true),
    }
}
