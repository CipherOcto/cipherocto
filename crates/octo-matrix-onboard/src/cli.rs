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
    /// End-to-end encryption flows (mission 0850h-b). These are
    /// non-auth subcommands that operate on a config produced by
    /// `octo-matrix-onboard login` and a running session. They cover
    /// cross-signing bootstrap, emoji-SAS device verification, and
    /// 4S recovery-key generation/restore.
    E2ee {
        #[command(subcommand)]
        action: E2eeAction,
    },
    /// Multi-account session store operations (mission 0850h-d).
    /// `login` writes its output to this store when `--store` is
    /// passed; `session list` / `use` / `remove` / `import` operate
    /// on the same store.
    Session {
        #[command(subcommand)]
        action: SessionAction,
    },
    /// Print version and exit.
    Version,
}

#[derive(Subcommand, Debug)]
pub enum E2eeAction {
    /// Generate cross-signing keys and upload to the homeserver.
    /// Idempotent: no-op if the user already has cross-signing set up.
    Bootstrap(E2eeBootstrapArgs),
    /// Interactively verify a paired device via emoji-SAS. The
    /// operator runs this on the new device while the second device
    /// (phone, browser) sends the verification request.
    Verify(E2eeVerifyArgs),
    /// 4S recovery-key operations.
    Recovery {
        #[command(subcommand)]
        action: RecoveryAction,
    },
    /// Out-of-band verification of an already-logged-in session
    /// (rotated/refreshed device). UX equivalent to Element's
    /// "Verify this device".
    VerifySession(E2eeVerifySessionArgs),
}

#[derive(Subcommand, Debug)]
pub enum RecoveryAction {
    /// Generate a fresh 4S recovery key and write it to a file
    /// (mode 0600). WARNING: this invalidates any previously issued
    /// key — existing encrypted history backed up under the old key
    /// will become unreadable.
    Generate(E2eeRecoveryGenerateArgs),
    /// Restore from a 4S key read on stdin. The key is read into a
    /// zeroed buffer on drop and is never logged or echoed.
    Restore(E2eeRecoveryRestoreArgs),
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
    /// Overwrite existing output file. R1-L2: `--force` is
    /// meaningful only when writing to a file. clap's
    /// `requires = "out"` makes the combination `--force
    /// --stdout` a parse error (the operator sees a clear
    /// "the following required arguments were not provided"
    /// message) rather than silently accepting `--force` and
    /// ignoring it.
    #[arg(long, requires = "out")]
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
    /// Read password from stdin. The clap-level rejection of the
    /// `--password <value>` form is a side effect of the flag being
    /// boolean (`ArgAction::SetTrue`): clap fails with "unexpected
    /// argument" because `--password <value>` is parsed as a value-
    /// taking flag, not the bool. The actual security guarantee is
    /// that the password is never accepted on the command line at
    /// all (no shell-history leak), and never logged.
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
    /// Path to a config file written by `login`. Used as the source
    /// of credentials unless `--store` is also set, in which case
    /// the store is queried by `(user_id, device_id)` and the
    /// file is treated as a metadata-only fallback for the
    /// device-id check.
    #[arg(long)]
    pub config: PathBuf,
    /// Optional path to the multi-account stoolap session store.
    /// When set, credentials are loaded from the store row matching
    /// the config's `(user_id, device_id)`. This is the 0850h-d
    /// multi-account path; without it, whoami reads the file
    /// directly (0850h-a / 0850h-c legacy behavior).
    #[arg(long)]
    pub store: Option<PathBuf>,
}

/// Shared flags for every E2EE subcommand.
#[derive(Args, Debug, Clone)]
pub struct E2eeConfigArgs {
    /// Path to a config file written by `octo-matrix-onboard login`.
    #[arg(long)]
    pub config: PathBuf,
}

#[derive(Args, Debug)]
pub struct E2eeBootstrapArgs {
    #[command(flatten)]
    pub base: E2eeConfigArgs,
    /// Suppress the (slow) progress messages from the SDK's
    /// bootstrap. The first-time bootstrap may take 30+ seconds
    /// while the SDK generates Olm keys; this flag silences the
    /// informational output.
    #[arg(long)]
    pub quiet: bool,
}

#[derive(Args, Debug)]
pub struct E2eeVerifyArgs {
    #[command(flatten)]
    pub base: E2eeConfigArgs,
    /// The user ID of the device we're verifying against (e.g.
    /// `@alice:example.com` for self-verification across our own
    /// devices, or another user's ID for cross-user verification).
    #[arg(long)]
    pub user_id: String,
    /// Flow ID of the verification request received on the second
    /// device. The second device must initiate the request and the
    /// operator pastes the flow ID here.
    #[arg(long)]
    pub flow_id: String,
}

#[derive(Args, Debug)]
pub struct E2eeRecoveryGenerateArgs {
    #[command(flatten)]
    pub base: E2eeConfigArgs,
    /// File to write the recovery key to (mode 0600). The key is
    /// 16 space-separated base64 groups (4S spec). This is the
    /// ONLY copy — losing the file means losing access to encrypted
    /// history.
    #[arg(long)]
    pub out: PathBuf,
    /// Overwrite an existing recovery-key file. By default the
    /// command refuses to overwrite.
    #[arg(long)]
    pub force: bool,
}

#[derive(Args, Debug)]
pub struct E2eeRecoveryRestoreArgs {
    #[command(flatten)]
    pub base: E2eeConfigArgs,
}

#[derive(Args, Debug)]
pub struct E2eeVerifySessionArgs {
    #[command(flatten)]
    pub base: E2eeConfigArgs,
    /// User ID of the device being verified (defaults to ourselves,
    /// i.e. the device this CLI is logged in as).
    #[arg(long)]
    pub user_id: Option<String>,
}

#[derive(Subcommand, Debug)]
pub enum SessionAction {
    /// List all sessions in the multi-account store, ordered by
    /// insertion position. Each row prints user_id, device_id,
    /// homeserver, login type, and a redacted token preview.
    List(SessionListArgs),
    /// Mark a session as the most-recently-used. Updates
    /// `last_used` to the current epoch seconds; does NOT change
    /// `position` (chronological multi-account ordering is
    /// preserved).
    Use(SessionUseArgs),
    /// Remove a session. Refuses to remove when the row is missing.
    Remove(SessionRemoveArgs),
    /// Import a legacy 0850h-a / 0850h-c JSON config into the store.
    /// Refuses to overwrite an existing `(user_id, device_id)` row
    /// unless `--force` is set.
    Import(SessionImportArgs),
}

#[derive(Args, Debug, Clone)]
pub struct SessionStoreArgs {
    /// Path to the multi-account stoolap store. Defaults to the
    /// per-platform `ProjectDirs("com", "cipherocto", "cipherocto")
    /// / data_dir() / sessions.db` location.
    #[arg(long)]
    pub store: Option<PathBuf>,
}

#[derive(Args, Debug)]
pub struct SessionListArgs {
    #[command(flatten)]
    pub store: SessionStoreArgs,
}

#[derive(Args, Debug)]
pub struct SessionUseArgs {
    #[command(flatten)]
    pub store: SessionStoreArgs,
    /// The Matrix user ID of the session to mark as latest.
    pub user_id: String,
    /// The device ID of the session to mark as latest.
    pub device_id: String,
}

#[derive(Args, Debug)]
pub struct SessionRemoveArgs {
    #[command(flatten)]
    pub store: SessionStoreArgs,
    /// The Matrix user ID of the session to remove.
    pub user_id: String,
    /// The device ID of the session to remove.
    pub device_id: String,
}

#[derive(Args, Debug)]
pub struct SessionImportArgs {
    #[command(flatten)]
    pub store: SessionStoreArgs,
    /// Path to the legacy JSON config to import.
    pub file: PathBuf,
    /// Overwrite an existing row with the same `(user_id, device_id)`.
    #[arg(long)]
    pub force: bool,
}
