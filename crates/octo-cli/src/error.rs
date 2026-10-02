//! Operator-facing error envelope — RFC-0011 §Error Handling.

use std::io::Write;
use thiserror::Error;

// Substrate floor constant re-exported from `octo_wallet::error` via
// `pub use crate::identity_store::MIN_PASSPHRASE_CHARS`. Brought
// into scope so the `thiserror` `#[error]` template below can
// reference it without a path qualifier (thiserror format strings
// only support identifier captures, not `crate::Const` paths).
use octo_wallet::error::MIN_PASSPHRASE_CHARS;

/// Categorical band of known keys in the verifier's `KeySet`
/// per RFC-0011-h §Redaction Layer (3 bands; avoids leaking
/// exact verifier `KeySet` cardinality).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KnownKeysBand {
    /// 0 keys in `KeySet` (verifier misconfiguration)
    None,
    /// 1-8 keys in `KeySet` (typical deployment)
    Few,
    /// >8 keys in `KeySet` (large deployment or
    /// > post-rotation grace period)
    Many,
}

impl KnownKeysBand {
    /// Map raw `KeySet` cardinality to the categorical
    /// 3-band quantization per RFC-0011-h §Redaction Layer.
    /// Bucket thresholds (0 / 1-8 / >8) are implementation
    /// detail and are NOT echoed in operator-facing render;
    /// only the enum tag is interpolated by the
    /// `#[error(... {known_keys_band:?})]` Debug formatter.
    /// Feature-gated to mirror the `AttachError::UnknownKeyId`
    /// translation arm: only callable when the
    /// `octo-attach-key-rotation` Cargo feature is enabled.
    #[cfg(feature = "octo-attach-key-rotation")]
    pub(crate) fn from_count(n: usize) -> Self {
        match n {
            0 => Self::None,
            1..=8 => Self::Few,
            _ => Self::Many,
        }
    }
}

/// Every operator-visible failure mode of the `octo` CLI.
///
/// `#[non_exhaustive]` per F-14 + Wave 4.5 finding 10 — additive growth
/// must remain non-semver-breaking. Downstream `match` sites use
/// wildcard patterns (verified via the `render`/`hint`/`exit_code`
/// methods, which cover every current variant).
#[derive(Error, Debug)]
#[non_exhaustive]
pub enum OctoCliError {
    /// Argument parsing failed.
    #[error("{0}")]
    ClapParse(#[from] clap::Error),
    /// No active identity in the wallet.
    #[error("no active identity")]
    NoActiveIdentity,
    /// A mutating command was invoked without confirmation flags.
    ///
    /// Fires for any operator mode (Human / Ci / Dev) when the
    /// confirmation gate is unmet.
    ///
    /// The flag that satisfies the gate is MODE-DEPENDENT: Human mode
    /// requires `--confirm --confirm-acknowledge`, while Ci and Dev
    /// require `--allow-write`. This variant carries only `command`,
    /// so the message and the hint must both name every mode's flag
    /// rather than assuming Human.
    ///
    /// The previous doc comment here claimed "the operator sees the
    /// per-mode help text from `OctoCliError::render`". `render`
    /// has no mode logic at all — it prints `Display` and `hint()`
    /// verbatim — so that claim was false, and the consequence was
    /// observable: an operator in `--mode ci` who read the hint,
    /// added `--confirm`, and retried got byte-identical output,
    /// because `--confirm` does not satisfy the Ci gate. The hint
    /// pointed at a flag that could never work.
    #[error("ConfirmationRequired: mutating command {command} needs its mode's confirmation flag")]
    ConfirmationRequired {
        /// Command that required confirmation.
        command: String,
    },
    /// A command was invoked under `--mode auditor`.
    ///
    /// Auditor is a read-only role and is denied **before** the
    /// confirmation gate fires. Wave 3 LOW: a separate variant lets
    /// the operator see "auditor mode is read-only" rather than the
    /// generic "--confirm required" (which would be misleading —
    /// adding `--confirm` does not unblock an Auditor session).
    ///
    /// The variant covers BOTH mutating commands and the read-only
    /// ones the role is deliberately denied. `whoami`, `show` and
    /// `list` mutate nothing, so the previous wording — "refusing
    /// mutating command" — asserted a mutation that never happened
    /// and told an auditor to leave the role "to perform mutations"
    /// for a command that performs none. The message is written to
    /// be true of either case.
    #[error("auditor mode is read-only; refusing command {command}")]
    AuditorDenied {
        /// Command the auditor attempted to invoke.
        command: String,
    },
    /// A rotation is already in flight.
    #[error("identity rotation already in progress")]
    AlreadyRotating,
    /// Requested identity is unknown.
    #[error("identity not found: {0}")]
    IdentityNotFound(String),
    /// HSM backend unavailable.
    #[error("HSM unavailable: {0}")]
    HsmUnavailable(String),
    /// Identity already revoked, or already present in the wallet.
    ///
    /// Two distinct substrate conditions share this slot. A record
    /// in the `Revoked` lifecycle, and a `register` whose DID is
    /// already in the index. The message covers both because the
    /// variant carries no payload to tell them apart, and asserting
    /// only the first told the operator a duplicate registration was
    /// a revocation.
    #[error("identity already revoked or already registered")]
    AlreadyRevoked,
    /// Caveat expression failed to parse.
    #[error("caveat parse error: {message}")]
    CaveatParse {
        /// Parser diagnostic.
        message: String,
    },
    /// Caveats parsed but combine illegally.
    #[error("invalid caveat combination: {detail}")]
    InvalidCaveatCombination {
        /// Why the combination is invalid.
        detail: String,
    },
    /// Requested holder is unknown.
    #[error("holder not found: {0}")]
    HolderNotFound(String),
    /// Attenuation would widen authority.
    #[error("attenuation violation: {0}")]
    AttenuationViolation(String),
    /// Signing operation failed.
    #[error("signing failed: {0}")]
    SigningFailed(String),
    /// Parent capability is unknown.
    #[error("parent capability not found: {0}")]
    ParentCapNotFound(String),
    /// Requested policy is unknown.
    #[error("policy not found: {0}")]
    PolicyNotFound(String),
    /// Requested policy version is unknown.
    #[error("policy `{policy}` has no version {version}")]
    PolicyVersionNotFound {
        /// Policy name.
        policy: String,
        /// Requested version.
        version: u32,
    },
    /// Requested role is unknown (RFC-0011-d §Error Handling).
    #[error("role not found: {0}")]
    RoleNotFound(String),
    /// Operator OCTO stake is below the role's minimum (RFC-0011-d).
    #[error("stake insufficient: required {required} micro-OCTO, available {available}")]
    StakeInsufficient {
        /// Required stake (micro-OCTO).
        required: u64,
        /// Available stake (micro-OCTO).
        available: u64,
    },
    /// Role exists but cannot be selected (RFC-0011-d).
    #[error("role `{role_id}` not selectable: {reason}")]
    RoleNotSelectable {
        /// Role slug.
        role_id: String,
        /// Why the role cannot be selected.
        reason: String,
    },
    /// Active signer DID does not match the operator DID (F-16).
    #[error("signer mismatch: signer did `{signer_did}` != operator did `{operator_did}`")]
    SignerMismatch {
        /// Signer DID derived from the active public key.
        signer_did: String,
        /// Operator DID supplied on the command line.
        operator_did: String,
    },
    /// RFC-0855p-c §5a group binding ceremony rejected the
    /// domain-coordinator binding (stale `PlatformAdminProof`, invalid
    /// state transition, signature mismatch). Maps from
    /// `octo_role::RoleError::GroupBindingRejected`. Recoverable —
    /// re-fetch the proof and retry.
    #[error("group binding rejected: {reason}")]
    GroupBindingRejected {
        /// Substrate reason string (RFC-0855p-c `BindingError`).
        reason: String,
    },
    /// Secret was offered on stdin without `--allow-stdin-secret`.
    #[error("secret material on pipe; pass --allow-stdin-secret to override")]
    StdinSecretRefused,
    /// Filter expression is malformed.
    #[error("invalid filter: {0}")]
    InvalidFilter(String),
    /// A deprecated stub was invoked (post-v2.0 cut; soft sentinel —
    /// see RFC-0011 §Changelog v2.0 entry).
    ///
    /// Carries `replaced_by` so operator switch tables and JSON
    /// envelope parsers can grep the outbound payload for a
    /// human-readable replacement hint. Non-stale code paths
    /// (e.g. clap `unrecognized subcommand`) supersede this path
    /// for operators who invoke a removed stub fresh today; this
    /// variant preserves the v1.1 `StaleStub` exit-65 contract
    /// for any operator that observed the hard-error during the
    /// v1.1 cycle, and for any library caller that surfaces the
    /// variant directly. Retained per
    /// [[cipherocto-design-principles]] §Extension over enumeration:
    /// `#[non_exhaustive]` library surface must not lose variants.
    #[error("`{name}` was removed; use `{replaced_by}` (see `octo --help` for the current subcommand list)")]
    StaleStub {
        /// Stub command name.
        name: String,
        /// Human-readable replacement hint (e.g. `octo-wallet init`
        /// for `octo init`).
        replaced_by: &'static str,
    },
    /// Reputation aggregate for the given `(did, role)` was not found
    /// (RFC-0011-b §Substrate `[ADD]` map: `ReputationError::AggregateEmpty`).
    #[error("reputation not found for did `{did}` role `{role}`")]
    ReputationNotFound {
        /// Subject DID.
        did: String,
        /// Role slug.
        role: String,
    },
    /// Reputation subject DID is in the `Revoked` lifecycle state
    /// (RFC-0968 §Roles and Authorities). Auditor mode fails closed
    /// regardless of caller mode (RFC-0011-b §Security Considerations 3).
    #[error("reputation revoked for did `{did}`")]
    ReputationRevoked {
        /// Subject DID.
        did: String,
    },
    /// Anchor chain digest mismatch (RFC-0011-b §Substrate `[ADD]`
    /// map: `ReputationError::AnchorDigestMismatch`). Surfaces the
    /// last anchored unix timestamp for diagnostic context.
    #[error("anchor chain broken for did `{did}` (last anchor at unix {last_anchor_unix})")]
    AnchorChainBroken {
        /// Subject DID.
        did: String,
        /// Unix seconds of the last accepted anchor.
        last_anchor_unix: i64,
    },
    /// `--no-anchor-verify` was passed outside Dev mode
    /// (RFC-0011-b §Security Considerations 1a; DEV-ONLY escape hatch).
    #[error(
        "--no-anchor-verify is DEV-only (got mode `{mode}`); switch to --mode dev or drop the flag"
    )]
    NoAnchorVerifyInMode {
        /// Resolved operator mode label (lowercase).
        mode: &'static str,
    },
    /// Role slug failed the substrate `Role::parse` guard
    /// (RFC-0011-b §7.4 `Role::parse` rejects empty / whitespace).
    #[error("invalid role slug `{slug}`: {reason}")]
    InvalidRoleSlug {
        /// The supplied role slug (sanitized).
        slug: String,
        /// Why the slug was rejected.
        reason: String,
    },
    /// TTL hop count is out of the `1..=8` range allowed by RFC-0871.
    /// Surfaces from `octo mesh forward --ttl-hops`. Substrate clamps to
    /// the per-node-type TTL ceiling from `RouterAnnouncePayload`; the
    /// CLI enforces the wider 1..=8 operator-facing bound at dispatch
    /// (RFC-0011-f §Error Handling). Exit 17 per the amendment-chain
    /// slot allocation reserved by RFC-0011-f §Exit Codes.
    #[error("invalid TTL hops: {hops} (must be in 1..=8 per RFC-0871 ceiling)")]
    InvalidTtlHops {
        /// The offending hop count.
        hops: u8,
    },
    /// Mesh capability is missing or insufficient for the requested
    /// dispatch. `forward` and `rpc` require an RFC-0957 capability
    /// caveat per RFC-0011-f §Design Goals; substrate-truth verification at the
    /// dispatch boundary surfaces this when the envelope carries only a
    /// signature authorization (no capability bound) or the bound
    /// capability's `Audience` caveat does not match the resolved peer
    /// DID (RFC-0957 §Attenuation Invariant). Exit 18.
    #[error("mesh capability missing or insufficient: {detail}")]
    MeshCapabilityInsufficient {
        /// Operator-safe diagnostic.
        detail: String,
    },
    /// Envelope authorization verification failed at the dispatch
    /// boundary per RFC-0871 §Algorithms "Envelope receive (node-side)"
    /// step 6 (signature verify against the current `verifying_key`) or
    /// the substrate's request/reply correlation path. CLI maps the
    /// substrate `ProtocolError` family (e.g. `SignerKeyRevoked`,
    /// `InvalidSignature`, `AudienceMismatch`) to this single
    /// operator-facing variant; the substrate owns the canonical
    /// distinction. Exit 19.
    #[error("envelope authorization failed: {detail}")]
    EnvelopeAuthorizationFailed {
        /// Operator-safe diagnostic.
        detail: String,
    },
    /// Endpoint URI scheme is not in the mesh allowlist
    /// (`tcp://`, `quic://`, `bluetooth://` per RFC-0011-f §Peer
    /// Summary Shape). Mapped from `octo_mesh::MeshError::InvalidEndpointScheme`
    /// at the `peer add` dispatch boundary. Exit 28 (shared slot
    /// with `forward`'s `InvalidTtlHops` per RFC-0011-f §Exit Codes).
    #[error("invalid endpoint URI scheme: `{scheme}` (allowlist: tcp://, quic://, bluetooth://)")]
    InvalidEndpointScheme {
        /// The rejected scheme (lowercase, no `://`).
        scheme: String,
    },
    /// RPC reply did not arrive within the substrate timeout ceiling
    /// (default 30s per RFC-0011-f §Performance Targets). Surfaced
    /// from `octo_mesh::MeshError::RpcTimeout` at the `mesh rpc`
    /// dispatch boundary. The substrate ceiling is
    /// `octo_mesh::rpc_invoke`'s `timeout_ms` parameter (CLI default
    /// 30_000). Exit 20 per RFC-0011-f §Error Handling +
    /// §Subcommand Taxonomy `rpc` "Exit codes" row; this slot is
    /// claimed by the mesh amendment chain (codes 17-30 reserved
    /// for mesh errors) and supersedes RFC-0011-b's prior use for
    /// `ReputationNotFound` — both failure modes are operator-
    /// unambiguous within their respective command surfaces.
    #[error("RPC timeout after {timeout_ms}ms: peer `{peer}` method `{method}`")]
    RpcTimeout {
        /// Target peer DID (RFC-0010 canonical wire form).
        peer: String,
        /// Method name (verbatim operator input).
        method: String,
        /// Timeout ceiling in milliseconds (substrate-defined;
        /// CLI default 30_000).
        timeout_ms: u64,
    },
    /// Vault not owned by the active DID (RFC-0011-e §Error Handling).
    /// Mapped from `octo_vault::ProjectionError::VaultUnknown` at the
    /// `vault balance` dispatch boundary. Exit 23 per RFC-0011-e
    /// §Error Handling (reserved 17-63 range; 23 slot claimed by
    /// the vault amendment chain for read-surface failures).
    #[error("vault not owned: {0}")]
    VaultNotOwned(String),
    /// Projected vault balance is below the requested transfer amount
    /// (RFC-0011-e §Error Handling). Raised at `vault transfer`
    /// pre-flight step 4 (`project_vault_balance` < `--amount`). Both
    /// operands are rendered in DQA canonical form (RFC-0960-v36
    /// §Wire Form) so the operator can diff them directly. Exit 24.
    ///
    /// Transfer amounts are NOT redacted — they are chain-public
    /// information per RFC-0011-e §Redaction
    #[error("insufficient balance: have {have}, need {need}")]
    InsufficientBalance {
        /// Projected balance, DQA canonical form.
        have: String,
        /// Requested transfer amount, DQA canonical form.
        need: String,
    },
    /// `vault transfer` invoked without an RFC-0011-d transfer-capability
    /// provisioning for the active DID (RFC-0011-e §Error Handling).
    /// Exit 25.
    ///
    /// This is also the **stub-with-error** state: until the substrate
    /// role-gate hook lands, `vault transfer` surfaces this variant
    /// regardless of HSM availability (RFC-0011-e §Implementation
    /// Phases). The CLI does NOT implement role provisioning — that is
    /// RFC-0011-d's surface (per `[[cipherocto-design-principles]]`
    /// no-parallel-abstractions).
    #[error(
        "role not provisioned: transfer requires a provisioned transfer capability; see RFC-0011-d"
    )]
    RoleNotProvisioned,
    /// Cross-chain transfer attempted without `--dest-chain-id`
    /// (RFC-0011-e §Security: Cross-Chain Confusion). Exit 26.
    ///
    /// The CLI surfaces BOTH the source chain (parsed from `--from`)
    /// and the substrate-resolved destination chain so the operator can
    /// see what the substrate detected. There is no `ChainIdResolver`
    /// trait in the projection substrate, so the destination chain
    /// arrives via an opaque substrate error.
    #[error("chain id mismatch: source chain `{from}` != destination chain `{to}`; pass --dest-chain-id to confirm a cross-chain transfer")]
    ChainIdMismatch {
        /// Source chain ID (canonical form, from `--from`).
        from: String,
        /// Substrate-resolved destination chain ID (canonical form).
        to: String,
    },
    /// `--chain-id` or `--dest-chain-id` failed RFC-0010 canonical-form
    /// parse (RFC-0011-e §Error Handling). Exit 26 — shared with
    /// [`OctoCliError::ChainIdMismatch`] because both are operator-input
    /// validation failures on chain IDs.
    #[error("invalid chain id: {received}")]
    InvalidChainId {
        /// The rejected operator input, verbatim.
        received: String,
    },

    /// Neither `OCTO_HOME` nor `$HOME` is set in the operator's
    /// environment. The CLI fails closed (exit 27) rather than
    /// defaulting to `/tmp/.octo` — a world-readable/writable
    /// directory on shared hosts, where any local user could race
    /// the operator's writes or inject peer-table entries
    /// (Wave 4.5 Lens-2 finding 3).
    #[error("no OCTO_HOME or HOME available; set $OCTO_HOME or $HOME before running this command")]
    NoOctoHome,

    /// Agent manifest JSON parse failure (RFC-0011-c §9.8). `path` is
    /// the operator-supplied `--manifest-path` so the operator can
    /// correct the file reference; `reason` is the underlying
    /// `serde_json` diagnostic. Maps from substrate
    /// `WalletError::ManifestParse { path, reason }`. Exit 39.
    #[error("agent manifest parse error at `{path}`: {reason}")]
    ManifestParseError {
        /// Operator-supplied `--manifest-path` value.
        path: String,
        /// Substrate diagnostic (sanitized).
        reason: String,
    },
    /// 6-step capability validation pipeline failed at the given
    /// 1-based step (RFC-0002 §Capability Validation; RFC-0011-c
    /// §9.6). Substrate signals each step's specific failure mode;
    /// CLI surfaces the step number to the operator. Exit 40.
    #[error("capability validation failed at step {0} (RFC-0002 §Capability Validation)")]
    CapabilityValidationFailed(usize),
    /// `register_agent` was called with `(manifest, active_did)`
    /// whose derived `agent_id` is already present in the wallet
    /// substrate registry (RFC-0011-c §9.10). Exit 41.
    #[error("agent already registered: {0}")]
    AgentAlreadyExists(uuid::Uuid),

    /// `--limit` argument was zero or otherwise unparseable for the
    /// agent-list filter (RFC-0011-c §9.8, slot 45 reserved by the
    /// agent amendment chain). Substrate clamps at 1024 silently —
    /// this variant surfaces the operator error up front so the
    /// `0` / non-numeric input never reaches the substrate. Exit 45.
    #[error("invalid --limit value `{0}` (must be 1..=1024)")]
    InvalidLimit(String),

    /// `--cursor` argument could not be parsed by the substrate
    /// (RFC-0011-c §9.8, slot 46 reserved by the agent amendment
    /// chain). Phase 1 cursors are opaque tokens; malformed input
    /// surfaces here so the CLI never reaches the substrate with
    /// ambiguous pagination state. Exit 46.
    #[error("invalid --cursor value `{0}` (malformed opaque token)")]
    InvalidCursor(String),

    /// `lookup_agent` miss — agent UUID not found in the caller-attested
    /// DID's wallet registry (RFC-0011-c §9.8 + RFC-0015 §6.2.3).
    /// Mirrors the `AgentAlreadyExists` (slot 41) inverse but for the
    /// not-found side. Exit 42.
    #[error("agent not found: {0}")]
    AgentNotFound(uuid::Uuid),

    /// Caller-attested DID does not match the filter's `holder_did`
    /// field — SECURITY HIGH (multi-DID enumeration prevention per
    /// RFC-0015 §6.2.1). Exit 17 per RFC-0011 §Exit Codes 17-63
    /// reserved range (NOT slot 13 which is `PolicyNotFound`, NOT
    /// slot 37 which is owned by RFC-0011-g `UnknownAttestationKind`).
    #[error("forbidden: holder DID mismatch")]
    ForbiddenHolderMismatch,

    /// `transition_agent` observed the per-agent lock held by a
    /// concurrent transition (RFC-0015-a §6.1 + RFC-0011-c §9.8,
    /// slot 43 reserved by the agent amendment chain). Exit 43.
    #[error("agent already in transition: {0}")]
    AlreadyInTransition(uuid::Uuid),

    /// `transition_agent` rejected the requested state-machine edge
    /// (RFC-0015-a §6.1 + Appendix A state-machine guard). The CLI
    /// renders the typed `from`/`to` labels (canonical
    /// `AgentState::as_str()` form) so the operator can diff against
    /// the substrate guard table. Exit 43 (shared slot with
    /// `AlreadyInTransition` — both are write-path state errors).
    #[error("invalid agent state transition: {from} -> {to}")]
    InvalidStateTransition {
        /// Current (rejected-source) state label (`AgentState::as_str`).
        from: String,
        /// Requested (rejected-target) state label (`AgentState::as_str`).
        to: String,
    },

    /// Audit chain sink is not configured (RFC-0015-a §6.1 paired-
    /// acceptance bridge — no `octo-audit` sink registered for the
    /// write-path façade). Exit 52 per RFC-0011-c §9.8 slot
    /// allocation + RFC-0016-a §6.7 audit-substrate-not-ready code.
    /// Same slot as `AuditUnavailable` substrate mapping; the CLI
    /// surfaces this when the substrate's `octo-audit-internal`
    /// feature is OFF (default build) OR when the sink registration
    /// step has not run.
    #[error("audit substrate not ready: register the audit sink or enable the octo-audit-internal feature")]
    AuditSubstrateNotReady,

    /// Receipt id not found in the receipt store (RFC-0016-a §6.7
    /// paired-with-RFC-0011-a; canonical decimal `u64` form).
    /// Mapped from `octo_audit::AuditError::ReceiptNotFound` at
    /// the dispatch boundary. Exit 17 per RFC-0016-a §6.7 table;
    /// shares the slot with `InvalidTtlHops` + `ForbiddenHolderMismatch`
    /// per the established amendment-chain shared-slot pattern
    /// (operator-unambiguous within their respective command surfaces).
    #[error("receipt not found: {0}")]
    ReceiptNotFound(String),

    /// Per-process trust boundary violated on the audit substrate
    /// (RFC-0016-a §6.7 paired-with-RFC-0011-a; canonical scrubbed
    /// path form). Mapped from
    /// `octo_audit::AuditError::PermissionDenied` at the dispatch
    /// boundary. Exit 13 per RFC-0016-a §6.7 table; shares the slot
    /// with `PolicyNotFound` (both are operator-input validation
    /// failures on access credentials — operator-unambiguous within
    /// their respective command surfaces).
    #[error("permission denied: {0}")]
    PermissionDenied(String),

    /// Substrate-faithful read pipeline failure on the audit surface
    /// (RFC-0011-a §Error Handling). Surfaces from the
    /// `list_receipts` / `get_receipt` substrate path when the
    /// substrate returns a non-`ReceiptNotFound` / non-`InvalidFilter`
    /// / non-`PermissionDenied` failure (e.g. `SinkSpecific` adapter
    /// failure, registry mutex poisoning). Exit 18 per RFC-0011-a
    /// §Error Handling — first occupant of the parent-reserved
    /// 17-63 range after `ReceiptNotFound` (17). Shares the slot
    /// with `MeshCapabilityInsufficient` (amendment-chain shared-
    /// slot pattern; operator-unambiguous within their respective
    /// command surfaces).
    #[error("audit read failed: {0}")]
    AuditReadFailed(String),

    /// `octo audit list` matched more rows than the substrate hard
    /// ceiling (RFC-0011-a §Filters; substrate `MAX_LIMIT = 10_000`
    /// per TV-AUD-4c). The operator gets an explicit signal — not a
    /// silently-clamped truncation — so the next call can narrow the
    /// filter and re-invoke. Exit 19 per RFC-0011-a §Error Handling
    /// — first occupant of the parent-reserved 17-63 range after
    /// `ReceiptNotFound` (17) + `AuditReadFailed` (18). Shares the
    /// slot with `EnvelopeAuthorizationFailed` (amendment-chain
    /// shared-slot pattern; operator-unambiguous within their
    /// respective command surfaces).
    #[error("audit response too large: matched {matched}, limit {limit} (RFC-0011-a §Filters MAX_LIMIT)")]
    AuditResponseTooLarge {
        /// Number of rows that matched the filter (before truncation).
        matched: usize,
        /// Substrate hard ceiling (`MAX_LIMIT = 10_000`).
        limit: usize,
    },

    /// `octo agent attach` observed the target agent exists but is
    /// not in `Running` state (RFC-0011-c §9.8 + RFC-0015-a §6.3,
    /// slot 48 reserved by the agent amendment chain). Exit 48.
    #[error("agent not running: {0}")]
    AgentNotRunning(uuid::Uuid),

    /// `octo agent attach` reached the runtime substrate boundary but
    /// the `octo-runtime` crate is not yet wired into the CLI
    /// dispatch path (per RFC-0011-c §Implementation Phases Phase 1
    /// release gate; substrate `octo_runtime::attach` exists but the
    /// in-process `RuntimeHandle` mint pathway requires the
    /// `octo agent run --detach` integration that ships in a follow-on
    /// mission). Exit 51 per RFC-0011-c §9.8 slot allocation.
    #[error("runtime substrate not ready: octo agent run --detach must ship before attach can bind to a live RuntimeHandle (per RFC-0011-c §Implementation Phases Phase 1)")]
    RuntimeSubstrateNotReady,

    /// `octo_runtime::attach` returned a substrate-level failure
    /// (handle revoked, channel closed, invalid attach token).
    /// Mapped from `octo_runtime::RuntimeError::HandleRevoked` /
    /// `RuntimeError::RuntimeAttachFailed { reason }` /
    /// `RuntimeError::EventStreamClosed` at the dispatch boundary.
    /// Exit 49 per RFC-0011-c §9.8.
    #[error("runtime attach failed: {reason}")]
    RuntimeAttachFailed {
        /// Substrate-internal failure reason (sanitized by CLI via
        /// `sanitize_substrate_error`).
        reason: String,
    },

    /// `octo_runtime::spawn_agent` returned a substrate-level failure
    /// (handle mint failed, channel registration error, reused
    /// handle). Mapped from
    /// `octo_runtime::RuntimeError::RuntimeSpawnFailed { reason }`
    /// at the dispatch boundary. Exit 44 per RFC-0011-c §9.8
    /// slot allocation (RFC-0011-c agent amendment chain slots
    /// 39-52 reserved). The legacy `RuntimeError::InvalidAttachHandle`
    /// variant was folded into `RuntimeSpawnFailed` during the
    /// Path B rename — substrate-visible 6-field `AttachHandle`
    /// token errors surface via the `AttachError` envelope
    /// (`From<AttachError> for OctoCliError` below).
    #[error("runtime spawn failed: {reason}")]
    RuntimeSpawnFailed {
        /// Substrate-internal failure reason (sanitized by CLI via
        /// `sanitize_substrate_error`).
        reason: String,
    },

    /// `octo governance snapshot` resolved to a snapshot whose
    /// `expires_at_unix <= now_unix` (TTL boundary inclusive on
    /// the stale side per RFC-0011-g §Performance Targets). Exit
    /// 35 per RFC-0011-g §Error Handling.
    #[error("snapshot stale: snapshot_id {snapshot_id_hex} expired {age_secs}s ago (TTL = 600s)")]
    SnapshotStale {
        /// Hex-encoded snapshot id (BLAKE3-256).
        snapshot_id_hex: String,
        /// Age in seconds since `expires_at_unix`.
        age_secs: u64,
    },

    /// `octo governance snapshot --proposal-state <state>` carried a
    /// label that did not match any canonical `ProposalState`. The
    /// CLI accepts the RFC-0011-g §Subcommand Taxonomy labels;
    /// the dispatch boundary translates to substrate-native
    /// `ProposalState` discriminants and surfaces this error when
    /// translation fails. Exit 2 per clap arg-parse convention.
    #[error("invalid proposal-state label `{state}`; expected one of `Open`, `Quorum-Reached`, `Closed-Accepted`, `Closed-Rejected`, `Closed-Expired` per RFC-0011-g §Subcommand Taxonomy")]
    InvalidProposalState {
        /// The unrecognized label supplied on the CLI.
        state: String,
    },

    /// `octo governance snapshot` reached the substrate boundary but
    /// the `octo-governance` cache layer or projection surface
    /// returned an internal failure. Maps from
    /// `GovernanceSnapshotError::CacheError { reason }` /
    /// `GovernanceSnapshotError::InvalidChainId { input, reason }`.
    /// Exit 51 per the governance substrate boundary slot
    /// (parallel to `RuntimeSubstrateNotReady` exit 51).
    #[error("governance substrate error: {reason}")]
    GovernanceSubstrateError {
        /// Sanitized substrate failure reason.
        reason: String,
    },

    /// `octo governance vote --vote-cap <cap_id>` rejected the
    /// capability token (RFC-0011-g §Error Handling + RFC-0957
    /// §Capability Verification). The substrate
    /// `GovernanceError::VoteRejected { reason }` envelope surfaces
    /// this with operator-readable rationale (capability caveat set
    /// failure per RFC-0957 §Attenuation Invariant: missing
    /// `Audience(proposal_id)` OR missing `Before(deadline)` OR
    /// missing `Provider(active_role)`). Exit 36 per RFC-0011-g
    /// §Error Handling.
    #[error("vote rejected: {reason}")]
    VoteRejected {
        /// RFC-0957 attenuation failure reason (caveat set mismatch).
        reason: String,
    },

    /// `octo governance attest <subject_did> <attestation_kind>` carried
    /// a kind reference that did not match any registered attestation
    /// kind in the substrate registry (RFC-0011-g §Attestation Kind
    /// Resolution — TypedDiscriminator pattern per
    /// [[cipherocto-design-principles]] §Extension over enumeration).
    /// The substrate carries the typed-discriminator namespace; this
    /// error surfaces the unknown kind verbatim so the operator can
    /// diff against the registered namespace. Exit 37 per RFC-0011-g
    /// §Error Handling (slot reserved at `error.rs:373` comment).
    #[error("unknown attestation kind: {kind_ref}")]
    UnknownAttestationKind {
        /// Typed-discriminator kind reference supplied on the CLI
        /// (`<namespace>:<subkind>` form per RFC-0011-g §Attestation Kind
        /// Resolution).
        kind_ref: String,
    },

    /// `octo governance attest` or `octo governance vote` invoked
    /// against a substrate that is not yet ready per the multi-prereq
    /// gate (RFC-0011-g §Compatibility Mixed-Version Compatibility).
    /// The substrate returns `GovernanceError::PrereqNotAccepted { rfc_ref }`
    /// when the relevant upstream RFC is still in Draft; the CLI
    /// surfaces this verbatim. Exit 38 per RFC-0011-g §Error Handling
    /// (no operator cost; advisory).
    #[error("prerequisite not accepted: {rfc_ref}")]
    PrereqNotAccepted {
        /// RFC reference of the gating prerequisite that has not
        /// yet reached Accepted status.
        rfc_ref: String,
    },

    /// Unexpected internal failure.
    #[error("internal error: {0}")]
    Internal(String),

    /// `AttachHandle` token's TTL elapsed (`now_unix > token.ttl_unix`)
    /// OR the substrate-reserved fail-CLOSED sentinel `ttl_unix == u64::MAX`
    /// (per RFC-0011-c §F.2 step (c) of `attach_with_token`).
    /// Both paths collapse to one envelope via substrate `AttachError::Expired`.
    /// Exit 53.
    #[error("attach handle expired: mint={mint_unix}, ttl={ttl_unix}, now={now_unix}")]
    AttachHandleExpired {
        /// `mint_timestamp_unix` from the token (mirrors substrate
        /// `AttachError::Expired::mint_unix` so the operator can
        /// read the full `{mint, ttl, now}` triplet without
        /// re-deriving it).
        mint_unix: u64,
        /// `ttl_unix` from the token.
        ttl_unix: u64,
        /// Operator-clock `now_unix` observed at validation.
        now_unix: u64,
    },

    /// `AttachHandle` signature verification failed against the
    /// expected holder public key — RFC-0011-c §F.2 step (a). Mapped
    /// from `octo_runtime::AttachError::BadSignature`. Exit 54.
    #[error("attach handle bad signature: {reason}")]
    AttachHandleBadSignature {
        /// Substrate-internal reason (sanitized via `sanitize_substrate_error`).
        reason: String,
    },

    /// `AttachHandle` claims a `session_id` that does not match the
    /// current spawn-time session id registered for the agent —
    /// RFC-0011-c §F.2 step (d). Mapped from
    /// `octo_runtime::AttachError::SessionMismatch`. Exit 55.
    #[error("attach session mismatch: token claims session `{token_session_hex}`, agent registered `{registered_session_hex}`")]
    AttachSessionMismatch {
        /// Hex-encoded session id from the token (64 lowercase hex chars).
        token_session_hex: String,
        /// Hex-encoded registered session id (64 lowercase hex chars).
        registered_session_hex: String,
    },

    /// `AttachHandle` references a `session_id` that is not present in
    /// the spawn-time registry for any agent under the active DID —
    /// RFC-0011-c §F.2 step (d) predecessor. Mapped from
    /// `octo_runtime::AttachError::UnknownSession`. Exit 56.
    #[error("attach session unknown: session `{0}` is not registered for any active agent")]
    AttachSessionUnknown(String),

    /// Stoolap-backed event-cursor persistence failure (Phase 2
    /// RFC-0011-c §F.3 feature-gated surface). Mapped from
    /// `octo_runtime::AttachError::PersistenceError`. Exit 57.
    #[error("persistence error: {0}")]
    PersistenceError(String),

    /// Revocation-set mutation/lookup failure on the process-singleton
    /// revocation set (RFC-0011-c §F.3). Mapped from
    /// `octo_runtime::AttachError::RevocationError`. Exit 58.
    #[error("revocation error: {0}")]
    RevocationError(String),

    /// Operator-supplied session id hex string failed to parse
    /// (wrong length, non-UTF-8 bytes, non-hex pair). CLI-parse
    /// failure on `octo agent revoke-attach --session-id` input —
    /// the supplied string is not a 64-char lowercase hex
    /// `SessionId`. Distinct from `AttachHandleBadSignature` (exit
    /// 54, substrate signature-verify failure). Exit 47 per
    /// RFC-0011-c §9.8 row.
    #[error("invalid session id hex: {reason}")]
    InvalidSessionIdHex {
        /// Diagnostic reason (length / encoding / non-hex).
        reason: String,
    },

    /// `octo agent attach --since <UNIX>` carried a cursor that is
    /// below the token's mint timestamp — the token cannot
    /// authorize events that pre-date it. Mapped from
    /// `octo_runtime::AttachError::InvalidSinceCursor` per
    /// RFC-0011-c §F.2 validation chain step (d). Exit 53 (shared
    /// slot with `AttachHandleExpired` per amendment-chain
    /// shared-slot pattern; operator-unambiguous within the attach
    /// command surface — the render layer distinguishes the two
    /// payloads).
    #[error("invalid since cursor: requested {requested} is below token mint {mint_unix}")]
    InvalidSinceCursor {
        /// Token mint timestamp (lower-bound replay horizon).
        mint_unix: u64,
        /// `since_unix` supplied by the caller (below mint).
        requested: u64,
    },

    /// `attach_with_token` step (e) found no registered handler for
    /// the token's `TransportKind` (RFC-0011-c §F.2 step (e) +
    /// [[cipherocto-design-principles]] §per-extension crates +
    /// registry). Substrate ships the built-in `InProcessHandler`;
    /// extension transports (e.g., `UnixSocket`) require a
    /// follow-on Layer D transport crate
    /// (`octo-runtime-transport-unix`, …) to register a handler
    /// at process startup. Mapped from
    /// `octo_runtime::AttachError::TransportHandlerNotRegistered`.
    /// Exit 59 per RFC-0011-c §9.8 extension slots 39-61.
    #[error("transport handler not registered for kind `{kind_label}`")]
    TransportHandlerNotRegistered {
        /// Discriminator label from the substrate surface
        /// (`InProcess`, `UnixSocket`, or `Raw(<uuid>)`).
        kind_label: String,
    },

    /// Operator passed `--detach --token-file` against an idempotent
    /// self-transition (`Registered → Running` no-op where the
    /// substrate did not mint a fresh `RuntimeHandle`). The clap
    /// interlock `requires = "detach"` lets the combo pass validation
    /// even when the spawn was a no-op; without a fresh handle there
    /// is no `session_id` to bind a token to. CLI-side dispatch
    /// surface — distinct from the 9 substrate `AttachError` mirror
    /// variants (slots 53-61). Exit 60 per RFC-0011-c §9.8 (reserved
    /// per this amendment cycle).
    #[error("token mint skipped: {reason}")]
    TokenMintSkipped {
        /// Operator-actionable reason (idempotent self-transition
        /// vs agent not in transition-eligible state).
        reason: String,
    },

    /// Replay detected — `since_unix` cursor at or behind the recorded
    /// session cursor (the consumption guarantee is violated). Mirrors
    /// `octo_runtime::AttachError::ReplayDetected` per the RFC-0011-c
    /// §9.7 follow-on amendment. Exit 61 (own slot — distinct from
    /// `Internal(reason)` exit 64).
    #[error(
        "replay detected: since cursor {since_unix} is at or behind the recorded cursor {recorded_cursor}"
    )]
    ReplayDetected {
        /// `since_unix` from the rejected attach invocation (at or
        /// behind the recorded session cursor).
        since_unix: u64,
        /// Highest `since_unix` previously accepted for this session.
        recorded_cursor: u64,
    },
    /// `octo network peers get` lookup miss.
    ///
    /// Substrate `GatewayCache::get` returns `Option::None`; CLI
    /// translates `None` → this variant (RFC-0011-h §Error Handling
    /// row 79 + RFC-0011-i §Error Handling). Exit 79.
    #[error("network peer not found")]
    NetworkPeerNotFound {
        /// Redacted gateway id (32 bytes hex per RFC-0011-h §Redaction Layer).
        gateway_id_hex: String,
    },
    /// `octo network identity show` precondition failure.
    ///
    /// `LocalGatewayIdentity::load` returns `NotInitialized` because
    /// `<octo_home>/network/local-gateway-identity.toml` is missing.
    /// CLI-side predicate, not substrate fault class per RFC-0011-h
    /// §Error Handling row 83 + RFC-0011-i §Error Handling. Exit 83.
    #[error("local gateway identity not initialized; run `octo network bootstrap` first")]
    NetworkLocalKeyUnavailable,
    /// `octo network trust-graph render --depth <n>` out-of-range.
    ///
    /// CLI-asserted clamp 1-100 protects substrate from OOM;
    /// `--depth 0` via programmatic bypass surfaces this variant
    /// (clap `value_parser` would catch user-facing path → exit 2
    /// `ValueValidation` per RFC-0011-h §Exit Codes row 2). CLI-only
    /// predicate; no substrate fault class. Exit 85.
    #[error("graph depth {depth} is below the 1..=100 range enforced by the CLI clamp")]
    NetworkGraphDepthBelowRange {
        /// Offending depth value.
        depth: u32,
    },
    /// `octo network identity show` DID codec rejection.
    ///
    /// CLI pre-validates DID format before substrate dispatch per
    /// RFC-0011-h §Error Handling row 86 + RFC-0011-i §Error Handling.
    /// DID redacted in operator-facing render. Exit 86.
    #[error("invalid network DID format (redacted in operator envelope)")]
    NetworkInvalidDid {
        /// Redacted DID string.
        did_redacted: String,
    },
    /// `octo network mode set` / `octo network authority rotate`
    /// persistence failure — TOML parse error or IO error.
    ///
    /// `BootstrapConfig::from_toml` or `save_toml` returned
    /// `BootstrapConfigError` which is forwarded to the operator
    /// envelope after CLI-side redaction. CLI-only predicate; the
    /// substrate failure class lives at the `BootstrapConfigError`
    /// boundary (Layer B per RFC-0011-j §Substrate-Additions G1 row).
    /// Exit 82.
    #[error("network config parse/write failure (redacted in operator envelope)")]
    NetworkConfigParseFailed {
        /// Redacted reason tag (one of: `io`, `toml_parse`, `toml_serialize`).
        kind_redacted: String,
        /// Optional path to the offending file (redacted if it
        /// contains operator home path components).
        path_redacted: Option<String>,
    },
    /// `octo network slash excluded` / `slash stats` /
    /// `slash list` / `slash show` / `mode show` / `authority show`
    /// companion mission closure gating.
    ///
    /// Pre-companion (G1/G6/G6b/G8 not yet LANDED), CLI dispatch
    /// surfaces exit 89 so operators see the substrate is unavailable
    /// rather than a confusing panic. Post-companion (after
    /// `next 931dc7b1` + `next ca3ceee1`), dispatch routes to
    /// substrate and exits 0. CLI-only predicate; no substrate
    /// fault class. Exit 89.
    #[error("network substrate unavailable (companion mission closure gating)")]
    NetworkSubstrateUnavailable {
        /// Companion mission tag (e.g., `G1`, `G6`, `G6b`, `G8`).
        companion: &'static str,
        /// Optional detail message (e.g., `BridgeError` variant
        /// for Phase 7 RFC-0011-o). Additive field; pre-existing
        /// call sites use `detail: ""` (empty detail).
        detail: String,
    },
    /// `octo network coordinator show` lookup miss.
    ///
    /// `CoordinatorRecord::load(coordinator_id)` returned `None`
    /// (substrate-faithful per RFC-0011-k §Substrate-Additions G12b
    /// row, landed at `next 10ae8e18`). Translates the substrate
    /// miss into a typed operator-facing exit. CLI-only predicate;
    /// no substrate fault class — the substrate returns `Option`
    /// not `Result`. Exit 84.
    #[error("network coordinator not found (substrate `CoordinatorRecord::load` returned None)")]
    NetworkCoordinatorNotFound {
        /// 52-char hex-encoded coordinator ID (CLI-parsed form).
        coordinator_id_redacted: String,
    },
    /// `octo network bind-envelope rebind-{prepare,commit,abort}`
    /// dry-run denial.
    ///
    /// Per RFC-0011-l Phase 4 §Subcommand Taxonomy rebind-* rows,
    /// the rebind trio defaults to `--dry-run` and requires
    /// `--confirm-acknowledge` (and for `rebind-commit` also
    /// `--confirm` per the §Security Considerations pastejacking
    /// defense) to lift dry-run. When the operator invokes with
    /// `--no-dry-run` but declines at the preview prompt, the
    /// CLI fires this typed exit. CLI-only predicate (the preview
    /// prompt is a CLI-side gate, not a substrate fault class).
    /// Exit 88 per RFC-0011-h §Error Handling row 88.
    #[error("network dry-run denied (operator declined at preview prompt)")]
    NetworkDryRunDenied {
        /// The rebind arm that the operator declined
        /// (`prepare` | `commit` | `abort`).
        arm: &'static str,
        /// The 52-char hex domain_id the operator was previewing.
        domain_id_redacted: String,
    },
    /// `AttachError::UnknownKeyId` translation (LANDED at
    /// `crates/octo-runtime/src/handle/error.rs` per
    /// RFC-0011-c §F.5.1 paired-acceptance bridge; gated on
    /// `octo-attach-key-rotation` Cargo feature). Variant mints
    /// unconditionally at slot 91 per RFC-0011-w §Motivation;
    /// the translation arm fires only when the substrate feature
    /// is enabled. Pre-RFC-0011-w, this slot was RESERVED per
    /// [^rotation-error-slot-prealloc].
    #[error("network key rotation: unknown key_id 0x{key_id_hex} (known_keys band: {known_keys_band:?})")]
    NetworkKeyRotationUnknownId {
        /// `key_id` discriminator rendered as 8 hex chars.
        /// Substrate `KeyId = u32` (typed version discriminator
        /// for holder signing key per RFC-0011-c §F.5.1; NOT a
        /// Layer A cryptographic secret) hex-encoded big-endian
        /// via `key_id.to_be_bytes()`. Mirrors the
        /// `hex::encode(declared)` / `hex::encode(actual)` pattern
        /// in the `SessionMismatch` translation arm.
        key_id_hex: String,
        /// Categorical band of known keys in the verifier's
        /// `KeySet` (active + grace period). 3-band quantization
        /// per §Redaction Layer: operator envelope renders the
        /// enum tag only (`None` / `Few` / `Many`) via the
        /// `#[error(... {known_keys_band:?})]` Debug formatter;
        /// the bucket thresholds (0 / 1-8 / >8) are implementation
        /// detail of `KnownKeysBand::from_count` and are NOT
        /// echoed in operator-facing render.
        known_keys_band: KnownKeysBand,
    },

    /// `WalletStore` is locked — the identity seed requires
    /// `WalletStore::unlock(passphrase)` to access (mission
    /// 0011-x-s-a-wallet-store-identity §AC-6, paired-acceptance
    /// bridge from `WalletError::Locked`). Unit variant; the
    /// substrate carries the no-payload `Locked` form so the
    /// CLI envelope mirrors it 1:1. Exit 92 per the wallet-store
    /// amendment-chain slot allocation (NOT shared with any
    /// existing slot — the wallet-store surface is its own
    /// amendment chain, distinct from agent amendment chain
    /// slots 39-52 and mesh amendment chain slots 17-30).
    #[error("wallet store is locked; unlock with a passphrase to access the identity key")]
    WalletLocked,

    /// Identity lifecycle transition was refused at the
    /// substrate (RFC-0011-x §Lifecycle Requirements). Mapped
    /// from the family of `WalletError` lifecycle refusal
    /// variants — `NotActive { .. }`,
    /// `RotationInProgress`, `NotRotating { .. }`, `SelfRotation`,
    /// `GracePeriodNotElapsed { .. }`, `InvalidSuccessorProof`,
    /// `InvalidRevocationProof` — via the `From<WalletError>`
    /// envelope. The substrate's `Display` impl is rendered
    /// verbatim into `reason` so the operator gets the canonical
    /// substrate-shape failure message without a CLI-side
    /// re-translation layer. Exit 43 per RFC-0011-x §Error
    /// Handling (shared with `AlreadyInTransition` /
    /// `InvalidStateTransition` per amendment-chain
    /// shared-slot pattern — both are state-machine write-path
    /// errors operator-unambiguous within their respective
    /// command surfaces; the render layer distinguishes the
    /// payloads).
    #[error("identity transition refused: {reason}")]
    IdentityTransitionRefused {
        /// Substrate `Display`-rendered reason string (sanitized
        /// via `sanitize_substrate_error` before the CLI
        /// envelope carries it).
        reason: String,
    },

    /// Supplied passphrase is below the enforced floor (mission
    /// 0011-x-s-a-wallet-store-identity §AC-28). Mapped from
    /// `WalletError::WeakPassphrase` at the dispatch boundary.
    /// Unit variant; the substrate carries the no-payload form
    /// and the `Display` impl interpolates
    /// `MIN_PASSPHRASE_CHARS` so the operator-visible message
    /// stays locked to the threshold the check compares against.
    /// Exit 2 per clap arg-parse convention (operator-input
    /// validation failures share exit 2 across the CLI surface;
    /// the render layer distinguishes the `WeakPassphrase`
    /// payload from `ClapParse` / `NoActiveIdentity` /
    /// `ConfirmationRequired`).
    #[error("passphrase is below the {MIN_PASSPHRASE_CHARS}-character floor")]
    WeakPassphrase,

    /// An operation the surface deliberately restricts to dev mode was
    /// attempted elsewhere.
    ///
    /// The variant exists because these refusals were reported as
    /// `Internal` at exit 64, whose hint tells the operator to re-run
    /// with `RUST_LOG=debug` and report the diagnostic. That is
    /// advice for an unexpected fault; this is a deliberate gate with
    /// a one-flag remedy, and sending an operator to a bug report
    /// because they omitted `--mode dev` is a false escalation.
    /// Exit 2 per the same operator-input-validation convention
    /// `WeakPassphrase` and `InvalidReason` follow.
    #[error("dev-mode-only operation refused: {detail}")]
    DevModeRequired {
        /// The refusal and its remedy, rendered so the operator sees
        /// the exact flag to add rather than a diagnostic request.
        detail: String,
    },

    /// Operator-supplied `--reason` was rejected by the substrate
    /// `validate_reason` guard before any state transition ran
    /// (RFC-0015 §6.2.5). Mapped from
    /// `WalletError::ReasonContainsControlChars` and
    /// `WalletError::ReasonTooLong` at the agent dispatch boundary.
    ///
    /// The variant exists because the previous mapping fell through
    /// `map_transition_wallet_error`'s catch-all to
    /// `Internal` at exit 64, which the `agent destroy` exit-code
    /// list documents as "unexpected substrate error". That blames
    /// the substrate for a value the operator typed. Exit 2 per the
    /// same operator-input-validation convention `WeakPassphrase`
    /// and `ClapParse` follow.
    ///
    /// `detail` carries a rendered, non-attacker-echoing summary —
    /// the offending code point in `<U+XXXX>` notation for the
    /// control-character form, the byte length for the
    /// over-length form. The raw offending bytes are never
    /// interpolated, per the substrate's own `validate_reason`
    /// contract.
    #[error("invalid --reason value: {detail}")]
    InvalidReason {
        /// Sanitized one-line description of why the reason was
        /// refused. Never the raw operator input.
        detail: String,
    },

    /// An operator-supplied input FILE was refused before any key was
    /// derived: it could not be read, its contents did not meet the
    /// stated contract, or its permission mode is group- or
    /// world-readable.
    ///
    /// Six distinct conditions share this variant and exit 2. On the
    /// seed file: a missing path, a payload that is neither 32 raw
    /// bytes nor 64 hex characters, hex that does not decode, and a
    /// permission mode that is group- or world-readable. On the
    /// passphrase file: a missing or unreadable path. On the payload:
    /// 32 bytes that are entirely ASCII hex characters, which would
    /// mint a different identity read as raw bytes.
    ///
    /// Every one of these was previously reported as `Internal` at
    /// exit 64, whose hint is "re-run with `RUST_LOG=debug` and
    /// report the diagnostic". That is advice for an unexpected
    /// fault. A 0644 seed file is a deliberate security gate, a
    /// wrong path is a typo, and a wrong-length seed is an input
    /// contract the message itself states. For the length case the
    /// CLI literally tells the operator the rule and then tells them
    /// to report a bug. Sending an operator to a bug report for any
    /// of these is a false escalation, and in CI the exit-64
    /// classification is what decides whether the pipeline retries
    /// or pages someone. Exit 2 per the same operator-input-
    /// validation convention `WeakPassphrase`, `InvalidReason` and
    /// `DevModeRequired` follow.
    #[error("input file refused: {detail}")]
    FileInputRejected {
        /// The refusal and its remedy. Carries the file's mode in
        /// octal for the permission form; never the file's contents,
        /// and never the passphrase.
        detail: String,
    },
}

impl OctoCliError {
    /// Process exit code for this failure.
    pub fn exit_code(&self) -> i32 {
        match self {
            Self::ClapParse(_) => 2,
            Self::NoActiveIdentity => 2,
            Self::ConfirmationRequired { .. } => 2,
            Self::AuditorDenied { .. } => 2,
            Self::AlreadyRotating => 3,
            Self::IdentityNotFound(_) => 4,
            Self::HsmUnavailable(_) => 5,
            Self::AlreadyRevoked => 6,
            Self::CaveatParse { .. } => 7,
            Self::InvalidCaveatCombination { .. } => 8,
            Self::HolderNotFound(_) => 9,
            Self::AttenuationViolation(_) => 10,
            Self::SigningFailed(_) => 11,
            Self::ParentCapNotFound(_) => 12,
            Self::PolicyNotFound(_) => 13,
            Self::PolicyVersionNotFound { .. } => 14,
            Self::StdinSecretRefused => 15,
            Self::InvalidFilter(_) => 16,
            Self::RoleNotFound(_) => 31,
            Self::StakeInsufficient { .. } => 32,
            Self::RoleNotSelectable { .. } => 33,
            // 34 reserved per F-16 (was RoleBindingConflict; intentionally
            // skipped — last-writer-wins per RFC-0011-d §Security 2).
            Self::SignerMismatch { .. } => 35,
            Self::GroupBindingRejected { .. } => 36,
            // 28 shared with `InvalidTtlHops` (RFC-0011-f §Exit Codes;
            // follow-on `forward` mission claims the same slot).
            Self::InvalidEndpointScheme { .. } => 28,
            Self::ReputationNotFound { .. } => 20,
            Self::ReputationRevoked { .. } => 21,
            Self::AnchorChainBroken { .. } => 22,
            Self::NoAnchorVerifyInMode { .. } => 2,
            Self::InvalidRoleSlug { .. } => 2,
            Self::InvalidTtlHops { .. } => 17,
            Self::MeshCapabilityInsufficient { .. } => 18,
            Self::EnvelopeAuthorizationFailed { .. } => 19,
            Self::RpcTimeout { .. } => 20,
            Self::VaultNotOwned(_) => 23,
            Self::InsufficientBalance { .. } => 24,
            Self::RoleNotProvisioned => 25,
            Self::ChainIdMismatch { .. } => 26,
            // Shared exit 26 with `ChainIdMismatch` per RFC-0011-e
            // §Error Handling — both are chain-ID input validation
            // failures.
            Self::InvalidChainId { .. } => 26,
            // Wave 4.5 Lens-2: fail-closed on missing $OCTO_HOME / $HOME
            // (no `/tmp/.octo` fallback — world-readable). Exit 27
            // (free slot in the 17-30 mesh reserved range).
            Self::NoOctoHome => 27,
            // RFC-0011-c §9.8: agent amendment chain slots 39-52.
            Self::ManifestParseError { .. } => 39,
            Self::CapabilityValidationFailed(_) => 40,
            Self::AgentAlreadyExists(_) => 41,
            Self::AgentNotFound(_) => 42,
            // RFC-0011 §Exit Codes 17-63 reserved range; slot 17
            // (free per amendment tracker — RFC-0011-g owns 37).
            Self::ForbiddenHolderMismatch => 17,
            Self::InvalidLimit(_) => 45,
            Self::InvalidCursor(_) => 46,
            // RFC-0015-a §6.3 + RFC-0011-c §9.8: agent amendment chain
            // write-path slots. `AlreadyInTransition` and
            // `InvalidStateTransition` share slot 43 (both are
            // state-machine write-path errors).
            Self::AlreadyInTransition(_) => 43,
            Self::InvalidStateTransition { .. } => 43,
            Self::AuditSubstrateNotReady => 52,
            // RFC-0016-a §6.7 paired-with-RFC-0011-a CLI-shape
            // variants. Shares slot 17 with `InvalidTtlHops` +
            // `ForbiddenHolderMismatch` (amendment-chain shared-slot
            // pattern; operator-unambiguous within respective command
            // surfaces).
            Self::ReceiptNotFound(_) => 17,
            // Shares slot 13 with `PolicyNotFound` (operator-input
            // validation failures on access credentials; operator-
            // unambiguous within respective command surfaces).
            Self::PermissionDenied(_) => 13,
            // Shares slot 18 with `MeshCapabilityInsufficient`
            // (amendment-chain shared-slot pattern; operator-
            // unambiguous within their respective command surfaces).
            Self::AuditReadFailed(_) => 18,
            // Shares slot 19 with `EnvelopeAuthorizationFailed`
            // (amendment-chain shared-slot pattern; operator-
            // unambiguous within their respective command surfaces).
            Self::AuditResponseTooLarge { .. } => 19,
            Self::AgentNotRunning(_) => 48,
            Self::RuntimeSubstrateNotReady => 51,
            Self::RuntimeAttachFailed { .. } => 49,
            Self::RuntimeSpawnFailed { .. } => 44,
            Self::SnapshotStale { .. } => 35,
            Self::InvalidProposalState { .. } => 2,
            Self::GovernanceSubstrateError { .. } => 51,
            Self::VoteRejected { .. } => 36,
            Self::UnknownAttestationKind { .. } => 37,
            Self::PrereqNotAccepted { .. } => 38,
            Self::Internal(_) => 64,
            Self::StaleStub { .. } => 65,
            // RFC-0011-c §F + §9.8: AttachHandle token pathway slots
            // 53-58 (paired with the substrate
            // `octo_runtime::AttachError::exit_code` table so the CLI
            // boundary and the substrate boundary agree on slot
            // allocation).
            Self::AttachHandleExpired { .. } => 53,
            Self::AttachHandleBadSignature { .. } => 54,
            Self::AttachSessionMismatch { .. } => 55,
            Self::AttachSessionUnknown(_) => 56,
            Self::PersistenceError(_) => 57,
            Self::RevocationError(_) => 58,
            Self::InvalidSessionIdHex { .. } => 47,
            // Shared slot with `AttachHandleExpired` (exit 53) per
            // amendment-chain shared-slot pattern; the render layer
            // distinguishes the two payloads.
            Self::InvalidSinceCursor { .. } => 53,
            // RFC-0011-c §F.2 step (e) + per-extension crates +
            // registry pattern: extension transport (UnixSocket, …)
            // without a registered handler surfaces as exit 59.
            Self::TransportHandlerNotRegistered { .. } => 59,
            // RFC-0011-c §F.6.1: CLI-dispatch-side precondition
            // failure (idempotent self-transition on `--detach`
            // `--token-file`). Distinct from substrate AttachError
            // mirror variants slots 53-59. Exit 60 reserved per
            // §9.8 reserved range 39-63.
            Self::TokenMintSkipped { .. } => 60,
            // RFC-0011-c §9.7 follow-on amendment + §9.8 row: own
            // slot 61 — distinct from `Internal(reason)` exit 64.
            // Typed-discriminator additive variant; preserves the
            // full `{since_unix, recorded_cursor}` pair on the CLI
            // boundary so the operator can read the diagnostic
            // without re-deriving from event-stream state.
            Self::ReplayDetected { .. } => 61,
            // RFC-0011-h §Error Handling + RFC-0011-i Phase 1 slots 79/83/85/86
            // (FORWARD-LOOKING per RFC-0011-h §Error Handling row 537
            // substrate-faithfulness rationale; variants land during
            // implementation across Phases 1-6).
            Self::NetworkPeerNotFound { .. } => 79,
            // RFC-0011-j §Error Handling row 82 — TOML parse/write
            // failure for `octo network mode set` + `authority rotate`.
            Self::NetworkConfigParseFailed { .. } => 82,
            Self::NetworkLocalKeyUnavailable => 83,
            // RFC-0011-k §Error Handling row 84 — coordinator lookup
            // miss (substrate-faithful Option::None translation).
            Self::NetworkCoordinatorNotFound { .. } => 84,
            Self::NetworkGraphDepthBelowRange { .. } => 85,
            Self::NetworkInvalidDid { .. } => 86,
            // RFC-0011-j §Error Handling row 89 — companion mission
            // closure gating. Pre-companion (G1/G6/G6b/G8 not yet
            // LANDED), CLI dispatch surfaces exit 89. Post-companion
            // (after substrate slice commits at `next 931dc7b1` and
            // `next ca3ceee1`), dispatch routes to substrate and
            // exits 0.
            Self::NetworkSubstrateUnavailable { .. } => 89,
            // RFC-0011-l Phase 4 §Error Handling row 88 — rebind-*
            // interactive dry-run denial. CLI-side preview prompt
            // decline; not a substrate fault class.
            Self::NetworkDryRunDenied { .. } => 88,
            // RFC-0011-w: slot 91 activation per
            // [^rotation-error-slot-prealloc]. Variant mints
            // unconditionally; reachable when the
            // `octo-attach-key-rotation` feature is enabled in
            // `octo-runtime` AND a CLI caller surfaces the
            // `AttachError::UnknownKeyId` translation path (future
            // amendment per RFC-0011-w §Future Work F1).
            Self::NetworkKeyRotationUnknownId { .. } => 91,
            // RFC-0011-x §Error Handling: wallet-store amendment
            // chain slots 92/93/94. Slot 92 = `WalletLocked`
            // (paired with `WalletError::Locked` per
            // `0011-x-s-a-wallet-store-identity` §AC-6). Slot 93 =
            // `IdentityTransitionRefused` (paired with the
            // `WalletError` lifecycle-refusal family). Slot 94 =
            // `WeakPassphrase` shares exit 2 with `ClapParse` /
            // `NoActiveIdentity` / `ConfirmationRequired` /
            // `AuditorDenied` / `InvalidRoleSlug` /
            // `NoAnchorVerifyInMode` / `InvalidProposalState` per
            // the established operator-input validation failure
            // shared-slot pattern (render layer distinguishes
            // the payloads).
            Self::WalletLocked => 92,
            Self::IdentityTransitionRefused { .. } => 43,
            Self::WeakPassphrase => 2,
            Self::InvalidReason { .. } => 2,
            Self::DevModeRequired { .. } => 2,
            Self::FileInputRejected { .. } => 2,
        }
    }

    /// Operator-safe message with substrate internals stripped.
    pub fn user_message(&self) -> String {
        sanitize_substrate_error(&self.to_string())
    }

    /// Per-variant remediation hint.
    pub fn hint(&self) -> Option<String> {
        let h: String = match self {
            Self::ClapParse(_) => "run `octo --help` for usage".to_string(),
            // Carries two distinct substrate conditions, so the hint
            // has to cover both. `NotActive` reaches here for a
            // `Designated` record as well as for a wallet with no
            // pointer at all, and the previous hint only mentioned
            // the second - it told an operator whose wallet already
            // had a selected identity to create or select one, which
            // they had already done.
            Self::NoActiveIdentity => "no ACTIVE identity: either the wallet has no active pointer (create or select one with `octo identity register` / `octo identity select --did <did>`), or the active record is `Designated` rather than `Active` and must be promoted - see `octo identity list` for the current lifecycle of every record".to_string(),
            Self::ConfirmationRequired { .. } => {
                "re-run with the flag your mode requires: `--confirm --confirm-acknowledge` \
                 in Human mode, `--allow-write` in Ci and Dev mode. `--confirm` alone never \
                 satisfies the Ci or Dev gate, so a retry with it repeats this error"
                    .to_string()
            }
            Self::AuditorDenied { .. } => {
                "auditor mode is read-only and this command is not part of that role; run it in a \
                 human or ci session. Auditor sessions are deliberately denied every identity \
                 command, including the read-only ones, so this refusal is the expected result \
                 and not a fault to report"
                    .to_string()
            }
            Self::AlreadyRotating => "complete or abort the in-flight rotation first".to_string(),
            // `identity show` takes an OPTIONAL single DID and defaults to the
            // active one. It never enumerates, so naming it here pointed an
            // operator who did not know the DID at a command that cannot list
            // them: `identity select --did <unknown>` and `identity show <unknown>`
            // both print this same hint, and neither discovers anything. The
            // command that enumerates is `identity list`.
            Self::IdentityNotFound(_) => {
                "list the identities in this wallet with `octo identity list`, then re-run with one of the DIDs it prints".to_string()
            }
            Self::HsmUnavailable(_) => "check that the HSM backend is reachable".to_string(),
            Self::AlreadyRevoked => {
                "this identity is already revoked or already registered; no action needed".to_string()
            }
            Self::CaveatParse { .. } => "check the caveat expression syntax".to_string(),
            Self::InvalidCaveatCombination { .. } => "remove conflicting caveats".to_string(),
            Self::HolderNotFound(_) => "verify the holder DID".to_string(),
            Self::AttenuationViolation(_) => "attenuation may only narrow authority".to_string(),
            Self::SigningFailed(_) => "verify the signing key is available".to_string(),
            Self::ParentCapNotFound(_) => "list capabilities with `octo capability list`".to_string(),
            Self::PolicyNotFound(_) => "list policies with `octo policy list`".to_string(),
            Self::PolicyVersionNotFound { .. } => "omit `--version` to use the latest version".to_string(),
            Self::StdinSecretRefused => "re-run with `--allow-stdin-secret` if intended".to_string(),
            Self::InvalidFilter(_) => "filter syntax is `key=value`".to_string(),
            Self::RoleNotFound(_) => "list roles with `octo role list`".to_string(),
            Self::StakeInsufficient { .. } => "top up the operator's OCTO stake and retry".to_string(),
            Self::RoleNotSelectable { .. } => "verify the role slug + operator permissions".to_string(),
            Self::SignerMismatch { .. } => {
                "the active signer does not match the supplied operator DID".to_string()
            }
            Self::GroupBindingRejected { .. } => {
                "re-fetch the platform admin proof and retry the role binding".to_string()
            }
            Self::ReputationNotFound { .. } => {
                "the subject has no aggregate for this role yet; attestations land first".to_string()
            }
            Self::ReputationRevoked { .. } => {
                "revoked DIDs are read-only across all operator modes (RFC-0011-b §Security 3)".to_string()
            }
            Self::AnchorChainBroken { .. } => {
                "the last anchor chain digest does not match the substrate; verify the chain".to_string()
            }
            Self::NoAnchorVerifyInMode { .. } => {
                "drop --no-anchor-verify or re-run with --mode dev".to_string()
            }
            Self::InvalidRoleSlug { .. } => {
                "role slugs must be non-empty and contain no whitespace".to_string()
            }
            Self::InvalidTtlHops { .. } => {
                "--ttl-hops must be in the inclusive range 1..=8 (RFC-0871 ceiling); substrate further clamps to per-node-type ceiling from RouterAnnouncePayload".to_string()
            }
            Self::MeshCapabilityInsufficient { .. } => {
                "the envelope must carry an Authorization::Capability with Audience caveat bound to the target peer DID (RFC-0957 §Attenuation Invariant)".to_string()
            }
            Self::EnvelopeAuthorizationFailed { .. } => {
                "verify the envelope signature against the current verifying key, the audience caveat matches the target peer DID, and the envelope has not expired".to_string()
            }
            Self::RpcTimeout { peer, method, timeout_ms } => {
                format!(
                    "the peer `{peer}` did not reply within {timeout_ms}ms for method `{method}`; verify peer reachability + RFC-0871 envelope transport + consider raising the timeout via --rpc-timeout-ms"
                )
            }
            Self::InvalidEndpointScheme { .. } => {
                "endpoint URI scheme must be one of tcp://, quic://, bluetooth://".to_string()
            }
            Self::VaultNotOwned(_) => {
                "the requested vault is not owned by the active DID; verify the vault_id".to_string()
            }
            Self::InsufficientBalance { .. } => {
                "top up the source vault or lower --amount; re-check with `octo vault balance <id>`"
                    .to_string()
            }
            Self::RoleNotProvisioned => {
                "provision a transfer capability for the active DID (`octo role select`); see RFC-0011-d"
                    .to_string()
            }
            Self::ChainIdMismatch { .. } => {
                "pass --dest-chain-id to confirm a cross-chain transfer, or target a vault on the source chain"
                    .to_string()
            }
            Self::InvalidChainId { .. } => {
                "chain IDs use the RFC-0010 canonical form (64-char lowercase hex)".to_string()
            }
            Self::NoOctoHome => {
                "set $OCTO_HOME (preferred) or $HOME before invoking this command; the CLI does not fall back to /tmp/.octo on shared hosts".to_string()
            }
            Self::ManifestParseError { .. } => {
                "verify the manifest file is valid JSON conforming to the AgentManifest wire form (RFC-0002 §Agent Manifest)".to_string()
            }
            Self::CapabilityValidationFailed(_) => {
                "the manifest failed one of the 6 capability-validation steps (RFC-0002 §Capability Validation); review the substrate diagnostic for the failing step".to_string()
            }
            Self::AgentAlreadyExists(_) => {
                "the derived agent_id (RFC-0011-c §9.10) is already registered; check with `octo agent list` (once that subcommand ships) before re-submitting".to_string()
            }
            Self::AgentNotFound(_) => {
                "the supplied agent_id was not registered to the active DID; check with `octo agent list` for owned agents".to_string()
            }
            Self::ForbiddenHolderMismatch => {
                "the requested holder_did does not match the active operator DID; SECURITY HIGH — multi-DID enumeration is denied at the substrate (RFC-0015 §6.2.1)".to_string()
            }
            Self::InvalidLimit(_) => {
                "pass `--limit` as a positive integer in the range 1..=1024 (the substrate hard ceiling per RFC-0015 §6.2.1)".to_string()
            }
            Self::InvalidCursor(_) => {
                "the supplied cursor is malformed; cursors are opaque tokens reserved for forward-compat pagination (Phase 2)".to_string()
            }
            Self::AlreadyInTransition(_) => {
                "the target agent is currently being transitioned by a concurrent command; wait for the in-flight transition to complete and retry".to_string()
            }
            Self::InvalidStateTransition { from, to } => {
                format!(
                    "the requested state transition `{from} -> {to}` is not a valid edge per RFC-0015-a Appendix A state-machine guard; valid edges are `registered -> running` (run mission) and `running -> terminated` (destroy mission)"
                )
            }
            Self::AuditSubstrateNotReady => {
                "the audit chain sink is not configured; run with --features octo-audit-internal OR ensure the runtime adapter has called `register_audit_sink` at startup".to_string()
            }
            Self::ReceiptNotFound(receipt_id) => {
                format!(
                    "receipt `{receipt_id}` is not present in the receipt store; verify the receipt_id (canonical decimal u64 form) or list receipts with `octo audit list` to find available ids"
                )
            }
            Self::PermissionDenied(path) => {
                format!(
                    "the audit substrate denied access to `{path}`; per-process trust boundary enforced — ensure the receipt store parent directory is owned by the process UID and mode 0700 (RFC-0016-a §Security 3)"
                )
            }
            Self::AuditReadFailed(reason) => {
                format!(
                    "the audit substrate read pipeline failed: `{reason}`; this is a substrate-internal failure distinct from a missing receipt (`ReceiptNotFound`, exit 17) or filter rejection (`InvalidFilter`, exit 16). Check the substrate logs and retry"
                )
            }
            Self::AuditResponseTooLarge { matched, limit } => {
                format!(
                    "the audit list matched {matched} rows, exceeding the substrate hard ceiling of {limit} (RFC-0011-a §Filters `MAX_LIMIT = 10_000` per TV-AUD-4c); narrow the filter (e.g. `--since`, `--until`, `--subject-did`, `--model`, `--status`) and re-invoke — there is no cursor pagination in Phase 1"
                )
            }
            Self::AgentNotRunning(_) => {
                "the target agent exists but is not in `Running` state; run `octo agent run` before `octo agent attach`".to_string()
            }
            Self::RuntimeSubstrateNotReady => {
                "the runtime substrate boundary is reached but the in-process RuntimeHandle mint pathway is not yet wired (per RFC-0011-c §Implementation Phases Phase 1); ship `octo agent run --detach` in a follow-on mission to bind attach to a live spawn".to_string()
            }
            Self::RuntimeAttachFailed { .. } => {
                "the runtime attach call failed at the substrate boundary (handle revoked, channel closed, or invalid attach token); verify the agent is in `Running` state via `octo agent list` and the spawn has not been terminated".to_string()
            }
            Self::SnapshotStale { .. } => {
                "the snapshot is stale (TTL = 600s per RFC-0011-g §Performance Targets); re-run with --force-refresh to bypass the cache".to_string()
            }
            Self::InvalidProposalState { .. } => {
                "use one of the RFC-0011-g §Subcommand Taxonomy labels: `Open`, `Quorum-Reached`, `Closed-Accepted`, `Closed-Rejected`, `Closed-Expired`".to_string()
            }
            Self::GovernanceSubstrateError { .. } => {
                "the governance substrate returned an internal failure (cache or projection surface); check the substrate logs and retry".to_string()
            }
            Self::VoteRejected { reason } => {
                format!(
                    "vote rejected: {reason}; verify the capability token's RFC-0957 caveat set (Audience / Before / Provider) or run `octo governance inspect <cap_id>` for details"
                )
            }
            Self::UnknownAttestationKind { kind_ref } => {
                format!(
                    "unknown attestation kind `{kind_ref}`; the discriminator is not registered. Use `octo governance attest --list-kinds` once Phase 2 substrate lands"
                )
            }
            Self::PrereqNotAccepted { rfc_ref } => {
                format!(
                    "the {rfc_ref} substrate is not Accepted; the `attest` / `vote` primitives are reserved until the prerequisite RFC lands. Check `accepted/` for the current state"
                )
            }
            Self::StaleStub { name, replaced_by } => format!(
                "this command was removed; see the migration notes \
                 (use `{replaced_by}` instead of `{name}`)"
            ),
            Self::Internal(_) => "re-run with `RUST_LOG=debug` and report the diagnostic".to_string(),
            Self::RuntimeSpawnFailed { .. } => {
                "the runtime spawn call failed at the substrate boundary (handle mint error, channel registration failure, or invalid attach handle token); retry; if the failure persists, check that the agent is in a transition-eligible state (`octo agent list`)".to_string()
            }
            Self::AttachHandleExpired { mint_unix, ttl_unix, now_unix } => {
                format!(
                    "the AttachHandle token has expired (mint={mint_unix}, ttl={ttl_unix}, now={now_unix}); mint a fresh token via `octo agent run --detach` and retry the attach"
                )
            }
            Self::AttachHandleBadSignature { .. } => {
                "the AttachHandle signature did not verify against the active holder public key; ensure the token was minted by the active DID and the canonical payload bytes were not tampered with".to_string()
            }
            Self::AttachSessionMismatch { .. } => {
                "the AttachHandle token's session_id does not match the current spawn-time session id for the target agent; mint a fresh token and retry".to_string()
            }
            Self::AttachSessionUnknown(_) => {
                "the AttachHandle references a session_id that is not registered for any active agent; verify the token and the target agent_id".to_string()
            }
            Self::PersistenceError(_) => {
                "the Stoolap-backed event-cursor persistence surface failed; check the substrate logs and verify the runtime persistence feature is enabled when required".to_string()
            }
            Self::RevocationError(_) => {
                "the process-singleton revocation set mutation/lookup failed (RFC-0011-c §F.3); this is a substrate-internal failure distinct from a successful revocation (which is silent)".to_string()
            }
            Self::InvalidSessionIdHex { .. } => {
                "the session id supplied on the CLI is not a 64-char lowercase hex string; verify the input and retry".to_string()
            }
            Self::InvalidSinceCursor { .. } => {
                "the `--since` cursor is below the token's mint timestamp; the token cannot authorize events that pre-date it; mint a fresh token via `octo agent run --detach` and retry".to_string()
            }
            Self::TransportHandlerNotRegistered { kind_label } => {
                format!(
                    "no transport handler is registered for kind `{kind_label}` (RFC-0011-c §F.2 step (e) + per-extension crates registry pattern); extension transports (UnixSocket, …) ship in follow-on Layer D crates (`octo-runtime-transport-unix`, …) that register at process startup"
                )
            }
            Self::TokenMintSkipped { reason } => {
                format!("{reason}; re-run `octo agent run --detach --token-file <path>` on a fresh `Registered → Running` transition, or destroy + recreate the agent first")
            }
            Self::ReplayDetected {
                since_unix,
                recorded_cursor,
            } => {
                format!(
                    "the AttachHandle token has been consumed once already (since cursor {since_unix} is at or behind the recorded cursor {recorded_cursor}); mint a fresh token via `octo agent run --detach` and retry the attach"
                )
            }
            // RFC-0011-h §Error Handling + RFC-0011-i Phase 1 slots 79/83/85/86
            Self::NetworkPeerNotFound { .. } => {
                "verify the gateway_id hex (32 bytes); peer cache lookup returned None; check FederationState membership".to_string()
            }
            Self::NetworkLocalKeyUnavailable => {
                "local gateway identity state is uninitialized; run `octo network bootstrap` to write `<octo_home>/network/local-gateway-identity.toml`".to_string()
            }
            Self::NetworkGraphDepthBelowRange { .. } => {
                "graph depth must be in 1..=100; the CLI clamp protects substrate from OOM".to_string()
            }
            Self::NetworkInvalidDid { .. } => {
                "verify the DID format (104-char hex of 52-byte RawDid per RFC-0010); CLI pre-validates before substrate dispatch".to_string()
            }
            // RFC-0011-j §Error Handling row 82 — TOML parse/write
            // failure for `octo network mode set` + `authority rotate`.
            Self::NetworkConfigParseFailed { .. } => {
                "verify `<octo_home>/network/<file>.toml` exists and is well-formed; the substrate-side BootstrapConfigError is forwarded with operator-safe redaction".to_string()
            }
            // RFC-0011-j §Error Handling row 89 — companion mission
            // closure gating. Pre-companion (G1/G6/G6b/G8 not yet
            // LANDED), CLI dispatch surfaces exit 89.
            Self::NetworkSubstrateUnavailable { companion, detail } => {
                if detail.is_empty() {
                    format!(
                        "the `{companion}` substrate trait is unavailable in this build (either companion mission YAML has not landed, or no per-extension impl crate has been registered); retry after the substrate slice commit lands and a concrete extension impl is registered"
                    )
                } else {
                    format!(
                        "the `{companion}` substrate trait returned an error: {detail}"
                    )
                }
            }
            // RFC-0011-k §Error Handling row 84 — coordinator lookup
            // miss. Substrate returns `Option::None`; CLI translates
            // to a typed exit so operator switch tables can grep on
            // exit code 84.
            Self::NetworkCoordinatorNotFound { .. } => {
                "verify the 52-char hex coordinator_id; substrate `CoordinatorRecord::load` returned None (Phase 6 persistence adapter is the planned follow-on, not yet LANDED)".to_string()
            }
            // RFC-0011-l Phase 4 §Error Handling row 88 — rebind-*
            // interactive dry-run denial. Operator declined at the
            // preview prompt; no substrate call attempted.
            Self::NetworkDryRunDenied { arm, .. } => {
                format!(
                    "rebind `{arm}` aborted at the preview prompt (per RFC-0011-l Phase 4 §Subcommand Taxonomy rebind-* rows); re-run with `--no-dry-run` + `--confirm-acknowledge` (and for `commit` also `--confirm` per the §Security Considerations pastejacking defense) only after the operator is ready to author the state change"
                )
            }
            Self::NetworkKeyRotationUnknownId { .. } => {
                "rotate the holder signing key per RFC-0011-c §F.5.1 paired-acceptance bridge: re-issue the holder signing key, register the new key_id in the verifier KeySet, and re-sign the attach token".to_string()
            }
            // RFC-0011-x §Error Handling: wallet-store amendment
            // chain slots 92/93/94 hints — paired with the
            // substrate `WalletError` envelope so the operator
            // gets an actionable remediation per failure class.
            Self::WalletLocked => {
                // The two forms that actually reach the passphrase. There is no
                // `octo identity unlock` subcommand and no `--passphrase-file`
                // flag on any identity subcommand - both were named here and
                // both fail on the very command that raised this error. The
                // first form is for a non-interactive run; the second is the
                // interactive prompt `acquire_passphrase` falls through to.
                "supply the passphrase on stdin with `--passphrase-stdin --allow-stdin-secret`, or run on a terminal and answer the interactive passphrase prompt; signing operations require the unlocked state (RFC-0011-x §Lifecycle Requirements)".to_string()
            }
            // One hint serves several refusal classes, so it leads
            // with the one whose remedy is NOT an action. When the
            // message names a grace period, the only cure is to
            // wait - and the previous wording invited the operator to
            // "resolve any in-flight rotation" or re-point the
            // wallet, which does not advance the operation and can
            // abort a legitimate rotation.
            Self::IdentityTransitionRefused { .. } => {
                "the requested identity lifecycle transition was refused at the substrate (RFC-0011-x §Lifecycle Requirements). If the message names a grace period, the only remedy is to WAIT for it to elapse - do not abort the rotation and do not re-point the wallet. Otherwise verify the active identity's state machine position with `octo identity list` and resolve any in-flight rotation; if the active identity is not the predecessor, `octo identity select --did <predecessor>` first, then `octo identity rotate-complete` or `octo identity rotate-abort`".to_string()
            }
            Self::DevModeRequired { detail } => detail.to_string(),
            Self::FileInputRejected { detail } => detail.to_string(),
            Self::WeakPassphrase => {
                format!(
                    "the passphrase is below the {}-character floor enforced at both `register` and `unlock` (mission 0011-x-s-a-wallet-store-identity §AC-28); supply a longer passphrase",
                    octo_wallet::error::MIN_PASSPHRASE_CHARS
                )
            }
            Self::InvalidReason { detail } => {
                format!(
                    "the substrate `validate_reason` guard refused the supplied `--reason` ({detail}) per RFC-0015 §6.2.5; reasons must be at most 256 bytes and free of control characters (U+0000-U+001F, U+007F), and the transition did not run"
                )
            }
        };
        Some(h)
    }

    /// Write this error to stderr and terminate the process.
    ///
    /// Render format (RFC-0011 §Error Handling):
    /// ```text
    /// error: <msg>
    ///   caused by: <chain>
    ///   hint: <hint>
    ///   exit code: <N>
    /// ```
    pub fn render(&self, force_json: bool) -> ! {
        let code = self.exit_code();
        let stderr = std::io::stderr();
        let mut w = stderr.lock();
        if force_json {
            let mut sources: Vec<String> = Vec::new();
            let mut src: Option<&dyn std::error::Error> = std::error::Error::source(self);
            while let Some(s) = src {
                sources.push(sanitize_substrate_error(&s.to_string()));
                src = s.source();
            }
            let body = serde_json::json!({
                "schema_version": crate::output::OutputEnvelope::<()>::SCHEMA_VERSION,
                "error": self.user_message(),
                "caused_by": sources,
                "hint": self.hint().map(|h| sanitize_substrate_error(&h)),
                "exit_code": code,
            });
            let _ = writeln!(w, "{body}");
        } else {
            let _ = write!(w, "{}", self.render_block());
        }
        let _ = w.flush();
        std::process::exit(code)
    }

    /// Build the human-readable stderr block [`Self::render`] emits on
    /// the non-JSON branch.
    ///
    /// Split out so the four-line shape has a testable surface.
    /// `render` returns `!` and calls `process::exit`, so a vector
    /// can only call it in a subprocess that kills the test runner.
    /// Before this split, the two vectors that nominally covered the
    /// shape could only assert that the format strings *mention*
    /// `caused by` and `exit code` — and in fact one of them defined
    /// its own private `Wrapper` / `Inner` error types, walked its
    /// own `#[source]` chain with its own loop, and asserted against
    /// those locals, proving nothing about production. `render` writes
    /// this block verbatim, so the strings have one source of truth.
    ///
    /// The line order is the envelope contract: the message, then
    /// zero or more `caused by:` frames in `source()` order, then the
    /// hint if the variant has one, then the exit code last so it is
    /// the final line a script can scrape.
    #[must_use]
    pub fn render_block(&self) -> String {
        let code = self.exit_code();
        let mut out = prefixed_lines("error: ", &self.user_message());
        let mut src: Option<&dyn std::error::Error> = std::error::Error::source(self);
        while let Some(s) = src {
            out.push_str(&prefixed_lines(
                "  caused by: ",
                &sanitize_substrate_error(&s.to_string()),
            ));
            src = s.source();
        }
        if let Some(hint) = self.hint() {
            // Sanitized here, as `user_message` and `caused by:` above
            // already are. `hint` is the ONE operator surface that was
            // emitted raw, and it is the surface that interpolates
            // variant payloads - `NetworkSubstrateUnavailable` renders
            // its `detail` straight into the hint text. Sanitizing at
            // the boundary rather than at each construction site means
            // an arm added later cannot be the one that forgets.
            out.push_str(&prefixed_lines(
                "  hint: ",
                &sanitize_substrate_error(&hint),
            ));
        }
        out.push_str(&format!("  exit code: {code}\n"));
        out
    }
}

/// Emit `text` with `first_prefix` on its opening line and
/// `continuation` on every line after it, each newline-terminated.
///
/// A `Display` impl need not be single-line. `clap::Error`, for one,
/// renders the argument name, the usage line, and a help pointer as
/// separate lines. Prefixing only the first would emit the remainder
/// unindented and unlabelled, so those lines are visually
/// indistinguishable from the lines `render_block` emits itself and a
/// reader cannot tell where the frame ends. Prefixing every line keeps
/// the frame boundary visible; the continuation prefix is two spaces
/// less than the opening one, which reads as a hanging indent.
fn prefixed_lines(first_prefix: &str, text: &str) -> String {
    let continuation = " ".repeat(first_prefix.len().saturating_sub(2));
    let mut out = String::with_capacity(text.len() + first_prefix.len() * 2);
    for (i, line) in text.split('\n').enumerate() {
        out.push_str(if i == 0 { first_prefix } else { &continuation });
        out.push_str(line);
        out.push('\n');
    }
    out
}

/// Gate helper that any future stdin reader calls before consuming pipe data.
///
/// Returns `StdinSecretRefused` (exit 15) unless the operator passed
/// `--allow-stdin-secret`. The flag is currently unused in this RFC's
/// command surface (no command reads stdin), but the gate is wired here so
/// that when a reader is added it can drop in `ensure_stdin_secret_allowed`
/// and inherit the refusal + exit-code contract for free.
pub fn ensure_stdin_secret_allowed(allow: bool) -> Result<(), OctoCliError> {
    if allow {
        Ok(())
    } else {
        Err(OctoCliError::StdinSecretRefused)
    }
}

/// RFC-0016-a §6.7 + RFC-0011-a canonical `[ADD]` error envelope
/// conversion from `octo_audit::AuditError`.
///
/// Manual `impl From` rather than thiserror's `#[from]` attribute
/// at variant level — multiple variants converting from the same
/// source type would create conflicting `From` impls (one per
/// variant). The manual match keeps the substrate → CLI mapping
/// substrate-faithful per RFC-0011-a `ADD` envelope pattern
/// (per-variant mapping; not a catch-all `Internal` wrapper).
///
/// **Defense-in-depth scrub pass (RFC-0016-a §6.8 + R1 reviewer
/// HIGH findings C8 + C9):** the three CLI-shape payload-bearing
/// variants (ReceiptNotFound, InvalidFilter, PermissionDenied)
/// route through `octo_audit::redact_substrate_error` before
/// constructing the CLI envelope. If the substrate payload matches
/// any of the 18 canonical scrubber patterns (hex digest, JWT,
/// WIF, BIP39 mnemonic, capability-secret base64, PEM/PGP/OpenSSH
/// private-key blocks, etc.), the entire payload collapses to the
/// canonical `<REDACTED>` marker. The SinkSpecific arm uses the
/// lighter `sanitize_substrate_error` + `cap_substrate_payload`
/// pipeline (3 string markers in `ERROR_MARKERS` —
/// `SQL:`, `query:`, `sqlite3_open` — plus the `crates/octo-`
/// path prefix; see the `sanitize_substrate_error` impl
/// constants)
/// because its payloads are substrate-internal Stoolap error
/// strings — the full 18-pattern sweep would be
/// over-redaction for that adapter-specific channel. This is the
/// second pass at the CLI boundary; the substrate-side scrubber
/// (defect 1a) is the first pass.
///
/// Mapping table per RFC-0016-a §6.7:
///
/// | Substrate variant                       | CLI variant                              | Exit |
/// | --------------------------------------- | ---------------------------------------- | ---- |
/// | `AuditError::ReceiptNotFound(s)`        | `OctoCliError::ReceiptNotFound(redact)`  | 17   |
/// | `AuditError::InvalidFilter(s)`          | `OctoCliError::InvalidFilter(redact)`    | 16   |
/// | `AuditError::PermissionDenied(s)`       | `OctoCliError::PermissionDenied(redact)` | 13   |
/// | `AuditError::AuditAppendFailed(_)`      | `OctoCliError::AuditSubstrateNotReady`   | 52   |
/// | `AuditError::ChainHashMismatch { .. }`  | `OctoCliError::Internal(reason)`         | 64   |
/// | `AuditError::SequenceGap { .. }`        | `OctoCliError::Internal(reason)`         | 64   |
/// | `AuditError::AlreadyExists(_)`          | `OctoCliError::Internal(reason)`         | 64   |
/// | `AuditError::SinkSpecific(_)`           | `OctoCliError::Internal(reason)`         | 64   |
impl From<octo_audit::AuditError> for OctoCliError {
    fn from(e: octo_audit::AuditError) -> Self {
        // Use a closure to keep the §6.8 redact pass DRY across the
        // 4 payload-bearing variants. The closure is invoked
        // exactly once per arm.
        let redact = |payload: &str| octo_audit::redact_substrate_error(payload);
        match e {
            octo_audit::AuditError::ReceiptNotFound(s) => Self::ReceiptNotFound(redact(&s)),
            octo_audit::AuditError::InvalidFilter(s) => Self::InvalidFilter(redact(&s)),
            octo_audit::AuditError::PermissionDenied(s) => Self::PermissionDenied(redact(&s)),
            // Substrate-faithful: `AuditAppendFailed(reason)` collapses
            // to operator-facing `AuditSubstrateNotReady` (unit variant)
            // — the reason is substrate-internal and intentionally NOT
            // surfaced to the CLI per RFC-0016-a §6.7 table footnote.
            octo_audit::AuditError::AuditAppendFailed(_) => Self::AuditSubstrateNotReady,
            // ChainHashMismatch was collapsed at the substrate layer
            // (R2.5) to `event_id: u64` only — no canonical/supplied
            // digests on the surface. Preserve `event_id` in the CLI
            // reason for parity with the `SequenceGap` / `AlreadyExists`
            // arms (each retains its event_id for diagnostic value);
            // the digest payloads were the original
            // 32-byte-hex-side-channel rationale for collapse.
            octo_audit::AuditError::ChainHashMismatch { event_id } => {
                Self::Internal(sanitize_substrate_error(&cap_substrate_payload(&format!(
                    "audit chain_hash mismatch at event_id {event_id}"
                ))))
            }
            // Original 3 substrate variants map to `Internal` (exit 64)
            // — these are pre-RFC-0016-a substrate-faithful failures
            // that don't have a CLI-shape mapping. The reason string
            // is sanitized at render time by `sanitize_substrate_error`.
            octo_audit::AuditError::SequenceGap { event_id, prev } => {
                Self::Internal(sanitize_substrate_error(&cap_substrate_payload(&format!(
                    "audit sequence gap: event_id {event_id} after {prev}"
                ))))
            }
            octo_audit::AuditError::AlreadyExists(event_id) => {
                Self::Internal(sanitize_substrate_error(&cap_substrate_payload(&format!(
                    "audit event_id {event_id} already persisted"
                ))))
            }
            octo_audit::AuditError::SinkSpecific(msg) => Self::Internal(sanitize_substrate_error(
                &cap_substrate_payload(&format!("audit sink-specific error: {msg}")),
            )),
            // `#[non_exhaustive]` on `AuditError` (RFC-0016-a §6.7
            // substrate column) means downstream exhaustive matches
            // MUST wildcard. Future substrate variants (added by
            // follow-on amendments without a paired CLI-shape
            // mapping) collapse to `Internal(reason)` per the same
            // pattern as `SinkSpecific`. Sanitizer applies the
            // lightweight 3-string-marker pattern plus the
            // `crates/octo-` path prefix (see `sanitize_substrate_error`
            // impl + `ERROR_MARKERS`) so an unknown future variant
            // that accidentally carries a key/path leaks only
            // `<redacted-*>` markers.
            _ => Self::Internal(sanitize_substrate_error(&cap_substrate_payload(&format!(
                "audit substrate error: {e}"
            )))),
        }
    }
}

/// 4 KiB upper bound on substrate error payload size before
/// sanitization (R1 reviewer MED C13 — SinkSpecific payload cap).
/// Substrate `SinkSpecific(msg)` carries unbounded adapter-emitted
/// text (Stoolap transaction diagnostics can include arbitrary
/// SQL fragments + stack frames). Cap before sanitizer so the CLI
/// envelope stays bounded regardless of substrate noise — matches
/// the octo-settlement scrubber's `MAX_INPUT_BYTES = 4 * 1024`.
pub(crate) const SUBSTRATE_PAYLOAD_CAP: usize = 4 * 1024;

/// Cap `s` at [`SUBSTRATE_PAYLOAD_CAP`] bytes (R1 MED C13).
/// Returns the slice with ` [truncated]` appended when oversize so
/// the operator sees that the cap fired (rather than a silent
/// truncation that hides the existence of overflow). Operates on
/// `&str` to avoid forcing an allocation on the bounded path.
pub(crate) fn cap_substrate_payload(s: &str) -> String {
    if s.len() <= SUBSTRATE_PAYLOAD_CAP {
        s.to_string()
    } else {
        // Truncate at the nearest char boundary at-or-before the
        // cap so we never split a UTF-8 codepoint. Append a
        // ` [truncated]` marker so operators can see the cap fired.
        let mut end = SUBSTRATE_PAYLOAD_CAP;
        while end > 0 && !s.is_char_boundary(end) {
            end -= 1;
        }
        let mut out = String::with_capacity(end + 14);
        out.push_str(&s[..end]);
        out.push_str(" [truncated]");
        out
    }
}

/// Strip substrate paths and storage-engine internals from an error string.
///
/// Marker policy (R16 Lens-2 F7): substring containment is too broad —
/// `query:` matches legitimate diagnostic text, `src/` matches URLs and
/// any path containing the directory name. We anchor on full crate paths
/// (`crates/octo-<name>/`) and require a word-boundary on both sides of
/// `SQL:` / `query:` / `sqlite3_open` so they only fire when used as SQL
/// noise markers, not as natural prose.
pub fn sanitize_substrate_error(s: &str) -> String {
    // Anchor SQL/storage markers with word-boundary semantics: the marker
    // must appear at the start, after whitespace, or after a punctuation
    // token. Trailing punctuation (`.`, `,`, `\n`) is also a valid boundary.
    // R17 Lens-2 F2: case-insensitive variants — `SQL:` and `Query:` are
    // both legitimate noise markers a substrate may emit.
    const ERROR_MARKERS: [&str; 3] = ["SQL:", "query:", "sqlite3_open"];
    let mut out = s.to_string();
    for marker in ERROR_MARKERS {
        while let Some(idx) = find_word_boundary_ci(&out, marker) {
            out.replace_range(idx..idx + marker.len(), "<substrate-error>");
        }
    }
    // Anchor path markers to the canonical `crates/octo-<name>/` prefix —
    // this catches `crates/octo-wallet/...`, `crates/octo-cap-macaroon/...`,
    // etc. without matching `src/` substrings in URLs or other contexts.
    const PATH_PREFIX: &str = "crates/octo-";
    while let Some(idx) = out.find(PATH_PREFIX) {
        // Find the end of the path: either a whitespace, closing punctuation,
        // or end-of-string. Stop at the first `)`/`]`/`,` so the
        // diagnostic `path:42:5` is replaced as one block.
        let tail = &out[idx..];
        let end = tail
            .find(char::is_whitespace)
            .or_else(|| tail.find([')', ']', ',']))
            .unwrap_or(out.len() - idx);
        out.replace_range(idx..idx + end, "<substrate-path>");
    }
    out
}

/// Map a substrate HSM error reason to `OctoCliError::HsmUnavailable`
/// (exit 5) with the reason sanitized via `sanitize_substrate_error`.
///
/// Sanitize substrate reason and wrap as `HsmUnavailable`. Centralizes
/// the cross-module mapping per RFC-0011-c §F.6.5.
#[must_use]
pub fn map_hsm_error(reason: &str) -> OctoCliError {
    OctoCliError::HsmUnavailable(sanitize_substrate_error(reason))
}

/// Case-insensitive ASCII word-boundary scan; markers are all ASCII so
/// byte-level ci match suffices (no locale-aware case folding needed).
fn find_word_boundary_ci(s: &str, marker: &str) -> Option<usize> {
    let bytes = s.as_bytes();
    let marker_bytes = marker.as_bytes();
    let marker_len = marker_bytes.len();
    let mut i = 0;
    while i + marker_len <= bytes.len() {
        // Word-boundary check on left.
        let left_ok = i == 0 || !bytes[i - 1].is_ascii_alphanumeric();
        if left_ok {
            // Case-insensitive byte comparison.
            let matches = bytes[i..i + marker_len]
                .iter()
                .zip(marker_bytes.iter())
                .all(|(a, b)| a.eq_ignore_ascii_case(b));
            if matches {
                return Some(i);
            }
        }
        i += 1;
    }
    None
}

/// RFC-0011-c §F.4 + §9.8: `octo_runtime::AttachError` → `OctoCliError`
/// per-variant mapping (slots 53-59 + 61 — excluding the CLI
/// dispatch-side `TokenMintSkipped` slot 60). Substrate owns the
/// canonical distinction; CLI mirrors via per-variant `From` arms
/// so an additive substrate variant lands a corresponding CLI slot
/// without central-enum edits. `#[non_exhaustive]` on both sides —
/// wildcard arms collapse unknown future variants to
/// `Internal(reason)`.
impl From<octo_runtime::AttachError> for OctoCliError {
    fn from(e: octo_runtime::AttachError) -> Self {
        match e {
            octo_runtime::AttachError::Expired {
                session_id: _,
                mint_unix,
                expired_at_unix,
                now_unix,
            } => {
                // Substrate-faithful mirror: the substrate carries
                // the full `{mint, ttl, now}` triplet + the typed
                // `session_id`. The CLI envelope surfaces all four
                // fields so the operator can read the canonical
                // values without re-deriving. `session_id` is
                // intentionally not surfaced on `AttachHandleExpired`
                // (typed-discriminator loss is acceptable here —
                // the TTL envelope is a clock problem, not a session
                // identity problem; the substrate's `Debug` impl on
                // the typed `[u8; 32]` makes the session id
                // recoverable for forensics via `Internal` if
                // needed).
                Self::AttachHandleExpired {
                    mint_unix,
                    ttl_unix: expired_at_unix,
                    now_unix,
                }
            }
            octo_runtime::AttachError::BadSignature { reason } => Self::AttachHandleBadSignature {
                reason: sanitize_substrate_error(&reason),
            },
            octo_runtime::AttachError::SessionMismatch { declared, actual } => {
                Self::AttachSessionMismatch {
                    token_session_hex: hex::encode(declared),
                    registered_session_hex: hex::encode(actual),
                }
            }
            octo_runtime::AttachError::UnknownSession { session_id } => {
                Self::AttachSessionUnknown(hex::encode(session_id))
            }
            octo_runtime::AttachError::PersistenceError(reason) => {
                Self::PersistenceError(sanitize_substrate_error(&reason))
            }
            octo_runtime::AttachError::RevocationError(reason) => {
                Self::RevocationError(sanitize_substrate_error(&reason))
            }
            octo_runtime::AttachError::InvalidSinceCursor {
                mint_unix,
                requested,
            } => Self::InvalidSinceCursor {
                mint_unix,
                requested,
            },
            octo_runtime::AttachError::TransportHandlerNotRegistered { kind_label } => {
                Self::TransportHandlerNotRegistered { kind_label }
            }
            octo_runtime::AttachError::ReplayDetected {
                since_unix,
                recorded_cursor,
            } => Self::ReplayDetected {
                since_unix,
                recorded_cursor,
            },
            // RFC-0011-w: slot 91 activation. Feature-gated to
            // mirror substrate `#[cfg(feature =
            // "octo-attach-key-rotation")]` on
            // `AttachError::UnknownKeyId`. When the feature is
            // disabled, the substrate variant does not exist
            // and the wildcard arm below catches any unknown
            // variant → `Internal(reason)` exit 64
            // (additive-safe per `#[non_exhaustive]`).
            #[cfg(feature = "octo-attach-key-rotation")]
            octo_runtime::AttachError::UnknownKeyId { key_id, known_keys } => {
                Self::NetworkKeyRotationUnknownId {
                    key_id_hex: redact_key_id(&key_id),
                    known_keys_band: KnownKeysBand::from_count(known_keys.len()),
                }
            }
            // Additive-safe wildcard per `#[non_exhaustive]` on both
            // enums. Future substrate variants collapse to
            // `Internal(reason)` exit 64 — same pattern as the audit
            // `From` impl above.
            _ => Self::Internal(sanitize_substrate_error(&cap_substrate_payload(&format!(
                "attach substrate error: {e}"
            )))),
        }
    }
}

/// Redact the substrate `KeyId = u32` discriminator to 8 hex
/// chars via `hex::encode(key_id.to_be_bytes())` per
/// RFC-0011-h §Redaction Layer. Mirrors the existing
/// `hex::encode(declared)` / `hex::encode(actual)` pattern in
/// the `SessionMismatch` translation arm (which hex-encodes
/// the 32-byte `SessionId`); per RFC-0011-c §F.5.1 the
/// substrate `KeyId` has different byte width but the same
/// RFC-0008 §Deterministic Encoding contract applies
/// (big-endian byte order). Feature-gated to mirror the
/// `AttachError::UnknownKeyId` translation arm: only callable
/// when the `octo-attach-key-rotation` Cargo feature is
/// enabled.
#[cfg(feature = "octo-attach-key-rotation")]
fn redact_key_id(key_id: &octo_runtime::handle::KeyId) -> String {
    hex::encode(key_id.to_be_bytes())
}

/// RFC-0011-x §Error Handling: `octo_wallet::WalletError` →
/// `OctoCliError` per-variant mapping. Substrate-faithful mirror
/// per [[cipherocto-design-principles]] §Extension over
/// enumeration; `#[non_exhaustive]` on `WalletError` keeps the
/// match additive-safe — future substrate variants collapse to
/// the wildcard arm and surface as `Internal(reason)` exit 64.
///
/// Per-variant mapping:
///
/// | Substrate variant                                       | CLI variant                                            | Exit |
/// | ------------------------------------------------------ | ------------------------------------------------------ | ---- |
/// | `WalletError::Locked`                                  | `OctoCliError::WalletLocked`                           | 92   |
/// | `WalletError::IdentityNotFound(did)`                   | `OctoCliError::IdentityNotFound(did.to_string())`      | 4    |
/// | `WalletError::WeakPassphrase`                          | `OctoCliError::WeakPassphrase`                         | 2    |
/// | `WalletError::AlreadyRevoked`                          | `OctoCliError::AlreadyRevoked`                         | 6    |
/// | `WalletError::Hsm(reason)`                             | `OctoCliError::HsmUnavailable(sanitize_substrate_error(reason))` | 5    |
/// | `WalletError::VaultSlotNotFound`                       | `OctoCliError::WalletLocked`                           | 92   |
/// | `WalletError::VaultDecryptionFailed`                   | `OctoCliError::WalletLocked`                           | 92   |
/// | `WalletError::NotActive { current_state: Revoked }`   | `OctoCliError::AlreadyRevoked`                         | 6    |
/// | `WalletError::NotActive { current_state: Rotating }`  | `OctoCliError::AlreadyRotating`                        | 3    |
/// | `WalletError::NotActive { current_state: Active / Designated }` | `OctoCliError::NoActiveIdentity`               | 2    |
/// | `WalletError::RotationInProgress`                      | `OctoCliError::IdentityTransitionRefused { reason }`   | 43   |
/// | `WalletError::NotRotating { .. }`                      | `OctoCliError::IdentityTransitionRefused { reason }`   | 43   |
/// | `WalletError::RotationEventMissing`                    | `OctoCliError::IdentityTransitionRefused { reason }`   | 43   |
/// | `WalletError::SuccessorKeyMismatch { .. }`              | `OctoCliError::IdentityTransitionRefused { reason }`   | 43   |
/// | `WalletError::SelfRotation`                            | `OctoCliError::IdentityTransitionRefused { reason }`   | 43   |
/// | `WalletError::GracePeriodNotElapsed { .. }`            | `OctoCliError::IdentityTransitionRefused { reason }`   | 43   |
/// | `WalletError::InvalidSuccessorProof`                   | `OctoCliError::IdentityTransitionRefused { reason }`   | 43   |
/// | `WalletError::InvalidRevocationProof`                  | `OctoCliError::IdentityTransitionRefused { reason }`   | 43   |
/// | `WalletError::IndexCorrupt { detail }`                  | `OctoCliError::Internal(detail)` (via this impl when raised by `unlock`, via `map_wallet_open_error` when raised by `open`) | 64   |
/// | `WalletError::ReasonContainsControlChars(<U+XXXX>)`      | `OctoCliError::InvalidReason { detail }`              | 2    |
/// | `WalletError::ReasonTooLong(len)`                        | `OctoCliError::InvalidReason { detail }`              | 2    |
/// | (all other substrate variants)                         | `OctoCliError::Internal(sanitize_substrate_error(...))`| 64   |
///
/// Defense-in-depth scrub pass: every arm that carries a substrate
/// string routes it through `sanitize_substrate_error` before it
/// reaches the CLI envelope, so an accidental substrate leak (path /
/// SQL marker / crate path prefix) collapses to the
/// `<substrate-error>` / `<substrate-path>` markers at the CLI
/// boundary. Those arms are the `IdentityTransitionRefused` group,
/// the `HsmUnavailable` arm, the `InvalidReason` arms, and the
/// wildcard `Internal` arm.
///
/// The arms are enumerated by property ("carries a substrate
/// string"), not by a hand-maintained count: an earlier revision of
/// this paragraph said "the three arms in this table" and went stale
/// the moment the `InvalidReason` arms landed, so the prose asserted
/// a set the code no longer matched. A count here is a claim about
/// code that no reviewer re-derives. The remaining arms carry no
/// substrate text at all, so there is nothing for the scrub to catch.
/// Matches the established `From<octo_audit::AuditError>` /
/// `From<octo_runtime::AttachError>` precedent at this layer.
impl From<octo_wallet::WalletError> for OctoCliError {
    fn from(e: octo_wallet::WalletError) -> Self {
        match e {
            octo_wallet::WalletError::Locked => Self::WalletLocked,
            // IdentityNotFound carries a typed `Did` payload per
            // the substrate `WalletError` shape — the CLI
            // `IdentityNotFound(String)` slot already exists at
            // exit 4 (no new slot allocation, per mission AC-7).
            // The DID is rendered as the canonical string form
            // via `Did::to_string()` so the operator sees the
            // RFC-0010 canonical wire format.
            octo_wallet::WalletError::IdentityNotFound(did) => {
                Self::IdentityNotFound(did.to_string())
            }
            octo_wallet::WalletError::WeakPassphrase => Self::WeakPassphrase,
            octo_wallet::WalletError::AlreadyRevoked => Self::AlreadyRevoked,
            // NotActive carries a `current_state` discriminator per
            // substrate shape — each terminal state reuses an existing
            // CLI variant + exit so the operator sees the canonical
            // substrate shape failure message. The discriminator is
            // honored here because the YAML translation table maps
            // each variant to a distinct existing slot (Revoked → 6
            // AlreadyRevoked, Rotating → 3 AlreadyRotating, other → 2
            // NoActiveIdentity). Collapsing all three into a single
            // IdentityTransitionRefused (exit 43) would have made the
            // select-on-revoked path exit 43 instead of 6, which is the
            // exact A17 adversary the §6 wall exists to answer.
            octo_wallet::WalletError::NotActive {
                current_state: octo_wallet::LifecycleState::Revoked,
            } => Self::AlreadyRevoked,
            octo_wallet::WalletError::NotActive {
                current_state: octo_wallet::LifecycleState::Rotating,
            } => Self::AlreadyRotating,
            octo_wallet::WalletError::NotActive { .. } => Self::NoActiveIdentity,
            // Hsm wraps the substrate's typed HsmError; the CLI
            // envelope surfaces the sanitized reason as
            // `HsmUnavailable` (exit 5) per the §New error variants
            // mapping table.
            octo_wallet::WalletError::Hsm(reason) => {
                Self::HsmUnavailable(sanitize_substrate_error(&reason.to_string()))
            }
            // Lifecycle refusal family — every member carries the
            // substrate's `Display` message as the CLI payload.
            // The substrate owns the canonical distinction; the
            // CLI envelope collapses them into one typed variant
            // (per RFC-0011-x §Error Handling amendment-chain
            // shared-slot pattern; operator-unambiguous within the
            // identity command surface).
            octo_wallet::WalletError::RotationInProgress
            | octo_wallet::WalletError::NotRotating { .. }
            | octo_wallet::WalletError::SelfRotation
            | octo_wallet::WalletError::GracePeriodNotElapsed { .. }
            | octo_wallet::WalletError::InvalidSuccessorProof
            | octo_wallet::WalletError::InvalidRevocationProof
            | octo_wallet::WalletError::RotationEventMissing
            | octo_wallet::WalletError::SuccessorKeyMismatch { .. } => {
                Self::IdentityTransitionRefused {
                    reason: sanitize_substrate_error(&e.to_string()),
                }
            }
            // VaultSlotNotFound and VaultDecryptionFailed are
            // both stored in the operator-facing envelope as
            // `WalletLocked` (slot 92, exit 92) per the §New
            // error variants translation table — a missing slot
            // or a wrong passphrase is operator-equivalent to a
            // locked store from the operator's side.
            octo_wallet::WalletError::VaultSlotNotFound(_) => Self::WalletLocked,
            octo_wallet::WalletError::VaultDecryptionFailed => Self::WalletLocked,
            // Defense in depth: the agent dispatch boundary
            // (`map_transition_wallet_error`) is the reachable path
            // for these two, but the same substrate variants can
            // arrive here from any other `WalletError`-consuming
            // handler. Without the arms they would collapse to
            // `Internal` at exit 64 and blame the substrate for an
            // operator-supplied `--reason`. The substrate renders
            // the offending control character in `<U+XXXX>`
            // notation, never the raw bytes, so the payload is safe
            // to carry verbatim.
            octo_wallet::WalletError::ReasonContainsControlChars(code_point) => {
                Self::InvalidReason {
                    detail: sanitize_substrate_error(&format!(
                        "contains control character {code_point}"
                    )),
                }
            }
            octo_wallet::WalletError::ReasonTooLong(len) => Self::InvalidReason {
                detail: sanitize_substrate_error(&format!("is {len} bytes, over the 256-byte cap")),
            },
            // Additive-safe wildcard per `#[non_exhaustive]` on
            // `WalletError`. Future substrate variants collapse
            // to `Internal(reason)` exit 64 — same pattern as
            // the audit / attach `From` impls above.
            _ => Self::Internal(sanitize_substrate_error(&cap_substrate_payload(&format!(
                "wallet substrate error: {e}"
            )))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// R15: every substrate wildcard arm caps its payload.
    ///
    /// The four audit-sink arms capped. The audit / attach / wallet
    /// wildcard arms did not, while the wallet arm's own comment
    /// said it followed "the same pattern as the audit / attach
    /// `From` impls above". Measured before the fix: a substrate
    /// error carrying a 16 KiB message reached the operator
    /// envelope at **16,590 bytes against a 4 KiB cap**.
    ///
    /// The cap exists because an unbounded substrate payload is a
    /// denial-of-surface for whatever renders it, and these three
    /// were the arms exempt from it.
    ///
    /// The assertion is on the length after the `Display` prefix,
    /// because `user_message()` prepends the variant's own
    /// `Display` head before the scrub. Each case is built through
    /// the substrate enum's own constructor, so the value asserted
    /// on is one an operator would actually receive.
    ///
    /// Two of the three cases reach a `_` wildcard today; the audit
    /// impl names every one of its ten `AuditError` variants, so its
    /// wildcard is currently unreachable and the audit case asserts
    /// the `SinkSpecific` named arm instead. It is kept because the
    /// wildcard is the row a future variant lands in, and a
    /// pre-emptive cap on an unreachable arm is what makes the
    /// future variant capped rather than not.
    ///
    /// Six further String-carrying arms (`PersistenceError`,
    /// `RevocationError`, `ReceiptNotFound`, `InvalidFilter`,
    /// `PermissionDenied`, `HsmUnavailable`) are also uncapped.
    /// They predate this work and are reported rather than fixed
    /// here; this vector is scoped to the three wildcards.
    #[test]
    fn r15_every_substrate_wildcard_arm_caps_its_payload() {
        let oversized = "z".repeat(SUBSTRATE_PAYLOAD_CAP * 4);
        let cases: [(&str, OctoCliError); 3] = [
            (
                "audit",
                OctoCliError::from(octo_audit::AuditError::SinkSpecific(oversized.clone())),
            ),
            (
                "attach",
                OctoCliError::from(octo_runtime::AttachError::Internal(oversized.clone())),
            ),
            (
                "wallet",
                OctoCliError::from(octo_wallet::WalletError::Config(oversized.clone())),
            ),
        ];
        let budget = SUBSTRATE_PAYLOAD_CAP + " [truncated]".len() + 64;
        for (name, e) in cases {
            let msg = e.user_message();
            assert!(
                msg.len() <= budget,
                "the {name} substrate wildcard arm must cap at {SUBSTRATE_PAYLOAD_CAP} bytes plus \
                 the marker and a short prefix, got {}",
                msg.len()
            );
            assert!(
                msg.ends_with(" [truncated]"),
                "the {name} arm must show the operator that the cap fired, got tail: {:?}",
                &msg[msg.len().saturating_sub(20)..]
            );
        }
    }

    // R1 MED C13 — cap_substrate_payload boundary tests.
    #[test]
    fn cap_substrate_payload_under_cap_passes_through() {
        let s = "x".repeat(SUBSTRATE_PAYLOAD_CAP);
        let out = cap_substrate_payload(&s);
        assert_eq!(out, s, "under-cap input passes through verbatim");
    }

    #[test]
    fn cap_substrate_payload_over_cap_truncates_with_marker() {
        let s = "y".repeat(SUBSTRATE_PAYLOAD_CAP + 100);
        let out = cap_substrate_payload(&s);
        assert!(
            out.ends_with(" [truncated]"),
            "over-cap output carries [truncated] marker, got tail: {}",
            &out[out.len().saturating_sub(20)..],
        );
        assert!(
            out.len() <= SUBSTRATE_PAYLOAD_CAP + " [truncated]".len(),
            "over-cap output must be <= cap + marker suffix",
        );
    }

    #[test]
    fn cap_substrate_payload_respects_utf8_boundary() {
        // 4 KiB - 1 ASCII bytes + a 2-byte UTF-8 codepoint = 4 KiB + 1
        // bytes total. The cap cuts between the last ASCII byte and
        // the first byte of the multi-byte codepoint — without the
        // boundary-safe backtracking loop the output would slice the
        // codepoint mid-sequence and produce invalid UTF-8.
        let mut s = "z".repeat(SUBSTRATE_PAYLOAD_CAP - 1);
        s.push('ã'); // 2-byte UTF-8 sequence straddling the cap
        let out = cap_substrate_payload(&s);
        // Output MUST be valid UTF-8 (no codepoint sliced).
        assert!(
            std::str::from_utf8(out.as_bytes()).is_ok(),
            "cap output MUST be valid UTF-8",
        );
        // Truncation marker MUST be present (input was 1 byte over cap).
        assert!(
            out.ends_with(" [truncated]"),
            "cap over-cap output MUST carry the [truncated] marker",
        );
        // Preserved prefix MUST hold exactly SUBSTRATE_PAYLOAD_CAP - 1
        // ASCII chars (the multi-byte codepoint was dropped, not split).
        let prefix_len = out
            .strip_suffix(" [truncated]")
            .expect("marker present")
            .len();
        assert_eq!(
            prefix_len,
            SUBSTRATE_PAYLOAD_CAP - 1,
            "preserved prefix MUST be SUBSTRATE_PAYLOAD_CAP - 1 ASCII bytes",
        );
    }
    #[test]
    fn tv_err2_internal_no_substrate_leak() {
        let e = OctoCliError::Internal("SQL: select * from wallet".into());
        // R16 Lens-2 F7: in-place replacement preserves context; the
        // `SQL:` marker is replaced, the surrounding text is retained.
        let msg = e.user_message();
        assert!(msg.contains("<substrate-error>"), "{msg}");
        assert!(!msg.contains("SQL:"), "{msg}");
    }

    /// tv_err3 - the production `render_block` really walks a
    /// `std::error::Error::source()` chain and emits one
    /// `caused by:` line per frame.
    ///
    /// The previous revision of this vector declared its own private
    /// `Wrapper` / `Inner` `#[derive(Error)]` types inside the test
    /// body, walked THAT chain with a locally written `while let`
    /// loop, and asserted that its own local `Display` strings
    /// appeared in its own local `Vec`. Production `render` was
    /// invoked exactly once, and only to check that a string the
    /// test had just formatted into the error survived a copy. The
    /// vector could not fail for any change to the render path -
    /// the precise defect class this review round exists to close.
    ///
    /// `ClapParse(#[from] clap::Error)` is the only variant carrying
    /// a `#[source]`, and a `clap::Error` does have its own source,
    /// so it is the one production path that exercises the walk. The
    /// assertions below read `render_block` - the same string
    /// `render` writes - and count the frames.
    #[test]
    fn tv_err3_source_chain_rendered() {
        // A real clap parse failure, not a hand-built stand-in.
        let parse = clap::Command::new("probe")
            .arg(clap::Arg::new("required").required(true))
            .try_get_matches_from(["probe"]);
        let clap_err = parse.expect_err("a missing required arg must not parse");
        let e: OctoCliError = clap_err.into();
        assert!(
            matches!(e, OctoCliError::ClapParse(_)),
            "a clap parse failure must become OctoCliError::ClapParse: {e:?}"
        );

        let block = e.render_block();
        assert!(
            block.starts_with("error: "),
            "the block must open with the error line: {block}"
        );
        assert!(
            block.contains("\n  caused by: "),
            "render_block must emit a caused-by line for the clap source: {block}"
        );
        // The scrub applies to the source frames exactly as it does
        // to the top-level message, so a substrate-shaped source
        // string cannot leak through the walk.
        let frames = block
            .lines()
            .filter(|l| l.trim_start().starts_with("caused by:"))
            .count();
        assert!(
            frames >= 1,
            "expected at least one source frame from clap::Error, got: {block}"
        );
        assert!(
            block.trim_end().ends_with("exit code: 2"),
            "the exit code must be the final line: {block}"
        );
    }

    /// tv_err3b - unchanged: the stdin-secret refusal message names
    /// the override flag and the pipe.
    #[test]
    fn tv_err3b_stdin_secret_refused_message_text() {
        let e = OctoCliError::StdinSecretRefused;
        let rendered = format!("{e}");
        assert!(
            rendered.contains("--allow-stdin-secret"),
            "rendered must mention the override flag: {rendered}"
        );
        assert!(
            rendered.contains("pipe"),
            "rendered must mention pipe: {rendered}"
        );
    }

    #[test]
    fn tv_err3c_stdin_gate_blocks_without_flag() {
        assert!(matches!(
            ensure_stdin_secret_allowed(false),
            Err(OctoCliError::StdinSecretRefused)
        ));
        assert!(ensure_stdin_secret_allowed(true).is_ok());
    }

    #[test]
    fn tv_err3d_render_emits_four_lines() {
        // The block is three lines for a source-less error and four
        // once a `caused by:` frame is present, so assert both shapes
        // against `render_block` - the exact string `render` writes.
        // The previous revision asserted only `hint().is_some()` and
        // `exit_code() == 4`: two facts about the accessors, neither
        // about what the operator sees, and the doc comment conceded
        // it could not capture stderr. `render_block` makes that
        // concession unnecessary.
        let e = OctoCliError::IdentityNotFound("alice".into());
        let block = e.render_block();
        let lines: Vec<&str> = block.lines().collect();
        assert_eq!(
            lines.len(),
            3,
            "a source-less error renders message + hint + exit code, got {}: {block}",
            lines.len()
        );
        assert!(lines[0].starts_with("error: "), "line 0: {block}");
        assert!(
            lines[1].starts_with("  hint: "),
            "line 1 must be the hint: {block}"
        );
        assert_eq!(lines[2], "  exit code: 4", "last line: {block}");

        // Every variant carries a hint - `hint()` is total, so the
        // hint-less branch in `render_block` is unreachable from the
        // current enum. It is kept because `#[non_exhaustive]` admits
        // future variants; a vector cannot pin a branch that has no
        // input, and asserting one would be another vector that
        // cannot fail.

        // Four lines is the maximal shape: a variant with a source
        // frame and a hint. `ClapParse` carries both.
        let parse = clap::Command::new("probe")
            .arg(clap::Arg::new("required").required(true))
            .try_get_matches_from(["probe"]);
        let with_source: OctoCliError = parse.expect_err("missing required arg").into();
        let four: Vec<String> = with_source
            .render_block()
            .lines()
            .map(str::to_string)
            .collect();
        assert!(
            four.len() >= 4,
            "a source-bearing error must add a caused-by frame: {four:?}"
        );
        // Every emitted line belongs to a labelled field. This is
        // the assertion the old code shape could not make: a
        // multi-line Display used to emit its continuation lines
        // unindented, so they were indistinguishable from lines
        // render_block emits itself.
        for (i, line) in four.iter().enumerate() {
            let labelled = ["error: ", "  caused by: ", "  hint: ", "  exit code: "]
                .iter()
                .any(|p| line.starts_with(p));
            assert!(
                labelled || line.starts_with("  "),
                "line {i} must open with a field label or hang under one, found                  a bare line: {line:?} in {four:?}"
            );
        }
        assert_eq!(
            four.iter()
                .filter(|l| l.starts_with("  caused by: "))
                .count(),
            1,
            "exactly one caused-by frame must be labelled: {four:?}"
        );
        let frame_start = four
            .iter()
            .position(|l| l.starts_with("  caused by: "))
            .expect("a labelled caused-by frame");
        let hint_at = four
            .iter()
            .position(|l| l.starts_with("  hint: "))
            .expect("the hint is the field before the exit code");
        assert!(
            frame_start < hint_at,
            "the caused-by frame must sit between the message and the hint: {four:?}"
        );
        // The frame's continuation lines carry the hanging indent,
        // not the opening label, so the frame boundary stays legible.
        assert!(
            four[frame_start + 1].starts_with("           ")
                && !four[frame_start + 1].contains("caused by:"),
            "frame continuation lines must hang under the label: {four:?}"
        );
    }

    #[test]
    fn tv_err4_exit_code_mapping() {
        let cases: Vec<(OctoCliError, i32)> = vec![
            (OctoCliError::NoActiveIdentity, 2),
            (
                OctoCliError::ConfirmationRequired {
                    command: "x".into(),
                },
                2,
            ),
            (OctoCliError::AlreadyRotating, 3),
            (OctoCliError::IdentityNotFound("d".into()), 4),
            (OctoCliError::HsmUnavailable("h".into()), 5),
            (OctoCliError::AlreadyRevoked, 6),
            (
                OctoCliError::CaveatParse {
                    message: "m".into(),
                },
                7,
            ),
            (
                OctoCliError::InvalidCaveatCombination { detail: "d".into() },
                8,
            ),
            (OctoCliError::HolderNotFound("h".into()), 9),
            (OctoCliError::AttenuationViolation("a".into()), 10),
            (OctoCliError::SigningFailed("s".into()), 11),
            (OctoCliError::ParentCapNotFound("p".into()), 12),
            (OctoCliError::PolicyNotFound("p".into()), 13),
            (
                OctoCliError::PolicyVersionNotFound {
                    policy: "p".into(),
                    version: 1,
                },
                14,
            ),
            (OctoCliError::StdinSecretRefused, 15),
            (OctoCliError::InvalidFilter("f".into()), 16),
            (OctoCliError::RoleNotFound("x".into()), 31),
            (
                OctoCliError::StakeInsufficient {
                    required: 0,
                    available: 0,
                },
                32,
            ),
            (
                OctoCliError::RoleNotSelectable {
                    role_id: "x".into(),
                    reason: "y".into(),
                },
                33,
            ),
            (
                OctoCliError::SignerMismatch {
                    signer_did: "a".into(),
                    operator_did: "b".into(),
                },
                35,
            ),
            (
                OctoCliError::AuditorDenied {
                    command: "x".into(),
                },
                2,
            ),
            (OctoCliError::InvalidTtlHops { hops: 9 }, 17),
            (
                OctoCliError::MeshCapabilityInsufficient {
                    detail: "no capability bound to forward envelope".into(),
                },
                18,
            ),
            (
                OctoCliError::EnvelopeAuthorizationFailed {
                    detail: "audience mismatch".into(),
                },
                19,
            ),
            (
                OctoCliError::RpcTimeout {
                    peer: "did:octo:zPeer".into(),
                    method: "quota.drain_queue".into(),
                    timeout_ms: 30_000,
                },
                20,
            ),
            (
                OctoCliError::InvalidEndpointScheme {
                    scheme: "file".into(),
                },
                28,
            ),
            (OctoCliError::Internal("i".into()), 64),
            (
                OctoCliError::StaleStub {
                    name: "init".into(),
                    replaced_by: "octo-wallet init",
                },
                65,
            ),
            (OctoCliError::NoOctoHome, 27),
            (
                OctoCliError::ManifestParseError {
                    path: "p".into(),
                    reason: "r".into(),
                },
                39,
            ),
            (OctoCliError::CapabilityValidationFailed(2), 40),
            (OctoCliError::AgentAlreadyExists(uuid::Uuid::nil()), 41),
            // RFC-0016-a §6.7 paired-with-RFC-0011-a CLI-shape variants.
            (OctoCliError::ReceiptNotFound("12345".into()), 17),
            (
                OctoCliError::PermissionDenied("/var/lib/octo/audit/receipts".into()),
                13,
            ),
            // RFC-0011-a §Error Handling: audit read-path slots
            // (18/19) — first occupants of the parent-reserved 17-63
            // range after `ReceiptNotFound` (17). `AuditReadFailed`
            // covers substrate-fallback failures distinct from
            // `ReceiptNotFound` / `InvalidFilter` / `PermissionDenied`;
            // `AuditResponseTooLarge` surfaces when the substrate
            // rejects a list call that would exceed `MAX_LIMIT`.
            (OctoCliError::AuditReadFailed("sink down".into()), 18),
            (
                OctoCliError::AuditResponseTooLarge {
                    matched: 12_345,
                    limit: 10_000,
                },
                19,
            ),
            // RFC-0011-c §9.8 + RFC-0015-a §6.3: agent amendment chain
            // attach-path slots (48/49/51) — paired with the substrate
            // `octo_runtime::RuntimeError::exit_code` table so the CLI
            // boundary and the substrate boundary agree on slot
            // allocation.
            (OctoCliError::AgentNotRunning(uuid::Uuid::nil()), 48),
            (OctoCliError::RuntimeAttachFailed { reason: "x".into() }, 49),
            (OctoCliError::RuntimeSubstrateNotReady, 51),
            // RFC-0011-g §Error Handling: governance snapshot slots
            // (35 / 2 / 51) — paired with the substrate
            // `octo_governance::GovernanceSnapshotError::exit_code`
            // table so the CLI boundary and the substrate boundary
            // agree on slot allocation. Exit 35 = stale snapshot
            // (passes `--force-refresh`); exit 2 = unrecognized
            // RFC-0011-g proposal-state label; exit 51 = substrate
            // (cache / chain) error.
            (
                OctoCliError::SnapshotStale {
                    snapshot_id_hex: "00".repeat(32),
                    age_secs: 42,
                },
                35,
            ),
            (
                OctoCliError::InvalidProposalState {
                    state: "Bogus".to_string(),
                },
                2,
            ),
            (
                OctoCliError::GovernanceSubstrateError {
                    reason: "cache miss race".to_string(),
                },
                51,
            ),
            // RFC-0011-g §Error Handling: Phase 2 attest + vote slots
            // (36/37/38) — paired with the substrate
            // `octo_governance::GovernanceError` variants once they
            // land (RFC-0011-g §Substrate ADD). Slot arithmetic:
            // 36 = VoteRejected, 37 = UnknownAttestationKind,
            // 38 = PrereqNotAccepted.
            (
                OctoCliError::VoteRejected {
                    reason: "audience caveat not satisfied".to_string(),
                },
                36,
            ),
            (
                OctoCliError::UnknownAttestationKind {
                    kind_ref: "did:octo:attest:novel-claim/v1".to_string(),
                },
                37,
            ),
            (
                OctoCliError::PrereqNotAccepted {
                    rfc_ref: "RFC-0855p-d".to_string(),
                },
                38,
            ),
            // RFC-0011-x §Error Handling: wallet-store amendment
            // chain slots 92/93/94. The 3 new variants land at
            // the end of the `OctoCliError` enum after the slot 91
            // rotation activation. Exit 92 = WalletLocked, exit
            // 43 = IdentityTransitionRefused (shared with agent
            // amendment chain write-path slots per the established
            // shared-slot pattern), exit 2 = WeakPassphrase
            // (shared with clap parse / NoActiveIdentity /
            // ConfirmationRequired / AuditorDenied per the
            // operator-input validation failure shared-slot
            // pattern).
            (OctoCliError::WalletLocked, 92),
            (
                OctoCliError::IdentityTransitionRefused {
                    reason: "identity not active (state: Designated)".to_string(),
                },
                43,
            ),
            (OctoCliError::WeakPassphrase, 2),
        ];
        for (e, code) in cases {
            assert_eq!(e.exit_code(), code, "{e:?}");
        }
        // ClapParse is constructed separately (24th variant).
        let clap_err = clap::Error::new(clap::error::ErrorKind::InvalidValue);
        assert_eq!(OctoCliError::ClapParse(clap_err).exit_code(), 2);
    }

    #[test]
    fn tv_err5_no_substrate_internals() {
        let cases = [
            // `crates/octo-wallet/...` → redacted (canonical path prefix).
            OctoCliError::Internal("failed at crates/octo-wallet/src/store.rs:42".into()),
            // `src/identity.rs` is NOT redacted under R16 Lens-2 F7
            // (substring `src/` was too broad — matches URLs etc.).
            OctoCliError::IdentityNotFound("src/identity.rs".into()),
            // `query: SELECT 1` → word-boundary match on `query:` redacts.
            OctoCliError::HsmUnavailable("query: SELECT 1".into()),
        ];
        for e in cases {
            let msg = e.user_message();
            assert!(!msg.contains("crates/octo-"), "{msg}");
            // `src/` substring intentionally not stripped (R16 Lens-2 F7).
            assert!(!msg.contains("SQL:"), "{msg}");
            assert!(!msg.contains("query:"), "{msg}");
        }
    }

    /// RFC-0011-f §Error Handling: mesh forward error variants must
    /// carry their assigned exit codes (17/18/19) and render a
    /// remediation hint. Pins the exit-code table reserved by the
    /// amendment-chain slot allocation.
    #[test]
    fn tv_mesh_forward_exit_codes_and_hints() {
        let ttl = OctoCliError::InvalidTtlHops { hops: 9 };
        assert_eq!(ttl.exit_code(), 17);
        let hint = ttl.hint().expect("hint required");
        assert!(
            hint.contains("1..=8"),
            "TTL hint must cite the inclusive range, got: {hint}"
        );

        let cap = OctoCliError::MeshCapabilityInsufficient {
            detail: "envelope has Authorization::Signature only".into(),
        };
        assert_eq!(cap.exit_code(), 18);
        let hint = cap.hint().expect("hint required");
        assert!(
            hint.contains("Audience"),
            "capability hint must mention Audience caveat, got: {hint}"
        );

        let auth = OctoCliError::EnvelopeAuthorizationFailed {
            detail: "audience mismatch: expected peer_a, got peer_b".into(),
        };
        assert_eq!(auth.exit_code(), 19);
        let hint = auth.hint().expect("hint required");
        assert!(
            hint.contains("audience") || hint.contains("signature"),
            "auth hint must mention audience or signature, got: {hint}"
        );

        let timeout = OctoCliError::RpcTimeout {
            peer: "did:octo:zPeer".into(),
            method: "quota.drain_queue".into(),
            timeout_ms: 30_000,
        };
        assert_eq!(timeout.exit_code(), 20);
        let timeout_hint = timeout.hint().expect("hint required");
        assert!(
            timeout_hint.contains("30000") || timeout_hint.contains("timeout"),
            "RpcTimeout hint must mention timeout / ceiling, got: {timeout_hint}"
        );
    }

    /// RFC-0016-a §6.7 + RFC-0011-a `ADD` envelope conversion:
    /// `octo_audit::AuditError` → `OctoCliError` per-variant mapping.
    /// Pins the substrate → CLI envelope so a future substrate variant
    /// addition lands a corresponding `match` arm or compile fails
    /// here — `AuditError` IS `#[non_exhaustive]` at the substrate
    /// layer (octo-audit-core Layer A frozen contract per
    /// CLAUDE.md §Architectural Principles), so additive substrate
    /// variants collapse to the wildcard `_` arm and surface as
    /// `Internal(reason)` exit code 64. Per-variant mapping below is
    /// exhaustive TODAY (8 variants listed) but additive-safe.
    #[test]
    fn tv_rfc0016a_audit_error_envelope_mapping() {
        // ReceiptNotFound (exit 17)
        let r: OctoCliError = octo_audit::AuditError::ReceiptNotFound("12345".into()).into();
        assert!(matches!(r, OctoCliError::ReceiptNotFound(ref s) if s == "12345"));
        assert_eq!(r.exit_code(), 17);

        // InvalidFilter (exit 16)
        let r: OctoCliError =
            octo_audit::AuditError::InvalidFilter("limit out of range".into()).into();
        assert!(matches!(r, OctoCliError::InvalidFilter(ref s) if s == "limit out of range"));
        assert_eq!(r.exit_code(), 16);

        // PermissionDenied (exit 13). Substrate payload
        // `/var/lib/octo/audit` matches Pattern 8 (absolute path)
        // so the defense-in-depth §6.8 scrub pass collapses the
        // payload to the canonical `<REDACTED>` marker per the
        // R1 reviewer HIGH finding C9 fix. CLI envelope carries
        // the marker (not the raw path) so the operator gets an
        // explicit substrate-shape signal without leaking the
        // audit home directory into log streams.
        let r: OctoCliError =
            octo_audit::AuditError::PermissionDenied("/var/lib/octo/audit".into()).into();
        assert!(
            matches!(r, OctoCliError::PermissionDenied(ref s) if s == "<REDACTED>"),
            "PermissionDenied path payload MUST be redacted at the CLI boundary (R1 C9), got {r:?}"
        );
        assert_eq!(r.exit_code(), 13);

        // AuditAppendFailed (exit 52; reason dropped per §6.7 footnote)
        let r: OctoCliError =
            octo_audit::AuditError::AuditAppendFailed("internal reason".into()).into();
        assert!(matches!(r, OctoCliError::AuditSubstrateNotReady));
        assert_eq!(r.exit_code(), 52);

        // SequenceGap (exit 64 via Internal)
        let r: OctoCliError = octo_audit::AuditError::SequenceGap {
            event_id: 5,
            prev: 3,
        }
        .into();
        assert!(matches!(r, OctoCliError::Internal(_)));
        assert_eq!(r.exit_code(), 64);

        // AlreadyExists (exit 64 via Internal)
        let r: OctoCliError = octo_audit::AuditError::AlreadyExists(42).into();
        assert!(matches!(r, OctoCliError::Internal(_)));
        assert_eq!(r.exit_code(), 64);

        // SinkSpecific (exit 64 via Internal)
        let r: OctoCliError = octo_audit::AuditError::SinkSpecific("adapter down".into()).into();
        assert!(matches!(r, OctoCliError::Internal(_)));
        assert_eq!(r.exit_code(), 64);

        // ChainHashMismatch (exit 64 via Internal — RFC-0016-a §6.7
        // 8th collapse-group variant; canonical-error mapping per
        // RFC-0016-a §6.7 + sanitize_substrate_error scrub at the
        // CLI boundary).
        let r: OctoCliError = octo_audit::AuditError::ChainHashMismatch { event_id: 7 }.into();
        assert!(matches!(r, OctoCliError::Internal(_)));
        assert_eq!(r.exit_code(), 64);
    }

    /// `tv_rfc0011c_attach_error_envelope_mapping` — exercise every
    /// `From<AttachError>` arm (RFC-0011-c §F.2 + §F.4) and assert
    /// the canonical CLI exit code per §9.8 slot allocation.
    ///
    /// Pattern analog to `tv_rfc0016a_audit_error_envelope_mapping`
    /// above; substrate-faithful mirror per [[memory-is-never-status-
    /// ground-truth]] (the assertion is the contract, not the doc).
    #[test]
    fn tv_rfc0011c_attach_error_envelope_mapping() {
        use octo_runtime::AttachError;

        // Expired → AttachHandleExpired (exit 53, §9.8 slot 53)
        let sid = [0xabu8; 32];
        let r: OctoCliError = AttachError::Expired {
            session_id: sid,
            mint_unix: 100,
            expired_at_unix: 200,
            now_unix: 300,
        }
        .into();
        assert!(
            matches!(
                r,
                OctoCliError::AttachHandleExpired {
                    mint_unix: 100,
                    ttl_unix: 200,
                    now_unix: 300
                }
            ),
            "Expired MUST map to AttachHandleExpired, got {r:?}"
        );
        assert_eq!(r.exit_code(), 53);

        // BadSignature → AttachHandleBadSignature (exit 54)
        let r: OctoCliError = AttachError::BadSignature {
            reason: "internal sig fail".into(),
        }
        .into();
        assert!(matches!(r, OctoCliError::AttachHandleBadSignature { .. }));
        assert_eq!(r.exit_code(), 54);

        // SessionMismatch → AttachSessionMismatch (exit 55)
        let r: OctoCliError = AttachError::SessionMismatch {
            declared: [0x01; 32],
            actual: [0x02; 32],
        }
        .into();
        assert!(matches!(r, OctoCliError::AttachSessionMismatch { .. }));
        assert_eq!(r.exit_code(), 55);

        // UnknownSession → AttachSessionUnknown (exit 56)
        let r: OctoCliError = AttachError::UnknownSession {
            session_id: [0xee; 32],
        }
        .into();
        assert!(
            matches!(r, OctoCliError::AttachSessionUnknown(ref s) if s == &hex::encode([0xeeu8; 32]))
        );
        assert_eq!(r.exit_code(), 56);

        // PersistenceError → PersistenceError (exit 57)
        let r: OctoCliError = AttachError::PersistenceError("db write failed".into()).into();
        assert!(matches!(r, OctoCliError::PersistenceError(ref s) if s == "db write failed"));
        assert_eq!(r.exit_code(), 57);

        // RevocationError → RevocationError (exit 58)
        let r: OctoCliError = AttachError::RevocationError("session revoked".into()).into();
        assert!(matches!(r, OctoCliError::RevocationError(ref s) if s == "session revoked"));
        assert_eq!(r.exit_code(), 58);

        // InvalidSinceCursor → InvalidSinceCursor (exit 53 shared with
        // AttachHandleExpired per amendment-chain shared-slot pattern)
        let r: OctoCliError = AttachError::InvalidSinceCursor {
            mint_unix: 100,
            requested: 50,
        }
        .into();
        assert!(matches!(
            r,
            OctoCliError::InvalidSinceCursor {
                mint_unix: 100,
                requested: 50
            }
        ));
        assert_eq!(r.exit_code(), 53);

        // TransportHandlerNotRegistered → TransportHandlerNotRegistered
        // (exit 59 per §F.2 step (e) extension-surface slot)
        let r: OctoCliError = AttachError::TransportHandlerNotRegistered {
            kind_label: "UnixSocket".into(),
        }
        .into();
        assert!(
            matches!(r, OctoCliError::TransportHandlerNotRegistered { ref kind_label } if kind_label == "UnixSocket")
        );
        assert_eq!(r.exit_code(), 59);

        // ReplayDetected → ReplayDetected (exit 61 per §9.7
        // follow-on amendment; own slot — distinct from
        // Internal(reason) exit 64)
        let r: OctoCliError = AttachError::ReplayDetected {
            since_unix: 1_000,
            recorded_cursor: 2_000,
        }
        .into();
        assert!(matches!(
            r,
            OctoCliError::ReplayDetected {
                since_unix: 1_000,
                recorded_cursor: 2_000
            }
        ));
        assert_eq!(r.exit_code(), 61);

        // Additive `#[non_exhaustive]` variant collapse path:
        // exercise the wildcard arm via constructing an unknown
        // `AttachError` is impossible (no private fields), so the
        // wildcard is contract-tested by the substrate's `non_exhaustive`
        // attribute alone — not asserted here.
    }

    /// RFC-0011 §Changelog v2.0 entry: `StaleStub` retains its
    /// `replaced_by: &'static str` field so operator switch tables
    /// can grep the outbound JSON envelope for a replacement hint.
    /// Pins the `Display` message (must surface `replaced_by`,
    /// `name`, and the `octo --help` operator pointer) and the
    /// exit-code slot (must stay 65 per RFC-0011 §Exit Code table).
    #[test]
    fn tv_stalestub_v2_replaced_by_display_format() {
        let err = OctoCliError::StaleStub {
            name: "init".into(),
            replaced_by: "octo-wallet init",
        };
        let rendered = err.to_string();
        assert!(
            rendered.contains("`octo-wallet init`"),
            "Display MUST surface `replaced_by` hint, got: {rendered}"
        );
        assert!(
            rendered.contains("`init`"),
            "Display MUST surface the original `name`, got: {rendered}"
        );
        assert!(
            rendered.contains("octo --help"),
            "Display MUST point at `octo --help` for discoverability, got: {rendered}"
        );
        assert_eq!(err.exit_code(), 65, "StaleStub MUST stay exit 65");
    }

    /// RFC-0011 §Changelog v2.0 entry: `StaleStub`'s `replaced_by`
    /// field is the library-API soft sentinel hint (per
    /// [[cipherocto-design-principles]] §Extension over enumeration,
    /// since `OctoCliError` is `#[non_exhaustive]`). Pin both
    /// `user_message()` and `Display` so a future refactor cannot
    /// silently regress the contract.
    #[test]
    fn tv_stalestub_v2_replaced_by_user_message() {
        let err = OctoCliError::StaleStub {
            name: "init".into(),
            replaced_by: "octo-wallet init",
        };
        let rendered = err.user_message();
        assert!(
            rendered.contains("`octo-wallet init`"),
            "user_message MUST surface `replaced_by` hint, got: {rendered}"
        );
        assert!(
            rendered.contains("`init`"),
            "user_message MUST surface the original `name`, got: {rendered}"
        );
    }

    /// RFC-0011-w §Test Vectors `tv_w_1` — variant construction,
    /// `exit_code()` returns 91, and `user_message()` renders the
    /// `#[error]` template with both `key_id_hex` (8 hex chars
    /// via `hex::encode(key_id.to_be_bytes())`) and
    /// `known_keys_band` (Debug-rendered enum tag `None` / `Few`
    /// / `Many`) interpolated. Feature OFF build: variant mints
    /// unconditionally per RFC-0011-w §Motivation so the slot 91
    /// arm is always present in the `#[non_exhaustive]` enum.
    #[test]
    fn tv_w_1_network_key_rotation_unknown_id_exit_91_and_template() {
        let err = OctoCliError::NetworkKeyRotationUnknownId {
            key_id_hex: "deadbeef".to_string(),
            known_keys_band: KnownKeysBand::Few,
        };
        assert_eq!(
            err.exit_code(),
            91,
            "exit_code() MUST return 91 per RFC-0011-w §Exit Codes row 91"
        );
        let msg = err.user_message();
        assert!(
            msg.contains("0xdeadbeef"),
            "user_message MUST interpolate key_id_hex as 8 hex chars, got: {msg}"
        );
        assert!(
            msg.contains("Few"),
            "user_message MUST interpolate known_keys_band enum tag via Debug formatter, got: {msg}"
        );
        // Hint arm contract per R1.B MEDIUM finding closure. The hint
        // arm added at slot 91 per the R1.5 fix sweep prescribes the
        // operational remediation (re-issue the holder signing key).
        // Mirrors the precedent at `tv_mesh_forward_exit_codes_and_hints`
        // which pairs exit-code + hint assertion per variant.
        let hint = err.hint().expect(
            "hint MUST be Some for NetworkKeyRotationUnknownId per RFC-0011-w §Detailed Design",
        );
        assert!(
            hint.contains("rotate the holder signing key"),
            "hint MUST prescribe the holder-signing-key rotation remediation, got: {hint}"
        );
        assert!(
            hint.contains("RFC-0011-c §F.5.1"),
            "hint MUST cite the paired-acceptance bridge section ref per RFC-0011-w §Redaction Layer, got: {hint}"
        );
    }

    /// RFC-0011-w §Test Vectors `tv_w_1b` — `KnownKeysBand::from_count`
    /// boundary coverage per R1.B HIGH finding closure. Exercises the
    /// `None` (0 keys) and `Many` (>8 keys) branches of the 3-band
    /// categorical quantization plus the `8 → Few` vs `9 → Many`
    /// off-by-one boundary that was previously untested.
    /// Feature-gated to mirror the substrate `octo-attach-key-rotation`
    /// feature that gates `from_count` itself.
    #[cfg(feature = "octo-attach-key-rotation")]
    #[test]
    fn tv_w_1b_band_quantization_boundaries() {
        // 0 keys → None (verifier misconfiguration sentinel)
        assert_eq!(
            KnownKeysBand::from_count(0),
            KnownKeysBand::None,
            "from_count(0) MUST yield None per RFC-0011-w §Redaction Layer"
        );
        // 1 key → Few (band lower edge)
        assert_eq!(
            KnownKeysBand::from_count(1),
            KnownKeysBand::Few,
            "from_count(1) MUST yield Few per 1..=8 threshold"
        );
        // 8 keys → Few (band upper edge; off-by-one test for 8 vs 9)
        assert_eq!(
            KnownKeysBand::from_count(8),
            KnownKeysBand::Few,
            "from_count(8) MUST yield Few per 1..=8 threshold (off-by-one sentinel)"
        );
        // 9 keys → Many (band lower edge; off-by-one test for 9 vs 8)
        assert_eq!(
            KnownKeysBand::from_count(9),
            KnownKeysBand::Many,
            "from_count(9) MUST yield Many per >8 threshold (off-by-one sentinel)"
        );
        // 16 keys → Many (interior band value)
        assert_eq!(
            KnownKeysBand::from_count(16),
            KnownKeysBand::Many,
            "from_count(16) MUST yield Many per >8 threshold (interior band value)"
        );
    }

    /// RFC-0011-w §Test Vectors `tv_w_2` — `From<octo_runtime::AttachError>`
    /// translation arm signature: maps
    /// `AttachError::UnknownKeyId { key_id, known_keys }` to
    /// `Self::NetworkKeyRotationUnknownId { key_id_hex: hex::encode(key_id.to_be_bytes()),
    /// known_keys_band: KnownKeysBand::from_count(known_keys.len()) }`.
    /// Feature ON build: the substrate `AttachError::UnknownKeyId`
    /// variant exists per RFC-0011-c §F.5.1 paired-acceptance bridge.
    #[cfg(feature = "octo-attach-key-rotation")]
    #[test]
    fn tv_w_2_attach_unknown_key_id_translates_to_slot_91_variant() {
        let known = vec![1u32, 2, 3, 4];
        let substrate_err = octo_runtime::AttachError::UnknownKeyId {
            key_id: 0xdeadbeef,
            known_keys: known.clone(),
        };
        let cli_err: OctoCliError = substrate_err.into();
        match cli_err {
            OctoCliError::NetworkKeyRotationUnknownId {
                key_id_hex,
                known_keys_band,
            } => {
                assert_eq!(
                    key_id_hex, "deadbeef",
                    "key_id_hex MUST be hex::encode(key_id.to_be_bytes()) per RFC-0011-w §Substrate-faithfulness audit, got: {key_id_hex}"
                );
                assert_eq!(
                    known_keys_band,
                    KnownKeysBand::Few,
                    "known_keys_band MUST match KnownKeysBand::from_count(known_keys.len()) per Vec::len bucketing thresholds (1..=8 → Few)"
                );
            }
            other => {
                panic!("translation arm MUST yield NetworkKeyRotationUnknownId, got: {other:?}")
            }
        }
    }

    // -----------------------------------------------------------------------
    // RFC-0011-x §Test Vectors — the six CLI-observable vectors owned by
    // the CLI mission (mission 0011-x-wallet-store-cli §Test Vectors):
    // tv_x_20, tv_x_29, tv_x_30, tv_x_43, tv_x_44, tv_x_48. These pin
    // the `From<octo_wallet::WalletError> for OctoCliError` translation
    // table and the related exit-code assignments at the unit-test layer
    // so substrate-faithfulness, exit-code sharing, and slot-94 floor
    // rendering all stay visible.
    // -----------------------------------------------------------------------

    /// tv_x_29 — `WalletError::Locked` maps to `OctoCliError::WalletLocked`
    /// at slot 92 with exit 92. No payload — the substrate carries the
    /// no-payload `Locked` form and the CLI mirrors it 1:1.
    #[test]
    fn tv_x_29_locked_maps_to_wallet_locked_slot_92() {
        let e: OctoCliError = octo_wallet::WalletError::Locked.into();
        assert!(
            matches!(e, OctoCliError::WalletLocked),
            "WalletError::Locked must map to OctoCliError::WalletLocked, got: {e:?}"
        );
        assert_eq!(
            e.exit_code(),
            92,
            "WalletLocked is the exit-92 slot per the wallet-store amendment chain"
        );
    }

    /// tv_x_30 — `WalletError::IdentityNotFound(did)` maps to the
    /// **existing** `OctoCliError::IdentityNotFound(String)` at exit 4.
    /// **No new slot minted** — the parent RFC reserved exit 4 for the
    /// "no such identity" case and the variant already exists in the
    /// CLI enum from RFC-0011-b days.
    #[test]
    fn tv_x_30_identity_not_found_maps_to_exit_4_no_new_slot() {
        let did = octo_wallet::Did("did:octo:0xaa".to_string());
        let e: OctoCliError = octo_wallet::WalletError::IdentityNotFound(did).into();
        match &e {
            OctoCliError::IdentityNotFound(s) => {
                assert_eq!(s, "did:octo:0xaa", "DID must round-trip via Did::to_string");
            }
            other => panic!(
                "WalletError::IdentityNotFound must map to the existing IdentityNotFound variant, got: {other:?}"
            ),
        }
        assert_eq!(
            e.exit_code(),
            4,
            "exit 4 was reserved for IdentityNotFound by RFC-0011-b; no new slot is minted"
        );
    }

    /// tv_x_43 — `WalletError::Config` has **no** translation arm in
    /// the general `From<WalletError>` impl, and that is still
    /// right: `Config` means many things across the substrate (a
    /// full disk, an Argon2 parameter failure), so mapping all of it
    /// to 27 would tell the operator to set `$OCTO_HOME` for an
    /// unrelated fault. The wildcard arm routes `Config` to
    /// `Internal(reason)` at exit 64.
    ///
    /// The previous revision of this comment went further and
    /// claimed exit 27 is "produced upstream by `home::resolve`
    /// before any command opens the store". That is true of the rest
    /// of the CLI and FALSE of the five identity subcommands, which
    /// call `WalletStore::open` directly and never reach
    /// `home::resolve` - so on this path exit 27 was unreachable and
    /// a missing `$OCTO_HOME` was reported as an internal error. The
    /// narrow fix is in `map_wallet_open_error`; this impl is
    /// unchanged.
    ///
    /// A second revision of this comment then said that mapper "sees
    /// only the ONE `Config` `open` can raise". That was also false,
    /// and the falsification came from the substrate rather than from
    /// review: `open` raised `Config` for a duplicate-DID index as
    /// well as for home resolution, so a damaged `store.json` was
    /// reported to the operator as a missing environment variable.
    /// The two index faults now raise the typed `IndexCorrupt`, so the
    /// mapper's `Config` arm is narrow in fact as well as in
    /// intention. What makes that claim checkable rather than
    /// hopeful is `tv_x_75` in the identity handler module, which
    /// drives a real duplicate-DID index through the mapper.
    #[test]
    fn tv_x_43_config_has_no_arm_exit_27_comes_from_home_resolve() {
        let e: OctoCliError =
            octo_wallet::WalletError::Config("Argon2id hash mismatch on vault".into()).into();
        match &e {
            OctoCliError::Internal(reason) => {
                // Defense-in-depth scrub: no $OCTO_HOME / $HOME in the
                // rendered text. The mapping arm never gets a chance to
                // emit those, because the arm does not exist.
                assert!(
                    !reason.contains("$OCTO_HOME"),
                    "Config must NOT carry $OCTO_HOME in rendered text: {reason}"
                );
                assert!(
                    !reason.contains("$HOME"),
                    "Config must NOT carry $HOME in rendered text: {reason}"
                );
                // And exit 64 — the catch-all, not the 27 reserved by
                // `home::resolve`.
                assert!(
                    e.exit_code() == 64,
                    "Config falls through to Internal at exit 64 (not 27): {e:?}"
                );
            }
            other => panic!(
                "Config must fall through to Internal at exit 64 (no dedicated arm); got: {other:?}"
            ),
        }
        // Belt-and-braces — assert NoOctoHome stays at exit 27
        // (the slot owned by home::resolve, NOT by Config).
        assert_eq!(OctoCliError::NoOctoHome.exit_code(), 27);
    }

    /// tv_x_44 — the eight `WalletError` lifecycle refusals all map to
    /// `OctoCliError::IdentityTransitionRefused { reason }` at slot 93
    /// with exit 43. The substrate owns the canonical distinction;
    /// the CLI envelope collapses them into one typed variant.
    /// `NotActive { current_state }` is a separate family and maps to
    /// `AlreadyRevoked` (exit 6), `AlreadyRotating` (exit 3), or
    /// `NoActiveIdentity` (exit 2) per the §New error variants translation
    /// table — the field discriminator is honored here. Asserted by
    /// `tv_x_44b_not_active_field_discriminator_respected`.
    ///
    /// The count is asserted against the impl, not just stated. The
    /// family grew by two after this vector was first written and
    /// nothing failed, because a hand-written case list and an
    /// or-pattern are two independent lists of the same set.
    #[test]
    fn tv_x_44_eight_lifecycle_refusals_map_to_slot_93_exit_43() {
        let cases: Vec<octo_wallet::WalletError> = vec![
            octo_wallet::WalletError::RotationInProgress,
            octo_wallet::WalletError::SelfRotation,
            octo_wallet::WalletError::GracePeriodNotElapsed {
                elapsed_secs: 0,
                required_secs: 86_400,
            },
            octo_wallet::WalletError::NotRotating {
                current_state: octo_wallet::LifecycleState::Active,
            },
            octo_wallet::WalletError::InvalidSuccessorProof,
            octo_wallet::WalletError::InvalidRevocationProof,
            // The two the family grew by AFTER this vector was
            // written. Both are in the translation table above and
            // both were missing here, which is why dropping either
            // from the impl's or-pattern left all 553 tests green.
            octo_wallet::WalletError::RotationEventMissing,
            octo_wallet::WalletError::SuccessorKeyMismatch {
                did: octo_wallet::Did("did:octo:mismatched-successor".to_owned()),
            },
        ];
        let case_count = cases.len();
        for substrate_err in cases {
            let substrate_dbg = format!("{:?}", substrate_err);
            let e: OctoCliError = substrate_err.into();
            match &e {
                OctoCliError::IdentityTransitionRefused { reason } => {
                    assert!(
                        !reason.is_empty(),
                        "IdentityTransitionRefused must carry a non-empty reason: {e:?}"
                    );
                }
                other => panic!(
                    "lifecycle refusal must map to IdentityTransitionRefused, got: {other:?} from substrate {substrate_dbg}"
                ),
            }
            assert_eq!(
                e.exit_code(),
                43,
                "IdentityTransitionRefused is exit 43 (shared with the agent amendment chain write-path slots)"
            );
        }

        // A hand-written case list goes stale when the impl's
        // or-pattern grows, and the two halves are the same family,
        // so nothing else connects them. This is the connection: the
        // runtime loop above cannot observe which variants the impl
        // actually routes to exit 43, only that the ones listed do.
        //
        // Source-level because that is what the property IS. The
        // claim is about correspondence between a list of names and a
        // pattern of names, and no runtime path observes a name.
        let src = include_str!("error.rs");
        let family = src
            .split("// Lifecycle refusal family")
            .nth(1)
            .expect("the translation impl must carry a lifecycle refusal family")
            .split("=> {")
            .next()
            .expect("the family must be followed by its arm body");
        let impl_members = family.matches("octo_wallet::WalletError::").count();
        assert_eq!(
            impl_members,
            case_count,
            "the impl's refusal family routes {impl_members} substrate variants to exit 43, and this \
             vector asserts {case_count}. Every member needs a case here, because a member without \
             one is unpinned: deleting it from the or-pattern sends it to the wildcard, where it \
             becomes an internal fault at exit 64 and the operator is told to report a bug for a \
             normal refusal. The families: {family}"
        );
    }

    /// tv_x_44b — the `NotActive { current_state }` field discriminator
    /// is honored at the CLI boundary. `Revoked` reuses the existing
    /// `AlreadyRevoked` slot (exit 6) so `select` on a terminal record
    /// surfaces the canonical substrate-shape failure; `Rotating` reuses
    /// `AlreadyRotating` (exit 3); the bare arm falls through to
    /// `NoActiveIdentity` (exit 2). Collapsing all three into
    /// `IdentityTransitionRefused` (exit 43) would have made the
    /// select-on-revoked path exit 43 — the exact A17 adversary the
    /// §6 wall exists to answer.
    #[test]
    fn tv_x_44b_not_active_field_discriminator_respected() {
        // Revoked -> AlreadyRevoked (exit 6)
        let e: OctoCliError = octo_wallet::WalletError::NotActive {
            current_state: octo_wallet::LifecycleState::Revoked,
        }
        .into();
        assert!(
            matches!(e, OctoCliError::AlreadyRevoked),
            "NotActive {{ current_state: Revoked }} must map to AlreadyRevoked (exit 6), got: {e:?}"
        );
        assert_eq!(e.exit_code(), 6);
        // The same output from the substrate's OWN revoked arm, which
        // is a different input. `NotActive { Revoked }` is what a
        // read or a select raises, `AlreadyRevoked` is what
        // `register` and the lifecycle rehydration raise. They land
        // on one exit for one reason - the identity is terminal - and
        // this arm had no vector at all: deleting it left all 553
        // tests passing, so the two substrate paths to exit 6 were
        // pinned one deep.
        let e: OctoCliError = octo_wallet::WalletError::AlreadyRevoked.into();
        assert!(
            matches!(e, OctoCliError::AlreadyRevoked),
            "the substrate's own AlreadyRevoked must map to AlreadyRevoked (exit 6). Falling \
             through to the wildcard would exit 64 and tell the operator to report a bug for an \
             identity that is terminal by design. Got {e:?}"
        );
        assert_eq!(e.exit_code(), 6);
        // Rotating -> AlreadyRotating (exit 3)
        let e: OctoCliError = octo_wallet::WalletError::NotActive {
            current_state: octo_wallet::LifecycleState::Rotating,
        }
        .into();
        assert!(
            matches!(e, OctoCliError::AlreadyRotating),
            "NotActive {{ current_state: Rotating }} must map to AlreadyRotating (exit 3), got: {e:?}"
        );
        assert_eq!(e.exit_code(), 3);
        // Other (Active / bare) -> NoActiveIdentity (exit 2)
        let e: OctoCliError = octo_wallet::WalletError::NotActive {
            current_state: octo_wallet::LifecycleState::Active,
        }
        .into();
        assert!(
            matches!(e, OctoCliError::NoActiveIdentity),
            "NotActive {{ current_state: Active }} must map to NoActiveIdentity (exit 2), got: {e:?}"
        );
        assert_eq!(e.exit_code(), 2);
        let e: OctoCliError = octo_wallet::WalletError::NotActive {
            current_state: octo_wallet::LifecycleState::Designated,
        }
        .into();
        assert!(
            matches!(e, OctoCliError::NoActiveIdentity),
            "NotActive {{ current_state: Designated }} must map to NoActiveIdentity (exit 2), got: {e:?}"
        );
        assert_eq!(e.exit_code(), 2);
    }

    /// `Hsm(HsmError)` transport failure maps to `HsmUnavailable` (slot 5)
    /// per the §New error variants translation table. The substrate
    /// carries the typed `HsmError` payload; the CLI envelope renders the
    /// sanitized reason.
    #[test]
    fn tv_x_44c_hsm_maps_to_hsm_unavailable() {
        // Build a representative HsmError. Substrate `HsmError` is a
        // typed enum; we exercise one variant that is reachable in
        // production (transport backend unreachable).
        let substrate_err = octo_wallet::WalletError::Hsm(
            octo_wallet::hsm::HsmError::NotConnected("hsm transport down".to_string()),
        );
        let e: OctoCliError = substrate_err.into();
        match &e {
            OctoCliError::HsmUnavailable(reason) => {
                assert!(
                    !reason.is_empty(),
                    "HsmUnavailable must carry a non-empty reason: {e:?}"
                );
            }
            other => panic!("Hsm must map to HsmUnavailable, got: {other:?}"),
        }
        assert_eq!(e.exit_code(), 5);
    }

    /// `VaultSlotNotFound(String)` and `VaultDecryptionFailed` map to
    /// `WalletLocked` (slot 92, exit 92) per the §New error variants
    /// translation table — a missing slot or a wrong passphrase is
    /// operator-equivalent to a locked store from the operator's side.
    #[test]
    fn tv_x_44d_vault_slot_maps_to_wallet_locked() {
        let e: OctoCliError =
            octo_wallet::WalletError::VaultSlotNotFound("missing-slot".to_string()).into();
        assert!(
            matches!(e, OctoCliError::WalletLocked),
            "VaultSlotNotFound must map to WalletLocked (exit 92), got: {e:?}"
        );
        assert_eq!(e.exit_code(), 92);
        let e: OctoCliError = octo_wallet::WalletError::VaultDecryptionFailed.into();
        assert!(
            matches!(e, OctoCliError::WalletLocked),
            "VaultDecryptionFailed must map to WalletLocked (exit 92), got: {e:?}"
        );
        assert_eq!(e.exit_code(), 92);
    }

    /// tv_x_c_37 - F-3. `WalletError::ReasonContainsControlChars` and
    /// `WalletError::ReasonTooLong` are operator-input rejections by
    /// the substrate `validate_reason` guard, raised BEFORE any state
    /// transition runs. Both previously fell through to the
    /// `#[non_exhaustive]` wildcard and rendered as `Internal` at
    /// exit 64, which every consuming handler's exit-code list
    /// documents as "unexpected substrate error" - blaming the
    /// substrate for a value the operator typed. Exit 2 is the
    /// operator-input family (`ClapParse`, `WeakPassphrase`).
    ///
    /// The assertions check the exit code AND that the result is not
    /// `Internal`, so a future re-widening of the wildcard fails
    /// rather than silently restoring the mislabelled exit 64.
    #[test]
    fn tv_x_c_37_reason_rejections_map_to_invalid_reason_exit_2() {
        for (substrate, label) in [
            (
                octo_wallet::WalletError::ReasonContainsControlChars("<U+001B>".to_string()),
                "control-character reason",
            ),
            (
                octo_wallet::WalletError::ReasonTooLong(300),
                "over-length reason",
            ),
        ] {
            let e: OctoCliError = substrate.into();
            assert_eq!(
                e.exit_code(),
                2,
                "{label} must exit 2 (operator input), got {} from {e:?}",
                e.exit_code()
            );
            assert!(
                matches!(e, OctoCliError::InvalidReason { .. }),
                "{label} must map to OctoCliError::InvalidReason, not Internal: {e:?}"
            );
            assert!(
                !matches!(e, OctoCliError::Internal(_)),
                "{label} must not be reported as a substrate failure: {e:?}"
            );
        }
    }

    /// tv_x_48 — `WalletError::WeakPassphrase` maps to
    /// `OctoCliError::WeakPassphrase` at slot 94 with exit 2. The
    /// rendered message names the `MIN_PASSPHRASE_CHARS` floor, so the
    /// operator sees the threshold the check compares against — never
    /// a fragment of the supplied passphrase.
    #[test]
    fn tv_x_48_weak_passphrase_maps_to_slot_94_exit_2_with_floor_message() {
        let e: OctoCliError = octo_wallet::WalletError::WeakPassphrase.into();
        assert!(
            matches!(e, OctoCliError::WeakPassphrase),
            "WalletError::WeakPassphrase must map to OctoCliError::WeakPassphrase, got: {e:?}"
        );
        assert_eq!(
            e.exit_code(),
            2,
            "WeakPassphrase is the slot-94 variant at exit 2 (operator-input validation family)"
        );
        let msg = e.user_message();
        let floor = octo_wallet::error::MIN_PASSPHRASE_CHARS;
        assert!(
            msg.contains(&floor.to_string()),
            "rendered message must name the MIN_PASSPHRASE_CHARS floor ({floor}): {msg}"
        );
        assert!(
            msg.contains("floor"),
            "rendered message must mention the floor: {msg}"
        );
        // Belt-and-braces — the rendered text must NOT carry any
        // supplied passphrase fragment, because the substrate carries
        // the no-payload form and the CLI envelope mirrors that.
        assert!(
            !msg.contains("passphrase="),
            "rendered message must not echo a supplied passphrase fragment: {msg}"
        );
    }

    /// R21: every operator-facing surface sanitizes, including the
    /// one that did not.
    ///
    /// `render` emits four things: `error` (from `user_message`,
    /// which sanitizes), `caused_by` (each frame sanitized), `hint`,
    /// and `exit_code`. **Three of the four sanitized. `hint` did
    /// not** - and `hint` is the surface that interpolates variant
    /// payloads, `NetworkSubstrateUnavailable` rendering its `detail`
    /// straight into the sentence. So substrate text reaching an
    /// operator had exactly one door, and it was the door the other
    /// three had already been closed on.
    ///
    /// `hint` is now sanitized at the boundary, not at each
    /// construction site. That is the durable form: a hint arm added
    /// later interpolating a new payload cannot be the one that
    /// forgets, because no arm is responsible for it any more.
    ///
    /// Two halves.
    ///
    /// The BEHAVIOURAL half goes through `render_block`, the real
    /// stderr render, so it observes what an operator would read. It
    /// deliberately builds the error with a RAW `detail`, not a
    /// sanitized one: if the construction site were the only thing
    /// holding the property, a vector that pre-sanitizes its input
    /// would pass while the display surface stayed open.
    ///
    /// The SOURCE half pins the JSON envelope's `hint` field, which
    /// `render_block` does not cover - `render` returns `!` and calls
    /// `process::exit`, so no vector can reach it at runtime. That is
    /// the same split `render_block` itself was extracted to enable.
    ///
    /// HONEST LIMIT: the source half constrains the text at the
    /// envelope site rather than the value that reaches a consumer.
    /// It is a spelling check on one line, and the behavioural half is
    /// what carries the property.
    #[test]
    fn tv_x_c_90_every_operator_surface_sanitizes_substrate_text() {
        // Behavioural: the stderr render, fed a RAW detail.
        let err = OctoCliError::NetworkSubstrateUnavailable {
            companion: "G9",
            detail: "slash bridge: internal error: query: SELECT did FROM records \
                     WHERE path = crates/octo-wallet/src/identity_store.rs"
                .to_string(),
        };
        let block = err.render_block();
        // The sanitizer's contract is to strip the two identifying
        // CLASSES - the `crates/octo-` path prefix and the
        // SQL/storage markers - not the whole surrounding sentence.
        // Asserting the markers, because asserting the absence of
        // arbitrary body text would be asserting a contract this
        // sanitizer does not have anywhere in the CLI.
        assert!(
            !block.contains("crates/octo-wallet"),
            "a substrate source path reached the operator's stderr block verbatim: {block}"
        );
        assert!(
            !block.to_lowercase().contains("query:"),
            "a substrate storage marker reached the operator's stderr block verbatim: {block}"
        );
        assert!(
            block.contains("<substrate-error>") && block.contains("<substrate-path>"),
            "the sanitizer must actually have fired on both classes, or this half asserts \
             nothing: {block}"
        );
        // The two things the operator needs must survive scrubbing:
        // the companion tag, so exit 89 stays attributable, and the
        // exit code, so operator switch tables still match.
        assert!(
            block.contains("G9"),
            "scrubbing ate the companion tag: {block}"
        );
        assert!(
            block.contains("exit code: 89"),
            "the stderr block lost its exit code line: {block}"
        );

        // Source: the envelope's `hint` field. Unreachable at runtime
        // because `render` calls `process::exit`.
        let src = include_str!("error.rs");
        assert!(
            src.contains("\"hint\": self.hint().map(|h| sanitize_substrate_error(&h))"),
            "the JSON envelope must sanitize the `hint` field the way it already sanitizes \
             `error` and `caused_by`. `hint` is the surface that interpolates variant payloads, \
             so an unsanitized hint hands substrate internals to any envelope consumer."
        );

        // Source: the construction sites, as defence in depth. Even
        // with both display surfaces closed, no site should be
        // building an operator-visible payload out of raw substrate
        // text.
        let network_src = include_str!("commands/network.rs");
        let production = network_src
            .split("#[cfg(test)]")
            .next()
            .expect("network.rs must have a production region");
        let mut checked = 0usize;
        for window in production.split("NetworkSubstrateUnavailable {").skip(1) {
            let detail_line = window
                .lines()
                .find(|l| l.trim_start().starts_with("detail:"))
                .unwrap_or_else(|| {
                    panic!(
                        "a NetworkSubstrateUnavailable construction has no detail field: {window}"
                    )
                });
            let value = detail_line
                .trim_start()
                .trim_start_matches("detail:")
                .trim()
                .trim_end_matches(',');
            let is_constant = value.contains('"');
            assert!(
                is_constant || value.contains("sanitize_substrate_error"),
                "a NetworkSubstrateUnavailable site assigns `detail` to a value that is neither a \
                 constant nor sanitized: `{value}`."
            );
            checked += 1;
        }
        assert!(
            checked >= 10,
            "only {checked} NetworkSubstrateUnavailable constructions were checked in the \
             production region of network.rs. If the sites moved, this vector is no longer \
             looking at the set it claims to cover."
        );
    }
}
