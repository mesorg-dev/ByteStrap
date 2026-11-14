//! Список публичных серверов места (аналог «Server Region» в Bloxstrap):
//! публичный API games.roblox.com + запуск клиента в выбранный сервер по JobId.

use std::time::Duration;

use serde::Deserialize;

const UA: &str = "ByteStrap/1.0 (+https://github.com/mesorg-dev/ByteStrap)";

#[derive(Debug, Clone)]
pub struct ServerInfo {
    /// JobId сервера.
    pub id: String,
    pub playing: u32,
    pub max: u32,
    /// Пинг в мс (если API вернул).
    pub ping: Option<u32>,
    /// FPS сервера (если API вернул).
    pub fps: Option<f32>,
}

#[derive(Deserialize)]
struct RawResp {
    data: Vec<RawServer>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawServer {
    id: String,
    #[serde(default)]
    playing: u32,
    #[serde(default)]
    max_players: u32,
    #[serde(default)]
    ping: Option<u32>,
    #[serde(default)]
    fps: Option<f32>,
}

/// Загрузить список публичных серверов места (лимит Roblox — 100).
pub async fn fetch(place_id: u64) -> anyhow::Result<Vec<ServerInfo>> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()?;
    let resp = client
        .get(format!(
            "https://games.roblox.com/v1/games/{place_id}/servers/Public?sortOrder=Asc&limit=100"
        ))
        .header("User-Agent", UA)
        .send()
        .await?;
    if !resp.status().is_success() {
        anyhow::bail!("HTTP {}", resp.status());
    }
    let raw: RawResp = resp.json().await?;
    Ok(raw
        .data
        .into_iter()
        .filter(|s| s.max_players > 0)
        .map(|s| ServerInfo {
            id: s.id,
            playing: s.playing,
            max: s.max_players,
            ping: s.ping,
            fps: s.fps,
        })
        .collect())
}

/// JobId сервера с наименьшим пингом (сначала заполненные, но не полные).
pub fn best(list: &[ServerInfo]) -> Option<&ServerInfo> {
    list.iter()
        .filter(|s| s.playing < s.max)
        .min_by_key(|s| (s.ping.unwrap_or(u32::MAX), u32::MAX - s.playing))
}

/// Ссылка `roblox://` для захода в конкретный сервер (без аккаунта).
pub fn server_uri(place_id: u64, job_id: &str) -> String {
    format!("roblox://placeId={place_id}&gameInstanceId={job_id}")
}
