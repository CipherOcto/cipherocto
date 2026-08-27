//! Mission 0010-c — TV for `ChainId::as_bytes()` 32-byte BLAKE3
//! derivation (RFC-0010 §32-byte addendum).
//!
//! Locks:
//! - TV-1: `as_bytes()` is deterministic across N calls on the same
//!   input (BLAKE3 determinism lock).
//! - TV-2: `as_bytes()` BLAKE3 known-vector — precomputed
//!   `BLAKE3("cipherocto/chain/v1/" + "cipherocto/testnet/v1")` =
//!   `eb200e7dde19aa1f2167f7e09ddf539181814cdfba808d55d6852c1bd411eeab`
//!   (32 bytes hex).
//! - TV-3: 17-byte `canonical_bytes()` + 32-byte `as_bytes()` coexist
//!   for same input — outputs differ in length AND content.
//!
//! Layer-A substrate concern (no randomness, no time, no I/O).

use octo_ident::chain::ChainId;

const KNOWN_VECTOR_HEX: &str = "eb200e7dde19aa1f2167f7e09ddf539181814cdfba808d55d6852c1bd411eeab";
const KNOWN_VECTOR_CHAIN: &str = "cipherocto/testnet/v1";
const DOMAIN_SEPARATOR: &[u8] = b"cipherocto/chain/v1/";

/// TV-1: `as_bytes()` determinism — same input → same 32 bytes
/// across N calls.
#[test]
fn as_bytes_is_deterministic_across_n_calls() {
    let chain = ChainId::new_unchecked(KNOWN_VECTOR_CHAIN);
    let first = chain.as_bytes();
    for _ in 0..1000 {
        assert_eq!(
            chain.as_bytes(),
            first,
            "as_bytes() must be deterministic (BLAKE3 invariant)"
        );
    }
}

/// TV-2: `as_bytes()` BLAKE3 known-vector pinning — defends against
/// domain-separator drift.
#[test]
fn as_bytes_known_vector_matches_blake3_256() {
    let chain = ChainId::new_unchecked(KNOWN_VECTOR_CHAIN);
    let bytes = chain.as_bytes();
    let hex = hex::encode(bytes);
    assert_eq!(
        hex,
        KNOWN_VECTOR_HEX,
        "as_bytes() must match BLAKE3-256(domain || chain_string) with domain = {:?}",
        std::str::from_utf8(DOMAIN_SEPARATOR).unwrap()
    );
    assert_eq!(
        bytes.len(),
        32,
        "as_bytes() must return exactly 32 bytes (BLAKE3-256 output size)"
    );
}

/// TV-3: 17-byte `ChainNamespace::canonical_bytes()` + 32-byte
/// `ChainId::as_bytes()` coexist for same input — they are distinct
/// canonical forms with distinct purposes (storage PK vs
/// WAL/audit log wire form).
#[test]
fn canonical_bytes_17_and_as_bytes_32_coexist() {
    let chain = ChainId::new_unchecked(KNOWN_VECTOR_CHAIN);
    let ns = chain
        .namespace()
        .expect("cipherocto/testnet/v1 is RFC-allocated");
    let c17 = ns.canonical_bytes();
    let a32 = chain.as_bytes();
    // 17-byte form is the legacy ChainNamespace variant-tagged form
    // (variant byte + 15-byte tag + length byte).
    assert_eq!(
        c17.len(),
        17,
        "namespace.canonical_bytes() must be 17 bytes"
    );
    assert_eq!(a32.len(), 32, "chain_id.as_bytes() must be 32 bytes");
    // The two outputs MUST differ in length AND content — distinct
    // canonical forms for distinct purposes.
    assert_ne!(
        c17.as_slice(),
        a32.as_slice(),
        "17-byte canonical_bytes() and 32-byte as_bytes() must \
         produce distinct outputs (different canonical forms)"
    );
}
