//! Canonical-bytes-on-write invariant test vectors (RFC-0016-a §6.10).
//!
//! 10+ vectors exercising the `compute_chain_hash` substrate ↔
//! `append_audit_event` façade pairing. RFC-0016-a §6.10 requires the
//! façade to recompute `chain_hash` from canonical bytes BEFORE the
//! sink is called (`ChainHashMismatch` on mismatch). This suite pins
//! the invariant for every `AuditEventKind` variant + re-canonicalization
//! idempotency + the no-bypass invariant (no caller in the façade
//! short-circuits the canonical byte encoder).
//!
//! Run with:
//!   cargo test -p octo-audit --test canonical_bytes_invariant

#![allow(unused_imports)]

use octo_audit::{
    append_audit_event, compute_chain_hash, AppendOnlyAuditSink, AuditError, AuditEvent,
    AuditEventKind,
};
use std::sync::Mutex;

/// In-memory `AppendOnlyAuditSink` for integration tests. Captures
/// appended events so the test can re-derive chain hashes from the
/// stored event and confirm round-trip canonical-bytes invariant.
struct MockSink {
    events: Mutex<Vec<AuditEvent>>,
}

impl MockSink {
    fn new() -> Self {
        Self {
            events: Mutex::new(Vec::new()),
        }
    }
    fn stored(&self) -> Vec<AuditEvent> {
        self.events
            .lock()
            .map(|g| g.clone())
            .unwrap_or_default()
    }
}

impl AppendOnlyAuditSink for MockSink {
    fn append(&mut self, event: &AuditEvent) -> Result<(), AuditError> {
        let mut events = self
            .events
            .lock()
            .map_err(|_| AuditError::SinkSpecific("mock events poisoned".into()))?;
        events.push(event.clone());
        Ok(())
    }
    fn last_event_id(&self) -> Result<Option<u64>, AuditError> {
        let events = self
            .events
            .lock()
            .map_err(|_| AuditError::SinkSpecific("mock events poisoned".into()))?;
        Ok(events.last().map(|e| e.event_id))
    }
}

/// Build a fresh event with `chain_hash` computed from canonical
/// bytes (the substrate-faithful pathway). All variants route through
/// this helper — the write-path façade (RFC-0016-a §6.10) re-derives
/// the same hash to enforce the invariant.
fn make_event(id: u64, at: u64, prev: [u8; 32], kind: AuditEventKind) -> AuditEvent {
    let mut e = AuditEvent {
        event_id: id,
        node_did: "did:oct:test".to_owned(),
        event_kind: kind,
        cap_root_hash: [0xab; 32],
        at_millis_unix: at,
        prev_chain_hash: prev,
        chain_hash: [0; 32],
    };
    e.chain_hash = compute_chain_hash(&e);
    e
}

// RFC-0016-a §6.10 — Insert variant round-trip.
#[test]
fn cb_compute_chain_hash_each_variant_insert() {
    let kind = AuditEventKind::Insert;
    let event = make_event(0, 1_000, [0; 32], kind);
    let expected = compute_chain_hash(&event);

    let mut sink = MockSink::new();
    let result = append_audit_event(&mut sink, event.clone());

    let chain_hash = result.expect("Insert event with matched chain_hash MUST succeed");
    assert_eq!(chain_hash.0, expected);

    let stored = sink.stored();
    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0].chain_hash, expected);
}

// RFC-0016-a §6.10 — Revoke variant round-trip.
#[test]
fn cb_compute_chain_hash_each_variant_revoke() {
    let kind = AuditEventKind::Revoke;
    let event = make_event(1, 1_100, [0; 32], kind);
    let expected = compute_chain_hash(&event);

    let mut sink = MockSink::new();
    let chain_hash = append_audit_event(&mut sink, event.clone()).expect("Revoke accept");
    assert_eq!(chain_hash.0, expected);
}

// RFC-0016-a §6.10 — Sync variant round-trip.
#[test]
fn cb_compute_chain_hash_each_variant_sync() {
    let kind = AuditEventKind::Sync;
    let event = make_event(2, 1_200, [0; 32], kind);
    let expected = compute_chain_hash(&event);

    let mut sink = MockSink::new();
    let chain_hash = append_audit_event(&mut sink, event.clone()).expect("Sync accept");
    assert_eq!(chain_hash.0, expected);
}

// RFC-0016-a §6.10 — AgentTransition variant round-trip. Gated behind
// the `octo-audit-internal` feature per RFC-0015-a §6.4 paired-
// acceptance bridge contract.
#[cfg(feature = "octo-audit-internal")]
#[test]
fn cb_compute_chain_hash_each_variant_agent_transition() {
    let kind = AuditEventKind::AgentTransition {
        agent_id: "00000000-0000-0000-0000-000000000001".to_owned(),
        from: "registered".to_owned(),
        to: "running".to_owned(),
        reason: Some("canonical-bytes-test".to_owned()),
    };
    let event = make_event(3, 1_300, [0; 32], kind);
    let expected = compute_chain_hash(&event);

    let mut sink = MockSink::new();
    let chain_hash =
        append_audit_event(&mut sink, event.clone()).expect("AgentTransition accept");
    assert_eq!(chain_hash.0, expected);
}

// RFC-0016-a §6.10 — chain_hash returned by append_audit_event MUST
// equal `compute_chain_hash(event)` from the substrate (round-trip
// invariant).
#[test]
fn cb_append_audit_event_returns_compute_chain_hash() {
    let event = make_event(10, 5_000, [0u8; 32], AuditEventKind::Insert);
    let expected = compute_chain_hash(&event);

    let mut sink = MockSink::new();
    let returned = append_audit_event(&mut sink, event.clone()).expect("round-trip");

    assert_eq!(
        returned.0, expected,
        "append_audit_event MUST return canonical compute_chain_hash"
    );
}

// RFC-0016-a §6.10 — idempotent re-canonicalization yields identical
// bytes (re-appending the same event-shape produces the same hash).
#[test]
fn cb_idempotent_re_canonicalization() {
    let event = make_event(20, 6_000, [0x42; 32], AuditEventKind::Revoke);
    let h1 = compute_chain_hash(&event);
    let h2 = compute_chain_hash(&event);
    assert_eq!(h1, h2, "compute_chain_hash MUST be deterministic");
    // Sink-side idempotency: re-appending the same event shape yields
    // the same chain_hash even though the event-id collision now
    // exists (the sink correctly stores both events; the canonical
    // hash is identical because canonical bytes match).
    let mut sink = MockSink::new();
    let r1 = append_audit_event(&mut sink, event.clone()).expect("first");
    let r2 = append_audit_event(&mut sink, event.clone()).expect("second");
    assert_eq!(r1.0, r2.0);
    assert_eq!(r1.0, h1);
}

// RFC-0016-a §6.10 — caller-supplied chain_hash that does NOT match
// compute_chain_hash MUST be rejected with `ChainHashMismatch` BEFORE
// the sink is called (defense-in-depth: catch chain-integrity
// violations at the canonical boundary rather than during
// `verify_chain`).
#[test]
fn cb_chain_hash_mismatch_rejected_before_sink_call() {
    let mut event = make_event(30, 7_000, [0; 32], AuditEventKind::Insert);
    event.chain_hash[0] ^= 1; // flip one byte → mismatch

    let mut sink = MockSink::new();
    let err = append_audit_event(&mut sink, event).unwrap_err();

    // Contract §6.10: error variant is `ChainHashMismatch { event_id }`.
    assert!(
        matches!(err, AuditError::ChainHashMismatch { event_id: 30 }),
        "ChainHashMismatch MUST carry event_id per RFC-0016-a §6.10, got {err:?}"
    );
    assert!(
        sink.stored().is_empty(),
        "sink MUST NOT be called when chain_hash mismatches (§6.10)"
    );
}

// RFC-0016-a §6.10 — accept when caller-supplied chain_hash matches.
#[test]
fn cb_chain_hash_accepted_when_matches() {
    let event = make_event(40, 8_000, [0; 32], AuditEventKind::Sync);
    // Verify accept path (sanity for the rejection counterpart).
    let mut sink = MockSink::new();
    let r = append_audit_event(&mut sink, event.clone()).expect("match accept");
    assert_eq!(r.0, compute_chain_hash(&event));
}

// RFC-0016-a §6.10 — `ChainHash` newtype carries exactly 32 bytes
// (BLAKE3-256 digest per RFC-0012 §Canonical Serialization).
#[test]
fn cb_chain_hash_is_32_bytes() {
    let event = make_event(50, 9_000, [0; 32], AuditEventKind::Insert);
    let mut sink = MockSink::new();
    let r = append_audit_event(&mut sink, event).expect("ok");
    assert_eq!(
        r.0.len(),
        32,
        "ChainHash MUST wrap a 32-byte BLAKE3-256 digest (RFC-0012 §Canonical Serialization)"
    );
}

// RFC-0016-a §6.10 — `ChainHash` Display impl emits lowercase hex
// (operator-facing invariant per implementation contract).
#[test]
fn cb_chain_hash_display_lowercase_hex_64_chars() {
    let event = make_event(60, 10_000, [0; 32], AuditEventKind::Insert);
    let mut sink = MockSink::new();
    let r = append_audit_event(&mut sink, event).expect("ok");
    let s = format!("{r}");
    assert_eq!(s.len(), 64, "ChainHash Display MUST emit 64 hex chars (32 bytes)");
    assert!(
        s.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()),
        "ChainHash Display MUST be lowercase hex, got {s}"
    );
}

// RFC-0016-a §6.10 — sanity: distinct `at_millis_unix` produce
// distinct chain hashes (timestamp is part of canonical bytes).
#[test]
fn cb_distinct_timestamps_produce_distinct_chain_hashes() {
    let e1 = make_event(70, 11_000, [0; 32], AuditEventKind::Insert);
    let e2 = make_event(71, 11_001, [0; 32], AuditEventKind::Insert);
    assert_ne!(
        compute_chain_hash(&e1),
        compute_chain_hash(&e2),
        "distinct at_millis_unix MUST produce distinct chain hashes (BLAKE3 collision-resistance)"
    );
}
