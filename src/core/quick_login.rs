//! Roblox Quick Login (Cross-Device Authentication)
//! Создание сессии, генерация QR-кода (SVG для Slint), опрос статуса, обмен токена на .ROBLOSECURITY.

use qrcode::QrCode;
use qrcode::render::svg;

const UA: &str = "ByteStrap/1.0 (+https://github.com/mesorg-dev/ByteStrap)";

#[derive(Debug, Clone)]
pub struct QuickLoginSession {
    pub code: String,
    pub private_key: String,
    pub qr_svg: String,
    pub expires_at: String,
}

#[derive(Debug, Clone)]
pub enum QuickLoginStatus {
    Waiting,
    Linked { name: String, pic_url: String },
    Success { cookie: String, username: String },
    Expired,
    Error(String),
}

/// Начать новую сессию Quick Sign-In
pub async fn create_session() -> Result<QuickLoginSession, String> {
    let client = reqwest::Client::new();
    let resp = client
        .post("https://apis.roblox.com/auth-token-service/v1/login/create")
        .header("User-Agent", UA)
        .header("Content-Type", "application/json")
        .body("{}")
        .send()
        .await
        .map_err(|e| format!("Сеть недоступна: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!("Roblox API error: HTTP {}", resp.status()));
    }

    let json: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("Некорректный JSON: {e}"))?;

    let code = json["code"]
        .as_str()
        .ok_or_else(|| "Отсутствует code в ответе".to_string())?
        .to_string();
    let private_key = json["privateKey"]
        .as_str()
        .ok_or_else(|| "Отсутствует privateKey в ответе".to_string())?
        .to_string();
    let expires_at = json["expirationTime"]
        .as_str()
        .unwrap_or("")
        .to_string();

    // Генерация QR-кода с адресом https://www.roblox.com/crossdevice?code={code}
    let qr_url = format!("https://www.roblox.com/crossdevice?code={code}");
    let qr_svg = match QrCode::new(qr_url.as_bytes()) {
        Ok(qr) => qr
            .render::<svg::Color>()
            .min_dimensions(180, 180)
            .dark_color(svg::Color("#ffffff"))
            .light_color(svg::Color("#00000000")) // прозрачный фон
            .build(),
        Err(e) => return Err(format!("QR code error: {e}")),
    };

    Ok(QuickLoginSession {
        code,
        private_key,
        qr_svg,
        expires_at,
    })
}

/// Проверить статус сессии один раз (с обработкой CSRF)
pub async fn check_status(code: &str, private_key: &str) -> Result<QuickLoginStatus, String> {
    let client = reqwest::Client::new();
    let body = serde_json::json!({
        "code": code,
        "privateKey": private_key
    });

    let send_req = |csrf: Option<String>| {
        let mut req = client
            .post("https://apis.roblox.com/auth-token-service/v1/login/status")
            .header("User-Agent", UA)
            .header("Content-Type", "application/json");
        if let Some(c) = csrf {
            req = req.header("x-csrf-token", c);
        }
        req.json(&body).send()
    };

    let mut resp = send_req(None).await.map_err(|e| e.to_string())?;

    // Обработка 403 CSRF challenge
    if resp.status().as_u16() == 403 {
        if let Some(csrf) = resp.headers().get("x-csrf-token").and_then(|v| v.to_str().ok()) {
            resp = send_req(Some(csrf.to_string()))
                .await
                .map_err(|e| e.to_string())?;
        }
    }

    if !resp.status().is_success() {
        if resp.status().as_u16() == 500 {
            return Ok(QuickLoginStatus::Expired);
        }
        return Err(format!("HTTP {}", resp.status()));
    }

    let val: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    let status_str = val["status"].as_str().unwrap_or("");

    match status_str {
        "Created" => Ok(QuickLoginStatus::Waiting),
        "UserLinked" => {
            let name = val["accountName"].as_str().unwrap_or("").to_string();
            let pic_url = val["accountPictureUrl"].as_str().unwrap_or("").to_string();
            Ok(QuickLoginStatus::Linked { name, pic_url })
        }
        "Validated" => {
            // Пользователь подтвердил вход! Обмениваем код и ключ на .ROBLOSECURITY куку
            let cookie = exchange_cookie(code, private_key).await?;
            let username = match crate::core::accounts::fetch_username(&cookie).await {
                Some(u) => u,
                None => {
                    let acc_name = val["accountName"].as_str().unwrap_or("").to_string();
                    if !acc_name.is_empty() {
                        acc_name
                    } else {
                        "RobloxUser".to_string()
                    }
                }
            };
            Ok(QuickLoginStatus::Success { cookie, username })
        }
        "Expired" | "Cancelled" => Ok(QuickLoginStatus::Expired),
        other => Err(format!("Неизвестный статус: {other}")),
    }
}

/// Обменять validated token на .ROBLOSECURITY через https://auth.roblox.com/v2/login
async fn exchange_cookie(code: &str, private_key: &str) -> Result<String, String> {
    let client = reqwest::Client::new();
    let body = serde_json::json!({
        "ctype": "AuthToken",
        "cvalue": code,
        "password": private_key
    });

    let send_req = |csrf: Option<String>| {
        let mut req = client
            .post("https://auth.roblox.com/v2/login")
            .header("User-Agent", UA)
            .header("Content-Type", "application/json");
        if let Some(c) = csrf {
            req = req.header("x-csrf-token", c);
        }
        req.json(&body).send()
    };

    let mut resp = send_req(None).await.map_err(|e| e.to_string())?;

    if resp.status().as_u16() == 403 {
        if let Some(csrf) = resp.headers().get("x-csrf-token").and_then(|v| v.to_str().ok()) {
            resp = send_req(Some(csrf.to_string()))
                .await
                .map_err(|e| e.to_string())?;
        }
    }

    if !resp.status().is_success() {
        return Err(format!("Обмен токена не удался: HTTP {}", resp.status()));
    }

    // Ищем .ROBLOSECURITY в Set-Cookie
    for val in resp.headers().get_all("set-cookie") {
        if let Ok(s) = val.to_str() {
            if s.contains(".ROBLOSECURITY=") {
                for part in s.split(';') {
                    let part = part.trim();
                    if let Some(cookie) = part.strip_prefix(".ROBLOSECURITY=") {
                        return Ok(cookie.to_string());
                    }
                }
            }
        }
    }

    Err("Кука .ROBLOSECURITY не найдена в ответе auth.roblox.com".into())
}
