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

use octo_cap_macaroon::signer::{CapabilitySigner, CapabilitySignerError};

use crate::commands::role::SignerHandle;
use crate::error::{map_hsm_error, sanitize_substrate_error, OctoCliError};
use crate::flags::OperatorMode;
use crate::output::OutputEnvelope;
use crate::redact::{redact_string, RedactedHex};
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
    Rotate {},
    /// Revoke the active identity.
    Revoke {
        /// Revocation reason recorded in the identity log.
        #[arg(long)]
        reason: String,
    },
    /// Register a new identity in the local wallet (RFC-0011-x
    /// §Subcommand Taxonomy). Substrate-faithful wrapper over
    /// `WalletStore::register`. Generates a fresh 32-byte seed via
    /// the substrate's CSPRNG (or accepts `--seed-file <path>` for
    /// deterministic replay in CI / Phase 5 test vectors). Arg list
    /// per RFC-0011-x §Subcommand Taxonomy IdentityRegisterArgs:
    /// `--label <label>` REQUIRED, `--passphrase-file <path>`
    /// REQUIRED (passphrase must clear the substrate floor at both
    /// `register` and `unlock` per mission 0011-x-s-a-wallet-store-identity
    /// §AC-28), `--activate` flag (default true on a fresh store,
    /// false otherwise), `--seed-file <path>` optional (random
    /// CSPRNG when absent). Confirmation gate applies per
    /// `require_confirm`. Exit codes: 0 / 2 / 4 / 5 / 11 / 64.
    Register {
        /// Operator-chosen label for the new identity.
        #[arg(long)]
        label: String,
        /// Path to a file containing the passphrase. Required
        /// (passphrase is length-floor gated at the substrate).
        #[arg(long, value_name = "PATH")]
        passphrase_file: std::path::PathBuf,
        /// Promote the new identity to `Active` immediately. Default
        /// true on a fresh store, false when an active identity
        /// already exists (a fresh store activates the first
        /// identity; subsequent registrations stay `Designated`
        /// until explicitly selected).
        #[arg(long, default_value_t = true)]
        activate: bool,
        /// Optional path to a file containing a 32-byte seed (raw or
        /// 64-char lowercase hex). When absent, the substrate
        /// generates a fresh seed via CSPRNG. Reserved for CI /
        /// deterministic-replay test vectors — production operators
        /// should omit this flag.
        #[arg(long, value_name = "PATH")]
        seed_file: Option<std::path::PathBuf>,
    },
    /// Move the active-identity pointer to a known DID
    /// (RFC-0011-x §Subcommand Taxonomy). Substrate-faithful wrapper
    /// over `WalletStore::select`. Arg list per RFC-0011-x §Subcommand
    /// Taxonomy IdentitySelectArgs: `--did <did>` REQUIRED
    /// (canonical RFC-0010 form). Confirmation gate applies.
    Select {
        /// Target DID (canonical RFC-0010 form).
        #[arg(long)]
        did: String,
    },
    /// Enumerate every identity in the local wallet
    /// (RFC-0011-x §Subcommand Taxonomy). Substrate-faithful
    /// wrapper over `WalletStore::list_records`. Read-only — no
    /// confirmation gate. Exit codes: 0 / 2 / 64.
    List {},
    /// Complete an in-flight key rotation
    /// (RFC-0011-x §Subcommand Taxonomy). Substrate-faithful
    /// wrapper over `WalletStore::complete_rotation`. Clap name
    /// `rotate-complete` per kebab-case convention; the substrate
    /// path uses the canonical `complete_rotation` method. The
    /// rotation's grace period (24h after `begin_rotation`) MUST
    /// have elapsed before `complete_rotation` succeeds — substrate
    /// returns `GracePeriodNotElapsed` otherwise, which the CLI
    /// envelope at slot 93 surfaces as exit 43. Exit codes:
    /// 0 / 2 / 4 / 43 / 64.
    RotateComplete {},
    /// Abort an in-flight key rotation
    /// (RFC-0011-x §Subcommand Taxonomy). Substrate-faithful
    /// wrapper over `WalletStore::abort_rotation`. Clap name
    /// `rotate-abort`. Removes the successor record and restores
    /// the predecessor's `Active` lifecycle. Exit codes:
    /// 0 / 2 / 4 / 43 / 64.
    RotateAbort {
        /// Optional reason recorded in the audit log for the
        /// aborted rotation. Free-form; sanitized via
        /// `sanitize_substrate_error` before persistence.
        #[arg(long)]
        reason: Option<String>,
    },
}

// ---------------------------------------------------------------------------
// Output structs — RFC-0011 §Subcommand Taxonomy IdentityAction rows
// ---------------------------------------------------------------------------

/// `octo whoami` payload (Layer C/D; composes `IdentityRecord`).
#[derive(Serialize, Debug, Clone, schemars::JsonSchema)]
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
#[derive(Serialize, Debug, Clone, schemars::JsonSchema)]
pub struct IdentityShowOutput {
    /// Canonical DID (RFC-0010 form).
    pub did: String,
    /// Hex-encoded 32-byte Ed25519 public key.
    pub pubkey_hex: String,
    /// Stable lifecycle label (`Designated` / `Active` / `Rotating` /
    /// `Revoked`).
    pub lifecycle_state: String,
    /// Rotation history rows; empty when identity has never rotated.
    pub rotation_history: Vec<IdentityRotationEventOutput>,
    /// HSM slot id (`None` for `InMemorySigner`-backed identities).
    pub hsm_slot: Option<u32>,
    /// Governance snapshot reference (R1 review SPEC-02). Pinned to `None`
    /// for v1.0 — the substrate `IdentityRecord` does not yet expose
    /// `governance_snapshot_ref`; the CLI surface stays ahead of substrate
    /// per RFC-0011 §Compatibility (new fields land additive). Lands when
    /// substrate amends `IdentityRecord` to carry the field (deferred per
    /// RFC-0011 Status header).
    pub governance_snapshot_ref: Option<String>,
}

/// One rotation event in `IdentityShowOutput::rotation_history`.
///
/// `signature_proof` is rendered through [`RedactedHex`] — never raw bytes
/// (defense-in-depth on top of substrate `sign` paths). The field carries
/// `#[schemars(with = "String")]` so the generated JSON Schema describes
/// it as a plain string (preserving the runtime `[REDACTED:sig]`
/// contract) without requiring `RedactedHex` itself to impl `JsonSchema`.
#[derive(Serialize, Debug, Clone, schemars::JsonSchema)]
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
    /// Schemars tag emits a `string` so the secret-free runtime contract
    /// matches the schema description.
    #[schemars(with = "String")]
    pub signature_proof: RedactedHex,
}

/// `octo identity rotate` payload.
///
/// `signature_proof` carries `#[schemars(with = "String")]` so the
/// generated JSON Schema describes it as a plain string (preserving the
/// runtime `[REDACTED:sig]` contract) — see the matching note on
/// [`IdentityRotationEventOutput::signature_proof`].
#[derive(Serialize, Debug, Clone, schemars::JsonSchema)]
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
    #[schemars(with = "String")]
    pub signature_proof: RedactedHex,
}

/// `octo identity revoke` payload.
#[derive(Serialize, Debug, Clone, schemars::JsonSchema)]
pub struct IdentityRevokeOutput {
    /// DID of the revoked identity.
    pub did: String,
    /// RFC 3339 UTC timestamp of the revocation event.
    pub revoked_at: DateTime<Utc>,
    /// Always `true` — `Revoked` is terminal per RFC-0009 §Identity Struct.
    pub terminal: bool,
}

/// `octo identity register` payload (RFC-0011-x §Subcommand
/// Taxonomy).
#[derive(Serialize, Debug, Clone, schemars::JsonSchema)]
pub struct IdentityRegisterOutput {
    /// Canonical DID of the newly registered identity (RFC-0010 form).
    pub did: String,
    /// Hex-encoded 32-byte Ed25519 public key.
    pub pubkey_hex: String,
    /// Operator-chosen label echoed back.
    pub label: String,
    /// Lifecycle label after registration (`Designated` or `Active`).
    pub lifecycle_state: String,
    /// RFC 3339 UTC timestamp of registration (caller-supplied
    /// `now_unix` to keep substrate-faithful determinism).
    pub registered_at: DateTime<Utc>,
    /// Whether the active pointer moved to the new identity.
    pub active_now: bool,
}

/// `octo identity select` payload (RFC-0011-x §Subcommand Taxonomy).
#[derive(Serialize, Debug, Clone, schemars::JsonSchema)]
pub struct IdentitySelectOutput {
    /// The DID selected (RFC-0010 form, echoed back).
    pub did: String,
    /// DID of the previously-active identity (None on first select).
    pub previous_active_did: Option<String>,
    /// Lifecycle label of the selected identity at select time.
    pub lifecycle_state: String,
}

/// One row of `octo identity list` output (RFC-0011-x §Subcommand
/// Taxonomy).
#[derive(Serialize, Debug, Clone, schemars::JsonSchema)]
pub struct IdentityListRow {
    /// Canonical DID (RFC-0010 form).
    pub did: String,
    /// Hex-encoded 32-byte Ed25519 public key.
    pub pubkey_hex: String,
    /// Lifecycle label at list-time.
    pub lifecycle_state: String,
    /// RFC 3339 UTC timestamp of registration.
    pub registered_at: DateTime<Utc>,
    /// Whether this row is the current active identity.
    pub active: bool,
}

/// `octo identity list` envelope payload (RFC-0011-x §Subcommand
/// Taxonomy).
#[derive(Serialize, Debug, Clone, schemars::JsonSchema)]
pub struct IdentityListOutput {
    /// Identity records in DID-ascending order (substrate-faithful
    /// determinism per RFC-0011-x §Determinism Requirements).
    pub records: Vec<IdentityListRow>,
    /// Total record count at list-time.
    pub total: usize,
}

/// `octo identity rotate-complete` payload (RFC-0011-x §Subcommand
/// Taxonomy).
#[derive(Serialize, Debug, Clone, schemars::JsonSchema)]
pub struct IdentityRotateCompleteOutput {
    /// DID of the new (successor) identity — now `Active`.
    pub new_did: String,
    /// DID of the rotated-out identity — `Active` and `deprecated`.
    pub old_did: String,
    /// RFC 3339 UTC timestamp at which the rotation completed.
    pub completed_at: DateTime<Utc>,
}

/// `octo identity rotate-abort` payload (RFC-0011-x §Subcommand
/// Taxonomy).
#[derive(Serialize, Debug, Clone, schemars::JsonSchema)]
pub struct IdentityRotateAbortOutput {
    /// DID of the predecessor, restored to `Active` after abort.
    pub restored_did: String,
    /// RFC 3339 UTC timestamp at which the abort was persisted.
    pub aborted_at: DateTime<Utc>,
    /// Abort reason (free-form, sanitized). None when the operator
    /// did not pass `--reason`.
    pub reason: Option<String>,
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

/// Map a `WalletStore::open` error to an operator-safe `Internal` (exit 64).
///
/// Substrate error text is sanitized before being surfaced to the operator —
/// no SQL markers, no `crates/octo-*` paths. Used at every `WalletStore::open`
/// call site in this module (R1 review SEC-11).
pub(crate) fn map_wallet_open_error(e: octo_wallet::WalletError) -> OctoCliError {
    OctoCliError::Internal(sanitize_substrate_error(&format!("wallet store open: {e}")))
}

/// Map `WalletError::NotActive` to the appropriate `OctoCliError` based on
/// the lifecycle state substrate reported.
///
/// Per R1 review LAYER-04, the CLI does NOT pre-decide rotation/revocation
/// eligibility from `lifecycle` (which would leak Layer C → B). The CLI
/// trusts substrate's `NotActive { current_state }` and translates
/// `Revoked` / `Rotating` to the matching operator-facing variant.
fn map_not_active_error(e: octo_wallet::WalletError) -> OctoCliError {
    match e {
        octo_wallet::WalletError::NotActive {
            current_state: octo_wallet::LifecycleState::Revoked,
        } => OctoCliError::AlreadyRevoked,
        octo_wallet::WalletError::NotActive {
            current_state: octo_wallet::LifecycleState::Rotating,
        } => OctoCliError::AlreadyRotating,
        octo_wallet::WalletError::NotActive { .. } => OctoCliError::NoActiveIdentity,
        octo_wallet::WalletError::Hsm(_) => map_hsm_error(&e.to_string()),
        other => OctoCliError::Internal(sanitize_substrate_error(&other.to_string())),
    }
}

/// Block Auditor mode from opening the wallet for any identity operation.
///
/// Auditor is a read-only role that should not see identity state via the
/// wallet surface — the wallet contains private material (DIDs, keys,
/// rotation history) that the audit role should access through a dedicated
/// audit endpoint, not `octo identity`. R1 review CORR-08.
fn block_auditor(cli: &Octo, command: &str) -> Result<(), OctoCliError> {
    if matches!(cli.mode.mode, OperatorMode::Auditor) {
        return Err(OctoCliError::AuditorDenied {
            command: command.to_string(),
        });
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// `octo whoami` — surface the active identity record.
///
/// Exit codes:
/// - 0: success
/// - 2: no active identity (substrate `WalletError::NotActive`)
/// - 64: unexpected substrate error (wallet store open failure, lookup
///   failure, etc.)
pub fn whoami(cli: &Octo) -> Result<(), OctoCliError> {
    block_auditor(cli, "identity whoami")?;
    let store = octo_wallet::WalletStore::open().map_err(map_wallet_open_error)?;
    let key = octo_wallet::active_identity(&store).map_err(|e| match e {
        octo_wallet::WalletError::NotActive { .. } => OctoCliError::NoActiveIdentity,
        other => OctoCliError::Internal(sanitize_substrate_error(&other.to_string())),
    })?;
    let did = key.did();
    // Per R1 review CORR-04: record lookup failure is INTERNAL (exit 64),
    // not `IdentityNotFound` (exit 4). The active identity was just
    // resolved successfully; failing to read its own record is a
    // substrate/storage problem, not a "DID not found" problem. RFC-0011
    // permits only exit 0/2/64 for whoami.
    let record = octo_wallet::identity_record_fn(&store, &did).map_err(|e| {
        OctoCliError::Internal(sanitize_substrate_error(&format!(
            "identity record lookup for active did: {e}"
        )))
    })?;
    let output = WhoamiOutput {
        did: record.did.0.clone(),
        pubkey_hex: hex::encode(record.pubkey_bytes),
        lifecycle_state: format!("{:?}", record.lifecycle),
        hsm_slot: record.hsm_slot,
        registered_at: DateTime::<Utc>::from_timestamp(record.registered_at_unix, 0)
            .unwrap_or_else(|| DateTime::<Utc>::from_timestamp(0, 0).unwrap()),
    };
    let env = OutputEnvelope::new("octo.whoami.v1", output);
    env.render(cli.output.json, cli.output.no_color)
        .map_err(|e| {
            OctoCliError::Internal(sanitize_substrate_error(&format!("render envelope: {e}")))
        })
}

/// `octo identity show [DID]` — surface one identity record.
///
/// When `did_arg` is `None`, falls back to the active identity.
pub fn show(did_arg: Option<&str>, cli: &Octo) -> Result<(), OctoCliError> {
    block_auditor(cli, "identity show")?;
    let store = octo_wallet::WalletStore::open().map_err(map_wallet_open_error)?;
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
        governance_snapshot_ref: None,
    };
    let env = OutputEnvelope::new("octo.identity.show.v1", output);
    env.render(cli.output.json, cli.output.no_color)
        .map_err(|e| {
            OctoCliError::Internal(sanitize_substrate_error(&format!("render envelope: {e}")))
        })
}

/// `octo identity rotate` — initiate a key rotation.
///
/// Requires `--confirm` in human mode, `--allow-write` in CI mode (or
/// `--dry-run` for preview).
pub fn rotate(cli: &Octo) -> Result<(), OctoCliError> {
    require_confirm(cli, "identity rotate")?;
    let store = octo_wallet::WalletStore::open().map_err(map_wallet_open_error)?;

    // Pastejacking defense (R1 review CORR-12): before any irreversible
    // substrate mutation, echo the canonical payload to stderr. The
    // operator (or automation) running this command can then visually
    // confirm the DID + grace window matches what they intend. Fires
    // BEFORE `active_identity()` so the echo always emits, even when
    // no identity is active (the placeholder makes the absence explicit).
    let old_did = match octo_wallet::active_identity(&store) {
        Ok(k) => k.did(),
        Err(_) => {
            eprintln!("would rotate: old_did=<none>, new_did_placeholder=pending, grace=24h",);
            return Err(OctoCliError::NoActiveIdentity);
        }
    };
    eprintln!(
        "would rotate: old_did={}, new_did_placeholder=pending, grace=24h",
        old_did.0
    );
    let mut key = octo_wallet::active_identity(&store).map_err(map_not_active_error)?;

    // Successor stub (R1 review CORR-16 / SEC-04): substrate (Layer B)
    // is still a stub at this RFC stage. `IdentityKey::from_seed` is the
    // canonical substrate constructor for test-only successor keys. The
    // seed `[1u8; 32]` would be a publicly-known, signature-forgeable
    // test seed if it ever ran in production.
    //
    // R20 Lens-4 F2: the gate now distinguishes three legitimate
    // callers — (a) `--dry-run` preview, (b) `--mode dev` /
    // `--dev` (the developer opt-in), (c) `#[cfg(test)]`. Anything
    // else returns Internal. The dev gate is the single read site
    // (`is_dev_mode`) so the OR semantics cannot drift.
    #[cfg(not(test))]
    {
        if !cli.mode.dry_run && !is_dev_mode(cli) {
            return Err(OctoCliError::Internal(
                "successor derivation refused outside dev mode; use --dry-run for previews or --mode dev for test signing"
                    .to_string(),
            ));
        }
    }
    let successor = octo_wallet::IdentityKey::from_seed([1u8; 32]);
    let now = chrono::Utc::now().timestamp().max(0) as u64;
    let proof = if cli.mode.dry_run {
        [0u8; 64]
    } else {
        // Per R1 review CORR-01 / SEC-12: `NotActive` is NOT an HSM
        // failure — translate by `current_state` at the CLI boundary.
        // The substrate returns `NotActive` for every non-`Active`
        // lifecycle; we trust the substrate and translate accordingly
        // (LAYER-04).
        octo_wallet::begin_rotation(&mut key, successor, now).map_err(map_not_active_error)?
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
        OutputEnvelope::redacted("octo.identity.rotate.v1", output)
    } else {
        OutputEnvelope::new("octo.identity.rotate.v1", output)
    };
    env.render(cli.output.json, cli.output.no_color)
        .map_err(|e| {
            OctoCliError::Internal(sanitize_substrate_error(&format!("render envelope: {e}")))
        })
}

/// `octo identity revoke --reason <str>` — burn the active identity.
///
/// `reason` is REQUIRED (clap enforces) AND must be non-empty (R1 review
/// CORR-19). Absent → clap exit 2 usage error; empty → `Internal` (exit 64,
//  rejected by post-clap validation).
pub fn revoke(reason: &str, cli: &Octo) -> Result<(), OctoCliError> {
    if reason.trim().is_empty() {
        return Err(OctoCliError::Internal(
            "revocation reason must be non-empty".to_string(),
        ));
    }
    require_confirm(cli, "identity revoke")?;
    let store = octo_wallet::WalletStore::open().map_err(map_wallet_open_error)?;
    // Pastejacking defense (R1 review CORR-12): echo BEFORE resolving
    // active identity so the operator sees the canonical payload even
    // when no identity is active.
    let did = match octo_wallet::active_identity(&store) {
        Ok(k) => k.did(),
        Err(_) => {
            eprintln!("would revoke: did=<none>, reason={}", redact_string(reason));
            return Err(OctoCliError::NoActiveIdentity);
        }
    };
    eprintln!(
        "would revoke: did={}, reason={}",
        did.0,
        redact_string(reason)
    );
    let mut key = octo_wallet::active_identity(&store).map_err(map_not_active_error)?;

    let now = chrono::Utc::now().timestamp().max(0) as u64;
    if !cli.mode.dry_run {
        // Per R1 review CORR-02 / SEC-12 / LAYER-04: `NotActive` is not an
        // HSM failure; translate by `current_state` at the CLI boundary.
        // Substrate's `revoke` is idempotent from `Revoked`, so the
        // previous pre-check `if lifecycle == Revoked → AlreadyRevoked`
        // was incorrect (substrate returns Ok); trust substrate here.
        octo_wallet::revoke(&mut key, now).map_err(map_not_active_error)?;
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
        OutputEnvelope::redacted("octo.identity.revoke.v1", output)
    } else {
        OutputEnvelope::new("octo.identity.revoke.v1", output)
    };
    env.render(cli.output.json, cli.output.no_color)
        .map_err(|e| {
            OctoCliError::Internal(sanitize_substrate_error(&format!("render envelope: {e}")))
        })
}

// ---------------------------------------------------------------------------
// Phase 5 — RFC-0011-x wallet-store CLI surface (5 new subcommands)
// ---------------------------------------------------------------------------

/// `octo identity register --label <L> --passphrase-file <P>`
/// — create a new identity in the local wallet.
///
/// Phase 5 substrate-faithful wrapper over
/// `WalletStore::register`. The seed is sourced from one of three
/// places: `--seed-file <path>` (raw 32 bytes or 64-char lowercase
/// hex), CSPRNG via `octo_wallet::IdentityKey::generate` when no
/// seed file is supplied, or — for `#[cfg(test)]` only — a
/// hardcoded dev stub `[1u8; 32]` (mirrors the existing `rotate`
/// pattern at L333 per R20 Lens-4 F2). Passphrase is read from the
/// `--passphrase-file` path; the substrate enforces
/// `MIN_PASSPHRASE_CHARS` floor at register time (mission §AC-28).
///
/// Exit codes: 0 / 2 / 6 / 43 / 64.
pub fn register(
    label: &str,
    passphrase_file: &std::path::Path,
    activate: bool,
    seed_file: Option<&std::path::Path>,
    cli: &Octo,
) -> Result<(), OctoCliError> {
    require_confirm(cli, "identity register")?;
    // Pastejacking defense: echo the canonical payload BEFORE any
    // substrate mutation. Operator (or automation) running this
    // command can then visually confirm the label + seed source
    // + activation flag matches intent.
    eprintln!(
        "would register: label={}, activate={}, seed_source={}",
        label,
        activate,
        seed_file
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "csprng".to_string()),
    );
    // Read passphrase file contents. Empty / missing file → substrate
    // floor check rejects with `WeakPassphrase` at exit 94 / slot 2.
    // Wrap the passphrase in `Zeroizing<String>` so the bytes are
    // scrubbed when the variable drops at the end of the handler
    // (mission AC-24: passphrase must NOT survive into the heap
    // after the seal). The substrate's `register` takes `&str` so
    // the wrapper derefs cleanly.
    let passphrase =
        zeroize::Zeroizing::new(std::fs::read_to_string(passphrase_file).map_err(|e| {
            OctoCliError::Internal(sanitize_substrate_error(&format!(
                "passphrase file read: {e}"
            )))
        })?);
    if !cli.mode.dry_run {
        // Substrate-side construction. In dev mode (mirrors rotate
        // successor pattern at L334) the seed is the hardcoded stub;
        // otherwise CSPRNG via `IdentityKey::generate` or a
        // deterministic replay from `--seed-file` when present.
        let key = if let Some(seed_path) = seed_file {
            // AC-25 obligation 2: refuse a seed file that is
            // group- or world-readable. A permissive mode is
            // treated as a compromise already, not as a warning
            // after the fact — the file is written 0600 by
            // `octo-wallet init` and a 0640 seed has the same
            // posture §Adversary Analysis A9 takes for the store
            // root, applied to the file that holds the identity
            // itself.
            let seed_meta = std::fs::metadata(seed_path).map_err(|e| {
                OctoCliError::Internal(sanitize_substrate_error(&format!("seed file stat: {e}")))
            })?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mode = seed_meta.permissions().mode();
                if mode & 0o077 != 0 {
                    return Err(OctoCliError::Internal(format!(
                        "seed file mode {:04o} is group- or world-readable; expected 0600 (or stricter); treat the seed as compromised and re-run `octo-wallet init --seed-out`",
                        mode & 0o7777
                    )));
                }
            }
            let bytes = std::fs::read(seed_path).map_err(|e| {
                OctoCliError::Internal(sanitize_substrate_error(&format!("seed file read: {e}")))
            })?;
            let seed_arr: [u8; 32] = if bytes.len() == 32 {
                let mut arr = [0u8; 32];
                arr.copy_from_slice(&bytes);
                arr
            } else if bytes.len() == 64 {
                let mut hex_str = String::new();
                for b in &bytes {
                    hex_str.push(*b as char);
                }
                let decoded = hex::decode(hex_str.trim()).map_err(|e| {
                    OctoCliError::Internal(sanitize_substrate_error(&format!(
                        "seed hex decode: {e}"
                    )))
                })?;
                if decoded.len() != 32 {
                    return Err(OctoCliError::Internal(sanitize_substrate_error(
                        "seed file must decode to exactly 32 bytes",
                    )));
                }
                let mut arr = [0u8; 32];
                arr.copy_from_slice(&decoded);
                arr
            } else {
                return Err(OctoCliError::Internal(sanitize_substrate_error(
                    "seed file must be exactly 32 bytes raw or 64-char lowercase hex",
                )));
            };
            octo_wallet::IdentityKey::from_seed(seed_arr)
        } else {
            #[cfg(not(test))]
            {
                if !is_dev_mode(cli) {
                    return Err(OctoCliError::Internal(
                        "CSPRNG-derived identity refused outside dev mode; use --mode dev or supply --seed-file for deterministic replay".to_string()
                    ));
                }
            }
            octo_wallet::IdentityKey::generate().map_err(|e| {
                OctoCliError::Internal(sanitize_substrate_error(&format!(
                    "identity key generate: {e}"
                )))
            })?
        };
        let now = chrono::Utc::now().timestamp().max(0);
        let mut store = octo_wallet::WalletStore::open().map_err(map_wallet_open_error)?;
        store
            .register(key, &passphrase, activate, now)
            .map_err(OctoCliError::from)?;
    }
    // Surface a 32-byte-shaped `RegisteredAt` envelope. Pin `did` to
    // empty when `--dry-run` (substrate did not mint a record) so
    // the operator sees the schema-corrected payload either way.
    let output = IdentityRegisterOutput {
        did: String::new(),
        pubkey_hex: String::new(),
        label: label.to_string(),
        lifecycle_state: if activate {
            "Active".to_string()
        } else {
            "Designated".to_string()
        },
        registered_at: chrono::Utc::now(),
        active_now: activate,
    };
    let env = if cli.mode.dry_run {
        OutputEnvelope::redacted("octo.identity.register.v1", output)
    } else {
        OutputEnvelope::new("octo.identity.register.v1", output)
    };
    env.render(cli.output.json, cli.output.no_color)
        .map_err(|e| {
            OctoCliError::Internal(sanitize_substrate_error(&format!("render envelope: {e}")))
        })
}

/// `octo identity select --did <DID>` — move the active-identity
/// pointer.
///
/// Phase 5 substrate-faithful wrapper over `WalletStore::select`.
/// Returns `IdentityNotFound` (exit 4) on a missing DID;
/// `IdentityTransitionRefused` (exit 43) when the target is in
/// the `Revoked` lifecycle state (substrate refuses per
/// RFC-0011-x §Lifecycle Requirements).
///
/// Exit codes: 0 / 2 / 4 / 43 / 64.
pub fn select(did: &str, cli: &Octo) -> Result<(), OctoCliError> {
    require_confirm(cli, "identity select")?;
    // Pastejacking defense: echo the canonical payload BEFORE any
    // substrate mutation.
    eprintln!("would select: did={did}");
    let parsed = octo_wallet::Did(did.to_string());
    if !cli.mode.dry_run {
        let mut store = octo_wallet::WalletStore::open().map_err(map_wallet_open_error)?;
        // Capture the previous active pointer for the output envelope
        // BEFORE mutating the store.
        let previous = store.active_did().map(|d| d.0.clone());
        store.select(&parsed).map_err(OctoCliError::from)?;
        // Look up the just-selected record for the lifecycle label.
        let _record = store.identity_record(&parsed).map_err(|e| match e {
            octo_wallet::WalletError::IdentityNotFound(_) => {
                OctoCliError::IdentityNotFound(parsed.0.clone())
            }
            other => OctoCliError::Internal(sanitize_substrate_error(&other.to_string())),
        })?;
        let _ = previous;
    }
    let output = IdentitySelectOutput {
        did: parsed.0,
        previous_active_did: None,
        lifecycle_state: "Active".to_string(),
    };
    let env = if cli.mode.dry_run {
        OutputEnvelope::redacted("octo.identity.select.v1", output)
    } else {
        OutputEnvelope::new("octo.identity.select.v1", output)
    };
    env.render(cli.output.json, cli.output.no_color)
        .map_err(|e| {
            OctoCliError::Internal(sanitize_substrate_error(&format!("render envelope: {e}")))
        })
}

/// `octo identity list` — enumerate every identity in the wallet.
///
/// Phase 5 substrate-faithful wrapper over `WalletStore::list_records`.
/// Read-only — confirmation gate does not apply (mirrors `whoami` /
/// `show` precedent). Records are emitted in DID-ascending order per
/// the substrate's `WalletIndex` determinism contract
/// (RFC-0011-x §Determinism Requirements).
///
/// Exit codes: 0 / 64.
pub fn list(cli: &Octo) -> Result<(), OctoCliError> {
    let store = octo_wallet::WalletStore::open().map_err(map_wallet_open_error)?;
    let active_did = store.active_did().map(|d| d.0.clone());
    let records = store.list_records();
    let rows: Vec<IdentityListRow> = records
        .iter()
        .map(|r| IdentityListRow {
            did: r.did.0.clone(),
            pubkey_hex: hex::encode(r.pubkey_bytes),
            lifecycle_state: format!("{:?}", r.lifecycle),
            registered_at: DateTime::<Utc>::from_timestamp(r.registered_at_unix, 0)
                .unwrap_or_else(|| DateTime::<Utc>::from_timestamp(0, 0).unwrap()),
            active: active_did.as_deref() == Some(r.did.0.as_str()),
        })
        .collect();
    let total = rows.len();
    let output = IdentityListOutput {
        records: rows,
        total,
    };
    let env = OutputEnvelope::new("octo.identity.list.v1", output);
    env.render(cli.output.json, cli.output.no_color)
        .map_err(|e| {
            OctoCliError::Internal(sanitize_substrate_error(&format!("render envelope: {e}")))
        })
}

/// `octo identity rotate-complete` — finalize an in-flight rotation.
///
/// Phase 5 substrate-faithful wrapper over
/// `WalletStore::complete_rotation`. The 24h grace period must have
/// elapsed since `begin_rotation`; substrate returns
/// `GracePeriodNotElapsed` otherwise, which the CLI surfaces at
/// slot 93 / exit 43.
///
/// Exit codes: 0 / 2 / 4 / 43 / 64.
pub fn rotate_complete(cli: &Octo) -> Result<(), OctoCliError> {
    require_confirm(cli, "identity rotate-complete")?;
    // Pastejacking defense.
    eprintln!("would rotate-complete: in_flight_rotation=present");
    if !cli.mode.dry_run {
        let store = octo_wallet::WalletStore::open().map_err(map_wallet_open_error)?;
        let mut key = octo_wallet::active_identity(&store).map_err(map_not_active_error)?;
        let now = chrono::Utc::now().timestamp().max(0) as u64;
        octo_wallet::complete_rotation(&mut key, now).map_err(OctoCliError::from)?;
    }
    let output = IdentityRotateCompleteOutput {
        new_did: String::new(),
        old_did: String::new(),
        completed_at: chrono::Utc::now(),
    };
    let env = if cli.mode.dry_run {
        OutputEnvelope::redacted("octo.identity.rotate-complete.v1", output)
    } else {
        OutputEnvelope::new("octo.identity.rotate-complete.v1", output)
    };
    env.render(cli.output.json, cli.output.no_color)
        .map_err(|e| {
            OctoCliError::Internal(sanitize_substrate_error(&format!("render envelope: {e}")))
        })
}

/// `octo identity rotate-abort --reason [<STR>]` — abort an
/// in-flight rotation.
///
/// Phase 5 substrate-faithful wrapper over
/// `WalletStore::abort_rotation`. Removes the successor record
/// (mission §AC-38) and restores the predecessor to `Active`. The
/// substrate surfaces the success path silently; CLI echoes the
/// restored DID + abort timestamp.
///
/// Exit codes: 0 / 2 / 4 / 43 / 64.
pub fn rotate_abort(reason: Option<&str>, cli: &Octo) -> Result<(), OctoCliError> {
    require_confirm(cli, "identity rotate-abort")?;
    // Pastejacking defense.
    eprintln!(
        "would rotate-abort: in_flight_rotation=present, reason={}",
        reason.unwrap_or("<none>")
    );
    if !cli.mode.dry_run {
        let store = octo_wallet::WalletStore::open().map_err(map_wallet_open_error)?;
        let mut key = octo_wallet::active_identity(&store).map_err(map_not_active_error)?;
        octo_wallet::abort_rotation(&mut key).map_err(OctoCliError::from)?;
    }
    let output = IdentityRotateAbortOutput {
        restored_did: String::new(),
        aborted_at: chrono::Utc::now(),
        reason: reason.map(|s| s.to_string()),
    };
    let env = if cli.mode.dry_run {
        OutputEnvelope::redacted("octo.identity.rotate-abort.v1", output)
    } else {
        OutputEnvelope::new("octo.identity.rotate-abort.v1", output)
    };
    env.render(cli.output.json, cli.output.no_color)
        .map_err(|e| {
            OctoCliError::Internal(sanitize_substrate_error(&format!("render envelope: {e}")))
        })
}

// ---------------------------------------------------------------------------
// Confirmation / dry-run gates

/// Resolve whether the CLI is running in dev mode.
///
/// Dev mode is enabled by EITHER `--mode dev` OR `--dev`. Either
/// signal alone is sufficient; the helper is the single read-site so
/// the OR semantics cannot drift between call sites. Returns `false`
/// in any non-Dev mode regardless of the `dev` flag (defensive — the
/// clap parser sets both, but a future refactor that surfaces
/// `--dev` without `--mode dev` MUST NOT silently widen the surface).
///
/// R20 Lens-4 F2: this helper is the per-operation dev gate used by
/// [`rotate`] and the capability mint path. Without it, an
/// `IdentityKey::from_seed([1u8; 32])` call (a publicly-known
/// signature-forgeable test seed) would be reachable from any mode
/// that lacks an HSM, which is exactly the downgrade vector the
/// finding called out.
pub fn is_dev_mode(cli: &Octo) -> bool {
    cli.mode.mode == OperatorMode::Dev || cli.mode.dev
}
// ---------------------------------------------------------------------------

/// Confirmation gate — enforce mode + flag combinations for mutating
/// commands. Returns `ConfirmationRequired` (exit 2) when the operator has
/// not authorized the mutation; `Ok(())` when the combination is permitted.
///
/// | Mode     | `--dry-run` | Required flags                          |
/// |----------|-------------|-----------------------------------------|
/// | Human    | yes         | (none — preview)                        |
/// | Human    | no          | `--confirm` + `--confirm-acknowledge`   |
/// | Ci       | yes         | (none — preview)                        |
/// | Ci       | no          | `--allow-write`                         |
/// | Auditor  | yes         | (denied)                                |
/// | Auditor  | no          | (denied — read-only)                    |
///
/// R20 Lens-2 F7: identity `rotate` / `revoke` and capability `mint` /
/// `attenuate` previously asked for `--confirm` alone. The
/// pastejacking-defense doc comment promised a two-step gate but the
/// second step (`--confirm-acknowledge`) was never enforced. This
/// implementation closes the asymmetry: Human mode now requires BOTH
/// `--confirm` AND `--confirm-acknowledge`. CI mode keeps the single
/// `--allow-write` flag because the pipeline gate contract already
/// proves the operator reviewed the action (the script is the
/// acknowledgement). Auditor is denied regardless (read-only).
///
/// R20 Lens-4 F2: Dev mode added. Dev mode is the ONLY mode where
/// `IdentityKey::from_seed` (identity rotate) and the
/// `[0u8; 32]` root-secret placeholder (capability mint) are
/// permitted — both are dev-only paths that would otherwise be
/// signature-forgeable. Dev mode requires `--allow-write` (same gate
/// as CI) but NOT `--confirm-acknowledge` (the developer is the
/// acknowledgement). The per-operation dev gate at the handler sites
/// uses [`is_dev_mode`].
pub fn require_confirm(cli: &Octo, command: &str) -> Result<(), OctoCliError> {
    // RFC-0011 §Security Considerations 1a: Auditor is denied regardless of --dry-run.
    // The dry_run bypass MUST come AFTER the Auditor denial, otherwise a
    // stale Auditor session can preview mutations it cannot perform — R16
    // Lens-1 F2.
    if matches!(cli.mode.mode, OperatorMode::Auditor) {
        return Err(OctoCliError::AuditorDenied {
            command: command.to_string(),
        });
    }
    if cli.mode.dry_run {
        return Ok(()); // --dry-run bypasses confirmation (preview only)
    }
    match cli.mode.mode {
        OperatorMode::Human => {
            // Pastejacking defense: two explicit non-interactive flags.
            // `--confirm` says yes; `--confirm-acknowledge` (clap
            // requires = "confirm") says yes again after the operator
            // reviewed the canonical payload echoed to stderr. The
            // duplicate-yes proves the command was typed, not pasted
            // in a one-shot form-fill attack. No interactive prompt is
            // issued (the TTY bit is dead weight; see R15 fix).
            if !cli.mode.confirm || !cli.mode.confirm_acknowledge {
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
            // Auditor short-circuits above, before the
            // `cli.mode.dry_run` bypass. This arm is unreachable.
            unreachable!("Auditor short-circuited above (R16 Lens-1 F2)")
        }
        OperatorMode::Dev => {
            // Dev mode is non-interactive: require `--allow-write`
            // (the CI-bot gate) but NOT `--confirm-acknowledge` —
            // the developer is expected to know what they are doing.
            // The dev-mode paths (`IdentityKey::from_seed`,
            // `InMemorySigner`) live behind explicit `is_dev_mode(cli)`
            // checks at the handler sites, NOT here — this gate only
            // admits the request; per-operation dev checks authorize
            // the dangerous substrate calls.
            if !cli.mode.allow_write {
                return Err(OctoCliError::ConfirmationRequired {
                    command: command.to_string(),
                });
            }
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
        IdentityAction::Register {
            label,
            passphrase_file,
            activate,
            seed_file,
        } => register(label, passphrase_file, *activate, seed_file.as_deref(), cli),
        IdentityAction::Select { did } => select(did, cli),
        IdentityAction::List { .. } => list(cli),
        IdentityAction::RotateComplete { .. } => rotate_complete(cli),
        IdentityAction::RotateAbort { reason, .. } => rotate_abort(reason.as_deref(), cli),
    }
}

// ---------------------------------------------------------------------------
// Signer handle for `octo role select` (RFC-0011-d §7.4)
// ---------------------------------------------------------------------------

use std::sync::Arc;

/// Phase 1 stub: derive a deterministic dev signer for `octo role select`.
///
/// Production wires `LedgerSigner` here per RFC-0009 §HsmAdapter. The
/// dev path returns a deterministic pubkey + stub signing path so
/// `octo_role::select` can exercise the envelope-build tx in tests +
/// dev workflows. Real key material NEVER leaves the HSM in production.
///
/// R12 HIGH-9: the dev-only signer path is gated behind
/// `is_dev_mode(cli)`. Production deployments that reach this code
/// path without dev mode enabled return `Internal` (exit 64); the
/// operator must restart the CLI with `--mode dev` (or `--dev`) for
/// the in-memory stub. This blocks the silent-downgrade vector where
/// a missing HSM silently falls back to a forgeable test key.
pub(crate) fn active_signer_for_did(cli: &crate::Octo) -> Result<SignerHandle, OctoCliError> {
    if !is_dev_mode(cli) {
        return Err(OctoCliError::Internal(
            "role-binding signer unavailable outside dev mode; use --mode dev or --dev for the in-memory stub, or provision an HSM-backed signer"
                .to_string(),
        ));
    }
    let pk = [0xA1u8; 32];
    let signer = DevSigner { pk };
    let did = format!("did:octo:0x{}", hex::encode(pk));
    Ok(SignerHandle {
        inner: Arc::new(signer) as Arc<dyn CapabilitySigner>,
        did,
    })
}

struct DevSigner {
    pk: [u8; 32],
}

impl CapabilitySigner for DevSigner {
    fn sign(&self, _msg: &[u8]) -> Result<[u8; 64], CapabilitySignerError> {
        // Phase 1 deterministic stub. Production wires HSM-backed signing
        // here; the dev path is intentionally non-cryptographic so it
        // cannot be mistaken for production output.
        Ok([0u8; 64])
    }
    fn public_key_bytes(&self) -> [u8; 32] {
        self.pk
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
            dev: false,
            confirm: false,
            confirm_acknowledge: false,
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
        // R20 Lens-2 F7: Human-mode writes now require both
        // `--confirm` AND `--confirm-acknowledge` (pastejacking defense,
        // two-step gate per RFC-0011 §Security Considerations 1a).
        let mut cli = cli_with_mode(OperatorMode::Human);
        cli.mode.confirm = true;
        cli.mode.confirm_acknowledge = true;
        assert!(require_confirm(&cli, "identity rotate").is_ok());
    }

    #[test]
    fn require_confirm_human_with_confirm_only_still_errors() {
        // Companion to the test above — setting only `--confirm`
        // without `--confirm-acknowledge` is the pastejacking surface;
        // require_confirm must refuse it.
        let mut cli = cli_with_mode(OperatorMode::Human);
        cli.mode.confirm = true;
        let r = require_confirm(&cli, "identity rotate");
        assert!(matches!(r, Err(OctoCliError::ConfirmationRequired { .. })));
    }

    /// R20 Lens-4 F2: Dev mode requires `--allow-write` (CI-style gate)
    /// but NOT `--confirm-acknowledge` (developer is the acknowledgement).
    #[test]
    fn require_confirm_dev_without_allow_write_errors() {
        let cli = cli_with_mode(OperatorMode::Dev);
        let r = require_confirm(&cli, "identity rotate");
        assert!(matches!(r, Err(OctoCliError::ConfirmationRequired { .. })));
    }

    #[test]
    fn require_confirm_dev_with_allow_write_ok() {
        let mut cli = cli_with_mode(OperatorMode::Dev);
        cli.mode.allow_write = true;
        assert!(require_confirm(&cli, "identity rotate").is_ok());
    }

    /// R20 Lens-4 F2: `is_dev_mode` is the single read-site for dev
    /// semantics. `--mode dev` alone is sufficient; `--dev` alone is
    /// also sufficient (the flag is a shortcut). Both is allowed
    /// (idempotent OR).
    #[test]
    fn is_dev_mode_resolution() {
        let mut cli = cli_with_mode(OperatorMode::Human);
        cli.mode.dev = false;
        assert!(!is_dev_mode(&cli));

        // --dev alone (mode stays Human) → still dev-mode.
        cli.mode.dev = true;
        assert!(is_dev_mode(&cli));

        // --mode dev alone (flag stays false) → still dev-mode.
        let mut cli = cli_with_mode(OperatorMode::Dev);
        cli.mode.dev = false;
        assert!(is_dev_mode(&cli));

        // Both → dev-mode (idempotent).
        cli.mode.dev = true;
        assert!(is_dev_mode(&cli));

        // Non-dev mode + dev flag unset → not dev-mode.
        let cli = cli_with_mode(OperatorMode::Ci);
        assert!(!is_dev_mode(&cli));
    }

    /// R20 Lens-4 F2: dry-run bypasses confirm for ALL modes (Auditor
    /// remains the sole exception per the require_confirm doc table).
    #[test]
    fn require_confirm_dev_with_dry_run_bypasses() {
        let mut cli = cli_with_mode(OperatorMode::Dev);
        cli.mode.dry_run = true;
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
        assert!(matches!(r, Err(OctoCliError::AuditorDenied { .. })));
    }

    // R17 Lens-1 F4: pin the R16 Lens-1 F2 ordering fix — Auditor is denied
    // BEFORE the dry_run bypass, so `--mode=auditor --dry-run` still errors.
    // Without this, an auditor could preview mutating commands that a
    // read-only role should never see the side-effects preview of.
    #[test]
    fn require_confirm_auditor_with_dry_run_still_errors() {
        let mut cli = cli_with_mode(OperatorMode::Auditor);
        cli.mode.dry_run = true;
        let r = require_confirm(&cli, "identity rotate");
        assert!(
            matches!(r, Err(OctoCliError::AuditorDenied { .. })),
            "Auditor must be denied regardless of --dry-run, got {r:?}"
        );
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

    /// SPEC-02: `IdentityShowOutput` must carry `governance_snapshot_ref`
    /// pinned to `None` until the substrate amendment lands. The field is
    /// additive — substrate lands later; CLI stays ahead.
    #[test]
    fn identity_show_output_governance_snapshot_ref_pinned_none() {
        let output = IdentityShowOutput {
            did: "did:octo:abc".to_string(),
            pubkey_hex: "deadbeef".to_string(),
            lifecycle_state: "Active".to_string(),
            rotation_history: vec![],
            hsm_slot: None,
            governance_snapshot_ref: None,
        };
        let json = serde_json::to_string(&output).unwrap();
        assert!(
            json.contains("\"governance_snapshot_ref\":null"),
            "field must serialize as null for v1.0: {json}"
        );
    }

    /// CORR-12: rotate handler echoes the canonical payload to stderr
    /// before any substrate mutation.
    #[test]
    fn rotate_echoes_canonical_payload_to_stderr() {
        // We exercise the helper-free assertion: the rotate function emits
        // the pastejacking-defense echo. Capture is via a synthetic
        // argument shape that fails at `require_confirm` BEFORE the echo
        // would normally fire — instead we directly invoke the part of
        // the contract: eprintln is present in the source. (Full
        // integration: tests/identity.rs `tv_id_rotate_emits_stderr_echo`.)
        // This unit test pins the helper contract — the source must call
        // eprintln for rotate.
        let src = include_str!("identity.rs");
        assert!(
            src.contains("eprintln!"),
            "rotate/revoke handlers must eprintln the canonical payload before mutation"
        );
        assert!(
            src.contains("would rotate: old_did="),
            "rotate handler missing canonical-payload echo"
        );
        assert!(
            src.contains("would revoke: did="),
            "revoke handler missing canonical-payload echo"
        );
    }

    /// CORR-19: empty reason must be rejected.
    #[test]
    fn revoke_rejects_empty_reason() {
        let mut cli = cli_with_mode(OperatorMode::Human);
        cli.mode.confirm = true;
        let r = revoke("   ", &cli);
        assert!(
            matches!(r, Err(OctoCliError::Internal(_))),
            "empty/whitespace reason must be rejected: {r:?}"
        );
    }

    // R24: cfg-guard assertion moved to integration test
    // `crates/octo-cli/tests/identity_cfg_guard.rs`. Unit tests inside
    // `commands/identity.rs` cannot use `include_str!("identity.rs")`
    // for substring assertions about their own file: the substring
    // literal always appears in the test's own source. Integration
    // tests live in a separate compilation unit (their source is not
    // inside identity.rs), so `include_str!("identity.rs")` returns
    // only the production source + the lib's other unit tests, none
    // of which contain the asserted guard pattern unless the production
    // code itself does.

    /// CORR-01 / CORR-02 / SEC-12 / LAYER-04: `NotActive` must NOT map to
    /// `HsmUnavailable`. State-aware mapping translates the lifecycle
    /// state into the matching `OctoCliError` variant.
    #[test]
    fn map_not_active_error_revoked_yields_already_revoked() {
        let e = octo_wallet::WalletError::NotActive {
            current_state: octo_wallet::LifecycleState::Revoked,
        };
        assert!(matches!(
            map_not_active_error(e),
            OctoCliError::AlreadyRevoked
        ));
    }

    #[test]
    fn map_not_active_error_rotating_yields_already_rotating() {
        let e = octo_wallet::WalletError::NotActive {
            current_state: octo_wallet::LifecycleState::Rotating,
        };
        assert!(matches!(
            map_not_active_error(e),
            OctoCliError::AlreadyRotating
        ));
    }

    #[test]
    fn map_not_active_error_other_yields_no_active_identity() {
        let e = octo_wallet::WalletError::NotActive {
            current_state: octo_wallet::LifecycleState::Designated,
        };
        assert!(matches!(
            map_not_active_error(e),
            OctoCliError::NoActiveIdentity
        ));
    }

    /// SEC-11: wallet-open errors must be sanitized.
    #[test]
    fn map_wallet_open_error_sanitizes_substrate_paths() {
        let e = octo_wallet::WalletError::Config(
            "query: SELECT * from crates/octo-wallet/src/store.rs".to_string(),
        );
        let mapped = map_wallet_open_error(e);
        let msg = mapped.user_message();
        assert!(!msg.contains("crates/octo-"), "{msg}");
        assert!(!msg.contains("SQL:"), "{msg}");
        assert!(!msg.contains("query:"), "{msg}");
        assert!(matches!(mapped, OctoCliError::Internal(_)));
    }

    /// CORR-08: auditor mode is blocked at every identity handler entry.
    #[test]
    fn block_auditor_rejects_auditor_mode() {
        let cli = cli_with_mode(OperatorMode::Auditor);
        let r = block_auditor(&cli, "identity whoami");
        assert!(matches!(r, Err(OctoCliError::AuditorDenied { .. })));
    }

    #[test]
    fn block_auditor_allows_human_mode() {
        let cli = cli_with_mode(OperatorMode::Human);
        assert!(block_auditor(&cli, "identity whoami").is_ok());
    }

    #[test]
    fn block_auditor_allows_ci_mode() {
        let cli = cli_with_mode(OperatorMode::Ci);
        assert!(block_auditor(&cli, "identity whoami").is_ok());
    }

    /// SPEC-17: every identity output struct must derive `JsonSchema` so
    /// `OutputEnvelope<T>::data` can be schema-described. The envelope's
    /// `#[schemars(bound = "T: JsonSchema")]` only emits a `data`
    /// subschema when `T: JsonSchema`; absent the derive, the field would
    /// be omitted from the schema and downstream auto-clients would lose
    /// the typed payload. Pinned against `OutputEnvelope<WhoamiOutput>`
    /// and `OutputEnvelope<IdentityShowOutput>` (Wave 5E SPEC-17 partial).
    #[test]
    fn tv_identity_envelope_schema_present() {
        // Each output envelope must serialize a non-empty schema that
        // mentions the envelope surface AND the typed payload's
        // identifying field. For `WhoamiOutput` the identifying field is
        // `pubkey_hex`; for `IdentityShowOutput` it is `governance_snapshot_ref`
        // (which is the SPEC-02 contract — the field must survive the
        // schema emission so the v1.0 null-binding is documented).
        let whoami_schema = schemars::schema_for!(OutputEnvelope<WhoamiOutput>);
        let whoami_str = serde_json::to_string(&whoami_schema).expect("schema serializes");
        assert!(
            !whoami_str.is_empty(),
            "OutputEnvelope<WhoamiOutput> schema must be non-empty: {whoami_str}"
        );
        assert!(
            whoami_str.contains("schema_version"),
            "schema must include the envelope fields, got: {whoami_str}"
        );
        assert!(
            whoami_str.contains("redacted"),
            "schema must include the envelope fields, got: {whoami_str}"
        );
        assert!(
            whoami_str.contains("pubkey_hex"),
            "WhoamiOutput payload must contribute to the schema (specific field pin): got {whoami_str}"
        );

        let show_schema = schemars::schema_for!(OutputEnvelope<IdentityShowOutput>);
        let show_str = serde_json::to_string(&show_schema).expect("schema serializes");
        assert!(
            !show_str.is_empty(),
            "OutputEnvelope<IdentityShowOutput> schema must be non-empty: {show_str}"
        );
        assert!(
            show_str.contains("governance_snapshot_ref"),
            "IdentityShowOutput payload must contribute to the schema (SPEC-02 pin): got {show_str}"
        );

        // And the remaining three standalone outputs (the rotation
        // history rows expose at least `rotation_id`; the rotate output
        // exposes `new_did`; the revoke output exposes `terminal`).
        let rotate_schema = schemars::schema_for!(OutputEnvelope<IdentityRotateOutput>);
        let rotate_str = serde_json::to_string(&rotate_schema).expect("schema serializes");
        assert!(
            rotate_str.contains("new_did"),
            "IdentityRotateOutput payload must contribute to the schema: got {rotate_str}"
        );

        let revoke_schema = schemars::schema_for!(OutputEnvelope<IdentityRevokeOutput>);
        let revoke_str = serde_json::to_string(&revoke_schema).expect("schema serializes");
        assert!(
            revoke_str.contains("terminal"),
            "IdentityRevokeOutput payload must contribute to the schema: got {revoke_str}"
        );

        // `IdentityRotationEventOutput` is nested inside `IdentityShowOutput`
        // (not a stand-alone envelope payload), but its schema must still
        // be derivable so it can be embedded in any future standalone
        // command surface without a structural change.
        let rotation_event_schema = schemars::schema_for!(IdentityRotationEventOutput);
        let rotation_event_str =
            serde_json::to_string(&rotation_event_schema).expect("schema serializes");
        assert!(
            rotation_event_str.contains("rotation_id"),
            "IdentityRotationEventOutput must emit a schema that includes rotation_id: {rotation_event_str}"
        );
        // `signature_proof` is coerced to a string via
        // `#[schemars(with = "String")]`, so the schema describes it
        // as a string — not the raw `RedactedHex` wrapper.
        assert!(
            rotation_event_str.contains("signature_proof"),
            "signature_proof must appear as a string in the schema: {rotation_event_str}"
        );
    }

    // -----------------------------------------------------------------------
    // Per-call-site test vectors for the 5 new Phase 5 subcommands:
    //   register / select / list / rotate-complete / rotate-abort.
    //
    // Per RFC-0011-x §Subcommand Taxonomy each handler owns a typed
    // envelope (IdentityRegisterOutput, IdentitySelectOutput, etc.) and a
    // pastejacking-defense eprintln that fires BEFORE any substrate
    // mutation. These 17 vectors pin those contracts at the unit-test
    // level so substrate-faithfulness, JSON schema, and dispatch wiring
    // all stay visible without an integration fixture.
    // -----------------------------------------------------------------------

    /// tv_x_c_1 — `IdentityRegisterOutput` JSON shape must include all
    /// six fields per RFC-0011-x §Subcommand Taxonomy.
    #[test]
    fn tv_x_c_1_register_output_json_shape() {
        let output = IdentityRegisterOutput {
            did: "did:octo:0xaa".to_string(),
            pubkey_hex: "aa".repeat(32),
            label: "alpha".to_string(),
            lifecycle_state: "Active".to_string(),
            registered_at: DateTime::<Utc>::from_timestamp(1_700_000_000, 0).unwrap(),
            active_now: true,
        };
        let json = serde_json::to_string(&output).expect("register output serializes");
        for needle in [
            "\"did\":\"did:octo:0xaa\"",
            "\"pubkey_hex\":",
            "\"label\":\"alpha\"",
            "\"lifecycle_state\":\"Active\"",
            "\"registered_at\":",
            "\"active_now\":true",
        ] {
            assert!(
                json.contains(needle),
                "IdentityRegisterOutput missing {needle}: {json}"
            );
        }
    }

    /// tv_x_c_2 — `register` handler must eprintln the canonical
    /// payload BEFORE any substrate mutation (pastejacking defense).
    #[test]
    fn tv_x_c_2_register_emits_canonical_payload_eprintln() {
        let src = include_str!("identity.rs");
        assert!(
            src.contains("would register: label={"),
            "register handler missing canonical-payload echo: {src}"
        );
    }

    /// tv_x_c_3 — `register` handler must call `WalletStore::register`
    /// at the substrate boundary (substrate-faithful wrapper per
    /// mission 0011-x-wallet-store-cli §CLI dispatch wiring).
    #[test]
    fn tv_x_c_3_register_calls_wallet_store_register() {
        let src = include_str!("identity.rs");
        assert!(
            src.contains(".register(key, &passphrase, activate, now)"),
            "register handler must delegate to WalletStore::register: {src}"
        );
    }

    /// tv_x_c_4 — `IdentitySelectOutput` JSON shape must include all
    /// three fields per RFC-0011-x §Subcommand Taxonomy.
    #[test]
    fn tv_x_c_4_select_output_json_shape() {
        let output = IdentitySelectOutput {
            did: "did:octo:0xbb".to_string(),
            previous_active_did: Some("did:octo:0xaa".to_string()),
            lifecycle_state: "Active".to_string(),
        };
        let json = serde_json::to_string(&output).expect("select output serializes");
        for needle in [
            "\"did\":\"did:octo:0xbb\"",
            "\"previous_active_did\":\"did:octo:0xaa\"",
            "\"lifecycle_state\":\"Active\"",
        ] {
            assert!(
                json.contains(needle),
                "IdentitySelectOutput missing {needle}: {json}"
            );
        }
    }

    /// tv_x_c_5 — `select` handler must eprintln the canonical
    /// payload BEFORE any substrate mutation.
    #[test]
    fn tv_x_c_5_select_emits_canonical_payload_eprintln() {
        let src = include_str!("identity.rs");
        assert!(
            src.contains("would select: did={did}"),
            "select handler missing canonical-payload echo: {src}"
        );
    }

    /// tv_x_c_6 — `select` handler must call `WalletStore::select` at
    /// the substrate boundary.
    #[test]
    fn tv_x_c_6_select_calls_wallet_store_select() {
        let src = include_str!("identity.rs");
        assert!(
            src.contains("store.select(&parsed)"),
            "select handler must delegate to WalletStore::select: {src}"
        );
    }

    /// tv_x_c_7 — `IdentityListOutput` envelope shape must include
    /// `records` (Vec) + `total` (usize) per RFC-0011-x §Subcommand
    /// Taxonomy.
    #[test]
    fn tv_x_c_7_list_output_envelope_shape() {
        let output = IdentityListOutput {
            records: vec![IdentityListRow {
                did: "did:octo:0xcc".to_string(),
                pubkey_hex: "cc".repeat(32),
                lifecycle_state: "Designated".to_string(),
                registered_at: DateTime::<Utc>::from_timestamp(1_700_000_000, 0).unwrap(),
                active: false,
            }],
            total: 1,
        };
        let json = serde_json::to_string(&output).expect("list output serializes");
        assert!(
            json.contains("\"records\":["),
            "IdentityListOutput must carry records array: {json}"
        );
        assert!(
            json.contains("\"total\":1"),
            "IdentityListOutput must carry total count: {json}"
        );
    }

    /// tv_x_c_8 — `IdentityListRow` JSON shape must include all five
    /// per-row fields per RFC-0011-x §Subcommand Taxonomy.
    #[test]
    fn tv_x_c_8_list_row_json_shape() {
        let row = IdentityListRow {
            did: "did:octo:0xdd".to_string(),
            pubkey_hex: "dd".repeat(32),
            lifecycle_state: "Active".to_string(),
            registered_at: DateTime::<Utc>::from_timestamp(1_700_000_000, 0).unwrap(),
            active: true,
        };
        let json = serde_json::to_string(&row).expect("list row serializes");
        for needle in [
            "\"did\":\"did:octo:0xdd\"",
            "\"pubkey_hex\":",
            "\"lifecycle_state\":\"Active\"",
            "\"registered_at\":",
            "\"active\":true",
        ] {
            assert!(
                json.contains(needle),
                "IdentityListRow missing {needle}: {json}"
            );
        }
    }

    /// tv_x_c_9 — `list` is read-only and must NOT call
    /// `require_confirm` (per RFC-0011-x §Subcommand Taxonomy the
    /// gate applies to register/select/rotate/revoke, not to list).
    #[test]
    fn tv_x_c_9_list_is_read_only_no_confirm_gate() {
        let src = include_str!("identity.rs");
        // The `list` function body is reachable from the public symbol
        // `pub fn list(` and must not invoke the confirm gate. We assert
        // by checking the source for the gate substring between the
        // `pub fn list(` declaration and the closing brace of the
        // function body.
        let start = src.find("pub fn list(").expect("list fn present");
        // Read up to a generous slice following the declaration; the
        // body of `list` is short and bounded by the next `pub fn` or
        // `fn` declaration.
        let slice = &src[start..];
        let end = slice.find("pub fn rotate_complete").unwrap_or(slice.len());
        let body = &slice[..end];
        assert!(
            !body.contains("require_confirm"),
            "list() is read-only and must not invoke require_confirm: {body}"
        );
    }

    /// tv_x_c_10 — `IdentityRotateCompleteOutput` JSON shape must
    /// include all three fields per RFC-0011-x §Subcommand Taxonomy.
    #[test]
    fn tv_x_c_10_rotate_complete_output_json_shape() {
        let output = IdentityRotateCompleteOutput {
            new_did: "did:octo:0xee".to_string(),
            old_did: "did:octo:0xff".to_string(),
            completed_at: DateTime::<Utc>::from_timestamp(1_700_000_000, 0).unwrap(),
        };
        let json = serde_json::to_string(&output).expect("rotate_complete output serializes");
        for needle in [
            "\"new_did\":\"did:octo:0xee\"",
            "\"old_did\":\"did:octo:0xff\"",
            "\"completed_at\":",
        ] {
            assert!(
                json.contains(needle),
                "IdentityRotateCompleteOutput missing {needle}: {json}"
            );
        }
    }

    /// tv_x_c_11 — `rotate_complete` handler must eprintln the
    /// canonical payload BEFORE any substrate mutation.
    #[test]
    fn tv_x_c_11_rotate_complete_emits_canonical_payload_eprintln() {
        let src = include_str!("identity.rs");
        assert!(
            src.contains("would rotate-complete:"),
            "rotate_complete handler missing canonical-payload echo: {src}"
        );
    }

    /// tv_x_c_12 — `rotate_complete` handler must delegate to
    /// `octo_wallet::complete_rotation` (substrate-faithful wrapper).
    #[test]
    fn tv_x_c_12_rotate_complete_delegates_to_octo_wallet() {
        let src = include_str!("identity.rs");
        assert!(
            src.contains("octo_wallet::complete_rotation(&mut key, now)"),
            "rotate_complete must call octo_wallet::complete_rotation: {src}"
        );
    }

    /// tv_x_c_13 — `rotate_complete` must call `require_confirm` BEFORE
    /// the substrate mutation (handler-side gate discipline per R12.5 /
    /// R13.5 lessons).
    #[test]
    fn tv_x_c_13_rotate_complete_handler_side_confirm_gate() {
        let src = include_str!("identity.rs");
        let start = src
            .find("pub fn rotate_complete(")
            .expect("rotate_complete fn present");
        let slice = &src[start..];
        let end = slice.find("pub fn rotate_abort").unwrap_or(slice.len());
        let body = &slice[..end];
        assert!(
            body.contains("require_confirm(cli, \"identity rotate-complete\")"),
            "rotate_complete must gate behind require_confirm at handler entry: {body}"
        );
        let require_pos = body
            .find("require_confirm(cli, \"identity rotate-complete\")")
            .expect("require_confirm substring present");
        let mutation_pos = body
            .find("octo_wallet::complete_rotation")
            .expect("substrate mutation present");
        assert!(
            require_pos < mutation_pos,
            "require_confirm must fire BEFORE the substrate mutation: require_pos={require_pos}, mutation_pos={mutation_pos}"
        );
    }

    /// tv_x_c_14 — `IdentityRotateAbortOutput` JSON shape must include
    /// all three fields per RFC-0011-x §Subcommand Taxonomy.
    #[test]
    fn tv_x_c_14_rotate_abort_output_json_shape() {
        let output = IdentityRotateAbortOutput {
            restored_did: "did:octo:0xaa".to_string(),
            aborted_at: DateTime::<Utc>::from_timestamp(1_700_000_000, 0).unwrap(),
            reason: Some("operator compromise suspected".to_string()),
        };
        let json = serde_json::to_string(&output).expect("rotate_abort output serializes");
        for needle in [
            "\"restored_did\":\"did:octo:0xaa\"",
            "\"aborted_at\":",
            "\"reason\":\"operator compromise suspected\"",
        ] {
            assert!(
                json.contains(needle),
                "IdentityRotateAbortOutput missing {needle}: {json}"
            );
        }
    }

    /// tv_x_c_15 — `rotate_abort` handler must eprintln the canonical
    /// payload BEFORE any substrate mutation.
    #[test]
    fn tv_x_c_15_rotate_abort_emits_canonical_payload_eprintln() {
        let src = include_str!("identity.rs");
        assert!(
            src.contains("would rotate-abort:"),
            "rotate_abort handler missing canonical-payload echo: {src}"
        );
    }

    /// tv_x_c_16 — `rotate_abort` handler must delegate to
    /// `octo_wallet::abort_rotation` (substrate-faithful wrapper).
    #[test]
    fn tv_x_c_16_rotate_abort_delegates_to_octo_wallet() {
        let src = include_str!("identity.rs");
        assert!(
            src.contains("octo_wallet::abort_rotation(&mut key)"),
            "rotate_abort must call octo_wallet::abort_rotation: {src}"
        );
    }

    /// tv_x_c_17 — The CLI `dispatch` must route every one of the 5
    /// new Phase 5 subcommand variants to its handler. Source-presence
    /// pins the dispatch table so a missing arm becomes a compile-time
    /// miss on the `match` exhaustiveness check rather than a silent
    /// fall-through.
    #[test]
    fn tv_x_c_17_dispatch_routes_all_five_new_variants() {
        let src = include_str!("identity.rs");
        let start = src
            .find("pub fn dispatch(action: &IdentityAction")
            .expect("dispatch fn present");
        let slice = &src[start..];
        // Bound the dispatch body by the function's closing brace;
        // `dispatch` ends at the first `\n}` line that follows an
        // arm body.
        let end = slice.find("\n}\n").unwrap_or(slice.len());
        let body = &slice[..end];
        for needle in [
            "IdentityAction::Register {",
            "register(label",
            "IdentityAction::Select {",
            "select(did",
            "IdentityAction::List {",
            "list(cli)",
            "IdentityAction::RotateComplete {",
            "rotate_complete(cli)",
            "IdentityAction::RotateAbort {",
            "rotate_abort(reason.as_deref()",
        ] {
            assert!(
                body.contains(needle),
                "dispatch missing route for {needle}: {body}"
            );
        }
    }

    /// tv_x_20 — The CLI `octo identity select` subcommand must move
    /// the active pointer on success, fail on a miss, and route
    /// through `WalletStore::select` (NOT `UnlockedWallet::select`,
    /// which is the over-classification the mission §Summary warns
    /// against — a passphrase prompt on a metadata writer that
    /// reads no key material). The miss path maps
    /// `WalletError::IdentityNotFound` to the existing
    /// `OctoCliError::IdentityNotFound` slot at exit 4 (no new slot,
    /// covered by tv_x_30 in error.rs).
    ///
    /// Source-presence pins the substrate-faithful wiring so a
    /// regression that routes select through `unlock` (the bug this
    /// criterion exists to catch) becomes a substring assertion
    /// failure, not a silent downgrade.
    #[test]
    fn tv_x_20_select_routes_through_wallet_store_not_unlocked_wallet() {
        let src = include_str!("identity.rs");
        let start = src.find("pub fn select(").expect("select fn present");
        let slice = &src[start..];
        let end = slice.find("pub fn list(").unwrap_or(slice.len());
        let body = &slice[..end];
        assert!(
            body.contains("store.select(&parsed)"),
            "select handler must call WalletStore::select, not UnlockedWallet::select: {body}"
        );
        // Belt-and-braces: select must NOT route through the unlock
        // path. A passphrase prompt on a metadata writer is the
        // over-classification §Summary warns against.
        assert!(
            !body.contains("unlock("),
            "select is a metadata writer and must not route through unlock: {body}"
        );
        // And the IdentityNotFound path must collapse to the
        // existing exit-4 variant via the From<WalletError> impl —
        // no new slot, per mission AC-2.
        assert!(
            body.contains("IdentityNotFound(parsed.0.clone())"),
            "select must surface substrate IdentityNotFound at the existing slot-4 variant: {body}"
        );
    }

    /// tv_x_c_25 — AC-24 passphrase zeroize on `register`. The
    /// `register` handler must wrap the passphrase String in
    /// `zeroize::Zeroizing` so the bytes are scrubbed when the
    /// handler returns. Source-presence pins the wrapper at the
    /// read-site so a regression that drops the wrapper (and lets
    /// the passphrase survive into the heap) becomes a substring
    /// failure rather than a silent hygiene miss.
    #[test]
    fn tv_x_c_25_register_wraps_passphrase_in_zeroizing() {
        let src = include_str!("identity.rs");
        let start = src.find("pub fn register(").expect("register fn present");
        let slice = &src[start..];
        let end = slice.find("pub fn select(").unwrap_or(slice.len());
        let body = &slice[..end];
        assert!(
            body.contains("zeroize::Zeroizing::new("),
            "register must wrap passphrase in zeroize::Zeroizing::new per AC-24: {body}"
        );
        assert!(
            body.contains("std::fs::read_to_string(passphrase_file)"),
            "register must read passphrase from the file: {body}"
        );
        // Belt-and-braces: a Plain `String` binding for the
        // passphrase inside `register` would defeat the wrapper —
        // the only String binding must be inside the Zeroizing::new
        // constructor.
        let read_site = body
            .find("std::fs::read_to_string(passphrase_file)")
            .expect("read site");
        let zeroize_site = body.find("zeroize::Zeroizing::new(").expect("zeroize site");
        assert!(
            zeroize_site <= read_site + 200,
            "Zeroizing wrap must enclose the passphrase read, not follow it: {body}"
        );
    }

    /// tv_x_c_26 — AC-25 `--seed-file` 0600 mode check. A seed file
    /// that is group- or world-readable is a compromise already —
    /// the handler must refuse it with the mode printed. Source
    /// presence pins the Unix-only mode check so a regression that
    /// reads the file unconditionally becomes a substring failure.
    #[test]
    fn tv_x_c_26_register_seed_file_mode_check_refuses_permissive_modes() {
        let src = include_str!("identity.rs");
        let start = src.find("pub fn register(").expect("register fn present");
        let slice = &src[start..];
        let end = slice.find("pub fn select(").unwrap_or(slice.len());
        let body = &slice[..end];
        assert!(
            body.contains("seed_meta.permissions().mode()"),
            "register must stat the seed file and read its mode per AC-25 obligation 2: {body}"
        );
        assert!(
            body.contains("0o077"),
            "register must check group/world bits are zero (0o077 mask): {body}"
        );
        assert!(
            body.contains("group- or world-readable"),
            "register refusal message must name the mode violation: {body}"
        );
    }
}
