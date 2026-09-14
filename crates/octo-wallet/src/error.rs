//! Wallet error type.

use thiserror::Error;
use uuid::Uuid;

use crate::agent::AgentState;
use crate::hsm::HsmError;
use crate::lifecycle::LifecycleState;

/// Top-level error for `octo-wallet`.
///
/// `#[non_exhaustive]` per [[cipherocto-design-principles]]
/// §Extension over enumeration: future variants land additively
/// without breaking downstream matchers (RFC-0015 §6.2.7 substrate
/// additions row 1).
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum WalletError {
    #[error("OS RNG failure: {0}")]
    OsRng(String),

    #[error("invalid audience ID: {0}")]
    InvalidAudienceId(String),

    #[error("invalid channel ID: {0}")]
    InvalidChannelId(String),

    #[error("HKDF expand failed: {0}")]
    HkdfExpand(String),

    #[error("signature verification failed: {0}")]
    Signature(String),

    #[error("HSM error: {0}")]
    Hsm(#[from] HsmError),

    #[error("vault slot not found: {0}")]
    VaultSlotNotFound(String),

    #[error("vault decryption failed (wrong passphrase or corrupted slot)")]
    VaultDecryptionFailed,

    #[error("vault KDF timed out")]
    VaultKdfTimeout,

    #[error("invalid slot ID `{0}` (must match [a-zA-Z0-9._-]+ and length 1..=128)")]
    InvalidSlotId(String),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("keystore parse error: {0}")]
    KeystoreParse(String),

    #[error("keystore version mismatch: expected {expected}, got {got}")]
    KeystoreVersion { expected: String, got: String },

    #[error("config error: {0}")]
    Config(String),

    // ----- Identity lifecycle errors (RFC-0009 §Lifecycle Requirements) -----
    /// `sign()` called when lifecycle state is not `Active` or `Rotating`
    /// (i.e. `Designated` or `Revoked`).
    #[error("identity not active (current state: {current_state:?})")]
    NotActive { current_state: LifecycleState },

    /// `activate()` called on a `Revoked` identity (terminal state).
    #[error("identity already revoked; cannot activate")]
    AlreadyRevoked,

    /// `activate()` called while identity is in the `Rotating` state.
    /// Caller must `complete_rotation()` or `abort_rotation()` first
    /// (rotation transitions: `Active ↔ Rotating` are owned by
    /// `IdentityKey::begin_rotation` / `complete_rotation` /
    /// `abort_rotation`).
    #[error("identity rotation in progress; complete or abort rotation first")]
    RotationInProgress,

    // ----- Rotation errors (RFC-0009 §Lifecycle + RFC-0853 §12) -----
    /// `complete_rotation()` or `abort_rotation()` called when lifecycle
    /// state is not `Rotating`.
    #[error("identity not rotating (current state: {current_state:?})")]
    NotRotating { current_state: LifecycleState },

    /// `begin_rotation()` invoked with `successor.public_key_bytes() ==
    /// self.public_key_bytes()` (cannot rotate to self).
    #[error("cannot rotate identity to itself (successor pubkey matches current)")]
    SelfRotation,

    /// `complete_rotation()` called before the 24-hour grace period elapsed
    /// (RFC-0853 §12).
    #[error(
        "rotation grace period not elapsed (elapsed: {elapsed_secs}s, required: {required_secs}s)"
    )]
    GracePeriodNotElapsed {
        elapsed_secs: u64,
        required_secs: u64,
    },

    /// `verify_successor_proof()` rejected the proof signature.
    #[error("invalid successor proof (signature verification failed)")]
    InvalidSuccessorProof,

    /// `verify_revocation_proof()` rejected the proof signature.
    /// Either the public key is not a valid Ed25519 point, the signature
    /// is malformed, or the signature does not verify against `b"revoke"`.
    #[error("invalid revocation proof (signature verification failed)")]
    InvalidRevocationProof,

    // ----- Nonce counter errors (RFC-0011-d §7.4) -----
    /// `next_nonce_counter` exhausted `u64::MAX` for the given operator
    /// DID. Substrate path: `octo_role::select_with_chain_id` wraps this as
    /// `RoleError::SigningFailed { reason: format!("nonce counter: {e}") }`.
    /// Exit code = 11 per `OctoCliError::SigningFailed` mapping.
    #[error("role-binding nonce counter exhausted (u64 saturated) for did")]
    NonceUnderflow,

    // ----- Agent manifest + capability-validation errors (RFC-0011-c §9.10) -----
    /// `AgentManifest::from_json` failed to deserialize the operator-supplied
    /// manifest file. `path` is the label the caller passed (CLI passes the
    /// `--manifest-path` value so operator errors surface the file they
    /// specified); `reason` is the underlying `serde_json` error.
    /// Exit code = 39 per RFC-0011-c §9.8.
    #[error("agent manifest parse error at `{path}`: {reason}")]
    ManifestParse { path: String, reason: String },

    /// The 6-step capability validation pipeline (RFC-0002 §Capability
    /// Validation) failed at the given 1-based step. The substrate signals
    /// each step's specific failure mode; the CLI surfaces the step number
    /// to the operator. Phase 1 only emits this variant when the macaroon
    /// substrate rejects a manifest — the wiring lands with the
    /// `octo-cap-macaroon` 6-step integration (out of scope for
    /// `0011-c-agent-create-subcommand`). Exit code = 40 per
    /// RFC-0011-c §9.8.
    #[error("capability validation failed at step {0} (RFC-0002 §Capability Validation)")]
    CapabilityValidationFailed(usize),

    /// `register_agent` was called with `(manifest, active_did)` whose
    /// derived `agent_id` is already present in the registry (RFC-0011-c
    /// §9.10 — `agent_id` is a deterministic function of the manifest
    /// digest + holder DID; duplicates emit `AgentAlreadyExists`). Exit
    /// code = 41 per RFC-0011-c §9.8.
    #[error("agent already registered: {0}")]
    AgentAlreadyExists(Uuid),

    // ----- Agent read-path errors (RFC-0015 §6.2.3 / §6.2.4 / §6.2.5) -----
    /// `lookup_agent` miss — agent UUID not found in the caller-attested
    /// DID's registry. CLI exit code = 42 per RFC-0011-c §9.8 (mirrors
    /// `OctoCliError::AgentAlreadyExists` exit-41 pattern but for the
    /// not-found side). Payload `Uuid` is the operator-supplied UUID
    /// for log redaction (CLI surfaces the typed variant; the byte
    /// payload is sanitized via `RedactedIdentifier` per
    /// `0011-c-agent-redaction-envelope`).
    #[error("agent not found: {0}")]
    AgentNotFound(Uuid),

    /// Caller-attested DID does not match the filter's `holder_did`
    /// field. SECURITY HIGH (multi-DID enumeration prevention per
    /// RFC-0015 §6.2.1). CLI exit code = 17 per RFC-0011 §Exit Codes
    /// 17-63 reserved range (RFC-0015 §6.2.4). Unit variant; the
    /// substrate-faithful byte-for-byte mirror in
    /// `OctoCliError::ForbiddenHolderMismatch` carries no payload.
    #[error("forbidden: holder DID mismatch")]
    ForbiddenHolderMismatch,

    /// `validate_reason` found a control character in
    /// `U+0000`-`U+001F` or `U+007F`. Payload is the offending
    /// character as a `String` in hex-escaped code-point notation
    /// (e.g., `<U+001B>` for ESC, `<U+0000>` for NUL), NEVER the raw
    /// byte — `Display` impl MUST NOT echo attacker bytes back to the
    /// terminal (pager-hijack mitigation per RFC-0015 §6.2.5).
    /// CLI exit code = 16 per `OctoCliError::InvalidFilter` mapping.
    #[error("reason contains control character: {0}")]
    ReasonContainsControlChars(String),

    /// `validate_reason` cap exceeded (input length over 256 bytes per
    /// RFC-0015 §6.2.5). Payload is the offending byte length.
    /// CLI exit code = 16 per `OctoCliError::InvalidFilter` mapping.
    #[error("reason exceeds 256 bytes (got {0})")]
    ReasonTooLong(usize),

    // ----- Agent write-path errors (RFC-0015-a §6.3 paired-acceptance bridge) -----
    /// `transition_agent` observed the in-flight `transitioning` flag
    /// on the `AgentRecord` is `true` inside the canonical GLOBAL
    /// `std::sync::Mutex` lock (RFC-0015-b §X.1). Payload carries the
    /// agent UUID the caller attempted to transition; the substrate
    /// holds the flag until the in-flight transition completes (or
    /// the RAII guard resets it on early-return). CLI exit code = 43
    /// per RFC-0011-c §9.8 + RFC-0015-a §6.3 slot allocation.
    #[error("agent already in transition: {0}")]
    AlreadyInTransition(Uuid),

    /// `transition_agent` rejected the requested transition because
    /// the state-machine guard (RFC-0015-a Appendix A) does not
    /// permit `from → to`. Payload carries the typed `AgentState`
    /// pair so the CLI surfaces the typed variant and the substrate
    /// retains the substrate-faithful enum form. Self-transition
    /// (`from == to`) returns `Ok(())` (idempotent success, no audit
    /// append) instead of this error. Terminal `Terminated` state
    /// always raises this error (terminal-by-construction per
    /// RFC-0015-a §6.1). CLI exit code = 43 per RFC-0015-a §6.3.
    #[error("invalid state transition: {from:?} -> {to:?}")]
    InvalidStateTransition {
        /// Current (rejected-source) state.
        from: AgentState,
        /// Requested (rejected-target) state.
        to: AgentState,
    },

    /// `transition_agent` rolled back the state transition because
    /// the audit append failed (RFC-0015-a §6.1 rollback contract).
    /// Payload carries the underlying `AuditError::SinkSpecific`
    /// reason string for log forensics; the CLI surfaces the typed
    /// variant and sanitizes the reason via `scrub_adapter_error`
    /// (RFC-0012-v3 §S5.1 per-façade scrubber contract). The agent
    /// record is NOT modified in this case (state rollback is
    /// synchronous before the function returns). CLI exit code = 52
    /// per RFC-0015-a §6.3 + RFC-0016-a §6.7 slot allocation.
    #[error("audit substrate unavailable: {0}")]
    AuditUnavailable(String),
}
