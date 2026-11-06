//! Локальная конфигурация ByteStrap Shell.
//!
//! Хранится в `%LOCALAPPDATA%\ByteStrap\Shell\config.json` — базовый каталог
//! общий с лаунчером (`playtime.json`, `Versions/`), но настройки свои.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::core::config::Config;

/// Акцентные цвета из мокапа (hex).
pub const ACCENTS: [&str; 6] = [
    "#78DC82", "#6EA8FE", "#B48CFF", "#FFC857", "#FF8087", "#EDEDED",
];

/// Локальные настройки приватности (дерево пользователя; API — Фаза B).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PrivacyCfg {
    /// 0 = Minimal, 1 = Mild, 2 = Moderate.
    pub maturity: usize,
    /// 0 = All, 1 = Friends only, 2 = None.
    pub preview: usize,
    pub chat: bool,
    pub camera: bool,
    pub studio: bool,
    /// 0..4 — Everyone / Friends & followers / Friends / Trusted / No one.
    pub online: usize,
    /// 0..4 — аналогично online.
    pub show_game: usize,
    pub activity: bool,
    /// 0..2 — Friends / Everyone / No one.
    pub servers: usize,
    pub contacts: bool,
    /// 0..2 — All / Friends / No one.
    pub trading: usize,
}

impl Default for PrivacyCfg {
    fn default() -> Self {
        Self {
            maturity: 2,
            preview: 0,
            chat: true,
            camera: false,
            studio: false,
            online: 0,
            show_game: 1,
            activity: true,
            servers: 0,
            contacts: false,
            trading: 0,
        }
    }
}

/// Строковые значения приватности (для выпадающих списков).
pub mod pv {
    pub const VISIBILITY: [&str; 5] = [
        "Everyone",
        "Friends & followers",
        "Friends",
        "Trusted friends",
        "No one",
    ];
    pub const MATURITY: [&str; 3] = ["Minimal", "Mild", "Moderate"];
    pub const PREVIEW: [&str; 3] = ["All games", "Friends only", "None"];
    pub const SERVERS: [&str; 3] = ["Friends", "Everyone", "No one"];
    pub const TRADING: [&str; 3] = ["All", "Friends", "No one"];
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ShellConfig {
    /// Путь к фону (jpg/png/gif/webp); пусто — дефолтный фон.
    pub bg_path: String,
    /// Размытие фона, px (0..30).
    pub bg_blur: i32,
    /// Затемнение фона, % (0..90).
    pub bg_dim: i32,
    /// Прозрачность карточек, % (40..100).
    pub card_opacity: i32,
    /// Акцентный цвет, hex вида `#78DC82`.
    pub accent: String,
    /// Язык шелла: "" = следовать конфигу лаунчера.
    pub lang: String,
    pub win_w: f32,
    pub win_h: f32,
    pub win_max: bool,
    /// Страница/значения Settings (локальные, Фаза B подключит к клиенту).
    pub privacy: PrivacyCfg,
    /// Пол в Account: 0 = Male, 1 = Female.
    pub acc_gender: usize,
    /// Запускать шелл при клике roblox:// (реестр — Фаза B).
    pub deeplink_shell: bool,
    /// Сворачивать в трей после запуска игры (трей — Фаза B).
    pub minimize_tray: bool,
    /// Закрывать шелл при выходе из игры.
    pub close_on_exit: bool,
    /// Идентификаторы заблокированных плейсов (пусто в Фазе A).
    pub blocked_places: Vec<String>,
    /// Разрешённые плейсы (пусто = все разрешены).
    pub allowed_places: Vec<String>,
}

impl Default for ShellConfig {
    fn default() -> Self {
        Self {
            bg_path: String::new(),
            bg_blur: 0,
            bg_dim: 55,
            card_opacity: 100,
            accent: "#78DC82".into(),
            lang: String::new(),
            win_w: 1280.0,
            win_h: 760.0,
            win_max: false,
            privacy: PrivacyCfg::default(),
            acc_gender: 1,
            deeplink_shell: true,
            minimize_tray: false,
            close_on_exit: false,
            blocked_places: Vec::new(),
            allowed_places: Vec::new(),
        }
    }
}

impl ShellConfig {
    /// `%LOCALAPPDATA%\ByteStrap\Shell\config.json` (база — у лаунчера).
    pub fn file_path() -> PathBuf {
        Config::base_dir().join("Shell").join("config.json")
    }

    pub fn load() -> Self {
        match std::fs::read_to_string(Self::file_path()) {
            Ok(s) => serde_json::from_str(&s).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self) {
        let path = Self::file_path();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(s) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(path, s);
        }
    }

    /// `#78DC82` → Color32; некорректная строка — дефолтный акцент.
    pub fn accent_color(&self) -> eframe::egui::Color32 {
        parse_hex(&self.accent).unwrap_or(theme_good())
    }
}

fn theme_good() -> eframe::egui::Color32 {
    crate::theme::GOOD
}

/// `#RRGGBB` → Color32.
pub fn parse_hex(s: &str) -> Option<eframe::egui::Color32> {
    let h = s.trim().trim_start_matches('#');
    if h.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&h[0..2], 16).ok()?;
    let g = u8::from_str_radix(&h[2..4], 16).ok()?;
    let b = u8::from_str_radix(&h[4..6], 16).ok()?;
    Some(eframe::egui::Color32::from_rgb(r, g, b))
}

/// Кэш обложек: `%LOCALAPPDATA%\ByteStrap\Shell\thumbs\{id}.png`.
pub fn thumb_path(base: &std::path::Path, id: &str) -> PathBuf {
    base.join("Shell").join("thumbs").join(format!("{id}.png"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_parse() {
        assert_eq!(
            parse_hex("#78DC82"),
            Some(eframe::egui::Color32::from_rgb(0x78, 0xDC, 0x82))
        );
        assert_eq!(parse_hex("6EA8FE"), Some(eframe::egui::Color32::from_rgb(0x6E, 0xA8, 0xFE)));
        assert_eq!(parse_hex("zzz"), None);
        assert_eq!(parse_hex("#12345"), None);
    }

    #[test]
    fn default_accent_is_green() {
        let cfg = ShellConfig::default();
        assert_eq!(cfg.accent, "#78DC82");
        assert_eq!(cfg.accent_color(), crate::theme::GOOD);
    }

    #[test]
    fn privacy_defaults() {
        let p = PrivacyCfg::default();
        assert_eq!(p.maturity, 2);
        assert!(p.chat);
        assert!(!p.camera);
        assert_eq!(p.show_game, 1);
    }
}
