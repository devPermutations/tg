//! `tg listen` — inbound daemon.
//!
//! Poll Telegram, gate by allowlist/pending, deliver text and downloaded
//! attachments to a tmux pane via send-keys. Pairing reminders for
//! unknown senders. "Agent offline" reply when the tmux target is gone.

use anyhow::{Context, Result};
use chrono::{Duration, Utc};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration as StdDuration;

use crate::api::{Client, Message, Update};
use crate::config::Config;
use crate::pending::{PendingStore, REMINDER_THROTTLE_SECS};
use crate::{paths, tmux};

const POLL_TIMEOUT_SECS: u32 = 30;

/// In-memory state that the listen loop carries across iterations.
/// Persisted state (offset, pending) lives on disk; this is purely
/// ephemeral throttle-and-metrics scratch space.
#[derive(Default)]
struct LoopState {
    /// Last "agent offline" reply sent per chat_id, used for the
    /// 30-second throttle so a sender doesn't get a flood of offline
    /// notices while the target pane is down.
    offline_reply_last_sent: std::collections::HashMap<i64, std::time::Instant>,
}

const OFFLINE_REPLY_THROTTLE_SECS: u64 = 30;

pub fn run(api_base: &str, tmux_bin: &str) -> Result<()> {
    let cfg_path = paths::config_path();
    let mut cfg = Config::load(&cfg_path)?;
    let mut client = Client::new(api_base, cfg.bot_token.clone());
    tracing::info!("tg listen starting; target={}", cfg.tmux_target);

    let reload_requested = Arc::new(AtomicBool::new(false));
    signal_hook::flag::register(signal_hook::consts::SIGHUP, Arc::clone(&reload_requested))
        .context("registering SIGHUP handler")?;
    tracing::info!("SIGHUP handler installed (send SIGHUP to reload config)");

    let mut offset = read_offset()?;
    let mut backoff_secs: u64 = 1;
    let mut state = LoopState::default();
    let tmux_client = tmux::TmuxClient::new(tmux_bin);

    loop {
        // SIGHUP since the last iteration? Reload the config in place.
        // get_updates is a long-poll (~30s) so reloads land within that
        // window of the signal arriving.
        if reload_requested.swap(false, Ordering::SeqCst) {
            match Config::load(&cfg_path) {
                Ok(new_cfg) => {
                    let token_changed = new_cfg.bot_token != cfg.bot_token;
                    tracing::info!(
                        "SIGHUP: reloaded config (allowlist={}, owner_chat_id={:?}, token_changed={})",
                        new_cfg.allow.len(),
                        new_cfg.owner_chat_id,
                        token_changed,
                    );
                    if token_changed {
                        client = Client::new(api_base, new_cfg.bot_token.clone());
                    }
                    cfg = new_cfg;
                }
                Err(e) => {
                    tracing::warn!(
                        "SIGHUP: config reload failed ({e:#}); keeping previous config"
                    );
                }
            }
        }

        match client.get_updates(offset, POLL_TIMEOUT_SECS) {
            Ok(updates) => {
                backoff_secs = 1;
                for u in updates {
                    let next = u.update_id + 1;
                    if let Err(e) = handle_update(u, &cfg, &client, &tmux_client, &mut state) {
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

fn handle_update(u: Update, cfg: &Config, client: &Client, tmux_client: &tmux::TmuxClient, state: &mut LoopState) -> Result<()> {
    let Some(msg) = u.message else { return Ok(()); };
    let chat_id = msg.chat.id;
    let user_label = msg.from.as_ref().and_then(|f| f.username.clone());

    // Gate
    if !cfg.is_allowed(chat_id) {
        // With an owner set, pairing is disabled — only the owner is
        // trusted to add contacts (via `tg allow`). Unknown senders are
        // silently dropped so the bot doesn't leak its existence.
        if cfg.owner_chat_id.is_some() {
            tracing::info!(
                "dropping inbound from unknown chat_id {chat_id}: pairing disabled (owner is set)"
            );
            return Ok(());
        }
        return handle_gated(client, chat_id, user_label.as_deref());
    }

    // Outbound-only contact: allowlisted, but not the owner. Silent
    // drop — no tmux injection, no Telegram reply (we don't want to
    // ack contact-list members about messages we ignored).
    if !cfg.delivers_inbound(chat_id) {
        tracing::info!("dropping inbound from {chat_id}: outbound-only contact (not owner)");
        return Ok(());
    }

    // Allowed and is owner — check tmux target.
    if !tmux_client.target_alive(&cfg.tmux_target) {
        // Throttle the offline reply so we don't spam the sender while
        // the pane is down. The pane being down is usually a "user closed
        // their session" or "host rebooted" state — recovery typically
        // takes minutes, not seconds, so one reply every 30s is plenty.
        let now = std::time::Instant::now();
        let last = state.offline_reply_last_sent.get(&chat_id).copied();
        let should_reply = match last {
            None => true,
            Some(t) => now.duration_since(t).as_secs() >= OFFLINE_REPLY_THROTTLE_SECS,
        };
        if should_reply {
            let msg = format!(
                "agent offline (target pane '{}' is not active)",
                cfg.tmux_target
            );
            if let Err(e) = client.send_message(chat_id, &msg, None, None) {
                tracing::warn!("offline-reply send_message failed for {chat_id}: {e:#}");
            } else {
                state.offline_reply_last_sent.insert(chat_id, now);
            }
        }
        tracing::warn!(
            "dropping inbound from {chat_id}: tmux target {} not alive (reply {})",
            cfg.tmux_target,
            if should_reply { "sent" } else { "throttled" },
        );
        return Ok(());
    }

    // Decide on text + attachment.
    let (body, attachment_path) = build_body(&msg, client)?;
    let line = tmux::format_inbound(user_label.as_deref(), chat_id, &body);
    let final_line = match attachment_path {
        Some(p) => {
            let mut s = format!("{line} [file: {}]", p.display());
            // Transcribe voice/audio synchronously before the send-keys
            // so the agent sees one complete prompt line. The pane will
            // sit silent for a few seconds while whisper runs; that's
            // fine — beats injecting `(voice 0:12) [file: ...]` then a
            // separate prompt with the transcript later.
            // Use the accessor so the new [transcription] table wins
            // over the legacy top-level `whisper_url` field.
            if let Some(whisper_url) = cfg.whisper_url() {
                if msg.media_ref().is_some_and(|m| m.is_transcribable()) {
                    match crate::transcribe::transcribe(&p, whisper_url, "ffmpeg") {
                        Ok(text) => {
                            let safe = crate::tmux::sanitize(&text);
                            s.push_str(&format!(" [transcript: {safe}]"));
                        }
                        Err(e) => {
                            tracing::warn!(
                                "transcription failed for {}: {e:#}", p.display()
                            );
                        }
                    }
                }
            }
            s
        }
        None => line,
    };
    tmux_client.send_line(&cfg.tmux_target, &final_line)?;
    tracing::info!(
        "delivered inbound from chat_id {chat_id} ({} bytes) to {}",
        final_line.len(),
        cfg.tmux_target,
    );
    Ok(())
}

fn build_body(msg: &Message, client: &Client) -> Result<(String, Option<PathBuf>)> {
    // Text or caption forms the body.
    let body = msg.text.clone()
        .or_else(|| msg.caption.clone())
        .unwrap_or_else(|| render_media_label(msg));

    let attachment_path = if let Some(media) = msg.media_ref() {
        let f = client.get_file(media.file_id())?;
        let path_part = f.file_path.as_deref().unwrap_or("");
        let ext = std::path::Path::new(path_part)
            .extension().and_then(|s| s.to_str()).unwrap_or("bin");
        let ts = chrono::Utc::now().timestamp();
        let inbox = paths::inbox_dir();
        std::fs::create_dir_all(&inbox)?;
        let stem = media.name_hint().unwrap_or(&f.file_unique_id);
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

/// Stand-in body when a message has media but no text/caption. Keep
/// these terse: they get typed into a prompt alongside the file path.
fn render_media_label(msg: &Message) -> String {
    if msg.photo.is_some() { return "(photo)".into(); }
    if msg.document.is_some() { return "(document)".into(); }
    if let Some(v) = &msg.voice {
        return format!("(voice {})", format_duration(v.duration));
    }
    if let Some(a) = &msg.audio {
        let title = a.title.as_deref()
            .or(a.performer.as_deref())
            .or(a.file_name.as_deref())
            .unwrap_or("audio");
        return format!("(audio {}: {})", format_duration(a.duration), title);
    }
    if let Some(s) = &msg.sticker {
        match s.emoji.as_deref() {
            Some(e) => return format!("(sticker {e})"),
            None => return "(sticker)".into(),
        }
    }
    "(unsupported media)".into()
}

fn format_duration(secs: u32) -> String {
    let m = secs / 60;
    let s = secs % 60;
    format!("{m}:{s:02}")
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
                let entry_mut = store.entries.get_mut(&chat_id).unwrap();
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
        let dir = tempdir().unwrap();
        let _home = crate::paths::test_helpers::set_test_tg_home(dir.path());
        assert_eq!(read_offset().unwrap(), 0);
    }

    #[test]
    fn write_then_read_offset_roundtrips() {
        let dir = tempdir().unwrap();
        let _home = crate::paths::test_helpers::set_test_tg_home(dir.path());
        write_offset(12345).unwrap();
        assert_eq!(read_offset().unwrap(), 12345);
    }

    use crate::api::{Audio, Chat, Sticker, Voice};

    fn empty_msg() -> Message {
        Message {
            message_id: 1,
            from: None,
            chat: Chat { id: 1, kind: "private".into() },
            text: None,
            caption: None,
            photo: None,
            document: None,
            voice: None,
            audio: None,
            sticker: None,
        }
    }

    #[test]
    fn format_duration_pads_seconds() {
        assert_eq!(format_duration(0), "0:00");
        assert_eq!(format_duration(7), "0:07");
        assert_eq!(format_duration(65), "1:05");
        assert_eq!(format_duration(3661), "61:01");
    }

    #[test]
    fn render_media_label_voice() {
        let mut m = empty_msg();
        m.voice = Some(Voice {
            file_id: "x".into(), file_unique_id: "y".into(),
            duration: 12, mime_type: Some("audio/ogg".into()), file_size: None,
        });
        assert_eq!(render_media_label(&m), "(voice 0:12)");
    }

    #[test]
    fn render_media_label_audio_uses_title() {
        let mut m = empty_msg();
        m.audio = Some(Audio {
            file_id: "x".into(), file_unique_id: "y".into(),
            duration: 65,
            performer: Some("Artist".into()),
            title: Some("Song".into()),
            file_name: None, mime_type: None, file_size: None,
        });
        assert_eq!(render_media_label(&m), "(audio 1:05: Song)");
    }

    #[test]
    fn render_media_label_audio_falls_back_to_filename() {
        let mut m = empty_msg();
        m.audio = Some(Audio {
            file_id: "x".into(), file_unique_id: "y".into(),
            duration: 3,
            performer: None, title: None,
            file_name: Some("podcast.mp3".into()),
            mime_type: None, file_size: None,
        });
        assert_eq!(render_media_label(&m), "(audio 0:03: podcast.mp3)");
    }

    #[test]
    fn render_media_label_sticker_with_emoji() {
        let mut m = empty_msg();
        m.sticker = Some(Sticker {
            file_id: "x".into(), file_unique_id: "y".into(),
            emoji: Some("🎉".into()),
            set_name: None, is_animated: false, is_video: false, file_size: None,
        });
        assert_eq!(render_media_label(&m), "(sticker 🎉)");
    }

    #[test]
    fn render_media_label_sticker_without_emoji() {
        let mut m = empty_msg();
        m.sticker = Some(Sticker {
            file_id: "x".into(), file_unique_id: "y".into(),
            emoji: None,
            set_name: None, is_animated: false, is_video: false, file_size: None,
        });
        assert_eq!(render_media_label(&m), "(sticker)");
    }

    #[test]
    fn render_media_label_unknown() {
        let m = empty_msg();
        assert_eq!(render_media_label(&m), "(unsupported media)");
    }
}
