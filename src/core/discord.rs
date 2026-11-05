//! Discord Rich Presence: named pipe `\\.\pipe\discord-ipc-{0..9}` (Windows).
//!
//! Кадр: u32 opcode LE + u32 длина LE + JSON. Opcodes: 0 HANDSHAKE,
//! 1 FRAME (команды), 2 CLOSE, 3 PING, 4 PONG. Воркер ленивое подключается
//! при старте сессии и закрывает соединение при её завершении (presence
//! пропадает вместе с пайпом).
//!
//! Активность повторяется каждые [`KEEPALIVE`] секунд: Roblox сам регистрирует
//! presence через Discord Game SDK и перетирает чужой слот — без повторов
//! в Discord остаётся только «играю в Roblox».
#![cfg_attr(not(windows), allow(dead_code))]

#[cfg(windows)]
use std::time::{Duration, Instant};

use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

/// ID приложения Discord (Developer Portal → New Application).
const DISCORD_CLIENT_ID: &str = "1567290388354437191";

/// Повтор SET_ACTIVITY с неизменным содержимым (защита от перетирания).
#[cfg(windows)]
const KEEPALIVE: Duration = Duration::from_secs(15);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Update {
    /// Игра идёт: строки активности (`None` — не показывать) + старт сессии.
    Session {
        details: Option<String>,
        state: Option<String>,
        start: Option<u64>,
    },
    /// Сессии нет — снять активность.
    Idle,
}

/// Состояние подключения к Discord (для карточки в Settings).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    /// Сессии нет — не пытались подключаться.
    Idle,
    /// Handshake прошёл, активность отправляется.
    Connected,
    /// Подключение не удалось (причина).
    Disconnected(String),
}

/// Хэндлер воркера: отправляй ему `Update`.
pub type Handle = UnboundedSender<Update>;
/// Приёмник состояния подключения (воркер → UI).
pub type StatusRx = UnboundedReceiver<Status>;

/// Запускает фоновый воркер Discord RP: `(Update-хэндлер, статус-приёмник)`.
pub fn spawn() -> (Handle, StatusRx) {
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    let (stx, srx) = tokio::sync::mpsc::unbounded_channel();
    let _ = std::thread::Builder::new()
        .name("discord-rp".into())
        .spawn(move || run(rx, stx));
    (tx, srx)
}

#[cfg(not(windows))]
fn run(rx: UnboundedReceiver<Update>, _status: UnboundedSender<Status>) {
    let mut rx = rx;
    while rx.blocking_recv().is_some() {}
}

#[cfg(windows)]
fn run(rx: UnboundedReceiver<Update>, status: UnboundedSender<Status>) {
    let rt = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(_) => return,
    };
    rt.block_on(worker(rx, status));
}

/// Отправить статус, если он изменился.
fn push_status(tx: &UnboundedSender<Status>, last: &mut Status, s: Status) {
    if *last != s {
        *last = s.clone();
        let _ = tx.send(s);
    }
}

#[cfg(windows)]
const OP_FRAME: u32 = 1;
#[cfg(windows)]
const OP_CLOSE: u32 = 2;
#[cfg(windows)]
const OP_PING: u32 = 3;
#[cfg(windows)]
const OP_PONG: u32 = 4;

#[cfg(windows)]
async fn worker(mut rx: UnboundedReceiver<Update>, status: UnboundedSender<Status>) {
    use tokio::sync::mpsc::error::TryRecvError;

    let nonce = std::sync::atomic::AtomicU64::new(1);
    let mut conn: Option<Conn> = None;
    let mut desired: Option<Update> = None;
    let mut last_sent: Option<(Update, Instant)> = None;
    let mut last_status = Status::Idle;
    let mut next_retry = Instant::now();

    loop {
        // — Ожидание: обновление / данные пайпа / пауза —
        let mut incoming: Option<Update> = None;
        if let Some(c) = &mut conn {
            tokio::select! {
                u = rx.recv() => incoming = u,
                res = tokio::time::timeout(Duration::from_millis(200), c.read_frame()) => {
                    match res {
                        Err(_elapsed) => {} // тишина на пайпе — ок
                        Ok(Ok(Some((OP_PING, payload)))) => {
                            let _ = c.send(OP_PONG, &payload).await;
                        }
                        Ok(Ok(Some((OP_CLOSE, _)))) => {
                            conn = None;
                            last_sent = None;
                            push_status(&status, &mut last_status,
                                Status::Disconnected("connection closed".into()));
                        }
                        Ok(Ok(Some((OP_FRAME, payload)))) => {
                            if payload.contains(r#""evt":"ERROR""#) {
                                conn = None;
                                last_sent = None;
                                push_status(&status, &mut last_status,
                                    Status::Disconnected(error_reason(&payload)));
                            }
                        }
                        Ok(Ok(Some(_))) => {} // прочие кадры — прочитаны, отброшены
                        Ok(Ok(None)) => {} // неполный кадр — ждём продолжения
                        Ok(Err(_)) => {
                            conn = None;
                            last_sent = None;
                            push_status(&status, &mut last_status,
                                Status::Disconnected("connection lost".into()));
                        }
                    }
                }
            }
        } else {
            match tokio::time::timeout(Duration::from_millis(400), rx.recv()).await {
                Ok(u) => incoming = u,
                Err(_) => {}
            }
        }

        // Коалесценция: важен только последний стейт.
        if let Some(u) = incoming {
            let mut u = u;
            loop {
                match rx.try_recv() {
                    Ok(next) => u = next,
                    Err(TryRecvError::Empty) | Err(TryRecvError::Disconnected) => break,
                }
            }
            desired = Some(u);
        }

        let wants_session = matches!(&desired, Some(Update::Session { .. }));

        // Ленивое подключение: только пока есть сессия, с паузой при ошибках.
        if conn.is_none() && wants_session && Instant::now() >= next_retry {
            match Conn::connect().await {
                Ok(c) => {
                    conn = Some(c);
                    last_sent = None; // после (ре)коннекта отправить заново
                    push_status(&status, &mut last_status, Status::Connected);
                }
                Err(e) => {
                    next_retry = Instant::now() + Duration::from_secs(5);
                    push_status(&status, &mut last_status, Status::Disconnected(connect_reason(&e)));
                }
            }
        }

        // Отправка активности / снятие / закрытие.
        if let Some(c) = &mut conn {
            match &desired {
                // Keepalive: тот же кадр повторяем, чтобы Roblox SDK не
                // занимал слот активности после нас.
                Some(u @ Update::Session { .. }) => {
                    let needs = match &last_sent {
                        None => true,
                        Some((prev, at)) => prev != u || at.elapsed() >= KEEPALIVE,
                    };
                    if needs {
                        let payload = set_activity_json(&nonce, u);
                        if c.send(OP_FRAME, &payload).await.is_ok() {
                            last_sent = Some((u.clone(), Instant::now()));
                        } else {
                            conn = None;
                            last_sent = None;
                            push_status(&status, &mut last_status,
                                Status::Disconnected("connection lost".into()));
                        }
                    }
                }
                // Сессии нет: снять активность (если что-то отправляли) и закрыть пайп.
                _ => {
                    if last_sent.is_some() {
                        let payload = clear_activity_json(&nonce);
                        let _ = c.send(OP_FRAME, &payload).await;
                    }
                    conn = None;
                    last_sent = None;
                    push_status(&status, &mut last_status, Status::Idle);
                }
            }
        } else if !wants_session {
            // Пайп уже закрыт (или не открывался) — сессии нет, статус спокоен.
            push_status(&status, &mut last_status, Status::Idle);
        }
    }
    // Канал закрыт (выход из приложения) — соединение закроется само.
}

/// Причина из ошибки подключения (короткая строка для UI).
#[cfg(windows)]
fn connect_reason(e: &std::io::Error) -> String {
    use std::io::ErrorKind;
    match e.kind() {
        ErrorKind::NotFound => "discord not running".into(),
        ErrorKind::TimedOut => "no response from discord".into(),
        _ => e.to_string(),
    }
}

/// `data.message` из кадра `{"evt":"ERROR",...}` (или запасной текст).
fn error_reason(payload: &str) -> String {
    serde_json::from_str::<serde_json::Value>(payload)
        .ok()
        .and_then(|v| v.pointer("/data/message").and_then(|m| m.as_str()).map(str::to_owned))
        .filter(|m| !m.is_empty())
        .unwrap_or_else(|| "discord error".into())
}

// ===== Протокол =====

fn frame(opcode: u32, payload: &str) -> Vec<u8> {
    let mut v = Vec::with_capacity(8 + payload.len());
    v.extend_from_slice(&opcode.to_le_bytes());
    v.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    v.extend_from_slice(payload.as_bytes());
    v
}

/// Вырезает один кадр из буфера (None — данных мало).
fn take_frame(buf: &mut Vec<u8>) -> Option<(u32, String)> {
    if buf.len() < 8 {
        return None;
    }
    let opcode = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]);
    let len = u32::from_le_bytes([buf[4], buf[5], buf[6], buf[7]]) as usize;
    if len > 4 * 1024 * 1024 {
        buf.clear(); // мусор — сбросить
        return None;
    }
    if buf.len() < 8 + len {
        return None;
    }
    let payload = String::from_utf8_lossy(&buf[8..8 + len]).into_owned();
    buf.drain(..8 + len);
    Some((opcode, payload))
}

fn set_activity_json(nonce: &std::sync::atomic::AtomicU64, u: &Update) -> String {
    let Update::Session {
        details,
        state,
        start,
    } = u
    else {
        return clear_activity_json(nonce);
    };
    let mut act = String::new();
    if let Some(d) = details {
        let d: String = d.chars().take(128).collect();
        act.push_str(&format!(r#""details":"{}","#, json_escape(&d)));
    }
    if let Some(s) = state {
        let s: String = s.chars().take(128).collect();
        act.push_str(&format!(r#""state":"{}","#, json_escape(&s)));
    }
    if let Some(t) = start {
        act.push_str(&format!(r#""timestamps":{{"start":{t}}},"#));
    }
    act.push_str(r#""instance":false"#);
    use std::sync::atomic::Ordering;
    let n = nonce.fetch_add(1, Ordering::Relaxed);
    format!(
        r#"{{"cmd":"SET_ACTIVITY","nonce":"{n}","args":{{"pid":{pid},"activity":{{{act}}}}}}}"#,
        n = n,
        pid = std::process::id(),
        act = act,
    )
}

fn clear_activity_json(nonce: &std::sync::atomic::AtomicU64) -> String {
    use std::sync::atomic::Ordering;
    let n = nonce.fetch_add(1, Ordering::Relaxed);
    format!(
        r#"{{"cmd":"SET_ACTIVITY","nonce":"{n}","args":{{"pid":{pid},"activity":null}}}}"#,
        n = n,
        pid = std::process::id(),
    )
}

fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

// ===== Windows: named pipe (tokio) =====

#[cfg(windows)]
struct Conn {
    client: tokio::net::windows::named_pipe::NamedPipeClient,
    buf: Vec<u8>,
}

#[cfg(windows)]
impl Conn {
    async fn connect() -> std::io::Result<Self> {
        use tokio::io::AsyncWriteExt;
        use tokio::net::windows::named_pipe::ClientOptions;

        let mut last_err =
            std::io::Error::new(std::io::ErrorKind::NotFound, "discord pipe not found");
        let mut client = None;
        for i in 0..10u8 {
            match ClientOptions::new().open(format!(r"\\.\pipe\discord-ipc-{i}")) {
                Ok(p) => {
                    client = Some(p);
                    break;
                }
                Err(e) => last_err = e,
            }
        }
        let mut c = Self {
            client: client.ok_or(last_err)?,
            buf: Vec::new(),
        };

        // HANDSHAKE → ждать READY (≤3 c).
        let hs = frame(
            0,
            &format!(r#"{{"v":1,"client_id":"{DISCORD_CLIENT_ID}"}}"#),
        );
        c.client.write_all(&hs).await?;
        let deadline = Instant::now() + Duration::from_secs(3);
        let mut ready = false;
        while Instant::now() < deadline && !ready {
            let rem = deadline.saturating_duration_since(Instant::now());
            match tokio::time::timeout(rem, c.read_frame()).await {
                Ok(Ok(Some((OP_CLOSE, _)))) => {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::ConnectionAborted,
                        "discord closed the connection",
                    ));
                }
                Ok(Ok(Some((_, payload)))) => {
                    if payload.contains(r#""evt":"READY""#) {
                        ready = true;
                    }
                    if payload.contains(r#""evt":"ERROR""#) {
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::ConnectionRefused,
                            error_reason(&payload),
                        ));
                    }
                }
                Ok(Ok(None)) => {} // неполный кадр
                Ok(Err(e)) => return Err(e),
                Err(_elapsed) => break,
            }
        }
        if !ready {
            return Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "no READY from discord",
            ));
        }
        Ok(c)
    }

    async fn send(&mut self, opcode: u32, payload: &str) -> std::io::Result<()> {
        use tokio::io::AsyncWriteExt;
        self.client.write_all(&frame(opcode, payload)).await
    }

    /// Один кадр из пайпа: `Some` — готовый, `None` — буфер неполон.
    async fn read_frame(&mut self) -> std::io::Result<Option<(u32, String)>> {
        use tokio::io::AsyncReadExt;
        if let Some(f) = take_frame(&mut self.buf) {
            return Ok(Some(f));
        }
        let mut tmp = [0u8; 4096];
        let n = self.client.read(&mut tmp).await?;
        if n == 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::ConnectionAborted,
                "discord pipe closed",
            ));
        }
        self.buf.extend_from_slice(&tmp[..n]);
        Ok(take_frame(&mut self.buf))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_roundtrip() {
        let mut buf = frame(1, r#"{"a":1}"#);
        assert_eq!(&buf[..4], &1u32.to_le_bytes());
        let (op, payload) = take_frame(&mut buf).unwrap();
        assert_eq!(op, 1);
        assert_eq!(payload, r#"{"a":1}"#);
        assert!(buf.is_empty());
        assert!(take_frame(&mut buf).is_none());
    }

    #[test]
    fn partial_frame_waits_for_bytes() {
        let full = frame(3, "ping");
        let mut buf = full[..6].to_vec();
        assert!(take_frame(&mut buf).is_none());
        buf.extend_from_slice(&full[6..]);
        let (op, payload) = take_frame(&mut buf).unwrap();
        assert_eq!(op, 3);
        assert_eq!(payload, "ping");
    }

    #[test]
    fn activity_json_is_valid() {
        use std::sync::atomic::AtomicU64;
        let nonce = AtomicU64::new(1);
        let s = set_activity_json(
            &nonce,
            &Update::Session {
                details: Some("ByteStrap".into()),
                state: Some(r#"Место "x" №1"#.into()),
                start: Some(1_700_000_000),
            },
        );
        let v: serde_json::Value = serde_json::from_str(&s).unwrap();
        assert_eq!(v["cmd"], "SET_ACTIVITY");
        assert_eq!(v["args"]["activity"]["details"], "ByteStrap");
        assert_eq!(v["args"]["activity"]["state"], "Место \"x\" №1");
        assert_eq!(v["args"]["activity"]["timestamps"]["start"], 1_700_000_000u64);

        let c = clear_activity_json(&nonce);
        let v: serde_json::Value = serde_json::from_str(&c).unwrap();
        assert!(v["args"]["activity"].is_null());
    }

    #[test]
    fn activity_omits_disabled_fields() {
        use std::sync::atomic::AtomicU64;
        let nonce = AtomicU64::new(1);
        let s = set_activity_json(
            &nonce,
            &Update::Session {
                details: None,
                state: Some("Только место".into()),
                start: None,
            },
        );
        let v: serde_json::Value = serde_json::from_str(&s).unwrap();
        let act = &v["args"]["activity"];
        assert!(act.get("details").is_none());
        assert!(act.get("timestamps").is_none());
        assert_eq!(act["state"], "Только место");
        assert_eq!(act["instance"], false);
    }

    #[test]
    fn escape_control_chars() {
        assert_eq!(json_escape("a\nb\"c\\d"), "a\\nb\\\"c\\\\d");
        assert_eq!(json_escape("\u{1}"), "\\u0001");
    }

    #[test]
    fn error_reason_from_discord_payload() {
        let p = r#"{"cmd":"DISPATCH","evt":"ERROR","data":{"code":4000,"message":"Invalid command"}}"#;
        assert_eq!(error_reason(p), "Invalid command");
        assert_eq!(error_reason("garbage"), "discord error");
    }
}
