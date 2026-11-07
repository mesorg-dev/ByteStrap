//! ByteStrap Shell — второе приложение лаунчера: плейсы с настоящими
//! обложками, запуск `roblox://`, настройки и настраиваемый фон.
//!
//! Каркас окна, карточки и оверлей настроек — в [`ui`] и [`settings`].

pub mod config;
pub mod net;
pub mod settings;
pub mod ui;

use std::collections::HashMap;
use std::sync::mpsc::{Receiver, Sender};
use std::time::{Duration, Instant};

use eframe::egui::{
    self, Color32, ColorImage, TextureHandle, TextureOptions,
};

use crate::core::config::Config;
use crate::core::playtime::{self, Playtime, SessionEvent};
use crate::core::{launcher, versions};
use crate::i18n::{fmt, tr, Lang};
use crate::theme;

use config::ShellConfig;
use net::Ev;

/// Карточка места (недавнее или рекомендация).
#[derive(Clone, Debug)]
pub struct Card {
    pub id: String,
    pub name: String,
    /// Сколько играют сейчас (None — ещё не загружено).
    pub playing: Option<u64>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum NetState {
    Checking,
    Online,
    Offline,
}

pub struct Shell {
    pub lang: Lang,
    /// Конфиг лаунчера: база каталога, канал клиента.
    pub lc: Config,
    pub cfg: ShellConfig,
    tx: Sender<Ev>,
    rx: Receiver<Ev>,
    pub net: NetState,
    pub recent: Vec<Card>,
    pub reco: Vec<Card>,
    pub thumbs: HashMap<String, TextureHandle>,
    /// Накопленное время по местам (для секции «Недавние»).
    playtime: Playtime,
    session_active: bool,
    /// Исходный (неразмытый) фон ≤1920.
    bg_source: Option<std::sync::Arc<ColorImage>>,
    /// Текущая отрисовочная текстура фона.
    bg_tex: Option<TextureHandle>,
    /// Дедлайн переразмытия (debounce ползунка).
    bg_job: Option<Instant>,
    loading: bool,
    pub search: String,
    pub settings_open: bool,
    pub page: settings::Page,
    /// (текст, момент затухания).
    pub toast: Option<(String, f64)>,
    /// Текущее время UI (для тостов).
    now: f64,
    /// roblox:// из argv — запустится в первом кадре.
    pending_launch: Option<String>,
    quit: bool,
}

impl Shell {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        theme::apply(&cc.egui_ctx);
        let lc = Config::load();
        let cfg = ShellConfig::load();
        let lang = if cfg.lang.is_empty() {
            Lang::resolve(&lc.language)
        } else {
            Lang::resolve(&cfg.lang)
        };

        let (tx, rx) = std::sync::mpsc::channel();
        let mut shell = Self {
            lang,
            lc,
            cfg,
            tx,
            rx,
            net: NetState::Checking,
            recent: Vec::new(),
            reco: Vec::new(),
            thumbs: HashMap::new(),
            playtime: Playtime::default(),
            session_active: false,
            bg_source: None,
            bg_tex: None,
            bg_job: None,
            loading: false,
            search: String::new(),
            settings_open: false,
            page: settings::Page::Look,
            toast: None,
            now: 0.0,
            pending_launch: None,
            quit: false,
        };

        shell.rebuild_recent();
        shell.rebuild_reco();
        shell.start_load();

        if !shell.cfg.bg_path.is_empty() {
            shell.load_bg();
        }
        shell
    }

    /// Ссылка roblox:// из аргументов командной строки.
    pub fn set_deep_link(&mut self, url: &str) {
        self.pending_launch = Some(url.to_string());
    }

    /// Недавние места из playtime.json (по последнему визиту, ≤8).
    pub fn rebuild_recent(&mut self) {
        let base = self.lc.base();
        self.playtime = Playtime::load(&base);
        let mut v: Vec<(String, u64, String)> = self
            .playtime
            .places
            .iter()
            .map(|(id, e)| (id.clone(), e.unix, e.name.clone()))
            .collect();
        v.sort_by(|a, b| b.1.cmp(&a.1));
        v.truncate(8);
        self.recent = v
            .into_iter()
            .map(|(id, _, name)| Card {
                name: if name.is_empty() { id.clone() } else { name },
                id,
                playing: None,
            })
            .collect();
    }

    /// Рекомендации из встроенного списка (имена-фолбэки).
    pub fn rebuild_reco(&mut self) {
        self.reco = net::RECO
            .iter()
            .map(|(id, name)| Card {
                id: (*id).to_string(),
                name: (*name).to_string(),
                playing: None,
            })
            .collect();
    }

    /// Фоновая загрузка метаданных и обложек.
    fn start_load(&mut self) {
        if self.loading {
            return;
        }
        self.loading = true;
        self.net = NetState::Checking;
        let ids: Vec<String> = self.recent.iter().map(|c| c.id.clone()).collect();
        net::spawn_load(ids, self.lc.base(), self.tx.clone());
    }

    /// Загрузить выбранный фоновый файл.
    pub fn load_bg(&mut self) {
        if self.cfg.bg_path.is_empty() {
            self.bg_source = None;
            self.bg_tex = None;
            self.bg_job = None;
            return;
        }
        net::spawn_bg(
            std::path::PathBuf::from(&self.cfg.bg_path),
            self.tx.clone(),
        );
    }

    /// Переразмыть фон (debounce: ползунок не плодит потоки).
    pub fn schedule_blur(&mut self) {
        self.bg_job = Some(Instant::now() + Duration::from_millis(180));
    }

    fn spawn_blur_now(&mut self) {
        let Some(src) = self.bg_source.clone() else { return };
        let sigma = self.cfg.bg_blur.max(0) as f32;
        net::spawn_blur(src, sigma, self.tx.clone());
    }

    pub fn toast(&mut self, text: impl Into<String>) {
        self.toast = Some((text.into(), self.now + 2.6));
    }

    /// Запуск игры по карточке.
    pub fn launch(&mut self, card: &Card) {
        let base = self.lc.base();
        let guid = crate::app::effective_installed(&self.lc)
            .or_else(|| versions::find_installed(&base));
        let Some(guid) = guid else {
            self.toast(tr(self.lang, "Shell_NoVersion"));
            return;
        };
        let Some(exe) = launcher::exe_path(&base, &guid) else {
            self.toast(tr(self.lang, "Shell_NoVersion"));
            return;
        };
        let url = format!("roblox://placeId={}", card.id);
        self.launch_url(&exe, &url, &card.name);
    }

    /// Запуск клиента с готовой ссылкой (deep link из argv).
    pub fn launch_deep_link(&mut self, url: &str) {
        let base = self.lc.base();
        let guid = crate::app::effective_installed(&self.lc)
            .or_else(|| versions::find_installed(&base));
        let Some(guid) = guid else {
            self.toast(tr(self.lang, "Shell_NoVersion"));
            return;
        };
        let Some(exe) = launcher::exe_path(&base, &guid) else {
            self.toast(tr(self.lang, "Shell_NoVersion"));
            return;
        };
        let name = url.trim_start_matches("roblox://placeId=").to_string();
        self.launch_url(&exe, url, &name);
    }

    fn launch_url(&mut self, exe: &std::path::Path, url: &str, name: &str) {
        match launcher::launch(exe, Some(url)) {
            Ok(()) => {
                self.toast(fmt(self.lang, "Shell_LaunchOkFmt", &[name]));
                self.start_session_watch();
            }
            Err(e) => self.toast(fmt(self.lang, "Shell_LaunchErrFmt", &[&e.to_string()])),
        }
    }

    /// Счётчик сессии: слить playtime по завершении игры.
    fn start_session_watch(&mut self) {
        if self.session_active {
            return;
        }
        self.session_active = true;
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let (stx, srx) = std::sync::mpsc::channel::<SessionEvent>();
            let relay = {
                let tx = tx.clone();
                std::thread::spawn(move || {
                    while let Ok(e) = srx.recv() {
                        if tx.send(Ev::Session(e)).is_err() {
                            break;
                        }
                    }
                })
            };
            playtime::watch_session(stx);
            let _ = relay.join();
        });
    }

    fn drain(&mut self, ctx: &egui::Context) {
        self.now = ctx.input(|i| i.time);

        // Дедлайн переразмытия фона.
        if let Some(t) = self.bg_job {
            if Instant::now() >= t {
                self.bg_job = None;
                self.spawn_blur_now();
            }
        }

        while let Ok(ev) = self.rx.try_recv() {
            match ev {
                Ev::Net(ok) => {
                    self.net = if ok { NetState::Online } else { NetState::Offline };
                }
                Ev::Meta(metas) => {
                    for m in metas {
                        for card in self.recent.iter_mut().chain(self.reco.iter_mut()) {
                            if card.id == m.id {
                                if !m.name.is_empty() {
                                    card.name = m.name.clone();
                                }
                                card.playing = Some(m.playing);
                            }
                        }
                    }
                }
                Ev::Thumb { id, img } => {
                    let tex =
                        ctx.load_texture(format!("shell_thumb_{id}"), img, TextureOptions::LINEAR);
                    self.thumbs.insert(id, tex);
                }
                Ev::Bg(src) => {
                    self.bg_source = Some(src);
                    self.spawn_blur_now();
                }
                Ev::Blurred(img) => {
                    self.bg_tex = Some(ctx.load_texture("shell_bg", img, TextureOptions::LINEAR));
                }
                Ev::Loaded => self.loading = false,
                Ev::Session(ev) => match ev {
                    SessionEvent::Tick { .. } => {}
                    SessionEvent::End { total, places } => {
                        self.session_active = false;
                        self.playtime.merge(total, &places);
                        self.playtime.save(&self.lc.base());
                        let before: Vec<String> = self.recent.iter().map(|c| c.id.clone()).collect();
                        self.rebuild_recent();
                        let after: Vec<String> = self.recent.iter().map(|c| c.id.clone()).collect();
                        if before != after {
                            self.loading = false;
                            self.start_load();
                        }
                        if self.cfg.close_on_exit {
                            self.quit = true;
                        }
                    }
                },
            }
        }

        // Перетащенный на окно файл фона (Settings → Внешний вид).
        let dropped: Vec<std::path::PathBuf> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .map(|f| f.path().to_path_buf())
                .collect()
        });
        for path in dropped {
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| e.to_ascii_lowercase())
                .unwrap_or_default();
            if matches!(ext.as_str(), "png" | "jpg" | "jpeg" | "gif" | "webp") {
                self.cfg.bg_path = path.display().to_string();
                self.load_bg();
                break;
            }
        }

        // Гашение тоста.
        if let Some((_, until)) = &self.toast {
            if self.now > *until {
                self.toast = None;
            }
        }

        // Deep link из argv — запускаем в первом кадре.
        if let Some(url) = self.pending_launch.take() {
            self.launch_deep_link(&url);
        }

        // Память о размере/развёрнутости окна.
        let viewport = ctx.input(|i| i.viewport().clone());
        if let Some(r) = viewport.inner_rect {
            if (r.width() - self.cfg.win_w).abs() > 1.0 || (r.height() - self.cfg.win_h).abs() > 1.0
            {
                self.cfg.win_w = r.width();
                self.cfg.win_h = r.height();
            }
        }
        if let Some(m) = viewport.maximized {
            self.cfg.win_max = m;
        }
    }

    /// Размытие CSS-пикселей → сигма гауссиана.
    pub fn accent(&self) -> Color32 {
        self.cfg.accent_color()
    }

    pub fn card_alpha(&self) -> u8 {
        (self.cfg.card_opacity.clamp(40, 100) * 255 / 100) as u8
    }
}

impl eframe::App for Shell {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.drain(&ctx);

        if self.quit {
            self.quit = false;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }

        ui::paint_background(self, ui);
        ui::header(self, ui);

        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("shell_home_scroll")
                    .auto_shrink([false, false])
                    .show(ui, |ui| ui::home(self, ui));
            });

        if self.settings_open {
            settings::overlay(self, &ctx);
        }
        ui::paint_toast(self, &ctx);
        ui::resize_grips(self, ui, &ctx);
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.cfg.save();
        self.lc.save();
    }
}
