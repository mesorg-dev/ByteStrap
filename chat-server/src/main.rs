use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, RwLock};

const MAX_PEERS_PER_ROOM: usize = 50;
const MAX_MSG_CHARS: usize = 250;
const MAX_USERNAME_CHARS: usize = 32;
const MAX_DISPLAY_NAME_CHARS: usize = 64;
const MAX_PLACE_CHARS: usize = 100;
const RATE_LIMIT_INTERVAL: Duration = Duration::from_secs(1);

#[derive(Debug, Clone, Serialize)]
struct PeerInfo {
    user_id: u64,
    username: String,
    display_name: String,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ClientMsg {
    Join {
        room: String,
        user_id: u64,
        username: String,
        #[serde(default)]
        display_name: String,
        #[serde(default)]
        place: String,
    },
    Chat {
        text: String,
    },
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ServerMsg {
    Peers {
        list: Vec<PeerInfo>,
    },
    PeerJoined {
        peer: PeerInfo,
    },
    PeerLeft {
        user_id: u64,
    },
    Message {
        user_id: u64,
        username: String,
        display_name: String,
        text: String,
        ts: u64,
    },
    Error {
        reason: String,
    },
}

struct ClientConn {
    peer: PeerInfo,
    tx: mpsc::UnboundedSender<String>,
}

#[derive(Default)]
struct Room {
    conns: HashMap<u64, ClientConn>,
}

impl Room {
    fn broadcast(&self, json: &str, exclude_conn: Option<u64>) {
        for (&cid, conn) in &self.conns {
            if Some(cid) == exclude_conn {
                continue;
            }
            let _ = conn.tx.send(json.to_string());
        }
    }

    fn unique_peers(&self) -> Vec<PeerInfo> {
        let mut seen = HashMap::new();
        for conn in self.conns.values() {
            seen.entry(conn.peer.user_id)
                .or_insert_with(|| conn.peer.clone());
        }
        seen.into_values().collect()
    }

    fn has_user(&self, user_id: u64) -> bool {
        self.conns.values().any(|c| c.peer.user_id == user_id)
    }
}

#[derive(Clone, Default)]
struct AppState {
    rooms: Arc<RwLock<HashMap<String, Room>>>,
    next_conn_id: Arc<AtomicU64>,
}

#[tokio::main]
async fn main() {
    let bind_addr = std::env::var("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:8787".to_string());
    let addr: SocketAddr = bind_addr
        .parse()
        .unwrap_or_else(|e| panic!("Invalid BIND_ADDR '{bind_addr}': {e}"));

    let state = AppState::default();
    let app = Router::new()
        .route("/health", get(health))
        .route("/ws", get(ws_handler))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .unwrap_or_else(|e| panic!("Failed to bind {addr}: {e}"));

    println!("bytestrap-chat-server listening on {addr}");
    axum::serve(listener, app).await.expect("server error");
}

async fn health() -> &'static str {
    "ok"
}

async fn ws_handler(ws: WebSocketUpgrade, State(state): State<AppState>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

fn send_server_msg(tx: &mpsc::UnboundedSender<String>, msg: &ServerMsg) {
    if let Ok(json) = serde_json::to_string(msg) {
        let _ = tx.send(json);
    }
}

fn is_valid_room(room: &str) -> bool {
    room.len() == 64 && room.bytes().all(|b| b.is_ascii_hexdigit())
}

fn sanitize_text(raw: &str) -> String {
    raw.chars()
        .filter(|c| !c.is_control())
        .collect::<String>()
        .trim()
        .to_string()
}

fn validate_chat_text(raw: &str) -> Result<String, &'static str> {
    let clean = sanitize_text(raw);
    if clean.is_empty() {
        return Err("empty message");
    }
    if clean.chars().count() > MAX_MSG_CHARS {
        return Err("message exceeds 250 characters");
    }
    Ok(clean)
}

fn validate_username(raw: &str) -> Result<String, &'static str> {
    let clean = sanitize_text(raw);
    if clean.is_empty() {
        return Err("invalid username");
    }
    if clean.chars().count() > MAX_USERNAME_CHARS {
        return Err("username too long");
    }
    Ok(clean)
}

fn sanitize_bounded(raw: &str, max_chars: usize) -> String {
    sanitize_text(raw).chars().take(max_chars).collect()
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

async fn leave_room(state: &AppState, room_key: &str, conn_id: u64) {
    let mut rooms = state.rooms.write().await;
    if let Some(room) = rooms.get_mut(room_key) {
        if let Some(removed) = room.conns.remove(&conn_id) {
            if room.conns.is_empty() {
                rooms.remove(room_key);
            } else if !room.has_user(removed.peer.user_id) {
                if let Ok(json) = serde_json::to_string(&ServerMsg::PeerLeft {
                    user_id: removed.peer.user_id,
                }) {
                    room.broadcast(&json, None);
                }
            }
        }
    }
}

async fn handle_socket(mut socket: WebSocket, state: AppState) {
    let conn_id = state.next_conn_id.fetch_add(1, Ordering::Relaxed) + 1;
    let (out_tx, mut out_rx) = mpsc::unbounded_channel::<String>();

    let mut joined: Option<(String, PeerInfo)> = None;
    let mut last_chat_at: Option<Instant> = None;

    loop {
        tokio::select! {
            Some(outbound) = out_rx.recv() => {
                if socket.send(Message::Text(outbound.into())).await.is_err() {
                    break;
                }
            }
            inbound = socket.recv() => {
                let Some(Ok(msg)) = inbound else {
                    break;
                };
                match msg {
                    Message::Text(txt) => {
                        let parsed: Result<ClientMsg, _> = serde_json::from_str(txt.as_str());
                        match parsed {
                            Err(_) => {
                                send_server_msg(&out_tx, &ServerMsg::Error {
                                    reason: "invalid json payload".into(),
                                });
                            }
                            Ok(ClientMsg::Join { room, user_id, username, display_name, place }) => {
                                let room_norm = room.trim().to_ascii_lowercase();
                                if !is_valid_room(&room_norm) {
                                    send_server_msg(&out_tx, &ServerMsg::Error {
                                        reason: "invalid room hash (expected 64 hex chars)".into(),
                                    });
                                    continue;
                                }
                                if user_id == 0 {
                                    send_server_msg(&out_tx, &ServerMsg::Error {
                                        reason: "invalid user_id".into(),
                                    });
                                    continue;
                                }
                                let username = match validate_username(&username) {
                                    Ok(u) => u,
                                    Err(reason) => {
                                        send_server_msg(&out_tx, &ServerMsg::Error {
                                            reason: reason.into(),
                                        });
                                        continue;
                                    }
                                };
                                let display_name = {
                                    let d = sanitize_bounded(&display_name, MAX_DISPLAY_NAME_CHARS);
                                    if d.is_empty() { username.clone() } else { d }
                                };
                                let _place = sanitize_bounded(&place, MAX_PLACE_CHARS);

                                if let Some((old_room, _)) = joined.take() {
                                    leave_room(&state, &old_room, conn_id).await;
                                }

                                let peer = PeerInfo {
                                    user_id,
                                    username,
                                    display_name,
                                };

                                let join_result = {
                                    let mut rooms = state.rooms.write().await;
                                    let room_entry = rooms.entry(room_norm.clone()).or_default();
                                    if room_entry.conns.len() >= MAX_PEERS_PER_ROOM {
                                        if room_entry.conns.is_empty() {
                                            rooms.remove(&room_norm);
                                        }
                                        Err("room is full (max 50 connections)")
                                    } else {
                                        let already_present = room_entry.has_user(peer.user_id);
                                        room_entry.conns.insert(conn_id, ClientConn {
                                            peer: peer.clone(),
                                            tx: out_tx.clone(),
                                        });
                                        let list = room_entry.unique_peers();
                                        if !already_present {
                                            if let Ok(json) = serde_json::to_string(&ServerMsg::PeerJoined {
                                                peer: peer.clone(),
                                            }) {
                                                room_entry.broadcast(&json, Some(conn_id));
                                            }
                                        }
                                        Ok(list)
                                    }
                                };

                                match join_result {
                                    Ok(list) => {
                                        joined = Some((room_norm, peer));
                                        send_server_msg(&out_tx, &ServerMsg::Peers { list });
                                    }
                                    Err(reason) => {
                                        send_server_msg(&out_tx, &ServerMsg::Error {
                                            reason: reason.into(),
                                        });
                                        break;
                                    }
                                }
                            }
                            Ok(ClientMsg::Chat { text }) => {
                                let Some((ref room_key, ref peer)) = joined else {
                                    send_server_msg(&out_tx, &ServerMsg::Error {
                                        reason: "join a room first".into(),
                                    });
                                    continue;
                                };

                                let now = Instant::now();
                                if let Some(prev) = last_chat_at {
                                    if now.duration_since(prev) < RATE_LIMIT_INTERVAL {
                                        send_server_msg(&out_tx, &ServerMsg::Error {
                                            reason: "rate limit: max 1 message per second".into(),
                                        });
                                        continue;
                                    }
                                }

                                let clean = match validate_chat_text(&text) {
                                    Ok(t) => t,
                                    Err(reason) => {
                                        send_server_msg(&out_tx, &ServerMsg::Error {
                                            reason: reason.into(),
                                        });
                                        continue;
                                    }
                                };

                                last_chat_at = Some(now);
                                let payload = ServerMsg::Message {
                                    user_id: peer.user_id,
                                    username: peer.username.clone(),
                                    display_name: peer.display_name.clone(),
                                    text: clean,
                                    ts: now_unix(),
                                };
                                if let Ok(json) = serde_json::to_string(&payload) {
                                    let rooms = state.rooms.read().await;
                                    if let Some(room) = rooms.get(room_key) {
                                        room.broadcast(&json, None);
                                    }
                                }
                            }
                        }
                    }
                    Message::Close(_) => break,
                    Message::Ping(_) | Message::Pong(_) | Message::Binary(_) => {}
                }
            }
        }
    }

    if let Some((room_key, _)) = joined {
        leave_room(&state, &room_key, conn_id).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn room_validation() {
        assert!(is_valid_room(&"a".repeat(64)));
        assert!(is_valid_room(&"0123456789abcdef".repeat(4)));
        assert!(!is_valid_room(&"a".repeat(63)));
        assert!(!is_valid_room(&"g".repeat(64)));
    }

    #[test]
    fn text_sanitization_and_limits() {
        assert_eq!(validate_chat_text("  привет\x00\n  ").unwrap(), "привет");
        assert!(validate_chat_text("   \x07\x1b  ").is_err());
        assert_eq!(validate_chat_text(&"я".repeat(250)).unwrap().chars().count(), 250);
        assert!(validate_chat_text(&"я".repeat(251)).is_err());
    }

    #[test]
    fn username_validation() {
        assert_eq!(validate_username("  Player_1\t ").unwrap(), "Player_1");
        assert!(validate_username("   ").is_err());
        assert!(validate_username(&"a".repeat(33)).is_err());
    }

    #[tokio::test]
    async fn room_cleanup_on_last_leave() {
        let state = AppState::default();
        let room_key = "a".repeat(64);
        let (tx, _rx) = mpsc::unbounded_channel();
        {
            let mut rooms = state.rooms.write().await;
            let room = rooms.entry(room_key.clone()).or_default();
            room.conns.insert(1, ClientConn {
                peer: PeerInfo {
                    user_id: 42,
                    username: "Tester".into(),
                    display_name: "Tester".into(),
                },
                tx,
            });
        }
        assert_eq!(state.rooms.read().await.len(), 1);
        leave_room(&state, &room_key, 1).await;
        assert_eq!(state.rooms.read().await.len(), 0);
    }
}
