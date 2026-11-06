//! Автообновление самого лаунчера с GitHub Releases (mesorg-dev/ByteStrap).
//! Схема: latest-релиз → сравнение тега с текущей версией → скачивание
//! exe во временную папку → батник ждёт выхода процесса, подменяет exe
//! и перезапускает. Соглашение о релизе: тег vX.Y.Z, ассет `*-win-x64.exe`.

use std::path::Path;
use std::time::Duration;

use anyhow::Result;
use serde::Deserialize;

pub const REPO_OWNER: &str = "mesorg-dev";
pub const REPO_NAME: &str = "ByteStrap";

#[derive(Clone, Debug)]
pub struct AppRelease {
    pub tag: String,
    pub notes: String,
    pub download_url: String,
}

pub fn current_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[derive(Deserialize)]
struct GhRelease {
    tag_name: Option<String>,
    body: Option<String>,
    assets: Option<Vec<GhAsset>>,
}

#[derive(Deserialize)]
struct GhAsset {
    name: Option<String>,
    browser_download_url: Option<String>,
}

fn client() -> Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .user_agent("ByteStrap-Updater/1.0")
        .build()?)
}

/// Latest-релиз с GitHub.
/// `Ok(None)` — релизов нет или нет exe-ассета; `Err` — сеть/API.
pub async fn get_latest() -> Result<Option<AppRelease>> {
    let url = format!(
        "https://api.github.com/repos/{REPO_OWNER}/{REPO_NAME}/releases/latest"
    );
    let text = client()?
        .get(&url)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;
    let r: GhRelease = serde_json::from_str(&text)?;
    let Some(tag) = r.tag_name.filter(|t| !t.is_empty()) else {
        return Ok(None);
    };

    // Первый .exe из ассетов (предпочтительно с win-x64 в имени).
    let mut download = String::new();
    let mut fallback = String::new();
    if let Some(assets) = r.assets {
        for a in assets {
            let an = a.name.unwrap_or_default();
            let au = a.browser_download_url.unwrap_or_default();
            if au.is_empty() || !an.to_lowercase().ends_with(".exe") {
                continue;
            }
            // Ассеты шелла (ByteStrap-Shell-*.exe) — не обновление лаунчера.
            if an.to_lowercase().contains("shell") {
                continue;
            }
            if fallback.is_empty() {
                fallback = au.clone();
            }
            if an.to_lowercase().contains("win-x64") {
                download = au;
                break;
            }
        }
    }
    if download.is_empty() {
        download = fallback;
    }
    if download.is_empty() {
        return Ok(None);
    }
    Ok(Some(AppRelease {
        tag,
        notes: r.body.unwrap_or_default(),
        download_url: download,
    }))
}

/// Тег новее текущей версии? Тег вида v1.2.3 (v опциональна).
pub fn is_newer(current: &str, tag: &str) -> bool {
    match (parse_version(current), parse_version(tag)) {
        (Some(cur), Some(rel)) => rel > cur,
        _ => false,
    }
}

fn parse_version(s: &str) -> Option<(u64, u64, u64)> {
    let mut s = s.trim().trim_start_matches(['v', 'V']);
    if let Some(cut) = s.find(['-', '+']) {
        s = &s[..cut];
    }
    let mut parts = s.split('.');
    let mut nums = [0u64; 3];
    for n in nums.iter_mut() {
        match parts.next() {
            Some(p) => *n = p.trim().parse().ok()?,
            None => break,
        }
    }
    if parts.next().is_some() {
        return None;
    }
    Some((nums[0], nums[1], nums[2]))
}

/// Скачать exe релиза. `progress(прочитано, всего)` вызывается по мере чтения.
#[cfg(windows)]
pub async fn download<F>(url: &str, dest: &Path, mut progress: F) -> Result<()>
where
    F: FnMut(u64, u64),
{
    use futures_util::StreamExt;

    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let resp = client()?.get(url).send().await?.error_for_status()?;
    let total = resp.content_length().unwrap_or(0);
    let mut stream = resp.bytes_stream();
    let mut file = std::fs::File::create(dest)?;
    let mut done = 0u64;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        std::io::Write::write_all(&mut file, &chunk)?;
        done += chunk.len() as u64;
        progress(done, total);
    }
    Ok(())
}

/// Установить скачанное и перезапуститься: батник ждёт выхода текущего
/// процесса (exe занят, пока жив), подменяет файл и стартует заново.
/// Пути уходят в окружение (батник чисто ASCII — кодировка не портит).
#[cfg(windows)]
pub fn install_and_restart(new_exe: &Path) -> Result<()> {
    let current = std::env::current_exe()?;
    let pid = std::process::id();
    let bat = std::env::temp_dir().join("bytestrap-update.bat");

    let script = format!(
        "@echo off\r\n\
         setlocal\r\n\
         :waitloop\r\n\
         tasklist /FI \"PID eq {pid}\" /NH 2>NUL | find \" {pid} \" >NUL\r\n\
         if not errorlevel 1 (timeout /t 1 /nobreak >NUL & goto waitloop)\r\n\
         move /y \"%BYSTRAP_NEW%\" \"%BYSTRAP_EXE%\" >NUL\r\n\
         if exist \"%BYSTRAP_EXE%\" start \"\" \"%BYSTRAP_EXE%\"\r\n\
         del \"%~f0\"\r\n"
    );
    std::fs::write(&bat, script)?;

    use std::os::windows::process::CommandExt;
    let mut cmd = std::process::Command::new(
        std::env::var("ComSpec").unwrap_or_else(|_| "cmd.exe".into()),
    );
    // Путь батника отдаём как есть — std сам экранирует его кавычками,
    // а /s снимает их у последнего аргумента /c. Свои кавычки вокруг пути
    // ломали бы разбор (std экранировал бы их как \").
    cmd.arg("/d")
        .arg("/s")
        .arg("/c")
        .arg(bat.as_os_str())
        .env("BYSTRAP_NEW", new_exe)
        .env("BYSTRAP_EXE", &current)
        .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
        .spawn()?;
    Ok(())
}

#[cfg(not(windows))]
pub async fn download<F>(_url: &str, _dest: &Path, _progress: F) -> Result<()>
where
    F: FnMut(u64, u64),
{
    Err(anyhow::anyhow!("обновление доступно только на Windows"))
}

#[cfg(not(windows))]
pub fn install_and_restart(_new_exe: &Path) -> Result<()> {
    Err(anyhow::anyhow!("обновление доступна только на Windows"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_parsing() {
        assert_eq!(parse_version("v1.2.3"), Some((1, 2, 3)));
        assert_eq!(parse_version("1.2"), Some((1, 2, 0)));
        assert_eq!(parse_version("1"), Some((1, 0, 0)));
        assert_eq!(parse_version("1.2.3-beta"), Some((1, 2, 3)));
        assert_eq!(parse_version("1.2.3+build"), Some((1, 2, 3)));
        assert_eq!(parse_version("abc"), None);
        assert_eq!(parse_version("1.2.3.4.5"), None);
        assert_eq!(parse_version(""), None);
    }

    #[test]
    fn newer_detection() {
        assert!(is_newer("1.0.0", "v1.0.1"));
        assert!(is_newer("1.0.0", "1.1.0"));
        assert!(!is_newer("1.1.0", "v1.0.9"));
        assert!(!is_newer("1.0.0", "v1.0.0"));
        assert!(!is_newer("garbage", "v1.0.0"));
        assert!(!is_newer("1.0.0", "garbage"));
    }
}
