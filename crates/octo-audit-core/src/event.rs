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
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuditEventKind {
    /// Capability inserted (RFC-0957 §Capability Lifecycle).
    Insert,
    /// Capability revoked (RFC-0957 §Capability Lifecycle).
    Revoke,
    /// Sync event (cross-replica state synchronization).
    Sync,
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
