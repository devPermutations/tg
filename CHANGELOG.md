# Changelog

All notable changes to `tg` are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html)
(pre-1.0: any minor bump can include behavior changes).

## [Unreleased]

(nothing yet — v0.6.0 in progress)

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
