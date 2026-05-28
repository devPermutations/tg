//! `tg allow`, `tg deny`, `tg list` — mutations on the allowlist.
//! `tg pair`, `tg pending`, `tg reject` are added in Task 11.

use anyhow::{anyhow, Result};

use crate::api::Client;
use crate::config::{AllowEntry, Config};
use crate::paths;
use chrono::Utc;
use crate::pending::PendingStore;

pub fn allow(chat_id: i64, label: Option<String>) -> Result<()> {
    let path = paths::config_path();
    let mut cfg = Config::load(&path)?;
    if cfg.allow.iter().any(|e| e.chat_id == chat_id) {
        return Err(anyhow!("chat_id {chat_id} already in allowlist"));
    }
    cfg.allow.push(AllowEntry { chat_id, label });
    cfg.save(&path)?;
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
    println!("removed chat_id {chat_id}");
    Ok(())
}

pub fn list() -> Result<()> {
    let cfg = Config::load(&paths::config_path())?;
    if cfg.allow.is_empty() && cfg.owner_chat_id.is_none() {
        println!("(allowlist empty, no owner set)");
        return Ok(());
    }
    if let Some(owner) = cfg.owner_chat_id {
        if !cfg.allow.iter().any(|e| e.chat_id == owner) {
            println!("{}\t(owner, no allow entry)\t(owner)", owner);
        }
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
            cfg.save(&path)?;
            println!("owner_chat_id set to {id}");
        }
        (None, true) => {
            cfg.owner_chat_id = None;
            cfg.save(&path)?;
            println!("owner_chat_id unset (all allowlisted senders will deliver)");
        }
        _ => return Err(anyhow!("pass exactly one of --chat-id N or --unset")),
    }
    Ok(())
}

// Used by the pair subcommand (Task 11) and the listen daemon (Task 13).
pub fn append_allow(cfg: &mut Config, chat_id: i64, label: Option<String>) -> Result<()> {
    if cfg.allow.iter().any(|e| e.chat_id == chat_id) {
        return Err(anyhow!("chat_id {chat_id} already in allowlist"));
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
        Err(e) if e.to_string().contains("already") => {
            // Already paired (race or rerun) — proceed to remove pending entry.
        }
        Err(e) => return Err(e),
    }
    store.remove(chat_id);
    store.save(&pending_path)?;

    // Notify on Telegram. Failure here doesn't roll back the pairing —
    // the chat_id is allowed regardless of whether the reply went out.
    let client = Client::new(api_base, cfg.bot_token.clone());
    if let Err(e) = client.send_message(chat_id, "Paired. You can now send messages.", None, None) {
        tracing::warn!("pair confirm reply failed for {chat_id}: {e}");
    }
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
    println!("dropped pending chat_id {chat_id}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    use crate::pending::PendingStore;
    use chrono::Utc;

    fn seed(home: &std::path::Path) {
        std::env::set_var("TG_HOME", home);
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
        let _g = crate::paths::test_lock::acquire();
        let dir = tempdir().unwrap();
        seed(dir.path());
        let result = allow(42, Some("alice".into()));
        let cfg_result = Config::load(&paths::config_path());
        std::env::remove_var("TG_HOME");

        result.unwrap();
        let cfg = cfg_result.unwrap();
        assert!(cfg.is_allowed(42));
    }

    #[test]
    fn allow_refuses_duplicate() {
        let _g = crate::paths::test_lock::acquire();
        let dir = tempdir().unwrap();
        seed(dir.path());
        let first = allow(42, None);
        let second = allow(42, None);
        std::env::remove_var("TG_HOME");

        first.unwrap();
        let err = second.unwrap_err().to_string();
        assert!(err.contains("already"));
    }

    #[test]
    fn deny_removes_and_refuses_missing() {
        let _g = crate::paths::test_lock::acquire();
        let dir = tempdir().unwrap();
        seed(dir.path());
        let add = allow(7, None);
        let remove1 = deny(7);
        let cfg_result = Config::load(&paths::config_path());
        let remove2 = deny(7);
        std::env::remove_var("TG_HOME");

        add.unwrap();
        remove1.unwrap();
        let cfg = cfg_result.unwrap();
        assert!(!cfg.is_allowed(7));
        let err = remove2.unwrap_err().to_string();
        assert!(err.contains("not in allowlist"));
    }

    #[test]
    fn pair_moves_pending_to_allow() {
        let _g = crate::paths::test_lock::acquire();
        let dir = tempdir().unwrap();
        seed(dir.path());
        let mut store = PendingStore::default();
        let entry = store.insert_new(42, Some("alice".into()), Utc::now()).clone();
        store.save(&paths::pending_path()).unwrap();

        // No real API needed: client.send_message will fail to connect
        // but pair() ignores reply failures (logs only). Point at an
        // unreachable URL.
        let pair_result = pair(&entry.code, "http://127.0.0.1:1");
        let cfg_result = Config::load(&paths::config_path());
        let store2_result = PendingStore::load(&paths::pending_path());
        std::env::remove_var("TG_HOME");

        pair_result.unwrap();
        let cfg = cfg_result.unwrap();
        assert!(cfg.is_allowed(42));
        let store2 = store2_result.unwrap();
        assert!(store2.get(42).is_none());
    }

    #[test]
    fn pair_unknown_code_errors() {
        let _g = crate::paths::test_lock::acquire();
        let dir = tempdir().unwrap();
        seed(dir.path());
        let result = pair("ZZZZZZ", "http://127.0.0.1:1");
        std::env::remove_var("TG_HOME");

        let err = result.unwrap_err().to_string();
        assert!(err.contains("unknown"));
    }

    #[test]
    fn set_owner_persists() {
        let _g = crate::paths::test_lock::acquire();
        let dir = tempdir().unwrap();
        seed(dir.path());
        let set_result = set_owner(Some(99), false);
        let cfg_after_set = Config::load(&paths::config_path());
        let unset_result = set_owner(None, true);
        let cfg_after_unset = Config::load(&paths::config_path());
        std::env::remove_var("TG_HOME");

        set_result.unwrap();
        assert_eq!(cfg_after_set.unwrap().owner_chat_id, Some(99));
        unset_result.unwrap();
        assert_eq!(cfg_after_unset.unwrap().owner_chat_id, None);
    }

    #[test]
    fn set_owner_rejects_both_and_neither() {
        let _g = crate::paths::test_lock::acquire();
        let dir = tempdir().unwrap();
        seed(dir.path());
        let neither = set_owner(None, false);
        std::env::remove_var("TG_HOME");

        let err = neither.unwrap_err().to_string();
        assert!(err.contains("exactly one"));
    }

    #[test]
    fn reject_removes_pending() {
        let _g = crate::paths::test_lock::acquire();
        let dir = tempdir().unwrap();
        seed(dir.path());
        let mut store = PendingStore::default();
        store.insert_new(7, None, Utc::now());
        store.save(&paths::pending_path()).unwrap();

        let result = reject(7);
        let store2_result = PendingStore::load(&paths::pending_path());
        std::env::remove_var("TG_HOME");

        result.unwrap();
        let store2 = store2_result.unwrap();
        assert!(store2.get(7).is_none());
    }
}
