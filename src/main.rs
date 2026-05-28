use clap::{Parser, Subcommand};

mod paths;
mod config;
mod pending;
mod tmux;
mod api;
mod init;
mod access;
mod send;
mod listen;
mod install;
mod transcribe;

#[derive(Parser)]
#[command(name = "tg", about = "Telegram bot CLI: daemon + outbound", version)]
struct Cli {
    /// Hidden: override Telegram API base URL (for tests).
    #[arg(long, hide = true, global = true, default_value = "https://api.telegram.org")]
    api_base: String,

    /// Hidden: override tmux binary path (for tests).
    #[arg(long, hide = true, global = true, default_value = "tmux")]
    tmux_bin: String,

    /// Hidden: override systemctl binary path (for tests).
    #[arg(long, hide = true, global = true, default_value = "systemctl")]
    systemctl_bin: String,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Write ~/.tg/config.toml interactively.
    Init {
        #[arg(long)] token: Option<String>,
        #[arg(long)] tmux_target: Option<String>,
        /// chat_id that owns this bot. Only the owner's DMs are delivered
        /// to the tmux pane; everyone else added via `tg allow` is
        /// outbound-only. Optional — if omitted, all allowlisted senders
        /// deliver (pre-0.2 behavior).
        #[arg(long = "owner-chat-id")] owner_chat_id: Option<i64>,
        #[arg(long)] force: bool,
    },
    /// Symlink into ~/.ir/tools/ and install + enable the systemd unit.
    Install,
    /// Inbound daemon: poll Telegram, gate, deliver via tmux send-keys.
    Listen,
    /// Send a message (with optional --file attachments).
    Send {
        #[arg(long)] chat_id: i64,
        #[arg(long)] text: Option<String>,
        #[arg(long)] file: Vec<std::path::PathBuf>,
        #[arg(long)] format: Option<String>,
        #[arg(long = "reply-to")] reply_to: Option<i64>,
    },
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
    /// Set or unset the owner chat_id. The owner is the single sender
    /// whose DMs are delivered to the tmux pane; everyone else is
    /// outbound-only.
    SetOwner {
        /// chat_id to designate as owner.
        #[arg(long, conflicts_with = "unset")] chat_id: Option<i64>,
        /// Remove the current owner (revert to "everyone in allowlist delivers").
        #[arg(long, conflicts_with = "chat_id")] unset: bool,
    },
    /// Confirm a pending pairing by code.
    Pair { code: String },
    /// List pending pairings.
    Pending,
    /// Drop a pending pairing silently (no Telegram reply).
    Reject {
        #[arg(long)] chat_id: i64,
    },
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("tg=info"))
        )
        .init();

    let cli = Cli::parse();
    match cli.command {
        Command::Init { token, tmux_target, owner_chat_id, force } =>
            init::run(init::InitOpts { token, tmux_target, owner_chat_id, force }),
        Command::Install => install::run(install::InstallOpts {
            systemctl_bin: cli.systemctl_bin.clone(),
            dry_run: false,
        }),
        Command::Listen => listen::run(&cli.api_base, &cli.tmux_bin),
        Command::Send { chat_id, text, file, format, reply_to } =>
            send::run(send::SendOpts { chat_id, text, files: file, format, reply_to }, &cli.api_base),
        Command::Allow { chat_id, label } => access::allow(chat_id, label),
        Command::Deny { chat_id } => access::deny(chat_id),
        Command::List => access::list(),
        Command::SetOwner { chat_id, unset } => access::set_owner(chat_id, unset),
        Command::Pair { code } => access::pair(&code, &cli.api_base),
        Command::Pending => access::pending(),
        Command::Reject { chat_id } => access::reject(chat_id),
    }
}
