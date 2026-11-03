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
    pub download_threads: u32,
    /// Roblox deployment channel: "LIVE" | "BETA" | "ZNext" | ...
    pub channel: String,
    pub auto_update: bool,
    pub close_on_launch: bool,
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
            download_threads: 4,
            channel: "LIVE".into(),
            auto_update: true,
            close_on_launch: false,
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
