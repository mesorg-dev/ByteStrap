//! Звуки интерфейса «Stellar Mono»: короткие клики, тумблеры, переходы.
//!
//! Реализация только под Windows (rodio/WASAPI); на остальных платформах —
//! беззвучная заглушка, чтобы не тянуть cpal в dev-сборки под Android.
//! Источник звука: каталог `sounds/` рядом с exe (click/toggle/tab/success/
//! error + .wav/.ogg/.mp3). Если файла нет — используется встроенный
//! синтезированный вариант, так что звук работает сразу из коробки.
//!
//! Потоковые типы rodio (cpal) не Send/Sync, поэтому весь движок живёт в
//! отдельном потоке, а наружу торчит только mpsc-канал + атомарные настройки.

#![allow(dead_code)]

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, OnceLock};

/// kind из глобала Snd.
pub const CLICK: i32 = 0;
pub const TOGGLE: i32 = 1;
pub const TAB: i32 = 2;
pub const SUCCESS: i32 = 3;
pub const ERROR: i32 = 4;

/// Канонические имена (ищем <name>.wav/.ogg/.mp3 рядом с exe).
const NAMES: [&str; 5] = ["click", "toggle", "tab", "success", "error"];

/// Сообщения аудио-потоку.
enum Msg {
    Play(i32),
}

/// Публичный хэндл: Send + Sync, реальный движок — в отдельном потоке.
pub struct Audio {
    tx: Sender<Msg>,
    enabled: Arc<AtomicBool>,
    volume: Arc<AtomicU32>,
}

/// Единственный экземпляр: движок создаётся один раз.
pub fn global() -> Arc<Audio> {
    static AUDIO: OnceLock<Arc<Audio>> = OnceLock::new();
    AUDIO.get_or_init(|| {
        let (tx, rx) = channel::<Msg>();
        let enabled = Arc::new(AtomicBool::new(true));
        let volume = Arc::new(AtomicU32::new(70));
        spawn_engine(rx, Arc::clone(&enabled), Arc::clone(&volume));
        Arc::new(Audio { tx, enabled, volume })
    }).clone()
}

impl Audio {
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
        let _ = self.tx.send(Msg::Play(kind));
    }
}

/// Отдельный поток: владеет OutputStream (не Send/Sync) и источниками.
fn spawn_engine(
    rx: std::sync::mpsc::Receiver<Msg>,
    enabled: Arc<AtomicBool>,
    volume: Arc<AtomicU32>,
) {
    std::thread::Builder::new()
        .name("audio".into())
        .spawn(move || {
            let engine = open_engine();
            let sources = load_sources();
            while let Ok(msg) = rx.recv() {
                if !enabled.load(Ordering::Relaxed) {
                    continue;
                }
                let Msg::Play(kind) = msg;
                let name: &str = match kind {
                    CLICK => "click",
                    TOGGLE => "toggle",
                    TAB => "tab",
                    SUCCESS => "success",
                    ERROR => "error",
                    _ => continue,
                };
                let Some(data) = sources.get(name) else { continue };
                engine_play(&engine, data, volume.load(Ordering::Relaxed));
            }
        })
        .ok();
}

// ===== Windows: rodio/WASAPI =====

#[cfg(windows)]
struct Engine {
    _stream: rodio::OutputStream,
    handle: rodio::OutputStreamHandle,
}

#[cfg(windows)]
fn open_engine() -> Option<Engine> {
    match rodio::OutputStream::try_default() {
        Ok((stream, handle)) => Some(Engine { _stream: stream, handle }),
        Err(_) => None,
    }
}

#[cfg(windows)]
fn engine_play(engine: &Option<Engine>, data: &[u8], volume: u32) {
    let Some(engine) = engine else { return };
    let cursor = std::io::Cursor::new(data.to_vec());
    let Ok(source) = rodio::Decoder::new(cursor) else { return };
    let Ok(sink) = rodio::Sink::try_new(&engine.handle) else { return };
    let vol = volume as f32 / 100.0;
    sink.set_volume(vol.clamp(0.0, 1.0) * 0.9);
    sink.append(source);
    sink.detach();
}

// ===== Прочие платформы: тишина =====

#[cfg(not(windows))]
struct Engine;

#[cfg(not(windows))]
fn open_engine() -> Option<Engine> {
    None
}

#[cfg(not(windows))]
fn engine_play(_engine: &Option<Engine>, _data: &[u8], _volume: u32) {}

// ===== Источники =====

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

// ===== Синтез мягких тактильных звуков =====

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

/// Мягкий Low-Pass фильтр 1-го порядка (y = prev + alpha * (x - prev)).
fn low_pass(samples: &mut [f32], alpha: f32) {
    let mut prev = 0.0f32;
    for s in samples.iter_mut() {
        prev += alpha * (*s - prev);
        *s = prev;
    }
}

/// Псевдослучайный генератор белого шума [-1.0..1.0].
struct Rng(u32);
impl Rng {
    fn new(seed: u32) -> Self { Self(seed) }
    fn next_f32(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        (self.0 % 65536) as f32 / 32768.0 - 1.0
    }
}

/// Синтез пяти мягких тактильных звуков без резких щелчков и высоких писков.
fn synth(kind: usize) -> Vec<u8> {
    const SAMPLE_RATE: f32 = 44_100.0;
    const PI2: f32 = 2.0 * std::f32::consts::PI;

    let samples: Vec<f32> = match kind as i32 {
        // CLICK: короткий матовый «ток» (28 мс), спад частоты ~480 -> ~170 Гц + 12% отфильтрованного шума в первые 5 мс.
        CLICK => {
            let ms = 28.0;
            let n = (ms * SAMPLE_RATE / 1000.0) as usize;
            let attack_samples = (0.0025 * SAMPLE_RATE) as usize; // 2.5 мс мягкая атака
            let tau = 0.007; // экспоненциальный спад
            let mut rng = Rng::new(0x4D5A1234);
            let mut phase = 0.0f32;
            let mut track = Vec::with_capacity(n);

            for i in 0..n {
                let t = i as f32 / SAMPLE_RATE;
                let attack = if i < attack_samples { i as f32 / attack_samples as f32 } else { 1.0 };
                let decay = (-t / tau).exp();
                // Спад частоты синуса от 480 Гц до 170 Гц
                let freq = 170.0 + (480.0 - 170.0) * (-t / 0.007).exp();
                phase += PI2 * freq / SAMPLE_RATE;
                let mut s = phase.sin();

                // Капля отфильтрованного шума в первые 5 мс
                if t < 0.005 {
                    let noise_fade = 1.0 - t / 0.005;
                    s += rng.next_f32() * 0.12 * noise_fade;
                }

                track.push(s * attack * decay * 0.30);
            }
            low_pass(&mut track, 0.28);
            track
        }

        // TOGGLE: мягкий поп (38 мс) с плавным подъёмом частоты ~330 Гц -> ~470 Гц.
        TOGGLE => {
            let ms = 38.0;
            let n = (ms * SAMPLE_RATE / 1000.0) as usize;
            let attack_samples = (0.003 * SAMPLE_RATE) as usize; // 3 мс
            let tau = 0.012;
            let mut phase = 0.0f32;
            let mut track = Vec::with_capacity(n);

            for i in 0..n {
                let t = i as f32 / SAMPLE_RATE;
                let attack = if i < attack_samples { i as f32 / attack_samples as f32 } else { 1.0 };
                let decay = (-t / tau).exp();
                let progress = (i as f32 / n as f32).min(1.0);
                let freq = 330.0 + (470.0 - 330.0) * progress;
                phase += PI2 * freq / SAMPLE_RATE;
                let s = phase.sin();

                track.push(s * attack * decay * 0.28);
            }
            low_pass(&mut track, 0.32);
            track
        }

        // TAB: ещё более тихий, воздушный тактильный щелчок (22 мс, ~560 -> ~260 Гц).
        TAB => {
            let ms = 22.0;
            let n = (ms * SAMPLE_RATE / 1000.0) as usize;
            let attack_samples = (0.002 * SAMPLE_RATE) as usize; // 2 мс
            let tau = 0.0055;
            let mut phase = 0.0f32;
            let mut track = Vec::with_capacity(n);

            for i in 0..n {
                let t = i as f32 / SAMPLE_RATE;
                let attack = if i < attack_samples { i as f32 / attack_samples as f32 } else { 1.0 };
                let decay = (-t / tau).exp();
                let freq = 260.0 + (560.0 - 260.0) * (-t / 0.005).exp();
                phase += PI2 * freq / SAMPLE_RATE;
                let s = phase.sin();

                track.push(s * attack * decay * 0.22);
            }
            low_pass(&mut track, 0.26);
            track
        }

        // SUCCESS: мягкий приглушённый двойной тон (~440 Гц и затем ~660 Гц, тёплый синус).
        SUCCESS => {
            let total_n = (160.0 * SAMPLE_RATE / 1000.0) as usize;
            let mut track = vec![0.0f32; total_n];

            // Тон 1: 440 Гц, старт на 0 мс
            let n1 = (60.0 * SAMPLE_RATE / 1000.0) as usize;
            let attack1 = (0.003 * SAMPLE_RATE) as usize;
            let tau1 = 0.022;
            let mut phase1 = 0.0f32;
            for i in 0..n1 {
                let t = i as f32 / SAMPLE_RATE;
                let att = if i < attack1 { i as f32 / attack1 as f32 } else { 1.0 };
                let decay = (-t / tau1).exp();
                phase1 += PI2 * 440.0 / SAMPLE_RATE;
                track[i] += phase1.sin() * att * decay * 0.24;
            }

            // Тон 2: 660 Гц, старт на 45 мс
            let start2 = (45.0 * SAMPLE_RATE / 1000.0) as usize;
            let n2 = (80.0 * SAMPLE_RATE / 1000.0) as usize;
            let attack2 = (0.003 * SAMPLE_RATE) as usize;
            let tau2 = 0.028;
            let mut phase2 = 0.0f32;
            for i in 0..n2 {
                let idx = start2 + i;
                if idx >= total_n { break; }
                let t = i as f32 / SAMPLE_RATE;
                let att = if i < attack2 { i as f32 / attack2 as f32 } else { 1.0 };
                let decay = (-t / tau2).exp();
                phase2 += PI2 * 660.0 / SAMPLE_RATE;
                track[idx] += phase2.sin() * att * decay * 0.25;
            }

            low_pass(&mut track, 0.35);
            track
        }

        // ERROR: короткий глухой низкий двойной «тук» (~190 -> ~130 Гц).
        ERROR => {
            let total_n = (110.0 * SAMPLE_RATE / 1000.0) as usize;
            let mut track = vec![0.0f32; total_n];

            // Тук 1: 190 -> 130 Гц
            let n1 = (35.0 * SAMPLE_RATE / 1000.0) as usize;
            let attack1 = (0.0025 * SAMPLE_RATE) as usize;
            let tau1 = 0.010;
            let mut phase1 = 0.0f32;
            for i in 0..n1 {
                let t = i as f32 / SAMPLE_RATE;
                let att = if i < attack1 { i as f32 / attack1 as f32 } else { 1.0 };
                let decay = (-t / tau1).exp();
                let freq = 130.0 + (190.0 - 130.0) * (-t / 0.008).exp();
                phase1 += PI2 * freq / SAMPLE_RATE;
                track[i] += phase1.sin() * att * decay * 0.26;
            }

            // Тук 2: 170 -> 120 Гц (тише и глуше, старт на 35 мс)
            let start2 = (35.0 * SAMPLE_RATE / 1000.0) as usize;
            let n2 = (40.0 * SAMPLE_RATE / 1000.0) as usize;
            let attack2 = (0.0025 * SAMPLE_RATE) as usize;
            let tau2 = 0.012;
            let mut phase2 = 0.0f32;
            for i in 0..n2 {
                let idx = start2 + i;
                if idx >= total_n { break; }
                let t = i as f32 / SAMPLE_RATE;
                let att = if i < attack2 { i as f32 / attack2 as f32 } else { 1.0 };
                let decay = (-t / tau2).exp();
                let freq = 120.0 + (170.0 - 120.0) * (-t / 0.009).exp();
                phase2 += PI2 * freq / SAMPLE_RATE;
                track[idx] += phase2.sin() * att * decay * 0.22;
            }

            low_pass(&mut track, 0.22);
            track
        }

        _ => Vec::new(),
    };

    wav_bytes(&samples)
}
