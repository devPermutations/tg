# Installing `tg` on platforms other than Linux+systemd

The default install path (`tg install`) assumes Linux with systemd
user services. For macOS, containers, or systems without systemd,
follow one of the paths below.

## macOS — launchd

A launchd plist template lives at `launchd/com.devpermutations.tg.plist`.
After `cargo install --path .` and `tg init`:

1. Copy the template to `~/Library/LaunchAgents/com.devpermutations.tg.plist`.
2. Replace every occurrence of `REPLACE_ME` with your macOS username
   (e.g., `alice` → `/Users/alice/.cargo/bin/tg`).
3. Ensure `~/Library/Logs/` exists.
4. Load and start:
   ```bash
   launchctl load ~/Library/LaunchAgents/com.devpermutations.tg.plist
   launchctl start com.devpermutations.tg
   ```
5. Tail logs:
   ```bash
   tail -f ~/Library/Logs/tg-listen.log
   ```

Note: send-keys into tmux works the same on macOS. Make sure tmux is
available (`brew install tmux`) and a session matching your
`tmux_target` config is running.

## Docker / containers

A `Dockerfile` at the repo root builds a static x86_64-musl image
(~10 MB). Use cases:

- **Build verification.** `docker build -t tg .` confirms the build
  works in a hermetic environment.
- **`tg send`-only deployment.** If you only need outbound (no
  listen), the container is fine:
  ```bash
  docker run --rm -v "$HOME/.tg:/home/tg/.tg" tg send --chat-id N --text "hi"
  ```

`tg listen` in a container is **not recommended** for production —
send-keys into the host's tmux pane requires either `--network host`
plus a tmux socket bind-mount, or a more complex setup that defeats
the point of containerization. Run `tg listen` directly on the host.

## Other Linux init systems (OpenRC, runit, s6)

`tg listen` is a simple foreground process. Write a service file
that calls `~/.cargo/bin/tg listen` and let your init system handle
restart-on-crash. The daemon writes to stdout/stderr; route logs
however your init system expects.

For OpenRC:

```bash
# /etc/init.d/tg-listen
#!/sbin/openrc-run
name="tg-listen"
command="$HOME/.cargo/bin/tg"
command_args="listen"
command_user="$HOME_USER"
output_log="/var/log/tg-listen.log"
error_log="/var/log/tg-listen.log"
pidfile="/run/tg-listen.pid"
command_background="yes"

depend() {
    need net
}
```

## Without any supervisor

Just `tg listen` in a tmux pane or a `nohup`:

```bash
nohup ~/.cargo/bin/tg listen > ~/tg-listen.log 2>&1 &
disown
```

You're responsible for restarting it if it crashes. The 401-on-bad-token
exit is permanent; transient errors retry with exponential backoff.
