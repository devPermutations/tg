# `tg` — design

**Date:** 2026-05-28
**Status:** approved (brainstorming) — awaiting implementation plan

## Purpose

A self-contained Rust CLI that owns a Telegram bot endpoint for one user. Two modes:

- **Inbound daemon** — long-poll Telegram, gate by allowlist, deliver messages
  into a tmux pane via `send-keys` so Claude Code (or any TUI in that pane)
  receives them as typed input.
- **Outbound CLI** — `tg send` posts text and attachments back to a chat. Usable
  from a shell, from `ir run tg send …`, or from any script.

Replaces the patched Bun/TS Telegram MCP plugin (which never reliably delivered
its `notifications/claude/channel` block to Claude Code's context). `tg` owns
its config, its state, and its supervisor unit — it does not read or write any
file outside `~/.tg/` and `~/.config/systemd/user/`.

## Non-goals (v1)

- Reactions, `edit_message`, voice/video/sticker handling — deferrable.
- Group chats — DM-only for v1.
- MCP protocol — not implemented.
- Multi-user / multi-bot — single bot, single tmux target, single allowlist.

## Architecture

Single binary, subcommands (`git`-style):

```
tg init        — write ~/.tg/config.toml
tg install     — symlink into ~/.ir/tools/, install + enable systemd unit
tg listen      — daemon: poll Telegram, gate, send-keys, attachments
tg send        — POST sendMessage / sendPhoto / sendDocument
tg allow       — append to [[allow]]
tg deny        — remove from [[allow]]
tg list        — print allowlist
tg pair <code> — confirm a pending pairing
tg pending     — list pending pairings
tg reject <id> — drop a pending pairing silently
```

Project layout:

```
~/projects/tg/
├── Cargo.toml
├── src/
│   ├── main.rs       # clap, dispatch
│   ├── config.rs     # ~/.tg/config.toml read/write
│   ├── pending.rs    # ~/.tg/pending.json read/write
│   ├── api.rs        # Telegram Bot API client (ureq, sync)
│   ├── listen.rs     # poll → gate → send-keys, attachment download
│   ├── send.rs       # outbound: text + multipart upload
│   ├── access.rs     # allow / deny / pair / pending / reject
│   ├── install.rs    # symlink + systemd unit copy + enable
│   └── tmux.rs       # send-keys wrapper (spawnSync for ordering)
├── systemd/tg-listen.service
└── docs/design.md
```

Dependencies (rough): `clap` (derive), `serde` + `serde_json`, `toml`, `ureq`
(rustls, gzip), `anyhow`, `tracing` + `tracing-subscriber`, `rand` (pairing
codes), `chrono` (timestamps). No `tokio` — listen is a blocking long-poll;
outbound is one-shot.

Runtime layout:

```
~/.tg/
├── config.toml       # bot_token, tmux_target, [[allow]]   (mode 0600)
├── pending.json      # daemon-managed pending pairings     (mode 0600)
├── state             # last update offset                  (mode 0600)
└── inbox/            # downloaded attachments
```

## Inbound flow — `tg listen`

1. Load config; read `state` for last `update_id` (default 0).
2. Blocking `GET /bot{token}/getUpdates?offset=N&timeout=30`.
3. For each update:
   a. Resolve `chat_id` and sender info.
   b. **Gate:**
      - In `[[allow]]` → deliver.
      - In `pending.json`, non-expired → re-send the "Still pending"
        reminder (throttled per chat_id, max one reminder per 30s). Drop
        the message. Move on.
      - In `pending.json`, expired → generate new code, replace entry,
        send fresh "Pairing required" reply. Drop.
      - Not in either → generate code (6 chars `[A-Z0-9]`, 1-hour
        expiry), write to `pending.json`, send "Pairing required — run
        in your terminal: `tg pair XXXXXX`". Drop.
   c. **If tmux target missing** (`tmux has-session -t <target>` fails) →
      `sendMessage` "agent offline (Claude Code not running)" to the
      sender; log; advance offset.
   d. **Deliver text:** `tmux send-keys -t <target> -l "[telegram @user
      (chat_id=N)] <cleaned-text>"` then `tmux send-keys -t <target>
      Enter`. Two separate `Command::status()` calls for ordering.
   e. **Deliver attachment:** call `getFile`, download from
      `https://api.telegram.org/file/bot{token}/{file_path}` into
      `~/.tg/inbox/<unix_ts>-<file_unique_id>.<ext>`. Append `[file:
      <abs_path>]` to the typed prompt so the agent can `Read` it.
4. Persist new offset to `state` via temp-file + rename.
5. Loop.

**Error handling.** Poll errors → exponential backoff (1s, 2s, 4s, … capped at
60s), tracing-warn each retry. 401 (invalid token) → exit 1, systemd restarts.
Tmux failures → log and advance offset (do not stall the poll loop on one bad
delivery).

**Text sanitization** before send-keys: replace `[\r\n]+` with single space,
strip `[\x00-\x1f\x7f]`. The `-l` flag means tmux sends characters literally
(no key-name interpretation), so the trailing `Enter` is the only thing that
submits.

## Outbound flow — `tg send`

```
tg send --chat-id 8583339367 --text "hello"
tg send --chat-id 8583339367 --text "look at this" --file /tmp/shot.png
tg send --chat-id 8583339367 --text "report" --file q3.pdf --file q4.pdf
tg send --chat-id 8583339367 --text "*bold*" --format markdownv2
tg send --chat-id 8583339367 --text "reply" --reply-to 535
```

Dispatch:
- No `--file` → `POST sendMessage` (form-encoded).
- One or more `--file` → for each file, sniff mime/extension: `image/*` →
  `sendPhoto` (inline preview), otherwise → `sendDocument`. First file
  carries `--text` as `caption`; subsequent files send no caption.
- `--format markdownv2` sets `parse_mode=MarkdownV2`. Caller is responsible
  for escaping special chars per Telegram's MarkdownV2 rules.
- `--reply-to <message_id>` sets `reply_parameters` for threading.

Exit codes: 0 on success; non-zero with the API's `description` field
written to stderr on failure. Each `--file` is a separate API call; partial
failures print which file failed and continue.

Hidden `--api-base <url>` flag (default `https://api.telegram.org`) lets
integration tests point `tg send` at a local mock.

## Config & access control

`~/.tg/config.toml`:

```toml
bot_token = "..."
tmux_target = "root:1"

[[allow]]
chat_id = 8583339367
label = "virgil"

[[allow]]
chat_id = 8598991658
label = "tim"
```

- File mode `0o600` (token-bearing). `tg` refuses to start if mode is wider.
- `tg init` is interactive: prompts for bot token (or `--token`) and
  tmux target (default `root:1`), writes the file. Allowlist starts empty.
- `tg allow --chat-id N [--label foo]` — append; refuse duplicate chat_id.
- `tg deny --chat-id N` — remove.
- `tg list` — print.

## Pairing flow

`~/.tg/pending.json` (daemon-managed, mode 0o600):

```json
{
  "8598991658": {
    "code": "K7M3P2",
    "username": "tim",
    "first_seen_at": "2026-05-28T00:42:11Z",
    "expires_at": "2026-05-28T01:42:11Z",
    "last_reminder_at": "2026-05-28T00:42:11Z"
  }
}
```

**Listen side** (already covered in the Inbound section). On unknown
sender: generate `[A-Z0-9]{6}` code, 1-hour expiry, throttle reminders to
one per 30s.

**CLI side:**
- `tg pair <code>` — find the matching entry (case-insensitive), validate
  not expired, then perform a two-step write: (1) append to
  `config.toml [[allow]]` (carry username → label), (2) remove the entry
  from `pending.json`. Cross-file atomicity isn't possible without a
  transaction layer; the recovery property is idempotency — step 1
  refuses duplicate chat_ids, so a crash between steps just leaves a
  stale pending entry which the next `tg pair` (or any allow-check)
  cleans up. After both writes succeed, `sendMessage` "Paired" to the
  chat. Errors: `unknown code`, `expired`, `already paired`.
- `tg pending` — print pending entries (code, chat_id, username, time
  remaining).
- `tg reject <chat_id>` — remove the pending entry silently (no Telegram
  reply); the sender will get a fresh "Pairing required" on their next DM.

**Security:**
- 6 chars × 36 alphabet = 2.2B possibilities, 1-hour window → infeasible
  to brute force.
- `tg pair` runs **locally** (not over Telegram). An attacker reading the
  reminder still needs shell access to confirm.
- All token-bearing files are mode `0o600`; `tg` refuses to start if any
  is wider.

## Install & supervisor

`systemd/tg-listen.service`:

```ini
[Unit]
Description=Telegram channel listener
After=network-online.target

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

Logs land in `journalctl --user -u tg-listen`. `tracing` writes plain
formatted lines to stderr; journald captures them.

**`tg install` (idempotent):**
1. If `~/.ir/tools/` exists, ensure `~/.ir/tools/tg` is a symlink pointing
   at `~/.cargo/bin/tg`. Cases: missing → create; correct symlink →
   skip; symlink pointing elsewhere or regular file → refuse with a
   clear message ("`~/.ir/tools/tg` exists and is not ours; resolve
   manually").
2. Copy `tg-listen.service` to `~/.config/systemd/user/` if the
   destination is missing or its contents differ.
3. `systemctl --user daemon-reload`.
4. `systemctl --user enable tg-listen` (does not start).
5. Print: "run `tg init`, then `systemctl --user start tg-listen`."

Bootstrap:

```
cargo install --path ~/projects/tg
tg install
tg init                       # writes config.toml
tg allow --chat-id 8583339367 --label virgil
systemctl --user start tg-listen
journalctl --user -u tg-listen -f
```

## Testing

**Unit tests** (`cargo test`):
- `config.rs`: load + save round-trips; mode-check refusal.
- `pending.rs`: add / remove / expire transitions; throttling clock.
- `access.rs`: allow/deny idempotency; pair success + failure modes.
- `tmux.rs`: argv construction for `send-keys -l` + `Enter`; text
  sanitization (newlines → space; C0 controls stripped).
- `send.rs`: multipart body shape per `--file`; mime dispatch
  (image/* → sendPhoto; else sendDocument).

**Integration tests:**
- *Outbound*: `tiny_http` mock listens on a random port, exposes
  `getMe`/`sendMessage`/`sendPhoto`/`sendDocument`. `tg send --api-base
  http://127.0.0.1:PORT …` is invoked; mock asserts the request shape.
- *Inbound*: same mock serves canned `getUpdates` responses. `tg listen
  --api-base … --tmux-bin /tmp/fake-tmux` runs against a recording shim;
  assert the shim received the right argv sequences.

**Manual smoke checklist** (in `docs/smoke.md`):
1. `tg init` and supply a fresh bot token.
2. `tg install && systemctl --user start tg-listen`.
3. DM the bot from a non-allowed account → expect "Pairing required" on
   the phone; `tg pending` shows the entry.
4. `tg pair <code>` → expect "Paired" on the phone, `tg list` shows the
   chat_id.
5. DM the bot again → message appears in `root:1` as `[telegram @user
   (chat_id=N)] …`, Enter fires the turn.
6. From the agent: `tg send --chat-id N --text "ack"` → phone receives.
7. `tg send --chat-id N --file ~/some.png` → photo arrives inline.
8. Stop Claude Code; DM the bot → expect "agent offline" reply.

## Open questions

None at v1. Reactions, edit_message, groups, multi-bot, attachment
metadata richer than path-string — all deferred to v2.
