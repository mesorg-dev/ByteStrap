//! Кастомизация клиента Roblox (звуки смерти, курсоры, шрифты).
//!
//! Автоматически применяет выбранные ассеты как managed-моды в папку `Mods/`
//! и напрямую в активную версию клиента `Versions/Client/`.

use std::path::{Path, PathBuf};
use anyhow::{Context, Result};
use crate::core::config::Config;
use crate::core::logger;
use crate::core::versions::STATIC_VERSION_DIR;

// Встроенные ассеты звуков
const ASSET_OOF: &[u8] = include_bytes!("../../assets/sounds/oof.ogg");

// Встроенные ассеты курсоров
const ASSET_CURSOR_2006_ARROW: &[u8] = include_bytes!("../../assets/cursors/2006/ArrowCursor.png");
const ASSET_CURSOR_2006_FAR: &[u8] = include_bytes!("../../assets/cursors/2006/ArrowFarCursor.png");
const ASSET_CURSOR_2013_ARROW: &[u8] = include_bytes!("../../assets/cursors/2013/ArrowCursor.png");
const ASSET_CURSOR_2013_FAR: &[u8] = include_bytes!("../../assets/cursors/2013/ArrowFarCursor.png");
const ASSET_CURSOR_CROSS_ARROW: &[u8] = include_bytes!("../../assets/cursors/crosshair/ArrowCursor.png");
const ASSET_CURSOR_CROSS_FAR: &[u8] = include_bytes!("../../assets/cursors/crosshair/ArrowFarCursor.png");

// Встроенные шрифты
const ASSET_FONT_JETBRAINS_REG: &[u8] = include_bytes!("../../assets/fonts/JetBrainsMono-Regular.ttf");
const ASSET_FONT_JETBRAINS_BOLD: &[u8] = include_bytes!("../../assets/fonts/JetBrainsMono-Bold.ttf");
const ASSET_FONT_INTER_REG: &[u8] = include_bytes!("../../assets/fonts/Inter-Regular.ttf");
const ASSET_FONT_INTER_BOLD: &[u8] = include_bytes!("../../assets/fonts/Inter-Bold.ttf");
const ASSET_FONT_UNBOUNDED_REG: &[u8] = include_bytes!("../../assets/fonts/Unbounded-Regular.ttf");
const ASSET_FONT_UNBOUNDED_BOLD: &[u8] = include_bytes!("../../assets/fonts/Unbounded-Bold.ttf");

/// Генерация чистого синтезированного звука урона (16-bit PCM WAV)
fn generate_hurt_sound() -> Vec<u8> {
    let sample_rate: u32 = 22050;
    let duration_s: f32 = 0.22;
    let num_samples = (sample_rate as f32 * duration_s) as usize;
    let data_len = (num_samples * 2) as u32;

    let mut out = Vec::with_capacity(44 + num_samples * 2);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(data_len + 36).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes()); // subchunk1 size
    out.extend_from_slice(&1u16.to_le_bytes());  // PCM
    out.extend_from_slice(&1u16.to_le_bytes());  // Mono
    out.extend_from_slice(&sample_rate.to_le_bytes());
    out.extend_from_slice(&(sample_rate * 2).to_le_bytes()); // byte rate
    out.extend_from_slice(&2u16.to_le_bytes());  // block align
    out.extend_from_slice(&16u16.to_le_bytes()); // bits per sample
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());

    for i in 0..num_samples {
        let t = i as f32 / sample_rate as f32;
        let progress = t / duration_s;
        let freq = 210.0 - (90.0 * progress);
        let envelope = (1.0 - progress).powf(1.4);
        let sample_val = (2.0 * std::f32::consts::PI * freq * t).sin();
        let sample_i16 = (sample_val * 24000.0 * envelope) as i16;
        out.extend_from_slice(&sample_i16.to_le_bytes());
    }

    out
}

/// Путь к папке текущей версии клиента Roblox
fn client_version_dir(base: &Path) -> PathBuf {
    base.join("Versions").join(STATIC_VERSION_DIR)
}

/// Применение всех настроек кастомизации
pub fn apply_all(base: &Path, config: &Config) -> Result<()> {
    logger::info("Применение настроек кастомизации (звук, курсор, шрифт)");
    let _ = apply_death_sound(base, config.death_sound_mode, &config.death_sound_path);
    let _ = apply_cursor(base, config.cursor_mode);
    let _ = apply_font(base, config.font_mode, &config.font_path);
    Ok(())
}

/// Применить звук смерти
pub fn apply_death_sound(base: &Path, mode: i32, custom_path: &str) -> Result<()> {
    let mod_dir = base.join("Mods").join("Custom_DeathSound");
    let mod_sound_dir = mod_dir.join("content").join("sounds");
    let mod_file = mod_sound_dir.join("ouch.ogg");

    let client_file = client_version_dir(base).join("content").join("sounds").join("ouch.ogg");

    if mode == 0 {
        // По умолчанию: удаляем мод
        if mod_dir.exists() {
            let _ = std::fs::remove_dir_all(&mod_dir);
        }
        return Ok(());
    }

    let bytes = match mode {
        1 => ASSET_OOF.to_vec(),
        2 => generate_hurt_sound(),
        3 => Vec::new(), // Без звука (0 байт)
        4 => {
            if !custom_path.is_empty() && Path::new(custom_path).exists() {
                std::fs::read(custom_path).context("read custom death sound")?
            } else {
                ASSET_OOF.to_vec()
            }
        }
        _ => return Ok(()),
    };

    // Записываем в папку модов
    std::fs::create_dir_all(&mod_sound_dir)?;
    std::fs::write(&mod_file, &bytes)?;

    // Записываем напрямую в активную версию клиента (если она существует)
    if let Some(parent) = client_file.parent() {
        if parent.exists() {
            let _ = std::fs::write(&client_file, &bytes);
        }
    }

    Ok(())
}

/// Применить стиль курсора
pub fn apply_cursor(base: &Path, mode: i32) -> Result<()> {
    let mod_dir = base.join("Mods").join("Custom_Cursor");
    let mod_cursor_dir = mod_dir.join("content").join("textures").join("Cursors").join("KeyboardMouse");
    let client_cursor_dir = client_version_dir(base).join("content").join("textures").join("Cursors").join("KeyboardMouse");

    if mode == 0 {
        // По умолчанию: удаляем мод
        if mod_dir.exists() {
            let _ = std::fs::remove_dir_all(&mod_dir);
        }
        return Ok(());
    }

    let (arrow, far) = match mode {
        1 => (ASSET_CURSOR_2006_ARROW, ASSET_CURSOR_2006_FAR),
        2 => (ASSET_CURSOR_2013_ARROW, ASSET_CURSOR_2013_FAR),
        3 => (ASSET_CURSOR_CROSS_ARROW, ASSET_CURSOR_CROSS_FAR),
        _ => return Ok(()),
    };

    std::fs::create_dir_all(&mod_cursor_dir)?;
    std::fs::write(mod_cursor_dir.join("ArrowCursor.png"), arrow)?;
    std::fs::write(mod_cursor_dir.join("ArrowFarCursor.png"), far)?;

    if client_cursor_dir.exists() {
        let _ = std::fs::write(client_cursor_dir.join("ArrowCursor.png"), arrow);
        let _ = std::fs::write(client_cursor_dir.join("ArrowFarCursor.png"), far);
    }

    Ok(())
}

/// Применить пользовательский шрифт
pub fn apply_font(base: &Path, mode: i32, custom_path: &str) -> Result<()> {
    let mod_dir = base.join("Mods").join("Custom_Font");
    let mod_font_dir = mod_dir.join("content").join("fonts");
    let client_font_dir = client_version_dir(base).join("content").join("fonts");

    if mode == 0 {
        // По умолчанию: удаляем мод
        if mod_dir.exists() {
            let _ = std::fs::remove_dir_all(&mod_dir);
        }
        return Ok(());
    }

    let (reg, bold) = match mode {
        1 => (ASSET_FONT_JETBRAINS_REG.to_vec(), ASSET_FONT_JETBRAINS_BOLD.to_vec()),
        2 => (ASSET_FONT_INTER_REG.to_vec(), ASSET_FONT_INTER_BOLD.to_vec()),
        3 => (ASSET_FONT_UNBOUNDED_REG.to_vec(), ASSET_FONT_UNBOUNDED_BOLD.to_vec()),
        4 => {
            if !custom_path.is_empty() && Path::new(custom_path).exists() {
                let bytes = std::fs::read(custom_path).context("read custom font")?;
                (bytes.clone(), bytes)
            } else {
                (ASSET_FONT_JETBRAINS_REG.to_vec(), ASSET_FONT_JETBRAINS_BOLD.to_vec())
            }
        }
        _ => return Ok(()),
    };

    std::fs::create_dir_all(&mod_font_dir)?;

    // Шрифты Roblox, которые замещаются
    let font_targets_reg = [
        "SourceSansPro-Regular.ttf",
        "SourceSansPro-Light.ttf",
        "SourceSansPro-It.ttf",
    ];
    let font_targets_bold = [
        "SourceSansPro-Bold.ttf",
        "SourceSansPro-Semibold.ttf",
    ];

    for name in &font_targets_reg {
        let _ = std::fs::write(mod_font_dir.join(name), &reg);
        if client_font_dir.exists() {
            let _ = std::fs::write(client_font_dir.join(name), &reg);
        }
    }
    for name in &font_targets_bold {
        let _ = std::fs::write(mod_font_dir.join(name), &bold);
        if client_font_dir.exists() {
            let _ = std::fs::write(client_font_dir.join(name), &bold);
        }
    }

    Ok(())
}
