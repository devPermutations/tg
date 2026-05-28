use clap::{Parser, Subcommand};

mod paths;
mod config;
mod pending;
mod tmux;
mod api;
mod init;
mod access;

#[derive(Parser)]
#[command(name = "tg", about = "Telegram bot CLI: daemon + outbound", version)]
struct Cli {
    /// Hidden: override Telegram API base URL (for tests).
    #[arg(long, hide = true, global = true, default_value = "https://api.telegram.org")]
    api_base: String,

    /// Hidden: override tmux binary path (for tests).
    #[arg(long, hide = true, global = true, default_value = "tmux")]
    tmux_bin: String,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Write ~/.tg/config.toml interactively.
    Init {
        #[arg(long)] token: Option<String>,
        #[arg(long)] tmux_target: Option<String>,
        #[arg(long)] force: bool,
    },
    /// Symlink into ~/.ir/tools/ and install + enable the systemd unit.
    Install,
    /// Inbound daemon: poll Telegram, gate, deliver via tmux send-keys.
    Listen,
    /// Send a message (with optional --file attachments).
    Send,
    /// Append a chat_id to the allowlist.
    Allow {
        #[arg(long)] chat_id: i64,
        #[arg(long)] label: Option<String>,
    },
    /// Remove a chat_id from the allowlist.
    Deny {
        #[arg(long)] chat_id: i64,
    },
    /// Print the current allowlist.
    List,
    /// Confirm a pending pairing by code.
    Pair,
    /// List pending pairings.
    Pending,
    /// Drop a pending pairing silently (no Telegram reply).
    Reject,
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();
    match cli.command {
        Command::Init { token, tmux_target, force } =>
            init::run(init::InitOpts { token, tmux_target, force }),
        Command::Install => todo!("Task 14: install"),
        Command::Listen => todo!("Task 13: listen"),
        Command::Send => todo!("Task 12: send"),
        Command::Allow { chat_id, label } => access::allow(chat_id, label),
        Command::Deny { chat_id } => access::deny(chat_id),
        Command::List => access::list(),
        Command::Pair => todo!("Task 11: pair"),
        Command::Pending => todo!("Task 11: pending"),
        Command::Reject => todo!("Task 11: reject"),
    }
}
