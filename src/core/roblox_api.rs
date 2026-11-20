//! Roblox Web API: получение списка онлайн-друзей с их текущей игрой
//! и получение недавних / избранных игр активного аккаунта по `.ROBLOSECURITY`.

use serde::{Deserialize, Serialize};

const UA: &str = "ByteStrap/1.0 (+https://github.com/mesorg-dev/ByteStrap)";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OnlineFriend {
    pub id: u64,
    pub name: String,
    pub display_name: String,
    /// 0 — offline, 1 — online, 2 — in_game, 3 — in_studio
    pub presence_type: u32,
    pub last_location: String,
    pub place_id: u64,
    pub game_id: String, // JobId
    pub avatar_url: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RecentGame {
    pub place_id: u64,
    pub name: String,
    pub creator: String,
    pub is_fav: bool,
}

#[derive(Deserialize)]
struct AuthUserResp {
    id: u64,
    #[allow(dead_code)]
    name: Option<String>,
}

#[derive(Deserialize)]
struct FriendItem {
    id: u64,
    name: Option<String>,
    #[serde(rename = "displayName")]
    display_name: Option<String>,
    #[serde(rename = "userPresenceType")]
    user_presence_type: Option<u32>,
    #[serde(rename = "lastLocation")]
    last_location: Option<String>,
    #[serde(rename = "placeId")]
    place_id: Option<u64>,
    #[serde(rename = "gameId")]
    game_id: Option<String>,
}

#[derive(Deserialize)]
struct FriendsListResp {
    data: Option<Vec<FriendItem>>,
}

#[derive(Deserialize)]
struct FavGameItem {
    id: Option<u64>,
    name: Option<String>,
    #[serde(rename = "rootPlace")]
    root_place: Option<FavRootPlace>,
}

#[derive(Deserialize)]
struct FavRootPlace {
    id: Option<u64>,
}

#[derive(Deserialize)]
struct FavGamesResp {
    data: Option<Vec<FavGameItem>>,
}

/// Получить текущий userId по куке.
pub async fn get_user_id(cookie: &str) -> Option<u64> {
    let client = reqwest::Client::new();
    let resp = client
        .get("https://users.roblox.com/v1/users/authenticated")
        .header("Cookie", format!(".ROBLOSECURITY={cookie}"))
        .header("User-Agent", UA)
        .send()
        .await
        .ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let u: AuthUserResp = resp.json().await.ok()?;
    Some(u.id)
}

/// Получить список друзей онлайн через Roblox API.
pub async fn fetch_online_friends(cookie: &str) -> Vec<OnlineFriend> {
    let Some(user_id) = get_user_id(cookie).await else {
        return Vec::new();
    };

    let client = reqwest::Client::new();
    let url = format!("https://friends.roblox.com/v1/users/{user_id}/friends/online");
    let Ok(resp) = client
        .get(&url)
        .header("Cookie", format!(".ROBLOSECURITY={cookie}"))
        .header("User-Agent", UA)
        .send()
        .await
    else {
        return Vec::new();
    };

    if !resp.status().is_success() {
        return Vec::new();
    }

    let Ok(flist) = resp.json::<FriendsListResp>().await else {
        return Vec::new();
    };

    let raw_friends = flist.data.unwrap_or_default();
    if raw_friends.is_empty() {
        return Vec::new();
    }

    let mut result: Vec<OnlineFriend> = raw_friends
        .into_iter()
        .map(|f| OnlineFriend {
            id: f.id,
            name: f.name.unwrap_or_default(),
            display_name: f.display_name.unwrap_or_default(),
            presence_type: f.user_presence_type.unwrap_or(1),
            last_location: f.last_location.unwrap_or_default(),
            place_id: f.place_id.unwrap_or(0),
            game_id: f.game_id.unwrap_or_default(),
            avatar_url: String::new(),
        })
        .collect();

    // Сортировка: сначала те, кто в игре (presence_type == 2), затем остальные
    result.sort_by(|a, b| {
        let a_ingame = a.presence_type == 2;
        let b_ingame = b.presence_type == 2;
        b_ingame.cmp(&a_ingame).then_with(|| a.display_name.cmp(&b.display_name))
    });

    result
}

/// Получить избранные / недавние игры аккаунта по куке.
pub async fn fetch_account_games(cookie: &str) -> Vec<RecentGame> {
    let Some(user_id) = get_user_id(cookie).await else {
        return Vec::new();
    };

    let client = reqwest::Client::new();
    let url = format!("https://games.roblox.com/v2/users/{user_id}/favorite/games?sortOrder=Asc&limit=12");
    let Ok(resp) = client
        .get(&url)
        .header("Cookie", format!(".ROBLOSECURITY={cookie}"))
        .header("User-Agent", UA)
        .send()
        .await
    else {
        return Vec::new();
    };

    if !resp.status().is_success() {
        return Vec::new();
    }

    let Ok(fav_resp) = resp.json::<FavGamesResp>().await else {
        return Vec::new();
    };

    fav_resp
        .data
        .unwrap_or_default()
        .into_iter()
        .filter_map(|g| {
            let place_id = g.root_place.and_then(|r| r.id).or(g.id)?;
            let name = g.name.unwrap_or_else(|| format!("Place {place_id}"));
            Some(RecentGame {
                place_id,
                name,
                creator: String::new(),
                is_fav: true,
            })
        })
        .collect()
}
