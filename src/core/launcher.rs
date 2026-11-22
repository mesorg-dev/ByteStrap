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
