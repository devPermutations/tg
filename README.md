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
| `tg reject --chat-id N` | Drop a pending pairing silently (no Telegram reply) |

## Documentation

- `docs/design.md` — architecture and design decisions
- `docs/plan.md` — implementation plan (task-by-task)
- `docs/smoke.md` — end-to-end manual verification

## Logs

```
journalctl --user -u tg-listen -f
```
