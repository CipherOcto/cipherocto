//! CLI argument parsing for `octo-telegram-mtproto-onboard`.
//!
//! Mirrors the TDLib `octo-telegram-onboard` crate's `cli`
//! module shape (top-level `Cli` with subcommands), with the
//! differences that:
//!
//! * the auth subcommands are renamed for clarity:
//!   `bot-token`, `user-code`, `qr-login` (vs the TDLib
//!   version's `bot-setup`, `user-login`, `qr-link`).
//! * the `--mode`/`-m` global flag controls verbose tracing
//!   level (default: 0 = info).

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

/// Top-level CLI.
#[derive(Debug, Parser)]
#[command(
    name = "octo-telegram-mtproto-onboard",
    version,
    about = "Authenticate a CipherOcto operator against Telegram via the pure-Rust MTProto adapter."
)]
pub struct Cli {
    /// Verbosity level: 0 = info (default), 1 = debug, 2+ = trace.
    #[arg(short, long, default_value_t = 0, global = true)]
    pub verbose: u8,

    #[command(subcommand)]
    pub command: Command,
}

/// Subcommands.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Authenticate using a Telegram bot token
    /// (`<bot_id>:<auth>`). No interactive prompts.
    BotToken(BotTokenArgs),
    /// Authenticate as a user: phone + SMS code (+ optional
    /// 2FA password). Prompts on stdin.
    UserCode(UserCodeArgs),
    /// Authenticate via QR login: operator scans a
    /// `tg://login?token=...` URL from another already-
    /// logged-in Telegram device. No phone, no SMS.
    QrLogin(QrLoginArgs),
    /// Read an existing session file and print the cached
    /// `self_id` / `username`. Does not contact Telegram.
    Whoami(WhoamiArgs),
    /// Print the binary version and exit.
    Version,
}

/// `bot-token` subcommand.
#[derive(Debug, Args)]
pub struct BotTokenArgs {
    /// Bot token (e.g. `123456789:AAEhBOweik6ad9JQBxxx`).
    /// If omitted, reads from stdin (one line).
    #[arg(long)]
    pub bot_token: Option<String>,

    /// `my.telegram.org` API id. If omitted, read from
    /// `--api-id-file` or the `TELEGRAM_API_ID` env var.
    #[arg(long)]
    pub api_id: Option<i32>,

    /// `my.telegram.org` API hash. If omitted, read from
    /// `--api-hash-file` or the `TELEGRAM_API_HASH` env var.
    #[arg(long)]
    pub api_hash: Option<String>,

    /// Directory where the session file and config will be
    /// written. Defaults to the first existing value of
    /// `--data-dir`, the `TELEGRAM_DATA_DIR` env var, or the
    /// OS-conventional location (see `default_data_dir`).
    #[arg(long)]
    pub data_dir: Option<PathBuf>,

    /// Where to write the JSON `OnboardOutput`. If omitted,
    /// prints to stdout.
    #[arg(long)]
    pub output: Option<PathBuf>,
}

/// `user-code` subcommand.
#[derive(Debug, Args)]
pub struct UserCodeArgs {
    /// E.164 phone number (e.g. `+15551234567`). If
    /// omitted, reads from stdin.
    #[arg(long)]
    pub phone: Option<String>,

    /// `my.telegram.org` API id.
    #[arg(long)]
    pub api_id: Option<i32>,

    /// `my.telegram.org` API hash.
    #[arg(long)]
    pub api_hash: Option<String>,

    /// On-disk data dir.
    #[arg(long)]
    pub data_dir: Option<PathBuf>,

    /// Where to write the JSON `OnboardOutput`. If omitted,
    /// prints to stdout.
    #[arg(long)]
    pub output: Option<PathBuf>,

    /// Read the SMS code from a file (test-friendly).
    /// Mutually exclusive with the stdin prompt.
    #[arg(long)]
    pub code_file: Option<PathBuf>,

    /// Read the 2FA password from a file (test-friendly).
    /// Mutually exclusive with the stdin prompt.
    #[arg(long)]
    pub password_file: Option<PathBuf>,
}

/// `qr-login` subcommand.
#[derive(Debug, Args)]
pub struct QrLoginArgs {
    /// `my.telegram.org` API id.
    #[arg(long)]
    pub api_id: Option<i32>,

    /// `my.telegram.org` API hash.
    #[arg(long)]
    pub api_hash: Option<String>,

    /// On-disk data dir.
    #[arg(long)]
    pub data_dir: Option<PathBuf>,

    /// Where to write the JSON `OnboardOutput`. If omitted,
    /// prints to stdout.
    #[arg(long)]
    pub output: Option<PathBuf>,

    /// Maximum time to wait for the operator to scan the
    /// QR code, in seconds. Default 300 (5 min).
    #[arg(long, default_value_t = 300)]
    pub timeout_secs: u64,

    /// Poll interval in seconds. Default 2.
    #[arg(long, default_value_t = 2)]
    pub poll_interval_secs: u64,

    /// Render the QR code as ASCII to stdout instead of
    /// pretty-printing the URL. Requires the
    /// `qr2term` feature (not enabled by default in
    /// Phase B).
    #[arg(long)]
    pub render_qr_ascii: bool,
}

/// `whoami` subcommand.
#[derive(Debug, Args)]
pub struct WhoamiArgs {
    /// On-disk data dir to read the session from. Defaults
    /// to the OS-conventional location.
    #[arg(long)]
    pub data_dir: Option<PathBuf>,

    /// Where to write the JSON `OnboardOutput`. If omitted,
    /// prints to stdout.
    #[arg(long)]
    pub output: Option<PathBuf>,
}

/// Default data dir: `<ProjectDirs::data_dir()>/telegram-mtproto`.
///
/// Falls back to `./.octo/telegram-mtproto` if the platform
/// does not provide a `ProjectDirs` (e.g. some test
/// environments). Operators can override with `--data-dir`
/// or the `TELEGRAM_DATA_DIR` env var.
pub fn default_data_dir() -> PathBuf {
    if let Some(d) = directories::ProjectDirs::from("io", "cipherocto", "octo") {
        return d.data_dir().join("telegram-mtproto");
    }
    PathBuf::from(".octo/telegram-mtproto")
}

/// Resolve the data dir, applying precedence: explicit flag,
/// env var, default.
pub fn resolve_data_dir(flag: Option<PathBuf>) -> PathBuf {
    if let Some(d) = flag {
        return d;
    }
    if let Ok(d) = std::env::var("TELEGRAM_DATA_DIR") {
        return PathBuf::from(d);
    }
    default_data_dir()
}

/// Resolve the API id, applying precedence: explicit flag,
/// env var, error.
pub fn resolve_api_id(flag: Option<i32>) -> Result<i32, String> {
    if let Some(id) = flag {
        return Ok(id);
    }
    if let Ok(s) = std::env::var("TELEGRAM_API_ID") {
        return s.parse().map_err(|e: std::num::ParseIntError| {
            format!("TELEGRAM_API_ID='{}' is not an i32: {}", s, e)
        });
    }
    Err("TELEGRAM_API_ID not set (use --api-id or env var)".to_string())
}

/// Resolve the API hash, applying precedence: explicit flag,
/// env var, error.
pub fn resolve_api_hash(flag: Option<String>) -> Result<String, String> {
    if let Some(h) = flag {
        if !h.is_empty() {
            return Ok(h);
        }
    }
    if let Ok(s) = std::env::var("TELEGRAM_API_HASH") {
        if !s.is_empty() {
            return Ok(s);
        }
    }
    Err("TELEGRAM_API_HASH not set (use --api-hash or env var)".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    // R26-S3: env-var tests MUST run serially — `cargo test`
    // runs tests in parallel by default, and
    // `std::env::set_var` / `remove_var` mutate a process-
    // global table. Without this lock, two parallel tests
    // would race on the same variable and assert flaky
    // outcomes. The previous code claimed "single-threaded
    // test process" via an `unsafe { ... }` block, which is
    // incorrect on a parallel test runner. `serial_test`
    // would also work but adds a dependency we don't need.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    /// Run a closure with `var` set to `value` (and restored
    /// on drop). Holds `ENV_LOCK` for the duration so
    /// concurrent env-mutating tests cannot race.
    fn with_env<F: FnOnce()>(var: &str, value: &str, f: F) {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        // SAFETY: we hold ENV_LOCK so no other test in this
        // binary is touching env vars concurrently. We
        // restore the prior value on scope exit (drop).
        let prior = std::env::var(var).ok();
        // SAFETY: see above.
        unsafe {
            std::env::set_var(var, value);
        }
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
        // SAFETY: see above.
        unsafe {
            match prior {
                Some(v) => std::env::set_var(var, v),
                None => std::env::remove_var(var),
            }
        }
        if let Err(panic) = result {
            std::panic::resume_unwind(panic);
        }
    }

    /// Run a closure with `var` removed. Holds `ENV_LOCK`
    /// and restores the prior value on scope exit.
    fn without_env<F: FnOnce()>(var: &str, f: F) {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let prior = std::env::var(var).ok();
        // SAFETY: see `with_env`.
        unsafe {
            std::env::remove_var(var);
        }
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
        // SAFETY: see `with_env`.
        unsafe {
            match prior {
                Some(v) => std::env::set_var(var, v),
                None => std::env::remove_var(var),
            }
        }
        if let Err(panic) = result {
            std::panic::resume_unwind(panic);
        }
    }

    #[test]
    fn default_data_dir_is_nonempty() {
        let d = default_data_dir();
        assert!(!d.as_os_str().is_empty());
    }

    #[test]
    fn resolve_data_dir_prefers_flag() {
        let explicit = PathBuf::from("/tmp/explicit");
        let resolved = resolve_data_dir(Some(explicit.clone()));
        assert_eq!(resolved, explicit);
    }

    #[test]
    fn resolve_data_dir_falls_back_to_default() {
        without_env("TELEGRAM_DATA_DIR", || {
            let resolved = resolve_data_dir(None);
            assert!(!resolved.as_os_str().is_empty());
        });
    }

    #[test]
    fn resolve_api_id_prefers_flag() {
        assert_eq!(resolve_api_id(Some(42)).unwrap(), 42);
    }

    #[test]
    fn resolve_api_id_parses_env_var() {
        with_env("TELEGRAM_API_ID", "99999", || {
            let id = resolve_api_id(None).unwrap();
            assert_eq!(id, 99999);
        });
    }

    #[test]
    fn resolve_api_id_rejects_non_numeric_env_var() {
        with_env("TELEGRAM_API_ID", "not-a-number", || {
            let e = resolve_api_id(None).unwrap_err();
            assert!(e.contains("not-a-number"));
        });
    }

    #[test]
    fn resolve_api_hash_prefers_flag() {
        assert_eq!(
            resolve_api_hash(Some("flag-hash".to_string())).unwrap(),
            "flag-hash"
        );
    }

    #[test]
    fn resolve_api_hash_rejects_empty_flag_and_unset_env() {
        without_env("TELEGRAM_API_HASH", || {
            let e = resolve_api_hash(None).unwrap_err();
            assert!(e.contains("TELEGRAM_API_HASH"));
        });
    }
}
