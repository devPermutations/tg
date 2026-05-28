use serde::Deserialize;
use anyhow::anyhow;
use anyhow::Result;

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

#[derive(Debug, Deserialize)]
pub struct File {
    pub file_id: String,
    pub file_unique_id: String,
    #[serde(default)] pub file_size: Option<u64>,
    pub file_path: Option<String>,
}
