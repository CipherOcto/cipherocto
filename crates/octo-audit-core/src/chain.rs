//! Chain-integrity helpers per RFC-0012 §Design Goals G5.
//!
//! `verify_chain` validates a sequence of `AuditEvent`s for:
//! - Strict `event_id` monotonicity (gap detection)
//! - `chain_hash` field matches `BLAKE3-256(canonical_bytes)`
//! - Strict `at_millis_unix` monotonicity (timestamp regression detection)
//!
//! `compute_chain_hash` produces the canonical `chain_hash` for a given
//! event given its `prev_chain_hash`.

pub use crate::error::AuditChainError;
use crate::event::AuditEvent;

/// Canonical serialization form (RFC-0012 §Canonical Serialization):
///
/// `[event_id (BE u64) | node_did (UTF-8) | event_kind (tag byte) |
///   cap_root_hash (32 bytes) | at_millis_unix (BE u64) | prev_chain_hash (32 bytes)]`
///
/// `chain_hash` is excluded (it IS the hash of the other fields + the
/// `prev_chain_hash`).
pub fn canonical_bytes(event: &AuditEvent) -> Vec<u8> {
    let mut buf = Vec::with_capacity(8 + event.node_did.len() + 1 + 32 + 8 + 32);
    buf.extend_from_slice(&event.event_id.to_be_bytes());
    buf.extend_from_slice(event.node_did.as_bytes());
    buf.push(event_kind_tag(&event.event_kind));
    buf.extend_from_slice(&event.cap_root_hash);
    buf.extend_from_slice(&event.at_millis_unix.to_be_bytes());
    buf.extend_from_slice(&event.prev_chain_hash);
    buf
}

fn event_kind_tag(kind: &crate::event::AuditEventKind) -> u8 {
    use crate::event::AuditEventKind::*;
    match kind {
        Insert => 0,
        Revoke => 1,
        Sync => 2,
        // `#[non_exhaustive]` extension variants cannot exist yet (the
        // substrate owns this enum); if a future amendment adds variants,
        // update this match.
    }
}

/// Compute the `chain_hash` for `event` as `BLAKE3-256(canonical_bytes)`.
pub fn compute_chain_hash(event: &AuditEvent) -> [u8; 32] {
    *blake3::hash(&canonical_bytes(event)).as_bytes()
}

/// Verify a chain of audit events. Returns `Ok(())` on a valid chain;
/// returns `Err(AuditChainError)` on the first detected violation.
///
/// # Errors
///
/// - `SequenceGap` — `event_id` is not the successor of the previous event's
///   `event_id` (e.g. skip 5 → 7).
/// - `HashMismatch` — an event's `chain_hash` field does not match
///   `compute_chain_hash(event)`.
/// - `TimestampRegression` — `at_millis_unix` decreased between consecutive
///   events.
pub fn verify_chain(events: &[AuditEvent]) -> Result<(), AuditChainError> {
    let mut last_event_id: Option<u64> = None;
    let mut last_at_millis: Option<u64> = None;
    for event in events {
        // Monotonicity: event_id must be strictly increasing.
        if let Some(prev) = last_event_id {
            if event.event_id != prev + 1 {
                return Err(AuditChainError::SequenceGap {
                    event_id: event.event_id,
                    prev,
                });
            }
        }

        // Hash integrity: chain_hash field must match recomputed hash.
        let expected = compute_chain_hash(event);
        if event.chain_hash != expected {
            return Err(AuditChainError::HashMismatch {
                event_id: event.event_id,
            });
        }

        // Timestamp monotonicity: at_millis_unix must be strictly increasing.
        if let Some(prev) = last_at_millis {
            if event.at_millis_unix <= prev {
                return Err(AuditChainError::TimestampRegression {
                    event_id: event.event_id,
                    prev,
                    current: event.at_millis_unix,
                });
            }
        }

        last_event_id = Some(event.event_id);
        last_at_millis = Some(event.at_millis_unix);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_event(id: u64, at: u64, prev: [u8; 32]) -> AuditEvent {
        let mut e = AuditEvent {
            event_id: id,
            node_did: "did:oct:test".to_owned(),
            event_kind: crate::event::AuditEventKind::Insert,
            cap_root_hash: [0xab; 32],
            at_millis_unix: at,
            prev_chain_hash: prev,
            chain_hash: [0; 32],
        };
        e.chain_hash = compute_chain_hash(&e);
        e
    }

    #[test]
    fn chain_empty_is_valid() {
        assert!(verify_chain(&[]).is_ok());
    }

    #[test]
    fn chain_single_accepts() {
        let e = make_event(0, 1000, [0; 32]);
        assert!(verify_chain(std::slice::from_ref(&e)).is_ok());
    }

    #[test]
    fn chain_monotonic_10() {
        let mut prev_hash = [0; 32];
        let mut events = Vec::new();
        for i in 0..10 {
            let e = make_event(i, 1000 + i * 100, prev_hash);
            prev_hash = e.chain_hash;
            events.push(e);
        }
        assert!(verify_chain(&events).is_ok());
    }

    #[test]
    fn chain_gap_rejects() {
        let e0 = make_event(0, 1000, [0; 32]);
        let e2 = make_event(2, 1200, e0.chain_hash); // skip 1
        let err = verify_chain(&[e0, e2]).unwrap_err();
        assert!(matches!(err, AuditChainError::SequenceGap { .. }));
    }

    #[test]
    fn chain_hash_mismatch_rejects() {
        let e0 = make_event(0, 1000, [0; 32]);
        let mut e1 = make_event(1, 1100, e0.chain_hash);
        e1.chain_hash[0] ^= 1; // flip one byte
        let err = verify_chain(&[e0, e1]).unwrap_err();
        assert!(matches!(err, AuditChainError::HashMismatch { .. }));
    }

    #[test]
    fn chain_timestamp_regression_rejects() {
        let e0 = make_event(0, 2000, [0; 32]);
        let e1 = make_event(1, 1000, e0.chain_hash); // regression
        let err = verify_chain(&[e0, e1]).unwrap_err();
        assert!(matches!(err, AuditChainError::TimestampRegression { .. }));
    }
}
