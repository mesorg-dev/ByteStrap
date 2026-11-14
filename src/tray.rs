//! Трей-иконка (Windows): меню «Открыть / Запустить Roblox / Цветокоррекция /
//! профили флагов и модов / любимые игры / Выход», левый клик возвращает окно,
//! тултип показывает время в игре.
//!
//! tray-icon требует свой win32-цикл на потоке создания — иконка строится
//! на главном потоке (winit сам прокачивает её сообщения), а события
//! уходит в общий канал контроллера как AsyncEvent::Tray.

#![allow(dead_code)]

#[cfg(windows)]
pub use imp::{set_tooltip, spawn, sync, Action, TrayData};

#[cfg(not(windows))]
pub use stub::*;

/// Данные для меню трея (строится заново при каждом изменении).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TrayData {
    /// Цветокоррекция (оверлей «Картинка клиента») включена.
    pub overlay_on: bool,
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
            overlay_on: false,
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
        /// Переключить цветокоррекцию.
        OverlayToggle,
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
    const ID_OVERLAY: &str = "bs-overlay";
    const PFX_FLAG: &str = "bs-fp:";
    const PFX_MOD: &str = "bs-mp:";
    const PFX_GAME: &str = "bs-gm:";

    type Acc = Option<tray_icon::menu::accelerator::Accelerator>;

    fn item(id: &str, text: &str) -> MenuItem {
        MenuItem::with_id(id, text, true, None::<Acc>)
    }

    fn check(id: &str, text: &str, on: bool) -> CheckMenuItem {
        CheckMenuItem::with_id(id, text, true, on, None::<Acc>)
    }

    /// Собрать меню по данным.
    fn build_menu(data: &TrayData) -> Menu {
        let menu = Menu::new();
        let open = item(ID_SHOW, "Открыть ByteStrap");
        let play = item(ID_PLAY, "Запустить Roblox");

        // Цветокоррекция: быстрый тумблер оверлея.
        let overlay = check(ID_OVERLAY, "Цветокоррекция", data.overlay_on);

        // Пресеты флагов.
        let mut flag_items: Vec<CheckMenuItem> = data
            .flag_profiles
            .iter()
            .map(|n| {
                let active = data.active_flag.eq_ignore_ascii_case(n);
                check(&format!("{PFX_FLAG}{n}"), n, active)
            })
            .collect();
        let flag_menu = Submenu::new("Пресеты флагов", true);
        if flag_items.is_empty() {
            let _ = flag_menu.append(&item("bs-noop", "Нет профилей"));
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
        let mod_menu = Submenu::new("Профили модов", true);
        if mod_items.is_empty() {
            let _ = mod_menu.append(&item("bs-noop-m", "Нет профилей"));
        } else {
            for it in &mod_items {
                let _ = mod_menu.append(it);
            }
        }

        // Любимые игры: топ-3 по времени.
        let game_menu = Submenu::new("Игры", true);
        if data.games.is_empty() {
            let _ = game_menu.append(&item("bs-noop-g", "Нет данных"));
        } else {
            for (id, name) in &data.games {
                let _ = game_menu.append(&item(&format!("{PFX_GAME}{id}"), name));
            }
        }

        let sep1 = PredefinedMenuItem::separator();
        let sep2 = PredefinedMenuItem::separator();
        let quit = item(ID_QUIT, "Выход");
        let _ = menu.append_items(&[
            &open,
            &play,
            &sep1,
            &overlay,
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

    /// Создать иконку с меню. Один раз на процесс; ошибки — тихо.
    pub fn spawn(tx: Sender<super::super::app_slint::AsyncEvent>) {
        TX.with(|slot| *slot.borrow_mut() = Some(tx.clone()));

        let data = DATA.with(|d| d.borrow().clone());
        let menu = build_menu(&data);
        let icon = Icon::from_rgba(render_star(32), 32, 32).ok();
        let builder = TrayIconBuilder::new()
            .with_tooltip("ByteStrap")
            .with_menu(Box::new(menu));
        let tray = match icon {
            Some(icon) => builder.with_icon(icon).build(),
            None => builder.build(),
        };
        let tray = match tray {
            Ok(t) => t,
            Err(e) => {
                crate::core::logger::info(format!("Трей недоступен: {e}"));
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
                    } else if id == ID_OVERLAY {
                        Some(Action::OverlayToggle)
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
                    if let Ok(TrayIconEvent::Click { button, button_state, .. }) =
                        TrayIconEvent::receiver().try_recv()
                    {
                        // Левый клик «вниз» — вернуть окно; правый открывает меню сам.
                        if button == tray_icon::MouseButton::Left
                            && button_state == tray_icon::MouseButtonState::Up
                        {
                            act = Some(Action::Show);
                        }
                    }
                }
                if let Some(a) = act {
                    let map = crate::app_slint::AsyncEvent::Tray(a);
                    if tx.send(map).is_err() {
                        break;
                    }
                }
                std::thread::sleep(std::time::Duration::from_millis(120));
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

    /// Белая четырёхлучевая звезда (как логотип) на прозрачном фоне.
    fn render_star(size: u32) -> Vec<u8> {
        let n = size as i32;
        let c = (n - 1) as f32 / 2.0;
        let mut buf = Vec::with_capacity((size * size * 4) as usize);
        for y in 0..n {
            for x in 0..n {
                let dx = (x as f32 - c).abs();
                let dy = (y as f32 - c).abs();
                let s = 1.0 - (dx + dy) / (c + 0.5);
                let (r, g, b, a) = if s > 0.25 {
                    (255u8, 255u8, 255u8, 255u8)
                } else {
                    (0, 0, 0, 0)
                };
                buf.extend_from_slice(&[r, g, b, a]);
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
        FlagProfile(String),
        ModProfile(String),
        Game(u64),
    }

    pub fn spawn(_tx: Sender<crate::app_slint::AsyncEvent>) {}

    pub fn sync(_data: TrayData) {}

    pub fn set_tooltip(_text: &str) {}
}
