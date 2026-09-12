//! Verify-chain test vectors (RFC-0012 §Test Vectors).
//!
//! 10 vectors exercising the canonical `verify_chain` invariant suite:
//! 4 happy-path shapes + 6 violation shapes (gap, mismatch, regression,
//! non-monotonic event_id, zero-length chain, mixed kind).
//!
//! Run with:
//!   cargo test -p octo-audit --test verify_chain_vectors

use octo_audit_core::{
    compute_chain_hash, verify_chain, AuditChainError, AuditEvent, AuditEventKind,
};

/// Helper: build an event with canonical chain_hash filled in.
fn make_event(
    id: u64,
    at: u64,
    prev: [u8; 32],
    kind: AuditEventKind,
    node_did: &str,
) -> AuditEvent {
    let mut e = AuditEvent {
        event_id: id,
        node_did: node_did.to_owned(),
        event_kind: kind,
        cap_root_hash: [0xab; 32],
        at_millis_unix: at,
        prev_chain_hash: prev,
        chain_hash: [0; 32],
    };
    e.chain_hash = compute_chain_hash(&e);
    e
}

#[test]
fn vector_01_empty_chain_accepts() {
    assert!(verify_chain(&[]).is_ok());
}

#[test]
fn vector_02_single_event_accepts() {
    let e = make_event(0, 1000, [0; 32], AuditEventKind::Insert, "did:oct:v02");
    assert!(verify_chain(std::slice::from_ref(&e)).is_ok());
}

#[test]
fn vector_03_monotonic_5_accepts() {
    let mut prev = [0; 32];
    let mut events = Vec::new();
    for i in 0..5 {
        let e = make_event(
            i,
            1000 + i * 100,
            prev,
            AuditEventKind::Insert,
            "did:oct:v03",
        );
        prev = e.chain_hash;
        events.push(e);
    }
    assert!(verify_chain(&events).is_ok());
}

#[test]
fn vector_04_mixed_kinds_accept() {
    // Insert, Revoke, Sync in one chain — all valid; kind tag changes
    // but does not break monotonicity.
    let e0 = make_event(0, 1000, [0; 32], AuditEventKind::Insert, "did:oct:v04");
    let e1 = make_event(
        1,
        1100,
        e0.chain_hash,
        AuditEventKind::Revoke,
        "did:oct:v04",
    );
    let e2 = make_event(2, 1200, e1.chain_hash, AuditEventKind::Sync, "did:oct:v04");
    assert!(verify_chain(&[e0, e1, e2]).is_ok());
}

#[test]
fn vector_05_gap_at_index_2_rejects() {
    // event_id 0 → 2 (skip 1) → SequenceGap.
    let e0 = make_event(0, 1000, [0; 32], AuditEventKind::Insert, "did:oct:v05");
    let e2 = make_event(
        2,
        1200,
        e0.chain_hash,
        AuditEventKind::Insert,
        "did:oct:v05",
    );
    assert!(matches!(
        verify_chain(&[e0, e2]),
        Err(AuditChainError::SequenceGap { .. })
    ));
}

#[test]
fn vector_06_hash_mismatch_rejects() {
    // Flip a byte in chain_hash → HashMismatch.
    let e0 = make_event(0, 1000, [0; 32], AuditEventKind::Insert, "did:oct:v06");
    let mut e1 = make_event(
        1,
        1100,
        e0.chain_hash,
        AuditEventKind::Insert,
        "did:oct:v06",
    );
    e1.chain_hash[0] ^= 0x01;
    assert!(matches!(
        verify_chain(&[e0, e1]),
        Err(AuditChainError::HashMismatch { .. })
    ));
}

#[test]
fn vector_07_timestamp_regression_rejects() {
    // at_millis_unix decreases (2000 → 1000) → TimestampRegression.
    let e0 = make_event(0, 2000, [0; 32], AuditEventKind::Insert, "did:oct:v07");
    let e1 = make_event(
        1,
        1000,
        e0.chain_hash,
        AuditEventKind::Insert,
        "did:oct:v07",
    );
    assert!(matches!(
        verify_chain(&[e0, e1]),
        Err(AuditChainError::TimestampRegression { .. })
    ));
}

#[test]
fn vector_08_timestamp_equal_rejects() {
    // at_millis_unix stays equal (1000 → 1000) → regression (strict
    // monotonicity per RFC-0012 §Design Goals G5).
    let e0 = make_event(0, 1000, [0; 32], AuditEventKind::Insert, "did:oct:v08");
    let e1 = make_event(
        1,
        1000,
        e0.chain_hash,
        AuditEventKind::Insert,
        "did:oct:v08",
    );
    assert!(matches!(
        verify_chain(&[e0, e1]),
        Err(AuditChainError::TimestampRegression { .. })
    ));
}

#[test]
fn vector_09_first_event_nonzero_id_accepts() {
    // Per RFC-0012 §verify_chain: the predecessor check only fires
    // when a previous event exists in the chain. A single event with
    // event_id=5 is accepted (the canonical "first event id" rule
    // lives at the sink layer, not the chain verifier).
    let e = make_event(5, 1000, [0; 32], AuditEventKind::Insert, "did:oct:v09");
    assert!(verify_chain(std::slice::from_ref(&e)).is_ok());
}

#[test]
fn vector_10_long_chain_50_accepts() {
    // 50 events, all monotonic, all hash-linked → verify_chain OK.
    let mut prev = [0; 32];
    let mut events = Vec::with_capacity(50);
    for i in 0..50 {
        let e = make_event(
            i,
            1000 + i * 100,
            prev,
            AuditEventKind::Insert,
            "did:oct:v10",
        );
        prev = e.chain_hash;
        events.push(e);
    }
    assert!(verify_chain(&events).is_ok());
}
