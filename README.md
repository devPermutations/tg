# tg

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)
[![Made with Rust](https://img.shields.io/badge/Made_with-Rust-orange.svg)](https://www.rust-lang.org/)

**Bridge a Telegram bot to your terminal.** The inbound daemon long-polls
Telegram and types each incoming message directly into a tmux pane via
`send-keys`. The outbound CLI sends text, photos, documents, voice, and
audio to any chat in your allowlist. A single static Rust binary plus a
systemd user unit.

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

## Features

- **Inbound daemon** (`tg listen`) — long-polls Telegram, gates by an
  explicit per-chat allowlist, downloads attachments, types the
  message into a tmux pane.
- **Outbound CLI** (`tg send`) — text + photo + document + voice +
  audio, with optional MarkdownV2 and reply threading.
- **Allowlist with two trust tiers** — one designated *owner* whose
  DMs reach the pane, plus *outbound-only contacts* you can send to
  but whose inbound is silently dropped. Unknown senders are dropped
  with no Telegram reply at all (no leak of bot existence).
- **Pairing flow** — for the legacy unowned mode, unknown senders get
  a one-time code on Telegram and you confirm with `tg pair XXXXXX`
  locally; in owner mode the pairing flow is disabled and you grow
  the contact list yourself with `tg allow`.
- **Voice / audio transcription** (optional) — point at a
  [whisper.cpp](https://github.com/ggerganov/whisper.cpp/tree/master/examples/server)
  HTTP server and inbound `.oga`/`.mp3`/etc. get transcribed before
  the prompt line is typed.
- **Attachment downloads** — photos, documents, voice, audio, and
  stickers go to `~/.tg/inbox/`; the path is appended to the typed
  line so an agent in the pane can `Read` the file directly.
- **systemd user-service supervisor** — `tg install` ships the unit
  template; logs go to journald.
- **`ir` tool integration** — `tg install` symlinks the binary into
  `~/.ir/tools/` so [ir](https://github.com/) (and any other harness
  using that tool dir) can drive `tg send` directly.

## Table of contents

- [Why this exists](#why-this-exists)
- [Install](#install)
- [Usage](#usage)
- [Subcommands](#subcommands)
- [Access control & validation](#access-control--validation)
- [Security model](#security-model)
- [Configuration](#configuration)
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
# Option 1: from crates.io (once v0.7+ is published there)
cargo install tgcli

# Option 2: from this repo
git clone https://github.com/devPermutations/tg
cd tg
cargo install --path .
```

Either way the installed binary is `tg`. Continue:

```bash
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

## Access control & validation

There is no Telegram chat that can reach your tmux pane until you say
so. `tg` enforces this with two layers: an **allowlist** (who the bot
talks to at all) and an **owner designation** (which one of those
people can actually inject keystrokes into your pane).

### Trust tiers

| Sender | Inbound (DMs → tmux pane) | Outbound (`tg send` → recipient) |
|---|---|---|
| **Owner** (`owner_chat_id`) | ✅ typed into the pane | ✅ |
| **Allowlisted contact** (in `[[allow]]`, not owner) | ❌ silent drop | ✅ |
| **Unknown** (not in allowlist) | ❌ silent drop, **no Telegram reply** | ✅ — outbound is not gated |

Outbound is intentionally ungated: `tg send --chat-id N` works for any
chat_id the bot has ever seen, even one not in your config. That's the
point of an outbound CLI — you decide who to talk to.
Anyone with shell access to your host can `tg send --chat-id N` to any chat the bot has ever seen — outbound assumes the local account is trusted.

Inbound is the dangerous direction (someone else's text becoming your
typed input), and *that* is what the allowlist and owner gate protect.

### Adding a contact

Two paths to extend the allowlist:

**1. Direct admin add (recommended when an owner is set).** You know
the chat_id (got it from a debug bot like `@RawDataBot`, or out of
band) and you add them yourself:

```bash
tg allow --chat-id 9876543210 --label tim
```

They're now an outbound-only contact — you can `tg send` them, but if
they DM the bot their messages are silently dropped.

**2. Pairing flow (legacy unowned mode only).** With no owner set,
unknown senders DM'ing the bot get a one-time code on Telegram. You
confirm it locally:

```bash
$ tg pending
9876543210  K7M3P2  tim  58m12s remaining
$ tg pair K7M3P2
paired chat_id 9876543210
```

The code is 6 chars from `[A-Z0-9]` (≈ 2.2B combinations), expires
after 1 hour, and reminder DMs are throttled to once per 30 seconds
per sender. The `tg pair` confirmation runs **locally** — an attacker
who reads the reminder over Telegram can't pair themselves without
shell access to your host. When `owner_chat_id` is set, the pairing
flow is disabled entirely (unknown senders get the silent drop
instead).

### Promoting a contact to owner

The owner is the single chat whose DMs are typed into the pane:

```bash
tg set-owner --chat-id 1234567890   # make this chat the owner
tg set-owner --unset                 # revert to "everyone in allowlist delivers"
```

You can only have one owner at a time. To swap, just run `set-owner`
again — the old owner becomes a regular outbound-only contact.

### Listing and inspection

```bash
$ tg list
1234567890  alice  (owner)
9876543210  bob

$ tg pending
(no pending pairings)
```

The `(owner)` tag is the gate. Anything without it is outbound-only.

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

`tg` injects text from a remote source into a terminal. That is a
sensitive operation, and the security model is built around the idea
that **only the owner is trusted to drive the terminal**. Everyone
else is, at most, an outbound-only contact. This section describes
the layered defenses and the threat boundaries.

### Threat model

In scope (we defend against these):

- A random Telegram user discovering and DM'ing the bot, attempting
  to inject keystrokes into your pane or extract information about
  who's allowlisted.
- An outbound-only contact (someone you `tg send` to) attempting to
  reverse the channel by DM'ing the bot.
- A malicious message body containing escape sequences, newline
  injection, or control characters intended to break out of the
  prompt line.
- A malicious attachment filename containing path separators or
  parent-directory references intended to write outside `~/.tg/inbox/`.
- A misconfigured filesystem permission widening the bot token to
  group/world readers.
- A crash during a config or state write leaving the file partially
  written or in an inconsistent state.

Out of scope (you handle these yourself):

- An attacker who already has shell access on the same user account
  reading `config.toml`. Use proper Unix permissions; don't bake the
  config into a container layer; don't share the host account.
- A compromised owner Telegram account. If the attacker controls the
  owner's chat, they can type anything into your pane. Vet who has
  access to your phone, use Telegram's 2FA, etc.
- A backdoored whisper.cpp build returning poisoned transcripts. If
  you're worried about your transcription backend, audit it or run a
  vetted build.
- Network-layer attacks on the Telegram Bot API. We rely on TLS
  to api.telegram.org; that's standard and outside this project's
  scope.

### Defenses

**1. Allowlist & validation (the primary gate).**
Inbound is gated by an explicit per-chat allowlist. Unknown senders
get no Telegram reply at all when an owner is configured — the bot
does not advertise its existence. See
[Access control & validation](#access-control--validation) above for
the full mechanism. Key properties:

- The allowlist is human-readable TOML; you can audit it with `cat`
  or `tg list`.
- Adding a contact requires either a `tg allow` from the host shell
  or, in legacy unowned mode, a `tg pair <code>` confirmation from
  the host shell. **No path lets a remote sender add themselves.**
- Pairing codes are 6 chars from `[A-Z0-9]` (≈ 2.2B combinations),
  expire in 1 hour, with reminder DMs rate-limited to once per 30
  seconds per chat. Brute force is infeasible.
- Owner mode (`owner_chat_id` set) disables the pairing flow
  entirely — only direct admin adds work.

**2. Owner gating (the second gate).**
Allowlisting a chat means you can `tg send` to them. It does *not*
mean their DMs reach your pane. Only the **owner**'s DMs are typed
into tmux. Other allowlisted contacts' DMs are silently dropped at
the daemon. This is what makes it safe to have contacts in the
allowlist without exposing your terminal to all of them.

**3. Token confidentiality.**
The bot token lives only in `~/.tg/config.toml` at mode `0600`. The
save path is an atomic write — open at `0600` via
`OpenOptions::mode()`, write, `flush + fsync`, rename over the
destination. The file never transiently exists at a wider mode, even
on overwrite. The daemon **refuses to start** if `config.toml` or
`pending.json` are wider than `0600`.

**4. Inbound-text sanitization.**
Before any message body is handed to `tmux send-keys`, runs of
newlines are collapsed to a single space and all C0 control
characters are stripped. The text is sent literally via
`send-keys -l` (no key-name interpretation), and the Enter key is a
separate `send-keys Enter` invocation. A message body cannot escape
its prompt line, cannot inject ANSI escape sequences, and cannot
inject extra key events.

**5. Attachment filename sanitization.**
Telegram-supplied filenames are reduced to `[A-Za-z0-9._-]` before
being joined to the inbox path. The output filename always has a
`{unix_ts}-` prefix, so even a maximally weird stem cannot produce
a path beginning with `/`, `..`, or similar — path-separator
injection is not possible. The 5 MB cap on audio for transcription
limits resource consumption.

**6. Crash safety.**
All file writes (`config.toml`, `pending.json`, the offset state
file) go through a temp-file + rename pattern with `flush + sync_all`
before the rename. A crash during a write leaves the previous file
intact; you never end up with a half-written allowlist.

**7. No outbound side-channels.**
The listen daemon never reads or writes outside `~/.tg/`. `tg
install` writes to `~/.config/systemd/user/` and (if `~/.ir/tools/`
exists) symlinks into it, with clear console output identifying each
mutation. The daemon makes outbound HTTP only to the Telegram Bot
API and the configured `whisper_url`.

### What we don't claim

- This isn't a replacement for a hardened bastion or a real audit
  trail. A motivated attacker who compromises your account can drive
  your terminal — the design assumes the owner's Telegram account is
  not hostile.
- We don't sandbox the typed-into pane. Whatever the agent in that
  pane can do, the owner of the Telegram account can also cause it
  to do. Match the pane's permissions to the owner's trust level.
- We don't perform syntactic validation on the content of a typed
  line beyond control-char stripping. If you're worried about an
  attacker sending "rm -rf /" as a message, don't put a root shell
  in the pane; put an agent in there that can ask for confirmation.

### Reporting a security issue

See [`SECURITY.md`](SECURITY.md) for the full disclosure policy,
supported-versions table, response-time expectations, and the list
of past advisories.

Quick summary: use [GitHub's private security
advisories](https://github.com/devPermutations/tg/security/advisories/new)
to report — not a public issue. Don't include real bot tokens or
chat_ids in any report; use placeholders.

### Audit process

The codebase is small (≈ 1500 LOC of Rust, no `unsafe`) and the
threat model + defenses above are intended to be fully auditable
in one sitting. The recommended pre-release checklist:

1. Run `/security-audit` (Claude Code's `security-guidance` plugin)
   on the working tree before tagging a release. The audit covers
   token leakage, path traversal, command injection, gate-logic
   bypasses, TLS, deserialization, multipart boundary collisions,
   and crash safety.
2. Triage findings against the threat model in this section.
3. Fix HIGH and MEDIUM findings before release; LOW findings can
   land in the next minor.

This is how the v0.5.1 token-leak vulnerability was found (audit
finding → fixed in v0.5.2). The full audit-finding report for that
issue is preserved in the v0.5.2 release notes and in
[GHSA-5pvm-3m24-8p3f](https://github.com/devPermutations/tg/security/advisories/GHSA-5pvm-3m24-8p3f).

If you're evaluating `tg` for production-like use, run your own
audit too — the small code surface makes that practical.

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
cargo test            # runs the full suite (unit + integration)
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

The crate is published as `tgcli` (the `tg` name was taken). The
binary it installs is still `tg`, so usage is unchanged — only the
install command differs:

```bash
cargo install tgcli
```

If you're building from source, `cargo install --path .` from a repo
clone also works.

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
