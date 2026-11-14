use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// "auto" | "ru" | "en"
    pub language: String,
    /// Custom base dir; empty = %LOCALAPPDATA%\ByteStrap
    pub bytestrap_path: String,
    pub installed_version: String,
    pub previous_version: String,
    pub active_flag_profile: String,
    /// Активный профиль модов (подсветка чипа на странице Mods).
    pub active_mod_profile: String,
    pub download_threads: u32,
    /// Roblox deployment channel: "LIVE" | "BETA" | "ZNext" | ...
    pub channel: String,
    pub auto_update: bool,
    pub close_on_launch: bool,
    /// Показывать статус сессии в Discord (Rich Presence).
    pub discord_rp: bool,
    /// Discord RP: показывать название места.
    pub discord_rp_place: bool,
    /// Discord RP: показывать время сессии.
    pub discord_rp_time: bool,
    /// Discord RP: показывать надпись «ByteStrap».
    pub discord_rp_brand: bool,
    /// Открывать ссылки roblox:// (друзья, приватки, плейсы) через ByteStrap.
    pub deeplink: bool,
    /// Звуки интерфейса (клики, тумблеры, переходы).
    pub sound: bool,
    /// Громкость звуков интерфейса: 0..100.
    pub sound_volume: u32,
    /// Крестик скрывает окно в трей вместо выхода.
    pub close_to_tray: bool,
    /// Оверлей «Картинка клиента» поверх окна Roblox.
    pub overlay_on: bool,
    /// Яркость оверлея: -50..50.
    pub overlay_bright: i32,
    /// Насыщенность: 0..200.
    pub overlay_sat: i32,
    /// Контраст: 50..150.
    pub overlay_contrast: i32,
    /// Авто-очистка кэша раз в сутки.
    pub auto_clean: bool,
    /// Показывать вкладку «Логи» в сайдбаре.
    pub show_logs: bool,
    /// Индекс акцентного цвета (палитра accents в app_slint).
    pub theme_accent: u32,
    /// Индекс фоновой палитры (глубина/оттенок фона).
    pub theme_bg: u32,
    /// Путь к пользовательскому фоновому изображению (пусто — нет).
    pub bg_image: String,
    /// Unix-время последней авто-очистки.
    pub auto_clean_last: u64,
    /// Unix-время последней проверки обновлений лаунчера (гейт 4 часа —
    /// лимит GitHub API 60 запросов/час на IP).
    pub update_check_last: u64,
    pub window_max: bool,
    pub win_w: f32,
    pub win_h: f32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            language: "auto".into(),
            bytestrap_path: String::new(),
            installed_version: String::new(),
            previous_version: String::new(),
            active_flag_profile: String::new(),
            active_mod_profile: String::new(),
            download_threads: 4,
            channel: "LIVE".into(),
            auto_update: true,
            close_on_launch: false,
            discord_rp: true,
            discord_rp_place: true,
            discord_rp_time: true,
            discord_rp_brand: true,
            deeplink: true,
            sound: true,
            sound_volume: 70,
            close_to_tray: true,
            overlay_on: false,
            overlay_bright: 0,
            overlay_sat: 100,
            overlay_contrast: 100,
            auto_clean: false,
            show_logs: true,
            theme_accent: 0,
            theme_bg: 0,
            bg_image: String::new(),
            auto_clean_last: 0,
            update_check_last: 0,
            window_max: false,
            win_w: 1120.0,
            win_h: 700.0,
        }
    }
}

impl Config {
    pub fn base_dir() -> PathBuf {
        dirs::data_local_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("ByteStrap")
    }

    pub fn file_path() -> PathBuf {
        Self::base_dir().join("config.json")
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

    pub fn base(&self) -> PathBuf {
        if self.bytestrap_path.trim().is_empty() {
            Self::base_dir()
        } else {
            PathBuf::from(&self.bytestrap_path)
        }
    }
}
