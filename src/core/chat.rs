//! Встроенный серверный чат ByteStrap:
//! - SHA-256 хэширование JobId (сервер никогда не видит реальный ID сервера Roblox);
//! - Однократное получение публичного профиля Roblox (`users.roblox.com/v1/users/{id}`) без кук;
//! - Фоновый WebSocket-клиент на `tokio-tungstenite` (не блокирует UI-поток Slint).

use std::collections::HashMap;
use std::sync::mpsc::Sender;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::sync::mpsc::{self as tokio_mpsc, UnboundedReceiver, UnboundedSender};
use tokio_tungstenite::tungstenite::Message;

use crate::app_slint::AsyncEvent;
use crate::core::logger;

const UA: &str = "ByteStrap/1.0 (+https://github.com/mesorg-dev/ByteStrap)";
pub const MAX_CHAT_CHARS: usize = 250;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserProfile {
    pub user_id: u64,
    pub username: String,
    pub display_name: String,
    pub is_fallback: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatPeer {
    pub user_id: u64,
    pub username: String,
    pub display_name: String,
}

#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub user_id: u64,
    pub username: String,
    pub display_name: String,
    pub text: String,
    pub ts: u64,
}

#[derive(Debug, Clone)]
pub enum ChatEvent {
    Connecting { room: String },
    Connected { room: String, peers: Vec<ChatPeer> },
    PeerJoined { room: String, peer: ChatPeer },
    PeerLeft { room: String, user_id: u64 },
    Message { room: String, msg: ChatMessage },
    ServerError { room: String, reason: String },
    Disconnected { room: String, reason: Option<String> },
    ProfileResolved(UserProfile),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectConfig {
    pub server_url: String,
    pub room: String,
    pub user_id: u64,
    pub username: String,
    pub display_name: String,
    pub place: String,
}

enum ChatCmd {
    Connect(ConnectConfig),
    SendMessage(String),
    Disconnect,
    Shutdown,
}

pub struct ChatHandle {
    tx: UnboundedSender<ChatCmd>,
}

impl ChatHandle {
    pub fn connect(&self, cfg: ConnectConfig) {
        let _ = self.tx.send(ChatCmd::Connect(cfg));
    }

    pub fn send_message(&self, text: String) {
        let _ = self.tx.send(ChatCmd::SendMessage(text));
    }

    pub fn disconnect(&self) {
        let _ = self.tx.send(ChatCmd::Disconnect);
    }
}

impl Drop for ChatHandle {
    fn drop(&mut self) {
        let _ = self.tx.send(ChatCmd::Shutdown);
    }
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum OutboundMsg<'a> {
    Join {
        room: &'a str,
        user_id: u64,
        username: &'a str,
        display_name: &'a str,
        place: &'a str,
    },
    Chat {
        text: &'a str,
    },
}

#[derive(Debug, Deserialize)]
struct WirePeer {
    user_id: u64,
    username: String,
    #[serde(default)]
    display_name: String,
}

impl From<WirePeer> for ChatPeer {
    fn from(w: WirePeer) -> Self {
        let display_name = if w.display_name.trim().is_empty() {
            w.username.clone()
        } else {
            w.display_name
        };
        Self {
            user_id: w.user_id,
            username: w.username,
            display_name,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum InboundMsg {
    Peers {
        list: Vec<WirePeer>,
    },
    PeerJoined {
        peer: WirePeer,
    },
    PeerLeft {
        user_id: u64,
    },
    Message {
        user_id: u64,
        username: String,
        #[serde(default)]
        display_name: String,
        text: String,
        ts: u64,
    },
    Error {
        reason: String,
    },
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RobloxUserResp {
    id: Option<u64>,
    name: Option<String>,
    display_name: Option<String>,
}

static PROFILE_CACHE: OnceLock<Mutex<HashMap<u64, UserProfile>>> = OnceLock::new();

fn profile_cache() -> &'static Mutex<HashMap<u64, UserProfile>> {
    PROFILE_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// SHA-256 хэш от JobId (64 lowercase hex-символа).
pub fn hash_job_id(job_id: &str) -> String {
    let norm = job_id.trim().to_ascii_lowercase();
    let digest = Sha256::digest(norm.as_bytes());
    let mut out = String::with_capacity(64);
    for b in digest {
        use std::fmt::Write;
        let _ = write!(&mut out, "{b:02x}");
    }
    out
}

/// Безопасный фолбэк-профиль `Player_{short_id}`, когда API недоступен или UserId не распарсился.
pub fn fallback_profile(user_id: Option<u64>) -> UserProfile {
    let uid = match user_id {
        Some(id) if id > 0 => id,
        _ => {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_nanos() as u64)
                .unwrap_or(1);
            let pid = std::process::id() as u64;
            let mixed = now ^ (pid.wrapping_mul(0x9E37_79B9_7F4A_7C15));
            10_000 + (mixed % 90_000)
        }
    };
    let s = uid.to_string();
    let short_id = if s.len() > 4 { &s[s.len() - 4..] } else { &s };
    let nick = format!("Player_{short_id}");
    UserProfile {
        user_id: uid,
        username: nick.clone(),
        display_name: nick,
        is_fallback: true,
    }
}

/// Получает кэшированный профиль или запрашивает публичный API Roblox без кук.
pub fn get_cached_profile(user_id: u64) -> Option<UserProfile> {
    profile_cache().lock().ok()?.get(&user_id).cloned()
}

pub fn spawn_profile_fetch(user_id: u64, tx: Sender<AsyncEvent>) {
    if let Some(cached) = get_cached_profile(user_id) {
        let _ = tx.send(AsyncEvent::Chat(ChatEvent::ProfileResolved(cached)));
        return;
    }

    std::thread::Builder::new()
        .name("bytestrap-user-api".into())
        .spawn(move || {
            let rt = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(r) => r,
                Err(_) => {
                    let _ = tx.send(AsyncEvent::Chat(ChatEvent::ProfileResolved(
                        fallback_profile(Some(user_id)),
                    )));
                    return;
                }
            };
            let profile = rt.block_on(fetch_user_profile(user_id));
            let _ = tx.send(AsyncEvent::Chat(ChatEvent::ProfileResolved(profile)));
        })
        .ok();
}

pub async fn fetch_user_profile(user_id: u64) -> UserProfile {
    if let Some(cached) = get_cached_profile(user_id) {
        return cached;
    }

    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .connect_timeout(Duration::from_secs(3))
        .build()
    {
        Ok(c) => c,
        Err(_) => return fallback_profile(Some(user_id)),
    };

    let url = format!("https://users.roblox.com/v1/users/{user_id}");
    if let Ok(resp) = client.get(&url).header("User-Agent", UA).send().await {
        if resp.status().is_success() {
            if let Ok(info) = resp.json::<RobloxUserResp>().await {
                if let Some(name) = info.name.map(|s| s.trim().to_string()).filter(|s| !s.is_empty()) {
                    let display_name = info
                        .display_name
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .unwrap_or_else(|| name.clone());
                    let profile = UserProfile {
                        user_id: info.id.unwrap_or(user_id),
                        username: name,
                        display_name,
                        is_fallback: false,
                    };
                    if let Ok(mut cache) = profile_cache().lock() {
                        cache.insert(user_id, profile.clone());
                    }
                    return profile;
                }
            }
        }
    }

    fallback_profile(Some(user_id))
}

/// Форматирует Unix-время (сек) в локальное `HH:MM`.
pub fn format_hhmm(ts: u64) -> String {
    let offset_secs = local_utc_offset_secs();
    let local = (ts as i64).saturating_add(offset_secs);
    let secs_of_day = local.rem_euclid(86_400);
    let h = secs_of_day / 3600;
    let m = (secs_of_day % 3600) / 60;
    format!("{h:02}:{m:02}")
}

fn local_utc_offset_secs() -> i64 {
    #[cfg(windows)]
    {
        use windows_sys::Win32::Foundation::SYSTEMTIME;
        use windows_sys::Win32::System::SystemInformation::{GetLocalTime, GetSystemTime};
        unsafe {
            let mut local: SYSTEMTIME = std::mem::zeroed();
            let mut utc: SYSTEMTIME = std::mem::zeroed();
            GetLocalTime(&mut local);
            GetSystemTime(&mut utc);
            let l_mins = (local.wHour as i64) * 60 + (local.wMinute as i64);
            let u_mins = (utc.wHour as i64) * 60 + (utc.wMinute as i64);
            let mut diff_mins = l_mins - u_mins;
            if diff_mins > 720 {
                diff_mins -= 1440;
            } else if diff_mins < -720 {
                diff_mins += 1440;
            }
            diff_mins * 60
        }
    }
    #[cfg(not(windows))]
    {
        0
    }
}

/// Запускает фоновый поток WebSocket-клиента чата.
pub fn spawn(event_tx: Sender<AsyncEvent>) -> ChatHandle {
    let (cmd_tx, cmd_rx) = tokio_mpsc::unbounded_channel();
    std::thread::Builder::new()
        .name("bytestrap-chat".into())
        .spawn(move || {
            let rt = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(r) => r,
                Err(e) => {
                    logger::error(format!("Chat runtime init failed: {e}"));
                    return;
                }
            };
            rt.block_on(worker_loop(cmd_rx, event_tx));
        })
        .ok();

    ChatHandle { tx: cmd_tx }
}

async fn worker_loop(
    mut cmd_rx: UnboundedReceiver<ChatCmd>,
    event_tx: Sender<AsyncEvent>,
) {
    let mut target: Option<ConnectConfig> = None;

    loop {
        let Some(cfg) = target.clone() else {
            match cmd_rx.recv().await {
                Some(ChatCmd::Connect(cfg)) => {
                    target = Some(cfg);
                }
                Some(ChatCmd::Disconnect) | Some(ChatCmd::SendMessage(_)) => {}
                Some(ChatCmd::Shutdown) | None => break,
            }
            continue;
        };

        let _ = event_tx.send(AsyncEvent::Chat(ChatEvent::Connecting {
            room: cfg.room.clone(),
        }));

        let conn_res = tokio::time::timeout(
            Duration::from_secs(8),
            tokio_tungstenite::connect_async(&cfg.server_url),
        )
        .await;

        let ws_stream = match conn_res {
            Ok(Ok((ws, _))) => ws,
            Ok(Err(e)) => {
                let reason = e.to_string();
                logger::warn(format!("Чат: ошибка подключения ({reason})"));
                let _ = event_tx.send(AsyncEvent::Chat(ChatEvent::Disconnected {
                    room: cfg.room.clone(),
                    reason: Some(reason),
                }));
                if !wait_backoff_or_cmd(&mut cmd_rx, &mut target, &event_tx).await {
                    break;
                }
                continue;
            }
            Err(_) => {
                let _ = event_tx.send(AsyncEvent::Chat(ChatEvent::Disconnected {
                    room: cfg.room.clone(),
                    reason: Some("connection timeout".into()),
                }));
                if !wait_backoff_or_cmd(&mut cmd_rx, &mut target, &event_tx).await {
                    break;
                }
                continue;
            }
        };

        let (mut ws_tx, mut ws_rx) = ws_stream.split();

        let join_msg = OutboundMsg::Join {
            room: &cfg.room,
            user_id: cfg.user_id,
            username: &cfg.username,
            display_name: &cfg.display_name,
            place: &cfg.place,
        };
        if let Ok(json) = serde_json::to_string(&join_msg) {
            if ws_tx.send(Message::Text(json.into())).await.is_err() {
                let _ = event_tx.send(AsyncEvent::Chat(ChatEvent::Disconnected {
                    room: cfg.room.clone(),
                    reason: Some("join send failed".into()),
                }));
                if !wait_backoff_or_cmd(&mut cmd_rx, &mut target, &event_tx).await {
                    break;
                }
                continue;
            }
        }

        let mut reconnect_now = false;
        let mut disconnect_reason: Option<String> = None;

        loop {
            tokio::select! {
                cmd = cmd_rx.recv() => {
                    match cmd {
                        Some(ChatCmd::Connect(new_cfg)) => {
                            if new_cfg.server_url != cfg.server_url
                                || new_cfg.room != cfg.room
                                || new_cfg.user_id != cfg.user_id
                                || new_cfg.username != cfg.username
                            {
                                let _ = ws_tx.send(Message::Close(None)).await;
                                target = Some(new_cfg);
                                reconnect_now = true;
                                break;
                            }
                        }
                        Some(ChatCmd::SendMessage(text)) => {
                            let clean: String = text
                                .chars()
                                .filter(|c| !c.is_control())
                                .collect::<String>()
                                .trim()
                                .chars()
                                .take(MAX_CHAT_CHARS)
                                .collect();
                            if !clean.is_empty() {
                                if let Ok(json) = serde_json::to_string(&OutboundMsg::Chat { text: &clean }) {
                                    if ws_tx.send(Message::Text(json.into())).await.is_err() {
                                        disconnect_reason = Some("send failed".into());
                                        break;
                                    }
                                }
                            }
                        }
                        Some(ChatCmd::Disconnect) => {
                            let _ = ws_tx.send(Message::Close(None)).await;
                            target = None;
                            reconnect_now = true;
                            let _ = event_tx.send(AsyncEvent::Chat(ChatEvent::Disconnected {
                                room: cfg.room.clone(),
                                reason: None,
                            }));
                            break;
                        }
                        Some(ChatCmd::Shutdown) | None => {
                            let _ = ws_tx.send(Message::Close(None)).await;
                            return;
                        }
                    }
                }
                incoming = ws_rx.next() => {
                    match incoming {
                        Some(Ok(Message::Text(txt))) => {
                            if let Ok(parsed) = serde_json::from_str::<InboundMsg>(txt.as_str()) {
                                match parsed {
                                    InboundMsg::Peers { list } => {
                                        let peers = list.into_iter().map(ChatPeer::from).collect();
                                        let _ = event_tx.send(AsyncEvent::Chat(ChatEvent::Connected {
                                            room: cfg.room.clone(),
                                            peers,
                                        }));
                                    }
                                    InboundMsg::PeerJoined { peer } => {
                                        let _ = event_tx.send(AsyncEvent::Chat(ChatEvent::PeerJoined {
                                            room: cfg.room.clone(),
                                            peer: peer.into(),
                                        }));
                                    }
                                    InboundMsg::PeerLeft { user_id } => {
                                        let _ = event_tx.send(AsyncEvent::Chat(ChatEvent::PeerLeft {
                                            room: cfg.room.clone(),
                                            user_id,
                                        }));
                                    }
                                    InboundMsg::Message { user_id, username, display_name, text, ts } => {
                                        let display_name = if display_name.trim().is_empty() {
                                            username.clone()
                                        } else {
                                            display_name
                                        };
                                        let _ = event_tx.send(AsyncEvent::Chat(ChatEvent::Message {
                                            room: cfg.room.clone(),
                                            msg: ChatMessage {
                                                user_id,
                                                username,
                                                display_name,
                                                text,
                                                ts,
                                            },
                                        }));
                                    }
                                    InboundMsg::Error { reason } => {
                                        let _ = event_tx.send(AsyncEvent::Chat(ChatEvent::ServerError {
                                            room: cfg.room.clone(),
                                            reason,
                                        }));
                                    }
                                }
                            }
                        }
                        Some(Ok(Message::Ping(data))) => {
                            let _ = ws_tx.send(Message::Pong(data)).await;
                        }
                        Some(Ok(Message::Close(_))) | None => {
                            break;
                        }
                        Some(Err(e)) => {
                            disconnect_reason = Some(e.to_string());
                            break;
                        }
                        _ => {}
                    }
                }
            }
        }

        if !reconnect_now {
            let _ = event_tx.send(AsyncEvent::Chat(ChatEvent::Disconnected {
                room: cfg.room.clone(),
                reason: disconnect_reason,
            }));
            if target.is_some() && !wait_backoff_or_cmd(&mut cmd_rx, &mut target, &event_tx).await {
                break;
            }
        }
    }
}

/// Ждёт 3 секунды перед переподключением, немедленно реагируя на новые команды.
/// Возвращает `false`, если пора завершать поток.
async fn wait_backoff_or_cmd(
    cmd_rx: &mut UnboundedReceiver<ChatCmd>,
    target: &mut Option<ConnectConfig>,
    event_tx: &Sender<AsyncEvent>,
) -> bool {
    let sleep = tokio::time::sleep(Duration::from_secs(3));
    tokio::pin!(sleep);

    loop {
        tokio::select! {
            _ = &mut sleep => return true,
            cmd = cmd_rx.recv() => {
                match cmd {
                    Some(ChatCmd::Connect(new_cfg)) => {
                        *target = Some(new_cfg);
                        return true;
                    }
                    Some(ChatCmd::Disconnect) => {
                        if let Some(old) = target.take() {
                            let _ = event_tx.send(AsyncEvent::Chat(ChatEvent::Disconnected {
                                room: old.room,
                                reason: None,
                            }));
                        }
                        return true;
                    }
                    Some(ChatCmd::SendMessage(_)) => {}
                    Some(ChatCmd::Shutdown) | None => return false,
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_room_hash_is_64_hex_chars() {
        let h1 = hash_job_id("A1B2C3D4-1111-2222-3333-444455556666");
        let h2 = hash_job_id(" a1b2c3d4-1111-2222-3333-444455556666 ");
        assert_eq!(h1, h2);
        assert_eq!(h1.len(), 64);
        assert!(h1.bytes().all(|b| b.is_ascii_hexdigit()));
    }

    #[test]
    fn fallback_profile_format() {
        let p = fallback_profile(Some(123456789));
        assert_eq!(p.user_id, 123456789);
        assert_eq!(p.username, "Player_6789");
        assert!(p.is_fallback);

        let anon = fallback_profile(None);
        assert!(anon.user_id > 0);
        assert!(anon.username.starts_with("Player_"));
        assert!(anon.is_fallback);
    }
}
