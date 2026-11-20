//! Менеджер аккаунтов Roblox: хранение `.ROBLOSECURITY` и запуск клиента
//! от имени выбранного аккаунта (auth-ticket → ссылка `roblox-player:`).
//!
//! Куки шифруются через Windows DPAPI (CryptProtectData/CryptUnprotectData)
//! перед сохранением в `accounts.json` с префиксом `dpapi:<hex>`.
//! В памяти кука расшифрована только для сетевых запросов к Roblox API.

use std::path::Path;

use serde::{Deserialize, Serialize};

const UA: &str = "ByteStrap/1.0 (+https://github.com/mesorg-dev/ByteStrap)";

#[cfg(windows)]
pub mod dpapi {
    use std::ptr;

    #[repr(C)]
    struct DataBlob {
        cb_data: u32,
        pb_data: *mut u8,
    }

    #[link(name = "crypt32")]
    extern "system" {
        fn CryptProtectData(
            p_data_in: *const DataBlob,
            sz_data_descr: *const u16,
            p_optional_entropy: *const DataBlob,
            pv_reserved: *mut std::ffi::c_void,
            p_prompt_struct: *mut std::ffi::c_void,
            dw_flags: u32,
            p_data_out: *mut DataBlob,
        ) -> i32;

        fn CryptUnprotectData(
            p_data_in: *const DataBlob,
            ppsz_data_descr: *mut *mut u16,
            p_optional_entropy: *const DataBlob,
            pv_reserved: *mut std::ffi::c_void,
            p_prompt_struct: *mut std::ffi::c_void,
            dw_flags: u32,
            p_data_out: *mut DataBlob,
        ) -> i32;
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn LocalFree(h_mem: *mut std::ffi::c_void) -> *mut std::ffi::c_void;
    }

    const CRYPTPROTECT_UI_FORBIDDEN: u32 = 0x1;

    pub fn encrypt_string(plaintext: &str) -> Result<String, String> {
        let bytes = plaintext.as_bytes();
        let in_blob = DataBlob {
            cb_data: bytes.len() as u32,
            pb_data: bytes.as_ptr() as *mut u8,
        };
        let mut out_blob = DataBlob {
            cb_data: 0,
            pb_data: ptr::null_mut(),
        };

        let res = unsafe {
            CryptProtectData(
                &in_blob,
                ptr::null(),
                ptr::null(),
                ptr::null_mut(),
                ptr::null_mut(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut out_blob,
            )
        };

        if res == 0 || out_blob.pb_data.is_null() {
            return Err("CryptProtectData failed".into());
        }

        let slice = unsafe {
            std::slice::from_raw_parts(out_blob.pb_data, out_blob.cb_data as usize)
        };
        let hex = hex_encode(slice);

        unsafe {
            LocalFree(out_blob.pb_data as *mut _);
        }

        Ok(format!("dpapi:{hex}"))
    }

    pub fn decrypt_string(encrypted: &str) -> Result<String, String> {
        let hex = encrypted.strip_prefix("dpapi:").unwrap_or(encrypted);
        let Some(bytes) = hex_decode(hex) else {
            return Err("Invalid hex in encrypted cookie".into());
        };

        let in_blob = DataBlob {
            cb_data: bytes.len() as u32,
            pb_data: bytes.as_ptr() as *mut u8,
        };
        let mut out_blob = DataBlob {
            cb_data: 0,
            pb_data: ptr::null_mut(),
        };

        let res = unsafe {
            CryptUnprotectData(
                &in_blob,
                ptr::null_mut(),
                ptr::null(),
                ptr::null_mut(),
                ptr::null_mut(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut out_blob,
            )
        };

        if res == 0 || out_blob.pb_data.is_null() {
            return Err("CryptUnprotectData failed".into());
        }

        let slice = unsafe {
            std::slice::from_raw_parts(out_blob.pb_data, out_blob.cb_data as usize)
        };
        let decrypted = String::from_utf8(slice.to_vec())
            .map_err(|e| format!("UTF-8 decode failed: {e}"))?;

        unsafe {
            LocalFree(out_blob.pb_data as *mut _);
        }

        Ok(decrypted)
    }

    fn hex_encode(bytes: &[u8]) -> String {
        let mut s = String::with_capacity(bytes.len() * 2);
        for b in bytes {
            use std::fmt::Write;
            let _ = write!(s, "{:02x}", b);
        }
        s
    }

    fn hex_decode(s: &str) -> Option<Vec<u8>> {
        if s.len() % 2 != 0 {
            return None;
        }
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok())
            .collect()
    }
}

#[cfg(not(windows))]
pub mod dpapi {
    pub fn encrypt_string(plaintext: &str) -> Result<String, String> {
        Ok(plaintext.to_string())
    }
    pub fn decrypt_string(encrypted: &str) -> Result<String, String> {
        Ok(encrypted.to_string())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Account {
    /// Стабильный идентификатор (для активного выбора).
    pub id: String,
    /// Отображаемое имя (username из Roblox API).
    pub name: String,
    /// Значение `.ROBLOSECURITY` без префикса.
    pub cookie: String,
    /// Unix-время добавления.
    pub added: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Accounts {
    pub accounts: Vec<Account>,
    /// id активного аккаунта (пусто — без входа).
    pub active: String,
}

impl Accounts {
    pub fn file_path(base: &Path) -> std::path::PathBuf {
        base.join("accounts.json")
    }

    pub fn load(base: &Path) -> Self {
        let mut loaded: Self = match std::fs::read_to_string(Self::file_path(base)) {
            Ok(s) => serde_json::from_str(&s).unwrap_or_default(),
            Err(_) => Self::default(),
        };

        // Расшифровка куки из DPAPI в память
        for a in &mut loaded.accounts {
            if a.cookie.starts_with("dpapi:") {
                match dpapi::decrypt_string(&a.cookie) {
                    Ok(plain) => a.cookie = plain,
                    Err(e) => {
                        crate::core::logger::warn(format!("Ошибка DPAPI расшифровки для аккаунта {}: {e}", a.name));
                    }
                }
            }
        }

        loaded
    }

    pub fn save(&self, base: &Path) {
        let path = Self::file_path(base);
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        // Зашифровать куки через DPAPI перед записью на диск
        let mut disk_accs = self.clone();
        for a in &mut disk_accs.accounts {
            if !a.cookie.is_empty() && !a.cookie.starts_with("dpapi:") {
                if let Ok(enc) = dpapi::encrypt_string(&a.cookie) {
                    a.cookie = enc;
                }
            }
        }

        if let Ok(s) = serde_json::to_string_pretty(&disk_accs) {
            let _ = std::fs::write(path, s);
        }
    }

    pub fn active(&self) -> Option<&Account> {
        if self.active.is_empty() {
            return None;
        }
        self.accounts.iter().find(|a| a.id == self.active)
    }

    /// Добавить аккаунт по куке (без дублей). Возвращает id.
    pub fn add(&mut self, cookie: &str, name: &str) -> String {
        let cookie = normalize_cookie(cookie);
        if let Some(a) = self.accounts.iter().find(|a| a.cookie == cookie) {
            if self.active.is_empty() {
                self.active = a.id.clone();
            }
            return a.id.clone();
        }
        let id = format!("acc{}", now_unix());
        self.accounts.push(Account {
            id: id.clone(),
            name: name.to_string(),
            cookie,
            added: now_unix(),
        });
        if self.active.is_empty() {
            self.active = id.clone();
        }
        id
    }

    pub fn remove(&mut self, id: &str) {
        self.accounts.retain(|a| a.id != id);
        if self.active == id {
            self.active = self.accounts.first().map(|a| a.id.clone()).unwrap_or_default();
        }
    }
}

fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Убрать префикс `.ROBLOSECURITY=` и лишние пробелы.
pub fn normalize_cookie(raw: &str) -> String {
    let t = raw.trim();
    let t = t.strip_prefix(".ROBLOSECURITY=").unwrap_or(t);
    t.trim().to_string()
}

/// Получить username аккаунта по куке (проверка валидности).
pub async fn fetch_username(cookie: &str) -> Option<String> {
    let client = reqwest::Client::new();
    let resp = client
        .get("https://users.roblox.com/v1/users/authenticated")
        .header("Cookie", format!(".ROBLOSECURITY={cookie}"))
        .header("User-Agent", UA)
        .send()
        .await
        .ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let v: serde_json::Value = resp.json().await.ok()?;
    v.get("name").and_then(|n| n.as_str()).map(str::to_string)
}

/// Получить authentication-ticket для запуска клиента (обработка CSRF-403).
pub async fn auth_ticket(cookie: &str) -> anyhow::Result<String> {
    let client = reqwest::Client::new();
    let cookie_hdr = format!(".ROBLOSECURITY={cookie}");
    let post = |csrf: Option<String>| {
        let mut req = client
            .post("https://auth.roblox.com/v1/authentication-ticket")
            .header("Cookie", cookie_hdr.clone())
            .header("Referer", "https://www.roblox.com/")
            .header("User-Agent", UA)
            .header("Content-Length", "0");
        if let Some(c) = csrf {
            req = req.header("x-csrf-token", c);
        }
        req.send()
    };

    let mut resp = post(None).await?;
    if resp.status().as_u16() == 403 {
        let csrf = resp
            .headers()
            .get("x-csrf-token")
            .and_then(|v| v.to_str().ok())
            .map(str::to_string);
        if let Some(csrf) = csrf {
            resp = post(Some(csrf)).await?;
        }
    }
    if !resp.status().is_success() {
        anyhow::bail!("HTTP {}", resp.status());
    }
    resp.headers()
        .get("rbx-authentication-ticket")
        .and_then(|v| v.to_str().ok())
        .map(str::to_string)
        .ok_or_else(|| anyhow::anyhow!("нет authentication-ticket в ответе"))
}

/// Собрать ссылку запуска `roblox-player:` с тикетом аккаунта.
/// `place` — если задан, клиент сразу зайдёт в этот плейс;
/// `job` — если задан JobId сервера, вход в конкретный сервер.
pub fn launch_uri(ticket: &str, place: Option<u64>, job: Option<&str>) -> String {
    let ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let btid = ms as u64 % 9_000_000_000 + 1_000_000_000;
    let place_url = match (place, job) {
        (Some(id), Some(j)) => format!(
            "https://assetgame.roblox.com/game/PlaceLauncher.ashx?request=RequestGameJob&browserTrackerId={btid}&placeId={id}&gameId={j}&isPlayTogetherGame=false"
        ),
        (Some(id), None) => format!(
            "https://assetgame.roblox.com/game/PlaceLauncher.ashx?request=RequestGame&browserTrackerId={btid}&placeId={id}&isPlayTogetherGame=false"
        ),
        _ => "https://www.roblox.com/home".to_string(),
    };
    format!(
        "roblox-player:1+launchmode:play+gameinfo:{ticket}+launchtime:{ms}+placelauncherurl:{}+browsertrackerid:{btid}+robloxLocale:ru_ru+gameLocale:ru_ru+channel:",
        percent_encode(&place_url)
    )
}

fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 2);
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}
