//! Трей-иконка (Windows): меню «Открыть / Запустить Roblox / Цветокоррекция /
//! профили флагов и модов / любимые игры / Выход», левый клик возвращает окно,
//! тултип показывает время в игре.
//!
//! tray-icon требует свой win32-цикл на потоке создания — иконка строится
//! на главном потоке (winit сам прокачивает её сообщения), а события
//! уходит в общий канал контроллера как AsyncEvent::Tray.

#![allow(dead_code)]

#[cfg(windows)]
pub use imp::{set_tooltip, spawn, sync, Action};

#[cfg(not(windows))]
pub use stub::*;

/// Данные для меню трея (строится заново при каждом изменении).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TrayData {
    pub lang: crate::i18n::Lang,
    /// Цветокоррекция (оверлей «Картинка клиента») включена.
    pub overlay_on: bool,
    /// Яркость ЦК (-50..50).
    pub overlay_bright: i32,
    /// Насыщенность ЦК (0..200).
    pub overlay_sat: i32,
    /// Контраст ЦК (50..150).
    pub overlay_contrast: i32,
    /// Сохранённые профили флагов.
    pub flag_profiles: Vec<String>,
    /// Активный профиль флагов (пусто — нет).
    pub active_flag: String,
    /// Сохранённые профили модов.
    pub mod_profiles: Vec<String>,
    /// Активный профиль модов (пусто — нет).
    pub active_mod: String,
    /// Топ-3 игры по наигранному времени: (placeId, название).
    pub games: Vec<(u64, String)>,
}

#[cfg(windows)]
mod imp {
    use std::sync::mpsc::Sender;

    use tray_icon::menu::{
        CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu,
    };
    use tray_icon::{Icon, TrayIcon, TrayIconBuilder, TrayIconEvent};

    use super::TrayData;

    thread_local! {
        /// TrayIcon не Send/Sync (Rc внутри) — но все вызовы только на главном потоке.
        static TRAY: std::cell::RefCell<Option<TrayIcon>> = const { std::cell::RefCell::new(None) };
        /// Текущее состояние меню (для пересборки).
        static DATA: std::cell::RefCell<TrayData> = const { std::cell::RefCell::new(TrayData {
            lang: crate::i18n::Lang::Ru,
            overlay_on: false,
            overlay_bright: 0,
            overlay_sat: 100,
            overlay_contrast: 100,
            flag_profiles: Vec::new(),
            active_flag: String::new(),
            mod_profiles: Vec::new(),
            active_mod: String::new(),
            games: Vec::new(),
        }) };
        /// Канал контроллера (для пересборки меню из sync без повторной передачи).
        static TX: std::cell::RefCell<Option<Sender<super::super::app_slint::AsyncEvent>>> =
            const { std::cell::RefCell::new(None) };
    }

    /// Что пользователь выбрал в трее.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum Action {
        Show,
        Play,
        Quit,
        /// Переключить цветокоррекцию (вкл/выкл).
        OverlayToggle,
        /// Установить пресет ЦК.
        OverlayPreset(String),
        /// Быстрая подстройка яркости/насыщенности.
        OverlayAdjust { bright: i32, sat: i32 },
        /// Сбросить параметры ЦК в дефолт.
        OverlayReset,
        /// Открыть настройки ЦК в главном окне.
        OverlaySettings,
        /// Применить профиль флагов.
        FlagProfile(String),
        /// Загрузить профиль модов.
        ModProfile(String),
        /// Запустить игру напрямую по placeId.
        Game(u64),
    }

    const ID_SHOW: &str = "bs-show";
    const ID_PLAY: &str = "bs-play";
    const ID_QUIT: &str = "bs-quit";
    const ID_OVERLAY_TOGGLE: &str = "bs-ov-toggle";
    const ID_OVERLAY_RESET: &str = "bs-ov-reset";
    const ID_OVERLAY_SETTINGS: &str = "bs-ov-settings";
    const ID_OVERLAY_BRIGHT_UP: &str = "bs-ov-b-up";
    const ID_OVERLAY_BRIGHT_DN: &str = "bs-ov-b-dn";
    const ID_OVERLAY_SAT_UP: &str = "bs-ov-s-up";
    const ID_OVERLAY_SAT_DN: &str = "bs-ov-s-dn";
    const PFX_OVERLAY_PRESET: &str = "bs-ov-p:";
    const PFX_FLAG: &str = "bs-fp:";
    const PFX_MOD: &str = "bs-mp:";
    const PFX_GAME: &str = "bs-gm:";

    fn item(id: &str, text: &str) -> MenuItem {
        MenuItem::with_id(id, text, true, None)
    }

    fn check(id: &str, text: &str, on: bool) -> CheckMenuItem {
        CheckMenuItem::with_id(id, text, true, on, None)
    }

    /// Собрать меню по данным.
    fn build_menu(data: &TrayData) -> Menu {
        use crate::i18n::tr;
        let lang = data.lang;

        let menu = Menu::new();
        let open = item(ID_SHOW, tr(lang, "Tray_Open"));
        let play = item(ID_PLAY, tr(lang, "Tray_Play"));

        // Цветокоррекция: подменю со всеми настройками и пресетами.
        let overlay_menu = Submenu::new(tr(lang, "Tray_Overlay"), true);
        let ov_toggle = check(ID_OVERLAY_TOGGLE, tr(lang, "Tray_OverlayToggle"), data.overlay_on);
        let _ = overlay_menu.append(&ov_toggle);
        let _ = overlay_menu.append(&PredefinedMenuItem::separator());

        let is_def = data.overlay_bright == 0 && data.overlay_sat == 100 && data.overlay_contrast == 100;
        let is_vib = data.overlay_bright == 10 && data.overlay_sat == 135 && data.overlay_contrast == 110;
        let is_cin = data.overlay_bright == -5 && data.overlay_sat == 115 && data.overlay_contrast == 125;
        let is_nit = data.overlay_bright == -15 && data.overlay_sat == 85 && data.overlay_contrast == 95;
        let is_pvp = data.overlay_bright == 20 && data.overlay_sat == 140 && data.overlay_contrast == 115;

        let preset_menu = Submenu::new(tr(lang, "Tray_Presets"), true);
        let _ = preset_menu.append(&check(&format!("{PFX_OVERLAY_PRESET}default"), tr(lang, "Tray_PresetDefault"), is_def));
        let _ = preset_menu.append(&check(&format!("{PFX_OVERLAY_PRESET}vibrant"), tr(lang, "Tray_PresetVibrant"), is_vib));
        let _ = preset_menu.append(&check(&format!("{PFX_OVERLAY_PRESET}cinema"), tr(lang, "Tray_PresetCinema"), is_cin));
        let _ = preset_menu.append(&check(&format!("{PFX_OVERLAY_PRESET}night"), tr(lang, "Tray_PresetNight"), is_nit));
        let _ = preset_menu.append(&check(&format!("{PFX_OVERLAY_PRESET}pvp"), tr(lang, "Tray_PresetPvp"), is_pvp));
        let _ = overlay_menu.append(&preset_menu);

        let adj_menu = Submenu::new(tr(lang, "Tray_QuickAdjust"), true);
        let _ = adj_menu.append(&item(ID_OVERLAY_BRIGHT_UP, tr(lang, "Tray_BrightUp")));
        let _ = adj_menu.append(&item(ID_OVERLAY_BRIGHT_DN, tr(lang, "Tray_BrightDn")));
        let _ = adj_menu.append(&item(ID_OVERLAY_SAT_UP, tr(lang, "Tray_SatUp")));
        let _ = adj_menu.append(&item(ID_OVERLAY_SAT_DN, tr(lang, "Tray_SatDn")));
        let _ = adj_menu.append(&item(ID_OVERLAY_RESET, tr(lang, "Tray_Reset")));
        let _ = overlay_menu.append(&adj_menu);

        let _ = overlay_menu.append(&PredefinedMenuItem::separator());
        let _ = overlay_menu.append(&item(ID_OVERLAY_SETTINGS, tr(lang, "Tray_OverlaySettings")));

        // Пресеты флагов.
        let mut flag_items: Vec<CheckMenuItem> = data
            .flag_profiles
            .iter()
            .map(|n| {
                let active = data.active_flag.eq_ignore_ascii_case(n);
                check(&format!("{PFX_FLAG}{n}"), n, active)
            })
            .collect();
        let flag_menu = Submenu::new(tr(lang, "Tray_FlagPresets"), true);
        if flag_items.is_empty() {
            let _ = flag_menu.append(&item("bs-noop", tr(lang, "Tray_NoProfiles")));
        } else {
            for it in &flag_items {
                let _ = flag_menu.append(it);
            }
        }

        // Профили модов.
        let mut mod_items: Vec<CheckMenuItem> = data
            .mod_profiles
            .iter()
            .map(|n| {
                let active = data.active_mod.eq_ignore_ascii_case(n);
                check(&format!("{PFX_MOD}{n}"), n, active)
            })
            .collect();
        let mod_menu = Submenu::new(tr(lang, "Tray_ModProfiles"), true);
        if mod_items.is_empty() {
            let _ = mod_menu.append(&item("bs-noop-m", tr(lang, "Tray_NoProfiles")));
        } else {
            for it in &mod_items {
                let _ = mod_menu.append(it);
            }
        }

        // Любимые игры: топ-3 по времени.
        let game_menu = Submenu::new(tr(lang, "Tray_Games"), true);
        if data.games.is_empty() {
            let _ = game_menu.append(&item("bs-noop-g", tr(lang, "Tray_NoData")));
        } else {
            for (id, name) in &data.games {
                let _ = game_menu.append(&item(&format!("{PFX_GAME}{id}"), name));
            }
        }

        let sep1 = PredefinedMenuItem::separator();
        let sep2 = PredefinedMenuItem::separator();
        let quit = item(ID_QUIT, tr(lang, "Tray_Quit"));
        let _ = menu.append_items(&[
            &open,
            &play,
            &sep1,
            &overlay_menu,
            &flag_menu,
            &mod_menu,
            &game_menu,
            &sep2,
            &quit,
        ]);
        // Клоны пунктов должны жить, пока живёт меню: держим их в Menu
        // через append (muda хранит владение пунктами в меню-объектах).
        flag_items.clear();
        mod_items.clear();
        menu
    }

    fn get_tray_icon() -> Option<Icon> {
        // 1. Пробуем загрузить встроенную иконку приложения из ресурсов PE-бинарника (ord 1)
        if let Ok(icon) = Icon::from_resource(1, Some((32, 32))) {
            return Some(icon);
        }
        if let Ok(icon) = Icon::from_resource(1, None) {
            return Some(icon);
        }
        // 2. Fallback: рендерим контрастный значок-бейдж с синим фоном и белой звездой
        Icon::from_rgba(render_icon(32), 32, 32).ok()
    }

    /// Создать иконку с меню. Один раз на процесс; ошибки — тихо.
    pub fn spawn(tx: Sender<super::super::app_slint::AsyncEvent>) {
        TX.with(|slot| *slot.borrow_mut() = Some(tx.clone()));

        let data = DATA.with(|d| d.borrow().clone());
        let menu = build_menu(&data);
        let icon = get_tray_icon();
        let builder = TrayIconBuilder::new()
            .with_tooltip("ByteStrap")
            .with_menu(Box::new(menu));
        let tray = match icon {
            Some(icon) => builder.with_icon(icon).build(),
            None => builder.build(),
        };
        let tray = match tray {
            Ok(t) => {
                crate::core::logger::info("Трей-иконка успешно создана");
                t
            }
            Err(e) => {
                crate::core::logger::warn(format!("Трей недоступен: {e}"));
                return;
            }
        };
        TRAY.with(|slot| *slot.borrow_mut() = Some(tray));

        // События меню и кликов → общий канал контроллера.
        std::thread::Builder::new()
            .name("tray-events".into())
            .spawn(move || loop {
                let mut act: Option<Action> = None;
                if let Ok(ev) = MenuEvent::receiver().try_recv() {
                    let id = ev.id().0.as_str();
                    act = if id == ID_SHOW {
                        Some(Action::Show)
                    } else if id == ID_PLAY {
                        Some(Action::Play)
                    } else if id == ID_QUIT {
                        Some(Action::Quit)
                    } else if id == ID_OVERLAY_TOGGLE {
                        Some(Action::OverlayToggle)
                    } else if id == ID_OVERLAY_RESET {
                        Some(Action::OverlayReset)
                    } else if id == ID_OVERLAY_SETTINGS {
                        Some(Action::OverlaySettings)
                    } else if id == ID_OVERLAY_BRIGHT_UP {
                        Some(Action::OverlayAdjust { bright: 10, sat: 0 })
                    } else if id == ID_OVERLAY_BRIGHT_DN {
                        Some(Action::OverlayAdjust { bright: -10, sat: 0 })
                    } else if id == ID_OVERLAY_SAT_UP {
                        Some(Action::OverlayAdjust { bright: 0, sat: 15 })
                    } else if id == ID_OVERLAY_SAT_DN {
                        Some(Action::OverlayAdjust { bright: 0, sat: -15 })
                    } else if let Some(p) = id.strip_prefix(PFX_OVERLAY_PRESET) {
                        Some(Action::OverlayPreset(p.to_string()))
                    } else if let Some(name) = id.strip_prefix(PFX_FLAG) {
                        Some(Action::FlagProfile(name.to_string()))
                    } else if let Some(name) = id.strip_prefix(PFX_MOD) {
                        Some(Action::ModProfile(name.to_string()))
                    } else if let Some(id) = id.strip_prefix(PFX_GAME) {
                        id.parse().ok().map(Action::Game)
                    } else {
                        None
                    };
                }
                if act.is_none() {
                    match TrayIconEvent::receiver().try_recv() {
                        Ok(TrayIconEvent::Click { button: tray_icon::MouseButton::Left, .. })
                        | Ok(TrayIconEvent::DoubleClick { button: tray_icon::MouseButton::Left, .. }) => {
                            act = Some(Action::Show);
                        }
                        _ => {}
                    }
                }
                if let Some(a) = act {
                    let map = crate::app_slint::AsyncEvent::Tray(a);
                    if tx.send(map).is_err() {
                        break;
                    }
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
            })
            .ok();
    }

    /// Пересобрать меню (главный поток). Вызывать при смене профилей,
    /// оверлея или топа игр.
    pub fn sync(data: TrayData) {
        let changed = DATA.with(|d| {
            let mut d = d.borrow_mut();
            if *d == data {
                false
            } else {
                *d = data.clone();
                true
            }
        });
        if !changed {
            return;
        }
        let menu = build_menu(&data);
        TRAY.with(|slot| {
            if let Some(t) = slot.borrow().as_ref() {
                let _ = t.set_menu(Some(Box::new(menu)));
            }
        });
    }

    /// Обновить тултип (главный поток): время сессии или просто имя.
    pub fn set_tooltip(text: &str) {
        TRAY.with(|slot| {
            if let Some(t) = slot.borrow().as_ref() {
                let _ = t.set_tooltip(Some(text));
            }
        });
    }

    /// Чёткая иконка для трея: тёмно-синий круг (#181926) с лазурной обводкой (#6ea8fe)
    /// и белой 4-лучевой звездой по центру. Контрастно видна на любой теме панели задач.
    fn render_icon(size: u32) -> Vec<u8> {
        let n = size as i32;
        let c = (n - 1) as f32 / 2.0;
        let radius = c - 1.0;
        let mut buf = Vec::with_capacity((size * size * 4) as usize);
        for y in 0..n {
            for x in 0..n {
                let dx = x as f32 - c;
                let dy = y as f32 - c;
                let dist = (dx * dx + dy * dy).sqrt();
                if dist <= radius {
                    let manhattan = dx.abs() + dy.abs();
                    let star_scale = 1.0 - manhattan / (c * 0.75);
                    if star_scale > 0.05 {
                        buf.extend_from_slice(&[255, 255, 255, 255]); // белая звезда
                    } else if dist > radius - 1.5 {
                        buf.extend_from_slice(&[110, 168, 254, 255]); // лазурный контур
                    } else {
                        buf.extend_from_slice(&[24, 25, 38, 255]); // тёмный фон
                    }
                } else {
                    buf.extend_from_slice(&[0, 0, 0, 0]); // прозрачно
                }
            }
        }
        buf
    }
}

#[cfg(not(windows))]
mod stub {
    //! Заглушка: трей доступен только на Windows.
    use std::sync::mpsc::Sender;

    use super::TrayData;

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum Action {
        Show,
        Play,
        Quit,
        OverlayToggle,
        OverlayPreset(String),
        OverlayAdjust { bright: i32, sat: i32 },
        OverlayReset,
        OverlaySettings,
        FlagProfile(String),
        ModProfile(String),
        Game(u64),
    }

    pub fn spawn(_tx: Sender<crate::app_slint::AsyncEvent>) {}

    pub fn sync(_data: TrayData) {}

    pub fn set_tooltip(_text: &str) {}
}
