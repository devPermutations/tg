//! Transcribe an inbound audio file via a whisper.cpp HTTP server.
//!
//! Pipeline mirrors the proven approach from mybot v3's voice.rs:
//! 1. Telegram-provided OGG/MP3/whatever is the input.
//! 2. ffmpeg converts it to 16 kHz mono WAV (whisper.cpp's expected
//!    input format).
//! 3. The WAV is POSTed to `{whisper_url}/inference` with multipart
//!    form fields `file` and `response_format=json`.
//! 4. The response JSON's `text` field is returned, trimmed.
//!
//! Failure modes:
//! - File too large (configurable, default 5 MB) → error.
//! - ffmpeg missing or exits non-zero → error with its stderr.
//! - Whisper server unreachable or non-2xx → error.
//! - Empty transcription text → error (caller decides whether to
//!   suppress or surface).
//!
//! Caller (the listen daemon) treats any error as "no transcript,
//! deliver the file path only" — transcription failure must not block
//! message delivery.

use anyhow::{anyhow, Context, Result};
use std::path::Path;
use std::process::{Command, Stdio};

/// Cap on audio file size that we'll convert + send to whisper. Larger
/// files are rejected early to avoid runaway ffmpeg/whisper work.
pub const MAX_AUDIO_BYTES: u64 = 5 * 1024 * 1024;

pub fn transcribe(audio_path: &Path, whisper_url: &str, ffmpeg_bin: &str) -> Result<String> {
    let meta = std::fs::metadata(audio_path)
        .with_context(|| format!("stat {}", audio_path.display()))?;
    if meta.len() > MAX_AUDIO_BYTES {
        return Err(anyhow!(
            "audio file too large: {} bytes (cap {} bytes)",
            meta.len(),
            MAX_AUDIO_BYTES
        ));
    }

    // Step 1: convert to 16 kHz mono WAV in a temp file.
    let wav = tempfile::Builder::new()
        .prefix("tg-tx-")
        .suffix(".wav")
        .tempfile()
        .context("create wav tempfile")?;
    let wav_path = wav.path().to_path_buf();

    let audio_str = audio_path
        .to_str()
        .ok_or_else(|| anyhow!("non-utf8 audio path"))?;
    let wav_str = wav_path
        .to_str()
        .ok_or_else(|| anyhow!("non-utf8 wav path"))?;

    let ff = Command::new(ffmpeg_bin)
        .args([
            "-i", audio_str,
            "-ar", "16000",
            "-ac", "1",
            "-f", "wav",
            "-y",
            wav_str,
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .with_context(|| format!("running {ffmpeg_bin}"))?;
    if !ff.status.success() {
        let stderr = String::from_utf8_lossy(&ff.stderr);
        let last = stderr.lines().last().unwrap_or("").to_string();
        return Err(anyhow!("ffmpeg exited {}: {last}", ff.status));
    }

    let wav_bytes = std::fs::read(&wav_path).context("read converted wav")?;
    drop(wav);

    // Step 2: POST multipart/form-data to whisper.
    let url = format!("{}/inference", whisper_url.trim_end_matches('/'));
    let boundary = format!("----tg-tx-{}", rand::random::<u64>());

    let mut body: Vec<u8> = Vec::with_capacity(wav_bytes.len() + 256);
    body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
    body.extend_from_slice(
        b"Content-Disposition: form-data; name=\"file\"; filename=\"voice.wav\"\r\n",
    );
    body.extend_from_slice(b"Content-Type: audio/wav\r\n\r\n");
    body.extend_from_slice(&wav_bytes);
    body.extend_from_slice(b"\r\n");
    body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
    body.extend_from_slice(b"Content-Disposition: form-data; name=\"response_format\"\r\n\r\n");
    body.extend_from_slice(b"json\r\n");
    body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());

    let agent = ureq::AgentBuilder::new()
        .timeout(std::time::Duration::from_secs(60))
        .build();
    let resp = agent
        .post(&url)
        .set("Content-Type", &format!("multipart/form-data; boundary={boundary}"))
        .send_bytes(&body)
        .with_context(|| format!("POST {url}"))?;
    let json: serde_json::Value = resp.into_json().context("parse whisper response")?;
    let text = json
        .get("text")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    if text.is_empty() {
        return Err(anyhow!("whisper returned empty transcription"));
    }
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refuses_oversized_file() {
        // Build a file larger than MAX_AUDIO_BYTES.
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let buf = vec![0u8; MAX_AUDIO_BYTES as usize + 1];
        std::fs::write(tmp.path(), &buf).unwrap();
        let err = transcribe(tmp.path(), "http://127.0.0.1:1", "ffmpeg")
            .unwrap_err()
            .to_string();
        assert!(err.contains("too large"), "got: {err}");
    }
}
