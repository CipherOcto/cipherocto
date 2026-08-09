//! Capability token (macaroon v1 per RFC-0957).
//!
//! Holder-bound + caveat-chained bearer token. The verifier checks:
//! 1. Holder signature (Ed25519 over `canonical_ser(root_id || caveats_wire)`).
//! 2. HMAC chain re-derivation against the issuer's root secret.
//! 3. Caveat evaluation against the request context.

pub mod audit_log;
pub mod audit_replay_log;
pub mod bearer_capsule_re_export;
pub mod caveat;
pub mod destination_nonce_store;
pub mod discharge;
pub mod dispatch;
pub mod dual_issuance;
pub mod federation;
pub mod gateway_authenticator;
pub mod gc;
pub mod gossip;
pub mod hop_envelope;
pub mod macaroon;
pub mod market_delivery;
pub mod redemption;
pub mod registry;
pub mod verify;
pub mod wire;
pub mod zk_mint;

pub use bearer_capsule_re_export::BearerCapsule;
pub use caveat::{Caveat, CaveatName, MicroOctoW, UnixTimeSecs};
pub use discharge::{
    verify_discharges, ChannelProvider, ChannelProviderRegistry, ChannelProviderResolver,
    DischargeChannel, DischargeError, DischargeRequest, DischargeVerification, EscrowBalance,
    EscrowDischargeProvider, RateLimitContext, RateLimitDischargeProvider,
    RevocationDischargeProvider, REVOCATION_DISCHARGE_TTL_SECS,
};
pub use macaroon::{
    hmac_blake3, macaroon_id, CapabilityCatalog, Macaroon, MacaroonError, MacaroonId,
};
pub use market_delivery::{
    DealSettled, DealSettledPayload, DeliveryError, EnvelopeId, MarketDeliveryEnvelope,
    MarketDeliveryEnvelopePreimage, RoleTag, SettlementChainError,
};
pub use octo_cap_macaroon::DischargeMacaroon;
pub use registry::{CapabilityClassRegistry, RegistryEntry, RegistryError};
pub use verify::{verify_with_resolve, VerifiedToken, VerifyContext, VerifyError};
pub use wire::{
    compute_cap_root_hash_from_wire, deserialize_wire, deserialize_wire_v2, serialize_wire,
    serialize_wire_v2, WireError, WireV2,
};
pub use zk_mint::{
    bundled_casm_hash, mint_with_zk, mint_with_zk_and_signers, proof_bundle_from_wire,
    proof_bundle_to_wire, CapabilityClass, ExecutionTrace, PrivateWitness, ProofBundle,
    PublicInputs, TraceStep, ZkMintError, COMPILED_CASM_BLAKE3_HASH,
};

// Mission 0957 Phase 2b-3: `CapabilityToken` + `MintError` moved into
// the `octo-cap-macaroon` extension crate. Re-exports here preserve
// `octo_wallet::capability::{CapabilityToken, MintError}` import paths.
// `MintError::Hsm` variant is replaced by `MintError::Signer` (new
// `CapabilitySigner` trait abstraction); callers mapping to `WalletError`
// should update their error mapping.
pub use octo_cap_macaroon::{CapabilityToken, MintError};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capability::caveat::Caveat;
    use crate::capability::macaroon::InMemoryCatalog;
    use crate::identity::IdentityKey;

    fn empty_catalog() -> InMemoryCatalog {
        InMemoryCatalog::default()
    }

    #[test]
    fn mint_and_verify_holder_sig() {
        let holder = IdentityKey::generate().unwrap();
        let root_secret = [0x42; 32];
        let caveats = [Caveat::Model("gpt-4".to_owned())];
        let token = CapabilityToken::mint(
            &root_secret,
            &holder,
            &octo_ident::test_helpers::sample_did(104),
            &caveats,
        )
        .unwrap();
        token.verify_holder_sig().unwrap();
    }

    #[test]
    fn attenuate_preserves_holder_pub() {
        let holder = IdentityKey::generate().unwrap();
        let root_secret = [0x42; 32];
        let catalog = empty_catalog();
        let token = CapabilityToken::mint(
            &root_secret,
            &holder,
            &octo_ident::test_helpers::sample_did(104),
            &[],
        )
        .unwrap();
        let attenuated = token
            .attenuate_with_signer(Caveat::Model("gpt-4".to_owned()), &holder, &catalog)
            .unwrap();
        assert_eq!(attenuated.holder_pub, token.holder_pub);
        attenuated.verify_holder_sig().unwrap();
    }

    #[test]
    fn attenuate_without_signer_breaks_holder_sig() {
        let holder = IdentityKey::generate().unwrap();
        let root_secret = [0x42; 32];
        let catalog = empty_catalog();
        let token = CapabilityToken::mint(
            &root_secret,
            &holder,
            &octo_ident::test_helpers::sample_did(104),
            &[],
        )
        .unwrap();
        let broken = token
            .attenuate(Caveat::Model("gpt-4".to_owned()), &catalog)
            .unwrap();
        // Without re-signing, holder sig is stale.
        assert!(broken.verify_holder_sig().is_err());
    }
}
