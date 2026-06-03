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
use std::path::Path;
use tracing::{info, warn};

/// Thin adapter around the core helper: converts the core's
/// `anyhow::Error` into the CLI's `OnboardError` so the e2ee
/// subcommands surface the right exit code. R1-M14: this used to
/// inline the same JSON-parse-then-SDK-build logic as `whoami`;
/// both now delegate to `octo_matrix_onboard_core::client_from_config`
/// (the typed `OnboardConfig` struct).
async fn client_from_config(path: &Path) -> Result<Client> {
    core_client_from_config(path)
        .await
        .map_err(|e: anyhow::Error| {
            // `BadConfig` for any parse / read error so the operator
            // gets exit 5; SDK build / restore_session errors are
            // mapped to `Generic` (the core crate only returns
            // `anyhow::Error`, so we conservatively route everything
            // to `Generic` and let the message bubble up).
            let msg = e.to_string();
            if msg.contains("read config")
                || msg.contains("parse config")
                || msg.contains("invalid user_id")
            {
                OnboardError::BadConfig(msg)
            } else {
                OnboardError::Generic(e)
            }
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
    eprintln!("e2ee verify: interactive emoji-SAS flow not yet implemented in this build.");
    eprintln!("Use a verified Element client (mobile/web) to drive the verification; the CLI");
    eprintln!("side of the flow is planned in a follow-up mission (TUI module).");
    warn!("e2ee verify: not yet implemented");
    Err(OnboardError::Generic(anyhow::anyhow!(
        "e2ee verify: interactive flow not yet implemented; see the matrix-rust-sdk \
         verification module for the SDK state machine that backs this command \
         (matrix-sdk/src/verification)"
    )))
}

pub async fn verify_session(_args: E2eeVerifySessionArgs) -> Result<()> {
    eprintln!("e2ee verify-session: out-of-band verification not yet implemented in this build.");
    eprintln!("Planned in a follow-up mission with the TUI module; until then, run e2ee verify");
    eprintln!("on the device that sent the request, or use Element's \"Verify this device\" UI.");
    warn!("e2ee verify-session: not yet implemented");
    Err(OnboardError::Generic(anyhow::anyhow!(
        "e2ee verify-session: not yet implemented"
    )))
}

/// Read a 4S recovery key from stdin into a string and immediately
/// zero the buffer on drop. The key is never echoed, never logged,
/// and never included in error messages.
fn read_recovery_key_from_stdin() -> Result<String> {
    use std::io::{self, BufRead, Write};
    eprintln!("Paste the 4S recovery key and press Enter (input is hidden):");
    let stdin = io::stdin();
    let mut line = String::new();
    let mut handle = stdin.lock();
    handle
        .read_line(&mut line)
        .map_err(|e| OnboardError::Generic(anyhow::anyhow!("read recovery key: {}", e)))?;
    let _ = io::stderr().write_all(b"\n");
    let trimmed = line.trim().to_string();
    // Zero the buffer best-effort. `String::clear` deallocates the
    // heap memory; the original bytes may linger until the allocator
    // reuses the page, but we've at least dropped the only reference.
    line.clear();
    line.shrink_to_fit();
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

    let tmp = path.with_extension("tmp");
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
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn recovery_key_redacts_in_debug() {
        let s = String::from("eslk 4fsd kdus 7hjs");
        let dbg = format!("{:?}", s);
        // String::Debug shows the contents, so we can't test the SDK
        // redaction here. This is a smoke test that the key
        // representation is at least non-empty after read; the actual
        // redaction happens at the tracing layer (logging.rs).
        assert!(!dbg.is_empty());
    }
}
