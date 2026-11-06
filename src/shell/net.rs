//! Сетевые данные шелла: метаданные мест, обложки с CDN, фон.
//!
//! Всё гоняется в фоновом потоке с собственным tokio-рантаймом и шлёт
//! [`Ev`] в UI-поток (канал `std::sync::mpsc`, как в лаунчере).

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;
use std::time::Duration;

use eframe::egui::ColorImage;

use super::config::thumb_path;

pub enum Ev {
    /// Доступность Roblox API (по результату первых запросов).
    Net(bool),
    /// Метаданные мест: имя + сколько играют сейчас.
    Meta(Vec<CardMeta>),
    /// Готовая декодированная обложка для place id.
    Thumb { id: String, img: ColorImage },
    /// Декодированный фон (ширина ≤ 1920).
    Bg(std::sync::Arc<ColorImage>),
    /// Размытая версия фона для отрисовки.
    Blurred(ColorImage),
    /// Фоновая загрузка каталога завершена.
    Loaded,
    /// Событие игровой сессии (Tick/End) из playtime-вотчера.
    Session(crate::core::playtime::SessionEvent),
}

#[derive(Clone, Debug)]
pub struct CardMeta {
    pub id: String,
    pub name: String,
    pub playing: u64,
}

/// Рекомендации: (placeId, имя-фолбэк). Счётчики и обложки — с API.
pub const RECO: &[(&str, &str)] = &[
    ("142823291", "Murder Mystery 2"),
    ("6516141723", "DOORS"),
    ("6872265039", "BedWars"),
    ("286090429", "Arsenal"),
    ("189707", "Natural Disaster"),
    ("8737602449", "PLS DONATE"),
    ("4924922222", "Brookhaven"),
    ("2753915549", "Blox Fruits"),
    ("920587237", "Adopt Me!"),
    ("126884695634066", "Grow a Garden"),
    ("606849621", "Jailbreak"),
    ("1962086868", "Tower of Hell"),
];

const API_TIMEOUT: Duration = Duration::from_secs(10);

/// Каталог мест (недавние + рекомендации): кэш обложек → API → обложки.
pub fn spawn_load(recent_ids: Vec<String>, base: PathBuf, tx: Sender<Ev>) {
    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("tokio runtime");
        rt.block_on(load_all(recent_ids, &base, &tx));
        let _ = tx.send(Ev::Loaded);
    });
}

/// Фон из файла: чтение → декод → даунскейл ≤1920.
pub fn spawn_bg(path: PathBuf, tx: Sender<Ev>) {
    std::thread::spawn(move || {
        let Ok(bytes) = std::fs::read(&path) else { return };
        let Ok(img) = image::load_from_memory(&bytes) else { return };
        let img = if img.width() > 1920 {
            let h = (u64::from(img.height()) * 1920 / u64::from(img.width())) as u32;
            img.resize(1920, h.max(1), image::imageops::FilterType::Triangle)
        } else {
            img
        };
        let rgba = img.to_rgba8();
        let ci = ColorImage::from_rgba_unmultiplied(
            [rgba.width() as usize, rgba.height() as usize],
            &rgba,
        );
        let _ = tx.send(Ev::Bg(std::sync::Arc::new(ci)));
    });
}

/// Гауссово размытие фона в фоновом потоке (ползунок не тормозит UI).
pub fn spawn_blur(src: std::sync::Arc<ColorImage>, sigma: f32, tx: Sender<Ev>) {
    std::thread::spawn(move || {
        if sigma <= 0.0 {
            let _ = tx.send(Ev::Blurred((*src).clone()));
            return;
        }
        let [w, h] = src.size;
        let mut buf = Vec::with_capacity(w * h * 4);
        for p in &src.pixels {
            let [r, g, b, a] = p.to_array();
            buf.extend_from_slice(&[r, g, b, a]);
        }
        let Some(img) = image::RgbaImage::from_raw(w as u32, h as u32, buf) else {
            return;
        };
        let out = image::imageops::blur(&img, sigma);
        let ci = ColorImage::from_rgba_unmultiplied(
            [out.width() as usize, out.height() as usize],
            &out,
        );
        let _ = tx.send(Ev::Blurred(ci));
    });
}

async fn load_all(recent_ids: Vec<String>, base: &Path, tx: &Sender<Ev>) {
    // Идентификаторы: недавние + рекомендации (без дублей).
    let mut all: Vec<String> = Vec::new();
    for id in recent_ids {
        if !all.contains(&id) {
            all.push(id);
        }
    }
    for (id, _) in RECO {
        if !all.iter().any(|a| a == id) {
            all.push((*id).to_string());
        }
    }

    // 1. Кэш обложек на диске — сразу рисуем карточки.
    let mut cached: HashSet<String> = HashSet::new();
    for id in &all {
        if let Some(img) = load_cached(base, id) {
            cached.insert(id.clone());
            let _ = tx.send(Ev::Thumb { id: id.clone(), img });
        }
    }

    let Ok(client) = reqwest::Client::builder().timeout(API_TIMEOUT).build() else {
        let _ = tx.send(Ev::Net(false));
        return;
    };

    // 2. placeId → universeId (параллельно).
    let mut set = tokio::task::JoinSet::new();
    for id in all.clone() {
        let c = client.clone();
        set.spawn(async move {
            let uid = fetch_universe(&c, &id).await;
            (id, uid)
        });
    }
    let mut uid_of: HashMap<String, u64> = HashMap::new();
    let mut place_of_uid: HashMap<u64, String> = HashMap::new();
    let mut any_ok = false;
    while let Some(res) = set.join_next().await {
        if let Ok((id, Some(uid))) = res {
            any_ok = true;
            place_of_uid.insert(uid, id.clone());
            uid_of.insert(id, uid);
        }
    }
    let _ = tx.send(Ev::Net(any_ok));

    // 3. Имена и счётчики одним батчем.
    if !uid_of.is_empty() {
        let uids: Vec<String> = uid_of.values().map(|u| u.to_string()).collect();
        if let Some(games) = fetch_games(&client, &uids).await {
            let metas: Vec<CardMeta> = games
                .into_iter()
                .map(|(id, name, playing)| CardMeta { id, name, playing })
                .collect();
            if !metas.is_empty() {
                let _ = tx.send(Ev::Meta(metas));
            }
        }
    }

    // 4. URL обложек: широкие по universe, для остальных — квадратные по placeId.
    let mut url_of: HashMap<String, String> = HashMap::new();
    let need: Vec<&String> = all.iter().filter(|id| !cached.contains(*id)).collect();
    if !need.is_empty() {
        let wide_uids: Vec<u64> = need
            .iter()
            .filter_map(|id| uid_of.get(*id).copied())
            .collect();
        if !wide_uids.is_empty() {
            let uid_strs: Vec<String> = wide_uids.iter().map(|u| u.to_string()).collect();
            if let Some(urls) = fetch_wide_thumbs(&client, &uid_strs).await {
                for (uid, url) in urls {
                    if let Some(place) = place_of_uid.get(&uid) {
                        url_of.insert(place.clone(), url);
                    }
                }
            }
        }
        let missing: Vec<String> = need
            .iter()
            .filter(|id| !url_of.contains_key(**id))
            .map(|id| (*id).clone())
            .collect();
        if !missing.is_empty() {
            if let Some(urls) = fetch_square_thumbs(&client, &missing).await {
                url_of.extend(urls);
            }
        }
    }

    // 5. Скачивание → кэш на диск → декод → в UI.
    let mut set = tokio::task::JoinSet::new();
    for (id, url) in url_of {
        let c = client.clone();
        let base = base.to_path_buf();
        set.spawn(async move {
            let bytes = download(&c, &url).await?;
            let img = decode(&bytes)?;
            let path = thumb_path(&base, &id);
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let _ = std::fs::write(path, &bytes);
            Some((id, img))
        });
    }
    while let Some(res) = set.join_next().await {
        if let Ok(Some((id, img))) = res {
            let _ = tx.send(Ev::Thumb { id, img });
        }
    }
}

async fn fetch_universe(client: &reqwest::Client, place_id: &str) -> Option<u64> {
    let v: serde_json::Value = client
        .get(format!("https://apis.roblox.com/universes/v1/places/{place_id}/universe"))
        .send()
        .await
        .ok()?
        .error_for_status()
        .ok()?
        .json()
        .await
        .ok()?;
    v["universeId"].as_u64()
}

/// universeIds → (rootPlaceId, name, playing).
async fn fetch_games(client: &reqwest::Client, universe_ids: &[String]) -> Option<Vec<(String, String, u64)>> {
    let url = format!("https://games.roblox.com/v1/games?universeIds={}", universe_ids.join(","));
    let v: serde_json::Value = client
        .get(url)
        .send()
        .await
        .ok()?
        .error_for_status()
        .ok()?
        .json()
        .await
        .ok()?;
    let data = v["data"].as_array()?;
    let mut out = Vec::with_capacity(data.len());
    for g in data {
        let Some(pid) = g["rootPlaceId"].as_u64() else { continue };
        out.push((
            pid.to_string(),
            g["name"].as_str().unwrap_or_default().to_string(),
            g["playing"].as_u64().unwrap_or(0),
        ));
    }
    Some(out)
}

/// universeIds → (universeId, imageUrl) обложек 768×432.
async fn fetch_wide_thumbs(client: &reqwest::Client, universe_ids: &[String]) -> Option<HashMap<u64, String>> {
    let url = format!(
        "https://thumbnails.roblox.com/v1/games/multiget/thumbnails?universeIds={}&countPerUniverse=1&defaults=true&size=768x432&format=Png&isCircular=false",
        universe_ids.join(",")
    );
    thumb_pairs(client, &url, true).await
}

/// placeIds → (placeId, imageUrl) квадратных иконок 512×512.
async fn fetch_square_thumbs(client: &reqwest::Client, place_ids: &[String]) -> Option<HashMap<String, String>> {
    let mut url = String::from("https://thumbnails.roblox.com/v1/places/gameicons?size=512x512&format=Png&isCircular=false");
    for id in place_ids {
        url.push_str("&placeIds=");
        url.push_str(id);
    }
    thumb_pairs(client, &url, false)
        .await
        .map(|m| m.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
}

/// Общий разбор `data[].thumbnails[0].imageUrl`.
async fn thumb_pairs(
    client: &reqwest::Client,
    url: &str,
    by_uid: bool,
) -> Option<HashMap<u64, String>> {
    let v: serde_json::Value = client
        .get(url)
        .send()
        .await
        .ok()?
        .error_for_status()
        .ok()?
        .json()
        .await
        .ok()?;
    let data = v["data"].as_array()?;
    let mut out = HashMap::new();
    for item in data {
        let Some(tid) = item["targetId"].as_u64() else { continue };
        let Some(list) = item["thumbnails"].as_array() else { continue };
        for t in list {
            if let Some(u) = t["imageUrl"].as_str() {
                if !u.is_empty() {
                    out.insert(tid, u.to_string());
                    break;
                }
            }
        }
        let _ = by_uid; // формат ответа одинаков; различие — только в URL
    }
    Some(out)
}

async fn download(client: &reqwest::Client, url: &str) -> Option<Vec<u8>> {
    let bytes = client
        .get(url)
        .send()
        .await
        .ok()?
        .error_for_status()
        .ok()?
        .bytes()
        .await
        .ok()?;
    Some(bytes.to_vec())
}

fn decode(bytes: &[u8]) -> Option<ColorImage> {
    let img = image::load_from_memory(bytes).ok()?.to_rgba8();
    Some(ColorImage::from_rgba_unmultiplied(
        [img.width() as usize, img.height() as usize],
        &img,
    ))
}

/// Обложка с диска: `Shell/thumbs/{id}.png`.
fn load_cached(base: &Path, id: &str) -> Option<ColorImage> {
    let bytes = std::fs::read(thumb_path(base, id)).ok()?;
    decode(&bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reco_list_is_unique() {
        let mut seen = HashSet::new();
        for (id, name) in RECO {
            assert!(seen.insert(*id), "дубль placeId {id} ({name})");
            assert!(!name.is_empty());
        }
        assert!(RECO.len() >= 6);
    }
}
