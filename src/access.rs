//! `tg allow`, `tg deny`, `tg list` — mutations on the allowlist.
//! `tg pair`, `tg pending`, `tg reject` are added in Task 11.

use anyhow::{anyhow, Result};

use crate::api::Client;
use crate::config::{AllowEntry, Config};
use crate::paths;
use chrono::Utc;
use crate::pending::PendingStore;

/// Outcome of attempting to add a chat to the allowlist.
#[derive(Debug)]
pub enum AllowError {
    /// The chat_id is already in the allowlist.
    Duplicate(i64),
}

impl std::fmt::Display for AllowError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AllowError::Duplicate(id) => write!(f, "chat_id {id} already in allowlist"),
        }
    }
}

impl std::error::Error for AllowError {}

pub fn allow(chat_id: i64, label: Option<String>) -> Result<()> {
    let path = paths::config_path();
    let mut cfg = Config::load(&path)?;
    append_allow(&mut cfg, chat_id, label).map_err(|e| anyhow!("{e}"))?;
    cfg.save(&path)?;
    tracing::info!("allowlist mutation: added chat_id {chat_id}");
    println!("added chat_id {chat_id}");
    Ok(())
}

pub fn deny(chat_id: i64) -> Result<()> {
    let path = paths::config_path();
    let mut cfg = Config::load(&path)?;
    let before = cfg.allow.len();
    cfg.allow.retain(|e| e.chat_id != chat_id);
    if cfg.allow.len() == before {
        return Err(anyhow!("chat_id {chat_id} not in allowlist"));
    }
    cfg.save(&path)?;
    tracing::info!("allowlist mutation: removed chat_id {chat_id}");
    println!("removed chat_id {chat_id}");
    Ok(())
}

pub fn list() -> Result<()> {
    let cfg = Config::load(&paths::config_path())?;
    if cfg.allow.is_empty() {
        println!("(allowlist empty)");
        return Ok(());
    }
    for e in &cfg.allow {
        let label = e.label.as_deref().unwrap_or("(no label)");
        let suffix = if cfg.is_owner(e.chat_id) { "\t(owner)" } else { "" };
        println!("{}\t{}{}", e.chat_id, label, suffix);
    }
    Ok(())
}

pub fn set_owner(chat_id: Option<i64>, unset: bool) -> Result<()> {
    let path = paths::config_path();
    let mut cfg = Config::load(&path)?;
    match (chat_id, unset) {
        (Some(id), false) => {
            cfg.owner_chat_id = Some(id);
            // Owner ⊆ allowlist invariant: ensure the owner has an
            // entry. If they're already in [[allow]], leave the label
            // alone. If not, add them with label "owner".
            if !cfg.allow.iter().any(|e| e.chat_id == id) {
                cfg.allow.push(AllowEntry {
                    chat_id: id,
                    label: Some("owner".to_string()),
                });
            }
            cfg.save(&path)?;
            tracing::info!("owner mutation: set owner_chat_id to {id}");
            println!("owner_chat_id set to {id}");
        }
        (None, true) => {
            cfg.owner_chat_id = None;
            // Don't remove the allow entry — they're still a known
            // contact, just no longer the inbound-delivery owner.
            cfg.save(&path)?;
            tracing::info!("owner mutation: unset owner_chat_id");
            println!("owner_chat_id unset (all allowlisted senders will deliver)");
        }
        _ => return Err(anyhow!("pass exactly one of --chat-id N or --unset")),
    }
    Ok(())
}

// Used by the pair subcommand and the set_owner helper. Returns a
// typed error so callers can match on the duplicate case without
// stringly-typed grepping.
pub fn append_allow(cfg: &mut Config, chat_id: i64, label: Option<String>) -> std::result::Result<(), AllowError> {
    if cfg.allow.iter().any(|e| e.chat_id == chat_id) {
        return Err(AllowError::Duplicate(chat_id));
    }
    cfg.allow.push(AllowEntry { chat_id, label });
    Ok(())
}

// Reserved for symmetry with pair-related call sites.
#[allow(dead_code)]
pub fn client_from_config(cfg: &Config, api_base: &str) -> Client {
    Client::new(api_base, cfg.bot_token.clone())
}

pub fn pair(code: &str, api_base: &str) -> Result<()> {
    let pending_path = paths::pending_path();
    let mut store = PendingStore::load(&pending_path)?;
    let needle = code.to_uppercase();

    let (chat_id, entry) = match store.find_by_code(&needle) {
        Some((id, e)) => (id, e.clone()),
        None => return Err(anyhow!("unknown pairing code")),
    };
    if entry.expires_at < Utc::now() {
        return Err(anyhow!("pairing code expired"));
    }

    // Two-step write: append to allowlist first; remove from pending only
    // if the allow-append succeeded (or was already done on a previous
    // retry).
    let cfg_path = paths::config_path();
    let mut cfg = Config::load(&cfg_path)?;
    match append_allow(&mut cfg, chat_id, entry.username.clone()) {
        Ok(()) => {
            cfg.save(&cfg_path)?;
        }
        Err(AllowError::Duplicate(_)) => {
            // Already paired (race or rerun) — proceed to remove pending entry.
        }
    }
    store.remove(chat_id);
    store.save(&pending_path)?;

    // Notify on Telegram. Failure here doesn't roll back the pairing —
    // the chat_id is allowed regardless of whether the reply went out.
    let client = Client::new(api_base, cfg.bot_token.clone());
    if let Err(e) = client.send_message(chat_id, "Paired. You can now send messages.", None, None) {
        tracing::warn!("pair confirm reply failed for {chat_id}: {e}");
    }
    tracing::info!("pairing mutation: paired chat_id {chat_id}");
    println!("paired chat_id {chat_id}");
    Ok(())
}

pub fn pending() -> Result<()> {
    let store = PendingStore::load(&paths::pending_path())?;
    if store.entries.is_empty() {
        println!("(no pending pairings)");
        return Ok(());
    }
    let now = Utc::now();
    for (chat_id, e) in &store.entries {
        let remaining = e.expires_at.signed_duration_since(now);
        let label = e.username.as_deref().unwrap_or("(no username)");
        let status = if remaining.num_seconds() < 0 {
            "expired".to_string()
        } else {
            format!("{}m{}s remaining", remaining.num_minutes(), remaining.num_seconds() % 60)
        };
        println!("{}\t{}\t{}\t{}", chat_id, e.code, label, status);
    }
    Ok(())
}

pub fn reject(chat_id: i64) -> Result<()> {
    let path = paths::pending_path();
    let mut store = PendingStore::load(&path)?;
    if store.remove(chat_id).is_none() {
        return Err(anyhow!("chat_id {chat_id} not pending"));
    }
    store.save(&path)?;
    tracing::info!("pairing mutation: rejected pending chat_id {chat_id}");
    println!("dropped pending chat_id {chat_id}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    use crate::pending::PendingStore;
    use chrono::Utc;

    fn seed() {
        // Assumes the caller already set the TEST_HOME override via
        // crate::paths::test_helpers::set_test_tg_home before calling.
        let cfg = Config {
            schema_version: 1,
            bot_token: "T".into(),
            tmux_target: "x".into(),
            owner_chat_id: None,
            transcription: None,
            whisper_url: None,
            allow: vec![],
        };
        cfg.save(&paths::config_path()).unwrap();
    }

    #[test]
    fn allow_then_list() {
        let dir = tempdir().unwrap();
        let _home = crate::paths::test_helpers::set_test_tg_home(dir.path());
        seed();
        allow(42, Some("alice".into())).unwrap();
        let cfg = Config::load(&paths::config_path()).unwrap();
        assert!(cfg.is_allowed(42));
    }

    #[test]
    fn allow_refuses_duplicate() {
        let dir = tempdir().unwrap();
        let _home = crate::paths::test_helpers::set_test_tg_home(dir.path());
        seed();
        allow(42, None).unwrap();
        let err = allow(42, None).unwrap_err().to_string();
        assert!(err.contains("already"));
    }

    #[test]
    fn deny_removes_and_refuses_missing() {
        let dir = tempdir().unwrap();
        let _home = crate::paths::test_helpers::set_test_tg_home(dir.path());
        seed();
        allow(7, None).unwrap();
        deny(7).unwrap();
        let cfg = Config::load(&paths::config_path()).unwrap();
        assert!(!cfg.is_allowed(7));
        let err = deny(7).unwrap_err().to_string();
        assert!(err.contains("not in allowlist"));
    }

    #[test]
    fn pair_moves_pending_to_allow() {
        let dir = tempdir().unwrap();
        let _home = crate::paths::test_helpers::set_test_tg_home(dir.path());
        seed();
        let mut store = PendingStore::default();
        let entry = store.insert_new(42, Some("alice".into()), Utc::now()).clone();
        store.save(&paths::pending_path()).unwrap();

        // No real API needed: client.send_message will fail to connect
        // but pair() ignores reply failures (logs only). Point at an
        // unreachable URL.
        pair(&entry.code, "http://127.0.0.1:1").unwrap();
        let cfg = Config::load(&paths::config_path()).unwrap();
        assert!(cfg.is_allowed(42));
        let store2 = PendingStore::load(&paths::pending_path()).unwrap();
        assert!(store2.get(42).is_none());
    }

    #[test]
    fn pair_unknown_code_errors() {
        let dir = tempdir().unwrap();
        let _home = crate::paths::test_helpers::set_test_tg_home(dir.path());
        seed();
        let err = pair("ZZZZZZ", "http://127.0.0.1:1").unwrap_err().to_string();
        assert!(err.contains("unknown"));
    }

    #[test]
    fn set_owner_persists() {
        let dir = tempdir().unwrap();
        let _home = crate::paths::test_helpers::set_test_tg_home(dir.path());
        seed();
        set_owner(Some(99), false).unwrap();
        assert_eq!(Config::load(&paths::config_path()).unwrap().owner_chat_id, Some(99));
        set_owner(None, true).unwrap();
        assert_eq!(Config::load(&paths::config_path()).unwrap().owner_chat_id, None);
    }

    #[test]
    fn set_owner_rejects_both_and_neither() {
        let dir = tempdir().unwrap();
        let _home = crate::paths::test_helpers::set_test_tg_home(dir.path());
        seed();
        let err = set_owner(None, false).unwrap_err().to_string();
        assert!(err.contains("exactly one"));
    }

    #[test]
    fn reject_removes_pending() {
        let dir = tempdir().unwrap();
        let _home = crate::paths::test_helpers::set_test_tg_home(dir.path());
        seed();
        let mut store = PendingStore::default();
        store.insert_new(7, None, Utc::now());
        store.save(&paths::pending_path()).unwrap();

        reject(7).unwrap();
        let store2 = PendingStore::load(&paths::pending_path()).unwrap();
        assert!(store2.get(7).is_none());
    }

    #[test]
    fn set_owner_appends_to_allowlist_when_missing() {
        let dir = tempdir().unwrap();
        let _home = crate::paths::test_helpers::set_test_tg_home(dir.path());
        seed();
        set_owner(Some(99), false).unwrap();
        let cfg = Config::load(&paths::config_path()).unwrap();
        assert_eq!(cfg.owner_chat_id, Some(99));
        assert_eq!(cfg.allow.len(), 1, "owner should be auto-added to allowlist");
        assert_eq!(cfg.allow[0].chat_id, 99);
        assert_eq!(cfg.allow[0].label.as_deref(), Some("owner"));
    }

    #[test]
    fn set_owner_keeps_existing_entry_label() {
        let dir = tempdir().unwrap();
        let _home = crate::paths::test_helpers::set_test_tg_home(dir.path());
        seed();
        // Pre-add owner with a meaningful label.
        allow(99, Some("virgil".into())).unwrap();
        set_owner(Some(99), false).unwrap();
        let cfg = Config::load(&paths::config_path()).unwrap();
        assert_eq!(cfg.allow.len(), 1, "should NOT duplicate");
        assert_eq!(cfg.allow[0].label.as_deref(), Some("virgil"), "should preserve existing label");
    }

    #[test]
    fn unset_owner_keeps_allow_entry() {
        let dir = tempdir().unwrap();
        let _home = crate::paths::test_helpers::set_test_tg_home(dir.path());
        seed();
        set_owner(Some(99), false).unwrap();
        set_owner(None, true).unwrap();
        let cfg = Config::load(&paths::config_path()).unwrap();
        assert_eq!(cfg.owner_chat_id, None);
        assert_eq!(cfg.allow.len(), 1, "allow entry should NOT be removed on unset-owner");
    }
}
