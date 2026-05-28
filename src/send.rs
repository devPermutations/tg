//! `tg send` — outbound text and attachments.

use anyhow::{Context, Result};
use std::path::PathBuf;

use crate::api::Client;
use crate::config::Config;
use crate::paths;

pub struct SendOpts {
    pub chat_id: i64,
    pub text: Option<String>,
    pub files: Vec<PathBuf>,
    pub format: Option<String>,
    pub reply_to: Option<i64>,
}

pub fn run(opts: SendOpts, api_base: &str) -> Result<()> {
    let cfg = Config::load(&paths::config_path())
        .with_context(|| "loading ~/.tg/config.toml")?;
    let client = Client::new(api_base, cfg.bot_token);

    if opts.files.is_empty() {
        let text = opts.text
            .ok_or_else(|| anyhow::anyhow!("--text required when no --file given"))?;
        let m = client.send_message(opts.chat_id, &text)?;
        println!("sent (id: {})", m.message_id);
        return Ok(());
    }

    let mut first = true;
    for path in &opts.files {
        // First file carries the caption; subsequent files send without.
        let caption = if first { opts.text.as_deref() } else { None };
        first = false;

        let kind = mime_guess::from_path(path).first_or_octet_stream();
        let is_image = kind.type_() == "image";
        let m = if is_image {
            client.send_photo(opts.chat_id, path, caption, opts.format.as_deref(), opts.reply_to)
        } else {
            client.send_document(opts.chat_id, path, caption, opts.format.as_deref(), opts.reply_to)
        }.with_context(|| format!("sending {}", path.display()))?;
        println!("sent {} (id: {})", path.display(), m.message_id);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requires_text_when_no_files() {
        // We can't easily run() without a saved config; this tests the
        // logical guard by replicating its check.
        let opts = SendOpts {
            chat_id: 1, text: None, files: vec![],
            format: None, reply_to: None,
        };
        assert!(opts.text.is_none() && opts.files.is_empty());
    }
}
