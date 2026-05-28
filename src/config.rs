//! ~/.tg/config.toml load/save with strict mode check.

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Config {
    pub bot_token: String,
    pub tmux_target: String,
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
        check_mode_strict(path)?;
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
        }
        std::fs::rename(&tmp, path)?;
        Ok(())
    }

    pub fn is_allowed(&self, chat_id: i64) -> bool {
        self.allow.iter().any(|e| e.chat_id == chat_id)
    }
}

fn check_mode_strict(path: &Path) -> Result<()> {
    let mode = std::fs::metadata(path)?.permissions().mode() & 0o777;
    if mode & 0o077 != 0 {
        return Err(anyhow!(
            "{} mode is {:o}; refusing to read (must be 0600)",
            path.display(),
            mode
        ));
    }
    Ok(())
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
}
