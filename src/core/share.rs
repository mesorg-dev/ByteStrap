//! Шаринг профилей между машинами: экспорт/импорт профилей флагов (JSON)
//! и профилей модов (zip с манифестом).
#![cfg_attr(not(windows), allow(dead_code))]

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::path::Path;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

use crate::core::flags;

const FLAG_FORMAT: &str = "bytestrap-flag-profile";
const MOD_FORMAT: &str = "bytestrap-mod-profile";
const MANIFEST: &str = "bytestrap-profile.json";
const FORMAT_VERSION: u32 = 1;

/// Уникальное имя: при совпадении добавляет « 2», « 3», …
fn unique_name<T: Fn(&str) -> bool>(exists: T, name: &str) -> String {
    let base = name.trim();
    let base = if base.is_empty() { "profile" } else { base };
    if !exists(base) {
        return base.to_string();
    }
    for i in 2..10_000 {
        let n = format!("{base} {i}");
        if !exists(&n) {
            return n;
        }
    }
    format!("{base} 10000")
}

// ===== Профили флагов =====

#[derive(Debug, Serialize, Deserialize)]
struct FlagBundle {
    format: String,
    version: u32,
    name: String,
    flags: BTreeMap<String, serde_json::Value>,
}

/// Экспорт профиля флагов в JSON-файл.
pub fn export_flag_profile(base: &Path, name: &str, path: &Path) -> Result<()> {
    let profiles = flags::load_profiles(base);
    let flags = profiles
        .profiles
        .get(name)
        .with_context(|| format!("no such profile: {name}"))?;
    let bundle = FlagBundle {
        format: FLAG_FORMAT.to_string(),
        version: FORMAT_VERSION,
        name: name.to_string(),
        flags: flags.clone(),
    };
    std::fs::write(path, serde_json::to_string_pretty(&bundle)?)?;
    Ok(())
}

/// Импорт профиля флагов из JSON-файла (с манифестом или «голым» объектом
/// флагов). Возвращает имя профиля (при совпадении — с числовым суффиксом).
pub fn import_flag_profile(base: &Path, path: &Path) -> Result<String> {
    let text = std::fs::read_to_string(path)?;
    let value: serde_json::Value = serde_json::from_str(&text).context("invalid JSON")?;

    let (name, flags) = if value.get("format").and_then(|f| f.as_str()) == Some(FLAG_FORMAT) {
        let bundle: FlagBundle = serde_json::from_value(value)?;
        if bundle.version != FORMAT_VERSION {
            bail!("unsupported format version");
        }
        (bundle.name, bundle.flags)
    } else {
        let obj = value
            .as_object()
            .ok_or_else(|| anyhow::anyhow!("expected a JSON object"))?;
        let stem = path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "profile".to_string());
        (stem, obj.clone().into_iter().collect())
    };

    let mut profiles = flags::load_profiles(base);
    let name = unique_name(|n| profiles.profiles.contains_key(n), &name);
    profiles.profiles.insert(name.clone(), flags);
    flags::save_profiles(base, &profiles)?;
    Ok(name)
}

// ===== Профили модов (zip) =====

#[derive(Debug, Serialize, Deserialize)]
struct ModManifest {
    format: String,
    version: u32,
    name: String,
}

/// Экспорт профиля модов в zip: манифест + содержимое `ModProfiles/<name>/`.
pub fn export_mod_profile(base: &Path, name: &str, path: &Path) -> Result<()> {
    let src = base.join("ModProfiles").join(name);
    anyhow::ensure!(src.is_dir(), "no such profile: {name}");

    let file = std::fs::File::create(path)?;
    let mut zip = zip::ZipWriter::new(file);
    let opts = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);

    let manifest = ModManifest {
        format: MOD_FORMAT.to_string(),
        version: FORMAT_VERSION,
        name: name.to_string(),
    };
    zip.start_file(MANIFEST, opts)?;
    zip.write_all(serde_json::to_string_pretty(&manifest)?.as_bytes())?;

    // Дерево профиля (скрытые компоненты пропускаем — в профиле их нет).
    let mut stack = vec![(src, String::new())];
    while let Some((dir, prefix)) = stack.pop() {
        for entry in std::fs::read_dir(&dir)? {
            let entry = entry?;
            let fname = entry.file_name().to_string_lossy().to_string();
            if fname.starts_with('.') {
                continue;
            }
            let rel = if prefix.is_empty() {
                fname.clone()
            } else {
                format!("{prefix}/{fname}")
            };
            let p = entry.path();
            if p.is_dir() {
                stack.push((p, rel));
            } else {
                zip.start_file(&rel, opts)?;
                let data = std::fs::read(&p)?;
                zip.write_all(&data)?;
            }
        }
    }
    zip.finish()?;
    Ok(())
}

/// Импорт профиля модов из zip. Возвращает имя профиля (при совпадении —
/// с числовым суффиксом).
pub fn import_mod_profile(base: &Path, path: &Path) -> Result<String> {
    let file = std::fs::File::open(path)?;
    let mut archive = zip::ZipArchive::new(file).context("not a zip archive")?;

    // Манифест (необязателен — имя берётся из имени файла).
    let mut manifest_name: Option<String> = None;
    if let Ok(mut m) = archive.by_name(MANIFEST) {
        let mut text = String::new();
        m.read_to_string(&mut text)?;
        let man: ModManifest = serde_json::from_str(&text).context("bad manifest")?;
        if man.format != MOD_FORMAT || man.version != FORMAT_VERSION {
            bail!("not a ByteStrap mod profile");
        }
        manifest_name = Some(man.name);
    }

    let profiles_root = base.join("ModProfiles");
    std::fs::create_dir_all(&profiles_root)?;
    let existing = |n: &str| profiles_root.join(n).exists();
    let name = manifest_name
        .as_deref()
        .filter(|n| !n.trim().is_empty())
        .map(|n| n.trim().to_string())
        .map(|n| unique_name(&existing, &n))
        .unwrap_or_else(|| {
            let stem = path
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "profile".to_string());
            unique_name(&existing, &stem)
        });
    let dst = profiles_root.join(&name);
    std::fs::create_dir_all(&dst)?;

    // Распаковка с защитой от zip-slip: enclosed_name() отбрасывает «..» и абсолютные.
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        let Some(rel) = entry.enclosed_name().map(|p| p.to_path_buf()) else {
            continue;
        };
        if entry.is_dir() || rel.as_os_str().is_empty() {
            continue;
        }
        let out_path = dst.join(&rel);
        if let Some(parent) = out_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut out = std::fs::File::create(&out_path)?;
        std::io::copy(&mut entry, &mut out)?;
    }

    // Zip, снятый с папки целиком: единственный каталог-обёртка с индексом
    // внутри → разворачиваем содержимое на уровень профиля.
    flatten_wrapper(&dst)?;

    let valid = dst.join("mods.json").exists()
        || std::fs::read_dir(&dst)?.any(|e| e.map(|e| e.path().is_dir()).unwrap_or(false));
    if !valid {
        let _ = std::fs::remove_dir_all(&dst);
        bail!("zip does not contain a mod profile");
    }
    // Манифест в содержимом профиля не нужен.
    let _ = std::fs::remove_file(dst.join(MANIFEST));
    Ok(name)
}

/// Если в профиле ровно один каталог и в нём лежит индекс — поднять его.
fn flatten_wrapper(dst: &Path) -> Result<()> {
    if dst.join("mods.json").exists() {
        return Ok(());
    }
    let mut dirs = Vec::new();
    let mut files = 0usize;
    for e in std::fs::read_dir(dst)? {
        let e = e?;
        if e.path().is_dir() {
            dirs.push(e.path());
        } else {
            files += 1;
        }
    }
    if files == 0 && dirs.len() == 1 && dirs[0].join("mods.json").exists() {
        let inner = &dirs[0];
        for e in std::fs::read_dir(inner)? {
            let e = e?;
            std::fs::rename(e.path(), dst.join(e.file_name()))?;
        }
        std::fs::remove_dir(inner)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::mods;

    fn write_file(p: &Path, data: &str) {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, data).unwrap();
    }

    #[test]
    fn flag_profile_roundtrip() {
        let src = tempfile::tempdir().unwrap();
        let dst = tempfile::tempdir().unwrap();

        let mut profiles = flags::load_profiles(src.path());
        profiles.profiles.insert(
            "MyPack".into(),
            [("FFlagTest".to_string(), serde_json::Value::Bool(true))]
                .into_iter()
                .collect(),
        );
        flags::save_profiles(src.path(), &profiles).unwrap();

        let file = src.path().join("out.json");
        export_flag_profile(src.path(), "MyPack", &file).unwrap();

        let name = import_flag_profile(dst.path(), &file).unwrap();
        assert_eq!(name, "MyPack");
        let got = flags::load_profiles(dst.path());
        assert_eq!(
            got.profiles["MyPack"]["FFlagTest"],
            serde_json::Value::Bool(true)
        );
    }

    #[test]
    fn flag_import_bare_object_uses_file_stem() {
        let dst = tempfile::tempdir().unwrap();
        let file = dst.path().join("coolflags.json");
        std::fs::write(&file, r#"{"DFIntTaskSchedulerTargetFps": 144}"#).unwrap();

        let name = import_flag_profile(dst.path(), &file).unwrap();
        assert_eq!(name, "coolflags");
        let got = flags::load_profiles(dst.path());
        assert_eq!(
            got.profiles["coolflags"]["DFIntTaskSchedulerTargetFps"],
            serde_json::Value::Number(144.into())
        );
    }

    #[test]
    fn flag_import_conflict_gets_suffix() {
        let dst = tempfile::tempdir().unwrap();
        let mut profiles = flags::load_profiles(dst.path());
        profiles.profiles.insert("Pack".into(), Default::default());
        flags::save_profiles(dst.path(), &profiles).unwrap();

        let file = dst.path().join("Pack.json");
        std::fs::write(&file, r#"{"format":"bytestrap-flag-profile","version":1,"name":"Pack","flags":{}}"#)
            .unwrap();
        let name = import_flag_profile(dst.path(), &file).unwrap();
        assert_eq!(name, "Pack 2");
    }

    #[test]
    fn mod_profile_zip_roundtrip() {
        let src = tempfile::tempdir().unwrap();
        let dst = tempfile::tempdir().unwrap();

        mods::sync_index(src.path()).unwrap();
        write_file(&src.path().join("Mods/MyMod/content/a.txt"), "hello");
        mods::sync_index(src.path()).unwrap();
        mods::save_profile(src.path(), "Pack").unwrap();

        let zip_path = src.path().join("Pack.zip");
        export_mod_profile(src.path(), "Pack", &zip_path).unwrap();

        let name = import_mod_profile(dst.path(), &zip_path).unwrap();
        assert_eq!(name, "Pack");
        assert!(dst.path().join("ModProfiles/Pack/mods.json").exists());
        assert_eq!(
            std::fs::read_to_string(dst.path().join("ModProfiles/Pack/MyMod/content/a.txt")).unwrap(),
            "hello"
        );
    }

    #[test]
    fn mod_import_conflict_gets_suffix() {
        let src = tempfile::tempdir().unwrap();
        let dst = tempfile::tempdir().unwrap();

        mods::sync_index(src.path()).unwrap();
        write_file(&src.path().join("Mods/MyMod/content/a.txt"), "hello");
        mods::sync_index(src.path()).unwrap();
        mods::save_profile(src.path(), "Pack").unwrap();
        let zip_path = src.path().join("Pack.zip");
        export_mod_profile(src.path(), "Pack", &zip_path).unwrap();

        std::fs::create_dir_all(dst.path().join("ModProfiles/Pack")).unwrap();
        let name = import_mod_profile(dst.path(), &zip_path).unwrap();
        assert_eq!(name, "Pack 2");
        assert!(dst.path().join("ModProfiles/Pack 2/mods.json").exists());
    }

    #[test]
    fn mod_import_rejects_foreign_zip() {
        let dst = tempfile::tempdir().unwrap();
        let zip_path = dst.path().join("junk.zip");
        let file = std::fs::File::create(&zip_path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let opts = zip::write::SimpleFileOptions::default();
        zip.start_file("readme.txt", opts).unwrap();
        zip.write_all(b"hello").unwrap();
        zip.finish().unwrap();

        assert!(import_mod_profile(dst.path(), &zip_path).is_err());
        assert!(!dst.path().join("ModProfiles/junk").exists());
    }

    #[test]
    fn mod_import_rejects_traversal_entries() {
        let dst = tempfile::tempdir().unwrap();
        let zip_path = dst.path().join("evil.zip");
        let file = std::fs::File::create(&zip_path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let opts = zip::write::SimpleFileOptions::default();
        zip.start_file("../escape.txt", opts).unwrap();
        zip.write_all(b"evil").unwrap();
        zip.start_file("mods.json", opts).unwrap();
        zip.write_all(br#"{"version":1,"mods":[]}"#).unwrap();
        zip.finish().unwrap();

        let name = import_mod_profile(dst.path(), &zip_path).unwrap();
        assert!(!dst.path().join("escape.txt").exists());
        assert!(dst.path().parent().unwrap().join("escape.txt").exists() == false);
        assert!(dst.path().join("ModProfiles").join(&name).join("mods.json").exists());
    }
}
