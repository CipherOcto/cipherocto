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
    /// Overwrite existing output file. Only meaningful with `--out`.
    #[arg(long, requires = "out")]
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

#[derive(Args, Debug)]
pub struct QrLinkArgs {
    /// Path to stoolap session database (default: ~/.local/share/octo/whatsapp/default.session.db).
    #[arg(long)]
    pub session_path: PathBuf,
    /// Initial group IDs to monitor (comma-separated, accepts digits-only
    /// or full JID like `120363012345678901@g.us`).
    /// Default: no groups (empty list).
    #[arg(long, value_parser = parse_groups, default_value = "")]
    pub groups: Vec<String>,
    /// WebSocket URL override (test/proxy). Or $OCTO_WHATSAPP_WS_URL.
    #[arg(long)]
    pub ws_url: Option<String>,
    #[command(flatten)]
    pub output: OutputArgs,
    /// Timeout in seconds (default: 300, how long to wait for Event::Connected).
    #[arg(long, default_value_t = 300)]
    pub timeout: u64,
}

#[derive(Args, Debug)]
pub struct PairLinkArgs {
    #[arg(long)]
    pub session_path: PathBuf,
    /// Phone number in E.164 (e.g., +15551234567). Or $OCTO_WHATSAPP_PHONE.
    #[arg(long)]
    pub phone: String,
    /// Custom pair code (operator-chosen). Or $OCTO_WHATSAPP_PAIR_CODE.
    #[arg(long)]
    pub pair_code: Option<String>,
    /// Initial group IDs to monitor (comma-separated, accepts digits-only
    /// or full JID). Default: no groups (empty list).
    #[arg(long, value_parser = parse_groups, default_value = "")]
    pub groups: Vec<String>,
    #[arg(long)]
    pub ws_url: Option<String>,
    #[command(flatten)]
    pub output: OutputArgs,
    #[arg(long, default_value_t = 300)]
    pub timeout: u64,
    // R1-M3: per-subcommand --verbose removed; use the global -v/--verbose.
}

#[derive(Args, Debug)]
pub struct WhoamiArgs {
    /// Path to a config file written by `qr-link` or `pair-link`.
    #[arg(long)]
    pub config: PathBuf,
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
