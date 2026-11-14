//! Менеджер аккаунтов Roblox: хранение `.ROBLOSECURITY` и запуск клиента
//! от имени выбранного аккаунта (auth-ticket → ссылка `roblox-player:`).
//!
//! Куки хранятся в открытом виде в `accounts.json` внутри папки ByteStrap —
//! это локальный файл пользователя, как и у прочих менеджеров аккаунтов.

use std::path::Path;

use serde::{Deserialize, Serialize};

const UA: &str = "ByteStrap/1.0 (+https://github.com/mesorg-dev/ByteStrap)";

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
        match std::fs::read_to_string(Self::file_path(base)) {
            Ok(s) => serde_json::from_str(&s).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self, base: &Path) {
        let path = Self::file_path(base);
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(s) = serde_json::to_string_pretty(self) {
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
/// `place` — если задан, клиент сразу зайдёт в этот плейс.
pub fn launch_uri(ticket: &str, place: Option<u64>) -> String {
    let ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let btid = ms as u64 % 9_000_000_000 + 1_000_000_000;
    let place_url = match place {
        Some(id) => format!(
            "https://assetgame.roblox.com/game/PlaceLauncher.ashx?request=RequestGame&browserTrackerId={btid}&placeId={id}&isPlayTogetherGame=false"
        ),
        None => "https://www.roblox.com/home".to_string(),
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
