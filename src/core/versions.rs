use std::path::{Path, PathBuf};
use std::time::SystemTime;

use anyhow::Result;
use serde::{Deserialize, Serialize};

use super::deploy::PLAYER_EXE;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledVersion {
    pub guid: String,
    pub size: u64,
    pub modified: SystemTime,
    pub has_player: bool,
    pub has_studio: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaselineFile {
    #[serde(rename = "Path")]
    pub path: String,
    #[serde(rename = "Size")]
    pub size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Baseline {
    #[serde(rename = "Files")]
    pub files: Vec<BaselineFile>,
}

#[derive(Debug, Default, Clone)]
pub struct IntegrityReport {
    pub checked: usize,
    pub missing: usize,
    pub size_mismatch: usize,
}

impl IntegrityReport {
    pub fn ok(&self) -> bool {
        self.missing == 0 && self.size_mismatch == 0
    }

    pub fn issues(&self) -> usize {
        self.missing + self.size_mismatch
    }
}

pub const STATIC_VERSION_DIR: &str = "Client";

pub fn is_version_guid(s: &str) -> bool {
    s == STATIC_VERSION_DIR
        || (s.len() == 24
            && s.starts_with("version-")
            && s[8..].chars().all(|c| c.is_ascii_hexdigit()))
}

fn version_dirs(base: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(rd) = std::fs::read_dir(base.join("Versions")) else {
        return out;
    };
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        if is_version_guid(&name) && e.path().is_dir() {
            out.push(e.path());
        }
    }
    out
}

/// Получение GUID установленной версии из version.txt / bytestrap.version.txt в папке Client,
/// либо из имени папки legacy-установки version-*.
pub fn get_installed_guid(base: &Path) -> Option<String> {
    let client_dir = base.join("Versions").join(STATIC_VERSION_DIR);
    for fname in &["bytestrap.version.txt", "version.txt"] {
        if let Ok(s) = std::fs::read_to_string(client_dir.join(fname)) {
            let t = s.trim();
            if !t.is_empty() {
                return Some(t.to_string());
            }
        }
    }
    for dir in version_dirs(base) {
        if let Some(name) = dir.file_name() {
            let name_str = name.to_string_lossy();
            if name_str != STATIC_VERSION_DIR && dir.join(PLAYER_EXE).exists() {
                return Some(name_str.to_string());
            }
        }
    }
    None
}

/// Миграция старых папок version-* в статичную папку Versions/Client,
/// чтобы путь к RobloxPlayerBeta.exe никогда не менялся (удобно для VPN split-tunneling).
pub fn migrate_legacy_versions(base: &Path) {
    let versions_dir = base.join("Versions");
    let client_dir = versions_dir.join(STATIC_VERSION_DIR);
    if client_dir.join(PLAYER_EXE).exists() {
        return;
    }
    let mut legacy = Vec::new();
    if let Ok(rd) = std::fs::read_dir(&versions_dir) {
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if name != STATIC_VERSION_DIR
                && name.starts_with("version-")
                && e.path().is_dir()
                && e.path().join(PLAYER_EXE).exists()
            {
                let mtime = e
                    .metadata()
                    .and_then(|m| m.modified())
                    .unwrap_or(SystemTime::UNIX_EPOCH);
                legacy.push((e.path(), name, mtime));
            }
        }
    }
    legacy.sort_by(|a, b| b.2.cmp(&a.2));
    if let Some((best_path, guid, _)) = legacy.first() {
        if client_dir.exists() {
            let _ = std::fs::remove_dir_all(&client_dir);
        }
        if std::fs::rename(best_path, &client_dir).is_ok() {
            let _ = std::fs::write(client_dir.join("bytestrap.version.txt"), guid.as_bytes());
            let _ = std::fs::write(client_dir.join("version.txt"), guid.as_bytes());
        }
    }
    // Чистим остальные старые папки version-*
    if let Ok(rd) = std::fs::read_dir(&versions_dir) {
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if name != STATIC_VERSION_DIR && name.starts_with("version-") && e.path().is_dir() {
                let _ = std::fs::remove_dir_all(e.path());
            }
        }
    }
}

/// Целая установка: папка Client (или legacy version-*) с exe и AppSettings.xml
/// (AppSettings пишется только после полной распаковки).
pub fn find_installed(base: &Path) -> Option<String> {
    let client_dir = base.join("Versions").join(STATIC_VERSION_DIR);
    if client_dir.join(PLAYER_EXE).exists() && client_dir.join("AppSettings.xml").exists() {
        return Some(STATIC_VERSION_DIR.to_string());
    }
    for dir in version_dirs(base) {
        let guid = dir.file_name()?.to_string_lossy().to_string();
        if dir.join(PLAYER_EXE).exists() && dir.join("AppSettings.xml").exists() {
            return Some(guid);
        }
    }
    None
}

pub fn list(base: &Path) -> Vec<InstalledVersion> {
    let mut list: Vec<InstalledVersion> = version_dirs(base)
        .into_iter()
        .filter_map(|dir| {
            let guid = dir.file_name()?.to_string_lossy().to_string();
            Some(InstalledVersion {
                modified: std::fs::metadata(&dir)
                    .and_then(|m| m.modified())
                    .unwrap_or(SystemTime::UNIX_EPOCH),
                guid,
                size: dir_size(&dir),
                has_player: dir.join(PLAYER_EXE).exists(),
                has_studio: dir.join("RobloxStudioBeta.exe").exists(),
            })
        })
        .collect();
    list.sort_by(|a, b| b.modified.cmp(&a.modified));
    list
}

pub fn dir_size(dir: &Path) -> u64 {
    let mut sum = 0u64;
    let Ok(rd) = std::fs::read_dir(dir) else {
        return 0;
    };
    for e in rd.flatten() {
        let p = e.path();
        match e.metadata() {
            Ok(m) if m.is_dir() => sum += dir_size(&p),
            Ok(m) => sum += m.len(),
            Err(_) => {}
        }
    }
    sum
}

pub fn delete(base: &Path, guid: &str) -> Result<()> {
    if !is_version_guid(guid) {
        anyhow::bail!("bad version guid");
    }
    let dir = base.join("Versions").join(guid);
    if dir.exists() {
        std::fs::remove_dir_all(&dir)?;
    }
    Ok(())
}

pub fn format_size(bytes: u64) -> String {
    if bytes >= 1_073_741_824 {
        format!("{:.1} ГБ", bytes as f64 / 1_073_741_824.0)
    } else if bytes >= 1_048_576 {
        format!("{} МБ", bytes / 1_048_576)
    } else {
        format!("{} КБ", bytes.max(1) / 1024)
    }
}

const BASELINE_FILE: &str = "bytestrap.integrity.json";

/// Снимок размеров оригинальных файлов — для проверки целостности.
pub fn write_baseline(version_dir: &Path) -> Result<()> {
    let mut files = Vec::new();
    collect_files(version_dir, version_dir, &mut files)?;
    files.sort_by(|a, b| a.path.cmp(&b.path));
    let baseline = Baseline { files };
    let json = serde_json::to_string_pretty(&baseline)?;
    std::fs::write(version_dir.join(BASELINE_FILE), json)?;
    Ok(())
}

pub fn ensure_baseline(version_dir: &Path) {
    if version_dir.join(BASELINE_FILE).exists() {
        return;
    }
    if version_dir.join(PLAYER_EXE).exists() && version_dir.join("AppSettings.xml").exists() {
        let _ = write_baseline(version_dir);
    }
}

fn collect_files(root: &Path, dir: &Path, out: &mut Vec<BaselineFile>) -> Result<()> {
    for e in std::fs::read_dir(dir)?.flatten() {
        let p = e.path();
        if p.is_dir() {
            // Флаги пользователя — не оригинал клиента, в снимок не берём
            if p.file_name().is_some_and(|n| n == "ClientSettings") {
                continue;
            }
            collect_files(root, &p, out)?;
        } else {
            // Исключаем маркеры версии ByteStrap и сам baseline
            if let Some(n) = p.file_name() {
                if n == "bytestrap.version.txt" || n == "version.txt" || n == BASELINE_FILE {
                    continue;
                }
            }
            let rel = p.strip_prefix(root)?.to_string_lossy().replace('\\', "/");
            out.push(BaselineFile {
                path: rel,
                size: e.metadata()?.len(),
            });
        }
    }
    Ok(())
}

pub fn check(version_dir: &Path) -> Result<IntegrityReport> {
    let mut report = IntegrityReport::default();
    let Ok(json) = std::fs::read_to_string(version_dir.join(BASELINE_FILE)) else {
        return Ok(report);
    };
    let baseline: Baseline = serde_json::from_str(&json)?;
    for f in &baseline.files {
        report.checked += 1;
        let path = version_dir.join(f.path.replace('/', std::path::MAIN_SEPARATOR_STR));
        match std::fs::metadata(&path) {
            Err(_) => report.missing += 1,
            Ok(m) if m.len() != f.size => report.size_mismatch += 1,
            Ok(_) => {}
        }
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guid_check() {
        assert!(is_version_guid(STATIC_VERSION_DIR));
        assert!(is_version_guid("version-0123456789abcdef"));
        assert!(!is_version_guid("version-zz23456789abcdef"));
        assert!(!is_version_guid("abc"));
    }

    #[test]
    fn migration_moves_legacy_to_client() {
        let td = tempfile::tempdir().unwrap();
        let base = td.path();
        let legacy_dir = base.join("Versions").join("version-0123456789abcdef");
        std::fs::create_dir_all(&legacy_dir).unwrap();
        std::fs::write(legacy_dir.join(PLAYER_EXE), b"exe").unwrap();
        std::fs::write(legacy_dir.join("AppSettings.xml"), b"xml").unwrap();

        migrate_legacy_versions(base);

        let client_dir = base.join("Versions").join(STATIC_VERSION_DIR);
        assert!(client_dir.join(PLAYER_EXE).exists());
        assert!(!legacy_dir.exists());
        assert_eq!(
            get_installed_guid(base),
            Some("version-0123456789abcdef".to_string())
        );
    }

    #[test]
    fn size_formatting() {
        assert_eq!(format_size(0), "0 КБ");
        assert_eq!(format_size(1024), "1 КБ");
        assert_eq!(format_size(1_048_576), "1 МБ");
        assert_eq!(format_size(1_572_864), "1 МБ"); // 1.5 МБ — целые МБ
        assert_eq!(format_size(1_073_741_824), "1.0 ГБ");
    }

    #[test]
    fn baseline_roundtrip_detects_damage() {
        let td = tempfile::tempdir().unwrap();
        let root = td.path();
        std::fs::create_dir_all(root.join("ClientSettings")).unwrap();
        std::fs::write(root.join("a.bin"), b"hello").unwrap();
        std::fs::write(root.join("ClientSettings").join("ClientAppSettings.json"), b"{}")
            .unwrap();

        write_baseline(root).unwrap();
        let rep = check(root).unwrap();
        assert_eq!(rep.checked, 1, "ClientSettings не входит в baseline");
        assert!(rep.ok());

        // Подменённый размер
        std::fs::write(root.join("a.bin"), b"hello world").unwrap();
        let rep = check(root).unwrap();
        assert_eq!(rep.size_mismatch, 1);
        assert!(!rep.ok());

        // Удалённый файл
        std::fs::remove_file(root.join("a.bin")).unwrap();
        let rep = check(root).unwrap();
        assert_eq!(rep.missing, 1);
        assert_eq!(rep.issues(), 1);
    }

    #[test]
    fn check_without_baseline_is_empty_ok() {
        let td = tempfile::tempdir().unwrap();
        let rep = check(td.path()).unwrap();
        assert_eq!(rep.checked, 0);
        assert!(rep.ok());
    }

    #[test]
    fn delete_rejects_bad_guid() {
        let td = tempfile::tempdir().unwrap();
        assert!(delete(td.path(), "nope").is_err());
    }
}
