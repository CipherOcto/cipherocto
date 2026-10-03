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
//! `list_active` remains a stub returning `Ok(Vec::new())` — the
//! active capability inventory is held in the in-process CLI
//! registry for now (the holder-registry substrate is not yet
//! wired). `mint` and `attenuate` are wired: `mint` delegates to
//! [`CapabilityToken::mint`], `attenuate` iterates `caveats` and
//! chains [`CapabilityToken::attenuate_with_signer`]. The composite
//! catalog is consulted per-append so the `WrappedOnly` chain guard
//! is enforced at the substrate boundary. Caveat-parse / caveat-
//! combination errors are surfaced directly by the CLI layer as
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
/// type directly. CLI-specific pre/post hooks (e.g., wire encoding,
/// holder-registry persistence per RFC-0969) compose here without
/// modifying [`CapabilityToken::mint`] itself (Layer B additive
/// principle).
///
/// # Errors
///
/// Returns whatever [`CapabilityToken::mint`] returns — RNG failure
/// surfaces as `MintError::Macaroon`, holder key rejection as
/// `MintError::Signer`.
pub fn mint<S: CapabilitySigner>(
    root_secret: &[u8; 32],
    holder: &S,
    holder_did: &str,
    caveats: &[Caveat],
) -> Result<CapabilityToken, MintError> {
    CapabilityToken::mint(root_secret, holder, holder_did, caveats)
}

/// Attenuate a parent capability by chaining a slice of caveats.
///
/// Thin facade over [`CapabilityToken::attenuate_with_signer`]. Each
/// caveat in `caveats` is appended to the parent in order, with the
/// holder key re-signing the chain after every append. The catalog
/// (composite storage + gossip) is consulted per-append so the
/// `WrappedOnly` chain guard is enforced at the substrate boundary
/// for every caveat, not only the last.
///
/// # Errors
///
/// Returns the first error from [`CapabilityToken::attenuate_with_signer`]:
/// `MintError::Macaroon` on catalog cycle / depth / parent-not-found /
/// `UnknownRawName`, or `MintError::Signer` on holder key rejection.
pub fn attenuate<S: CapabilitySigner>(
    parent: &CapabilityToken,
    caveats: &[Caveat],
    holder: &S,
    catalog: &CompositeCapabilityCatalog,
) -> Result<CapabilityToken, MintError> {
    let mut current = parent.clone();
    for caveat in caveats {
        current = current.attenuate_with_signer(caveat.clone(), holder, catalog)?;
    }
    Ok(current)
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

    /// `mint` is wired: returns a real `CapabilityToken` whose
    /// `holder_did` matches the input and whose macaroon carries
    /// the requested initial caveats. Holder signature is set, not
    /// stale, because the substrate path signs at mint.
    #[test]
    fn mint_returns_real_token_with_holder_did_and_initial_caveats() {
        let holder = fixture();
        let root = [0x42u8; 32];
        let initial = [Caveat::Model("gpt-4".to_owned())];
        let token = mint(&root, &holder, "did:octo:zMintLive", &initial)
            .expect("mint must succeed when wired");
        assert_eq!(token.holder_did, "did:octo:zMintLive");
        assert_eq!(token.macaroon.caveats, initial);
        assert!(
            !token.holder_sig_stale,
            "mint must produce a fresh, signed token"
        );
    }

    /// `attenuate` is wired: chains caveats in order, returns a
    /// non-stale signed token whose macaroon has all caveats
    /// appended. The catalog's `WrappedOnly` chain guard is enforced
    /// per-append at the substrate boundary.
    #[test]
    fn attenuate_chains_caveats_in_order_and_resigns() {
        let holder = fixture();
        let root = [0x42u8; 32];
        let parent = CapabilityToken::mint(
            &root,
            &holder,
            "did:octo:zAttenuateLive",
            &[Caveat::Model("gpt-4".to_owned())],
        )
        .expect("mint parent");
        let catalog = CompositeCapabilityCatalog::new(
            std::sync::Arc::new(InMemoryCatalog::default()),
            std::sync::Arc::new(NoopGossip),
        );
        let new_caveats = [
            Caveat::Before(2_000_000_000),
            Caveat::Model("gpt-3.5-turbo".to_owned()),
        ];
        let child = attenuate(&parent, &new_caveats, &holder, &catalog)
            .expect("attenuate must succeed when wired");
        let expected = {
            let mut all = parent.macaroon.caveats.clone();
            all.extend(new_caveats.iter().cloned());
            all
        };
        assert_eq!(child.macaroon.caveats, expected);
        assert!(
            !child.holder_sig_stale,
            "attenuate_with_signer path must resign, not mark stale"
        );
        // Sanity: parent is unchanged (attenuate is non-mutating).
        assert_eq!(parent.macaroon.caveats.len(), 1);
    }
}
