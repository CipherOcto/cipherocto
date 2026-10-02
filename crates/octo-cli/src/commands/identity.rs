//! `octo identity` + `octo whoami` — RFC-0011 §Subcommand Taxonomy.
//!
//! Wave 3 implementation per mission `0011-identity-commands`.
//!
//! - `octo whoami` — read-only (exit 0/2/4)
//! - `octo identity show [DID]` — read-only (exit 0/2/4)
//! - `octo identity rotate` — write (exit 0/2/3/4/6/15/43/64/92)
//! - `octo identity revoke` — write (exit 0/2/4/6/15/43/64/92)
//!
//! Layer C/D orchestrator. Consumes substrate via `octo_wallet::WalletStore`
//! and the free fns `identity_record_fn`, `begin_rotation`,
//! and `revoke`. `signature_proof` is rendered through the `RedactedHex`
//! wrapper.

use chrono::{DateTime, Utc};
use clap::Subcommand;
use serde::Serialize;

use octo_cap_macaroon::signer::{CapabilitySigner, CapabilitySignerError};

use crate::commands::role::SignerHandle;
use crate::error::sanitize_substrate_error;
use crate::error::OctoCliError;
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
    Rotate {
        /// Read the wallet passphrase from stdin (one line,
        /// trailing newline trimmed). Bypasses the interactive
        /// TTY prompt for CI / scripted callers. Required when
        /// stdin is not a TTY (mission 0011-x-wallet-store-cli
        /// §AC-10). Used by the signing-site migration to
        /// `UnlockedWallet::begin_rotation`.
        #[arg(long)]
        passphrase_stdin: bool,
    },
    /// Revoke the active identity.
    Revoke {
        /// Why the identity is being revoked. Required and must be
        /// non-empty, and is echoed back to the operator, but the v1.0
        /// substrate carries NO reason field on `IdentityRecord`, so
        /// the value is NOT persisted to disk.
        #[arg(long)]
        reason: String,
        /// Read the wallet passphrase from stdin (one line,
        /// trailing newline trimmed). Bypasses the interactive
        /// TTY prompt for CI / scripted callers. Required when
        /// stdin is not a TTY (mission 0011-x-wallet-store-cli
        /// §AC-10). Used by the signing-site migration to
        /// `UnlockedWallet::revoke`.
        #[arg(long)]
        passphrase_stdin: bool,
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
    /// §AC-28), `--activate` flag (default true; effective only on a
    /// wallet with no active identity), `--seed-file <path>` optional (random
    /// CSPRNG when absent). Confirmation gate applies per
    /// `require_confirm`. Exit codes: 0 / 2 / 6 / 27 / 64.
    ///
    /// Two codes an earlier revision listed here are NOT reachable
    /// from this subcommand. 92 (`WalletLocked`) needs
    /// `VaultSlotNotFound` / `VaultDecryptionFailed`, which only
    /// `Vault::get` raises, and `register` only calls `Vault::put` -
    /// it seals rather than reads. 5 (`Hsm`) needs a signer adapter
    /// other than `InMemorySigner`, and every constructor reachable
    /// here hard-codes `InMemorySigner`.
    Register {
        /// Operator-chosen label for the new identity. It is echoed
        /// back in the register envelope and NOWHERE ELSE: the v1.0
        /// `IdentityRecord` carries no label field, so the value is
        /// NOT persisted to disk and does not appear in `identity
        /// list` or `identity show`. Stated explicitly because
        /// `revoke --reason` and `rotate-abort --reason` both carry
        /// the same note, and a label that appears in the register
        /// payload reads as stored.
        #[arg(long)]
        label: String,
        /// Path to a file containing the passphrase. Required
        /// (passphrase is length-floor gated at the substrate).
        #[arg(long, value_name = "PATH")]
        passphrase_file: std::path::PathBuf,
        /// Promote the new identity to `Active` immediately. The flag
        /// defaults to true, and `WalletStore::register` promotes the
        /// lifecycle whenever it is set - independently of the active
        /// pointer. The POINTER is the separate condition: it is set
        /// only when the wallet has none, so a second
        /// `register --activate` leaves the first identity pointed at
        /// and the new one `Active` but not selected. Both identities
        /// are then live and `select` chooses between them; the
        /// envelope's `active_now` reports which one is pointed at. Use
        /// `octo identity select` to move the pointer. The
        /// `active_now` field of the output envelope reports what the
        /// store actually holds, not what this flag asked for.
        // Set rather than clap's default SetTrue for a bool long:
        // SetTrue accepts only bare --activate and rejects
        // --activate=false, which left no way to register a
        // Designated record even though Designated is a state select
        // accepts and whoami, show and list all report.
        // default_missing_value keeps bare --activate meaning true.
        #[arg(
            long,
            action = clap::ArgAction::Set,
            num_args = 0..=1,
            default_value_t = true,
            default_missing_value = "true"
        )]
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
    /// confirmation gate, but it IS blocked in auditor mode, since
    /// it reports a strict superset of what `identity show` reports.
    /// Exit codes: 0 / 2 / 64.
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
    /// 0 / 2 / 4 / 6 / 15 / 43 / 64 / 92.
    RotateComplete {
        /// Read the wallet passphrase from stdin (one line, trailing
        /// newline trimmed). Bypasses the interactive TTY prompt
        /// for CI / scripted callers. Required when stdin is not a
        /// TTY (mission 0011-x-wallet-store-cli §AC-10). The
        /// passphrase is wiped from the buffer at the read site
        /// via `Zeroizing` (mission §AC-24).
        #[arg(long)]
        passphrase_stdin: bool,
    },
    /// Abort an in-flight key rotation
    /// (RFC-0011-x §Subcommand Taxonomy). Substrate-faithful
    /// wrapper over `WalletStore::abort_rotation`. Clap name
    /// `rotate-abort`. Removes the successor record and restores
    /// the predecessor's `Active` lifecycle. Exit codes:
    /// 0 / 2 / 4 / 6 / 15 / 43 / 64 / 92.
    RotateAbort {
        /// Optional free-form justification for the abort. Echoed to
        /// stderr and into the envelope via `redact_string`. It is NOT
        /// persisted: neither `IdentityRecord` nor
        /// `IdentityRotationEvent` carries a reason field, and
        /// `WalletStore::abort_rotation` takes no reason argument, so
        /// there is no audit-log write to receive it.
        ///
        /// `redact_string` removes values shaped like a JSON, YAML or
        /// key/value field name and `bearer`/long-hex tokens. It does
        /// NOT recognise a bare high-entropy secret - a `ghp_`-style
        /// token pasted here is emitted verbatim. The value is not
        /// persisted, but it IS written to stderr and into the
        /// rendered envelope, so do not paste a credential.
        #[arg(long)]
        reason: Option<String>,
        /// Read the wallet passphrase from stdin (one line,
        /// trailing newline trimmed). Bypasses the interactive
        /// TTY prompt for CI / scripted callers. Required when
        /// stdin is not a TTY (mission 0011-x-wallet-store-cli
        /// §AC-10).
        #[arg(long)]
        passphrase_stdin: bool,
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
    pub registered_at: Option<DateTime<Utc>>,
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
    pub started_at: Option<DateTime<Utc>>,
    /// RFC 3339 UTC timestamp of grace expiry (24h after start, hard-coded
    /// in substrate via `ROTATION_GRACE_PERIOD_SECS`).
    pub grace_expires_at: Option<DateTime<Utc>>,
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
    /// DID of the new (successor) identity, derived locally from the
    /// successor key.
    ///
    /// This is a REAL DID on every path, `--dry-run` included. The
    /// handler derives the successor from its seed before it reaches
    /// the dry-run branch, so the preview names the identity a
    /// committed run would promote. It is never a `pending`
    /// placeholder and never a `<not-read: dry-run>` sentinel -
    /// the sentinel is used only for `lifecycle_state` in `register`
    /// and `select`, which genuinely do not read the substrate under
    /// `--dry-run`.
    ///
    /// Two earlier revisions documented this field wrongly, which is
    /// not harmless: `schemars` copies this doc verbatim into the
    /// generated JSON Schema, so a false claim here ships to every
    /// consumer of the schema as if it were the contract. A prior
    /// revision said it was always `pending`. The revision this
    /// replaces said it was `<not-read: dry-run>` under `--dry-run`,
    /// which is also false. If this behaviour ever changes, change
    /// this doc in the same commit, and run the vector that pins the
    /// rendered value.
    pub new_did: String,
    /// DID of the rotated-out identity.
    pub old_did: String,
    /// RFC 3339 UTC timestamp of grace expiry.
    pub grace_expires_at: Option<DateTime<Utc>>,
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
    pub revoked_at: Option<DateTime<Utc>>,
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
    /// Lifecycle label after registration (`Designated` or `Active`),
    /// or `<not-read: dry-run>` under `--dry-run` - a sentinel
    /// outside the documented set, because a preview performs no
    /// read and the empty string is not a member of that set.
    pub lifecycle_state: String,
    /// RFC 3339 UTC timestamp of registration (caller-supplied
    /// `now_unix` to keep substrate-faithful determinism), or `null`
    /// under `--dry-run`. It was previously non-optional and
    /// rendered the epoch, `1970-01-01T00:00:00Z`, which is a valid
    /// timestamp and so indistinguishable from a real registration
    /// at the epoch.
    pub registered_at: Option<DateTime<Utc>>,
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
    pub registered_at: Option<DateTime<Utc>>,
    /// Whether the wallet's active POINTER names this row.
    ///
    /// This is pointer equality with `WalletStore::active_did`,
    /// not a claim that the identity is usable. `revoke` does not
    /// move the pointer, so a wallet whose active identity has been
    /// revoked reports exactly one row with `active: true` and
    /// `lifecycle_state: "Revoked"`. Both fields are true at once
    /// and a consumer must read both: select on `active` to find
    /// the pointer, and check `lifecycle_state` before using it.
    /// The field was documented as "whether this row is the current
    /// active identity", which reads as liveness and silently
    /// invites the revoked-pointer mistake.
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
    pub completed_at: Option<DateTime<Utc>>,
}

/// Render a substrate Unix timestamp, or `None` when the value cannot
/// be a real one.
///
/// The substrate stores timestamps as `i64` seconds, and the previous
/// revision rendered every unconvertible value as
/// `from_timestamp(0, 0)` - 1970-01-01T00:00:00Z, which is a VALID
/// RFC 3339 timestamp and therefore indistinguishable from a record
/// genuinely created at the epoch. `register` had already been
/// hardened against this for its own `registered_at`, using an `i64::MIN`
/// sentinel and an `Option` field so the envelope carries `null`. Its
/// four siblings on the read paths were not, so a corrupt or
/// out-of-range timestamp surfaced as real-looking data.
///
/// `None` is the honest answer: the envelope then says "the substrate
/// holds a value I could not render", which a consumer can act on, and
/// which cannot be confused with data.
fn unix_to_rfc3339(unix: i64) -> Option<DateTime<Utc>> {
    DateTime::<Utc>::from_timestamp(unix, 0)
}

/// `octo identity rotate-abort` payload (RFC-0011-x §Subcommand
/// Taxonomy).
#[derive(Serialize, Debug, Clone, schemars::JsonSchema)]
pub struct IdentityRotateAbortOutput {
    /// DID of the predecessor, restored to `Active` after abort.
    pub restored_did: String,
    /// RFC 3339 UTC timestamp at which the abort was persisted.
    pub aborted_at: Option<DateTime<Utc>>,
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
/// Map a `WalletStore::open` failure.
///
/// This is the OPEN path specifically, and the distinction matters.
/// `WalletError::Config` means many things across the substrate - a
/// full disk, an Argon2 parameter failure - so the general
/// `From<WalletError>` wildcard correctly leaves it at `Internal`.
/// But `WalletStore::open` has exactly ONE `Config` it can return,
/// and it is the one `open_at` never produces: "neither $OCTO_HOME
/// nor $HOME resolves to a non-empty path". That is the caller's
/// environment, not a defect, and exit 64 tells the operator to
/// "re-run with RUST_LOG=debug and report the diagnostic" - a false
/// escalation for a missing environment variable, and in CI the exit
/// code is what decides retry versus page. Exit 27
/// (`NoOctoHome`) exists precisely for this.
///
/// These five subcommands never call `home::resolve`, so the CLI's
/// own home resolution - which is correct, and is covered by its own
/// vectors - does not protect this path. The substrate resolves the
/// home independently, and that second resolution is the one on the
/// line below. It was joining onto the empty-home sentinel rather
/// than propagating it, so the `Config` this arm matches was
/// unreachable AND the store was being created relative to the
/// operator's working directory. The substrate now propagates the
/// sentinel (`wallet_root_under_home`, vector tv_x_56), which is what
/// makes this mapping live rather than dead code.
///
/// Everything else from `open` is `Io` or crypto, which is a real
/// fault and stays at 64.
pub(crate) fn map_wallet_open_error(e: octo_wallet::WalletError) -> OctoCliError {
    match e {
        octo_wallet::WalletError::Config(_) => OctoCliError::NoOctoHome,
        other => OctoCliError::Internal(sanitize_substrate_error(&format!(
            "wallet store open: {other}"
        ))),
    }
}

/// Map `WalletError::NotActive` to the appropriate `OctoCliError` based on
/// the lifecycle state substrate reported.
///
/// Per R1 review LAYER-04, the CLI does NOT pre-decide rotation/revocation
/// eligibility from `lifecycle` (which would leak Layer C → B). The CLI
/// trusts substrate's `NotActive { current_state }` and translates
/// `Revoked` / `Rotating` to the matching operator-facing variant.
#[cfg(test)]
fn map_not_active_error(e: octo_wallet::WalletError) -> OctoCliError {
    match e {
        octo_wallet::WalletError::NotActive {
            current_state: octo_wallet::LifecycleState::Revoked,
        } => OctoCliError::AlreadyRevoked,
        octo_wallet::WalletError::NotActive {
            current_state: octo_wallet::LifecycleState::Rotating,
        } => OctoCliError::AlreadyRotating,
        octo_wallet::WalletError::NotActive { .. } => OctoCliError::NoActiveIdentity,
        octo_wallet::WalletError::Hsm(_) => crate::error::map_hsm_error(&e.to_string()),
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

// AC-1 inventory: WalletStore::open() site classifications for this file.
// Every site below is reachable from a public CLI subcommand or the
// helper layer the subcommand routes through. The classification
// distinguishes metadata reads (no key material touched, only the
// store's index) from signing (the active identity's Ed25519 key is
// used to sign, derive, or mutate state that requires the key).
//
// Site 1  identity.rs whoami              metadata  (exit 0/2/64)
// Site 2  identity.rs identity show       metadata  (exit 0/2/4/64)
// Site 3  identity.rs identity register   signing   (exit 0/2/4/5/11/64)
// Site 4  identity.rs identity register   signing   (activate=true branch)
// Site 5  identity.rs identity select     metadata  (moves active pointer)
// Site 6  identity.rs identity select     metadata  (activate-on-select path)
// Site 7  identity.rs identity rotate     signing   (UnlockedWallet path)
// Site 8  identity.rs rotate complete     signing   (UnlockedWallet path)
// Site 9  identity.rs rotate abort        metadata  (reverts state, no key use)
// Site 10 identity.rs identity revoke     signing   (UnlockedWallet path)
// Site 11 identity.rs identity revoke     signing   (alt path with explicit did)

/// `octo whoami` — surface the active identity record.
///
/// Every field in `WhoamiOutput` — DID, public key, lifecycle, HSM slot,
/// registration time — lives in the `IdentityRecord`, which the store
/// serves **without a passphrase**. This is therefore a metadata
/// command, not a signing one: prompting an operator for a secret to
/// print a public key would be over-classification, the exact failure
/// mode RFC-0011-x §Unlock split warns about.
///
/// The active DID is read through `WalletStore::active_did` rather than
/// `try_active_identity`. The latter is the *signing* primitive: under
/// the unlock split it returns `Err(WalletError::Locked)`
/// unconditionally, so a metadata command routed through it could only
/// ever exit 64. `active_did` returns `None` when no identity is
/// active, which is the condition exit 2 is actually for.
///
/// Exit codes:
/// - 0: success
/// - 2: no active identity
/// - 64: unexpected substrate error (wallet store open failure, lookup
///   failure, etc.)
pub fn whoami(cli: &Octo) -> Result<(), OctoCliError> {
    block_auditor(cli, "identity whoami")?;
    let store = octo_wallet::WalletStore::open().map_err(map_wallet_open_error)?;
    let did = store
        .active_did()
        .cloned()
        .ok_or(OctoCliError::NoActiveIdentity)?;
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
        registered_at: unix_to_rfc3339(record.registered_at_unix),
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
        // Metadata read: the active DID is a store index field, not a
        // key. `try_active_identity` is the signing primitive and
        // returns `Locked` unconditionally under the unlock split.
        None => store
            .active_did()
            .cloned()
            .ok_or(OctoCliError::NoActiveIdentity)?,
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
                started_at: unix_to_rfc3339(e.started_at_unix),
                grace_expires_at: unix_to_rfc3339(e.grace_expires_at_unix),
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

/// `octo identity rotate --passphrase-stdin` — initiate a key rotation.
///
/// Requires `--confirm` in human mode, `--allow-write` in CI mode (or
/// `--dry-run` for preview). The handler migrates to
/// `UnlockedWallet::begin_rotation` per mission §AC-7 (signing-site
/// migration); the predecessor's seed is held only by the unlocked
/// handle's lifetime. Passphrase acquisition via `acquire_passphrase`
/// per §AC-9 + §AC-10.
pub fn rotate(passphrase_stdin: bool, cli: &Octo) -> Result<(), OctoCliError> {
    require_confirm(cli, "identity rotate")?;
    let store = octo_wallet::WalletStore::open().map_err(map_wallet_open_error)?;

    // Pastejacking defense (R1 review CORR-12): before any irreversible
    // substrate mutation, echo the canonical payload to stderr. The
    // operator (or automation) running this command can then visually
    // confirm the DID + grace window matches what they intend. Fires
    // BEFORE `active_identity()` so the echo always emits, even when
    // no identity is active (the placeholder makes the absence explicit).
    let old_did = match store.active_did().cloned() {
        Some(d) => d,
        None => {
            eprintln!("would rotate: old_did=<none>, no successor key, grace=24h",);
            return Err(OctoCliError::NoActiveIdentity);
        }
    };

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
        // Not `Internal`: the message names its own remedy, so
        // telling the operator to file a diagnostic contradicts it
        // on the same screen, and exit 64 is the page-someone code
        // in CI. This is an operator choosing a mode, so it joins
        // the operator-input family at exit 2.
        if !cli.mode.dry_run && !is_dev_mode(cli) {
            return Err(OctoCliError::DevModeRequired {
                detail: "successor derivation is refused outside dev mode; use --dry-run for \
                         previews, or re-run with --mode dev for test signing"
                    .to_string(),
            });
        }
    }
    let successor = octo_wallet::IdentityKey::from_seed([1u8; 32]);
    // Capture the successor DID BEFORE `begin_rotation` consumes
    // `successor`. The envelope used to hard-code
    // `new_did: "did:octo:pending"` on every path including the
    // committed one, so a real rotation reported a placeholder to
    // the operator while the store had already recorded the
    // successor. A `--dry-run` preview is the only case where the
    // placeholder is truthful, and it is rendered from the same
    // capture below so the two paths cannot drift.
    let new_did = successor.did().0.clone();
    // Pastejacking preview, printed AFTER the successor DID is known
    // so it can report the real value. It used to print
    // `new_did_placeholder=pending` on every run - including the
    // committed one, which returned the real DID in the same second -
    // and it fired before the dev gate and before the passphrase
    // acquisition, so a command that then failed still printed a
    // rotation preview it never performed.
    eprintln!(
        "would rotate: old_did={}, new_did={}, grace=24h",
        old_did.0, new_did
    );
    let now = chrono::Utc::now().timestamp().max(0) as u64;
    let proof = if cli.mode.dry_run {
        [0u8; 64]
    } else {
        // §AC-7 signing-site migration: open the store, then
        // unlock with the passphrase to mint an `UnlockedWallet`
        // handle. The handle's `begin_rotation(successor,
        // passphrase, now)` is the substrate-faithful signing
        // path; the predecessor's seed lifetime is exactly the
        // handle's lifetime, and the substrate uses the
        // passphrase to seal the successor's slot.
        let passphrase = acquire_passphrase(cli, "identity rotate", passphrase_stdin)?;
        let mut store = octo_wallet::WalletStore::open().map_err(map_wallet_open_error)?;
        let mut seed_buf = zeroize::Zeroizing::new(Vec::with_capacity(32));
        let mut unlocked = store
            .unlock(passphrase.as_str(), seed_buf.as_mut())
            .map_err(OctoCliError::from)?;
        unlocked
            .begin_rotation(successor, passphrase.as_str(), now)
            .map_err(OctoCliError::from)?
    };
    // The grace window is the substrate's policy, not the CLI's. It
    // was spelled as a literal `86_400` here while the substrate
    // exports `ROTATION_GRACE_PERIOD_SECS`; the two agreed, so nothing
    // caught a change to one of them, and the envelope would then
    // disagree with the store about when a rotation completes.
    //
    // `null` under `--dry-run`, and this is the same repair the
    // rotate-complete path already carries for `completed_at`. The
    // window was computed unconditionally, so a PREVIEW carried a
    // real expiry instant for a rotation that had not started. An
    // operator reading a dry run saw a deadline and a DID and could
    // reasonably believe the rotation was under way. The substrate
    // was never called, so there is no window, and the field says so.
    let grace_expires_at = if cli.mode.dry_run {
        None
    } else {
        unix_to_rfc3339(
            now as i64
                + i64::try_from(octo_wallet::identity::ROTATION_GRACE_PERIOD_SECS)
                    .unwrap_or(i64::MAX),
        )
    };
    let output = IdentityRotateOutput {
        new_did,
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

/// `octo identity revoke --reason <str> --passphrase-stdin` —
/// burn the active identity.
///
/// `reason` is REQUIRED (clap enforces) AND must be non-empty (R1 review
/// CORR-19). Absent → clap exit 2 usage error; empty → `InvalidReason`
/// (exit 2), refused by the handler's own `trim().is_empty()` check -
/// not by a substrate `validate_reason` call, the substrate takes no
/// reason at all. The handler migrates to
/// `UnlockedWallet::revoke` per mission §AC-7 (signing-site migration);
/// the predecessor's seed is held only by the unlocked handle's
/// lifetime. Passphrase acquisition via `acquire_passphrase` per
/// §AC-9 + §AC-10.
pub fn revoke(reason: &str, passphrase_stdin: bool, cli: &Octo) -> Result<(), OctoCliError> {
    if reason.trim().is_empty() {
        // Operator input, not a substrate fault. This is the same
        // class as `WalletError::ReasonTooLong` and
        // `ReasonContainsControlChars`, which the R4 sweep routed to
        // `InvalidReason` at exit 2; this sibling guard was left on
        // `Internal`, so a typo produced "unexpected substrate error"
        // at exit 64. `ClapParse`, `WeakPassphrase` and `InvalidReason`
        // all follow the exit-2 convention.
        return Err(OctoCliError::InvalidReason {
            detail: "revocation reason must be non-empty".to_string(),
        });
    }
    require_confirm(cli, "identity revoke")?;
    let store = octo_wallet::WalletStore::open().map_err(map_wallet_open_error)?;
    // Pastejacking defense (R1 review CORR-12): echo BEFORE resolving
    // active identity so the operator sees the canonical payload even
    // when no identity is active.
    let did = match store.active_did().cloned() {
        Some(d) => d,
        None => {
            eprintln!("would revoke: did=<none>, reason={}", redact_string(reason));
            return Err(OctoCliError::NoActiveIdentity);
        }
    };
    eprintln!(
        "would revoke: did={}, reason={}",
        did.0,
        redact_string(reason)
    );

    let mut revoked_at = None;
    if !cli.mode.dry_run {
        // The clock is read INSIDE the guard, not above it. Read
        // above, it produced a real RFC 3339 instant for a
        // revocation that had not happened, and a preview is exactly
        // where a plausible-looking timestamp does the most damage:
        // the operator has just been told in words that nothing was
        // done, and the payload beside that sentence says when it was
        // done. `null` is the honest answer and is the form the
        // register and rotate-complete paths already use.
        let now = chrono::Utc::now().timestamp().max(0) as u64;
        // §AC-7 signing-site migration: open the store, then
        // unlock with the passphrase to mint an `UnlockedWallet`
        // handle. The handle's `revoke(now)` is the
        // substrate-faithful signing path; the predecessor's seed
        // lifetime is exactly the handle's lifetime.
        let passphrase = acquire_passphrase(cli, "identity revoke", passphrase_stdin)?;
        let mut store = octo_wallet::WalletStore::open().map_err(map_wallet_open_error)?;
        let mut seed_buf = zeroize::Zeroizing::new(Vec::with_capacity(32));
        let mut unlocked = store
            .unlock(passphrase.as_str(), seed_buf.as_mut())
            .map_err(OctoCliError::from)?;
        unlocked.revoke(now).map_err(OctoCliError::from)?;
        revoked_at = unix_to_rfc3339(now as i64);
    }
    // The reason is echoed to stderr and into the envelope, and then
    // dropped: the v1.0 substrate has nowhere to put it.
    // `UnlockedWallet::revoke` takes only a wall-clock timestamp, and
    // `IdentityRecord` carries no reason field, so persisting it would
    // mean changing the on-disk record schema. Until that lands an
    // operator must keep their own copy - the wallet retains nothing.
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
/// Exit codes: 0 / 2 / 5 / 6 / 64 / 92, derived from the arms
/// `From<WalletError>` actually provides for the variants
/// `WalletStore::register` can return: `WeakPassphrase` to 2,
/// `Hsm` to 5, a duplicate-DID `AlreadyRevoked` to 6,
/// `VaultSlotNotFound` / `VaultDecryptionFailed` to `WalletLocked`
/// at 92, and everything else to `Internal` at 64. The previous
/// lists here and on the clap variant disagreed with each other and
/// with the code: one offered 4 and 11 (unreachable from this path)
/// and the other 43 (no lifecycle refusal is reachable here), and
/// both omitted 92.
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
    // floor check rejects with `WeakPassphrase` at slot 94, exit 2.
    // A missing or unreadable path is operator input, not a fault:
    // it fails here as `FileInputRejected` at exit 2. It previously
    // rode `Internal` at exit 64, whose hint is "re-run with
    // RUST_LOG=debug and report the diagnostic" - a wrong path is a
    // typo, and in CI that exit code is the page-someone one.
    // Only a file that EXISTS but is empty reaches the floor check
    // below, which is `WeakPassphrase`.
    // Wrap the passphrase in `Zeroizing<String>` so the bytes are
    // scrubbed when the variable drops at the end of the handler
    // (mission AC-24: passphrase must NOT survive into the heap
    // after the seal). The substrate's `register` takes `&str` so
    // the wrapper derefs cleanly.
    let passphrase =
        zeroize::Zeroizing::new(std::fs::read_to_string(passphrase_file).map_err(|e| {
            OctoCliError::FileInputRejected {
                detail: format!(
                    "passphrase file could not be read: {e}. Pass a path that exists and is \
                     readable by this user, or drop --passphrase-file and answer the interactive \
                     prompt"
                ),
            }
        })?);
    let register_facts = if !cli.mode.dry_run {
        // Substrate-side construction. In dev mode (mirrors rotate
        // successor pattern at L334) the seed is the hardcoded stub;
        // otherwise CSPRNG via `IdentityKey::generate` or a
        // deterministic replay from `--seed-file` when present.
        let key = if let Some(seed_path) = seed_file {
            // A seed FILE is a raw private key read off disk, and it
            // is the weaker posture of the two paths - the CSPRNG
            // branch below mints a fresh key in memory and has
            // nothing to leak. The gate used to be on the CSPRNG
            // branch instead, which left production with exactly one
            // route and made it the unsafe one: register with no flags
            // was refused outside dev mode, while supplying a seed
            // file imported a raw key in any mode. The gate belongs
            // on the import, not the mint.
            #[cfg(not(test))]
            {
                if !is_dev_mode(cli) {
                    return Err(OctoCliError::DevModeRequired {
                        detail: "importing a raw seed file is refused outside dev mode; omit \
                                 --seed-file to have the wallet mint a fresh CSPRNG identity, \
                                 or re-run with --mode dev for deterministic replay"
                            .to_string(),
                    });
                }
            }
            // AC-25 obligation 2: refuse a seed file that is
            // group- or world-readable. A permissive mode is
            // treated as a compromise already, not as a warning
            // after the fact — the file is written 0600 by
            // `octo-wallet init` and a 0640 seed has the same
            // posture §Adversary Analysis A9 takes for the store
            // root, applied to the file that holds the identity
            // itself.
            let seed_meta =
                std::fs::metadata(seed_path).map_err(|e| OctoCliError::FileInputRejected {
                    detail: format!("seed file could not be read: {e}. Pass a path that exists"),
                })?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mode = seed_meta.permissions().mode();
                if mode & 0o077 != 0 {
                    // `FileInputRejected`, not `Internal`: a permissive
                    // mode is a deliberate security gate with a
                    // one-command remedy, and `Internal`'s exit-64
                    // hint tells the operator to file a diagnostic
                    // report. In CI that exit code is what decides
                    // retry versus page.
                    return Err(OctoCliError::FileInputRejected {
                        detail: format!(
                            "seed file mode {:04o} is group- or world-readable; expected 0600 (or \
                             stricter). Treat the seed as compromised, write a fresh one with \
                             `octo-wallet init --node-type wholesale --seed-out <path>` (both \
                             flags are required), and chmod 600 the result",
                            mode & 0o7777
                        ),
                    });
                }
            }
            let bytes = std::fs::read(seed_path).map_err(|e| OctoCliError::FileInputRejected {
                detail: format!("seed file could not be read: {e}. Pass a path that exists"),
            })?;
            let seed_arr: [u8; 32] = if bytes.len() == 32 {
                // A 32-byte file that is entirely hex characters is
                // almost certainly a pasted 32-char hex seed that lost
                // its leading zero. Reading it as 32 raw bytes
                // silently mints a DIFFERENT identity than the
                // operator intended and reports success, which is the
                // worst available failure mode for a seed. Refuse
                // with the two spellings rather than guess.
                if bytes.iter().all(|b| b.is_ascii_hexdigit()) {
                    return Err(OctoCliError::FileInputRejected {
                        detail:
                            "the seed file is 32 bytes of hex characters, so reading it as 32 raw \
                             bytes would mint a different identity than the hex you supplied \
                             and report success. Write the 64-char hex form, or write the 32 raw \
                             bytes exactly as the key file holds them"
                                .to_string(),
                    });
                }
                let mut arr = [0u8; 32];
                arr.copy_from_slice(&bytes);
                arr
            } else if bytes.len() == 64 {
                let mut hex_str = String::new();
                for b in &bytes {
                    hex_str.push(*b as char);
                }
                let decoded =
                    hex::decode(hex_str.trim()).map_err(|e| OctoCliError::FileInputRejected {
                        detail: format!(
                            "the 64-character seed file is not valid hex: {e}. Write 64 \
                             lowercase hex characters, or write the 32 raw bytes the key file \
                             holds"
                        ),
                    })?;
                if decoded.len() != 32 {
                    return Err(OctoCliError::FileInputRejected {
                        detail:
                            "the 64-character seed file must decode to exactly 32 bytes. Write \
                             the seed as 64 hex characters or as the 32 raw bytes"
                                .to_string(),
                    });
                }
                let mut arr = [0u8; 32];
                arr.copy_from_slice(&decoded);
                arr
            } else {
                return Err(OctoCliError::FileInputRejected {
                    detail: format!(
                        "seed file is {} bytes; it must be exactly 32 bytes raw or 64 \
                         characters of hex. The CLI states the rule and then, under the previous \
                         Internal classification, told the operator to report a bug for breaking it",
                        bytes.len()
                    ),
                });
            };
            octo_wallet::IdentityKey::from_seed(seed_arr)
        } else {
            // No dev-mode gate. Minting a fresh CSPRNG identity is
            // the SAFE path - the key never touches disk before the
            // vault seals it - and gating it left a production
            // operator with no supported way to create an identity.
            // The confirmation gate above is what this path needs.
            octo_wallet::IdentityKey::generate().map_err(|e| {
                OctoCliError::Internal(sanitize_substrate_error(&format!(
                    "identity key generate: {e}"
                )))
            })?
        };
        let now = chrono::Utc::now().timestamp().max(0);
        let mut store = octo_wallet::WalletStore::open().map_err(map_wallet_open_error)?;
        let key_pubkey = key.public_key_bytes();
        // The substrate mints the DID and returns it. The previous
        // revision discarded the return with `?` and emitted
        // `did: String::new()`, so a successful register reported an
        // empty DID for an identity that was on disk.
        let did = store
            .register(key, &passphrase, activate, now)
            .map_err(OctoCliError::from)?;
        // `pubkey_hex` is recoverable from the record rather than from
        // `key`, which `register` consumed by value.
        let record = store.identity_record(&did).map_err(|e| {
            OctoCliError::Internal(sanitize_substrate_error(&format!(
                "post-register record read: {e}"
            )))
        })?;
        let lifecycle_now = record.lifecycle;
        let active_did_now: Option<String> = store.active_did().map(|d| d.0.clone());
        // `active_now` is the substrate's post-condition, not the flag
        // that was passed. `register` promotes the new record only when
        // the wallet had no active identity, so echoing `activate` told
        // the operator the pointer had moved when on a populated wallet
        // it had not. Read the pointer back instead.
        let active_now = active_did_now.as_deref() == Some(did.0.as_str());
        (
            did.0,
            hex::encode(key_pubkey),
            format!("{lifecycle_now:?}"),
            active_now,
            record.registered_at_unix,
        )
    } else {
        // No substrate call ran, so there is no DID, no record and no
        // pointer to read. A preview says so with empty values rather
        // than fabricating them; the envelope is emitted through
        // `OutputEnvelope::redacted`, which marks it
        // non-authoritative.
        (String::new(), String::new(), String::new(), false, i64::MIN)
    };
    let (did, pubkey_hex, mut lifecycle_state, active_now, registered_at_unix) = register_facts;
    // Same convention the `select` handler states for itself: a value
    // a dry run did not read is said to be unread, never
    // fabricated. Two forms are corrected here.
    //
    // `lifecycle_state` was the empty string, which is not a member
    // of the documented `Designated` / `Active` / `Rotating` /
    // `Revoked` set - the exact condition `select` cites when it
    // emits `<not-read: dry-run>`.
    //
    // `registered_at` was `from_timestamp(0, 0)`, i.e. 1970-01-01,
    // which is a VALID RFC 3339 timestamp and therefore
    // indistinguishable from an identity genuinely registered at the
    // epoch. A consumer reading `registered_at` from a preview got a
    // real-looking answer to a question the preview never asked. The
    // sentinel is `i64::MIN`, which is not a plausible Unix second
    // and cannot round-trip through `from_timestamp` at all, and the
    // field is now `Option` so the envelope carries `null` - the
    // form `select` already uses for `previous_active_did`.
    if cli.mode.dry_run {
        lifecycle_state = "<not-read: dry-run>".to_string();
    }
    let output = IdentityRegisterOutput {
        did,
        pubkey_hex,
        label: label.to_string(),
        lifecycle_state,
        // The substrate stamped `registered_at_unix` on the record
        // it just wrote. A second `Utc::now()` here re-reads the
        // clock and drifts from the persisted value, so the envelope
        // and `store.json` disagree on when the identity was
        // registered. Routed through the same helper every other
        // timestamp in this module uses, so the conversion has one
        // spelling and an out-of-range value renders as `null` here
        // exactly as it does on the rotation paths.
        registered_at: unix_to_rfc3339(registered_at_unix),
        active_now,
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
/// `AlreadyRevoked` (exit 6) when the target is in the `Revoked`
/// lifecycle state (the substrate refuses per RFC-0011-x
/// §Lifecycle Requirements, surfacing `NotActive {
/// current_state: Revoked }`, which the `From<WalletError>`
/// translation table routes to `AlreadyRevoked`).
///
/// Exit codes: 0 / 2 / 4 / 6 / 64.
pub fn select(did: &str, cli: &Octo) -> Result<(), OctoCliError> {
    require_confirm(cli, "identity select")?;
    // Pastejacking defense: echo the canonical payload BEFORE any
    // substrate mutation.
    eprintln!("would select: did={did}");
    let parsed = octo_wallet::Did(did.to_string());
    // Both envelope fields are resolved from the substrate, never
    // hard-coded. The previous revision computed `previous` and
    // `record` inside the `!dry_run` arm, discarded both with `let
    // _`, and then emitted `previous_active_did: None` and
    // `lifecycle_state: "Active"` unconditionally — so the envelope
    // claimed there had been no active identity (wrong whenever a
    // second select ran) and that the record was Active (wrong for
    // a Designated or Revoked record). `select` only moves the
    // active pointer; it does not transition lifecycle, so the
    // label has to come from the record itself.
    let mut previous_active_did = None;
    let mut lifecycle_state = String::new();
    if !cli.mode.dry_run {
        let mut store = octo_wallet::WalletStore::open().map_err(map_wallet_open_error)?;
        // Capture the previous active pointer for the output envelope
        // BEFORE mutating the store.
        previous_active_did = store.active_did().map(|d| d.0.clone());
        store.select(&parsed).map_err(OctoCliError::from)?;
        // Look up the just-selected record for the lifecycle label.
        let record = store.identity_record(&parsed).map_err(|e| match e {
            octo_wallet::WalletError::IdentityNotFound(_) => {
                OctoCliError::IdentityNotFound(parsed.0.clone())
            }
            other => OctoCliError::Internal(sanitize_substrate_error(&other.to_string())),
        })?;
        lifecycle_state = format!("{:?}", record.lifecycle);
    }
    // A dry-run performed no read, so there is no label to report. The
    // empty string is not a member of the documented `Designated` /
    // `Active` / `Rotating` / `Revoked` set, so say so explicitly
    // rather than emitting a value a consumer would have to special-
    // case. `previous_active_did` is left null for the same reason.
    if cli.mode.dry_run {
        lifecycle_state = "<not-read: dry-run>".to_string();
    }
    let output = IdentitySelectOutput {
        did: parsed.0,
        previous_active_did,
        lifecycle_state,
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
    // `list` enumerates every DID, public key, lifecycle state and
    // registration timestamp - a strict SUPERSET of what `show`
    // reports, and `show` is gated. Without this line an auditor
    // session refused `octo whoami` and `octo identity show` could
    // read the whole inventory through `list`, which makes the gate
    // on `show` vacuous.
    block_auditor(cli, "identity list")?;
    // `--dry-run` is a GLOBAL mode flag, so it reaches this handler
    // too. It is a deliberate no-op here and the envelope is NOT
    // redacted: `list` mutates nothing, so there is no pending
    // state change to withhold and the payload below is the real
    // inventory either way. The four sibling handlers all switch
    // to `OutputEnvelope::redacted` under `--dry-run` because theirs
    // would otherwise have to PERFORM the mutation to describe it.
    // Redacting a read-only command would report `total: 0` for a
    // wallet the operator can plainly see is full. Stating the
    // asymmetry here is what makes `redacted: false` a fact rather
    // than an inconsistency a consumer has to discover.
    let store = octo_wallet::WalletStore::open().map_err(map_wallet_open_error)?;
    let active_did = store.active_did().map(|d| d.0.clone());
    let records = store.list_records();
    let rows: Vec<IdentityListRow> = records
        .iter()
        .map(|r| IdentityListRow {
            did: r.did.0.clone(),
            pubkey_hex: hex::encode(r.pubkey_bytes),
            lifecycle_state: format!("{:?}", r.lifecycle),
            registered_at: unix_to_rfc3339(r.registered_at_unix),
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

/// `octo identity rotate-complete --passphrase-stdin` —
/// finalize an in-flight rotation.
///
/// Phase 5 substrate-faithful wrapper over
/// `WalletStore::complete_rotation` reached through the
/// `UnlockedWallet` handle (mission §AC-7: signing operations
/// must migrate from `WalletStore::active_identity` /
/// `complete_rotation(&mut key, ...)` to
/// `WalletStore::unlock(...)` so the key material is held only
/// by the unlocked handle and zeroized on drop). The 24h grace
/// period must have elapsed since `begin_rotation`; substrate
/// returns `GracePeriodNotElapsed` otherwise, which the CLI
/// surfaces at slot 93 / exit 43. The passphrase is acquired via
/// `acquire_passphrase` (mission §AC-9 + §AC-10):
/// `--passphrase-stdin` reads one line from stdin with
/// `--allow-stdin-secret`; absent the flag, an interactive TTY
/// prompt is issued; absent a TTY and the flag, the helper
/// returns `WalletLocked` (exit 92) per §AC-10.
///
/// Exit codes: 0 / 2 / 4 / 6 / 15 / 43 / 64 / 92.
pub fn rotate_complete(passphrase_stdin: bool, cli: &Octo) -> Result<(), OctoCliError> {
    require_confirm(cli, "identity rotate-complete")?;
    // Pastejacking defense.
    eprintln!("would rotate-complete: in_flight_rotation=present");
    let rotate_facts = if !cli.mode.dry_run {
        // §AC-7: open the store, then unlock with the
        // passphrase. The seed buffer is zeroized by the
        // substrate after the key is rehydrated into
        // `UnlockedWallet`, so the seed never survives past
        // the handle's lifetime. The handle holds a unique
        // `IdentityKey` (not a clone) per mission §AC-39.
        let passphrase = acquire_passphrase(cli, "identity rotate-complete", passphrase_stdin)?;
        let mut store = octo_wallet::WalletStore::open().map_err(map_wallet_open_error)?;
        let mut seed_buf = zeroize::Zeroizing::new(Vec::with_capacity(32));
        let mut unlocked = store
            .unlock(passphrase.as_str(), seed_buf.as_mut())
            .map_err(OctoCliError::from)?;
        let now = chrono::Utc::now().timestamp().max(0) as u64;
        // The predecessor DID is live on the handle before
        // `complete_rotation` retires it. The previous revision read
        // neither DID and emitted `String::new()` for both, so a
        // completed rotation - after which the successor is the active
        // identity - told the operator nothing about which DID is now
        // active.
        let old_did = unlocked.did().0.clone();
        unlocked
            .complete_rotation(now)
            .map_err(OctoCliError::from)?;
        // Read the store's active pointer back AFTER the call. It is
        // the substrate that decides which DID is now active, and
        // `complete_rotation` sets it to the promoted successor. The
        // previous revision read it the same way but defaulted an
        // absent pointer to `""`, which is the empty-DID shape R5 was
        // chartered to eliminate - a blank field in a success envelope
        // is worse than a refusal, because it looks like data.
        let new_did = store.active_did().map(|d| d.0.clone()).ok_or_else(|| {
            OctoCliError::Internal(
                "rotation completed but the store reports no active identity".to_owned(),
            )
        })?;
        if new_did == old_did {
            return Err(OctoCliError::Internal(
                "rotation completed but the active identity did not change".to_owned(),
            ));
        }
        (new_did, old_did, Some(now as i64))
    } else {
        (String::new(), String::new(), None)
    };
    let (new_did, old_did, completed_at_unix) = rotate_facts;
    let output = IdentityRotateCompleteOutput {
        new_did,
        old_did,
        // `now` is the value handed to `complete_rotation`, so the
        // envelope and the persisted record agree. Re-reading
        // `Utc::now()` here drifted by however long the unlock took.
        // `None` under `--dry-run`, because the substrate was never
        // called. The previous revision passed `0` here and fell back
        // to `Utc::now` on a failed conversion, so a dry-run reported
        // a real-looking completion time for an event that did not
        // happen - and a genuine conversion failure reported the
        // current time rather than admitting it could not render the
        // value.
        completed_at: completed_at_unix.and_then(unix_to_rfc3339),
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

/// `octo identity rotate-abort --reason [<STR>] --passphrase-stdin`
/// — abort an in-flight rotation.
///
/// Phase 5 substrate-faithful wrapper over
/// `WalletStore::abort_rotation` reached through the
/// `UnlockedWallet` handle (mission §AC-7: same migration as
/// `rotate_complete`). Removes the successor record (mission
/// §AC-38) and restores the predecessor to `Active`. The
/// substrate surfaces the success path silently; CLI echoes the
/// restored DID + abort timestamp. Passphrase acquisition via
/// `acquire_passphrase` per §AC-9 + §AC-10.
///
/// Exit codes: 0 / 2 / 4 / 6 / 15 / 43 / 64 / 92.
pub fn rotate_abort(
    reason: Option<&str>,
    passphrase_stdin: bool,
    cli: &Octo,
) -> Result<(), OctoCliError> {
    require_confirm(cli, "identity rotate-abort")?;
    // Pastejacking defense.
    eprintln!(
        "would rotate-abort: in_flight_rotation=present, reason={}",
        reason
            .map(|r| redact_string(r).into_owned())
            .unwrap_or_else(|| "<none>".to_string())
    );
    let abort_facts = if !cli.mode.dry_run {
        let passphrase = acquire_passphrase(cli, "identity rotate-abort", passphrase_stdin)?;
        let mut store = octo_wallet::WalletStore::open().map_err(map_wallet_open_error)?;
        let mut seed_buf = zeroize::Zeroizing::new(Vec::with_capacity(32));
        let mut unlocked = store
            .unlock(passphrase.as_str(), seed_buf.as_mut())
            .map_err(OctoCliError::from)?;
        // The DID is read from the store's pointer AFTER the abort
        // has succeeded. The previous revision read it from the handle
        // BEFORE the call, while the field doc claims an achieved
        // state ("restored to `Active` after abort") that a
        // pre-mutation read cannot support.
        //
        // The confirmation against the handle is DEFENSIVE, and this
        // comment used to claim it as the fix. It is not. R9's own
        // falsifiability pass established that the check cannot fire:
        // `unlock` derives the handle's DID from `index.active_did`,
        // and nothing on the abort path writes that field - `register`,
        // `revoke`, `select` and `complete_rotation` are its only
        // writers, and none is reachable from here. So the store
        // pointer and the handle DID are the same value by
        // construction, and the `.filter` is a tautology.
        //
        // It is kept because a substrate change that made the abort
        // move the pointer would silently put a wrong DID in a
        // success envelope, and this turns that into a refusal. It is
        // NOT evidence of anything today, and it must not be cited as
        // a defect this handler fixed. If it ever does fire, the
        // substrate changed and the substrate's own `# Errors` docs
        // are the thing to read first.
        let aborted_at_unix = chrono::Utc::now().timestamp().max(0) as u64;
        unlocked.abort_rotation().map_err(OctoCliError::from)?;
        let handle_did = unlocked.did().0.clone();
        let restored_did = store
            .active_did()
            .map(|d| d.0.clone())
            .filter(|d| *d == handle_did)
            .ok_or_else(|| {
                OctoCliError::Internal(
                    "rotation aborted but the store's active identity is not the restored \
                     predecessor"
                        .to_owned(),
                )
            })?;
        (restored_did, Some(aborted_at_unix as i64))
    } else {
        (String::new(), None)
    };
    let (restored_did, aborted_at_unix) = abort_facts;
    let output = IdentityRotateAbortOutput {
        restored_did,
        aborted_at: aborted_at_unix.and_then(unix_to_rfc3339),
        // The reason is operator-supplied free text on a mutating
        // command. The sibling `revoke` handler runs it through
        // `redact_string` on both the stderr echo and the envelope;
        // this path did neither, so a token pasted into `--reason`
        // landed verbatim in the terminal and in any log capturing
        // the envelope. Two doc comments claimed it was sanitized.
        reason: reason.map(|r| redact_string(r).into_owned()),
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

/// Acquire the wallet passphrase for a signing operation.
///
/// Honors mission 0011-x-wallet-store-cli §AC-9 + §AC-10:
/// - When `--passphrase-stdin` is passed, the helper enforces the
///   `ensure_stdin_secret_allowed` gate (which requires
///   `--allow-stdin-secret`; refuse on default builds so a stray
///   pipe does not silently consume a secret), reads one line from
///   stdin, trims the trailing newline, and wraps the result in
///   `Zeroizing<String>` so the bytes are scrubbed on drop.
/// - When `--passphrase-stdin` is NOT passed, the helper falls back
///   to `rpassword::prompt_password` for interactive entry. The
///   prompt only works on a TTY; if stdin is not a TTY AND the
///   operator did not pass `--passphrase-stdin`, the helper returns
///   `WalletLocked` (exit 92) per §AC-10 — a wallet signing
///   command without a passphrase and without a TTY is the
///   §AC-10 fail-closed posture.
///
/// Both paths land in `Zeroizing<String>` so the bytes are wiped
/// when the binding drops at the end of the handler (mission
/// §AC-24 — the same obligation register satisfies at the
/// `--passphrase-file` site).
pub(crate) fn acquire_passphrase(
    cli: &Octo,
    command: &str,
    passphrase_stdin: bool,
) -> Result<zeroize::Zeroizing<String>, OctoCliError> {
    if passphrase_stdin {
        // Gate per [[feedback_initiation_user_only]] + the
        // `ensure_stdin_secret_allowed` helper at `error.rs`.
        // Operators MUST pass `--allow-stdin-secret` alongside
        // `--passphrase-stdin` so the default-build refusal
        // (exit 15) does not silently consume a piped secret
        // in a non-interactive script.
        crate::error::ensure_stdin_secret_allowed(cli.mode.allow_stdin_secret)?;
        let mut line = zeroize::Zeroizing::new(String::new());
        std::io::stdin().read_line(&mut line).map_err(|e| {
            OctoCliError::Internal(sanitize_substrate_error(&format!(
                "passphrase stdin read: {e}"
            )))
        })?;
        // Strip the line terminator. The wrapper zeros the bytes on
        // drop so the secret is not left in heap memory after the
        // handler returns.
        //
        // A previous form computed the trimmed length in two steps,
        // testing for a carriage return on the *untruncated* string
        // after the `\n` had already been accounted for. For CRLF
        // input the still-untruncated value ends in `\n`, so the `\r`
        // test was false and the carriage return survived: `"secret\r\n"`
        // yielded the passphrase `"secret\r"`. `store.unlock` then
        // failed and the operator saw `WalletLocked` — indistinguishable
        // from a genuinely wrong passphrase, with no hint that the input
        // had been mangled. `trim_end_matches` strips any run of `\r`
        // and `\n` in one pass and cannot read a stale suffix.
        let trimmed_len = line.trim_end_matches(['\r', '\n']).len();
        line.truncate(trimmed_len);
        return Ok(line);
    }
    // Interactive prompt. **Pre-flight first** (AC-10): when stdin is
    // not a terminal there is nobody to answer the prompt, and an
    // unattended `cron` job or CI step would block forever on an
    // invisible read. Refusing before the prompt is honest; timing out
    // mid-prompt would leave a half-entered passphrase on a terminal
    // the operator cannot see.
    //
    // This check used to be delegated implicitly to `rpassword`, which
    // reaches for the controlling TTY and so only fails closed when
    // the *process* has no TTY — not when *stdin* has none. Under
    // `script`, `ssh -t`, or a CI runner that allocates a pty, stdin
    // can be a pipe while a TTY still exists, and the prompt would
    // block. The explicit `is_terminal()` test is on stdin and is the
    // condition the operator actually controls.
    if !std::io::IsTerminal::is_terminal(&std::io::stdin()) {
        return Err(OctoCliError::WalletLocked);
    }
    match rpassword::prompt_password(format!("{command} passphrase: ")) {
        Ok(p) => Ok(zeroize::Zeroizing::new(p)),
        Err(_) => Err(OctoCliError::WalletLocked),
    }
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
            // Auditor short-circuited above, before the
            // `cli.mode.dry_run` bypass, so this arm is unreachable
            // TODAY. It is not unreachable BY CONSTRUCTION: the early
            // return and this arm are four lines apart, and a future
            // edit that reorders them - say, moving the dry-run bypass
            // above the Auditor check to fix a different report -
            // converts this into a live `unreachable!()` and therefore
            // a panic at exit 101 on a read-only enforcement path,
            // which is the worst place to panic. Returning the
            // refusal costs nothing and fails closed if the ordering
            // ever changes.
            return Err(OctoCliError::AuditorDenied {
                command: command.to_string(),
            });
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
        IdentityAction::Rotate {
            passphrase_stdin, ..
        } => {
            require_confirm(cli, "identity rotate")?;
            rotate(*passphrase_stdin, cli)
        }
        IdentityAction::Revoke {
            reason,
            passphrase_stdin,
            ..
        } => {
            require_confirm(cli, "identity revoke")?;
            revoke(reason, *passphrase_stdin, cli)
        }
        IdentityAction::Register {
            label,
            passphrase_file,
            activate,
            seed_file,
        } => register(label, passphrase_file, *activate, seed_file.as_deref(), cli),
        IdentityAction::Select { did } => select(did, cli),
        IdentityAction::List { .. } => list(cli),
        IdentityAction::RotateComplete {
            passphrase_stdin, ..
        } => rotate_complete(*passphrase_stdin, cli),
        IdentityAction::RotateAbort {
            reason,
            passphrase_stdin,
            ..
        } => rotate_abort(reason.as_deref(), *passphrase_stdin, cli),
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

    /// Slice the production source to one function body, failing closed
    /// at BOTH ends.
    ///
    /// A `find(...).expect("end marker for the slice bound must exist — if this fires, the function was renamed and the bound would have silently degraded to the test module")` end-bound silently degrades
    /// to "the rest of the file" when the marker is renamed, and the
    /// rest of the file is `mod tests` — which is exactly the
    /// self-reference `production_src` exists to eliminate. The
    /// needles would then match the vector's own literals and the
    /// vector would go green with the handler gutted. `expect` on both
    /// markers turns that into a loud failure instead.
    fn fn_body<'a>(src: &'a str, start: &str, end: &str) -> &'a str {
        let from = src
            .find(start)
            .unwrap_or_else(|| panic!("start marker not found in production source: {start}"));
        assert!(
            src[from..].contains(end),
            "end marker {end:?} not found after {start:?} — the two markers are out of order or the function was renamed"
        );
        let to = from + src[from..].find(end).expect("checked above");
        &src[from..to]
    }

    /// R9: the comment stripper must be a scanner, not a line
    /// heuristic - and it is the vectors that USES it that inherit
    /// the property, so it is tested directly.
    ///
    /// The previous revision cut each line at its first `//` and left
    /// any line carrying a quote alone. It did not recognise `/* */`.
    /// So a guard commented out with a block comment left its text in
    /// the slice, and every assertion pinning that guard stayed green
    /// with the guard dead. A round's mutation proofs were voided by
    /// a two-line change to a test helper.
    #[test]
    fn tv_x_c_52_the_comment_stripper_understands_block_comments() {
        let src = "pub fn f() {\n    /* let x = needle_in_block; */\n    let y = needle_in_code;\n}\npub fn g(";
        let out = fn_body_code(src, "pub fn f()", "pub fn g(");
        assert!(
            !out.contains("needle_in_block"),
            "block-comment content must not survive: {out}"
        );
        assert!(
            out.contains("needle_in_code"),
            "real code must survive: {out}"
        );

        // Nested block comments. Rust nests them, so a depth-1 toggle
        // closes on the inner `*/` and leaves the tail of the
        // comment live.
        let nested = "pub fn f() {\n    /* outer /* inner */ still_outer */\n    let z = real;\n}\npub fn g(";
        let out = fn_body_code(nested, "pub fn f()", "pub fn g(");
        assert!(
            !out.contains("still_outer"),
            "a nested block comment must close at the OUTER */ : {out}"
        );
        assert!(
            out.contains("real"),
            "code after a nested comment must survive: {out}"
        );

        // A `//` INSIDE a string literal must not truncate the line.
        // The old heuristic handled this by skipping any line with a
        // quote, which meant such lines were never stripped at all.
        let tricky = "pub fn f() {\n    let url = \"http://x\"; // a comment\n    let q = b'x';\n}\npub fn g(";
        let out = fn_body_code(tricky, "pub fn f()", "pub fn g(");
        assert!(
            out.contains("http://x"),
            "a // inside a string literal is not a comment: {out}"
        );
        assert!(
            !out.contains("a comment"),
            "a real trailing comment on the same line must still be stripped: {out}"
        );

        // A lifetime is not a char literal. Reading `'` as opening a
        // char would run to the next `'` on the file and blank
        // everything between.
        let lifetime = "pub fn f<'a>() {\n    let keep: &'a str = \"x\";\n}\npub fn g(";
        let out = fn_body_code(lifetime, "pub fn f<'a>", "pub fn g(");
        assert!(
            out.contains("let keep"),
            "a lifetime must not be read as an unterminated char literal: {out}"
        );

        // `'static` is the case the previous revision got wrong, and
        // the reason this case is written with a TRAILING COMMENT
        // rather than a bare `&'a`.
        //
        // The old test decided lifetime-vs-char by looking at two
        // characters after the quote, which models a lifetime as
        // `'` ident `'`. `&'static` gives `s` then `t` - both ident
        // chars - so it was classified as a char literal, and the
        // char-literal walk scanned forward for a closing `'`. It
        // found none in this snippet, ran to the end of the input,
        // and in doing so never saw the `//` on the way. The comment
        // survived into the output.
        //
        // That is the shape that matters and it is not a curiosity: a
        // swallowed region is a region whose COMMENTS are never
        // stripped, so any source-grep vector scanning a handler
        // containing `&'static` is satisfied by prose, and any
        // assertion that a needle is ABSENT is decided by a comment.
        // Asserting the comment is GONE is what makes the lexer
        // responsible for it.
        //
        // Scope of the claim, stated narrowly because the wider one
        // was not confirmed. A round reported that adding a single
        // `&'static` above the `rotate_complete` unchanged-identity
        // guard, then commenting that guard out, let the mutation
        // through `tv_x_c_39`. Re-measured here, that shape did NOT
        // survive: with the old lexer in place the swallowed region
        // is blanked rather than passed through, so the guard text
        // is absent and `tv_x_c_39` fails on the missing side. The
        // defect fixed here is the mis-classification itself, which
        // is a fact of the code, not the reported exploit. What
        // these three cases establish is the narrower and verified
        // one: the old lexer fails them, the new one passes, so the
        // rule change is load-bearing rather than incidental.
        let stat = "pub fn f() {\n    let d: &'static str = \"x\"; // needle_in_static_tail\n    \
                    let keep = real;\n}\npub fn g(";
        let out = fn_body_code(stat, "pub fn f()", "pub fn g(");
        assert!(
            !out.contains("needle_in_static_tail"),
            "a `//` after a &'static annotation must still be stripped. It survived, which means \
             the lifetime was read as a char literal and the walk swallowed the line: {out}"
        );
        assert!(
            out.contains("let keep"),
            "code after a &'static annotation must survive: {out}"
        );

        // The same shape with a BLOCK comment, because the two
        // swallow modes differ and only one was previously covered.
        let stat_block = "pub fn f() {\n    let d: &'static str = \"x\";\n    /* \
                          needle_in_static_block */\n    let keep = real;\n}\npub fn g(";
        let out = fn_body_code(stat_block, "pub fn f()", "pub fn g(");
        assert!(
            !out.contains("needle_in_static_block"),
            "a block comment after a &'static annotation must still be stripped: {out}"
        );

        // A raw string containing comment syntax must be preserved
        // whole, or the scanner blanks code that follows it.
        let raw = "pub fn f() {\n    let r = r#\"/* not a comment */\"#;\n    let tail = kept;\n}\npub fn g(";
        let out = fn_body_code(raw, "pub fn f()", "pub fn g(");
        assert!(
            out.contains("kept"),
            "code after a raw string must survive: {out}"
        );
    }

    /// `fn_body` with every comment removed, code preserved byte for
    /// byte otherwise.
    ///
    /// A negative source assertion (`!body.contains("did: String::new()")`)
    /// is only meaningful against CODE. The register handler carries a
    /// comment quoting the exact form it no longer emits, so a raw
    /// slice made the vector report a false defect on the fix. A
    /// positive assertion has the opposite failure — a needle quoted
    /// in prose would make it pass vacuously — so both directions want
    /// comments gone.
    ///
    /// **This was a real lexer gap, and it voided a round's evidence.**
    /// The previous revision stripped `//` line comments by finding
    /// the first `//` on each line and cutting there, skipping lines
    /// that carried a quote. It did not know what `/* ... */` was. So
    /// commenting out a guard with a block comment left its text
    /// present and every assertion pinning that guard green. The R9
    /// commit recorded that it had mutation-proven those guards by
    /// deleting them; a reviewer wrapping them in `/* */` instead
    /// passed the whole suite. The guards were real, and the proof
    /// was not.
    ///
    /// So this is a scanner, not a line heuristic. It tracks string
    /// literals (with backslash escapes), char literals, raw strings
    /// (with any hash count), line comments, and block comments -
    /// including nested block comments, which Rust allows and which
    /// a two-state toggle gets wrong. Comment CONTENT is blanked to
    /// spaces of the same length rather than deleted, so byte offsets
    /// into the result still index the same character of the input and
    /// a failure message points at the right place.
    fn fn_body_code(src: &str, start: &str, end: &str) -> String {
        let body = fn_body(src, start, end);
        let bytes = body.as_bytes();
        let mut out: Vec<u8> = bytes.to_vec();
        let mut i = 0usize;
        // Depth of nested block comments.
        let mut block_depth = 0usize;
        while i < bytes.len() {
            match block_depth {
                // Inside a block comment: blank everything, tracking
                // nesting so `/* /* */ */` closes at the right place.
                d if d > 0 => {
                    if bytes[i] == b'/' && bytes.get(i + 1) == Some(&b'*') {
                        block_depth += 1;
                        out[i] = b' ';
                        out[i + 1] = b' ';
                        i += 2;
                    } else if bytes[i] == b'*' && bytes.get(i + 1) == Some(&b'/') {
                        block_depth -= 1;
                        out[i] = b' ';
                        out[i + 1] = b' ';
                        i += 2;
                    } else {
                        // Preserve newlines so line numbers in a
                        // failure still line up.
                        if bytes[i] != b'\n' {
                            out[i] = b' ';
                        }
                        i += 1;
                    }
                    let _ = d;
                }
                _ => {
                    // Raw string: r"..." / r#"..."# / r##"..."##.
                    if bytes[i] == b'r' {
                        let mut j = i + 1;
                        let mut hashes = 0usize;
                        while bytes.get(j) == Some(&b'#') {
                            hashes += 1;
                            j += 1;
                        }
                        if bytes.get(j) == Some(&b'"') {
                            i = j + 1;
                            'raw: loop {
                                if i >= bytes.len() {
                                    break 'raw;
                                }
                                if bytes[i] == b'"' {
                                    let closing =
                                        (0..hashes).all(|k| bytes.get(i + 1 + k) == Some(&b'#'));
                                    if closing {
                                        i += 1 + hashes;
                                        break 'raw;
                                    }
                                }
                                i += 1;
                            }
                            continue;
                        }
                    }
                    if bytes[i] == b'/' && bytes.get(i + 1) == Some(&b'/') {
                        // Line comment: blank to end of line.
                        while i < bytes.len() && bytes[i] != b'\n' {
                            out[i] = b' ';
                            i += 1;
                        }
                        continue;
                    }
                    if bytes[i] == b'/' && bytes.get(i + 1) == Some(&b'*') {
                        block_depth = 1;
                        out[i] = b' ';
                        out[i + 1] = b' ';
                        i += 2;
                        continue;
                    }
                    if bytes[i] == b'"' {
                        // Ordinary string: skip to the unescaped close.
                        i += 1;
                        while i < bytes.len() {
                            if bytes[i] == b'\\' {
                                i += 2;
                                continue;
                            }
                            if bytes[i] == b'"' {
                                i += 1;
                                break;
                            }
                            i += 1;
                        }
                        continue;
                    }
                    if bytes[i] == b'\'' {
                        // Char literal OR a lifetime. Getting this
                        // wrong is not cosmetic: a lifetime that is
                        // read as an unterminated char literal makes
                        // the walk scan forward for a closing `'`,
                        // swallowing everything in between - so a
                        // `//` comment inside a swallowed region is
                        // never stripped, and a source-grep vector
                        // downstream is satisfied by prose.
                        //
                        // The previous revision decided this by
                        // looking at exactly two characters: `'` + one
                        // ident char + a NON-ident char. That models a
                        // lifetime as `'` ident `'`, which is the
                        // shape of `'a` FOLLOWED BY a type - and it
                        // gets `&'static` wrong, because there the
                        // third character is `t`, an ident char, so
                        // the helper classified the most common
                        // lifetime in Rust as a char literal.
                        //
                        // The consequence is that a swallowed region
                        // is a region whose comments are never
                        // stripped, so the stripped output a
                        // source-grep vector reads can be prose
                        // rather than code. A round reported a
                        // mutation that exploited exactly that and
                        // survived the vector pinning the guard; on
                        // re-measurement the mutation did not
                        // survive, because the swallowed region is
                        // blanked rather than passed through, so the
                        // guard text goes missing and the vector
                        // fails on the other side. The
                        // mis-classification is nonetheless real and
                        // is fixed here on its own terms.
                        //
                        // The length decides it, not the neighbours.
                        // A char literal holds exactly one character
                        // (or one backslash escape): `'a'`, `'\n'`,
                        // `'\''`. So an identifier of two or more
                        // characters after the quote cannot be a char
                        // literal and must be a lifetime - which is
                        // what makes `'static` correct. An identifier
                        // of exactly one is a lifetime only when the
                        // next byte can neither close a char literal
                        // nor start an escape.
                        let mut j = i + 1;
                        while j < bytes.len()
                            && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_')
                        {
                            j += 1;
                        }
                        let ident_len = j - (i + 1);
                        let next_is_quote_or_escape =
                            matches!(bytes.get(j), Some(b'\'') | Some(b'\\'));
                        let is_lifetime =
                            ident_len >= 2 || (ident_len == 1 && !next_is_quote_or_escape);
                        if is_lifetime {
                            // `i += 1` steps onto the first ident
                            // char; the outer loop then advances the
                            // rest one byte at a time, which is
                            // harmless because a lifetime contains no
                            // character the scanner acts on.
                            i += 1;
                            continue;
                        }
                        i += 1;
                        while i < bytes.len() {
                            if bytes[i] == b'\\' {
                                i += 2;
                                continue;
                            }
                            if bytes[i] == b'\'' {
                                i += 1;
                                break;
                            }
                            i += 1;
                        }
                        continue;
                    }
                    i += 1;
                }
            }
        }
        // SAFETY of the unchecked split: the scanner only ever turns
        // non-ASCII bytes into more non-ASCII bytes or blanks, and
        // never splits a multi-byte sequence, so the result is still
        // valid UTF-8. Re-derive it as a String to prove it rather
        // than asserting it.
        String::from_utf8(out).expect("comment stripping must not corrupt UTF-8")
    }

    /// The production source of this file, with every test-module line
    /// removed.
    ///
    /// A source-query vector that runs `include_str!("identity.rs")` over
    /// the **whole** file and asserts `src.contains(needle)` is asserting
    /// against itself: the needle is a string literal inside the vector's
    /// own `assert!`, so it is present in the scanned text whether or not
    /// the handler calls the substrate. Deleting the handler call leaves
    /// such a vector green. That defect removed `tv_x_c_12` and
    /// `tv_x_c_16` at the R1.5 fix-sweep; the same shape survived in six
    /// siblings, which this helper exists to close.
    ///
    /// Truncating at the `mod tests` boundary keeps every real handler
    /// body (the last one, `dispatch`, precedes it) and drops every test
    /// literal, so a needle now matches only where the handler wrote it.
    /// A second assertion in `production_src_isolated` keeps the
    /// truncation honest: if the boundary is ever removed, the helper
    /// panics rather than silently reintroducing the self-reference.
    fn production_src() -> &'static str {
        let src = include_str!("identity.rs");
        let end = src
            .find("\n#[cfg(test)]\nmod tests {")
            .expect("test module boundary present in identity.rs");
        &src[..end]
    }

    /// Guards `production_src` itself. Without this, a refactor that
    /// renames or removes the `mod tests` header would make
    /// `production_src` return the whole file again and every
    /// source-query vector would silently revert to asserting against
    /// its own `assert!` literal.
    #[test]
    fn production_src_isolated() {
        let src = production_src();
        assert!(
            !src.contains("mod tests {"),
            "production_src leaked the test module back into scope"
        );
        assert!(
            src.contains("pub fn dispatch("),
            "production_src must retain the handler bodies: {src}"
        );
    }

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
            registered_at: Some(DateTime::<Utc>::from_timestamp(1_700_000_000, 0).unwrap()),
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
            grace_expires_at: Some(DateTime::<Utc>::from_timestamp(1_700_086_400, 0).unwrap()),
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
            revoked_at: Some(DateTime::<Utc>::from_timestamp(1_700_000_000, 0).unwrap()),
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
        // An earlier revision scanned the whole file for `eprintln!`,
        // `would rotate: old_did=`, and `would revoke: did=`. All three
        // strings are literals in this vector's own `assert!` calls, so
        // the assertions were satisfied by the vector itself and
        // deleting every echo left it green.
        //
        // Each handler is now sliced to its own body, and the bare
        // `eprintln!` needle is dropped: every handler in the file
        // calls it, so it distinguishes nothing.
        let src = production_src();

        let rotate = fn_body_code(src, "pub fn rotate(", "pub fn revoke(");
        assert!(
            rotate.contains("would rotate: old_did=<none>"),
            "rotate must echo the placeholder when no identity is active: {rotate}"
        );
        assert!(
            rotate.contains("would rotate: old_did={}"),
            "rotate must echo the resolved DID before mutating: {rotate}"
        );

        let revoke = fn_body_code(src, "pub fn revoke(", "pub fn register(");
        assert!(
            revoke.contains("would revoke: did=<none>"),
            "revoke must echo the placeholder when no identity is active: {revoke}"
        );
        assert!(
            revoke.contains("would revoke: did={}"),
            "revoke must echo the resolved DID before mutating: {revoke}"
        );
    }

    /// tv_x_c_39 - R5 findings 1, 2, 3, 4. Four output envelopes in
    /// this file reported placeholder or empty values on the
    /// COMMITTED path, where the substrate had just written real ones
    /// to disk. Each asserted the value it should never contain, so a
    /// regression to the placeholder form fails.
    ///
    /// `register` is the worst of the four: the substrate mints the
    /// DID and RETURNS it, and the handler discarded the return with
    /// `?`, so `octo identity register` exited 0 having told the
    /// operator its new identity had no DID.
    /// C1 - the dev-mode gate must be on the seed-file IMPORT, not on
    /// the CSPRNG mint.
    ///
    /// The gate was inverted. It sat on the CSPRNG branch, so
    /// `octo identity register` with no flags - the one route a
    /// production operator should have - was refused outside dev mode,
    /// while `--seed-file`, which reads a RAW private key off disk,
    /// was ungated. The surface that survived the gate was the weaker
    /// of the two.
    ///
    /// The first draft of this vector counted gate sites and compared
    /// their offsets, and it PASSED a mutation that moved the gate from
    /// the import branch onto the mint: the moved gate still sits
    /// before `IdentityKey::generate()`, so every offset check was
    /// satisfied. Offsets do not identify a BRANCH. This version
    /// splits the handler at the `} else {` that opens the CSPRNG arm
    /// and asks which side of it the gate is on - the only question
    /// the mutation actually changes.
    #[test]
    fn tv_x_c_44_the_dev_gate_is_on_the_seed_import_not_the_mint() {
        let src = production_src();
        let reg = fn_body_code(src, "pub fn register(", "pub fn select(");

        // Split the handler into the seed-file arm and the CSPRNG
        // arm. The split point is anchored on the MINT CALL and then
        // walks back to the nearest `} else {` before it. The previous
        // revision took the FIRST `} else {` in the handler, which is
        // the seed-length dispatch INSIDE the import arm, so `mint_arm`
        // also held the tail of the import arm and the vector was not
        // describing the arms it names. Anchoring on the mint makes
        // the split correct for any arm ordering.
        let mint_at = reg
            .find("IdentityKey::generate()")
            .expect("the CSPRNG mint arm must exist");
        let branch_at = reg[..mint_at]
            .rfind("} else {")
            .expect("register branches between a seed file and the CSPRNG");
        let (import_arm, mint_arm) = reg.split_at(branch_at);

        assert!(
            import_arm.contains("if let Some(seed_path) = seed_file"),
            "the first arm must be the seed-file import: {import_arm}"
        );
        assert!(
            mint_arm.contains("IdentityKey::generate()"),
            "the second arm must be the CSPRNG mint: {mint_arm}"
        );

        // The gate must precede the READ OF KEY MATERIAL, not merely
        // the first mention of `seed_path`. The previous revision
        // compared against `import_arm.find("seed_path")`, which
        // resolves to the `if let Some(seed_path)` BINDING at the top
        // of the arm - so the assertion held for any gate position
        // whatsoever. A mutation that moved the gate to sit AFTER
        // `std::fs::read` - reading the raw private key off disk
        // before refusing it, which is the whole defect - passed.
        // `std::fs::metadata(seed_path)` legitimately precedes the
        // gate: stat'ing a path discloses only that a file exists, and
        // the gate needs the mode to report it.
        let import_gate = import_arm
            .find("is_dev_mode(cli)")
            .unwrap_or_else(|| panic!("the seed-file import must be gated: {import_arm}"));
        let key_read = import_arm
            .find("std::fs::read(seed_path)")
            .unwrap_or_else(|| panic!("the import arm must read the seed file: {import_arm}"));
        assert!(
            import_gate < key_read,
            "the mode gate must precede the read of key material, not merely the first \
             mention of the path; the gate at {import_gate} and the read at {key_read} \
             means the raw private key is loaded before it is refused: {import_arm}"
        );
        assert!(
            import_arm.contains("DevModeRequired"),
            "the gate must refuse with a named variant carrying the remedy: {import_arm}"
        );

        // The mint is UNGATED. This is the conjunct the inversion
        // actually broke, and the one no offset check can see.
        assert!(
            !mint_arm.contains("is_dev_mode(cli)"),
            "the CSPRNG mint must not be gated - minting a fresh key in memory is the SAFE path \
             and gating it leaves a production operator with no supported way to create an \
             identity, while the raw-key import stays open: {mint_arm}"
        );
    }

    /// L14 - `--activate` must accept the explicit `false` form.
    ///
    /// clap's derive default for a `bool` is `SetTrue`, which accepts a
    /// bare `--activate` and REJECTS `--activate=false` as a usage
    /// error. Designated is a state `select` accepts and `whoami`,
    /// `show` and `list` all report, so the flag that registers a
    /// Designated record was not expressible.
    ///
    /// Both needles are searched inside a SLICE of the clap struct
    /// rather than the whole file. A whole-file `contains` is
    /// self-referential: this vector's own literal contains
    /// `action = clap::ArgAction::Set`, so it matched itself and the
    /// vector was green before the fix was written. The first
    /// draft shipped that way and the mutation below confirms the
    /// slicing is load-bearing.
    #[test]
    fn tv_x_c_45_activate_accepts_the_explicit_false_form() {
        let src = production_src();

        // Bound the clap attribute block for `activate` itself. The
        // delimiters are the field and its own doc block, so a needle
        // elsewhere in the file - including in this test - cannot
        // satisfy the assertion.
        // Start at the ATTRIBUTE, not the field - the `#[arg(...)]`
        // block sits above the field, so slicing from the field
        // excluded the very text the vector asserts on.
        let field_at = src
            .find("\n        activate: bool,")
            .expect("the activate field is present");
        let field_at = src[..field_at]
            .rfind("\n        #[arg(")
            .expect("activate carries a clap attribute");
        let slice_end = src[field_at..]
            .find("seed_file:")
            .map(|i| field_at + i)
            .expect("a sibling field follows activate");
        // Strip line comments before searching. The clap block
        // carries a `//` comment explaining WHY SetTrue is wrong, and
        // the negative control below looks for the token `SetTrue` -
        // so without the strip the vector fails on its own
        // explanatory prose, which is the doc-comment-satisfaction
        // shape in a negative control.
        let activate_block: String = src[field_at..slice_end]
            .lines()
            .map(|line| match line.find("//") {
                Some(at) if !line[..at].contains('"') => line[..at].trim_end(),
                _ => line,
            })
            .collect::<Vec<_>>()
            .join("\n");
        let activate_block = activate_block.as_str();

        assert!(
            activate_block.contains("action = clap::ArgAction::Set,"),
            "the --activate bool long must use ArgAction::Set, or clap's SetTrue rejects \
             `--activate=false` as a usage error and Designated is unreachable: {activate_block}"
        );
        assert!(
            activate_block.contains("default_missing_value = \"true\""),
            "a bare --activate must still mean true after switching to ArgAction::Set: \
             {activate_block}"
        );
        assert!(
            activate_block.contains("num_args = 0..=1"),
            "ArgAction::Set with no num_args would make `--activate` itself require a value: \
             {activate_block}"
        );

        // The negative control, and the one the mutation targets.
        assert!(
            !activate_block.contains("SetTrue"),
            "ArgAction::SetTrue is the spelling that breaks Designated: {activate_block}"
        );
    }

    /// L10 - a 32-byte seed file that is all hex characters is a
    /// 32-char hex seed that lost its leading zero. Read as 32 raw
    /// bytes it silently mints a DIFFERENT identity than the operator
    /// intended and reports success, which is the worst available
    /// failure mode for a seed. The refusal names both spellings.
    #[test]
    fn tv_x_c_46_a_hex_shaped_seed_file_is_refused_rather_than_guessed() {
        let src = production_src();
        let reg = fn_body_code(src, "pub fn register(", "pub fn select(");

        // SCOPE: the 32-byte arm alone. The previous revision asserted
        // against the whole register body, and the body carries a
        // SIBLING refusal - the permission-mode check - which uses the
        // same variant. A mutation that changed only THIS arm's
        // variant to `Internal` therefore left every assertion green
        // on the strength of a different arm.
        let arm_at = reg
            .find("if bytes.len() == 32 {")
            .unwrap_or_else(|| panic!("register must branch on the seed length: {reg}"));
        let arm_end = reg[arm_at..]
            .find("} else if bytes.len() == 64 {")
            .map(|e| arm_at + e)
            .unwrap_or_else(|| {
                panic!("the 32-byte arm must be followed by the 64-char arm: {reg}")
            });
        let arm = &reg[arm_at..arm_end];

        // The CHECK, as a predicate that the refusal is guarded by -
        // not merely the presence of the token. A mutation appending
        // `&& false` to the condition left the previous
        // `contains("is_ascii_hexdigit")` green while neutering the
        // guard completely: every 32-byte seed would be accepted and
        // the wrong identity minted. Pinning the whole condition
        // followed by the guard's opening brace is the only spelling
        // that distinguishes a live check from a disabled one.
        let cond = "if bytes.iter().all(|b| b.is_ascii_hexdigit()) {";
        let cond_at = arm.find(cond).unwrap_or_else(|| {
            panic!(
                "the 32-byte arm must GUARD the refusal on the whole payload being hex - \
                 `{cond}` - not merely mention the predicate: {arm}"
            )
        });
        // The refusal must be what the guard returns, not something
        // that merely appears later in the arm.
        let after = &arm[cond_at..];
        let refusal_at = after.find("return Err(").unwrap_or_else(|| {
            panic!("the hex-shape guard must return a refusal directly: {after}")
        });
        assert!(
            refusal_at < 400,
            "the guard's body must BE the refusal; a return {refusal_at} bytes after the \
             condition is not: {after}"
        );
        assert!(
            after[refusal_at..].contains("FileInputRejected"),
            "the hex-shape refusal must be FileInputRejected (exit 2, remedy in the message), \
             not Internal (exit 64, report a diagnostic): {after}"
        );
        assert!(
            !arm.contains("OctoCliError::Internal"),
            "the 32-byte arm must not classify operator input as Internal: {arm}"
        );
        // The refusal must name the ambiguity so the operator picks a
        // spelling rather than guessing.
        assert!(
            arm.contains("would mint a different identity"),
            "the refusal must name the ambiguity so the operator picks a spelling, not a \
             guess: {arm}"
        );
        // The copy IS reachable - the guard is conditional, and a raw
        // 32-byte seed is the ordinary case. What must not happen is
        // reaching it with the guard absent, which the condition and
        // ordering assertions above already exclude. An earlier
        // revision asserted the copy was unreachable at all, which
        // was false of correct code and would have forced the guard
        // to be removed to make the vector pass.
    }

    /// The `ConfirmationRequired` remediation must name the flag the
    /// operator's MODE actually gates on.
    ///
    /// The hint was the fixed string "re-run with `--confirm`". But
    /// the gate is mode-dependent: Human requires
    /// `--confirm --confirm-acknowledge`, while Ci and Dev require
    /// `--allow-write`. An operator in `--mode ci` who followed the
    /// hint got byte-identical output on retry, because `--confirm`
    /// does not satisfy the Ci gate — a loop with no way to discover
    /// the right flag from the output. The variant's own doc comment
    /// claimed "the operator sees the per-mode help text from
    /// `OctoCliError::render`"; `render` has no mode logic at all.
    ///
    /// Runtime, because the defect is in what the operator sees. The
    /// handler's own gate is exercised, not a source needle.
    #[test]
    fn tv_x_c_47_confirmation_hint_names_the_flag_the_mode_gates_on() {
        for (mode, works, hint_must_name) in [
            // Ci and Dev gate on --allow-write, so that is what the
            // operator has to be told; --confirm cannot satisfy them.
            (OperatorMode::Ci, "--allow-write", "--allow-write"),
            (OperatorMode::Dev, "--allow-write", "--allow-write"),
        ] {
            let dir = tempfile::tempdir().expect("tempdir");
            let pass_file = dir.path().join("pass.txt");
            std::fs::write(&pass_file, "correct-horse-battery-staple").expect("write passphrase");
            let seed_file = dir.path().join("seed.bin");
            std::fs::write(&seed_file, [0x5Au8; 32]).expect("write seed");
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&seed_file, std::fs::Permissions::from_mode(0o600))
                    .expect("chmod seed");
            }

            let cli = cli_with_mode(mode);
            // No confirmation flag at all: the gate must fire.
            let err = register("tv-x-c-47", &pass_file, true, Some(&seed_file), &cli)
                .expect_err("an unconfirmed mutation must be refused");
            let rendered = err
                .hint()
                .unwrap_or_else(|| panic!("{mode:?} ConfirmationRequired must carry a hint"));
            assert!(
                rendered.contains(hint_must_name),
                "in {mode:?} the gate is `{works}`, so the hint must name it, or the operator \
                 retries with a flag that cannot work and gets the same error: {rendered}"
            );
            assert!(
                !rendered.contains("with `--confirm` to acknowledge"),
                "the hint must not tell the operator to add `--confirm` when that flag does not \
                 satisfy the {mode:?} gate: {rendered}"
            );
        }
    }

    /// The `FileInputRejected` variant must exist, exit 2, and render
    /// its own detail as the hint.
    ///
    /// Both seed-file refusals previously rode `Internal` at exit 64,
    /// whose hint is "re-run with `RUST_LOG=debug` and report the
    /// diagnostic". A 0644 seed file — a deliberate security gate with
    /// a one-command remedy — was escalated to a bug report, and in a
    /// CI pipeline the exit code is what decides retry versus page.
    #[test]
    fn tv_x_c_48_seed_file_refusals_exit_two_with_the_remedy_in_the_hint() {
        let err = OctoCliError::FileInputRejected {
            detail: "seed file mode 0644 is group- or world-readable; expected 0600".to_string(),
        };
        assert_eq!(
            err.exit_code(),
            2,
            "an operator-supplied file is input validation, not an internal fault"
        );
        assert_eq!(
            err.hint().as_deref(),
            Some("seed file mode 0644 is group- or world-readable; expected 0600"),
            "the remedy must be the hint, not a request for a diagnostic report"
        );
        // And the exit-2 operator-input family it joins.
        assert_eq!(OctoCliError::WeakPassphrase.exit_code(), 2);
        assert_eq!(
            OctoCliError::DevModeRequired {
                detail: "x".to_string()
            }
            .exit_code(),
            2
        );
    }

    #[test]
    fn tv_x_c_39_no_committed_envelope_reports_a_placeholder_did() {
        let src = production_src();

        // Every leg below asserts the DATA FLOW, not a literal. The
        // first draft used negative spelling checks
        // (`!body.contains("did: String::new())"`), which a reviewer
        // mutation defeated by keeping the field name and emptying the
        // source instead: `restored_did: abort_facts` fed by a
        // committed arm returning `String::new()`. A placeholder can
        // reach the envelope through any binding, so the only sound
        // check is that the value the envelope names is derived, on
        // the committed path, from a substrate read.

        // register: the substrate MINTS the DID and returns it. The
        // previous revision discarded the return with `?`.
        let reg = fn_body_code(src, "pub fn register(", "pub fn select(");
        // The whole committed-arm sequence in ONE needle. Each
        // conjunct separately is defeatable by shadowing - a later
        // `let did = Did(String::new())` satisfies `let did = store`
        // and the destructure while discarding the substrate return,
        // and the envelope then reports an empty DID that the
        // `active_now` comparison also matches, self-consistently
        // wrong. Requiring the substrate call to be IMMEDIATELY
        // followed by the fact extraction closes that.
        assert!(
            reg.contains(
                ".register(key, &passphrase, activate, now)\n            .map_err(OctoCliError::from)?;"
            ),
            "register must consume the substrate's returned DID with `?` rather than \
             discard it: {reg}"
        );
        // The destructure, tolerating the `mut` the dry-run sentinel
        // needs on `lifecycle_state`. The previous revision pinned
        // the bare form, so the fix for a fabricated `lifecycle_state`
        // would have failed this vector - the test enforcing a shape
        // the code has a legitimate reason to leave.
        let tail_at = reg
            .find("= register_facts;")
            .unwrap_or_else(|| panic!("register must destructure register_facts: {reg}"));
        let head_at = reg[..tail_at]
            .rfind("let (did, pubkey_hex,")
            .unwrap_or_else(|| panic!("register must destructure register_facts: {reg}"));
        let destructure = &reg[head_at..tail_at + "= register_facts;".len()];
        assert!(
            destructure.contains("lifecycle_state")
                && destructure.contains("active_now")
                && destructure.contains("registered_at_unix"),
            "every envelope field must be bound from register_facts: {destructure}"
        );
        // The shorthand bindings, anchored INSIDE the output struct
        // literal. The previous revision used a whole-body
        // `contains("did,")`, which the LET-DESTRUCTURE above
        // satisfies on its own - a dead conjunct that could not fail
        // for any reason including the one it names.
        let lit = reg
            .find("let output = IdentityRegisterOutput {")
            .map(|i| &reg[i..])
            .and_then(|post| post.find("};").map(|e| &post[..e]))
            .unwrap_or_else(|| {
                panic!("register must build an IdentityRegisterOutput literal: {reg}")
            });
        for field in ["did,", "pubkey_hex,", "lifecycle_state,", "active_now,"] {
            assert!(
                lit.contains(field),
                "{field} must be a shorthand binding from the committed arm, not a literal: \
                 {lit}"
            );
        }
        // The committed arm's tuple, which is where the substrate DID
        // actually enters the envelope. A reviewer mutation that
        // changed this element to `String::new()` - the exact
        // regression this vector exists to catch - left every
        // assertion above green, because the destructured NAMES were
        // still bound and still used; only the VALUE was dropped.
        let tuple = reg
            .find("let active_now = active_did_now")
            .map(|i| &reg[i..])
            .and_then(|post| post.find("\n    } else {").map(|e| &post[..e]))
            .unwrap_or_else(|| {
                panic!("the committed arm must build a facts tuple from the substrate: {reg}")
            });
        assert!(
            tuple.contains("did.0,") && tuple.contains("hex::encode(key_pubkey)"),
            "the committed arm's tuple must carry the substrate DID and pubkey, not empty \
             strings: {tuple}"
        );
        assert!(
            !reg.contains("did: String::new()")
                && !reg.contains("pubkey_hex: String::new()")
                && !reg.contains("active_now: activate"),
            "register must report substrate facts, not empty strings or the flag it was \
             passed; promotion only happens when the wallet had no active identity: {reg}"
        );
        // The `hex::encode(...)` conjunct alone is provenance-BLIND:
        // it is satisfied by any identifier, so replacing the binding
        // `let key_pubkey = key.public_key_bytes();` with a constant
        // leaves the envelope reporting 64 zeros and the vector green
        // across the whole module. Pin the binding's source too.
        assert!(
            reg.contains("let key_pubkey = key.public_key_bytes();")
                && reg.contains("hex::encode(key_pubkey)"),
            "pubkey_hex must be the record's public key read off the key, hex-encoded - \
             pinning only the `hex::encode` conjunct lets a constant through: {reg}"
        );
        assert!(
            reg.contains("store.active_did().map(|d| d.0.clone()) == Some(did.0.as_str())")
                || reg.contains("active_did_now.as_deref() == Some(did.0.as_str())"),
            "active_now must be the pointer read back from the store, compared to the new \
             DID - not the activate flag echoed: {reg}"
        );

        // F5: the envelope timestamps. Re-reading `Utc::now()` in the
        // envelope drifts from the value the substrate persisted, so
        // the envelope and `store.json` disagree on when. Each field
        // must be built from the substrate's own stamp, and the
        // spelling-bound negative control is what makes the conjunct
        // above falsifiable.
        assert!(
            reg.contains("registered_at: unix_to_rfc3339(registered_at_unix)")
                && reg.contains("record.registered_at_unix,")
                && !reg.contains("registered_at: chrono::Utc::now()"),
            "registered_at must be derived from the substrate's own registered_at_unix - a \
             second `Utc::now()` drifts from the persisted value: {reg}"
        );

        // rotate-complete: the predecessor is retired and the
        // successor promoted, so BOTH DIDs are knowable.
        let complete = fn_body_code(src, "pub fn rotate_complete(", "pub fn rotate_abort(");
        assert!(
            complete.contains("let old_did = unlocked.did().0.clone()")
                && complete.contains(".active_did()")
                && complete.contains("let new_did = store"),
            "rotate-complete must read the predecessor from the handle and the successor \
             from the store pointer: {complete}"
        );
        assert!(
            complete.contains("(new_did, old_did, Some(now as i64))")
                && complete.contains("new_did,")
                && complete.contains("old_did,"),
            "both envelope fields must be the destructured committed values: {complete}"
        );
        // The committed arm must not be able to yield an empty DID.
        // An absent pointer is now a refusal, not a `""` that reads
        // like data in a success envelope - and a completion that left
        // the active identity unchanged is also a refusal, because
        // reporting it as a success would tell the operator the old
        // key is retired when it is not.
        assert!(
            complete.contains("rotation completed but the store reports no active identity")
                && complete.contains("if new_did == old_did {")
                && complete.contains("rotation completed but the active identity did not change")
                && !complete.contains("unwrap_or_default()")
                && !complete.contains("unwrap_or(String::new())")
                && !complete.contains("new_did: String::new()")
                && !complete.contains("old_did: String::new()"),
            "rotate-complete must refuse an absent or unchanged active identity rather than \
             fall back to empty DIDs: {complete}"
        );
        // `None` under dry-run, not a `0` epoch sentinel. `0` is a
        // VALID timestamp that chrono renders, so a dry-run used to
        // report a real-looking completion time for an event that
        // never happened.
        assert!(
            complete.contains("completed_at: completed_at_unix.and_then(unix_to_rfc3339)")
                && complete.contains("(new_did, old_did, Some(now as i64))")
                && complete.contains("(String::new(), String::new(), None)")
                && !complete.contains("completed_at: chrono::Utc::now()")
                && !complete.contains("i64::try_from(completed_at_unix)"),
            "completed_at must be the `now` handed to complete_rotation, absent under \
             dry-run, and never a second clock read: {complete}"
        );

        // rotate-abort: abort restores THIS handle's record to Active.
        let abort = fn_body_code(src, "pub fn rotate_abort(", "pub fn is_dev_mode(");
        // The BINDING alone is not enough: a mutation that keeps
        // `let restored_did = unlocked.did().0.clone();` and then
        // returns `String::new()` as the arm's tail is a dead store -
        // the value is computed and discarded - and passed an
        // existence check. So the assertion pins the tail: the
        // abort call, then the binding as the arm's value.
        assert!(
            abort.contains(
                "unlocked.abort_rotation().map_err(OctoCliError::from)?;\n        \
                 let handle_did = unlocked.did().0.clone();"
            ) && abort.contains("let (restored_did, aborted_at_unix) = abort_facts")
                && abort.contains("restored_did,"),
            "the committed arm must RETURN the restored DID, and the store read must come \
             AFTER the abort call - a pre-mutation read cannot support the field doc's \
             claim that the identity was restored: {abort}"
        );
        // The envelope reports what the STORE holds, not what the
        // handler assumed going in, and a store that disagrees with
        // the handle is a refusal rather than a field that quietly
        // carries the wrong DID.
        assert!(
            abort.contains(".active_did()")
                && abort.contains(".filter(|d| *d == handle_did)")
                && abort.contains("rotation aborted but the store's active identity is not the"),
            "restored_did must be the store's active pointer read AFTER the abort, with a \
             defensive confirmation against the handle. The confirmation cannot fire today - \
             unlock derives the handle DID from the pointer and the abort path never writes it - \
             so this pins the SHAPE, not a defect this handler fixed. Saying otherwise is how a \
             tautology gets cited as a repair: {abort}"
        );
        assert!(
            abort.contains("aborted_at: aborted_at_unix.and_then(unix_to_rfc3339)")
                && abort.contains("(restored_did, Some(aborted_at_unix as i64))")
                && abort.contains("(String::new(), None)")
                && !abort.contains("aborted_at: chrono::Utc::now()")
                && !abort.contains("i64::try_from(aborted_at_unix)"),
            "aborted_at must be built from the captured stamp, absent under dry-run, and \
             never a second clock read: {abort}"
        );
        assert!(
            !abort.contains("restored_did: String::new()"),
            "rotate-abort must not fall back to an empty restored DID: {abort}"
        );
        assert!(
            abort.contains("reason: reason.map(|r| redact_string(r).into_owned())"),
            "rotate-abort must redact the operator reason before it reaches the \
             envelope, as the sibling revoke handler does: {abort}"
        );
    }

    /// Serialize the tests that set `OCTO_HOME`. The variable is
    /// process-global, so two such tests running concurrently would
    /// read each other's wallet root. Every other test in this module
    /// is pure and does not take the lock.
    static OCTO_HOME_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// tv_x_c_40 - the register envelope must describe the record that
    /// actually landed on disk. RUNTIME, not source-scanning.
    ///
    /// `tv_x_c_39` and its predecessors are source assertions, and a
    /// source assertion cannot tell a substrate-derived value from a
    /// placeholder when both are just expressions. Two reviewer
    /// mutations exploited exactly that: `let did = Did(String::new())`
    /// shadowing the substrate return, and
    /// `let restored_did = String::new()` keeping the correct tail.
    /// Every additional needle closed one and left the next open.
    ///
    /// So this vector EXECUTES `register` against a temporary
    /// `OCTO_HOME` and checks the wallet the substrate actually
    /// wrote. There is no string for a mutation to reword.
    #[test]
    fn tv_x_c_40_register_persists_what_the_envelope_claims() {
        let _guard = OCTO_HOME_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let home = tempfile::tempdir().expect("tempdir");
        let root = home.path().join("wallet");
        // SAFETY: the lock above is the only writer of OCTO_HOME among
        // this module's tests, and no other test reads it.
        unsafe { std::env::set_var("OCTO_HOME", home.path()) };

        let dir = tempfile::tempdir().expect("seed dir");
        let seed_file = dir.path().join("seed.hex");
        std::fs::write(&seed_file, hex::encode([7u8; 32])).expect("write seed");
        // 0600: the handler refuses a group- or world-readable seed,
        // which is correct behaviour - a tempdir write leaves 0644.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&seed_file, std::fs::Permissions::from_mode(0o600))
                .expect("chmod seed");
        }
        let pass_file = dir.path().join("pass.txt");
        std::fs::write(&pass_file, "correct-horse-battery-staple").expect("write passphrase");

        // Human mode with BOTH confirmation flags. Dev mode still
        // passes through `require_confirm`, and the pastejacking
        // contract requires `--confirm` AND `--confirm-acknowledge`.
        let mut cli = cli_with_mode(OperatorMode::Human);
        cli.mode.confirm = true;
        cli.mode.confirm_acknowledge = true;
        let result = register("tv-x-c-40", &pass_file, true, Some(&seed_file), &cli);
        unsafe { std::env::remove_var("OCTO_HOME") };
        result.expect("register against a temp OCTO_HOME must succeed");

        // Read what the SUBSTRATE persisted, independently of
        // whatever the handler put in its envelope.
        let store = octo_wallet::WalletStore::open_at(&root).expect("reopen the wallet");
        let active = store
            .active_did()
            .expect("a fresh store promotes its first identity");
        let record = store
            .identity_record(active)
            .expect("record for the active DID");

        assert_eq!(
            active.0,
            store.identity_record(active).expect("record").did.0,
            "the promoted identity must be the one the pointer names"
        );
        // The DID is derived from the 32-byte seed, so the record's
        // own public key pins what the DID must be. A register that
        // reported an empty DID would leave nothing to compare, which
        // is why the assertion is that the record EXISTS and the DID
        // is well-formed - the substrate side - and the register
        // envelope side is covered by tv_x_c_39's data-flow pins.
        assert!(
            !record.did.0.is_empty() && record.did.0.starts_with("did:octo:"),
            "the persisted DID must be a canonical DID, got {:?}",
            record.did.0
        );
        assert_eq!(
            record.lifecycle,
            octo_wallet::LifecycleState::Active,
            "an activated registration on a fresh store must persist Active"
        );
    }

    /// tv_x_c_41 - the full rotate/abort cycle must leave the
    /// predecessor Active and the successor gone, and the handler
    /// must run the abort it claims to run. RUNTIME, for the reason
    /// `tv_x_c_40` gives.
    ///
    /// A source vector for `rotate_abort` cannot distinguish "read
    /// the DID from the handle" from "bind a placeholder and return
    /// it" - a reviewer mutation did exactly that and left the whole
    /// suite green. This one drives register, rotate and rotate-abort
    /// against one temporary `OCTO_HOME` and then inspects the
    /// substrate's own index.
    #[test]
    fn tv_x_c_41_rotate_abort_restores_the_predecessor_and_drops_the_successor() {
        let _guard = OCTO_HOME_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let home = tempfile::tempdir().expect("tempdir");
        let root = home.path().join("wallet");
        // SAFETY: the lock above is the only writer of OCTO_HOME among
        // this module's tests, and no other test reads it.
        unsafe { std::env::set_var("OCTO_HOME", home.path()) };

        let dir = tempfile::tempdir().expect("work dir");
        let seed_file = dir.path().join("seed.hex");
        std::fs::write(&seed_file, hex::encode([7u8; 32])).expect("write seed");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&seed_file, std::fs::Permissions::from_mode(0o600))
                .expect("chmod seed");
        }
        let pass_file = dir.path().join("pass.txt");
        std::fs::write(&pass_file, "correct-horse-battery-staple").expect("write passphrase");

        let mut cli = cli_with_mode(OperatorMode::Human);
        cli.mode.confirm = true;
        cli.mode.confirm_acknowledge = true;
        let registered = register("tv-x-c-41", &pass_file, true, Some(&seed_file), &cli);
        unsafe { std::env::remove_var("OCTO_HOME") };
        registered.expect("register");

        // Begin the rotation through the substrate. `rotate` itself
        // reads the passphrase from stdin, and `std::io::set_stdin`
        // is unstable, so driving the handler is not possible here
        // without a new dependency. The claim this vector proves -
        // that an aborted rotation restores the predecessor and
        // leaves no event naming a deleted successor - is a claim
        // about the substrate, and the substrate is what is called.
        {
            let mut store = octo_wallet::WalletStore::open_at(&root).expect("reopen to rotate");
            let mut seed_out = Vec::new();
            let mut unlocked = store
                .unlock("correct-horse-battery-staple", &mut seed_out)
                .expect("unlock");
            let successor = octo_wallet::IdentityKey::from_seed([1u8; 32]);
            unlocked
                .begin_rotation(successor, "correct-horse-battery-staple", 1_700_000_010)
                .expect("begin_rotation");
        }

        let store = octo_wallet::WalletStore::open_at(&root).expect("reopen");
        let predecessor = store.active_did().expect("active DID").clone();
        let during = store
            .identity_record(&predecessor)
            .expect("predecessor record")
            .clone();
        assert_eq!(
            during.lifecycle,
            octo_wallet::LifecycleState::Rotating,
            "begin_rotation must leave the predecessor Rotating on disk. This vector drives \
             the SUBSTRATE call the handler makes - it never invokes the rotate handler - so \
             the message names the substrate, not the handler"
        );
        assert_eq!(
            during.rotation_history.len(),
            1,
            "begin_rotation must persist the rotation event, so the next PROCESS can find it - \
             the CLI runs rotate and rotate-abort as two OS processes and store.json is the \
             only carrier"
        );
        let successor_did = during.rotation_history[0].successor_did.clone();

        // Abort. `rotate_abort` reads the passphrase from stdin, and
        // `std::io::set_stdin` is unstable, so this drives the
        // SUBSTRATE call the handler makes. The handler-side wiring
        // (confirmation gate, echo, envelope field derivation) is
        // covered by `tv_x_c_39`; what needs proving at runtime is
        // that the abort it invokes actually restores the wallet, and
        // no source assertion can establish that.
        {
            let mut store = octo_wallet::WalletStore::open_at(&root).expect("reopen to abort");
            let mut seed_out = Vec::new();
            let mut unlocked = store
                .unlock("correct-horse-battery-staple", &mut seed_out)
                .expect("unlock the in-flight rotation");
            assert_eq!(
                unlocked.did().0,
                predecessor.0,
                "unlock must hand back the predecessor that begin_rotation left Rotating"
            );
            unlocked.abort_rotation().expect("abort_rotation");
        }

        let store = octo_wallet::WalletStore::open_at(&root).expect("reopen after abort");
        let after = store
            .identity_record(&predecessor)
            .expect("predecessor record after abort")
            .clone();
        assert_eq!(
            after.lifecycle,
            octo_wallet::LifecycleState::Active,
            "an aborted rotation must restore the predecessor to Active"
        );
        assert!(
            after.rotation_history.is_empty(),
            "an aborted rotation must leave no event naming a successor whose record \
             abort deleted; found {:?}",
            after.rotation_history
        );
        assert!(
            store.identity_record(&successor_did).is_err(),
            "the aborted successor's record must be gone from the index"
        );
    }

    /// tv_x_c_38 - CORR-19 plus the R5 exit-code correction. An empty
    /// or whitespace-only reason is operator input, so it must be
    /// refused as such: `InvalidReason` at exit 2, alongside
    /// `ClapParse` and `WeakPassphrase`. The previous assertion
    /// pinned `Internal` at exit 64 - the variant every consuming
    /// handler's exit-code list documents as "unexpected substrate
    /// error" - so a typo was reported to the operator as a substrate
    /// fault. The sibling substrate guards
    /// (`ReasonContainsControlChars`, `ReasonTooLong`) were
    /// corrected to the same slot in the previous round; this local
    /// guard was the last one left on the wrong one.
    #[test]
    fn revoke_rejects_empty_reason() {
        let mut cli = cli_with_mode(OperatorMode::Human);
        cli.mode.confirm = true;
        for reason in [
            "", "   ", "	
",
        ] {
            let r = revoke(reason, false, &cli);
            let e = r.expect_err("a blank reason must be refused");
            assert_eq!(
                e.exit_code(),
                2,
                "a blank reason is operator input and must exit 2, got {} from {e:?}",
                e.exit_code()
            );
            assert!(
                matches!(e, OctoCliError::InvalidReason { .. }),
                "a blank reason must map to InvalidReason, not Internal: {e:?}"
            );
        }
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
        // The sanitization property is the point of this vector and
        // is unchanged. The VARIANT is now asserted separately: this
        // revision pinned `Internal` at exit 64, which is right for
        // every OTHER `Config` the substrate can raise and wrong for
        // the only one `WalletStore::open` actually raises.
    }

    /// A `Config` from the OPEN path is a missing environment, not a
    /// fault. The prior vector pinned the opposite, on the reasoning
    /// that exit 27 is produced upstream by `home::resolve` - which
    /// is true of the rest of the CLI and false of these five
    /// subcommands, which call `WalletStore::open` directly and never
    /// reach `home::resolve`. So exit 27 was unreachable here and
    /// `env -u OCTO_HOME -u HOME octo identity list` reported an
    /// internal error at exit 64.
    #[test]
    fn tv_x_c_49_a_missing_home_from_the_open_path_is_exit_twenty_seven() {
        assert!(
            matches!(
                map_wallet_open_error(octo_wallet::WalletError::Config(
                    "neither $OCTO_HOME nor $HOME resolves to a non-empty path".to_string()
                )),
                OctoCliError::NoOctoHome
            ),
            "a Config from WalletStore::open is the empty-root case and must map to 27"
        );
        // And a genuine fault stays at 64 rather than being swept
        // into the same bucket.
        assert!(
            matches!(
                map_wallet_open_error(octo_wallet::WalletError::Io(std::io::Error::other("disk"))),
                OctoCliError::Internal(_)
            ),
            "an Io from the open path is a real fault and must stay at 64"
        );
    }

    /// tv_x_c_51 — the `new_did` doc must not name a sentinel the
    /// field never carries, and `schemars` ships it into the schema.
    ///
    /// Two earlier revisions had this doc wrong in sequence: it said
    /// the field was always `pending`, then that it was
    /// `<not-read: dry-run>` under `--dry-run`. Both shipped into the
    /// generated JSON Schema as the contract. The second was wrong
    /// because `rotate` derives the successor from its seed BEFORE it
    /// reaches the dry-run branch, so a dry-run preview names the
    /// identity a committed run would promote - a real DID.
    ///
    /// A doc that is wrong is not a style problem here, so the vector
    /// pins two things: that the doc names no sentinel for this
    /// field, and that the derivation it claims is the one the
    /// handler actually performs, before the branch.
    #[test]
    fn tv_x_c_51_the_new_did_doc_matches_what_the_field_carries() {
        let src = production_src();

        // The field's own doc block, scoped to the struct it belongs
        // to. A file-wide absence check would pass vacuously: the
        // sentinel IS used, by register/select `lifecycle_state`.
        let start = src
            .find("pub struct IdentityRotateOutput")
            .expect("IdentityRotateOutput present in production source");
        let field = start
            + src[start..]
                .find("pub new_did: String,")
                .expect("new_did field present in IdentityRotateOutput");
        let doc_start = src[..field].rfind("/// DID of the new").expect(
            "the new_did doc must be a line doc - if this fires the doc moved to a form this \
                 vector does not model, and the vector must be reworked rather than deleted",
        );
        let doc = &src[doc_start..field];
        // Whitespace-normalised, because rustfmt rewraps doc comments
        // and a substring that spans a line break is a substring that
        // breaks on the next unrelated edit. Normalising here is what
        // keeps this vector about the doc's CLAIM rather than about
        // where the formatter happened to break it.
        let flat = doc
            .lines()
            .map(|l| l.trim_start().trim_start_matches("/").trim())
            .collect::<Vec<_>>()
            .join(" ");
        assert!(
            !flat.contains("not-read: dry-run")
                || flat.contains("never a `<not-read: dry-run>` sentinel"),
            "the new_did doc must not claim the dry-run sentinel for a field that carries a real \
             DID under --dry-run. schemars copies this text into the JSON Schema, so a false \
             claim here is a false contract for every consumer: {doc}"
        );
        // Affirmative, not a bare absence. A doc that REFERENCES the
        // old behaviour ("a prior revision said it was always
        // `pending`") trips a naive absence check while claiming the
        // opposite, which is how a source-grep vector ends up
        // forbidding the very sentence that makes the doc correct.
        assert!(
            flat.contains("never a `pending` placeholder"),
            "the new_did doc must affirm that the field is never a pending placeholder, so a \
             reader is not left choosing between a claim and a historical note: {doc}"
        );
        assert!(
            flat.contains("derived locally from the successor key"),
            "the doc must state the derivation it is claiming, so a reader can check it: {doc}"
        );

        // The derivation is real, and it happens before the branch
        // that a reader would assume makes the value unknown.
        let body = fn_body_code(src, "pub fn rotate(", "pub fn revoke(");
        let derive = body
            .find("let new_did = successor.did().0.clone();")
            .expect("rotate must derive new_did from the successor key");
        let dry_branch = body
            .find("let proof = if cli.mode.dry_run {")
            .expect("the dry-run branch must be findable");
        assert!(
            derive < dry_branch,
            "new_did must be derived BEFORE the dry-run branch. After it, the value would depend \
             on a branch the doc says does not exist, and the two would silently disagree."
        );
        assert!(
            !body.contains("new_did: \"did:octo:pending\"")
                && !body.contains("new_did: String::new()"),
            "rotate must not bind new_did to a placeholder: {body}"
        );
    }

    ///
    /// The dry-run arm of `rotate-complete` and `rotate-abort` used to
    /// hand the envelope `0`, and `0` is a VALID unix timestamp that
    /// chrono renders as `1970-01-01T00:00:00Z`. A dry-run therefore
    /// reported a real-looking completion time for a rotation that had
    /// not happened — the epoch is indistinguishable from a genuine
    /// 1970 stamp to whatever consumes the envelope.
    ///
    /// Two layers pin this, and the split matters:
    ///
    /// - WHICH arm supplies the value is a source-level question, so
    ///   `tv_x_c_39` pins the binding. It was mutation-proven: making
    ///   either dry-run arm hand back `Some(0)` fails `tv_x_c_39`.
    /// - WHAT reaches the wire is a serialization question, and that
    ///   is what this vector proves. Reverting the field to a
    ///   non-`Option` `DateTime` does not compile, so the type
    ///   change is itself the structural pin: there is no longer any
    ///   expression a future revision can write that fabricates a
    ///   timestamp where there is none.
    ///
    /// This vector therefore pins the JSON a script actually reads,
    /// not the Rust expression that produces it.
    #[test]
    fn tv_x_c_50_a_dry_run_timestamp_is_null_not_the_epoch() {
        let complete = IdentityRotateCompleteOutput {
            new_did: String::new(),
            old_did: String::new(),
            completed_at: None,
        };
        let json = serde_json::to_string(&complete).expect("rotate-complete output serializes");
        assert!(
            json.contains("\"completed_at\":null"),
            "a dry-run must render completed_at as null: {json}"
        );
        assert!(
            !json.contains("1970-01-01"),
            "the epoch must never appear as a rotation timestamp - it is indistinguishable \
             from a real 1970 stamp: {json}"
        );

        let abort = IdentityRotateAbortOutput {
            restored_did: String::new(),
            aborted_at: None,
            reason: None,
        };
        let json = serde_json::to_string(&abort).expect("rotate-abort output serializes");
        assert!(
            json.contains("\"aborted_at\":null"),
            "a dry-run must render aborted_at as null: {json}"
        );
        assert!(
            !json.contains("1970-01-01"),
            "the epoch must never appear as an abort timestamp: {json}"
        );

        // And the committed path still renders a real stamp, so the
        // `None` arm is not simply always taken.
        let committed = IdentityRotateCompleteOutput {
            new_did: "did:octo:0xee".to_string(),
            old_did: "did:octo:0xff".to_string(),
            completed_at: unix_to_rfc3339(1_700_000_000),
        };
        let json = serde_json::to_string(&committed).expect("committed output serializes");
        assert!(
            json.contains("\"completed_at\":\"2023-11-14T22:13:20Z\""),
            "a committed rotation must render the substrate's own stamp: {json}"
        );
    }

    /// CORR-08: auditor mode is blocked at every identity handler entry.
    /// `list` reports every DID, public key, lifecycle state and
    /// registration timestamp - a strict superset of what
    /// `identity show` reports, and `show` is gated. Without the gate
    /// on `list` the gate on `show` is vacuous: an auditor session
    /// refused `whoami` and `show` reads the whole inventory through
    /// `list`. The vector calls the real handler so it fails if the
    /// gate line is deleted, not merely if `block_auditor` misbehaves.
    #[test]
    fn tv_x_c_42_list_is_blocked_in_auditor_mode() {
        let cli = cli_with_mode(OperatorMode::Auditor);
        let r = list(&cli);
        assert!(
            matches!(r, Err(OctoCliError::AuditorDenied { .. })),
            "list must refuse auditor mode: {r:?}"
        );
    }

    /// The gate is present on `list` and not accidentally absent.
    #[test]
    fn tv_x_c_43_auditor_gate_covers_every_identity_reader() {
        let src = production_src();
        for (start, end, name) in [
            ("pub fn whoami(", "pub fn show(", "whoami"),
            ("pub fn show(", "pub fn rotate(", "show"),
            ("pub fn list(", "pub fn rotate_complete(", "list"),
        ] {
            let body = fn_body_code(src, start, end);
            assert!(
                body.contains(&format!("block_auditor(cli, \"identity {name}\")?;")),
                "{name} must gate auditor mode: {body}"
            );
        }
    }

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
            registered_at: Some(DateTime::<Utc>::from_timestamp(1_700_000_000, 0).unwrap()),
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
        let src = production_src();
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
        let src = production_src();
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
        let src = production_src();
        assert!(
            src.contains("would select: did={did}"),
            "select handler missing canonical-payload echo: {src}"
        );
    }

    /// tv_x_c_6 — `select` handler must call `WalletStore::select` at
    /// the substrate boundary.
    #[test]
    fn tv_x_c_6_select_calls_wallet_store_select() {
        let src = production_src();
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
                registered_at: Some(DateTime::<Utc>::from_timestamp(1_700_000_000, 0).unwrap()),
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
            registered_at: Some(DateTime::<Utc>::from_timestamp(1_700_000_000, 0).unwrap()),
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
        let end = slice.find("pub fn rotate_complete").expect("end marker for the slice bound must exist — if this fires, the function was renamed and the bound would have silently degraded to the test module");
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
            completed_at: Some(DateTime::<Utc>::from_timestamp(1_700_000_000, 0).unwrap()),
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
        let src = production_src();
        assert!(
            src.contains("would rotate-complete:"),
            "rotate_complete handler missing canonical-payload echo: {src}"
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
        let end = slice.find("pub fn rotate_abort").expect("end marker for the slice bound must exist — if this fires, the function was renamed and the bound would have silently degraded to the test module");
        let body = &slice[..end];
        assert!(
            body.contains("require_confirm(cli, \"identity rotate-complete\")"),
            "rotate_complete must gate behind require_confirm at handler entry: {body}"
        );
        let require_pos = body
            .find("require_confirm(cli, \"identity rotate-complete\")")
            .expect("require_confirm substring present");
        // Belt and braces: handler-side confirm gate MUST come
        // before the unlock call (which would prompt for the
        // passphrase or read stdin). After AC-7 the
        // `rotate_complete` handler delegates to
        // `UnlockedWallet::complete_rotation` rather than the
        // free `octo_wallet::complete_rotation`; the substring
        // assertion looks for the new path.
        let mutation_pos = body
            .find(".complete_rotation(now)")
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
            aborted_at: Some(DateTime::<Utc>::from_timestamp(1_700_000_000, 0).unwrap()),
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
        let src = production_src();
        assert!(
            src.contains("would rotate-abort:"),
            "rotate_abort handler missing canonical-payload echo: {src}"
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
        let end = slice.find("\n}\n").expect("end marker for the slice bound must exist — if this fires, the function was renamed and the bound would have silently degraded to the test module");
        let body = &slice[..end];
        for needle in [
            "IdentityAction::Register {",
            "register(label",
            "IdentityAction::Select {",
            "select(did",
            "IdentityAction::List {",
            "list(cli)",
            "IdentityAction::RotateComplete {",
            "rotate_complete(*passphrase_stdin, cli)",
            "IdentityAction::RotateAbort {",
            "rotate_abort(reason.as_deref(), *passphrase_stdin, cli)",
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
        let end = slice.find("pub fn list(").expect("end marker for the slice bound must exist — if this fires, the function was renamed and the bound would have silently degraded to the test module");
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
        let end = slice.find("pub fn select(").expect("end marker for the slice bound must exist — if this fires, the function was renamed and the bound would have silently degraded to the test module");
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
        // DIRECTION, not proximity. The previous assertion was a
        // +/-200 byte window with no ordering test, so the
        // follow-the-read form
        //     let plain = read_to_string(...)?;
        //     let wrapped = Zeroizing::new(plain);
        // satisfied it while leaving an unzeroized `String` on the
        // heap. The wrap must OPEN AT OR BEFORE the read: a
        // `Zeroizing::new(read_to_string(...)?)` sites the
        // constructor first, a follow-the-read form sites it second,
        // and only the second is wrong.
        assert!(
            zeroize_site <= read_site,
            "the Zeroizing wrap must OPEN AT OR BEFORE the passphrase read, not follow it - \
             a `let plain = read(...); Zeroizing::new(plain)` leaves the plaintext on the \
             heap: {body}"
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
        let end = slice.find("pub fn select(").expect("end marker for the slice bound must exist — if this fires, the function was renamed and the bound would have silently degraded to the test module");
        let body = &slice[..end];
        // These three were three INDEPENDENT substring checks, so
        // the vector stayed green when the check was neutered with
        // `if false && mode & 0o077 != 0 {` - the substrings
        // survived, the refusal did not. What has to be proven is
        // that they form ONE live guard: the mode read feeding a
        // condition that actually tests the mask.
        let mode_at = body
            .find("let mode = seed_meta.permissions().mode();")
            .unwrap_or_else(|| {
                panic!(
                    "register must stat the seed file and read its mode per AC-25 \
                         obligation 2: {body}"
                )
            });
        let guard_at = body[mode_at..]
            .find("if mode & 0o077 != 0 {")
            .unwrap_or_else(|| {
                panic!(
                    "the mode read must feed a LIVE guard on the 0o077 mask - three \
                         independent substrings do not make a refusal, and `if false && ...` \
                         satisfies all of them: {body}"
                )
            });
        let guard = &body[mode_at + guard_at..];
        assert!(
            guard.contains("group- or world-readable"),
            "the refusal INSIDE the guard must name the mode violation, so an operator can \
             tell a permissions problem from a read problem: {guard}"
        );
        assert!(
            guard.find("return Err(").is_some(),
            "the guard must actually return, not log: {guard}"
        );
    }

    /// tv_x_c_27 — AC-9 `--passphrase-stdin` flag on
    /// `rotate-complete` + `rotate-abort`. The clap variants must
    /// declare the flag so an automation caller can opt out of
    /// the interactive TTY prompt per mission
    /// 0011-x-wallet-store-cli §AC-9 first clause. Source-presence
    /// pins the variant declaration; a regression that drops the
    /// flag becomes a substring failure rather than a silent
    /// hydration of `--allow-stdin-secret` with no read site.
    #[test]
    fn tv_x_c_27_rotate_complete_abort_declare_passphrase_stdin_flag() {
        let src = include_str!("identity.rs");
        // The two clap variants must each carry a
        // `passphrase_stdin: bool` field with a `--passphrase-stdin`
        // long flag.
        let start = src
            .find("RotateComplete {")
            .expect("RotateComplete variant present");
        let slice = &src[start..];
        let end = slice.find("RotateAbort {").expect("end marker for the slice bound must exist — if this fires, the function was renamed and the bound would have silently degraded to the test module");
        let rotate_complete_block = &slice[..end];
        assert!(
            rotate_complete_block.contains("passphrase_stdin: bool"),
            "RotateComplete variant must carry passphrase_stdin field per AC-9: {rotate_complete_block}"
        );
        // The flag check was whole-block `contains("#[arg(long)]")`,
        // which ANY field in the block satisfies. Scope it to the
        // attribute immediately above `passphrase_stdin`, so a
        // sibling field's flag cannot stand in for this one's.
        let pc_field = rotate_complete_block
            .find("passphrase_stdin: bool")
            .expect("passphrase_stdin field");
        let pc_field = rotate_complete_block[..pc_field]
            .rfind("#[arg(")
            .expect("passphrase_stdin carries a clap attribute");
        let pc_attr = &rotate_complete_block[pc_field..];
        assert!(
            pc_attr.contains("long")
                && pc_attr.contains('\n')
                && pc_attr[..pc_attr.find("passphrase_stdin").unwrap_or(0)].contains("long"),
            "the clap attribute IMMEDIATELY above passphrase_stdin must declare `long` - a \
             block-wide #[arg(long)] anywhere is satisfied by a sibling field: {pc_attr}"
        );

        let start = src
            .find("RotateAbort {")
            .expect("RotateAbort variant present");
        let slice = &src[start..];
        let end = slice.find("}").expect("end marker for the slice bound must exist — if this fires, the function was renamed and the bound would have silently degraded to the test module");
        let rotate_abort_block = &slice[..end];
        assert!(
            rotate_abort_block.contains("passphrase_stdin: bool"),
            "RotateAbort variant must carry passphrase_stdin field per AC-9: {rotate_abort_block}"
        );
        // The abort half checked only the field and never the flag -
        // the same defect one arm up, half-fixed.
        let pa_field = rotate_abort_block
            .find("passphrase_stdin: bool")
            .expect("passphrase_stdin field");
        let pa_field = rotate_abort_block[..pa_field]
            .rfind("#[arg(")
            .expect("passphrase_stdin carries a clap attribute");
        let pa_attr = &rotate_abort_block[pa_field..];
        assert!(
            pa_attr[..pa_attr.find("passphrase_stdin").unwrap_or(0)].contains("long"),
            "the clap attribute IMMEDIATELY above passphrase_stdin must declare `long`: \
             {pa_attr}"
        );
    }

    /// tv_x_c_28 — AC-10 no-TTY pre-flight. The
    /// `acquire_passphrase` helper must fail closed with
    /// `WalletLocked` (exit 92) when the interactive prompt is
    /// unreachable (no `--passphrase-stdin` AND no TTY). Source-
    /// presence pins the failure mode + exit-code assignment so
    /// a regression that falls through to a different error
    /// (e.g. a panic or a `StdinSecretRefused` exit 15) becomes
    /// a substring failure rather than a silent terminal hang.
    #[test]
    fn tv_x_c_28_acquire_passphrase_returns_wallet_locked_on_no_tty() {
        let src = include_str!("identity.rs");
        // The helper must reference the rpassword prompt as the
        // interactive path AND must collapse the prompt failure
        // to `OctoCliError::WalletLocked` so the §AC-10 exit 92
        // contract holds. A regression that drops the WalletLocked
        // translation falls through to whatever rpassword returns
        // (typically an io::Error with exit 64 Internal), which
        // does NOT match the mission contract.
        //
        // The scan is bounded to the `acquire_passphrase` **body**, not
        // the whole file. An earlier revision scanned the whole file and
        // was satisfied by the helper's own doc comment, which names
        // both `rpassword::prompt_password` and `WalletLocked` while
        // describing what the body should do — so deleting the prompt
        // call left the vector green. The doc comment is a
        // restatement of the contract, not the contract.
        let start = src
            .find("pub(crate) fn acquire_passphrase(")
            .expect("acquire_passphrase fn present");
        let slice = &src[start..];
        let end = slice.find("pub fn require_confirm(").expect("end marker for the slice bound must exist — if this fires, the function was renamed and the bound would have silently degraded to the test module");
        let body = &slice[..end];
        assert!(
            body.contains("rpassword::prompt_password"),
            "acquire_passphrase must use rpassword::prompt_password for interactive entry: {body}"
        );
        assert!(
            body.contains("OctoCliError::WalletLocked"),
            "acquire_passphrase must fail closed with WalletLocked per AC-10: {body}"
        );
    }

    /// tv_x_c_34 — AC-10 passphrase stdin terminator handling.
    ///
    /// The trim was computed in two steps, testing `ends_with(chr)` on
    /// the *untruncated* string after the newline had already been
    /// counted. For CRLF input the string still ends in a newline, so
    /// the carriage return survived and the passphrase gained a
    /// trailing CR. The operator then got `WalletLocked` —
    /// indistinguishable from a wrong passphrase.
    ///
    /// The first twelve lines of this vector looped over string
    /// literals it declared itself and asserted stdlib behaviour on
    /// them. No CipherOcto change can affect that loop, so it was
    /// twelve lines of green regardless of the production code. It is
    /// deleted.
    ///
    /// The scan also used `fn_body`, which does not strip line
    /// comments, so a mutation that deleted the call and left the
    /// token in a trailing comment passed; and a mutation narrowing
    /// the character set to just the CR reintroduced the exact bug the
    /// vector exists to catch, undetectably. This uses
    /// `fn_body_code` and pins the BOTH-character set as a single
    /// literal.
    #[test]
    fn tv_x_c_34_stdin_passphrase_strips_crlf() {
        let src = production_src();
        let body = fn_body_code(
            src,
            "pub(crate) fn acquire_passphrase(",
            "pub fn require_confirm(",
        );

        // The exact call, with BOTH terminators. Two separate
        // `contains` conjuncts would each be satisfied by a
        // half-correct set, and a narrowing to one character is the
        // whole bug.
        assert!(
            body.contains("trim_end_matches(['\\r', '\\n'])"),
            "acquire_passphrase must strip CR and LF in one call - \
             trim_end_matches(['\\r', '\\n']) - not a two-step length computation and not a \
             single-character set: {body}"
        );
        assert!(
            !body.contains("ends_with('\\r')"),
            "acquire_passphrase must not test for a carriage return against the untruncated \
             string: {body}"
        );
    }

    /// tv_x_c_35 - F-5. `identity select` must resolve both envelope
    /// fields from the substrate. The previous revision computed
    /// `previous` and `record`, discarded both with `let _`, and
    /// emitted `previous_active_did: None` and
    /// `lifecycle_state: "Active".to_string()` unconditionally, so a
    /// second `select` reported "no previous identity" and a
    /// Revoked record reported "Active".
    ///
    /// `select` only moves the active pointer; it does not transition
    /// lifecycle, which is exactly why a hard-coded "Active" is
    /// wrong rather than merely stale.
    #[test]
    fn tv_x_c_35_select_resolves_envelope_fields_from_substrate() {
        let src = production_src();
        let body = fn_body_code(src, "pub fn select(", "pub fn list(");

        assert!(
            !body.contains("previous_active_did: None,"),
            "select must not hard-code a null previous_active_did: {body}"
        );
        assert!(
            !body.contains("lifecycle_state: \"Active\".to_string()"),
            "select must not hard-code the lifecycle label; it is read from the \
             record the substrate returns: {body}"
        );
        assert!(
            !body.contains("let _ = previous;") && !body.contains("let _record ="),
            "select must not discard the substrate lookups it performs: {body}"
        );
        // ORDERING, not just presence. The assertion message above
        // claims the capture happens "before mutating the store", and
        // a reviewer mutation moved the capture to AFTER
        // `store.select(&parsed)` - leaving `previous_active_did`
        // permanently equal to the identity just selected, so the
        // field carried no information and the vector stayed green.
        let capture = body
            .find("previous_active_did = store.active_did()")
            .unwrap_or_else(|| panic!("select must capture the prior active pointer: {body}"));
        let mutate = body
            .find("store.select(&parsed)")
            .unwrap_or_else(|| panic!("select must mutate the store: {body}"));
        assert!(
            capture < mutate,
            "the prior active pointer must be read BEFORE select moves it; at {} \
             vs {} the field always equals the identity just selected: {body}",
            capture,
            mutate
        );
        let read_record = body
            .find("store.identity_record(&parsed)")
            .unwrap_or_else(|| panic!("select must read the record it echoes: {body}"));
        let format = body
            .find("format!(\"{:?}\", record.lifecycle)")
            .unwrap_or_else(|| {
                panic!("select must derive lifecycle_state from the record: {body}")
            });
        assert!(
            read_record < format,
            "lifecycle_state must be derived FROM the record, so the read precedes \
             the format; at {} vs {} the format cannot be using it: {body}",
            read_record,
            format
        );
    }

    /// tv_x_c_36 - F-7. `identity rotate` must report the real
    /// successor DID on the committed path. It used to emit
    /// `new_did: "did:octo:pending"` unconditionally, so a rotation
    /// that had already been written to the store still reported a
    /// placeholder to the operator.
    ///
    /// The capture must precede `begin_rotation`, which consumes
    /// `successor` by value - hence the ordering assertion, not just
    /// the presence of the string.
    #[test]
    fn tv_x_c_36_rotate_reports_the_real_successor_did() {
        let src = production_src();
        let body = fn_body(src, "pub fn rotate(", "pub fn revoke(");

        assert!(
            !body.contains("new_did: \"did:octo:pending\".to_string()"),
            "rotate must not emit the pending placeholder as its new_did: {body}"
        );
        let capture = body
            .find("let new_did = successor.did()")
            .expect("rotate must capture the successor DID before begin_rotation consumes it");
        let consumed = body
            .find("begin_rotation(")
            .expect("rotate must call begin_rotation");
        assert!(
            capture < consumed,
            "the successor DID must be captured before begin_rotation consumes it: {body}"
        );
        assert!(
            body.contains("new_did,"),
            "the IdentityRotateOutput must carry the captured DID: {body}"
        );
    }

    /// tv_x_c_33 — AC-13 bounded runtime. The no-TTY pre-flight is
    /// the last thing standing between an unattended `cron` job or CI
    /// step and a process that blocks forever on an invisible prompt.
    ///
    /// `tv_x_c_28` proves the pre-flight's *shape* by reading the
    /// `acquire_passphrase` body. That is not the same claim: a
    /// regression that kept the `WalletLocked` arm but moved the
    /// prompt ahead of it would leave `tv_x_c_28` green and hang the
    /// process. The property that actually matters is that the call
    /// **returns**, and it can only be shown by running it.
    ///
    /// AC-10's "fails closed before prompting" is a **production**
    /// obligation, so the vector pins the pre-check in the production
    /// body and pins its position relative to the prompt.
    ///
    /// An earlier revision of this vector instead *ran*
    /// `acquire_passphrase` and bounded it with a five-second channel
    /// timeout. That measured the test environment, not the code:
    /// `cargo test` hands the test binary whatever stdin the developer
    /// has, so under an interactive terminal the call reached
    /// `rpassword`, wrote a live prompt into the operator's shell, and
    /// the vector failed after five seconds with a leaked worker thread
    /// — while CI, which has no TTY, stayed green. Reproduced twice
    /// independently before it was removed.
    ///
    /// The run-time half is kept as a separate, honestly-scoped vector
    /// below rather than folded in here.
    #[test]
    fn tv_x_c_33_no_tty_preflight_precedes_the_prompt() {
        let src = production_src();
        let body = fn_body(
            src,
            "pub(crate) fn acquire_passphrase(",
            "pub fn require_confirm(",
        );
        // The FULLY-QUALIFIED call, not the zero-arg substring
        // `is_terminal()`. The code is
        // `std::io::IsTerminal::is_terminal(&std::io::stdin())` - a
        // path-qualified call with one argument - so the zero-arg
        // form occurs ONLY inside backticks in the doc comment that
        // describes the pre-flight. A mutation that deleted the
        // pre-flight entirely left this vector green, along with
        // three siblings, because the needle still resolved to the
        // prose explaining the code that was gone.
        let precheck = body
            .find("std::io::IsTerminal::is_terminal(&std::io::stdin())")
            .unwrap_or_else(|| {
                panic!("acquire_passphrase must test the TTY before prompting: {body}")
            });
        let prompt = body
            .find("rpassword::prompt_password")
            .unwrap_or_else(|| panic!("acquire_passphrase must prompt via rpassword: {body}"));
        assert!(
            precheck < prompt,
            "the no-TTY pre-flight must come BEFORE the prompt, or the operator is prompted on a pipe: {body}"
        );
        // The pre-flight's OWN error, anchored on the guard that
        // returns it. `body.contains("OctoCliError::WalletLocked")`
        // was satisfied by the `Err(_)` arm at the end of the
        // function, so a mutation that replaced the pre-flight's
        // `WalletLocked` with an `Internal` still passed.
        assert!(
            body.contains(
                "if !std::io::IsTerminal::is_terminal(&std::io::stdin()) {\n        return Err(OctoCliError::WalletLocked);"
            ),
            "the no-TTY pre-flight must return WalletLocked itself: {body}"
        );
        assert!(
            body.contains("Err(_) => Err(OctoCliError::WalletLocked)"),
            "a prompt failure must also fail closed as WalletLocked, not as an \
             unclassified Internal at exit 64: {body}"
        );
    }

    /// The bounded-runtime half of the old `tv_x_c_33`, kept separate
    /// because it is genuinely environment-scoped. It runs **only**
    /// when stdin is a terminal-free pipe — the case the property is
    /// about — and says so when it does not run, rather than failing on
    /// a developer's terminal or silently passing.
    ///
    /// `CI` environments have no TTY, so this executes on the runner.
    #[test]
    fn tv_x_c_33b_no_tty_prompt_returns_within_bound_when_stdin_is_not_a_tty() {
        const BOUND: std::time::Duration = std::time::Duration::from_secs(5);
        if std::io::IsTerminal::is_terminal(&std::io::stdin()) {
            eprintln!(
                "SKIPPED: stdin is a terminal, so this vector's precondition (a non-TTY \
                 stdin) does not hold. Running it here would prompt the developer. \
                 Covered on any non-TTY runner, including CI."
            );
            return;
        }
        let cli = cli_with_mode(OperatorMode::Ci);
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let outcome = super::acquire_passphrase(&cli, "rotate complete", false)
                .map_err(|e| (format!("{e}"), e.exit_code()))
                .map(|_| (String::from("<passphrase>"), 0));
            let _ = tx.send(outcome);
        });
        let outcome = rx
            .recv_timeout(BOUND)
            .expect("acquire_passphrase must not block on a TTY-less stdin");
        let (rendered, exit_code) = outcome
            .expect_err("a non-TTY stdin that never delivers a byte must not yield a passphrase");
        assert_eq!(
            exit_code, 92,
            "the no-TTY pre-flight must fail closed as WalletLocked at exit 92: {rendered}"
        );
    }

    /// tv_x_c_29 — AC-7 partial. The `rotate_complete` handler
    /// must migrate from `octo_wallet::complete_rotation(&mut
    /// key, ...)` (free fn) to `store.unlock(...)` →
    /// `unlocked.complete_rotation(...)` (UnlockedWallet method).
    /// The free fn is the metadata-only path; the unlocked path
    /// is the signing path per the §AC-7 split. Source-presence
    /// pins both: (a) `store.unlock(` call appears in the
    /// `rotate_complete` body, AND (b) `octo_wallet::complete_rotation`
    /// is NOT used (a regression that adds the free fn alongside
    /// the unlocked path leaves the metadata-only mutation
    /// reachable, which is the over-classification §AC-7 warns
    /// against).
    #[test]
    fn tv_x_c_29_rotate_complete_uses_unlocked_wallet_migration() {
        let src = include_str!("identity.rs");
        let start = src
            .find("pub fn rotate_complete(")
            .expect("rotate_complete fn present");
        let slice = &src[start..];
        let end = slice.find("pub fn rotate_abort").expect("end marker for the slice bound must exist — if this fires, the function was renamed and the bound would have silently degraded to the test module");
        let body = &slice[..end];
        assert!(
            body.contains(".unlock(passphrase.as_str(), seed_buf.as_mut())"),
            "rotate_complete must migrate to UnlockedWallet via store.unlock per AC-7: {body}"
        );
        assert!(
            body.contains(".complete_rotation(now)"),
            "rotate_complete must call UnlockedWallet::complete_rotation per AC-7: {body}"
        );
        assert!(
            !body.contains("octo_wallet::complete_rotation("),
            "rotate_complete must NOT keep the free-fn path per AC-7 migration: {body}"
        );
    }

    /// tv_x_c_30 — AC-7 partial (rotate-abort mirror of tv_x_c_29).
    /// The `rotate_abort` handler must likewise migrate to
    /// `UnlockedWallet::abort_rotation` so the seed is held by
    /// the handle's lifetime, not by the long-lived free-fn
    /// `IdentityKey` clone.
    #[test]
    fn tv_x_c_30_rotate_abort_uses_unlocked_wallet_migration() {
        let src = include_str!("identity.rs");
        let start = src
            .find("pub fn rotate_abort(")
            .expect("rotate_abort fn present");
        let slice = &src[start..];
        let end = slice.find("// ---").expect("end marker for the slice bound must exist — if this fires, the function was renamed and the bound would have silently degraded to the test module");
        let body = &slice[..end];
        assert!(
            body.contains(".unlock(passphrase.as_str(), seed_buf.as_mut())"),
            "rotate_abort must migrate to UnlockedWallet via store.unlock per AC-7: {body}"
        );
        assert!(
            body.contains("unlocked.abort_rotation()"),
            "rotate_abort must call UnlockedWallet::abort_rotation per AC-7: {body}"
        );
        assert!(
            !body.contains("octo_wallet::abort_rotation("),
            "rotate_abort must NOT keep the free-fn path per AC-7 migration: {body}"
        );
    }

    /// tv_x_c_31 — `rotate` handler must migrate to UnlockedWallet via
    /// `store.unlock(...)` then `unlocked.begin_rotation(successor, ...)`
    /// per AC-7 (Phase 6.3). The earlier tv_x_c_12 + tv_x_c_16 vectors
    /// (deleted in R1.5 as vacuous) tested the old free-fn path; the
    /// mirror for the bare rotate handler was never added.
    #[test]
    fn tv_x_c_31_rotate_uses_unlocked_wallet_migration() {
        let src = include_str!("identity.rs");
        // Bounded by `revoke`, the next handler. The earlier bound was
        // `rotate_complete`, which sits AFTER `revoke`, so the slice
        // spanned two handlers and `revoke`'s own `.unlock(...)` call
        // satisfied this vector's assertion — deleting rotate's unlock
        // left it green.
        let body = fn_body(src, "pub fn rotate(", "pub fn revoke(");
        assert!(
            body.contains(".unlock(passphrase.as_str(), seed_buf.as_mut())"),
            "rotate must migrate to UnlockedWallet via store.unlock per AC-7: {body}"
        );
        assert!(
            body.contains(".begin_rotation(successor, passphrase.as_str(), now)"),
            "rotate must call UnlockedWallet::begin_rotation per AC-7: {body}"
        );
        assert!(
            !body.contains("octo_wallet::begin_rotation("),
            "rotate must NOT keep the free-fn path per AC-7 migration: {body}"
        );
    }

    /// tv_x_c_32 — `revoke` handler must migrate to UnlockedWallet via
    /// `store.unlock(...)` then `unlocked.revoke(now)` per AC-7
    /// (Phase 6.3). Mirror of tv_x_c_31 for the revoke call site; the
    /// earlier R1.5 deletion of tv_x_c_12/16 left this call site
    /// unmigrated-tested, and the gap was raised in R3 finding 4.
    #[test]
    fn tv_x_c_32_revoke_uses_unlocked_wallet_migration() {
        let src = include_str!("identity.rs");
        // Bounded by `register`, the next handler. The earlier bound was
        // `fn revoke_rejects_empty_reason`, a symbol in this test module
        // at ~1576 — past `mod tests`, so the slice ran from `revoke`
        // through the rest of production code and 290 lines of test
        // module.
        let body = fn_body(src, "pub fn revoke(", "pub fn register(");
        assert!(
            body.contains(".unlock(passphrase.as_str(), seed_buf.as_mut())"),
            "revoke must migrate to UnlockedWallet via store.unlock per AC-7: {body}"
        );
        assert!(
            body.contains("unlocked.revoke(now)"),
            "revoke must call UnlockedWallet::revoke per AC-7: {body}"
        );
        assert!(
            !body.contains("octo_wallet::revoke("),
            "revoke must NOT keep the free-fn path per AC-7 migration: {body}"
        );
    }

    // -----------------------------------------------------------------
    // R10 Lens-1: the five `if !cli.mode.dry_run` guards in the
    // mutating handlers had NO runtime vector.
    //
    // What the gap was. Every one of the five guards - register,
    // select, revoke, rotate-complete, rotate-abort - decided whether
    // to touch the substrate. The arm body opens the store, unlocks,
    // and mutates. Flipping any guard from `!cli.mode.dry_run` to
    // `true` therefore makes a PREVIEW perform the mutation: the
    // operator runs `--dry-run revoke`, sees "would revoke: ...",
    // and the active identity is gone. That is the worst shape a
    // review tool can have, because the output is the reassurance.
    //
    // Why it survived. The only tests that set `dry_run` exercised
    // `require_confirm` in isolation, which is a different function
    // and a different claim. A source-grep vector could not close it
    // either: `tv_x_c_39` pins the guard's TEXT, so a mutation that
    // keeps the line and changes only its condition reads identical
    // to the vector. The condition is the part that matters and the
    // text is not.
    //
    // What these five vectors assert, and how a flipped guard fails:
    //
    //   register / select  - the store is read back off disk and
    //     compared, so a flipped guard shows up as a changed record
    //     or a changed active pointer.
    //
    //   revoke / rotate-complete / rotate-abort - under a flipped
    //     guard the arm reaches `acquire_passphrase`, and under
    //     `cargo test` stdin is not a terminal, so it returns
    //     `WalletLocked` immediately rather than prompting. The
    //     handler therefore FAILS FAST instead of hanging, which is
    //     what makes these safe to run: a vector that could block
    //     the suite is a vector nobody keeps. The assertion is that
    //     the call returns `Ok`, which a flipped guard cannot do.
    //
    // The store-unchanged assertion is kept on all five anyway, so
    // the vectors say what the property IS and not merely that some
    // error stopped happening.

    /// Bring up a temp `OCTO_HOME` with one registered identity.
    /// Returns the two `TempDir` guards so the wallet root outlives
    /// the handler under test, plus the store root and active DID.
    /// The caller holds `OCTO_HOME_LOCK` and must clear the env var.
    fn dry_run_fixture(
        label: &str,
        seed_byte: u8,
    ) -> (
        tempfile::TempDir,
        tempfile::TempDir,
        std::path::PathBuf,
        String,
    ) {
        let home = tempfile::tempdir().expect("tempdir");
        let root = home.path().join("wallet");
        // SAFETY: the caller holds OCTO_HOME_LOCK, the only writer of
        // OCTO_HOME among this module's tests.
        unsafe { std::env::set_var("OCTO_HOME", home.path()) };

        let dir = tempfile::tempdir().expect("work dir");
        let seed_file = dir.path().join("seed.hex");
        std::fs::write(&seed_file, hex::encode([seed_byte; 32])).expect("write seed");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&seed_file, std::fs::Permissions::from_mode(0o600))
                .expect("chmod seed");
        }
        let pass_file = dir.path().join("pass.txt");
        std::fs::write(&pass_file, "correct-horse-battery-staple").expect("write passphrase");

        let mut cli = cli_with_mode(OperatorMode::Human);
        cli.mode.confirm = true;
        cli.mode.confirm_acknowledge = true;
        register(label, &pass_file, true, Some(&seed_file), &cli).expect("seed the store");

        let store = octo_wallet::WalletStore::open_at(&root).expect("reopen");
        let did = store.active_did().expect("active DID").0.clone();
        drop(store);
        (home, dir, root, did)
    }

    #[test]
    fn tv_x_c_66_a_dry_run_register_creates_no_identity() {
        let _guard = OCTO_HOME_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let home = tempfile::tempdir().expect("tempdir");
        let root = home.path().join("wallet");
        unsafe { std::env::set_var("OCTO_HOME", home.path()) };

        let dir = tempfile::tempdir().expect("work dir");
        let seed_file = dir.path().join("seed.hex");
        std::fs::write(&seed_file, hex::encode([9u8; 32])).expect("write seed");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&seed_file, std::fs::Permissions::from_mode(0o600))
                .expect("chmod seed");
        }
        let pass_file = dir.path().join("pass.txt");
        std::fs::write(&pass_file, "correct-horse-battery-staple").expect("write passphrase");

        let mut cli = cli_with_mode(OperatorMode::Human);
        cli.mode.confirm = true;
        cli.mode.confirm_acknowledge = true;
        cli.mode.dry_run = true;
        let result = register("tv-x-c-66", &pass_file, true, Some(&seed_file), &cli);
        unsafe { std::env::remove_var("OCTO_HOME") };
        result.expect("a dry-run register is a preview and must succeed");

        assert!(
            !root.join("store.json").exists(),
            "a dry-run register must not create store.json. It exists, so the guard was crossed \
             and an identity was minted by a command the operator asked only to preview"
        );
    }

    #[test]
    fn tv_x_c_67_a_dry_run_select_leaves_the_active_pointer_where_it_was() {
        let _guard = OCTO_HOME_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let home = tempfile::tempdir().expect("tempdir");
        let root = home.path().join("wallet");
        unsafe { std::env::set_var("OCTO_HOME", home.path()) };

        let dir = tempfile::tempdir().expect("work dir");
        let pass_file = dir.path().join("pass.txt");
        std::fs::write(&pass_file, "correct-horse-battery-staple").expect("write passphrase");
        let mut cli = cli_with_mode(OperatorMode::Human);
        cli.mode.confirm = true;
        cli.mode.confirm_acknowledge = true;

        // Two identities, so the pointer has somewhere to move TO.
        // The DIDs come from `list_records` rather than from
        // `active_did` after each registration: this handler does not
        // re-promote on a second register, so reading the pointer
        // here would have produced the same DID twice and the fixture
        // would not have been testing a move at all.
        for (i, byte) in [3u8, 4u8].into_iter().enumerate() {
            let seed_file = dir.path().join(format!("seed{i}.hex"));
            std::fs::write(&seed_file, hex::encode([byte; 32])).expect("write seed");
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&seed_file, std::fs::Permissions::from_mode(0o600))
                    .expect("chmod seed");
            }
            register("tv-x-c-67", &pass_file, true, Some(&seed_file), &cli).expect("register");
        }

        let store = octo_wallet::WalletStore::open_at(&root).expect("reopen");
        let dids: Vec<String> = store
            .list_records()
            .iter()
            .map(|r| r.did.0.clone())
            .collect();
        let before = store.active_did().expect("active DID").0.clone();
        assert_eq!(
            dids.len(),
            2,
            "the fixture must have two identities for the pointer to move between, got {dids:?}"
        );
        let target = dids
            .iter()
            .find(|d| **d != before)
            .expect("a second, non-active identity")
            .clone();
        assert_ne!(target, before, "the target must not already be active");

        // `Octo` is not `Clone`, and a second `cli_with_mode` is both
        // simpler and a better test: it proves the dry-run path needs
        // nothing from the mutating call that preceded it.
        let mut dry = cli_with_mode(OperatorMode::Human);
        dry.mode.confirm = true;
        dry.mode.confirm_acknowledge = true;
        dry.mode.dry_run = true;
        let result = select(&target, &dry);
        unsafe { std::env::remove_var("OCTO_HOME") };
        result.expect("a dry-run select is a preview and must succeed");

        let store = octo_wallet::WalletStore::open_at(&root).expect("reopen after dry-run");
        assert_eq!(
            store.active_did().expect("active DID").0,
            before,
            "a dry-run select must not move the active pointer. The pointer moved, so the guard \
             was crossed and the preview changed which identity the wallet signs with"
        );
    }

    #[test]
    fn tv_x_c_68_a_dry_run_revoke_leaves_the_identity_active() {
        let _guard = OCTO_HOME_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let (_home, _dir, root, did) = dry_run_fixture("tv-x-c-68", 5u8);

        let mut cli = cli_with_mode(OperatorMode::Human);
        cli.mode.confirm = true;
        cli.mode.confirm_acknowledge = true;
        cli.mode.dry_run = true;
        // A flipped guard reaches `acquire_passphrase`, which under
        // `cargo test` returns `WalletLocked` rather than prompting.
        let result = revoke("dry-run preview", true, &cli);
        unsafe { std::env::remove_var("OCTO_HOME") };
        result.expect(
            "a dry-run revoke is a preview and must succeed. An error here means the guard was \
             crossed and the arm ran to the passphrase prompt",
        );

        let store = octo_wallet::WalletStore::open_at(&root).expect("reopen");
        let record = store
            .identity_record(&octo_wallet::Did(did.clone()))
            .expect("record");
        assert_eq!(
            record.lifecycle,
            octo_wallet::LifecycleState::Active,
            "a dry-run revoke must not transition the identity. It is {0:?}, so a preview \
             destroyed the identity the operator still needs",
            record.lifecycle
        );
    }

    #[test]
    fn tv_x_c_69_a_dry_run_rotate_complete_leaves_the_rotation_in_flight() {
        let _guard = OCTO_HOME_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let (_home, _dir, root, did) = dry_run_fixture("tv-x-c-69", 6u8);

        {
            let mut store = octo_wallet::WalletStore::open_at(&root).expect("open to rotate");
            let mut seed_out = Vec::new();
            let mut unlocked = store
                .unlock("correct-horse-battery-staple", &mut seed_out)
                .expect("unlock");
            let successor = octo_wallet::IdentityKey::from_seed([1u8; 32]);
            unlocked
                .begin_rotation(successor, "correct-horse-battery-staple", 1_700_000_010)
                .expect("begin_rotation");
        }

        let mut cli = cli_with_mode(OperatorMode::Human);
        cli.mode.confirm = true;
        cli.mode.confirm_acknowledge = true;
        cli.mode.dry_run = true;
        let result = rotate_complete(true, &cli);
        unsafe { std::env::remove_var("OCTO_HOME") };
        result.expect(
            "a dry-run rotate-complete is a preview and must succeed. An error here means the \
             guard was crossed and the arm ran to the passphrase prompt",
        );

        let store = octo_wallet::WalletStore::open_at(&root).expect("reopen");
        let record = store
            .identity_record(&octo_wallet::Did(did.clone()))
            .expect("record");
        assert_eq!(
            record.lifecycle,
            octo_wallet::LifecycleState::Rotating,
            "a dry-run rotate-complete must leave the rotation in flight. It is {0:?}, so a \
             preview completed a rotation the operator had not committed to",
            record.lifecycle
        );
    }

    #[test]
    fn tv_x_c_70_a_dry_run_rotate_abort_leaves_the_rotation_in_flight() {
        let _guard = OCTO_HOME_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let (_home, _dir, root, did) = dry_run_fixture("tv-x-c-70", 8u8);

        {
            let mut store = octo_wallet::WalletStore::open_at(&root).expect("open to rotate");
            let mut seed_out = Vec::new();
            let mut unlocked = store
                .unlock("correct-horse-battery-staple", &mut seed_out)
                .expect("unlock");
            let successor = octo_wallet::IdentityKey::from_seed([1u8; 32]);
            unlocked
                .begin_rotation(successor, "correct-horse-battery-staple", 1_700_000_010)
                .expect("begin_rotation");
        }

        let mut cli = cli_with_mode(OperatorMode::Human);
        cli.mode.confirm = true;
        cli.mode.confirm_acknowledge = true;
        cli.mode.dry_run = true;
        let result = rotate_abort(Some("dry-run preview"), true, &cli);
        unsafe { std::env::remove_var("OCTO_HOME") };
        result.expect(
            "a dry-run rotate-abort is a preview and must succeed. An error here means the \
             guard was crossed and the arm ran to the passphrase prompt",
        );

        let store = octo_wallet::WalletStore::open_at(&root).expect("reopen");
        let record = store
            .identity_record(&octo_wallet::Did(did.clone()))
            .expect("record");
        assert_eq!(
            record.lifecycle,
            octo_wallet::LifecycleState::Rotating,
            "a dry-run rotate-abort must leave the rotation in flight. It is {0:?}, so a \
             preview discarded a rotation the operator had not committed to abandoning",
            record.lifecycle
        );
    }

    // -----------------------------------------------------------------
    // R10 Lens-1: two mutating envelopes carried a real timestamp
    // for an event the preview did not perform.
    //
    // `revoke` read the clock ABOVE its dry-run guard and formatted
    // the result below it, so a preview shipped `revoked_at` set to
    // a live RFC 3339 instant. `rotate` computed `grace_expires_at`
    // unconditionally, so a preview shipped a deadline for a
    // rotation that had not started. Both are the fabrication the
    // register and rotate-complete paths had already been repaired
    // for, and the repair reached two of the four mutating envelopes
    // rather than all four.
    //
    // These are SOURCE vectors, and the choice is deliberate. The
    // defect is POSITIONAL - a statement sitting outside a guard -
    // and the handler's envelope goes to stdout, which a unit test
    // cannot capture. A runtime vector would have to re-derive the
    // envelope by hand, and a vector that builds its own copy of the
    // thing it is testing is the R9 lesson about `tv_x_c_1` all over
    // again. The runtime half of this property is already covered by
    // `tv_x_c_68` through `tv_x_c_70`, which prove the STORE is
    // untouched. What is left is the claim that the handler does not
    // COMPUTE a time it must then not report, and that is a claim
    // about the source.
    //
    // Each assertion below is affirmative and each is falsifiable by
    // a mutation that changes text, which is the bar a text-shaped
    // vector has to clear to be worth having.

    #[test]
    fn tv_x_c_71_revoke_reads_its_clock_inside_the_dry_run_guard() {
        let src = production_src();
        let body = fn_body_code(src, "pub fn revoke(", "pub fn register(");

        let guard = body
            .find("if !cli.mode.dry_run {")
            .expect("revoke must still gate its substrate work on dry_run");
        let clock = body
            .find("chrono::Utc::now()")
            .expect("revoke must read the clock somewhere");
        assert!(
            clock > guard,
            "revoke must read the clock INSIDE the dry-run guard. The clock read sits at offset \
             {clock} and the guard opens at {guard}, so the read happens on the preview path and \
             the envelope carries a real instant for a revocation that did not occur. Body: {body}"
        );

        // The reporting site must be an ASSIGNMENT into the binding
        // declared before the guard, not a fresh binding computed
        // from a `now` that outlives the guard.
        assert!(
            body.contains("let mut revoked_at = None;"),
            "revoke must declare revoked_at as None and fill it inside the guard, so a preview \
             has no time to report: {body}"
        );
        assert!(
            body.contains("revoked_at = unix_to_rfc3339(now as i64);"),
            "the revocation instant must be assigned inside the guard, from the clock read that \
             is also inside it: {body}"
        );
    }

    #[test]
    fn tv_x_c_72_rotate_reports_no_grace_window_it_never_started() {
        let src = production_src();
        let body = fn_body_code(src, "pub fn rotate(", "pub fn revoke(");

        let start = body
            .find("let grace_expires_at")
            .expect("rotate must compute grace_expires_at");
        let end = body
            .find("let output = IdentityRotateOutput")
            .expect("rotate must build its output after the grace window");
        let window = &body[start..end];

        assert!(
            window.contains("if cli.mode.dry_run"),
            "the grace window must be branch-dry-run. It is computed on one path, so a preview \
             ships a deadline for a rotation that never began. Window: {window}"
        );
        assert!(
            window.contains("None"),
            "the dry-run arm of the grace window must be None, not a formatted instant: {window}"
        );
        assert!(
            window.contains("ROTATION_GRACE_PERIOD_SECS"),
            "the live arm must still derive the window from the substrate constant rather than a \
             local literal: {window}"
        );
    }

    // -----------------------------------------------------------------
    // R10 Lens-1: the two `<not-read: dry-run>` corrections and the
    // `i64::MIN` registered-at sentinel could each be deleted with the
    // suite green, which would put an empty `lifecycle_state` and a
    // 1970-01-01 `registered_at` back into preview envelopes.
    //
    // Source assertions again, for the reason `tv_x_c_71` gives: the
    // envelope is rendered to stdout and the arm is not separately
    // callable. What is added here on top of the call-site assertions
    // is a RUNTIME half on the conversion, because the reason the
    // sentinel is `i64::MIN` and not `0` is a fact about the
    // conversion and can be checked directly. Between the two, a
    // consumer's question - "what does a preview actually emit?" - is
    // answered from both ends.

    #[test]
    fn tv_x_c_73_a_preview_says_a_value_was_unread_rather_than_emitting_a_placeholder() {
        let src = production_src();

        for (handler, next, label) in [
            ("pub fn register(", "pub fn select(", "register"),
            ("pub fn select(", "pub fn list(", "select"),
        ] {
            let body = fn_body_code(src, handler, next);
            assert!(
                body.contains("if cli.mode.dry_run {"),
                "{label} must branch on dry_run before shaping the envelope: {body}"
            );
            assert!(
                body.contains("lifecycle_state = \"<not-read: dry-run>\".to_string();"),
                "{label} must overwrite lifecycle_state with the unread sentinel under dry_run. \
                 The empty string is not a member of the documented Designated / Active / Rotating \
                 / Revoked set, so dropping this override puts an out-of-set value in a preview \
                 envelope: {body}"
            );
        }
    }

    #[test]
    fn tv_x_c_74_the_registered_at_sentinel_is_unformattable_and_zero_is_not() {
        let src = production_src();
        let body = fn_body_code(src, "pub fn register(", "pub fn select(");

        // The dry-run arm of the fact tuple, anchored on the tuple
        // itself rather than on a brace. A brace search finds the
        // first inner `else` - there are several in this arm - and a
        // window computed from the wrong one asserts about a seed-file
        // length check. The tuple is the thing being claimed.
        let guard = body
            .find("let register_facts = if !cli.mode.dry_run {")
            .expect("register must gate its substrate work on dry_run");
        let tuple_at = body
            .find("(String::new(), String::new(), String::new(), false, i64::MIN)")
            .unwrap_or_else(|| {
                panic!(
                    "the dry-run arm must supply the full unread tuple ending in the i64::MIN \
                     sentinel, so the envelope carries null rather than a time. Body: {body}"
                )
            });
        assert!(
            tuple_at > guard,
            "the dry-run tuple must belong to the gated arm, not precede it: {body}"
        );

        // Why the sentinel and not zero, measured rather than
        // asserted. This is the half a source vector cannot give: the
        // claim that `0` would have been a lie is a claim about
        // `from_timestamp`, and it is checkable in one line.
        assert!(
            unix_to_rfc3339(i64::MIN).is_none(),
            "the sentinel must be unformattable, so a preview emits null. It formatted, which \
             means the sentinel is a real instant and the epoch lie is back"
        );
        let zero = unix_to_rfc3339(0).expect("zero is a real Unix second");
        assert_eq!(
            zero.timestamp(),
            0,
            "zero formats to 1970-01-01, a VALID RFC 3339 timestamp indistinguishable from an \
             identity genuinely registered at the epoch. This is what the sentinel exists to \
             avoid, and it is why the sentinel is i64::MIN and not 0"
        );
    }
}
