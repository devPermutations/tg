# `tg` Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a self-contained Rust CLI (`tg`) that owns a Telegram bot endpoint: inbound daemon delivers messages via tmux send-keys; outbound CLI sends text and attachments. Pairing-based access control. systemd-supervised. No MCP, no Bun.

**Architecture:** Single binary, subcommands. Synchronous HTTP via `ureq`. No tokio. Config + state under `~/.tg/`. Each subcommand maps to one module. Mock HTTP and a fake tmux shim allow integration tests without hitting Telegram or a real tmux.

**Tech Stack:** Rust 1.93+, `clap` (derive), `serde` + `serde_json` + `toml`, `ureq` (rustls + gzip), `anyhow`, `tracing` + `tracing-subscriber`, `rand`, `chrono`, `tiny_http` (test-only).

**Spec:** `docs/design.md` (commit `020caa1`).

**Project root:** `~/projects/tg/`. All paths below are relative to this root unless prefixed `~/`.

---

## File Structure

```
~/projects/tg/
├── Cargo.toml
├── .gitignore
├── src/
│   ├── main.rs          # clap entry, dispatch to subcommands
│   ├── config.rs        # ~/.tg/config.toml load/save, mode check
│   ├── pending.rs       # ~/.tg/pending.json load/save, expiry
│   ├── tmux.rs          # send-keys wrapper, text sanitization
│   ├── api.rs           # Telegram Bot API client
│   ├── listen.rs        # poll loop, gate, deliver
│   ├── send.rs          # outbound: text + multipart attachments
│   ├── access.rs        # allow/deny/list/pair/pending/reject
│   ├── install.rs       # symlink + systemd unit
│   └── paths.rs         # resolve ~/.tg/ with TG_HOME override
├── systemd/tg-listen.service
├── tests/
│   ├── outbound.rs      # tg send against mock telegram
│   └── inbound.rs       # tg listen against mock telegram + fake tmux
├── docs/
│   ├── design.md
│   ├── plan.md
│   └── smoke.md
└── README.md
```

Test fixtures (created inline by tests, not committed): a `fake-tmux.sh` shim
that records its argv to a temp file; the tiny_http mock builds in each test.

---

## Task 1: Bootstrap the project skeleton

**Files:**
- Create: `Cargo.toml`
- Create: `.gitignore`
- Create: `src/main.rs`
- Create: `README.md`

- [ ] **Step 1: Write `Cargo.toml`**

```toml
[package]
name = "tg"
version = "0.1.0"
edition = "2021"
description = "Self-contained Telegram bot CLI: inbound daemon (tmux send-keys delivery) + outbound CLI (text + attachments)."

[[bin]]
name = "tg"
path = "src/main.rs"

[dependencies]
clap = { version = "4", features = ["derive"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
toml = "0.8"
ureq = { version = "2", default-features = false, features = ["tls", "gzip", "json"] }
anyhow = "1"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
rand = "0.8"
chrono = { version = "0.4", features = ["serde"] }
mime_guess = "2"

[dev-dependencies]
tiny_http = "0.12"
tempfile = "3"
```

- [ ] **Step 2: Write `.gitignore`**

```
/target
*.swp
```

- [ ] **Step 3: Write `src/main.rs` skeleton**

```rust
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "tg", about = "Telegram bot CLI: daemon + outbound", version)]
struct Cli {
    /// Hidden: override Telegram API base URL (for tests).
    #[arg(long, hide = true, global = true, default_value = "https://api.telegram.org")]
    api_base: String,

    /// Hidden: override tmux binary path (for tests).
    #[arg(long, hide = true, global = true, default_value = "tmux")]
    tmux_bin: String,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Write ~/.tg/config.toml interactively.
    Init,
    /// Symlink into ~/.ir/tools/ and install + enable the systemd unit.
    Install,
    /// Inbound daemon: poll Telegram, gate, deliver via tmux send-keys.
    Listen,
    /// Send a message (with optional --file attachments).
    Send,
    /// Append a chat_id to the allowlist.
    Allow,
    /// Remove a chat_id from the allowlist.
    Deny,
    /// Print the current allowlist.
    List,
    /// Confirm a pending pairing by code.
    Pair,
    /// List pending pairings.
    Pending,
    /// Drop a pending pairing silently (no Telegram reply).
    Reject,
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();
    match cli.command {
        Command::Init => todo!("Task 9: init"),
        Command::Install => todo!("Task 14: install"),
        Command::Listen => todo!("Task 13: listen"),
        Command::Send => todo!("Task 12: send"),
        Command::Allow => todo!("Task 10: allow"),
        Command::Deny => todo!("Task 10: deny"),
        Command::List => todo!("Task 10: list"),
        Command::Pair => todo!("Task 11: pair"),
        Command::Pending => todo!("Task 11: pending"),
        Command::Reject => todo!("Task 11: reject"),
    }
}
```

- [ ] **Step 4: Write `README.md`**

```markdown
# tg

Self-contained Telegram bot CLI. See `docs/design.md` for design and
`docs/smoke.md` for end-to-end verification steps.
```

- [ ] **Step 5: Verify it builds**

Run: `cargo build`
Expected: clean compile, warnings about unused fields are OK at this stage.

- [ ] **Step 6: Commit**

```bash
cd ~/projects/tg
git add Cargo.toml Cargo.lock .gitignore src/main.rs README.md
git commit -m "feat: bootstrap tg crate with clap subcommand skeleton"
```

---

## Task 2: Paths module with `TG_HOME` override

**Files:**
- Create: `src/paths.rs`
- Modify: `src/main.rs` (add `mod paths;`)
- Test: `src/paths.rs` (inline `#[cfg(test)] mod tests`)

- [ ] **Step 1: Write failing test in `src/paths.rs`**

```rust
//! Resolves ~/.tg/ paths, with TG_HOME override for tests.

use std::path::{Path, PathBuf};

pub fn tg_home() -> PathBuf {
    if let Ok(p) = std::env::var("TG_HOME") {
        return PathBuf::from(p);
    }
    let home = std::env::var("HOME").expect("HOME not set");
    PathBuf::from(home).join(".tg")
}

pub fn config_path() -> PathBuf { tg_home().join("config.toml") }
pub fn pending_path() -> PathBuf { tg_home().join("pending.json") }
pub fn state_path() -> PathBuf { tg_home().join("state") }
pub fn inbox_dir() -> PathBuf { tg_home().join("inbox") }

pub fn ensure_dir(p: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(p)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tg_home_uses_override() {
        std::env::set_var("TG_HOME", "/tmp/tg-test-xyz");
        assert_eq!(tg_home(), PathBuf::from("/tmp/tg-test-xyz"));
        std::env::remove_var("TG_HOME");
    }

    #[test]
    fn config_path_is_under_home() {
        std::env::set_var("TG_HOME", "/tmp/x");
        assert_eq!(config_path(), PathBuf::from("/tmp/x/config.toml"));
        std::env::remove_var("TG_HOME");
    }
}
```

- [ ] **Step 2: Add `mod paths;` to `src/main.rs`**

Insert at the top of `src/main.rs`, right after the `use` statements:

```rust
mod paths;
```

- [ ] **Step 3: Run tests**

Run: `cargo test paths::`
Expected: 2 tests pass.

⚠️ Tests in this module manipulate `TG_HOME` env var. Cargo runs tests in
parallel by default — if more tests get added that also touch this env var,
they may race. The two above are isolated (set and unset within the same
test) and don't share writeable paths, so they're safe.

- [ ] **Step 4: Commit**

```bash
git add src/paths.rs src/main.rs
git commit -m "feat(paths): resolve ~/.tg/ subpaths with TG_HOME test override"
```

---

## Task 3: Config module — load, save, mode check

**Files:**
- Create: `src/config.rs`
- Modify: `src/main.rs` (add `mod config;`)
- Test: inline

- [ ] **Step 1: Write `src/config.rs` with failing tests**

```rust
//! ~/.tg/config.toml load/save with strict mode check.

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Config {
    pub bot_token: String,
    pub tmux_target: String,
    #[serde(default, rename = "allow")]
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
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let body = toml::to_string_pretty(self)?;
        std::fs::write(path, body)?;
        let mut perms = std::fs::metadata(path)?.permissions();
        perms.set_mode(0o600);
        std::fs::set_permissions(path, perms)?;
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
        assert!(err.contains("mode"));
    }

    #[test]
    fn is_allowed_checks_chat_id() {
        let cfg = sample();
        assert!(cfg.is_allowed(1));
        assert!(!cfg.is_allowed(2));
    }
}
```

- [ ] **Step 2: Add `mod config;` to `src/main.rs`**

Insert after `mod paths;`:

```rust
mod config;
```

- [ ] **Step 3: Run tests**

Run: `cargo test config::`
Expected: 4 tests pass.

- [ ] **Step 4: Commit**

```bash
git add src/config.rs src/main.rs Cargo.lock
git commit -m "feat(config): toml read/write with strict 0600 mode check"
```

---

## Task 4: Pending module — pending pairings store

**Files:**
- Create: `src/pending.rs`
- Modify: `src/main.rs` (add `mod pending;`)
- Test: inline

- [ ] **Step 1: Write `src/pending.rs` with failing tests**

```rust
//! ~/.tg/pending.json: chat_id -> pending pairing entry.

use anyhow::{Context, Result};
use chrono::{DateTime, Duration, Utc};
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

pub const CODE_LEN: usize = 6;
pub const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
pub const EXPIRY_HOURS: i64 = 1;
pub const REMINDER_THROTTLE_SECS: i64 = 30;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PendingEntry {
    pub code: String,
    pub username: Option<String>,
    pub first_seen_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub last_reminder_at: DateTime<Utc>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq)]
pub struct PendingStore {
    /// chat_id -> entry (chat_id as string because JSON map keys must be strings)
    #[serde(flatten)]
    pub entries: HashMap<String, PendingEntry>,
}

impl PendingStore {
    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() { return Ok(Self::default()); }
        let body = std::fs::read_to_string(path)
            .with_context(|| format!("reading {}", path.display()))?;
        let store: PendingStore = serde_json::from_str(&body)
            .with_context(|| format!("parsing {}", path.display()))?;
        Ok(store)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() { std::fs::create_dir_all(parent)?; }
        let body = serde_json::to_string_pretty(self)?;
        std::fs::write(path, body)?;
        let mut perms = std::fs::metadata(path)?.permissions();
        perms.set_mode(0o600);
        std::fs::set_permissions(path, perms)?;
        Ok(())
    }

    pub fn get(&self, chat_id: i64) -> Option<&PendingEntry> {
        self.entries.get(&chat_id.to_string())
    }

    pub fn insert_new(&mut self, chat_id: i64, username: Option<String>, now: DateTime<Utc>) -> &PendingEntry {
        let entry = PendingEntry {
            code: generate_code(),
            username,
            first_seen_at: now,
            expires_at: now + Duration::hours(EXPIRY_HOURS),
            last_reminder_at: now,
        };
        self.entries.insert(chat_id.to_string(), entry);
        self.entries.get(&chat_id.to_string()).unwrap()
    }

    /// Remove the entry for `chat_id`; returns it if present.
    pub fn remove(&mut self, chat_id: i64) -> Option<PendingEntry> {
        self.entries.remove(&chat_id.to_string())
    }

    /// Find by code (case-insensitive). Returns (chat_id, entry).
    pub fn find_by_code(&self, code: &str) -> Option<(i64, &PendingEntry)> {
        let needle = code.to_uppercase();
        self.entries.iter().find_map(|(k, v)| {
            if v.code == needle {
                k.parse::<i64>().ok().map(|id| (id, v))
            } else { None }
        })
    }
}

pub fn generate_code() -> String {
    let mut rng = rand::thread_rng();
    (0..CODE_LEN).map(|_| {
        let i = rng.gen_range(0..ALPHABET.len());
        ALPHABET[i] as char
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn now() -> DateTime<Utc> { Utc::now() }

    #[test]
    fn generated_code_is_six_alnum_upper() {
        let c = generate_code();
        assert_eq!(c.len(), CODE_LEN);
        assert!(c.chars().all(|ch| ALPHABET.contains(&(ch as u8))));
    }

    #[test]
    fn save_then_load_roundtrips() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("pending.json");
        let mut s = PendingStore::default();
        s.insert_new(42, Some("alice".into()), now());
        s.save(&p).unwrap();
        let loaded = PendingStore::load(&p).unwrap();
        assert_eq!(s, loaded);
    }

    #[test]
    fn load_missing_file_returns_empty() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("does-not-exist.json");
        let s = PendingStore::load(&p).unwrap();
        assert!(s.entries.is_empty());
    }

    #[test]
    fn find_by_code_is_case_insensitive() {
        let mut s = PendingStore::default();
        let e = s.insert_new(42, None, now()).clone();
        let (id, found) = s.find_by_code(&e.code.to_lowercase()).unwrap();
        assert_eq!(id, 42);
        assert_eq!(found, &e);
    }

    #[test]
    fn remove_works() {
        let mut s = PendingStore::default();
        s.insert_new(7, None, now());
        assert!(s.remove(7).is_some());
        assert!(s.remove(7).is_none());
    }
}
```

- [ ] **Step 2: Add `mod pending;` to `src/main.rs`**

```rust
mod pending;
```

- [ ] **Step 3: Run tests**

Run: `cargo test pending::`
Expected: 5 tests pass.

- [ ] **Step 4: Commit**

```bash
git add src/pending.rs src/main.rs Cargo.lock
git commit -m "feat(pending): pending-pairing store with code generation"
```

---

## Task 5: Tmux module — send-keys wrapper with sanitization

**Files:**
- Create: `src/tmux.rs`
- Modify: `src/main.rs` (add `mod tmux;`)
- Test: inline

- [ ] **Step 1: Write `src/tmux.rs` with failing tests**

```rust
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
```

- [ ] **Step 2: Add `mod tmux;` to `src/main.rs`**

```rust
mod tmux;
```

- [ ] **Step 3: Run tests**

Run: `cargo test tmux::`
Expected: 6 tests pass.

- [ ] **Step 4: Commit**

```bash
git add src/tmux.rs src/main.rs Cargo.lock
git commit -m "feat(tmux): send-keys wrapper with inbound-text formatter"
```

---

## Task 6: API module — Telegram client + sendMessage + getUpdates

**Files:**
- Create: `src/api.rs`
- Modify: `src/main.rs` (add `mod api;`)
- Test: inline

- [ ] **Step 1: Write `src/api.rs` with failing tests**

```rust
//! Telegram Bot API client (sync, ureq).
//!
//! Operations needed by tg:
//! - getUpdates (long-poll for inbound)
//! - sendMessage (text outbound, pairing reminders, "agent offline")
//! - sendPhoto / sendDocument (multipart outbound, Task 7)
//! - getFile + file download (inbound attachments, Task 8)

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Clone)]
pub struct Client {
    pub api_base: String,
    pub token: String,
    pub agent: ureq::Agent,
}

impl Client {
    pub fn new(api_base: impl Into<String>, token: impl Into<String>) -> Self {
        Self {
            api_base: api_base.into(),
            token: token.into(),
            agent: ureq::AgentBuilder::new()
                .timeout(std::time::Duration::from_secs(35))
                .build(),
        }
    }

    fn endpoint(&self, method: &str) -> String {
        format!("{}/bot{}/{}", self.api_base.trim_end_matches('/'), self.token, method)
    }

    /// POST application/json. Used for plain text endpoints.
    fn post_json<T: Serialize, R: serde::de::DeserializeOwned>(
        &self, method: &str, body: &T,
    ) -> Result<R> {
        let url = self.endpoint(method);
        let resp = self.agent.post(&url)
            .send_json(serde_json::to_value(body)?)
            .with_context(|| format!("POST {method}"))?;
        let parsed: ApiResponse<R> = resp.into_json()?;
        parsed.into_result()
    }

    pub fn get_updates(&self, offset: i64, timeout_secs: u32) -> Result<Vec<Update>> {
        #[derive(Serialize)]
        struct Req { offset: i64, timeout: u32, allowed_updates: Vec<&'static str> }
        let body = Req {
            offset,
            timeout: timeout_secs,
            allowed_updates: vec!["message"],
        };
        self.post_json("getUpdates", &body)
    }

    pub fn send_message(&self, chat_id: i64, text: &str) -> Result<Message> {
        #[derive(Serialize)]
        struct Req<'a> { chat_id: i64, text: &'a str }
        self.post_json("sendMessage", &Req { chat_id, text })
    }
}

/// Wrapper for Telegram's `{ok, result|description}` envelope.
#[derive(Deserialize)]
pub struct ApiResponse<T> {
    pub ok: bool,
    pub result: Option<T>,
    pub description: Option<String>,
}

impl<T> ApiResponse<T> {
    pub fn into_result(self) -> Result<T> {
        if self.ok {
            self.result.ok_or_else(|| anyhow!("Telegram API: ok=true but no result"))
        } else {
            Err(anyhow!("Telegram API: {}", self.description.unwrap_or_default()))
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct Update {
    pub update_id: i64,
    pub message: Option<Message>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Message {
    pub message_id: i64,
    pub from: Option<User>,
    pub chat: Chat,
    #[serde(default)] pub text: Option<String>,
    #[serde(default)] pub caption: Option<String>,
    #[serde(default)] pub photo: Option<Vec<PhotoSize>>,
    #[serde(default)] pub document: Option<Document>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct User {
    pub id: i64,
    #[serde(default)] pub username: Option<String>,
    #[serde(default)] pub first_name: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Chat {
    pub id: i64,
    #[serde(rename = "type")] pub kind: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct PhotoSize {
    pub file_id: String,
    pub file_unique_id: String,
    pub width: u32,
    pub height: u32,
    #[serde(default)] pub file_size: Option<u64>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Document {
    pub file_id: String,
    pub file_unique_id: String,
    #[serde(default)] pub file_name: Option<String>,
    #[serde(default)] pub mime_type: Option<String>,
    #[serde(default)] pub file_size: Option<u64>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;
    use std::sync::Arc;
    use std::thread;

    fn spawn_mock<F: Send + 'static>(handler: F) -> (String, thread::JoinHandle<()>)
    where F: Fn(tiny_http::Request)
    {
        // Bind directly with tiny_http on port 0; ask it for the chosen port.
        let server = Arc::new(tiny_http::Server::http("127.0.0.1:0").unwrap());
        let port = server.server_addr().to_ip().unwrap().port();
        let s2 = server.clone();
        let join = thread::spawn(move || {
            // Single request, then return — tests are scoped.
            if let Ok(req) = s2.recv() {
                handler(req);
            }
        });
        (format!("http://127.0.0.1:{port}"), join)
    }

    #[test]
    fn send_message_parses_envelope() {
        let (base, join) = spawn_mock(|req| {
            let body = r#"{"ok":true,"result":{"message_id":42,"chat":{"id":1,"type":"private"}}}"#;
            req.respond(tiny_http::Response::from_string(body)
                .with_header("Content-Type: application/json".parse::<tiny_http::Header>().unwrap()))
                .unwrap();
        });
        let c = Client::new(base, "TOKEN");
        let m = c.send_message(1, "hi").unwrap();
        assert_eq!(m.message_id, 42);
        join.join().unwrap();
    }

    #[test]
    fn send_message_propagates_error_description() {
        let (base, join) = spawn_mock(|req| {
            let body = r#"{"ok":false,"description":"Bad Request: chat not found"}"#;
            req.respond(tiny_http::Response::from_string(body)
                .with_header("Content-Type: application/json".parse::<tiny_http::Header>().unwrap()))
                .unwrap();
        });
        let c = Client::new(base, "TOKEN");
        let err = c.send_message(1, "hi").unwrap_err().to_string();
        assert!(err.contains("chat not found"), "got: {err}");
        join.join().unwrap();
    }
}
```

- [ ] **Step 2: Add `mod api;` to `src/main.rs`**

```rust
mod api;
```

- [ ] **Step 3: Run tests**

Run: `cargo test api::`
Expected: 2 tests pass.

- [ ] **Step 4: Commit**

```bash
git add src/api.rs src/main.rs Cargo.lock
git commit -m "feat(api): telegram client with getUpdates and sendMessage"
```

---

## Task 7: API multipart — sendPhoto and sendDocument

**Files:**
- Modify: `src/api.rs` (extend `Client`)
- Test: inline (added to existing `mod tests`)

- [ ] **Step 1: Add multipart helper and methods to `src/api.rs`**

Append inside `impl Client` (after `send_message`):

```rust
    /// POST multipart/form-data with one file field. Used for
    /// sendPhoto/sendDocument. Builds the body by hand — ureq has no
    /// multipart helper.
    fn post_multipart(
        &self,
        method: &str,
        fields: &[(&str, &str)],
        file_field: &str,
        file_name: &str,
        file_mime: &str,
        file_bytes: &[u8],
    ) -> Result<Message> {
        let boundary = format!("----tg-{}", rand::random::<u64>());
        let mut body: Vec<u8> = Vec::new();
        for (name, value) in fields {
            body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
            body.extend_from_slice(format!("Content-Disposition: form-data; name=\"{name}\"\r\n\r\n").as_bytes());
            body.extend_from_slice(value.as_bytes());
            body.extend_from_slice(b"\r\n");
        }
        body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
        body.extend_from_slice(
            format!("Content-Disposition: form-data; name=\"{file_field}\"; filename=\"{file_name}\"\r\n").as_bytes()
        );
        body.extend_from_slice(format!("Content-Type: {file_mime}\r\n\r\n").as_bytes());
        body.extend_from_slice(file_bytes);
        body.extend_from_slice(b"\r\n");
        body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());

        let url = self.endpoint(method);
        let resp = self.agent.post(&url)
            .set("Content-Type", &format!("multipart/form-data; boundary={boundary}"))
            .send_bytes(&body)
            .with_context(|| format!("POST {method} (multipart)"))?;
        let parsed: ApiResponse<Message> = resp.into_json()?;
        parsed.into_result()
    }

    pub fn send_photo(
        &self, chat_id: i64, file_path: &std::path::Path,
        caption: Option<&str>, parse_mode: Option<&str>, reply_to: Option<i64>,
    ) -> Result<Message> {
        self.send_file("sendPhoto", "photo", chat_id, file_path, caption, parse_mode, reply_to)
    }

    pub fn send_document(
        &self, chat_id: i64, file_path: &std::path::Path,
        caption: Option<&str>, parse_mode: Option<&str>, reply_to: Option<i64>,
    ) -> Result<Message> {
        self.send_file("sendDocument", "document", chat_id, file_path, caption, parse_mode, reply_to)
    }

    fn send_file(
        &self, method: &str, file_field: &str, chat_id: i64,
        path: &std::path::Path, caption: Option<&str>,
        parse_mode: Option<&str>, reply_to: Option<i64>,
    ) -> Result<Message> {
        let bytes = std::fs::read(path)
            .with_context(|| format!("reading {}", path.display()))?;
        let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("file");
        let mime = mime_guess::from_path(path).first_or_octet_stream().to_string();
        let chat_id_s = chat_id.to_string();
        // Telegram's new `reply_parameters` field is a JSON object; the
        // legacy `reply_to_message_id` scalar is still supported and is
        // simpler to encode as a multipart form field.
        let reply_to_s = reply_to.map(|i| i.to_string());
        let mut fields: Vec<(&str, &str)> = vec![("chat_id", chat_id_s.as_str())];
        if let Some(c) = caption { fields.push(("caption", c)); }
        if let Some(pm) = parse_mode { fields.push(("parse_mode", pm)); }
        if let Some(rs) = reply_to_s.as_deref() {
            fields.push(("reply_to_message_id", rs));
        }
        self.post_multipart(method, &fields, file_field, name, &mime, &bytes)
    }
```

- [ ] **Step 2: Add multipart test**

Append inside `mod tests`:

```rust
    #[test]
    fn send_photo_emits_multipart_with_chat_id() {
        let (base, join) = spawn_mock(|mut req| {
            assert!(req.url().ends_with("/sendPhoto"));
            let ct = req.headers().iter()
                .find(|h| h.field.equiv("Content-Type"))
                .map(|h| h.value.as_str().to_string())
                .unwrap_or_default();
            assert!(ct.starts_with("multipart/form-data; boundary="));
            let mut body = Vec::new();
            req.as_reader().read_to_end(&mut body).unwrap();
            let s = String::from_utf8_lossy(&body);
            assert!(s.contains("name=\"chat_id\""));
            assert!(s.contains("\r\n\r\n42\r\n"));
            assert!(s.contains("name=\"photo\""));
            let body_resp = r#"{"ok":true,"result":{"message_id":7,"chat":{"id":42,"type":"private"}}}"#;
            req.respond(tiny_http::Response::from_string(body_resp)
                .with_header("Content-Type: application/json".parse::<tiny_http::Header>().unwrap()))
                .unwrap();
        });

        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), b"fakepng").unwrap();

        let c = Client::new(base, "TOKEN");
        let m = c.send_photo(42, tmp.path(), Some("cap"), None, None).unwrap();
        assert_eq!(m.message_id, 7);
        join.join().unwrap();
    }
```

Also add the `Read` import at the top of `mod tests`:

```rust
    use std::io::Read;
```

- [ ] **Step 3: Run tests**

Run: `cargo test api::`
Expected: 3 tests pass.

- [ ] **Step 4: Commit**

```bash
git add src/api.rs
git commit -m "feat(api): multipart sendPhoto/sendDocument"
```

---

## Task 8: API getFile and file download

**Files:**
- Modify: `src/api.rs`
- Test: inline

- [ ] **Step 1: Add getFile + download_file to `src/api.rs`**

Add new struct above `impl Client`:

```rust
#[derive(Debug, Deserialize)]
pub struct File {
    pub file_id: String,
    pub file_unique_id: String,
    #[serde(default)] pub file_size: Option<u64>,
    pub file_path: Option<String>,
}
```

Append inside `impl Client`:

```rust
    pub fn get_file(&self, file_id: &str) -> Result<File> {
        #[derive(Serialize)]
        struct Req<'a> { file_id: &'a str }
        self.post_json("getFile", &Req { file_id })
    }

    /// Downloads the file referenced by `File.file_path` and writes the
    /// bytes to `dest`. Returns the number of bytes written.
    pub fn download_file(&self, file: &File, dest: &std::path::Path) -> Result<u64> {
        let path = file.file_path.as_deref()
            .ok_or_else(|| anyhow!("getFile response has no file_path"))?;
        let url = format!("{}/file/bot{}/{}",
            self.api_base.trim_end_matches('/'), self.token, path);
        let resp = self.agent.get(&url).call()
            .with_context(|| format!("GET {url}"))?;
        let mut reader = resp.into_reader();
        if let Some(parent) = dest.parent() { std::fs::create_dir_all(parent)?; }
        let mut out = std::fs::File::create(dest)?;
        let n = std::io::copy(&mut reader, &mut out)?;
        Ok(n)
    }
```

- [ ] **Step 2: Add test**

Append inside `mod tests`:

```rust
    #[test]
    fn get_file_parses_response() {
        let (base, join) = spawn_mock(|req| {
            let body = r#"{"ok":true,"result":{"file_id":"X","file_unique_id":"Y","file_path":"documents/file.pdf","file_size":1234}}"#;
            req.respond(tiny_http::Response::from_string(body)
                .with_header("Content-Type: application/json".parse::<tiny_http::Header>().unwrap()))
                .unwrap();
        });
        let c = Client::new(base, "TOKEN");
        let f = c.get_file("X").unwrap();
        assert_eq!(f.file_path.as_deref(), Some("documents/file.pdf"));
        join.join().unwrap();
    }
```

- [ ] **Step 3: Run tests**

Run: `cargo test api::`
Expected: 4 tests pass.

- [ ] **Step 4: Commit**

```bash
git add src/api.rs
git commit -m "feat(api): getFile + download_file"
```

---

## Task 9: `tg init` subcommand

**Files:**
- Create: `src/init.rs`
- Modify: `src/main.rs` (add `mod init;`, expand `Init` variant args, wire in match arm)
- Test: inline

- [ ] **Step 1: Write `src/init.rs`**

```rust
//! `tg init` — interactive (or flag-driven) initial config write.

use anyhow::{anyhow, Result};
use std::io::{BufRead, Write};

use crate::config::Config;
use crate::paths;

pub struct InitOpts {
    pub token: Option<String>,
    pub tmux_target: Option<String>,
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
        allow: vec![],
    };
    cfg.save(&path)?;
    println!("wrote {} (mode 0600)", path.display());
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
    fn writes_config_with_flags() {
        let dir = tempdir().unwrap();
        std::env::set_var("TG_HOME", dir.path());
        let opts = InitOpts {
            token: Some("TOKEN".into()),
            tmux_target: Some("root:1".into()),
            force: false,
        };
        run(opts).unwrap();
        let cfg = Config::load(&paths::config_path()).unwrap();
        assert_eq!(cfg.bot_token, "TOKEN");
        assert_eq!(cfg.tmux_target, "root:1");
        std::env::remove_var("TG_HOME");
    }

    #[test]
    fn refuses_overwrite_without_force() {
        let dir = tempdir().unwrap();
        std::env::set_var("TG_HOME", dir.path());
        let opts = InitOpts {
            token: Some("A".into()), tmux_target: Some("x".into()), force: false,
        };
        run(opts).unwrap();
        let opts2 = InitOpts {
            token: Some("B".into()), tmux_target: Some("y".into()), force: false,
        };
        let err = run(opts2).unwrap_err().to_string();
        assert!(err.contains("already exists"));
        std::env::remove_var("TG_HOME");
    }
}
```

- [ ] **Step 2: Wire into `src/main.rs`**

Replace the `Init` variant in the `Command` enum:

```rust
    /// Write ~/.tg/config.toml interactively.
    Init {
        #[arg(long)] token: Option<String>,
        #[arg(long)] tmux_target: Option<String>,
        #[arg(long)] force: bool,
    },
```

Add `mod init;` near the other module declarations.

Replace the match arm:

```rust
        Command::Init { token, tmux_target, force } =>
            init::run(init::InitOpts { token, tmux_target, force }),
```

- [ ] **Step 3: Run tests**

Run: `cargo test init::`
Expected: 2 tests pass.

⚠️ These tests set `TG_HOME`. They share the same env var with the paths
tests. Cargo's default test runner uses one process with multiple threads —
env vars are process-global. If you see flakes, run with
`RUST_TEST_THREADS=1`. We'll address this properly with a serialization
helper in Task 18 if it becomes a problem.

- [ ] **Step 4: Commit**

```bash
git add src/init.rs src/main.rs
git commit -m "feat(init): tg init writes config.toml interactively or via flags"
```

---

## Task 10: Allow / deny / list subcommands

**Files:**
- Create: `src/access.rs`
- Modify: `src/main.rs`
- Test: inline

- [ ] **Step 1: Write `src/access.rs`**

```rust
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
        let dir = tempdir().unwrap();
        seed(dir.path());
        allow(42, Some("alice".into())).unwrap();
        let cfg = Config::load(&paths::config_path()).unwrap();
        assert!(cfg.is_allowed(42));
        std::env::remove_var("TG_HOME");
    }

    #[test]
    fn allow_refuses_duplicate() {
        let dir = tempdir().unwrap();
        seed(dir.path());
        allow(42, None).unwrap();
        let err = allow(42, None).unwrap_err().to_string();
        assert!(err.contains("already"));
        std::env::remove_var("TG_HOME");
    }

    #[test]
    fn deny_removes_and_refuses_missing() {
        let dir = tempdir().unwrap();
        seed(dir.path());
        allow(7, None).unwrap();
        deny(7).unwrap();
        let cfg = Config::load(&paths::config_path()).unwrap();
        assert!(!cfg.is_allowed(7));
        let err = deny(7).unwrap_err().to_string();
        assert!(err.contains("not in allowlist"));
        std::env::remove_var("TG_HOME");
    }
}
```

- [ ] **Step 2: Wire into `src/main.rs`**

Add `mod access;`.

Replace the three variants in `Command`:

```rust
    /// Append a chat_id to the allowlist.
    Allow {
        #[arg(long)] chat_id: i64,
        #[arg(long)] label: Option<String>,
    },
    /// Remove a chat_id from the allowlist.
    Deny {
        #[arg(long)] chat_id: i64,
    },
    /// Print the current allowlist.
    List,
```

Replace the corresponding match arms:

```rust
        Command::Allow { chat_id, label } => access::allow(chat_id, label),
        Command::Deny { chat_id } => access::deny(chat_id),
        Command::List => access::list(),
```

- [ ] **Step 3: Run tests**

Run: `cargo test access::`
Expected: 3 tests pass.

- [ ] **Step 4: Commit**

```bash
git add src/access.rs src/main.rs
git commit -m "feat(access): tg allow / deny / list subcommands"
```

---

## Task 11: Pair / pending / reject subcommands

**Files:**
- Modify: `src/access.rs` (add pair/pending/reject)
- Modify: `src/main.rs`
- Test: inline

- [ ] **Step 1: Append to `src/access.rs`**

Add `use chrono::Utc;` to the imports section at the top.

Then append below the `list()` function:

```rust
use crate::pending::PendingStore;

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
    // if the allow-append succeeded.
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
    if let Err(e) = client.send_message(chat_id, "Paired. You can now send messages.") {
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
```

- [ ] **Step 2: Add inline tests**

Append inside `mod tests`:

```rust
    use crate::pending::PendingStore;

    #[test]
    fn pair_moves_pending_to_allow() {
        let dir = tempdir().unwrap();
        seed(dir.path());
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
        std::env::remove_var("TG_HOME");
    }

    #[test]
    fn pair_unknown_code_errors() {
        let dir = tempdir().unwrap();
        seed(dir.path());
        let err = pair("ZZZZZZ", "http://127.0.0.1:1").unwrap_err().to_string();
        assert!(err.contains("unknown"));
        std::env::remove_var("TG_HOME");
    }

    #[test]
    fn reject_removes_pending() {
        let dir = tempdir().unwrap();
        seed(dir.path());
        let mut store = PendingStore::default();
        store.insert_new(7, None, Utc::now());
        store.save(&paths::pending_path()).unwrap();

        reject(7).unwrap();
        let store2 = PendingStore::load(&paths::pending_path()).unwrap();
        assert!(store2.get(7).is_none());
        std::env::remove_var("TG_HOME");
    }
```

- [ ] **Step 3: Wire into `src/main.rs`**

Replace the three Command variants:

```rust
    /// Confirm a pending pairing by code.
    Pair { code: String },
    /// List pending pairings.
    Pending,
    /// Drop a pending pairing silently (no Telegram reply).
    Reject {
        #[arg(long)] chat_id: i64,
    },
```

Replace the match arms (note: pair needs `api_base` from the top-level Cli
struct — pass it through):

```rust
        Command::Pair { code } => access::pair(&code, &cli.api_base),
        Command::Pending => access::pending(),
        Command::Reject { chat_id } => access::reject(chat_id),
```

- [ ] **Step 4: Run tests**

Run: `cargo test access::`
Expected: 6 tests pass total in this module (3 from Task 10 + 3 new).

- [ ] **Step 5: Commit**

```bash
git add src/access.rs src/main.rs
git commit -m "feat(access): tg pair / pending / reject subcommands"
```

---

## Task 12: `tg send` subcommand

**Files:**
- Create: `src/send.rs`
- Modify: `src/main.rs`
- Test: inline (basic logic only — wire-level test lives in `tests/outbound.rs`)

- [ ] **Step 1: Write `src/send.rs`**

```rust
//! `tg send` — outbound text and attachments.

use anyhow::{Context, Result};
use std::path::PathBuf;

use crate::api::Client;
use crate::config::Config;
use crate::paths;

pub struct SendOpts {
    pub chat_id: i64,
    pub text: Option<String>,
    pub files: Vec<PathBuf>,
    pub format: Option<String>,
    pub reply_to: Option<i64>,
}

pub fn run(opts: SendOpts, api_base: &str) -> Result<()> {
    let cfg = Config::load(&paths::config_path())
        .with_context(|| "loading ~/.tg/config.toml")?;
    let client = Client::new(api_base, cfg.bot_token);

    if opts.files.is_empty() {
        let text = opts.text
            .ok_or_else(|| anyhow::anyhow!("--text required when no --file given"))?;
        let m = client.send_message(opts.chat_id, &text)?;
        println!("sent (id: {})", m.message_id);
        return Ok(());
    }

    let mut first = true;
    for path in &opts.files {
        // First file carries the caption; subsequent files send without.
        let caption = if first { opts.text.as_deref() } else { None };
        first = false;

        let kind = mime_guess::from_path(path).first_or_octet_stream();
        let is_image = kind.type_() == "image";
        let m = if is_image {
            client.send_photo(opts.chat_id, path, caption, opts.format.as_deref(), opts.reply_to)
        } else {
            client.send_document(opts.chat_id, path, caption, opts.format.as_deref(), opts.reply_to)
        }.with_context(|| format!("sending {}", path.display()))?;
        println!("sent {} (id: {})", path.display(), m.message_id);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requires_text_when_no_files() {
        // We can't easily run() without a saved config; this tests the
        // logical guard by replicating its check.
        let opts = SendOpts {
            chat_id: 1, text: None, files: vec![],
            format: None, reply_to: None,
        };
        assert!(opts.text.is_none() && opts.files.is_empty());
    }
}
```

- [ ] **Step 2: Wire into `src/main.rs`**

Add `mod send;`.

Replace the `Send` variant:

```rust
    /// Send a message (with optional --file attachments).
    Send {
        #[arg(long)] chat_id: i64,
        #[arg(long)] text: Option<String>,
        #[arg(long)] file: Vec<std::path::PathBuf>,
        #[arg(long)] format: Option<String>,
        #[arg(long = "reply-to")] reply_to: Option<i64>,
    },
```

Replace the match arm:

```rust
        Command::Send { chat_id, text, file, format, reply_to } =>
            send::run(send::SendOpts { chat_id, text, files: file, format, reply_to }, &cli.api_base),
```

- [ ] **Step 3: Verify compilation**

Run: `cargo build`
Expected: clean build.

- [ ] **Step 4: Commit**

```bash
git add src/send.rs src/main.rs
git commit -m "feat(send): tg send subcommand for text + attachments"
```

---

## Task 13: `tg listen` subcommand — poll loop and inbound flow

**Files:**
- Create: `src/listen.rs`
- Modify: `src/main.rs`
- Test: gate-logic helpers inline; full poll loop tested in `tests/inbound.rs` (Task 16).

- [ ] **Step 1: Write `src/listen.rs`**

```rust
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
const STATE_FILE_TMP_SUFFIX: &str = ".tmp";

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
        let _ = client.send_message(chat_id, "agent offline (Claude Code not running)");
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
        let _ = client.send_message(chat_id, &text);
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
    let tmp = p.with_extension(format!("state{STATE_FILE_TMP_SUFFIX}"));
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
        std::env::set_var("TG_HOME", dir.path());
        assert_eq!(read_offset().unwrap(), 0);
        std::env::remove_var("TG_HOME");
    }

    #[test]
    fn write_then_read_offset_roundtrips() {
        let dir = tempdir().unwrap();
        std::env::set_var("TG_HOME", dir.path());
        write_offset(12345).unwrap();
        assert_eq!(read_offset().unwrap(), 12345);
        std::env::remove_var("TG_HOME");
    }
}
```

- [ ] **Step 2: Wire into `src/main.rs`**

Add `mod listen;`.

Replace the `Listen` match arm:

```rust
        Command::Listen => listen::run(&cli.api_base, &cli.tmux_bin),
```

- [ ] **Step 3: Run tests**

Run: `cargo test listen::`
Expected: 2 tests pass.

- [ ] **Step 4: Commit**

```bash
git add src/listen.rs src/main.rs
git commit -m "feat(listen): poll loop, gating, attachment download, offline reply"
```

---

## Task 14: `tg install` subcommand + systemd unit file

**Files:**
- Create: `systemd/tg-listen.service`
- Create: `src/install.rs`
- Modify: `src/main.rs`
- Test: inline (file-operations only; systemctl calls mocked via the bin path)

- [ ] **Step 1: Write `systemd/tg-listen.service`**

```ini
[Unit]
Description=Telegram channel listener (tg)
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
ExecStart=%h/.cargo/bin/tg listen
Restart=always
RestartSec=5
StandardOutput=journal
StandardError=journal

[Install]
WantedBy=default.target
```

- [ ] **Step 2: Write `src/install.rs`**

```rust
//! `tg install` — symlink into ~/.ir/tools/ and install systemd unit.
//!
//! Idempotent. Each step checks current state before mutating.

use anyhow::{anyhow, Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

const SERVICE_BODY: &str = include_str!("../systemd/tg-listen.service");
const SERVICE_NAME: &str = "tg-listen.service";

pub struct InstallOpts {
    pub systemctl_bin: String,
    pub dry_run: bool,
}

pub fn run(opts: InstallOpts) -> Result<()> {
    let home = std::env::var("HOME").context("HOME not set")?;
    let home = PathBuf::from(home);

    let cargo_bin = home.join(".cargo/bin/tg");
    if !cargo_bin.exists() {
        return Err(anyhow!(
            "{} does not exist; run `cargo install --path .` first",
            cargo_bin.display()
        ));
    }

    // Step 1: ~/.ir/tools/tg symlink (if ~/.ir/tools/ exists).
    let tools_dir = home.join(".ir/tools");
    if tools_dir.exists() {
        ensure_symlink(&cargo_bin, &tools_dir.join("tg"), opts.dry_run)?;
    } else {
        println!("(skip) ~/.ir/tools/ does not exist; no ir-tool symlink created");
    }

    // Step 2: copy systemd unit if missing or different.
    let unit_dir = home.join(".config/systemd/user");
    let unit_path = unit_dir.join(SERVICE_NAME);
    ensure_unit(&unit_path, opts.dry_run)?;

    // Step 3-4: daemon-reload + enable (do not start).
    if !opts.dry_run {
        run_cmd(&opts.systemctl_bin, &["--user", "daemon-reload"])?;
        run_cmd(&opts.systemctl_bin, &["--user", "enable", SERVICE_NAME])?;
    }

    println!("install complete. run `tg init` if you haven't, then `systemctl --user start tg-listen`.");
    Ok(())
}

fn ensure_symlink(target: &Path, link: &Path, dry_run: bool) -> Result<()> {
    match std::fs::symlink_metadata(link) {
        Ok(meta) if meta.file_type().is_symlink() => {
            let actual = std::fs::read_link(link)?;
            if actual == target {
                println!("(ok) {} -> {}", link.display(), target.display());
                return Ok(());
            }
            return Err(anyhow!(
                "{} exists and points to {} (not our binary); resolve manually",
                link.display(), actual.display()
            ));
        }
        Ok(_) => {
            return Err(anyhow!(
                "{} exists and is not a symlink; resolve manually",
                link.display()
            ));
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            // Create it.
        }
        Err(e) => return Err(e.into()),
    }
    if dry_run {
        println!("(dry-run) would symlink {} -> {}", link.display(), target.display());
    } else {
        if let Some(parent) = link.parent() { std::fs::create_dir_all(parent)?; }
        std::os::unix::fs::symlink(target, link)?;
        println!("symlinked {} -> {}", link.display(), target.display());
    }
    Ok(())
}

fn ensure_unit(path: &Path, dry_run: bool) -> Result<()> {
    let needs_write = match std::fs::read_to_string(path) {
        Ok(existing) => existing != SERVICE_BODY,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => true,
        Err(e) => return Err(e.into()),
    };
    if !needs_write {
        println!("(ok) {} up to date", path.display());
        return Ok(());
    }
    if dry_run {
        println!("(dry-run) would write {}", path.display());
        return Ok(());
    }
    if let Some(parent) = path.parent() { std::fs::create_dir_all(parent)?; }
    std::fs::write(path, SERVICE_BODY)?;
    println!("wrote {}", path.display());
    Ok(())
}

fn run_cmd(bin: &str, args: &[&str]) -> Result<()> {
    let status = Command::new(bin).args(args).status()
        .with_context(|| format!("running {bin} {}", args.join(" ")))?;
    if !status.success() {
        return Err(anyhow!("{bin} {} exited {status}", args.join(" ")));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn ensure_symlink_creates_when_missing() {
        let dir = tempdir().unwrap();
        let target = dir.path().join("bin/tg");
        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        std::fs::write(&target, "").unwrap();
        let link = dir.path().join("tools/tg");
        ensure_symlink(&target, &link, false).unwrap();
        assert_eq!(std::fs::read_link(&link).unwrap(), target);
    }

    #[test]
    fn ensure_symlink_refuses_alien_existing() {
        let dir = tempdir().unwrap();
        let link = dir.path().join("tg");
        std::fs::write(&link, "alien").unwrap();
        let target = dir.path().join("ours");
        std::fs::write(&target, "").unwrap();
        let err = ensure_symlink(&target, &link, false).unwrap_err().to_string();
        assert!(err.contains("not a symlink"));
    }

    #[test]
    fn ensure_unit_writes_then_idempotent() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("unit.service");
        ensure_unit(&p, false).unwrap();
        assert!(p.exists());
        // Same body → no rewrite (we can't observe directly, but call again).
        ensure_unit(&p, false).unwrap();
    }
}
```

- [ ] **Step 3: Wire into `src/main.rs`**

Add `mod install;`.

Add a hidden flag for the systemctl bin override and replace the Install
arm.

In the `Cli` struct, add:

```rust
    /// Hidden: override systemctl binary path (for tests).
    #[arg(long, hide = true, global = true, default_value = "systemctl")]
    systemctl_bin: String,
```

Replace the match arm:

```rust
        Command::Install => install::run(install::InstallOpts {
            systemctl_bin: cli.systemctl_bin.clone(),
            dry_run: false,
        }),
```

- [ ] **Step 4: Run tests**

Run: `cargo test install::`
Expected: 3 tests pass.

- [ ] **Step 5: Verify the systemd unit body is embedded**

Run: `cargo build && strings target/debug/tg | grep "Telegram channel listener"`
Expected: one match (the include_str! brought the unit body into the binary).

- [ ] **Step 6: Commit**

```bash
git add systemd/tg-listen.service src/install.rs src/main.rs
git commit -m "feat(install): tg install symlinks bin and writes systemd unit"
```

---

## Task 15: Outbound integration test

**Files:**
- Create: `tests/outbound.rs`

- [ ] **Step 1: Write `tests/outbound.rs`**

```rust
//! End-to-end test for `tg send` against a local tiny_http mock.
//!
//! Builds the binary via cargo, sets up a tempdir TG_HOME with a
//! config.toml, points `--api-base` at the mock, and asserts the request
//! shape the mock saw.

use std::io::Read;
use std::process::Command;
use std::sync::Arc;
use std::thread;

fn binary() -> std::path::PathBuf {
    let exe = std::env::var("CARGO_BIN_EXE_tg").expect("CARGO_BIN_EXE_tg not set");
    std::path::PathBuf::from(exe)
}

fn write_config(home: &std::path::Path, token: &str) {
    let cfg = format!("bot_token = \"{token}\"\ntmux_target = \"none:0\"\n");
    let path = home.join("config.toml");
    std::fs::create_dir_all(home).unwrap();
    std::fs::write(&path, cfg).unwrap();
    let mut perms = std::fs::metadata(&path).unwrap().permissions();
    use std::os::unix::fs::PermissionsExt;
    perms.set_mode(0o600);
    std::fs::set_permissions(&path, perms).unwrap();
}

#[test]
fn send_text_hits_send_message_with_expected_body() {
    let home = tempfile::tempdir().unwrap();
    write_config(home.path(), "TOKEN");

    let server = Arc::new(tiny_http::Server::http("127.0.0.1:0").unwrap());
    let port = server.server_addr().to_ip().unwrap().port();
    let s2 = server.clone();

    let join = thread::spawn(move || {
        let mut req = s2.recv().unwrap();
        assert!(req.url().contains("/botTOKEN/sendMessage"), "got {}", req.url());
        let mut body = Vec::new();
        req.as_reader().read_to_end(&mut body).unwrap();
        let s = String::from_utf8_lossy(&body);
        assert!(s.contains("\"chat_id\":42"), "got body: {s}");
        assert!(s.contains("\"text\":\"hello\""), "got body: {s}");

        let resp = r#"{"ok":true,"result":{"message_id":99,"chat":{"id":42,"type":"private"}}}"#;
        req.respond(tiny_http::Response::from_string(resp)
            .with_header("Content-Type: application/json".parse::<tiny_http::Header>().unwrap()))
            .unwrap();
    });

    let out = Command::new(binary())
        .args([
            "--api-base", &format!("http://127.0.0.1:{port}"),
            "send", "--chat-id", "42", "--text", "hello",
        ])
        .env("TG_HOME", home.path())
        .output()
        .unwrap();
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("sent (id: 99)"), "got: {stdout}");

    join.join().unwrap();
}

#[test]
fn send_file_hits_send_photo_with_multipart() {
    let home = tempfile::tempdir().unwrap();
    write_config(home.path(), "TOKEN");

    let server = Arc::new(tiny_http::Server::http("127.0.0.1:0").unwrap());
    let port = server.server_addr().to_ip().unwrap().port();
    let s2 = server.clone();

    let join = thread::spawn(move || {
        let mut req = s2.recv().unwrap();
        assert!(req.url().contains("/botTOKEN/sendPhoto"), "got {}", req.url());
        let ct = req.headers().iter()
            .find(|h| h.field.equiv("Content-Type"))
            .map(|h| h.value.as_str().to_string()).unwrap_or_default();
        assert!(ct.starts_with("multipart/form-data"), "ct: {ct}");

        let resp = r#"{"ok":true,"result":{"message_id":5,"chat":{"id":42,"type":"private"}}}"#;
        req.respond(tiny_http::Response::from_string(resp)
            .with_header("Content-Type: application/json".parse::<tiny_http::Header>().unwrap()))
            .unwrap();
    });

    // Create a tiny PNG-shaped file. `Builder` is the way to get a
    // suffixed tempfile in the tempfile crate.
    let tmp = tempfile::Builder::new().suffix(".png").tempfile().unwrap();
    std::fs::write(tmp.path(), b"\x89PNG\r\n\x1a\nFAKE").unwrap();

    let out = Command::new(binary())
        .args([
            "--api-base", &format!("http://127.0.0.1:{port}"),
            "send", "--chat-id", "42", "--file", tmp.path().to_str().unwrap(),
        ])
        .env("TG_HOME", home.path())
        .output()
        .unwrap();
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));

    join.join().unwrap();
}
```

- [ ] **Step 2: Run the integration test**

Run: `cargo test --test outbound`
Expected: 2 tests pass.

- [ ] **Step 3: Commit**

```bash
git add tests/outbound.rs
git commit -m "test(outbound): tg send hits sendMessage and sendPhoto correctly"
```

---

## Task 16: Inbound integration test

**Files:**
- Create: `tests/inbound.rs`

- [ ] **Step 1: Write `tests/inbound.rs`**

```rust
//! End-to-end test for `tg listen` against a mock telegram and a fake
//! tmux shim that records its invocations.

use std::process::{Command, Stdio};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

fn binary() -> std::path::PathBuf {
    let exe = std::env::var("CARGO_BIN_EXE_tg").expect("CARGO_BIN_EXE_tg not set");
    std::path::PathBuf::from(exe)
}

/// Writes a shell script to `path` that records its argv to
/// `record_path` (one line per invocation, args joined with `\t`) and
/// always exits 0.
fn write_fake_tmux(path: &std::path::Path, record_path: &std::path::Path) {
    let script = format!(
        "#!/bin/bash\n\
         printf '%s\\t' \"$@\" >> {}\n\
         printf '\\n' >> {}\n\
         # has-session pretends pane exists.\n\
         exit 0\n",
        record_path.display(), record_path.display());
    std::fs::write(path, script).unwrap();
    use std::os::unix::fs::PermissionsExt;
    let mut perms = std::fs::metadata(path).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(path, perms).unwrap();
}

fn write_config(home: &std::path::Path, token: &str, allow_chat: i64) {
    let cfg = format!(
        "bot_token = \"{token}\"\n\
         tmux_target = \"test:0\"\n\
         \n\
         [[allow]]\n\
         chat_id = {allow_chat}\n\
         label = \"alice\"\n");
    let path = home.join("config.toml");
    std::fs::create_dir_all(home).unwrap();
    std::fs::write(&path, cfg).unwrap();
    use std::os::unix::fs::PermissionsExt;
    let mut perms = std::fs::metadata(&path).unwrap().permissions();
    perms.set_mode(0o600);
    std::fs::set_permissions(&path, perms).unwrap();
}

#[test]
fn allowed_text_message_is_send_keysed_to_pane() {
    let home = tempfile::tempdir().unwrap();
    let shim_dir = tempfile::tempdir().unwrap();
    let shim = shim_dir.path().join("fake-tmux");
    let record = shim_dir.path().join("argv.log");
    write_fake_tmux(&shim, &record);
    write_config(home.path(), "TOKEN", 100);

    let server = Arc::new(tiny_http::Server::http("127.0.0.1:0").unwrap());
    let port = server.server_addr().to_ip().unwrap().port();
    let s2 = server.clone();

    // Serve one getUpdates with a single message, then one empty, then
    // exit the test by killing tg listen.
    let mock_join = thread::spawn(move || {
        let mut served = 0u32;
        while let Ok(req) = s2.recv() {
            served += 1;
            let url = req.url().to_string();
            assert!(url.contains("/getUpdates"), "url: {url}");
            let body = if served == 1 {
                r#"{"ok":true,"result":[{"update_id":1,"message":{"message_id":7,"from":{"id":100,"username":"alice"},"chat":{"id":100,"type":"private"},"text":"hi there"}}]}"#
            } else {
                r#"{"ok":true,"result":[]}"#
            };
            req.respond(tiny_http::Response::from_string(body)
                .with_header("Content-Type: application/json".parse::<tiny_http::Header>().unwrap()))
                .unwrap();
            if served >= 2 { return; }
        }
    });

    let mut child = Command::new(binary())
        .args([
            "--api-base", &format!("http://127.0.0.1:{port}"),
            "--tmux-bin", shim.to_str().unwrap(),
            "listen",
        ])
        .env("TG_HOME", home.path())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();

    // Give listen time to handle one update + one empty poll.
    thread::sleep(Duration::from_secs(3));
    let _ = child.kill();
    let _ = child.wait();
    let _ = mock_join.join();

    let log = std::fs::read_to_string(&record).unwrap();
    // Expect at least: has-session, send-keys -l "[telegram @alice ...]", send-keys Enter.
    assert!(log.contains("has-session"), "no has-session in log: {log}");
    assert!(log.contains("send-keys"), "no send-keys in log: {log}");
    assert!(log.contains("@alice"), "no @alice in log: {log}");
    assert!(log.contains("hi there"), "no body in log: {log}");
    assert!(log.contains("Enter"), "no Enter in log: {log}");
}
```

- [ ] **Step 2: Run the test**

Run: `cargo test --test inbound`
Expected: 1 test passes (it has a 3-second wait; that's a sleep, not a
deadlock — the mock thread exits as soon as it's served two requests).

- [ ] **Step 3: Commit**

```bash
git add tests/inbound.rs
git commit -m "test(inbound): listen routes allowed text through tmux send-keys"
```

---

## Task 17: Smoke checklist + README polish

**Files:**
- Create: `docs/smoke.md`
- Modify: `README.md`

- [ ] **Step 1: Write `docs/smoke.md`**

```markdown
# tg — manual smoke checklist

These steps verify the binary end-to-end against the real Telegram API
and a live tmux. Run them once after build to catch anything the
integration tests don't model (terminal rendering, real send-keys
interaction with a Claude Code TUI, journald output).

## Prerequisites

- A Telegram bot token (create one via @BotFather; save it).
- A running tmux pane named `root:1` (or whatever you'll set
  `tmux_target` to). Claude Code or a plain shell in that pane is fine
  for the smoke test.

## Steps

1. **Install.**
   ```
   cd ~/projects/tg
   cargo install --path .
   tg install
   ```
   Expect symlink + systemd unit messages, no errors.

2. **Init.**
   ```
   tg init
   ```
   Paste your bot token; press Enter for the default `root:1`. Expect
   `wrote ~/.tg/config.toml (mode 0600)`.

3. **Start the listener.**
   ```
   systemctl --user start tg-listen
   journalctl --user -u tg-listen -f
   ```
   Leave the journal tailing in another terminal.

4. **Send a DM from your phone to the bot.**
   - Expected on phone: "Pairing required — run in your terminal: `tg
     pair XXXXXX`".
   - Expected in journal: a line noting the pending pair.

5. **Pair.**
   ```
   tg pending           # confirms the entry shows up
   tg pair XXXXXX       # use the code from your phone
   ```
   Expect: phone receives "Paired. You can now send messages." and
   `tg list` now shows the chat_id.

6. **Send another DM.**
   The message text should appear in your tmux pane formatted as
   `[telegram @yourname (chat_id=NNN)] <text>`, and Enter should fire.
   If you're inside Claude Code, the line becomes a user turn.

7. **Reply.**
   ```
   tg send --chat-id NNN --text "ack"
   ```
   Expect the message on your phone.

8. **Attachment outbound.**
   ```
   tg send --chat-id NNN --file ~/some.png
   tg send --chat-id NNN --file ~/some.pdf --text "see attached"
   ```
   Photo arrives inline; PDF arrives as a document with caption.

9. **Attachment inbound.**
   Send a photo from your phone. In tmux, you should see the formatted
   line ending with `[file: /home/.../.tg/inbox/...png]`.

10. **Offline reply.**
    Stop the tmux pane: in another terminal, `tmux kill-session -t
    root`. Send a DM. You should receive "agent offline (Claude Code not
    running)" on your phone within ~1s.

11. **Cleanup the test pair (optional).**
    ```
    tg deny --chat-id NNN
    ```
```

- [ ] **Step 2: Polish `README.md`**

```markdown
# tg

Self-contained Telegram bot CLI: an inbound daemon delivers messages
into a tmux pane via `send-keys`, and an outbound CLI sends text and
attachments. Replaces the patched Bun/TS Telegram MCP plugin with a
single Rust binary.

## Install

```bash
cargo install --path .
tg install     # symlink into ~/.ir/tools/, install systemd unit
tg init        # write ~/.tg/config.toml (prompts for token)
systemctl --user start tg-listen
```

## Subcommands

| Command | Purpose |
| --- | --- |
| `tg init` | Write `~/.tg/config.toml` |
| `tg install` | Symlink binary into `~/.ir/tools/`, install + enable systemd unit |
| `tg listen` | Inbound daemon (usually run via systemd) |
| `tg send` | Outbound text and/or attachments |
| `tg allow` | Append chat_id to allowlist |
| `tg deny` | Remove chat_id from allowlist |
| `tg list` | Print allowlist |
| `tg pair <code>` | Confirm a pending pairing |
| `tg pending` | List pending pairings |
| `tg reject` | Drop a pending pairing silently |

## Documentation

- `docs/design.md` — architecture and design decisions
- `docs/plan.md` — implementation plan (task-by-task)
- `docs/smoke.md` — end-to-end manual verification

## Logs

```
journalctl --user -u tg-listen -f
```
```

- [ ] **Step 3: Commit**

```bash
git add docs/smoke.md README.md
git commit -m "docs: smoke checklist and README polish"
```

---

## Task 18 (optional): Test serialization helper for `TG_HOME` tests

**Files:**
- Modify: `src/paths.rs`

**When to run this task:** only if Tasks 9, 10, 11, and 13 show flakes when
`cargo test` runs with default parallelism. If you've completed those
tasks and they're green at the default thread count, skip this and move
on. If they flake, do this task and re-run them with the new helper.

- [ ] **Step 1: Add a test-only mutex to `src/paths.rs`**

Append to the file:

```rust
#[cfg(test)]
pub mod test_lock {
    use std::sync::{Mutex, MutexGuard};
    use std::sync::OnceLock;
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    /// Hold this guard for the duration of any test that sets TG_HOME.
    pub fn acquire() -> MutexGuard<'static, ()> {
        LOCK.get_or_init(|| Mutex::new(())).lock().unwrap_or_else(|e| e.into_inner())
    }
}
```

- [ ] **Step 2: Update each `TG_HOME`-using test to acquire the lock**

In every `#[test]` that does `std::env::set_var("TG_HOME", ...)`, add as
the first line:

```rust
        let _g = crate::paths::test_lock::acquire();
```

This includes tests in `config::tests`, `pending::tests`, `init::tests`,
`access::tests`, `listen::tests`. (The `tests/outbound.rs` and
`tests/inbound.rs` integration tests run in separate processes and don't
need the lock.)

- [ ] **Step 3: Run the whole test suite**

Run: `cargo test`
Expected: all tests pass at default parallelism.

- [ ] **Step 4: Commit**

```bash
git add src/*.rs
git commit -m "test: serialize TG_HOME-mutating unit tests via a process-wide mutex"
```

---

## Final verification

After all tasks, run:

```bash
cd ~/projects/tg
cargo build --release
cargo test
```

Expected: clean release build; all unit + integration tests pass.

Then proceed to `docs/smoke.md` for live verification against the real
Telegram API and a real tmux pane.
