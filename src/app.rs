use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::Arc;

use eframe::egui::{
    self, Align, Color32, CursorIcon, FontFamily, FontId, Frame, Layout, Margin, Panel, Pos2,
    Rect, RichText, Sense, Vec2,
};

use crate::core::config::Config;
use crate::core::playtime::{self, Playtime, SessionEvent};
use crate::core::{deep_link, deploy, discord, launcher, mods, updater, versions};
use crate::i18n::{fmt, tr, Lang};
use crate::theme;
use crate::ui;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Home,
    Flags,
    Versions,
    Mods,
    Settings,
    About,
}

pub enum AsyncEvent {
    Latest(Result<deploy::ClientVersion, String>),
    Install(deploy::Event),
    UpdateCheck(Result<Option<updater::AppRelease>, String>),
    UpdateProgress { done: u64, total: u64 },
    UpdateDone(Result<(), String>),
    ChannelPreview(Result<deploy::ClientVersion, String>),
    ModsDone(Result<mods::ApplyStats, String>),
    /// Загрузка профиля модов завершена: (имя, статистика применения).
    ProfileLoaded(Result<(String, Option<mods::ApplyStats>), String>),
    /// Очистка мусора завершена: (байт, файлов).
    CleanupDone(u64, usize),
    Session(SessionEvent),
}

/// Состояние карточки «обновление лаунчера» на странице About.
#[derive(Default)]
pub enum UpdateUi {
    #[default]
    Idle,
    Checking,
    Found(updater::AppRelease),
    UpToDate,
    Downloading { done: u64, total: u64 },
    Error(String),
}

#[derive(Default)]
pub struct InstallUi {
    pub active: bool,
    pub phase: deploy::Phase,
    pub done: u64,
    pub total: u64,
    pub error: Option<String>,
}

/// Очередь конфликтов импорта флагов (окно спроса).
pub struct ImportQueue {
    pub conflicts: VecDeque<(String, String)>,
    pub replace_all: bool,
    pub keep_rest: bool,
    pub applied: usize,
    pub skipped: usize,
}

/// Подтверждаемые действия (диалог Да/Нет).
pub enum Confirm {
    ClearAll,
    ApplyProfile(String),
    DeleteProfile(String),
    ApplyChannel(String),
    RevertMods,
    LoadModProfile(String),
    DeleteModProfile(String),
}

/// Какая страница показывает модалку «Сохранить как…».
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProfileDlg {
    Flags,
    Mods,
}

pub struct App {
    pub lang: Lang,
    pub page: Page,
    pub config: Config,
    pub status: String,
    pub installed: Option<String>,
    pub latest: Option<deploy::ClientVersion>,
    pub latest_error: Option<String>,
    pub install: InstallUi,
    /// Таблица FastFlags (имя, значение).
    pub flags: Vec<(String, String)>,
    pub flags_dirty: bool,
    pub flags_search: String,
    pub flags_selected: Option<usize>,
    /// GUID версии, из которой загружена таблица.
    pub flags_loaded_for: Option<String>,
    pub flag_profile_name: String,
    /// Выбранный в комбобоксе профиль (общий для страниц Flags/Mods).
    pub profile_sel: String,
    /// Открытая модалка «Сохранить как…».
    pub profile_dlg: Option<ProfileDlg>,
    /// Имя профиля модов в поле ввода на странице Mods.
    pub mod_profile_name: String,
    /// (текст, ошибка) — строка-сообщение над таблицей флагов.
    pub flag_msg: Option<(String, bool)>,
    pub confirm: Option<Confirm>,
    pub import: Option<ImportQueue>,
    pub update: UpdateUi,
    /// Просьба закрыть окно (после подмены exe апдейтером).
    pub quit: bool,
    pub selected_version: Option<String>,
    pub integrity: Option<versions::IntegrityReport>,
    /// Черновик канала на странице Settings (до применения).
    pub channel_draft: String,
    /// Превью версии для черновика канала.
    pub channel_preview: Option<Result<deploy::ClientVersion, String>>,
    pub channel_checking: bool,
    /// Результат последнего применения/отката модов.
    pub mods_msg: Option<(String, bool)>,
    /// Файлы включённых модов (относительные пути) для таблицы Mods.
    pub mods_files: Vec<crate::core::mods::ModFile>,
    /// Поле «Запуск по ссылке» на Home.
    pub link_input: String,
    /// (текст, ошибка) — фидбек строки «Запуск по ссылке».
    pub link_msg: Option<(String, bool)>,
    /// Моды на диске (папки + состояние) для страницы Mods.
    pub mods_list: Vec<crate::core::mods::ModInfo>,
    /// Первый scan модов уже выполнен.
    pub mods_scanned: bool,
    /// Идёт ли очистка мусора.
    pub cleanup_busy: bool,
    /// Результат последней очистки: (текст, ошибка).
    pub cleanup_msg: Option<(String, bool)>,
    /// Секунды текущей игровой сессии (таймер на Home).
    pub session_secs: u64,
    /// Текущее место сессии (id из лога).
    pub session_place: Option<String>,
    /// Что показывать в Discord RP (имя места, fallback — id).
    session_display: Option<String>,
    /// Идёт ли сейчас отсчёт сессии.
    pub session_active: bool,
    /// Фоновый воркер Discord Rich Presence.
    discord: discord::Handle,
    /// Приёмник статуса подключения Discord RP.
    discord_rx: discord::StatusRx,
    /// Последний известный статус подключения Discord RP.
    pub discord_status: discord::Status,
    /// Deep link, ожидающий установки (запустится после Finished).
    pending_launch: Option<String>,
    /// Регистрация roblox:// в реестре для UI (None — не Windows).
    pub deeplink_ok: Option<bool>,
    /// Накопленное время по местам и суммарно.
    pub playtime: Playtime,
    pub(crate) tx: Sender<AsyncEvent>,
    rx: Receiver<AsyncEvent>,
    cancel: Option<Arc<AtomicBool>>,
}

/// Активная версия: сначала выбранная в конфиге (если целая),
/// иначе первая целая установка на диске.
pub fn effective_installed(config: &Config) -> Option<String> {
    let base = config.base();
    let iv = &config.installed_version;
    if !iv.is_empty() && versions::is_version_guid(iv) {
        let dir = base.join("Versions").join(iv);
        if dir.join(deploy::PLAYER_EXE).exists() && dir.join("AppSettings.xml").exists() {
            return Some(iv.clone());
        }
    }
    versions::find_installed(&base)
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        theme::apply(&cc.egui_ctx);
        let config = Config::load();
        let lang = Lang::resolve(&config.language);
        let installed = effective_installed(&config);

        let (tx, rx) = std::sync::mpsc::channel();
        let quiet_tx = tx.clone();
        let channel = config.channel.clone();
        std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .expect("tokio runtime");
            let res = rt
                .block_on(deploy::latest_version(&channel))
                .map_err(|e| e.to_string());
            let _ = quiet_tx.send(AsyncEvent::Latest(res));
        });

        let channel_draft = config.channel.clone();
        let playtime = Playtime::load(&config.base());
        let (discord, discord_rx) = discord::spawn();
        let mut app = Self {
            lang,
            page: Page::Home,
            config,
            status: String::new(),
            installed,
            latest: None,
            latest_error: None,
            install: InstallUi::default(),
            flags: Vec::new(),
            flags_dirty: false,
            flags_search: String::new(),
            flags_selected: None,
            flags_loaded_for: None,
            flag_profile_name: String::new(),
            profile_sel: String::new(),
            profile_dlg: None,
            mod_profile_name: String::new(),
            flag_msg: None,
            confirm: None,
            import: None,
            update: UpdateUi::Idle,
            quit: false,
            selected_version: None,
            integrity: None,
            channel_draft,
            channel_preview: None,
            channel_checking: false,
            mods_msg: None,
            mods_files: Vec::new(),
            link_input: String::new(),
            link_msg: None,
            mods_list: Vec::new(),
            mods_scanned: false,
            cleanup_busy: false,
            cleanup_msg: None,
            session_secs: 0,
            session_place: None,
            session_display: None,
            session_active: false,
            discord,
            discord_rx,
            discord_status: discord::Status::Idle,
            pending_launch: None,
            deeplink_ok: None,
            playtime,
            tx,
            rx,
            cancel: None,
        };
        // roblox:// в реестре: самовосстановление + статус для Settings.
        if app.config.deeplink {
            deep_link::ensure();
        }
        app.deeplink_ok = deep_link::status();
        if app.config.auto_update {
            app.check_updates();
        }
        app
    }

    pub fn set_lang(&mut self, lang: Lang) {
        self.lang = lang;
        self.config.language = lang.code().to_string();
        self.config.save();
    }

    /// Перечитать список модов и файлов включённых модов с диска.
    pub fn refresh_mods(&mut self) {
        let base = self.config.base();
        self.mods_list = mods::scan_mods(&base);
        self.mods_files = mods::scan_enabled(&base);
        self.mods_scanned = true;
    }

    /// Применить моды к активной версии (в фоне).
    pub fn start_mods_apply(&mut self) {
        let Some(guid) = self.installed.clone() else {
            return;
        };
        let base = self.config.base();
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let res = mods::apply(&base, &guid).map_err(|e| e.to_string());
            let _ = tx.send(AsyncEvent::ModsDone(res));
        });
    }

    /// Откатить моды (в фоне).
    pub fn start_mods_revert(&mut self) {
        let Some(guid) = self.installed.clone() else {
            return;
        };
        let base = self.config.base();
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let res = mods::revert(&base, &guid).map_err(|e| e.to_string());
            let _ = tx.send(AsyncEvent::ModsDone(res));
        });
    }

    /// Пере-применить моды после изменения списка (в фоне): откат → применение.
    pub fn start_mods_reapply(&mut self) {
        let Some(guid) = self.installed.clone() else {
            return;
        };
        let base = self.config.base();
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let _ = mods::revert(&base, &guid); // бэкапов может не быть — ок
            let res = mods::apply(&base, &guid).map_err(|e| e.to_string());
            let _ = tx.send(AsyncEvent::ModsDone(res));
        });
    }

    /// Загрузить профиль модов (в фоне): откат → замена `Mods/` → применение.
    pub fn start_mod_profile_load(&mut self, name: String) {
        let base = self.config.base();
        let guid = self.installed.clone();
        self.mods_msg = None;
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let res = mods::load_profile(&base, guid.as_deref(), &name)
                .map(|stats| (name, stats))
                .map_err(|e| e.to_string());
            let _ = tx.send(AsyncEvent::ProfileLoaded(res));
        });
    }

    /// Очистка мусора (в фоне).
    pub fn start_cleanup(&mut self) {
        if self.cleanup_busy || self.install.active {
            return;
        }
        let base = self.config.base();
        self.cleanup_busy = true;
        self.cleanup_msg = None;
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let r = crate::core::cleanup::clean(&base);
            let _ = tx.send(AsyncEvent::CleanupDone(r.freed, r.files));
        });
    }

    /// Перетащенные в окно файлы → папка модов (только на странице Mods).
    fn handle_drops(&mut self, ctx: &egui::Context) {
        if self.page != Page::Mods {
            return;
        }
        let dropped: Vec<std::path::PathBuf> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .map(|f| f.path().to_path_buf())
                .collect()
        });
        if dropped.is_empty() {
            return;
        }
        match mods::import_paths(&self.config.base(), &dropped) {
            Ok(n) => {
                self.refresh_mods();
                let text = fmt(self.lang, "Mods_ImportedFmt", &[&n.to_string()]);
                self.mods_msg = Some((text, false));
            }
            Err(e) => self.mods_msg = Some((e.to_string(), true)),
        }
    }

    /// Проверка latest-релиза лаунчера на GitHub.
    pub fn check_updates(&mut self) {
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
    pub fn start_update(&mut self, rel: updater::AppRelease) {
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

    pub fn update_available(&self) -> bool {
        match (&self.installed, &self.latest) {
            (Some(inst), Some(lat)) => inst != &lat.guid,
            (None, Some(_)) => true,
            _ => false,
        }
    }

    pub fn start_install(&mut self, force: bool) {
        if self.install.active {
            return;
        }
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

    /// Ремонт конкретной версии: полная перезапаковка её же пакетов.
    pub fn start_repair(&mut self, guid: String) {
        if self.install.active {
            return;
        }
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
                &cfg, &guid, true, false, &etx, &cancel, repair_for.as_deref(),
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

    pub fn activate_version(&mut self, guid: &str) {
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
    }

    pub fn delete_version(&mut self, guid: &str) {
        if self.install.active || Some(guid) == self.installed.as_deref() {
            return;
        }
        if versions::delete(&self.config.base(), guid).is_ok() {
            if self.config.installed_version == guid {
                self.config.installed_version.clear();
                self.config.save();
            }
            if self.selected_version.as_deref() == Some(guid) {
                self.selected_version = None;
            }
            self.integrity = None;
            self.installed = effective_installed(&self.config);
        }
    }

    pub fn check_integrity(&mut self) {
        let Some(guid) = self
            .selected_version
            .clone()
            .or_else(|| effective_installed(&self.config))
        else {
            return;
        };
        let dir = self.config.base().join("Versions").join(guid);
        self.integrity = versions::check(&dir).ok();
    }

    pub fn cancel_install(&mut self) {
        if let Some(c) = &self.cancel {
            c.store(true, Ordering::Relaxed);
        }
    }

    pub fn play(&mut self) {
        self.play_arg(None);
    }

    /// Запуск по ссылке из поля на Home (https://roblox.com/games/… или roblox://…).
    pub fn play_link(&mut self, input: &str) {
        match crate::core::roblox_link::parse_arg(input) {
            Some(url) => {
                self.link_msg = Some((tr(self.lang, "Home_LinkOk").to_string(), false));
                self.play_arg(Some(&url));
            }
            None => {
                self.link_msg = Some((tr(self.lang, "Home_LinkBad").to_string(), true));
            }
        }
    }

    /// Запуск игры. `arg` — roblox:// deep link (друзья, приватки, плейсы).
    pub fn play_arg(&mut self, arg: Option<&str>) {
        let guid = self
            .installed
            .clone()
            .or_else(|| versions::find_installed(&self.config.base()));
        let Some(guid) = guid else {
            // Версии нет — ставим и запустим с ссылкой после Finished.
            self.pending_launch = arg.map(str::to_owned);
            self.start_install(false);
            return;
        };
        match launcher::exe_path(&self.config.base(), &guid) {
            Some(exe) => match launcher::launch(&exe, arg) {
                Ok(()) => {
                    self.install.error = None;
                    self.start_session_watch();
                    if self.config.close_on_launch {
                        self.quit = true;
                    }
                }
                Err(e) => self.install.error = Some(e.to_string()),
            },
            None => {
                self.pending_launch = arg.map(str::to_owned);
                self.start_install(false);
            }
        }
    }

    /// Фоновый watcher сессии: Tick/End из playtime::watch_session.
    fn start_session_watch(&mut self) {
        if self.session_active {
            return;
        }
        self.session_active = true;
        self.session_secs = 0;
        self.session_place = None;
        self.session_display = None;
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let (stx, srx) = std::sync::mpsc::channel::<SessionEvent>();
            let relay = {
                let tx = tx.clone();
                std::thread::spawn(move || {
                    while let Ok(e) = srx.recv() {
                        if tx.send(AsyncEvent::Session(e)).is_err() {
                            break;
                        }
                    }
                })
            };
            playtime::watch_session(stx);
            let _ = relay.join();
        });
    }

    /// Синхронизировать Discord RP с конфигом: выключили — Idle,
    /// включили — текущая сессия (если идёт).
    pub fn sync_discord(&self) {
        if !self.config.discord_rp || !self.session_active {
            let _ = self.discord.send(discord::Update::Idle);
        } else if self.session_display.is_some() {
            self.push_discord(self.session_secs);
        }
    }

    /// Отправить в Discord активность по настройкам RP (что показывать).
    fn push_discord(&self, secs: u64) {
        let details = self
            .config
            .discord_rp_brand
            .then(|| "ByteStrap".to_string());
        let state = if self.config.discord_rp_place {
            self.session_display.clone()
        } else {
            None
        };
        let start = self
            .config
            .discord_rp_time
            .then(|| playtime::now_unix().saturating_sub(secs));
        let msg = if details.is_some() || state.is_some() || start.is_some() {
            discord::Update::Session {
                details,
                state,
                start,
            }
        } else {
            discord::Update::Idle
        };
        let _ = self.discord.send(msg);
    }

    fn drain_events(&mut self) {
        while let Ok(s) = self.discord_rx.try_recv() {
            self.discord_status = s;
        }
        while let Ok(ev) = self.rx.try_recv() {
            match ev {
                AsyncEvent::Latest(Ok(v)) => {
                    self.latest = Some(v);
                    self.latest_error = None;
                }
                AsyncEvent::Latest(Err(e)) => {
                    self.latest_error = Some(e);
                }
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
                AsyncEvent::ChannelPreview(res) => {
                    self.channel_checking = false;
                    self.channel_preview = Some(res);
                }
                AsyncEvent::ModsDone(res) => match res {
                    Ok(stats) => {
                        let text = fmt(
                            self.lang,
                            "Mods_AppliedFmt",
                            &[&stats.applied.to_string(), &stats.skipped.to_string()],
                        );
                        self.mods_msg = Some((text, false));
                        self.refresh_mods();
                    }
                    Err(e) => self.mods_msg = Some((e, true)),
                },
                AsyncEvent::ProfileLoaded(res) => {
                    // Дерево меняется в любом случае — пересканировать.
                    self.refresh_mods();
                    match res {
                        Ok((name, _stats)) => {
                            if self.config.active_mod_profile != name {
                                self.config.active_mod_profile = name.clone();
                                self.config.save();
                            }
                            self.profile_sel = name.clone();
                            let text = fmt(self.lang, "Mods_ProfileLoadedFmt", &[&name]);
                            self.mods_msg = Some((text, false));
                        }
                        Err(e) => self.mods_msg = Some((e, true)),
                    }
                }
                AsyncEvent::CleanupDone(freed, files) => {
                    self.cleanup_busy = false;
                    let text = if files == 0 {
                        tr(self.lang, "Settings_CleanupNone").to_string()
                    } else {
                        fmt(
                            self.lang,
                            "Settings_CleanupDoneFmt",
                            &[
                                &versions::format_size(freed),
                                &files.to_string(),
                            ],
                        )
                    };
                    self.cleanup_msg = Some((text, false));
                }
                AsyncEvent::Session(ev) => match ev {
                    SessionEvent::Tick {
                        secs,
                        place,
                        place_name,
                    } => {
                        self.session_secs = secs;
                        // Discord RP: state = имя места (fallback — id).
                        self.session_display = place_name.or_else(|| place.clone());
                        if self.config.discord_rp {
                            self.push_discord(secs);
                        }
                        self.session_place = place;
                    }
                    SessionEvent::End { total, places } => {
                        self.session_active = false;
                        self.session_secs = 0;
                        self.session_place = None;
                        self.session_display = None;
                        let _ = self.discord.send(discord::Update::Idle);
                        self.playtime.merge(total, &places);
                        self.playtime.save(&self.config.base());
                    }
                },
                AsyncEvent::Install(ev) => match ev {
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
                        self.integrity = None;
                        // Моды: после чистой установки накладываем заново.
                        if !already {
                            self.refresh_mods();
                            if self.mods_list.iter().any(|m| m.enabled) {
                                self.start_mods_apply();
                            }
                        }
                        // Deep link ждал установку — запускаем игру с ним.
                        if let Some(url) = self.pending_launch.take() {
                            self.play_arg(Some(&url));
                        }
                    }
                    deploy::Event::Failed(e) => {
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
                },
            }
        }
    }

    fn compute_status(&mut self) {
        self.status = if self.install.active {
            match self.install.phase {
                deploy::Phase::Check => fmt(self.lang, "Home_Status_Checking", &[]),
                deploy::Phase::Download => {
                    let pct = if self.install.total > 0 {
                        (self.install.done * 100 / self.install.total).to_string()
                    } else {
                        "…".into()
                    };
                    fmt(self.lang, "Home_Status_Downloading", &[&pct])
                }
                deploy::Phase::Extract => fmt(
                    self.lang,
                    "Home_Status_Extracting",
                    &[
                        &self.install.done.to_string(),
                        &self.install.total.to_string(),
                    ],
                ),
                deploy::Phase::Setup => fmt(self.lang, "Home_Status_Setup", &[]),
            }
        } else if let Some(err) = &self.install.error {
            fmt(self.lang, "Home_Status_Error", &[err])
        } else {
            String::new()
        };
    }

    fn titlebar(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        let status = self.status.clone();
        let lang = self.lang;

        Panel::top("titlebar")
            .exact_size(44.0)
            .frame(Frame::NONE.fill(theme::BG))
            .show(ui, |ui| {
                let rect = ui.max_rect();

                let drag = ui.interact(rect, ui.scope_id().with("titlebar_drag"), Sense::click_and_drag());
                if drag.drag_started() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                }
                if drag.double_clicked() {
                    let maximized = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
                    ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
                }
                if drag.hovered() {
                    ctx.set_cursor_icon(CursorIcon::Default);
                }

                let logo = Rect::from_min_size(rect.min + Vec2::new(16.0, 5.0), Vec2::splat(34.0));
                ui.painter().rect_filled(logo, 6.0, theme::FG);
                ui.painter().text(
                    logo.center(),
                    egui::Align2::CENTER_CENTER,
                    "B",
                    FontId::new(17.0, FontFamily::Name("heading".into())),
                    theme::BG,
                );
                ui.painter().text(
                    Pos2::new(logo.right() + 10.0, rect.top() + 16.0),
                    egui::Align2::LEFT_CENTER,
                    "ByteStrap",
                    FontId::new(13.0, FontFamily::Proportional),
                    theme::FG,
                );
                ui.painter().text(
                    Pos2::new(logo.right() + 10.0, rect.top() + 31.0),
                    egui::Align2::LEFT_CENTER,
                    tr(lang, "App_Sub"),
                    FontId::new(9.0, FontFamily::Monospace),
                    theme::FG_DIMMER,
                );

                if !status.is_empty() {
                    ui.painter().text(
                        Pos2::new(rect.center().x, rect.center().y),
                        egui::Align2::CENTER_CENTER,
                        &status,
                        FontId::new(11.5, FontFamily::Monospace),
                        theme::FG_DIM,
                    );
                }

                let btn_size = Vec2::new(46.0, 44.0);
                let maximized = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
                let buttons: [(egui::ViewportCommand, &str, &str); 3] = [
                    (egui::ViewportCommand::Minimized(true), "—", tr(lang, "Win_Min")),
                    (
                        egui::ViewportCommand::Maximized(!maximized),
                        if maximized { "❐" } else { "▢" },
                        tr(lang, "Win_Max"),
                    ),
                    (egui::ViewportCommand::Close, "✕", tr(lang, "Win_Close")),
                ];

                let mut x = rect.right() - btn_size.x * 3.0;
                for (cmd, glyph, tip) in buttons {
                    let r = Rect::from_min_size(Pos2::new(x, rect.top()), btn_size);
                    let resp = ui.interact(r, ui.scope_id().with(("winbtn", glyph)), Sense::click());
                    let hovered = resp.hovered();
                    let fill = if hovered && glyph == "✕" {
                        theme::BAD
                    } else if hovered {
                        theme::LINE_SOFT
                    } else {
                        Color32::TRANSPARENT
                    };
                    ui.painter().rect_filled(r, 0.0, fill);
                    ui.painter().text(
                        r.center(),
                        egui::Align2::CENTER_CENTER,
                        glyph,
                        FontId::new(13.0, FontFamily::Proportional),
                        if hovered && glyph == "✕" {
                            theme::FG
                        } else {
                            theme::FG_DIM
                        },
                    );
                    if hovered {
                        resp.clone().on_hover_text(tip);
                    }
                    if resp.clicked() {
                        ctx.send_viewport_cmd(cmd);
                    }
                    x += btn_size.x;
                }
            });
    }

    fn sidebar(&mut self, ui: &mut egui::Ui) {
        let page = self.page;
        let lang = self.lang;

        Panel::left("sidebar")
            .exact_size(236.0)
            .frame(
                Frame::NONE
                    .fill(theme::SIDEBAR)
                    .inner_margin(Margin::symmetric(12, 14)),
            )
            .show(ui, |ui| {
                ui.add_space(4.0);

                let items: [(Page, &str, fn(&egui::Ui, Rect, Color32)); 6] = [
                    (Page::Home, tr(lang, "Nav_Home"), ui::icons::home),
                    (Page::Flags, tr(lang, "Nav_Flags"), ui::icons::sliders),
                    (Page::Versions, tr(lang, "Nav_Versions"), ui::icons::layers),
                    (Page::Mods, tr(lang, "Nav_Mods"), ui::icons::wrench),
                    (Page::Settings, tr(lang, "Nav_Settings"), ui::icons::gear),
                    (Page::About, tr(lang, "Nav_About"), ui::icons::info),
                ];

                for (p, label, icon_fn) in items {
                    let active = p == page;
                    let rect = ui.available_rect_before_wrap();
                    let r = Rect::from_min_size(rect.min, Vec2::new(rect.width(), 46.0));
                    let resp = ui.interact(r, ui.scope_id().with(("nav", label)), Sense::click());
                    ui.allocate_space(Vec2::new(0.0, 50.0));

                    let fill = if active {
                        theme::LINE_SOFT
                    } else if resp.hovered() {
                        theme::LINE_SOFT.gamma_multiply(0.55)
                    } else {
                        Color32::TRANSPARENT
                    };
                    ui.painter().rect_filled(r, theme::R_SOFT, fill);
                    if active {
                        ui.painter().rect_filled(
                            Rect::from_min_size(
                                Pos2::new(r.left(), r.top() + 10.0),
                                Vec2::new(2.0, 26.0),
                            ),
                            1.0,
                            theme::FG,
                        );
                    }

                    let icon_color = if active { theme::FG } else { theme::FG_DIMMER };
                    icon_fn(
                        ui,
                        Rect::from_center_size(
                            Pos2::new(r.left() + 24.0, r.center().y),
                            Vec2::splat(18.0),
                        ),
                        icon_color,
                    );
                    ui.painter().text(
                        Pos2::new(r.left() + 44.0, r.center().y),
                        egui::Align2::LEFT_CENTER,
                        label,
                        FontId::new(13.0, FontFamily::Proportional),
                        if active { theme::FG } else { theme::FG_DIM },
                    );

                    if resp.clicked() {
                        self.page = p;
                    }
                }

                ui.with_layout(Layout::bottom_up(Align::Min), |ui| {
                    ui.horizontal(|ui| {
                        let (r, _) = ui.allocate_exact_size(Vec2::new(8.0, 12.0), Sense::hover());
                        ui.painter().circle_filled(r.left_center(), 3.0, theme::GOOD);
                        ui.label(
                            RichText::new(tr(lang, "Footer_Ready"))
                                .size(11.0)
                                .family(egui::FontFamily::Monospace)
                                .color(theme::FG_DIMMER),
                        );
                    });
                });
            });
    }

    fn resize_grips(&self, ui: &egui::Ui, ctx: &egui::Context) {
        let screen = ui.input(|i| {
            i.viewport()
                .inner_rect
                .unwrap_or_else(|| i.content_rect())
        });
        let m = 6.0;
        let corners: [(Rect, egui::ViewportCommand); 8] = [
            (
                Rect::from_min_size(screen.min, Vec2::splat(m * 2.0)),
                egui::ViewportCommand::BeginResize(egui::ResizeDirection::NorthWest),
            ),
            (
                Rect::from_min_size(
                    Pos2::new(screen.right() - m * 2.0, screen.top()),
                    Vec2::splat(m * 2.0),
                ),
                egui::ViewportCommand::BeginResize(egui::ResizeDirection::NorthEast),
            ),
            (
                Rect::from_min_size(
                    Pos2::new(screen.left(), screen.bottom() - m * 2.0),
                    Vec2::splat(m * 2.0),
                ),
                egui::ViewportCommand::BeginResize(egui::ResizeDirection::SouthWest),
            ),
            (
                Rect::from_min_size(screen.max - Vec2::splat(m * 2.0), Vec2::splat(m * 2.0)),
                egui::ViewportCommand::BeginResize(egui::ResizeDirection::SouthEast),
            ),
            (
                Rect::from_min_size(
                    Pos2::new(screen.left(), screen.top() + m * 2.0),
                    Vec2::new(m, screen.height() - m * 4.0),
                ),
                egui::ViewportCommand::BeginResize(egui::ResizeDirection::West),
            ),
            (
                Rect::from_min_size(
                    Pos2::new(screen.right() - m, screen.top() + m * 2.0),
                    Vec2::new(m, screen.height() - m * 4.0),
                ),
                egui::ViewportCommand::BeginResize(egui::ResizeDirection::East),
            ),
            (
                Rect::from_min_size(
                    Pos2::new(screen.left() + m * 2.0, screen.top()),
                    Vec2::new(screen.width() - m * 4.0, m),
                ),
                egui::ViewportCommand::BeginResize(egui::ResizeDirection::North),
            ),
            (
                Rect::from_min_size(
                    Pos2::new(screen.left() + m * 2.0, screen.bottom() - m),
                    Vec2::new(screen.width() - m * 4.0, m),
                ),
                egui::ViewportCommand::BeginResize(egui::ResizeDirection::South),
            ),
        ];
        for (i, (rect, cmd)) in corners.into_iter().enumerate() {
            let resp = ui.interact(rect, ui.scope_id().with(("resize", i)), Sense::click_and_drag());
            if resp.hovered() {
                ctx.set_cursor_icon(match i {
                    0 | 3 => CursorIcon::ResizeNwSe,
                    1 | 2 => CursorIcon::ResizeNeSw,
                    4 | 5 => CursorIcon::ResizeHorizontal,
                    _ => CursorIcon::ResizeVertical,
                });
            }
            if resp.drag_started() {
                ctx.send_viewport_cmd(cmd);
            }
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.drain_events();
        self.handle_drops(&ctx);
        self.compute_status();
        if self.quit {
            self.quit = false;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }

        self.titlebar(&ctx, ui);
        self.sidebar(ui);

        let page = self.page;
        egui::CentralPanel::default()
            .frame(
                Frame::NONE
                    .fill(theme::BG)
                    .inner_margin(Margin::symmetric(32, 26)),
            )
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| match page {
                        Page::Home => ui::home::show(ui, self),
                        Page::Flags => ui::flags::show(ui, self),
                        Page::Versions => ui::versions::show(ui, self),
                        Page::Mods => ui::mods::show(ui, self),
                        Page::Settings => ui::settings::show(ui, self),
                        Page::About => ui::about::show(ui, self),
                    });
            });

        self.resize_grips(ui, &ctx);
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.config.save();
    }
}
