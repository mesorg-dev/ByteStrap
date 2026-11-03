//! Счётчик времени в игре: сессия + разбивка по местам.
//!
//! Сессия стартует вместе с плеером; в фоне watcher раз в секунду
//! проверяет процесс и сканирует лог Roblox на предмет смены места.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PlaceEntry {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub secs: u64,
    #[serde(default)]
    pub unix: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Playtime {
    #[serde(default)]
    pub total_secs: u64,
    #[serde(default)]
    pub places: BTreeMap<String, PlaceEntry>,
}

impl Playtime {
    pub fn file(base: &Path) -> PathBuf {
        base.join("playtime.json")
    }

    pub fn load(base: &Path) -> Self {
        std::fs::read_to_string(Self::file(base))
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, base: &Path) {
        if let Ok(s) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(Self::file(base), s);
        }
    }

    /// Слияние итогов сессии: total + разбивка по местам (id, name, secs).
    pub fn merge(&mut self, total: u64, places: &[(String, String, u64)]) {
        self.total_secs += total;
        let now = now_unix();
        for (id, name, secs) in places {
            if *secs == 0 {
                continue;
            }
            let e = self.places.entry(id.clone()).or_default();
            e.secs += secs;
            e.unix = now;
            if e.name.is_empty() && !name.is_empty() {
                e.name = name.clone();
            }
        }
    }

    /// Топ-N мест по времени.
    pub fn top(&self, n: usize) -> Vec<(&String, &PlaceEntry)> {
        let mut v: Vec<_> = self.places.iter().collect();
        v.sort_by(|a, b| b.1.secs.cmp(&a.1.secs));
        v.truncate(n);
        v
    }
}

/// HH:MM:SS.
pub fn fmt_duration(secs: u64) -> String {
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;
    format!("{h:02}:{m:02}:{s:02}")
}

pub fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub enum SessionEvent {
    /// Каждую секунду: суммарное время сессии и текущее место.
    Tick {
        secs: u64,
        place: Option<String>,
        /// Имя места из API (если уже разрешено).
        place_name: Option<String>,
    },
    /// Процесс завершён: итог и разбивка по местам (id, name, secs).
    End {
        total: u64,
        places: Vec<(String, String, u64)>,
    },
}

/// Работает, пока жив RobloxPlayerBeta.exe; шлёт Tick и End.
pub fn watch_session(tx: Sender<SessionEvent>) {
    let start = Instant::now();
    let mut place: Option<String> = None;
    let mut per_place: BTreeMap<String, u64> = BTreeMap::new();
    let mut names: BTreeMap<String, String> = BTreeMap::new();
    let mut log_offset: u64 = 0;
    let mut log_path = newest_log();

    loop {
        std::thread::sleep(Duration::from_secs(1));
        if !player_running() {
            break;
        }
        if log_path.is_none() {
            log_path = newest_log();
        }
        if let Some(p) = &log_path {
            let found = scan_new_place(p, &mut log_offset);
            if let Some(id) = found {
                if place.as_deref() != Some(id.as_str()) && !names.contains_key(&id) {
                    let rt = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build();
                    if let Ok(rt) = rt {
                        if let Some(n) = rt.block_on(fetch_place_name(&id)) {
                            names.insert(id.clone(), n);
                        }
                    }
                }
                place = Some(id);
            }
        }
        let total = start.elapsed().as_secs();
        if let Some(id) = &place {
            *per_place.entry(id.clone()).or_default() += 1;
        }
        let _ = tx.send(SessionEvent::Tick {
            secs: total,
            place: place.clone(),
            place_name: place.as_ref().and_then(|id| names.get(id)).cloned(),
        });
    }

    let total = start.elapsed().as_secs();
    let places: Vec<(String, String, u64)> = per_place
        .into_iter()
        .map(|(id, secs)| {
            let name = names.get(&id).cloned().unwrap_or_default();
            (id, name, secs)
        })
        .collect();
    let _ = tx.send(SessionEvent::End { total, places });
}

/// Есть ли живой процесс плеера.
fn player_running() -> bool {
    #[cfg(windows)]
    {
        std::process::Command::new("tasklist")
            .args(["/FI", "IMAGENAME eq RobloxPlayerBeta.exe", "/NH"])
            .output()
            .map(|o| {
                String::from_utf8_lossy(&o.stdout)
                    .to_ascii_lowercase()
                    .contains("robloxplayerbeta")
            })
            .unwrap_or(false)
    }
    #[cfg(not(windows))]
    {
        false
    }
}

pub(crate) fn logs_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Roblox")
        .join("logs")
}

/// Самый свежий *.log в логах Roblox.
fn newest_log() -> Option<PathBuf> {
    let dir = logs_dir();
    let mut best: Option<(std::time::SystemTime, PathBuf)> = None;
    for e in std::fs::read_dir(&dir).ok()?.flatten() {
        let p = e.path();
        if p.extension()?.to_str()? != "log" {
            continue;
        }
        let m = e.metadata().ok()?.modified().ok()?;
        if best.as_ref().map(|(t, _)| m > *t).unwrap_or(true) {
            best = Some((m, p));
        }
    }
    best.map(|(_, p)| p)
}

/// Читает новые байты лога и возвращает последний встретившийся placeId.
fn scan_new_place(path: &Path, offset: &mut u64) -> Option<String> {
    use std::io::{Read, Seek, SeekFrom};

    let mut f = std::fs::File::open(path).ok()?;
    let len = f.metadata().ok()?.len();
    if len < *offset {
        // лог перекатился
        *offset = 0;
    }
    if len == *offset {
        return None;
    }
    f.seek(SeekFrom::Start(*offset)).ok()?;
    let mut buf = Vec::new();
    f.take(len - *offset).read_to_end(&mut buf).ok()?;
    *offset = len;

    let text = String::from_utf8_lossy(&buf);
    let mut found = None;
    for line in text.lines() {
        if let Some(id) = find_place_id(line) {
            found = Some(id);
        }
    }
    found
}

/// `placeId=123…` / `"placeId":123` / `PlaceId: 123` → Some(id).
fn find_place_id(line: &str) -> Option<String> {
    let lower = line.to_ascii_lowercase();
    let mut idx = 0usize;
    while let Some(pos) = lower[idx..].find("placeid") {
        let start = idx + pos + 7;
        let window = &line[start..line.len().min(start + 24)];
        let digits: String = window
            .chars()
            .skip_while(|c| !c.is_ascii_digit())
            .take_while(|c| c.is_ascii_digit())
            .collect();
        if digits.len() >= 5 {
            return Some(digits);
        }
        idx = (start + 1).min(line.len());
        if idx >= line.len() {
            break;
        }
    }
    None
}

/// Имя места через Roblox API (universe → games).
async fn fetch_place_name(id: &str) -> Option<String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(8))
        .build()
        .ok()?;
    let u: serde_json::Value = client
        .get(format!("https://apis.roblox.com/universes/v1/places/{id}/universe"))
        .send()
        .await
        .ok()?
        .json()
        .await
        .ok()?;
    let uid = u["universeId"].as_u64()?;
    let g: serde_json::Value = client
        .get(format!("https://games.roblox.com/v1/games?universeIds={uid}"))
        .send()
        .await
        .ok()?
        .json()
        .await
        .ok()?;
    g["data"][0]["name"].as_str().map(|s| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duration_format() {
        assert_eq!(fmt_duration(0), "00:00:00");
        assert_eq!(fmt_duration(61), "00:01:01");
        assert_eq!(fmt_duration(3671), "01:01:11");
    }

    #[test]
    fn place_id_parsing() {
        assert_eq!(
            find_place_id("2025-01-01T00:00:00.000Z,1.0,6,Game,8,PlaceId: 123456789").as_deref(),
            Some("123456789")
        );
        assert_eq!(
            find_place_id(r#"{"placeId":9876543210}"#).as_deref(),
            Some("9876543210")
        );
        assert_eq!(
            find_place_id("join roblox://placeId=555000777").as_deref(),
            Some("555000777")
        );
        assert_eq!(find_place_id("no ids here"), None);
        assert_eq!(find_place_id("placeId=x"), None);
    }

    #[test]
    fn playtime_merge_and_top() {
        let mut pt = Playtime::default();
        pt.merge(100, &[("111".into(), "Game A".into(), 60)]);
        pt.merge(50, &[("111".into(), String::new(), 30), ("222".into(), "B".into(), 20)]);
        assert_eq!(pt.total_secs, 150);
        assert_eq!(pt.places["111"].secs, 90);
        assert_eq!(pt.places["111"].name, "Game A");
        assert_eq!(pt.places["222"].secs, 20);
        let top = pt.top(1);
        assert_eq!(top[0].0, "111");
    }

    #[test]
    fn save_load_roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        let mut pt = Playtime::default();
        pt.merge(42, &[("333".into(), "X".into(), 42)]);
        pt.save(tmp.path());
        let back = Playtime::load(tmp.path());
        assert_eq!(back.total_secs, 42);
        assert_eq!(back.places["333"].name, "X");
    }
}
