# tg

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)
[![Made with Rust](https://img.shields.io/badge/Made_with-Rust-orange.svg)](https://www.rust-lang.org/)

**Bridge a Telegram bot to your terminal.** The inbound daemon long-polls
Telegram and types each incoming message directly into a tmux pane via
`send-keys`. The outbound CLI sends text, photos, and documents to any
chat in your allowlist. A single static Rust binary plus a systemd user
unit.

Whatever's running in that pane sees the message as if you'd typed it
yourself — a shell, an agent, a TUI editor, a REPL. There's no
app-specific integration: if it reads stdin, it reads Telegram messages.

```text
[on your phone, you DM the bot]
   "deploy the staging service?"

[in your tmux pane]
   $ [telegram @alice (chat_id=1234567890)] deploy the staging service?
   $
```

## Table of contents

- [Why this exists](#why-this-exists)
- [Install](#install)
- [Usage](#usage)
- [Subcommands](#subcommands)
- [Configuration](#configuration)
- [Security model](#security-model)
- [Architecture](#architecture)
- [Testing](#testing)
- [Troubleshooting](#troubleshooting)
- [FAQ](#faq)
- [Roadmap](#roadmap)
- [Contributing](#contributing)
- [Acknowledgments](#acknowledgments)
- [License](#license)

## Why this exists

Phones are great for noticing things, terrible for typing into a
terminal. I wanted to glance at my phone, tap out a message, and have
it land in whatever I'd left running in a tmux pane — a shell, an
agent, a REPL, an editor — without that program needing to know
anything about Telegram.

The `tmux send-keys` mechanism is the smallest possible integration
surface: from the target app's perspective, the message just appears
as keyboard input. That keeps `tg` simple and makes it work
universally.

Point `tmux_target` at the pane you want messages delivered to and
that's it.

## Install

Prerequisites:

- Rust 1.75+ (just for the build)
- tmux 3.0+
- A Telegram bot token (from [@BotFather](https://t.me/BotFather))
- systemd (for the daemon supervisor; not required if you run `tg
  listen` manually)

```bash
git clone https://github.com/devPermutations/tg
cd tg
cargo install --path .

tg install   # symlinks ~/.cargo/bin/tg into ~/.ir/tools/ (if dir exists),
             # installs + enables ~/.config/systemd/user/tg-listen.service

tg init      # writes ~/.tg/config.toml at mode 0600;
             # prompts for token and tmux target (default: root:1);
             # pass --owner-chat-id N to lock inbound delivery to one chat
             # (everyone else added later becomes outbound-only)

systemctl --user start tg-listen
journalctl --user -u tg-listen -f          # tail logs
```

To allowlist someone, either:

- Have them DM the bot first. They'll get a pairing code on Telegram;
  you confirm with `tg pair XXXXXX` from your terminal.
- Or add them directly: `tg allow --chat-id 1234567890 --label alice`.

To find a chat_id without pairing: the bot can use any Telegram
debug-bot like `@RawDataBot` or check `tg listen`'s journal — every
unknown sender is logged.

## Usage

```text
$ tg send --chat-id 1234567890 --text "hello"
sent (id: 99)

$ tg send --chat-id 1234567890 --text "look at this" --file ~/screenshot.png
sent /home/me/screenshot.png (id: 100)

$ tg send --chat-id 1234567890 --text "*bold*" --format markdownv2
sent (id: 101)

$ tg send --chat-id 1234567890 --text "reply" --reply-to 99
sent (id: 102)

$ tg pending
9876543210	K7M3P2	bob	58m12s remaining

$ tg pair K7M3P2
paired chat_id 9876543210

$ tg list
1234567890	alice	(owner)
9876543210	bob
```

`alice` is the owner — her DMs deliver to the tmux pane. `bob` is in
the allowlist but is **outbound-only**: `tg send --chat-id 9876543210`
works for him, but if he DMs the bot his messages are silently dropped.

To designate or change the owner: `tg set-owner --chat-id N` (or `tg
set-owner --unset` to revert to "everyone in the allowlist delivers",
the v0.1 behavior).

Incoming text messages appear in your tmux pane as:

```
[telegram @alice (chat_id=1234567890)] hello there
```

Attachments are downloaded to `~/.tg/inbox/` and the path is appended
to the typed line. Supported media types: **photos, documents, voice
messages, audio files, stickers**.

```
[telegram @alice (chat_id=1234567890)] check this out [file: /home/me/.tg/inbox/1716852720-AgADBA...jpg]
```

When a message has media but no caption, a stand-in label appears:

```
[telegram @alice (chat_id=1234567890)] (voice 0:12) [file: /home/me/.tg/inbox/.../voice.ogg]
[telegram @alice (chat_id=1234567890)] (audio 3:42: Bohemian Rhapsody) [file: ...]
[telegram @alice (chat_id=1234567890)] (sticker 🎉) [file: ...]
```

Inbound video, animation (GIF), and video_note types are currently
unsupported — they render as `(unsupported media)` with no download.

### Voice transcription (optional)

If you set `whisper_url` in `config.toml` to a
[whisper.cpp HTTP server](https://github.com/ggerganov/whisper.cpp/tree/master/examples/server),
`tg listen` will transcribe inbound voice and audio attachments before
delivering the prompt line, appending `[transcript: ...]`:

```toml
whisper_url = "http://127.0.0.1:8178"
```

Pipeline: download the OGG/MP3 → `ffmpeg` to 16 kHz mono WAV → POST to
`{whisper_url}/inference` (multipart, `response_format=json`) → parse
`{"text":"..."}` → append to the typed line.

Quick container setup:

```bash
mkdir -p ~/whisper-models
wget -O ~/whisper-models/ggml-base.en.bin \
  https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.en.bin
docker run -d --name whisper-server --restart unless-stopped \
  -p 8178:8080 \
  -v ~/whisper-models:/models:ro \
  --entrypoint /app/build/bin/whisper-server \
  ghcr.io/ggml-org/whisper.cpp:main \
  --host 0.0.0.0 --port 8080 -m /models/ggml-base.en.bin
```

Requires `ffmpeg` on the host running `tg listen`. The 5 MB audio
size cap is hard-coded; oversized files skip transcription with a
warning log.

If transcription fails (whisper down, ffmpeg missing, network blip,
audio empty), the message still delivers — just with `[file: ...]`
and no `[transcript: ...]`.

## Subcommands

| Command | Purpose |
| --- | --- |
| `tg init` | Write `~/.tg/config.toml` (interactive, or via `--token` / `--tmux-target` / `--owner-chat-id` / `--force`) |
| `tg install` | Symlink binary into `~/.ir/tools/` if present, install + enable systemd user unit |
| `tg listen` | Inbound daemon (usually run via systemd, not directly) |
| `tg send` | Outbound: `--chat-id N --text "..."`, optional repeated `--file PATH`, `--format markdownv2`, `--reply-to MSGID` |
| `tg allow` | `--chat-id N [--label foo]` — append to allowlist |
| `tg deny` | `--chat-id N` — remove from allowlist |
| `tg list` | Print allowlist (owner-tagged) |
| `tg set-owner` | `--chat-id N` to designate the inbound-delivery owner; `--unset` to revert |
| `tg pair <code>` | Confirm a pending pairing |
| `tg pending` | List pending pairings (code, chat_id, label, expiry) |
| `tg reject` | `--chat-id N` — drop a pending pairing silently |

Every subcommand supports `--help`.

## Configuration

`~/.tg/config.toml` (mode `0600`):

```toml
bot_token = "..."
tmux_target = "root:1"

# Optional. If set, only this chat_id's DMs are typed into the tmux pane.
# Other [[allow]] entries become outbound-only (you can `tg send` them but
# their DMs are silently dropped). If omitted, every allowlisted sender
# delivers — pre-0.2 behavior.
owner_chat_id = 1234567890

[[allow]]
chat_id = 1234567890
label = "alice"

[[allow]]
chat_id = 9876543210
label = "bob"           # outbound-only because not the owner
```

Runtime layout under `~/.tg/`:

```
~/.tg/
├── config.toml       # bot token, tmux target, [[allow]] entries
├── pending.json      # daemon-managed pending pairings
├── state             # last Telegram update offset
└── inbox/            # downloaded attachments
```

There are no environment-variable overrides for `config.toml` location
in v0.1 except `TG_HOME` (intended for tests, but works at runtime if
you really want it elsewhere).

The systemd unit at `~/.config/systemd/user/tg-listen.service` runs `tg
listen` with `Restart=always, RestartSec=5`. Logs go to journald.

## Security model

- **Bot token confidentiality.** The token lives only in
  `config.toml`. Atomic-write at `0600` via `OpenOptions::mode()` + a
  sibling temp file + rename — the file never transiently exists at a
  wider mode, even on overwrite. The daemon **refuses to start** if
  `config.toml` or `pending.json` have any group/other bits set.
- **No silent leakage of bot existence.** With `owner_chat_id` set
  (the recommended deployment), unknown senders get **no reply at
  all** — their DMs are silently dropped, so the bot's existence is
  not advertised to anyone you haven't explicitly allowlisted. (In the
  legacy unowned mode, unknown senders get a throttled pairing
  reminder instead.)
- **Owner-only inbound delivery.** With `owner_chat_id` set, only the
  owner's DMs are typed into the tmux pane. Other allowlisted contacts
  can be `tg send`-ed to but their inbound DMs are silently dropped —
  the agent in the pane never sees them. This stops contact-list
  members from injecting prompts into whatever's running in the pane.
- **Pairing disabled when owner is set.** With an owner configured,
  the bot will not auto-create pending pairings for unknown senders.
  The owner is the only party authorized to grow the contact list,
  via `tg allow --chat-id N`.
- **Pairing code strength.** 6 alphanumeric uppercase characters
  (`[A-Z0-9]`), 36⁶ ≈ 2.2B possibilities, expires in 1 hour, rate-limited
  by the once-per-30s reminder cadence. The `tg pair` confirmation runs
  **locally on your machine** — an attacker reading the code over
  Telegram cannot pair themselves.
- **Filename sanitization on inbound attachments.** Original
  Telegram-supplied filenames are reduced to `[A-Za-z0-9._-]` before
  being joined to the inbox path. Path-separator injection is not
  possible.
- **Text sanitization before tmux delivery.** C0 control characters are
  stripped and runs of newlines are collapsed to a single space, so a
  message body can't escape its prompt line or inject escape sequences.
- **No outbound side-channel.** The daemon never reads or writes outside
  `~/.tg/`. The install command writes to `~/.config/systemd/user/` and
  optionally `~/.ir/tools/`, both with clear console output.

What's **not** in v0.1's threat model:

- An attacker with shell access on the same user account can read
  `config.toml`. Use proper file-system permissions and don't ship the
  config in a Docker layer.
- An attacker who controls the Telegram account that's already
  allowlisted can inject text into your tmux pane. The sanitization
  prevents escape sequences and multi-line injection, but the text
  itself is whatever the attacker types. If your tmux pane is running
  an agent that can take destructive actions, vet your allowlist.

## Architecture

Single binary, subcommands (`git`-style). Synchronous HTTP via `ureq`,
no tokio. The listen daemon is a blocking long-poll loop; the outbound
CLI is one-shot. State persistence is atomic temp-file + rename in
every location that holds bytes that matter.

```
┌────────────┐  long-poll   ┌────────┐
│ Telegram   │ ───────────▶ │ tg     │
│ Bot API    │              │ listen │
└────────────┘ ◀─────────── │        │ ──── tmux send-keys ──▶ ┌──────────┐
                  reply     └────────┘     stdin              │ tmux pane│
                                                              │ (shell / │
                                                              │ TUI /    │
                                                              │ agent)   │
                                                              └──────────┘
```

See [`docs/design.md`](docs/design.md) for the full spec and
[`docs/plan.md`](docs/plan.md) for the task-by-task build plan that
produced the v0.1 implementation. The plan is broken into 17 TDD tasks
that an unfamiliar engineer (or an agent) could execute end-to-end.

## Testing

```bash
cargo test            # 40 tests: 37 unit, 1 inbound integration, 2 outbound integration
```

Integration tests stub `api.telegram.org` with `tiny_http` and stub
tmux with a fake-shim shell script that records its argv. Unit tests
serialize their `TG_HOME` env-var mutation through a process-wide
mutex, so `cargo test` is safe at default parallelism.

[`docs/smoke.md`](docs/smoke.md) is an 11-step manual checklist that
verifies the binary against the real Telegram API + a real tmux. Run
it once after building before declaring a release usable.

## Troubleshooting

**`tg-listen` exits immediately with code 1.**
Check `journalctl --user -u tg-listen -n 20`. The most common cause is
a bad bot token (the daemon catches HTTP 401 and exits cleanly so
systemd surfaces it). Re-run `tg init --force` and re-paste the token,
double-checking for trailing whitespace.

**`config.toml mode is XXX; refusing to read (must be 0600)`.**
The file's permissions got widened, probably by a backup tool or an
editor. `chmod 600 ~/.tg/config.toml`.

**Bot replies "Pairing required" but I never get the message in my
pane.**
Either: you haven't run `tg pair XXXXXX` yet, or the pairing code is
expired (1-hour TTL — DM again to get a fresh one), or your chat_id
got allowlisted but the pane no longer matches `tmux_target`. Verify
with `tg list` and `tmux has-session -t <target>`.

**Two clients are polling the same bot.**
Telegram's Bot API rejects concurrent `getUpdates` long-polls — only
one client can hold the connection. If you have another tool (a
webhook, a previous bot framework, another `tg listen` instance) using
the same bot token, the second one will fail with HTTP 409. Either
stop the other client or use a different bot.

**Send fails with `Bad Request: chat not found`.**
The chat_id you passed isn't in the bot's known chats. Telegram only
exposes a chat_id to the bot after that chat has DM'd the bot at least
once (or you've been added to a group with the bot).

**`tg install` says `~/.ir/tools/tg exists and is not a symlink`.**
You have a different binary at that path. Resolve manually — either
`rm` it or move it out of the way; then re-run `tg install`.

**The systemd unit isn't auto-starting at boot.**
Lingering must be enabled for user services to start without a login:
`sudo loginctl enable-linger $USER`. Otherwise `tg-listen` only runs
during your active sessions.

## FAQ

**Why `tmux send-keys` rather than IPC / DBus / a tool-call protocol?**
Send-keys works on every TUI without modification. The agent / shell /
editor in the pane doesn't need to know `tg` exists — it just sees
"the user typed something." That's a vastly smaller integration
surface than wiring a tool-call protocol into every possible target.

**Does this work with group chats?**
Not in v0.1 — DM-only by design. Group support is on the roadmap.

**Why ureq and not reqwest?**
The daemon is a single blocking poll loop. ureq is sync, much smaller,
no tokio runtime, faster compile. There's no concurrent work to gain
from async here.

**Can I run two bots side-by-side?**
Not from one install of `tg`. Each `~/.tg/` is single-bot. You'd need
two separate `TG_HOME` directories and two systemd units. The code
supports it via the env var but the install command doesn't.

**Is this published on crates.io?**
Not currently — the name `tg` is already taken. If you want
`cargo install tg`-style installation, open an issue with a rename
proposal (`tgcli`, `tg-bot`, etc.) and we'll consider it.

## Roadmap

Possible v0.2 directions, none committed:

- Group chat support (different access model — per-group allowlist).
- Reactions and message edits (the Telegram Bot API supports both;
  surface as `tg react` and `tg edit-message`).
- Inbox cleanup / age-out policy for `~/.tg/inbox/`.
- Bracketed-paste mode for tmux send-keys so the target app can
  distinguish typed input from injected input.
- `tg send` reading text from stdin (`echo body | tg send --chat-id N`).
- Multi-bot support (`tg --home /alt/.tg listen`).

If you want any of these and would use them, open an issue describing
your use case.

## Contributing

Pull requests welcome. Before opening one:

- Read [`docs/design.md`](docs/design.md). It's the source of truth for
  what's in scope.
- Run `cargo test` and `cargo build --release` — both must pass.
- Stay within the v0.1 architectural boundaries (single binary, no
  tokio). New deps need a justification.
- Add a test for any new behavior — unit if pure, integration if it
  touches the binary as a process. Look at `tests/outbound.rs` for the
  pattern.

For larger changes: open an issue first to talk through the design.

The repo is small and well-commented; `git log --oneline` reads as a
TDD progression matching `docs/plan.md`.

## Acknowledgments

The access model — per-chat allowlist with a code-based pairing flow —
follows the shape used by the Telegram channel plugin in
[`anthropics/claude-plugins-official`](https://github.com/anthropics/claude-plugins-official),
which is where I first saw it applied to a single-user bot.

The `tmux send-keys`-as-input-channel pattern long predates this
project and has been used by many people for many purposes; this is
just one more application.

## License

MIT — see [LICENSE](LICENSE).
