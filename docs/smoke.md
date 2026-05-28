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

### Step 12: SIGHUP-based config reload

1. While the daemon is running (`systemctl --user is-active tg-listen` →
   `active`), edit the allowlist via the CLI:
   ```
   tg allow --chat-id 1234567890 --label test-add
   ```
   The daemon doesn't see it yet — the config is on disk but the running
   process still has the old in-memory copy.

2. Send the daemon a SIGHUP:
   ```
   systemctl --user kill --signal=HUP tg-listen
   ```
   (Alternatively: `kill -HUP $(pgrep -u $USER -f "tg listen")`.)

3. Watch the journal:
   ```
   journalctl --user -u tg-listen -f
   ```
   Within ~30 seconds (one long-poll window), expect:
   ```
   INFO tg::listen: SIGHUP: reloaded config (allowlist=N, owner_chat_id=Some(M), token_changed=false)
   ```

4. Have the newly-added chat DM the bot. The message should now
   reach your pane (or get the outbound-only silent-drop, depending
   on whether they're the owner). Pre-SIGHUP they would have been
   rejected.

5. Optional: rotate the bot token via @BotFather, `tg init --force
   --token <NEW>`, then SIGHUP. The reload log should show
   `token_changed=true` and the daemon should keep polling under
   the new token without restart.
