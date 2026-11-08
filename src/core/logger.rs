//! Файловый лог лаунчера: `%LOCALAPPDATA%\ByteStrap\logs\launcher.log`.
//!
//! Плюс кольцевой буфер в памяти — страница «Логи» показывает его сразу,
//! без чтения диска. Запись best-effort: ошибки записи не роняют приложение.

use std::collections::VecDeque;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

pub const LEVEL_DEBUG: u8 = 0;
pub const LEVEL_INFO: u8 = 1;
pub const LEVEL_WARN: u8 = 2;
pub const LEVEL_ERROR: u8 = 3;

const MAX_RING: usize = 1000;
/// Ротация лога при достижении 1 МБ.
const MAX_FILE_BYTES: u64 = 1_048_576;

#[derive(Clone)]
pub struct Entry {
    /// `HH:MM:SS`.
    pub time: String,
    pub level: u8,
    pub text: String,
}

static RING: Mutex<VecDeque<Entry>> = Mutex::new(VecDeque::new());

pub fn dir() -> PathBuf {
    crate::core::config::Config::base_dir().join("logs")
}

pub fn file() -> PathBuf {
    dir().join("launcher.log")
}

pub fn tag(level: u8) -> &'static str {
    match level {
        LEVEL_DEBUG => "DEBUG",
        LEVEL_WARN => "WARN",
        LEVEL_ERROR => "ERROR",
        _ => "INFO",
    }
}

/// Локальное время `HH:MM:SS` (Windows) / UTC (остальные платформы — девелоперский хост).
fn now_hms() -> String {
    #[cfg(windows)]
    {
        use windows_sys::Win32::System::SystemInformation::{GetLocalTime, SYSTEMTIME};
        unsafe {
            let mut st: SYSTEMTIME = std::mem::zeroed();
            GetLocalTime(&mut st);
            format!("{:02}:{:02}:{:02}", st.wHour, st.wMinute, st.wSecond)
        }
    }
    #[cfg(not(windows))]
    {
        let secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        format!("{:02}:{:02}:{:02}", (secs / 3600) % 24, (secs / 60) % 60, secs % 60)
    }
}

fn write_file(time: &str, level: u8, text: &str) -> std::io::Result<()> {
    let dir = dir();
    fs::create_dir_all(&dir)?;
    let path = file();
    if let Ok(md) = fs::metadata(&path) {
        if md.len() > MAX_FILE_BYTES {
            let _ = fs::rename(&path, dir.join("launcher.old.log"));
        }
    }
    let mut f = OpenOptions::new().create(true).append(true).open(path)?;
    writeln!(f, "[{time}] [{}] {text}", tag(level))
}

pub fn push(level: u8, text: impl Into<String>) {
    let text = text.into();
    let time = now_hms();
    if let Ok(mut ring) = RING.lock() {
        if ring.len() >= MAX_RING {
            ring.pop_front();
        }
        ring.push_back(Entry {
            time: time.clone(),
            level,
            text: text.clone(),
        });
    }
    let _ = write_file(&time, level, &text);
}

pub fn debug(text: impl Into<String>) {
    push(LEVEL_DEBUG, text);
}

pub fn info(text: impl Into<String>) {
    push(LEVEL_INFO, text);
}

pub fn warn(text: impl Into<String>) {
    push(LEVEL_WARN, text);
}

pub fn error(text: impl Into<String>) {
    push(LEVEL_ERROR, text);
}

/// Снимок кольцевого буфера (для страницы «Логи»).
pub fn ring() -> Vec<Entry> {
    RING.lock().map(|r| r.iter().cloned().collect()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tags() {
        assert_eq!(tag(LEVEL_DEBUG), "DEBUG");
        assert_eq!(tag(LEVEL_INFO), "INFO");
        assert_eq!(tag(LEVEL_WARN), "WARN");
        assert_eq!(tag(LEVEL_ERROR), "ERROR");
    }
}
