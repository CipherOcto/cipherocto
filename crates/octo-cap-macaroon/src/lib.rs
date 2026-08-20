//! Macaroon v1 cryptographic foundation + substrate (RFC-0957 §3.1–3.2).
//!
//! Layer 4 extension crate per RFC-0965 caveat discriminator pattern. This
//! crate owns the **full macaroon substrate**:
//!
//! - **Crypto foundation**: HMAC-BLAKE3 + macaroon_id derivation +
//!   capability_id domain separator.
//! - **Caveat DSL**: 24 caveat variants + `CaveatName` + `CaveatSet`
//!   + `set_subsumes` + `RawCaveat` escape hatch (RFC-0957 §3.1 +
//!   RFC-0965 §3 caveats).
//! - **Macaroon struct**: `Macaroon` + `mint` + `attenuate` + `verify_signature`
//!   + `verify_full` + `compute_capability_id` (RFC-0957 §3.2).
//! - **Catalog traits**: `CapabilityCatalog` + `CapabilityGossip` +
//!   `InMemoryCatalog` (RFC-0957-A1 §Phase 3). Production
//!   `TransportDeliveryCatalog` lives in the `octo-cap-macaroon-transport`
//!   glue crate (Phase 2c-1; keeps this crate free of Layer D dep).
//!
//! ## Scope (Mission 0957-ext-macaroon Phase 2)
//!
//! Phase 2 extraction: crypto foundation (Phase 1) + caveat DSL +
//! macaroon struct + catalog traits. The `CapabilityToken` struct
//! (the holder-bound envelope around a macaroon + Ed25519 sig +
//! discharges) remains in `crates/octo-wallet/src/capability/mod.rs`
//! for now; full migration lands in a Phase 2b follow-on. The wire
//! format (`crates/octo-wallet/src/capability/wire.rs`) and discharge
//! providers (`crates/octo-wallet/src/capability/discharge.rs`) are
//! also follow-on migrations.
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
//! This crate depends on Layer A primitives (`blake3`, `hex`, `rand`,
//! `serde`, `ed25519-dalek`, `base64`) + the `cipherocto-encoding`
//! constraint substrate (Layer B-adjacent). It does NOT depend on
//! `quota-router-storage`, `octo-wallet`, `octo-protocol`, or any
//! higher-layer substrate. Downstream crates may depend on it.
//!
//! **Phase 2c-2 (2026-08-09):** dropped the `CapabilityCatalog::holder_registry`
//! accessor (zero call sites). Removed the `quota-router-storage` dep.
//! Holder registry lookups are wired directly by downstream crates.
//!
//! ## Attenuation invariant (RFC-0957 §3.5)
//!
//! Attenuators MAY add caveats but MUST NOT remove caveats. The
//! `Macaroon::attenuate` routine + `verify_full` enforce this. The
//! `set_subsumes` helper checks caveat-set monotonicity for the
//! `verify_full` path.

#![forbid(unsafe_code)]
#![allow(missing_docs)]

pub mod caveat;
pub mod discharge;
pub mod dqa_serde;
pub mod macaroon;
pub mod signer;
pub mod token;
pub mod wire;

pub mod bundle_v2;
pub mod catalog;

pub mod vault_lookup;
pub mod vault_verify_error;

// Mission 0206-003 v3.0 trait move: HolderRegistry + RegistryError +
// the domain types (Clock, HolderKind, HolderRecord, BearerCapsule,
// CapabilityTokenLike, CapabilityClass) all moved here from
// `crates/quota-router-storage/`. Sole source of truth for the
// capability-macaroon domain types per RFC-0206 v2.1 §Layer B.
pub mod bearer_capsule_stub;
pub mod clock;
pub mod holder_kind;
pub mod holder_record;
pub mod holder_registry;

// Re-exports for ergonomic single-import paths.
pub use bearer_capsule_stub::BearerCapsule;
pub use bundle_v2::{
    BundleV2Error, CapabilityBundleV2, CapabilityBundleV2Envelope, CapabilityTokenV2,
    BUNDLE_ID_DOMAIN_V2, BUNDLE_VERSION_V2, CIPHEROCTO_V2_BUNDLE_PREFIX, MAX_CHAIN_DEPTH,
};
pub use catalog::{CompositeCapabilityCatalog, CompositeGossip};
pub use caveat::{
    set_subsumes, set_subsumes_with_registry, ActionTemplate, AskId, AttenuationError, Blake3,
    CachePolicy, Caveat, CaveatName, FactoryVet, ModelRef, OverlayIdentity, PaidQueryDecision,
    PaidQueryRejectionReason, PaymentCaveat, PerAxisMax, PermissionKind, ProviderId, RateLimit,
    RawCaveat, UnixTimeSecs, ISO3166, PAID_QUERY_CAVEAT_NAME,
};
pub use clock::{Clock, FixedClock, SystemClock};
pub use discharge::{
    verify_discharges, ChannelProvider, ChannelProviderRegistry, ChannelProviderResolver,
    DischargeChannel, DischargeError, DischargeRequest, DischargeVerification, EscrowBalance,
    EscrowDischargeProvider, RateLimitContext, RateLimitDischargeProvider,
    RevocationDischargeProvider, REVOCATION_DISCHARGE_TTL_SECS,
};
pub use holder_kind::HolderKind;
pub use holder_record::{CapabilityClass, CapabilityTokenLike, HolderRecord};
pub use holder_registry::{HolderRegistry, RegistryError};
pub use macaroon::{
    check_wrapped_chain, check_wrapped_depth, compute_capability_id, CapabilityCatalog,
    CapabilityGossip, CatalogGossipError, Macaroon, MacaroonError, MAX_WRAPPED_DEPTH,
};
pub use signer::{CapabilitySigner, CapabilitySignerError};
pub use token::{CapabilityToken, DischargeMacaroon, MintError};
pub use vault_lookup::{VaultLookup, VaultLookupExt, VaultRowSnapshot};
pub use vault_verify_error::VaultVerifyError;
pub use wire::{
    compute_cap_root_hash_from_wire, deserialize_wire, deserialize_wire_v2, serialize_wire,
    serialize_wire_v2, WireError, WireV2,
};

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
