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

use std::path::{Path, PathBuf};

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

    /// Read the API id from a file (one line, trimmed).
    /// R2-ARCH-5 / R2-OPS-6: the round-1 docs claimed
    /// `--api-id-file` was supported but the flag was
    /// never declared, so `clap` rejected it. The fix
    /// adds the flag plus a corresponding
    /// `--api-hash-file`. Precedence: explicit
    /// `--api-id` / `--api-hash` flag → `--api-id-file`
    /// / `--api-hash-file` → `TELEGRAM_API_ID` /
    /// `TELEGRAM_API_HASH` env var. File-mode is
    /// preferred over env vars for systemd / k8s
    /// `Secret` mounts.
    #[arg(long)]
    pub api_id_file: Option<PathBuf>,

    /// Read the API hash from a file (one line, trimmed).
    /// See `--api-id-file` for precedence and rationale.
    #[arg(long)]
    pub api_hash_file: Option<PathBuf>,

    /// Directory where the session file and config will be
    /// written. Defaults to the first existing value of
    /// `--data-dir`, the `TELEGRAM_DATA_DIR` env var, or the
    /// OS-conventional location (see `default_data_dir`).
    #[arg(long)]
    pub data_dir: Option<PathBuf>,

    /// Where to write the JSON `OnboardOutput`. If omitted,
    /// prints to stdout.
    ///
    /// R2-OPS-15: the JSON schema is documented in
    /// `octo_telegram_mtproto_onboard_core::OnboardOutput`.
    /// At the time of writing the schema is:
    /// `{ "schema_version": 1, "mode": "bot_token", "self_id":
    /// <i64>, "self_username": <string|null>, "is_bot":
    /// <bool>, "data_dir": <string>, "config_path":
    /// <string>, "session_path": <string>, "elapsed_ms":
    /// <i64> }`. The file is created with `0o600` (operator-
    /// only) per R2-IE-15.
    #[arg(
        long,
        long_help = "Path to write the JSON OnboardOutput. Schema: { schema_version: 1, mode, self_id, self_username, is_bot, data_dir, config_path, session_path, elapsed_ms }. Defaults to stdout. The file is created with mode 0o600 (operator-only)."
    )]
    pub output: Option<PathBuf>,

    /// Overwrite `<data_dir>/config.json` if it already
    /// exists. The default is to refuse (safer for
    /// automation). R2-ARCH-22: the round-1 review
    /// observed that the CLI had no `--force` flag, so a
    /// re-onboard always failed with a confusing "file
    /// exists" error.
    #[arg(
        long,
        long_help = "Overwrite <data_dir>/config.json if it already exists. Default: refuse to overwrite."
    )]
    pub force: bool,
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

    /// R2-ARCH-5 / R2-OPS-6: read the API id from a file.
    /// See `BotTokenArgs::api_id_file` for precedence and
    /// rationale.
    #[arg(long)]
    pub api_id_file: Option<PathBuf>,

    /// R2-ARCH-5 / R2-OPS-6: read the API hash from a file.
    /// See `BotTokenArgs::api_id_file` for precedence and
    /// rationale.
    #[arg(long)]
    pub api_hash_file: Option<PathBuf>,

    /// On-disk data dir.
    #[arg(long)]
    pub data_dir: Option<PathBuf>,

    /// Where to write the JSON `OnboardOutput`. If omitted,
    /// prints to stdout. See `BotTokenArgs::output` for the
    /// schema (R2-OPS-15).
    #[arg(
        long,
        long_help = "Path to write the JSON OnboardOutput. See BotTokenArgs::output for the schema. Defaults to stdout. The file is created with mode 0o600 (operator-only)."
    )]
    pub output: Option<PathBuf>,

    /// Read the SMS code from a file (test-friendly).
    /// Mutually exclusive with the stdin prompt.
    #[arg(long)]
    pub code_file: Option<PathBuf>,

    /// Read the 2FA password from a file (test-friendly).
    /// Mutually exclusive with the stdin prompt.
    #[arg(long)]
    pub password_file: Option<PathBuf>,

    /// R2-OPS-12: how long to wait for the operator to
    /// type the SMS code, in seconds. Default 60.
    #[arg(
        long,
        default_value_t = 60,
        long_help = "How long to wait for the SMS code, in seconds. Default 60. R2-OPS-12."
    )]
    pub code_timeout_secs: u64,

    /// R2-OPS-12: how long to wait for the 2FA password,
    /// in seconds. Default 60.
    #[arg(
        long,
        default_value_t = 60,
        long_help = "How long to wait for the 2FA password, in seconds. Default 60. R2-OPS-12."
    )]
    pub password_timeout_secs: u64,

    /// Overwrite `<data_dir>/config.json` if it already
    /// exists. See `BotTokenArgs::force` (R2-ARCH-22).
    #[arg(
        long,
        long_help = "Overwrite <data_dir>/config.json if it already exists. Default: refuse to overwrite. R2-ARCH-22."
    )]
    pub force: bool,
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

    /// R2-ARCH-5 / R2-OPS-6: read the API id from a file.
    /// See `BotTokenArgs::api_id_file` for precedence and
    /// rationale.
    #[arg(long)]
    pub api_id_file: Option<PathBuf>,

    /// R2-ARCH-5 / R2-OPS-6: read the API hash from a file.
    /// See `BotTokenArgs::api_id_file` for precedence and
    /// rationale.
    #[arg(long)]
    pub api_hash_file: Option<PathBuf>,

    /// On-disk data dir.
    #[arg(long)]
    pub data_dir: Option<PathBuf>,

    /// Where to write the JSON `OnboardOutput`. If omitted,
    /// prints to stdout. See `BotTokenArgs::output` for the
    /// schema (R2-OPS-15).
    #[arg(
        long,
        long_help = "Path to write the JSON OnboardOutput. See BotTokenArgs::output for the schema. Defaults to stdout. The file is created with mode 0o600 (operator-only)."
    )]
    pub output: Option<PathBuf>,

    /// Maximum time to wait for the operator to scan the
    /// QR code, in seconds. Default 300 (5 min). R2-IE-17:
    /// must be > 0 (the core floors at 1s).
    #[arg(
        long,
        default_value_t = 300,
        long_help = "Maximum time to wait for the QR scan, in seconds. Default 300. Must be > 0 (R2-IE-17)."
    )]
    pub timeout_secs: u64,

    /// Poll interval in seconds. Default 2. R2-IE-17: must
    /// be > 0 (the core floors at 100ms).
    #[arg(
        long,
        default_value_t = 2,
        long_help = "QR poll interval, in seconds. Default 2. Must be > 0 (R2-IE-17)."
    )]
    pub poll_interval_secs: u64,

    /// Render the QR code as ASCII to stdout instead of
    /// pretty-printing the URL. Requires the
    /// `qr2term` feature (not enabled by default in
    /// Phase B).
    #[arg(long)]
    pub render_qr_ascii: bool,

    /// Overwrite `<data_dir>/config.json` if it already
    /// exists. See `BotTokenArgs::force` (R2-ARCH-22).
    #[arg(
        long,
        long_help = "Overwrite <data_dir>/config.json if it already exists. Default: refuse to overwrite. R2-ARCH-22."
    )]
    pub force: bool,
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
///
/// IE-6 (R26): the prior qualifier (`"io", "cipherocto",
/// "octo"`) collided with the workspace-level namespace
/// used by other cipherocto tools. On Linux this resolved
/// to `$XDG_DATA_HOME/cipherocto/octo/telegram-mtproto`,
/// which is also the parent of any future
/// `io.cipherocto.octo.*` app — a future "octo-admin"
/// tool would land at `…/cipherocto/octo/admin/` and
/// would share the same data root as the Telegram
/// onboard tool. Use a per-app qualifier
/// (`"io", "cipherocto", "octo-telegram-mtproto"`) so the
/// data root is unique to this binary. The fall-back
/// `.octo/telegram-mtproto` path remains stable so any
/// in-the-wild deployments don't have to migrate.
pub fn default_data_dir() -> PathBuf {
    // IE-6 (R26): the third positional arg of
    // `ProjectDirs::from` is the application name, which
    // becomes the leaf of the data root. Pin it to a
    // unique name to avoid collision with future tools.
    if let Some(d) = directories::ProjectDirs::from("io", "cipherocto", "octo-telegram-mtproto") {
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
///
/// R2-ARCH-5 / R2-OPS-6: also accept a file path
/// (`--api-id-file`). The file contents (trimmed) are
/// parsed as an `i32`. Precedence: explicit
/// `--api-id` flag → `--api-id-file` →
/// `TELEGRAM_API_ID` env var → error.
pub fn resolve_api_id(flag: Option<i32>, file: Option<&Path>) -> Result<i32, String> {
    if let Some(id) = flag {
        return Ok(id);
    }
    if let Some(path) = file {
        let body = std::fs::read_to_string(path)
            .map_err(|e| format!("--api-id-file {}: {}", path.display(), e))?;
        let trimmed = body.trim();
        let id: i32 = trimmed.parse().map_err(|e: std::num::ParseIntError| {
            format!(
                "--api-id-file {}: '{}' is not an i32: {}",
                path.display(),
                trimmed,
                e
            )
        })?;
        return Ok(id);
    }
    if let Ok(s) = std::env::var("TELEGRAM_API_ID") {
        return s.parse().map_err(|e: std::num::ParseIntError| {
            format!("TELEGRAM_API_ID='{}' is not an i32: {}", s, e)
        });
    }
    Err("TELEGRAM_API_ID not set (use --api-id, --api-id-file, or env var)".to_string())
}

/// Resolve the API hash, applying precedence: explicit flag,
/// env var, error.
///
/// R2-ARCH-5 / R2-OPS-6: also accept a file path
/// (`--api-hash-file`). The file contents (trimmed) are
/// used as the hash. Precedence: explicit `--api-hash`
/// flag → `--api-hash-file` → `TELEGRAM_API_HASH` env var
/// → error.
pub fn resolve_api_hash(flag: Option<String>, file: Option<&Path>) -> Result<String, String> {
    if let Some(h) = flag {
        if !h.is_empty() {
            return Ok(h);
        }
    }
    if let Some(path) = file {
        let body = std::fs::read_to_string(path)
            .map_err(|e| format!("--api-hash-file {}: {}", path.display(), e))?;
        let trimmed = body.trim();
        if trimmed.is_empty() {
            return Err(format!("--api-hash-file {}: file is empty", path.display()));
        }
        return Ok(trimmed.to_string());
    }
    if let Ok(s) = std::env::var("TELEGRAM_API_HASH") {
        if !s.is_empty() {
            return Ok(s);
        }
    }
    Err("TELEGRAM_API_HASH not set (use --api-hash, --api-hash-file, or env var)".to_string())
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
        assert_eq!(resolve_api_id(Some(42), None).unwrap(), 42);
    }

    #[test]
    fn resolve_api_id_parses_env_var() {
        with_env("TELEGRAM_API_ID", "99999", || {
            let id = resolve_api_id(None, None).unwrap();
            assert_eq!(id, 99999);
        });
    }

    #[test]
    fn resolve_api_id_rejects_non_numeric_env_var() {
        with_env("TELEGRAM_API_ID", "not-a-number", || {
            let e = resolve_api_id(None, None).unwrap_err();
            assert!(e.contains("not-a-number"));
        });
    }

    /// R2-ARCH-5 / R2-OPS-6: `--api-id-file` is the
    /// third tier of the precedence chain (after the
    /// explicit `--api-id` flag and before the env var).
    /// The file's trimmed contents are parsed as an
    /// `i32`.
    #[test]
    fn resolve_api_id_reads_from_file() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("id");
        std::fs::write(&path, "12345\n").unwrap();
        let id = resolve_api_id(None, Some(&path)).unwrap();
        assert_eq!(id, 12345);
    }

    #[test]
    fn resolve_api_id_file_trims_whitespace() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("id");
        std::fs::write(&path, "  42  \n").unwrap();
        let id = resolve_api_id(None, Some(&path)).unwrap();
        assert_eq!(id, 42);
    }

    #[test]
    fn resolve_api_id_file_rejects_non_numeric() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("id");
        std::fs::write(&path, "not-a-number").unwrap();
        let e = resolve_api_id(None, Some(&path)).unwrap_err();
        assert!(e.contains("not-a-number"));
        assert!(e.contains("--api-id-file"));
    }

    #[test]
    fn resolve_api_id_flag_wins_over_file() {
        // Explicit flag trumps the file.
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("id");
        std::fs::write(&path, "99999").unwrap();
        let id = resolve_api_id(Some(42), Some(&path)).unwrap();
        assert_eq!(id, 42);
    }

    #[test]
    fn resolve_api_id_file_wins_over_env_var() {
        // File trumps env var.
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("id");
        std::fs::write(&path, "12345").unwrap();
        with_env("TELEGRAM_API_ID", "99999", || {
            let id = resolve_api_id(None, Some(&path)).unwrap();
            assert_eq!(id, 12345);
        });
    }

    #[test]
    fn resolve_api_hash_prefers_flag() {
        assert_eq!(
            resolve_api_hash(Some("flag-hash".to_string()), None).unwrap(),
            "flag-hash"
        );
    }

    #[test]
    fn resolve_api_hash_rejects_empty_flag_and_unset_env() {
        without_env("TELEGRAM_API_HASH", || {
            let e = resolve_api_hash(None, None).unwrap_err();
            assert!(e.contains("TELEGRAM_API_HASH"));
        });
    }

    /// R2-ARCH-5 / R2-OPS-6: `--api-hash-file` is the
    /// third tier of the precedence chain. The file's
    /// trimmed contents are used as the hash.
    #[test]
    fn resolve_api_hash_reads_from_file() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("hash");
        std::fs::write(&path, "abc123def456\n").unwrap();
        let h = resolve_api_hash(None, Some(&path)).unwrap();
        assert_eq!(h, "abc123def456");
    }

    #[test]
    fn resolve_api_hash_file_rejects_empty() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("hash");
        std::fs::write(&path, "   \n").unwrap();
        let e = resolve_api_hash(None, Some(&path)).unwrap_err();
        assert!(e.contains("empty"));
    }

    #[test]
    fn resolve_api_hash_flag_wins_over_file() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("hash");
        std::fs::write(&path, "from-file").unwrap();
        let h = resolve_api_hash(Some("from-flag".to_string()), Some(&path)).unwrap();
        assert_eq!(h, "from-flag");
    }

    #[test]
    fn resolve_api_hash_file_wins_over_env_var() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("hash");
        std::fs::write(&path, "from-file").unwrap();
        with_env("TELEGRAM_API_HASH", "from-env", || {
            let h = resolve_api_hash(None, Some(&path)).unwrap();
            assert_eq!(h, "from-file");
        });
    }
}
