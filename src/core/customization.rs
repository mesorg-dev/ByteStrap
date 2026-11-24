//! Кастомизация клиента Roblox (звуки смерти, курсоры, шрифты, PvP-текстуры, небо, Shift-Lock).
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

// Список основных материалов для 1x1 текстур: (имя папки, цвет PvP 1x1, цвет FPS Boost)
const MATERIALS: &[(&str, [u8; 4], [u8; 4])] = &[
    ("plastic", [160, 160, 160, 255], [210, 210, 210, 255]),
    ("smoothplastic", [170, 170, 170, 255], [220, 220, 220, 255]),
    ("concrete", [140, 140, 140, 255], [190, 190, 190, 255]),
    ("slate", [110, 115, 120, 255], [160, 165, 170, 255]),
    ("grass", [95, 135, 80, 255], [140, 185, 120, 255]),
    ("wood", [140, 105, 75, 255], [190, 155, 125, 255]),
    ("woodplanks", [145, 110, 80, 255], [195, 160, 130, 255]),
    ("metal", [150, 155, 160, 255], [200, 205, 210, 255]),
    ("brick", [150, 85, 75, 255], [195, 135, 125, 255]),
    ("cobblestone", [125, 125, 125, 255], [175, 175, 175, 255]),
    ("fabric", [130, 130, 135, 255], [180, 180, 185, 255]),
    ("granite", [135, 130, 130, 255], [185, 180, 180, 255]),
    ("ice", [180, 210, 225, 255], [215, 235, 245, 255]),
    ("marble", [200, 200, 205, 255], [230, 230, 235, 255]),
    ("pebble", [130, 128, 125, 255], [180, 178, 175, 255]),
    ("sand", [195, 180, 145, 255], [225, 215, 185, 255]),
    ("corrodedmetal", [120, 95, 80, 255], [165, 140, 125, 255]),
    ("diamondplate", [145, 150, 155, 255], [195, 200, 205, 255]),
    ("foil", [190, 195, 200, 255], [225, 230, 235, 255]),
    ("snow", [225, 230, 235, 255], [245, 245, 250, 255]),
];

const SKY_SIDES: &[&str] = &["bk", "dn", "ft", "lf", "rt", "up"];

/// Кодирование RgbaImage в PNG байты
fn encode_png(img: &image::RgbaImage) -> Vec<u8> {
    let mut buf = std::io::Cursor::new(Vec::new());
    let _ = img.write_to(&mut buf, image::ImageFormat::Png);
    buf.into_inner()
}

/// Генерация 1x1 сплошного PNG
fn generate_1x1_png(color: [u8; 4]) -> Vec<u8> {
    let mut img = image::RgbaImage::new(1, 1);
    img.put_pixel(0, 0, image::Rgba(color));
    encode_png(&img)
}

/// Генерация грани скайбокса (256x256)
fn generate_skybox_face(mode: i32, side: &str) -> Vec<u8> {
    let size = 256u32;
    let mut img = image::RgbaImage::new(size, size);
    match mode {
        1 => {
            // Чёрный PvP
            for pixel in img.pixels_mut() {
                *pixel = image::Rgba([0, 0, 0, 255]);
            }
        }
        2 => {
            // Ночной космос (звёзды)
            let side_seed = match side {
                "bk" => 101,
                "dn" => 202,
                "ft" => 303,
                "lf" => 404,
                "rt" => 505,
                "up" => 606,
                _ => 707,
            };
            for y in 0..size {
                let progress = y as f32 / size as f32;
                let bg_r = (10.0 - 5.0 * progress) as u8;
                let bg_g = (14.0 - 7.0 * progress) as u8;
                let bg_b = (28.0 - 12.0 * progress) as u8;
                for x in 0..size {
                    let mut h = (x.wrapping_mul(374761393))
                        ^ (y.wrapping_mul(668265263))
                        ^ side_seed;
                    h = (h ^ (h >> 13)).wrapping_mul(1274126177);
                    let threshold = if side == "dn" { 350 } else { 120 };
                    if h % threshold == 0 {
                        let br = 180 + (h % 75) as u8;
                        img.put_pixel(x, y, image::Rgba([br, br, 255, 255]));
                    } else {
                        img.put_pixel(x, y, image::Rgba([bg_r, bg_g, bg_b, 255]));
                    }
                }
            }
        }
        3 => {
            // Закат (фиолетово-оранжевый градиент)
            for y in 0..size {
                let progress = y as f32 / (size - 1) as f32;
                let (r, g, b) = if side == "up" {
                    (50, 25, 70)
                } else if side == "dn" {
                    (40, 22, 25)
                } else if progress < 0.6 {
                    let t = progress / 0.6;
                    (
                        (55.0 + (225.0 - 55.0) * t) as u8,
                        (30.0 + (105.0 - 30.0) * t) as u8,
                        (75.0 + (55.0 - 75.0) * t) as u8,
                    )
                } else {
                    let t = (progress - 0.6) / 0.4;
                    (
                        (225.0 + (245.0 - 225.0) * t) as u8,
                        (105.0 + (170.0 - 105.0) * t) as u8,
                        (55.0 + (90.0 - 55.0) * t) as u8,
                    )
                };
                for x in 0..size {
                    img.put_pixel(x, y, image::Rgba([r, g, b, 255]));
                }
            }
        }
        4 => {
            // Ясный день (мягкий сине-голубой градиент)
            for y in 0..size {
                let progress = y as f32 / (size - 1) as f32;
                let (r, g, b) = if side == "up" {
                    (65, 140, 235)
                } else if side == "dn" {
                    (175, 205, 230)
                } else {
                    (
                        (65.0 + (185.0 - 65.0) * progress) as u8,
                        (140.0 + (220.0 - 140.0) * progress) as u8,
                        (235.0 + (250.0 - 235.0) * progress) as u8,
                    )
                };
                for x in 0..size {
                    img.put_pixel(x, y, image::Rgba([r, g, b, 255]));
                }
            }
        }
        _ => {}
    }
    encode_png(&img)
}

/// Генерация прицела Shift-Lock (32x32 transparent PNG)
fn generate_shiftlock_cursor(mode: i32) -> Vec<u8> {
    let size = 32u32;
    let mut img = image::RgbaImage::new(size, size);
    let cx = 15.5f32;
    let cy = 15.5f32;

    for y in 0..size {
        for x in 0..size {
            let dx = x as f32 - cx;
            let dy = y as f32 - cy;
            let d = (dx * dx + dy * dy).sqrt();

            let pixel = match mode {
                1 => {
                    // Точка (Dot): белая точка с чётким тёмным контуром
                    if d <= 1.8 {
                        image::Rgba([255, 255, 255, 255])
                    } else if d <= 3.2 {
                        let a = ((3.2 - d) / 1.4 * 230.0).clamp(0.0, 230.0) as u8;
                        image::Rgba([0, 0, 0, a])
                    } else {
                        image::Rgba([0, 0, 0, 0])
                    }
                }
                2 => {
                    // Крестик (Crosshair): тонкий белый крестик с тёмным кантом
                    let ax = dx.abs();
                    let ay = dy.abs();
                    let on_cross_core = (ax <= 0.6 && ay >= 2.0 && ay <= 8.5)
                        || (ay <= 0.6 && ax >= 2.0 && ax <= 8.5)
                        || (d <= 1.2);
                    let on_cross_outline = (ax <= 1.6 && ay >= 1.0 && ay <= 9.5)
                        || (ay <= 1.6 && ax >= 1.0 && ax <= 9.5)
                        || (d <= 2.2);

                    if on_cross_core {
                        image::Rgba([255, 255, 255, 255])
                    } else if on_cross_outline {
                        image::Rgba([0, 0, 0, 210])
                    } else {
                        image::Rgba([0, 0, 0, 0])
                    }
                }
                3 => {
                    // Кольцо (Ring): тонкое кольцо с точкой в центре
                    if d <= 1.2 {
                        image::Rgba([255, 255, 255, 255])
                    } else if d >= 4.5 && d <= 6.2 {
                        image::Rgba([255, 255, 255, 255])
                    } else if (d >= 3.5 && d < 4.5) || (d > 6.2 && d <= 7.2) {
                        image::Rgba([0, 0, 0, 210])
                    } else {
                        image::Rgba([0, 0, 0, 0])
                    }
                }
                _ => image::Rgba([0, 0, 0, 0]),
            };
            img.put_pixel(x, y, pixel);
        }
    }
    encode_png(&img)
}

/// Генерация курсора мыши (32x32 transparent PNG)
fn generate_mouse_cursor(mode: i32) -> Vec<u8> {
    let size = 32u32;
    let mut img = image::RgbaImage::new(size, size);
    let cx = 15.5f32;
    let cy = 15.5f32;

    for y in 0..size {
        for x in 0..size {
            let dx = x as f32 - cx;
            let dy = y as f32 - cy;
            let d = (dx * dx + dy * dy).sqrt();

            let pixel = match mode {
                4 => {
                    // Минимал-точка
                    if d <= 2.0 {
                        image::Rgba([255, 255, 255, 255])
                    } else if d <= 3.5 {
                        let a = ((3.5 - d) / 1.5 * 240.0).clamp(0.0, 240.0) as u8;
                        image::Rgba([0, 0, 0, a])
                    } else {
                        image::Rgba([0, 0, 0, 0])
                    }
                }
                5 => {
                    // Тонкий крестик
                    let ax = dx.abs();
                    let ay = dy.abs();
                    let is_core = (ax <= 0.6 && ay <= 7.0) || (ay <= 0.6 && ax <= 7.0);
                    let is_border = (ax <= 1.6 && ay <= 8.0) || (ay <= 1.6 && ax <= 8.0);
                    if is_core {
                        image::Rgba([255, 255, 255, 255])
                    } else if is_border {
                        image::Rgba([0, 0, 0, 220])
                    } else {
                        image::Rgba([0, 0, 0, 0])
                    }
                }
                _ => image::Rgba([0, 0, 0, 0]),
            };
            img.put_pixel(x, y, pixel);
        }
    }
    encode_png(&img)
}

/// Рекурсивное копирование папки
fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<()> {
    if !src.is_dir() {
        return Ok(());
    }
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let path = entry.path();
        let target = dst.join(entry.file_name());
        if path.is_dir() {
            copy_dir_recursive(&path, &target)?;
        } else {
            let _ = std::fs::copy(&path, &target);
        }
    }
    Ok(())
}

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
    logger::info("Применение настроек кастомизации (звук, курсоры, шрифты, PvP-моды)");
    let _ = apply_death_sound(base, config.death_sound_mode, &config.death_sound_path);
    let _ = apply_cursor(base, config.cursor_mode, &config.cursor_path);
    let _ = apply_font(base, config.font_mode, &config.font_path);
    let _ = apply_textures(base, config.textures_mode, &config.textures_path);
    let _ = apply_skybox(base, config.skybox_mode, &config.skybox_path);
    let _ = apply_shiftlock(base, config.shiftlock_mode, &config.shiftlock_path);
    Ok(())
}

/// Применить звук смерти
pub fn apply_death_sound(base: &Path, mode: i32, custom_path: &str) -> Result<()> {
    let mod_dir = base.join("Mods").join("Custom_DeathSound");
    let mod_sound_dir = mod_dir.join("content").join("sounds");
    let mod_file = mod_sound_dir.join("ouch.ogg");

    let client_file = client_version_dir(base).join("content").join("sounds").join("ouch.ogg");

    if mode == 0 {
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

    std::fs::create_dir_all(&mod_sound_dir)?;
    std::fs::write(&mod_file, &bytes)?;

    if let Some(parent) = client_file.parent() {
        if parent.exists() {
            let _ = std::fs::write(&client_file, &bytes);
        }
    }

    Ok(())
}

/// Применить стиль курсора
pub fn apply_cursor(base: &Path, mode: i32, custom_path: &str) -> Result<()> {
    let mod_dir = base.join("Mods").join("Custom_Cursor");
    let mod_cursor_dir = mod_dir.join("content").join("textures").join("Cursors").join("KeyboardMouse");
    let client_cursor_dir = client_version_dir(base).join("content").join("textures").join("Cursors").join("KeyboardMouse");

    if mode == 0 {
        if mod_dir.exists() {
            let _ = std::fs::remove_dir_all(&mod_dir);
        }
        return Ok(());
    }

    let (arrow, far): (Vec<u8>, Vec<u8>) = match mode {
        1 => (ASSET_CURSOR_2006_ARROW.to_vec(), ASSET_CURSOR_2006_FAR.to_vec()),
        2 => (ASSET_CURSOR_2013_ARROW.to_vec(), ASSET_CURSOR_2013_FAR.to_vec()),
        3 => (ASSET_CURSOR_CROSS_ARROW.to_vec(), ASSET_CURSOR_CROSS_FAR.to_vec()),
        4 | 5 => {
            let bytes = generate_mouse_cursor(mode);
            (bytes.clone(), bytes)
        }
        6 => {
            if !custom_path.is_empty() && Path::new(custom_path).exists() {
                let bytes = std::fs::read(custom_path).context("read custom cursor png")?;
                (bytes.clone(), bytes)
            } else {
                let bytes = generate_mouse_cursor(4);
                (bytes.clone(), bytes)
            }
        }
        _ => return Ok(()),
    };

    std::fs::create_dir_all(&mod_cursor_dir)?;
    std::fs::write(mod_cursor_dir.join("ArrowCursor.png"), &arrow)?;
    std::fs::write(mod_cursor_dir.join("ArrowFarCursor.png"), &far)?;

    if client_cursor_dir.exists() {
        let _ = std::fs::write(client_cursor_dir.join("ArrowCursor.png"), &arrow);
        let _ = std::fs::write(client_cursor_dir.join("ArrowFarCursor.png"), &far);
    }

    Ok(())
}

/// Применить пользовательский шрифт
pub fn apply_font(base: &Path, mode: i32, custom_path: &str) -> Result<()> {
    let mod_dir = base.join("Mods").join("Custom_Font");
    let mod_font_dir = mod_dir.join("content").join("fonts");
    let client_font_dir = client_version_dir(base).join("content").join("fonts");

    if mode == 0 {
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

/// Применить текстуры материалов (PvP 1x1, FPS Boost или своя папка)
pub fn apply_textures(base: &Path, mode: i32, custom_path: &str) -> Result<()> {
    let mod_dir = base.join("Mods").join("Custom_Textures");
    let client_tex_dir = client_version_dir(base)
        .join("PlatformContent")
        .join("pc")
        .join("textures");

    if mode == 0 {
        if mod_dir.exists() {
            let _ = std::fs::remove_dir_all(&mod_dir);
        }
        return Ok(());
    }

    match mode {
        1 | 2 => {
            let mod_tex_dir = mod_dir
                .join("PlatformContent")
                .join("pc")
                .join("textures");
            std::fs::create_dir_all(&mod_tex_dir)?;

            for (mat, col_pvp, col_fps) in MATERIALS {
                let col = if mode == 1 { *col_pvp } else { *col_fps };
                let bytes = generate_1x1_png(col);
                let mat_mod = mod_tex_dir.join(mat);
                let _ = std::fs::create_dir_all(&mat_mod);
                let _ = std::fs::write(mat_mod.join("diffuse.png"), &bytes);

                if client_tex_dir.exists() {
                    let mat_client = client_tex_dir.join(mat);
                    if mat_client.exists() {
                        let _ = std::fs::write(mat_client.join("diffuse.png"), &bytes);
                    }
                }
            }
        }
        3 => {
            if !custom_path.is_empty() && Path::new(custom_path).is_dir() {
                let p = Path::new(custom_path);
                let _ = copy_dir_recursive(p, &mod_dir);
                if client_version_dir(base).exists() {
                    let _ = copy_dir_recursive(p, &client_version_dir(base));
                }
            }
        }
        _ => return Ok(()),
    }

    Ok(())
}

/// Применить скайбокс (небо)
pub fn apply_skybox(base: &Path, mode: i32, custom_path: &str) -> Result<()> {
    let mod_dir = base.join("Mods").join("Custom_Skybox");
    let mod_sky_dir = mod_dir.join("content").join("sky");
    let client_sky_dir = client_version_dir(base).join("content").join("sky");

    if mode == 0 {
        if mod_dir.exists() {
            let _ = std::fs::remove_dir_all(&mod_dir);
        }
        return Ok(());
    }

    match mode {
        1..=4 => {
            std::fs::create_dir_all(&mod_sky_dir)?;
            for side in SKY_SIDES {
                let bytes = generate_skybox_face(mode, side);
                let _ = std::fs::write(mod_sky_dir.join(format!("sky512_{side}.png")), &bytes);
                let _ = std::fs::write(mod_sky_dir.join(format!("{side}.png")), &bytes);

                if client_sky_dir.exists() {
                    let _ = std::fs::write(client_sky_dir.join(format!("sky512_{side}.png")), &bytes);
                    let _ = std::fs::write(client_sky_dir.join(format!("{side}.png")), &bytes);
                }
            }
        }
        5 => {
            if !custom_path.is_empty() && Path::new(custom_path).is_dir() {
                let p = Path::new(custom_path);
                let _ = copy_dir_recursive(p, &mod_sky_dir);
                if client_sky_dir.exists() {
                    let _ = copy_dir_recursive(p, &client_sky_dir);
                }
            }
        }
        _ => return Ok(()),
    }

    Ok(())
}

/// Применить прицел Shift-Lock
pub fn apply_shiftlock(base: &Path, mode: i32, custom_path: &str) -> Result<()> {
    let mod_dir = base.join("Mods").join("Custom_ShiftLock");
    let mod_tex_dir = mod_dir.join("content").join("textures");
    let client_tex_dir = client_version_dir(base).join("content").join("textures");

    if mode == 0 {
        if mod_dir.exists() {
            let _ = std::fs::remove_dir_all(&mod_dir);
        }
        return Ok(());
    }

    let bytes = match mode {
        1..=3 => generate_shiftlock_cursor(mode),
        4 => {
            if !custom_path.is_empty() && Path::new(custom_path).exists() {
                std::fs::read(custom_path).context("read custom shiftlock png")?
            } else {
                generate_shiftlock_cursor(1)
            }
        }
        _ => return Ok(()),
    };

    std::fs::create_dir_all(&mod_tex_dir)?;
    std::fs::write(mod_tex_dir.join("MouseLockedCursor.png"), &bytes)?;

    if client_tex_dir.exists() {
        let _ = std::fs::write(client_tex_dir.join("MouseLockedCursor.png"), &bytes);
    }

    Ok(())
}
