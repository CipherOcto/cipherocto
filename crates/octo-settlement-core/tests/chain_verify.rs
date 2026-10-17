//! `verify_receipt_chain` test vectors (RFC-0014 §Test Vectors).
//!
//! Split into two sections per substrate-faithful test policy:
//!
//! - **RFC canonical** — vectors that exercise canonical RFC-0014
//!   §Test Vectors IDs (`chain-empty`, `chain-monotonic`,
//!   `chain-settlement-hash-mismatch`,
//!   `receipt-compute-receipt-id-stable`,
//!   `receipt-canonical-bytes-stable`). Each test carries the
//!   canonical RFC ID alias comment.
//!
//! - **Mission-defined supplementary** — vectors exercising substrate
//!   code paths not covered by a canonical RFC-0014 §Test Vectors
//!   entry (boundary cases, stress variants, duplicate-id path).
//!   These carry a `mission-defined:` prefix in the alias comment
//!   and explicitly disclose that they are NOT canonical RFC IDs.
//!
//! `domain-separator-byte-pin` is exercised in `domain_separator.rs`
//! (single canonical surface per substrate-faithful test policy — no
//! duplication here).
//!
//! Run with:
//!   cargo test -p octo-settlement-core --test chain_verify
#![allow(clippy::doc_markdown)]

use octo_settlement_core::{receipt_id_for, verify_receipt_chain, Receipt, SettlementError};

fn make_receipt(id: u64, ask: [u8; 32], ts: u64, sig: Vec<u8>) -> Receipt {
    let mut r = Receipt {
        receipt_id: id,
        ask_id: ask,
        settlement_hash: [0; 32],
        router_id: "did:oct:router".to_owned(),
        router_sig: sig,
        timestamp_unix: ts,
        ..Default::default()
    };
    r.settlement_hash = receipt_id_for(&r);
    r
}

// === RFC canonical vectors ===

// RFC-0014 §Test Vectors `chain-empty`: empty receipt sequence.
#[test]
fn vector_01_chain_empty_accepts() {
    assert!(verify_receipt_chain(&[]).is_ok());
}

// RFC-0014 §Test Vectors `chain-monotonic` (10-receipt variant).
#[test]
fn vector_02_chain_monotonic_10_accepts() {
    let mut receipts = Vec::with_capacity(10);
    for i in 0..10 {
        receipts.push(make_receipt(i, [0x01; 32], 1000 + i * 100, vec![0xaa]));
    }
    assert!(verify_receipt_chain(&receipts).is_ok());
}

// RFC-0014 §Test Vectors `chain-settlement-hash-mismatch` (interior-
// position variant): `settlement_hash` flipped by 1 byte on the third
// receipt.
#[test]
fn vector_03_chain_settlement_hash_mismatch_interior_rejects() {
    let r0 = make_receipt(0, [0x01; 32], 1000, vec![0xaa]);
    let r1 = make_receipt(1, [0x01; 32], 1100, vec![0xaa]);
    let mut r2 = make_receipt(2, [0x01; 32], 1200, vec![0xaa]);
    r2.settlement_hash[0] ^= 0x01;
    assert!(matches!(
        verify_receipt_chain(&[r0, r1, r2]),
        Err(SettlementError::ChainIntegrity { .. })
    ));
}

// RFC-0014 §Test Vectors `chain-settlement-hash-mismatch` (first-
// position variant).
#[test]
fn vector_04_chain_settlement_hash_mismatch_first_rejects() {
    let mut r0 = make_receipt(0, [0x01; 32], 1000, vec![0xaa]);
    r0.settlement_hash[0] ^= 0x01;
    let r1 = make_receipt(1, [0x01; 32], 1100, vec![0xaa]);
    assert!(matches!(
        verify_receipt_chain(&[r0, r1]),
        Err(SettlementError::ChainIntegrity { .. })
    ));
}

// RFC-0014 §Test Vectors `receipt-compute-receipt-id-stable`: same
// Receipt input yields identical `settlement_hash` across calls.
// Required for cross-replica consensus.
#[test]
fn vector_05_receipt_compute_receipt_id_stable() {
    let r = make_receipt(0, [0x01; 32], 1000, vec![0xaa, 0xbb]);
    let h1 = receipt_id_for(&r);
    let h2 = receipt_id_for(&r);
    assert_eq!(h1, h2);
}

// RFC-0014 §Test Vectors `receipt-canonical-bytes-stable`: two
// structurally identical Receipts yield identical `settlement_hash`.
// Proves canonical-byte determinism across replicas.
#[test]
fn vector_06_receipt_canonical_bytes_stable() {
    let r1 = make_receipt(7, [0x42; 32], 1700, vec![0xde, 0xad, 0xbe, 0xef]);
    let r2 = make_receipt(7, [0x42; 32], 1700, vec![0xde, 0xad, 0xbe, 0xef]);
    assert_eq!(receipt_id_for(&r1), receipt_id_for(&r2));
}

// === Mission-defined supplementary vectors ===
// (not canonical RFC-0014 §Test Vectors IDs; documented for substrate
// code-path coverage)

// mission-defined: single-receipt chain (1-receipt boundary of
// `chain-monotonic`; RFC-0014 has no explicit single-event vector).
#[test]
fn mission_01_single_receipt_accepts() {
    let r = make_receipt(0, [0x01; 32], 1000, vec![0xaa, 0xbb]);
    assert!(verify_receipt_chain(std::slice::from_ref(&r)).is_ok());
}

// mission-defined: 50-receipt stress variant of `chain-monotonic`.
#[test]
fn mission_02_chain_monotonic_50_accepts() {
    let mut receipts = Vec::with_capacity(50);
    for i in 0..50 {
        receipts.push(make_receipt(
            i,
            [((i % 256) as u8); 32],
            1000 + i * 100,
            vec![i as u8],
        ));
    }
    assert!(verify_receipt_chain(&receipts).is_ok());
}

// mission-defined: sequence gap detection. RFC-0014 §Test Vectors has
// no `chain-gap` entry (substrate-faithful: receipt_id monotonicity is
// enforced at the sink layer per §Trait G3, not the chain verifier).
// This vector documents the verifier's own gap-detection path.
#[test]
fn mission_03_sequence_gap_rejects() {
    let r0 = make_receipt(0, [0x01; 32], 1000, vec![0xaa]);
    let r1 = make_receipt(1, [0x01; 32], 1100, vec![0xaa]);
    let r3 = make_receipt(3, [0x01; 32], 1300, vec![0xaa]);
    assert!(matches!(
        verify_receipt_chain(&[r0, r1, r3]),
        Err(SettlementError::SequenceGap { .. })
    ));
}

// mission-defined: predecessor check boundary — chain verifier only
// checks `receipt_id == prev + 1` when a previous receipt exists in
// the slice. Leading `receipt_id != 0` is accepted at the verifier
// (the canonical "first receipt id" rule lives at the sink layer).
#[test]
fn mission_04_leading_nonzero_receipt_id_accepted() {
    let r1 = make_receipt(1, [0x01; 32], 1100, vec![0xaa]);
    let r2 = make_receipt(2, [0x01; 32], 1200, vec![0xaa]);
    assert!(verify_receipt_chain(&[r1, r2]).is_ok());
}

// mission-defined: duplicate `receipt_id` collapses to SequenceGap
// (substrate-faithful: the verifier does not distinguish duplicates
// from out-of-order; both surface as SequenceGap).
#[test]
fn mission_05_duplicate_receipt_id_rejects_as_sequence_gap() {
    let r0 = make_receipt(0, [0x01; 32], 1000, vec![0xaa]);
    let r0_dup = make_receipt(0, [0x01; 32], 1100, vec![0xbb]);
    assert!(matches!(
        verify_receipt_chain(&[r0, r0_dup]),
        Err(SettlementError::SequenceGap { .. })
    ));
}
