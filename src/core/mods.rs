//! Оверлей модов: файлы в `base/Mods/` копируются в версию клиента
//! с бэкапом оригиналов в `base/Mods/.backup/`.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

const BACKUP_DIR: &str = ".backup";
const PROFILES_DIR: &str = "ModProfiles";

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

// ===== Профили модов =====

fn profiles_root(base: &Path) -> PathBuf {
    base.join(PROFILES_DIR)
}

/// Имя профиля безопасно для файловой системы (и Windows).
fn valid_name(name: &str) -> bool {
    let t = name.trim();
    !t.is_empty()
        && t.len() <= 64
        && !t.starts_with('.')
        && !t.ends_with('.')
        && !t
            .chars()
            .any(|c| matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|'))
}

/// Рекурсивная копия дерева; при `skip_hidden` точки-компоненты пропускаются.
fn copy_tree(src: &Path, dst: &Path, skip_hidden: bool) -> Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().to_string();
        if skip_hidden && name.starts_with('.') {
            continue;
        }
        let from = entry.path();
        let to = dst.join(&name);
        if from.is_dir() {
            copy_tree(&from, &to, skip_hidden)?;
        } else {
            if let Some(p) = to.parent() {
                std::fs::create_dir_all(p)?;
            }
            std::fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

/// Сохранить текущее содержимое `Mods/` как профиль (перезаписывает).
pub fn save_profile(base: &Path, name: &str) -> Result<()> {
    let name = name.trim();
    anyhow::ensure!(valid_name(name), "invalid profile name");
    let src = base.join("Mods");
    let dst = profiles_root(base).join(name);
    if dst.exists() {
        std::fs::remove_dir_all(&dst)?;
    }
    std::fs::create_dir_all(&dst)?;
    if src.is_dir() {
        copy_tree(&src, &dst, true)?; // .backup в профиль не попадает
    }
    Ok(())
}

/// Загрузить профиль: откат (если есть guid) → чистка `Mods/` → копия → применение.
///
/// Возвращает статистику применения или `None`, если применять нечего/нет клиента.
pub fn load_profile(base: &Path, guid: Option<&str>, name: &str) -> Result<Option<ApplyStats>> {
    let name = name.trim();
    anyhow::ensure!(valid_name(name), "invalid profile name");
    let src = profiles_root(base).join(name);
    anyhow::ensure!(src.is_dir(), "no such profile");

    // Сначала вернуть оригиналы, чтобы старые модовые файлы не остались в версии.
    if let Some(g) = guid {
        let _ = revert(base, g); // бэкапов может не быть — это ок
    }

    let root = base.join("Mods");
    if root.is_dir() {
        for entry in std::fs::read_dir(&root)? {
            let entry = entry?;
            let fname = entry.file_name().to_string_lossy().to_string();
            // Без guid бэкап не трогаем (к нему привязаны оригиналы).
            if guid.is_none() && fname == BACKUP_DIR {
                continue;
            }
            let p = entry.path();
            if p.is_dir() {
                std::fs::remove_dir_all(&p)?;
            } else {
                std::fs::remove_file(&p)?;
            }
        }
    } else {
        std::fs::create_dir_all(&root)?;
    }
    copy_tree(&src, &root, false)?;

    if let Some(g) = guid {
        if scan(base)?.is_empty() {
            return Ok(None);
        }
        Ok(Some(apply(base, g)?))
    } else {
        Ok(None)
    }
}

/// Список профилей (имена директорий), по алфавиту.
pub fn list_profiles(base: &Path) -> Vec<String> {
    let mut out = Vec::new();
    if let Ok(rd) = std::fs::read_dir(profiles_root(base)) {
        for e in rd.flatten() {
            if e.path().is_dir() {
                let n = e.file_name().to_string_lossy().to_string();
                if !n.starts_with('.') {
                    out.push(n);
                }
            }
        }
    }
    out.sort();
    out
}

/// Удалить профиль.
pub fn delete_profile(base: &Path, name: &str) -> Result<()> {
    let name = name.trim();
    anyhow::ensure!(valid_name(name), "invalid profile name");
    let dst = profiles_root(base).join(name);
    anyhow::ensure!(dst.is_dir(), "no such profile");
    std::fs::remove_dir_all(&dst)?;
    Ok(())
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

    #[test]
    fn profile_save_load_roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        let base = tmp.path();

        std::fs::create_dir_all(base.join("Mods/content")).unwrap();
        std::fs::write(base.join("Mods/content/a.txt"), "mod").unwrap();
        save_profile(base, "pack").unwrap();

        // Меняем текущие моды: файл перезаписан + лишний добавлен.
        std::fs::write(base.join("Mods/content/a.txt"), "changed").unwrap();
        std::fs::write(base.join("Mods/extra.txt"), "x").unwrap();

        let stats = load_profile(base, None, "pack").unwrap();
        assert!(stats.is_none(), "без клиента применять нечего");
        assert_eq!(
            std::fs::read_to_string(base.join("Mods/content/a.txt")).unwrap(),
            "mod"
        );
        assert!(
            !base.join("Mods/extra.txt").exists(),
            "файл вне профиля должен исчезнуть"
        );
    }

    #[test]
    fn profile_save_skips_backup_and_lists() {
        let tmp = tempfile::tempdir().unwrap();
        let base = tmp.path();
        std::fs::create_dir_all(base.join("Mods/.backup")).unwrap();
        std::fs::write(base.join("Mods/.backup/orig.txt"), "orig").unwrap();
        std::fs::write(base.join("Mods/mod.txt"), "mod").unwrap();

        save_profile(base, "alpha").unwrap();
        save_profile(base, "beta").unwrap();

        assert!(!base.join("ModProfiles/alpha/.backup").exists());
        assert!(base.join("ModProfiles/alpha/mod.txt").exists());
        assert_eq!(list_profiles(base), vec!["alpha".to_string(), "beta".to_string()]);

        delete_profile(base, "alpha").unwrap();
        assert_eq!(list_profiles(base), vec!["beta".to_string()]);
        assert!(delete_profile(base, "alpha").is_err());
    }

    #[test]
    fn profile_bad_names_rejected() {
        let tmp = tempfile::tempdir().unwrap();
        let base = tmp.path();
        for bad in ["", "  ", "..", "a/b", "a\\b", ".hidden", "x:yy", "trailing."] {
            assert!(
                save_profile(base, bad).is_err(),
                "name {bad:?} must be rejected"
            );
            assert!(load_profile(base, None, bad).is_err());
            assert!(delete_profile(base, bad).is_err());
        }
        assert!(list_profiles(base).is_empty());
    }

    #[test]
    fn profile_load_replaces_applied_version_files() {
        let tmp = tempfile::tempdir().unwrap();
        let base = tmp.path();
        let guid = "version-0000000000000000";

        // Оригинал клиента + мод A поверх.
        let vdir = base.join("Versions").join(guid).join("content/test.txt");
        std::fs::create_dir_all(vdir.parent().unwrap()).unwrap();
        std::fs::write(&vdir, "original").unwrap();
        std::fs::create_dir_all(base.join("Mods/content")).unwrap();
        std::fs::write(base.join("Mods/content/test.txt"), "modA").unwrap();
        save_profile(base, "packA").unwrap();

        apply(base, guid).unwrap();
        assert_eq!(std::fs::read_to_string(&vdir).unwrap(), "modA");

        // Профиль B меняет мод; после load версия содержит modB, бэкап свежий.
        std::fs::write(base.join("Mods/content/test.txt"), "modB").unwrap();
        save_profile(base, "packB").unwrap();
        load_profile(base, Some(guid), "packB").unwrap();
        assert_eq!(std::fs::read_to_string(&vdir).unwrap(), "modB");
        assert_eq!(
            std::fs::read_to_string(base.join("Mods/.backup/content/test.txt")).unwrap(),
            "original",
            "бэкап должен содержать оригинал клиента"
        );
    }
}
