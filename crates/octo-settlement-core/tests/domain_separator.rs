//! Domain-separator byte-pin tests (RFC-0014 §Test Vectors).
//!
//! Split per substrate-faithful test policy:
//!
//! - **RFC canonical** — `domain-separator-byte-pin` is not a
//!   canonical RFC-0014 §Test Vectors ID; the canonical substrate
//!   byte-pin lives at `reservation-mint-domain-separator` (canonical
//!   receipt-side byte-pin is implicit in `receipt-id-for-ask`). We
//!   keep this file as the single substrate surface for the byte-pin
//!   invariant with `mission-defined:` prefix on the alias comment.
//!
//! - **Mission-defined supplementary** — `receipt-id-for-consumes-
//!   domain-separator` proves the separator is part of the hash input
//!   by external recomputation.
//!
//! Run with:
//!   cargo test -p octo-settlement-core --test domain_separator
#![allow(clippy::doc_markdown)]

use octo_settlement_core::{receipt_id_for, Receipt, CHAIN_DOMAIN_SEPARATOR};

// === Mission-defined supplementary vectors ===
// (RFC-0014 §Test Vectors has no explicit settlement-side byte-pin
// vector; the canonical substrate byte-pin ID is
// `reservation-mint-domain-separator` and is implicit in
// `receipt-id-for-ask`. This file is the single substrate surface
// for the byte-pin invariant.)

// mission-defined: settlement-side domain-separator byte-pin (the
// canonical byte-pinned value is the substrate `CHAIN_DOMAIN_SEPARATOR`
// constant; this is the single test surface per substrate-faithful
// no-duplication policy).
#[test]
fn mission_01_domain_separator_byte_pin() {
    assert_eq!(CHAIN_DOMAIN_SEPARATOR, b"cipherocto/reservation/v1/");
    // Length sanity: 26 bytes.
    assert_eq!(CHAIN_DOMAIN_SEPARATOR.len(), 26);
}

// mission-defined: `receipt_id_for` produces a hash that is
// identical to one we compute externally with
// `blake3::Hasher::new_keyed(&[0;32])` over
// `CHAIN_DOMAIN_SEPARATOR || canonical_receipt_bytes`. This proves
// the domain separator is part of the hash input (not silently
// dropped or substituted).
#[test]
fn mission_02_receipt_id_for_consumes_domain_separator() {
    let r = Receipt {
        receipt_id: 5,
        ask_id: [0x07; 32],
        settlement_hash: [0; 32],
        router_id: "did:oct:router".to_owned(),
        router_sig: vec![0xde, 0xad],
        timestamp_unix: 1700,
        ..Default::default()
    };
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
