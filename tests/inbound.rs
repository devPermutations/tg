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
