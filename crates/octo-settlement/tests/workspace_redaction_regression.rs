//! Workspace Display redaction regression suite
//! (RFC-0014-v3 §S5 + mission 0014-v3 Deliverable 1).
//!
//! Asserts every public Display-emitting path reachable from
//! `octo-settlement` + the shadow 8-variant `SettlementError` at
//! `quota-router-sm-engine` (Layer C specialized node) emits
//! `<redacted-hash>` sentinels for hash fields (no raw `[u8; 32]`
//! bytes leaked). Coverage: canonical substrate 7 variants + shadow
//! 8 variants + `SettlementHashOpaque` Debug + Display (symmetric).
//!
//! ## Substrate §FW2a paired-acceptance DEFERRED
//!
//! The canonical `octo_settlement_core::SettlementError::AskNotFound`
//! + `AlreadyConsumed` variants still use raw `[u8; 32]` + `{0:?}`
//! Display format-string vector per RFC-0014-v3 §FW2a (paired
//! substrate amendment DEFERRED to a future RFC-0014-v3.1 round).
//! Those two variants are EXCLUDED from the no-raw-byte assertion
//! below; the substrate defect is documented and tracked. All other
//! variants (5 canonical + all 8 shadow) must emit redacted
//! sentinels.
//!
//! Run with:
//!   cargo test -p octo-settlement --test workspace_redaction_regression

use octo_settlement_core::SettlementError as CanonicalSettlementError;
use octo_settlement_core::SettlementHashOpaque;
use quota_router_sm_engine::AskState;
use quota_router_sm_engine::SettlementError as ShadowSettlementError;

// --- Shadow 8-variant SettlementError Display redaction (Layer C) ---

#[test]
fn shadow_ask_not_found_redacts_hash() {
    let e = ShadowSettlementError::AskNotFound(SettlementHashOpaque::new([0xab; 32]));
    let s = format!("{}", e);
    assert!(s.contains("<redacted-hash>"), "{}", s);
    assert!(
        !s.contains("ababababababababababababababababababababababababababababababababab"),
        "{}",
        s
    );
}

#[test]
fn shadow_already_consumed_redacts_hash() {
    let e = ShadowSettlementError::AlreadyConsumed(SettlementHashOpaque::new([0xcd; 32]));
    let s = format!("{}", e);
    assert!(s.contains("<redacted-hash>"), "{}", s);
}

#[test]
fn shadow_invalid_transition_displays_state_pair() {
    let e = ShadowSettlementError::InvalidTransition {
        from: AskState::Minted,
        to: AskState::Consumed,
    };
    let s = format!("{}", e);
    assert!(s.contains("invalid state transition"), "{}", s);
    assert!(s.contains("Minted"), "{}", s);
    assert!(s.contains("Consumed"), "{}", s);
}

#[test]
fn shadow_reservation_not_found_displays_message() {
    let e = ShadowSettlementError::ReservationNotFound("res-123".to_owned());
    let s = format!("{}", e);
    assert!(s.contains("res-123"), "{}", s);
}

#[test]
fn shadow_reservation_expired_displays_message() {
    let e = ShadowSettlementError::ReservationExpired("res-456".to_owned());
    let s = format!("{}", e);
    assert!(s.contains("res-456"), "{}", s);
}

#[test]
fn shadow_invalid_reservation_transition_displays_states() {
    let e = ShadowSettlementError::InvalidReservationTransition {
        from: quota_router_sm_engine::ReservationState::Reserved,
        to: quota_router_sm_engine::ReservationState::Released,
    };
    let s = format!("{}", e);
    assert!(s.contains("invalid reservation transition"), "{}", s);
    assert!(s.contains("Reserved"), "{}", s);
    assert!(s.contains("Released"), "{}", s);
}

#[test]
fn shadow_settlement_hash_mismatch_redacts_both_hashes() {
    let e = ShadowSettlementError::SettlementHashMismatch {
        expected: SettlementHashOpaque::new([0x11; 32]),
        got: SettlementHashOpaque::new([0x22; 32]),
    };
    let s = format!("{}", e);
    assert!(s.contains("settlement hash mismatch"), "{}", s);
    assert!(s.contains("<redacted-hash>"), "{}", s);
    // Raw 0x11 / 0x22 byte hex MUST NOT leak via Display.
    assert!(
        !s.contains("1111111111111111111111111111111111111111111111111111111111111111"),
        "{}",
        s
    );
    assert!(
        !s.contains("2222222222222222222222222222222222222222222222222222222222222222"),
        "{}",
        s
    );
}

#[test]
fn shadow_storage_error_delegates_to_storage_error_display() {
    let e = ShadowSettlementError::Storage(quota_router_sm_engine::StorageError::Decode(
        "decode failed".to_owned(),
    ));
    let s = format!("{}", e);
    assert!(s.contains("storage error"), "{}", s);
    assert!(s.contains("decode failed"), "{}", s);
}

// --- Canonical substrate SettlementError Display redaction (Layer A) ---

#[test]
fn canonical_invalid_transition_displays_state_strings() {
    let e = CanonicalSettlementError::InvalidTransition {
        from: "Minted".to_owned(),
        to: "Consumed".to_owned(),
    };
    let s = format!("{}", e);
    assert!(s.contains("invalid state transition"), "{}", s);
    assert!(s.contains("Minted"), "{}", s);
    assert!(s.contains("Consumed"), "{}", s);
}

#[test]
fn canonical_sequence_gap_displays_receipt_ids() {
    let e = CanonicalSettlementError::SequenceGap {
        receipt_id: 100,
        prev: 99,
    };
    let s = format!("{}", e);
    assert!(s.contains("sequence gap"), "{}", s);
    assert!(s.contains("100"), "{}", s);
    assert!(s.contains("99"), "{}", s);
}

#[test]
fn canonical_chain_integrity_displays_receipt_id() {
    let e = CanonicalSettlementError::ChainIntegrity { receipt_id: 42 };
    let s = format!("{}", e);
    assert!(s.contains("chain integrity violation"), "{}", s);
    assert!(s.contains("42"), "{}", s);
}

#[test]
fn canonical_already_exists_displays_receipt_id() {
    let e = CanonicalSettlementError::AlreadyExists(7);
    let s = format!("{}", e);
    assert!(s.contains("7"), "{}", s);
    assert!(s.contains("already persisted"), "{}", s);
}

#[test]
fn canonical_sink_specific_preserves_payload_substrate_faithful() {
    let e = CanonicalSettlementError::SinkSpecific("stoolap tx aborted".to_owned());
    let s = format!("{}", e);
    // Substrate-faithful posture per RFC-0014-v3 §S5.3 + §FW3:
    // SinkSpecific payload is UNBOUNDED at substrate; cap lives at
    // scrubber (4 KiB input cap, 4 KiB output cap). Display remains
    // verbatim at substrate; cap-at-scrubber sentinel fires ONLY
    // after the scrubber-side wrapper sees > 4 KiB input.
    assert!(s.contains("stoolap tx aborted"), "{}", s);
}

// --- §FW2a paired-acceptance DEFERRED (substrate defect tracked) ---

#[test]
fn canonical_ask_not_found_substrate_defect_documented() {
    // §FW2a paired-acceptance DEFERRED: canonical substrate variant
    // still uses raw `[u8; 32]` + `{0:?}` Display format-string
    // vector. We do NOT assert no-raw-bytes here; we document the
    // defect + assert the EXISTENCE of the raw-byte leak so future
    // paired-acceptance amendment round (RFC-0014-v3.1) can verify
    // the closure. Once §FW2a lands, this test flips from
    // `documented` to `closed`.
    //
    // `{0:?}` on `[u8; 32]` emits decimal byte values (Rust Debug
    // form `[151, 151, ...]`). `0x99` = 153 (decimal); repeated 32x.
    let e = CanonicalSettlementError::AskNotFound([0x99; 32]);
    let s = format!("{}", e);
    assert!(s.contains("ask not found"), "{}", s);
    // The raw bytes leak via Display (substrate defect, tracked).
    // We assert the decimal form of `0x99` (= 153) appears in the
    // output. Once §FW2a lands + the format string flips to `{0}`
    // (Display via SettlementHashOpaque), this assertion will fail
    // and the paired-acceptance amendment closes the defect.
    assert!(
        s.contains("153"),
        "expected the §FW2a DEFERRED substrate defect to leak the raw byte values via {{0:?}}; \
         if this assertion fails the paired-acceptance amendment has landed: {}",
        s
    );
}

#[test]
fn canonical_already_consumed_substrate_defect_documented() {
    // §FW2a paired-acceptance DEFERRED — symmetric to AskNotFound.
    // `0x88` = 136 (decimal); `{0:?}` emits `[136, 136, ...]`.
    let e = CanonicalSettlementError::AlreadyConsumed([0x88; 32]);
    let s = format!("{}", e);
    assert!(s.contains("already consumed"), "{}", s);
    assert!(
        s.contains("136"),
        "expected the §FW2a DEFERRED substrate defect to leak the raw byte values via {{0:?}}: {}",
        s
    );
}

// --- SettlementHashOpaque Debug + Display symmetry (RFC-0014-v3 §S5.2) ---

#[test]
fn settlement_hash_opaque_display_emits_redacted_sentinel() {
    let h = SettlementHashOpaque::new([0xab; 32]);
    assert_eq!(format!("{}", h), "<redacted-hash>");
}

#[test]
fn settlement_hash_opaque_debug_emits_redacted_sentinel_symmetric() {
    let h = SettlementHashOpaque::new([0xab; 32]);
    assert_eq!(format!("{:?}", h), "SettlementHashOpaque(<redacted-hash>)");
}

#[test]
fn settlement_hash_opaque_display_redacts_extremes() {
    assert_eq!(
        format!("{}", SettlementHashOpaque::new([0; 32])),
        "<redacted-hash>"
    );
    assert_eq!(
        format!("{}", SettlementHashOpaque::new([0xff; 32])),
        "<redacted-hash>"
    );
    assert_eq!(
        format!("{}", SettlementHashOpaque::new([0x42; 32])),
        "<redacted-hash>"
    );
}

#[test]
fn shadow_ask_not_found_redacts_hash_with_specific_byte() {
    // Additional coverage: pick a byte (0x77 = 119 decimal) NOT used in
    // `shadow_ask_not_found_redacts_hash` (0xab) to widen the byte
    // coverage of the redacted-Display invariant.
    let e = ShadowSettlementError::AskNotFound(SettlementHashOpaque::new([0x77; 32]));
    let s = format!("{}", e);
    assert!(s.contains("<redacted-hash>"), "{}", s);
    assert!(
        !s.contains("7777777777777777777777777777777777777777777777777777777777777777"),
        "raw bytes must not leak via Display: {}",
        s
    );
}

#[test]
fn shadow_settlement_hash_mismatch_redacts_with_distinct_hashes() {
    // Belt-and-suspenders: distinct non-overlapping byte values per
    // field to assert redaction covers all 32-byte slots.
    let e = ShadowSettlementError::SettlementHashMismatch {
        expected: SettlementHashOpaque::new([0xa1; 32]),
        got: SettlementHashOpaque::new([0xb2; 32]),
    };
    let s = format!("{}", e);
    assert!(s.contains("settlement hash mismatch"), "{}", s);
    assert!(s.contains("<redacted-hash>"), "{}", s);
    assert!(
        !s.contains("a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1"),
        "expected hash bytes must not leak: {}",
        s
    );
    assert!(
        !s.contains("b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2"),
        "got hash bytes must not leak: {}",
        s
    );
}

#[test]
fn canonical_ask_not_found_substrate_defect_documented_specific_marker() {
    // Tightened from R1-S review: assert the Rust Debug tuple form
    // `[153, 153, ...]` (decimal byte values) — more specific to the
    // raw-byte-leak defect than bare `s.contains("153")`.
    let e = CanonicalSettlementError::AskNotFound([0x99; 32]);
    let s = format!("{}", e);
    assert!(s.contains("ask not found"), "{}", s);
    assert!(
        s.contains("[153,") || s.contains("153, 153"),
        "expected the §FW2a DEFERRED substrate defect to leak raw byte values via \
         {{0:?}} emitting the Rust Debug tuple form `[153, 153, ...]`; if this assertion \
         fails the paired-acceptance amendment has landed: {}",
        s
    );
}

#[test]
fn canonical_already_consumed_substrate_defect_documented_specific_marker() {
    let e = CanonicalSettlementError::AlreadyConsumed([0x88; 32]);
    let s = format!("{}", e);
    assert!(s.contains("already consumed"), "{}", s);
    assert!(
        s.contains("[136,") || s.contains("136, 136"),
        "expected the §FW2a DEFERRED substrate defect to leak raw byte values via \
         {{0:?}} emitting the Rust Debug tuple form `[136, 136, ...]`: {}",
        s
    );
}

#[test]
fn shadow_already_consumed_redacts_with_specific_byte() {
    // Widened byte coverage for the redacted-Display invariant.
    let e = ShadowSettlementError::AlreadyConsumed(SettlementHashOpaque::new([0x5e; 32]));
    let s = format!("{}", e);
    assert!(s.contains("<redacted-hash>"), "{}", s);
    assert!(
        !s.contains("5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e"),
        "raw bytes must not leak via Display: {}",
        s
    );
}
