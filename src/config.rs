//! ~/.tg/config.toml load/save with strict mode check.

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Config {
    pub bot_token: String,
    pub tmux_target: String,
    /// The single chat_id whose inbound DMs are delivered to the tmux
    /// pane. All other allowlisted senders are outbound-only — their
    /// DMs are silently dropped, but `tg send --chat-id N` to them
    /// still works. If `None`, every allowlisted sender delivers
    /// (pre-0.2 behavior).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner_chat_id: Option<i64>,
    /// If set, inbound voice and audio attachments are transcribed by
    /// POSTing to `{whisper_url}/inference` (whisper.cpp's HTTP
    /// server API). The transcript is appended to the typed prompt
    /// line. Example: "http://127.0.0.1:8178". Default: no
    /// transcription.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub whisper_url: Option<String>,
    #[serde(default)]
    pub allow: Vec<AllowEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AllowEntry {
    pub chat_id: i64,
    #[serde(default)]
    pub label: Option<String>,
}

impl Config {
    pub fn load(path: &Path) -> Result<Self> {
        crate::paths::check_mode_strict(path)?;
        let body = std::fs::read_to_string(path)
            .with_context(|| format!("reading {}", path.display()))?;
        let cfg: Config = toml::from_str(&body)
            .with_context(|| format!("parsing {}", path.display()))?;
        Ok(cfg)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let parent = path.parent().ok_or_else(|| anyhow!("config path has no parent"))?;
        std::fs::create_dir_all(parent)?;
        let body = toml::to_string_pretty(self)?;
        // Write to a sibling temp file at 0o600, then atomically rename
        // over the destination. The temp file never exists with looser
        // permissions, and a crash mid-write leaves the original config
        // intact.
        let tmp = parent.join(format!(".config.toml.tmp.{}", std::process::id()));
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

    pub fn is_allowed(&self, chat_id: i64) -> bool {
        Some(chat_id) == self.owner_chat_id
            || self.allow.iter().any(|e| e.chat_id == chat_id)
    }

    /// True if `chat_id` is the configured owner. When `owner_chat_id`
    /// is `None`, no chat_id is the owner and all allowlisted senders
    /// deliver (pre-0.2 behavior).
    pub fn is_owner(&self, chat_id: i64) -> bool {
        Some(chat_id) == self.owner_chat_id
    }

    /// True if inbound delivery to tmux should occur for `chat_id`.
    /// - If `owner_chat_id` is set: only the owner delivers.
    /// - If not set: any allowlisted sender delivers (pre-0.2 behavior).
    pub fn delivers_inbound(&self, chat_id: i64) -> bool {
        match self.owner_chat_id {
            Some(_) => self.is_owner(chat_id),
            None => self.is_allowed(chat_id),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use tempfile::tempdir;

    fn sample() -> Config {
        Config {
            bot_token: "TOKEN".into(),
            tmux_target: "root:1".into(),
            owner_chat_id: None,
            whisper_url: None,
            allow: vec![AllowEntry { chat_id: 1, label: Some("alice".into()) }],
        }
    }

    #[test]
    fn save_then_load_roundtrips() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("config.toml");
        let cfg = sample();
        cfg.save(&p).unwrap();
        let loaded = Config::load(&p).unwrap();
        assert_eq!(cfg, loaded);
    }

    #[test]
    fn save_sets_mode_0600() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("config.toml");
        sample().save(&p).unwrap();
        let mode = std::fs::metadata(&p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }

    #[test]
    fn load_refuses_world_readable() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("config.toml");
        sample().save(&p).unwrap();
        let mut perms = std::fs::metadata(&p).unwrap().permissions();
        perms.set_mode(0o644);
        std::fs::set_permissions(&p, perms).unwrap();
        let err = Config::load(&p).unwrap_err().to_string();
        assert!(err.contains("644"), "expected error to include octal mode 644; got: {err}");
    }

    #[test]
    fn is_allowed_checks_chat_id() {
        let cfg = sample();
        assert!(cfg.is_allowed(1));
        assert!(!cfg.is_allowed(2));
    }

    #[test]
    fn owner_is_implicitly_allowed_even_without_allow_entry() {
        let cfg = Config {
            bot_token: "T".into(),
            tmux_target: "x".into(),
            owner_chat_id: Some(99),
            whisper_url: None,
            allow: vec![],
        };
        assert!(cfg.is_allowed(99));
        assert!(cfg.is_owner(99));
        assert!(!cfg.is_owner(1));
    }

    #[test]
    fn delivers_inbound_only_for_owner_when_set() {
        let cfg = Config {
            bot_token: "T".into(),
            tmux_target: "x".into(),
            owner_chat_id: Some(99),
            whisper_url: None,
            allow: vec![
                AllowEntry { chat_id: 99, label: Some("me".into()) },
                AllowEntry { chat_id: 100, label: Some("brother".into()) },
            ],
        };
        // Owner: delivers to tmux
        assert!(cfg.delivers_inbound(99));
        // Allowlisted contact but not owner: outbound-only
        assert!(!cfg.delivers_inbound(100));
        assert!(cfg.is_allowed(100));
        // Unknown: not allowed, won't deliver
        assert!(!cfg.delivers_inbound(101));
        assert!(!cfg.is_allowed(101));
    }

    #[test]
    fn delivers_inbound_for_all_when_no_owner() {
        // pre-0.2 behavior: owner_chat_id None means everyone allowlisted delivers.
        let cfg = Config {
            bot_token: "T".into(),
            tmux_target: "x".into(),
            owner_chat_id: None,
            whisper_url: None,
            allow: vec![
                AllowEntry { chat_id: 99, label: None },
                AllowEntry { chat_id: 100, label: None },
            ],
        };
        assert!(cfg.delivers_inbound(99));
        assert!(cfg.delivers_inbound(100));
        assert!(!cfg.delivers_inbound(101));
    }

    #[test]
    fn config_without_owner_field_parses_for_backward_compat() {
        let body = r#"
bot_token = "T"
tmux_target = "root:1"

[[allow]]
chat_id = 42
"#;
        let cfg: Config = toml::from_str(body).unwrap();
        assert_eq!(cfg.owner_chat_id, None);
        assert_eq!(cfg.allow.len(), 1);
    }
}
