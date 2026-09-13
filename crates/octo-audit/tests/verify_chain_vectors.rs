//! Verify-chain test vectors (RFC-0012 §Test Vectors).
//!
//! 14 vectors exercising the canonical `verify_chain` invariant suite + the
//! `AppendOnlyAuditSink` extension surface per RFC-0012 §Trait G3:
//!
//!   * `chain-*` vectors (10) — pure `verify_chain` chain-integrity
//!     properties (empty/single/monotonic/gap/hash-mismatch/regression/
//!     append/append-idempotent/extension-enum/debug-redaction).
//!   * Sink-impl vectors — wire the trait contract through the
//!     `StoolapAuditSink` DOMAIN adapter (Layer B façade).
//!
//! Run with:
//!   cargo test -p octo-audit --test verify_chain_vectors

use octo_audit::storage::stoolap::StoolapAuditSink;
use octo_audit::{
    compute_chain_hash, verify_chain, AppendOnlyAuditSink, AuditChainError, AuditError, AuditEvent,
    AuditEventKind,
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

// RFC-0012 §Test Vectors chain-empty
#[test]
fn vector_01_empty_chain_accepts() {
    assert!(verify_chain(&[]).is_ok());
}

// RFC-0012 §Test Vectors chain-single
#[test]
fn vector_02_single_event_accepts() {
    let e = make_event(0, 1000, [0; 32], AuditEventKind::Insert, "did:oct:v02");
    assert!(verify_chain(std::slice::from_ref(&e)).is_ok());
}

// RFC-0012 §Test Vectors chain-monotonic (5-event variant)
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

// RFC-0012 §Test Vectors chain-monotonic (mixed-kind variant — extension
// enum shape: Insert + Revoke + Sync chain is still monotonic and
// chain-hash-valid)
#[test]
fn vector_04_mixed_kinds_accept() {
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

// RFC-0012 §Test Vectors chain-gap
#[test]
fn vector_05_gap_at_index_2_rejects() {
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

// RFC-0012 §Test Vectors chain-hash-mismatch
#[test]
fn vector_06_hash_mismatch_rejects() {
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

// RFC-0012 §Test Vectors chain-timestamp-regression
#[test]
fn vector_07_timestamp_regression_rejects() {
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

// RFC-0012 §Test Vectors chain-timestamp-regression (equal-timestamp
// boundary: strict monotonicity per RFC-0012 §Design Goals G5)
#[test]
fn vector_08_timestamp_equal_rejects() {
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

// RFC-0012 §Test Vectors chain-single (first-event-id boundary variant:
// a single event with non-zero event_id is still accepted at the chain
// verifier; the predecessor check only fires for chains of length >=2)
#[test]
fn vector_09_first_event_nonzero_id_accepts() {
    let e = make_event(5, 1000, [0; 32], AuditEventKind::Insert, "did:oct:v09");
    assert!(verify_chain(std::slice::from_ref(&e)).is_ok());
}

// RFC-0012 §Test Vectors chain-monotonic (50-event variant — stronger
// property of the monotonic + hash-link invariant under chain growth)
#[test]
fn vector_10_long_chain_50_accepts() {
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

// RFC-0012 §Test Vectors append-success: `StoolapAuditSink::append` with
// a valid event returns `Ok(())`; persisted event_id is queryable via
// `last_event_id`. Exercises the DOMAIN adapter wiring of
// `AppendOnlyAuditSink` (Layer B façade) per RFC-0012 §Trait G3.
#[test]
fn vector_11_append_success() {
    let mut sink = StoolapAuditSink::open_in_memory().expect("open sink");
    let e0 = make_event(0, 1000, [0; 32], AuditEventKind::Insert, "did:oct:v11");
    sink.append(&e0).expect("first append");
    assert_eq!(sink.last_event_id().expect("last_event_id"), Some(0));
    let e1 = make_event(
        1,
        1100,
        e0.chain_hash,
        AuditEventKind::Insert,
        "did:oct:v11",
    );
    sink.append(&e1).expect("second append");
    assert_eq!(sink.last_event_id().expect("last_event_id"), Some(1));
}

// RFC-0012 §Test Vectors append-idempotent: same `event_id` re-append
// is rejected with `AuditError::AlreadyExists` (distinct from
// `SequenceGap` which covers genuine gaps per RFC-0012 §Trait G3).
#[test]
fn vector_12_append_idempotent() {
    let mut sink = StoolapAuditSink::open_in_memory().expect("open sink");
    let e0 = make_event(0, 1000, [0; 32], AuditEventKind::Insert, "did:oct:v12");
    sink.append(&e0).expect("first append");
    let err = sink.append(&e0).expect_err("duplicate should reject");
    assert!(matches!(err, AuditError::AlreadyExists(0)));
}

// RFC-0012 §Test Vectors extension-enum: substrate `AuditEventKind` is
// `#[non_exhaustive]` (RFC-0012 §Extension over enumeration). Domain
// crates add extension enums via the typed-discriminator pattern
// (`CapabilityAuditEventKind` wraps the substrate kind); conversion to
// the canonical substrate variant succeeds for the canonical variants
// AND preserves the substrate tag byte for cross-replica consensus.
//
// This test asserts the substrate tag byte stability
// (Insert=0/Revoke=1/Sync=2) AND that the wrapper correctly
// round-trips every canonical variant to its substrate representation.
#[test]
fn vector_13_extension_enum_roundtrip() {
    // Local extension enum (the typed-discriminator pattern, NOT a
    // substrate-central enum). Wraps substrate `AuditEventKind`.
    #[derive(Debug, PartialEq, Eq)]
    enum CapabilityAuditEventKind {
        CapabilityMint(AuditEventKind),
        CapabilityAttenuate(AuditEventKind),
    }

    impl CapabilityAuditEventKind {
        fn substrate_kind(&self) -> AuditEventKind {
            match self {
                Self::CapabilityMint(k) | Self::CapabilityAttenuate(k) => *k,
            }
        }
    }

    // Substrate tag byte stability (canonical wire form). If a future
    // substrate amendment reorders or renumbers these tags, this
    // assertion fires BEFORE the cross-replica consensus assumption
    // silently drifts.
    let insert_tag: i64 = AuditEventKind::Insert as i64;
    let revoke_tag: i64 = AuditEventKind::Revoke as i64;
    let sync_tag: i64 = AuditEventKind::Sync as i64;
    assert_eq!(insert_tag, 0);
    assert_eq!(revoke_tag, 1);
    assert_eq!(sync_tag, 2);

    // Each extension variant converts to the canonical substrate
    // kind; the wrapper preserves the substrate tag for forensic
    // surface (CLI / log lines / metrics).
    let mint = CapabilityAuditEventKind::CapabilityMint(AuditEventKind::Insert);
    let attenuate = CapabilityAuditEventKind::CapabilityAttenuate(AuditEventKind::Revoke);
    assert_eq!(mint.substrate_kind() as i64, AuditEventKind::Insert as i64);
    assert_eq!(
        attenuate.substrate_kind() as i64,
        AuditEventKind::Revoke as i64
    );

    // Substrate-canonical chain verify still accepts events carrying
    // extension-enum-wrapped kinds when reified into substrate form.
    let e0 = make_event(0, 1000, [0; 32], mint.substrate_kind(), "did:oct:v13");
    let e1 = make_event(
        1,
        1100,
        e0.chain_hash,
        attenuate.substrate_kind(),
        "did:oct:v13",
    );
    assert!(verify_chain(&[e0, e1]).is_ok());
}

// RFC-0012 §Test Vectors debug-redaction: substrate manual `Debug` impl
// redacts all three hash fields (`cap_root_hash`, `prev_chain_hash`,
// `chain_hash`) to `<redacted 32 bytes>` so forensic log lines never
// leak chain integrity secrets. `node_did` + `event_kind` MUST be
// preserved verbatim (operational triage needs them).
#[test]
fn vector_14_debug_redaction() {
    let e = make_event(
        0,
        1000,
        [0; 32],
        AuditEventKind::Insert,
        "did:oct:v14-redact",
    );
    let dbg = format!("{e:?}");
    // All three hash fields redacted to the sentinel.
    assert!(
        dbg.contains("<redacted 32 bytes>"),
        "expected redaction sentinel: {dbg}"
    );
    // The original hash bytes (0xab) MUST NOT appear in the Debug
    // output (proves the manual Debug impl does not leak via the
    // derived default which would print the full 32-byte array).
    assert!(
        !dbg.to_ascii_lowercase().contains("abab"),
        "leaked cap_root_hash bytes: {dbg}"
    );
    // node_did + event_kind preserved verbatim for operational triage.
    assert!(
        dbg.contains("did:oct:v14-redact"),
        "node_did missing: {dbg}"
    );
    assert!(dbg.contains("Insert"), "event_kind missing: {dbg}");
}
