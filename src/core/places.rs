//! Избранное и недавние места Roblox: быстрый запуск по placeId.
//!
//! Хранение — `places.json` в базе лаунчера; имя места подтягивается
//! из Roblox API в фоне (как в playtime).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Одна запись: место + как часто запускали.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PlaceEntry {
    pub id: u64,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub fav: bool,
    #[serde(default)]
    pub visits: u32,
    #[serde(default)]
    pub unix: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Places {
    #[serde(default)]
    pub items: Vec<PlaceEntry>,
}

const CAP: usize = 40;

impl Places {
    pub fn file(base: &Path) -> PathBuf {
        base.join("places.json")
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

    /// Зарегистрировать запуск: в начало, дедуп, счётчик, ограничение списка.
    pub fn launch(&mut self, id: u64, base: &Path) {
        let now = crate::core::playtime::now_unix();
        let prev = self.items.iter().find(|e| e.id == id).cloned();
        self.items.retain(|e| e.id != id);
        let mut entry =
            PlaceEntry { id, name: String::new(), fav: false, visits: 1, unix: now };
        if let Some(p) = prev {
            entry.visits += p.visits;
            entry.name = p.name;
            entry.fav = p.fav;
        }
        self.items.insert(0, entry);
        if self.items.len() > CAP {
            self.items.truncate(CAP);
        }
        self.save(base);
    }

    /// Записать разрешённое имя (если запись есть).
    pub fn set_name(&mut self, id: u64, name: &str, base: &Path) {
        if let Some(e) = self.items.iter_mut().find(|e| e.id == id) {
            e.name = name.to_string();
            self.save(base);
        }
    }

    pub fn toggle_fav(&mut self, id: u64, base: &Path) {
        if let Some(e) = self.items.iter_mut().find(|e| e.id == id) {
            e.fav = !e.fav;
            self.save(base);
        }
    }

    pub fn remove(&mut self, id: u64, base: &Path) {
        self.items.retain(|e| e.id != id);
        self.save(base);
    }

    /// Избранные первыми, затем по свежести.
    pub fn sorted(&self) -> Vec<&PlaceEntry> {
        let mut v: Vec<&PlaceEntry> = self.items.iter().collect();
        v.sort_by(|a, b| {
            b.fav.cmp(&a.fav).then(b.unix.cmp(&a.unix))
        });
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    #[test]
    fn launch_dedup_and_visits() {
        let dir = tmp();
        let mut p = Places::default();
        p.launch(111, dir.path());
        p.launch(222, dir.path());
        p.launch(111, dir.path());
        assert_eq!(p.items.len(), 2);
        assert_eq!(p.items[0].id, 111);
        assert_eq!(p.items[0].visits, 2);
        let loaded = Places::load(dir.path());
        assert_eq!(loaded.items[0].visits, 2);
    }

    #[test]
    fn fav_and_remove() {
        let dir = tmp();
        let mut p = Places::default();
        p.launch(111, dir.path());
        p.launch(222, dir.path());
        p.toggle_fav(222, dir.path());
        let s = p.sorted();
        assert_eq!(s[0].id, 222);
        assert!(s[0].fav);
        p.remove(111, dir.path());
        assert_eq!(p.items.len(), 1);
    }

    #[test]
    fn cap_limits() {
        let dir = tmp();
        let mut p = Places::default();
        for i in 0..60 {
            p.launch(i, dir.path());
        }
        assert_eq!(p.items.len(), CAP);
    }
}
