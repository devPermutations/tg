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
