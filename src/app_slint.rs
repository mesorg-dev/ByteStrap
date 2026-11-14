//! Контроллер интерфейса (единственный UI-слой, Фаза 2 — egui удалён).
//!
//! Вся логика лаунчера живёт здесь: статус, установка, запуск, сессия игры,
//! счётчики, страница «Логи» (tail/follow файлов Roblox + кольцевой буфер
//! лаунчера). .slint-файлы — только отрисовка.

slint::include_modules!();

use std::cell::RefCell;
use std::collections::{BTreeMap, VecDeque};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};

#[cfg(windows)]
use slint::winit_030::{winit, EventResult, WinitWindowAccessor};
use slint::{ComponentHandle, ModelRc, SharedString, Timer, TimerMode, VecModel, Weak};

use crate::core::config::Config;
use crate::core::logview::{self, Follow, Line as SrcLine};
use crate::core::playtime::{self, Playtime, SessionEvent};
use crate::core::places::{Places};
use crate::core::flags::FlagError;
use crate::core::overlay;
use crate::core::{deep_link, deploy, discord, flags, launcher, logger, mods, presets, roblox_link, updater, versions};
use crate::i18n::{fmt, tr, Lang};
use crate::tray;

// ===== Общие типы (перенесены из src/app.rs в Фазе 2) =====

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
    /// Действие пользователя в трее (Windows).
    #[cfg(windows)]
    Tray(tray::Action),
    /// Имя места получено: (id, Some(имя) | None).
    PlaceName((u64, Option<String>)),
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

/// Активная версия клиента с проверкой целостности установки.
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

// ===== Состояние =====

/// Что подтверждает общая модалка (тело собирает push_confirm).
#[derive(Clone)]
enum Confirm {
    ApplyChannel,
    RevertMods,
    LoadModProfile(String),
    DeleteModProfile(String),
    ClearAll,
    ApplyProfile(String),
    DeleteProfile(String),
}

/// Какой вводной диалог открыт («Сохранить как…»).
#[derive(Clone, Copy)]
enum Dlg {
    ModsProfile,
    FlagsProfile,
}

/// Очередь конфликтов импорта FastFlags (окно спрашивает по одному).
struct ImportQueue {
    conflicts: VecDeque<(String, String)>,
    replace_all: bool,
    keep_rest: bool,
    applied: usize,
    skipped: usize,
}

/// Локализация опций пресетов (канон EN → RU).
fn local_option(lang: Lang, opt: &str) -> String {
    if lang != Lang::Ru {
        return opt.to_string();
    }
    match opt {
        "Auto" => "Авто".into(),
        "Low" => "Низкая".into(),
        "Medium" => "Средняя".into(),
        "High" => "Высокая".into(),
        "Off" => "Выкл".into(),
        "Far" => "Дальняя".into(),
        _ => opt.to_string(),
    }
}

fn set_flag_pair(flags: &mut Vec<(String, String)>, name: &str, value: &str) {
    match flags.iter_mut().find(|(n, _)| n == name) {
        Some(row) => row.1 = value.to_string(),
        None => flags.push((name.to_string(), value.to_string())),
    }
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

/// Параметры оверлея из конфига.
fn push_overlay(c: &Ctx) -> overlay::Params {
    overlay::Params {
        enabled: c.config.overlay_on,
        bright: c.config.overlay_bright,
        satur: c.config.overlay_sat,
        contrast: c.config.overlay_contrast,
    }
}

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

    // — Настройки —
    channel_draft: String,
    channel_preview: Option<Result<deploy::ClientVersion, String>>,
    channel_checking: bool,
    confirm: Option<Confirm>,
    cleanup_busy: bool,
    cleanup_msg: Option<(String, bool)>,
    discord: discord::Handle,
    discord_rx: discord::StatusRx,
    discord_status: discord::Status,
    deeplink_ok: Option<bool>,
    pending_launch: Option<String>,
    mods_count: usize,
    flags_count: usize,
    playtime: Playtime,
    /// Избранное/недавние места (страница Games).
    places: Places,
    /// (текст, ошибка) — строка под полем ссылки на Home.
    link_msg: Option<(String, bool)>,
    /// (текст, ошибка) — строка под полем placeId на Games.
    games_msg: Option<(String, bool)>,
    /// (текст, ошибка) — статус экспорта/импорта конфигурации.
    cfg_msg: Option<(String, bool)>,
    /// Оверлей «Картинка клиента» поверх окна Roblox.
    overlay: overlay::Handle,

    // — Сессия игры (Tick/End из playtime::watch_session) —
    session_active: bool,
    session_secs: u64,
    session_place: Option<String>,
    session_display: Option<String>,

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

    // — Моды —
    mods_list: Vec<mods::ModInfo>,
    mods_files: Vec<mods::ModFile>,
    mods_scanned: bool,
    /// Модели модов изменились — пересобрать при следующем тике
    /// (иначе пересборка каждые 100 мс съедала бы фокус и клики).
    mods_dirty: bool,
    mods_msg: Option<(String, bool)>,
    /// Выбранный профиль (общий для страниц Mods/Flags).
    profile_sel: String,
    /// Открытый вводной диалог (None — закрыт).
    save_dlg: Option<Dlg>,
    dlg_input: String,

    // — FastFlags —
    flags: Vec<(String, String)>,
    flags_loaded_for: Option<String>,
    flags_selected: Option<usize>,
    flags_dirty: bool,
    flags_search: String,
    flag_msg: Option<(String, bool)>,
    import: Option<ImportQueue>,
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
            Some(exe) => {
                // FastFlags: гарантированно на месте непосредственно перед
                // запуском (после любых проверок/обновлений клиента).
                self.ensure_flags_before_launch(&guid);
                match launcher::launch(&exe, arg) {
                    Ok(()) => {
                        self.install.error = None;
                        logger::info("Клиент Roblox запущен");
                        self.start_session_watch();
                        if self.config.close_on_launch {
                            self.quit = true;
                        }
                    }
                    Err(e) => {
                        logger::error(format!("Не удалось запустить клиента: {e}"));
                        self.install.error = Some(e.to_string());
                    }
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

    /// Флаги перед запуском: активный профиль (или правки из памяти) пишем в
    /// ClientSettings установленной версии — на случай затёртых апдейтом.
    /// Пустой набор не пишем, чтобы не стирать сохранённый файл.
    fn ensure_flags_before_launch(&mut self, guid: &str) {
        let map: BTreeMap<String, String> =
            if !self.config.active_flag_profile.is_empty() {
                let profiles = flags::load_profiles(&self.config.base());
                match profiles.profiles.get(&self.config.active_flag_profile) {
                    Some(p) => flags::profile_to_flags(p).into_iter().collect(),
                    None => self.flags.iter().cloned().collect(),
                }
            } else {
                self.flags.iter().cloned().collect()
            };
        if map.is_empty() {
            return;
        }
        let dir = self.flags_dir(guid);
        match flags::save(&dir, &map) {
            Ok(()) => {
                self.flags = map.into_iter().collect();
                self.flags_dirty = false;
            }
            Err(e) => logger::warn(format!("FastFlags: не удалось записать перед запуском: {e}")),
        }
    }

    /// placeId из ввода: число, roblox://…placeId=N или ссылка roblox.com/games/N/…
    fn parse_place(input: &str) -> Option<u64> {
        let t = input.trim();
        if t.bytes().all(|b| b.is_ascii_digit()) && !t.is_empty() {
            return t.parse().ok();
        }
        if let Some(url) = roblox_link::parse_arg(t) {
            // roblox://placeId=N | roblox://experiences/start?placeId=N&…
            let after = url.rsplit("placeId=").next()?;
            let digits: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
            return digits.parse().ok();
        }
        None
    }

    /// Кнопка «Играть» на Games: разобрать ввод, запомнить место и запустить.
    fn games_go(&mut self, input: &str) {
        let Some(id) = Self::parse_place(input) else {
            self.games_msg = Some((tr(self.lang, "Games_BadId").to_string(), true));
            return;
        };
        self.launch_place(id);
    }

    /// Запуск по placeId из списка: регистрация + roblox://.
    fn launch_place(&mut self, id: u64) {
        self.places.launch(id, &self.config.base());
        self.places.save(&self.config.base());
        let url = format!("roblox://placeId={id}");
        self.play(Some(&url));
        let ok = self.install.error.is_none();
        let text = if ok {
            fmt(self.lang, "Games_AddedFmt", &[&id.to_string()])
        } else {
            tr(self.lang, "Games_StartErr").to_string()
        };
        self.games_msg = Some((text, !ok));
        // Имя места — в фоне (Roblox API).
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build();
            let name = match rt {
                Ok(rt) => rt.block_on(playtime::fetch_place_name(&id.to_string())),
                Err(_) => None,
            };
            let _ = tx.send(AsyncEvent::PlaceName((id, name)));
        });
    }

    fn games_fav(&mut self, id: &str) {
        let Ok(id) = id.parse::<u64>() else { return };
        self.places.toggle_fav(id, &self.config.base());
    }

    fn games_remove(&mut self, id: &str) {
        let Ok(id) = id.parse::<u64>() else { return };
        let name = self
            .places
            .sorted()
            .into_iter()
            .find(|e| e.id == id)
            .map(|e| e.name.clone())
            .unwrap_or_default();
        self.places.remove(id, &self.config.base());
        self.games_msg = Some((
            fmt(self.lang, "Games_RemovedFmt", &[&if name.is_empty() { id.to_string() } else { name }]),
            false,
        ));
    }

    fn push_games(&self, ui: &AppWindow) {
        ui.set_games_msg(
            self.games_msg
                .as_ref()
                .map(|(t, _)| t.clone())
                .unwrap_or_default()
                .into(),
        );
        ui.set_games_msg_err(self.games_msg.as_ref().map(|(_, e)| *e).unwrap_or(false));

        let rows: Vec<GameRow> = self
            .places
            .sorted()
            .iter()
            .enumerate()
            .map(|(i, e)| {
                let day = playtime::day_key(e.unix);
                let ddmm = if day.len() >= 10 {
                    format!("{}.{}", &day[8..10], &day[5..7])
                } else {
                    String::new()
                };
                GameRow {
                    id: e.id.to_string().into(),
                    name: if e.name.is_empty() {
                        e.id.to_string()
                    } else {
                        e.name.clone()
                    }
                    .into(),
                    visits: e.visits.to_string().into(),
                    last: ddmm.into(),
                    fav: e.fav,
                    alt: i % 2 == 1,
                    bad: e.name.is_empty(),
                }
            })
            .collect();
        ui.set_games_rows(ModelRc::new(VecModel::from(rows)));
    }

    /// Тултип трея (главный поток): сессия или просто имя.
    fn tray_tooltip(&self) {
        #[cfg(windows)]
        {
            let text = if self.session_active {
                format!("ByteStrap — {}", tr(self.lang, "Home_Status_Ready"))
            } else {
                "ByteStrap".to_string()
            };
            tray::set_tooltip(&text);
        }
    }

    /// Экспорт конфигурации (настройки + флаги + места) одним JSON.
    #[cfg(not(windows))]
    fn cfg_export(&mut self) {}

    #[cfg(windows)]
    fn cfg_export(&mut self) {
        let lang = self.lang;
        let base = self.config.base();
        if let Some(path) = rfd::FileDialog::new()
            .set_title(tr(lang, "Dlg_CfgExport"))
            .add_filter("JSON", &["json"])
            .set_file_name("bytestrap-config.json")
            .save_file()
        {
            let res = (|| -> anyhow::Result<()> {
                use anyhow::Context;
                let flags_profiles = flags::load_profiles(&base);
                let cfg = serde_json::json!({
                    "app": "ByteStrap",
                    "version": 1,
                    "config": self.config,
                    "flag_profiles": flags_profiles,
                    "places": self.places,
                });
                let text = serde_json::to_string_pretty(&cfg).context("serialize")?;
                std::fs::write(&path, text).context("write")?;
                Ok(())
            })();
            match res {
                Ok(()) => {
                    self.cfg_msg = Some((tr(lang, "Cfg_Exported").to_string(), false));
                    logger::info("Конфигурация экспортирована");
                }
                Err(e) => {
                    self.cfg_msg = Some((fmt(lang, "Cfg_Err", &[&e.to_string()]), true));
                }
            }
        }
    }

    /// Импорт конфигурации: JSON → config.json + flag_profiles.json + places.json.
    #[cfg(not(windows))]
    fn cfg_import(&mut self) {}

    #[cfg(windows)]
    fn cfg_import(&mut self) {
        let lang = self.lang;
        let base = self.config.base();
        if let Some(path) = rfd::FileDialog::new()
            .set_title(tr(lang, "Dlg_CfgImport"))
            .add_filter("JSON", &["json"])
            .add_filter(tr(lang, "Dlg_AllFiles"), &["*"])
            .pick_file()
        {
            let res = (|| -> anyhow::Result<()> {
                use anyhow::Context;
                let text = std::fs::read_to_string(&path).context("read")?;
                let v: serde_json::Value = serde_json::from_str(&text).context("parse")?;
                if v["app"].as_str() != Some("ByteStrap") {
                    anyhow::bail!("not a ByteStrap config");
                }
                // Настройки.
                if let Some(cfg) = v["config"].as_object() {
                    let mut cfg = cfg.clone();
                    if cfg["bytestrap_path"].as_str().is_none_or(|p| p.is_empty()) {
                        cfg.remove("bytestrap_path");
                    }
                    let new_config: Config = serde_json::from_value(serde_json::Value::Object(cfg))
                        .context("config shape")?;
                    self.apply_config(new_config);
                }
                // Профили флагов.
                if let Ok(profiles) =
                    serde_json::from_value::<flags::Profiles>(v["flag_profiles"].clone())
                {
                    let _ = flags::save_profiles(&base, &profiles);
                }
                // Места.
                if let Ok(pl) = serde_json::from_value::<Places>(v["places"].clone()) {
                    pl.save(&base);
                    self.places = pl;
                }
                Ok(())
            })();
            match res {
                Ok(()) => {
                    self.cfg_msg = Some((tr(lang, "Cfg_Imported").to_string(), false));
                    logger::info("Конфигурация импортирована");
                    // Язык мог смениться.
                    let ui = self.weak.clone().unwrap();
                    apply_i18n(&ui, self.lang);
                    self.refresh_counters();
                    self.push_settings(&ui);
                }
                Err(e) => {
                    self.cfg_msg = Some((fmt(lang, "Cfg_Err", &[&e.to_string()]), true));
                }
            }
        }
    }

    /// Применить загруженный конфиг поверх текущего состояния.
    #[cfg(windows)]
    fn apply_config(&mut self, cfg: Config) {
        self.lang = Lang::resolve(&cfg.language);
        self.config = cfg;
        self.config.save();
        self.channel_draft = self.config.channel.clone();
        self.refresh_counters();
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

    /// Пересборка меню трея: оверлей, профили флагов/модов, топ-3 игр.
    fn refresh_tray(&self) {
        let base = self.config.base();
        let profiles = flags::load_profiles(&base);
        let flag_profiles: Vec<String> = profiles.profiles.keys().cloned().collect();
        let mod_profiles = mods::list_profiles(&base);
        let games = self
            .playtime
            .top(3)
            .into_iter()
            .filter_map(|(id, e)| {
                let pid: u64 = id.parse().ok()?;
                let name = if e.name.is_empty() {
                    format!("Place {id}")
                } else {
                    e.name.clone()
                };
                Some((pid, name))
            })
            .collect();
        tray::sync(tray::TrayData {
            overlay_on: self.config.overlay_on,
            flag_profiles,
            active_flag: self.config.active_flag_profile.clone(),
            mod_profiles,
            active_mod: self.config.active_mod_profile.clone(),
            games,
        });
    }

    /// Включить/выключить цветокоррекцию (из трея или UI) и сохранить.
    fn set_overlay_on(&mut self, v: bool) {
        self.config.overlay_on = v;
        self.config.save();
        self.overlay.update(push_overlay(self));
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

    // ---- События ----

    fn drain(&mut self) {
        while let Ok(st) = self.discord_rx.try_recv() {
            self.discord_status = st;
        }
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
                        // Авто-проверка (не ручная) падает тихо: не спамим
                        // ошибкой в UI из-за сети/лимитов GitHub API.
                        Err(e) => {
                            logger::warn(format!("Проверка обновлений: {e}"));
                            UpdateUi::Idle
                        }
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
                        self.refresh_counters();
                        self.mods_dirty = true;
                    }
                    Err(e) => self.mods_msg = Some((e, true)),
                },
                AsyncEvent::ProfileLoaded(res) => {
                    // Дерево меняется в любом случае — пересканировать.
                    self.refresh_mods();
                    self.refresh_counters();
                    self.mods_dirty = true;
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
                    self.refresh_tray();
                }
                AsyncEvent::CleanupDone(freed, files) => {
                    self.cleanup_busy = false;
                    let text = if files == 0 {
                        tr(self.lang, "Settings_CleanupNone").to_string()
                    } else {
                        fmt(
                            self.lang,
                            "Settings_CleanupDoneFmt",
                            &[&versions::format_size(freed), &files.to_string()],
                        )
                    };
                    logger::info(format!("Очистка: {text}"));
                    self.cleanup_msg = Some((text, false));
                }
                #[cfg(windows)]
                AsyncEvent::Tray(action) => match action {
                    tray::Action::Show => {
                        let ui = self.weak.clone().unwrap();
                        let _ = ui.show();
                        winit_restore(&ui);
                    }
                    tray::Action::Play => {
                        self.play(None);
                        let ui = self.weak.clone().unwrap();
                        let _ = ui.show();
                    }
                    tray::Action::Quit => {
                        self.quit = true;
                    }
                    tray::Action::OverlayToggle => {
                        self.set_overlay_on(!self.config.overlay_on);
                        let ui = self.weak.clone().unwrap();
                        ui.set_set_overlay_on(self.config.overlay_on);
                        self.refresh_tray();
                    }
                    tray::Action::FlagProfile(name) => {
                        self.apply_flag_profile(&name);
                        let ui = self.weak.clone().unwrap();
                        self.push_flags(&ui);
                        self.refresh_tray();
                    }
                    tray::Action::ModProfile(name) => {
                        self.start_mod_profile_load(name);
                        self.refresh_tray();
                    }
                    tray::Action::Game(id) => {
                        self.launch_place(id);
                        self.refresh_tray();
                    }
                },
                AsyncEvent::PlaceName((id, name)) => {
                    if let Some(name) = name {
                        if !name.is_empty() {
                            self.places.set_name(id, &name, &self.config.base());
                        }
                    }
                }
                AsyncEvent::Session(ev) => match ev {
                    SessionEvent::Tick {
                        secs,
                        place,
                        place_name,
                    } => {
                        self.session_secs = secs;
                        self.tray_tooltip();
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
                        self.playtime.merge(total, &places);
                        self.playtime.save(&self.config.base());
                        // Возврат к статусу лаунчера / снятие активности.
                        self.sync_discord();
                        // Топ игр по времени мог измениться.
                        self.refresh_tray();
                    }
                },
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
                self.integrity = None;
                // Моды: после чистой установки накладываем заново.
                if !already {
                    self.refresh_mods();
                    if self.mods_list.iter().any(|m| m.enabled) {
                        self.start_mods_apply();
                    }
                    self.refresh_counters();
                    self.mods_dirty = true;
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

    /// `force` — ручная кнопка «Проверить» (игнорирует 4-часовой гейт).
    /// Авто-проверка на старте: не чаще раза в 4 часа, ошибки — тихо.
    fn check_updates(&mut self, force: bool) {
        if matches!(
            self.update,
            UpdateUi::Checking | UpdateUi::Downloading { .. }
        ) {
            return;
        }
        let now = playtime::now_unix();
        if !force && now.saturating_sub(self.config.update_check_last) < 4 * 3600 {
            return;
        }
        self.config.update_check_last = now;
        self.config.save();
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

    // ---- Моды (порт src/app.rs) ----

    fn refresh_mods(&mut self) {
        let base = self.config.base();
        self.mods_list = mods::scan_mods(&base);
        self.mods_files = mods::scan_enabled(&base);
        self.mods_scanned = true;
    }

    fn start_mods_apply(&mut self) {
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

    fn start_mods_revert(&mut self) {
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

    fn start_mods_reapply(&mut self) {
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

    fn start_mod_profile_load(&mut self, name: String) {
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

    /// После вкл/выкл или перестановки мода: пере-применить или пересканировать.
    fn mods_after_change(&mut self) {
        if self.installed.is_some() {
            self.start_mods_reapply();
        } else {
            self.refresh_mods();
        }
    }

    fn mods_toggle(&mut self, id: &str, on: bool) {
        let base = self.config.base();
        let _ = mods::set_enabled(&base, id, on);
        self.mods_after_change();
    }

    fn mods_move(&mut self, id: &str, delta: i32) {
        let base = self.config.base();
        let _ = mods::move_mod(&base, id, delta);
        self.mods_after_change();
    }

    fn mods_open_folder(&self) {
        let p = self.config.base().join("Mods");
        let _ = std::fs::create_dir_all(&p);
        open_folder(&p);
    }

    fn mods_open_mod_folder(&self, id: &str) {
        open_folder(&self.config.base().join("Mods").join(id));
    }

    fn profile_sel_valid(&self) -> bool {
        mods::list_profiles(&self.config.base())
            .iter()
            .any(|n| n == &self.profile_sel)
    }

    fn profile_load_click(&mut self) {
        if !self.profile_sel_valid() {
            return;
        }
        let name = self.profile_sel.clone();
        if self.mods_list.is_empty() {
            self.start_mod_profile_load(name);
        } else {
            self.confirm = Some(Confirm::LoadModProfile(name));
        }
    }

    fn open_save_dlg(&mut self, kind: Dlg) {
        self.dlg_input = match kind {
            Dlg::ModsProfile => {
                if self.profile_sel_valid() {
                    self.profile_sel.clone()
                } else {
                    self.config.active_mod_profile.clone()
                }
            }
            Dlg::FlagsProfile => {
                if self.flags_profile_sel_valid() {
                    self.profile_sel.clone()
                } else {
                    self.config.active_flag_profile.clone()
                }
            }
        };
        self.save_dlg = Some(kind);
    }

    fn save_dlg_ok(&mut self, name: &str) {
        let lang = self.lang;
        let name = name.trim().to_string();
        match self.save_dlg {
            Some(Dlg::ModsProfile) => {
                if name.is_empty() {
                    self.mods_msg = Some((tr(lang, "Prof_NeedName").to_string(), false));
                    return;
                }
                self.save_dlg = None;
                match mods::save_profile(&self.config.base(), &name) {
                    Ok(()) => {
                        self.dlg_input.clear();
                        self.profile_sel = name.clone();
                        self.mods_msg =
                            Some((fmt(lang, "Prof_SavedFmt", &[&name]), false));
                    }
                    Err(e) => self.mods_msg = Some((e.to_string(), true)),
                }
            }
            Some(Dlg::FlagsProfile) => {
                if name.is_empty() {
                    self.flag_msg =
                        Some((tr(lang, "Prof_NeedName").to_string(), false));
                    return;
                }
                if self.flags.is_empty() {
                    self.flag_msg =
                        Some((tr(lang, "Prof_EmptyTable").to_string(), false));
                    return;
                }
                self.save_dlg = None;
                let base = self.config.base();
                let mut profiles = flags::load_profiles(&base);
                profiles
                    .profiles
                    .insert(name.clone(), flags::flags_to_profile(&self.flags));
                match flags::save_profiles(&base, &profiles) {
                    Ok(()) => {
                        self.dlg_input.clear();
                        self.profile_sel = name.clone();
                        self.flag_msg =
                            Some((fmt(lang, "Prof_SavedFmt", &[&name]), false));
                    }
                    Err(e) => {
                        self.flag_msg = Some((
                            fmt(lang, "Flags_SaveErr", &[&e.to_string()]),
                            true,
                        ));
                    }
                }
                self.refresh_tray();
            }
            None => {}
        }
    }

    fn save_dlg_cancel(&mut self) {
        self.save_dlg = None;
        self.dlg_input.clear();
    }

    #[cfg(windows)]
    fn profile_export(&mut self) {
        let lang = self.lang;
        let base = self.config.base();
        let name = self.profile_sel.clone();
        if let Some(path) = rfd::FileDialog::new()
            .set_title(tr(lang, "Dlg_ProfExport"))
            .add_filter("ZIP", &["zip"])
            .set_file_name(format!("{name}.zip"))
            .save_file()
        {
            match crate::core::share::export_mod_profile(&base, &name, &path) {
                Ok(()) => {
                    self.mods_msg = Some((fmt(lang, "Prof_ExportedFmt", &[&name]), false));
                }
                Err(e) => {
                    self.mods_msg = Some((fmt(lang, "Prof_ShareErr", &[&e.to_string()]), true));
                }
            }
        }
    }

    #[cfg(not(windows))]
    fn profile_export(&mut self) {}

    #[cfg(windows)]
    fn profile_import(&mut self) {
        let lang = self.lang;
        let base = self.config.base();
        if let Some(path) = rfd::FileDialog::new()
            .set_title(tr(lang, "Dlg_ProfImport"))
            .add_filter("ZIP", &["zip"])
            .add_filter(tr(lang, "Dlg_AllFiles"), &["*"])
            .pick_file()
        {
            match crate::core::share::import_mod_profile(&base, &path) {
                Ok(name) => {
                    self.profile_sel = name.clone();
                    self.mods_msg = Some((fmt(lang, "Prof_ImportedFmt", &[&name]), false));
                }
                Err(e) => {
                    self.mods_msg = Some((fmt(lang, "Prof_ShareErr", &[&e.to_string()]), true));
                }
            }
        }
    }

    #[cfg(not(windows))]
    fn profile_import(&mut self) {}

    // ---- FastFlags (порт src/ui/flags.rs) ----

    fn flags_dir(&self, guid: &str) -> PathBuf {
        self.config.base().join("Versions").join(guid)
    }

    /// (Пере)загрузка таблицы при смене активной версии.
    fn flags_ensure_loaded(&mut self) {
        let Some(target) = self.installed.clone() else {
            return;
        };
        if self.flags_loaded_for.as_ref() != Some(&target) {
            let dir = self.flags_dir(&target);
            self.flags = flags::load(&dir).into_iter().collect();
            self.flags_loaded_for = Some(target);
            self.flags_dirty = false;
            self.flags_selected = None;
            self.flag_msg = None;
            self.import = None;
            self.refresh_counters();
        }
    }

    fn flags_profile_sel_valid(&self) -> bool {
        flags::load_profiles(&self.config.base())
            .profiles
            .keys()
            .any(|n| n == &self.profile_sel)
    }

    fn flags_save(&mut self) {
        if !self.flags_dirty {
            return;
        }
        let Some(guid) = self.installed.clone() else {
            return;
        };
        let lang = self.lang;
        let dir = self.flags_dir(&guid);
        let map: BTreeMap<String, String> = self.flags.iter().cloned().collect();
        match flags::save(&dir, &map) {
            Ok(()) => {
                self.flags_dirty = false;
                if !self.config.active_flag_profile.is_empty() {
                    self.config.active_flag_profile.clear();
                    self.config.save();
                }
                self.flag_msg = Some((fmt(lang, "Flags_Saved", &[]), false));
                self.refresh_counters();
            }
            Err(e) => {
                self.flag_msg =
                    Some((fmt(lang, "Flags_SaveErr", &[&e.to_string()]), true));
            }
        }
    }

    fn flags_add(&mut self) {
        self.flags.push(("NewFlag".to_string(), "true".to_string()));
        self.flags_selected = Some(self.flags.len() - 1);
        self.flags_dirty = true;
        self.flag_msg = None;
        self.refresh_counters();
    }

    fn flags_delete_sel(&mut self) {
        if let Some(i) = self.flags_selected.take() {
            if i < self.flags.len() {
                self.flags.remove(i);
                self.flags_dirty = true;
                self.refresh_counters();
            }
        }
    }

    fn flags_clear_request(&mut self) {
        if !self.flags.is_empty() {
            self.confirm = Some(Confirm::ClearAll);
        }
    }

    fn flags_search_edited(&mut self, q: &str) {
        self.flags_search = q.to_string();
    }

    fn flags_row_select(&mut self, idx: usize) {
        if idx < self.flags.len() {
            self.flags_selected = Some(idx);
        }
    }

    fn flags_cell_edited(&mut self, idx: usize, col: i32, text: &str) {
        if idx >= self.flags.len() {
            return;
        }
        self.flags_selected = Some(idx);
        if col == 0 {
            if self.flags[idx].0 != text {
                self.flags[idx].0 = text.to_string();
                self.flags_dirty = true;
            }
            return;
        }
        let lang = self.lang;
        let name = self.flags[idx].0.trim().to_string();
        match flags::validate(&name, text.trim()) {
            Ok(()) => {
                self.flags[idx].1 = text.to_string();
                self.flags_dirty = true;
                if matches!(self.flag_msg, Some((_, true))) {
                    self.flag_msg = None;
                }
            }
            Err(e) => {
                let detail = text.trim().to_string();
                self.flag_msg = Some((error_text(lang, &name, &detail, e), true));
            }
        }
    }

    fn preset_pick(&mut self, pi: i32, oi: i32) {
        if pi < 0 || oi < 0 {
            return;
        }
        let Some(p) = presets::PRESETS.get(pi as usize) else {
            return;
        };
        if p.kind != presets::Kind::Combo || oi as usize > p.map.len() {
            return;
        }
        presets::apply(&mut self.flags, p, oi as usize);
        self.flags_dirty = true;
        self.flag_msg = None;
    }

    fn preset_toggle(&mut self, pi: i32, on: bool) {
        if pi < 0 {
            return;
        }
        let Some(p) = presets::PRESETS.get(pi as usize) else {
            return;
        };
        if p.kind != presets::Kind::Toggle {
            return;
        }
        presets::apply(&mut self.flags, p, usize::from(on));
        self.flags_dirty = true;
        self.flag_msg = None;
    }

    fn flags_profile_load(&mut self) {
        if !self.flags_profile_sel_valid() {
            return;
        }
        let name = self.profile_sel.clone();
        if self.flags.is_empty() {
            self.apply_flag_profile(&name);
        } else {
            self.confirm = Some(Confirm::ApplyProfile(name));
        }
    }

    fn apply_flag_profile(&mut self, name: &str) {
        let lang = self.lang;
        let base = self.config.base();
        let profiles = flags::load_profiles(&base);
        let Some(profile) = profiles.profiles.get(name) else {
            self.flag_msg = Some((fmt(lang, "Prof_NoSuchFmt", &[name]), false));
            return;
        };
        self.flags = flags::profile_to_flags(profile);
        self.flags_selected = None;
        let Some(guid) = self.installed.clone() else {
            self.flags_dirty = true;
            return;
        };
        let dir = self.flags_dir(&guid);
        let map: BTreeMap<String, String> = self.flags.iter().cloned().collect();
        match flags::save(&dir, &map) {
            Ok(()) => {
                self.flags_dirty = false;
                if !self.config.active_flag_profile.eq_ignore_ascii_case(name) {
                    self.config.active_flag_profile = name.to_string();
                    self.config.save();
                }
                self.flag_msg = Some((
                    fmt(
                        lang,
                        "Prof_AppliedFmt",
                        &[name, &self.flags.len().to_string()],
                    ),
                    false,
                ));
            }
            Err(e) => {
                self.flag_msg =
                    Some((fmt(lang, "Flags_SaveErr", &[&e.to_string()]), true));
            }
        }
        self.refresh_tray();
    }

    fn flags_profile_delete_request(&mut self) {
        if self.flags_profile_sel_valid() {
            let name = self.profile_sel.clone();
            self.confirm = Some(Confirm::DeleteProfile(name));
        }
    }

    #[cfg(windows)]
    fn flags_profile_export(&mut self) {
        let lang = self.lang;
        let base = self.config.base();
        let name = self.profile_sel.clone();
        if let Some(path) = rfd::FileDialog::new()
            .set_title(tr(lang, "Dlg_ProfExport"))
            .add_filter("JSON", &["json"])
            .set_file_name(format!("{name}.json"))
            .save_file()
        {
            match crate::core::share::export_flag_profile(&base, &name, &path) {
                Ok(()) => {
                    self.flag_msg =
                        Some((fmt(lang, "Prof_ExportedFmt", &[&name]), false));
                }
                Err(e) => {
                    self.flag_msg = Some((
                        fmt(lang, "Prof_ShareErr", &[&e.to_string()]),
                        true,
                    ));
                }
            }
        }
    }

    #[cfg(not(windows))]
    fn flags_profile_export(&mut self) {}

    #[cfg(windows)]
    fn flags_profile_import(&mut self) {
        let lang = self.lang;
        let base = self.config.base();
        if let Some(path) = rfd::FileDialog::new()
            .set_title(tr(lang, "Dlg_ProfImport"))
            .add_filter("JSON", &["json"])
            .add_filter(tr(lang, "Dlg_AllFiles"), &["*"])
            .pick_file()
        {
            match crate::core::share::import_flag_profile(&base, &path) {
                Ok(name) => {
                    self.profile_sel = name.clone();
                    self.flag_msg =
                        Some((fmt(lang, "Prof_ImportedFmt", &[&name]), false));
                }
                Err(e) => {
                    self.flag_msg = Some((
                        fmt(lang, "Prof_ShareErr", &[&e.to_string()]),
                        true,
                    ));
                }
            }
        }
    }

    #[cfg(not(windows))]
    fn flags_profile_import(&mut self) {}

    #[cfg(windows)]
    fn flags_import_file(&mut self) {
        let lang = self.lang;
        let picked = rfd::FileDialog::new()
            .set_title(tr(lang, "Dlg_FlagsImport"))
            .add_filter("JSON", &["json"])
            .add_filter(tr(lang, "Dlg_AllFiles"), &["*"])
            .pick_file();
        if let Some(path) = picked {
            if let Ok(text) = std::fs::read_to_string(path) {
                self.apply_import(&text);
            }
        }
    }

    #[cfg(not(windows))]
    fn flags_import_file(&mut self) {}

    #[cfg(windows)]
    fn flags_paste(&mut self) {
        match clipboard_json() {
            Some(t) => self.apply_import(&t),
            None => {
                let lang = self.lang;
                self.flag_msg = Some((tr(lang, "Flags_NoClipboard").to_string(), false));
            }
        }
    }

    #[cfg(not(windows))]
    fn flags_paste(&mut self) {}

    #[cfg(windows)]
    fn flags_export(&mut self) {
        let lang = self.lang;
        let json = flags::export_json(&self.flags);
        let count = self.flags.len().to_string();
        let picked = rfd::FileDialog::new()
            .set_title("Export")
            .add_filter("JSON", &["json"])
            .set_file_name("fastflags.json")
            .save_file();
        if let Some(path) = picked {
            match std::fs::write(path, json) {
                Ok(()) => {
                    self.flag_msg =
                        Some((fmt(lang, "Flags_ExportedFmt", &[&count]), false));
                }
                Err(e) => {
                    self.flag_msg = Some((
                        fmt(lang, "Flags_ExportErr", &[&e.to_string()]),
                        true,
                    ));
                }
            }
        }
    }

    #[cfg(not(windows))]
    fn flags_export(&mut self) {}

    /// Наложение импорта: новые — сразу, совпадающие — пропуск,
    /// конфликты — в очередь окна спроса.
    #[cfg_attr(not(windows), allow(dead_code))]
    fn apply_import(&mut self, text: &str) {
        let lang = self.lang;
        let incoming = match flags::parse_import(text) {
            Ok(v) => v,
            Err(e) => {
                self.flag_msg =
                    Some((fmt(lang, "Flags_ImportErr", &[&e.to_string()]), true));
                return;
            }
        };
        if incoming.is_empty() {
            self.flag_msg = Some((tr(lang, "Flags_EmptyImport").to_string(), false));
            return;
        }

        let mut conflicts = VecDeque::new();
        let mut applied = 0usize;
        for (k, v) in incoming {
            match self.flags.iter().position(|(n, _)| *n == k) {
                None => {
                    self.flags.push((k, v));
                    applied += 1;
                }
                Some(i) => {
                    if self.flags[i].1 != v {
                        conflicts.push_back((k, v));
                    }
                }
            }
        }

        if conflicts.is_empty() {
            self.flags_dirty = true;
            self.flag_msg = Some((
                fmt(lang, "Flags_Imported", &[&applied.to_string(), "0"]),
                false,
            ));
        } else {
            self.import = Some(ImportQueue {
                conflicts,
                replace_all: false,
                keep_rest: false,
                applied,
                skipped: 0,
            });
        }
    }

    /// Решение в окне конфликта: 0 — оставить, 1 — заменить,
    /// 2 — заменить все, 3 — отмена.
    fn import_decide(&mut self, d: i32) {
        match d {
            0 => {
                if let Some(q) = &mut self.import {
                    q.keep_rest = true;
                }
            }
            1 => {
                if let Some(q) = &mut self.import {
                    if let Some((n, v)) = q.conflicts.pop_front() {
                        set_flag_pair(&mut self.flags, &n, &v);
                        q.applied += 1;
                    }
                }
            }
            2 => {
                if let Some(q) = &mut self.import {
                    q.replace_all = true;
                    if let Some((n, v)) = q.conflicts.pop_front() {
                        set_flag_pair(&mut self.flags, &n, &v);
                        q.applied += 1;
                    }
                }
            }
            _ => self.import = None,
        }
        self.drain_import();
    }

    /// Дожим очереди конфликтов (после решения или при пустой очереди).
    fn drain_import(&mut self) {
        let lang = self.lang;
        let mut finished: Option<(usize, usize)> = None;
        if let Some(q) = &mut self.import {
            loop {
                if q.keep_rest {
                    q.skipped += q.conflicts.len();
                    q.conflicts.clear();
                } else if q.replace_all {
                    match q.conflicts.pop_front() {
                        Some((n, v)) => {
                            set_flag_pair(&mut self.flags, &n, &v);
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
            self.import = None;
            self.flags_dirty = true;
            self.flag_msg = Some((
                fmt(lang, "Flags_Imported", &[&a.to_string(), &s.to_string()]),
                false,
            ));
        }
    }

    // ---- Настройки (порт src/app.rs) ----

    fn set_lang(&mut self, lang: Lang) {
        self.lang = lang;
        self.config.language = lang.code().to_string();
        self.config.save();
        logger::info(format!("Язык переключён: {}", lang.code()));
    }

    fn sync_discord(&self) {
        if !self.config.discord_rp {
            let _ = self.discord.send(discord::Update::Idle);
        } else if self.session_active && self.session_display.is_some() {
            self.push_discord(self.session_secs);
        } else {
            // Лаунчер открыт, сессии нет — статус «в меню ByteStrap».
            let _ = self.discord.send(discord::Update::Launcher);
        }
    }

    fn start_cleanup(&mut self) {
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

    /// Выбор канала: превью версии в фоне (как в egui).
    fn channel_pick(&mut self, ch: &str) {
        if self.channel_draft.eq_ignore_ascii_case(ch) {
            return;
        }
        self.channel_draft = ch.to_string();
        self.channel_preview = None;
        self.channel_checking = true;
        let ch = ch.to_string();
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .expect("tokio runtime");
            let res = rt
                .block_on(deploy::latest_version(&ch))
                .map_err(|e| e.to_string());
            let _ = tx.send(AsyncEvent::ChannelPreview(res));
        });
    }

    /// Запрос подтверждения смены канала (открывает модалку).
    fn channel_apply_request(&mut self) {
        self.confirm = Some(Confirm::ApplyChannel);
    }

    fn confirm_yes(&mut self) {
        let lang = self.lang;
        match self.confirm.take() {
            Some(Confirm::ApplyChannel) => {
                self.config.channel = self.channel_draft.clone();
                self.config.save();
                self.latest = None;
                let tx = self.tx.clone();
                let channel = self.config.channel.clone();
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
                self.start_install(true);
            }
            Some(Confirm::RevertMods) => self.start_mods_revert(),
            Some(Confirm::LoadModProfile(name)) => self.start_mod_profile_load(name),
            Some(Confirm::DeleteModProfile(name)) => {
                let base = self.config.base();
                match mods::delete_profile(&base, &name) {
                    Ok(()) => {
                        if self.dlg_input.trim().eq_ignore_ascii_case(&name) {
                            self.dlg_input.clear();
                        }
                        if self.profile_sel.eq_ignore_ascii_case(&name) {
                            self.profile_sel.clear();
                        }
                        if self.config.active_mod_profile.eq_ignore_ascii_case(&name) {
                            self.config.active_mod_profile.clear();
                            self.config.save();
                        }
                        self.mods_msg = Some((fmt(lang, "Prof_DeletedFmt", &[&name]), false));
                    }
                    Err(e) => self.mods_msg = Some((e.to_string(), true)),
                }
                self.refresh_tray();
            }
            Some(Confirm::ClearAll) => {
                self.flags.clear();
                self.flags_selected = None;
                self.flags_dirty = true;
                self.flag_msg = Some((fmt(lang, "Flags_Cleared", &[]), false));
                self.refresh_counters();
            }
            Some(Confirm::ApplyProfile(name)) => self.apply_flag_profile(&name),
            Some(Confirm::DeleteProfile(name)) => {
                let base = self.config.base();
                let mut profiles = flags::load_profiles(&base);
                profiles.profiles.remove(&name);
                match flags::save_profiles(&base, &profiles) {
                    Ok(()) => {
                        if self.dlg_input.trim().eq_ignore_ascii_case(&name) {
                            self.dlg_input.clear();
                        }
                        if self.profile_sel.eq_ignore_ascii_case(&name) {
                            self.profile_sel.clear();
                        }
                        if self
                            .config
                            .active_flag_profile
                            .eq_ignore_ascii_case(&name)
                        {
                            self.config.active_flag_profile.clear();
                            self.config.save();
                        }
                        self.flag_msg =
                            Some((fmt(lang, "Prof_DeletedFmt", &[&name]), false));
                    }
                    Err(e) => {
                        self.flag_msg = Some((
                            fmt(lang, "Flags_SaveErr", &[&e.to_string()]),
                            true,
                        ));
                    }
                }
                self.refresh_tray();
            }
            None => {}
        }
    }

    fn confirm_no(&mut self) {
        self.confirm = None;
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

        // Живая сессия: «в игре {0} · место {1}» (пусто — не играем).
        let session_text = if self.session_active {
            fmt(
                lang,
                "Home_Session",
                &[
                    &playtime::fmt_duration(self.session_secs),
                    self.session_place.as_deref().unwrap_or("…"),
                ],
            )
        } else {
            String::new()
        };
        ui.set_session_text(session_text.as_str().into());

        // 14 дней активности: минуты (старые → новые) + подписи DD.MM.
        let mut days: Vec<i32> = Vec::with_capacity(14);
        let mut labels: Vec<SharedString> = Vec::with_capacity(14);
        let now = playtime::now_unix();
        for i in (0..14).rev() {
            let key = playtime::day_key(now - (i as u64) * 86_400);
            days.push((*self.playtime.days.get(&key).unwrap_or(&0) / 60) as i32);
            if i == 0 || i == 13 {
                labels.push(format!("{}.{}", &key[8..10], &key[5..7]).into());
            } else {
                labels.push("".into());
            }
        }
        ui.set_play_days(ModelRc::new(VecModel::from(days)));
        ui.set_play_labels(ModelRc::new(VecModel::from(labels)));

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

    /// Всё, кроме пути (путь пушится только при входе на страницу,
    /// чтобы не сбивать курсор в поле ввода).
    fn push_settings(&self, ui: &AppWindow) {
        let lang = self.lang;

        let draft = &self.channel_draft;
        let sel = if draft.eq_ignore_ascii_case("LIVE") {
            0
        } else if draft.eq_ignore_ascii_case("BETA") {
            1
        } else if draft.eq_ignore_ascii_case("ZNext") {
            2
        } else {
            -1
        };
        ui.set_set_ch_sel(sel);
        ui.set_set_ch_checking(self.channel_checking);
        let (pk, pt) = match &self.channel_preview {
            None if self.channel_checking => {
                (1, tr(lang, "Settings_ChannelCheck").to_string())
            }
            Some(Ok(v)) => (2, fmt(lang, "Settings_ChannelVer", &[&v.version])),
            Some(Err(e)) => (3, fmt(lang, "Settings_ChannelErr", &[e])),
            _ => (0, String::new()),
        };
        ui.set_set_ch_preview_kind(pk);
        ui.set_set_ch_preview_text(pt.as_str().into());
        ui.set_set_ch_changed(!draft.eq_ignore_ascii_case(&self.config.channel));

        ui.set_set_lang_code(if lang.code() == "en" { 1 } else { 0 });

        ui.set_set_threads(self.config.download_threads as i32);
        ui.set_set_auto_update(self.config.auto_update);
        ui.set_set_close_launch(self.config.close_on_launch);
        ui.set_set_close_tray(self.config.close_to_tray);
        ui.set_set_auto_clean(self.config.auto_clean);
        ui.set_set_show_logs(self.config.show_logs);
        ui.set_set_discord_rp(self.config.discord_rp);
        ui.set_set_rp_place(self.config.discord_rp_place);
        ui.set_set_rp_time(self.config.discord_rp_time);
        ui.set_set_rp_brand(self.config.discord_rp_brand);

        let (dsk, dst) = match &self.discord_status {
            discord::Status::Connected => {
                (1, tr(lang, "Settings_DiscordRP_Connected").to_string())
            }
            discord::Status::Disconnected(reason) => (
                2,
                format!("{}: {}", tr(lang, "Settings_DiscordRP_NoDiscord"), reason),
            ),
            discord::Status::Idle => (0, tr(lang, "Settings_DiscordRP_Wait").to_string()),
        };
        ui.set_set_discord_status(dsk);
        ui.set_set_discord_text(dst.as_str().into());

        ui.set_set_deeplink(self.config.deeplink);
        ui.set_set_deeplink_ok(match self.deeplink_ok {
            None => 0,
            Some(true) => 1,
            Some(false) => 2,
        });

        ui.set_set_sound(self.config.sound);
        ui.set_set_sound_vol(self.config.sound_volume.min(100) as i32);

        ui.set_set_overlay_on(self.config.overlay_on);
        ui.set_set_overlay_bright(self.config.overlay_bright);
        ui.set_set_overlay_sat(self.config.overlay_sat);
        ui.set_set_overlay_contrast(self.config.overlay_contrast);

        ui.set_set_cleanup_busy(self.cleanup_busy);
        match &self.cleanup_msg {
            Some((text, err)) => {
                ui.set_set_cleanup_text(text.as_str().into());
                ui.set_set_cleanup_err(*err);
            }
            None => {
                ui.set_set_cleanup_text("".into());
                ui.set_set_cleanup_err(false);
            }
        }

        match &self.cfg_msg {
            Some((text, err)) => {
                ui.set_set_cfg_msg(text.as_str().into());
                ui.set_set_cfg_msg_err(*err);
            }
            None => {
                ui.set_set_cfg_msg("".into());
                ui.set_set_cfg_msg_err(false);
            }
        }
    }

    fn push_confirm(&self, ui: &AppWindow) {
        let lang = self.lang;
        let body = match &self.confirm {
            Some(Confirm::ApplyChannel) => {
                Some(fmt(lang, "Settings_ChannelConfirm", &[&self.channel_draft]))
            }
            Some(Confirm::RevertMods) => Some(tr(lang, "Mods_RevertConfirm").to_string()),
            Some(Confirm::LoadModProfile(n)) => Some(fmt(lang, "Mods_LoadConfirm", &[n.as_str()])),
            Some(Confirm::DeleteModProfile(n)) => Some(fmt(lang, "Prof_DelConfirm", &[n.as_str()])),
            Some(Confirm::ClearAll) => Some(tr(lang, "Flags_ClearConfirm").to_string()),
            Some(Confirm::ApplyProfile(n)) => {
                Some(fmt(lang, "Prof_ApplyConfirm", &[n.as_str()]))
            }
            Some(Confirm::DeleteProfile(n)) => {
                Some(fmt(lang, "Prof_DelConfirm", &[n.as_str()]))
            }
            None => None,
        };
        ui.set_confirm_body(body.unwrap_or_default().as_str().into());
    }

    fn push_path(&self, ui: &AppWindow) {
        ui.set_set_path(self.config.bytestrap_path.as_str().into());
        let hint = Config::base_dir().to_string_lossy().to_string();
        ui.set_set_path_hint(hint.as_str().into());
        // Токен: поле ввода перечитывает значение (сброс/вход на страницу).
        ui.set_set_path_token(ui.get_set_path_token() + 1);
    }

    fn push_mods(&self, ui: &AppWindow) {
        let lang = self.lang;
        let base = self.config.base();

        let mods_dir = base.join("Mods").to_string_lossy().to_string();
        ui.set_mods_path(mods_dir.as_str().into());

        let rows: Vec<ModRow> = self
            .mods_list
            .iter()
            .enumerate()
            .map(|(i, m)| ModRow {
                id: m.id.as_str().into(),
                meta: format!(
                    "{} · {}",
                    fmt(lang, "Mods_CountFmt", &[&m.files.to_string()]),
                    versions::format_size(m.size)
                )
                .into(),
                enabled: m.enabled,
                can_up: i > 0,
                can_down: i + 1 < self.mods_list.len(),
            })
            .collect();
        ui.set_mods_list(ModelRc::new(VecModel::from(rows)));

        match &self.mods_msg {
            Some((text, err)) => {
                ui.set_mods_msg(text.as_str().into());
                ui.set_mods_msg_err(*err);
            }
            None => {
                ui.set_mods_msg("".into());
                ui.set_mods_msg_err(false);
            }
        }

        let any_enabled = self.mods_list.iter().any(|m| m.enabled);
        ui.set_mods_apply_enabled(self.installed.is_some() && any_enabled);
        ui.set_mods_revert_enabled(
            self.installed.is_some() && base.join("Mods/.backup").exists(),
        );

        let guid = self.installed.clone().unwrap_or_default();
        let files: Vec<ModFileRow> = self
            .mods_files
            .iter()
            .map(|f| ModFileRow {
                rel: f.rel.as_str().into(),
                size: format!("{} B", f.size).into(),
                status_kind: if guid.is_empty() {
                    1
                } else {
                    match mods::file_status(&base, &guid, &f.rel) {
                        mods::FileStatus::Applied => 0,
                        mods::FileStatus::Missing => 1,
                        mods::FileStatus::Clean => 2,
                    }
                },
            })
            .collect();
        ui.set_mods_files(ModelRc::new(VecModel::from(files)));
        let count = fmt(lang, "Mods_CountFmt", &[&self.mods_files.len().to_string()]);
        ui.set_mods_files_count(count.as_str().into());

        let profiles = mods::list_profiles(&base);
        let sel_index = profiles
            .iter()
            .position(|n| n == &self.profile_sel)
            .map(|i| i as i32)
            .unwrap_or(-1);
        let sel_valid = sel_index >= 0;
        let sel_text = if sel_valid {
            self.profile_sel.clone()
        } else if !self.config.active_mod_profile.is_empty()
            && profiles
                .iter()
                .any(|n| n.eq_ignore_ascii_case(&self.config.active_mod_profile))
        {
            self.config.active_mod_profile.clone()
        } else {
            tr(lang, "Prof_Select").to_string()
        };
        let items: Vec<SharedString> = profiles.iter().cloned().map(Into::into).collect();
        ui.set_mods_profiles(ModelRc::new(VecModel::from(items)));
        ui.set_mods_profile_sel_text(sel_text.as_str().into());
        ui.set_mods_profile_sel_index(sel_index);
        ui.set_mods_profile_sel_valid(sel_valid);
        ui.set_mods_profiles_empty(profiles.is_empty());
    }

    fn push_flags(&mut self, ui: &AppWindow) {
        self.flags_ensure_loaded();
        ui.set_flags_search(self.flags_search.as_str().into());
        self.push_flags_rows(ui);
        self.push_flags_meta(ui);
    }

    /// Сообщение, красная точка, деталь выделения, окно конфликта импорта.
    fn push_flags_meta(&self, ui: &AppWindow) {
        let lang = self.lang;
        ui.set_flags_has_client(self.installed.is_some());
        ui.set_flags_dirty(self.flags_dirty);
        ui.set_flags_can_delete(self.flags_selected.is_some());

        match &self.flag_msg {
            Some((text, err)) => {
                ui.set_flags_msg(text.as_str().into());
                ui.set_flags_msg_err(*err);
            }
            None => {
                ui.set_flags_msg("".into());
                ui.set_flags_msg_err(false);
            }
        }

        let detail = self
            .flags_selected
            .and_then(|i| self.flags.get(i))
            .and_then(|(n, _)| {
                let name = n.trim();
                flags::lookup(name).map(|info| {
                    let mut text = format!("{} · {} · {}", info.category, flags::hint(info), info.desc);
                    if !flags::is_allowlisted(name) {
                        text.push_str(&format!(
                            " · ⚠ {}",
                            tr(self.lang, "Flags_NotAllowlisted")
                        ));
                    }
                    text
                })
            })
            .unwrap_or_default();
        ui.set_flags_sel_detail(detail.as_str().into());

        let (open, body) = match self.import.as_ref().and_then(|q| q.conflicts.front())
        {
            Some((name, incoming)) => {
                let old = self
                    .flags
                    .iter()
                    .find(|(n, _)| n == name)
                    .map(|(_, v)| v.clone())
                    .unwrap_or_default();
                (true, fmt(lang, "Flags_ConflictMsg", &[name, &old, incoming]))
            }
            None => (false, String::new()),
        };
        ui.set_import_open(open);
        ui.set_import_body(body.as_str().into());
    }

    /// Таблица, пресеты, профили (пересборка моделей — фокус внутри
    /// редактируемых ячеек при этом теряется, поэтому зовётся только
    /// на структурных изменениях).
    fn push_flags_rows(&self, ui: &AppWindow) {
        let lang = self.lang;

        let query = self.flags_search.trim().to_lowercase();
        let rows: Vec<FlagRow> = self
            .flags
            .iter()
            .enumerate()
            .filter(|(_, (n, _))| query.is_empty() || n.to_lowercase().contains(&query))
            .map(|(i, (n, v))| FlagRow {
                idx: i as i32,
                name: n.as_str().into(),
                value: v.as_str().into(),
                desc: flags::lookup(n.trim())
                    .map(|f| format!("{} · {}", f.category, f.desc))
                    .unwrap_or_default()
                    .into(),
                selected: self.flags_selected == Some(i),
                alt: i % 2 == 1,
                works: flags::is_allowlisted(n),
            })
            .collect();
        ui.set_flags_rows(ModelRc::new(VecModel::from(rows)));
        ui.set_flags_table_empty(self.flags.is_empty());

        let presets: Vec<PresetRow> = presets::PRESETS
            .iter()
            .map(|p| {
                let cur = presets::current_index(&self.flags, p);
                let current = p.options.get(cur).copied().unwrap_or("Auto");
                PresetRow {
                    label: tr(lang, p.label_key).into(),
                    desc: tr(lang, p.desc_key).into(),
                    kind: if p.kind == presets::Kind::Toggle { 1 } else { 0 },
                    options: ModelRc::new(VecModel::from(
                        p.options
                            .iter()
                            .map(|o| local_option(lang, o).into())
                            .collect::<Vec<SharedString>>(),
                    )),
                    selected: cur as i32,
                    current: local_option(lang, current).into(),
                    on: cur == 1,
                    works: p.works,
                }
            })
            .collect();
        ui.set_flags_presets(ModelRc::new(VecModel::from(presets)));

        let base = self.config.base();
        let names: Vec<String> = flags::load_profiles(&base)
            .profiles
            .keys()
            .cloned()
            .collect();
        let sel_index = names
            .iter()
            .position(|n| n == &self.profile_sel)
            .map(|i| i as i32)
            .unwrap_or(-1);
        let sel_valid = sel_index >= 0;
        let sel_text = if sel_valid {
            self.profile_sel.clone()
        } else if !self.config.active_flag_profile.is_empty()
            && names
                .iter()
                .any(|n| n.eq_ignore_ascii_case(&self.config.active_flag_profile))
        {
            self.config.active_flag_profile.clone()
        } else {
            tr(lang, "Prof_Select").to_string()
        };
        let items: Vec<SharedString> = names.iter().cloned().map(Into::into).collect();
        ui.set_flags_profiles(ModelRc::new(VecModel::from(items)));
        ui.set_flags_profile_sel_text(sel_text.as_str().into());
        ui.set_flags_profile_sel_index(sel_index);
        ui.set_flags_profile_sel_valid(sel_valid);
        ui.set_flags_profiles_empty(names.is_empty());
    }

    fn push_dlg(&self, ui: &AppWindow) {
        let lang = self.lang;
        match &self.save_dlg {
            None => ui.set_dlg_open(false),
            Some(Dlg::ModsProfile) => {
                let base = self.config.base();
                let n = self.dlg_input.trim();
                let exists =
                    !n.is_empty() && mods::list_profiles(&base).iter().any(|p| p == n);
                ui.set_dlg_open(true);
                ui.set_dlg_desc(tr(lang, "Prof_SaveAsDesc").into());
                ui.set_dlg_hint(tr(lang, "Flags_ProfileName").into());
                ui.set_dlg_ok_label(
                    (if exists {
                        tr(lang, "Prof_Overwrite")
                    } else {
                        tr(lang, "Prof_Save")
                    })
                    .into(),
                );
            }
            Some(Dlg::FlagsProfile) => {
                let base = self.config.base();
                let n = self.dlg_input.trim();
                let exists = !n.is_empty()
                    && flags::load_profiles(&base).profiles.contains_key(n);
                ui.set_dlg_open(true);
                ui.set_dlg_desc(tr(lang, "Prof_SaveAsDesc").into());
                ui.set_dlg_hint(tr(lang, "Flags_ProfileName").into());
                ui.set_dlg_ok_label(
                    (if exists {
                        tr(lang, "Prof_Overwrite")
                    } else {
                        tr(lang, "Prof_Save")
                    })
                    .into(),
                );
            }
        }
    }

    fn push_dlg_input(&self, ui: &AppWindow) {
        ui.set_dlg_input(self.dlg_input.as_str().into());
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

/// Стартовый размер не больше ~90% рабочей области экрана: на маленьких
/// мониторах (1280×1024, 1366×768) окно 1120×700 иначе вылезает за края.
#[cfg(windows)]
fn clamp_window_to_screen(ui: &AppWindow) {
    use windows_sys::Win32::Foundation::RECT;
    use windows_sys::Win32::UI::WindowsAndMessaging::{SystemParametersInfoW, SPI_GETWORKAREA};
    let mut rc: RECT = unsafe { std::mem::zeroed() };
    let ok = unsafe { SystemParametersInfoW(SPI_GETWORKAREA, 0, &mut rc as *mut _ as *mut _, 0) };
    if ok == 0 {
        return;
    }
    let scale = ui.window().scale_factor();
    let w = (rc.right - rc.left) as f32 / scale;
    let h = (rc.bottom - rc.top) as f32 / scale;
    // Минимум окна (820×520) не превышаем, даже если экран крошечный.
    let max_w = (w * 0.9).max(820.0);
    let max_h = (h * 0.9).max(520.0);
    if max_w < 1120.0 || max_h < 700.0 {
        ui.window().set_size(slint::PhysicalSize::new(
            (max_w * scale).round() as u32,
            (max_h * scale).round() as u32,
        ));
    }
}

#[cfg(not(windows))]
fn clamp_window_to_screen(_ui: &AppWindow) {}

#[cfg(windows)]
fn winit_min(ui: &AppWindow) {
    let _ = ui.window().with_winit_window(|x| x.set_minimized(true));
}

#[cfg(not(windows))]
fn winit_min(_ui: &AppWindow) {}

/// Вернуть окно из трея: показать, развернуть из свернутого вида,
/// вывести на передний план и отдать фокус (клик по иконке трея).

#[cfg(windows)]
fn winit_restore(ui: &AppWindow) {    let _ = ui.window().with_winit_window(|x| {
        if !x.is_visible() {
            x.set_visible(true);
        }
        x.set_minimized(false);
        x.focus_window();
    });
}

#[cfg(not(windows))]
#[allow(dead_code)]
fn winit_restore(_ui: &AppWindow) {}

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
    t.set_nav_games(tr(lang, "Nav_Games").into());
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
    t.set_home_chart(tr(lang, "Home_Chart").into());
    t.set_home_places(tr(lang, "Home_Places").into());
    t.set_games_h1(tr(lang, "Games_H1").into());
    t.set_games_sub(tr(lang, "Games_Sub").into());
    t.set_games_hint(tr(lang, "Games_Hint").into());
    t.set_games_add(tr(lang, "Games_Add").into());
    t.set_games_col_name(tr(lang, "Games_ColName").into());
    t.set_games_col_visits(tr(lang, "Games_ColVisits").into());
    t.set_games_col_last(tr(lang, "Games_ColLast").into());
    t.set_games_empty(tr(lang, "Games_Empty").into());
    t.set_games_fav(tr(lang, "Games_Fav").into());
    t.set_games_remove(tr(lang, "Games_Remove").into());
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
    t.set_settings_h1(tr(lang, "Settings_H1").into());
    t.set_settings_sub(tr(lang, "Settings_Sub").into());
    t.set_settings_channel(tr(lang, "Settings_Channel").into());
    t.set_settings_channel_desc(tr(lang, "Settings_ChannelDesc").into());
    t.set_settings_channel_check(tr(lang, "Settings_ChannelCheck").into());
    t.set_settings_channel_ver(tr(lang, "Settings_ChannelVer").into());
    t.set_settings_channel_err(tr(lang, "Settings_ChannelErr").into());
    t.set_settings_channel_apply(tr(lang, "Settings_ChannelApply").into());
    t.set_settings_channel_confirm(tr(lang, "Settings_ChannelConfirm").into());
    t.set_settings_lang(tr(lang, "Settings_Lang").into());
    t.set_settings_threads(tr(lang, "Settings_Threads").into());
    t.set_settings_threads_desc(tr(lang, "Settings_ThreadsDesc").into());
    t.set_settings_auto_update(tr(lang, "Settings_AutoUpdate").into());
    t.set_settings_auto_update_desc(tr(lang, "Settings_AutoUpdateDesc").into());
    t.set_settings_close_on_launch(tr(lang, "Settings_CloseOnLaunch").into());
    t.set_settings_close_on_launch_desc(tr(lang, "Settings_CloseOnLaunchDesc").into());
    t.set_settings_discord_rp(tr(lang, "Settings_DiscordRP").into());
    t.set_settings_discord_rp_desc(tr(lang, "Settings_DiscordRPDesc").into());
    t.set_settings_rp_place(tr(lang, "Settings_DiscordRP_Place").into());
    t.set_settings_rp_time(tr(lang, "Settings_DiscordRP_Time").into());
    t.set_settings_rp_brand(tr(lang, "Settings_DiscordRP_Brand").into());
    t.set_settings_discord_wait(tr(lang, "Settings_DiscordRP_Wait").into());
    t.set_settings_discord_connected(tr(lang, "Settings_DiscordRP_Connected").into());
    t.set_settings_discord_no_discord(tr(lang, "Settings_DiscordRP_NoDiscord").into());
    t.set_settings_deeplink(tr(lang, "Settings_Deeplink").into());
    t.set_settings_deeplink_desc(tr(lang, "Settings_DeeplinkDesc").into());
    t.set_settings_deeplink_on(tr(lang, "Settings_Deeplink_On").into());
    t.set_settings_deeplink_fail(tr(lang, "Settings_Deeplink_Fail").into());
    t.set_settings_path(tr(lang, "Settings_Path").into());
    t.set_settings_path_desc(tr(lang, "Settings_PathDesc").into());
    t.set_settings_path_reset(tr(lang, "Settings_PathReset").into());
    t.set_settings_cleanup(tr(lang, "Settings_Cleanup").into());
    t.set_settings_cleanup_desc(tr(lang, "Settings_CleanupDesc").into());
    t.set_settings_cleanup_btn(tr(lang, "Settings_CleanupBtn").into());
    t.set_settings_cleanup_busy(tr(lang, "Settings_CleanupBusy").into());
    t.set_settings_sound(tr(lang, "Settings_Sound").into());
    t.set_settings_sound_desc(tr(lang, "Settings_SoundDesc").into());
    t.set_settings_sound_on(tr(lang, "Settings_SoundOn").into());
    t.set_settings_sound_volume(tr(lang, "Settings_SoundVolume").into());
    t.set_settings_picture(tr(lang, "Settings_Picture").into());
    t.set_settings_picture_desc(tr(lang, "Settings_PictureDesc").into());
    t.set_settings_picture_on(tr(lang, "Settings_PictureOn").into());
    t.set_settings_pic_bright(tr(lang, "Settings_PicBright").into());
    t.set_settings_pic_sat(tr(lang, "Settings_PicSat").into());
    t.set_settings_pic_contrast(tr(lang, "Settings_PicContrast").into());
    t.set_settings_close_tray(tr(lang, "Settings_CloseTray").into());
    t.set_settings_close_tray_desc(tr(lang, "Settings_CloseTrayDesc").into());
    t.set_settings_show_logs(tr(lang, "Settings_ShowLogs").into());
    t.set_settings_show_logs_desc(tr(lang, "Settings_ShowLogsDesc").into());
    t.set_settings_auto_clean(tr(lang, "Settings_AutoClean").into());
    t.set_settings_auto_clean_desc(tr(lang, "Settings_AutoCleanDesc").into());
    t.set_settings_profile(tr(lang, "Settings_Profile").into());
    t.set_settings_profile_desc(tr(lang, "Settings_ProfileDesc").into());
    t.set_settings_profile_export(tr(lang, "Settings_ProfileExport").into());
    t.set_settings_profile_import(tr(lang, "Settings_ProfileImport").into());
    t.set_btn_yes(tr(lang, "Btn_Yes").into());
    t.set_btn_no(tr(lang, "Btn_No").into());
    t.set_mods_h1(tr(lang, "Mods_H1").into());
    t.set_mods_sub(tr(lang, "Mods_Sub").into());
    t.set_mods_where(tr(lang, "Mods_Where").into());
    t.set_mods_open_folder(tr(lang, "Mods_OpenFolder").into());
    t.set_mods_refresh(tr(lang, "Mods_Refresh").into());
    t.set_mods_apply(tr(lang, "Mods_Apply").into());
    t.set_mods_revert(tr(lang, "Mods_Revert").into());
    t.set_mods_empty_managed(tr(lang, "Mods_EmptyManaged").into());
    t.set_mods_no_enabled(tr(lang, "Mods_NoEnabled").into());
    t.set_mods_up(tr(lang, "Mods_Up").into());
    t.set_mods_down(tr(lang, "Mods_Down").into());
    t.set_mods_st_applied(tr(lang, "Mods_StApplied").into());
    t.set_mods_st_missing(tr(lang, "Mods_StMissing").into());
    t.set_mods_st_clean(tr(lang, "Mods_StClean").into());
    t.set_mods_profiles(tr(lang, "Mods_Profiles").into());
    t.set_mods_profiles_empty(tr(lang, "Mods_ProfilesEmpty").into());
    t.set_prof_select(tr(lang, "Prof_Select").into());
    t.set_prof_load(tr(lang, "Prof_Load").into());
    t.set_prof_save_as(tr(lang, "Prof_SaveAs").into());
    t.set_prof_delete(tr(lang, "Prof_Delete").into());
    t.set_prof_export(tr(lang, "Prof_Export").into());
    t.set_prof_import(tr(lang, "Prof_Import").into());
    t.set_profile_name_hint(tr(lang, "Flags_ProfileName").into());
    t.set_flags_h1(tr(lang, "Flags_H1").into());
    t.set_flags_sub(tr(lang, "Flags_Sub").into());
    t.set_flags_search(tr(lang, "Flags_Search").into());
    t.set_flags_add(tr(lang, "Flags_Add").into());
    t.set_flags_save(tr(lang, "Flags_Save").into());
    t.set_flags_import(tr(lang, "Flags_Import").into());
    t.set_flags_paste(tr(lang, "Flags_Paste").into());
    t.set_flags_export(tr(lang, "Flags_Export").into());
    t.set_flags_delete(tr(lang, "Flags_Delete").into());
    t.set_flags_clear(tr(lang, "Flags_Clear").into());
    t.set_flags_presets(tr(lang, "Flags_Presets").into());
    t.set_flags_col_name(tr(lang, "Flags_ColName").into());
    t.set_flags_col_value(tr(lang, "Flags_ColValue").into());
    t.set_flags_col_desc(tr(lang, "Flags_ColDesc").into());
    t.set_flags_conflict_title(tr(lang, "Flags_ConflictTitle").into());
    t.set_flags_conflict_keep(tr(lang, "Flags_ConflictKeep").into());
    t.set_flags_conflict_replace(tr(lang, "Flags_ConflictReplace").into());
    t.set_flags_conflict_replace_all(tr(lang, "Flags_ConflictReplaceAll").into());
    t.set_flags_profiles(tr(lang, "Flags_Profiles").into());
    t.set_flags_profiles_empty(tr(lang, "Flags_ProfilesEmpty").into());
    t.set_flags_empty(tr(lang, "Empty_Flags").into());
    t.set_flags_search_empty(tr(lang, "Search_Empty").into());
    t.set_flags_no_client(tr(lang, "Home_Status_NoClient").into());
    t.set_common_soon(tr(lang, "Common_ComingSoon").into());
}

// ===== Колбэки UI =====

fn wire(ui: &AppWindow, ctx: &Rc<RefCell<Ctx>>) {
    let weak = ctx.borrow().weak.clone();

    // — Окно —
    {
        let w = weak.clone();
        #[cfg_attr(not(windows), allow(unused_variables))]
        let ctx2 = Rc::clone(ctx);
        ui.on_win_min(move || {
            let ui = &w.unwrap();
            // Режим «сворачивать в трей»: кнопка «—» прячет окно с панели
            // задач, процесс остаётся в трее.
            #[cfg(windows)]
            if ctx2.borrow().config.close_to_tray {
                let _ = ui.hide();
                return;
            }
            winit_min(ui);
        });
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
            let mut c = ctx.borrow_mut();
            c.config.save();
            #[cfg(windows)]
            let to_tray = c.config.close_to_tray;
            #[cfg(not(windows))]
            let to_tray = false;
            if to_tray {
                // Окно скрывается, лаунчер остаётся в трее.
                let ui = c.weak.clone().unwrap();
                let _ = ui.hide();
                logger::info("Окно скрыто — работа в трее");
            } else {
                c.quit = true;
                logger::info("Окно закрыто — конфиг сохранён");
                let _ = slint::quit_event_loop();
            }
        });
    }

    // — Games —
    {
        let ctx = Rc::clone(ctx);
        let weak = weak.clone();
        ui.on_games_add(move |t: SharedString| {
            ctx.borrow_mut().games_go(&t);
            let c = ctx.borrow();
            let ui = weak.unwrap();
            c.push_games(&ui);
        });
    }
    {
        let ctx = Rc::clone(ctx);
        let weak = weak.clone();
        ui.on_games_launch(move |id: SharedString| {
            if let Ok(id) = id.parse::<u64>() {
                ctx.borrow_mut().launch_place(id);
                let c = ctx.borrow();
                let ui = weak.unwrap();
                c.push_games(&ui);
            }
        });
    }
    {
        let ctx = Rc::clone(ctx);
        let weak = weak.clone();
        ui.on_games_fav(move |id: SharedString| {
            ctx.borrow_mut().games_fav(&id);
            let c = ctx.borrow();
            let ui = weak.unwrap();
            c.push_games(&ui);
        });
    }
    {
        let ctx = Rc::clone(ctx);
        let weak = weak.clone();
        ui.on_games_remove(move |id: SharedString| {
            ctx.borrow_mut().games_remove(&id);
            let c = ctx.borrow();
            let ui = weak.unwrap();
            c.push_games(&ui);
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
                ctx.borrow_mut().check_updates(true);
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

    // — Настройки —
    {
        let ctx = Rc::clone(ctx);
        let weak = weak.clone();
        ui.on_ch_pick(move |ch: SharedString| {
            {
                ctx.borrow_mut().channel_pick(ch.as_str());
            }
            let c = ctx.borrow();
            c.push_settings(&weak.unwrap());
        });
    }
    {
        let ctx = Rc::clone(ctx);
        let weak = weak.clone();
        ui.on_ch_apply(move || {
            {
                ctx.borrow_mut().channel_apply_request();
            }
            let c = ctx.borrow();
            c.push_settings(&weak.unwrap());
            c.push_confirm(&weak.unwrap());
        });
    }
    {
        let ctx = Rc::clone(ctx);
        let weak = weak.clone();
        ui.on_lang_pick(move |i: i32| {
            let lang = {
                let mut c = ctx.borrow_mut();
                c.set_lang(Lang::resolve(if i == 1 { "en" } else { "ru" }));
                c.lang
            };
            let ui = weak.unwrap();
            apply_i18n(&ui, lang);
            ctx.borrow().push_settings(&ui);
        });
    }
    {
        let ctx = Rc::clone(ctx);
        let weak = weak.clone();
        ui.on_threads_set(move |t: i32| {
            {
                let mut c = ctx.borrow_mut();
                c.config.download_threads = t.clamp(1, 8) as u32;
                c.config.save();
            }
            let c = ctx.borrow();
            c.push_settings(&weak.unwrap());
        });
    }
    {
        let ctx = Rc::clone(ctx);
        let weak = weak.clone();
        ui.on_auto_update_set(move |v: bool| {
            {
                let mut c = ctx.borrow_mut();
                c.config.auto_update = v;
                c.config.save();
            }
            let c = ctx.borrow();
            c.push_settings(&weak.unwrap());
        });
    }
    {
        let ctx = Rc::clone(ctx);
        let weak = weak.clone();
        ui.on_close_launch_set(move |v: bool| {
            {
                let mut c = ctx.borrow_mut();
                c.config.close_on_launch = v;
                c.config.save();
            }
            let c = ctx.borrow();
            c.push_settings(&weak.unwrap());
        });
    }
    {
        let ctx = Rc::clone(ctx);
        let weak = weak.clone();
        ui.on_discord_rp_set(move |v: bool| {
            {
                let mut c = ctx.borrow_mut();
                c.config.discord_rp = v;
                c.config.save();
                c.sync_discord();
            }
            let c = ctx.borrow();
            c.push_settings(&weak.unwrap());
        });
    }
    {
        let ctx = Rc::clone(ctx);
        let weak = weak.clone();
        ui.on_rp_opt_set(move |k: i32, v: bool| {
            {
                let mut c = ctx.borrow_mut();
                match k {
                    0 => c.config.discord_rp_place = v,
                    1 => c.config.discord_rp_time = v,
                    _ => c.config.discord_rp_brand = v,
                }
                c.config.save();
                c.sync_discord();
            }
            let c = ctx.borrow();
            c.push_settings(&weak.unwrap());
        });
    }
    {
        let ctx = Rc::clone(ctx);
        let weak = weak.clone();
        ui.on_deeplink_set(move |v: bool| {
            {
                let mut c = ctx.borrow_mut();
                c.config.deeplink = v;
                c.config.save();
                if v {
                    let _ = deep_link::register();
                } else {
                    let _ = deep_link::unregister();
                }
                c.deeplink_ok = deep_link::status();
            }
            let c = ctx.borrow();
            c.push_settings(&weak.unwrap());
        });
    }
    {
        let ctx = Rc::clone(ctx);
        ui.on_path_edited(move |t: SharedString| {
            let mut c = ctx.borrow_mut();
            c.config.bytestrap_path = t.to_string();
            c.config.save();
        });
    }
    {
        let ctx = Rc::clone(ctx);
        let weak = weak.clone();
        ui.on_path_reset(move || {
            {
                let mut c = ctx.borrow_mut();
                c.config.bytestrap_path.clear();
                c.config.save();
            }
            let c = ctx.borrow();
            c.push_path(&weak.unwrap());
        });
    }
    {
        let ctx = Rc::clone(ctx);
        let weak = weak.clone();
        ui.on_cleanup_run(move || {
            {
                ctx.borrow_mut().start_cleanup();
            }
            let c = ctx.borrow();
            c.push_settings(&weak.unwrap());
        });
    }
    {
        let ctx = Rc::clone(ctx);
        let weak = weak.clone();
        let audio = crate::audio::global();
        ui.on_sound_set(move |v: bool| {
            audio.set_enabled(v);
            {
                let mut c = ctx.borrow_mut();
                c.config.sound = v;
                c.config.save();
            }
            let c = ctx.borrow();
            c.push_settings(&weak.unwrap());
        });
    }
    // — Оверлей «Картинка клиента» —
    {
        let ctx = Rc::clone(ctx);
        let weak = weak.clone();
        ui.on_overlay_set(move |v: bool| {
            ctx.borrow_mut().set_overlay_on(v);
            ctx.borrow().refresh_tray();
            let c = ctx.borrow();
            c.push_settings(&weak.unwrap());
        });
    }
    {
        let ctx = Rc::clone(ctx);
        let weak = weak.clone();
        ui.on_overlay_bright_set(move |v: i32| {
            {
                let mut c = ctx.borrow_mut();
                c.config.overlay_bright = v.clamp(-50, 50);
                c.config.save();
                c.overlay.update(push_overlay(&c));
            }
            let c = ctx.borrow();
            c.push_settings(&weak.unwrap());
        });
    }
    {
        let ctx = Rc::clone(ctx);
        let weak = weak.clone();
        ui.on_overlay_sat_set(move |v: i32| {
            {
                let mut c = ctx.borrow_mut();
                c.config.overlay_sat = v.clamp(0, 200);
                c.config.save();
                c.overlay.update(push_overlay(&c));
            }
            let c = ctx.borrow();
            c.push_settings(&weak.unwrap());
        });
    }
    {
        let ctx = Rc::clone(ctx);
        let weak = weak.clone();
        ui.on_overlay_contrast_set(move |v: i32| {
            {
                let mut c = ctx.borrow_mut();
                c.config.overlay_contrast = v.clamp(50, 150);
                c.config.save();
                c.overlay.update(push_overlay(&c));
            }
            let c = ctx.borrow();
            c.push_settings(&weak.unwrap());
        });
    }
    {
        let ctx = Rc::clone(ctx);
        let weak = weak.clone();
        let audio = crate::audio::global();
        ui.on_sound_vol_set(move |v: i32| {
            audio.set_volume(v.clamp(0, 100) as u32);
            {
                let mut c = ctx.borrow_mut();
                c.config.sound_volume = v.clamp(0, 100) as u32;
                c.config.save();
            }
            let c = ctx.borrow();
            c.push_settings(&weak.unwrap());
        });
    }
    // — Трей и профиль конфигурации —
    {
        let ctx = Rc::clone(ctx);
        ui.on_close_tray_set(move |v: bool| {
            let mut c = ctx.borrow_mut();
            c.config.close_to_tray = v;
            c.config.save();
        });
    }
    {
        let ctx = Rc::clone(ctx);
        ui.on_auto_clean_set(move |v: bool| {
            let mut c = ctx.borrow_mut();
            c.config.auto_clean = v;
            c.config.save();
        });
    }
    {
        let ctx = Rc::clone(ctx);
        ui.on_show_logs_set(move |v: bool| {
            let mut c = ctx.borrow_mut();
            c.config.show_logs = v;
            c.config.save();
            let ui = c.weak.clone().unwrap();
            ui.set_set_show_logs(v);
            // Скрыли «Логи», а открыты они — вернуться на главную.
            if !v && ui.get_page() == 5 {
                ui.set_page(0);
            }
            c.push_settings(&ui);
        });
    }
    {
        let ctx = Rc::clone(ctx);
        ui.on_cfg_export(move || {
            ctx.borrow_mut().cfg_export();
            let c = ctx.borrow();
            let ui = c.weak.clone().unwrap();
            c.push_settings(&ui);
        });
    }
    {
        let ctx = Rc::clone(ctx);
        ui.on_cfg_import(move || {
            ctx.borrow_mut().cfg_import();
        });
    }
    {
        let ctx = Rc::clone(ctx);
        let weak = weak.clone();
        ui.on_confirm_yes(move || {
            {
                ctx.borrow_mut().confirm_yes();
            }
            let c = ctx.borrow();
            c.push_settings(&weak.unwrap());
            c.push_home(&weak.unwrap());
            c.push_confirm(&weak.unwrap());
            c.push_mods(&weak.unwrap());
            c.push_dlg(&weak.unwrap());
            c.push_flags_rows(&weak.unwrap());
            c.push_flags_meta(&weak.unwrap());
        });
    }
    {
        let ctx = Rc::clone(ctx);
        let weak = weak.clone();
        ui.on_confirm_no(move || {
            ctx.borrow_mut().confirm_no();
            let c = ctx.borrow();
            c.push_settings(&weak.unwrap());
            c.push_confirm(&weak.unwrap());
        });
    }

    // — Моды —
    {
        let ctx = Rc::clone(ctx);
        ui.on_mods_open_folder(move || {
            ctx.borrow().mods_open_folder();
        });
    }
    {
        let ctx = Rc::clone(ctx);
        let weak = weak.clone();
        ui.on_mods_refresh(move || {
            {
                ctx.borrow_mut().refresh_mods();
            }
            let c = ctx.borrow();
            c.push_mods(&weak.unwrap());
        });
    }
    {
        let ctx = Rc::clone(ctx);
        let weak = weak.clone();
        ui.on_mods_apply(move || {
            {
                ctx.borrow_mut().start_mods_apply();
            }
            let c = ctx.borrow();
            c.push_mods(&weak.unwrap());
        });
    }
    {
        let ctx = Rc::clone(ctx);
        let weak = weak.clone();
        ui.on_mods_revert(move || {
            ctx.borrow_mut().confirm = Some(Confirm::RevertMods);
            let c = ctx.borrow();
            c.push_confirm(&weak.unwrap());
        });
    }
    {
        let ctx = Rc::clone(ctx);
        let weak = weak.clone();
        ui.on_mods_toggle_mod(move |id: SharedString, on: bool| {
            {
                let mut c = ctx.borrow_mut();
                c.mods_toggle(id.as_str(), on);
                c.refresh_mods();
            }
            let c = ctx.borrow();
            c.push_mods(&weak.unwrap());
        });
    }
    {
        let ctx = Rc::clone(ctx);
        let weak = weak.clone();
        ui.on_mods_move_mod(move |id: SharedString, d: i32| {
            {
                let mut c = ctx.borrow_mut();
                c.mods_move(id.as_str(), d);
                c.refresh_mods();
            }
            let c = ctx.borrow();
            c.push_mods(&weak.unwrap());
        });
    }
    {
        let ctx = Rc::clone(ctx);
        ui.on_mods_open_mod_folder(move |id: SharedString| {
            ctx.borrow().mods_open_mod_folder(id.as_str());
        });
    }
    {
        let ctx = Rc::clone(ctx);
        let weak = weak.clone();
        ui.on_mods_profile_pick(move |i: i32| {
            {
                let mut c = ctx.borrow_mut();
                if i >= 0 {
                    let names = mods::list_profiles(&c.config.base());
                    if let Some(n) = names.get(i as usize) {
                        c.profile_sel = n.clone();
                    }
                }
            }
            let c = ctx.borrow();
            c.push_mods(&weak.unwrap());
        });
    }
    {
        let ctx = Rc::clone(ctx);
        let weak = weak.clone();
        ui.on_mods_profile_load(move || {
            {
                ctx.borrow_mut().profile_load_click();
            }
            let c = ctx.borrow();
            c.push_mods(&weak.unwrap());
            c.push_confirm(&weak.unwrap());
        });
    }
    {
        let ctx = Rc::clone(ctx);
        let weak = weak.clone();
        ui.on_mods_profile_save_as(move || {
            {
                ctx.borrow_mut().open_save_dlg(Dlg::ModsProfile);
            }
            let c = ctx.borrow();
            c.push_dlg(&weak.unwrap());
            c.push_dlg_input(&weak.unwrap());
        });
    }
    {
        let ctx = Rc::clone(ctx);
        let weak = weak.clone();
        ui.on_mods_profile_delete(move || {
            {
                let mut c = ctx.borrow_mut();
                if c.profile_sel_valid() {
                    let name = c.profile_sel.clone();
                    c.confirm = Some(Confirm::DeleteModProfile(name));
                }
            }
            let c = ctx.borrow();
            c.push_confirm(&weak.unwrap());
        });
    }
    {
        let ctx = Rc::clone(ctx);
        let weak = weak.clone();
        ui.on_mods_profile_export(move || {
            {
                ctx.borrow_mut().profile_export();
            }
            let c = ctx.borrow();
            c.push_mods(&weak.unwrap());
        });
    }
    {
        let ctx = Rc::clone(ctx);
        let weak = weak.clone();
        ui.on_mods_profile_import(move || {
            {
                ctx.borrow_mut().profile_import();
            }
            let c = ctx.borrow();
            c.push_mods(&weak.unwrap());
        });
    }

    // — Вводной диалог —
    {
        let ctx = Rc::clone(ctx);
        ui.on_dlg_edited(move |t: SharedString| {
            // try_borrow: edited может прийти во время публикации свойств.
            if let Ok(mut c) = ctx.try_borrow_mut() {
                c.dlg_input = t.to_string();
            }
        });
    }
    {
        let ctx = Rc::clone(ctx);
        let weak = weak.clone();
        ui.on_dlg_ok(move |t: SharedString| {
            {
                ctx.borrow_mut().save_dlg_ok(t.as_str());
            }
            let c = ctx.borrow();
            c.push_dlg(&weak.unwrap());
            c.push_dlg_input(&weak.unwrap());
            c.push_mods(&weak.unwrap());
        });
    }
    {
        let ctx = Rc::clone(ctx);
        let weak = weak.clone();
        ui.on_dlg_cancel(move || {
            {
                ctx.borrow_mut().save_dlg_cancel();
            }
            let c = ctx.borrow();
            c.push_dlg(&weak.unwrap());
            c.push_dlg_input(&weak.unwrap());
        });
    }

    // — FastFlags —
    {
        let ctx = Rc::clone(ctx);
        let weak = weak.clone();
        ui.on_flags_search_edited(move |q: SharedString| {
            {
                ctx.borrow_mut().flags_search_edited(q.as_str());
            }
            let c = ctx.borrow();
            c.push_flags_rows(&weak.unwrap());
        });
    }
    {
        let ctx = Rc::clone(ctx);
        let weak = weak.clone();
        ui.on_flags_add(move || {
            {
                ctx.borrow_mut().flags_add();
            }
            let c = ctx.borrow();
            c.push_flags_rows(&weak.unwrap());
            c.push_flags_meta(&weak.unwrap());
        });
    }
    {
        let ctx = Rc::clone(ctx);
        let weak = weak.clone();
        ui.on_flags_save(move || {
            {
                ctx.borrow_mut().flags_save();
            }
            let c = ctx.borrow();
            c.push_flags_meta(&weak.unwrap());
        });
    }
    {
        let ctx = Rc::clone(ctx);
        let weak = weak.clone();
        ui.on_flags_import(move || {
            {
                ctx.borrow_mut().flags_import_file();
            }
            let c = ctx.borrow();
            c.push_flags_rows(&weak.unwrap());
            c.push_flags_meta(&weak.unwrap());
        });
    }
    {
        let ctx = Rc::clone(ctx);
        let weak = weak.clone();
        ui.on_flags_paste(move || {
            {
                ctx.borrow_mut().flags_paste();
            }
            let c = ctx.borrow();
            c.push_flags_rows(&weak.unwrap());
            c.push_flags_meta(&weak.unwrap());
        });
    }
    {
        let ctx = Rc::clone(ctx);
        let weak = weak.clone();
        ui.on_flags_export(move || {
            {
                ctx.borrow_mut().flags_export();
            }
            let c = ctx.borrow();
            c.push_flags_meta(&weak.unwrap());
        });
    }
    {
        let ctx = Rc::clone(ctx);
        let weak = weak.clone();
        ui.on_flags_delete(move || {
            {
                ctx.borrow_mut().flags_delete_sel();
            }
            let c = ctx.borrow();
            c.push_flags_rows(&weak.unwrap());
            c.push_flags_meta(&weak.unwrap());
        });
    }
    {
        let ctx = Rc::clone(ctx);
        let weak = weak.clone();
        ui.on_flags_clear(move || {
            {
                ctx.borrow_mut().flags_clear_request();
            }
            let c = ctx.borrow();
            c.push_confirm(&weak.unwrap());
        });
    }
    {
        let ctx = Rc::clone(ctx);
        let weak = weak.clone();
        ui.on_flags_row_select(move |i: i32| {
            {
                ctx.borrow_mut().flags_row_select(i.max(0) as usize);
            }
            let c = ctx.borrow();
            c.push_flags_rows(&weak.unwrap());
            c.push_flags_meta(&weak.unwrap());
        });
    }
    {
        let ctx = Rc::clone(ctx);
        let weak = weak.clone();
        ui.on_flags_cell_edited(move |idx: i32, col: i32, t: SharedString| {
            {
                ctx.borrow_mut().flags_cell_edited(idx.max(0) as usize, col, t.as_str());
            }
            let c = ctx.borrow();
            // Без push_flags_rows: пересборка строк съела бы фокус ячейки.
            c.push_flags_meta(&weak.unwrap());
        });
    }
    {
        let ctx = Rc::clone(ctx);
        let weak = weak.clone();
        ui.on_preset_pick(move |pi: i32, oi: i32| {
            {
                ctx.borrow_mut().preset_pick(pi, oi);
            }
            let c = ctx.borrow();
            c.push_flags_rows(&weak.unwrap());
            c.push_flags_meta(&weak.unwrap());
        });
    }
    {
        let ctx = Rc::clone(ctx);
        let weak = weak.clone();
        ui.on_preset_toggle(move |pi: i32, on: bool| {
            {
                ctx.borrow_mut().preset_toggle(pi, on);
            }
            let c = ctx.borrow();
            c.push_flags_rows(&weak.unwrap());
            c.push_flags_meta(&weak.unwrap());
        });
    }
    {
        let ctx = Rc::clone(ctx);
        let weak = weak.clone();
        ui.on_flags_profile_pick(move |i: i32| {
            {
                let mut c = ctx.borrow_mut();
                if i >= 0 {
                    let names: Vec<String> = flags::load_profiles(&c.config.base())
                        .profiles
                        .keys()
                        .cloned()
                        .collect();
                    if let Some(n) = names.get(i as usize) {
                        c.profile_sel = n.clone();
                    }
                }
            }
            let c = ctx.borrow();
            c.push_flags_rows(&weak.unwrap());
        });
    }
    {
        let ctx = Rc::clone(ctx);
        let weak = weak.clone();
        ui.on_flags_profile_load(move || {
            {
                ctx.borrow_mut().flags_profile_load();
            }
            let c = ctx.borrow();
            c.push_flags_rows(&weak.unwrap());
            c.push_flags_meta(&weak.unwrap());
            c.push_confirm(&weak.unwrap());
        });
    }
    {
        let ctx = Rc::clone(ctx);
        let weak = weak.clone();
        ui.on_flags_profile_save_as(move || {
            {
                ctx.borrow_mut().open_save_dlg(Dlg::FlagsProfile);
            }
            let c = ctx.borrow();
            c.push_dlg(&weak.unwrap());
            c.push_dlg_input(&weak.unwrap());
        });
    }
    {
        let ctx = Rc::clone(ctx);
        let weak = weak.clone();
        ui.on_flags_profile_delete(move || {
            {
                ctx.borrow_mut().flags_profile_delete_request();
            }
            let c = ctx.borrow();
            c.push_confirm(&weak.unwrap());
        });
    }
    {
        let ctx = Rc::clone(ctx);
        let weak = weak.clone();
        ui.on_flags_profile_export(move || {
            {
                ctx.borrow_mut().flags_profile_export();
            }
            let c = ctx.borrow();
            c.push_flags_meta(&weak.unwrap());
        });
    }
    {
        let ctx = Rc::clone(ctx);
        let weak = weak.clone();
        ui.on_flags_profile_import(move || {
            {
                ctx.borrow_mut().flags_profile_import();
            }
            let c = ctx.borrow();
            c.push_flags_rows(&weak.unwrap());
            c.push_flags_meta(&weak.unwrap());
        });
    }
    {
        let ctx = Rc::clone(ctx);
        let weak = weak.clone();
        ui.on_import_decision(move |d: i32| {
            {
                ctx.borrow_mut().import_decide(d);
            }
            let c = ctx.borrow();
            c.push_flags_rows(&weak.unwrap());
            c.push_flags_meta(&weak.unwrap());
        });
    }
}

// ===== Точка входа =====

/// Точка входа: основной (и единственный) бинарник `bytestrap`.
pub fn run() -> Result<(), slint::PlatformError> {
    logger::info("Интерфейс Slint запущен");

    let ui = AppWindow::new()?;
    clamp_window_to_screen(&ui);
    let config = Config::load();
    let lang = Lang::resolve(&config.language);
    apply_i18n(&ui, lang);

    // Звук интерфейса: движок (только Windows), начальные громкость/вкл.
    let audio = crate::audio::global();
    audio.set_enabled(config.sound);
    audio.set_volume(config.sound_volume);
    {
        let snd = ui.global::<Snd>();
        snd.set_enabled(config.sound);
        snd.set_volume(config.sound_volume.min(100) as i32);
        let audio = crate::audio::global();
        snd.on_play(move |kind| audio.play(kind));
    }

    let installed = effective_installed(&config);
    let mods_count = mods::scan_mods(&config.base()).len();
    let flags_count = match &installed {
        Some(guid) => flags::load(&config.base().join("Versions").join(guid)).len(),
        None => 0,
    };
    let playtime = Playtime::load(&config.base());

    // roblox:// в реестре: самовосстановление + статус для Settings.
    if config.deeplink {
        deep_link::ensure();
    }
    let deeplink_ok = deep_link::status();
    let channel_draft = config.channel.clone();
    let (discord, discord_rx) = discord::spawn();
    // RP: сразу показываем статус лаунчера (если включено).
    if config.discord_rp {
        let _ = discord.send(discord::Update::Launcher);
    }

    let (tx, rx) = mpsc::channel();
    let places = Places::load(&config.base());
    // Авто-очистка: раз в сутки при старте (если включена).
    if config.auto_clean && playtime::now_unix() - config.auto_clean_last > 20 * 3600 {
        let mut cfg = config.clone();
        cfg.auto_clean_last = playtime::now_unix();
        cfg.save();
        let base_clean = config.base().clone();
        let tx = tx.clone();
        std::thread::Builder::new()
            .name("auto-clean".into())
            .spawn(move || {
                let rep = crate::core::cleanup::clean(&base_clean);
                let _ = tx.send(AsyncEvent::CleanupDone(rep.freed, rep.files));
            })
            .ok();
    }

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

    // Трей строится на главном потоке (win32-цикл качает winit).
    #[cfg(windows)]
    tray::spawn(tx.clone());

    let weak = ui.as_weak();
    let overlay = overlay::spawn(overlay::Params {
        enabled: config.overlay_on,
        bright: config.overlay_bright,
        satur: config.overlay_sat,
        contrast: config.overlay_contrast,
    });
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
        channel_draft,
        channel_preview: None,
        channel_checking: false,
        confirm: None,
        cleanup_busy: false,
        cleanup_msg: None,
        discord,
        discord_rx,
        discord_status: discord::Status::Idle,
        deeplink_ok,
        pending_launch: None,
        mods_count,
        flags_count,
        playtime,
        places,
        link_msg: None,
        games_msg: None,
        cfg_msg: None,
        overlay,
        session_active: false,
        session_secs: 0,
        session_place: None,
        session_display: None,
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
        mods_list: Vec::new(),
        mods_files: Vec::new(),
        mods_scanned: false,
        mods_dirty: false,
        mods_msg: None,
        profile_sel: String::new(),
        save_dlg: None,
        dlg_input: String::new(),
        flags: Vec::new(),
        flags_loaded_for: None,
        flags_selected: None,
        flags_dirty: false,
        flags_search: String::new(),
        flag_msg: None,
        import: None,
    }));

    wire(&ui, &ctx);
    // Меню трея: начальные данные (оверлей, профили, топ игр).
    ctx.borrow().refresh_tray();
    ui.set_about_version(updater::current_version().into());
    ui.set_win_share(cfg!(windows));
    if ctx.borrow().config.auto_update {
        ctx.borrow_mut().check_updates(false);
    }
    ctx.borrow().push_home(&ui);

    // Клик по roblox:// в командной строке (друзья, приватки, плейсы):
    // сразу запускаем игру с этой ссылкой (счётчик сессии и RP работают).
    if let Some(url) = std::env::args().skip(1).find(|a| a.starts_with("roblox://")) {
        ctx.borrow_mut().play(Some(&url));
    }

    // Перетащенные в окно файлы → папка модов (как в egui, только Windows).
    #[cfg(windows)]
    {
        let drop_ctx = Rc::clone(&ctx);
        let drop_weak = weak.clone();
        ui.window().on_winit_window_event(move |_win, event| {
            if let winit::event::WindowEvent::DroppedFile(path) = event {
                let Some(page_ui) = drop_weak.upgrade() else {
                    return EventResult::Propagate;
                };
                if page_ui.get_page() == 4 {
                    let mut c = drop_ctx.borrow_mut();
                    match mods::import_paths(&c.config.base(), std::slice::from_ref(path)) {
                        Ok(n) => {
                            c.refresh_mods();
                            c.refresh_counters();
                            c.mods_dirty = true;
                            let text = fmt(c.lang, "Mods_ImportedFmt", &[&n.to_string()]);
                            c.mods_msg = Some((text, false));
                            logger::info(format!("Перетащено файлов модов: {n}"));
                        }
                        Err(e) => c.mods_msg = Some((e.to_string(), true)),
                    }
                }
            }
            EventResult::Propagate
        });
    }

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
                if page == 1 {
                    c.push_games(&ui);
                }
                if page == 2 {
                    // Полный пуш только при входе: пересборка строк каждые
                    // 100 мс съедала бы фокус редактируемых ячеек.
                    c.push_flags(&ui);
                }
                if page == 3 {
                    c.push_versions(&ui);
                }
                if page == 4 {
                    if !c.mods_scanned {
                        c.refresh_mods();
                    }
                    c.push_mods(&ui);
                    c.push_dlg(&ui);
                }
                if page == 5 {
                    c.logs_tick(&ui);
                }
                if page == 6 {
                    c.push_settings(&ui);
                    c.push_path(&ui);
                }
            }
            if page == 1 {
                // Места могли смениться фоновым фетчем имени.
                c.push_games(&ui);
            }
            if page == 4 && c.mods_dirty {
                c.mods_dirty = false;
                c.push_mods(&ui);
                c.push_dlg(&ui);
            }
            if page == 6 {
                c.push_settings(&ui);
            }
            if page == 7 {
                c.push_about(&ui);
            }
            c.push_confirm(&ui);
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
            if ui.get_page() == 5 {
                c.logs_tick(&ui);
            }
        });
    }

    let res = ui.run();

    ctx.borrow().config.save();
    logger::info("Интерфейс Slint завершён");
    res
}

// ===== Визуальные скриншоты (headless) =====
// BYTESTRAP_SHOTS=1 cargo test take_page_shots -- --nocapture
// Рендерит каждую страницу в target/shots/*.png через software renderer
// (i-slint-backend-testing), без дисплея. В обычном cargo test — no-op.
#[cfg(test)]
mod shots {
    use super::*;

    fn rows<T: Clone + 'static>(v: Vec<T>) -> ModelRc<T> {
        ModelRc::new(VecModel::from(v))
    }

    fn strings(v: &[&str]) -> ModelRc<SharedString> {
        rows(v.iter().map(|s| (*s).into()).collect())
    }

    fn shots_setup() -> AppWindow {
        slint::platform::set_platform(Box::new(i_slint_backend_testing::TestingBackend::new(
            i_slint_backend_testing::TestingBackendOptions {
                mock_time: true,
                threading: false,
                renderer_name: Some("software".into()),
            },
        )))
        .expect("set_platform");
        // configure_test_fonts() недоступен: в опубликованном крейте
        // отсутствуют include_bytes-шрифты из репозитория Slint. Используем
        // системные шрифты (fontique).

        let ui = AppWindow::new().expect("AppWindow");
        apply_i18n(&ui, Lang::Ru);

        // ---- Общее состояние (как после запуска с установленным клиентом) ----
        ui.set_status_text("Клиент готов".into());
        ui.set_status_dot(1);
        ui.set_busy(false);
        ui.set_primary_label("Играть".into());
        ui.set_version_text("Версия: 0.9.40".into());
        ui.set_mods_count("12".into());
        ui.set_flags_count("248".into());
        ui.set_hours_count("86".into());
        ui.set_show_update(true);
        ui.set_win_share(true);
        ui.set_progress(0.0);
        ui.set_progress_text("".into());

        // ---- Home ----
        ui.set_session_text("В игре 01:24:05 · Bloxburg".into());
        ui.set_top_places(rows(vec![
            PlaceRow { name: "Bloxburg".into(), duration: "14 ч 05 мин".into() },
            PlaceRow { name: "Grow a Garden".into(), duration: "9 ч 41 мин".into() },
            PlaceRow { name: "Rivals".into(), duration: "6 ч 12 мин".into() },
            PlaceRow { name: "Dead Rails".into(), duration: "3 ч 57 мин".into() },
            PlaceRow { name: "Blade Ball".into(), duration: "2 ч 30 мин".into() },
        ]));
        ui.set_link_paste_text("roblox://experiences/start?placeId=920587237".into());
        ui.set_link_msg("Ссылка скопирована".into());
        ui.set_link_msg_bad(false);

        // ---- Активность за 14 дней ----
        ui.set_play_days(rows(vec![
            0, 12, 34, 5, 0, 51, 63, 22, 0, 8, 41, 76, 18, 30,
        ]));
        ui.set_play_labels(strings(&[
            "12.11", "", "", "", "", "", "", "", "", "", "", "", "", "30.10",
        ]));

        // ---- Games ----
        ui.set_games_rows(rows(vec![
            GameRow { id: "920587237".into(), name: "Welcome to Bloxburg".into(), visits: "14".into(), last: "12.11".into(), fav: true, alt: false, bad: false },
            GameRow { id: "12688469563406".into(), name: "Grow a Garden".into(), visits: "9".into(), last: "11.11".into(), fav: false, alt: true, bad: false },
            GameRow { id: "17625359962".into(), name: "Rivals".into(), visits: "6".into(), last: "10.11".into(), fav: true, alt: false, bad: false },
            GameRow { id: "7040729968".into(), name: "7734576442".into(), visits: "3".into(), last: "08.11".into(), fav: false, alt: true, bad: true },
            GameRow { id: "13924028732".into(), name: "Blade Ball".into(), visits: "2".into(), last: "05.11".into(), fav: false, alt: false, bad: false },
        ]));
        ui.set_games_msg("Место 920587237 запущено".into());
        ui.set_games_msg_err(false);

        // ---- Flags ----
        ui.set_flags_has_client(true);
        ui.set_flags_table_empty(false);
        ui.set_flags_dirty(true);
        ui.set_flags_msg("Импортировано: 3, пропущено: 1".into());
        ui.set_flags_msg_err(false);
        ui.set_flags_search("".into());
        ui.set_flags_can_delete(true);
        ui.set_flags_sel_detail(
            "Rendering · Включает новое освещение · Отрисовка нового освещения".into(),
        );
        ui.set_flags_rows(rows(vec![
            FlagRow { idx: 0, name: "FFlagDebugForceFutureLights".into(), value: "true".into(), desc: "Rendering · Новое освещение".into(), selected: true, alt: false, works: false },
            FlagRow { idx: 1, name: "DFFlagTextureQualityOverride".into(), value: "2".into(), desc: "Graphics · Качество текстур".into(), selected: false, alt: true, works: true },
            FlagRow { idx: 2, name: "FIntDebugFRMClamp".into(), value: "16".into(), desc: "Performance · Ограничение FRM".into(), selected: false, alt: false, works: false },
            FlagRow { idx: 3, name: "FFlagRenderFixPBR".into(), value: "false".into(), desc: "Rendering · PBR-фикс".into(), selected: false, alt: true, works: false },
            FlagRow { idx: 4, name: "FIntTextureQualityOverride1".into(), value: "3".into(), desc: "Graphics · Уровень текстур".into(), selected: false, alt: false, works: false },
            FlagRow { idx: 5, name: "FFlagHandleAltEnterFullscreenManually".into(), value: "false".into(), desc: "UI · Полноэкранный режим".into(), selected: false, alt: true, works: true },
            FlagRow { idx: 6, name: "DFIntTaskSchedulerTargetFps".into(), value: "60".into(), desc: "Performance · Лимит FPS".into(), selected: false, alt: false, works: false },
            FlagRow { idx: 7, name: "FFlagEnableInGameMenuModern".into(), value: "true".into(), desc: "UI · Новое меню".into(), selected: false, alt: true, works: false },
            FlagRow { idx: 8, name: "FIntRenderShadowIntensity".into(), value: "3".into(), desc: "Rendering · Тени".into(), selected: false, alt: false, works: false },
            FlagRow { idx: 9, name: "FFlagDebugPreferSingleThread".into(), value: "false".into(), desc: "Performance · Один поток".into(), selected: false, alt: true, works: false },
            FlagRow { idx: 10, name: "DFIntConnectionMTU".into(), value: "1400".into(), desc: "Network · MTU".into(), selected: false, alt: false, works: false },
            FlagRow { idx: 11, name: "FFlagAdServiceEnabled".into(), value: "false".into(), desc: "Ads · Реклама".into(), selected: false, alt: true, works: false },
        ]));
        // Реальные пресеты контроллера (на пустом наборе флагов — как при первом запуске).
        let preset_rows: Vec<PresetRow> = presets::PRESETS
            .iter()
            .map(|p| {
                let cur = presets::current_index(&[], p);
                let current = p.options.get(cur).copied().unwrap_or("Auto");
                PresetRow {
                    label: tr(Lang::Ru, p.label_key).into(),
                    desc: tr(Lang::Ru, p.desc_key).into(),
                    kind: if p.kind == presets::Kind::Toggle { 1 } else { 0 },
                    options: rows(
                        p.options
                            .iter()
                            .map(|o| local_option(Lang::Ru, o).into())
                            .collect::<Vec<SharedString>>(),
                    ),
                    selected: cur as i32,
                    current: local_option(Lang::Ru, current).into(),
                    on: cur == 1,
                    works: p.works,
                }
            })
            .collect();
        ui.set_flags_presets(rows(preset_rows));
        ui.set_flags_profiles(strings(&["Default", "PvP", "Низкий ПК"]));
        ui.set_flags_profile_sel_text("Default".into());
        ui.set_flags_profile_sel_index(0);
        ui.set_flags_profile_sel_valid(true);
        ui.set_flags_profiles_empty(false);

        // ---- Versions ----
        ui.set_ver_installed_guid("version-56d8f3a2c1b0e947".into());
        ui.set_ver_selected_guid("version-7c1e9b44a2d0f836".into());
        ui.set_ver_versions(rows(vec![
            VersionRow { guid: "version-56d8f3a2c1b0e947".into(), size: "1,42 ГБ".into(), active: true, no_player: false, alt: false },
            VersionRow { guid: "version-7c1e9b44a2d0f836".into(), size: "1,41 ГБ".into(), active: false, no_player: false, alt: true },
            VersionRow { guid: "version-9a02c7e51b63d480".into(), size: "1,39 ГБ".into(), active: false, no_player: false, alt: false },
            VersionRow { guid: "version-3f8d16c0e7a95b21".into(), size: "1,38 ГБ".into(), active: false, no_player: true, alt: true },
            VersionRow { guid: "version-0b47e2d98c15af63".into(), size: "1,35 ГБ".into(), active: false, no_player: false, alt: false },
        ]));
        ui.set_ver_integrity_kind(1);
        ui.set_ver_integrity_text("Хеш-сверка пройдена · 0 проблем".into());
        ui.set_ver_checked_text("checked: сегодня, 14:32".into());

        // ---- Mods ----
        ui.set_mods_path("C:\\Users\\Linar\\AppData\\Local\\ByteStrap\\Versions\\version-56d8f3a2c1b0e947".into());
        ui.set_mods_apply_enabled(true);
        ui.set_mods_revert_enabled(true);
        ui.set_mods_msg("Применено: 4 из 5".into());
        ui.set_mods_msg_err(false);
        ui.set_mods_files_count("Файлов: 6".into());
        ui.set_mods_list(rows(vec![
            ModRow { id: "Blaze ESP".into(), meta: "файлов: 3 · 12,4 МБ".into(), enabled: true, can_up: false, can_down: true },
            ModRow { id: "FPS Boost".into(), meta: "файлов: 1 · 84 КБ".into(), enabled: true, can_up: true, can_down: true },
            ModRow { id: "Old Death Sound".into(), meta: "файлов: 1 · 12 КБ".into(), enabled: true, can_up: true, can_down: true },
            ModRow { id: "Shaders".into(), meta: "файлов: 5 · 4,8 МБ".into(), enabled: false, can_up: true, can_down: true },
            ModRow { id: "Custom Skybox".into(), meta: "файлов: 2 · 2,1 МБ".into(), enabled: true, can_up: true, can_down: true },
            ModRow { id: "Low GFX".into(), meta: "файлов: 1 · 320 КБ".into(), enabled: false, can_up: true, can_down: true },
            ModRow { id: "Old Cursor".into(), meta: "файлов: 2 · 48 КБ".into(), enabled: false, can_up: true, can_down: true },
            ModRow { id: "Classic Menu".into(), meta: "файлов: 4 · 1,2 МБ".into(), enabled: false, can_up: true, can_down: false },
        ]));
        ui.set_mods_files(rows(vec![
            ModFileRow { rel: "Blaze/esp.lua".into(), size: "4,2 КБ".into(), status_kind: 0 },
            ModFileRow { rel: "Blaze/config.json".into(), size: "1,1 КБ".into(), status_kind: 0 },
            ModFileRow { rel: "fps-boost/flags.txt".into(), size: "84 B".into(), status_kind: 0 },
            ModFileRow { rel: "shaders/enable.post".into(), size: "12,8 КБ".into(), status_kind: 1 },
            ModFileRow { rel: "skybox/day.tex".into(), size: "1,9 МБ".into(), status_kind: 1 },
            ModFileRow { rel: "content/skybox.map".into(), size: "256 МБ".into(), status_kind: 2 },
        ]));
        ui.set_mods_profiles(strings(&["Default", "PvP"]));
        ui.set_mods_profile_sel_text("Default".into());
        ui.set_mods_profile_sel_index(0);
        ui.set_mods_profile_sel_valid(true);
        ui.set_mods_profiles_empty(false);

        // ---- Logs ----
        let log: Vec<(&str, i32, &str)> = vec![
            ("14:31:58", 1, "Запуск ByteStrap 0.9.40"),
            ("14:31:58", 0, "Загрузка конфигурации: config.json"),
            ("14:31:58", 1, "Клиент найден: version-56d8f3a2c1b0e947"),
            ("14:31:59", 1, "Проверка обновлений лаунчера…"),
            ("14:32:00", 1, "Доступна версия 0.9.41"),
            ("14:32:00", 0, "Discord RPC: инициализация"),
            ("14:32:01", 1, "Discord RPC подключён"),
            ("14:32:01", 2, "Каталог Mods отсутствует — создан"),
            ("14:32:02", 1, "Загружено флагов: 248"),
            ("14:32:02", 0, "Профиль флагов: Default"),
            ("14:32:03", 1, "Сессия запущена: placeId=920587237"),
            ("14:32:03", 0, "Rich Presence: обновление состояния"),
            ("14:32:04", 1, "Сессия завершена: 00:04:12"),
            ("14:32:05", 0, "Playtime: сохранение (86 ч)"),
            ("14:32:06", 3, "Не удалось применить мод: файл занят"),
            ("14:32:06", 0, "Повтор через 5 секунд…"),
            ("14:32:07", 1, "Мод применён: FPS Boost"),
            ("14:32:08", 2, "Клиент не запущен — проверка integrity пропущена"),
            ("14:32:09", 1, "Хеш-сверка пройдена: 12 файлов"),
            ("14:32:10", 0, "Кэш версий обновлён"),
            ("14:32:11", 1, "Deep-link зарегистрирован: roblox://"),
            ("14:32:12", 0, "Скачивание манифеста каналов…"),
            ("14:32:13", 1, "Канал: LIVE (0.9.40)"),
            ("14:32:14", 0, "Очистка временных файлов: 128 МБ"),
            ("14:32:15", 2, "Антивирус: файл подозрительный — пропущено"),
            ("14:32:16", 1, "Готово к запуску"),
        ];
        ui.set_log_lines(rows(
            log.iter()
                .enumerate()
                .map(|(i, (t, l, x))| LogLine {
                    time: (*t).into(),
                    level: *l,
                    text: (*x).into(),
                    zebra: i % 2 == 0,
                })
                .collect(),
        ));
        ui.set_log_files(strings(&["bytestrap.log", "Player.log", "Player.previous.log"]));
        ui.set_log_selected(0);
        ui.set_log_follow(true);
        ui.set_log_missing(false);
        ui.set_log_info("26 строк · bytestrap.log".into());

        // ---- Settings ----
        ui.set_set_ch_sel(0);
        ui.set_set_ch_checking(false);
        ui.set_set_ch_preview_kind(2);
        ui.set_set_ch_preview_text("Доступна версия 0.9.40".into());
        ui.set_set_ch_changed(false);
        ui.set_set_lang_code(0);
        ui.set_set_threads(4);
        ui.set_set_auto_update(true);
        ui.set_set_close_launch(false);
        ui.set_set_discord_rp(true);
        ui.set_set_rp_place(true);
        ui.set_set_rp_time(true);
        ui.set_set_rp_brand(true);
        ui.set_set_discord_status(1);
        ui.set_set_discord_text("Подключено".into());
        ui.set_set_deeplink(true);
        ui.set_set_deeplink_ok(1);
        ui.set_set_sound(true);
        ui.set_set_sound_vol(70);
        ui.set_set_cleanup_busy(false);
        ui.set_set_cleanup_text("".into());
        ui.set_set_cleanup_err(false);
        ui.set_set_close_tray(true);
        ui.set_set_auto_clean(true);
        ui.set_set_cfg_msg("Конфигурация сохранена".into());
        ui.set_set_cfg_msg_err(false);
        ui.set_set_path("C:\\Users\\Linar\\AppData\\Local\\ByteStrap".into());
        ui.set_set_path_hint("Папка с версиями, модами и профилями".into());
        ui.set_set_path_token(0);

        // ---- About ----
        ui.set_about_version("0.9.40".into());
        ui.set_update_kind(3);
        ui.set_update_text("Доступна версия 0.9.41".into());
        ui.set_update_notes("- Исправлено окно логов\n- Ускорен запуск клиента\n- Новый счётчик сессии в профиле".into());
        ui.set_update_pct(0.0);

        ui
    }

    #[test]
    fn take_page_shots() {
        if std::env::var("BYTESTRAP_SHOTS").is_err() {
            return;
        }

        let ui = shots_setup();
        ui.show().expect("show");

        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/shots");
        std::fs::create_dir_all(&dir).expect("mkdir shots");

        let shots: Vec<(i32, String)> = vec![
            (0, "0-home".into()),
            (1, "1-games".into()),
            (2, "2-flags".into()),
            (3, "3-versions".into()),
            (4, "4-mods".into()),
            (5, "5-logs".into()),
            (6, "6-settings".into()),
            (7, "7-about".into()),
            // Прокрутки: таблица флагов, файлы модов, низ настроек.
            (2, "2-flags-table".into()),
            (4, "4-mods-files".into()),
            (6, "6-settings-b".into()),
            // Модальные поверхности.
            (4, "4-mods-saveas".into()),
            (2, "2-flags-import".into()),
            (0, "0-home-confirm".into()),
        ];

        for (page, name) in shots {
            ui.set_page(page);
            // Сброс скролла предыдущего снимка (delta_y > 0 — к началу).
            scroll(&ui, 1_000.0, 4);
            // Сброс модальных поверхностей предыдущего снимка.
            ui.set_dlg_open(false);
            ui.set_import_open(false);
            ui.set_confirm_body("".into());
            match name.as_str() {
                "2-flags-table" | "4-mods-files" | "6-settings-b" => {
                    scroll(&ui, -120.0, 6);
                }
                "4-mods-saveas" => {
                    ui.set_dlg_open(true);
                    ui.set_dlg_desc("Сохранить профиль модов".into());
                    ui.set_dlg_hint("Название профиля".into());
                    ui.set_dlg_input("PvP-32-нмр".into());
                    ui.set_dlg_ok_label("Сохранить".into());
                }
                "2-flags-import" => {
                    ui.set_import_open(true);
                    ui.set_import_body(
                        "Конфликт импорта: 3 файла уже существуют в папке Mods.\n\n\
                         blaze/esp.lua — перезаписать?\n\
                         fps-boost/flags.txt — перезаписать?\n\
                         shaders/enable.post — пропустить".into(),
                    );
                }
                "0-home-confirm" => {
                    ui.set_confirm_body("Удалить версию version-0b47e2d98c15af63?".into());
                }
                _ => {}
            }

            let buf = ui.window().take_snapshot().unwrap_or_else(|e| {
                panic!("snapshot {name}: {e}");
            });
            let path = dir.join(format!("{name}.png"));
            image::save_buffer(&path, buf.as_bytes(), buf.width(), buf.height(), image::ColorType::Rgba8)
                .unwrap_or_else(|e| panic!("save {name}: {e}"));
            println!("saved {name}");
        }
    }

    /// Дебаг PopupWindow: открыть дропдаун пресета FPS на Flags и снять.
    #[test]
    fn take_popup_shot() {
        if std::env::var("BYTESTRAP_POPUP").is_err() {
            return;
        }
        let ui = shots_setup();
        ui.set_page(2);
        ui.show().expect("show");
        // FPS-пресет (строка 0): центр комбобокса в окне.
        click(&ui, 1007.0, 324.0);
        // Анимация появления попапа (mock_time) — промотать вперёд.
        i_slint_backend_testing::mock_elapsed_time(std::time::Duration::from_millis(400));
        let buf = ui.window().take_snapshot().expect("snapshot");
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/shots");
        std::fs::create_dir_all(&dir).expect("mkdir");
        image::save_buffer(
            dir.join("popup.png"),
            buf.as_bytes(),
            buf.width(),
            buf.height(),
            image::ColorType::Rgba8,
        )
        .expect("save");
        println!("saved popup.png");
    }

    fn click(ui: &AppWindow, x: f32, y: f32) {
        use slint::platform::{PointerEventButton, WindowEvent};
        ui.window().dispatch_event(WindowEvent::PointerPressed {
            position: slint::LogicalPosition::new(x, y),
            button: PointerEventButton::Left,
        });
        ui.window().dispatch_event(WindowEvent::PointerReleased {
            position: slint::LogicalPosition::new(x, y),
            button: PointerEventButton::Left,
        });
    }

    /// Прокрутка Flickable под указателем (700, 500) в окне.
    fn scroll(ui: &AppWindow, dy: f32, times: usize) {
        for _ in 0..times {
            ui.window().dispatch_event(slint::platform::WindowEvent::PointerScrolled {
                position: slint::LogicalPosition::new(700.0, 500.0),
                delta_x: 0.0,
                delta_y: dy,
            });
        }
    }
}
