//! tmux send-keys wrapper.
//!
//! Delivers a text line to a tmux pane, then a separate Enter keypress
//! (two `tmux` invocations: the first writes the text bytes to the pty,
//! the second sends an Enter key — Process::status() blocks until each
//! exits, guaranteeing ordering).

use anyhow::{Context, Result};
use std::process::Command;

/// Strip newlines and C0 controls so an attacker-controlled string can't
/// inject extra prompt lines or escape sequences. `tmux send-keys -l`
/// otherwise types each character literally.
pub fn sanitize(text: &str) -> String {
    text.replace(['\r', '\n'], " ")
        .chars()
        .filter(|c| !c.is_control() || *c == ' ')
        .collect()
}

/// Build the formatted prompt line that gets typed into the pane.
pub fn format_inbound(username: Option<&str>, chat_id: i64, text: &str) -> String {
    let header = match username {
        Some(u) => format!("@{u} (chat_id={chat_id})"),
        None => format!("chat_id={chat_id}"),
    };
    format!("[telegram {header}] {}", sanitize(text))
}

/// Check whether the given tmux target exists. Used to decide "agent
/// offline" behavior.
pub fn target_alive(tmux_bin: &str, target: &str) -> bool {
    Command::new(tmux_bin)
        .args(["has-session", "-t", target])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Send `line` followed by Enter to `target`. Two separate invocations.
pub fn send_line(tmux_bin: &str, target: &str, line: &str) -> Result<()> {
    let status = Command::new(tmux_bin)
        .args(["send-keys", "-t", target, "-l", line])
        .status()
        .with_context(|| format!("invoking {tmux_bin} send-keys -l"))?;
    if !status.success() {
        anyhow::bail!("tmux send-keys -l exited {status}");
    }
    let status = Command::new(tmux_bin)
        .args(["send-keys", "-t", target, "Enter"])
        .status()
        .with_context(|| format!("invoking {tmux_bin} send-keys Enter"))?;
    if !status.success() {
        anyhow::bail!("tmux send-keys Enter exited {status}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_replaces_newlines_with_space() {
        assert_eq!(sanitize("a\nb\rc"), "a b c");
    }

    #[test]
    fn sanitize_strips_c0_controls() {
        let raw = "hi\x01\x02\x7fworld";
        assert_eq!(sanitize(raw), "hiworld");
    }

    #[test]
    fn sanitize_preserves_normal_chars() {
        assert_eq!(sanitize("hello, world! 🎉"), "hello, world! 🎉");
    }

    #[test]
    fn format_inbound_with_username() {
        let got = format_inbound(Some("virgil"), 8583339367, "hi");
        assert_eq!(got, "[telegram @virgil (chat_id=8583339367)] hi");
    }

    #[test]
    fn format_inbound_without_username() {
        let got = format_inbound(None, 42, "hi");
        assert_eq!(got, "[telegram chat_id=42] hi");
    }

    #[test]
    fn format_inbound_sanitizes_body() {
        let got = format_inbound(None, 1, "line1\nline2");
        assert_eq!(got, "[telegram chat_id=1] line1 line2");
    }
}
