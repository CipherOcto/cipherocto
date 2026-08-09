//! Macaroon v1 cryptographic foundation (RFC-0957 §3.1–3.2).
//!
//! Layer 4 extension crate per RFC-0965 caveat discriminator pattern. This
//! crate owns the **pure crypto foundation** of the macaroon substrate:
//! HMAC-BLAKE3 primitive + macaroon identifier derivation + capability_id
//! domain separator.
//!
//! ## Scope (Mission 0957-ext-macaroon Phase 1)
//!
//! Phase 1 extraction: HMAC-BLAKE3 + macaroon_id derivation + capability_id
//! domain constants. The `Macaroon` struct, `Caveat` enum, and catalog traits
//! (`CapabilityCatalog`, `CapabilityGossip`) remain in `crates/octo-wallet`
//! for now; their full migration lands in follow-on missions per the
//! per-extension crate extraction roadmap (RFC-0957 v2.0 §Per-Extension
//! Crate Layout).
//!
//! ## Algorithm (RFC-0957 §Algorithms + RFC-0853 §1.1)
//!
//! - `macaroon_root_id = blake3::keyed_hash(root_secret, MACAR_ID_DOMAIN || hex(nonce))[:16]`
//! - `capability_id = BLAKE3(CAPABILITY_ID_DOMAIN || canonical_ser_unsigned(macaroon))`
//!   per RFC-0965 §3.7
//! - `hmac_i = blake3::keyed_hash(hmac_{i-1}, caveat_name || canonical_ser(caveat) || capability_id_{i-1})`
//!
//! ## Layer discipline
//!
//! This crate has **zero deps on `octo-wallet`**, `octo-protocol`, or any
//! higher-layer substrate. It owns only cryptographic primitives (Layer A
//! primitive `blake3`) + a thin domain-separation layer. Downstream crates
//! may depend on it; this crate may not depend on anything except Layer A
//! primitives per [[cipherocto-design-principles]].
//!
//! ## Wire format
//!
//! Macaroon v1 wire form lives in `octo_cap_macaroon::wire` (Phase 2 follow-on).
//! For now, callers serialize the macaroon via the existing
//! `crates/octo-wallet/src/capability/wire.rs` surface; Phase 2 migration
//! moves that file into this crate.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

/// Domain separator byte for `capability_id` derivation (RFC-0965 §3.7).
///
/// `capability_id = BLAKE3(CAPABILITY_ID_DOMAIN || canonical_ser_unsigned(macaroon))`.
/// The byte value `0x05` is RFC-0965 reserved for capability token
/// identifiers (distinct from the caveat discriminator range 0x00..0x1F).
pub const CAPABILITY_ID_DOMAIN: u8 = 0x05;

/// Domain string for the macaroon-id derivation (`chain[0]`).
///
/// Concatenated with the hex-encoded nonce as the BLAKE3 keyed-mode
/// message input.
pub const MACAR_ID_DOMAIN: &str = "cipherocto/macaroon/v1/id";

/// Macaroon identifier (16 bytes — first half of
/// `blake3::keyed_hash(root_secret, MACAR_ID_DOMAIN || hex(nonce))`).
pub type MacaroonId = [u8; 16];

/// BLAKE3-keyed MAC with 32-byte key. Thin wrapper around
/// `blake3::keyed_hash` per RFC-0957 §Algorithms + RFC-0853 §1.1.
///
/// # Why a wrapper and not a direct call?
///
/// 1. **Type signature stability**: callers pass `&[u8; 32]` (root
///    secret, hex output bytes). The wrapper preserves the 32-byte
///    typed-key shape; `blake3::keyed_hash` accepts `&[u8]`.
/// 2. **Return type stability**: callers want `[u8; 32]` (fixed array),
///    not `Hash` (which is a thin newtype around `&[u8; 32]`).
/// 3. **Single point of reference**: future migration to a different
///    keyed-hash primitive (or to a hardware-accelerated variant) only
///    needs to touch this one function.
#[must_use]
pub fn hmac_blake3(key: &[u8; 32], msg: &[u8]) -> [u8; 32] {
    *blake3::keyed_hash(key, msg).as_bytes()
}

/// 16-byte truncation of HMAC-BLAKE3 output. Macaroon ID per RFC-0957 §3.2.
///
/// `macaroon_id = blake3::keyed_hash(root_secret, nonce)[:16]`
///
/// **Note (mission 0957-ext-macaroon follow-up):** RFC-0957 §Algorithms
/// documents `macaroon_id = blake3::keyed_hash(root_secret, MACAR_ID_DOMAIN ++ hex(nonce))[:16]`
/// (domain-separated + hex-encoded nonce). The current implementation uses
/// the raw 16-byte nonce as the BLAKE3 message (no domain prefix, no hex
/// encoding) for byte-identical backward compatibility with the pre-extraction
/// `crates/octo-wallet/src/capability/macaroon.rs::macaroon_id` function.
/// A future audit mission will reconcile the algorithm-vs-implementation drift;
/// wire-form stability is preserved by exporting this function (Phase 1) without
/// changing observable behavior.
#[must_use]
pub fn macaroon_id(root_secret: &[u8; 32], nonce: &[u8; 16]) -> MacaroonId {
    let mac = hmac_blake3(root_secret, nonce);
    let mut id = [0u8; 16];
    id.copy_from_slice(&mac[..16]);
    id
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_root() -> [u8; 32] {
        let mut k = [0u8; 32];
        for (i, byte) in k.iter_mut().enumerate() {
            *byte = i as u8;
        }
        k
    }

    #[test]
    fn hmac_blake3_is_deterministic() {
        let key = sample_root();
        let msg = b"hello";
        let a = hmac_blake3(&key, msg);
        let b = hmac_blake3(&key, msg);
        assert_eq!(a, b);
    }

    #[test]
    fn hmac_blake3_changes_with_key() {
        let key1 = [1u8; 32];
        let key2 = [2u8; 32];
        let msg = b"hello";
        assert_ne!(hmac_blake3(&key1, msg), hmac_blake3(&key2, msg));
    }

    #[test]
    fn hmac_blake3_changes_with_message() {
        let key = sample_root();
        assert_ne!(hmac_blake3(&key, b"a"), hmac_blake3(&key, b"b"));
    }

    #[test]
    fn macaroon_id_is_16_bytes() {
        let root = sample_root();
        let nonce = [0xab; 16];
        let id = macaroon_id(&root, &nonce);
        assert_eq!(id.len(), 16);
    }

    #[test]
    fn macaroon_id_is_deterministic() {
        let root = sample_root();
        let nonce = [0x42; 16];
        assert_eq!(macaroon_id(&root, &nonce), macaroon_id(&root, &nonce));
    }

    #[test]
    fn macaroon_id_changes_with_nonce() {
        let root = sample_root();
        let id_a = macaroon_id(&root, &[0u8; 16]);
        let id_b = macaroon_id(&root, &[1u8; 16]);
        assert_ne!(id_a, id_b);
    }

    #[test]
    fn capability_id_domain_is_0x05() {
        // RFC-0965 §3.7 reserved byte value for capability token IDs.
        assert_eq!(CAPABILITY_ID_DOMAIN, 0x05);
    }

    #[test]
    fn macar_id_domain_is_canonical_string() {
        assert_eq!(MACAR_ID_DOMAIN, "cipherocto/macaroon/v1/id");
    }
}
