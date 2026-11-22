//! Список публичных серверов места (аналог «Server Region» в Bloxstrap):
//! публичный API games.roblox.com + запуск клиента в выбранный сервер по JobId.
//! Также определение региона и реального пинга активного игрового сервера.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use serde::Deserialize;

const UA: &str = "ByteStrap/1.0 (+https://github.com/mesorg-dev/ByteStrap)";

#[derive(Debug, Clone)]
pub struct ServerInfo {
    /// JobId сервера.
    pub id: String,
    pub playing: u32,
    pub max: u32,
    /// Пинг в мс (если API вернул).
    pub ping: Option<u32>,
    /// FPS сервера (если API вернул).
    pub fps: Option<f32>,
    /// Регион и флаг («🇩🇪 Франкфурт»).
    pub region: String,
    /// 0 = зелёный (<80ms), 1 = жёлтый (80-150ms), 2 = красный (>150ms), 3 = серый (нет данных)
    pub ping_kind: i32,
}

/// Информация об активном сервере, к которому подключен клиент Roblox.
#[derive(Debug, Clone)]
pub struct ActiveServer {
    pub ip: String,
    pub port: u16,
    pub region: String,
    pub detail: String,
    pub ping_ms: Option<u32>,
}

#[derive(Deserialize)]
struct RawResp {
    data: Vec<RawServer>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawServer {
    id: String,
    #[serde(default)]
    playing: u32,
    #[serde(default)]
    max_players: u32,
    #[serde(default)]
    ping: Option<u32>,
    #[serde(default)]
    fps: Option<f32>,
}

/// Ответ от ipinfo.io
#[derive(Debug, Deserialize)]
struct IpInfoResp {
    city: Option<String>,
    region: Option<String>,
    country: Option<String>,
}

/// Ответ от ip-api.com
#[derive(Debug, Deserialize)]
struct IpApiResp {
    status: Option<String>,
    country: Option<String>,
    #[serde(rename = "countryCode")]
    country_code: Option<String>,
    #[serde(rename = "regionName")]
    region_name: Option<String>,
    city: Option<String>,
}

static IP_CACHE: OnceLock<Mutex<HashMap<String, (String, String)>>> = OnceLock::new();

fn ip_cache() -> &'static Mutex<HashMap<String, (String, String)>> {
    IP_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn get_cached_location(ip: &str) -> Option<(String, String)> {
    ip_cache().lock().ok()?.get(ip).cloned()
}

pub fn save_cached_location(ip: &str, loc: (String, String)) {
    if let Ok(mut c) = ip_cache().lock() {
        c.insert(ip.to_string(), loc);
    }
}

pub fn get_cached_or_default(ip: &str) -> (String, String) {
    if let Some(cached) = get_cached_location(ip) {
        cached
    } else {
        ("Определение региона…".into(), format!("Подключение к {ip}"))
    }
}

/// Преобразование двухбуквенного ISO-кода страны в флаг-эмодзи (Regional Indicator Symbols).
pub fn country_flag(code: &str) -> String {
    let code = code.trim().to_ascii_uppercase();
    if code.len() == 2 {
        let mut chars = code.chars();
        if let (Some(c1), Some(c2)) = (chars.next(), chars.next()) {
            if c1.is_ascii_alphabetic() && c2.is_ascii_alphabetic() {
                let r1 = char::from_u32(0x1F1E6 + (c1 as u32 - 'A' as u32));
                let r2 = char::from_u32(0x1F1E6 + (c2 as u32 - 'A' as u32));
                if let (Some(r1), Some(r2)) = (r1, r2) {
                    return format!("{r1}{r2}");
                }
            }
        }
    }
    "🌐".to_string()
}

/// Локализация названия страны.
pub fn localize_country(code: &str, fallback: &str) -> String {
    match code.trim().to_ascii_uppercase().as_str() {
        "DE" => "Германия".into(),
        "US" => "США".into(),
        "GB" | "UK" => "Великобритания".into(),
        "FR" => "Франция".into(),
        "NL" => "Нидерланды".into(),
        "PL" => "Польша".into(),
        "SG" => "Сингапур".into(),
        "JP" => "Япония".into(),
        "AU" => "Австралия".into(),
        "BR" => "Бразилия".into(),
        "CA" => "Канада".into(),
        "KR" => "Южная Корея".into(),
        "FI" => "Финляндия".into(),
        "SE" => "Швеция".into(),
        "IE" => "Ирландия".into(),
        "ES" => "Испания".into(),
        "IT" => "Италия".into(),
        "IN" => "Индия".into(),
        "HK" => "Гонконг".into(),
        "AE" => "ОАЭ".into(),
        "TR" => "Турция".into(),
        "RU" => "Россия".into(),
        "UA" => "Украина".into(),
        "KZ" => "Казахстан".into(),
        _ => {
            if !fallback.is_empty() {
                fallback.into()
            } else {
                code.into()
            }
        }
    }
}

/// Локализация названия города.
pub fn localize_city(city: &str) -> String {
    let trimmed = city.trim();
    if trimmed.eq_ignore_ascii_case("Frankfurt am Main")
        || trimmed.eq_ignore_ascii_case("Frankfurt")
    {
        "Франкфурт".into()
    } else if trimmed.eq_ignore_ascii_case("Miami") {
        "Майами".into()
    } else if trimmed.eq_ignore_ascii_case("Ashburn") {
        "Ашберн".into()
    } else if trimmed.eq_ignore_ascii_case("Chicago") {
        "Чикаго".into()
    } else if trimmed.eq_ignore_ascii_case("Dallas") {
        "Даллас".into()
    } else if trimmed.eq_ignore_ascii_case("San Mateo") {
        "Сан-Матео".into()
    } else if trimmed.eq_ignore_ascii_case("Los Angeles") {
        "Лос-Анджелес".into()
    } else if trimmed.eq_ignore_ascii_case("New York")
        || trimmed.eq_ignore_ascii_case("New York City")
    {
        "Нью-Йорк".into()
    } else if trimmed.eq_ignore_ascii_case("Atlanta") {
        "Атланта".into()
    } else if trimmed.eq_ignore_ascii_case("Seattle") {
        "Сиэтл".into()
    } else if trimmed.eq_ignore_ascii_case("London") {
        "Лондон".into()
    } else if trimmed.eq_ignore_ascii_case("Paris") {
        "Париж".into()
    } else if trimmed.eq_ignore_ascii_case("Amsterdam") {
        "Амстердам".into()
    } else if trimmed.eq_ignore_ascii_case("Warsaw") {
        "Варшава".into()
    } else if trimmed.eq_ignore_ascii_case("Tokyo") {
        "Токио".into()
    } else if trimmed.eq_ignore_ascii_case("Singapore") {
        "Сингапур".into()
    } else if trimmed.eq_ignore_ascii_case("Sydney") {
        "Сидней".into()
    } else if trimmed.eq_ignore_ascii_case("Melbourne") {
        "Мельбурн".into()
    } else if trimmed.eq_ignore_ascii_case("Seoul") {
        "Сеул".into()
    } else if trimmed.eq_ignore_ascii_case("Toronto") {
        "Торонто".into()
    } else if trimmed.eq_ignore_ascii_case("Sao Paulo") {
        "Сан-Паулу".into()
    } else {
        trimmed.into()
    }
}

fn fallback_ip_location(ip: &str) -> (String, String) {
    ("🌐 Сервер Roblox".into(), format!("IP: {ip}"))
}

/// Асинхронное определение реальной страны и города по IP-адресу (ipinfo.io + ip-api.com).
pub async fn resolve_ip_location(ip: &str) -> (String, String) {
    if let Some(cached) = get_cached_location(ip) {
        return cached;
    }

    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(4))
        .connect_timeout(Duration::from_secs(3))
        .build()
    {
        Ok(c) => c,
        Err(_) => return fallback_ip_location(ip),
    };

    // 1. Попытка через ipinfo.io
    let ipinfo_url = format!("https://ipinfo.io/{ip}/json");
    if let Ok(resp) = client.get(&ipinfo_url).header("User-Agent", UA).send().await {
        if resp.status().is_success() {
            if let Ok(info) = resp.json::<IpInfoResp>().await {
                if let Some(country) = info.country.filter(|c| !c.is_empty()) {
                    let city = info.city.unwrap_or_default();
                    let region_name = info.region.unwrap_or_default();
                    let flag = country_flag(&country);
                    let country_ru = localize_country(&country, &country);
                    let city_ru = localize_city(&city);

                    let region_str = if !city_ru.is_empty() {
                        format!("{flag} {country_ru}, {city_ru}")
                    } else {
                        format!("{flag} {country_ru}")
                    };

                    let mut detail_parts = Vec::new();
                    if !city.is_empty() {
                        detail_parts.push(city);
                    }
                    if !region_name.is_empty()
                        && region_name != detail_parts.first().cloned().unwrap_or_default()
                    {
                        detail_parts.push(region_name);
                    }
                    detail_parts.push(country);
                    let detail_str = detail_parts.join(", ");

                    let result = (region_str, detail_str);
                    save_cached_location(ip, result.clone());
                    return result;
                }
            }
        }
    }

    // 2. Фолбэк через ip-api.com
    let ipapi_url =
        format!("http://ip-api.com/json/{ip}?fields=status,country,countryCode,regionName,city");
    if let Ok(resp) = client.get(&ipapi_url).header("User-Agent", UA).send().await {
        if resp.status().is_success() {
            if let Ok(info) = resp.json::<IpApiResp>().await {
                if info.status.as_deref() == Some("success") {
                    let country_code = info.country_code.unwrap_or_default();
                    let country_name = info.country.unwrap_or_default();
                    let city = info.city.unwrap_or_default();
                    let region_name = info.region_name.unwrap_or_default();

                    let flag = country_flag(&country_code);
                    let country_ru = localize_country(&country_code, &country_name);
                    let city_ru = localize_city(&city);

                    let region_str = if !city_ru.is_empty() {
                        format!("{flag} {country_ru}, {city_ru}")
                    } else {
                        format!("{flag} {country_ru}")
                    };

                    let mut detail_parts = Vec::new();
                    if !city.is_empty() {
                        detail_parts.push(city);
                    }
                    if !region_name.is_empty() {
                        detail_parts.push(region_name);
                    }
                    if !country_name.is_empty() {
                        detail_parts.push(country_name);
                    }
                    let detail_str = detail_parts.join(", ");

                    let result = (region_str, detail_str);
                    save_cached_location(ip, result.clone());
                    return result;
                }
            }
        }
    }

    let fallback = fallback_ip_location(ip);
    save_cached_location(ip, fallback.clone());
    fallback
}

pub fn resolve_ip_region(ip: &str) -> (String, String) {
    if let Some(cached) = get_cached_location(ip) {
        cached
    } else {
        fallback_ip_location(ip)
    }
}

/// Загрузить список публичных серверов места (лимит Roblox — 100).
pub async fn fetch(place_id: u64) -> anyhow::Result<Vec<ServerInfo>> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()?;
    let resp = client
        .get(format!(
            "https://games.roblox.com/v1/games/{place_id}/servers/Public?sortOrder=Asc&limit=100"
        ))
        .header("User-Agent", UA)
        .send()
        .await?;
    if !resp.status().is_success() {
        anyhow::bail!("HTTP {}", resp.status());
    }
    let raw: RawResp = resp.json().await?;
    Ok(raw
        .data
        .into_iter()
        .filter(|s| s.max_players > 0)
        .enumerate()
        .map(|(idx, s)| {
            let (region, ping_kind) = determine_region_and_kind(s.ping, &s.id, idx);
            ServerInfo {
                id: s.id,
                playing: s.playing,
                max: s.max_players,
                ping: s.ping,
                fps: s.fps,
                region,
                ping_kind,
            }
        })
        .collect())
}

/// Определение региона и градации пинга для строки сервера из публичного API.
fn determine_region_and_kind(ping: Option<u32>, _id: &str, _idx: usize) -> (String, i32) {
    match ping {
        Some(p) if p <= 65 => ("Близкий (EU)".into(), 0),
        Some(p) if p <= 130 => ("Средний (EU/US)".into(), 1),
        Some(_) => ("Дальний (US/Asia)".into(), 2),
        None => ("Облако Roblox".into(), 3),
    }
}

/// JobId сервера с наименьшим пингом (сначала заполненные, но не полные).
pub fn best(list: &[ServerInfo]) -> Option<&ServerInfo> {
    list.iter()
        .filter(|s| s.playing < s.max)
        .min_by_key(|s| (s.ping.unwrap_or(u32::MAX), u32::MAX - s.playing))
}

/// Ссылка `roblox://` для захода в конкретный сервер (без аккаунта).
pub fn server_uri(place_id: u64, job_id: &str) -> String {
    format!("roblox://placeId={place_id}&gameInstanceId={job_id}")
}

fn is_valid_ipv4(ip: &str) -> bool {
    let octets: Vec<&str> = ip.split('.').collect();
    if octets.len() == 4 && octets.iter().all(|o| o.parse::<u8>().is_ok()) {
        let first: u8 = octets[0].parse().unwrap_or(0);
        // Отсеиваем локальные и служебные диапазоны: 10.x.x.x (внутренняя сеть Roblox RCC),
        // 127.x.x.x (loopback), 192.168.x.x (домашняя сеть), 0.x.x.x.
        return first != 10 && first != 127 && first != 0 && !(first == 192 && octets[1] == "168");
    }
    false
}

/// Поиск реального публичного IP-адреса игрового сервера в строке лога Roblox.
pub fn find_server_ip(line: &str) -> Option<(String, u16)> {
    let lower = line.to_ascii_lowercase();

    // 1. UDMUX Address = 128.116.5.33, Port = 60718
    if let Some(p) = lower.find("udmux address = ") {
        let rest = &line[p + 16..];
        if let Some((ip_part, port_part)) = rest.split_once(',') {
            let ip = ip_part.trim();
            if let Some(port_idx) = port_part.to_ascii_lowercase().find("port = ") {
                let port_digits: String = port_part[port_idx + 7..]
                    .chars()
                    .take_while(|c| c.is_ascii_digit())
                    .collect();
                if let Ok(port) = port_digits.parse::<u16>() {
                    if is_valid_ipv4(ip) {
                        return Some((ip.to_string(), port));
                    }
                }
            }
        }
    }

    // 2. Стандартные маркеры подключения
    let pos = if let Some(p) = lower.find("connection accepted from ") {
        p + 25
    } else if let Some(p) = lower.find("creating connection to ") {
        p + 23
    } else if let Some(p) = lower.find("connecting to ") {
        p + 14
    } else if let Some(p) = lower.find("connection to ") {
        p + 14
    } else if let Some(p) = lower.find("serverid: ") {
        p + 10
    } else {
        return None;
    };

    let sub = line[pos..].trim_start();
    let end = sub
        .find(|c: char| {
            c.is_whitespace() || c == '.' && sub[sub.find('.').unwrap() + 1..].starts_with("..")
        })
        .unwrap_or(sub.len());
    let token = sub[..end].trim_matches(|c: char| {
        c == '.' || c == ',' || c == ';' || c == '!' || c == '|'
    });

    let parts: Vec<&str> = token.split(|c| c == ':' || c == '|').collect();
    if parts.len() == 2 {
        let ip = parts[0].trim();
        let port: u16 = parts[1].trim().parse().ok()?;
        if is_valid_ipv4(ip) {
            return Some((ip.to_string(), port));
        }
    }
    None
}

/// Замер реального пинга до IP-адреса сервера через быстрое TCP-подключение.
pub async fn measure_ping(ip: &str, port: u16) -> Option<u32> {
    let start = std::time::Instant::now();
    let addr = format!("{}:{}", ip, port);
    if let Ok(Ok(_)) = tokio::time::timeout(
        Duration::from_millis(1200),
        tokio::net::TcpStream::connect(&addr),
    )
    .await
    {
        return Some(start.elapsed().as_millis() as u32);
    }
    // Если игровой порт принимает только UDP, измеряем TCP RTT до веб/портов сервера
    let fallback = format!("{}:443", ip);
    let start2 = std::time::Instant::now();
    if let Ok(Ok(_)) = tokio::time::timeout(
        Duration::from_millis(1200),
        tokio::net::TcpStream::connect(&fallback),
    )
    .await
    {
        return Some(start2.elapsed().as_millis() as u32);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_country_flag() {
        assert_eq!(country_flag("DE"), "🇩🇪");
        assert_eq!(country_flag("US"), "🇺🇸");
        assert_eq!(country_flag("GB"), "🇬🇧");
        assert_eq!(country_flag("JP"), "🇯🇵");
        assert_eq!(country_flag("XX"), "🌐".to_string().as_str());
    }

    #[test]
    fn test_localize_country_and_city() {
        assert_eq!(localize_country("DE", "Germany"), "Германия");
        assert_eq!(localize_country("US", "United States"), "США");
        assert_eq!(localize_city("Frankfurt am Main"), "Франкфурт");
        assert_eq!(localize_city("Miami"), "Майами");
    }

    #[test]
    fn test_find_server_ip_udmux_and_accepted() {
        let line1 = "Info [DFLog::NetworkClient] Connection accepted from 128.116.5.33|60718";
        assert_eq!(
            find_server_ip(line1),
            Some(("128.116.5.33".to_string(), 60718))
        );

        let line2 = "[FLog::Network] UDMUX Address = 128.116.5.33, Port = 60718 | RCC Server Address = 10.206.10.207, Port = 60718";
        assert_eq!(
            find_server_ip(line2),
            Some(("128.116.5.33".to_string(), 60718))
        );

        let line3 = "Info [DFLog::RbxTransportRnaExp] Creating connection to 128.116.45.33:53142";
        assert_eq!(
            find_server_ip(line3),
            Some(("128.116.45.33".to_string(), 53142))
        );

        // Внутренний IP не должен считаться сервером
        let line_internal = "Connecting to 10.206.10.207:60718";
        assert_eq!(find_server_ip(line_internal), None);
    }
}
