// RFC-0957-A1 §Future Work F3 — Audit log.
//
// Append-only log of insert / revoke / sync events on the HolderRegistry.
// Each entry is BLAKE3-chained against the previous entry's chain hash;
// tampering breaks the chain and is detectable on replay.
//
// Schema (per RFC-0862 substrate; per RFC-0957-A1 §F3):
//   event_id        : u64    (monotonic per node_did)
//   node_did        : String
//   event_kind      : AuditEventKind { Insert | Revoke | Sync }
//   cap_root_hash   : [u8;32] (PK of the HolderRecord referenced)
//   at_millis_unix  : u64
//   prev_chain_hash : [u8;32] (BLAKE3 of previous entry; [0;32] for entry 0)
//   chain_hash      : [u8;32] (BLAKE3 over canonical entry serialization)
//
// Storage: separate stoolap table `holder_registry_audit_log` (mission 0862).
// This mission ships the in-memory primitive + chain integrity check; the
// stoolap adapter lands in mission 0862.
//
// Security: `AuditEvent` Debug redaction is the load-bearing primitive for
// this module. `cap_root_hash` MUST be redacted from any forensic surface
// to prevent reputation-laundering via leaked audit history. `node_did` +
// `event_kind` are preserved for forensics.

use blake3::Hasher;

/// Audit event kind discriminant.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AuditEventKind {
    /// A new `HolderRecord` was inserted.
    Insert,
    /// An existing `HolderRecord` was revoked.
    Revoke,
    /// A federation sync landed a remote delta.
    Sync,
}

impl AuditEventKind {
    /// Stable string label (for log lines / metric labels).
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Insert => "insert",
            Self::Revoke => "revoke",
            Self::Sync => "sync",
        }
    }
}

/// Append-only audit event.
///
/// Manual `Debug` impl redacts `cap_root_hash` per RFC-0957-A1 §F3 security
/// note; `node_did` + `event_kind` are preserved for forensics.
#[derive(Clone, PartialEq, Eq)]
pub struct AuditEvent {
    /// Monotonic per `node_did`.
    pub event_id: u64,
    /// DID of the node that emitted the event.
    pub node_did: String,
    /// Event kind.
    pub event_kind: AuditEventKind,
    /// PK of the HolderRecord the event references.
    pub cap_root_hash: [u8; 32],
    /// Event timestamp in milliseconds (RFC-0957-A1 §Data Structures).
    pub at_millis_unix: u64,
    /// BLAKE3 of the previous entry in this `node_did` log; `[0;32]` for entry 0.
    pub prev_chain_hash: [u8; 32],
    /// BLAKE3 over canonical entry serialization.
    pub chain_hash: [u8; 32],
}

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

impl AuditEvent {
    /// Canonical serialization for BLAKE3 hashing:
    ///   `u32(8) || event_id (u64 BE) || u32(|node_did|) || node_did || u32(4) || event_kind (u32 BE)
    ///   || u32(32) || cap_root_hash || u32(8) || at_millis_unix || u32(32) || prev_chain_hash`
    ///
    /// Length-prefixed to prevent concatenation-collision attacks
    /// (matching `CapabilityToken::holder_msg` discipline per RFC-0957-A1 §Security).
    fn canonical_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(64 + self.node_did.len());
        out.extend_from_slice(&u32_field(8));
        out.extend_from_slice(&self.event_id.to_be_bytes());
        let nd_bytes = self.node_did.as_bytes();
        out.extend_from_slice(&u32_field(nd_bytes.len()));
        out.extend_from_slice(nd_bytes);
        out.extend_from_slice(&u32_field(4));
        out.extend_from_slice(&(self.event_kind as u32).to_be_bytes());
        out.extend_from_slice(&u32_field(32));
        out.extend_from_slice(&self.cap_root_hash);
        out.extend_from_slice(&u32_field(8));
        out.extend_from_slice(&self.at_millis_unix.to_be_bytes());
        out.extend_from_slice(&u32_field(32));
        out.extend_from_slice(&self.prev_chain_hash);
        out
    }

    /// Compute the BLAKE3 chain hash for this entry. Caller must set
    /// `prev_chain_hash` before calling.
    #[must_use]
    pub fn compute_chain_hash(&self) -> [u8; 32] {
        let mut hasher = Hasher::new();
        hasher.update(&self.canonical_bytes());
        *hasher.finalize().as_bytes()
    }
}

fn u32_field(n: usize) -> [u8; 4] {
    u32::try_from(n)
        .expect("audit canonical_ser field length fits in u32")
        .to_be_bytes()
}

/// Append a new audit event, computing `chain_hash` from `prev_chain_hash`.
///
/// Returns the new `AuditEvent` with all fields populated (including
/// `chain_hash`). Caller persists the event.
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
    event.chain_hash = event.compute_chain_hash();
    event
}

/// Errors from chain integrity verification.
#[derive(Debug, thiserror::Error)]
pub enum AuditChainError {
    /// A chain link is broken (`prev_chain_hash` of entry N does not equal
    /// `chain_hash` of entry N-1).
    #[error("audit chain broken at event_id={event_id}")]
    Broken {
        event_id: u64,
        prev: [u8; 32],
        actual: [u8; 32],
    },
    /// The chain hash of an entry does not match the recomputed hash.
    #[error("audit chain hash mismatch at event_id={event_id}")]
    HashMismatch {
        event_id: u64,
        stored: [u8; 32],
        recomputed: [u8; 32],
    },
}

/// Verify the integrity of an audit log chain.
///
/// Returns `Ok(())` if every entry's `prev_chain_hash` matches the previous
/// entry's `chain_hash` AND every entry's `chain_hash` matches its recomputed
/// hash. The first entry MUST have `prev_chain_hash == [0; 32]` (genesis).
///
/// `Ok(())` on empty slice — no entries, nothing to verify.
pub fn verify_chain(events: &[AuditEvent]) -> Result<(), AuditChainError> {
    let mut expected_prev = [0u8; 32];
    for event in events {
        if event.prev_chain_hash != expected_prev {
            return Err(AuditChainError::Broken {
                event_id: event.event_id,
                prev: expected_prev,
                actual: event.prev_chain_hash,
            });
        }
        let recomputed = event.compute_chain_hash();
        if recomputed != event.chain_hash {
            return Err(AuditChainError::HashMismatch {
                event_id: event.event_id,
                stored: event.chain_hash,
                recomputed,
            });
        }
        expected_prev = event.chain_hash;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let mut events = Vec::new();
        let insert = append_event(
            &octo_ident::test_helpers::sample_did(53),
            AuditEventKind::Insert,
            [0x42; 32],
            1_700_000_000_000,
            [0; 32],
            0,
        );
        events.push(insert.clone());
        let revoke = append_event(
            &octo_ident::test_helpers::sample_did(53),
            AuditEventKind::Revoke,
            [0x42; 32],
            1_700_000_001_000,
            insert.chain_hash,
            1,
        );
        events.push(revoke.clone());
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].event_kind, AuditEventKind::Insert);
        assert_eq!(events[1].event_kind, AuditEventKind::Revoke);
        // Chain must verify.
        assert!(verify_chain(&events).is_ok());
    }

    #[test]
    fn tampering_with_log_breaks_chain_check() {
        // TV F3: tampering fails BLAKE3 chain check.
        let mut events = append_n(&octo_ident::test_helpers::sample_did(53), 3);
        // Tamper: rewrite event[1].cap_root_hash. The recompute MUST detect.
        events[1].cap_root_hash = [0xFF; 32];
        let r = verify_chain(&events);
        assert!(matches!(r, Err(AuditChainError::HashMismatch { .. })));
    }

    #[test]
    fn chain_verify_accepts_genesis_prev_zero() {
        let events = append_n(&octo_ident::test_helpers::sample_did(53), 1);
        assert!(verify_chain(&events).is_ok());
    }

    #[test]
    fn empty_chain_verifies() {
        assert!(verify_chain(&[]).is_ok());
    }

    #[test]
    fn broken_prev_link_detected() {
        let mut events = append_n(&octo_ident::test_helpers::sample_did(53), 2);
        // Inject a wrong prev_chain_hash on event[1].
        events[1].prev_chain_hash = [0x99; 32];
        let r = verify_chain(&events);
        assert!(matches!(r, Err(AuditChainError::Broken { .. })));
    }

    #[test]
    fn audit_event_debug_redacts_cap_root_hash() {
        let event = append_event(
            &octo_ident::test_helpers::sample_did(53),
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
        assert!(
            s.contains(&octo_ident::test_helpers::sample_did(53)),
            "node_did missing: {s}"
        );
        assert!(s.contains("Insert"), "event_kind missing: {s}");
    }

    #[test]
    fn event_kind_labels_stable() {
        assert_eq!(AuditEventKind::Insert.as_str(), "insert");
        assert_eq!(AuditEventKind::Revoke.as_str(), "revoke");
        assert_eq!(AuditEventKind::Sync.as_str(), "sync");
    }

    #[test]
    fn chain_hash_is_deterministic() {
        let a = append_event(
            &octo_ident::test_helpers::sample_did(53),
            AuditEventKind::Insert,
            [0x33; 32],
            1_700_000_000_000,
            [0; 32],
            0,
        );
        let b = append_event(
            &octo_ident::test_helpers::sample_did(53),
            AuditEventKind::Insert,
            [0x33; 32],
            1_700_000_000_000,
            [0; 32],
            0,
        );
        assert_eq!(a.chain_hash, b.chain_hash);
    }
}
