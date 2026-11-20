//! Галерея скриншотов Roblox:
//! Сканирование папки `%USERPROFILE%\Pictures\Roblox`,
//! метаданные файлов, копирование реального изображения в буфер Windows (arboard)
//! для отправки в Discord / мессенджеры, открытие в просмотрщике и проводнике.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

#[derive(Debug, Clone)]
pub struct ScreenshotItem {
    pub path: String,
    pub name: String,
    pub date: String,
    pub size: String,
    pub timestamp: u64,
}

/// Папка со скриншотами Roblox на Windows.
pub fn screenshots_dir() -> PathBuf {
    dirs::picture_dir()
        .map(|p| p.join("Roblox"))
        .unwrap_or_else(|| {
            dirs::home_dir()
                .map(|p| p.join("Pictures").join("Roblox"))
                .unwrap_or_else(|| PathBuf::from("."))
        })
}

/// Сканировать папку скриншотов Roblox (сортировка: свежие сверху).
pub fn scan() -> Vec<ScreenshotItem> {
    let dir = screenshots_dir();
    let Ok(entries) = fs::read_dir(&dir) else {
        return Vec::new();
    };

    let mut list = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
            continue;
        };
        let ext_lower = ext.to_ascii_lowercase();
        if !matches!(ext_lower.as_str(), "png" | "jpg" | "jpeg" | "gif" | "webp") {
            continue;
        }

        let meta = entry.metadata().ok();
        let bytes = meta.as_ref().map(|m| m.len()).unwrap_or(0);
        let time = meta
            .as_ref()
            .and_then(|m| m.modified().ok())
            .unwrap_or(SystemTime::UNIX_EPOCH);
        let unix = time
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let size_str = format_size(bytes);
        let date_str = format_date(unix);
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("screenshot")
            .to_string();

        list.push(ScreenshotItem {
            path: path.to_string_lossy().into_owned(),
            name,
            date: date_str,
            size: size_str,
            timestamp: unix,
        });
    }

    list.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
    list
}

/// Копировать изображение скриншота в системный буфер обмена для мгновенной вставки (Ctrl+V) в Discord.
#[cfg(windows)]
pub fn copy_to_clipboard(path_str: &str) -> Result<(), String> {
    let path = Path::new(path_str);
    if !path.exists() {
        return Err("Файл скриншота не найден".into());
    }

    let img = image::open(path).map_err(|e| format!("Не удалось декодировать изображение: {e}"))?;
    let rgba = img.into_rgba8();
    let (width, height) = (rgba.width() as usize, rgba.height() as usize);
    let raw_bytes = rgba.into_raw();

    let mut clipboard = arboard::Clipboard::new().map_err(|e| format!("Буфер обмена недоступен: {e}"))?;
    let img_data = arboard::ImageData {
        width,
        height,
        bytes: std::borrow::Cow::Owned(raw_bytes),
    };
    clipboard
        .set_image(img_data)
        .map_err(|e| format!("Ошибка записи в буфер: {e}"))?;

    Ok(())
}

#[cfg(not(windows))]
pub fn copy_to_clipboard(_path_str: &str) -> Result<(), String> {
    Err("Копирование изображений поддерживается только на Windows".into())
}

/// Открыть скриншот в стандартной программе просмотра изображений.
pub fn open_image(path_str: &str) -> Result<(), String> {
    let path = Path::new(path_str);
    if !path.exists() {
        return Err("Файл не найден".into());
    }
    #[cfg(windows)]
    {
        std::process::Command::new("explorer")
            .arg(path.as_os_str())
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Открыть папку со скриншотом в Проводнике Windows и подсветить файл.
pub fn open_folder(path_str: &str) -> Result<(), String> {
    let path = Path::new(path_str);
    #[cfg(windows)]
    {
        if path.exists() {
            let _ = std::process::Command::new("explorer")
                .arg(format!("/select,{}", path.display()))
                .spawn();
        } else {
            let dir = screenshots_dir();
            let _ = fs::create_dir_all(&dir);
            let _ = std::process::Command::new("explorer")
                .arg(dir.as_os_str())
                .spawn();
        }
    }
    Ok(())
}

/// Удалить файл скриншота.
pub fn delete_image(path_str: &str) -> Result<(), String> {
    let path = Path::new(path_str);
    if path.exists() {
        fs::remove_file(path).map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn format_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}

fn format_date(unix: u64) -> String {
    if unix == 0 {
        return String::new();
    }
    let day = crate::core::playtime::day_key(unix);
    let hours = (unix / 3600 + 3) % 24;
    let minutes = (unix % 3600) / 60;
    if day.len() >= 10 {
        format!("{}.{} {:02}:{:02}", &day[8..10], &day[5..7], hours, minutes)
    } else {
        format!("{day} {:02}:{:02}", hours, minutes)
    }
}
