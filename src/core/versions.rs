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

pub fn is_version_guid(s: &str) -> bool {
    s.len() == 24
        && s.starts_with("version-")
        && s[8..].chars().all(|c| c.is_ascii_hexdigit())
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

/// Целая установка: папка version-* с exe и AppSettings.xml
/// (AppSettings пишется только после полной распаковки).
pub fn find_installed(base: &Path) -> Option<String> {
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
        assert!(is_version_guid("version-0123456789abcdef"));
        assert!(!is_version_guid("version-zz23456789abcdef"));
        assert!(!is_version_guid("abc"));
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
