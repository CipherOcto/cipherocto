//! Mission 0010-f2-registry-namespacing — multi-chain registry TV.
//!
//! Verifies that `register_in_chain` + `resolve_in_chain` (new
//! additive `DidRegistry` methods from RFC-0010 v1.4) isolate DIDs
//! across chain namespaces. The same 32-byte `canonical_hash`
//! registered on two different chains must resolve independently
//! under their respective chain.
//!
//! **NOTE:** Tests serialize via Mutex because stoolap `memory://`
//! shares global catalog state across threads.
//!
//! Moved from `crates/quota-router-storage/tests/stoolap_chain_namespace.rs`
//! in mission 0206-003 v3.0 (the DID registry adapter now lives in
//! `octo-ident-storage`).

use std::sync::{Mutex, PoisonError};

use octo_ident::{ChainId, DidRegistry};
use octo_ident_storage::{StoolapDidRegistry, MAINNET_CHAIN_ID_BYTES};

static MIGRATION_LOCK: Mutex<()> = Mutex::new(());

fn sample_hash(seed: u8) -> [u8; 32] {
    let mut h = [0u8; 32];
    for (i, b) in h.iter_mut().enumerate() {
        *b = seed.wrapping_add(u8::try_from(i).expect("loop index fits in u8"));
    }
    h
}

fn sample_pk(seed: u8) -> [u8; 32] {
    let mut k = [0u8; 32];
    for (i, b) in k.iter_mut().enumerate() {
        *b = seed.wrapping_add(
            u8::try_from(i)
                .expect("loop index fits in u8")
                .wrapping_mul(3),
        );
    }
    k
}

fn sample_doc(seed: u8) -> octo_ident::DidDocument {
    octo_ident::DidDocument {
        public_key: sample_pk(seed),
        revoked: false,
        ..Default::default()
    }
}

/// Register the same `canonical_hash` on two distinct chains; verify
/// both resolve independently under their respective chain.
#[test]
fn register_in_chain_isolates_dids_across_chains() {
    let _guard = MIGRATION_LOCK
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    let reg = StoolapDidRegistry::open_in_memory().expect("open");

    let mainnet = ChainId::default();
    let partner = ChainId::new("partner-mainnet").expect("valid partner chain");

    let hash = sample_hash(0x42);
    let doc_mainnet = sample_doc(0x10);
    let doc_partner = sample_doc(0x20);

    // Same hash, two chains, different docs.
    reg.register_in_chain(&mainnet, &hash, doc_mainnet.clone())
        .expect("register on mainnet");
    reg.register_in_chain(&partner, &hash, doc_partner.clone())
        .expect("register on partner");

    // Resolve on mainnet → mainnet doc.
    let resolved_mainnet = reg
        .resolve_in_chain(&mainnet, &hash)
        .expect("resolve mainnet")
        .expect("present on mainnet");
    assert_eq!(resolved_mainnet.public_key, doc_mainnet.public_key);

    // Resolve on partner → partner doc (different from mainnet).
    let resolved_partner = reg
        .resolve_in_chain(&partner, &hash)
        .expect("resolve partner")
        .expect("present on partner");
    assert_eq!(resolved_partner.public_key, doc_partner.public_key);

    // Cross-chain isolation: partner resolve on mainnet chain does
    // NOT see the partner row (the partner chain row is in a
    // different namespace).
    // (Implicitly verified above — mainnet resolve returned
    // mainnet doc, not partner doc.)

    // Single-chain resolve (`resolve`) on the same hash returns the
    // mainnet doc (default namespace).
    let resolved_default = reg
        .resolve(&hash)
        .expect("resolve default")
        .expect("present");
    assert_eq!(resolved_default.public_key, doc_mainnet.public_key);
}

/// Verify `MAINNET_CHAIN_ID_BYTES` matches the runtime derivation.
#[test]
fn mainnet_bytes_match_chain_id_default() {
    let mainnet = ChainId::default();
    let ns = mainnet.namespace().expect("namespace");
    let canonical = ns.canonical_bytes();
    assert_eq!(
        canonical, MAINNET_CHAIN_ID_BYTES,
        "MAINNET_CHAIN_ID_BYTES must match ChainId::default().namespace().canonical_bytes()"
    );
}
