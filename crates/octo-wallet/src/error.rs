//! Wallet error type.

use thiserror::Error;
use uuid::Uuid;

use crate::agent::AgentState;
use crate::hsm::HsmError;
use crate::identity_record::Did;
use crate::lifecycle::LifecycleState;

/// Minimum passphrase length enforced at both `WalletStore::register` and
/// `WalletStore::unlock` (mission 0011-x-s-a-wallet-store-identity §AC-28).
/// Re-exported from `identity_store` per AC-28 ("the floor is
/// `pub const MIN_PASSPHRASE_CHARS: usize = 12` declared in
/// `identity_store.rs`"). The `WeakPassphrase` `#[error]` message below
/// interpolates this constant, so the sentence an operator reads cannot
/// drift from the threshold the check compares against.
pub use crate::identity_store::MIN_PASSPHRASE_CHARS;

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

    /// `store.json` is internally inconsistent: the index says
    /// something the file cannot support, so the store refuses to
    /// open rather than resolve it one way for reads and another for
    /// writes.
    ///
    /// This is a TYPED variant rather than more `Config(String)`
    /// because the two are not the same kind of fault and an operator
    /// can act on only one of them. `Config` from `WalletStore::open`
    /// means the environment could not be resolved, and the remedy is
    /// to set `$OCTO_HOME` or `$HOME`. `IndexCorrupt` means the
    /// environment was fine and the file is damaged, and the remedy is
    /// to restore or repair the index. The CLI maps the first to exit
    /// 27 and the second to exit 64, because telling an operator whose
    /// `OCTO_HOME` is already set to set it is worse than useless: it
    /// sends them away from the file that is actually broken.
    ///
    /// A separate variant is also what stops the failure being
    /// absorbed: a `Config(_)` catch-all cannot distinguish the two,
    /// so a caller that wanted to handle the corruption specifically
    /// would have to match on message text.
    #[error("wallet index is inconsistent: {detail}")]
    IndexCorrupt { detail: String },

    // ----- Identity lifecycle errors (RFC-0009 §Lifecycle Requirements) -----
    /// `sign()` called when lifecycle state is not `Active` or `Rotating`
    /// (i.e. `Designated` or `Revoked`).
    #[error("identity not active (current state: {current_state:?})")]
    NotActive { current_state: LifecycleState },

    /// `activate()` called on a `Revoked` identity (terminal state).
    /// Two distinct conditions share this variant: `activate()` on a
    /// record already in the `Revoked` lifecycle (terminal), and
    /// `register` whose DID is already in the index. The message
    /// names both. It previously said only "already revoked; cannot
    /// activate", which is the second condition's message applied to
    /// the first - so a re-registration told the operator their
    /// identity had been revoked, a fact the substrate does not know
    /// and cannot assert. `OctoCliError::AlreadyRevoked` was widened
    /// for this reason and the two layers disagreed.
    #[error("identity already revoked or already registered")]
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

    /// `unlock()` rehydrating a record in the `Rotating` lifecycle
    /// whose `rotation_history` is empty, so the rotation's start
    /// time and successor are unrecoverable.
    ///
    /// `complete_rotation` reads the start time through an
    /// `.expect(...)`, so completing such a rotation would panic -
    /// exit 101, no envelope. This is raised BY `unlock`, which
    /// every key-taking path routes through, so the previous
    /// remediation ("abort the in-flight rotation") named
    /// `rotate-abort` - a command that acquires its handle from
    /// this same `unlock` and therefore exits with this same error.
    /// The advice was unreachable. The real repair is off the CLI:
    /// edit `store.json` to set the record's lifecycle back to
    /// `Active`, or restore the rotation event that `unlock` is
    /// looking for.
    #[error(
        "rotation start state is missing from the wallet record; this cannot be repaired \
         from the CLI, because every key-taking command acquires its handle through the same \
         check. Edit store.json: set the record's lifecycle back to Active, or restore the \
         rotation event the record should carry"
    )]
    RotationEventMissing,

    /// The successor key rehydrated from a vault slot does not
    /// derive the DID the rotation event names, so the index's
    /// `pubkey_bytes` and its `did` disagree.
    ///
    /// Completing the rotation would promote the named DID to active
    /// while every signature it makes is under a different key. `Did`
    /// is derived from the public key, so the mismatch is exact and
    /// detectable rather than a judgement call.
    #[error("successor key does not derive the DID recorded for it ({did})")]
    SuccessorKeyMismatch { did: Did },

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
    #[error("role-binding nonce counter exhausted (u64 saturated)")]
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

    // ----- WalletStore errors (mission 0011-x-s-a-wallet-store-identity §AC-6) -----
    /// The store is locked. `WalletStore::open` is metadata-only; the
    /// identity seed requires `WalletStore::unlock(passphrase)`. CLI
    /// exit code = 92 per `OctoCliError::WalletLocked` mapping at
    /// slot 92.
    #[error("wallet store is locked; unlock with a passphrase to access the identity key")]
    Locked,

    /// No record for this DID in the store index. Mints **no
    /// `OctoCliError` slot** — it maps to the existing
    /// `OctoCliError::IdentityNotFound(String)` at exit 4, so the
    /// CLI's slot table is unchanged for this variant (mission
    /// 0011-x-s-a-wallet-store-identity §AC-7). The first
    /// `Did`-typed payload in `WalletError`.
    #[error("no identity record for {0}")]
    IdentityNotFound(Did),

    /// A supplied passphrase is below the enforced floor. Carries no
    /// detail of the passphrase itself, and none of the store's
    /// contents. The `Display` message interpolates
    /// `MIN_PASSPHRASE_CHARS` so the sentence an operator reads
    /// cannot drift from the threshold the check compares against
    /// (mission 0011-x-s-a-wallet-store-identity §AC-28). CLI exit
    /// code = 2 per `OctoCliError::WeakPassphrase` mapping at slot 94.
    #[error("passphrase is below the {MIN_PASSPHRASE_CHARS}-character floor")]
    WeakPassphrase,
}
