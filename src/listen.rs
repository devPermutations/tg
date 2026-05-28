//! `tg listen` — inbound daemon.
//!
//! Poll Telegram, gate by allowlist/pending, deliver text and downloaded
//! attachments to a tmux pane via send-keys. Pairing reminders for
//! unknown senders. "Agent offline" reply when the tmux target is gone.

use anyhow::{Context, Result};
use chrono::{Duration, Utc};
use std::path::PathBuf;
use std::time::Duration as StdDuration;

use crate::api::{Client, Message, Update};
use crate::config::Config;
use crate::pending::{PendingStore, REMINDER_THROTTLE_SECS};
use crate::{paths, tmux};

const POLL_TIMEOUT_SECS: u32 = 30;

pub fn run(api_base: &str, tmux_bin: &str) -> Result<()> {
    let cfg_path = paths::config_path();
    let cfg = Config::load(&cfg_path)?;
    let client = Client::new(api_base, cfg.bot_token.clone());
    tracing::info!("tg listen starting; target={}", cfg.tmux_target);

    let mut offset = read_offset()?;
    let mut backoff_secs: u64 = 1;

    loop {
        match client.get_updates(offset, POLL_TIMEOUT_SECS) {
            Ok(updates) => {
                backoff_secs = 1;
                for u in updates {
                    let next = u.update_id + 1;
                    if let Err(e) = handle_update(u, &cfg, &client, tmux_bin) {
                        tracing::warn!("handle_update failed: {e:#}");
                    }
                    offset = offset.max(next);
                    write_offset(offset)?;
                }
            }
            Err(e) => {
                let s = format!("{e:#}");
                // 401 = invalid bot token. Retrying never recovers this;
                // exit so systemd flags it (the unit's RestartSec=5
                // produces a tight crash loop in journald that's easy to
                // spot — better than spinning silently).
                if s.contains("status code 401") || s.contains("Unauthorized") {
                    tracing::error!("getUpdates fatal (401): {s}");
                    std::process::exit(1);
                }
                tracing::warn!("getUpdates failed: {s}; backoff {backoff_secs}s");
                std::thread::sleep(StdDuration::from_secs(backoff_secs));
                backoff_secs = (backoff_secs * 2).min(60);
            }
        }
    }
}

fn handle_update(u: Update, cfg: &Config, client: &Client, tmux_bin: &str) -> Result<()> {
    let Some(msg) = u.message else { return Ok(()); };
    let chat_id = msg.chat.id;
    let user_label = msg.from.as_ref().and_then(|f| f.username.clone());

    // Gate
    if !cfg.is_allowed(chat_id) {
        return handle_gated(client, chat_id, user_label.as_deref());
    }

    // Allowed — check tmux target.
    if !tmux::target_alive(tmux_bin, &cfg.tmux_target) {
        let _ = client.send_message(chat_id, "agent offline (Claude Code not running)", None, None);
        tracing::warn!("dropping inbound from {chat_id}: tmux target {} not alive", cfg.tmux_target);
        return Ok(());
    }

    // Decide on text + attachment.
    let (body, attachment_path) = build_body(&msg, client)?;
    let line = tmux::format_inbound(user_label.as_deref(), chat_id, &body);
    let final_line = match attachment_path {
        Some(p) => format!("{line} [file: {}]", p.display()),
        None => line,
    };
    tmux::send_line(tmux_bin, &cfg.tmux_target, &final_line)?;
    Ok(())
}

fn build_body(msg: &Message, client: &Client) -> Result<(String, Option<PathBuf>)> {
    // Text or caption forms the body.
    let body = msg.text.clone()
        .or_else(|| msg.caption.clone())
        .unwrap_or_else(|| {
            if msg.photo.is_some() { "(photo)".into() }
            else if msg.document.is_some() { "(document)".into() }
            else { "(unsupported)".into() }
        });

    // Attachment: largest photo, or the document.
    let file_id_kind: Option<(&str, Option<&str>)> = msg.photo.as_ref()
        .and_then(|sizes| sizes.last())
        .map(|p| (p.file_id.as_str(), None))
        .or_else(|| msg.document.as_ref().map(|d| (
            d.file_id.as_str(),
            d.file_name.as_deref(),
        )));

    let attachment_path = if let Some((file_id, name_hint)) = file_id_kind {
        let f = client.get_file(file_id)?;
        let path_part = f.file_path.as_deref().unwrap_or("");
        let ext = std::path::Path::new(path_part)
            .extension().and_then(|s| s.to_str()).unwrap_or("bin");
        let ts = chrono::Utc::now().timestamp();
        let inbox = paths::inbox_dir();
        std::fs::create_dir_all(&inbox)?;
        let stem = name_hint.unwrap_or(&f.file_unique_id);
        let safe_stem: String = stem.chars()
            .map(|c| if c.is_alphanumeric() || c == '.' || c == '-' || c == '_' { c } else { '_' })
            .collect();
        let dest = inbox.join(format!("{ts}-{safe_stem}.{ext}"));
        let n = client.download_file(&f, &dest)?;
        tracing::info!("downloaded {} bytes to {}", n, dest.display());
        Some(dest)
    } else {
        None
    };

    Ok((body, attachment_path))
}

fn handle_gated(client: &Client, chat_id: i64, username: Option<&str>) -> Result<()> {
    let pending_path = paths::pending_path();
    let mut store = PendingStore::load(&pending_path)?;
    let now = Utc::now();
    let throttle = Duration::seconds(REMINDER_THROTTLE_SECS);

    let needs_send_message: Option<String>;

    if let Some(entry) = store.get(chat_id) {
        if entry.expires_at > now {
            // Still pending and not expired — throttled reminder.
            if now.signed_duration_since(entry.last_reminder_at) >= throttle {
                let code = entry.code.clone();
                let entry_mut = store.entries.get_mut(&chat_id.to_string()).unwrap();
                entry_mut.last_reminder_at = now;
                store.save(&pending_path)?;
                needs_send_message = Some(format!(
                    "Still pending — run in your terminal: `tg pair {code}`"
                ));
            } else {
                return Ok(()); // throttled
            }
        } else {
            // Expired — replace with a fresh entry.
            store.remove(chat_id);
            let entry = store.insert_new(chat_id, username.map(|s| s.to_string()), now).clone();
            store.save(&pending_path)?;
            needs_send_message = Some(format!(
                "Pairing required — run in your terminal: `tg pair {}`",
                entry.code
            ));
        }
    } else {
        // New — create fresh entry.
        let entry = store.insert_new(chat_id, username.map(|s| s.to_string()), now).clone();
        store.save(&pending_path)?;
        needs_send_message = Some(format!(
            "Pairing required — run in your terminal: `tg pair {}`",
            entry.code
        ));
    }

    if let Some(text) = needs_send_message {
        let _ = client.send_message(chat_id, &text, None, None);
    }
    Ok(())
}

fn read_offset() -> Result<i64> {
    let p = paths::state_path();
    if !p.exists() { return Ok(0); }
    let s = std::fs::read_to_string(&p)?;
    let n: i64 = s.trim().parse()
        .with_context(|| format!("parsing offset in {}", p.display()))?;
    Ok(n)
}

fn write_offset(offset: i64) -> Result<()> {
    let p = paths::state_path();
    if let Some(parent) = p.parent() { std::fs::create_dir_all(parent)?; }
    let tmp = p.with_file_name("state.tmp");
    std::fs::write(&tmp, offset.to_string())?;
    std::fs::rename(&tmp, &p)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn read_offset_returns_zero_when_missing() {
        let _g = crate::paths::test_lock::acquire();
        let dir = tempdir().unwrap();
        std::env::set_var("TG_HOME", dir.path());
        let result = read_offset();
        std::env::remove_var("TG_HOME");
        assert_eq!(result.unwrap(), 0);
    }

    #[test]
    fn write_then_read_offset_roundtrips() {
        let _g = crate::paths::test_lock::acquire();
        let dir = tempdir().unwrap();
        std::env::set_var("TG_HOME", dir.path());
        let write_result = write_offset(12345);
        let read_result = read_offset();
        std::env::remove_var("TG_HOME");
        write_result.unwrap();
        assert_eq!(read_result.unwrap(), 12345);
    }
}
