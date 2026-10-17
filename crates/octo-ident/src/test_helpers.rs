//! Test helpers for canonical DID minting.
//!
//! Mission 0010-b: every test fixture that needs a canonical DID MUST call
//! `sample_did(seed)` from this module instead of inventing a `did:octo:*`
//! literal. This is enforced by the grep audit in Mission 0010-b
//! (acceptance criterion: zero bare-name `did:octo:` literals).
//!
//! ## Usage
//!
//! ```ignore
//! use octo_ident::test_helpers::sample_did;
//!
//! let did = sample_did(1);
//! assert!(did.starts_with("did:octo:z"));
//! ```

use crate::{CanonicalCodec, DidCodec, WireDid};

/// Generate a canonical W3C DID wire form for tests.
///
/// `seed` controls the input pubkey; two calls with the same seed return
/// byte-equal DIDs (deterministic per RFC-0010 §Determinism Requirements).
#[must_use]
pub fn sample_did(seed: u8) -> String {
    let mut pubkey = [0u8; 32];
    for (i, byte) in pubkey.iter_mut().enumerate() {
        *byte = seed.wrapping_add(i as u8);
    }
    let raw = CanonicalCodec::mint(&pubkey);
    let wire: WireDid = CanonicalCodec::raw_to_wire(&raw).expect("mint to wire");
    wire.as_str().to_owned()
}

/// Generate a canonical W3C DID for tests where the caller wants a typed
/// `WireDid` value rather than a `String`.
#[must_use]
pub fn sample_wire(seed: u8) -> WireDid {
    let s = sample_did(seed);
    WireDid(s)
}
