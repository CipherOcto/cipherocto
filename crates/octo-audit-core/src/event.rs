//! Canonical `AuditEvent` struct + `AuditEventKind` enum per RFC-0012 §Module
//! Layout `event` + RFC-0957-A1 §F3 byte-identical field shape.

use serde::{Deserialize, Serialize};

/// Canonical audit event struct. Field shape byte-identical to
/// `crates/octo-wallet/src/capability/audit_log.rs` (RFC-0957-A1 §F3) +
/// RFC-0012 §Module Layout.
///
/// `node_did` is held as `String` (raw canonical DID wire form per
/// RFC-0010); canonical-form conversion happens at the domain call
/// boundary (the substrate stays free of `octo-ident` / Layer B deps
/// per CLAUDE.md §Architectural Principles layer direction rule).
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditEvent {
    /// Monotonic event identifier (per-stream; sink enforces monotonicity).
    pub event_id: u64,
    /// Subject node DID (raw canonical wire form; conversion lives at domain
    /// call boundary).
    pub node_did: String,
    /// Event kind tag (Insert / Revoke / Sync + extension variants).
    pub event_kind: AuditEventKind,
    /// Capability root hash (32-byte BLAKE3-256 digest; redacted in Debug).
    pub cap_root_hash: [u8; 32],
    /// Wall-clock timestamp in milliseconds since UNIX epoch.
    pub at_millis_unix: u64,
    /// Previous chain hash (32-byte BLAKE3-256 digest; `[0;32]` for first
    /// event in chain; redacted in Debug).
    pub prev_chain_hash: [u8; 32],
    /// Current chain hash (32-byte BLAKE3-256 digest of canonical bytes;
    /// redacted in Debug).
    pub chain_hash: [u8; 32],
}

/// Canonical audit event kind tag. `#[non_exhaustive]` per
/// CLAUDE.md §Extension over enumeration — extension variants land
/// via typed-discriminator namespaces (e.g. `CapabilityMint` /
/// `CapabilityAttenuate` from `octo-wallet`), NOT via central enum edits.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuditEventKind {
    /// Capability inserted (RFC-0957 §Capability Lifecycle).
    Insert,
    /// Capability revoked (RFC-0957 §Capability Lifecycle).
    Revoke,
    /// Sync event (cross-replica state synchronization).
    Sync,
    /// Agent lifecycle state transition (RFC-0015-a §6.1 write-path
    /// trio). Emitted by `transition_agent` (octo-wallet Layer B) on
    /// successful `Registered → Running` or `Running → Terminated`
    /// transitions. Carries the canonical UUID hex of the agent, the
    /// `from` + `to` state labels (lowercase, see `AgentState::as_str`),
    /// and an optional operator-supplied reason (already scrubbed via
    /// `validate_reason` upstream — no control characters present).
    ///
    /// # Layer discipline + cfg gating rationale
    ///
    /// The variant is gated behind `#[cfg(feature =
    /// "octo-audit-internal")]` per RFC-0015-a §6.4 paired-acceptance
    /// bridge contract: the Layer A frozen substrate (`octo-audit-core`)
    /// preserves its frozen contract for default builds (the variant is
    /// INVISIBLE in `cargo build` of dependent crates), and becomes
    /// permanent (cfg dropped) once RFC-0012-v2 is Accepted. Per
    /// RFC-0012-v3 §S5.2.1 Layer B/C label swap, this is a DOMAIN
    /// facade surface extension, not a substrate broadening.
    ///
    /// `agent_id` is held as `String` (canonical UUID hex form per
    /// RFC-0010) to keep `octo-audit-core` Layer A frozen free of the
    /// `uuid` crate (the canonical form is verified at the domain
    /// call boundary, see `octo-wallet::transition_agent`).
    #[cfg(feature = "octo-audit-internal")]
    AgentTransition {
        /// Canonical UUID hex of the transitioning agent
        /// (`AgentRecord::manifest.manifest_id`).
        agent_id: String,
        /// Previous lifecycle state label (`AgentState::as_str`).
        from: String,
        /// New lifecycle state label (`AgentState::as_str`).
        to: String,
        /// Operator-supplied reason (already scrubbed via
        /// `validate_reason`); `None` when no reason was provided.
        reason: Option<String>,
    },
}

/// Manual `Debug` impl redacts the 3 hash fields per RFC-0957-A1 §F3 +
/// RFC-0012 §Module Layout `event` redaction requirement. `format!("{:?}",
/// event)` shows `<redacted 32 bytes>` instead of raw hash bytes.
impl std::fmt::Debug for AuditEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuditEvent")
            .field("event_id", &self.event_id)
            .field("node_did", &self.node_did)
            .field("event_kind", &self.event_kind)
            .field("cap_root_hash", &"<redacted 32 bytes>")
            .field("at_millis_unix", &self.at_millis_unix)
            .field("prev_chain_hash", &"<redacted 32 bytes>")
            .field("chain_hash", &"<redacted 32 bytes>")
            .finish()
    }
}
