//! Discord Rich Presence: named pipe `\\.\pipe\discord-ipc-{0..9}` (Windows).
//!
//! Кадр: u32 opcode LE + u32 длина LE + JSON. Оpcodes: 0 HANDSHAKE,
//! 1 FRAME (команды), 2 CLOSE, 3 PING, 4 PONG. Воркер лениво подключается
//! при старте сессии и закрывает соединение при её завершении (presence
//! пропадает вместе с пайпом).
#![cfg_attr(not(windows), allow(dead_code))]

use std::sync::mpsc::{Receiver, Sender};
#[cfg(windows)]
use std::sync::mpsc::RecvTimeoutError;
#[cfg(windows)]
use std::time::{Duration, Instant};

/// ID приложения Discord (Developer Portal → New Application).
/// TODO: подставить реальный ID перед релизом.
const DISCORD_CLIENT_ID: &str = "0";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Update {
    /// Игра идёт: отображаемое имя места + unix-секунды начала сессии.
    Session { place: String, start: u64 },
    /// Сессия завершена — снять активность.
    Idle,
}

/// Запускает фоновый воркер Discord RP; отправляй ему `Update`.
pub fn spawn() -> Sender<Update> {
    let (tx, rx) = std::sync::mpsc::channel();
    let _ = std::thread::Builder::new()
        .name("discord-rp".into())
        .spawn(move || run(rx));
    tx
}

#[cfg(not(windows))]
fn run(rx: Receiver<Update>) {
    while rx.recv().is_ok() {}
}

#[cfg(windows)]
fn run(rx: Receiver<Update>) {
    use std::sync::atomic::{AtomicU64, Ordering};

    let nonce = AtomicU64::new(1);
    let mut conn: Option<Conn> = None;
    let mut desired: Option<Update> = None;
    let mut last_sent: Option<(String, u64)> = None;
    let mut next_retry = Instant::now();

    loop {
        // Входящие: важен только последний стейт (тики коалесцируются).
        let incoming = match rx.recv_timeout(Duration::from_millis(400)) {
            Ok(u) => {
                let mut u = u;
                while let Ok(next) = rx.try_recv() {
                    u = next;
                }
                Some(u)
            }
            Err(RecvTimeoutError::Timeout) => None,
            Err(RecvTimeoutError::Disconnected) => break,
        };
        if let Some(u) = incoming {
            desired = Some(u);
        }

        // Мёртвое соединение — забыть (presence уже пропал/пропадёт).
        if let Some(c) = &conn {
            if c.dead.load(Ordering::Relaxed) {
                conn = None;
                last_sent = None;
            }
        }

        let want: Option<(String, u64)> = match &desired {
            Some(Update::Session { place, start }) => Some((place.clone(), *start)),
            _ => None,
        };

        // Ленивое подключение: только пока есть сессия, с паузой при ошибках.
        if conn.is_none() && want.is_some() && Instant::now() >= next_retry {
            match Conn::connect() {
                Ok(c) => {
                    conn = Some(c);
                    last_sent = None; // после (ре)коннекта отправить заново
                }
                Err(_) => next_retry = Instant::now() + Duration::from_secs(5),
            }
        }

        let Some(c) = &conn else {
            continue;
        };

        match (&want, &last_sent) {
            (Some((place, start)), last) => {
                let needs = match last {
                    None => true,
                    Some((lp, ls)) => lp != place || start.abs_diff(*ls) > 5,
                };
                if needs {
                    let payload = set_activity_json(&nonce, place, *start);
                    if c.send(OP_FRAME, &payload).is_ok() {
                        last_sent = Some((place.clone(), *start));
                    } else {
                        conn = None;
                    }
                }
            }
            // Сессии нет: снять активность один раз и закрыть пайп.
            (None, Some(_)) => {
                let payload = clear_activity_json(&nonce);
                let _ = c.send(OP_FRAME, &payload);
                conn = None;
                last_sent = None;
            }
            (None, None) => conn = None,
        }
    }
    // Канал закрыт (выход из приложения) — соединение закроется само.
}

// ===== Протокол =====

#[cfg(windows)]
const OP_HANDSHAKE: u32 = 0;
#[cfg(windows)]
const OP_FRAME: u32 = 1;
#[cfg(windows)]
const OP_CLOSE: u32 = 2;
#[cfg(windows)]
const OP_PING: u32 = 3;
#[cfg(windows)]
const OP_PONG: u32 = 4;

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

fn set_activity_json(nonce: &std::sync::atomic::AtomicU64, place: &str, start: u64) -> String {
    use std::sync::atomic::Ordering;
    let n = nonce.fetch_add(1, Ordering::Relaxed);
    let state: String = place.chars().take(128).collect();
    let state = json_escape(&state);
    format!(
        concat!(
            r#"{{"cmd":"SET_ACTIVITY","nonce":"{n}","#,
            r#""args":{{"pid":{pid},"activity":{{"details":"ByteStrap","#,
            r#""state":"{state}","timestamps":{{"start":{start}}}}}}}}}"#
        ),
        n = n,
        pid = std::process::id(),
        state = state,
        start = start,
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

// ===== Windows: named pipe =====

#[cfg(windows)]
#[link(name = "kernel32")]
extern "system" {
    fn SetNamedPipeHandleState(
        h_named_pipe: *mut std::ffi::c_void,
        lp_mode: *const u32,
        lp_max_collection_count: *const u32,
        lp_collect_data_timeout: *const u32,
    ) -> i32;
}

#[cfg(windows)]
struct Conn {
    pipe: std::sync::Arc<std::sync::Mutex<std::os::windows::named_pipe::NamedPipeClient>>,
    dead: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

#[cfg(windows)]
impl Conn {
    fn connect() -> std::io::Result<Self> {
        use std::os::windows::named_pipe::ClientOptions;
        use std::sync::atomic::AtomicBool;
        use std::sync::{Arc, Mutex};

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
        let p = client.ok_or(last_err)?;
        if !set_nowait(&p) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                "SetNamedPipeHandleState failed",
            ));
        }
        let pipe = Arc::new(Mutex::new(p));

        // HANDSHAKE → ждать READY (≤3 c) nowait-опросом.
        let hs = frame(
            OP_HANDSHAKE,
            &format!(r#"{{"v":1,"client_id":"{DISCORD_CLIENT_ID}"}}"#),
        );
        write_locked(&pipe, &hs)?;
        let mut buf: Vec<u8> = Vec::new();
        let deadline = Instant::now() + Duration::from_secs(3);
        let mut ready = false;
        while Instant::now() < deadline {
            match read_locked(&pipe, &mut buf) {
                Ok(true) => {
                    while let Some((op, payload)) = take_frame(&mut buf) {
                        if op == OP_CLOSE {
                            return Err(std::io::Error::new(
                                std::io::ErrorKind::ConnectionAborted,
                                "discord closed the connection",
                            ));
                        }
                        if payload.contains(r#""evt":"READY""#) {
                            ready = true;
                        }
                        if payload.contains(r#""evt":"ERROR""#) {
                            return Err(std::io::Error::new(
                                std::io::ErrorKind::ConnectionRefused,
                                "discord handshake error",
                            ));
                        }
                    }
                    if ready {
                        break;
                    }
                }
                Ok(false) => std::thread::sleep(Duration::from_millis(50)),
                Err(e) => return Err(e),
            }
        }
        if !ready {
            return Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "no READY from discord",
            ));
        }

        let dead = Arc::new(AtomicBool::new(false));
        spawn_reader(pipe.clone(), dead.clone());
        Ok(Self { pipe, dead })
    }

    fn send(&self, opcode: u32, payload: &str) -> std::io::Result<()> {
        write_locked(&self.pipe, &frame(opcode, payload))
    }
}

/// PIPE_NOWAIT: чтение не блокирует мьютекс (опрос раз в 150 мс).
#[cfg(windows)]
fn set_nowait(p: &std::os::windows::named_pipe::NamedPipeClient) -> bool {
    use std::os::windows::io::AsRawHandle;
    let mode: u32 = 0x1; // PIPE_READMODE_BYTE | PIPE_NOWAIT
    unsafe {
        SetNamedPipeHandleState(
            p.as_raw_handle() as *mut _,
            &mode,
            std::ptr::null(),
            std::ptr::null(),
        ) != 0
    }
}

#[cfg(windows)]
fn write_locked(
    pipe: &std::sync::Mutex<std::os::windows::named_pipe::NamedPipeClient>,
    bytes: &[u8],
) -> std::io::Result<()> {
    use std::io::Write;
    let mut g = pipe
        .lock()
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::Other, "pipe poisoned"))?;
    g.write_all(bytes)
}

/// nowait-чтение: `Ok(true)` — есть данные, `Ok(false)` — пусто.
#[cfg(windows)]
fn read_locked(
    pipe: &std::sync::Mutex<std::os::windows::named_pipe::NamedPipeClient>,
    buf: &mut Vec<u8>,
) -> std::io::Result<bool> {
    use std::io::Read;
    let mut g = pipe
        .lock()
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::Other, "pipe poisoned"))?;
    let mut tmp = [0u8; 4096];
    match g.read(&mut tmp) {
        Ok(0) => Ok(false),
        Ok(n) => {
            buf.extend_from_slice(&tmp[..n]);
            Ok(true)
        }
        // ERROR_NO_DATA (232) в nowait-режиме = «пока нет данных».
        Err(e)
            if e.raw_os_error() == Some(232) || e.kind() == std::io::ErrorKind::WouldBlock =>
        {
            Ok(false)
        }
        Err(e) => Err(e),
    }
}

/// Читает кадры и отвечает на PING; при CLOSE/ошибке помечает `dead`.
#[cfg(windows)]
fn spawn_reader(
    pipe: std::sync::Arc<std::sync::Mutex<std::os::windows::named_pipe::NamedPipeClient>>,
    dead: std::sync::Arc<std::sync::atomic::AtomicBool>,
) {
    use std::sync::atomic::Ordering;
    let _ = std::thread::Builder::new()
        .name("discord-rp-read".into())
        .spawn(move || {
            let mut buf: Vec<u8> = Vec::new();
            while !dead.load(Ordering::Relaxed) {
                std::thread::sleep(Duration::from_millis(150));
                if read_locked(&pipe, &mut buf).is_err() {
                    dead.store(true, Ordering::Relaxed);
                    return;
                }
                while let Some((op, payload)) = take_frame(&mut buf) {
                    match op {
                        OP_CLOSE => {
                            dead.store(true, Ordering::Relaxed);
                            return;
                        }
                        OP_PING => {
                            if write_locked(&pipe, &frame(OP_PONG, &payload)).is_err() {
                                dead.store(true, Ordering::Relaxed);
                                return;
                            }
                        }
                        _ => {}
                    }
                }
                if buf.len() > 1024 * 1024 {
                    dead.store(true, Ordering::Relaxed);
                    return;
                }
            }
        });
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
        let s = set_activity_json(&nonce, r#"Место "x" №1"#, 1_700_000_000);
        let v: serde_json::Value = serde_json::from_str(&s).unwrap();
        assert_eq!(v["cmd"], "SET_ACTIVITY");
        assert_eq!(v["args"]["activity"]["state"], "Место \"x\" №1");
        assert_eq!(v["args"]["activity"]["timestamps"]["start"], 1_700_000_000u64);

        let c = clear_activity_json(&nonce);
        let v: serde_json::Value = serde_json::from_str(&c).unwrap();
        assert!(v["args"]["activity"].is_null());
    }

    #[test]
    fn escape_control_chars() {
        assert_eq!(json_escape("a\nb\"c\\d"), "a\\nb\\\"c\\\\d");
        assert_eq!(json_escape("\u{1}"), "\\u0001");
    }
}
