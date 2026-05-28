use anyhow::{anyhow, Result};
use chrono::Utc;

use crate::api::Client;
use crate::config::Config;
use crate::paths;
use crate::pending::PendingStore;

use super::allowlist::{append_allow, AllowError};

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
    use crate::config::Config;

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
        cfg.save(&crate::paths::config_path()).unwrap();
    }

    #[test]
    fn pair_moves_pending_to_allow() {
        let dir = tempdir().unwrap();
        let _home = crate::paths::test_helpers::set_test_tg_home(dir.path());
        seed();
        let mut store = PendingStore::default();
        let entry = store.insert_new(42, Some("alice".into()), Utc::now()).clone();
        store.save(&crate::paths::pending_path()).unwrap();

        // No real API needed: client.send_message will fail to connect
        // but pair() ignores reply failures (logs only). Point at an
        // unreachable URL.
        pair(&entry.code, "http://127.0.0.1:1").unwrap();
        let cfg = Config::load(&crate::paths::config_path()).unwrap();
        assert!(cfg.is_allowed(42));
        let store2 = PendingStore::load(&crate::paths::pending_path()).unwrap();
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
    fn reject_removes_pending() {
        let dir = tempdir().unwrap();
        let _home = crate::paths::test_helpers::set_test_tg_home(dir.path());
        seed();
        let mut store = PendingStore::default();
        store.insert_new(7, None, Utc::now());
        store.save(&crate::paths::pending_path()).unwrap();

        reject(7).unwrap();
        let store2 = PendingStore::load(&crate::paths::pending_path()).unwrap();
        assert!(store2.get(7).is_none());
    }
}
