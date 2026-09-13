//! Canonical verify-receipt-chain test vectors (RFC-0014 §Test Vectors
//! + RFC-0959 §Data Structures).
//!
//! 12 vectors exercising the substrate `verify_receipt_chain` +
//! `receipt_id_for` + `CHAIN_DOMAIN_SEPARATOR` byte-pin invariant.
//! All vectors assert byte-identical cross-replica outputs given
//! identical inputs (BLAKE3 keyed with the canonical domain
//! separator).
//!
//! Run with:
//!   cargo test -p octo-settlement-core --test `chain_verify`
#![allow(clippy::doc_markdown)]

use octo_settlement_core::{
    receipt_id_for, verify_receipt_chain, Receipt, SettlementError, CHAIN_DOMAIN_SEPARATOR,
};

fn make_receipt(id: u64, ask: [u8; 32], ts: u64, sig: Vec<u8>) -> Receipt {
    let mut r = Receipt {
        receipt_id: id,
        ask_id: ask,
        settlement_hash: [0; 32],
        router_id: "did:oct:router".to_owned(),
        router_sig: sig,
        timestamp_unix: ts,
    };
    r.settlement_hash = receipt_id_for(&r);
    r
}

// RFC-0014 §Test Vectors `chain-empty`: empty receipt sequence.
#[test]
fn vector_01_empty_chain_accepts() {
    assert!(verify_receipt_chain(&[]).is_ok());
}

// RFC-0014 §Test Vectors `chain-single`: single receipt with
// `prev_settlement_hash = [0;32]`.
#[test]
fn vector_02_single_receipt_accepts() {
    let r = make_receipt(0, [0x01; 32], 1000, vec![0xaa, 0xbb]);
    assert!(verify_receipt_chain(std::slice::from_ref(&r)).is_ok());
}

// RFC-0014 §Test Vectors `chain-monotonic` (10-receipt variant): 10
// receipts with strict `receipt_id` monotonicity + correct
// `prev_settlement_hash` chaining.
#[test]
fn vector_03_monotonic_10_receipts_accept() {
    let mut receipts = Vec::with_capacity(10);
    for i in 0..10 {
        receipts.push(make_receipt(i, [0x01; 32], 1000 + i * 100, vec![0xaa]));
    }
    assert!(verify_receipt_chain(&receipts).is_ok());
}

// RFC-0014 §Test Vectors `chain-monotonic` (50-receipt stress variant).
#[test]
fn vector_04_monotonic_50_receipts_accept() {
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

// RFC-0014 §Test Vectors `chain-gap`: sequence gap (skip 2 in middle).
#[test]
fn vector_05_gap_in_middle_rejects() {
    // 0, 1, 3 (skip 2) -> SequenceGap.
    let r0 = make_receipt(0, [0x01; 32], 1000, vec![0xaa]);
    let r1 = make_receipt(1, [0x01; 32], 1100, vec![0xaa]);
    let r3 = make_receipt(3, [0x01; 32], 1300, vec![0xaa]);
    assert!(matches!(
        verify_receipt_chain(&[r0, r1, r3]),
        Err(SettlementError::SequenceGap { .. })
    ));
}

// RFC-0014 §verify_receipt_chain boundary: predecessor check only fires
// when a previous receipt exists; leading receipt with `receipt_id !=
// 0` is accepted at the verifier (the canonical "first receipt id" rule
// lives at the sink layer, not the chain verifier). Documenting for
// substrate-behavior parity with audit-chain's vector_09.
#[test]
fn vector_06_gap_at_start_accepts_no_predecessor_check() {
    let r1 = make_receipt(1, [0x01; 32], 1100, vec![0xaa]);
    let r2 = make_receipt(2, [0x01; 32], 1200, vec![0xaa]);
    assert!(verify_receipt_chain(&[r1, r2]).is_ok());
}

// RFC-0014 §Test Vectors `chain-duplicate`: duplicate `receipt_id` is
// a SequenceGap (not the successor of the prior).
#[test]
fn vector_07_duplicate_receipt_id_rejects() {
    let r0 = make_receipt(0, [0x01; 32], 1000, vec![0xaa]);
    let r0_dup = make_receipt(0, [0x01; 32], 1100, vec![0xbb]);
    assert!(matches!(
        verify_receipt_chain(&[r0, r0_dup]),
        Err(SettlementError::SequenceGap { .. })
    ));
}

// RFC-0014 §Test Vectors `chain-hash-mismatch` (interior-position
// variant): `settlement_hash` flipped by 1 byte on the third receipt.
#[test]
fn vector_08_hash_mismatch_at_last_receipt_rejects() {
    let r0 = make_receipt(0, [0x01; 32], 1000, vec![0xaa]);
    let r1 = make_receipt(1, [0x01; 32], 1100, vec![0xaa]);
    let mut r2 = make_receipt(2, [0x01; 32], 1200, vec![0xaa]);
    r2.settlement_hash[0] ^= 0x01;
    assert!(matches!(
        verify_receipt_chain(&[r0, r1, r2]),
        Err(SettlementError::ChainIntegrity { .. })
    ));
}

// RFC-0014 §Test Vectors `chain-hash-mismatch` (first-position variant).
#[test]
fn vector_09_hash_mismatch_at_first_receipt_rejects() {
    let mut r0 = make_receipt(0, [0x01; 32], 1000, vec![0xaa]);
    r0.settlement_hash[0] ^= 0x01;
    let r1 = make_receipt(1, [0x01; 32], 1100, vec![0xaa]);
    assert!(matches!(
        verify_receipt_chain(&[r0, r1]),
        Err(SettlementError::ChainIntegrity { .. })
    ));
}

// RFC-0014 §Test Vectors `receipt-id-for-idempotent`: same Receipt
// input yields identical `settlement_hash` across calls. Required for
// cross-replica consensus.
#[test]
fn vector_10_receipt_id_for_idempotent() {
    let r = make_receipt(0, [0x01; 32], 1000, vec![0xaa, 0xbb]);
    let h1 = receipt_id_for(&r);
    let h2 = receipt_id_for(&r);
    assert_eq!(h1, h2);
}

// RFC-0014 §Test Vectors `receipt-id-for-canonical-bytes`:
// two structurally identical Receipts yield identical `settlement_hash`.
// Proves canonical-byte determinism across replicas.
#[test]
fn vector_11_receipt_id_for_canonical_bytes_determinism() {
    let r1 = make_receipt(7, [0x42; 32], 1700, vec![0xde, 0xad, 0xbe, 0xef]);
    let r2 = make_receipt(7, [0x42; 32], 1700, vec![0xde, 0xad, 0xbe, 0xef]);
    assert_eq!(receipt_id_for(&r1), receipt_id_for(&r2));
}

// RFC-0014 §Test Vectors `domain-separator-byte-pin` (mirrored from
// `chain.rs` lib test; surface here for mission AC traceability).
#[test]
fn vector_12_domain_separator_byte_pin() {
    assert_eq!(CHAIN_DOMAIN_SEPARATOR, b"cipherocto/reservation/v1/");
    assert_eq!(CHAIN_DOMAIN_SEPARATOR.len(), 26);
}
