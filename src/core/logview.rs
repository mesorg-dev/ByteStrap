//! Просмотр логов: список файлов Roblox, чтение хвоста и донаращивание (follow).
//!
//! Используется страницей «Логи» нового Slint-интерфейса. Roblox пишет
//! `Player.log` (и ротирует в `Player.previous.log`), лаунчер — свой
//! `launcher.log` (см. [`crate::core::logger`]).

use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::core::logger::{LEVEL_DEBUG, LEVEL_ERROR, LEVEL_INFO, LEVEL_WARN};

/// Максимум строк в памяти UI (хвост).
pub const MAX_LINES: usize = 1000;
/// Байт хвоста при первичном открытии.
const TAIL_BYTES: u64 = 256 * 1024;

#[derive(Clone)]
pub struct LogFile {
    /// Последний компонент пути (`Player.log`, `launcher.log`, …).
    pub name: String,
    pub path: PathBuf,
    pub size: u64,
}

/// Логи Roblox, новые сверху.
pub fn roblox_logs() -> Vec<LogFile> {
    let dir = crate::core::playtime::logs_dir();
    let mut out: Vec<(SystemTime, LogFile)> = Vec::new();
    for e in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
        let p = e.path();
        if p.extension().and_then(|s| s.to_str()) != Some("log") {
            continue;
        }
        let Ok(md) = e.metadata() else { continue };
        let mtime = md.modified().unwrap_or(SystemTime::UNIX_EPOCH);
        let name = p
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        out.push((
            mtime,
            LogFile {
                name,
                path: p,
                size: md.len(),
            },
        ));
    }
    out.sort_by(|a, b| b.0.cmp(&a.0));
    out.into_iter().map(|(_, f)| f).collect()
}

/// Лог самого лаунчера (файл может ещё не существовать).
pub fn launcher_log() -> LogFile {
    let path = crate::core::logger::file();
    let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
    LogFile {
        name: "launcher.log".into(),
        path,
        size,
    }
}

/// Состояние дочитывания файла: смещение + незавершённая строка.
#[derive(Default)]
pub struct Follow {
    pub offset: u64,
    pending: String,
}

fn read_bytes(path: &Path, from: u64, want: u64) -> Option<(Vec<u8>, u64)> {
    let mut f = std::fs::File::open(path).ok()?;
    f.seek(SeekFrom::Start(from)).ok()?;
    let mut buf = Vec::new();
    f.take(want).read_to_end(&mut buf).ok()?;
    let n = buf.len() as u64;
    Some((buf, from + n))
}

/// Первое открытие: хвост файла (последние `TAIL_BYTES`, без урезанной первой строки).
/// Незавершённая последняя строка уходит в pending.
pub fn load_tail(path: &Path, st: &mut Follow) -> String {
    let Ok(md) = std::fs::metadata(path) else {
        st.offset = 0;
        st.pending.clear();
        return String::new();
    };
    let len = md.len();
    let start = len.saturating_sub(TAIL_BYTES);
    let Some((raw, _)) = read_bytes(path, start, len - start) else {
        st.offset = 0;
        st.pending.clear();
        return String::new();
    };
    let mut text = String::from_utf8_lossy(&raw).into_owned();
    if start > 0 {
        if let Some(pos) = text.find('\n') {
            text = text[pos + 1..].to_string();
        }
    }
    st.offset = len;
    if let Some(pos) = text.rfind('\n') {
        st.pending = text[pos + 1..].to_string();
        text[..=pos].to_string()
    } else {
        st.pending = text;
        String::new()
    }
}

/// Донаращивание: новые завершённые строки от `st.offset`.
/// Если файл уменьшился (ротация) — читаем хвост заново.
pub fn follow_read(path: &Path, st: &mut Follow) -> String {
    let Ok(md) = std::fs::metadata(path) else {
        return String::new();
    };
    let len = md.len();
    if len < st.offset {
        return load_tail(path, st);
    }
    if len == st.offset {
        return String::new();
    }
    let want = len - st.offset;
    let Some((raw, new_off)) = read_bytes(path, st.offset, want) else {
        return String::new();
    };
    st.offset = new_off;
    let mut combined = std::mem::take(&mut st.pending);
    combined.push_str(&String::from_utf8_lossy(&raw));
    match combined.rfind('\n') {
        Some(pos) => {
            st.pending = combined[pos + 1..].to_string();
            combined[..=pos].to_string()
        }
        None => {
            st.pending = combined;
            String::new()
        }
    }
}

#[derive(Clone)]
pub struct Line {
    /// `HH:MM:SS` (или секунды от старта клиента); пусто, если нет.
    pub time: String,
    /// 0 DEBUG · 1 INFO · 2 WARN · 3 ERROR.
    pub level: u8,
    pub text: String,
    pub zebra: bool,
}

fn is_time_like(s: &str) -> bool {
    s.contains(':')
        && s.len() <= 12
        && s.starts_with(|c: char| c.is_ascii_digit())
        && s.chars()
            .all(|c| c.is_ascii_digit() || c == ':' || c == '.')
}

fn level_from_tag(tag: &str) -> Option<u8> {
    match tag.to_ascii_lowercase().as_str() {
        "error" | "fatal" | "critical" => Some(LEVEL_ERROR),
        "warn" | "warning" => Some(LEVEL_WARN),
        "debug" | "verbose" | "trace" => Some(LEVEL_DEBUG),
        "info" => Some(LEVEL_INFO),
        _ => None,
    }
}

pub fn classify(s: &str) -> u8 {
    let l = s.to_ascii_lowercase();
    if l.contains("error") || l.contains("[fatal") || l.contains("critical") {
        LEVEL_ERROR
    } else if l.contains("warn") {
        LEVEL_WARN
    } else if l.contains("debug") || l.contains("verbose") {
        LEVEL_DEBUG
    } else {
        LEVEL_INFO
    }
}

/// Разбор одной строки: время в колонку, уровень, текст.
pub fn parse_line(raw: &str, zebra: bool) -> Line {
    let raw = raw.trim_end_matches('\r');
    let mut s = raw;
    let mut time = String::new();
    let mut level = None;

    // Формат лаунчера: `[12:33:44] [ERROR] текст`.
    if s.starts_with('[') {
        if let Some(end) = s.find(']') {
            let inner = &s[1..end];
            if is_time_like(inner) {
                time = inner.to_string();
                s = s[end + 1..].trim_start();
                if s.starts_with('[') {
                    if let Some(e2) = s.find(']') {
                        level = level_from_tag(&s[1..e2]);
                        s = s[e2 + 1..].trim_start();
                    }
                }
            }
        }
    }

    // Формат Roblox: `2025-06-01T12:33:44.123Z …`.
    if time.is_empty()
        && s.len() >= 19
        && s.as_bytes()[4] == b'-'
        && s.as_bytes()[10] == b'T'
    {
        if let Some(t_pos) = s.find('T') {
            let rest = &s[t_pos + 1..];
            let end = rest
                .find(|c: char| !c.is_ascii_digit() && c != ':')
                .unwrap_or(rest.len());
            if end >= 8 {
                time = rest[..8].to_string();
                let mut i = t_pos + 1 + end;
                // Срезаем дробную часть и зону (`Z` / `+03:00`).
                let bytes = s.as_bytes();
                if i < bytes.len() && bytes[i] == b'.' {
                    i += 1;
                    while i < bytes.len() && bytes[i].is_ascii_digit() {
                        i += 1;
                    }
                }
                if i < bytes.len() && (bytes[i] == b'Z' || bytes[i] == b'+') {
                    while i < bytes.len()
                        && (bytes[i].is_ascii_digit()
                            || bytes[i] == b':'
                            || bytes[i] == b'Z'
                            || bytes[i] == b'+')
                    {
                        i += 1;
                    }
                }
                s = s[i..].trim_start();
            }
        }
    }

    // Стартовые секунды клиента: `0.123  [FLog::…]`.
    if time.is_empty() {
        let tok_end = s.find(char::is_whitespace).unwrap_or(s.len());
        let tok = &s[..tok_end];
        if tok.len() <= 12
            && tok.starts_with(|c: char| c.is_ascii_digit())
            && tok
                .chars()
                .all(|c| c.is_ascii_digit() || c == '.' || c == ':')
        {
            time = tok.to_string();
            s = s[tok_end..].trim_start();
        }
    }

    let level = level.unwrap_or_else(|| classify(s));
    Line {
        time,
        level,
        text: s.trim().to_string(),
        zebra,
    }
}

/// Разбор чанка завершённых строк (заканчивается на `\n`).
/// Возвращает строки и новое zebra-состояние.
pub fn parse_chunk(chunk: &str, mut zebra: bool) -> (Vec<Line>, bool) {
    let mut out = Vec::new();
    for raw in chunk.split('\n') {
        if raw.is_empty() {
            continue;
        }
        zebra = !zebra;
        out.push(parse_line(raw, zebra));
    }
    (out, zebra)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn parse_launcher_line() {
        let l = parse_line("[12:33:44] [ERROR] что-то сломалось", false);
        assert_eq!(l.time, "12:33:44");
        assert_eq!(l.level, LEVEL_ERROR);
        assert_eq!(l.text, "что-то сломалось");
    }

    #[test]
    fn parse_info_line() {
        let l = parse_line("[12:00:01] [INFO] привет", false);
        assert_eq!(l.level, LEVEL_INFO);
        assert_eq!(l.text, "привет");
    }

    #[test]
    fn parse_roblox_date_line() {
        let l = parse_line("2025-06-01T12:33:44.123Z  [FLog::Output] hello", false);
        assert_eq!(l.time, "12:33:44");
        assert_eq!(l.text, "[FLog::Output] hello");
    }

    #[test]
    fn parse_uptime_line() {
        let l = parse_line("0.123  [FLog::Output] tick", false);
        assert_eq!(l.time, "0.123");
        assert_eq!(l.text, "[FLog::Output] tick");
    }

    #[test]
    fn classify_levels() {
        assert_eq!(classify("something ERROR here"), LEVEL_ERROR);
        assert_eq!(classify("just a Warning"), LEVEL_WARN);
        assert_eq!(classify("plain line"), LEVEL_INFO);
    }

    #[test]
    fn tail_and_follow_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("a.log");
        fs::write(&p, "l1\nl2\n").unwrap();

        let mut st = Follow::default();
        let tail = load_tail(&p, &mut st);
        assert_eq!(tail, "l1\nl2\n");
        assert!(follow_read(&p, &mut st).is_empty());

        fs::write(&p, "l1\nl2\nl3\nl4\n").unwrap();
        let more = follow_read(&p, &mut st);
        assert_eq!(more, "l3\nl4\n");

        // Ротация: файл стал меньше.
        fs::write(&p, "new\n").unwrap();
        let rot = follow_read(&p, &mut st);
        assert_eq!(rot, "new\n");
    }

    #[test]
    fn partial_line_goes_to_pending() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("a.log");
        fs::write(&p, "full\npart").unwrap();

        let mut st = Follow::default();
        let tail = load_tail(&p, &mut st);
        assert_eq!(tail, "full\n");

        fs::write(&p, "full\npartial line\n").unwrap();
        let more = follow_read(&p, &mut st);
        assert_eq!(more, "partial line\n");
    }

    #[test]
    fn parse_chunk_zebra_flips() {
        let (lines, z) = parse_chunk("a\nb\nc\n", false);
        assert_eq!(lines.len(), 3);
        let flags: Vec<bool> = lines.iter().map(|l| l.zebra).collect();
        assert_eq!(flags, vec![true, false, true]);
        assert!(z);
    }
}
