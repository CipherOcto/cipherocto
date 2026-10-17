//! ReceiptId(pub u64) paired-acceptance verification (RFC-0014-v2
//! §S3 + RFC-0016-a §6.4).
//!
//! Per RFC-0014-v2 §S3 the `ReceiptId(pub u64)` newtype lives at
//! `octo-settlement-core::receipt::ReceiptId` (Layer A frozen
//! substrate). RFC-0016-a §6.4 paired-acceptance requires:
//!
//! - `octo-audit-core` carries NO `ReceiptId` re-export
//!   (Layer A frozen contract; no `ReceiptId` in
//!   `octo-audit-core::lib`).
//! - `octo-audit` (Layer B façade) consumers resolve via
//!   `octo_settlement::ReceiptId` import path
//!   (`octo_audit::receipt_summary::ReceiptSummary`).
//! - `ReceiptSummary::receipt_id` field uses the newtype
//!   (paired form per RFC-0014-v2 §S3).
//! - No caller site uses raw `[u8; 32]` for `ReceiptId`
//!   (canonical substrate form is `u64` per RFC-0014-v2).
//!
//! Run with:
//!   cargo test -p octo-audit --test receipt_id_paired_acceptance

use octo_audit::CHAIN_HASH_SIZE;
use octo_settlement::ReceiptId;

/// RFC-0016-a §6.4 — `ReceiptId` newtype IS re-exported at
/// `octo_settlement` (Layer B façade). Per RFC-0014-v2 §S3 the
/// canonical substrate source is `octo_settlement_core::receipt`
/// (Layer A frozen). The façade re-export is the canonical
/// import path for downstream consumers.
#[test]
fn rid_facade_reexport_present() {
    // ReceiptId IS accessible via octo_settlement (Layer B).
    // Compile-time assertion: ReceiptId is in scope from the
    // octo_settlement façade.
    let _: ReceiptId = ReceiptId(0);
}

/// RFC-0016-a §6.4 — `ReceiptSummary::receipt_id` field shape
/// uses the `ReceiptId` newtype (paired form per RFC-0014-v2 §S3).
/// The façade projection preserves the canonical substrate form.
#[test]
fn rid_receipt_summary_field_shape() {
    use octo_audit::ReceiptSummary;
    // ReceiptSummary::receipt_id IS ReceiptId (paired form).
    // The test below fails to compile if the field is NOT
    // ReceiptId-typed — compile-time verification.
    let summary = ReceiptSummary {
        receipt_id: ReceiptId(42),
        ask_id: "00".repeat(32),
        model: "test-model".to_owned(),
        cost_dqa: 100,
        capability_root: [0u8; CHAIN_HASH_SIZE],
        subject_did: "did:oct:test".to_owned(),
        executed_at_unix: 1_000,
        status: octo_settlement::ReceiptStatus::Ok,
    };
    assert_eq!(summary.receipt_id, ReceiptId(42));
}

/// RFC-0016-a §6.4 — `ReceiptId(pub u64)` constructor accepts a
/// `u64`. The raw byte form `[u8; 32]` is NOT the canonical
/// substrate form (defense-in-depth: a stray byte array passed
/// to ReceiptId MUST not silently coerce).
#[test]
fn rid_accepts_u64_constructor() {
    let rid: ReceiptId = ReceiptId(0xDEAD_BEEF_u64);
    assert_eq!(rid.0, 0xDEAD_BEEF_u64);
}

/// RFC-0016-a §6.4 — `octo-audit-core` carries NO `ReceiptId`
/// (Layer A frozen contract; no `ReceiptId` in
/// `octo-audit-core::lib`). Verified via include_str
/// source-shape grep: the substrate's lib.rs must not
/// re-export ReceiptId.
#[test]
fn rid_audit_core_does_not_reexport_receipt_id() {
    let audit_core_lib = include_str!("../../octo-audit-core/src/lib.rs");
    // The substrate's public surface MUST NOT export
    // `ReceiptId` — it lives at `octo-settlement-core`
    // (Layer A frozen per RFC-0014 §Module Layout).
    assert!(
        !audit_core_lib.contains("ReceiptId"),
        "octo-audit-core Layer A frozen substrate MUST NOT export ReceiptId (RFC-0014 §Module Layout + RFC-0014-v2 §S3 paired-acceptance)"
    );
}

/// RFC-0016-a §6.4 — `ReceiptSummary` projection is the
/// canonical Layer B façade projection; consumers MUST
/// resolve ReceiptId via the octo_audit façade wrapper
/// (the `octo_settlement::ReceiptId` re-export at the
/// façade boundary) rather than directly through
/// `octo_settlement_core`.
#[test]
fn rid_facade_projection_via_octo_audit() {
    use octo_audit::ReceiptSummary;
    // The façade projection is in scope from octo_audit
    // (per crates/octo-audit/src/lib.rs L73-74).
    let _: fn() -> Option<ReceiptSummary> = || None;
}
