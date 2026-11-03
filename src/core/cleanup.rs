//! Очистка временных файлов: оборванные загрузки, логи Roblox, системный temp.

use std::path::Path;

/// Итог очистки.
#[derive(Debug, Default, Clone, Copy)]
pub struct Report {
    pub freed: u64,
    pub files: usize,
}

fn dir_stats(path: &Path) -> (u64, usize) {
    let mut freed = 0u64;
    let mut files = 0usize;
    if let Ok(rd) = std::fs::read_dir(path) {
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                let (f, n) = dir_stats(&p);
                freed += f;
                files += n;
            } else if let Ok(m) = e.metadata() {
                freed += m.len();
                files += 1;
            }
        }
    }
    (freed, files)
}

fn remove_path(path: &Path, rep: &mut Report) {
    if !path.exists() {
        return;
    }
    if path.is_dir() {
        let (f, n) = dir_stats(path);
        rep.freed += f;
        rep.files += n;
        let _ = std::fs::remove_dir_all(path);
    } else if let Ok(m) = path.metadata() {
        rep.freed += m.len();
        rep.files += 1;
        let _ = std::fs::remove_file(path);
    }
}

/// Удаляет мусор из базы лаунчера и системы.
///
/// Занятые файлы (например, логи запущенного Roblox) пропускаются без ошибки.
pub fn clean(base: &Path) -> Report {
    let logs = crate::core::playtime::logs_dir();
    let temp_roblox = std::env::temp_dir().join("Roblox");
    clean_all(base, &logs, &temp_roblox)
}

fn clean_all(base: &Path, roblox_logs: &Path, temp_roblox: &Path) -> Report {
    let mut rep = Report::default();
    // Стадия загрузки — в норме удаляется после установки.
    remove_path(&base.join("Downloads"), &mut rep);
    remove_path(&base.join(".clientsettings-tmp"), &mut rep);
    // Логи клиента (Roblox пересоздаст папку при следующем запуске).
    remove_path(roblox_logs, &mut rep);
    // Системный temp Roblox.
    remove_path(temp_roblox, &mut rep);
    rep
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_removes_targets_and_counts() {
        let tmp = tempfile::tempdir().unwrap();
        let base = tmp.path();

        std::fs::create_dir_all(base.join("Downloads/x")).unwrap();
        std::fs::write(base.join("Downloads/x/a.zip"), vec![7u8; 100]).unwrap();
        std::fs::write(base.join(".clientsettings-tmp"), "data").unwrap();
        std::fs::create_dir_all(base.join("fake_logs")).unwrap();
        std::fs::write(base.join("fake_logs/l.log"), vec![1u8; 64]).unwrap();
        std::fs::create_dir_all(base.join("fake_tmp")).unwrap();
        std::fs::write(base.join("fake_tmp/t.bin"), vec![2u8; 32]).unwrap();

        let rep = clean_all(base, &base.join("fake_logs"), &base.join("fake_tmp"));

        assert!(!base.join("Downloads").exists());
        assert!(!base.join(".clientsettings-tmp").exists());
        assert!(!base.join("fake_logs").exists());
        assert!(!base.join("fake_tmp").exists());
        assert_eq!(rep.freed, 100 + 4 + 64 + 32);
        assert_eq!(rep.files, 4);
    }

    #[test]
    fn clean_missing_targets_is_ok() {
        let tmp = tempfile::tempdir().unwrap();
        let rep = clean_all(tmp.path(), &tmp.path().join("nope"), &tmp.path().join("nope2"));
        assert_eq!(rep.freed, 0);
        assert_eq!(rep.files, 0);
    }
}
