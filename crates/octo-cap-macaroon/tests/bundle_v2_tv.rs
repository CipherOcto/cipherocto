//! 10 byte-exact V2 bundle test vectors (mission `0957-f-v2-bundle-tv-fixture`).
//!
//! Per RFC-0008 Class A determinism: borsh layout is deterministic for
//! a fixed struct schema. The TV suite asserts precomputed canonical
//! bytes against live derivation, so any borsh schema drift on V2
//! wire form breaks a named test (not just a structural equality
//! check). Companion to the 17 inline structural tests in
//! `crates/octo-cap-macaroon/src/bundle_v2.rs::tests`.
//!
//! ## Layer discipline
//!
//! Layer 4 extension crate (`octo-cap-macaroon` per
//! [[cipherocto-design-principles]]). `holder_record_bytes` +
//! `discharge_macaroon_bytes` use raw `Vec<u8>` (bytes-indirection
//! pattern per `bundle_v2.rs` doc §DischargeMacaroon bytes) — NO
//! `HolderRecord` import, that lives in `quota-router-storage`
//! (Layer B), and L4 → B-substrate is forbidden.
//!
//! ## Borsh version pin
//!
//! `borsh = "=1.5.0"` (per `Cargo.toml:50`). The precomputed bytes
//! in this file are valid for that version only; schema drift
//! requires regenerating via the one-shot debug helper at the bottom
//! of the file.
//!
//! ## Pattern source
//!
//! Mirrors `crates/octo-ident/tests/chain_namespace_tv.rs::tv10`
//! (precomputed `[u8;17]` array as inline byte contract).

use octo_cap_macaroon::bundle_v2::{
    BundleV2Error, CapabilityBundleV2, CapabilityBundleV2Envelope, CapabilityTokenV2,
    BUNDLE_ID_DOMAIN_V2, BUNDLE_VERSION_V2, CIPHEROCTO_V2_BUNDLE_PREFIX, MAX_CHAIN_DEPTH,
};

// =========================================================================
// Fixture builders
// =========================================================================
//
// Mirrors `bundle_v2.rs::tests::v2_root_fixture` / `v2_child_fixture`
// (those are private `#[cfg(test)]`, so the field shape is duplicated
// here). Keep in sync.

/// Minimal V2 root bundle (chain_depth = 0, chain_parent = zero).
fn root_bundle() -> CapabilityBundleV2 {
    let token_v2 = CapabilityTokenV2 {
        chain_depth: 0,
        chain_parent: [0u8; 32],
        audience_did: "did:octo:zV2RootHolder".to_owned(),
        channel_id: [0xA1; 16],
        expires_at_unix_secs: 1_700_003_600,
        issuer_did: "did:octo:zV2Issuer".to_owned(),
    };
    let holder_record_bytes = br#"{"private_holder_secret":"zV2PrivateRootHandle"}"#.to_vec();
    let discharge_macaroon_bytes = br#"{"channel":"escrow","root_secret_hash":"aa"}"#.to_vec();
    CapabilityBundleV2::new(token_v2, holder_record_bytes, discharge_macaroon_bytes)
        .expect("v2 root fixture")
}

/// V2 child bundle (chain_depth = 1, chain_parent = `[0xCC; 32]`).
fn child_bundle() -> CapabilityBundleV2 {
    let mut bundle = root_bundle();
    bundle.token_v2.chain_depth = 1;
    bundle.token_v2.chain_parent = [0xCC; 32];
    bundle
}

// =========================================================================
// Test vectors
// =========================================================================

/// TV-1 `BUNDLE_VERSION_V2` is the wire version byte 2.
#[test]
fn tv1_bundle_version_v2_is_2() {
    assert_eq!(BUNDLE_VERSION_V2, 2);
}

/// TV-2 `BUNDLE_ID_DOMAIN_V2` is the canonical BLAKE3 domain string.
#[test]
fn tv2_id_domain_is_canonical_string() {
    assert_eq!(BUNDLE_ID_DOMAIN_V2, "cipherocto/bundle/v2/id");
}

/// TV-3 `CIPHEROCTO_V2_BUNDLE_PREFIX` is the canonical 16-byte ASCII
/// prefix `b"cipherocto/v2\x00\x00\x00"`.
#[test]
fn tv3_envelope_prefix_is_canonical_16_bytes() {
    let expected: [u8; 16] = [
        0x63, 0x69, 0x70, 0x68, 0x65, 0x72, 0x6f, 0x63, 0x74, 0x6f, 0x2f, 0x76, 0x32, 0x00, 0x00,
        0x00,
    ];
    assert_eq!(CIPHEROCTO_V2_BUNDLE_PREFIX, &expected);
    // First 13 bytes = ASCII "cipherocto/v2"; last 3 = padding.
    assert_eq!(&CIPHEROCTO_V2_BUNDLE_PREFIX[..13], b"cipherocto/v2");
    assert_eq!(&CIPHEROCTO_V2_BUNDLE_PREFIX[13..], &[0u8; 3]);
}

/// TV-4 `MAX_CHAIN_DEPTH` is 8 per RFC-0009 v1.2 §Hierarchical
/// Attenuation Chains.
#[test]
fn tv4_max_chain_depth_constant_is_8() {
    assert_eq!(MAX_CHAIN_DEPTH, 8);
}

/// TV-5 root bundle `canonical_ser()` matches precomputed borsh
/// bytes (byte-exact wire lock).
#[test]
fn tv5_root_bundle_canonical_ser_bytes() {
    let bundle = root_bundle();
    let bytes = bundle.canonical_ser().expect("ser");
    // Precomputed via `print_precomputed_bytes` (one-shot helper at end of file).
    let expected: [u8; 206] = [
        0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x16, 0x00, 0x00, 0x00, 0x64, 0x69, 0x64, 0x3a, 0x6f, 0x63, 0x74,
        0x6f, 0x3a, 0x7a, 0x56, 0x32, 0x52, 0x6f, 0x6f, 0x74, 0x48, 0x6f, 0x6c, 0x64, 0x65, 0x72,
        0xa1, 0xa1, 0xa1, 0xa1, 0xa1, 0xa1, 0xa1, 0xa1, 0xa1, 0xa1, 0xa1, 0xa1, 0xa1, 0xa1, 0xa1,
        0xa1, 0x10, 0xff, 0x53, 0x65, 0x00, 0x00, 0x00, 0x00, 0x12, 0x00, 0x00, 0x00, 0x64, 0x69,
        0x64, 0x3a, 0x6f, 0x63, 0x74, 0x6f, 0x3a, 0x7a, 0x56, 0x32, 0x49, 0x73, 0x73, 0x75, 0x65,
        0x72, 0x30, 0x00, 0x00, 0x00, 0x7b, 0x22, 0x70, 0x72, 0x69, 0x76, 0x61, 0x74, 0x65, 0x5f,
        0x68, 0x6f, 0x6c, 0x64, 0x65, 0x72, 0x5f, 0x73, 0x65, 0x63, 0x72, 0x65, 0x74, 0x22, 0x3a,
        0x22, 0x7a, 0x56, 0x32, 0x50, 0x72, 0x69, 0x76, 0x61, 0x74, 0x65, 0x52, 0x6f, 0x6f, 0x74,
        0x48, 0x61, 0x6e, 0x64, 0x6c, 0x65, 0x22, 0x7d, 0x2c, 0x00, 0x00, 0x00, 0x7b, 0x22, 0x63,
        0x68, 0x61, 0x6e, 0x6e, 0x65, 0x6c, 0x22, 0x3a, 0x22, 0x65, 0x73, 0x63, 0x72, 0x6f, 0x77,
        0x22, 0x2c, 0x22, 0x72, 0x6f, 0x6f, 0x74, 0x5f, 0x73, 0x65, 0x63, 0x72, 0x65, 0x74, 0x5f,
        0x68, 0x61, 0x73, 0x68, 0x22, 0x3a, 0x22, 0x61, 0x61, 0x22, 0x7d,
    ];
    assert_eq!(
        bytes, expected,
        "root bundle canonical_ser must match precomputed"
    );
}

/// TV-6 child bundle `canonical_ser()` matches precomputed bytes
/// (byte-exact wire lock for chain_depth=1 + non-zero chain_parent).
#[test]
fn tv6_child_bundle_canonical_ser_bytes() {
    let bundle = child_bundle();
    let bytes = bundle.canonical_ser().expect("ser");
    let expected: [u8; 206] = [
        0x02, 0x01, 0xcc, 0xcc, 0xcc, 0xcc, 0xcc, 0xcc, 0xcc, 0xcc, 0xcc, 0xcc, 0xcc, 0xcc, 0xcc,
        0xcc, 0xcc, 0xcc, 0xcc, 0xcc, 0xcc, 0xcc, 0xcc, 0xcc, 0xcc, 0xcc, 0xcc, 0xcc, 0xcc, 0xcc,
        0xcc, 0xcc, 0xcc, 0xcc, 0x16, 0x00, 0x00, 0x00, 0x64, 0x69, 0x64, 0x3a, 0x6f, 0x63, 0x74,
        0x6f, 0x3a, 0x7a, 0x56, 0x32, 0x52, 0x6f, 0x6f, 0x74, 0x48, 0x6f, 0x6c, 0x64, 0x65, 0x72,
        0xa1, 0xa1, 0xa1, 0xa1, 0xa1, 0xa1, 0xa1, 0xa1, 0xa1, 0xa1, 0xa1, 0xa1, 0xa1, 0xa1, 0xa1,
        0xa1, 0x10, 0xff, 0x53, 0x65, 0x00, 0x00, 0x00, 0x00, 0x12, 0x00, 0x00, 0x00, 0x64, 0x69,
        0x64, 0x3a, 0x6f, 0x63, 0x74, 0x6f, 0x3a, 0x7a, 0x56, 0x32, 0x49, 0x73, 0x73, 0x75, 0x65,
        0x72, 0x30, 0x00, 0x00, 0x00, 0x7b, 0x22, 0x70, 0x72, 0x69, 0x76, 0x61, 0x74, 0x65, 0x5f,
        0x68, 0x6f, 0x6c, 0x64, 0x65, 0x72, 0x5f, 0x73, 0x65, 0x63, 0x72, 0x65, 0x74, 0x22, 0x3a,
        0x22, 0x7a, 0x56, 0x32, 0x50, 0x72, 0x69, 0x76, 0x61, 0x74, 0x65, 0x52, 0x6f, 0x6f, 0x74,
        0x48, 0x61, 0x6e, 0x64, 0x6c, 0x65, 0x22, 0x7d, 0x2c, 0x00, 0x00, 0x00, 0x7b, 0x22, 0x63,
        0x68, 0x61, 0x6e, 0x6e, 0x65, 0x6c, 0x22, 0x3a, 0x22, 0x65, 0x73, 0x63, 0x72, 0x6f, 0x77,
        0x22, 0x2c, 0x22, 0x72, 0x6f, 0x6f, 0x74, 0x5f, 0x73, 0x65, 0x63, 0x72, 0x65, 0x74, 0x5f,
        0x68, 0x61, 0x73, 0x68, 0x22, 0x3a, 0x22, 0x61, 0x61, 0x22, 0x7d,
    ];
    assert_eq!(
        bytes, expected,
        "child bundle canonical_ser must match precomputed"
    );
}

/// TV-7 root `bundle_id()` = BLAKE3(`BUNDLE_ID_DOMAIN_V2` ‖ root_ser)
/// matches precomputed 32-byte digest.
#[test]
fn tv7_root_bundle_id_is_precomputed_blake3() {
    let bundle = root_bundle();
    let id = bundle.bundle_id();
    let expected: [u8; 32] = [
        0xb0, 0x96, 0xbf, 0x69, 0xdb, 0xe2, 0xa5, 0xb3, 0xa3, 0x24, 0xd0, 0x55, 0x7c, 0xee, 0x92,
        0xb6, 0xbe, 0xda, 0xe4, 0x24, 0xdc, 0xed, 0x37, 0xdb, 0x54, 0x09, 0xc9, 0xf4, 0x4c, 0xf2,
        0xf5, 0x21,
    ];
    assert_eq!(id, expected, "root bundle_id must match precomputed");
}

/// TV-8 child `bundle_id()` differs from TV-7 + matches precomputed
/// 32-byte digest (chain_parent sensitivity lock).
#[test]
fn tv8_child_bundle_id_differs_from_root_and_matches_precomputed() {
    let root = root_bundle();
    let child = child_bundle();
    assert_ne!(
        root.bundle_id(),
        child.bundle_id(),
        "chain_parent change MUST change bundle_id"
    );
    let expected: [u8; 32] = [
        0xe7, 0x08, 0x8e, 0x0f, 0x18, 0x78, 0x64, 0xbb, 0x65, 0xdf, 0xda, 0xb6, 0x12, 0x72, 0xfb,
        0x2b, 0x8d, 0x9e, 0xa3, 0x48, 0x17, 0x7d, 0xd1, 0xfb, 0xb0, 0xf7, 0xff, 0xe0, 0x78, 0x5c,
        0xad, 0x1e,
    ];
    assert_eq!(
        child.bundle_id(),
        expected,
        "child bundle_id must match precomputed"
    );
}

/// TV-9 envelope `canonical_ser()` = `CIPHEROCTO_V2_BUNDLE_PREFIX` ‖
/// `bundle.canonical_ser()` (byte-exact envelope wire lock).
#[test]
fn tv9_envelope_canonical_ser_is_prefix_concat_bundle() {
    let bundle = root_bundle();
    let env = CapabilityBundleV2Envelope::new(bundle.clone());
    let bytes = env.canonical_ser().expect("envelope ser");
    // First 16 bytes MUST equal the prefix.
    assert_eq!(
        &bytes[..16],
        CIPHEROCTO_V2_BUNDLE_PREFIX.as_slice(),
        "envelope canonical_ser must emit prefix as first 16 bytes"
    );
    // Roundtrip preserves inner bundle (struct equality after decode).
    let decoded = CapabilityBundleV2Envelope::canonical_de(&bytes).expect("envelope de");
    assert_eq!(
        decoded.bundle, bundle,
        "envelope roundtrip must preserve bundle"
    );
    assert_eq!(
        decoded.prefix, *CIPHEROCTO_V2_BUNDLE_PREFIX,
        "envelope roundtrip must preserve prefix"
    );
}

/// TV-10 chain_depth boundary: `chain_depth == MAX_CHAIN_DEPTH`
/// accepted (boundary), `chain_depth == MAX_CHAIN_DEPTH + 1`
/// rejected via `canonical_de` (post-borsh validation).
#[test]
fn tv10_chain_depth_boundary_accept_reject() {
    // Boundary accept: depth == MAX_CHAIN_DEPTH.
    let mut bundle = root_bundle();
    bundle.token_v2.chain_depth = MAX_CHAIN_DEPTH;
    let bytes = bundle.canonical_ser().expect("ser at boundary");
    let decoded = CapabilityBundleV2::canonical_de(&bytes).expect("de at boundary");
    assert_eq!(
        decoded.token_v2.chain_depth, MAX_CHAIN_DEPTH,
        "depth == MAX must be accepted"
    );

    // Boundary reject: depth == MAX_CHAIN_DEPTH + 1 (via borsh bypass).
    let mut too_deep = root_bundle();
    too_deep.token_v2.chain_depth = MAX_CHAIN_DEPTH + 1;
    let bytes = too_deep.canonical_ser().expect("ser too deep");
    let err = CapabilityBundleV2::canonical_de(&bytes)
        .expect_err("depth > MAX must be rejected by canonical_de");
    assert!(
        matches!(err, BundleV2Error::ChainDepthTooLarge(d) if d == MAX_CHAIN_DEPTH + 1),
        "wrong error variant for over-depth, got {err:?}"
    );
}

// =========================================================================
// One-shot helper — regenerate the expected byte arrays above.
// =========================================================================
//
// Run with:
//   cargo test -p octo-cap-macaroon --test bundle_v2_tv -- --ignored --nocapture
//
// Then paste the printed bytes into TV-5 / TV-6 / TV-7 / TV-8 above.

#[test]
#[ignore = "regenerate precomputed TV bytes after borsh schema drift"]
fn print_precomputed_bytes() {
    let root = root_bundle();
    let root_bytes = root.canonical_ser().expect("ser");
    println!(
        "\nTV-5 root bundle canonical_ser ({} bytes):",
        root_bytes.len()
    );
    print_byte_array(&root_bytes);
    println!("\nlet expected: [u8; {}] = [", root_bytes.len());
    print_pascal_array(&root_bytes);
    println!("];\n");

    let child = child_bundle();
    let child_bytes = child.canonical_ser().expect("ser");
    println!(
        "\nTV-6 child bundle canonical_ser ({} bytes):",
        child_bytes.len()
    );
    print_byte_array(&child_bytes);
    println!("\nlet expected: [u8; {}] = [", child_bytes.len());
    print_pascal_array(&child_bytes);
    println!("];\n");

    let root_id = root.bundle_id();
    println!("\nTV-7 root bundle_id (32 bytes):");
    print_byte_array(&root_id);
    println!("\nlet expected: [u8; 32] = [");
    print_pascal_array(&root_id);
    println!("];\n");

    let child_id = child.bundle_id();
    println!("\nTV-8 child bundle_id (32 bytes):");
    print_byte_array(&child_id);
    println!("\nlet expected: [u8; 32] = [");
    print_pascal_array(&child_id);
    println!("];\n");
}

fn print_byte_array(bytes: &[u8]) {
    for (i, b) in bytes.iter().enumerate() {
        if i % 16 == 0 {
            print!("        ");
        }
        print!("0x{:02x}, ", b);
        if i % 16 == 15 || i == bytes.len() - 1 {
            println!();
        }
    }
}

fn print_pascal_array(bytes: &[u8]) {
    for (i, b) in bytes.iter().enumerate() {
        if i % 15 == 0 {
            print!("        ");
        }
        print!("0x{:02x}, ", b);
        if i % 15 == 14 || i == bytes.len() - 1 {
            println!();
        }
    }
}
