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

    pub fn send_message(&self, chat_id: i64, text: &str) -> Result<Message> {
        #[derive(Serialize)]
        struct Req<'a> { chat_id: i64, text: &'a str }
        self.post_json("sendMessage", &Req { chat_id, text })
    }
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
        let m = c.send_message(1, "hi").unwrap();
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
        let err = c.send_message(1, "hi").unwrap_err().to_string();
        assert!(err.contains("chat not found"), "got: {err}");
        join.join().unwrap();
    }
}
