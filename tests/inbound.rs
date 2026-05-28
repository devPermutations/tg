//! End-to-end test for `tg listen` against a mock telegram and a fake
//! tmux shim that records its invocations.

use std::process::{Command, Stdio};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

fn binary() -> std::path::PathBuf {
    // CARGO_BIN_EXE_<name> is injected by cargo for integration tests (since Rust 1.43).
    // However, cargo 1.93 (host) has a bug where it silently omits the injection; the fix
    // landed sometime between 1.93 and 1.95 (confirmed: 1.95 injects it correctly).
    // The fallback below keeps tests green on the host until the toolchain is updated.
    // NOTE: the fallback hardcodes `debug`; running `cargo test --release` on the host
    // will break if the binary is only built in release. Upgrade to >=1.95 when possible.
    if let Ok(exe) = std::env::var("CARGO_BIN_EXE_tg") {
        return std::path::PathBuf::from(exe);
    }
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR")
        .expect("CARGO_MANIFEST_DIR not set");
    // Integration tests run against the same profile as the test itself.
    // The test binary lives in target/debug/deps; the app binary in target/debug/.
    let mut p = std::path::PathBuf::from(&manifest_dir);
    p.push("target");
    p.push("debug");
    p.push("tg");
    p
}

/// Writes a shell script to `path` that records its argv to
/// `record_path` (one line per invocation, args joined with `\t`) and
/// always exits 0.
fn write_fake_tmux(path: &std::path::Path, record_path: &std::path::Path) {
    let script = format!(
        "#!/bin/bash\n\
         printf '%s\\t' \"$@\" >> {}\n\
         printf '\\n' >> {}\n\
         # has-session pretends pane exists.\n\
         exit 0\n",
        record_path.display(), record_path.display());
    std::fs::write(path, script).unwrap();
    use std::os::unix::fs::PermissionsExt;
    let mut perms = std::fs::metadata(path).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(path, perms).unwrap();
}

fn write_config(home: &std::path::Path, token: &str, allow_chat: i64) {
    let cfg = format!(
        "bot_token = \"{token}\"\n\
         tmux_target = \"test:0\"\n\
         \n\
         [[allow]]\n\
         chat_id = {allow_chat}\n\
         label = \"alice\"\n");
    let path = home.join("config.toml");
    std::fs::create_dir_all(home).unwrap();
    std::fs::write(&path, cfg).unwrap();
    use std::os::unix::fs::PermissionsExt;
    let mut perms = std::fs::metadata(&path).unwrap().permissions();
    perms.set_mode(0o600);
    std::fs::set_permissions(&path, perms).unwrap();
}

#[test]
fn allowed_text_message_is_send_keysed_to_pane() {
    let home = tempfile::tempdir().unwrap();
    let shim_dir = tempfile::tempdir().unwrap();
    let shim = shim_dir.path().join("fake-tmux");
    let record = shim_dir.path().join("argv.log");
    write_fake_tmux(&shim, &record);
    write_config(home.path(), "TOKEN", 100);

    let server = Arc::new(tiny_http::Server::http("127.0.0.1:0").unwrap());
    let port = server.server_addr().to_ip().unwrap().port();
    let s2 = server.clone();

    // Serve one getUpdates with a single message, then one empty, then
    // exit the test by killing tg listen.
    let mock_join = thread::spawn(move || {
        let mut served = 0u32;
        while let Ok(req) = s2.recv() {
            served += 1;
            let url = req.url().to_string();
            assert!(url.contains("/getUpdates"), "url: {url}");
            let body = if served == 1 {
                r#"{"ok":true,"result":[{"update_id":1,"message":{"message_id":7,"from":{"id":100,"username":"alice"},"chat":{"id":100,"type":"private"},"text":"hi there"}}]}"#
            } else {
                r#"{"ok":true,"result":[]}"#
            };
            req.respond(tiny_http::Response::from_string(body)
                .with_header("Content-Type: application/json".parse::<tiny_http::Header>().unwrap()))
                .unwrap();
            if served >= 2 { return; }
        }
    });

    let mut child = Command::new(binary())
        .args([
            "--api-base", &format!("http://127.0.0.1:{port}"),
            "--tmux-bin", shim.to_str().unwrap(),
            "listen",
        ])
        .env("TG_HOME", home.path())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();

    // Give listen time to handle one update + one empty poll.
    thread::sleep(Duration::from_secs(3));
    let _ = child.kill();
    let _ = child.wait();
    let _ = mock_join.join();

    let log = std::fs::read_to_string(&record).unwrap();
    // Expect at least: has-session, send-keys -l "[telegram @alice ...]", send-keys Enter.
    assert!(log.contains("has-session"), "no has-session in log: {log}");
    assert!(log.contains("send-keys"), "no send-keys in log: {log}");
    assert!(log.contains("@alice"), "no @alice in log: {log}");
    assert!(log.contains("hi there"), "no body in log: {log}");
    assert!(log.contains("Enter"), "no Enter in log: {log}");
}

#[test]
fn agent_offline_reply_when_pane_down() {
    let home = tempfile::tempdir().unwrap();
    let shim_dir = tempfile::tempdir().unwrap();
    let shim = shim_dir.path().join("fake-tmux-dead");
    let record = shim_dir.path().join("argv.log");
    write_fake_tmux_dead(&shim, &record);
    write_config(home.path(), "TOKEN", 100);
    // Force allow+owner alignment for v0.6 invariant
    set_owner_in_config(home.path(), 100);

    let server = Arc::new(tiny_http::Server::http("127.0.0.1:0").unwrap());
    let port = server.server_addr().to_ip().unwrap().port();
    let s2 = server.clone();

    let mock_join = thread::spawn(move || {
        let mut got_offline_reply = false;
        let mut served = 0u32;
        while let Ok(mut req) = s2.recv() {
            served += 1;
            let url = req.url().to_string();

            if url.contains("/getUpdates") {
                let body = if served == 1 {
                    r#"{"ok":true,"result":[{"update_id":1,"message":{"message_id":7,"from":{"id":100,"username":"alice"},"chat":{"id":100,"type":"private"},"text":"are you there?"}}]}"#
                } else {
                    r#"{"ok":true,"result":[]}"#
                };
                req.respond(tiny_http::Response::from_string(body)
                    .with_header("Content-Type: application/json".parse::<tiny_http::Header>().unwrap()))
                    .unwrap();
            } else if url.contains("/sendMessage") {
                // This is the offline reply. Capture body to verify.
                let mut body = Vec::new();
                req.as_reader().read_to_end(&mut body).unwrap();
                let s = String::from_utf8_lossy(&body);
                if s.contains("agent+offline") || s.contains("agent offline") {
                    if s.contains("test") {
                        got_offline_reply = true;
                    }
                }
                req.respond(tiny_http::Response::from_string(
                    r#"{"ok":true,"result":{"message_id":99,"chat":{"id":100,"type":"private"}}}"#
                ).with_header("Content-Type: application/json".parse::<tiny_http::Header>().unwrap()))
                .unwrap();
            } else {
                // Unknown endpoint — close it
                req.respond(tiny_http::Response::from_string("?").with_status_code(404)).unwrap();
            }

            if served >= 4 { return got_offline_reply; }
        }
        got_offline_reply
    });

    let mut child = Command::new(binary())
        .args([
            "--api-base", &format!("http://127.0.0.1:{port}"),
            "--tmux-bin", shim.to_str().unwrap(),
            "listen",
        ])
        .env("TG_HOME", home.path())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();

    thread::sleep(Duration::from_secs(5));
    let _ = child.kill();
    let _ = child.wait();
    let got_offline_reply = mock_join.join().unwrap();

    assert!(got_offline_reply, "expected the daemon to send an 'agent offline' reply via Telegram");

    let log = std::fs::read_to_string(&record).unwrap();
    // The shim should have been called with has-session but NOT with send-keys.
    assert!(log.contains("has-session"), "expected has-session probe; got: {log}");
    assert!(!log.contains("send-keys"), "expected NO send-keys call (pane is dead); got: {log}");
}

#[test]
fn unknown_sender_silent_drop_when_owner_set() {
    let home = tempfile::tempdir().unwrap();
    let shim_dir = tempfile::tempdir().unwrap();
    let shim = shim_dir.path().join("fake-tmux");
    let record = shim_dir.path().join("argv.log");
    write_fake_tmux(&shim, &record);
    write_config(home.path(), "TOKEN", 100);
    set_owner_in_config(home.path(), 100);
    // 100 is owner+allowed; 999 is NOT in allowlist and NOT owner.

    let server = Arc::new(tiny_http::Server::http("127.0.0.1:0").unwrap());
    let port = server.server_addr().to_ip().unwrap().port();
    let s2 = server.clone();

    let mock_join = thread::spawn(move || {
        let mut any_send_message = false;
        let mut served = 0u32;
        while let Ok(mut req) = s2.recv() {
            served += 1;
            let url = req.url().to_string();

            if url.contains("/getUpdates") {
                // Serve one inbound from chat_id=999 (unknown), then empties.
                let body = if served == 1 {
                    r#"{"ok":true,"result":[{"update_id":1,"message":{"message_id":7,"from":{"id":999,"username":"attacker"},"chat":{"id":999,"type":"private"},"text":"hello bot"}}]}"#
                } else {
                    r#"{"ok":true,"result":[]}"#
                };
                req.respond(tiny_http::Response::from_string(body)
                    .with_header("Content-Type: application/json".parse::<tiny_http::Header>().unwrap()))
                    .unwrap();
            } else if url.contains("/sendMessage") {
                // We expect this to NEVER happen for an unknown sender in owner mode.
                any_send_message = true;
                let mut buf = Vec::new();
                req.as_reader().read_to_end(&mut buf).unwrap();
                req.respond(tiny_http::Response::from_string(r#"{"ok":true}"#)).unwrap();
            } else {
                req.respond(tiny_http::Response::from_string("?").with_status_code(404)).unwrap();
            }

            if served >= 3 { return any_send_message; }
        }
        any_send_message
    });

    let mut child = Command::new(binary())
        .args([
            "--api-base", &format!("http://127.0.0.1:{port}"),
            "--tmux-bin", shim.to_str().unwrap(),
            "listen",
        ])
        .env("TG_HOME", home.path())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();

    thread::sleep(Duration::from_secs(4));
    let _ = child.kill();
    let _ = child.wait();
    let any_send_message = mock_join.join().unwrap();

    // The bot must NEVER acknowledge unknown senders when owner is set.
    assert!(!any_send_message, "expected NO sendMessage call for unknown sender in owner mode");

    // No tmux send-keys either — the gate fires before tmux is consulted.
    // The log file may not exist at all if the shim was never called; that's fine.
    let log = std::fs::read_to_string(&record).unwrap_or_default();
    assert!(!log.contains("send-keys"), "expected NO send-keys for unknown sender; got: {log}");
}

/// Shim variant that fails on `has-session`, simulating a dead pane.
fn write_fake_tmux_dead(path: &std::path::Path, record_path: &std::path::Path) {
    let script = format!(
        "#!/bin/bash\n\
         printf '%s\\t' \"$@\" >> {rec}\n\
         printf '\\n' >> {rec}\n\
         if [ \"$1\" = \"has-session\" ]; then exit 1; fi\n\
         exit 0\n",
        rec = record_path.display());
    std::fs::write(path, script).unwrap();
    use std::os::unix::fs::PermissionsExt;
    let mut perms = std::fs::metadata(path).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(path, perms).unwrap();
}

/// Appends `owner_chat_id` to the seeded config so the v0.6 invariant
/// (owner ⊆ allowlist) holds. `write_config` already adds an `[[allow]]`
/// entry for `allow_chat`; we just add the `owner_chat_id` line.
fn set_owner_in_config(home: &std::path::Path, chat_id: i64) {
    let path = home.join("config.toml");
    let current = std::fs::read_to_string(&path).unwrap();
    // Insert owner_chat_id right after tmux_target.
    let new = current.replace(
        "tmux_target = \"test:0\"\n",
        &format!("tmux_target = \"test:0\"\nowner_chat_id = {chat_id}\n"),
    );
    std::fs::write(&path, new).unwrap();
    use std::os::unix::fs::PermissionsExt;
    let mut perms = std::fs::metadata(&path).unwrap().permissions();
    perms.set_mode(0o600);
    std::fs::set_permissions(&path, perms).unwrap();
}
