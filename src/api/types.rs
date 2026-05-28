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

/// What kind of attachment a Telegram message carries. Used by the
/// listen daemon to dispatch download + display. Each variant carries
/// the `file_id` (Telegram-provided handle to fetch via `getFile`)
/// and any optional metadata that's specific to that kind (filename
/// hint, sticker emoji, etc.).
///
/// The variants explicitly enumerate the supported kinds — video,
/// animation, and video_note are NOT handled in v0.7 and would be
/// added as new variants here.
#[derive(Debug)]
pub enum MediaRef<'a> {
    Photo { file_id: &'a str },
    Document { file_id: &'a str, name_hint: Option<&'a str> },
    Voice { file_id: &'a str },
    Audio { file_id: &'a str, name_hint: Option<&'a str> },
    Sticker { file_id: &'a str, emoji: Option<&'a str> },
}

impl<'a> MediaRef<'a> {
    pub fn file_id(&self) -> &'a str {
        match self {
            MediaRef::Photo { file_id }
            | MediaRef::Document { file_id, .. }
            | MediaRef::Voice { file_id }
            | MediaRef::Audio { file_id, .. }
            | MediaRef::Sticker { file_id, .. } => file_id,
        }
    }

    pub fn name_hint(&self) -> Option<&'a str> {
        match self {
            MediaRef::Document { name_hint, .. } | MediaRef::Audio { name_hint, .. } => *name_hint,
            _ => None,
        }
    }

    /// True if this media kind should be transcribed when whisper is
    /// configured. Voice and audio yes; photo/document/sticker no.
    pub fn is_transcribable(&self) -> bool {
        matches!(self, MediaRef::Voice { .. } | MediaRef::Audio { .. })
    }
}

impl Message {
    /// Returns the dispatch-relevant attachment reference, if any.
    /// Precedence matches v0.5/0.6 behavior: photo > document > voice
    /// > audio > sticker. Returns `None` for text-only messages or
    /// media kinds we don't handle (video, animation, video_note).
    pub fn media_ref(&self) -> Option<MediaRef<'_>> {
        if let Some(sizes) = self.photo.as_ref() {
            if let Some(p) = sizes.last() {
                return Some(MediaRef::Photo { file_id: p.file_id.as_str() });
            }
        }
        if let Some(d) = &self.document {
            return Some(MediaRef::Document {
                file_id: d.file_id.as_str(),
                name_hint: d.file_name.as_deref(),
            });
        }
        if let Some(v) = &self.voice {
            return Some(MediaRef::Voice { file_id: v.file_id.as_str() });
        }
        if let Some(a) = &self.audio {
            return Some(MediaRef::Audio {
                file_id: a.file_id.as_str(),
                name_hint: a.file_name.as_deref(),
            });
        }
        if let Some(s) = &self.sticker {
            return Some(MediaRef::Sticker {
                file_id: s.file_id.as_str(),
                emoji: s.emoji.as_deref(),
            });
        }
        None
    }
}
