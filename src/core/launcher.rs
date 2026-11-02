use std::path::{Path, PathBuf};

use super::deploy::PLAYER_EXE;

pub fn exe_path(base: &Path, guid: &str) -> Option<PathBuf> {
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
