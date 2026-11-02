use std::fs::File;
use std::io::Write;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};
use futures_util::StreamExt;
use reqwest::header::{CONTENT_RANGE, RANGE};
use reqwest::StatusCode;
use serde::Deserialize;

use super::config::Config;
use super::versions;

pub const PLAYER_EXE: &str = "RobloxPlayerBeta.exe";
const VERSION_API: &str = "https://clientsettings.roblox.com/v2/client-version/WindowsPlayer";
const CDN: &str = "https://setup.rbxcdn.com";
const UA: &str = concat!("ByteStrap/", env!("CARGO_PKG_VERSION"));

const APP_SETTINGS_XML: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\r\n<Settings>\r\n\t<ContentFolder>content</ContentFolder>\r\n\t<BaseUrl>http://www.roblox.com</BaseUrl>\r\n</Settings>\r\n";

/// Карта распаковки плеера — как CommonAppData._commonMap в Voidstrap;
/// неизвестные пакеты уходят в корень версии.
const PLAYER_DIR_MAP: &[(&str, &str)] = &[
    ("Libraries.zip", ""),
    ("RobloxApp.zip", ""),
    ("redist.zip", ""),
    ("shaders.zip", "shaders"),
    ("ssl.zip", "ssl"),
    ("WebView2.zip", ""),
    ("WebView2RuntimeInstaller.zip", "WebView2RuntimeInstaller"),
    ("content-avatar.zip", "content/avatar"),
    ("content-configs.zip", "content/configs"),
    ("content-fonts.zip", "content/fonts"),
    ("content-sky.zip", "content/sky"),
    ("content-sounds.zip", "content/sounds"),
    ("content-textures2.zip", "content/textures"),
    ("content-models.zip", "content/models"),
    ("content-textures3.zip", "PlatformContent/pc/textures"),
    ("content-terrain.zip", "PlatformContent/pc/terrain"),
    ("content-platform-fonts.zip", "PlatformContent/pc/fonts"),
    (
        "content-platform-dictionaries.zip",
        "PlatformContent/pc/shared_compression_dictionaries",
    ),
    ("extracontent-luapackages.zip", "ExtraContent/LuaPackages"),
    ("extracontent-translations.zip", "ExtraContent/translations"),
    ("extracontent-models.zip", "ExtraContent/models"),
    ("extracontent-textures.zip", "ExtraContent/textures"),
    ("extracontent-places.zip", "ExtraContent/places"),
];

#[derive(Clone, Debug, Deserialize)]
pub struct ClientVersion {
    pub version: String,
    pub guid: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Phase {
    #[default]
    Check,
    Download,
    Extract,
    Setup,
}

#[derive(Clone, Debug)]
pub enum Event {
    Progress {
        phase: Phase,
        done: u64,
        total: u64,
    },
    Finished {
        guid: String,
        already: bool,
        /// Сделать версию активной в конфиге (лаунчер выставляет сам).
        activate: bool,
    },
    Failed(String),
    Cancelled,
}

pub struct Package {
    pub name: String,
    pub packed: u64,
}

pub fn is_version_guid(s: &str) -> bool {
    s.len() == 24
        && s.starts_with("version-")
        && s[8..].chars().all(|c| c.is_ascii_hexdigit())
}

fn api_client() -> Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .user_agent(UA)
        .timeout(Duration::from_secs(30))
        .connect_timeout(Duration::from_secs(15))
        .build()?)
}

fn file_client() -> Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .user_agent(UA)
        .connect_timeout(Duration::from_secs(15))
        .pool_max_idle_per_host(16)
        .build()?)
}

pub async fn latest_version() -> Result<ClientVersion> {
    let client = api_client()?;
    let v: serde_json::Value = client
        .get(VERSION_API)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let version = v["version"].as_str().unwrap_or("").to_string();
    let guid = v["clientVersionUpload"].as_str().unwrap_or("").to_string();
    if version.is_empty() || !is_version_guid(&guid) {
        bail!("bad version api response");
    }
    Ok(ClientVersion { version, guid })
}

fn package_url(guid: &str, name: &str) -> String {
    format!("{CDN}/{guid}-{name}")
}

pub fn parse_manifest(text: &str) -> Result<Vec<Package>> {
    let lines: Vec<&str> = text.lines().collect();
    if lines.first().map(|s| s.trim()) != Some("v0") {
        bail!("bad manifest header");
    }
    let mut list = Vec::new();
    let mut i = 1;
    while i + 3 < lines.len() {
        let name = lines[i].trim();
        i += 4;
        if name.is_empty() || name.to_ascii_lowercase().ends_with(".exe") {
            continue;
        }
        let packed: u64 = lines[i - 2]
            .trim()
            .parse()
            .with_context(|| format!("bad packed size for {name}"))?;
        list.push(Package {
            name: name.to_string(),
            packed,
        });
    }
    if list.is_empty() {
        bail!("empty manifest");
    }
    Ok(list)
}

fn dir_for(name: &str) -> &str {
    PLAYER_DIR_MAP
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(name))
        .map(|(_, v)| *v)
        .unwrap_or("")
}

async fn download_file(
    client: &reqwest::Client,
    url: &str,
    dest: &Path,
    expected: u64,
    done: &AtomicU64,
    cancel: &AtomicBool,
) -> Result<()> {
    if cancel.load(Ordering::Relaxed) {
        bail!("cancelled");
    }
    let existing = std::fs::metadata(dest).map(|m| m.len()).unwrap_or(0);
    if expected > 0 && existing == expected {
        done.fetch_add(existing, Ordering::Relaxed);
        return Ok(());
    }
    let mut start = if expected == 0 || existing >= expected {
        0
    } else {
        existing
    };

    let mut req = client.get(url);
    if start > 0 {
        req = req.header(RANGE, format!("bytes={start}-"));
    }
    let res = req.send().await?.error_for_status()?;
    let append = start > 0
        && res.status() == StatusCode::PARTIAL_CONTENT
        && res
            .headers()
            .get(CONTENT_RANGE)
            .map(|v| v.to_str().unwrap_or("").starts_with(&format!("bytes {start}-")))
            .unwrap_or(false);
    if !append {
        start = 0;
    }

    let mut opts = std::fs::OpenOptions::new();
    opts.create(true).write(true);
    if append {
        opts.append(true);
    } else {
        opts.truncate(true);
    }
    let mut file = opts.open(dest)?;
    if start > 0 {
        done.fetch_add(start, Ordering::Relaxed);
    }

    let mut stream = res.bytes_stream();
    while let Some(chunk) = stream.next().await {
        if cancel.load(Ordering::Relaxed) {
            bail!("cancelled");
        }
        let chunk = chunk?;
        file.write_all(&chunk)?;
        done.fetch_add(chunk.len() as u64, Ordering::Relaxed);
    }
    file.flush()?;

    if expected > 0 {
        let len = std::fs::metadata(dest).map(|m| m.len()).unwrap_or(0);
        if len != expected {
            bail!("bad size for {}: {len} != {expected}", dest.display());
        }
    }
    Ok(())
}

fn extract_zip(zip_path: &Path, dest: &Path) -> Result<()> {
    std::fs::create_dir_all(dest)?;
    let f = File::open(zip_path)?;
    let mut archive = zip::ZipArchive::new(f)?;
    // extract() не даёт путям выйти за пределы dest (zip-slip)
    archive.extract(dest)?;
    Ok(())
}

/// Установка/обновление клиента до последней версии.
pub async fn ensure_installed(
    cfg: &Config,
    tx: &Sender<Event>,
    cancel: &Arc<AtomicBool>,
    force: bool,
) -> Result<String> {
    let send = |e: Event| {
        let _ = tx.send(e);
    };

    send(Event::Progress {
        phase: Phase::Check,
        done: 0,
        total: 0,
    });
    let latest = latest_version().await.context("version api")?;
    if cancel.load(Ordering::Relaxed) {
        send(Event::Cancelled);
        bail!("cancelled");
    }

    let base = cfg.base();
    let installed = versions::find_installed(&base);
    if !force && installed.as_deref() == Some(latest.guid.as_str()) {
        versions::ensure_baseline(&base.join("Versions").join(&latest.guid));
        send(Event::Finished {
            guid: latest.guid.clone(),
            already: true,
            activate: true,
        });
        return Ok(latest.guid);
    }

    ensure_guid(cfg, &latest.guid, force, true, tx, cancel, installed.as_deref()).await
}

/// Распаковка конкретной версии (guid): манифест → параллельные зипы
/// (с возобновлением по Range) → распаковка по карте → AppSettings.xml
/// (маркер целой установки) → снимок файлов. `repair_for` — версия, которую
/// держим в ретеншене при чистке.
pub async fn ensure_guid(
    cfg: &Config,
    guid: &str,
    force: bool,
    activate: bool,
    tx: &Sender<Event>,
    cancel: &Arc<AtomicBool>,
    repair_for: Option<&str>,
) -> Result<String> {
    let send = |e: Event| {
        let _ = tx.send(e);
    };
    let cancelled = || cancel.load(Ordering::Relaxed);

    send(Event::Progress {
        phase: Phase::Download,
        done: 0,
        total: 0,
    });
    let base = cfg.base();
    let client = file_client()?;
    let manifest_url = format!("{CDN}/{guid}-rbxPkgManifest.txt");
    let manifest = client
        .get(&manifest_url)
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;
    let packages = parse_manifest(&manifest)?;

    let version_dir = base.join("Versions").join(guid);
    let dl_dir = base.join("Downloads").join(guid);
    std::fs::create_dir_all(&version_dir)?;
    std::fs::create_dir_all(&dl_dir)?;

    // Недокачанная установка без AppSettings.xml или принудительный ремонт:
    // удаляем каталог (ClientSettings с флагами сохраняем), распаковка заново.
    let incomplete =
        version_dir.join(PLAYER_EXE).exists() && !version_dir.join("AppSettings.xml").exists();
    if force || incomplete {
        let cs = version_dir.join("ClientSettings");
        let saved = base.join(".clientsettings-tmp");
        let had_cs = cs.exists();
        if had_cs {
            let _ = std::fs::remove_dir_all(&saved);
            let _ = std::fs::rename(&cs, &saved);
        }
        let _ = std::fs::remove_dir_all(&version_dir);
        std::fs::create_dir_all(&version_dir)?;
        if had_cs && saved.exists() {
            let _ = std::fs::rename(&saved, &cs);
        }
    }

    let total: u64 = packages.iter().map(|p| p.packed).sum();
    let done = Arc::new(AtomicU64::new(0));
    let stop_reporter = Arc::new(AtomicBool::new(false));
    let reporter = {
        let tx = tx.clone();
        let done = Arc::clone(&done);
        let stop = Arc::clone(&stop_reporter);
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_millis(120)).await;
                if stop.load(Ordering::Relaxed) {
                    break;
                }
                let _ = tx.send(Event::Progress {
                    phase: Phase::Download,
                    done: done.load(Ordering::Relaxed),
                    total,
                });
            }
        })
    };

    let threads = cfg.download_threads.clamp(1, 8) as usize;
    let download_result: Result<()> = {
        let mut stream = futures_util::stream::iter(packages.iter().map(|p| {
            let client = client.clone();
            let url = package_url(guid, &p.name);
            let dest = dl_dir.join(&p.name);
            let done = Arc::clone(&done);
            let cancel = Arc::clone(cancel);
            let name = p.name.clone();
            async move {
                download_file(&client, &url, &dest, p.packed, &done, &cancel)
                    .await
                    .with_context(|| format!("download {name}"))
            }
        }))
        .buffer_unordered(threads);
        let mut result = Ok(());
        while let Some(r) = stream.next().await {
            if let Err(e) = r {
                result = Err(e);
                break;
            }
            if cancelled() {
                result = Err(anyhow!("cancelled"));
                break;
            }
        }
        result
    };
    stop_reporter.store(true, Ordering::Relaxed);
    let _ = reporter.await;

    if let Err(e) = download_result {
        if cancelled() {
            send(Event::Cancelled);
        }
        return Err(e);
    }
    if cancelled() {
        send(Event::Cancelled);
        bail!("cancelled");
    }

    send(Event::Progress {
        phase: Phase::Extract,
        done: 0,
        total: packages.len() as u64,
    });
    for (i, p) in packages.iter().enumerate() {
        if cancelled() {
            send(Event::Cancelled);
            bail!("cancelled");
        }
        let dest = version_dir.join(dir_for(&p.name));
        extract_zip(&dl_dir.join(&p.name), &dest)
            .with_context(|| format!("extract {}", p.name))?;
        send(Event::Progress {
            phase: Phase::Extract,
            done: i as u64 + 1,
            total: packages.len() as u64,
        });
    }

    send(Event::Progress {
        phase: Phase::Setup,
        done: 1,
        total: 1,
    });
    std::fs::write(version_dir.join("AppSettings.xml"), APP_SETTINGS_XML)?;
    versions::write_baseline(&version_dir)?;

    // Чистка: загрузки целиком, старые версии кроме новых/текущей/предыдущей
    let _ = std::fs::remove_dir_all(base.join("Downloads"));
    let mut keep = vec![
        guid.to_string(),
        cfg.installed_version.clone(),
        cfg.previous_version.clone(),
    ];
    if let Some(r) = repair_for {
        keep.push(r.to_string());
    }
    for dir in versions::list(&base) {
        if !keep.iter().any(|k| !k.is_empty() && *k == dir.guid) {
            let _ = std::fs::remove_dir_all(base.join("Versions").join(&dir.guid));
        }
    }

    send(Event::Finished {
        guid: guid.to_string(),
        already: false,
        activate,
    });
    Ok(guid.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_guid_validation() {
        assert!(is_version_guid("version-0123456789abcdef"));
        assert!(is_version_guid("version-0123456789ABCDEF"));
        assert!(!is_version_guid(""));
        assert!(!is_version_guid("version-"));
        assert!(!is_version_guid("version-0123456789abcde")); // 23
        assert!(!is_version_guid("version-0123456789abcdef0")); // 25
        assert!(!is_version_guid("0123456789abcdef")); // нет префикса
        assert!(!is_version_guid("version-0123456789abcdeg")); // не hex
    }

    #[test]
    fn manifest_parses_quads_and_skips_exe() {
        let text = "v0\n\
            content.zip\nsig1\n100\n200\n\
            RobloxPlayerInstaller.exe\nsig2\n300\n400\n\
            more.zip\nsig3\n50\n60\n";
        let list = parse_manifest(text).unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].name, "content.zip");
        assert_eq!(list[0].packed, 100);
        assert_eq!(list[1].name, "more.zip");
        assert_eq!(list[1].packed, 50);
    }

    #[test]
    fn manifest_rejects_bad_input() {
        assert!(parse_manifest("").is_err());
        assert!(parse_manifest("v1\na\nb\n1\n2\n").is_err());
        // только exe — скачивать нечего
        assert!(parse_manifest("v0\nsetup.exe\ns\n1\n2\n").is_err());
    }
}
