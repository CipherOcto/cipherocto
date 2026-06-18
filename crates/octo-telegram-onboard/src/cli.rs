//! CLI surface — clap derive arg structs.

use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    name = "octo-telegram-onboard",
    version,
    about = "Authenticate against Telegram via TDLib and write a JSON config for octo-adapter-telegram.",
    long_about = None,
)]
pub struct Cli {
    /// Increase log verbosity (INFO → DEBUG). Secrets stay redacted at every level.
    #[arg(short, long, global = true)]
    pub verbose: bool,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Non-interactive bot auth via TDLib.
    BotSetup(BotSetupArgs),
    /// QR-code login: render a QR in the terminal, scan it from your
    /// already-logged-in Telegram app on another device.
    QrLink(QrLinkArgs),
    /// Interactive user-account auth via TDLib (phone + code + 2FA).
    UserLogin(UserLoginArgs),
    /// Verify existing session by calling get_me().
    Whoami(WhoamiArgs),
    /// Refresh user_id/username in an existing config by calling
    /// get_me() on the existing TDLib session. Useful when the
    /// initial auth wrote a partial session (user_id=0) because
    /// get_me timed out.
    RefreshIdentity(RefreshIdentityArgs),
    /// Session management (list, verify, remove).
    Session {
        #[command(subcommand)]
        action: SessionAction,
    },
    /// Print version and exit.
    Version,
}

#[derive(Args, Debug, Clone)]
pub struct BotSetupArgs {
    /// Telegram bot token (or $TELEGRAM_BOT_TOKEN).
    #[arg(long, env = "TELEGRAM_BOT_TOKEN")]
    pub bot_token: Option<String>,

    /// API ID from my.telegram.org (or $TELEGRAM_API_ID).
    #[arg(long, env = "TELEGRAM_API_ID")]
    pub api_id: Option<i32>,

    /// API hash from my.telegram.org (or $TELEGRAM_API_HASH).
    #[arg(long, env = "TELEGRAM_API_HASH")]
    pub api_hash: Option<String>,

    /// TDLib data directory (default: ~/.local/share/octo/telegram/default/).
    #[arg(long)]
    pub data_dir: Option<PathBuf>,

    /// Ed25519 verifying key (base64, optional; or $TELEGRAM_VERIFYING_KEY).
    #[arg(long, env = "TELEGRAM_VERIFYING_KEY")]
    pub verifying_key: Option<String>,

    /// Output config file path (default: ~/.config/octo/telegram.json).
    #[arg(long, conflicts_with = "stdout")]
    pub out: Option<PathBuf>,

    /// Write JSON to stdout instead of a file.
    #[arg(long, conflicts_with = "out")]
    pub stdout: bool,

    /// Overwrite existing config file.
    #[arg(long)]
    pub force: bool,

    /// Auth timeout in seconds (default: 30).
    #[arg(long, default_value = "30")]
    pub timeout: u64,
}

#[derive(Args, Debug, Clone)]
pub struct QrLinkArgs {
    /// API ID from my.telegram.org (or $TELEGRAM_API_ID).
    #[arg(long, env = "TELEGRAM_API_ID")]
    pub api_id: Option<i32>,

    /// API hash from my.telegram.org (or $TELEGRAM_API_HASH).
    #[arg(long, env = "TELEGRAM_API_HASH")]
    pub api_hash: Option<String>,

    /// TDLib data directory (default: ~/.local/share/octo/telegram/default/).
    #[arg(long)]
    pub data_dir: Option<PathBuf>,

    /// Ed25519 verifying key (base64, optional; or $TELEGRAM_VERIFYING_KEY).
    #[arg(long, env = "TELEGRAM_VERIFYING_KEY")]
    pub verifying_key: Option<String>,

    /// Output config file path (default: ~/.config/octo/telegram.json).
    #[arg(long, conflicts_with = "stdout")]
    pub out: Option<PathBuf>,

    /// Write JSON to stdout instead of a file.
    #[arg(long, conflicts_with = "out")]
    pub stdout: bool,

    /// Overwrite existing config file.
    #[arg(long)]
    pub force: bool,

    /// Auth timeout in seconds (default: 300 = 5min to scan the QR).
    #[arg(long, default_value = "300")]
    pub timeout: u64,
}

impl QrLinkArgs {
    pub fn resolved_data_dir(&self) -> PathBuf {
        self.data_dir
            .clone()
            .unwrap_or_else(default_telegram_data_dir)
    }
}

#[derive(Args, Debug, Clone)]
pub struct UserLoginArgs {
    /// API ID from my.telegram.org (or $TELEGRAM_API_ID).
    #[arg(long, env = "TELEGRAM_API_ID")]
    pub api_id: Option<i32>,

    /// API hash from my.telegram.org (or $TELEGRAM_API_HASH).
    #[arg(long, env = "TELEGRAM_API_HASH")]
    pub api_hash: Option<String>,

    /// Phone number (or $TELEGRAM_PHONE).
    #[arg(long, env = "TELEGRAM_PHONE")]
    pub phone: Option<String>,

    /// TDLib data directory.
    #[arg(long)]
    pub data_dir: Option<PathBuf>,

    /// Ed25519 verifying key (base64, optional; or $TELEGRAM_VERIFYING_KEY).
    #[arg(long, env = "TELEGRAM_VERIFYING_KEY")]
    pub verifying_key: Option<String>,

    /// Output config file path.
    #[arg(long, conflicts_with = "stdout")]
    pub out: Option<PathBuf>,

    /// Write JSON to stdout instead of a file.
    #[arg(long, conflicts_with = "out")]
    pub stdout: bool,

    /// Overwrite existing config file.
    #[arg(long)]
    pub force: bool,

    /// Auth timeout in seconds (default: 300).
    #[arg(long, default_value = "300")]
    pub timeout: u64,
}

#[derive(Args, Debug, Clone)]
pub struct WhoamiArgs {
    /// Path to TelegramConfig JSON file (or $TELEGRAM_CONFIG).
    #[arg(long, env = "TELEGRAM_CONFIG")]
    pub config: Option<PathBuf>,
}

#[derive(Args, Debug, Clone)]
pub struct RefreshIdentityArgs {
    /// Path to TelegramConfig JSON file (or $TELEGRAM_CONFIG).
    #[arg(long, env = "TELEGRAM_CONFIG")]
    pub config: Option<PathBuf>,

    /// API ID (or $TELEGRAM_API_ID). Used as a fallback when the
    /// config doesn't have api_id (e.g., partial sessions from
    /// qr-link, which only writes data_dir/mode/username).
    #[arg(long, env = "TELEGRAM_API_ID")]
    pub api_id: Option<i32>,

    /// API hash (or $TELEGRAM_API_HASH). Same fallback rationale
    /// as --api-id.
    #[arg(long, env = "TELEGRAM_API_HASH")]
    pub api_hash: Option<String>,

    /// Timeout in seconds for the get_me call (default: 60).
    /// Should be longer than the post-auth bookkeeping TDLib
    /// needs to settle; 60s is generous for a healthy session.
    #[arg(long, default_value = "60")]
    pub timeout: u64,
}

#[derive(Subcommand, Debug)]
pub enum SessionAction {
    /// Show known TDLib database dirs.
    List(SessionListArgs),
    /// Check if a TDLib database has a valid session.
    Verify {
        /// Directory to verify.
        dir: PathBuf,
        /// Path to TelegramConfig JSON for get_me validation (default: ~/.config/octo/telegram.json).
        #[arg(long)]
        config: Option<PathBuf>,
    },
    /// Delete a TDLib database dir (with confirmation).
    Remove {
        /// Directory to remove.
        dir: PathBuf,
        /// Skip confirmation prompt.
        #[arg(long)]
        yes: bool,
    },
}

#[derive(Args, Debug, Clone)]
pub struct SessionListArgs {
    /// Base directory to scan (default: ~/.local/share/octo/telegram/).
    #[arg(long)]
    pub base_dir: Option<PathBuf>,
    /// Path to TelegramConfig JSON for get_me fallback (default: ~/.config/octo/telegram.json; or $TELEGRAM_CONFIG).
    #[arg(long, env = "TELEGRAM_CONFIG")]
    pub config: Option<PathBuf>,
}

impl SessionListArgs {
    pub fn resolved_base_dir(&self) -> PathBuf {
        self.base_dir
            .clone()
            .unwrap_or_else(default_octo_telegram_dir)
    }
}

/// Base directory for all Telegram sessions: `~/.local/share/octo/telegram/`.
fn default_octo_telegram_dir() -> PathBuf {
    let mut base = dirs::data_dir()
        .or_else(dirs::home_dir)
        .unwrap_or_else(|| PathBuf::from("."));
    base.push("octo");
    base.push("telegram");
    base
}

/// Default data dir for bot-setup/user-login: `<base>/default`.
fn default_telegram_data_dir() -> PathBuf {
    let mut dir = default_octo_telegram_dir();
    dir.push("default");
    dir
}

impl BotSetupArgs {
    pub fn resolved_data_dir(&self) -> PathBuf {
        self.data_dir
            .clone()
            .unwrap_or_else(default_telegram_data_dir)
    }
}

impl UserLoginArgs {
    pub fn resolved_data_dir(&self) -> PathBuf {
        self.data_dir
            .clone()
            .unwrap_or_else(default_telegram_data_dir)
    }
}
