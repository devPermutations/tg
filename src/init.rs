//! `tg init` — interactive (or flag-driven) initial config write.

use anyhow::{anyhow, Result};
use std::io::{BufRead, Write};

use crate::config::Config;
use crate::paths;

pub struct InitOpts {
    pub token: Option<String>,
    pub tmux_target: Option<String>,
    pub owner_chat_id: Option<i64>,
    pub force: bool,
}

pub fn run(opts: InitOpts) -> Result<()> {
    let path = paths::config_path();
    if path.exists() && !opts.force {
        return Err(anyhow!(
            "{} already exists; pass --force to overwrite",
            path.display()
        ));
    }

    let token = match opts.token {
        Some(t) => t,
        None => prompt("Bot token: ", false)?,
    };
    if token.is_empty() {
        return Err(anyhow!("bot token cannot be empty"));
    }

    let tmux_target = match opts.tmux_target {
        Some(t) => t,
        None => {
            let t = prompt("tmux target [root:1]: ", true)?;
            if t.is_empty() { "root:1".to_string() } else { t }
        }
    };

    let cfg = Config {
        bot_token: token,
        tmux_target,
        owner_chat_id: opts.owner_chat_id,
        whisper_url: None,
        allow: vec![],
    };
    cfg.save(&path)?;
    println!("wrote {} (mode 0600)", path.display());
    if cfg.owner_chat_id.is_none() {
        println!(
            "note: no owner_chat_id set. All allowlisted senders will deliver \
            to your tmux pane. Run `tg set-owner --chat-id N` to lock inbound \
            delivery to a single chat."
        );
    }
    Ok(())
}

fn prompt(label: &str, allow_empty: bool) -> Result<String> {
    print!("{label}");
    std::io::stdout().flush()?;
    let mut line = String::new();
    std::io::stdin().lock().read_line(&mut line)?;
    let trimmed = line.trim().to_string();
    if !allow_empty && trimmed.is_empty() {
        return Err(anyhow!("input cannot be empty"));
    }
    Ok(trimmed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn writes_config_with_owner() {
        let _g = crate::paths::test_lock::acquire();
        let dir = tempdir().unwrap();
        std::env::set_var("TG_HOME", dir.path());
        let opts = InitOpts {
            token: Some("TOKEN".into()),
            tmux_target: Some("root:1".into()),
            owner_chat_id: Some(42),
            force: false,
        };
        let run_result = run(opts);
        let cfg_path = paths::config_path();
        std::env::remove_var("TG_HOME");

        run_result.unwrap();
        let cfg = Config::load(&cfg_path).unwrap();
        assert_eq!(cfg.owner_chat_id, Some(42));
    }

    #[test]
    fn writes_config_with_flags() {
        let _g = crate::paths::test_lock::acquire();
        let dir = tempdir().unwrap();
        std::env::set_var("TG_HOME", dir.path());
        let opts = InitOpts {
            token: Some("TOKEN".into()),
            tmux_target: Some("root:1".into()),
            owner_chat_id: None,
            force: false,
        };
        let run_result = run(opts);
        let cfg_path = paths::config_path();
        std::env::remove_var("TG_HOME");

        run_result.unwrap();
        let cfg = Config::load(&cfg_path).unwrap();
        assert_eq!(cfg.bot_token, "TOKEN");
        assert_eq!(cfg.tmux_target, "root:1");
    }

    #[test]
    fn refuses_overwrite_without_force() {
        let _g = crate::paths::test_lock::acquire();
        let dir = tempdir().unwrap();
        std::env::set_var("TG_HOME", dir.path());
        let first = run(InitOpts {
            token: Some("A".into()), tmux_target: Some("x".into()),
            owner_chat_id: None, force: false,
        });
        let second = run(InitOpts {
            token: Some("B".into()), tmux_target: Some("y".into()),
            owner_chat_id: None, force: false,
        });
        std::env::remove_var("TG_HOME");

        first.unwrap();
        let err = second.unwrap_err().to_string();
        assert!(err.contains("already exists"));
    }
}
