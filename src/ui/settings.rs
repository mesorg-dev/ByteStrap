use eframe::egui::{self, Button, RichText, Stroke, Ui};

use crate::app::{App, AsyncEvent, Confirm};
use crate::i18n::{fmt, tr, Lang};
use crate::theme;

const CHANNELS: [&str; 3] = ["LIVE", "BETA", "ZNext"];

fn card_label(ui: &mut Ui, lang: Lang, title_key: &'static str, desc_key: &'static str) {
    ui.label(RichText::new(tr(lang, title_key)).strong().color(theme::FG));
    ui.add_space(2.0);
    ui.label(RichText::new(tr(lang, desc_key)).size(11.0).color(theme::FG_DIMMER));
    ui.add_space(8.0);
}

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

pub fn show(ui: &mut Ui, app: &mut App) {
    let lang = app.lang;
    ui.add_space(6.0);
    ui.label(
        RichText::new(tr(lang, "Settings_H1"))
            .size(23.0)
            .family(egui::FontFamily::Name("heading".into()))
            .color(theme::FG),
    );
    ui.label(RichText::new(tr(lang, "Settings_Sub")).color(theme::FG_DIM));
    ui.add_space(14.0);

    // ===== Канал Roblox =====
    let mut apply_channel: Option<String> = None;
    theme::card_frame().show(ui, |ui| {
        card_label(ui, lang, "Settings_Channel", "Settings_ChannelDesc");

        ui.horizontal(|ui| {
            for ch in CHANNELS {
                let selected = app.channel_draft.eq_ignore_ascii_case(ch);
                let text = if selected {
                    RichText::new(ch).color(theme::BG).strong()
                } else {
                    RichText::new(ch).color(theme::FG)
                };
                let mut btn = Button::new(text).min_size(egui::vec2(90.0, 30.0));
                if selected {
                    btn = btn.fill(theme::FG);
                } else {
                    btn = btn.stroke(Stroke::new(1.0, theme::LINE));
                }
                if ui.add(btn).clicked() && !selected {
                    app.channel_draft = ch.to_string();
                    app.channel_preview = None;
                    app.channel_checking = true;
                    let channel = ch.to_string();
                    let tx = app.tx.clone();
                    std::thread::spawn(move || {
                        let rt = tokio::runtime::Builder::new_multi_thread()
                            .enable_all()
                            .build()
                            .expect("tokio runtime");
                        let res = rt
                            .block_on(crate::core::deploy::latest_version(&channel))
                            .map_err(|e| e.to_string());
                        let _ = tx.send(AsyncEvent::ChannelPreview(res));
                    });
                }
            }
        });

        ui.add_space(8.0);
        match &app.channel_preview {
            None if app.channel_checking => {
                ui.label(
                    RichText::new(tr(lang, "Settings_ChannelCheck"))
                        .family(egui::FontFamily::Monospace)
                        .color(theme::FG_DIM),
                );
            }
            Some(Ok(v)) => {
                ui.label(
                    RichText::new(fmt(lang, "Settings_ChannelVer", &[&v.version]))
                        .family(egui::FontFamily::Monospace)
                        .size(12.0)
                        .color(theme::GOOD),
                );
            }
            Some(Err(e)) => {
                ui.label(
                    RichText::new(fmt(lang, "Settings_ChannelErr", &[e]))
                        .family(egui::FontFamily::Monospace)
                        .size(12.0)
                        .color(theme::BAD),
                );
            }
            _ => {}
        }

        let changed = !app.channel_draft.eq_ignore_ascii_case(&app.config.channel);
        if changed {
            ui.add_space(8.0);
            if ui
                .add(primary_btn(tr(lang, "Settings_ChannelApply")))
                .clicked()
            {
                apply_channel = Some(app.channel_draft.clone());
            }
        }
    });

    // ===== Диалог смены канала =====
    let mut answer: Option<bool> = None;
    if let Some(Confirm::ApplyChannel(ch)) = &app.confirm {
        let body = fmt(lang, "Settings_ChannelConfirm", &[ch]);
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

    ui.add_space(12.0);

    // ===== Язык =====
    theme::card_frame().show(ui, |ui| {
        ui.label(
            RichText::new(tr(lang, "Settings_Lang")).strong().color(theme::FG),
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
                let mut btn = Button::new(text).min_size(egui::vec2(120.0, 30.0));
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

    // ===== Загрузка / поведение =====
    theme::card_frame().show(ui, |ui| {
        card_label(ui, lang, "Settings_Threads", "Settings_ThreadsDesc");
        ui.horizontal(|ui| {
            let mut threads = app.config.download_threads;
            let resp = ui.add(
                egui::Slider::new(&mut threads, 1..=8)
                    .fixed_decimals(0)
                    .show_value(false),
            );
            if resp.changed() {
                app.config.download_threads = threads;
                app.config.save();
            }
            ui.label(
                RichText::new(threads.to_string())
                    .family(egui::FontFamily::Monospace)
                    .color(theme::FG_DIM),
            );
        });

        ui.add_space(10.0);
        ui.separator();
        ui.add_space(10.0);

        let mut auto = app.config.auto_update;
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(
                    RichText::new(tr(lang, "Settings_AutoUpdate"))
                        .strong()
                        .color(theme::FG),
                );
                ui.label(
                    RichText::new(tr(lang, "Settings_AutoUpdateDesc"))
                        .size(11.0)
                        .color(theme::FG_DIMMER),
                );
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.checkbox(&mut auto, "").changed() {
                    app.config.auto_update = auto;
                    app.config.save();
                }
            });
        });

        ui.add_space(8.0);
        let mut close = app.config.close_on_launch;
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(
                    RichText::new(tr(lang, "Settings_CloseOnLaunch"))
                        .strong()
                        .color(theme::FG),
                );
                ui.label(
                    RichText::new(tr(lang, "Settings_CloseOnLaunchDesc"))
                        .size(11.0)
                        .color(theme::FG_DIMMER),
                );
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.checkbox(&mut close, "").changed() {
                    app.config.close_on_launch = close;
                    app.config.save();
                }
            });
        });

        ui.add_space(8.0);
        let mut rp = app.config.discord_rp;
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(
                    RichText::new(tr(lang, "Settings_DiscordRP"))
                        .strong()
                        .color(theme::FG),
                );
                ui.label(
                    RichText::new(tr(lang, "Settings_DiscordRPDesc"))
                        .size(11.0)
                        .color(theme::FG_DIMMER),
                );
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.checkbox(&mut rp, "").changed() {
                    app.config.discord_rp = rp;
                    app.config.save();
                    app.sync_discord();
                }
            });
        });

        if app.config.discord_rp {
            // Что показывать в presence.
            ui.add_space(6.0);
            #[derive(Clone, Copy)]
            enum Rp {
                Place,
                Time,
                Brand,
            }
            let rows: [(&str, Rp, bool); 3] = [
                (
                    "Settings_DiscordRP_Place",
                    Rp::Place,
                    app.config.discord_rp_place,
                ),
                (
                    "Settings_DiscordRP_Time",
                    Rp::Time,
                    app.config.discord_rp_time,
                ),
                (
                    "Settings_DiscordRP_Brand",
                    Rp::Brand,
                    app.config.discord_rp_brand,
                ),
            ];
            ui.indent("discord_rp_opts", |ui| {
                for (key, field, mut value) in rows {
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(tr(lang, key))
                                .size(12.0)
                                .color(theme::FG),
                        );
                        ui.with_layout(
                            egui::Layout::right_to_left(egui::Align::Center),
                            |ui| {
                                if ui.checkbox(&mut value, "").changed() {
                                    match field {
                                        Rp::Place => app.config.discord_rp_place = value,
                                        Rp::Time => app.config.discord_rp_time = value,
                                        Rp::Brand => app.config.discord_rp_brand = value,
                                    }
                                    app.config.save();
                                    app.sync_discord();
                                }
                            },
                        );
                    });
                }
            });

            // Статус подключения — чтобы видеть, дошёл ли presence до Discord.
            ui.add_space(4.0);
            let (color, text) = match &app.discord_status {
                crate::core::discord::Status::Connected => {
                    (theme::GOOD, tr(lang, "Settings_DiscordRP_Connected").to_string())
                }
                crate::core::discord::Status::Disconnected(reason) => (
                    theme::BAD,
                    format!("{}: {}", tr(lang, "Settings_DiscordRP_NoDiscord"), reason),
                ),
                crate::core::discord::Status::Idle => {
                    (theme::FG_DIMMER, tr(lang, "Settings_DiscordRP_Wait").to_string())
                }
            };
            ui.horizontal(|ui| {
                ui.label(RichText::new("●").size(10.0).color(color));
                ui.label(RichText::new(text).size(11.0).color(color));
            });
        }
    });

    ui.add_space(12.0);

    // ===== Путь установки =====
    theme::card_frame().show(ui, |ui| {
        card_label(ui, lang, "Settings_Path", "Settings_PathDesc");
        ui.horizontal(|ui| {
            let mut path = app.config.bytestrap_path.clone();
            let resp = ui.add(
                egui::TextEdit::singleline(&mut path)
                    .hint_text(crate::core::config::Config::base_dir().to_string_lossy())
                    .desired_width(ui.available_width() - 110.0)
                    .margin(egui::vec2(8.0, 5.0)),
            );
            if resp.changed() {
                app.config.bytestrap_path = path;
                app.config.save();
            }
            if ui.add(outline_btn(tr(lang, "Settings_PathReset"))).clicked() {
                app.config.bytestrap_path.clear();
                app.config.save();
            }
        });
    });

    ui.add_space(12.0);

    // ===== Очистка =====
    theme::card_frame().show(ui, |ui| {
        card_label(ui, lang, "Settings_Cleanup", "Settings_CleanupDesc");
        ui.horizontal(|ui| {
            let busy = app.cleanup_busy;
            let label = if busy {
                tr(lang, "Settings_CleanupBusy")
            } else {
                tr(lang, "Settings_CleanupBtn")
            };
            if ui
                .add_enabled(!busy && !app.install.active, primary_btn(label))
                .clicked()
            {
                app.start_cleanup();
            }
            if let Some((text, err)) = &app.cleanup_msg {
                ui.label(
                    RichText::new(text)
                        .size(12.0)
                        .family(egui::FontFamily::Monospace)
                        .color(if *err { theme::BAD } else { theme::GOOD }),
                );
            }
        });
    });

    // ===== Обработка диалога =====
    if let Some(yes) = answer {
        let confirm = app.confirm.take();
        if yes {
            if let Some(Confirm::ApplyChannel(ch)) = confirm {
                app.config.channel = ch;
                app.config.save();
                app.latest = None;
                app.latest_error = None;
                let tx = app.tx.clone();
                let channel = app.config.channel.clone();
                std::thread::spawn(move || {
                    let rt = tokio::runtime::Builder::new_multi_thread()
                        .enable_all()
                        .build()
                        .expect("tokio runtime");
                    let res = rt
                        .block_on(crate::core::deploy::latest_version(&channel))
                        .map_err(|e| e.to_string());
                    let _ = tx.send(AsyncEvent::Latest(res));
                });
                app.start_install(true);
            }
        }
    }
    if let Some(ch) = apply_channel {
        app.confirm = Some(Confirm::ApplyChannel(ch));
    }
}
