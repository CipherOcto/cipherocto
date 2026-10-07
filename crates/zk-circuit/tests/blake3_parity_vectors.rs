//! Rust-side anchor for the Cairo BLAKE3 parity vectors.
//!
//! `cairo/src/blake3.cairo`'s test module hard-codes the expected digests for
//! a set of `blake3::keyed_hash` vectors. Those constants are only meaningful
//! while the `blake3` crate still produces them, so this test re-derives every
//! one and fails loudly if upstream ever changes.
//!
//! This is the half of the cross-implementation check that lives in Rust. The
//! other half is `scarb cairo-test`, which runs the pure-Cairo implementation
//! against the same constants. Neither half is useful alone: the Cairo tests
//! alone would happily pass against a wrong set of constants, and these alone
//! would not catch a divergence between the two implementations.
//!
//! Regenerate with `cargo run -p zk-circuit --example blake3_vectors`.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::doc_markdown,
    clippy::explicit_into_iter_loop,
    clippy::explicit_iter_loop,
    clippy::format_collect,
    clippy::manual_rotate,
    clippy::many_single_char_names,
    clippy::needless_range_loop,
    clippy::uninlined_format_args,
    clippy::unreadable_literal,
    clippy::useless_vec
)]

fn test_key() -> [u8; 32] {
    let mut k = [0u8; 32];
    let mut s: u64 = 0x1234_5678_9abc_def0 | 1;
    for b in k.iter_mut() {
        s ^= s >> 12;
        s ^= s << 25;
        s ^= s >> 27;
        *b = (s.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 32) as u8;
    }
    k
}

fn k2() -> [u8; 32] {
    let mut k = [0u8; 32];
    let mut s: u64 = 0x0bad_c0de_dead_beef | 1;
    for b in k.iter_mut() {
        s ^= s >> 12;
        s ^= s << 25;
        s ^= s >> 27;
        *b = (s.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 32) as u8;
    }
    k
}

fn repeat(byte: u8, n: usize) -> Vec<u8> {
    vec![byte; n]
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[test]
fn keyed_hash_vectors_still_match_upstream_blake3() {
    let k = test_key();

    let cases: Vec<(&str, [u8; 32], Vec<u8>, &str)> = vec![
        (
            "empty",
            k,
            Vec::new(),
            "8ac4e03c4d21df357633ea3f238b09419c3b5a6f19c92a3f2d5c977e1962178b",
        ),
        (
            "one_byte",
            k,
            b"a".to_vec(),
            "0bd4779ad64153f0304335cea7f267e792341f33bc93567c917a72bf701a632b",
        ),
        (
            "partial_final_chunk",
            k,
            repeat(0xab, 55),
            "f366ef8e51b499eece37075eaeb012a5134e1fcfa8d5c07b1cbb2106fb3cbbe7",
        ),
        (
            "exactly_one_chunk",
            k,
            repeat(0x5a, 64),
            "c3723093f79e39621a02267a94b43b9394b1b44d72019a84379542fcbe306fb4",
        ),
        (
            "two_chunks",
            k,
            repeat(0xc3, 65),
            "7f6155d7e7da2cee97ee13598ab833e398845dffebce0ec3f11babeea9c46c14",
        ),
        (
            "two_full_chunks",
            k,
            repeat(0x11, 128),
            "01c6747c407c7ef533b9b46995a127a932ee9b09b7cea615b2f89f8b384ab298",
        ),
        (
            "three_chunks",
            k,
            repeat(0xfe, 129),
            "0eb809d02c45585818ba85a4d7c5780ea0c8ff44214cc5f78fa7358a13799d77",
        ),
    ];

    for (name, key, msg, want) in cases {
        let got = blake3::keyed_hash(&key, &msg);
        assert_eq!(
            hex(got.as_bytes()),
            want,
            "vector {name} (msg_len={}) no longer matches upstream blake3; \
             update cairo/src/blake3.cairo",
            msg.len()
        );
    }
}

#[test]
fn macaroon_shaped_vector_still_matches_upstream_blake3() {
    let mut msg = Vec::new();
    msg.extend_from_slice(b"amount_max");
    msg.extend_from_slice(&[0u8, 0, 0, 32]);
    msg.extend_from_slice(&[0x42u8; 32]);
    msg.extend_from_slice(&[0x77u8; 32]);
    assert_eq!(msg.len(), 78);

    let got = blake3::keyed_hash(&k2(), &msg);
    assert_eq!(
        hex(got.as_bytes()),
        "ae3182f5fec221338d9a6d69ffac53778f8c50afa327b84d59261868b68d42aa",
        "macaroon_shaped vector no longer matches upstream blake3; \
         update cairo/src/blake3.cairo"
    );
}

#[test]
fn canonical_unkeyed_empty_vector_is_the_published_constant() {
    assert_eq!(
        hex(blake3::hash(b"").as_bytes()),
        "af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262"
    );
}
