//! `tg allow`, `tg deny`, `tg list` — mutations on the allowlist.
//! `tg pair`, `tg pending`, `tg reject` are added in Task 11.

use anyhow::{anyhow, Result};

use crate::api::Client;
use crate::config::{AllowEntry, Config};
use crate::paths;

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
    if cfg.allow.is_empty() {
        println!("(allowlist empty)");
        return Ok(());
    }
    for e in &cfg.allow {
        let label = e.label.as_deref().unwrap_or("(no label)");
        println!("{}\t{}", e.chat_id, label);
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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn seed(home: &std::path::Path) {
        std::env::set_var("TG_HOME", home);
        let cfg = Config {
            bot_token: "T".into(),
            tmux_target: "x".into(),
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
}
