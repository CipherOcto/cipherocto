//! End-to-end encryption subcommands (mission 0850h-b).
//!
//! Five subcommands, all operating on a config produced by
//! `octo-matrix-onboard login`:
//!
//! - `e2ee bootstrap` — generate cross-signing keys, upload to the
//!   homeserver (idempotent).
//! - `e2ee verify` — interactive emoji-SAS verification of a paired
//!   device. Currently emits a clear "interactive flow not yet
//!   implemented" message; the SDK's `VerificationRequest` /
//!   `SasVerification` state machine is the canonical reference but
//!   the full TUI wiring is deferred.
//! - `e2ee recovery generate` — generate a fresh 4S recovery key and
//!   write it to disk (mode 0600). Uses the SDK's `Recovery::reset_key`.
//! - `e2ee recovery restore` — read a 4S key on stdin and import it via
//!   `Recovery::recover`. The key is read into a zeroed buffer on drop
//!   and is never logged.
//! - `e2ee verify-session` — out-of-band verification of an
//!   already-logged-in session. Also deferred to the TUI module.
//!
//! ## Why some flows are stubbed
//!
//! The interactive flows (`verify`, `verify-session`) drive the SDK's
//! `VerificationRequest` / `SasVerification` state machine, which
//! requires an event loop + user prompts. A TUI crate (`dialoguer` is
//! the conventional Rust pick per the mission's implementation notes)
//! is the next step. For this mission we wire the SDK entry points
//! so the CLI surfaces clear progress, and leave the TUI as a
//! follow-up. The non-interactive flows (`bootstrap`, recovery
//! generate/restore) are fully implemented because they have no
//! user-prompt loop.

use crate::cli::{
    E2eeBootstrapArgs, E2eeRecoveryGenerateArgs, E2eeRecoveryRestoreArgs, E2eeVerifyArgs,
    E2eeVerifySessionArgs,
};
use crate::error::{OnboardError, Result};
use matrix_sdk::Client;
use octo_matrix_onboard_core::client_from_config::client_from_config as core_client_from_config;
use octo_matrix_onboard_core::CoreError;
use std::path::Path;
use tracing::{info, warn};

/// Thin adapter around the core helper: converts the core's
/// typed `CoreError` into the CLI's `OnboardError` so the e2ee
/// subcommands surface the right exit code. R2-M1: previously
/// substring-matched on the error message; now `match`es on the
/// variant directly, so a future "read config X" string in an
/// SDK error message can't misclassify.
async fn client_from_config(path: &Path) -> Result<Client> {
    core_client_from_config(path).await.map_err(|e| match e {
        CoreError::Read { path, source } => {
            OnboardError::BadConfig(format!("read config {path:?}: {source}"))
        }
        CoreError::Parse { path, source } => {
            OnboardError::BadConfig(format!("parse config {path:?}: {source}"))
        }
        CoreError::InvalidUserId { value, source } => {
            OnboardError::BadConfig(format!("invalid user_id {value:?}: {source}"))
        }
        // SDK-side failures (build / restore_session) are not config
        // problems; they go to `Generic` so the operator sees the
        // raw SDK error.
        other => OnboardError::Generic(anyhow::anyhow!(other)),
    })
}

pub async fn bootstrap(args: E2eeBootstrapArgs) -> Result<()> {
    let client = client_from_config(&args.base.config).await?;

    if !args.quiet {
        eprintln!("Bootstrapping cross-signing — first run may take 30+ seconds...");
    }
    let result = client.encryption().bootstrap_cross_signing(None).await;
    match result {
        Ok(()) => {
            info!("cross-signing bootstrap complete");
            eprintln!("Cross-signing keys generated and uploaded to the homeserver.");
            eprintln!();
            eprintln!("Recovery key (4S) is NOT created by bootstrap. To create one, run:");
            eprintln!("  octo-matrix-onboard e2ee recovery generate --config <path> --out <path>");
            Ok(())
        }
        Err(e) => {
            // Cross-signing already set up is a non-fatal no-op.
            let msg = e.to_string();
            if msg.contains("already") || msg.contains("exists") {
                eprintln!("Cross-signing is already set up on this account.");
                Ok(())
            } else {
                Err(OnboardError::Generic(anyhow::anyhow!(
                    "cross-signing bootstrap: {}",
                    msg
                )))
            }
        }
    }
}

pub async fn verify(_args: E2eeVerifyArgs) -> Result<()> {
    // Full SAS UX requires a TUI crate (`dialoguer` per mission notes).
    // The SDK's `VerificationRequest` and `SasVerification` state
    // machines are the canonical reference; the TUI is a follow-up.
    //
    // R1-L10: the error message previously pointed at `docs/`
    // for the SDK state machine, but `docs/` does not contain such
    // a doc (the only matrix-related doc is the 0850h-a design
    // plan). The authoritative reference is upstream:
    // <https://github.com/matrix-org/matrix-rust-sdk/tree/main/crates/matrix-sdk/src/verification>
    //
    // R3-L2: removed the `eprintln!` lines that duplicated the
    // error message. The binary's top-level error printer routes
    // `OnboardError::Display` to stderr, so emitting the same text
    // twice (once via `eprintln!` and once via the error's Display)
    // was a duplicated-output bug. The single error below carries
    // all the hint text.
    warn!("e2ee verify: not yet implemented");
    Err(OnboardError::Generic(anyhow::anyhow!(
        "e2ee verify: interactive emoji-SAS flow not yet implemented in this build. \
         Use a verified Element client (mobile/web) to drive the verification; the CLI \
         side of the flow is planned in a follow-up mission (TUI module). The SDK state \
         machine that backs this command lives in matrix-sdk/src/verification."
    )))
}

pub async fn verify_session(_args: E2eeVerifySessionArgs) -> Result<()> {
    // R3-L2: same fix as `verify` — drop the duplicated `eprintln!`
    // lines; the error's Display impl is already routed to stderr
    // by the binary entrypoint.
    warn!("e2ee verify-session: not yet implemented");
    Err(OnboardError::Generic(anyhow::anyhow!(
        "e2ee verify-session: out-of-band verification not yet implemented in this build. \
         Planned in a follow-up mission with the TUI module; until then, run e2ee verify \
         on the device that sent the request, or use Element's \"Verify this device\" UI."
    )))
}

/// Read a 4S recovery key from stdin with TTY echo disabled,
/// returning a `Zeroizing<String>` that zeros its heap buffer on
/// drop. The key is never echoed, never logged, and never included
/// in error messages.
///
/// R2-H4: heap-zeroing on drop via `Zeroizing<String>`.
///
/// R3-H2: previously used `BufRead::read_line` on `io::stdin()`,
/// which does NOT disable TTY echo. The prompt claimed input was
/// hidden, but the recovery key appeared in the terminal, in
/// scrollback, in tmux logs, and in `~/.bash_history`-style
/// terminal histories. `rpassword::prompt_password` handles the
/// cross-platform "disable echo, read line, restore echo" dance
/// (termios on Unix, SetConsoleMode on Windows), with proper
/// cleanup even if the user hits Ctrl-C mid-input.
fn read_recovery_key_from_stdin() -> Result<zeroize::Zeroizing<String>> {
    use zeroize::Zeroizing;
    // Try /dev/tty first (hidden input). Fall back to actual stdin
    // when /dev/tty is unavailable (piped input, CI, non-interactive).
    let raw = match rpassword::prompt_password(
        "Paste the 4S recovery key and press Enter (input is hidden): ",
    ) {
        Ok(s) => s,
        Err(e) => {
            // /dev/tty failed (ENXERROR, ENOTTY, etc.) — read from stdin.
            tracing::debug!(error = %e, "/dev/tty unavailable, falling back to stdin");
            let mut line = String::new();
            std::io::stdin().read_line(&mut line).map_err(|e| {
                OnboardError::Generic(anyhow::anyhow!("read recovery key from stdin: {}", e))
            })?;
            line
        }
    };
    // Move into a `Zeroizing` wrapper immediately so the heap
    // buffer is zeroed on drop. `rpassword` returns a plain
    // `String`; the original allocation goes out of scope at the
    // end of this function (Rust drops `raw` after `trimmed` is
    // returned), but we wrap and trim defensively rather than
    // relying on the move semantics.
    let owned = Zeroizing::new(raw);
    let trimmed = Zeroizing::new(owned.trim().to_string());
    if trimmed.is_empty() {
        return Err(OnboardError::Cancelled("empty recovery key".into()));
    }
    Ok(trimmed)
}

pub async fn recovery_generate(args: E2eeRecoveryGenerateArgs) -> Result<()> {
    let client = client_from_config(&args.base.config).await?;

    if args.out.exists() && !args.force {
        return Err(OnboardError::BadConfig(format!(
            "{:?} already exists; pass --force to overwrite (WARNING: this invalidates \
             the previous recovery key and any encrypted history backed up under it)",
            args.out
        )));
    }

    eprintln!("Generating fresh 4S recovery key — invalidates any existing key.");
    let new_key = client
        .encryption()
        .recovery()
        .reset_key()
        .await
        .map_err(|e| OnboardError::Generic(anyhow::anyhow!("reset_key: {}", e)))?;

    // Atomic write with 0600 mode (matches 0850h-a's `output` module
    // contract; we don't import output::write because this is a
    // single-secret file with stricter format requirements).
    write_recovery_key(&args.out, &new_key)?;

    info!(out = ?args.out, "recovery key generated and written");
    eprintln!();
    eprintln!("Recovery key written to {:?} (mode 0600).", args.out);
    eprintln!("THIS IS THE ONLY COPY. Store it in a password manager or print and lock away.");
    eprintln!("If you lose it AND have no verified device, you lose access to encrypted history.");
    Ok(())
}

pub async fn recovery_restore(args: E2eeRecoveryRestoreArgs) -> Result<()> {
    let client = client_from_config(&args.base.config).await?;
    let key = read_recovery_key_from_stdin()?;

    // R2-L9: validate the 4S format up front. The 4S spec (MSC3861)
    // requires 12 groups of 4 alphanumeric characters separated
    // by single spaces (e.g. "AAAA BBBB CCCC DDDD EEEE FFFF
    // GGGG HHHH IIII JJJJ KKKK LLLL"). Some clients use lowercase
    // — the SDK normalizes — but the structural shape is fixed.
    // Catching the shape here surfaces a clear "expected 12 groups
    // of 4 chars separated by spaces; got N groups" message
    // instead of an opaque SDK error.
    if let Err(reason) = validate_four_s_format(&key) {
        return Err(OnboardError::BadConfig(format!(
            "recovery key is not in 4S format: {} (expected 12 groups of 4 alphanumeric chars separated by spaces)",
            reason
        )));
    }

    eprintln!("Restoring from 4S key...");
    client
        .encryption()
        .recovery()
        .recover(&key)
        .await
        .map_err(|e| OnboardError::Generic(anyhow::anyhow!("recover: {}", e)))?;
    eprintln!("Recovery complete — secrets bundle restored.");
    Ok(())
}

/// R2-L9: structural check on a 4S recovery key. The check is
/// intentionally permissive on the alphabetic case (the SDK
/// normalizes) but strict on the structure. Returns `Ok(())` on
/// valid input; `Err(reason)` otherwise. The `reason` is a
/// short string suitable for a "key is not in 4S format: REASON"
/// operator-facing message.
fn validate_four_s_format(key: &str) -> std::result::Result<(), &'static str> {
    let groups: Vec<&str> = key.split_ascii_whitespace().collect();
    if groups.len() != 12 {
        return Err("expected 12 whitespace-separated groups");
    }
    for (i, group) in groups.iter().enumerate() {
        if group.len() != 4 {
            return Err("each group must be 4 characters");
        }
        if !group.chars().all(|c| c.is_ascii_alphanumeric()) {
            return Err("group contains a non-alphanumeric character");
        }
        let _ = i; // suppress unused warning when no group-specific message is needed
    }
    Ok(())
}

/// Atomic write of the recovery key to disk with mode 0600. Mirrors
/// the write protocol in `output::write_atomic` (mission 0850h-a)
/// without depending on the on-disk config schema.
///
/// R1-M15: the `mode(0o600)` call is Unix-only. On non-Unix the
/// function still writes the file (default permissions apply) but
/// does not attempt to set 0600.
fn write_recovery_key(path: &Path, key: &str) -> Result<()> {
    use std::io::Write;
    let parent = path
        .parent()
        .ok_or_else(|| OnboardError::BadConfig(format!("invalid path {:?}", path)))?;
    std::fs::create_dir_all(parent)
        .map_err(|e| OnboardError::Generic(anyhow::anyhow!("create parent {:?}: {}", parent, e)))?;

    // R2-L8: warn (don't refuse) when the parent directory is
    // world-writable on Unix. The `0o600` mode on the file itself
    // is meaningless if the parent directory is `1777` — any user
    // on the box can `rm` the file and replace it with a symlink
    // to their own key. We deliberately do NOT hard-fail here:
    // the operator may have a legitimate reason (CI, scratch
    // space, intentional quarantine); the warning gives them a
    // chance to fix the path before proceeding.
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        match std::fs::metadata(parent) {
            Ok(meta) => {
                let mode = meta.mode();
                if mode & 0o002 != 0 {
                    eprintln!(
                        "WARNING: parent directory {:?} is world-writable (mode {:o}). \
                         The recovery key file will be mode 0600, but any user on the box \
                         can replace it with a symlink. Choose a directory you control \
                         (e.g. ~/.local/share/cipherocto/).",
                        parent,
                        mode & 0o7777
                    );
                }
            }
            Err(_) => {
                // Newly created or unreadable — `create_dir_all`
                // succeeded above, so unreadable here would be
                // surprising. Silent.
            }
        }
    }

    let tmp = path.with_extension("tmp");
    // R3-M2: the tmp file holds the plaintext recovery key under
    // mode 0600. If any of `open` / `write_all` / `sync_all` /
    // `rename` fails, we MUST clean it up — otherwise a leftover
    // `path.tmp` containing the key sits on disk until the next
    // run. We use a small RAII guard that unlinks the path on
    // drop unless explicitly disarmed; on the happy path we disarm
    // it right before `rename` consumes the tmp file.
    struct TmpGuard<'a> {
        path: &'a Path,
        armed: bool,
    }
    impl<'a> Drop for TmpGuard<'a> {
        fn drop(&mut self) {
            if self.armed {
                let _ = std::fs::remove_file(self.path);
            }
        }
    }
    let mut guard = TmpGuard {
        path: &tmp,
        armed: true,
    };
    {
        let mut opts = std::fs::OpenOptions::new();
        opts.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o600);
        }
        let mut f = opts
            .open(&tmp)
            .map_err(|e| OnboardError::Generic(anyhow::anyhow!("open tmp: {}", e)))?;
        f.write_all(key.as_bytes())
            .and_then(|_| f.write_all(b"\n"))
            .map_err(|e| OnboardError::Generic(anyhow::anyhow!("write tmp: {}", e)))?;
        f.sync_all()
            .map_err(|e| OnboardError::Generic(anyhow::anyhow!("sync tmp: {}", e)))?;
    }
    std::fs::rename(&tmp, path)
        .map_err(|e| OnboardError::Generic(anyhow::anyhow!("rename: {}", e)))?;
    // Disarm the guard — `rename` consumed the tmp file, so any
    // `remove_file` on its path would race a future write.
    guard.armed = false;
    Ok(())
}

/// R2-L9: tests for the 4S format validator. Each test exercises
/// one rejection branch and one acceptance branch.
#[cfg(test)]
mod four_s_tests {
    use super::validate_four_s_format;

    #[test]
    fn accepts_well_formed_key() {
        let key = "AAAA BBBB CCCC DDDD EEEE FFFF GGGG HHHH IIII JJJJ KKKK LLLL";
        assert!(validate_four_s_format(key).is_ok());
    }

    #[test]
    fn rejects_wrong_group_count() {
        let key = "AAAA BBBB CCCC DDDD"; // only 4 groups
        let err: &'static str = validate_four_s_format(key).unwrap_err();
        assert!(err.contains("12"), "got: {err}");
    }

    #[test]
    fn rejects_wrong_group_length() {
        let key = "AAAA BBBB CCCC DDDD EEEE FFFF GGGG HHHH IIII JJJJ KKKK LLLLLL"; // 13th group has 6 chars
        let err: &'static str = validate_four_s_format(key).unwrap_err();
        assert!(err.contains("4 characters"), "got: {err}");
    }

    #[test]
    fn rejects_non_alphanumeric() {
        let key = "AAAA BBBB CCCC DDDD EEEE FFFF GGGG HHHH IIII JJJJ KKKK LL!!";
        let err: &'static str = validate_four_s_format(key).unwrap_err();
        assert!(err.contains("non-alphanumeric"), "got: {err}");
    }

    #[test]
    fn accepts_lowercase() {
        // SDK normalizes case, so our pre-check should be permissive.
        let key = "aaaa bbbb cccc dddd eeee ffff gggg hhhh iiii jjjj kkkk llll";
        assert!(validate_four_s_format(key).is_ok());
    }
}

#[cfg(test)]
mod tests {
    use crate::logging::RedactingFormat;
    use tracing_subscriber::fmt::MakeWriter;
    use tracing_subscriber::prelude::__tracing_subscriber_SubscriberExt;

    /// Captures `RedactingFormat` output to a `Vec<u8>`.
    #[derive(Clone, Default)]
    struct VecWriter(std::sync::Arc<std::sync::Mutex<Vec<u8>>>);

    impl VecWriter {
        fn output(&self) -> String {
            String::from_utf8(self.0.lock().unwrap().clone()).unwrap()
        }
    }

    impl std::io::Write for VecWriter {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl<'a> MakeWriter<'a> for VecWriter {
        type Writer = VecWriter;
        fn make_writer(&'a self) -> Self::Writer {
            self.clone()
        }
    }

    /// R2-H4 (replacement for the previous no-op
    /// `recovery_key_redacts_in_debug` test): capture
    /// `RedactingFormat` output of a `tracing::info!` call whose
    /// `recovery_key` field carries a 4S-shaped secret, and assert
    /// the raw key bytes do NOT appear in the rendered line.
    #[test]
    fn recovery_key_is_redacted_in_tracing_output() {
        let buf = VecWriter::default();
        let filter = tracing_subscriber::EnvFilter::new("info");
        let fmt_layer = tracing_subscriber::fmt::layer()
            .with_writer(buf.clone())
            .event_format(RedactingFormat::default());
        let subscriber = tracing_subscriber::registry()
            .with(filter)
            .with(crate::logging::RedactLayer)
            .with(fmt_layer);
        // A realistic 4S-shaped key (4 groups of 4 lowercase alphanum).
        const RAW_KEY: &str = "eslk 4fsd kdus 7hjs";
        tracing::subscriber::with_default(subscriber, || {
            tracing::info!(recovery_key = RAW_KEY, "stored");
        });
        let output = buf.output();
        assert!(
            !output.contains(RAW_KEY),
            "raw recovery key leaked into tracing output: {output}"
        );
        assert!(
            output.contains("eslk 4fs***"),
            "expected redacted recovery_key (first 8 + ***) in output, got: {output}"
        );
    }

    /// R2-H4: `Zeroizing<String>` zeros its bytes on drop. We can't
    /// directly observe heap memory, but we CAN observe that
    /// `Zeroizing::zeroize` clears the buffer (length becomes 0
    /// and all bytes are zeroed). The `Drop` impl on `Zeroizing`
    /// calls `zeroize` before deallocating, so the same clearing
    /// happens when the value goes out of scope.
    #[test]
    fn recovery_key_wrapper_is_zeroizing() {
        use zeroize::Zeroize;
        let mut k = zeroize::Zeroizing::new(String::from("eslk 4fsd kdus 7hjs"));
        assert_eq!(k.as_str(), "eslk 4fsd kdus 7hjs");
        assert_eq!(k.len(), 19);
        k.zeroize();
        // After explicit zeroize, the String is empty (Zeroize
        // impl for String clears the bytes AND truncates). The
        // contract: heap memory is overwritten with zeros before
        // any deallocation.
        assert_eq!(k.len(), 0, "Zeroize::zeroize should clear the String");
        assert!(k.is_empty());
        // And the wrapper is still usable (no use-after-free).
        k.push_str("rotated");
        assert_eq!(k.as_str(), "rotated");
    }
}
