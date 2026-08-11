//! 10 canonical chain-namespace test vectors (RFC-0010 v1.4
//! §ChainId Namespace Extension §Test Vectors).
//!
//! Per RFC-0010 v1.4 §Compatibility: BLAKE3 derivation is RFC-0008
//! Class A deterministic. Validation rules are pure functions. The
//! TV suite covers canonical encodings + error paths + cross-namespace
//! distinctness.
//!
//! Cross-impl conformance measured against reference impl in
//! `crates/octo-ident/src/chain.rs`. See `CIPHEROCTO_MAINNET_TAG` for
//! the precomputed mainnet tag.

use octo_ident::{
    ChainId, ChainNamespace, ChainNamespaceError, NamespaceVariant, CIPHEROCTO_MAINNET,
    CIPHEROCTO_MAINNET_TAG, MAX_NAMESPACE_LEN,
};

/// TV-1 cipherocto_mainnet_resolves_to_rfc_variant — canonical
/// mainnet literal resolves to the `Rfc` variant + the precomputed tag.
#[test]
fn tv1_mainnet_resolves_to_rfc_variant() {
    let c = ChainId::new(CIPHEROCTO_MAINNET).expect("mainnet literal is valid");
    let ns = c.namespace().expect("namespace resolves");
    assert_eq!(ns.variant(), NamespaceVariant::Rfc);
    assert_eq!(ns.tag(), &CIPHEROCTO_MAINNET_TAG);
    assert_eq!(ns.length(), CIPHEROCTO_MAINNET.len() as u8);
}

/// TV-2 partner_mainnet_resolves_to_user_variant — user-extension
/// literal resolves to the `User` variant + a distinct tag.
#[test]
fn tv2_partner_resolves_to_user_variant() {
    let c = ChainId::new("partner-mainnet").expect("partner literal is valid");
    let ns = c.namespace().expect("namespace resolves");
    assert_eq!(ns.variant(), NamespaceVariant::User);
    assert_ne!(
        ns.tag(),
        &CIPHEROCTO_MAINNET_TAG,
        "user-extension must not collide with the mainnet tag"
    );
    assert_eq!(ns.length(), "partner-mainnet".len() as u8);
}

/// TV-3 empty_literal_rejected — empty namespace literal rejected at
/// construction time.
#[test]
fn tv3_empty_literal_rejected() {
    assert_eq!(ChainId::new("").unwrap_err(), ChainNamespaceError::Empty);
}

/// TV-4 too_long_literal_rejected — literal > MAX_NAMESPACE_LEN (64
/// chars) rejected with explicit len / max.
#[test]
fn tv4_too_long_literal_rejected() {
    let s = "a".repeat(MAX_NAMESPACE_LEN + 1);
    assert_eq!(
        ChainId::new(s).unwrap_err(),
        ChainNamespaceError::TooLong {
            len: MAX_NAMESPACE_LEN + 1,
            max: MAX_NAMESPACE_LEN,
        }
    );
}

/// TV-5 control_char_literal_rejected — literal containing a control
/// character (NUL) rejected.
#[test]
fn tv5_control_char_literal_rejected() {
    let err = ChainId::new("cipherocto\u{0000}mainnet").unwrap_err();
    assert!(matches!(err, ChainNamespaceError::ControlChar('\0')));
}

/// TV-6 canonical_bytes_round_trip — `from_canonical_bytes(canonical_bytes(ns)) == ns`.
#[test]
fn tv6_canonical_bytes_round_trip() {
    let c = ChainId::new(CIPHEROCTO_MAINNET).unwrap();
    let ns = c.namespace().unwrap();
    let bytes = ns.canonical_bytes();
    let back = ChainNamespace::from_canonical_bytes(&bytes).unwrap();
    assert_eq!(back, ns);
}

/// TV-7 distinct_literals_produce_distinct_tags — two distinct
/// valid literals produce two distinct BLAKE3 tags.
#[test]
fn tv7_distinct_literals_produce_distinct_tags() {
    let c1 = ChainId::new("cipherocto-mainnet").unwrap();
    let c2 = ChainId::new("partner-mainnet").unwrap();
    let t1 = *c1.namespace().unwrap().tag();
    let t2 = *c2.namespace().unwrap().tag();
    assert_ne!(
        t1, t2,
        "distinct literals MUST produce distinct BLAKE3 tags"
    );
}

/// TV-8 rfc_tag_length_disambiguates — same tag + different length
/// encodes distinct canonical bytes (the tag-only check would
/// otherwise be ambiguous; the length byte disambiguates per
/// RFC-0010 v1.4 §Data Structures).
#[test]
fn tv8_rfc_tag_length_disambiguates() {
    let mainnet_ns = ChainId::new(CIPHEROCTO_MAINNET)
        .unwrap()
        .namespace()
        .unwrap();
    let mut bytes = mainnet_ns.canonical_bytes();
    // Bump the length byte (variant stays 0x01 = Rfc) — produces a
    // distinct canonical encoding that decodes back to a Rfc-variant
    // namespace with a different length field.
    bytes[16] = bytes[16].wrapping_add(1);
    let back = ChainNamespace::from_canonical_bytes(&bytes).unwrap();
    assert_ne!(
        mainnet_ns.canonical_bytes(),
        back.canonical_bytes(),
        "canonical bytes MUST differ across length-byte changes"
    );
    assert_eq!(back.variant(), NamespaceVariant::Rfc);
    assert_ne!(back.length(), mainnet_ns.length());
}

/// TV-9 reserved_variant_byte_rejected — `from_canonical_bytes`
/// rejects `0x00` (Reserved) and `0x03-0xFF` (also Reserved) at the
/// variant byte.
#[test]
fn tv9_reserved_variant_byte_rejected() {
    for variant_byte in [0x00_u8, 0x03, 0x7F, 0xFF] {
        let mut bytes = [0u8; 17];
        bytes[0] = variant_byte;
        assert_eq!(
            ChainNamespace::from_canonical_bytes(&bytes).unwrap_err(),
            ChainNamespaceError::ReservedVariant(variant_byte)
        );
    }
}

/// TV-10 mainnet_canonical_bytes_match_precomputed — precomputed
/// canonical bytes match the live derivation, so the constant is
/// authoritative for cross-impl conformance checks.
#[test]
fn tv10_mainnet_canonical_bytes_match_precomputed() {
    let ns = ChainId::new(CIPHEROCTO_MAINNET)
        .unwrap()
        .namespace()
        .unwrap();
    let bytes = ns.canonical_bytes();
    let expected: [u8; 17] = {
        let mut e = [0u8; 17];
        e[0] = NamespaceVariant::Rfc as u8; // 0x01
        e[1..16].copy_from_slice(&CIPHEROCTO_MAINNET_TAG);
        e[16] = CIPHEROCTO_MAINNET.len() as u8; // 17
        e
    };
    assert_eq!(
        bytes, expected,
        "CIPHEROCTO_MAINNET canonical bytes must match the precomputed layout"
    );
}
