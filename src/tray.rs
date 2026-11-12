//! Трей-иконка (Windows): меню «Открыть / Запустить Roblox / Выход»,
//! левый клик возвращает окно, тултип показывает время в игре.
//!
//! tray-icon требует свой win32-цикл на потоке создания — иконка строится
//! на главном потоке (winit сам прокачивает её сообщения), а события
//! уходит в общий канал контроллера как AsyncEvent::Tray.

#![allow(dead_code)]

#[cfg(windows)]
pub use imp::{set_tooltip, spawn, Action};

#[cfg(not(windows))]
pub use stub::*;

#[cfg(windows)]
mod imp {
    use std::sync::mpsc::Sender;

    use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
    use tray_icon::{Icon, TrayIcon, TrayIconBuilder, TrayIconEvent};

    thread_local! {
        /// TrayIcon не Send/Sync (Rc внутри) — но все вызовы только на главном потоке.
        static TRAY: std::cell::RefCell<Option<TrayIcon>> = const { std::cell::RefCell::new(None) };
    }

    /// Что пользователь выбрал в трее.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum Action {
        Show,
        Play,
        Quit,
    }

    const ID_SHOW: &str = "bs-show";
    const ID_PLAY: &str = "bs-play";
    const ID_QUIT: &str = "bs-quit";

    /// Создать иконку с меню. Один раз на процесс; ошибки — тихо.
    pub fn spawn(tx: Sender<super::super::app_slint::AsyncEvent>) {
        let menu = Menu::new();
        let open = MenuItem::with_id(ID_SHOW, "Открыть ByteStrap", true, None::<tray_icon::menu::accelerator::Accelerator>);
        let play = MenuItem::with_id(ID_PLAY, "Запустить Roblox", true, None::<tray_icon::menu::accelerator::Accelerator>);
        let sep1 = PredefinedMenuItem::separator();
        let quit = MenuItem::with_id(ID_QUIT, "Выход", true, None::<tray_icon::menu::accelerator::Accelerator>);
        let _ = menu.append_items(&[&open, &play, &sep1, &quit]);

        let icon = Icon::from_rgba(render_star(32), 32, 32).ok();
        let builder = TrayIconBuilder::with_id("bytestrap")
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
                    act = match id {
                        ID_SHOW => Some(Action::Show),
                        ID_PLAY => Some(Action::Play),
                        ID_QUIT => Some(Action::Quit),
                        _ => None,
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
                    let map = match a {
                        Action::Show => crate::app_slint::AsyncEvent::Tray(Action::Show),
                        Action::Play => crate::app_slint::AsyncEvent::Tray(Action::Play),
                        Action::Quit => crate::app_slint::AsyncEvent::Tray(Action::Quit),
                    };
                    if tx.send(map).is_err() {
                        break;
                    }
                }
                std::thread::sleep(std::time::Duration::from_millis(120));
            })
            .ok();
    }

    /// Обновить тултип (главный поток): время сессии или просто имя.
    pub fn set_tooltip(text: &str) {
        TRAY.with(|slot| {
            if let Some(t) = slot.borrow_mut().as_ref() {
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

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum Action {
        Show,
        Play,
        Quit,
    }

    pub fn spawn(_tx: Sender<crate::app_slint::AsyncEvent>) {}

    pub fn set_tooltip(_text: &str) {}
}
