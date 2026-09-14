//! Canonical `ChainHash` newtype + `append_audit_event` write-path
//! façade (RFC-0016-a §6.2 + §6.3).
//!
//! `ChainHash(pub [u8; 32])` is the canonical BLAKE3 chain-hash of an
//! `AuditEvent` row, returned by `append_audit_event` for downstream
//! verification + cross-substrate reference (display form is lowercase
//! hex per RFC-0012 canonical wire form).
//!
//! `append_audit_event(sink: &mut AppendOnlyAuditSink, event: AuditEvent) -> Result<ChainHash, AuditError>`
//! is the Layer B façade write path that wraps the Layer A frozen
//! substrate trait (`AppendOnlyAuditSink::append`). The Rust borrow
//! checker enforces single-writer per sink instance at the type level
//! (`&mut self`) per RFC-0012-v2 + RFC-0016-a §6.11 read-stall-while-write
//! invariant — concurrent readers (`list_receipts`, `get_receipt` from
//! RFC-0016 KEEP) block for the duration of the write.
//!
//! ## Layer discipline
//!
//! Lives in `octo-audit` (Layer B façade). Depends on
//! `octo-audit-core` (Layer A frozen) + `octo-settlement` (Layer B →
//! Layer B per RFC-0014 §Module Layout for `ReceiptId` re-export per
//! RFC-0016-a §6.4).

use std::fmt;

use octo_audit_core::{AppendOnlyAuditSink, AuditError, AuditEvent};

/// Canonical BLAKE3-256 chain-hash of an `AuditEvent` row
/// (RFC-0016-a §6.3 + RFC-0012 §Trait G3).
///
/// `Display` emits the canonical lowercase hex form (RFC-0012
/// canonical wire form). The `0x` prefix is NOT emitted (matches the
/// `compute_chain_hash` output in `octo-audit-core::chain`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ChainHash(pub [u8; 32]);

impl ChainHash {
    /// Borrow the inner 32-byte array.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Canonical lowercase hex form (no `0x` prefix).
    #[must_use]
    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }
}

impl fmt::Display for ChainHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", hex::encode(self.0))
    }
}

/// Append a pre-constructed `AuditEvent` to a `&mut AppendOnlyAuditSink`
/// (RFC-0016-a §6.2 + RFC-0012 §Trait G3 single-writer lock + §6.11
/// read-stall-while-write invariant).
///
/// Returns the canonical `ChainHash` on success. The Rust borrow
/// checker enforces single-writer per sink instance at the type level
/// (`&mut self`) so concurrent writers must serialize via an external
/// `Mutex` (out of scope per RFC-0016-a §Implicit Assumptions
/// Audit #5). Concurrent readers (`list_receipts`, `get_receipt` from
/// RFC-0016 KEEP) block for the duration of the write per RFC-0016-a
/// §6.11.
///
/// # Errors
///
/// - `AuditError::SequenceGap` / `AuditError::AlreadyExists` — surfaced
///   from the underlying `AppendOnlyAuditSink::append` call per
///   RFC-0012 §Trait G3.
pub fn append_audit_event(
    sink: &mut dyn AppendOnlyAuditSink,
    event: AuditEvent,
) -> Result<ChainHash, AuditError> {
    sink.append(&event)?;
    Ok(ChainHash(event.chain_hash))
}

#[cfg(test)]
mod tests {
    use super::*;
    use octo_audit_core::{compute_chain_hash, AuditEventKind};
    use std::sync::Mutex;

    struct MockSink {
        events: Mutex<Vec<AuditEvent>>,
        last_id: Mutex<Option<u64>>,
    }

    impl MockSink {
        fn new() -> Self {
            Self {
                events: Mutex::new(Vec::new()),
                last_id: Mutex::new(None),
            }
        }
    }

    impl AppendOnlyAuditSink for MockSink {
        fn append(&mut self, event: &AuditEvent) -> Result<(), AuditError> {
            let mut events = self
                .events
                .lock()
                .map_err(|_| AuditError::SinkSpecific("mock events poisoned".into()))?;
            let mut last_id = self
                .last_id
                .lock()
                .map_err(|_| AuditError::SinkSpecific("mock last_id poisoned".into()))?;
            *last_id = Some(event.event_id);
            events.push(event.clone());
            Ok(())
        }
        fn last_event_id(&self) -> Result<Option<u64>, AuditError> {
            let last_id = self
                .last_id
                .lock()
                .map_err(|_| AuditError::SinkSpecific("mock last_id poisoned".into()))?;
            Ok(*last_id)
        }
    }

    fn make_event(id: u64) -> AuditEvent {
        AuditEvent {
            event_id: id,
            node_did: "did:octo:test".to_string(),
            event_kind: AuditEventKind::Sync,
            cap_root_hash: [0u8; 32],
            at_millis_unix: 1_700_000_000,
            prev_chain_hash: [0u8; 32],
            chain_hash: [0u8; 32],
        }
    }

    #[test]
    fn chain_hash_display_emits_lowercase_hex() {
        let mut bytes = [0u8; 32];
        bytes[0] = 0xAB;
        bytes[1] = 0xCD;
        bytes[31] = 0xEF;
        let h = ChainHash(bytes);
        let s = format!("{h}");
        assert_eq!(s.len(), 64, "hex form MUST be exactly 64 chars");
        assert!(s.starts_with("abcd"), "lowercase hex prefix, got: {s}");
        assert!(s.ends_with("ef"), "lowercase hex suffix, got: {s}");
    }

    #[test]
    fn chain_hash_to_hex_matches_display() {
        let bytes = [0x42u8; 32];
        let h = ChainHash(bytes);
        assert_eq!(h.to_hex(), format!("{h}"));
    }

    #[test]
    fn append_audit_event_returns_chain_hash_from_sink() {
        let mut sink = MockSink::new();
        let mut event = make_event(0);
        // Substrate-faithful: the sink assigns chain_hash on append
        // (RFC-0012-v2 canonical-bytes-on-write invariant). Mirror
        // that in the MockSink by computing it before append.
        event.chain_hash = compute_chain_hash(&event);
        let expected = event.chain_hash;
        let result = append_audit_event(&mut sink, event).expect("append succeeds");
        assert_eq!(
            result.0, expected,
            "ChainHash MUST equal compute_chain_hash(event)"
        );
    }

    #[test]
    fn append_audit_event_propagates_sink_error() {
        // Sink that always errors on append.
        struct FailingSink;
        impl AppendOnlyAuditSink for FailingSink {
            fn append(&mut self, _event: &AuditEvent) -> Result<(), AuditError> {
                Err(AuditError::SinkSpecific("intentional failure".into()))
            }
            fn last_event_id(&self) -> Result<Option<u64>, AuditError> {
                Ok(None)
            }
        }
        let mut sink = FailingSink;
        let result = append_audit_event(&mut sink, make_event(0));
        assert!(
            matches!(result, Err(AuditError::SinkSpecific(msg)) if msg == "intentional failure")
        );
    }
}
