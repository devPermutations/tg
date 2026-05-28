//! End-to-end test for the transcription path: tg listen receives a
//! voice message, downloads the .oga, ffmpeg-converts, POSTs to a
//! mock whisper.cpp server, appends [transcript: ...] to the typed
//! line. Requires `ffmpeg` on the host; the test skips itself if
//! ffmpeg isn't available.

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
    if let Ok(p) = std::env::var("CARGO_BIN_EXE_tg") {
        return std::path::PathBuf::from(p);
    }
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR")
        .expect("CARGO_MANIFEST_DIR not set");
    std::path::PathBuf::from(manifest_dir).join("target/debug/tg")
}

fn write_fake_tmux(path: &std::path::Path, record_path: &std::path::Path) {
    let script = format!(
        "#!/bin/bash\n\
         printf '%s\\t' \"$@\" >> {rec}\n\
         printf '\\n' >> {rec}\n\
         exit 0\n",
        rec = record_path.display());
    std::fs::write(path, script).unwrap();
    use std::os::unix::fs::PermissionsExt;
    let mut perms = std::fs::metadata(path).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(path, perms).unwrap();
}

/// Write config with the legacy `whisper_url` top-level field, which is
/// what `listen.rs` reads directly via `cfg.whisper_url.as_deref()`.
fn write_config(home: &std::path::Path, token: &str, owner: i64, whisper_port: u16) {
    let cfg = format!(
        "bot_token = \"{token}\"\n\
         tmux_target = \"test:0\"\n\
         owner_chat_id = {owner}\n\
         whisper_url = \"http://127.0.0.1:{whisper_port}\"\n\
         \n\
         [[allow]]\n\
         chat_id = {owner}\n\
         label = \"owner\"\n");
    let path = home.join("config.toml");
    std::fs::create_dir_all(home).unwrap();
    std::fs::write(&path, cfg).unwrap();
    use std::os::unix::fs::PermissionsExt;
    let mut perms = std::fs::metadata(&path).unwrap().permissions();
    perms.set_mode(0o600);
    std::fs::set_permissions(&path, perms).unwrap();
}

fn ffmpeg_available() -> bool {
    Command::new("ffmpeg")
        .arg("-version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

#[test]
fn voice_message_transcribed_via_whisper_mock() {
    if !ffmpeg_available() {
        eprintln!("skipping transcribe e2e: ffmpeg not on PATH");
        return;
    }

    let home = tempfile::tempdir().unwrap();
    let shim_dir = tempfile::tempdir().unwrap();
    let shim = shim_dir.path().join("fake-tmux");
    let record = shim_dir.path().join("argv.log");
    write_fake_tmux(&shim, &record);

    // Bind Telegram mock and whisper mock on separate OS-assigned ports.
    let tg_server = Arc::new(tiny_http::Server::http("127.0.0.1:0").unwrap());
    let tg_port = tg_server.server_addr().to_ip().unwrap().port();
    let whisper_server = Arc::new(tiny_http::Server::http("127.0.0.1:0").unwrap());
    let whisper_port = whisper_server.server_addr().to_ip().unwrap().port();

    write_config(home.path(), "TOKEN", 100, whisper_port);

    // Build a 1-second Opus OGG file that ffmpeg can transcode to WAV.
    let audio_path = home.path().join("voice.oga");
    let status = Command::new("ffmpeg")
        .args([
            "-f", "lavfi",
            "-i", "sine=frequency=440:duration=1",
            "-c:a", "libopus",
            "-y", audio_path.to_str().unwrap(),
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .unwrap();
    assert!(status.success(), "ffmpeg failed to build test .oga");

    // Telegram mock: getUpdates → voice update, getFile, file download.
    let audio_path_str = audio_path.to_str().unwrap().to_string();
    let tg_server2 = tg_server.clone();
    let tg_join = thread::spawn(move || {
        let mut served = 0u32;
        while let Ok(req) = tg_server2.recv() {
            served += 1;
            let url = req.url().to_string();
            if url.contains("/getUpdates") {
                let body = if served == 1 {
                    r#"{"ok":true,"result":[{"update_id":1,"message":{"message_id":7,"from":{"id":100,"username":"alice"},"chat":{"id":100,"type":"private"},"voice":{"file_id":"VOICE_FID","file_unique_id":"VOICE_UID","duration":1,"mime_type":"audio/ogg"}}}]}"#.to_string()
                } else {
                    r#"{"ok":true,"result":[]}"#.to_string()
                };
                req.respond(
                    tiny_http::Response::from_string(body)
                        .with_header("Content-Type: application/json".parse::<tiny_http::Header>().unwrap()),
                ).unwrap();
            } else if url.contains("/getFile") {
                // Return a file_path that points to our local fixture;
                // the download URL is {api_base}/file/botTOKEN/{file_path}.
                let body = r#"{"ok":true,"result":{"file_id":"VOICE_FID","file_unique_id":"VOICE_UID","file_path":"fixture.oga"}}"#;
                req.respond(
                    tiny_http::Response::from_string(body)
                        .with_header("Content-Type: application/json".parse::<tiny_http::Header>().unwrap()),
                ).unwrap();
            } else if url.contains("/file/bot") && url.ends_with("fixture.oga") {
                // Serve the actual bytes of our test .oga.
                let bytes = std::fs::read(&audio_path_str).unwrap();
                req.respond(tiny_http::Response::from_data(bytes)).unwrap();
            } else {
                req.respond(
                    tiny_http::Response::from_string("?").with_status_code(404),
                ).unwrap();
            }
            if served >= 5 {
                return;
            }
        }
    });

    // Whisper mock: accept the multipart POST, return a canned transcript.
    let whisper_server2 = whisper_server.clone();
    let whisper_join = thread::spawn(move || {
        if let Ok(mut req) = whisper_server2.recv() {
            assert!(
                req.url().contains("/inference"),
                "unexpected whisper url: {}",
                req.url()
            );
            let mut body = Vec::new();
            req.as_reader().read_to_end(&mut body).unwrap();
            assert!(
                body.len() > 100,
                "expected real wav body, got {} bytes",
                body.len()
            );
            req.respond(
                tiny_http::Response::from_string(r#"{"text":"hello from the mock"}"#)
                    .with_header("Content-Type: application/json".parse::<tiny_http::Header>().unwrap()),
            ).unwrap();
        }
    });

    let mut child = Command::new(binary())
        .args([
            "--api-base", &format!("http://127.0.0.1:{tg_port}"),
            "--tmux-bin", shim.to_str().unwrap(),
            "listen",
        ])
        .env("TG_HOME", home.path())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();

    // Allow enough time: poll (~3s) + getFile + download + ffmpeg + whisper POST.
    thread::sleep(Duration::from_secs(10));
    let _ = child.kill();
    let _ = child.wait();
    let _ = tg_join.join();
    let _ = whisper_join.join();

    let log = std::fs::read_to_string(&record).unwrap();
    assert!(log.contains("send-keys"), "no send-keys recorded: {log}");
    assert!(log.contains("@alice"), "no @alice tag: {log}");
    assert!(log.contains("(voice 0:01)"), "no voice label: {log}");
    assert!(
        log.contains("[transcript: hello from the mock]"),
        "no transcript: {log}"
    );
}
