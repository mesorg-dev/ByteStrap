//! Разбор ссылок Roblox в аргумент запуска плеера.
//!
//! `roblox://` уходит плееру как есть; `https://…roblox.com/games/<id>/…`
//! (страница плейса, приватка, конкретный сервер) собирается в deep link.
//! Глобальный перехват https в Windows невозможен — поэтому это ядро
//! поля «Запуск по ссылке» на Home.

/// Разобрать пользовательскую строку в аргумент для `play_arg`.
///
/// - `roblox://experiences/start?…` — как есть;
/// - `https://www.roblox.com/games/123/Name?privateServerLinkCode=…` —
///   `roblox://experiences/start?placeId=123&privateServerLinkCode=…`;
/// - схема http(s) и www/web необязательны;
/// - чужой домен или мусор — `None`.
pub fn parse_arg(input: &str) -> Option<String> {
    let s = input.trim();
    if s.is_empty() {
        return None;
    }
    if s.starts_with("roblox://") {
        return Some(s.to_string());
    }

    // Схема опциональна (копируют без https://).
    let rest = s.strip_prefix("https://")
        .or_else(|| s.strip_prefix("http://"))
        .unwrap_or(s);

    let (authority, tail) = match rest.find(['/', '?', '#']) {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, ""),
    };
    // user:info@host:port → host
    let host = authority.rsplit('@').next()?.split(':').next()?.to_ascii_lowercase();
    if host != "roblox.com" && !host.ends_with(".roblox.com") {
        return None;
    }

    let path_query = tail.split('#').next()?; // без фрагмента
    let (path, query) = match path_query.split_once('?') {
        Some((p, q)) => (p, q),
        None => (path_query, ""),
    };

    // /games/<placeId>/…
    let mut segs = path.split('/').filter(|x| !x.is_empty());
    if segs.next()? != "games" {
        return None;
    }
    let place = segs.next()?;
    if place.is_empty() || !place.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }

    let mut game_instance = None;
    let mut private_code = None;
    for pair in query.split('&').filter(|p| !p.is_empty()) {
        let (k, v) = match pair.split_once('=') {
            Some((k, v)) => (k, v),
            None => (pair, ""),
        };
        let k = pct_decode(k);
        let v = pct_decode(v);
        if v.is_empty() {
            continue;
        }
        match k.as_str() {
            "gameInstanceId" => game_instance = Some(v),
            "privateServerLinkCode" => private_code = Some(v),
            _ => {}
        }
    }

    Some(if let Some(g) = game_instance {
        format!("roblox://experiences/start?placeId={place}&gameInstanceId={g}")
    } else if let Some(c) = private_code {
        format!("roblox://experiences/start?placeId={place}&privateServerLinkCode={c}")
    } else {
        format!("roblox://placeId={place}")
    })
}

/// Декодирует `%XX` в байтах (не трогает `+` и обычные символы).
fn pct_decode(s: &str) -> String {
    if !s.contains('%') {
        return s.to_string();
    }
    fn hex(b: u8) -> Option<u8> {
        match b {
            b'0'..=b'9' => Some(b - b'0'),
            b'a'..=b'f' => Some(b - b'a' + 10),
            b'A'..=b'F' => Some(b - b'A' + 10),
            _ => None,
        }
    }
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let (Some(h), Some(l)) = (hex(b[i + 1]), hex(b[i + 2])) {
                out.push((h << 4) | l);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_place_page() {
        assert_eq!(
            parse_arg("https://www.roblox.com/games/286090429/Adopt-Me!/").as_deref(),
            Some("roblox://placeId=286090429")
        );
        assert_eq!(
            parse_arg("https://web.roblox.com/games/123").as_deref(),
            Some("roblox://placeId=123")
        );
        // Без схемы и с фрагментом.
        assert_eq!(
            parse_arg("www.roblox.com/games/555/#section").as_deref(),
            Some("roblox://placeId=555")
        );
    }

    #[test]
    fn private_server_link() {
        assert_eq!(
            parse_arg("https://www.roblox.com/games/994732206/BedWars?privateServerLinkCode=ABC123").as_deref(),
            Some("roblox://experiences/start?placeId=994732206&privateServerLinkCode=ABC123")
        );
    }

    #[test]
    fn game_instance_link_decodes_percent() {
        assert_eq!(
            parse_arg(
                "https://www.roblox.com/games/123/x?gameInstanceId=ab%2Dcd%2Def"
            )
            .as_deref(),
            Some("roblox://experiences/start?placeId=123&gameInstanceId=ab-cd-ef")
        );
    }

    #[test]
    fn roblox_scheme_passes_through() {
        assert_eq!(
            parse_arg("roblox://experiences/start?placeId=7").as_deref(),
            Some("roblox://experiences/start?placeId=7")
        );
        assert_eq!(parse_arg("  roblox://placeId=8  ").as_deref(), Some("roblox://placeId=8"));
    }

    #[test]
    fn rejects_foreign_and_garbage() {
        assert_eq!(parse_arg("https://youtube.com/watch?v=dQw4w9WgXcQ"), None);
        assert_eq!(parse_arg("https://www.roblox.com/catalog/123/Item"), None);
        assert_eq!(parse_arg("https://www.roblox.com/games/abc"), None);
        assert_eq!(parse_arg("https://www.roblox.com/games"), None);
        assert_eq!(parse_arg(""), None);
        assert_eq!(parse_arg("просто текст"), None);
    }
}
