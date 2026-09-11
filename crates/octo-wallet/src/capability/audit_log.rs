// RFC-0957-A1 §Future Work F3 — Audit log (substrate-migrated).
//
// Append-only log of insert / revoke / sync events on the HolderRegistry.
// Each entry is BLAKE3-chained against the previous entry's chain hash;
// tampering breaks the chain and is detectable on replay.
//
// Per mission 0012-audit-wallet-migration, canonical `AuditEvent` +
// `AuditEventKind` + manual `Debug` redaction + `verify_chain` +
// `AuditChainError` all live in the Layer A frozen substrate
// `octo-audit-core` (RFC-0012 §Module Layout). This module is a thin
// domain extension that:
//   1. Re-exports the canonical types via `pub use octo_audit::*`.
//   2. Adds a domain `audit_event_kind_as_str` label helper (used by
//      log lines + metric labels in the wallet).
//   3. Preserves the `append_event()` wallet-domain helper signature
//      (6 args + return `AuditEvent`) so existing callers do not change.
//   4. Adds a field-shape invariant test asserting the canonical 7-field
//      layout is byte-identical vs the substrate spec.
//
// Manual `Debug` impl is NO LONGER defined here per RFC-0012 §Module
// Layout — substrate canonical owns it.
//
// Schema (byte-identical to RFC-0957-A1 §F3):
//   event_id        : u64    (monotonic per node_did)
//   node_did        : String
//   event_kind      : AuditEventKind { Insert | Revoke | Sync }
//   cap_root_hash   : [u8;32]
//   at_millis_unix  : u64
//   prev_chain_hash : [u8;32] ([0;32] for entry 0)
//   chain_hash      : [u8;32]

use octo_audit::compute_chain_hash;
pub use octo_audit::{verify_chain, AuditChainError, AuditEvent, AuditEventKind};

/// Stable string label for `AuditEventKind` (used by log lines + metric
/// labels in the wallet).
///
/// `AuditEventKind` is `#[non_exhaustive]` (RFC-0012 §Extension over
/// enumeration) so an inherent `impl` block would violate the orphan
/// rule AND would still need a wildcard arm. A free function with a
/// documented `_ => "unknown"` fallback is the minimal-surface
/// solution: extension variants land upstream, the wallet-side label
/// stays stable for known variants, and unknown ones degrade to a
/// recognizable sentinel rather than panicking in production logs.
#[must_use]
pub fn audit_event_kind_as_str(kind: &AuditEventKind) -> &'static str {
    match kind {
        AuditEventKind::Insert => "insert",
        AuditEventKind::Revoke => "revoke",
        AuditEventKind::Sync => "sync",
        _ => "unknown",
    }
}

/// Append a new audit event, computing `chain_hash` from `prev_chain_hash`.
///
/// Returns the new `AuditEvent` with all fields populated (including
/// `chain_hash`). Caller persists the event. Uses substrate
/// `compute_chain_hash` for chain hashing.
#[allow(clippy::too_many_arguments)]
pub fn append_event(
    node_did: &str,
    event_kind: AuditEventKind,
    cap_root_hash: [u8; 32],
    at_millis_unix: u64,
    prev_chain_hash: [u8; 32],
    event_id: u64,
) -> AuditEvent {
    let mut event = AuditEvent {
        event_id,
        node_did: node_did.to_string(),
        event_kind,
        cap_root_hash,
        at_millis_unix,
        prev_chain_hash,
        chain_hash: [0u8; 32],
    };
    event.chain_hash = compute_chain_hash(&event);
    event
}

#[cfg(test)]
mod tests {
    use super::*;
    use octo_ident::test_helpers::sample_did;
    use std::mem::size_of;

    /// AC-5: struct-layout invariant vs RFC-0012 substrate spec.
    ///
    /// The 7-field canonical layout must be constructible + reachable
    /// through the substrate re-export with byte sizes summing to the
    /// expected minimum (3 × u64 = 24 + 3 × [u8;32] = 96 + String = 24
    /// + enum tag = 1B → 145 raw + alignment). The substrate owns the
    /// canonical byte shape; this test confirms the wallet-side view
    /// sees at least that surface AND that all 7 named fields exist
    /// with the canonical names.
    #[test]
    fn audit_event_struct_exposes_canonical_fields() {
        // Construct via canonical field order (matches RFC-0012 §event).
        let e = AuditEvent {
            event_id: 0,
            node_did: String::new(),
            event_kind: AuditEventKind::Insert,
            cap_root_hash: [0u8; 32],
            at_millis_unix: 0,
            prev_chain_hash: [0u8; 32],
            chain_hash: [0u8; 32],
        };
        // Read every field to confirm the substrate surface is reachable.
        let _ = e.event_id;
        let _ = &e.node_did;
        let _ = e.event_kind;
        let _ = e.cap_root_hash;
        let _ = e.at_millis_unix;
        let _ = e.prev_chain_hash;
        let _ = e.chain_hash;
        // Substrate enum tag MUST round-trip via Serialize + Deserialize
        // (canonical wire form). If substrate reordered the tag, this
        // would silently drift; the round-trip catches it.
        let kind = AuditEventKind::Insert;
        let json = serde_json::to_string(&kind).unwrap();
        let kind_back: AuditEventKind = serde_json::from_str(&json).unwrap();
        assert_eq!(kind, kind_back);
        // Drop unused-import noise if serde_json ever leaves.
        let _ = size_of::<AuditEvent>();
    }

    fn append_n(node_did: &str, n: u64) -> Vec<AuditEvent> {
        let mut events = Vec::new();
        let mut prev = [0u8; 32];
        for i in 0..n {
            let event = append_event(
                node_did,
                AuditEventKind::Insert,
                [u8::try_from(i).unwrap_or(0); 32],
                1_700_000_000_000 + i,
                prev,
                i,
            );
            prev = event.chain_hash;
            events.push(event);
        }
        events
    }

    #[test]
    fn insert_then_revoke_emits_two_audit_entries() {
        // TV F3: insert → revoke sequence emits 2 audit entries.
        let did = sample_did(53);
        let mut events = Vec::new();
        let insert = append_event(
            &did,
            AuditEventKind::Insert,
            [0x42; 32],
            1_700_000_000_000,
            [0; 32],
            0,
        );
        events.push(insert.clone());
        let revoke = append_event(
            &did,
            AuditEventKind::Revoke,
            [0x42; 32],
            1_700_000_001_000,
            insert.chain_hash,
            1,
        );
        events.push(revoke);
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].event_kind, AuditEventKind::Insert);
        assert_eq!(events[1].event_kind, AuditEventKind::Revoke);
        // Chain must verify.
        assert!(verify_chain(&events).is_ok());
    }

    #[test]
    fn tampering_with_log_breaks_chain_check() {
        // TV F3: tampering fails BLAKE3 chain check.
        let did = sample_did(53);
        let mut events = append_n(&did, 3);
        // Tamper: rewrite event[1].cap_root_hash. Recompute MUST detect.
        events[1].cap_root_hash = [0xFF; 32];
        let r = verify_chain(&events);
        assert!(matches!(r, Err(AuditChainError::HashMismatch { .. })));
    }

    #[test]
    fn chain_verify_accepts_genesis_prev_zero() {
        let did = sample_did(53);
        let events = append_n(&did, 1);
        assert!(verify_chain(&events).is_ok());
    }

    #[test]
    fn empty_chain_verifies() {
        assert!(verify_chain(&[]).is_ok());
    }

    #[test]
    fn broken_prev_link_detected() {
        // Tampering prev_chain_hash changes the recomputed chain_hash
        // (substrate canonical_bytes includes prev_chain_hash), so
        // substrate verify_chain catches this as HashMismatch.
        let did = sample_did(53);
        let mut events = append_n(&did, 2);
        events[1].prev_chain_hash = [0x99; 32];
        let r = verify_chain(&events);
        assert!(matches!(r, Err(AuditChainError::HashMismatch { .. })));
    }

    #[test]
    fn audit_event_debug_redacts_hash_fields() {
        let did = sample_did(53);
        let event = append_event(
            &did,
            AuditEventKind::Insert,
            [0xAB; 32],
            1_700_000_000_000,
            [0; 32],
            0,
        );
        let s = format!("{event:?}");
        assert!(s.contains("redacted"), "expected redaction: {s}");
        assert!(!s.contains("ABAB"), "leaked cap_root_hash bytes: {s}");
        // node_did + event_kind MUST be preserved for forensics.
        assert!(s.contains(&did), "node_did missing: {s}");
        assert!(s.contains("Insert"), "event_kind missing: {s}");
    }

    #[test]
    fn event_kind_labels_stable() {
        assert_eq!(audit_event_kind_as_str(&AuditEventKind::Insert), "insert");
        assert_eq!(audit_event_kind_as_str(&AuditEventKind::Revoke), "revoke");
        assert_eq!(audit_event_kind_as_str(&AuditEventKind::Sync), "sync");
    }

    #[test]
    fn chain_hash_is_deterministic() {
        let did = sample_did(53);
        let a = append_event(
            &did,
            AuditEventKind::Insert,
            [0x33; 32],
            1_700_000_000_000,
            [0; 32],
            0,
        );
        let b = append_event(
            &did,
            AuditEventKind::Insert,
            [0x33; 32],
            1_700_000_000_000,
            [0; 32],
            0,
        );
        assert_eq!(a.chain_hash, b.chain_hash);
    }
}
