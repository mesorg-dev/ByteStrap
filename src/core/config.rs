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
    /// Discord RP: кастомный статус (строка деталей/состояния).
    pub discord_custom_status: String,
    /// Discord RP: режим инкогнито (скрывать название плейса).
    pub discord_incognito: bool,
    /// Discord RP: показывать регион/сервер.
    pub discord_show_server: bool,
    /// Показывать всплывающее уведомление с регионом и пингом при подключении к серверу.
    #[serde(default = "default_true")]
    pub notify_server: bool,
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
    /// Anti-AFK: имитировать пробел в окне Roblox во время игры.
    pub anti_afk: bool,
    /// Показывать splash-экран при запуске лаунчера.
    pub splash_on: bool,
    /// Путь к пользовательскому изображению splash-экрана (пусто — логотип).
    pub splash_image: String,
    /// Unix-время последней авто-очистки.
    pub auto_clean_last: u64,
    /// Unix-время последней проверки обновлений лаунчера (гейт 4 часа —
    /// лимит GitHub API 60 запросов/час на IP).
    pub update_check_last: u64,
    pub window_max: bool,
    pub win_w: f32,
    pub win_h: f32,
    /// Центрировать окно лаунчера при запуске.
    #[serde(default)]
    pub center_window: bool,
    /// Центрировать окно Roblox при запуске.
    #[serde(default)]
    pub center_game: bool,
    /// Разрешение окна Roblox: 0 — Авто/Системное, 1 — 1920×1080, 2 — 1600×900, 3 — 1440×1080 (4:3), 4 — 1280×720, 5 — 1024×768 (4:3), 6 — Кастом.
    #[serde(default)]
    pub game_res_mode: i32,
    #[serde(default = "default_res_w")]
    pub game_res_w: u32,
    #[serde(default = "default_res_h")]
    pub game_res_h: u32,
    /// Анимировать фон (GIF).
    #[serde(default = "default_true")]
    pub live_bg: bool,
    /// Режим звука смерти: 0 — По умолчанию, 1 — Oof, 2 — Minecraft, 3 — Без звука, 4 — Кастом.
    #[serde(default)]
    pub death_sound_mode: i32,
    #[serde(default)]
    pub death_sound_path: String,
    /// Режим курсора мыши: 0 — По умолчанию, 1 — 2006-2013, 2 — 2013-2020, 3 — Прицел/Точка, 4 — Минимал-точка, 5 — Тонкий крестик, 6 — Свой.
    #[serde(default)]
    pub cursor_mode: i32,
    #[serde(default)]
    pub cursor_path: String,
    /// Режим текстур/PvP: 0 — По умолчанию, 1 — PvP 1x1 (гладкие), 2 — FPS Boost (светлые 1x1), 3 — Своя папка.
    #[serde(default)]
    pub textures_mode: i32,
    #[serde(default)]
    pub textures_path: String,
    /// Режим скайбокса (небо): 0 — По умолчанию, 1 — Чёрный PvP, 2 — Ночной космос, 3 — Закат, 4 — Ясный день, 5 — Своя папка.
    #[serde(default)]
    pub skybox_mode: i32,
    #[serde(default)]
    pub skybox_path: String,
    /// Режим прицела Shift-Lock: 0 — По умолчанию, 1 — Точка (Dot), 2 — Крестик, 3 — Кольцо, 4 — Свой PNG.
    #[serde(default)]
    pub shiftlock_mode: i32,
    #[serde(default)]
    pub shiftlock_path: String,
    /// Режим шрифта: 0 — По умолчанию, 1 — JetBrains Mono, 2 — Inter, 3 — Unbounded, 4 — Кастом.
    #[serde(default)]
    pub font_mode: i32,
    #[serde(default)]
    pub font_path: String,
    /// Предпочитаемый регион: 0 — Авто, 1 — DE Германия, 2 — GB Лондон, 3 — US Восток, 4 — US Запад, 5 — SG Азия.
    #[serde(default)]
    pub preferred_region: i32,
    /// Встроенный чат с игроками ByteStrap на одном сервере Roblox.
    #[serde(default = "default_true")]
    pub chat_enabled: bool,
    /// Уведомлять о встрече другого игрока с ByteStrap на сервере.
    #[serde(default = "default_true")]
    pub chat_notify: bool,
    /// Адрес WebSocket-сервера чата.
    #[serde(default = "default_chat_server_url")]
    pub chat_server_url: String,
    /// Локально замученные user_id в чате.
    #[serde(default)]
    pub muted_users: Vec<u64>,
}

fn default_res_w() -> u32 { 1920 }
fn default_res_h() -> u32 { 1080 }
fn default_true() -> bool { true }
fn default_chat_server_url() -> String { "ws://127.0.0.1:8787/ws".into() }

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
            discord_custom_status: String::new(),
            discord_incognito: false,
            discord_show_server: true,
            notify_server: true,
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
            anti_afk: false,
            splash_on: true,
            splash_image: String::new(),
            auto_clean_last: 0,
            update_check_last: 0,
            window_max: false,
            win_w: 1120.0,
            win_h: 700.0,
            center_window: true,
            center_game: false,
            game_res_mode: 0,
            game_res_w: 1920,
            game_res_h: 1080,
            live_bg: true,
            death_sound_mode: 0,
            death_sound_path: String::new(),
            cursor_mode: 0,
            cursor_path: String::new(),
            textures_mode: 0,
            textures_path: String::new(),
            skybox_mode: 0,
            skybox_path: String::new(),
            shiftlock_mode: 0,
            shiftlock_path: String::new(),
            font_mode: 0,
            font_path: String::new(),
            preferred_region: 0,
            chat_enabled: true,
            chat_notify: true,
            chat_server_url: default_chat_server_url(),
            muted_users: Vec::new(),
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
