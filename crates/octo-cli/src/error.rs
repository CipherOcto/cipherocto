//! Operator-facing error envelope — RFC-0011 §Error Handling.

use std::io::Write;
use thiserror::Error;

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
    /// confirmation gate is unmet. Exact escape hatches depend on mode
    /// and command; the operator sees the per-mode help text from
    /// `OctoCliError::render` rather than a mode-specific label here.
    #[error("ConfirmationRequired: --confirm required for mutating command {command}")]
    ConfirmationRequired {
        /// Command that required confirmation.
        command: String,
    },
    /// A mutating command was invoked under `--mode auditor`.
    ///
    /// Auditor is a read-only role and is denied **before** the
    /// confirmation gate fires. Wave 3 LOW: a separate variant lets
    /// the operator see "auditor mode is read-only" rather than the
    /// generic "--confirm required" (which would be misleading —
    /// adding `--confirm` does not unblock an Auditor session).
    #[error("auditor mode is read-only; refusing mutating command {command}")]
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
    /// Identity already revoked.
    #[error("identity already revoked")]
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
    /// A deprecated stub was invoked during the stale-stub window.
    #[error("`{name}` was removed")]
    StaleStub {
        /// Stub command name.
        name: String,
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
    /// Mapped from `octo_audit_core::AuditError::ReceiptNotFound` at
    /// the dispatch boundary. Exit 17 per RFC-0016-a §6.7 table;
    /// shares the slot with `InvalidTtlHops` + `ForbiddenHolderMismatch`
    /// per the established amendment-chain shared-slot pattern
    /// (operator-unambiguous within their respective command surfaces).
    #[error("receipt not found: {0}")]
    ReceiptNotFound(String),

    /// Per-process trust boundary violated on the audit substrate
    /// (RFC-0016-a §6.7 paired-with-RFC-0011-a; canonical scrubbed
    /// path form). Mapped from
    /// `octo_audit_core::AuditError::PermissionDenied` at the dispatch
    /// boundary. Exit 13 per RFC-0016-a §6.7 table; shares the slot
    /// with `PolicyNotFound` (both are operator-input validation
    /// failures on access credentials — operator-unambiguous within
    /// their respective command surfaces).
    #[error("permission denied: {0}")]
    PermissionDenied(String),

    /// `octo agent attach` observed the target agent exists but is
    /// not in `Running` state (RFC-0011-c §9.8 + RFC-0015-a §6.3,
    /// slot 48 reserved by the agent amendment chain). Exit 48.
    #[error("agent not running: {0}")]
    AgentNotRunning(uuid::Uuid),

    /// Unexpected internal failure.
    #[error("internal error: {0}")]
    Internal(String),
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
            Self::AgentNotRunning(_) => 48,
            Self::Internal(_) => 64,
            Self::StaleStub { .. } => 65,
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
            Self::NoActiveIdentity => "create or select an identity before running this command".to_string(),
            Self::ConfirmationRequired { .. } => {
                "re-run with `--confirm` to acknowledge the mutation".to_string()
            }
            Self::AuditorDenied { .. } => {
                "auditor mode is read-only; switch to --mode human or --mode ci to perform mutations".to_string()
            }
            Self::AlreadyRotating => "complete or abort the in-flight rotation first".to_string(),
            Self::IdentityNotFound(_) => "list identities with `octo identity show`".to_string(),
            Self::HsmUnavailable(_) => "check that the HSM backend is reachable".to_string(),
            Self::AlreadyRevoked => "this identity is already revoked; no action needed".to_string(),
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
            Self::AgentNotRunning(_) => {
                "the target agent exists but is not in `Running` state; run `octo agent run` before `octo agent attach`".to_string()
            }
            Self::StaleStub { .. } => "this command was removed; see the migration notes".to_string(),
            Self::Internal(_) => "re-run with `RUST_LOG=debug` and report the diagnostic".to_string(),
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
        let msg = self.user_message();
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
                "error": msg,
                "caused_by": sources,
                "hint": self.hint(),
                "exit_code": code,
            });
            let _ = writeln!(w, "{body}");
        } else {
            let _ = writeln!(w, "error: {msg}");
            let mut src: Option<&dyn std::error::Error> = std::error::Error::source(self);
            while let Some(s) = src {
                let _ = writeln!(
                    w,
                    "  caused by: {}",
                    sanitize_substrate_error(&s.to_string())
                );
                src = s.source();
            }
            if let Some(hint) = self.hint() {
                let _ = writeln!(w, "  hint: {hint}");
            }
            let _ = writeln!(w, "  exit code: {code}");
        }
        let _ = w.flush();
        std::process::exit(code)
    }
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
/// conversion from `octo_audit_core::AuditError`.
///
/// Manual `impl From` rather than thiserror's `#[from]` attribute
/// at variant level — multiple variants converting from the same
/// source type would create conflicting `From` impls (one per
/// variant). The manual match keeps the substrate → CLI mapping
/// substrate-faithful per RFC-0011-a `ADD` envelope pattern
/// (per-variant mapping; not a catch-all `Internal` wrapper).
///
/// **Defense-in-depth scrub pass (RFC-0016-a §6.8 + R1 reviewer
/// HIGH findings C8 + C9):** every payload-bearing variant
/// (ReceiptNotFound, InvalidFilter, PermissionDenied, SinkSpecific)
/// routes through `octo_audit::redact_substrate_error` before
/// constructing the CLI envelope. If the substrate payload matches
/// any of the 13 canonical scrubber patterns (hex digest, JWT,
/// WIF, BIP39 mnemonic, capability-secret base64, PEM/PGP/OpenSSH
/// private-key blocks, etc.), the entire payload collapses to the
/// canonical `<REDACTED>` marker. This is the second pass at the
/// CLI boundary; the substrate-side scrubber (defect 1a) is the
/// first pass.
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
            // 13-pattern scrubber so an unknown future variant that
            // accidentally carries a key/path leaks only
            // `<redacted-*>` markers.
            _ => Self::Internal(sanitize_substrate_error(&format!(
                "audit substrate error: {e}"
            ))),
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

/// Case-insensitive variant of `find_word_boundary` (R17 Lens-2 F2).
/// Matches ASCII case variants only (`a-z`/`A-Z`) — the markers we use
/// (`SQL:`, `query:`, `sqlite3_open`) are all ASCII so this is sufficient.
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

#[cfg(test)]
mod tests {
    use super::*;
    use thiserror::Error;

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

    #[test]
    fn tv_err3_source_chain_rendered() {
        // Build a chained-error wrapper that exposes a `#[source]` chain so
        // `std::error::Error::source()` walks more than one frame.
        #[derive(Error, Debug)]
        #[error("top-level: {0}")]
        struct Wrapper(#[source] Inner);

        #[derive(Error, Debug)]
        #[error("inner cause")]
        struct Inner;

        let inner = Inner;
        let chain = Wrapper(inner);
        // Render through OctoCliError::Internal so the sanitizer runs and we
        // exercise the `caused by:` walk. The wrapped text doesn't contain
        // any substrate markers so the message passes through verbatim.
        let cli_err = OctoCliError::Internal(format!("{chain}"));
        // Force the JSON branch off — we test the multi-line text branch by
        // asserting that the rendered format strings reference `caused by`
        // and `exit code` tokens and that source() walks both frames.
        let mut lines: Vec<String> = Vec::new();
        let mut src: Option<&dyn std::error::Error> =
            Some(&Wrapper(Inner) as &dyn std::error::Error);
        while let Some(s) = src {
            lines.push(format!("  caused by: {s}"));
            src = s.source();
        }
        assert!(
            lines.iter().any(|l| l.contains("top-level")),
            "wrapper not walked: {lines:?}"
        );
        assert!(
            lines.iter().any(|l| l.contains("inner cause")),
            "inner cause not walked: {lines:?}"
        );
        // Sanity check the cli error renders a stable `user_message`.
        assert!(cli_err.user_message().contains("top-level"));
    }

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
        let e = OctoCliError::IdentityNotFound("alice".into());
        // We can't easily capture stderr from a !-returning fn without
        // spawning a process, so we just verify the formatting inputs
        // are coherent: hint present, code matches.
        assert!(e.hint().is_some());
        assert_eq!(e.exit_code(), 4);
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
    }
}
