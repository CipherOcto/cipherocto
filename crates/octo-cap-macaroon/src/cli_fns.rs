//! Layer B `[ADD]` free functions per RFC-0011 §Subcommand Taxonomy entries #10–13.
//!
//! Thin facades over [`crate::token::CapabilityToken`] so the CLI
//! (`octo-cli`, Layer C/D) can consume a single, stable surface
//! without reaching into the substrate's internal
//! `mint`/`attenuate`/`attenuate_with_signer` matrix directly.
//!
//! ## Layer discipline
//!
//! Per [[cipherocto-design-principles]]: this module is **Layer B
//! additive surface**; it depends downward into [`crate::token`],
//! [`crate::signer`], and [`crate::catalog`]. Higher layers consume
//! these free functions — they never reach into the underlying types
//! from outside.
//!
//! ## Stub status
//!
//! `list_active`, `mint`, and `attenuate` are intentional stubs
//! (return `Ok(Vec::new())` / `Err(MintError::HolderSig(...))`).
//! Phase 1 of `0011-capability-commands` lands the substrate surface
//! + types; full implementation lives in Phase 2 once the holder
//! registry / catalog wiring decisions are finalized per RFC-0011
//! §Implementation Phases. Caveat-parse / caveat-combination errors
//! are surfaced directly by the CLI layer as
//! `OctoCliError::CaveatParse` (exit 7) or
//! `OctoCliError::InvalidCaveatCombination` (exit 8) — the substrate
//! does not need its own caveat-validation variant.

use crate::catalog::CompositeCapabilityCatalog;
use crate::caveat::Caveat;
use crate::cli_summary::CapabilitySummary;
use crate::signer::CapabilitySigner;
use crate::token::{CapabilityToken, MintError};

/// List active capabilities held by `holder`.
///
/// Stub: returns an empty `Vec`. The full implementation reads from
/// the holder registry (Layer B per RFC-0206) once the holder
/// registry substrate is wired into the CLI integration point.
///
/// # Errors
///
/// Never errors in the stub form. The full implementation will
/// surface `MintError::HolderSig` / registry read errors.
pub fn list_active<S: CapabilitySigner + ?Sized>(
    _holder: &S,
) -> Result<Vec<CapabilitySummary>, MintError> {
    Ok(Vec::new())
}

/// Mint a new capability token.
///
/// Thin facade over [`CapabilityToken::mint`] that the CLI reaches
/// via this single entry point rather than calling the substrate
/// type directly. Future CLI-specific pre/post hooks (e.g., wire
/// encoding, holder-registry persistence per RFC-0969) will be
/// composed here without modifying [`CapabilityToken::mint`] itself
/// (Layer B additive principle).
///
/// # Errors
///
/// Returns whatever [`CapabilityToken::mint`] returns. Today the stub
/// short-circuits to `MintError::HolderSig("stub: not implemented")`
/// because `CapabilityToken::mint` does not yet consume `holder_did`
/// as a separate parameter (it stores `holder_did` itself). The full
/// implementation will call [`CapabilityToken::mint`] directly.
pub fn mint<S: CapabilitySigner + ?Sized>(
    root_secret: &[u8; 32],
    holder: &S,
    holder_did: &str,
    caveats: &[Caveat],
) -> Result<CapabilityToken, MintError> {
    let _ = (root_secret, holder, holder_did, caveats);
    Err(MintError::HolderSig(
        "stub: octo_cap_macaroon::cli_fns::mint is not yet wired; \
         use CapabilityToken::mint directly until Phase 2 lands"
            .to_owned(),
    ))
}

/// Attenuate a parent capability.
///
/// Thin facade over [`CapabilityToken::attenuate_with_signer`]. Stub:
/// short-circuits to `MintError::HolderSig`. The full
/// implementation iterates `caveats` and chains
/// `parent.attenuate_with_signer(c, holder, catalog.as_ref())` for
/// each caveat, then returns the final token.
///
/// # Errors
///
/// Returns whatever [`CapabilityToken::attenuate_with_signer`] returns.
pub fn attenuate<S: CapabilitySigner + ?Sized>(
    parent: &CapabilityToken,
    caveats: &[Caveat],
    holder: &S,
    catalog: &CompositeCapabilityCatalog,
) -> Result<CapabilityToken, MintError> {
    let _ = (parent, caveats, holder, catalog);
    Err(MintError::HolderSig(
        "stub: octo_cap_macaroon::cli_fns::attenuate is not yet wired; \
         use CapabilityToken::attenuate_with_signer directly until \
         Phase 2 lands"
            .to_owned(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::macaroon::{CapabilityGossip, CatalogGossipError, InMemoryCatalog};
    use crate::signer::CapabilitySignerError;
    use async_trait::async_trait;
    use ed25519_dalek::{Signer, SigningKey};

    struct TestSigner(SigningKey);

    impl CapabilitySigner for TestSigner {
        fn sign(&self, msg: &[u8]) -> Result<[u8; 64], CapabilitySignerError> {
            Ok(self.0.sign(msg).to_bytes())
        }
        fn public_key_bytes(&self) -> [u8; 32] {
            self.0.verifying_key().to_bytes()
        }
    }

    /// No-op gossip backend for the stub test — `attenuate` never
    /// actually invokes gossip, but `CompositeCapabilityCatalog::new`
    /// requires `&dyn CapabilityGossip`. Mirrors the `RecordingGossip`
    /// pattern from `catalog/composite.rs` tests.
    struct NoopGossip;

    #[async_trait]
    impl CapabilityGossip for NoopGossip {
        async fn gossip_to_buyer(
            &self,
            _buyer_did: &str,
            _env: &[u8],
        ) -> Result<(), CatalogGossipError> {
            Ok(())
        }
    }

    fn fixture() -> TestSigner {
        TestSigner(SigningKey::from_bytes(&[0x42u8; 32]))
    }

    /// `list_active` stub must return an empty Vec, not panic.
    /// CLI layer depends on this contract for the no-caps case.
    #[test]
    fn list_active_stub_returns_empty() {
        let holder = fixture();
        let result = list_active(&holder).expect("list_active stub");
        assert!(result.is_empty(), "list_active stub must return empty Vec");
    }

    /// `mint` stub must return `HolderSig` (not panic, not hang). The
    /// stub uses the closest available variant (no `Other`/caveat
    /// variant exists in `MintError` per LAYER-01 layer-model audit).
    /// CLI depends on this contract to render a "not implemented"
    /// error envelope rather than crashing.
    #[test]
    fn mint_stub_returns_holder_sig() {
        let holder = fixture();
        let root = [0x42u8; 32];
        let result = mint(&root, &holder, "did:octo:zStubMint", &[]);
        match result {
            Err(MintError::HolderSig(msg)) => {
                assert!(
                    msg.contains("not yet wired"),
                    "stub error must explain Phase 2 status, got: {msg}"
                );
            }
            other => panic!("expected HolderSig stub error, got {other:?}"),
        }
    }

    /// `attenuate` stub must return `HolderSig`. Composite catalog
    /// + signer + parent token are constructed but unused (the stub
    /// binds them with `let _`); this test pins the contract.
    #[test]
    fn attenuate_stub_returns_holder_sig() {
        let holder = fixture();
        let root = [0x42u8; 32];
        // Mint a real parent via the substrate entry point so we have
        // a non-garbage `CapabilityToken` to pass.
        let parent = CapabilityToken::mint(
            &root,
            &holder,
            "did:octo:zStubAttenuate",
            &[Caveat::Model("gpt-4".to_owned())],
        )
        .expect("mint parent");
        let catalog = CompositeCapabilityCatalog::new(
            std::sync::Arc::new(InMemoryCatalog::default()),
            std::sync::Arc::new(NoopGossip),
        );
        let result = attenuate(&parent, &[Caveat::Before(2_000_000_000)], &holder, &catalog);
        match result {
            Err(MintError::HolderSig(msg)) => {
                assert!(
                    msg.contains("not yet wired"),
                    "stub error must explain Phase 2 status, got: {msg}"
                );
            }
            other => panic!("expected HolderSig stub error, got {other:?}"),
        }
    }
}
