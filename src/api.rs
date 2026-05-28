//! Telegram Bot API client (sync, ureq).
//!
//! Operations needed by tg:
//! - getUpdates (long-poll for inbound)
//! - sendMessage (text outbound, pairing reminders, "agent offline")
//! - sendPhoto / sendDocument (multipart outbound, Task 7)
//! - getFile + file download (inbound attachments, Task 8)

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Clone)]
pub struct Client {
    pub api_base: String,
    pub token: String,
    pub agent: ureq::Agent,
}

impl Client {
    pub fn new(api_base: impl Into<String>, token: impl Into<String>) -> Self {
        Self {
            api_base: api_base.into(),
            token: token.into(),
            agent: ureq::AgentBuilder::new()
                .timeout(std::time::Duration::from_secs(35))
                .build(),
        }
    }

    fn endpoint(&self, method: &str) -> String {
        format!("{}/bot{}/{}", self.api_base.trim_end_matches('/'), self.token, method)
    }

    /// POST application/json. Used for plain text endpoints.
    fn post_json<T: Serialize, R: serde::de::DeserializeOwned>(
        &self, method: &str, body: &T,
    ) -> Result<R> {
        let url = self.endpoint(method);
        let resp = self.agent.post(&url)
            .send_json(serde_json::to_value(body)?)
            .with_context(|| format!("POST {method}"))?;
        let parsed: ApiResponse<R> = resp.into_json()?;
        parsed.into_result()
    }

    pub fn get_updates(&self, offset: i64, timeout_secs: u32) -> Result<Vec<Update>> {
        #[derive(Serialize)]
        struct Req { offset: i64, timeout: u32, allowed_updates: Vec<&'static str> }
        let body = Req {
            offset,
            timeout: timeout_secs,
            allowed_updates: vec!["message"],
        };
        self.post_json("getUpdates", &body)
    }

    pub fn send_message(
        &self, chat_id: i64, text: &str,
        parse_mode: Option<&str>, reply_to: Option<i64>,
    ) -> Result<Message> {
        #[derive(Serialize)]
        struct Req<'a> {
            chat_id: i64,
            text: &'a str,
            #[serde(skip_serializing_if = "Option::is_none")]
            parse_mode: Option<&'a str>,
            #[serde(skip_serializing_if = "Option::is_none")]
            reply_to_message_id: Option<i64>,
        }
        self.post_json("sendMessage", &Req {
            chat_id, text,
            parse_mode,
            reply_to_message_id: reply_to,
        })
    }

    /// POST multipart/form-data with one file field. Used for
    /// sendPhoto/sendDocument. Builds the body by hand — ureq has no
    /// multipart helper.
    fn post_multipart(
        &self,
        method: &str,
        fields: &[(&str, &str)],
        file_field: &str,
        file_name: &str,
        file_mime: &str,
        file_bytes: &[u8],
    ) -> Result<Message> {
        let boundary = format!("----tg-{}", rand::random::<u64>());
        let mut body: Vec<u8> = Vec::new();
        for (name, value) in fields {
            body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
            body.extend_from_slice(format!("Content-Disposition: form-data; name=\"{name}\"\r\n\r\n").as_bytes());
            body.extend_from_slice(value.as_bytes());
            body.extend_from_slice(b"\r\n");
        }
        body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
        body.extend_from_slice(
            format!("Content-Disposition: form-data; name=\"{file_field}\"; filename=\"{file_name}\"\r\n").as_bytes()
        );
        body.extend_from_slice(format!("Content-Type: {file_mime}\r\n\r\n").as_bytes());
        body.extend_from_slice(file_bytes);
        body.extend_from_slice(b"\r\n");
        body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());

        let url = self.endpoint(method);
        let resp = self.agent.post(&url)
            .set("Content-Type", &format!("multipart/form-data; boundary={boundary}"))
            .send_bytes(&body)
            .with_context(|| format!("POST {method} (multipart)"))?;
        let parsed: ApiResponse<Message> = resp.into_json()?;
        parsed.into_result()
    }

    pub fn send_photo(
        &self, chat_id: i64, file_path: &std::path::Path,
        caption: Option<&str>, parse_mode: Option<&str>, reply_to: Option<i64>,
    ) -> Result<Message> {
        self.send_file("sendPhoto", "photo", chat_id, file_path, caption, parse_mode, reply_to)
    }

    pub fn send_document(
        &self, chat_id: i64, file_path: &std::path::Path,
        caption: Option<&str>, parse_mode: Option<&str>, reply_to: Option<i64>,
    ) -> Result<Message> {
        self.send_file("sendDocument", "document", chat_id, file_path, caption, parse_mode, reply_to)
    }

    fn send_file(
        &self, method: &str, file_field: &str, chat_id: i64,
        path: &std::path::Path, caption: Option<&str>,
        parse_mode: Option<&str>, reply_to: Option<i64>,
    ) -> Result<Message> {
        let bytes = std::fs::read(path)
            .with_context(|| format!("reading {}", path.display()))?;
        let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("file");
        let mime = mime_guess::from_path(path).first_or_octet_stream().to_string();
        let chat_id_s = chat_id.to_string();
        // Telegram's new `reply_parameters` field is a JSON object; the
        // legacy `reply_to_message_id` scalar is still supported and is
        // simpler to encode as a multipart form field.
        let reply_to_s = reply_to.map(|i| i.to_string());
        let mut fields: Vec<(&str, &str)> = vec![("chat_id", chat_id_s.as_str())];
        if let Some(c) = caption { fields.push(("caption", c)); }
        if let Some(pm) = parse_mode { fields.push(("parse_mode", pm)); }
        if let Some(rs) = reply_to_s.as_deref() {
            fields.push(("reply_to_message_id", rs));
        }
        self.post_multipart(method, &fields, file_field, name, &mime, &bytes)
    }

    pub fn get_file(&self, file_id: &str) -> Result<File> {
        #[derive(Serialize)]
        struct Req<'a> { file_id: &'a str }
        self.post_json("getFile", &Req { file_id })
    }

    /// Downloads the file referenced by `File.file_path` and writes the
    /// bytes to `dest`. Returns the number of bytes written.
    pub fn download_file(&self, file: &File, dest: &std::path::Path) -> Result<u64> {
        let path = file.file_path.as_deref()
            .ok_or_else(|| anyhow!("getFile response has no file_path"))?;
        let url = format!("{}/file/bot{}/{}",
            self.api_base.trim_end_matches('/'), self.token, path);
        let resp = self.agent.get(&url).call()
            .with_context(|| format!("GET {url}"))?;
        let mut reader = resp.into_reader();
        if let Some(parent) = dest.parent() { std::fs::create_dir_all(parent)?; }
        let mut out = std::fs::File::create(dest)?;
        let n = std::io::copy(&mut reader, &mut out)?;
        Ok(n)
    }
}

#[derive(Debug, Deserialize)]
pub struct File {
    pub file_id: String,
    pub file_unique_id: String,
    #[serde(default)] pub file_size: Option<u64>,
    pub file_path: Option<String>,
}

/// Wrapper for Telegram's `{ok, result|description}` envelope.
#[derive(Deserialize)]
pub struct ApiResponse<T> {
    pub ok: bool,
    pub result: Option<T>,
    pub description: Option<String>,
}

impl<T> ApiResponse<T> {
    pub fn into_result(self) -> Result<T> {
        if self.ok {
            self.result.ok_or_else(|| anyhow!("Telegram API: ok=true but no result"))
        } else {
            Err(anyhow!("Telegram API: {}", self.description.unwrap_or_default()))
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct Update {
    pub update_id: i64,
    pub message: Option<Message>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Message {
    pub message_id: i64,
    pub from: Option<User>,
    pub chat: Chat,
    #[serde(default)] pub text: Option<String>,
    #[serde(default)] pub caption: Option<String>,
    #[serde(default)] pub photo: Option<Vec<PhotoSize>>,
    #[serde(default)] pub document: Option<Document>,
    #[serde(default)] pub voice: Option<Voice>,
    #[serde(default)] pub audio: Option<Audio>,
    #[serde(default)] pub sticker: Option<Sticker>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct User {
    pub id: i64,
    #[serde(default)] pub username: Option<String>,
    #[serde(default)] pub first_name: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Chat {
    pub id: i64,
    #[serde(rename = "type")] pub kind: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct PhotoSize {
    pub file_id: String,
    pub file_unique_id: String,
    pub width: u32,
    pub height: u32,
    #[serde(default)] pub file_size: Option<u64>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Document {
    pub file_id: String,
    pub file_unique_id: String,
    #[serde(default)] pub file_name: Option<String>,
    #[serde(default)] pub mime_type: Option<String>,
    #[serde(default)] pub file_size: Option<u64>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Voice {
    pub file_id: String,
    pub file_unique_id: String,
    /// Duration in seconds.
    pub duration: u32,
    #[serde(default)] pub mime_type: Option<String>,
    #[serde(default)] pub file_size: Option<u64>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Audio {
    pub file_id: String,
    pub file_unique_id: String,
    /// Duration in seconds.
    pub duration: u32,
    #[serde(default)] pub performer: Option<String>,
    #[serde(default)] pub title: Option<String>,
    #[serde(default)] pub file_name: Option<String>,
    #[serde(default)] pub mime_type: Option<String>,
    #[serde(default)] pub file_size: Option<u64>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Sticker {
    pub file_id: String,
    pub file_unique_id: String,
    #[serde(default)] pub emoji: Option<String>,
    #[serde(default)] pub set_name: Option<String>,
    #[serde(default)] pub is_animated: bool,
    #[serde(default)] pub is_video: bool,
    #[serde(default)] pub file_size: Option<u64>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;

    fn spawn_mock<F: Send + 'static>(handler: F) -> (String, thread::JoinHandle<()>)
    where F: Fn(tiny_http::Request)
    {
        // Bind directly with tiny_http on port 0; ask it for the chosen port.
        let server = Arc::new(tiny_http::Server::http("127.0.0.1:0").unwrap());
        let port = server.server_addr().to_ip().unwrap().port();
        let s2 = server.clone();
        let join = thread::spawn(move || {
            // Single request, then return — tests are scoped.
            if let Ok(req) = s2.recv() {
                handler(req);
            }
        });
        (format!("http://127.0.0.1:{port}"), join)
    }

    #[test]
    fn send_message_parses_envelope() {
        let (base, join) = spawn_mock(|req| {
            let body = r#"{"ok":true,"result":{"message_id":42,"chat":{"id":1,"type":"private"}}}"#;
            req.respond(tiny_http::Response::from_string(body)
                .with_header("Content-Type: application/json".parse::<tiny_http::Header>().unwrap()))
                .unwrap();
        });
        let c = Client::new(base, "TOKEN");
        let m = c.send_message(1, "hi", None, None).unwrap();
        assert_eq!(m.message_id, 42);
        join.join().unwrap();
    }

    #[test]
    fn send_message_propagates_error_description() {
        let (base, join) = spawn_mock(|req| {
            let body = r#"{"ok":false,"description":"Bad Request: chat not found"}"#;
            req.respond(tiny_http::Response::from_string(body)
                .with_header("Content-Type: application/json".parse::<tiny_http::Header>().unwrap()))
                .unwrap();
        });
        let c = Client::new(base, "TOKEN");
        let err = c.send_message(1, "hi", None, None).unwrap_err().to_string();
        assert!(err.contains("chat not found"), "got: {err}");
        join.join().unwrap();
    }

    #[test]
    fn send_photo_emits_multipart_with_chat_id() {
        let (base, join) = spawn_mock(|mut req| {
            assert!(req.url().ends_with("/sendPhoto"));
            let ct = req.headers().iter()
                .find(|h| h.field.equiv("Content-Type"))
                .map(|h| h.value.as_str().to_string())
                .unwrap_or_default();
            assert!(ct.starts_with("multipart/form-data; boundary="));
            let mut body = Vec::new();
            req.as_reader().read_to_end(&mut body).unwrap();
            let s = String::from_utf8_lossy(&body);
            assert!(s.contains("name=\"chat_id\""));
            assert!(s.contains("\r\n\r\n42\r\n"));
            assert!(s.contains("name=\"photo\""));
            let body_resp = r#"{"ok":true,"result":{"message_id":7,"chat":{"id":42,"type":"private"}}}"#;
            req.respond(tiny_http::Response::from_string(body_resp)
                .with_header("Content-Type: application/json".parse::<tiny_http::Header>().unwrap()))
                .unwrap();
        });

        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), b"fakepng").unwrap();

        let c = Client::new(base, "TOKEN");
        let m = c.send_photo(42, tmp.path(), Some("cap"), None, None).unwrap();
        assert_eq!(m.message_id, 7);
        join.join().unwrap();
    }

    #[test]
    fn get_file_parses_response() {
        let (base, join) = spawn_mock(|req| {
            let body = r#"{"ok":true,"result":{"file_id":"X","file_unique_id":"Y","file_path":"documents/file.pdf","file_size":1234}}"#;
            req.respond(tiny_http::Response::from_string(body)
                .with_header("Content-Type: application/json".parse::<tiny_http::Header>().unwrap()))
                .unwrap();
        });
        let c = Client::new(base, "TOKEN");
        let f = c.get_file("X").unwrap();
        assert_eq!(f.file_path.as_deref(), Some("documents/file.pdf"));
        join.join().unwrap();
    }
}
