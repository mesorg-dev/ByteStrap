//! Звуки интерфейса «Stellar Mono»: короткие клики, тумблеры, переходы.
//!
//! Реализация только под Windows (rodio/WASAPI); на остальных платформах —
//! беззвучная заглушка, чтобы не тянуть cpal в dev-сборки под Android.
//! Источник звука: каталог `sounds/` рядом с exe (click/toggle/tab/success/
//! error + .wav/.ogg/.mp3). Если файла нет — используется встроенный
//! синтезированный вариант, так что звук работает сразу из коробки.

#![allow(dead_code)]

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

/// kind из глобала Snd.
pub const CLICK: i32 = 0;
pub const TOGGLE: i32 = 1;
pub const TAB: i32 = 2;
pub const SUCCESS: i32 = 3;
pub const ERROR: i32 = 4;

/// Канонические имена (ищем <name>.wav/.ogg/.mp3 рядом с exe).
const NAMES: [&str; 5] = ["click", "toggle", "tab", "success", "error"];

pub struct Audio {
    _keep: Option<Engine>,
    enabled: AtomicBool,
    volume: AtomicU32,
    /// Готовые к воспроизведению байты на имя (None — не найдено).
    sources: Mutex<HashMap<&'static str, Vec<u8>>>,
}

/// Единственный экземпляр: движок expensive, создаём один раз.
pub fn global() -> Arc<Audio> {
    static AUDIO: OnceLock<Arc<Audio>> = OnceLock::new();
    AUDIO.get_or_init(Audio::new).clone()
}

#[cfg(windows)]
struct Engine {
    _stream: rodio::OutputStream,
    handle: rodio::OutputStreamHandle,
}

#[cfg(not(windows))]
struct Engine;

impl Audio {
    pub fn new() -> Arc<Self> {
        let engine = Self::open_engine();
        let sources = Self::load_sources();
        Arc::new(Self {
            _keep: engine,
            enabled: AtomicBool::new(true),
            volume: AtomicU32::new(70),
            sources: Mutex::new(sources),
        })
    }

    #[cfg(windows)]
    fn open_engine() -> Option<Engine> {
        match rodio::OutputStream::try_default() {
            Ok((stream, handle)) => Some(Engine { _stream: stream, handle }),
            Err(_) => None,
        }
    }

    #[cfg(not(windows))]
    fn open_engine() -> Option<Engine> {
        None
    }

    /// Пользовательские файлы из `sounds/` рядом с exe; чего нет — синтез.
    fn load_sources() -> HashMap<&'static str, Vec<u8>> {
        let mut map: HashMap<&'static str, Vec<u8>> = HashMap::new();
        let dir = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.join("sounds")));

        for (i, name) in NAMES.iter().enumerate() {
            let mut loaded = None;
            if let Some(dir) = &dir {
                for ext in ["wav", "ogg", "mp3"] {
                    let path = dir.join(format!("{name}.{ext}"));
                    if path.is_file() {
                        if let Ok(bytes) = std::fs::read(&path) {
                            loaded = Some(bytes);
                            break;
                        }
                    }
                }
            }
            map.insert(*name, loaded.unwrap_or_else(|| synth(i)));
        }
        map
    }

    pub fn set_enabled(&self, on: bool) {
        self.enabled.store(on, Ordering::Relaxed);
    }

    pub fn set_volume(&self, vol: u32) {
        self.volume.store(vol.min(100), Ordering::Relaxed);
    }

    /// Воспроизвести звук по kind; любые ошибки — молча.
    pub fn play(&self, kind: i32) {
        if !self.enabled.load(Ordering::Relaxed) {
            return;
        }
        let name: &str = match kind {
            CLICK => "click",
            TOGGLE => "toggle",
            TAB => "tab",
            SUCCESS => "success",
            ERROR => "error",
            _ => return,
        };
        let data = {
            let map = self.sources.lock().unwrap();
            match map.get(name) {
                Some(bytes) => bytes.clone(),
                None => return,
            }
        };
        self.sink_play(&data);
    }

    #[cfg(windows)]
    fn sink_play(&self, data: &[u8]) {
        let Some(engine) = &self._keep else { return };
        let cursor = std::io::Cursor::new(data.to_vec());
        let Ok(source) = rodio::Decoder::new(cursor) else { return };
        let Ok(sink) = rodio::Sink::try_new(&engine.handle) else { return };
        let vol = self.volume.load(Ordering::Relaxed) as f32 / 100.0;
        sink.set_volume(vol.clamp(0.0, 1.0) * 0.9);
        sink.append(source);
        sink.detach();
    }

    #[cfg(not(windows))]
    fn sink_play(&self, _data: &[u8]) {}
}

// ===== Синтез заглушек =====

/// Собрать WAV (16-bit PCM mono 44100) из f32-сэмплов [-1..1].
fn wav_bytes(samples: &[f32]) -> Vec<u8> {
    let rate: u32 = 44_100;
    let data_len = (samples.len() * 2) as u32;
    let mut out = Vec::with_capacity(44 + samples.len() * 2);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVE");
    out.extend_from_slice(b"fmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // mono
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&(rate * 2).to_le_bytes()); // byte rate
    out.extend_from_slice(&2u16.to_le_bytes()); // block align
    out.extend_from_slice(&16u16.to_le_bytes()); // bits
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for &s in samples {
        let v = (s.clamp(-1.0, 1.0) * 32767.0) as i16;
        out.extend_from_slice(&v.to_le_bytes());
    }
    out
}

/// Нежная огибающая Ханна на n сэмплов.
fn hann(n: usize) -> Vec<f32> {
    (0..n)
        .map(|i| {
            let t = i as f32 / (n.max(2) - 1) as f32;
            0.5 - 0.5 * (2.0 * std::f32::consts::PI * t).cos()
        })
        .collect()
}

/// Короткий тон с экспоненциальным затуханием.
fn tone(freq: f32, ms: usize, decay: f32, amp: f32) -> Vec<f32> {
    let n = ms * 441 / 1000;
    let env = hann(n);
    (0..n)
        .map(|i| {
            let t = i as f32 / 44_100.0;
            env[i] * amp * (2.0 * std::f32::consts::PI * freq * t).sin() * (-t * decay).exp()
        })
        .collect()
}

/// Смешать дорожки (обрезать по самой короткой не пустой).
fn mix(tracks: Vec<Vec<f32>>) -> Vec<f32> {
    let len = tracks.iter().map(|t| t.len()).max().unwrap_or(0);
    let mut out = vec![0.0f32; len];
    for t in tracks {
        for (i, s) in t.iter().enumerate() {
            out[i] += s;
        }
    }
    for s in out.iter_mut() {
        *s = s.clamp(-1.0, 1.0);
    }
    out
}

/// Сдвиг дорожки на ms.
fn delay(track: Vec<f32>, ms: usize) -> Vec<f32> {
    let mut out = vec![0.0f32; ms * 441 / 1000];
    out.extend(track);
    out
}

/// Синтез пяти звуков-заглушек: сухой тик, мягкий блик, «вжух», динь, глухой бумп.
fn synth(kind: usize) -> Vec<u8> {
    let samples = match kind {
        // click: сухой стеклянный тик — высокий тон + щелчок.
        0 => mix(vec![
            tone(2100.0, 42, 120.0, 0.32),
            tone(3600.0, 22, 200.0, 0.14),
        ]),
        // toggle: мягкий блип 520→700 Гц.
        1 => {
            let n = 80 * 441 / 1000;
            let env = hann(n);
            let track: Vec<f32> = (0..n)
                .map(|i| {
                    let t = i as f32 / 44_100.0;
                    let f = 520.0 + 180.0 * (i as f32 / n as f32);
                    env[i] * 0.3 * (2.0 * std::f32::consts::PI * f * t).sin()
                })
                .collect();
            track
        }
        // tab: «вжух» — шумовой свип с мягкой огибающей.
        2 => {
            let n = 140 * 441 / 1000;
            let env = hann(n);
            let mut seed = 0x2545F491u32;
            let mut rnd = move || {
                seed ^= seed << 13;
                seed ^= seed >> 17;
                seed ^= seed << 5;
                (seed % 65536) as f32 / 65536.0 - 0.5
            };
            let mut prev = 0.0f32;
            let track: Vec<f32> = (0..n)
                .map(|i| {
                    let k = i as f32 / n as f32;
                    // низкочастотный шум + тон, поднимающийся на подъёме свипа
                    let lp = prev + 0.22 * (rnd() - prev);
                    prev = lp;
                    let f = 240.0 + 660.0 * k;
                    let t = i as f32 / 44_100.0;
                    env[i] * (0.5 * lp * 0.9 + 0.12 * (2.0 * std::f32::consts::PI * f * t).sin())
                })
                .collect();
            track
        }
        // success: двузвучный «динь» (A5 → E6).
        3 => mix(vec![
            tone(880.0, 120, 22.0, 0.22),
            delay(tone(1318.5, 160, 16.0, 0.24), 95),
        ]),
        // error: глухой бумп 150 Гц + шумовой толчок.
        4 => mix(vec![
            tone(150.0, 190, 18.0, 0.5),
            tone(98.0, 190, 14.0, 0.3),
            {
                let n = 60 * 441 / 1000;
                let env = hann(n);
                let mut seed = 0x9E3779B9u32;
                let mut rnd = move || {
                    seed ^= seed << 13;
                    seed ^= seed >> 17;
                    seed ^= seed << 5;
                    (seed % 65536) as f32 / 65536.0 - 0.5
                };
                let mut prev = 0.0f32;
                (0..n)
                    .map(|i| {
                        let lp = prev + 0.12 * (rnd() - prev);
                        prev = lp;
                        env[i] * lp * 0.7
                    })
                    .collect()
            },
        ]),
        _ => Vec::new(),
    };
    wav_bytes(&samples)
}
