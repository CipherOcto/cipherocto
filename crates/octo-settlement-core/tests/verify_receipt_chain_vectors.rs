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
//!   cargo test -p octo-settlement-core --test verify_receipt_chain_vectors

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

#[test]
fn vector_01_empty_chain_accepts() {
    assert!(verify_receipt_chain(&[]).is_ok());
}

#[test]
fn vector_02_single_receipt_accepts() {
    let r = make_receipt(0, [0x01; 32], 1000, vec![0xaa, 0xbb]);
    assert!(verify_receipt_chain(std::slice::from_ref(&r)).is_ok());
}

#[test]
fn vector_03_monotonic_10_receipts_accept() {
    let mut receipts = Vec::with_capacity(10);
    for i in 0..10 {
        receipts.push(make_receipt(i, [0x01; 32], 1000 + i * 100, vec![0xaa]));
    }
    assert!(verify_receipt_chain(&receipts).is_ok());
}

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

#[test]
fn vector_05_gap_in_middle_rejects() {
    // 0, 1, 3 (skip 2) → SequenceGap.
    let r0 = make_receipt(0, [0x01; 32], 1000, vec![0xaa]);
    let r1 = make_receipt(1, [0x01; 32], 1100, vec![0xaa]);
    let r3 = make_receipt(3, [0x01; 32], 1300, vec![0xaa]);
    assert!(matches!(
        verify_receipt_chain(&[r0, r1, r3]),
        Err(SettlementError::SequenceGap { .. })
    ));
}

#[test]
fn vector_06_gap_at_start_accepts_no_predecessor_check() {
    // Per RFC-0014 §verify_receipt_chain: the predecessor check only
    // fires when a previous receipt exists in the chain. A leading
    // receipt with receipt_id != 0 is accepted (the canonical "first
    // receipt id" rule lives at the sink layer, not the chain
    // verifier). Documenting here for substrate-behavior parity with
    // audit-chain's vector_09.
    let r1 = make_receipt(1, [0x01; 32], 1100, vec![0xaa]);
    let r2 = make_receipt(2, [0x01; 32], 1200, vec![0xaa]);
    assert!(verify_receipt_chain(&[r1, r2]).is_ok());
}

#[test]
fn vector_07_duplicate_receipt_id_rejects() {
    // 0, 0 → SequenceGap (duplicate id not equal to expected successor 1).
    let r0 = make_receipt(0, [0x01; 32], 1000, vec![0xaa]);
    let r0_dup = make_receipt(0, [0x01; 32], 1100, vec![0xbb]);
    assert!(matches!(
        verify_receipt_chain(&[r0, r0_dup]),
        Err(SettlementError::SequenceGap { .. })
    ));
}

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

#[test]
fn vector_10_receipt_id_for_idempotent() {
    // Calling receipt_id_for twice on the same Receipt yields identical
    // bytes — pure function property required for cross-replica
    // consensus.
    let r = make_receipt(0, [0x01; 32], 1000, vec![0xaa, 0xbb]);
    let h1 = receipt_id_for(&r);
    let h2 = receipt_id_for(&r);
    assert_eq!(h1, h2);
}

#[test]
fn vector_11_receipt_id_for_canonical_bytes_determinism() {
    // Two receipts with identical fields yield identical settlement_hash
    // — proves canonical-byte determinism across replicas.
    let r1 = make_receipt(7, [0x42; 32], 1700, vec![0xde, 0xad, 0xbe, 0xef]);
    let r2 = make_receipt(7, [0x42; 32], 1700, vec![0xde, 0xad, 0xbe, 0xef]);
    assert_eq!(receipt_id_for(&r1), receipt_id_for(&r2));
}

#[test]
fn vector_12_domain_separator_byte_pin() {
    // The canonical domain separator MUST be byte-pinned per RFC-0014
    // §Test Vectors. Any drift breaks cross-replica consensus — this
    // vector is the load-bearing assertion for the whole chain.
    assert_eq!(CHAIN_DOMAIN_SEPARATOR, b"cipherocto/reservation/v1/");
    // Length sanity: 26 bytes.
    assert_eq!(CHAIN_DOMAIN_SEPARATOR.len(), 26);
}
