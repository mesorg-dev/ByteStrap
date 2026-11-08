//! Контроллер Slint-интерфейса (Фаза 0 прототипа).
//!
//! Вся логика лаунчера живёт здесь: статус, установка, запуск, счётчики,
//! страница «Логи» (tail/follow файлов Roblox + кольцевой буфер лаунчера).
//! .slint-файлы — только отрисовка. egui-интерфейс (`src/app.rs`)
//! остаётся основным до завершения миграции.

slint::include_modules!();

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};

#[cfg(windows)]
use slint::winit_030::WinitWindowAccessor;
use slint::{ComponentHandle, ModelRc, SharedString, Timer, TimerMode, VecModel, Weak};

use crate::app::{effective_installed, AsyncEvent, InstallUi, UpdateUi};
use crate::core::config::Config;
use crate::core::logview::{self, Follow, Line as SrcLine};
use crate::core::playtime::{self, Playtime};
use crate::core::{deploy, flags, launcher, logger, mods, roblox_link, updater, versions};
use crate::i18n::{fmt, tr, Lang};

// ===== Состояние =====

struct Ctx {
    weak: Weak<AppWindow>,
    lang: Lang,
    config: Config,
    installed: Option<String>,
    latest: Option<deploy::ClientVersion>,
    install: InstallUi,
    update: UpdateUi,
    /// Выбранная в таблице версия (страница «Версии»).
    selected_version: Option<String>,
    /// Результат последней проверки целостности.
    integrity: Option<versions::IntegrityReport>,
    quit: bool,
    pending_launch: Option<String>,
    mods_count: usize,
    flags_count: usize,
    playtime: Playtime,
    /// (текст, ошибка) — строка под полем ссылки на Home.
    link_msg: Option<(String, bool)>,
    tx: Sender<AsyncEvent>,
    rx: Receiver<AsyncEvent>,
    cancel: Option<Arc<AtomicBool>>,

    // — Логи —
    files: Vec<logview::LogFile>,
    selected: usize,
    follow: bool,
    search: String,
    /// Файл, чей хвост уже загружен в `all`.
    loaded: Option<PathBuf>,
    fst: Follow,
    all: Vec<SrcLine>,
    zebra: bool,
    /// Сколько записей кольцевого буфера лаунчера уже показано.
    ring_seen: usize,
    /// Подсказка в строке состояния (например «Скопировано»).
    info_override: Option<(String, Instant)>,
    last_page: i32,
}

impl Ctx {
    fn phase_status(&self) -> String {
        let lang = self.lang;
        if self.install.active {
            match self.install.phase {
                deploy::Phase::Check => fmt(lang, "Home_Status_Checking", &[]),
                deploy::Phase::Download => {
                    let pct = if self.install.total > 0 {
                        (self.install.done * 100 / self.install.total).to_string()
                    } else {
                        "…".into()
                    };
                    fmt(lang, "Home_Status_Downloading", &[&pct])
                }
                deploy::Phase::Extract => fmt(
                    lang,
                    "Home_Status_Extracting",
                    &[
                        &self.install.done.to_string(),
                        &self.install.total.to_string(),
                    ],
                ),
                deploy::Phase::Setup => fmt(lang, "Home_Status_Setup", &[]),
            }
        } else if let Some(err) = &self.install.error {
            fmt(lang, "Home_Status_Error", &[err])
        } else {
            String::new()
        }
    }

    fn update_available(&self) -> bool {
        match (&self.installed, &self.latest) {
            (Some(inst), Some(lat)) => inst != &lat.guid,
            (None, Some(_)) => true,
            _ => false,
        }
    }

    fn refresh_counters(&mut self) {
        let base = self.config.base();
        self.mods_count = mods::scan_mods(&base).len();
        self.flags_count = match &self.installed {
            Some(guid) => flags::load(&base.join("Versions").join(guid)).len(),
            None => 0,
        };
    }

    // ---- Установка (порт src/app.rs) ----

    fn start_install(&mut self, force: bool) {
        if self.install.active {
            return;
        }
        logger::info(if force {
            "Запущена принудительная переустановка клиента"
        } else {
            "Запущена установка/обновление клиента"
        });
        self.install = InstallUi {
            active: true,
            phase: deploy::Phase::Check,
            done: 0,
            total: 0,
            error: None,
        };

        let cfg = self.config.clone();
        let tx = self.tx.clone();
        let cancel = Arc::new(AtomicBool::new(false));
        self.cancel = Some(Arc::clone(&cancel));

        std::thread::spawn(move || {
            let (etx, erx) = mpsc::channel::<deploy::Event>();
            let relay = {
                let tx = tx.clone();
                std::thread::spawn(move || {
                    while let Ok(e) = erx.recv() {
                        if tx.send(AsyncEvent::Install(e)).is_err() {
                            break;
                        }
                    }
                })
            };
            let rt = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .expect("tokio runtime");
            let res = rt.block_on(deploy::ensure_installed(&cfg, &etx, &cancel, force));
            drop(etx);
            let _ = relay.join();
            if let Err(e) = res {
                if !cancel.load(Ordering::Relaxed) {
                    let _ = tx.send(AsyncEvent::Install(deploy::Event::Failed(e.to_string())));
                }
            }
        });
    }

    fn cancel_install(&mut self) {
        logger::info("Отмена установки запрошена");
        if let Some(c) = &self.cancel {
            c.store(true, Ordering::Relaxed);
        }
    }

    // ---- Запуск ----

    fn play(&mut self, arg: Option<&str>) {
        let guid = self
            .installed
            .clone()
            .or_else(|| versions::find_installed(&self.config.base()));
        let Some(guid) = guid else {
            self.pending_launch = arg.map(str::to_owned);
            self.start_install(false);
            return;
        };
        match launcher::exe_path(&self.config.base(), &guid) {
            Some(exe) => match launcher::launch(&exe, arg) {
                Ok(()) => {
                    self.install.error = None;
                    logger::info("Клиент Roblox запущен");
                    if self.config.close_on_launch {
                        self.quit = true;
                    }
                }
                Err(e) => {
                    logger::error(format!("Не удалось запустить клиента: {e}"));
                    self.install.error = Some(e.to_string());
                }
            },
            None => {
                self.pending_launch = arg.map(str::to_owned);
                self.start_install(false);
            }
        }
    }

    fn play_link(&mut self, input: &str) {
        match roblox_link::parse_arg(input) {
            Some(url) => {
                self.link_msg = Some((tr(self.lang, "Home_LinkOk").to_string(), false));
                self.play(Some(&url));
            }
            None => {
                self.link_msg = Some((tr(self.lang, "Home_LinkBad").to_string(), true));
            }
        }
    }

    // ---- События ----

    fn drain(&mut self) {
        while let Ok(ev) = self.rx.try_recv() {
            match ev {
                AsyncEvent::Latest(Ok(v)) => {
                    self.latest = Some(v);
                }
                AsyncEvent::Latest(Err(e)) => logger::warn(format!(
                    "Не удалось получить актуальную версию клиента: {e}"
                )),
                AsyncEvent::Install(ev) => self.on_install(ev),
                AsyncEvent::UpdateCheck(res) => {
                    self.update = match res {
                        Ok(Some(rel)) => {
                            if updater::is_newer(updater::current_version(), &rel.tag) {
                                UpdateUi::Found(rel)
                            } else {
                                UpdateUi::UpToDate
                            }
                        }
                        Ok(None) => UpdateUi::UpToDate,
                        Err(e) => UpdateUi::Error(e),
                    };
                }
                AsyncEvent::UpdateProgress { done, total } => {
                    if matches!(self.update, UpdateUi::Downloading { .. }) {
                        self.update = UpdateUi::Downloading { done, total };
                    }
                }
                AsyncEvent::UpdateDone(res) => match res {
                    Ok(()) => {
                        self.update = UpdateUi::Idle;
                        self.quit = true;
                    }
                    Err(e) => self.update = UpdateUi::Error(e),
                },
                _ => {}
            }
        }
    }

    fn on_install(&mut self, ev: deploy::Event) {
        match ev {
            deploy::Event::Progress {
                phase,
                done,
                total,
            } => {
                if self.install.active {
                    self.install.phase = phase;
                    self.install.done = done;
                    self.install.total = total;
                }
            }
            deploy::Event::Finished {
                guid,
                already,
                activate,
            } => {
                if already {
                    logger::info(format!("Версия {guid} уже установлена"));
                } else {
                    logger::info(format!("Установка версии {guid} завершена"));
                }
                if activate && !already {
                    let old = std::mem::take(&mut self.config.installed_version);
                    if !old.is_empty() && old != guid {
                        self.config.previous_version = old;
                    }
                    self.config.installed_version = guid;
                    self.config.save();
                }
                self.installed = effective_installed(&self.config);
                self.install = InstallUi::default();
                self.cancel = None;
                if !already {
                    self.refresh_counters();
                }
                if let Some(url) = self.pending_launch.take() {
                    self.play(Some(&url));
                }
            }
            deploy::Event::Failed(e) => {
                logger::error(format!("Ошибка установки: {e}"));
                if self.install.active {
                    self.install.active = false;
                    self.install.error = Some(e.to_string());
                }
                self.cancel = None;
                self.pending_launch = None;
            }
            deploy::Event::Cancelled => {
                self.install = InstallUi::default();
                self.cancel = None;
                self.pending_launch = None;
            }
        }
    }

    // ---- Обновления лаунчера (порт src/app.rs) ----

    fn check_updates(&mut self) {
        if matches!(
            self.update,
            UpdateUi::Checking | UpdateUi::Downloading { .. }
        ) {
            return;
        }
        self.update = UpdateUi::Checking;
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .expect("tokio runtime");
            let res = rt.block_on(updater::get_latest()).map_err(|e| e.to_string());
            let _ = tx.send(AsyncEvent::UpdateCheck(res));
        });
    }

    /// Скачать релиз и перезапуститься (bat подменит exe после выхода).
    fn start_update(&mut self, rel: updater::AppRelease) {
        if matches!(self.update, UpdateUi::Downloading { .. }) {
            return;
        }
        self.update = UpdateUi::Downloading {
            done: 0,
            total: 0,
        };
        let tx = self.tx.clone();
        let dest = std::env::temp_dir().join("bytestrap-update.exe");
        std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .expect("tokio runtime");
            let ptx = tx.clone();
            let res = rt
                .block_on(updater::download(&rel.download_url, &dest, move |done, total| {
                    let _ = ptx.send(AsyncEvent::UpdateProgress { done, total });
                }))
                .map_err(|e| e.to_string());
            let done = match res {
                Ok(()) => updater::install_and_restart(&dest).map_err(|e| e.to_string()),
                Err(e) => Err(e),
            };
            let _ = tx.send(AsyncEvent::UpdateDone(done));
        });
    }

    // ---- Версии (порт src/app.rs) ----

    fn activate_version(&mut self, guid: &str) {
        if !versions::is_version_guid(guid) {
            return;
        }
        let old = std::mem::take(&mut self.config.installed_version);
        if !old.is_empty() && old != guid {
            self.config.previous_version = old;
        }
        self.config.installed_version = guid.to_string();
        self.config.save();
        self.installed = effective_installed(&self.config);
        self.integrity = None;
        self.refresh_counters();
        logger::info(format!("Активирована версия {guid}"));
    }

    fn delete_version(&mut self, guid: &str) {
        if self.install.active || Some(guid) == self.installed.as_deref() {
            return;
        }
        if versions::delete(&self.config.base(), guid).is_ok() {
            logger::info(format!("Версия {guid} удалена"));
            if self.config.installed_version == guid {
                self.config.installed_version.clear();
                self.config.save();
            }
            if self.selected_version.as_deref() == Some(guid) {
                self.selected_version = None;
            }
            self.integrity = None;
            self.installed = effective_installed(&self.config);
            self.refresh_counters();
        }
    }

    fn check_integrity(&mut self) {
        let Some(guid) = self
            .selected_version
            .clone()
            .or_else(|| effective_installed(&self.config))
        else {
            return;
        };
        let dir = self.config.base().join("Versions").join(guid);
        self.integrity = versions::check(&dir).ok();
        logger::info("Проверка целостности завершена");
    }

    fn start_repair(&mut self, guid: String) {
        if self.install.active {
            return;
        }
        logger::info(format!("Запущен ремонт версии {guid}"));
        self.install = InstallUi {
            active: true,
            phase: deploy::Phase::Check,
            done: 0,
            total: 0,
            error: None,
        };

        let cfg = self.config.clone();
        let repair_for = effective_installed(&self.config);
        let tx = self.tx.clone();
        let cancel = Arc::new(AtomicBool::new(false));
        self.cancel = Some(Arc::clone(&cancel));

        std::thread::spawn(move || {
            let (etx, erx) = std::sync::mpsc::channel::<deploy::Event>();
            let relay = {
                let tx = tx.clone();
                std::thread::spawn(move || {
                    while let Ok(e) = erx.recv() {
                        if tx.send(AsyncEvent::Install(e)).is_err() {
                            break;
                        }
                    }
                })
            };
            let rt = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .expect("tokio runtime");
            let res = rt.block_on(deploy::ensure_guid(
                &cfg,
                &guid,
                true,
                false,
                &etx,
                &cancel,
                repair_for.as_deref(),
            ));
            drop(etx);
            let _ = relay.join();
            if let Err(e) = res {
                if !cancel.load(Ordering::Relaxed) {
                    let _ = tx.send(AsyncEvent::Install(deploy::Event::Failed(e.to_string())));
                }
            }
        });
    }

    // ---- Публикация в UI ----

    fn push_home(&self, ui: &AppWindow) {
        let lang = self.lang;
        let busy = self.install.active;

        let status_text = if busy {
            self.phase_status()
        } else if let Some(err) = &self.install.error {
            fmt(lang, "Home_Status_Error", &[err])
        } else if self.installed.is_none() {
            tr(lang, "Home_Status_NoClient").to_string()
        } else if self.update_available() {
            match &self.latest {
                Some(v) => fmt(lang, "Home_Status_Update", &[&v.version]),
                None => tr(lang, "Home_Status_Ready").to_string(),
            }
        } else {
            tr(lang, "Home_Status_Ready").to_string()
        };

        ui.set_status_text(status_text.as_str().into());
        ui.set_status_dot(if busy {
            2
        } else if self.install.error.is_some() {
            3
        } else if self.installed.is_some() {
            1
        } else {
            0
        });
        ui.set_busy(busy);
        ui.set_show_update(self.installed.is_some() && self.update_available());

        let primary = if self.installed.is_some() {
            tr(lang, "Home_Play")
        } else {
            tr(lang, "Home_Install")
        };
        ui.set_primary_label(primary.into());

        ui.set_progress(if self.install.total > 0 {
            (self.install.done as f32 / self.install.total as f32).clamp(0.0, 1.0)
        } else {
            0.0
        });
        let progress_text = if busy && self.install.phase == deploy::Phase::Download && self.install.total > 0 {
            let done_mb = self.install.done as f64 / 1_048_576.0;
            let total_mb = self.install.total as f64 / 1_048_576.0;
            fmt(
                lang,
                "Home_MbOf",
                &[&format!("{done_mb:.1}"), &format!("{total_mb:.0}")],
            )
        } else {
            String::new()
        };
        ui.set_progress_text(progress_text.as_str().into());

        match &self.link_msg {
            Some((text, bad)) => {
                ui.set_link_msg(text.as_str().into());
                ui.set_link_msg_bad(*bad);
            }
            None => {
                ui.set_link_msg("".into());
                ui.set_link_msg_bad(false);
            }
        }

        let version_text = match (&self.latest, &self.installed) {
            (Some(lat), _) => fmt(lang, "Home_Version", &[&lat.version]),
            (None, Some(inst)) => fmt(lang, "Home_Version", &[inst]),
            _ => tr(lang, "Home_VersionNone").to_string(),
        };
        ui.set_version_text(version_text.as_str().into());
        ui.set_mods_count(self.mods_count.to_string().as_str().into());
        ui.set_flags_count(self.flags_count.to_string().as_str().into());
        ui.set_hours_count((self.playtime.total_secs / 3600).to_string().as_str().into());

        let rows: Vec<PlaceRow> = self
            .playtime
            .top(5)
            .into_iter()
            .map(|(id, e)| {
                let name = if e.name.is_empty() { id } else { &e.name };
                PlaceRow {
                    name: name.as_str().into(),
                    duration: playtime::fmt_duration(e.secs).into(),
                }
            })
            .collect();
        ui.set_top_places(ModelRc::new(VecModel::from(rows)));
    }

    fn push_versions(&self, ui: &AppWindow) {
        let lang = self.lang;
        let list = versions::list(&self.config.base());
        let rows: Vec<VersionRow> = list
            .iter()
            .enumerate()
            .map(|(i, v)| VersionRow {
                guid: v.guid.as_str().into(),
                size: versions::format_size(v.size).into(),
                active: self.installed.as_deref() == Some(v.guid.as_str()),
                no_player: !v.has_player,
                alt: i % 2 == 1,
            })
            .collect();
        ui.set_ver_versions(ModelRc::new(VecModel::from(rows)));
        ui.set_ver_selected_guid(
            self.selected_version.as_deref().unwrap_or("").into(),
        );
        ui.set_ver_installed_guid(self.installed.as_deref().unwrap_or("").into());

        let (text, kind) = match &self.integrity {
            None => (tr(lang, "Ver_Check").to_string(), 0),
            Some(r) if r.ok() => (fmt(lang, "Ver_IntegrityOk", &[]), 1),
            Some(r) => (
                fmt(lang, "Ver_IntegrityIssues", &[&r.issues().to_string()]),
                2,
            ),
        };
        ui.set_ver_integrity_text(text.as_str().into());
        ui.set_ver_integrity_kind(kind);
        let checked = self
            .integrity
            .as_ref()
            .map(|r| format!("checked: {}", r.checked))
            .unwrap_or_default();
        ui.set_ver_checked_text(checked.as_str().into());
    }

    fn push_about(&self, ui: &AppWindow) {
        let lang = self.lang;
        ui.set_about_version(updater::current_version().into());
        let (kind, text, notes, pct) = match &self.update {
            UpdateUi::Idle => (0, String::new(), String::new(), 0.0),
            UpdateUi::Checking => {
                (1, tr(lang, "About_Checking").to_string(), String::new(), 0.0)
            }
            UpdateUi::UpToDate => {
                (2, tr(lang, "About_UpToDate").to_string(), String::new(), 0.0)
            }
            UpdateUi::Found(rel) => (
                3,
                fmt(lang, "About_UpdateAvail", &[&rel.tag]),
                rel.notes.trim().to_string(),
                0.0,
            ),
            UpdateUi::Downloading { done, total } => {
                let pct = if *total > 0 {
                    *done * 100 / *total
                } else {
                    0
                };
                (
                    4,
                    fmt(lang, "About_Downloading", &[&pct.to_string()]),
                    String::new(),
                    pct as f32 / 100.0,
                )
            }
            UpdateUi::Error(e) => (5, fmt(lang, "About_UpdateErr", &[e]), String::new(), 0.0),
        };
        ui.set_update_kind(kind);
        ui.set_update_text(text.as_str().into());
        ui.set_update_notes(notes.as_str().into());
        ui.set_update_pct(pct);
    }

    // ---- Логи ----

    fn reload_selected(&mut self) {
        self.loaded = None;
        self.all.clear();
        self.zebra = false;
        self.fst = Follow::default();
        self.ring_seen = 0;
    }

    fn labels(&self) -> Vec<SharedString> {
        let lang = self.lang;
        self.files
            .iter()
            .enumerate()
            .map(|(i, f)| {
                if i == 0 {
                    tr(lang, "Logs_Launcher").into()
                } else {
                    f.name.as_str().into()
                }
            })
            .collect()
    }

    fn matches(&self, l: &SrcLine, q: &str) -> bool {
        q.is_empty() || l.text.to_lowercase().contains(q) || l.time.contains(q)
    }

    fn filtered_text(&self) -> String {
        let q = self.search.to_lowercase();
        self.all
            .iter()
            .filter(|l| self.matches(l, &q))
            .map(|l| l.text.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn trim(&mut self) {
        if self.all.len() > logview::MAX_LINES {
            let cut = self.all.len() - logview::MAX_LINES;
            self.all.drain(..cut);
        }
    }

    fn logs_tick(&mut self, ui: &AppWindow) {
        // Список файлов: лаунчер первым, дальше — свежие логи Roblox.
        let mut files = vec![logview::launcher_log()];
        files.extend(logview::roblox_logs());
        self.files = files;
        if self.selected >= self.files.len() {
            self.selected = 0;
        }
        ui.set_log_files(ModelRc::new(VecModel::from(self.labels())));
        ui.set_log_selected(self.selected as i32);
        ui.set_log_follow(self.follow);

        let path = self.files[self.selected].path.clone();
        let mut new_lines = 0usize;
        let missing;

        if self.selected == 0 {
            // Лаунчер: кольцевой буфер в памяти (показывает строки мгновенно).
            if self.loaded.as_deref() != Some(path.as_path()) {
                self.all.clear();
                self.zebra = false;
                self.ring_seen = 0;
                self.loaded = Some(path);
            }
            let ring = logger::ring();
            if ring.len() < self.ring_seen {
                // Вытеснение из кольца — читаем буфер заново.
                self.all.clear();
                self.zebra = false;
                self.ring_seen = 0;
            }
            new_lines = ring.len() - self.ring_seen;
            for e in &ring[self.ring_seen..] {
                self.zebra = !self.zebra;
                self.all.push(SrcLine {
                    time: e.time.clone(),
                    level: e.level,
                    text: e.text.clone(),
                    zebra: self.zebra,
                });
            }
            self.ring_seen = ring.len();
            missing = false;
        } else {
            // Логи Roblox: хвост + дочитывание (follow).
            if self.loaded.as_deref() != Some(path.as_path()) {
                self.all.clear();
                self.zebra = false;
                self.fst = Follow::default();
                self.loaded = Some(path.clone());
                if path.exists() {
                    let tail = logview::load_tail(&path, &mut self.fst);
                    let (ls, z) = logview::parse_chunk(&tail, self.zebra);
                    new_lines = ls.len();
                    self.zebra = z;
                    self.all.extend(ls);
                }
            } else if path.exists() {
                let more = logview::follow_read(&path, &mut self.fst);
                if !more.is_empty() {
                    let (ls, z) = logview::parse_chunk(&more, self.zebra);
                    new_lines = ls.len();
                    self.zebra = z;
                    self.all.extend(ls);
                }
            }
            missing = !path.exists();
        }
        self.trim();

        // Публикация отфильтрованного вида.
        let q = self.search.to_lowercase();
        let rows: Vec<LogLine> = self
            .all
            .iter()
            .filter(|l| self.matches(l, &q))
            .map(|l| LogLine {
                time: l.time.as_str().into(),
                level: l.level as i32,
                text: l.text.as_str().into(),
                zebra: l.zebra,
            })
            .collect();
        let count = rows.len();
        ui.set_log_lines(ModelRc::new(VecModel::from(rows)));
        ui.set_log_missing(missing);

        // Строка состояния: подсказка действия или число строк.
        let lang = self.lang;
        let override_fresh = self
            .info_override
            .as_ref()
            .is_some_and(|(_, at)| at.elapsed() < Duration::from_secs(2));
        let info = match &self.info_override {
            Some((text, _)) if override_fresh => text.clone(),
            _ => fmt(lang, "Logs_LinesFmt", &[&count.to_string()]),
        };
        if self.info_override.is_some() && !override_fresh {
            self.info_override = None;
        }
        ui.set_log_info(info.as_str().into());

        // Автопрокрутка вниз при новых строках.
        if self.follow && new_lines > 0 && count > 0 {
            ui.set_log_jump_token(ui.get_log_jump_token() + 1);
        }
    }
}

// ===== Буфер обмена и папки =====

#[cfg(windows)]
fn clipboard_text() -> Option<String> {
    let mut c = arboard::Clipboard::new().ok()?;
    let t = c.get_text().ok()?;
    let trimmed = t.trim().to_string();
    if trimmed.is_empty() { None } else { Some(trimmed) }
}

#[cfg(not(windows))]
fn clipboard_text() -> Option<String> {
    None
}

#[cfg(windows)]
fn clipboard_set(text: &str) -> bool {
    arboard::Clipboard::new()
        .and_then(|mut c| c.set_text(text.to_string()))
        .is_ok()
}

#[cfg(not(windows))]
fn clipboard_set(_text: &str) -> bool {
    false
}

#[cfg(windows)]
fn open_folder(path: &Path) {
    let _ = std::process::Command::new("explorer").arg(path).spawn();
}

#[cfg(not(windows))]
fn open_folder(_path: &Path) {}

#[cfg(windows)]
fn open_url(url: &str) {
    let _ = std::process::Command::new("cmd")
        .args(["/C", "start", url])
        .spawn();
}

#[cfg(not(windows))]
fn open_url(url: &str) {
    let _ = std::process::Command::new("xdg-open").arg(url).spawn();
}

// ===== Операции с окном (winit доступен только на Windows) =====

#[cfg(windows)]
fn winit_min(ui: &AppWindow) {
    let _ = ui.window().with_winit_window(|x| x.set_minimized(true));
}

#[cfg(not(windows))]
fn winit_min(_ui: &AppWindow) {}

#[cfg(windows)]
fn winit_toggle_max(ui: &AppWindow) {
    let _ = ui.window().with_winit_window(|x| {
        let max = x.is_maximized();
        x.set_maximized(!max);
    });
}

#[cfg(not(windows))]
fn winit_toggle_max(_ui: &AppWindow) {}

#[cfg(windows)]
fn winit_drag(ui: &AppWindow) {
    let _ = ui.window().with_winit_window(|x| {
        let _ = x.drag_window();
    });
}

#[cfg(not(windows))]
fn winit_drag(_ui: &AppWindow) {}

// ===== Перевод строк =====

fn apply_i18n(ui: &AppWindow, lang: Lang) {
    let t = ui.global::<T>();
    t.set_nav_home(tr(lang, "Nav_Home").into());
    t.set_nav_flags(tr(lang, "Nav_Flags").into());
    t.set_nav_versions(tr(lang, "Nav_Versions").into());
    t.set_nav_mods(tr(lang, "Nav_Mods").into());
    t.set_nav_logs(tr(lang, "Nav_Logs").into());
    t.set_nav_settings(tr(lang, "Nav_Settings").into());
    t.set_nav_about(tr(lang, "Nav_About").into());
    t.set_app_sub(tr(lang, "App_Sub").into());
    t.set_win_min(tr(lang, "Win_Min").into());
    t.set_win_max(tr(lang, "Win_Max").into());
    t.set_win_close(tr(lang, "Win_Close").into());
    t.set_footer_ready(tr(lang, "Footer_Ready").into());
    t.set_btn_cancel(tr(lang, "Btn_Cancel").into());
    t.set_btn_copy(tr(lang, "Btn_Copy").into());
    t.set_home_h1(tr(lang, "Home_H1").into());
    t.set_home_sub(tr(lang, "Home_Sub").into());
    t.set_home_play(tr(lang, "Home_Play").into());
    t.set_home_install(tr(lang, "Home_Install").into());
    t.set_home_update(tr(lang, "Home_Update").into());
    t.set_home_link_hint(tr(lang, "Home_LinkHint").into());
    t.set_home_link_go(tr(lang, "Home_LinkGo").into());
    t.set_home_link_paste(tr(lang, "Home_LinkPaste").into());
    t.set_home_places(tr(lang, "Home_Places").into());
    t.set_logs_h1(tr(lang, "Logs_H1").into());
    t.set_logs_sub(tr(lang, "Logs_Sub").into());
    t.set_logs_launcher(tr(lang, "Logs_Launcher").into());
    t.set_logs_follow(tr(lang, "Logs_Follow").into());
    t.set_logs_copied(tr(lang, "Logs_Copied").into());
    t.set_logs_copy_fail(tr(lang, "Logs_CopyFail").into());
    t.set_logs_folder(tr(lang, "Logs_Folder").into());
    t.set_logs_refresh(tr(lang, "Logs_Refresh").into());
    t.set_logs_search(tr(lang, "Logs_Search").into());
    t.set_logs_empty(tr(lang, "Logs_Empty").into());
    t.set_logs_missing(tr(lang, "Logs_Missing").into());
    t.set_logs_no_files(tr(lang, "Logs_NoFiles").into());
    t.set_ver_h1(tr(lang, "Ver_H1").into());
    t.set_ver_sub(tr(lang, "Ver_Sub").into());
    t.set_ver_activate(tr(lang, "Ver_Activate").into());
    t.set_ver_delete(tr(lang, "Ver_Delete").into());
    t.set_ver_empty(tr(lang, "Ver_Empty").into());
    t.set_ver_col_guid(tr(lang, "Ver_ColGuid").into());
    t.set_ver_col_size(tr(lang, "Ver_ColSize").into());
    t.set_ver_col_status(tr(lang, "Ver_ColStatus").into());
    t.set_ver_active(tr(lang, "Ver_Active").into());
    t.set_ver_check(tr(lang, "Ver_Check").into());
    t.set_ver_integrity_ok(tr(lang, "Ver_IntegrityOk").into());
    t.set_ver_integrity_issues(tr(lang, "Ver_IntegrityIssues").into());
    t.set_ver_repair(tr(lang, "Ver_Repair").into());
    t.set_about_h1(tr(lang, "About_H1").into());
    t.set_about_sub(tr(lang, "About_Sub").into());
    t.set_about_version(tr(lang, "About_Version").into());
    t.set_about_repo(tr(lang, "About_Repo").into());
    t.set_about_license(tr(lang, "About_License").into());
    t.set_about_check_updates(tr(lang, "About_CheckUpdates").into());
    t.set_about_checking(tr(lang, "About_Checking").into());
    t.set_about_up_to_date(tr(lang, "About_UpToDate").into());
    t.set_about_update_avail(tr(lang, "About_UpdateAvail").into());
    t.set_about_download(tr(lang, "About_Download").into());
    t.set_about_downloading(tr(lang, "About_Downloading").into());
    t.set_about_update_err(tr(lang, "About_UpdateErr").into());
    t.set_common_soon(tr(lang, "Common_ComingSoon").into());
}

// ===== Колбэки UI =====

fn wire(ui: &AppWindow, ctx: &Rc<RefCell<Ctx>>) {
    let weak = ctx.borrow().weak.clone();

    // — Окно —
    {
        let w = weak.clone();
        ui.on_win_min(move || winit_min(&w.unwrap()));
    }
    {
        let w = weak.clone();
        ui.on_win_max(move || winit_toggle_max(&w.unwrap()));
    }
    {
        let w = weak.clone();
        ui.on_titlebar_drag(move || winit_drag(&w.unwrap()));
    }
    {
        let ctx = Rc::clone(ctx);
        ui.on_close_request(move || {
            ctx.borrow().config.save();
            logger::info("Окно закрыто — конфиг сохранён");
            let _ = slint::quit_event_loop();
        });
    }

    // — Home —
    {
        let ctx = Rc::clone(ctx);
        let weak = weak.clone();
        ui.on_primary(move || {
            {
                let mut c = ctx.borrow_mut();
                if c.install.active {
                    return;
                }
                if c.installed.is_some() {
                    c.play(None);
                } else {
                    c.start_install(false);
                }
            }
            let c = ctx.borrow();
            c.push_home(&weak.unwrap());
        });
    }
    {
        let ctx = Rc::clone(ctx);
        let weak = weak.clone();
        ui.on_update_client(move || {
            {
                ctx.borrow_mut().start_install(false);
            }
            let c = ctx.borrow();
            c.push_home(&weak.unwrap());
        });
    }
    {
        let ctx = Rc::clone(ctx);
        let weak = weak.clone();
        ui.on_cancel_install(move || {
            ctx.borrow_mut().cancel_install();
            let c = ctx.borrow();
            c.push_home(&weak.unwrap());
        });
    }
    {
        let ctx = Rc::clone(ctx);
        let weak = weak.clone();
        ui.on_link_go(move |url: SharedString| {
            {
                ctx.borrow_mut().play_link(url.as_str());
            }
            let c = ctx.borrow();
            c.push_home(&weak.unwrap());
        });
    }
    {
        let ctx = Rc::clone(ctx);
        let weak = weak.clone();
        ui.on_link_paste(move || {
            if let Some(text) = clipboard_text() {
                let ui = weak.unwrap();
                ctx.borrow_mut().link_msg = None;
                ui.set_link_paste_text(text.as_str().into());
                ui.set_link_paste_token(ui.get_link_paste_token() + 1);
            }
        });
    }
    {
        let ctx = Rc::clone(ctx);
        ui.on_link_changed(move || {
            // try_borrow: edited может прийти, пока идёт публикация свойств.
            if let Ok(mut c) = ctx.try_borrow_mut() {
                c.link_msg = None;
            }
        });
    }

    // — Логи —
    {
        let ctx = Rc::clone(ctx);
        ui.on_log_select(move |i: i32| {
            let mut c = ctx.borrow_mut();
            c.selected = i.max(0) as usize;
            c.reload_selected();
            let ui = c.weak.clone().unwrap();
            c.logs_tick(&ui);
        });
    }
    {
        let ctx = Rc::clone(ctx);
        ui.on_log_refresh(move || {
            let mut c = ctx.borrow_mut();
            c.reload_selected();
            let ui = c.weak.clone().unwrap();
            c.logs_tick(&ui);
        });
    }
    {
        let ctx = Rc::clone(ctx);
        ui.on_log_folder(move || {
            let c = ctx.borrow();
            if let Some(f) = c.files.get(c.selected) {
                let dir = f.path.parent().unwrap_or(&f.path);
                open_folder(dir);
            }
        });
    }
    {
        let ctx = Rc::clone(ctx);
        ui.on_log_copy(move || {
            let text = ctx.borrow().filtered_text();
            let ok = !text.is_empty() && clipboard_set(&text);
            let mut c = ctx.borrow_mut();
            let key = if ok { "Logs_Copied" } else { "Logs_CopyFail" };
            c.info_override = Some((tr(c.lang, key).to_string(), Instant::now()));
            let ui = c.weak.clone().unwrap();
            c.logs_tick(&ui);
        });
    }
    {
        let ctx = Rc::clone(ctx);
        ui.on_log_follow_toggle(move || {
            let mut c = ctx.borrow_mut();
            c.follow = !c.follow;
            let ui = c.weak.clone().unwrap();
            ui.set_log_follow(c.follow);
            if c.follow {
                let tok = ui.get_log_jump_token() + 1;
                ui.set_log_jump_token(tok);
            }
        });
    }
    {
        let ctx = Rc::clone(ctx);
        ui.on_log_search(move |q: SharedString| {
            let mut c = ctx.borrow_mut();
            c.search = q.to_string();
            let ui = c.weak.clone().unwrap();
            c.logs_tick(&ui);
        });
    }

    // — Версии —
    {
        let ctx = Rc::clone(ctx);
        let weak = weak.clone();
        ui.on_ver_select(move |g: SharedString| {
            {
                let mut c = ctx.borrow_mut();
                c.selected_version = Some(g.to_string()).filter(|s| !s.is_empty());
                c.integrity = None;
            }
            let c = ctx.borrow();
            c.push_versions(&weak.unwrap());
        });
    }
    {
        let ctx = Rc::clone(ctx);
        let weak = weak.clone();
        ui.on_ver_activate(move || {
            {
                let mut c = ctx.borrow_mut();
                if let Some(g) = c.selected_version.clone() {
                    c.activate_version(&g);
                }
            }
            let c = ctx.borrow();
            c.push_versions(&weak.unwrap());
        });
    }
    {
        let ctx = Rc::clone(ctx);
        let weak = weak.clone();
        ui.on_ver_delete(move || {
            {
                let mut c = ctx.borrow_mut();
                if let Some(g) = c.selected_version.clone() {
                    c.delete_version(&g);
                }
            }
            let c = ctx.borrow();
            c.push_versions(&weak.unwrap());
        });
    }
    {
        let ctx = Rc::clone(ctx);
        let weak = weak.clone();
        ui.on_ver_check(move || {
            ctx.borrow_mut().check_integrity();
            let c = ctx.borrow();
            c.push_versions(&weak.unwrap());
        });
    }
    {
        let ctx = Rc::clone(ctx);
        ui.on_ver_repair(move || {
            let mut c = ctx.borrow_mut();
            let g = c.selected_version.clone().or_else(|| c.installed.clone());
            if let Some(g) = g {
                c.start_repair(g);
            }
        });
    }

    // — Обновления лаунчера —
    {
        let ctx = Rc::clone(ctx);
        let weak = weak.clone();
        ui.on_about_check(move || {
            {
                ctx.borrow_mut().check_updates();
            }
            let c = ctx.borrow();
            c.push_about(&weak.unwrap());
        });
    }
    {
        let ctx = Rc::clone(ctx);
        let weak = weak.clone();
        ui.on_about_download(move || {
            {
                let mut c = ctx.borrow_mut();
                if let UpdateUi::Found(rel) = &c.update {
                    let rel = rel.clone();
                    c.start_update(rel);
                }
            }
            let c = ctx.borrow();
            c.push_about(&weak.unwrap());
        });
    }
    ui.on_about_open_repo(move || {
        open_url("https://github.com/mesorg-dev/ByteStrap");
    });
}

// ===== Точка входа =====

/// Точка входа нового интерфейса (бинарник `bytestrap-slint`).
pub fn run() -> Result<(), slint::PlatformError> {
    logger::info("Интерфейс Slint запущен");

    let ui = AppWindow::new()?;
    let config = Config::load();
    let lang = Lang::resolve(&config.language);
    apply_i18n(&ui, lang);

    let installed = effective_installed(&config);
    let mods_count = mods::scan_mods(&config.base()).len();
    let flags_count = match &installed {
        Some(guid) => flags::load(&config.base().join("Versions").join(guid)).len(),
        None => 0,
    };
    let playtime = Playtime::load(&config.base());

    let (tx, rx) = mpsc::channel();
    {
        let tx = tx.clone();
        let channel = config.channel.clone();
        std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .expect("tokio runtime");
            let res = rt
                .block_on(deploy::latest_version(&channel))
                .map_err(|e| e.to_string());
            let _ = tx.send(AsyncEvent::Latest(res));
        });
    }

    let weak = ui.as_weak();
    let ctx = Rc::new(RefCell::new(Ctx {
        weak: weak.clone(),
        lang,
        config,
        installed,
        latest: None,
        install: InstallUi::default(),
        update: UpdateUi::Idle,
        selected_version: None,
        integrity: None,
        quit: false,
        pending_launch: None,
        mods_count,
        flags_count,
        playtime,
        link_msg: None,
        tx,
        rx,
        cancel: None,
        files: Vec::new(),
        selected: 0,
        follow: true,
        search: String::new(),
        loaded: None,
        fst: Follow::default(),
        all: Vec::new(),
        zebra: false,
        ring_seen: 0,
        info_override: None,
        last_page: -1,
    }));

    wire(&ui, &ctx);
    ui.set_about_version(updater::current_version().into());
    if ctx.borrow().config.auto_update {
        ctx.borrow_mut().check_updates();
    }
    ctx.borrow().push_home(&ui);

    // Общий тик (100 мс): события + публикация состояния Home.
    let tick = Timer::default();
    {
        let ctx = Rc::clone(&ctx);
        tick.start(TimerMode::Repeated, Duration::from_millis(100), move || {
            let mut c = ctx.borrow_mut();
            c.drain();
            if c.quit {
                let _ = slint::quit_event_loop();
                return;
            }
            let ui = c.weak.clone().unwrap();
            c.push_home(&ui);
            let page = ui.get_page();
            if page != c.last_page {
                if page == 2 {
                    c.push_versions(&ui);
                }
                if page == 4 {
                    c.logs_tick(&ui);
                }
            }
            if page == 6 {
                c.push_about(&ui);
            }
            c.last_page = page;
        });
    }

    // Тик логов (1 с): только на странице «Логи».
    let logs_timer = Timer::default();
    {
        let ctx = Rc::clone(&ctx);
        logs_timer.start(TimerMode::Repeated, Duration::from_secs(1), move || {
            let mut c = ctx.borrow_mut();
            let ui = c.weak.clone().unwrap();
            if ui.get_page() == 4 {
                c.logs_tick(&ui);
            }
        });
    }

    let res = ui.run();

    ctx.borrow().config.save();
    logger::info("Интерфейс Slint завершён");
    res
}
