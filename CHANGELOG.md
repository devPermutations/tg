# Changelog

All notable changes to `tg` are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html)
(pre-1.0: any minor bump can include behavior changes).

## [Unreleased]

(no unreleased changes)

## [0.7.0] — 2026-05-28

Architecture-review part-two: deferred items now closed.

### Added

- **Deployment artifacts** for non-Linux+systemd targets:
  `launchd/com.devpermutations.tg.plist` (macOS LaunchAgent template),
  `Dockerfile` (multi-stage Alpine musl static build), and
  `.github/workflows/{ci,release}.yml` (cargo fmt+clippy+test on push,
  static-musl binary attached to GitHub releases on tag push).
- `docs/install-non-linux.md` walking through macOS / Docker / OpenRC.
- **SIGHUP-based config reload.** `systemctl --user kill --signal=HUP
  tg-listen` (or `kill -HUP <pid>`) makes the daemon re-read its
  config from disk without dropping the Telegram long-poll. Token
  changes rebuild the ureq Client transparently. Reload failures log
  a warning and keep the in-memory copy — never crash on bad config.
- `MediaRef` enum on the API surface (`api::types`) with `Photo`,
  `Document`, `Voice`, `Audio`, `Sticker` variants. `Message::media_ref()`
  extracts it; `MediaRef::is_transcribable()` dispatches the whisper
  branch. Replaces the previous opaque `Option<(&str, Option<&str>)>`
  in `build_body`.
- `TmuxClient` struct wrapping `tmux_bin: &str` plus `target_alive`
  and `send_line` methods. `handle_update` loses the deep string-
  threading.

### Changed

- **Crate renamed from `tg` to `tgcli`** for crates.io publication.
  The binary name stays `tg`; only the install command differs:
  `cargo install tgcli`. Source-tree installs (`cargo install --path .`)
  are unaffected.
- **`TG_HOME` test-mutex replaced with a per-thread override.**
  `paths::test_helpers::set_test_tg_home(path)` returns a guard
  that overrides `tg_home()` for the calling thread and restores
  on drop. The process-wide `Mutex` is gone; unit tests run truly
  parallel.
- **`api.rs` split into `api/client.rs` + `api/types.rs` + `api/mod.rs`**
  (re-exports preserve all existing `crate::api::*` paths).
- **`access.rs` split into `access/allowlist.rs` + `access/pairing.rs`
  + `access/mod.rs`** (re-exports preserve `crate::access::*`).

### Internal

- New deps: `signal-hook = "0.3"` (one transitive dep,
  `signal-hook-registry`).
- Test count: 70 total (64 unit + 3 inbound integration + 2 outbound
  integration + 1 transcribe e2e). The thread-local refactor
  removed 4 mutex-acquire calls; the paths module gained 3 new
  tests for the thread-local isolation invariant.

## [0.6.0] — 2026-05-28

Architecture-review pass. No behavior-breaking changes for users on
v0.5; everything is additive or internal.

### Added

- `Config::schema_version: u32` field (defaults to `1`). Forward-compat
  marker so future versions can detect and migrate v1 configs.
- `[transcription]` table in `config.toml` (`backend = "whisper-cpp"`,
  `url = "..."`). The legacy bare `whisper_url` field still parses
  and is automatically promoted at read time via the new
  `Config::whisper_url()` accessor.
- `tracing::info!` audit-log lines on every state mutation: `allow`,
  `deny`, `set_owner`, `pair`, `reject`. Daemon journals now read as
  an audit trail.
- `tracing::info!` on successful inbound delivery — happy-path is no
  longer silent in the journal.
- Default `RUST_LOG` filter is now `tg=info` (was: error-only). A
  fresh `journalctl --user -u tg-listen -f` actually shows lifecycle.
- New integration tests: `agent_offline_reply_when_pane_down`,
  `unknown_sender_silent_drop_when_owner_set` (both in
  `tests/inbound.rs`), and the full
  `voice_message_transcribed_via_whisper_mock` pipeline in
  `tests/transcribe.rs`.

### Changed

- Owner ⊆ allowlist is now a true invariant. `tg init --owner-chat-id
  N` and `tg set-owner --chat-id N` both auto-append an `[[allow]]`
  entry for the owner (label `"owner"`). `tg list`'s special-case
  block for "owner without allow entry" was dropped — that state is
  no longer possible.
- "Agent offline" Telegram reply is throttled to once per 30 seconds
  per chat_id. The message no longer mentions Claude Code; it now
  names the configured tmux target pane.
- `PendingStore::entries` is now `HashMap<i64, PendingEntry>` (was
  `HashMap<String, _>`). Wire format unchanged — JSON map keys are
  still strings, handled at the Serde boundary.
- `access::append_allow` returns a typed `AllowError` (was: anyhow
  with `.to_string().contains("already")` sentinel matching).
- Token-redaction helper lifted from `Client::redact_err` (method,
  bound to one secret) to a free `api::redact_err(prefix, e,
  &[secrets])` function. `transcribe` now uses it too, in case a
  user configures `http://user:pass@host` for `whisper_url`.

### Fixed

- Documentation drift: README no longer claims "40 tests"; the
  literal count is gone (replaced with "runs the full suite").
- `docs/design.md` layout block now lists `transcribe.rs`.
- Outbound disclaimer added to README's access-control section
  (outbound is intentionally ungated; assumes the local account is
  trusted).

## [0.5.2] — 2026-05-28

### Security

- **Bot token leaked into journald via ureq error formatting.**
  See [GHSA-5pvm-3m24-8p3f](https://github.com/devPermutations/tg/security/advisories/GHSA-5pvm-3m24-8p3f).
  Fixed by adding `Client::redact_err` that strips the bot token from
  every error string before it reaches `tracing` or `anyhow`. CVSS 7.1
  (HIGH).

## [0.5.1] — 2026-05-28

### Documentation

- Restructure README around access control and security: explicit
  threat model, layered defenses, trust-tier table, pairing flow
  documented.

## [0.5.0] — 2026-05-28

### Added

- Optional voice / audio transcription via a whisper.cpp HTTP server.
  Set `whisper_url` in `config.toml`; the daemon transcribes
  synchronously before delivering the prompt line, appending
  `[transcript: ...]`. Requires `ffmpeg` on the host.

## [0.4.0] — 2026-05-28

### Added

- Inbound voice messages, audio files, and stickers download to
  `~/.tg/inbox/`. Rendered with terse labels: `(voice 0:12)`,
  `(audio M:SS: title)`, `(sticker 🎉)`.

## [0.3.0] — 2026-05-28

### Changed

- When `owner_chat_id` is set, the daemon silently drops DMs from
  unknown senders (no pairing reminder). The owner is the only party
  authorized to add contacts via `tg allow`.

## [0.2.0] — 2026-05-28

### Added

- `owner_chat_id` field locks inbound delivery to a single designated
  chat. All other allowlist entries become outbound-only.
- `tg set-owner --chat-id N` and `tg set-owner --unset` commands.

## [0.1.0] — 2026-05-28

### Added

- Initial release. Inbound daemon (`tg listen`) long-polls Telegram
  and types messages into a tmux pane via `send-keys`. Outbound CLI
  (`tg send`) sends text, photos, and documents. Per-chat allowlist
  with code-based pairing flow. systemd user-service supervisor.
