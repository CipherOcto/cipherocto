//! Domain-separator byte-pin tests (RFC-0014 §Test Vectors
//! `domain-separator-byte-pin` + mission 0014-settlement-verify-chain-tests).
//!
//! 2 vectors proving the canonical `CHAIN_DOMAIN_SEPARATOR` is
//! byte-pinned AND is consumed by `receipt_id_for` as the leading
//! input to the BLAKE3 keyed hash.
//!
//! Run with:
//!   cargo test -p octo-settlement-core --test domain_separator
#![allow(clippy::doc_markdown)]

use octo_settlement_core::{receipt_id_for, Receipt, CHAIN_DOMAIN_SEPARATOR};

// RFC-0014 §Test Vectors `domain-separator-byte-pin`: the canonical
// domain separator MUST be byte-identical. Any drift breaks
// cross-replica consensus — this vector is the load-bearing assertion
// for the whole chain.
#[test]
fn vector_01_chain_domain_separator_byte_pin() {
    assert_eq!(CHAIN_DOMAIN_SEPARATOR, b"cipherocto/reservation/v1/");
    // Length sanity: 26 bytes.
    assert_eq!(CHAIN_DOMAIN_SEPARATOR.len(), 26);
}

// RFC-0014 §Test Vectors `receipt-id-for-consumes-domain-separator`:
// `receipt_id_for` produces a hash that is identical to one we compute
// externally with `blake3::Hasher::new_keyed(&[0;32])` over
// `CHAIN_DOMAIN_SEPARATOR || canonical_receipt_bytes`. This proves the
// domain separator is part of the hash input (not silently dropped or
// substituted).
#[test]
fn vector_02_receipt_id_for_uses_domain_separator() {
    let r = Receipt {
        receipt_id: 5,
        ask_id: [0x07; 32],
        settlement_hash: [0; 32],
        router_id: "did:oct:router".to_owned(),
        router_sig: vec![0xde, 0xad],
        timestamp_unix: 1700,
    };
    // Manually compute the expected hash with the documented key
    // ([0;32]) and explicit domain-separator prefix.
    let mut expected = blake3::Hasher::new_keyed(&[0; 32]);
    expected.update(CHAIN_DOMAIN_SEPARATOR);
    expected.update(&r.receipt_id.to_be_bytes());
    expected.update(&r.ask_id);
    expected.update(r.router_id.as_bytes());
    expected.update(&r.router_sig);
    expected.update(&r.timestamp_unix.to_be_bytes());
    let expected_bytes: [u8; 32] = *expected.finalize().as_bytes();

    assert_eq!(receipt_id_for(&r), expected_bytes);
}
