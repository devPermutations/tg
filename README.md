# tg

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

A small Rust CLI that wires a Telegram bot to your terminal.

- **`tg listen`** — a daemon that long-polls Telegram and types each incoming
  message directly into a tmux pane via `send-keys`, so the message becomes
  user input for whatever you have running there (a shell, a TUI, an agent).
- **`tg send`** — outbound CLI. Sends text, photos, and documents to any
  chat in your allowlist.

Access is gated by a per-chat allowlist with a pairing flow: an unknown
sender DM'ing the bot gets a one-time code; you confirm with `tg pair
XXXXXX` from your terminal. State and config live under `~/.tg/` at mode
`0600`.

No MCP server, no Node/Bun runtime, no plugin manager — a single static
Rust binary plus a systemd user unit.

## Why this exists

I wanted a Telegram bridge into [Claude
Code](https://docs.claude.com/en/docs/claude-code) — type a message on my
phone, have it land in the agent's prompt — without running a Node/Bun
MCP server and without depending on undocumented Claude Code
notification machinery. The send-keys approach is dumb-simple and works
on any TUI: Claude Code, Aider, a plain shell, Vim, whatever.

If you want the same shape for some other terminal app, this CLI works
out of the box. Point `tmux_target` at the pane you want messages
delivered to.

## Install

Prerequisites: Rust 1.75+, tmux, a Telegram bot token (from
[@BotFather](https://t.me/BotFather)), and systemd (for the daemon
supervisor).

```bash
git clone https://github.com/devPermutations/tg
cd tg
cargo install --path .

tg install   # symlinks ~/.cargo/bin/tg -> ~/.ir/tools/tg (if dir exists),
             # installs + enables ~/.config/systemd/user/tg-listen.service

tg init      # writes ~/.tg/config.toml (mode 0600); prompts for token
             # and tmux target (default: root:1)

systemctl --user start tg-listen
journalctl --user -u tg-listen -f          # tail logs
```

To allowlist someone, either:
- Have them DM the bot first; they'll get a pairing code; you run `tg
  pair XXXXXX`.
- Or add them directly: `tg allow --chat-id 12345 --label alice`.

## Usage

```
$ tg send --chat-id 8583339367 --text "hello"
sent (id: 99)

$ tg send --chat-id 8583339367 --text "look at this" --file ~/screenshot.png
sent /home/me/screenshot.png (id: 100)

$ tg send --chat-id 8583339367 --text "*bold*" --format markdownv2
sent (id: 101)

$ tg send --chat-id 8583339367 --text "reply" --reply-to 99
sent (id: 102)

$ tg pending
8598991658	K7M3P2	tim	58m12s remaining

$ tg pair K7M3P2
paired chat_id 8598991658

$ tg list
8583339367	virgil
8598991658	tim
```

Incoming text messages appear in your tmux pane as:

```
[telegram @virgil (chat_id=8583339367)] hello there
```

Attachments (photos / documents) are downloaded to `~/.tg/inbox/` and
the path is appended to the typed line:

```
[telegram @virgil (chat_id=8583339367)] check this out [file: /home/me/.tg/inbox/1716852720-AgADBA...jpg]
```

## Subcommands

| Command | Purpose |
| --- | --- |
| `tg init` | Write `~/.tg/config.toml` (interactive or via `--token` / `--tmux-target` / `--force`) |
| `tg install` | Symlink binary into `~/.ir/tools/` if present, install + enable systemd user unit |
| `tg listen` | Inbound daemon (usually run via systemd, not directly) |
| `tg send` | Outbound: `--chat-id N --text "..."`, optional repeated `--file PATH`, `--format markdownv2`, `--reply-to MSGID` |
| `tg allow` | `--chat-id N [--label foo]` — append to allowlist |
| `tg deny` | `--chat-id N` — remove from allowlist |
| `tg list` | Print allowlist |
| `tg pair <code>` | Confirm a pending pairing |
| `tg pending` | List pending pairings (code, chat_id, label, expiry) |
| `tg reject` | `--chat-id N` — drop a pending pairing silently |

## Configuration

`~/.tg/config.toml` (mode `0600`):

```toml
bot_token = "..."
tmux_target = "root:1"

[[allow]]
chat_id = 8583339367
label = "virgil"
```

Runtime layout under `~/.tg/`:

```
~/.tg/
├── config.toml       # bot token, tmux target, [[allow]] entries
├── pending.json      # daemon-managed pending pairings
├── state             # last Telegram update offset
└── inbox/            # downloaded attachments
```

## Security model

- The bot token lives only in `config.toml`, mode `0600`, atomic-write at
  `0600` (the file never transiently exists at a wider mode).
- The daemon refuses to read `config.toml` or `pending.json` if mode is
  wider than `0600`.
- Unknown senders are silently dropped except for the pairing reminder
  (throttled to once per 30 seconds per chat).
- Pairing codes are 6 alphanumeric uppercase characters, expire in 1
  hour. The `tg pair` confirmation runs **locally** in your terminal —
  an attacker reading the code over Telegram cannot pair themselves.
- Attachment filenames are sanitized to `[A-Za-z0-9._-]` before being
  joined to the inbox path (no separator injection).
- Text injected into the tmux pane has C0 controls stripped and
  newline-runs collapsed before being typed.

## What this is NOT

- Not a Telegram client library — uses the Bot API only.
- Not a group-chat tool — DM-only for v1.
- Not multi-bot or multi-user.
- Not an MCP server — outbound is a regular CLI command. If you want
  agent-callable outbound, your agent can shell out to `tg send` (e.g.
  via Claude Code's `Bash` tool, or your own subprocess wrapper).

## Architecture

Single binary, subcommands (`git`-style). Synchronous HTTP via `ureq`,
no tokio: the listen daemon is a blocking long-poll loop; the outbound
CLI is one-shot. State persistence is atomic temp-file + rename.

See [`docs/design.md`](docs/design.md) for the full spec and
[`docs/plan.md`](docs/plan.md) for the task-by-task build plan that
produced the v0.1 implementation.

## Testing

```bash
cargo test            # 40 tests: 37 unit, 1 inbound integration, 2 outbound integration
```

Integration tests stub `api.telegram.org` with `tiny_http` and stub
tmux with a fake-shim shell script that records its argv. Unit tests
serialize their `TG_HOME` env-var mutation through a process-wide
mutex, so `cargo test` is safe at default parallelism.

See [`docs/smoke.md`](docs/smoke.md) for the end-to-end manual
verification checklist.

## License

MIT — see [LICENSE](LICENSE).
