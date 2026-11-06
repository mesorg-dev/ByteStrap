//! Регистрация протокола `roblox://` в реестре Windows.
//!
//! `HKCU\Software\Classes\roblox` (без админа): клики по ссылкам — из Discord,
//! браузера, друзей, приватных серверов — открывают игру через ByteStrap.
//! Команда: `"<exe>" "%1"`, вся ссылка уходит плееру как есть
//! (`RobloxPlayerBeta.exe roblox://placeId=...` — клиент сам её разбирает).

#[cfg(windows)]
mod imp {
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;

    const KEY: &str = r"Software\Classes\roblox";

    fn exe_command() -> std::io::Result<String> {
        let exe = std::env::current_exe()?;
        Ok(format!("\"{}\" \"%1\"", exe.display()))
    }

    /// Текущая команда в реестре (для теста: сохранить/вернуть).
    fn current_command() -> Option<String> {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let key = hkcu.open_subkey(KEY).ok()?;
        let shell = key.open_subkey(r"shell\open\command").ok()?;
        shell.get_value::<String, _>("").ok()
    }

    /// Прописать `roblox://` на текущий exe.
    pub fn register() -> std::io::Result<()> {
        let cmd = exe_command()?;
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let (key, _) = hkcu.create_subkey(KEY)?;
        key.set_value("URL Protocol", &"")?;
        key.set_value("", &"URL:Roblox")?;
        let (shell, _) = key.create_subkey(r"shell\open\command")?;
        shell.set_value("", &cmd)?;
        Ok(())
    }

    /// Убрать регистрацию (выключили тумблер).
    pub fn unregister() -> std::io::Result<()> {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        match hkcu.delete_subkey_all(KEY) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e),
        }
    }

    /// Прописан ли именно текущий exe (чужая регистрация / exe переехал).
    pub fn is_registered() -> bool {
        let Ok(expected) = exe_command() else {
            return false;
        };
        current_command().as_deref() == Some(expected.as_str())
    }

    /// Самовосстановление при старте: ключ удалён или путь изменился.
    pub fn ensure() {
        if !is_registered() {
            let _ = register();
        }
    }

    /// Состояние для UI: `Some(true/false)` (None — не Windows).
    pub fn status() -> Option<bool> {
        Some(is_registered())
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn registry_roundtrip_restores_previous() {
            let before = current_command();
            register().unwrap();
            assert!(is_registered());
            unregister().unwrap();
            assert!(!is_registered());
            // Вернуть, как было (машина разработчика / runner).
            if let Some(cmd) = before {
                let hkcu = RegKey::predef(HKEY_CURRENT_USER);
                let (key, _) = hkcu.create_subkey(KEY).unwrap();
                key.set_value("URL Protocol", &"").unwrap();
                let (shell, _) = key.create_subkey(r"shell\open\command").unwrap();
                shell.set_value("", &cmd).unwrap();
            }
        }
    }
}

#[cfg(not(windows))]
mod imp {
    pub fn register() -> std::io::Result<()> {
        Ok(())
    }
    pub fn unregister() -> std::io::Result<()> {
        Ok(())
    }
    pub fn ensure() {}
    pub fn status() -> Option<bool> {
        None
    }
}

pub use imp::*;
