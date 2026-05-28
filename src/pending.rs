//! ~/.tg/pending.json: chat_id -> pending pairing entry.

use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, Duration, Utc};
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

pub const CODE_LEN: usize = 6;
pub const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
pub const EXPIRY_HOURS: i64 = 1;
pub const REMINDER_THROTTLE_SECS: i64 = 30;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PendingEntry {
    pub code: String,
    pub username: Option<String>,
    pub first_seen_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub last_reminder_at: DateTime<Utc>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq)]
pub struct PendingStore {
    /// chat_id -> entry (chat_id as string because JSON map keys must be strings)
    #[serde(flatten)]
    pub entries: HashMap<String, PendingEntry>,
}

impl PendingStore {
    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() { return Ok(Self::default()); }
        let body = std::fs::read_to_string(path)
            .with_context(|| format!("reading {}", path.display()))?;
        let store: PendingStore = serde_json::from_str(&body)
            .with_context(|| format!("parsing {}", path.display()))?;
        Ok(store)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let parent = path.parent().ok_or_else(|| anyhow!("pending path has no parent"))?;
        std::fs::create_dir_all(parent)?;
        let body = serde_json::to_string_pretty(self)?;
        // Atomic write at 0600 — same pattern as config::Config::save.
        let tmp = parent.join(format!(".pending.json.tmp.{}", std::process::id()));
        {
            let mut f = std::fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .mode(0o600)
                .open(&tmp)?;
            f.write_all(body.as_bytes())?;
        }
        std::fs::rename(&tmp, path)?;
        Ok(())
    }

    pub fn get(&self, chat_id: i64) -> Option<&PendingEntry> {
        self.entries.get(&chat_id.to_string())
    }

    pub fn insert_new(&mut self, chat_id: i64, username: Option<String>, now: DateTime<Utc>) -> &PendingEntry {
        let entry = PendingEntry {
            code: generate_code(),
            username,
            first_seen_at: now,
            expires_at: now + Duration::hours(EXPIRY_HOURS),
            last_reminder_at: now,
        };
        self.entries.insert(chat_id.to_string(), entry);
        self.entries.get(&chat_id.to_string()).unwrap()
    }

    /// Remove the entry for `chat_id`; returns it if present.
    pub fn remove(&mut self, chat_id: i64) -> Option<PendingEntry> {
        self.entries.remove(&chat_id.to_string())
    }

    /// Find by code (case-insensitive). Returns (chat_id, entry).
    pub fn find_by_code(&self, code: &str) -> Option<(i64, &PendingEntry)> {
        let needle = code.to_uppercase();
        self.entries.iter().find_map(|(k, v)| {
            if v.code == needle {
                k.parse::<i64>().ok().map(|id| (id, v))
            } else { None }
        })
    }
}

pub fn generate_code() -> String {
    let mut rng = rand::thread_rng();
    (0..CODE_LEN).map(|_| {
        let i = rng.gen_range(0..ALPHABET.len());
        ALPHABET[i] as char
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn now() -> DateTime<Utc> { Utc::now() }

    #[test]
    fn generated_code_is_six_alnum_upper() {
        let c = generate_code();
        assert_eq!(c.len(), CODE_LEN);
        assert!(c.chars().all(|ch| ALPHABET.contains(&(ch as u8))));
    }

    #[test]
    fn save_then_load_roundtrips() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("pending.json");
        let mut s = PendingStore::default();
        s.insert_new(42, Some("alice".into()), now());
        s.save(&p).unwrap();
        let loaded = PendingStore::load(&p).unwrap();
        assert_eq!(s, loaded);
    }

    #[test]
    fn load_missing_file_returns_empty() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("does-not-exist.json");
        let s = PendingStore::load(&p).unwrap();
        assert!(s.entries.is_empty());
    }

    #[test]
    fn find_by_code_is_case_insensitive() {
        let mut s = PendingStore::default();
        let e = s.insert_new(42, None, now()).clone();
        let (id, found) = s.find_by_code(&e.code.to_lowercase()).unwrap();
        assert_eq!(id, 42);
        assert_eq!(found, &e);
    }

    #[test]
    fn remove_works() {
        let mut s = PendingStore::default();
        s.insert_new(7, None, now());
        assert!(s.remove(7).is_some());
        assert!(s.remove(7).is_none());
    }
}
