use std::path::{Path, PathBuf};

use super::deploy::PLAYER_EXE;

pub fn exe_path(base: &Path, guid: &str) -> Option<PathBuf> {
    let client_exe = base
        .join("Versions")
        .join(super::versions::STATIC_VERSION_DIR)
        .join(PLAYER_EXE);
    if client_exe.exists() {
        return Some(client_exe);
    }
    if !super::versions::is_version_guid(guid) {
        return None;
    }
    let exe = base.join("Versions").join(guid).join(PLAYER_EXE);
    exe.exists().then_some(exe)
}

/// Запуск плеера. `arg` — опциональная ссылка вида `roblox://placeId=...`.
#[cfg(windows)]
pub fn launch(exe: &Path, arg: Option<&str>) -> std::io::Result<()> {
    use std::process::{Command, Stdio};

    let mut cmd = Command::new(exe);
    cmd.current_dir(exe.parent().unwrap_or(Path::new(".")))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if let Some(a) = arg {
        cmd.arg(a);
    }
    cmd.spawn()?;
    Ok(())
}

#[cfg(not(windows))]
pub fn launch(_exe: &Path, _arg: Option<&str>) -> std::io::Result<()> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "запуск клиента доступен только на Windows",
    ))
}

pub const STUDIO_EXE: &str = "RobloxStudioBeta.exe";
pub const STUDIO_LAUNCHER_EXE: &str = "RobloxStudioLauncherBeta.exe";
pub const STUDIO_LAUNCHER_URL: &str = "https://setup.rbxcdn.com/RobloxStudioLauncherBeta.exe";

/// Поиск установленного исполняемого файла Roblox Studio.
pub fn find_studio_exe(base: &Path) -> Option<PathBuf> {
    // 1. В папке версий ByteStrap
    let candidates = [
        base.join("Versions").join("Studio").join(STUDIO_EXE),
        base.join("Versions").join(super::versions::STATIC_VERSION_DIR).join(STUDIO_EXE),
    ];
    for c in &candidates {
        if c.exists() {
            return Some(c.clone());
        }
    }

    // 2. В стандартных директориях Roblox
    if let Some(local) = dirs::data_local_dir() {
        let versions_dir = local.join("Roblox").join("Versions");
        if let Ok(entries) = std::fs::read_dir(&versions_dir) {
            for entry in entries.flatten() {
                let exe = entry.path().join(STUDIO_EXE);
                if exe.exists() {
                    return Some(exe);
                }
            }
        }
    }

    // 3. В Program Files
    for pf in ["ProgramFiles(x86)", "ProgramFiles"] {
        if let Ok(dir) = std::env::var(pf) {
            let versions_dir = Path::new(&dir).join("Roblox").join("Versions");
            if let Ok(entries) = std::fs::read_dir(&versions_dir) {
                for entry in entries.flatten() {
                    let exe = entry.path().join(STUDIO_EXE);
                    if exe.exists() {
                        return Some(exe);
                    }
                }
            }
        }
    }

    None
}

/// Запуск или автоматическая загрузка и установка Roblox Studio.
#[cfg(windows)]
pub fn launch_or_install_studio(base: &Path) -> std::io::Result<()> {
    if let Some(exe) = find_studio_exe(base) {
        return launch(&exe, Some("-ide"));
    }

    // Скачивание официального установщика RobloxStudioLauncherBeta.exe
    let cache_dir = base.join("Cache");
    std::fs::create_dir_all(&cache_dir)?;
    let installer = cache_dir.join(STUDIO_LAUNCHER_EXE);

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build();
    if let Ok(rt) = rt {
        let _ = rt.block_on(async {
            if let Ok(resp) = reqwest::get(STUDIO_LAUNCHER_URL).await {
                if let Ok(bytes) = resp.bytes().await {
                    let _ = std::fs::write(&installer, &bytes);
                }
            }
        });
    }

    if installer.exists() {
        launch(&installer, Some("-ide"))
    } else {
        Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "Не удалось загрузить RobloxStudioLauncherBeta.exe",
        ))
    }
}

#[cfg(not(windows))]
pub fn launch_or_install_studio(_base: &Path) -> std::io::Result<()> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "Roblox Studio доступен только на Windows",
    ))
}

#[allow(dead_code)] // search/history — этап «поиска мест»
pub fn place_url(place_id: u64) -> String {
    format!("roblox://placeId={place_id}")
}

#[allow(dead_code)] // search/history — этап «поиска мест»
pub fn server_url(place_id: u64, job_id: &str) -> String {
    if job_id.is_empty() {
        place_url(place_id)
    } else {
        format!("roblox://placeId={place_id}&gameInstanceId={job_id}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exe_path_finds_static_client() {
        let td = tempfile::tempdir().unwrap();
        let base = td.path();
        let client_dir = base
            .join("Versions")
            .join(super::super::versions::STATIC_VERSION_DIR);
        std::fs::create_dir_all(&client_dir).unwrap();
        let client_exe = client_dir.join(PLAYER_EXE);
        std::fs::write(&client_exe, b"dummy").unwrap();

        assert_eq!(exe_path(base, "whatever"), Some(client_exe));
    }
}
