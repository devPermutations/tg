//! ~/.tg/pending.json: chat_id -> pending pairing entry.

use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, Duration, Utc};
use rand::Rng;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::HashMap;
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

pub const CODE_LEN: usize = 6;
pub(crate) const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
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

/// In-memory store keyed by i64 chat_id.
///
/// Wire format (JSON) uses string keys because JSON requires it.  The
/// custom Serialize/Deserialize impls convert between i64 and String so
/// that the rest of the codebase never has to call `.to_string()` on a
/// chat_id, and `find_by_code` never does an unchecked `.parse::<i64>()`.
///
/// Existing pending.json files (v0.5, string-keyed) load transparently —
/// `"42"` in the file deserializes as the i64 `42`.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct PendingStore {
    pub entries: HashMap<i64, PendingEntry>,
}

impl Serialize for PendingStore {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        // Serialize as a flat JSON object with string keys.
        let string_keyed: HashMap<String, &PendingEntry> =
            self.entries.iter().map(|(k, v)| (k.to_string(), v)).collect();
        string_keyed.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for PendingStore {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        // Deserialize a flat JSON object with string keys, then parse
        // each key as i64.  Non-integer keys are rejected with an error.
        let string_keyed = HashMap::<String, PendingEntry>::deserialize(deserializer)?;
        let mut entries = HashMap::with_capacity(string_keyed.len());
        for (k, v) in string_keyed {
            let id: i64 = k.parse().map_err(|_| {
                serde::de::Error::custom(format!("pending.json key is not a valid i64: {k:?}"))
            })?;
            entries.insert(id, v);
        }
        Ok(PendingStore { entries })
    }
}

impl PendingStore {
    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() { return Ok(Self::default()); }
        crate::paths::check_mode_strict(path)?;
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
            f.flush()?;
            f.sync_all()?;
        }
        std::fs::rename(&tmp, path)?;
        Ok(())
    }

    pub fn get(&self, chat_id: i64) -> Option<&PendingEntry> {
        self.entries.get(&chat_id)
    }

    pub fn insert_new(&mut self, chat_id: i64, username: Option<String>, now: DateTime<Utc>) -> &PendingEntry {
        let entry = PendingEntry {
            code: generate_code(),
            username,
            first_seen_at: now,
            expires_at: now + Duration::hours(EXPIRY_HOURS),
            last_reminder_at: now,
        };
        self.entries.insert(chat_id, entry);
        self.entries.get(&chat_id).unwrap()
    }

    /// Remove the entry for `chat_id`; returns it if present.
    pub fn remove(&mut self, chat_id: i64) -> Option<PendingEntry> {
        self.entries.remove(&chat_id)
    }

    /// Find by code (case-insensitive). Returns (chat_id, entry).
    pub fn find_by_code(&self, code: &str) -> Option<(i64, &PendingEntry)> {
        let needle = code.to_uppercase();
        self.entries.iter().find_map(|(k, v)| {
            if v.code == needle {
                Some((*k, v))
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

    #[test]
    fn save_sets_mode_0600() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempdir().unwrap();
        let p = dir.path().join("pending.json");
        let mut s = PendingStore::default();
        s.insert_new(1, None, now());
        s.save(&p).unwrap();
        let mode = std::fs::metadata(&p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }

    #[test]
    fn legacy_string_keyed_json_still_parses() {
        // pending.json files written by v0.5 had string-keyed JSON
        // objects (because JSON requires string keys). The runtime type
        // is now HashMap<i64, V> but the wire format hasn't changed.
        // This test pins that backward-compat property.
        let body = r#"{
  "8583339367": {
    "code": "K7M3P2",
    "username": "alice",
    "first_seen_at": "2026-05-28T00:42:11Z",
    "expires_at": "2026-05-28T01:42:11Z",
    "last_reminder_at": "2026-05-28T00:42:11Z"
  }
}"#;
        let store: PendingStore = serde_json::from_str(body).unwrap();
        assert_eq!(store.entries.len(), 1);
        assert!(store.get(8583339367).is_some());
    }
}
