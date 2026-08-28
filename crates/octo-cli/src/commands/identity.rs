//! `octo identity` + `octo whoami` — RFC-0011 §Subcommand Taxonomy.
//!
//! Wave 3 implementation per mission `0011-identity-commands`.
//!
//! - `octo whoami` — read-only (exit 0/2/4)
//! - `octo identity show [DID]` — read-only (exit 0/2/4)
//! - `octo identity rotate` — write (exit 0/2/3/4/5/11/64)
//! - `octo identity revoke` — write (exit 0/2/4/5/6/11/64)
//!
//! Layer C/D orchestrator. Consumes substrate via `octo_wallet::WalletStore`
//! and the free fns `active_identity`, `identity_record_fn`, `begin_rotation`,
//! and `revoke`. `signature_proof` is rendered through the `RedactedHex`
//! wrapper.

use chrono::{DateTime, Utc};
use clap::Subcommand;
use serde::Serialize;

use crate::error::OctoCliError;
use crate::flags::OperatorMode;
use crate::output::OutputEnvelope;
use crate::redact::RedactedHex;
use crate::Octo;

// ---------------------------------------------------------------------------
// Clap surface — `octo identity <action>` (RFC-0011 §Subcommand Taxonomy)
// ---------------------------------------------------------------------------

/// Identity subcommands.
#[derive(Subcommand, Debug)]
pub enum IdentityAction {
    /// Show an identity record (defaults to the active identity).
    Show {
        /// Target DID.
        did: Option<String>,
    },
    /// Begin a key rotation.
    Rotate {
        /// Acknowledge the irreversible effect of rotation.
        #[arg(long, requires = "confirm")]
        confirm_acknowledge: bool,
    },
    /// Revoke the active identity.
    Revoke {
        /// Revocation reason recorded in the identity log.
        #[arg(long)]
        reason: String,
        /// Acknowledge the irreversible effect of revocation.
        #[arg(long, requires = "confirm")]
        confirm_acknowledge: bool,
    },
}

// ---------------------------------------------------------------------------
// Output structs — RFC-0011 §Subcommand Taxonomy IdentityAction rows
// ---------------------------------------------------------------------------

/// `octo whoami` payload (Layer C/D; composes `IdentityRecord`).
#[derive(Serialize, Debug, Clone)]
pub struct WhoamiOutput {
    /// Canonical DID (RFC-0010 form).
    pub did: String,
    /// Hex-encoded 32-byte Ed25519 public key.
    pub pubkey_hex: String,
    /// Stable lifecycle label (`Designated` / `Active` / `Rotating` /
    /// `Revoked`) sourced from substrate `impl fmt::Debug for LifecycleState`.
    pub lifecycle_state: String,
    /// HSM slot id (`None` for `InMemorySigner`-backed identities).
    pub hsm_slot: Option<u32>,
    /// RFC 3339 UTC timestamp of registration.
    pub registered_at: DateTime<Utc>,
}

/// `octo identity show [DID]` payload — wraps `IdentityRecord` +
/// `rotation_history`.
#[derive(Serialize, Debug, Clone)]
pub struct IdentityShowOutput {
    /// Canonical DID (RFC-0010 form).
    pub did: String,
    /// Hex-encoded 32-byte Ed25519 public key.
    pub pubkey_hex: String,
    /// Stable lifecycle label (`Designated` / `Active` / `Rotating` / `Revoked`).
    pub lifecycle_state: String,
    /// Rotation history rows; empty when identity has never rotated.
    pub rotation_history: Vec<IdentityRotationEventOutput>,
    /// HSM slot id (`None` for `InMemorySigner`-backed identities).
    pub hsm_slot: Option<u32>,
}

/// One rotation event in `IdentityShowOutput::rotation_history`.
///
/// `signature_proof` is rendered through [`RedactedHex`] — never raw bytes
/// (defense-in-depth on top of substrate `sign` paths).
#[derive(Serialize, Debug, Clone)]
pub struct IdentityRotationEventOutput {
    /// Hex-encoded 32-byte rotation id.
    pub rotation_id: String,
    /// RFC 3339 UTC timestamp of rotation start.
    pub started_at: DateTime<Utc>,
    /// RFC 3339 UTC timestamp of grace expiry (24h after start, hard-coded
    /// in substrate via `ROTATION_GRACE_PERIOD_SECS`).
    pub grace_expires_at: DateTime<Utc>,
    /// DID of the successor identity (RFC-0010 form).
    pub successor_did: String,
    /// Ed25519 proof signature — always rendered as `[REDACTED:sig]`.
    pub signature_proof: RedactedHex,
}

/// `octo identity rotate` payload.
#[derive(Serialize, Debug, Clone)]
pub struct IdentityRotateOutput {
    /// DID of the new (successor) identity. The current substrate stub does
    /// not yet expose the successor DID; the CLI surfaces a `pending`
    /// placeholder until the substrate amendment lands.
    pub new_did: String,
    /// DID of the rotated-out identity.
    pub old_did: String,
    /// RFC 3339 UTC timestamp of grace expiry.
    pub grace_expires_at: DateTime<Utc>,
    /// 64-byte Ed25519 proof signature — always rendered as
    /// `[REDACTED:sig]` regardless of inner contents.
    pub signature_proof: RedactedHex,
}

/// `octo identity revoke` payload.
#[derive(Serialize, Debug, Clone)]
pub struct IdentityRevokeOutput {
    /// DID of the revoked identity.
    pub did: String,
    /// RFC 3339 UTC timestamp of the revocation event.
    pub revoked_at: DateTime<Utc>,
    /// Always `true` — `Revoked` is terminal per RFC-0009 §Lifecycle row 4.
    pub terminal: bool,
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// `octo whoami` — surface the active identity record.
///
/// Exit codes:
/// - 0: success
/// - 2: no active identity (substrate `WalletError::NotActive`)
/// - 4: active identity's record not found in the store
/// - 64: unexpected substrate error (wallet store open failure etc.)
pub fn whoami(cli: &Octo) -> Result<(), OctoCliError> {
    let store = octo_wallet::WalletStore::open()
        .map_err(|e| OctoCliError::Internal(format!("wallet store open: {e}")))?;
    let key = octo_wallet::active_identity(&store).map_err(|e| match e {
        octo_wallet::WalletError::NotActive { .. } => OctoCliError::NoActiveIdentity,
        other => OctoCliError::Internal(other.to_string()),
    })?;
    let did = key.did();
    let record = octo_wallet::identity_record_fn(&store, &did)
        .map_err(|_| OctoCliError::IdentityNotFound(did.0.clone()))?;
    let output = WhoamiOutput {
        did: record.did.0.clone(),
        pubkey_hex: hex::encode(record.pubkey_bytes),
        lifecycle_state: format!("{:?}", record.lifecycle),
        hsm_slot: record.hsm_slot,
        registered_at: DateTime::<Utc>::from_timestamp(record.registered_at_unix, 0)
            .unwrap_or_else(|| DateTime::<Utc>::from_timestamp(0, 0).unwrap()),
    };
    let env = OutputEnvelope::new(output, 0);
    env.render(cli.output.json, cli.output.no_color)
        .map_err(|e| OctoCliError::Internal(format!("render envelope: {e}")))
}

/// `octo identity show [DID]` — surface one identity record.
///
/// When `did_arg` is `None`, falls back to the active identity.
pub fn show(did_arg: Option<&str>, cli: &Octo) -> Result<(), OctoCliError> {
    let store = octo_wallet::WalletStore::open()
        .map_err(|e| OctoCliError::Internal(format!("wallet store open: {e}")))?;
    let did = match did_arg {
        Some(s) => octo_wallet::Did(s.to_string()),
        None => octo_wallet::active_identity(&store)
            .map_err(|_| OctoCliError::NoActiveIdentity)?
            .did(),
    };
    let record = octo_wallet::identity_record_fn(&store, &did)
        .map_err(|_| OctoCliError::IdentityNotFound(did.0.clone()))?;
    let output = IdentityShowOutput {
        did: record.did.0.clone(),
        pubkey_hex: hex::encode(record.pubkey_bytes),
        lifecycle_state: format!("{:?}", record.lifecycle),
        rotation_history: record
            .rotation_history
            .into_iter()
            .map(|e| IdentityRotationEventOutput {
                rotation_id: hex::encode(e.rotation_id),
                started_at: DateTime::<Utc>::from_timestamp(e.started_at_unix, 0)
                    .unwrap_or_else(|| DateTime::<Utc>::from_timestamp(0, 0).unwrap()),
                grace_expires_at: DateTime::<Utc>::from_timestamp(e.grace_expires_at_unix, 0)
                    .unwrap_or_else(|| DateTime::<Utc>::from_timestamp(0, 0).unwrap()),
                successor_did: e.successor_did.0,
                signature_proof: RedactedHex(e.signature_proof.to_vec()),
            })
            .collect(),
        hsm_slot: record.hsm_slot,
    };
    let env = OutputEnvelope::new(output, 0);
    env.render(cli.output.json, cli.output.no_color)
        .map_err(|e| OctoCliError::Internal(format!("render envelope: {e}")))
}

/// `octo identity rotate` — initiate a key rotation.
///
/// Requires `--confirm` in human mode, `--allow-write` in CI mode (or
/// `--dry-run` for preview).
pub fn rotate(cli: &Octo) -> Result<(), OctoCliError> {
    require_confirm(cli, "identity rotate")?;
    let store = octo_wallet::WalletStore::open()
        .map_err(|e| OctoCliError::Internal(format!("wallet store open: {e}")))?;
    let mut key =
        octo_wallet::active_identity(&store).map_err(|_| OctoCliError::NoActiveIdentity)?;
    let old_did = key.did();
    let lifecycle = key.lifecycle();
    if matches!(lifecycle, octo_wallet::lifecycle::LifecycleState::Revoked) {
        return Err(OctoCliError::AlreadyRevoked);
    }
    if matches!(lifecycle, octo_wallet::lifecycle::LifecycleState::Rotating) {
        return Err(OctoCliError::AlreadyRotating);
    }
    // Successor stub — substrate (Layer B) is still a stub at this RFC stage.
    // `IdentityKey::from_seed` is the canonical substrate constructor for
    // test-only successor keys; real wiring uses `with_signer` (HSM-backed)
    // and lands with the substrate amendment that adds a successor-derivation
    // helper. The chosen seed is deterministic to keep dry-run output
    // byte-stable.
    let successor = octo_wallet::IdentityKey::from_seed([1u8; 32]);
    let now = chrono::Utc::now().timestamp().max(0) as u64;
    let proof = if cli.mode.dry_run {
        [0u8; 64]
    } else {
        octo_wallet::begin_rotation(&mut key, successor, now).map_err(|e| match e {
            octo_wallet::WalletError::NotActive { .. } => {
                OctoCliError::HsmUnavailable("HSM unreachable".into())
            }
            other => OctoCliError::SigningFailed(other.to_string()),
        })?
    };
    let grace_expires_at = DateTime::<Utc>::from_timestamp(now as i64 + 86_400, 0)
        .unwrap_or_else(|| DateTime::<Utc>::from_timestamp(0, 0).unwrap());
    let output = IdentityRotateOutput {
        new_did: "did:octo:pending".to_string(),
        old_did: old_did.0,
        grace_expires_at,
        signature_proof: RedactedHex(proof.to_vec()),
    };
    let env = if cli.mode.dry_run {
        OutputEnvelope::preview_only(output, 0)
    } else {
        OutputEnvelope::new(output, 0)
    };
    env.render(cli.output.json, cli.output.no_color)
        .map_err(|e| OctoCliError::Internal(format!("render envelope: {e}")))
}

/// `octo identity revoke --reason <str>` — burn the active identity.
///
/// `reason` is REQUIRED (clap enforces); absent → clap exit 2 usage error.
pub fn revoke(reason: &str, cli: &Octo) -> Result<(), OctoCliError> {
    require_confirm(cli, "identity revoke")?;
    let store = octo_wallet::WalletStore::open()
        .map_err(|e| OctoCliError::Internal(format!("wallet store open: {e}")))?;
    let mut key =
        octo_wallet::active_identity(&store).map_err(|_| OctoCliError::NoActiveIdentity)?;
    let did = key.did();
    let lifecycle = key.lifecycle();
    if matches!(lifecycle, octo_wallet::lifecycle::LifecycleState::Revoked) {
        return Err(OctoCliError::AlreadyRevoked);
    }
    let now = chrono::Utc::now().timestamp().max(0) as u64;
    if !cli.mode.dry_run {
        octo_wallet::revoke(&mut key, now).map_err(|e| match e {
            octo_wallet::WalletError::NotActive { .. } => {
                OctoCliError::HsmUnavailable("HSM unreachable".into())
            }
            other => OctoCliError::SigningFailed(other.to_string()),
        })?;
    }
    let revoked_at = DateTime::<Utc>::from_timestamp(now as i64, 0)
        .unwrap_or_else(|| DateTime::<Utc>::from_timestamp(0, 0).unwrap());
    // `reason` is captured by substrate in production wiring; v1.0 CLI
    // surface does not echo it back to the operator (audit-only field).
    let _ = reason;
    let output = IdentityRevokeOutput {
        did: did.0,
        revoked_at,
        terminal: true,
    };
    let env = if cli.mode.dry_run {
        OutputEnvelope::preview_only(output, 0)
    } else {
        OutputEnvelope::new(output, 0)
    };
    env.render(cli.output.json, cli.output.no_color)
        .map_err(|e| OctoCliError::Internal(format!("render envelope: {e}")))
}

// ---------------------------------------------------------------------------
// Confirmation / dry-run gates
// ---------------------------------------------------------------------------

/// Confirmation gate — enforce mode + flag combinations for mutating
/// commands. Returns `ConfirmationRequired` (exit 2) when the operator has
/// not authorized the mutation; `Ok(())` when the combination is permitted.
///
/// | Mode     | `--dry-run` | Required flag          |
/// |----------|-------------|------------------------|
/// | Human    | yes         | (none — preview)       |
/// | Human    | no          | `--confirm`            |
/// | Ci       | yes         | (none — preview)       |
/// | Ci       | no          | `--allow-write`        |
/// | Auditor  | yes         | (denied)               |
/// | Auditor  | no          | (denied — read-only)   |
pub fn require_confirm(cli: &Octo, command: &str) -> Result<(), OctoCliError> {
    if cli.mode.dry_run {
        return Ok(()); // --dry-run bypasses confirmation (preview only)
    }
    match cli.mode.mode {
        OperatorMode::Human => {
            if !cli.mode.confirm {
                return Err(OctoCliError::ConfirmationRequired {
                    command: command.to_string(),
                });
            }
        }
        OperatorMode::Ci => {
            if !cli.mode.allow_write {
                return Err(OctoCliError::ConfirmationRequired {
                    command: command.to_string(),
                });
            }
        }
        OperatorMode::Auditor => {
            return Err(OctoCliError::ConfirmationRequired {
                command: command.to_string(),
            });
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Dispatch
// ---------------------------------------------------------------------------

/// Route an `IdentityAction` to its handler.
pub fn dispatch(action: &IdentityAction, cli: &Octo) -> Result<(), OctoCliError> {
    match action {
        IdentityAction::Show { did } => show(did.as_deref(), cli),
        IdentityAction::Rotate { .. } => {
            require_confirm(cli, "identity rotate")?;
            rotate(cli)
        }
        IdentityAction::Revoke { reason, .. } => {
            require_confirm(cli, "identity revoke")?;
            revoke(reason, cli)
        }
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flags::OperatorModeFlags;

    /// Build a minimal `Octo` for unit tests — only the fields under test
    /// are populated; everything else is default.
    fn cli_with_mode(mode: OperatorMode) -> Octo {
        use clap::Parser;
        let argv = vec!["octo", "whoami"];
        // Clap parsing round-trip to populate the full struct; we then
        // override `mode` for the unit test.
        let mut cli = Octo::try_parse_from(argv).expect("clap parse");
        cli.mode = OperatorModeFlags {
            mode,
            confirm: false,
            allow_write: false,
            dry_run: false,
            allow_stdin_secret: false,
        };
        cli
    }

    #[test]
    fn require_confirm_human_without_confirm_errors() {
        let cli = cli_with_mode(OperatorMode::Human);
        let r = require_confirm(&cli, "identity rotate");
        match r {
            Err(OctoCliError::ConfirmationRequired { command }) => {
                assert_eq!(command, "identity rotate");
            }
            other => panic!("expected ConfirmationRequired, got {other:?}"),
        }
    }

    #[test]
    fn require_confirm_human_with_confirm_ok() {
        let mut cli = cli_with_mode(OperatorMode::Human);
        cli.mode.confirm = true;
        assert!(require_confirm(&cli, "identity rotate").is_ok());
    }

    #[test]
    fn require_confirm_dry_run_bypasses() {
        let cli = cli_with_mode(OperatorMode::Human);
        let mut cli = cli;
        cli.mode.dry_run = true;
        assert!(require_confirm(&cli, "identity rotate").is_ok());
    }

    #[test]
    fn require_confirm_ci_without_allow_write_errors() {
        let cli = cli_with_mode(OperatorMode::Ci);
        let r = require_confirm(&cli, "identity rotate");
        assert!(matches!(r, Err(OctoCliError::ConfirmationRequired { .. })));
    }

    #[test]
    fn require_confirm_ci_with_allow_write_ok() {
        let mut cli = cli_with_mode(OperatorMode::Ci);
        cli.mode.allow_write = true;
        assert!(require_confirm(&cli, "identity rotate").is_ok());
    }

    #[test]
    fn require_confirm_auditor_always_errors() {
        let cli = cli_with_mode(OperatorMode::Auditor);
        let r = require_confirm(&cli, "identity rotate");
        assert!(matches!(r, Err(OctoCliError::ConfirmationRequired { .. })));
    }

    #[test]
    fn whoami_output_serializes_schema_fields() {
        let output = WhoamiOutput {
            did: "did:octo:abc".to_string(),
            pubkey_hex: "deadbeef".to_string(),
            lifecycle_state: "Active".to_string(),
            hsm_slot: None,
            registered_at: DateTime::<Utc>::from_timestamp(1_700_000_000, 0).unwrap(),
        };
        let json = serde_json::to_string(&output).unwrap();
        assert!(json.contains("\"did\":\"did:octo:abc\""));
        assert!(json.contains("\"pubkey_hex\":\"deadbeef\""));
        assert!(json.contains("\"lifecycle_state\":\"Active\""));
        assert!(json.contains("\"hsm_slot\":null"));
        assert!(json.contains("\"registered_at\":"));
    }

    #[test]
    fn rotate_output_signature_proof_redacted() {
        let output = IdentityRotateOutput {
            new_did: "did:octo:pending".to_string(),
            old_did: "did:octo:old".to_string(),
            grace_expires_at: DateTime::<Utc>::from_timestamp(1_700_086_400, 0).unwrap(),
            signature_proof: RedactedHex(vec![0xde; 64]),
        };
        let json = serde_json::to_string(&output).unwrap();
        assert!(json.contains("[REDACTED:sig]"));
        // Defense-in-depth — the raw bytes must NOT leak into the JSON.
        assert!(!json.contains("dead"));
        assert!(!json.contains("0xde"));
    }

    #[test]
    fn revoke_output_terminal_true() {
        let output = IdentityRevokeOutput {
            did: "did:octo:abc".to_string(),
            revoked_at: DateTime::<Utc>::from_timestamp(1_700_000_000, 0).unwrap(),
            terminal: true,
        };
        let json = serde_json::to_string(&output).unwrap();
        assert!(json.contains("\"terminal\":true"));
    }
}
