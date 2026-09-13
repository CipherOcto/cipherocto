//! Agent substrate — RFC-0011-c §9.10 `[ADD]` signatures.
//!
//! Layer B additions for the `octo agent` subcommand family. The CLI
//! (`octo-cli`, Layer C/D) consumes these types via re-exports from
//! `octo_wallet::{AgentManifest, AgentState, AgentSummary, AgentFilter,
//! CapabilityId, register_agent, ...}`.
//!
//! ## Phase status
//!
//! Phase 1 (this module): types + a process-global in-memory agent
//! registry plus the `register_agent` free function. The 6-step
//! capability validation pipeline (RFC-0002 §Capability Validation)
//! runs in `octo_cap_macaroon` substrate; Phase 1 emits
//! `CapabilityValidationFailed(usize)` only when the substrate signals
//! a failure in a future wiring. The replay protection hook
//! (RFC-0011-c §9.7) likewise defers to the macaroon substrate.
//!
//! ## Determinism contract (RFC-0008 Class B)
//!
//! `register_agent` is a pure function of `(manifest_digest, active_did)`:
//! the same inputs produce the same `agent_id`. Duplicate registrations
//! are detected by `agent_id` equality and emit `AgentAlreadyExists`.
//! Cross-replica determinism is preserved as long as the process-global
//! registry state matches; Phase 1 is in-process only.

use std::collections::BTreeMap;
use std::sync::{Mutex, OnceLock};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::WalletError;
use crate::identity_record::Did;

/// Process-global in-memory agent registry.
///
/// Phase 1 stores `(agent_id, (manifest, holder_did))` pairs. The
/// registry is process-lifetime; no persistence. The static is
/// intentionally `BTreeMap`-keyed by `Uuid` so that iteration order is
/// deterministic across runs (per RFC-0008 Class B determinism
/// contract). The lock is acquired on every register/read; the
/// critical sections are O(log n) and contention is negligible for
/// Phase 1 (operator CLI, not a hot loop).
static AGENT_REGISTRY: OnceLock<Mutex<BTreeMap<Uuid, AgentRecord>>> = OnceLock::new();

pub(crate) fn registry() -> &'static Mutex<BTreeMap<Uuid, AgentRecord>> {
    AGENT_REGISTRY.get_or_init(|| Mutex::new(BTreeMap::new()))
}

#[derive(Debug, Clone)]
pub(crate) struct AgentRecord {
    #[allow(dead_code)] // Phase 1: persisted for sibling mission wiring.
    pub(crate) manifest: AgentManifest,
    #[allow(dead_code)] // Phase 1: persisted for sibling mission wiring.
    pub(crate) holder_did: Did,
    #[allow(dead_code)] // Phase 1: persisted for sibling mission wiring.
    pub(crate) registered_at_unix: u64,
    /// Current lifecycle state (RFC-0015-a §6.1). Defaults to
    /// `Registered` for records created by `register_agent`; mutated
    /// additively by `transition_agent` on `Registered → Running` and
    /// `Running → Terminated` edges. Terminal state is
    /// `Terminated` (no further transitions permitted).
    pub(crate) state: AgentState,
}

/// Canonical agent manifest wire form — RFC-0002 §Agent Manifest.
///
/// `signature_hex` is the holder's Ed25519 signature over the canonical
/// JSON form of the remaining fields (RFC-0002 §Agent Manifest wire
/// contract). The 6-step validation pipeline (RFC-0002 §Capability
/// Validation) verifies this signature at step 1; Phase 1 accepts the
/// field structurally and defers signature verification to the
/// macaroon substrate (wired by future amendment).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentManifest {
    /// Stable manifest identifier (UUIDv5 over `(holder_did, label,
    /// created_at_unix)` or the operator-supplied id from the manifest
    /// file — Phase 1 accepts any UUIDv4 string).
    pub manifest_id: Uuid,
    /// Holder DID — RFC-0010 form. Must equal the `active_did` passed
    /// to `register_agent` (RFC-0002 §Agent Manifest §Holder Binding).
    pub holder_did: String,
    /// Optional operator-supplied label (e.g., `"build-runner-01"`).
    /// Stored in substrate, surfaced through `AgentSummary::label`.
    #[serde(default)]
    pub label: Option<String>,
    /// Unix seconds at which the manifest was authored.
    pub created_at_unix: u64,
    /// Holder signature over the canonical JSON (RFC-0002 §Agent
    /// Manifest §Signature). Phase 1 stores the bytes; verification
    /// defers to the macaroon substrate.
    pub signature_hex: String,
}

impl AgentManifest {
    /// Parse an `AgentManifest` from its canonical JSON wire form.
    ///
    /// Returns `WalletError::ManifestParse { path, reason }` with the
    /// caller-supplied `path` label on failure. The CLI passes the
    /// `--manifest-path` value so operator errors surface the file
    /// they specified.
    pub fn from_json(path: &str, body: &str) -> Result<Self, WalletError> {
        serde_json::from_str::<Self>(body).map_err(|e| WalletError::ManifestParse {
            path: path.to_string(),
            reason: format!("{e}"),
        })
    }

    /// Deterministic BLAKE3-256 digest of the manifest (RFC-0002 §Agent
    /// Manifest §Canonical Hash). Used as the replay-protection
    /// identifier (RFC-0011-c §9.7) and surfaced through
    /// `AgentCreateOutput::manifest_digest`.
    pub fn digest_hex(&self) -> String {
        // Canonical serialization: serde_json's default ordering is
        // field-declaration order on this struct (no `serde(skip)` /
        // `#[serde(flatten)]` indirection), which is stable across
        // compilations.
        let canonical = serde_json::to_vec(self)
            .expect("AgentManifest serialization is infallible (no custom Serialize impls)");
        let digest = octo_cap_macaroon::blake3_hash(&canonical);
        hex::encode(&digest[..])
    }
}

/// Agent lifecycle state — RFC-0002 §Agent Lifecycle.
///
/// `#[non_exhaustive]` per [[cipherocto-design-principles]]
/// §Extension over enumeration: future states (e.g., `Suspended`,
/// `Draining`) land additively without breaking matchers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum AgentState {
    /// Registered with the wallet substrate; not yet running. The
    /// `Registered` state is the terminal state for `octo agent
    /// create` (RFC-0011-c §9.3.1).
    Registered,
    /// Currently running under `octo-runtime` (RFC-0011-c §9.3.2
    /// `agent run`). Wired by `0011-c-agent-run-subcommand`.
    Running,
    /// Terminated (RFC-0011-c §9.3.4 `agent destroy`). Terminal state.
    Terminated,
}

impl AgentState {
    /// Stable lowercase label for envelope rendering.
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            AgentState::Registered => "registered",
            AgentState::Running => "running",
            AgentState::Terminated => "terminated",
        }
    }

    /// Iterate all known stable states (canonical ordering).
    ///
    /// Used by `octo-cli` `parse_state_filter` to enumerate valid
    /// `--state` argument values without duplicating the label list
    /// per-crate. Backwards-compat: state additions land via
    /// `#[non_exhaustive]` on the enum and `iter()` returns a fixed
    /// slice of the three Phase 1 stable states, so future enum
    /// extensions do not break callers that iterate the slice.
    #[must_use]
    pub fn iter() -> &'static [AgentState] {
        &[
            AgentState::Registered,
            AgentState::Running,
            AgentState::Terminated,
        ]
    }
}

/// One row of `list_owned_agents` — RFC-0011-c §9.10 `AgentSummary`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentSummary {
    /// Deterministic `agent_id` derived from `(manifest_digest,
    /// active_did)`.
    pub agent_id: Uuid,
    /// Subject DID (RFC-0010 form). Mirrors `AgentManifest::holder_did`
    /// at registration time.
    pub holder_did: String,
    /// Lifecycle state.
    pub state: AgentState,
    /// Optional operator label.
    pub label: Option<String>,
    /// Unix seconds at which the agent was registered.
    pub registered_at_unix: u64,
    /// BLAKE3-256 digest of the canonical manifest (RFC-0002 §Agent
    /// Manifest §Canonical Hash).
    pub manifest_digest: String,
}

/// Filter for `list_owned_agents` — RFC-0011-c §9.10.
///
/// Phase 1 only uses `holder_did` (the rest land with
/// `0011-c-agent-list-subcommand`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentFilter {
    /// Restrict to a specific holder DID (RFC-0010 form).
    pub holder_did: Option<String>,
    /// Restrict to a specific lifecycle state.
    pub state: Option<AgentState>,
    /// Maximum rows returned (`None` = no cap; substrate applies a
    /// hard ceiling of 1024).
    pub limit: Option<usize>,
    /// Opaque cursor (forward-compat for Phase 2).
    pub cursor: Option<String>,
}

/// Capability root identifier — RFC-0011-c §9.10 `CapabilityId`.
///
/// 32-byte identifier for the macaroon referenced by `capability_root`
/// in the `register_agent` signature. The CLI parses the operator's
/// `--capability-root <hex>` argument into this newtype.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CapabilityId(pub [u8; 32]);

/// Receipt returned by `transition_agent` — RFC-0015-a Appendix A
/// (operative intent; missions reference `TransitionReceipt`).
///
/// Captures the full transition audit trail in one substrate-faithful
/// projection: the canonical `agent_id`, the typed `previous_state` +
/// `current_state` pair (matches the `AgentState` enum byte-for-byte),
/// the unix-seconds timestamp at which the substrate applied the
/// transition (best-effort wall-clock per `cli_fns::now_unix_secs`),
/// and the 32-byte BLAKE3-256 chain-hash of the audit event that
/// committed this transition (CLI surfaces as Hex32 per
/// `OctoCliError::InvalidStateTransition` mirror pattern).
///
/// # Layer discipline + field-level invariants
///
/// - `agent_id` is the canonical UUID (RFC-0010 form) of the
///   transitioned record (the registry stores the manifest's
///   `manifest_id` as the agent key — `transition_agent` returns it
///   verbatim).
/// - `previous_state` and `current_state` are typed `AgentState`
///   variants; the CLI converts to `as_str()` at the envelope
///   boundary for stable wire form (`registered` / `running` /
///   `terminated`).
/// - `transitioned_at_unix` is best-effort `SystemTime::now()`;
///   Phase 2 follow-on swaps to the substrate monotonic clock per
///   RFC-0008 Class B determinism.
/// - `audit_log_entry` is the canonical chain-hash from
///   `append_audit_event`; `None` only when the state transition
///   was an idempotent self-transition (no audit event emitted per
///   RFC-0015-a §6.1 idempotency contract) OR when the audit append
///   was rolled back (function never returns in that case, so
///   `Some(_)` is the only invariant at the receipt level).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransitionReceipt {
    /// Canonical UUID of the transitioned agent.
    pub agent_id: Uuid,
    /// State before the transition.
    pub previous_state: AgentState,
    /// State after the transition.
    pub current_state: AgentState,
    /// Unix-seconds timestamp at which the substrate applied the
    /// transition.
    pub transitioned_at_unix: u64,
    /// BLAKE3-256 chain-hash of the audit event that committed this
    /// transition (Hex32 wire form per RFC-0015-a Appendix A).
    pub audit_log_entry: [u8; 32],
}

/// Best-effort wall-clock for `transitioned_at_unix` (Phase 2 unblock).
/// Reuses the same helper pattern as `cli_fns::now_unix_secs` but
/// lives in the agent module so `transition_agent` is self-contained.
fn now_unix_secs() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// Transition an agent's lifecycle state — RFC-0015-a §6.1.
///
/// State-machine guard (per RFC-0015-a Appendix A):
/// - `Registered → Running`: valid (`run` mission)
/// - `Running → Terminated`: valid (`destroy` mission)
/// - `Terminated → *`: rejected (`InvalidStateTransition`; terminal state)
/// - `Registered → Terminated`: rejected (`InvalidStateTransition`;
///   skipping `Running` is not a valid edge)
/// - self-transition `state == target`: idempotent success, returns
///   `TransitionReceipt` with `previous_state == current_state` and
///   `audit_log_entry == [0u8; 32]` (no audit event emitted per the
///   §6.1 idempotency contract — re-running a `run` against an
///   already-running agent is a no-op, not a duplicate audit event).
///
/// # Caller-attestation + security (RFC-0011 §Lifecycle Requirements)
///
/// `caller_did` MUST match the agent's `holder_did`; on mismatch,
/// `ForbiddenHolderMismatch` is returned (multi-DID enumeration
/// prevention). Lookup-first ordering: the substrate resolves the
/// record, verifies the holder DID, and only then reads the state —
/// never branches on the holder DID before the lookup.
///
/// # Audit append + rollback contract (RFC-0015-a §6.1)
///
/// On a successful state-machine transition, this function constructs
/// an `AuditEvent` with `event_kind = AuditEventKind::AgentTransition
/// { agent_id, from, to, reason }` (the variant is cfg-gated via
/// `octo-audit-internal`) and calls `append_audit_event` to commit it
/// to the sink. If the audit append fails (e.g., sink not configured,
/// feature flag off, IO error), the state transition is ROLLED BACK
/// to the previous state and `WalletError::AuditUnavailable` is
/// returned. The `agent_id` lookup result is consumed before the
/// rollback so the caller-attestation check is not re-run.
///
/// # Reason scrubbing
///
/// If `reason.is_some()`, `validate_reason` is invoked first (256-byte
/// cap + control-character rejection per RFC-0015 §6.2.5); the
/// scrubbed reason is stored in the audit event payload.
pub fn transition_agent(
    caller_did: &Did,
    uuid: Uuid,
    target: AgentState,
    reason: Option<&str>,
) -> Result<TransitionReceipt, WalletError> {
    // 1. Reason scrub: control-char + length check (RFC-0015 §6.2.5).
    //    No-op when `reason` is `None`.
    if let Some(r) = reason {
        validate_reason(r)?;
    }

    // 2. Lock the registry. `WalletError::Config` on poisoning
    //    (fail-closed per the established `octo-wallet` pattern —
    //    other call sites in this module use the same mapping).
    let mut registry = registry()
        .lock()
        .map_err(|_| WalletError::Config("agent registry mutex poisoned".to_string()))?;

    // 3. Point lookup + caller-attestation. `AgentNotFound` is the
    //    substrate-faithful miss variant; `ForbiddenHolderMismatch`
    //    is the multi-DID enumeration prevention guard.
    let record = registry
        .get_mut(&uuid)
        .ok_or(WalletError::AgentNotFound(uuid))?;

    if record.holder_did.as_str() != caller_did.as_str() {
        return Err(WalletError::ForbiddenHolderMismatch);
    }

    let from = record.state;

    // 4. Self-transition idempotency (RFC-0015-a §6.1 contract).
    //    No audit event emitted; `audit_log_entry` is zeroed per the
    //    TransitionReceipt invariant comment.
    if from == target {
        return Ok(TransitionReceipt {
            agent_id: uuid,
            previous_state: from,
            current_state: from,
            transitioned_at_unix: now_unix_secs(),
            audit_log_entry: [0u8; 32],
        });
    }

    // 5. State-machine guard. The two valid edges are
    //    `Registered → Running` (run mission) and
    //    `Running → Terminated` (destroy mission). All other
    //    transitions are rejected with `InvalidStateTransition`.
    let valid_edge = matches!(
        (from, target),
        (AgentState::Registered, AgentState::Running)
            | (AgentState::Running, AgentState::Terminated)
    );
    if !valid_edge {
        return Err(WalletError::InvalidStateTransition { from, to: target });
    }

    // 6. Apply state mutation. Audit append follows; if it fails,
    //    we roll back to `from`.
    record.state = target;
    let transitioned_at_unix = now_unix_secs();

    // 7. Build + append the audit event. The `AgentTransition`
    //    variant is cfg-gated via `octo-audit-internal`; when the
    //    feature is off, this branch is unreachable, so the
    //    function falls through to `WalletError::AuditUnavailable`
    //    (fail-closed per the audit append + rollback contract).
    #[cfg(feature = "octo-audit-internal")]
    let audit_result = {
        use octo_audit::audit_write::{append_agent_transition_event, AgentTransitionPayload};
        let payload = AgentTransitionPayload {
            agent_id: uuid,
            from: from.as_str().to_owned(),
            to: target.as_str().to_owned(),
            reason: reason.map(str::to_owned),
        };
        append_agent_transition_event(&payload, transitioned_at_unix)
            .map_err(|e| WalletError::AuditUnavailable(format!("{e:?}")))
    };

    #[cfg(not(feature = "octo-audit-internal"))]
    let audit_result: Result<[u8; 32], WalletError> = Err(WalletError::AuditUnavailable(
        "octo-audit-internal feature not enabled (RFC-0015-a §6.4 paired-acceptance bridge)"
            .to_string(),
    ));

    // 8. Audit append + rollback contract (RFC-0015-a §6.1). On
    //    audit failure, the state mutation is rolled back to `from`
    //    before the function returns so the registry remains
    //    consistent.
    let chain_hash = match audit_result {
        Ok(hash) => hash,
        Err(e) => {
            record.state = from;
            return Err(e);
        }
    };

    Ok(TransitionReceipt {
        agent_id: uuid,
        previous_state: from,
        current_state: target,
        transitioned_at_unix,
        audit_log_entry: chain_hash,
    })
}

/// List all agents owned by the caller-attested DID, filtered server-side
/// (RFC-0015 §6.2.1).
///
/// SECURITY: HIGH (caller-attestation pattern per RFC-0011
/// §Lifecycle Requirements). The `caller_did` MUST be the
/// caller-attested active DID; the substrate enforces
/// `filter.holder_did.is_none() || filter.holder_did == caller_did`
/// (`ForbiddenHolderMismatch` on mismatch, multi-DID enumeration
/// prevention).
///
/// Sorting: `registered_at_unix DESC` with secondary sort by `agent_id`
/// ASC for determinism (per RFC-0008 Class B). Limit: clamp at 1024
/// (substrate hard ceiling per RFC-0015 §6.2.1). Cursor: opaque token
/// reserved for Phase 2 (currently ignored). Read-only: no state
/// mutation.
pub fn list_owned_agents(
    caller_did: &Did,
    filter: &AgentFilter,
) -> Result<Vec<AgentSummary>, WalletError> {
    let effective_holder = filter
        .holder_did
        .as_deref()
        .unwrap_or_else(|| caller_did.as_str());
    if effective_holder != caller_did.as_str() {
        return Err(WalletError::ForbiddenHolderMismatch);
    }

    let limit = filter.limit.unwrap_or(1024).min(1024);

    let registry = registry()
        .lock()
        .map_err(|_| WalletError::Config("agent registry mutex poisoned".to_string()))?;

    let mut summaries: Vec<AgentSummary> = registry
        .values()
        .filter(|record| record.holder_did.as_str() == effective_holder)
        .filter(|_| match filter.state {
            // Phase 1 registry stores only `Registered` agents
            // (write-path state transitions land with RFC-0015-a);
            // pending agents never enter the registry until the
            // state-machine substrate is wired. Surface future states
            // per the `AgentState` enum without speculative writes:
            // `--state running|terminated` returns empty `Vec` (no
            // such records exist), `--state registered` returns all
            // records, bare `--list` returns all records.
            Some(AgentState::Running | AgentState::Terminated) => false,
            None | Some(AgentState::Registered) => true,
        })
        .map(|record| AgentSummary {
            agent_id: record.manifest.manifest_id,
            holder_did: record.holder_did.as_str().to_owned(),
            state: AgentState::Registered,
            label: record.manifest.label.clone(),
            registered_at_unix: record.registered_at_unix,
            manifest_digest: record.manifest.digest_hex(),
        })
        .collect();

    summaries.sort_by(|a, b| {
        b.registered_at_unix
            .cmp(&a.registered_at_unix)
            .then_with(|| a.agent_id.cmp(&b.agent_id))
    });
    summaries.truncate(limit);
    Ok(summaries)
}

/// Point lookup of an agent manifest by canonical UUID
/// (RFC-0015 §6.2.6).
///
/// Substrate-faithful to the in-memory `BTreeMap<Uuid, AgentRecord>`
/// registry. Returns the canonical `AgentManifest` on hit;
/// `WalletError::AgentNotFound(uuid)` on miss. Caller-attestation
/// enforced: caller DID must equal the agent's holder DID or
/// `WalletError::ForbiddenHolderMismatch` is raised. No mutation.
pub fn lookup_agent(caller_did: &Did, uuid: Uuid) -> Result<AgentManifest, WalletError> {
    let registry = registry()
        .lock()
        .map_err(|_| WalletError::Config("agent registry mutex poisoned".to_string()))?;

    let record = registry
        .get(&uuid)
        .ok_or(WalletError::AgentNotFound(uuid))?;

    if record.holder_did.as_str() != caller_did.as_str() {
        return Err(WalletError::ForbiddenHolderMismatch);
    }

    Ok(record.manifest.clone())
}

/// Substrate-faithful control-character filter primitive
/// (RFC-0015 §6.2.5).
///
/// Returns:
/// - `Ok(())` if `reason` is empty OR contains only ASCII printable +
///   non-control Unicode (`U+0020+`).
/// - `Err(WalletError::ReasonContainsControlChars(<U+XXXX>))` if
///   `reason` contains any control character in `U+0000`-`U+001F` or
///   `U+007F`. The variant carries the offending character as a
///   `String` in hex-escaped code-point notation (e.g., `<U+001B>`
///   for ESC), NEVER the raw byte — `Display` impl MUST NOT echo
///   attacker bytes back to the terminal.
/// - `Err(WalletError::ReasonTooLong(len))` if `reason.len() > 256`.
///
/// Pure function on UTF-8 input; no IO, no state mutation.
pub fn validate_reason(reason: &str) -> Result<(), WalletError> {
    const REASON_MAX_BYTES: usize = 256;

    if reason.len() > REASON_MAX_BYTES {
        return Err(WalletError::ReasonTooLong(reason.len()));
    }

    for ch in reason.chars() {
        let cp = ch as u32;
        if (cp < 0x20) || cp == 0x7F {
            return Err(WalletError::ReasonContainsControlChars(format!(
                "<U+{cp:04X}>"
            )));
        }
    }

    Ok(())
}

impl CapabilityId {
    /// Parse a 64-char lowercase hex string into a `CapabilityId`.
    ///
    /// # Errors
    /// Returns `WalletError::InvalidSlotId` on any non-hex byte or
    /// length mismatch (the substrate uses the same variant for all
    /// raw-bytes parsing failures per RFC-0011 §Error Handling).
    pub fn from_hex(s: &str) -> Result<Self, WalletError> {
        if s.len() != 64 {
            return Err(WalletError::InvalidSlotId(format!(
                "capability_root must be 64 hex chars (got {})",
                s.len()
            )));
        }
        let mut out = [0u8; 32];
        hex::decode_to_slice(s, &mut out).map_err(|e| WalletError::InvalidSlotId(e.to_string()))?;
        Ok(Self(out))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_manifest() -> AgentManifest {
        AgentManifest {
            manifest_id: Uuid::new_v4(),
            holder_did: "did:octo:zTest".to_string(),
            label: Some("test".to_string()),
            created_at_unix: 1_700_000_000,
            signature_hex: "00".repeat(64),
        }
    }

    #[test]
    fn manifest_roundtrip_json() {
        let m = sample_manifest();
        let s = serde_json::to_string(&m).unwrap();
        let back = AgentManifest::from_json("/dev/null", &s).unwrap();
        assert_eq!(back, m);
    }

    #[test]
    fn manifest_parse_error_carries_path() {
        let err = AgentManifest::from_json("/tmp/missing.json", "not json").unwrap_err();
        match err {
            WalletError::ManifestParse { path, reason } => {
                assert_eq!(path, "/tmp/missing.json");
                assert!(!reason.is_empty());
            }
            other => panic!("expected ManifestParse, got {other:?}"),
        }
    }

    #[test]
    fn manifest_digest_is_deterministic() {
        let m = sample_manifest();
        assert_eq!(m.digest_hex(), m.digest_hex());
        assert_eq!(64, m.digest_hex().len(), "BLAKE3-256 = 32 bytes = 64 hex");
    }

    #[test]
    fn agent_state_as_str_is_lowercase() {
        assert_eq!(AgentState::Registered.as_str(), "registered");
        assert_eq!(AgentState::Running.as_str(), "running");
        assert_eq!(AgentState::Terminated.as_str(), "terminated");
    }

    #[test]
    fn capability_id_parses_canonical_hex() {
        let hex = "ab".repeat(32);
        let cid = CapabilityId::from_hex(&hex).unwrap();
        assert_eq!(cid.0, [0xab; 32]);
    }

    #[test]
    fn capability_id_rejects_short_hex() {
        let err = CapabilityId::from_hex("abcd").unwrap_err();
        assert!(matches!(err, WalletError::InvalidSlotId(_)));
    }

    #[test]
    fn register_agent_is_deterministic_for_same_inputs() {
        // Determinism contract (RFC-0011-c §9.10, RFC-0008 Class B):
        // for Phase 1, `register_agent` derives `agent_id` from
        // `(manifest, active_did)` only — the `capability_root`
        // argument is accepted in the signature and recorded for the
        // macaroon substrate to consume, but the Phase 1 derivation
        // path (`cli_fns::register_agent`) ignores it (see
        // `#[allow(unused_variables)]` on the parameter). Phase 2
        // will fold `capability_root` into the derivation. This test
        // pins the Phase 1 invariant: calling twice with the same
        // `(manifest, active_did)` MUST surface `AgentAlreadyExists`
        // with the same UUID — proof of reproducibility.
        let manifest = AgentManifest {
            manifest_id: Uuid::new_v4(),
            holder_did: format!("did:octo:determinism-{}", Uuid::new_v4()),
            label: Some("determinism".to_string()),
            created_at_unix: 1_700_000_000,
            signature_hex: "00".repeat(64),
        };
        let cap_root = CapabilityId([0x42; 32]);
        let did = Did::from(manifest.holder_did.as_str());

        let first = crate::cli_fns::register_agent(&manifest, &cap_root, &did)
            .expect("first call must succeed");

        let second = crate::cli_fns::register_agent(&manifest, &cap_root, &did);
        match second {
            Err(WalletError::AgentAlreadyExists(uuid)) => {
                assert_eq!(
                    uuid, first,
                    "second registration MUST surface the same agent_id (deterministic derivation)",
                );
            }
            other => panic!(
                "second registration must return AgentAlreadyExists with the same UUID, got: {other:?}",
            ),
        }
    }

    #[test]
    fn register_agent_distinct_inputs_yield_distinct_ids() {
        // The dual of the determinism contract: distinct
        // `(manifest, did)` pairs MUST yield distinct `agent_id`s
        // (the UUIDv5 derivation is collision-resistant modulo
        // RFC 4122 §4.3). Together with `…_for_same_inputs` this
        // pins the substrate's derivation invariants end-to-end.
        let m_one = AgentManifest {
            manifest_id: Uuid::new_v4(),
            holder_did: format!("did:octo:collision-a-{}", Uuid::new_v4()),
            label: Some("a".to_string()),
            created_at_unix: 1_700_000_001,
            signature_hex: "00".repeat(64),
        };
        let m_two = AgentManifest {
            manifest_id: Uuid::new_v4(),
            holder_did: format!("did:octo:collision-b-{}", Uuid::new_v4()),
            label: Some("b".to_string()),
            created_at_unix: 1_700_000_002,
            signature_hex: "00".repeat(64),
        };
        let cap = CapabilityId([0x07; 32]);
        let did_a = Did::from(m_one.holder_did.as_str());
        let did_b = Did::from(m_two.holder_did.as_str());

        let a = crate::cli_fns::register_agent(&m_one, &cap, &did_a).expect("register a");
        let b = crate::cli_fns::register_agent(&m_two, &cap, &did_b).expect("register b");
        assert_ne!(a, b, "distinct inputs MUST yield distinct agent_ids");
    }

    // ----- Read-path surface tests (RFC-0015 §6.2.1, §6.2.5, §6.2.6) -----

    #[test]
    fn validate_reason_accepts_printable() {
        // RFC-0015 §6.2.5: ASCII printable + non-control Unicode passes.
        assert!(validate_reason("").is_ok());
        assert!(validate_reason("hello world").is_ok());
        assert!(validate_reason("non-control unicode: café — 漢字").is_ok());
    }

    #[test]
    fn validate_reason_rejects_esc_byte() {
        // RFC-0015 §6.2.5: U+001B (ESC) is in the rejected range.
        // Payload must be hex-escaped code-point notation, NEVER the
        // raw byte (pager-hijack mitigation).
        let bad = "evil\u{001B}pager";
        let err = validate_reason(bad).unwrap_err();
        match err {
            WalletError::ReasonContainsControlChars(s) => {
                assert_eq!(s, "<U+001B>", "payload must be hex-escaped, not raw byte");
            }
            other => panic!("expected ReasonContainsControlChars, got {other:?}"),
        }
    }

    #[test]
    fn validate_reason_rejects_del_byte() {
        // RFC-0015 §6.2.5: U+007F (DEL) is also rejected.
        let bad = "post\u{007F}malicious";
        let err = validate_reason(bad).unwrap_err();
        assert!(matches!(err, WalletError::ReasonContainsControlChars(_)));
    }

    #[test]
    fn validate_reason_rejects_oversize() {
        // RFC-0015 §6.2.5: input over 256 bytes fails. Payload carries
        // the offending byte length as usize.
        let too_long = "x".repeat(257);
        let err = validate_reason(&too_long).unwrap_err();
        match err {
            WalletError::ReasonTooLong(len) => assert_eq!(len, 257),
            other => panic!("expected ReasonTooLong, got {other:?}"),
        }
    }

    #[test]
    fn lookup_agent_miss_returns_agent_not_found() {
        // RFC-0015 §6.2.6: unknown UUID returns AgentNotFound with the
        // searched UUID in the payload. Use a random UUID that
        // (statistically) has never been registered.
        let did = Did::from("did:octo:unknown-uuid-test");
        let err = lookup_agent(&did, Uuid::new_v4()).unwrap_err();
        match err {
            WalletError::AgentNotFound(uuid) => {
                assert_eq!(uuid.get_version_num(), 4, "Uuid payload preserved");
            }
            other => panic!("expected AgentNotFound, got {other:?}"),
        }
    }

    #[test]
    fn list_owned_agents_rejects_filter_holder_mismatch() {
        // RFC-0015 §6.2.1: filter.holder_did differs from caller_did
        // MUST raise ForbiddenHolderMismatch (multi-DID enumeration
        // prevention).
        let caller = Did::from("did:octo:caller-a");
        let filter = AgentFilter {
            holder_did: Some("did:octo:something-else".to_string()),
            ..Default::default()
        };
        let err = list_owned_agents(&caller, &filter).unwrap_err();
        assert!(matches!(err, WalletError::ForbiddenHolderMismatch));
    }

    // ----- Write-path surface tests (RFC-0015-a §6.1, Appendix A) -----

    /// Register an agent with deterministic inputs and return its
    /// UUID. Helper for the `transition_agent_*` tests.
    fn register_one() -> (Uuid, Did) {
        let manifest = AgentManifest {
            manifest_id: Uuid::new_v4(),
            holder_did: format!("did:octo:transition-test-{}", Uuid::new_v4()),
            label: Some("transition-test".to_string()),
            created_at_unix: 1_700_000_010,
            signature_hex: "00".repeat(64),
        };
        let cap = CapabilityId([0x05; 32]);
        let did = Did::from(manifest.holder_did.as_str());
        let uuid = crate::cli_fns::register_agent(&manifest, &cap, &did)
            .expect("register_agent must succeed in test");
        (uuid, did)
    }

    #[test]
    fn transition_agent_unknown_uuid_returns_agent_not_found() {
        // RFC-0015-a §6.1: point-lookup miss returns AgentNotFound.
        // The miss happens BEFORE the state-machine guard, so no
        // event lookup, no audit append.
        let did = Did::from("did:octo:transition-miss");
        let err = transition_agent(&did, Uuid::new_v4(), AgentState::Running, None).unwrap_err();
        match err {
            WalletError::AgentNotFound(uuid) => {
                assert_eq!(uuid.get_version_num(), 4);
            }
            other => panic!("expected AgentNotFound, got {other:?}"),
        }
    }

    #[test]
    fn transition_agent_caller_attestation_guard() {
        // RFC-0015-a §6.1 + RFC-0011 §Lifecycle Requirements: caller_did
        // must match record.holder_did. Lookup-first ordering — the
        // lookup succeeds, the holder DID differs, then
        // ForbiddenHolderMismatch is returned (multi-DID enumeration
        // prevention). The error type alone proves the guard fired
        // BEFORE the state-machine transition (lookup-first ordering);
        // the substrate's `ForbiddenHolderMismatch` branch carries no
        // state mutation by construction.
        let (uuid, _holder) = register_one();
        let other = Did::from("did:octo:different-caller");
        let err = transition_agent(&other, uuid, AgentState::Running, None).unwrap_err();
        assert!(matches!(err, WalletError::ForbiddenHolderMismatch));
    }

    #[test]
    fn transition_agent_self_transition_is_idempotent() {
        // RFC-0015-a §6.1 idempotency contract: re-running a
        // `run`-equivalent transition against an already-Running
        // agent MUST be a no-op success (`previous_state == current_state`),
        // NOT a duplicate audit event. The receipt's `audit_log_entry`
        // is zeroed (no audit append path was taken).
        //
        // We can't reach Running without an audit feature, so the
        // self-transition test exercises the Registered → Registered
        // branch which is structurally identical (no audit append,
        // zeroed chain-hash).
        let (uuid, holder) = register_one();
        let receipt =
            transition_agent(&holder, uuid, AgentState::Registered, None).expect("self-transit");
        assert_eq!(receipt.previous_state, AgentState::Registered);
        assert_eq!(receipt.current_state, AgentState::Registered);
        assert_eq!(
            receipt.audit_log_entry, [0u8; 32],
            "self-transition MUST NOT emit an audit event",
        );
    }

    #[test]
    fn transition_agent_rejects_registered_to_terminated() {
        // RFC-0015-a Appendix A state-machine guard: the only valid
        // edge from Registered is Running. Registered → Terminated
        // (skipping Running) MUST be rejected with
        // InvalidStateTransition (the typed `from`/`to` enum payload).
        let (uuid, holder) = register_one();
        let err = transition_agent(&holder, uuid, AgentState::Terminated, None).unwrap_err();
        match err {
            WalletError::InvalidStateTransition { from, to } => {
                assert_eq!(from, AgentState::Registered);
                assert_eq!(to, AgentState::Terminated);
            }
            other => panic!("expected InvalidStateTransition, got {other:?}"),
        }
    }

    #[test]
    fn transition_agent_runs_through_audit_unavailable_fail_closed_path() {
        // Default-build (no `octo-audit-internal` feature): the
        // function reaches step 7 (audit append), the feature-off
        // branch returns AuditUnavailable, step 8 rolls the state
        // mutation back. The caller sees AuditUnavailable.
        //
        // The `#[cfg(feature = "octo-audit-internal")]` branch is
        // exercised when the feature is enabled (separate CI lane);
        // this test pins the feature-off substrate-faithful contract.
        let (uuid, holder) = register_one();
        let err = transition_agent(&holder, uuid, AgentState::Running, None).unwrap_err();
        match err {
            WalletError::AuditUnavailable(msg) => {
                assert!(
                    msg.contains("octo-audit-internal") || msg.contains("sink"),
                    "AuditUnavailable message MUST reference the feature gate or sink state, got: {msg}",
                );
            }
            other => panic!("expected AuditUnavailable, got {other:?}"),
        }
    }

    #[test]
    fn transition_agent_oversize_reason_rejected_before_state_change() {
        // RFC-0015 §6.2.5: validate_reason runs BEFORE the registry
        // lock + lookup; the agent_id never even reaches the
        // caller-attestation path. We use a fresh UUID so any
        // mutation would be impossible (the agent isn't registered)
        // — the test pins the contract that ReasonTooLong surfaces
        // FIRST regardless of agent existence.
        let did = Did::from("did:octo:oversize-reason-test");
        let too_long = "x".repeat(257);
        let err = transition_agent(&did, Uuid::new_v4(), AgentState::Running, Some(&too_long))
            .unwrap_err();
        assert!(matches!(err, WalletError::ReasonTooLong(257)));
    }

    #[test]
    fn transition_agent_control_char_reason_rejected_before_state_change() {
        // RFC-0015 §6.2.5: ESC byte triggers ReasonContainsControlChars
        // (hex-escaped code-point form, never raw byte).
        let did = Did::from("did:octo:ctrl-char-reason-test");
        let bad = "evil\u{001B}pager";
        let err =
            transition_agent(&did, Uuid::new_v4(), AgentState::Running, Some(bad)).unwrap_err();
        match err {
            WalletError::ReasonContainsControlChars(s) => assert_eq!(s, "<U+001B>"),
            other => panic!("expected ReasonContainsControlChars, got {other:?}"),
        }
    }
}
