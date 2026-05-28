//! End-to-end test for `tg send` against a local tiny_http mock.
//!
//! Builds the binary via cargo, sets up a tempdir TG_HOME with a
//! config.toml, points `--api-base` at the mock, and asserts the request
//! shape the mock saw.

use std::process::Command;
use std::sync::Arc;
use std::thread;

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

fn write_config(home: &std::path::Path, token: &str) {
    let cfg = format!("bot_token = \"{token}\"\ntmux_target = \"none:0\"\n");
    let path = home.join("config.toml");
    std::fs::create_dir_all(home).unwrap();
    std::fs::write(&path, cfg).unwrap();
    let mut perms = std::fs::metadata(&path).unwrap().permissions();
    use std::os::unix::fs::PermissionsExt;
    perms.set_mode(0o600);
    std::fs::set_permissions(&path, perms).unwrap();
}

#[test]
fn send_text_hits_send_message_with_expected_body() {
    let home = tempfile::tempdir().unwrap();
    write_config(home.path(), "TOKEN");

    let server = Arc::new(tiny_http::Server::http("127.0.0.1:0").unwrap());
    let port = server.server_addr().to_ip().unwrap().port();
    let s2 = server.clone();

    let join = thread::spawn(move || {
        let mut req = s2.recv().unwrap();
        assert!(req.url().contains("/botTOKEN/sendMessage"), "got {}", req.url());
        let mut body = Vec::new();
        req.as_reader().read_to_end(&mut body).unwrap();
        let s = String::from_utf8_lossy(&body);
        assert!(s.contains("\"chat_id\":42"), "got body: {s}");
        assert!(s.contains("\"text\":\"hello\""), "got body: {s}");

        let resp = r#"{"ok":true,"result":{"message_id":99,"chat":{"id":42,"type":"private"}}}"#;
        req.respond(tiny_http::Response::from_string(resp)
            .with_header("Content-Type: application/json".parse::<tiny_http::Header>().unwrap()))
            .unwrap();
    });

    let out = Command::new(binary())
        .args([
            "--api-base", &format!("http://127.0.0.1:{port}"),
            "send", "--chat-id", "42", "--text", "hello",
        ])
        .env("TG_HOME", home.path())
        .output()
        .unwrap();
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("sent (id: 99)"), "got: {stdout}");

    join.join().unwrap();
}

#[test]
fn send_file_hits_send_photo_with_multipart() {
    let home = tempfile::tempdir().unwrap();
    write_config(home.path(), "TOKEN");

    let server = Arc::new(tiny_http::Server::http("127.0.0.1:0").unwrap());
    let port = server.server_addr().to_ip().unwrap().port();
    let s2 = server.clone();

    let join = thread::spawn(move || {
        let req = s2.recv().unwrap();
        assert!(req.url().contains("/botTOKEN/sendPhoto"), "got {}", req.url());
        let ct = req.headers().iter()
            .find(|h| h.field.equiv("Content-Type"))
            .map(|h| h.value.as_str().to_string()).unwrap_or_default();
        assert!(ct.starts_with("multipart/form-data"), "ct: {ct}");

        let resp = r#"{"ok":true,"result":{"message_id":5,"chat":{"id":42,"type":"private"}}}"#;
        req.respond(tiny_http::Response::from_string(resp)
            .with_header("Content-Type: application/json".parse::<tiny_http::Header>().unwrap()))
            .unwrap();
    });

    // Create a tiny PNG-shaped file.
    let tmp = tempfile::Builder::new().suffix(".png").tempfile().unwrap();
    std::fs::write(tmp.path(), b"\x89PNG\r\n\x1a\nFAKE").unwrap();

    let out = Command::new(binary())
        .args([
            "--api-base", &format!("http://127.0.0.1:{port}"),
            "send", "--chat-id", "42", "--file", tmp.path().to_str().unwrap(),
        ])
        .env("TG_HOME", home.path())
        .output()
        .unwrap();
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));

    join.join().unwrap();
}
