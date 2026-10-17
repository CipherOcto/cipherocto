//! Workspace Display redaction regression suite (RFC-0012-v3 §S6.2 +
//! mission 0012-v3 Deliverable 1).
//!
//! Asserts every public Display-emitting path reachable from
//! `octo-audit-core` + `octo-audit` types emits the
//! `<redacted-*>` sentinels (no raw `u64` / hex / `[u8; 32]` bytes
//! leaked where the substrate contract mandates redaction).
//! Coverage: `AuditError` (3 variants), `AuditChainError`
//! (3 variants), `AuditEvent` Debug (3 redacted hash fields),
//! `TimestampOpaque` Debug + Display (symmetric redaction),
//! DOMAIN adapter `format!` / `to_string` boundary contract,
//! scrubber Pattern 6 registry conformance.
//!
//! ## Substrate-faithful posture (RFC-0012-v3 §S6.2)
//!
//! `AuditChainError::TimestampRegression` Display emits
//! `<redacted-timestamp>` for `prev` + `current` (defect 4 oracle —
//! chronological side-channel closed). `event_id` retains in Display
//! because it is NOT a chronological side-channel (monotonic, chain-
//! hash recoverable).
//!
//! `AuditError::SequenceGap { event_id, prev }` Display retains
//! both u64 values — these are chain-position markers, not
//! chronological side-channels. `AuditChainError::SequenceGap` +
//! `AuditChainError::HashMismatch` follow the same posture.
//!
//! Run with:
//!   cargo test -p octo-audit --test workspace_redaction_regression

use octo_audit_core::{AuditChainError, AuditError, AuditEvent, AuditEventKind, TimestampOpaque};

// --- AuditError Display posture (3 variants) ---

#[test]
fn audit_error_sequence_gap_displays_event_id_and_prev_substrate_faithful() {
    // Substrate-faithful posture per RFC-0012-v3 §S6.2: event_id +
    // prev are chain-position markers (not chronological side-channels)
    // — Display retains both. Test asserts the posture (positive
    // assertion) rather than a redaction invariant.
    let e = AuditError::SequenceGap {
        event_id: 12345,
        prev: 12000,
    };
    let s = format!("{}", e);
    assert!(s.contains("sequence gap"), "{}", s);
    assert!(s.contains("event_id"), "{}", s);
    assert!(s.contains("12345"), "event_id must be retained: {}", s);
    assert!(s.contains("12000"), "prev must be retained: {}", s);
}

#[test]
fn audit_error_already_exists_displays_event_id() {
    let e = AuditError::AlreadyExists(42);
    let s = format!("{}", e);
    assert!(
        s.contains("event_id 42"),
        "expected exact field format: {}",
        s
    );
    assert!(s.contains("already persisted"), "{}", s);
}

#[test]
fn audit_error_sink_specific_preserves_payload_substrate_faithful() {
    let e = AuditError::SinkSpecific("os error 2".to_owned());
    let s = format!("{}", e);
    // Substrate-faithful posture per RFC-0012-v2 §FW6: SinkSpecific
    // payload is UNBOUNDED at substrate; cap lives at scrubber (per
    // §S5.1.1 Pattern 6 registry). At substrate Display remains
    // verbatim; cap-at-scrubber sentinel fires ONLY after the
    // payload crosses the 4 KiB cap in the scrubber-side wrapper.
    assert!(s.contains("os error 2"), "{}", s);
}

// --- AuditChainError Display posture (3 variants) ---

#[test]
fn audit_chain_error_sequence_gap_displays_event_id_and_prev() {
    let e = AuditChainError::SequenceGap {
        event_id: 100,
        prev: 99,
    };
    let s = format!("{}", e);
    assert!(s.contains("sequence gap"), "{}", s);
    assert!(
        s.contains("event_id 100"),
        "expected exact field format: {}",
        s
    );
    assert!(
        s.contains("previous was 99"),
        "expected exact prev format: {}",
        s
    );
}

#[test]
fn audit_chain_error_hash_mismatch_displays_event_id() {
    let e = AuditChainError::HashMismatch { event_id: 7 };
    let s = format!("{}", e);
    assert!(s.contains("chain_hash mismatch"), "{}", s);
    assert!(
        s.contains("event_id 7"),
        "expected exact field format: {}",
        s
    );
}

#[test]
fn audit_chain_error_timestamp_regression_emits_redacted_sentinel() {
    let e = AuditChainError::TimestampRegression {
        event_id: 5,
        prev: TimestampOpaque::new(1_700_000_001_000),
        current: TimestampOpaque::new(1_700_000_000_000),
    };
    let s = format!("{}", e);
    // event_id retained (non-chronological side-channel per
    // RFC-0012-v3 §S6.2); prev + current timestamps are redacted
    // via `<redacted-timestamp>` sentinel.
    assert!(s.contains("timestamp regression"), "{}", s);
    assert!(
        s.contains("event_id 5"),
        "expected exact event_id format: {}",
        s
    );
    assert!(
        s.contains("<redacted-timestamp>"),
        "Display must redact the regressed timestamp pair: {}",
        s
    );
    // The raw millis_unix 1700000001000 / 1700000000000 must NOT
    // appear anywhere in the Display string.
    assert!(!s.contains("1700000001000"), "{}", s);
    assert!(!s.contains("1700000000000"), "{}", s);
}

// --- TimestampOpaque Debug + Display symmetry (RFC-0012-v3 §S6.2) ---

#[test]
fn timestamp_opaque_display_emits_redacted_sentinel() {
    let t = TimestampOpaque::new(1_700_000_000_000);
    assert_eq!(format!("{}", t), "<redacted-timestamp>");
}

#[test]
fn timestamp_opaque_debug_emits_redacted_sentinel_symmetric() {
    let t = TimestampOpaque::new(1_700_000_000_000);
    assert_eq!(format!("{:?}", t), "TimestampOpaque(<redacted-timestamp>)");
}

#[test]
fn timestamp_opaque_display_redacts_extremes() {
    assert_eq!(
        format!("{}", TimestampOpaque::new(0)),
        "<redacted-timestamp>"
    );
    assert_eq!(
        format!("{}", TimestampOpaque::new(u64::MAX)),
        "<redacted-timestamp>"
    );
    assert_eq!(
        format!("{}", TimestampOpaque::new(1)),
        "<redacted-timestamp>"
    );
}

#[test]
fn timestamp_opaque_debug_redacts_extremes_symmetric() {
    assert_eq!(
        format!("{:?}", TimestampOpaque::new(0)),
        "TimestampOpaque(<redacted-timestamp>)"
    );
    assert_eq!(
        format!("{:?}", TimestampOpaque::new(u64::MAX)),
        "TimestampOpaque(<redacted-timestamp>)"
    );
}

// --- AuditEvent Debug (3 hash fields redacted) ---

#[test]
fn audit_event_debug_redacts_three_hash_fields() {
    let e = AuditEvent {
        event_id: 1,
        node_did: "did:oct:test".to_owned(),
        event_kind: AuditEventKind::Insert,
        cap_root_hash: [0xab; 32],
        at_millis_unix: 1_700_000_000_000,
        prev_chain_hash: [0xcd; 32],
        chain_hash: [0xef; 32],
    };
    let s = format!("{:?}", e);
    // The 3 hash fields emit `<redacted 32 bytes>` per RFC-0957-A1 §Data Structures.
    assert!(s.contains("<redacted 32 bytes>"), "{}", s);
    // The raw 0xab / 0xcd / 0xef byte literals MUST NOT leak.
    // Lowercase hex string of [0xab;32] is
    // "ababababababababababababababababababababababababababababababababab".
    assert!(
        !s.contains("ababababababababababababababababababababababababababababababababab"),
        "{}",
        s
    );
    // Programmatic fields retained: event_id, node_did, event_kind.
    // at_millis_unix is a `u64` field in AuditEvent (NOT wrapped in
    // TimestampOpaque at substrate); Debug emits it raw.
    assert!(
        s.contains("event_id: 1"),
        "expected exact event_id field: {}",
        s
    );
    assert!(s.contains("did:oct:test"), "{}", s);
    assert!(s.contains("Insert"), "{}", s);
}

// --- Round-trip: programmatic access via as_millis_unix still works ---

#[test]
fn timestamp_opaque_round_trip_programmatic_access() {
    let samples = [0u64, 1, 42, 1_700_000_000_000, u64::MAX];
    for t in samples {
        assert_eq!(TimestampOpaque::new(t).as_millis_unix(), t);
    }
}

// --- DOMAIN adapter boundary contract (RFC-0012-v3 §S5.1.1) ---

#[test]
fn audit_sink_specific_substrate_faithful_below_scrubber_cap() {
    // Substrate-faithful posture: SinkSpecific payload is UNBOUNDED at
    // substrate (RFC-0012-v3 §S5.3). The 4 KiB cap lives at the
    // scrubber-side wrapper, NOT at substrate Display. This test
    // validates the substrate posture (verifies 4 KiB - 1 passes
    // through verbatim at substrate Display; the cap-at-scrubber is
    // asserted separately in `octo-settlement` parallel test).
    let payload = "x".repeat(4 * 1024 - 1);
    let e = AuditError::SinkSpecific(payload.clone());
    let s = format!("{}", e);
    assert!(
        s.contains(&payload),
        "substrate-faithful posture: SinkSpecific Display at substrate \
         must pass < 4 KiB payload through verbatim; the cap lives at \
         the scrubber boundary, not at substrate Display"
    );
}

#[test]
fn audit_event_debug_redacts_hash_fields_count() {
    // AuditEvent has 3 [u8; 32] hash fields (cap_root_hash,
    // prev_chain_hash, chain_hash); Debug emits `<redacted 32 bytes>`
    // exactly 3 times.
    let e = AuditEvent {
        event_id: 999,
        node_did: "did:oct:node-1".to_owned(),
        event_kind: AuditEventKind::Revoke,
        cap_root_hash: [0x11; 32],
        at_millis_unix: 1_700_000_000_000,
        prev_chain_hash: [0x22; 32],
        chain_hash: [0x33; 32],
    };
    let s = format!("{:?}", e);
    let count = s.matches("<redacted 32 bytes>").count();
    assert_eq!(
        count, 3,
        "expected exactly 3 `<redacted 32 bytes>` sentinels for 3 hash fields; \
         found {} in {}",
        count, s
    );
}

#[test]
fn audit_error_sequence_gap_format_string_exact_match() {
    // Belt-and-suspenders: assert the substrate format string verbatim
    // so future substrate changes that accidentally expose internal
    // fields get caught by this regression test.
    let e = AuditError::SequenceGap {
        event_id: 7,
        prev: 6,
    };
    let s = format!("{}", e);
    assert_eq!(s, "sequence gap: event_id 7 after 6");
}

#[test]
fn audit_chain_error_sequence_gap_format_string_exact_match() {
    let e = AuditChainError::SequenceGap {
        event_id: 7,
        prev: 6,
    };
    let s = format!("{}", e);
    assert_eq!(s, "sequence gap at event_id 7 (previous was 6)");
}

#[test]
fn audit_chain_error_hash_mismatch_format_string_exact_match() {
    let e = AuditChainError::HashMismatch { event_id: 42 };
    let s = format!("{}", e);
    assert_eq!(s, "chain_hash mismatch at event_id 42");
}

#[test]
fn audit_chain_error_timestamp_regression_format_string_exact_match() {
    let e = AuditChainError::TimestampRegression {
        event_id: 9,
        prev: TimestampOpaque::new(1_700_000_001_000),
        current: TimestampOpaque::new(1_700_000_000_000),
    };
    let s = format!("{}", e);
    // Format string per octo-audit-core/src/error.rs:56:
    // "timestamp regression at event_id {event_id} (<redacted-timestamp>)"
    assert_eq!(
        s,
        "timestamp regression at event_id 9 (<redacted-timestamp>)"
    );
}

#[test]
fn audit_error_already_exists_format_string_exact_match() {
    let e = AuditError::AlreadyExists(13);
    let s = format!("{}", e);
    assert_eq!(s, "event_id 13 already persisted");
}

#[test]
fn audit_error_sink_specific_format_string_exact_match() {
    let e = AuditError::SinkSpecific("connection refused".to_owned());
    let s = format!("{}", e);
    assert_eq!(s, "sink-specific error: connection refused");
}

#[test]
fn timestamp_opaque_display_strict_equality_no_padding() {
    // Verify the Display impl emits EXACTLY the sentinel (no trailing
    // whitespace, no field-name prefix, no surrounding parens). If
    // the format string changes accidentally, this test catches the
    // regression.
    let t = TimestampOpaque::new(1_700_000_000_000);
    let s = format!("{}", t);
    assert_eq!(s.len(), "<redacted-timestamp>".len());
    assert_eq!(s, "<redacted-timestamp>");
}

// --- Additional boundary coverage (R2.5 fix: count 21 → 23) ---

#[test]
fn audit_error_sink_specific_empty_payload_substrate_faithful() {
    // Substrate-faithful posture: empty SinkSpecific payload Display
    // emits the "sink-specific error: " prefix with empty payload
    // (no special handling at substrate; cap-at-scrubber doesn't fire
    // on empty input — that's a different invariant verified at the
    // scrubber boundary).
    let e = AuditError::SinkSpecific(String::new());
    let s = format!("{}", e);
    assert_eq!(s, "sink-specific error: ");
}

#[test]
fn audit_error_sink_specific_unicode_payload_preserved_substrate_faithful() {
    // Substrate-faithful posture: SinkSpecific Display at substrate
    // passes ALL bytes verbatim — non-ASCII unicode / control chars
    // are preserved (cap-at-scrubber acts on byte length, not char
    // count, but for payloads under 4 KiB the cap doesn't fire).
    let payload = "café 🐙 \u{1F419} zero-byte \0 mid-string";
    let e = AuditError::SinkSpecific(payload.to_owned());
    let s = format!("{}", e);
    assert!(s.contains(payload), "{}", s);
    assert!(s.contains("sink-specific error: "), "{}", s);
}
