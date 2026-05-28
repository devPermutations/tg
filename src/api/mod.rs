//! Telegram Bot API client and wire types.
//!
//! Split into `client` (HTTP + redaction) and `types` (deserialized
//! Bot API entities). External callers should treat `crate::api::*`
//! as the public surface — both submodules' contents are re-exported
//! here.

mod client;
mod types;

pub use client::Client;
pub(crate) use client::redact_err;
pub use types::{
    ApiResponse, Audio, Chat, Document, File, MediaRef, Message, PhotoSize,
    Sticker, Update, User, Voice,
};
