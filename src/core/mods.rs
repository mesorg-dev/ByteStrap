//! Оверлей модов: файлы в `base/Mods/` копируются в версию клиента
//! с бэкапом оригиналов в `base/Mods/.backup/`.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

const BACKUP_DIR: &str = ".backup";

#[derive(Debug, Clone)]
pub struct ModFile {
    /// Путь относительно корня Mods (через `/`).
    pub rel: String,
    pub size: u64,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct ApplyStats {
    pub applied: usize,
    pub skipped: usize,
    pub failed: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileStatus {
    /// Файл в версии совпадает с модом.
    Applied,
    /// Мод есть, но в версии файла нет (нужен apply/reinstall).
    Missing,
    /// В версии файл есть, мода нет (обычный файл клиента).
    Clean,
}

fn is_hidden(rel: &str) -> bool {
    rel.split('/').any(|c| c.starts_with('.'))
}

/// Обход дерева Mods → относительные пути с `/`.
pub fn scan(base: &Path) -> Result<Vec<ModFile>> {
    let root = base.join("Mods");
    let mut out = Vec::new();
    if !root.exists() {
        return Ok(out);
    }
    let mut stack = vec![(root.clone(), String::new())];
    while let Some((dir, prefix)) = stack.pop() {
        let entries = match std::fs::read_dir(&dir) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if prefix.is_empty() && name.starts_with('.') {
                continue;
            }
            let rel = if prefix.is_empty() {
                name.clone()
            } else {
                format!("{prefix}/{name}")
            };
            if is_hidden(&rel) {
                continue;
            }
            let path = entry.path();
            if path.is_dir() {
                stack.push((path, rel));
            } else if let Ok(meta) = entry.metadata() {
                out.push(ModFile {
                    rel,
                    size: meta.len(),
                });
            }
        }
    }
    out.sort_by(|a, b| a.rel.cmp(&b.rel));
    Ok(out)
}

fn backup_path(base: &Path, rel: &str) -> PathBuf {
    base.join("Mods").join(BACKUP_DIR).join(rel)
}

fn version_path(base: &Path, guid: &str, rel: &str) -> PathBuf {
    base.join("Versions").join(guid).join(rel)
}

/// Статус файла: применён / отсутствует / чисто.
pub fn file_status(base: &Path, guid: &str, rel: &str) -> FileStatus {
    let target = version_path(base, guid, rel);
    let source = base.join("Mods").join(rel);
    if !target.exists() {
        return if source.exists() {
            FileStatus::Missing
        } else {
            FileStatus::Clean
        };
    }
    if !source.exists() {
        return FileStatus::Clean;
    }
    match (
        std::fs::metadata(&target).map(|m| m.len()),
        std::fs::metadata(&source).map(|m| m.len()),
    ) {
        (Ok(a), Ok(b)) if a == b => FileStatus::Applied,
        _ => FileStatus::Missing,
    }
}

/// Копирует моды в версию. Оригиналы сохраняются в `.backup` (один раз).
pub fn apply(base: &Path, guid: &str) -> Result<ApplyStats> {
    let files = scan(base)?;
    if files.is_empty() {
        anyhow::bail!("mods folder is empty");
    }
    let mut stats = ApplyStats::default();
    for f in &files {
        let src = base.join("Mods").join(&f.rel);
        let dst = version_path(base, guid, &f.rel);
        let bak = backup_path(base, &f.rel);

        // Бэкап оригинала — только если его ещё нет и файл был не модом.
        if !bak.exists() && dst.exists() {
            if let Some(parent) = bak.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            if std::fs::copy(&dst, &bak).is_err() {
                stats.failed += 1;
                continue;
            }
        }
        if let Some(parent) = dst.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                let _ = e;
                stats.failed += 1;
                continue;
            }
        }
        match std::fs::copy(&src, &dst) {
            Ok(_) => stats.applied += 1,
            Err(_) => stats.failed += 1,
        }
    }
    if stats.applied == 0 && stats.failed > 0 {
        anyhow::bail!("could not apply any mod file");
    }
    Ok(stats)
}

/// Восстанавливает оригиналы из `.backup` и чистит его.
pub fn revert(base: &Path, guid: &str) -> Result<ApplyStats> {
    let bak_root = base.join("Mods").join(BACKUP_DIR);
    let mut stats = ApplyStats::default();
    if !bak_root.exists() {
        // Бэкапов нет — убираем просто модовые файлы из версии нельзя без orig,
        // но если и бэкапа нет, значит ничего не применяли.
        anyhow::bail!("no backups to revert");
    }
    let backups: Vec<ModFile> = scan_backup(&bak_root)?;
    for f in &backups {
        let src = bak_root.join(&f.rel);
        let dst = version_path(base, guid, &f.rel);
        if !src.exists() {
            continue;
        }
        if let Some(parent) = dst.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        match std::fs::copy(&src, &dst) {
            Ok(_) => stats.applied += 1,
            Err(_) => stats.failed += 1,
        }
    }
    // Удаляем бэкапы и файлы модов (моды остаются в Mods/).
    let _ = std::fs::remove_dir_all(&bak_root);
    Ok(stats)
}

fn scan_backup(root: &Path) -> Result<Vec<ModFile>> {
    let mut out = Vec::new();
    let mut stack = vec![(root.to_path_buf(), String::new())];
    while let Some((dir, prefix)) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            let rel = if prefix.is_empty() {
                name.clone()
            } else {
                format!("{prefix}/{name}")
            };
            let path = entry.path();
            if path.is_dir() {
                stack.push((path, rel));
            } else if let Ok(meta) = entry.metadata() {
                out.push(ModFile {
                    rel,
                    size: meta.len(),
                });
            }
        }
    }
    out.sort_by(|a, b| a.rel.cmp(&b.rel));
    Ok(out)
}

/// Проверка: применены ли все моды к версии.
#[allow(dead_code)] // включается на этапе авто-применения после установки
pub fn all_applied(base: &Path, guid: &str) -> Result<bool> {
    let files = scan(base)?;
    Ok(files
        .iter()
        .all(|f| file_status(base, guid, &f.rel) == FileStatus::Applied))
}

/// Создать папку Mods, если нет.
#[allow(dead_code)]
pub fn ensure_dir(base: &Path) -> Result<PathBuf> {
    let root = base.join("Mods");
    std::fs::create_dir_all(&root).context("create mods dir")?;
    Ok(root)
}

/// (для тестов) — консистентность скана: все пути без `..`.
#[allow(dead_code)]
fn paths_safe(files: &[ModFile]) -> bool {
    files
        .iter()
        .all(|f| !f.rel.contains("..") && !f.rel.starts_with('/'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scan_empty_missing_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let files = scan(tmp.path()).unwrap();
        assert!(files.is_empty());
    }

    #[test]
    fn scan_and_apply_revert_roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        let base = tmp.path();
        let guid = "version-0000000000000000";

        // Клиент с оригиналом + мод поверх.
        let vdir = base.join("Versions").join(guid).join("content/test.txt");
        std::fs::create_dir_all(vdir.parent().unwrap()).unwrap();
        std::fs::write(&vdir, "original").unwrap();

        let mdir = base.join("Mods").join("content/test.txt");
        std::fs::create_dir_all(mdir.parent().unwrap()).unwrap();
        std::fs::write(&mdir, "modded").unwrap();

        let files = scan(base).unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].rel, "content/test.txt");
        assert_eq!(file_status(base, guid, "content/test.txt"), FileStatus::Missing);

        let stats = apply(base, guid).unwrap();
        assert_eq!(stats.applied, 1);
        assert_eq!(file_status(base, guid, "content/test.txt"), FileStatus::Applied);
        assert!(all_applied(base, guid).unwrap());

        let stats = revert(base, guid).unwrap();
        assert_eq!(stats.applied, 1);
        assert_eq!(
            std::fs::read_to_string(&vdir).unwrap(),
            "original",
            "revert must restore the original"
        );
    }

    #[test]
    fn hidden_backup_skipped_in_scan() {
        let tmp = tempfile::tempdir().unwrap();
        let base = tmp.path();
        std::fs::create_dir_all(base.join("Mods/.backup/content")).unwrap();
        std::fs::write(base.join("Mods/.backup/content/x.txt"), "bak").unwrap();
        std::fs::write(base.join("Mods/visible.txt"), "ok").unwrap();
        let files = scan(base).unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].rel, "visible.txt");
    }
}
