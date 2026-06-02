//! CLI surface — clap derive arg structs.

use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    name = "octo-matrix-onboard",
    version,
    about = "Authenticate against a Matrix homeserver and write a JSON config for octo-adapter-matrix-sdk.",
    long_about = None,
)]
pub struct Cli {
    /// Increase log verbosity (INFO → DEBUG). Tokens stay redacted at
    /// every level via the tracing-subscriber redaction layer.
    #[arg(short, long, global = true)]
    pub verbose: bool,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Authenticate via one of four modes and write a config file.
    Login {
        #[command(subcommand)]
        mode: LoginMode,
    },
    /// Load a config, call /whoami, print resolved user/device.
    /// Does NOT go through the adapter cdylib — uses matrix-sdk Client
    /// directly (the CLI is a standalone binary).
    Whoami(WhoamiArgs),
    /// Print version and exit.
    Version,
}

#[derive(Subcommand, Debug)]
pub enum LoginMode {
    /// Password login via `m.login.password`.
    Password(PasswordArgs),
    /// OAuth 2.0 Authorization Code login via `OAuth::login_with_authorization_code`.
    Oidc(OidcArgs),
    /// SSO login via `OAuth::login_sso` (MSC 2964 / MSC 3861).
    Sso(OidcArgs),
    /// QR login via `LoginWithGeneratedQrCode` (MSC 4108). CLI
    /// generates the QR; the operator scans it with a verified Element
    /// client (e.g. Element Android's "Link new device" flow).
    Qr(QrArgs),
}

/// Common output flags shared by every login mode.
#[derive(Args, Debug, Clone)]
pub struct OutputArgs {
    /// Output file path (default: ~/.config/octo/matrix.json on Unix,
    /// %APPDATA%\octo\matrix.json on Windows).
    #[arg(long, conflicts_with = "stdout")]
    pub out: Option<PathBuf>,
    /// Write JSON to stdout instead of a file.
    #[arg(long, conflicts_with = "out")]
    pub stdout: bool,
    /// Overwrite existing output file. By default the CLI refuses to
    /// overwrite — protects against re-running against the wrong
    /// homeserver.
    #[arg(long)]
    pub force: bool,
}

#[derive(Args, Debug)]
pub struct PasswordArgs {
    /// Matrix homeserver URL (e.g. https://matrix.example.com)
    #[arg(long)]
    pub homeserver: String,
    /// MXID or localpart (e.g. @alice:example.com or alice)
    #[arg(long)]
    pub user: String,
    /// Read password from stdin. This is the ONLY accepted form —
    /// passing `--password <value>` is rejected at clap level to
    /// prevent shell-history leaks.
    #[arg(long, action = clap::ArgAction::SetTrue)]
    pub password_stdin: bool,
    /// Device display name (optional). Helps the operator distinguish
    /// the onboarded session in Element's device list.
    #[arg(long)]
    pub device_name: Option<String>,
    #[command(flatten)]
    pub output: OutputArgs,
}

#[derive(Args, Debug)]
pub struct OidcArgs {
    /// Matrix homeserver URL
    #[arg(long)]
    pub homeserver: String,
    /// Device display name (optional)
    #[arg(long)]
    pub device_name: Option<String>,
    /// Headless mode: print the OIDC URL and expected redirect URI;
    /// operator pastes the final redirect URL on stdin.
    #[arg(long)]
    pub no_listener: bool,
    /// Port for the localhost callback listener (default 8080).
    #[arg(long, default_value_t = 8080)]
    pub port: u16,
    #[command(flatten)]
    pub output: OutputArgs,
}

#[derive(Args, Debug)]
pub struct QrArgs {
    /// Matrix homeserver URL
    #[arg(long)]
    pub homeserver: String,
    /// Device display name (optional)
    #[arg(long)]
    pub device_name: Option<String>,
    /// Timeout in seconds before giving up on a scan (default 300s,
    /// matches MSC 4108 rendezvous channel TTL).
    #[arg(long, default_value_t = 300)]
    pub timeout: u64,
    #[command(flatten)]
    pub output: OutputArgs,
}

#[derive(Args, Debug)]
pub struct WhoamiArgs {
    /// Path to a config file written by `login`.
    #[arg(long)]
    pub config: PathBuf,
}
