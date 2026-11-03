//! Managed-моды: каждая подпапка `Mods/<Мод>/` — мод; включение и порядок
//! применения хранятся в `Mods/mods.json`. Файлы включённых модов
//! копируются в версию с бэкапом оригиналов в `Mods/.backup/`.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

const BACKUP_DIR: &str = ".backup";
const PROFILES_DIR: &str = "ModProfiles";
const INDEX_FILE: &str = "mods.json";
/// Имя мода для миграции плоской раскладки v0.1.x.
const LEGACY_MOD: &str = "Default";

#[derive(Debug, Clone)]
pub struct ModFile {
    /// Путь для таблицы: `<Мод>/<внутренний путь>` через `/`.
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

// ===== Индекс =====

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModEntry {
    pub id: String,
    pub enabled: bool,
    pub order: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModIndex {
    pub version: u32,
    pub mods: Vec<ModEntry>,
}

impl Default for ModIndex {
    fn default() -> Self {
        Self {
            version: 1,
            mods: Vec::new(),
        }
    }
}

fn index_path(base: &Path) -> PathBuf {
    base.join("Mods").join(INDEX_FILE)
}

pub fn load_index(base: &Path) -> ModIndex {
    std::fs::read_to_string(index_path(base))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save_index(base: &Path, idx: &ModIndex) -> Result<()> {
    let p = index_path(base);
    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&p, serde_json::to_string_pretty(idx)?)?;
    Ok(())
}

/// Папки модов на диске (скрытые пропущены), по алфавиту.
fn mod_dirs(base: &Path) -> Vec<String> {
    let mut out = Vec::new();
    if let Ok(rd) = std::fs::read_dir(base.join("Mods")) {
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if name.starts_with('.') || !e.path().is_dir() {
                continue;
            }
            out.push(name);
        }
    }
    out.sort();
    out
}

/// Миграция плоской раскладки v0.1.x → managed: всё содержимое `Mods/`
/// (кроме скрытого) переезжает в `Mods/Default/`, создаётся индекс.
fn migrate_flat(base: &Path) -> ModIndex {
    let root = base.join("Mods");
    let mut idx = ModIndex::default();
    let entries: Vec<PathBuf> = std::fs::read_dir(&root)
        .map(|rd| {
            rd.flatten()
                .map(|e| e.path())
                .filter(|p| {
                    p.file_name()
                        .map(|n| !n.to_string_lossy().starts_with('.'))
                        .unwrap_or(false)
                })
                .collect()
        })
        .unwrap_or_default();
    if entries.is_empty() {
        return idx;
    }
    let dst = root.join(LEGACY_MOD);
    let _ = std::fs::create_dir_all(&dst);
    for p in entries {
        let Some(name) = p.file_name().map(|n| n.to_os_string()) else {
            continue;
        };
        // При конфликте (папка Default уже существует) — сливаем вручную.
        let to = dst.join(&name);
        if to.exists() {
            if p.is_dir() {
                let _ = copy_tree(&p, &to, false);
                let _ = std::fs::remove_dir_all(&p);
            } else if let Ok(meta) = p.metadata() {
                if meta.is_file() {
                    let _ = std::fs::copy(&p, &to);
                    let _ = std::fs::remove_file(&p);
                }
            }
        } else {
            let _ = std::fs::rename(&p, &to);
        }
    }
    idx.mods.push(ModEntry {
        id: LEGACY_MOD.into(),
        enabled: true,
        order: 0,
    });
    idx
}

/// Синхронизирует индекс с диском (миграция плоской раскладки, автодобавление
/// новых папок, удаление записей без папок) и возвращает его, отсортированный
/// по порядку применения.
pub fn sync_index(base: &Path) -> Result<ModIndex> {
    let root = base.join("Mods");
    std::fs::create_dir_all(&root)?;

    let mut idx = if index_path(base).exists() {
        load_index(base)
    } else {
        let idx = migrate_flat(base);
        save_index(base, &idx)?;
        idx
    };

    let dirs = mod_dirs(base);
    idx.mods.retain(|m| dirs.contains(&m.id));
    let mut next = idx.mods.iter().map(|m| m.order).max().map(|o| o + 1).unwrap_or(0);
    for d in &dirs {
        if !idx.mods.iter().any(|m| m.id == *d) {
            idx.mods.push(ModEntry {
                id: d.clone(),
                enabled: true,
                order: next,
            });
            next += 1;
        }
    }
    idx.mods
        .sort_by(|a, b| a.order.cmp(&b.order).then(a.id.cmp(&b.id)));
    for (i, m) in idx.mods.iter_mut().enumerate() {
        m.order = i;
    }
    save_index(base, &idx)?;
    Ok(idx)
}

/// Мод на диске + состояние из индекса.
#[derive(Debug, Clone)]
pub struct ModInfo {
    pub id: String,
    pub enabled: bool,
    pub files: usize,
    pub size: u64,
}

/// Обход поддерева → относительные пути с `/`; скрытые компоненты пропускаются.
fn walk(root: &Path) -> Vec<ModFile> {
    let mut out = Vec::new();
    if !root.exists() {
        return out;
    }
    let mut stack = vec![(root.to_path_buf(), String::new())];
    while let Some((dir, prefix)) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') {
                continue;
            }
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
    out
}

/// Список модов для страницы (папка + состояние + счётчики).
pub fn scan_mods(base: &Path) -> Vec<ModInfo> {
    let Ok(idx) = sync_index(base) else {
        return Vec::new();
    };
    idx.mods
        .iter()
        .map(|m| {
            let files = walk(&base.join("Mods").join(&m.id));
            ModInfo {
                id: m.id.clone(),
                enabled: m.enabled,
                files: files.len(),
                size: files.iter().map(|f| f.size).sum(),
            }
        })
        .collect()
}

/// Файлы включённых модов для таблицы; rel = `<Мод>/<путь>`.
pub fn scan_enabled(base: &Path) -> Vec<ModFile> {
    let Ok(idx) = sync_index(base) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for m in idx.mods.iter().filter(|m| m.enabled) {
        for f in walk(&base.join("Mods").join(&m.id)) {
            out.push(ModFile {
                rel: format!("{}/{}", m.id, f.rel),
                size: f.size,
            });
        }
    }
    out
}

fn backup_path(base: &Path, rel: &str) -> PathBuf {
    base.join("Mods").join(BACKUP_DIR).join(rel)
}

fn version_path(base: &Path, guid: &str, rel: &str) -> PathBuf {
    base.join("Versions").join(guid).join(rel)
}

/// Статус файла (rel вида `<Мод>/<путь>`): применён / отсутствует / чисто.
pub fn file_status(base: &Path, guid: &str, rel: &str) -> FileStatus {
    let Some((mod_id, inner)) = rel.split_once('/') else {
        return FileStatus::Clean;
    };
    let target = version_path(base, guid, inner);
    let source = base.join("Mods").join(mod_id).join(inner);
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

/// Копирует включённые моды в версию (в порядке order; поздние перетирают
/// ранние). Оригиналы сохраняются в `.backup` (один раз).
pub fn apply(base: &Path, guid: &str) -> Result<ApplyStats> {
    let idx = sync_index(base)?;
    let mut stats = ApplyStats::default();
    let mut any = false;
    for m in idx.mods.iter().filter(|m| m.enabled) {
        let mod_root = base.join("Mods").join(&m.id);
        for f in walk(&mod_root) {
            let src = mod_root.join(&f.rel);
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
                if std::fs::create_dir_all(parent).is_err() {
                    stats.failed += 1;
                    continue;
                }
            }
            match std::fs::copy(&src, &dst) {
                Ok(_) => {
                    stats.applied += 1;
                    any = true;
                }
                Err(_) => stats.failed += 1,
            }
        }
    }
    if !any {
        anyhow::bail!("could not apply any mod file");
    }
    Ok(stats)
}

/// Восстанавливает оригиналы из `.backup` и чистит его.
pub fn revert(base: &Path, guid: &str) -> Result<ApplyStats> {
    let bak_root = base.join("Mods").join(BACKUP_DIR);
    let mut stats = ApplyStats::default();
    if !bak_root.exists() {
        // Бэкапов нет — значит ничего не применяли.
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
    // Удаляем бэкапы (модовые файлы в Mods/ остаются).
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

/// Проверка: применены ли все файлы включённых модов.
#[allow(dead_code)] // включается на этапе авто-применения после установки
pub fn all_applied(base: &Path, guid: &str) -> Result<bool> {
    let files = scan_enabled(base);
    Ok(files
        .iter()
        .all(|f| file_status(base, guid, &f.rel) == FileStatus::Applied))
}

/// Включить/выключить мод.
pub fn set_enabled(base: &Path, id: &str, on: bool) -> Result<()> {
    let mut idx = sync_index(base)?;
    let entry = idx
        .mods
        .iter_mut()
        .find(|m| m.id == id)
        .context("no such mod")?;
    entry.enabled = on;
    save_index(base, &idx)
}

/// Сдвинуть мод в порядке применения: `-1` вверх, `+1` вниз.
pub fn move_mod(base: &Path, id: &str, delta: i32) -> Result<()> {
    let mut idx = sync_index(base)?;
    let pos = idx
        .mods
        .iter()
        .position(|m| m.id == id)
        .context("no such mod")?;
    let other = if delta < 0 {
        pos.checked_sub(1)
    } else {
        (pos + 1 < idx.mods.len()).then_some(pos + 1)
    };
    if let Some(o) = other {
        idx.mods.swap(pos, o);
        for (i, m) in idx.mods.iter_mut().enumerate() {
            m.order = i;
        }
    }
    save_index(base, &idx)
}

/// Создать папку Mods, если нет.
#[allow(dead_code)]
pub fn ensure_dir(base: &Path) -> Result<PathBuf> {
    let root = base.join("Mods");
    std::fs::create_dir_all(&root).context("create mods dir")?;
    Ok(root)
}

// ===== Профили =====

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

/// Сохранить текущее содержимое `Mods/` (включая индекс) как профиль.
pub fn save_profile(base: &Path, name: &str) -> Result<()> {
    let name = name.trim();
    anyhow::ensure!(valid_name(name), "invalid profile name");
    let _ = sync_index(base); // индекс в профиле должен отражать диск
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

    // Легаси-профиль без индекса: следующий sync_index мигрирует дерево.
    if let Some(g) = guid {
        if sync_index(base)?.mods.iter().all(|m| !m.enabled) {
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

/// Импорт перетащенных файлов/папок в `Mods/`:
/// папка становится модом (`Mods/<имя>/`), одиночные файлы складываются
/// в `Mods/Default/`. Возвращает количество элементов.
pub fn import_paths(base: &Path, paths: &[PathBuf]) -> Result<usize> {
    let root = base.join("Mods");
    std::fs::create_dir_all(&root)?;
    let mut n = 0usize;
    for src in paths {
        if !src.exists() {
            continue;
        }
        let Some(name) = src.file_name().map(|n| n.to_string_lossy().to_string()) else {
            continue;
        };
        if name.starts_with('.') || name == INDEX_FILE {
            continue;
        }
        if src.is_dir() {
            copy_tree(src, &root.join(&name), false)?;
            n += 1;
        } else {
            let dst = root.join(LEGACY_MOD).join(&name);
            if let Some(p) = dst.parent() {
                std::fs::create_dir_all(p)?;
            }
            std::fs::copy(src, &dst)?;
            n += 1;
        }
    }
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(base: &Path, rel: &str, data: &str) {
        let p = base.join("Mods").join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, data).unwrap();
    }

    /// Индекс как после первого открытия страницы: плоская миграция уже
    /// отработала (или нечего мигрировать), дальше папки = моды.
    fn sync_first(base: &Path) {
        sync_index(base).unwrap();
    }

    #[test]
    fn scan_empty_missing_dir() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(scan_mods(tmp.path()).is_empty());
        assert!(scan_enabled(tmp.path()).is_empty());
        // sync создаёт пустые Mods + индекс.
        assert!(tmp.path().join("Mods/mods.json").exists());
    }

    #[test]
    fn migration_wraps_flat_into_default() {
        let tmp = tempfile::tempdir().unwrap();
        let base = tmp.path();
        write(base, "content/a.txt", "flat");
        std::fs::create_dir_all(base.join("Mods/extra")).unwrap();
        std::fs::write(base.join("Mods/extra/b.txt"), "dir").unwrap();

        let idx = sync_index(base).unwrap();
        assert_eq!(idx.mods.len(), 1);
        assert_eq!(idx.mods[0].id, LEGACY_MOD);
        assert!(idx.mods[0].enabled);

        assert_eq!(
            std::fs::read_to_string(base.join("Mods/Default/content/a.txt")).unwrap(),
            "flat"
        );
        assert_eq!(
            std::fs::read_to_string(base.join("Mods/Default/extra/b.txt")).unwrap(),
            "dir"
        );
        assert!(!base.join("Mods/content").exists());

        let files = scan_enabled(base);
        assert_eq!(files.len(), 2);
        assert_eq!(files[0].rel, "Default/content/a.txt");
    }

    #[test]
    fn hidden_backup_not_migrated_or_listed() {
        let tmp = tempfile::tempdir().unwrap();
        let base = tmp.path();
        std::fs::create_dir_all(base.join("Mods/.backup")).unwrap();
        std::fs::write(base.join("Mods/.backup/x.txt"), "bak").unwrap();
        write(base, "visible.txt", "ok");

        let files = scan_enabled(base);
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].rel, "Default/visible.txt");
        assert!(base.join("Mods/.backup/x.txt").exists());
        assert!(!base.join("Mods/Default/.backup").exists());
    }

    #[test]
    fn only_enabled_mods_applied() {
        let tmp = tempfile::tempdir().unwrap();
        let base = tmp.path();
        let guid = "version-0000000000000000";
        sync_first(base);

        // Оригинал клиента (бэкап при первом apply).
        let vdir = base.join("Versions").join(guid).join("content/x.txt");
        std::fs::create_dir_all(vdir.parent().unwrap()).unwrap();
        std::fs::write(&vdir, "original").unwrap();

        write(base, "A/content/x.txt", "from-A");
        write(base, "B/content/x.txt", "from-B");
        sync_index(base).unwrap();
        set_enabled(base, "B", false).unwrap();

        let stats = apply(base, guid).unwrap();
        assert_eq!(stats.applied, 1);
        assert_eq!(
            std::fs::read_to_string(base.join("Versions").join(guid).join("content/x.txt")).unwrap(),
            "from-A"
        );

        // Переключаемся: A выкл, B вкл → после reapply версия содержит B.
        set_enabled(base, "A", false).unwrap();
        set_enabled(base, "B", true).unwrap();
        let _ = revert(base, guid).unwrap();
        apply(base, guid).unwrap();
        assert_eq!(
            std::fs::read_to_string(base.join("Versions").join(guid).join("content/x.txt")).unwrap(),
            "from-B"
        );
    }

    #[test]
    fn later_mod_overwrites_earlier() {
        let tmp = tempfile::tempdir().unwrap();
        let base = tmp.path();
        let guid = "version-0000000000000000";
        sync_first(base);

        // Оригинал клиента — бэкап должен сохранять именно его.
        let vdir = base.join("Versions").join(guid).join("content/x.txt");
        std::fs::create_dir_all(vdir.parent().unwrap()).unwrap();
        std::fs::write(&vdir, "original").unwrap();

        write(base, "A/content/x.txt", "first");
        write(base, "B/content/x.txt", "second");
        let idx = sync_index(base).unwrap();
        // Порядок по алфавиту: A затем B → выигрывает B.
        assert_eq!(idx.mods[0].id, "A");
        apply(base, guid).unwrap();
        assert_eq!(
            std::fs::read_to_string(base.join("Versions").join(guid).join("content/x.txt")).unwrap(),
            "second"
        );
        assert_eq!(
            std::fs::read_to_string(base.join("Mods/.backup/content/x.txt")).unwrap(),
            "original",
            "бэкап должен содержать оригинал клиента"
        );

        // Сдвигаем A ниже B → A применяется позже и выигрывает.
        move_mod(base, "A", 1).unwrap();
        let _ = revert(base, guid).unwrap();
        apply(base, guid).unwrap();
        assert_eq!(
            std::fs::read_to_string(base.join("Versions").join(guid).join("content/x.txt")).unwrap(),
            "first"
        );
    }

    #[test]
    fn set_enabled_and_move_mod_persist() {
        let tmp = tempfile::tempdir().unwrap();
        let base = tmp.path();
        sync_first(base);
        write(base, "A/a.txt", "1");
        write(base, "B/b.txt", "2");

        set_enabled(base, "A", false).unwrap();
        move_mod(base, "B", -1).unwrap();

        let idx = load_index(base);
        let a = idx.mods.iter().find(|m| m.id == "A").unwrap();
        let b = idx.mods.iter().find(|m| m.id == "B").unwrap();
        assert!(!a.enabled);
        assert_eq!(b.order, 0);
        assert_eq!(a.order, 1);

        assert!(set_enabled(base, "Ghost", true).is_err());
        assert!(move_mod(base, "Ghost", 1).is_err());
    }

    #[test]
    fn scan_and_apply_revert_roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        let base = tmp.path();
        let guid = "version-0000000000000000";

        // Клиент с оригиналом + плоский мод поверх (легаси-раскладка).
        let vdir = base.join("Versions").join(guid).join("content/test.txt");
        std::fs::create_dir_all(vdir.parent().unwrap()).unwrap();
        std::fs::write(&vdir, "original").unwrap();
        write(base, "content/test.txt", "modded");

        let files = scan_enabled(base);
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].rel, "Default/content/test.txt");
        assert_eq!(
            file_status(base, guid, "Default/content/test.txt"),
            FileStatus::Missing
        );

        let stats = apply(base, guid).unwrap();
        assert_eq!(stats.applied, 1);
        assert_eq!(
            file_status(base, guid, "Default/content/test.txt"),
            FileStatus::Applied
        );
        assert!(all_applied(base, guid).unwrap());

        let stats = revert(base, guid).unwrap();
        assert_eq!(stats.applied, 1);
        assert_eq!(std::fs::read_to_string(&vdir).unwrap(), "original");
    }

    #[test]
    fn profile_save_load_roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        let base = tmp.path();
        sync_first(base);

        write(base, "Pack/content/a.txt", "mod");
        save_profile(base, "pack").unwrap();

        // Меняем текущие моды: файл перезаписан + лишний добавлен.
        write(base, "Pack/content/a.txt", "changed");
        write(base, "Pack/extra.txt", "x");

        let stats = load_profile(base, None, "pack").unwrap();
        assert!(stats.is_none(), "без клиента применять нечего");
        assert_eq!(
            std::fs::read_to_string(base.join("Mods/Pack/content/a.txt")).unwrap(),
            "mod"
        );
        assert!(
            !base.join("Mods/Pack/extra.txt").exists(),
            "файл вне профиля должен исчезнуть"
        );
    }

    #[test]
    fn profile_save_skips_backup_and_lists() {
        let tmp = tempfile::tempdir().unwrap();
        let base = tmp.path();
        std::fs::create_dir_all(base.join("Mods/.backup")).unwrap();
        std::fs::write(base.join("Mods/.backup/orig.txt"), "orig").unwrap();
        write(base, "mod.txt", "mod");

        save_profile(base, "alpha").unwrap();
        save_profile(base, "beta").unwrap();

        assert!(!base.join("ModProfiles/alpha/.backup").exists());
        // Плоский файл мигрирует в Default при sync (save вызывает sync).
        assert!(base.join("ModProfiles/alpha/Default/mod.txt").exists());
        assert_eq!(
            list_profiles(base),
            vec!["alpha".to_string(), "beta".to_string()]
        );

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
        sync_first(base);

        // Оригинал клиента + мод A поверх.
        let vdir = base.join("Versions").join(guid).join("content/test.txt");
        std::fs::create_dir_all(vdir.parent().unwrap()).unwrap();
        std::fs::write(&vdir, "original").unwrap();
        write(base, "PackA/content/test.txt", "modA");
        save_profile(base, "packA").unwrap();

        apply(base, guid).unwrap();
        assert_eq!(std::fs::read_to_string(&vdir).unwrap(), "modA");

        // Профиль B меняет мод; после load версия содержит modB, бэкап свежий.
        write(base, "PackA/content/test.txt", "modB");
        save_profile(base, "packB").unwrap();
        load_profile(base, Some(guid), "packB").unwrap();
        assert_eq!(std::fs::read_to_string(&vdir).unwrap(), "modB");
        assert_eq!(
            std::fs::read_to_string(base.join("Mods/.backup/content/test.txt")).unwrap(),
            "original",
            "бэкап должен содержать оригинал клиента"
        );
    }

    #[test]
    fn import_paths_creates_mod_folders() {
        let tmp = tempfile::tempdir().unwrap();
        let base = tmp.path();

        let pack = tmp.path().join("MyPack");
        std::fs::create_dir_all(pack.join("content")).unwrap();
        std::fs::write(pack.join("content/a.txt"), "from-pack").unwrap();
        let loose = tmp.path().join("single.txt");
        std::fs::write(&loose, "from-file").unwrap();

        std::fs::create_dir_all(base.join("Mods/Default")).unwrap();
        std::fs::write(base.join("Mods/Default/keep.txt"), "keep").unwrap();

        let n = import_paths(base, &[pack.clone(), loose.clone()]).unwrap();
        assert_eq!(n, 2);
        assert_eq!(
            std::fs::read_to_string(base.join("Mods/MyPack/content/a.txt")).unwrap(),
            "from-pack"
        );
        assert_eq!(
            std::fs::read_to_string(base.join("Mods/Default/single.txt")).unwrap(),
            "from-file"
        );
        assert_eq!(
            std::fs::read_to_string(base.join("Mods/Default/keep.txt")).unwrap(),
            "keep"
        );

        // Индекс и скрытые файлы не импортируются.
        let idx = base.join("Mods/mods.json");
        let n = import_paths(base, &[idx, tmp.path().join("ghost")]).unwrap();
        assert_eq!(n, 0);
    }
}
