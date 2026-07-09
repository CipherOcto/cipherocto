//! CLI surface — clap derive arg structs.
//!
//! Mission AC §Binary structure. R1-M3: `PairLinkArgs::custom_code`
//! (not `custom_pair_code`). R2-H1: `OutputArgs` is a real type.
//! R2-L2: `--groups` parsing documented. R3-M2: `parse_groups`
//! value parser.

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(
    name = "octo-whatsapp-onboard",
    version,
    about = "Authenticate against WhatsApp Web and write a JSON config for octo-adapter-whatsapp.",
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
    /// Render QR code in terminal, wait for phone scan.
    QrLink(QrLinkArgs),
    /// Issue a 6-character pair code via the WhatsApp Web protocol.
    PairLink(PairLinkArgs),
    /// Verify existing session.
    Whoami(WhoamiArgs),
    /// Drive the caBLE v2 tunnel as the CLI (emulating WA Web
    /// Browser — the FIDO QR publisher). Generates a fresh
    /// HandshakeV2 + P-256 keypair, renders the FIDO QR to
    /// stderr for the operator to scan with the phone's Google
    /// Lens, then connects to `wss://cable.ua5v.com` and waits
    /// for the phone's signed assertion. Uses the WebAuthn JSON
    /// from `--options-file` (or a built-in mirror of the WA
    /// Web bot-verification payload). Prints the resulting
    /// PublicKeyCredential JSON on success.
    CompanionLink(CompanionLinkArgs),
    /// Multi-account session store operations.
    Session {
        #[command(subcommand)]
        action: SessionAction,
    },
    /// Print version and exit.
    Version,
}

#[derive(Subcommand, Debug)]
pub enum SessionAction {
    /// List known session databases.
    List(SessionListArgs),
    /// Check if a specific session DB has a valid Signal session.
    Verify(SessionVerifyArgs),
    /// Delete a session DB (with confirmation).
    Remove(SessionRemoveArgs),
}

/// Common output flags shared by every link mode.
#[derive(Args, Debug, Clone)]
pub struct OutputArgs {
    /// Output file path (default: ~/.config/octo/whatsapp.json on Unix).
    #[arg(long, conflicts_with = "stdout")]
    pub out: Option<PathBuf>,
    /// Write JSON to stdout instead of a file.
    #[arg(long)]
    pub stdout: bool,
    /// Overwrite existing output file.
    #[arg(long)]
    pub force: bool,
}

/// Custom value parser for `--groups` (R2-L2 + R3-M2):
/// comma-separated; whitespace trimmed; empty entries rejected;
/// duplicates NOT deduplicated. Empty input returns an empty Vec
/// (R2-L2: `--groups ""` is the default for "no groups", not an error).
fn parse_groups(s: &str) -> std::result::Result<Vec<String>, String> {
    if s.is_empty() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for entry in s.split(',') {
        let trimmed = entry.trim();
        if trimmed.is_empty() {
            return Err(format!("empty group entry in {s:?}"));
        }
        out.push(trimmed.to_string());
    }
    Ok(out)
}

/// Default session path: $OCTO_WHATSAPP_SESSION_PATH or
/// ~/.local/share/octo/whatsapp/default.session.db
fn default_session_path() -> PathBuf {
    if let Ok(p) = std::env::var("OCTO_WHATSAPP_SESSION_PATH") {
        return PathBuf::from(p);
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home)
        .join(".local")
        .join("share")
        .join("octo")
        .join("whatsapp")
        .join("default.session.db")
}

#[derive(Args, Debug)]
pub struct QrLinkArgs {
    /// Path to stoolap session database (default: ~/.local/share/octo/whatsapp/default.session.db).
    #[arg(long, default_value_os_t = default_session_path())]
    pub session_path: PathBuf,
    /// Initial group IDs to monitor (comma-separated, accepts digits-only
    /// or full JID like `120363012345678901@g.us`).
    /// Default: no groups (empty list).
    /// R9: the previous `default_value = ""` caused a clap
    /// downcast panic. clap 4.x requires a `ToString`-implementing
    /// type for `default_value_t` on `Vec<T>`, which `Vec<String>`
    /// does not provide. The fix is to omit `default_value_t` and
    /// rely on clap's default for `Vec<T>` (which is `vec![]`).
    /// `parse_groups` handles the empty-input case (R2-L2).
    #[arg(long, value_parser = parse_groups)]
    pub groups: Vec<String>,
    /// WebSocket URL override (test/proxy). Or $OCTO_WHATSAPP_WS_URL.
    #[arg(long)]
    pub ws_url: Option<String>,
    #[command(flatten)]
    pub output: OutputArgs,
    /// Timeout in seconds (default: 300, how long to wait for Event::Connected).
    #[arg(long, default_value_t = 300)]
    pub timeout: u64,
    /// Wait for the initial history sync (OfflineSyncCompleted) before
    /// exiting. The server expects the client to be fully synchronized
    /// before performing operations like creating groups.
    #[arg(long)]
    pub wait_sync: bool,
    /// Snapshot the existing session DB (and meta sidecar) to
    /// `<path>.broken-<unix-timestamp>` siblings, then proceed with a
    /// fresh pair. Use to recover from `Event::LoggedOut` on the same
    /// phone number — the server rejects retries from a device whose
    /// DB still represents the logged-out identity. A no-op if no
    /// existing session is present.
    #[arg(long)]
    pub reset: bool,
    // R1-M3: per-subcommand --verbose removed; use the global -v/--verbose.
}

#[derive(Args, Debug)]
pub struct PairLinkArgs {
    #[arg(long, default_value_os_t = default_session_path())]
    pub session_path: PathBuf,
    /// Phone number in E.164 (e.g., +15551234567). Or $OCTO_WHATSAPP_PHONE.
    #[arg(long)]
    pub phone: String,
    /// Custom pair code (operator-chosen). Or $OCTO_WHATSAPP_PAIR_CODE.
    #[arg(long)]
    pub pair_code: Option<String>,
    /// Initial group IDs to monitor (comma-separated, accepts digits-only
    /// or full JID). Default: no groups (empty list).
    /// R9: omit `default_value_t` (same rationale as qr-link).
    #[arg(long, value_parser = parse_groups)]
    pub groups: Vec<String>,
    #[arg(long)]
    pub ws_url: Option<String>,
    #[command(flatten)]
    pub output: OutputArgs,
    #[arg(long, default_value_t = 300)]
    pub timeout: u64,
    /// Mission 0850p-a-ci-mode-pair-link: bypass `Event::Connected`
    /// wait; load a pre-paired session DB from `--session-path` and
    /// exit 0 if the session is valid. For CI/CD deployments where
    /// the phone is not available.
    #[arg(long)]
    pub ci: bool,
    /// Snapshot the existing session DB (and meta sidecar) to
    /// `<path>.broken-<unix-timestamp>` siblings, then proceed with a
    /// fresh pair. Use to recover from `Event::LoggedOut` on the same
    /// phone number. Ignored when `--ci` is set (CI loads a
    /// pre-paired DB; no reset applies). A no-op if no existing
    /// session is present.
    #[arg(long)]
    pub reset: bool,
    // R1-M3: per-subcommand --verbose removed; use the global -v/--verbose.
}

#[derive(Args, Debug)]
pub struct WhoamiArgs {
    /// Path to a config file written by `qr-link` or `pair-link`.
    #[arg(long)]
    pub config: PathBuf,
    /// Mission 0850p-a-replaced-state: if set, the CLI exits with
    /// code 8 on `BotState::Replaced`, 7 on `BotState::SessionExpired`,
    /// 2 on other `LoggedOut`. Used by CI/CD to trigger re-pair
    /// automation on detected replacement.
    #[arg(long)]
    pub detect_replacement: bool,
}

#[derive(Args, Debug)]
pub struct SessionListArgs {
    /// Base directory to scan (default: ~/.local/share/octo/whatsapp/).
    #[arg(long)]
    pub base_dir: Option<PathBuf>,
}

#[derive(Args, Debug)]
pub struct SessionVerifyArgs {
    /// Path to the session DB to verify.
    pub db_path: PathBuf,
}

#[derive(Args, Debug)]
pub struct SessionRemoveArgs {
    /// Path to the session DB to remove.
    pub db_path: PathBuf,
    /// Skip the interactive confirmation (for CI).
    #[arg(long)]
    pub yes: bool,
}

/// Session 10 — CLI emulates WA Web Browser for the FIDO / caBLE
/// passkey step. Generates a fresh HandshakeV2 (P-256 static
/// keypair + random 16-byte secret), renders the FIDO QR to
/// stderr (Unicode block characters) for the operator to scan
/// with the phone's Google Lens, then drives the full Noise-
/// over-WebSocket handshake against `wss://cable.ua5v.com` and
/// receives the signed assertion over the encrypted tunnel. On
/// success prints the resulting PublicKeyCredential JSON.
///
/// Operator flow:
///   1. Phone: WA → Settings → Linked Devices → Link a Device
///   2. Run this command on the laptop.
///   3. A QR appears on stderr. Scan it with Google Lens.
///   4. The phone asserts via its passkey; we receive + print.
#[derive(Args, Debug)]
pub struct CompanionLinkArgs {
    /// Path to a file containing the WebAuthn
    /// `PublicKeyCredentialRequestOptions` JSON wacore would have
    /// emitted via `Event::PairPasskeyRequest.request_options_json`.
    /// If omitted, a built-in mirror of the WA Web bot-verification
    /// payload is used (rpId=whatsapp.com, 32B challenge, no
    /// allowCredentials, uvm extension).
    #[arg(long)]
    pub options_file: Option<PathBuf>,
    /// Timeout in seconds for the full QR-display + handshake +
    /// assertion round-trip (default: 120 — generous for the
    /// operator to scan with the phone).
    #[arg(long, default_value_t = 120)]
    pub timeout: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_groups_comma_separated() {
        assert_eq!(parse_groups("a,b,c").unwrap(), vec!["a", "b", "c"]);
    }

    #[test]
    fn parse_groups_trims_whitespace() {
        assert_eq!(parse_groups("a, b, c").unwrap(), vec!["a", "b", "c"]);
    }

    #[test]
    fn parse_groups_rejects_empty_entry() {
        assert!(parse_groups("a,,b").is_err());
    }

    #[test]
    fn parse_groups_preserves_duplicates() {
        // R2-L2: duplicates NOT deduplicated
        assert_eq!(parse_groups("a,a,b").unwrap(), vec!["a", "a", "b"]);
    }

    #[test]
    fn parse_groups_empty_string_returns_empty_vec() {
        assert_eq!(parse_groups("").unwrap(), Vec::<String>::new());
    }
}
