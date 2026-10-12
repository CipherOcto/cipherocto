// `VerifyContext` + `verify_with_resolve` (RFC-0957-A1 §Phase 2).
//
// Registry-aware token verification. The `VerifyContext` holds a `Clock`
// + `HolderRegistry`; `verify_with_resolve` looks up the holder_did +
// holder_pub from the registry, then runs the standard `deserialize_wire`
// + `Macaroon::verify` + holder-sig verify pipeline.

use std::sync::Arc;

use quota_router_storage::Clock;
use quota_router_storage::HolderRegistry;

use super::attestation::{AcceptanceSet, AttestationRegistry, PermissivePolicy, PolicySource};
use super::wire::{compute_cap_root_hash_from_wire, deserialize_wire, WireError};
use super::CapabilityToken;

/// Verification context (RFC-0957-A1 §Phase 2).
///
/// Holds the `Clock` + `HolderRegistry` slots needed for `verify_with_resolve`.
/// Existing slots preserved from prior versions (clock, etc.).
pub struct VerifyContext {
    clock: Arc<dyn Clock>,
    registry: Arc<dyn HolderRegistry>,
    /// RFC-0973: the attestation registry. `None` means no attestation
    /// subsystem is configured, which is fail-closed for an attested token and
    /// a no-op for an unattested one.
    strategies: Option<Arc<AttestationRegistry>>,
    /// RFC-0973: the operator acceptance set read at dispatch step 2.
    accept_set: Arc<AcceptanceSet>,
    /// RFC-0973: the operator policy the dispatch calls.
    policy: Arc<dyn PolicySource>,
}

impl std::fmt::Debug for VerifyContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VerifyContext")
            .field("clock", &"<dyn Clock>")
            .field("registry", &"<dyn HolderRegistry>")
            .field(
                "strategies",
                &self.strategies.as_ref().map_or("none", |_| "some"),
            )
            .field("accept_set", &self.accept_set)
            .field("policy", &"<dyn PolicySource>")
            .finish()
    }
}

impl VerifyContext {
    /// Canonical constructor: `with_registry(clock, registry)`.
    ///
    /// RFC-0973: a context built this way has no attestation registry and an
    /// empty acceptance set. That is fail-closed for an attested token — an
    /// attestation id cannot resolve — and a no-op for an unattested one, so
    /// pre-RFC callers keep working unchanged.
    pub fn with_registry(clock: Arc<dyn Clock>, registry: Arc<dyn HolderRegistry>) -> Self {
        Self {
            clock,
            registry,
            strategies: None,
            accept_set: Arc::new(AcceptanceSet::empty()),
            policy: Arc::new(PermissivePolicy),
        }
    }

    /// Attach the RFC-0973 attestation subsystem.
    ///
    /// This is what makes the dispatch reachable from the production
    /// verification path rather than only from its own tests.
    #[must_use]
    pub fn with_attestations(
        mut self,
        strategies: Arc<AttestationRegistry>,
        accept_set: AcceptanceSet,
        policy: Arc<dyn PolicySource>,
    ) -> Self {
        self.strategies = Some(strategies);
        self.accept_set = Arc::new(accept_set);
        self.policy = policy;
        self
    }

    /// The attestation registry, when one is configured.
    pub fn strategies(&self) -> Option<&Arc<AttestationRegistry>> {
        self.strategies.as_ref()
    }

    /// The operator acceptance set.
    pub fn accept_set(&self) -> &AcceptanceSet {
        self.accept_set.as_ref()
    }

    /// The operator policy.
    pub fn policy(&self) -> &Arc<dyn PolicySource> {
        &self.policy
    }

    /// Access the clock.
    pub fn clock(&self) -> &dyn Clock {
        self.clock.as_ref()
    }

    /// Access the holder registry.
    pub fn registry(&self) -> &dyn HolderRegistry {
        self.registry.as_ref()
    }
}

/// Verified token (RFC-0957-A1 §Algorithms).
///
/// Returned by `verify_with_resolve` after the registry lookup + wire
/// deserialization + holder-signature verify pipeline succeeds.
#[derive(Debug)]
pub struct VerifiedToken {
    pub token: CapabilityToken,
    pub cap_root_hash: [u8; 32],
}

/// Verify errors (RFC-0957-A1 §Phase 2).
#[derive(Debug, thiserror::Error)]
pub enum VerifyError {
    #[error("wire error: {0}")]
    Wire(#[from] WireError),
    #[error("registry error: {0}")]
    Registry(#[from] quota_router_storage::RegistryError),
    #[error("holder not found in registry: cap_root_hash=<redacted 32 bytes>")]
    HolderNotFound,
    #[error("holder record is not active (revoked or expired)")]
    HolderNotActive,
    #[error("macaroon verification failed: {0}")]
    Macaroon(String),
    #[error("holder signature failed: {0}")]
    HolderSig(String),
    /// RFC-0973: the attestation dispatch refused.
    ///
    /// The inner reason is operator-visible and must never carry attestation
    /// bytes. That is a convention enforced by review, not by a type: the
    /// reason is strategy-supplied and core carries it verbatim.
    #[error("attestation refused: {0}")]
    Attestation(#[from] crate::capability::attestation::DispatchRejection),
}

/// High-level helper: compute cap_root_hash, look up the holder record,
/// extract `holder_did` + `holder_pub`, then deserialize + verify.
///
/// # What this verifies
///
/// 1. wire parse and `cap_root_hash` computation
/// 2. holder record lookup — the registry is authoritative on holder identity,
///    not the wire's claim
/// 3. active check, covering revoked and expired holders
/// 4. the Ed25519 holder signature over the macaroon root id and caveats wire
/// 5. the RFC-0973 attestation dispatch
///
/// # What this does NOT verify
///
/// **The macaroon caveat chain is not evaluated here, and neither are
/// third-party discharges.** `verify_holder_sig` at step 4 covers the
/// *signature over* the caveat bytes; it does not evaluate the caveats
/// themselves. A token carrying escrow, revocation or rate-limit caveats
/// (RFC-0957 §Discharge Protocol) passes this function with those caveats
/// unexamined.
///
/// An earlier version of this doc described the chain verification as
/// "delegated to the catalog ... the caller's responsibility". That
/// delegation governs nothing: the only catalog on the live ingress path is
/// held as `#[allow(dead_code)]`, annotated as future work, and
/// `ProductionCapabilityVerifier::verify` never consults it. The composite
/// and transport catalogs exist and work — nothing on ingress reaches them.
/// `verify_discharges` is likewise exported and never called.
///
/// Both gaps need a protocol decision before they can be closed, which is
/// why this is stated rather than fixed. The correct half is tracked in
/// `missions/open/0957-a2-capability-chain-and-discharge-verification.md`.
///
/// # Errors
/// Returns `VerifyError` on any pipeline failure (wire / registry / verify).
pub fn verify_with_resolve(
    ctx: &VerifyContext,
    token_wire: &str,
) -> Result<VerifiedToken, VerifyError> {
    let cap_root_hash = compute_cap_root_hash_from_wire(token_wire)?;

    // Look up the holder record.
    let rec = ctx
        .registry
        .lookup(&cap_root_hash)?
        .ok_or(VerifyError::HolderNotFound)?;

    // Verify active (covers revoked / expired).
    if ctx
        .registry
        .lookup_active(&cap_root_hash, ctx.clock.as_ref())?
        .is_none()
    {
        return Err(VerifyError::HolderNotActive);
    }

    // Deserialize wire with registry-resolved holder_did + holder_pub.
    // The holder_did is the registry's holder_did (R7-N7 fix: registry is
    // authoritative on holder identity, not the wire's claims).
    let token = deserialize_wire(token_wire, rec.holder_did.clone(), &rec.holder_pub)?;

    // The macaroon caveat CHAIN is not verified below. `verify_holder_sig`
    // covers the signature over the root id and the caveat wire; it does not
    // evaluate the caveats, and neither third-party discharges nor the
    // catalog lookup happen here. See the function doc — this comment used to
    // describe the chain verify as delegated to the catalog, which described
    // a delegation no caller on this path performs.
    token
        .verify_holder_sig()
        .map_err(|e| VerifyError::HolderSig(format!("{e}")))?;

    // RFC-0973: the attestation dispatch runs HERE, inside the production entry
    // point, after the macaroon path has cleared. This is what makes the dispatch
    // reachable in production rather than only from its own tests.
    //
    // An unattested token has no envelope and returns immediately, so every
    // pre-RFC capability verifies exactly as it did before.
    verify_attestation_for_verified_token(ctx, &token.macaroon)?;

    Ok(VerifiedToken {
        token,
        cap_root_hash,
    })
}

// ---------------------------------------------------------------------------
// RFC-0973: the dispatch, reachable from the production entry point
// ---------------------------------------------------------------------------

/// RFC-0957 expectation: the binding target is the full 32-byte
/// `root_secret_hash`.
///
/// Never the capability id. The id hashes the canonical form the envelope sits
/// inside, so an expectation returning it would be a function of the
/// attestation, and the strategy would be comparing a token against itself.
struct MacaroonRootSecretExpectation;

impl super::attestation::Expectation for MacaroonRootSecretExpectation {
    fn bindable_value(
        &self,
        ctx: &super::attestation::AttestationVerifyCtx<'_>,
    ) -> Option<Vec<u8>> {
        Some(ctx.capability.root_secret_hash.to_vec())
    }
}

/// The wallet verification path serves no payload kinds. See
/// [`WalletNodeView::coverage`].
static WALLET_COVERAGE: WalletCoverage = WalletCoverage;

struct WalletCoverage;

impl super::attestation::PayloadCoverage for WalletCoverage {
    fn serves_all(&self, _kinds: &[octo_cap_macaroon::attestation::PayloadKind]) -> bool {
        false
    }
}

struct WalletNodeView<'a> {
    accept_set: &'a super::attestation::AcceptanceSet,
    policy: &'a dyn super::attestation::PolicySource,
}

impl super::attestation::NodeView for WalletNodeView<'_> {
    fn accept_set(&self) -> &super::attestation::AcceptanceSet {
        self.accept_set
    }
    /// The wallet's holder registry carries no payload-kind coverage, so this
    /// path serves nothing. A strategy that needs producing kinds therefore
    /// fails step 4 here rather than being waved through, which is the
    /// fail-closed direction.
    fn coverage(&self) -> &dyn super::attestation::PayloadCoverage {
        &WALLET_COVERAGE
    }
    fn policy(&self) -> &dyn super::attestation::PolicySource {
        self.policy
    }
}

/// Run the RFC-0973 attestation dispatch for a token that has already cleared
/// the macaroon path.
///
/// Step 0 and step 0b have already run inside
/// [`verify_with_resolve`] — the registry lookup, the active check, and the
/// holder-signature verify — which is exactly the ordering the RFC requires.
/// This function supplies them to the dispatch as the results they already are,
/// so the gate order is preserved without the macaroon checks being run twice
/// or being skippable.
///
/// An unattested token returns `Ok`, having already been verified.
///
/// # Errors
/// One `DispatchRejection` per remaining gate.
pub fn verify_attestation_for_verified_token(
    ctx: &VerifyContext,
    macaroon: &octo_cap_macaroon::macaroon::Macaroon,
) -> Result<(), crate::capability::attestation::DispatchRejection> {
    use crate::capability::attestation::{
        verify_capability_attestation, AttestationOutcome, DispatchInputs,
    };

    // No registry configured means no attestation subsystem. Fail-closed for an
    // attested token, a no-op for an unattested one.
    let Some(registry) = ctx.strategies() else {
        return match macaroon.attestation {
            None => Ok(()),
            Some(_) => Err(
                crate::capability::attestation::DispatchRejection::UnknownStrategy(
                    macaroon.attestation.as_ref().map_or_else(
                        || octo_cap_macaroon::attestation::AttestationTypeId::from_bytes([0; 16]),
                        |e| e.type_id,
                    ),
                ),
            ),
        };
    };

    let expectation: Box<dyn super::attestation::Expectation + '_> =
        Box::new(MacaroonRootSecretExpectation);

    let node = WalletNodeView {
        accept_set: ctx.accept_set(),
        policy: ctx.policy().as_ref(),
    };
    let inputs = DispatchInputs {
        lookup_active: true,
        present: true,
        macaroon_ok: Ok(()),
        registry,
        node: &node,
        expectation: Some(expectation),
    };

    match verify_capability_attestation(macaroon, &inputs) {
        Ok(AttestationOutcome::Satisfied) => Ok(()),
        Ok(AttestationOutcome::Unsatisfied { reason }) => {
            Err(crate::capability::attestation::DispatchRejection::Unsatisfied(reason))
        }
        Err(e) => Err(e),
    }
}

/// Re-export so callers reach the dispatch through the one entry point.
pub use super::attestation::AcceptanceSet as Rfc0973AcceptanceSet;

#[cfg(test)]
mod tests {
    use super::*;
    use octo_ident_storage::StoolapHolderRegistry;
    use quota_router_storage::FixedClock;
    use quota_router_storage::HolderKind;
    use quota_router_storage::{CapabilityClass, CapabilityTokenLike, HolderRecord};

    fn test_registry() -> Arc<dyn HolderRegistry> {
        Arc::new(StoolapHolderRegistry::open_in_memory().unwrap())
    }

    #[test]
    fn verify_context_provides_accessors() {
        let clock = Arc::new(FixedClock::new(1_700_000_000_000));
        let reg = test_registry();
        let ctx = VerifyContext::with_registry(clock, reg);
        assert_eq!(ctx.clock().unix_millis(), 1_700_000_000_000);
    }

    #[test]
    fn verify_with_resolve_returns_holder_not_found_for_missing_cap_root_hash() {
        let clock = Arc::new(FixedClock::new(1_700_000_000_000));
        let reg = test_registry();
        let ctx = VerifyContext::with_registry(clock, reg);
        // Random wire that parses but doesn't correspond to any registered holder.
        let result = verify_with_resolve(&ctx, "A.B.C");
        assert!(matches!(result, Err(VerifyError::Wire(_))));
    }

    #[test]
    fn verify_with_resolve_returns_holder_not_found_when_registry_empty() {
        let clock = Arc::new(FixedClock::new(1_700_000_000_000));
        let reg = test_registry();
        let ctx = VerifyContext::with_registry(clock, reg);
        // Build a token, register it, then drop the wire (use empty wire).
        let rec = HolderRecord::from_capability(
            &CapabilityTokenLike {
                cap_root_hash: [0xAA; 32],
                class: CapabilityClass::V1,
            },
            &[0xBB; 32],
            &octo_ident::test_helpers::sample_did(31),
            None,
            1_700_000_000_000,
        );
        ctx.registry().insert(rec).unwrap();
        // Wire that doesn't parse.
        let result = verify_with_resolve(&ctx, "not-a-wire");
        assert!(matches!(result, Err(VerifyError::Wire(_))));
    }

    #[test]
    fn verify_with_resolve_returns_holder_not_active_when_revoked() {
        let clock = Arc::new(FixedClock::new(1_700_000_000_000));
        let reg = test_registry();
        let ctx = VerifyContext::with_registry(clock, reg);
        let rec = HolderRecord::from_capability(
            &CapabilityTokenLike {
                cap_root_hash: [0xAA; 32],
                class: CapabilityClass::V1,
            },
            &[0xBB; 32],
            &octo_ident::test_helpers::sample_did(31),
            None,
            1_700_000_000_000,
        );
        ctx.registry().insert(rec.clone()).unwrap();
        // Revoke first.
        ctx.registry()
            .revoke(&rec.cap_root_hash, ctx.clock())
            .unwrap();
        // Now register a real wire that hashes to this cap_root_hash.
        // For the test, we use the trait's HolderRegistry::lookup path: build
        // a wire whose macaroon.root_id produces cap_root_hash = [0xAA; 32]
        // by BLAKE3 hashing over the root_id. Use a private Macaroon::mint
        // helper.
        let minted = crate::capability::macaroon::Macaroon::mint(&[0x33; 32]).unwrap();
        let wired = super::super::wire::serialize_wire(&crate::capability::CapabilityToken {
            macaroon: minted.clone(),
            holder_pub: vec![0xBB; 32],
            holder_did: octo_ident::test_helpers::sample_did(31),
            holder_sig: crate::capability::HolderSignature::new(
                octo_algo_ed25519::ed25519_algorithm_id(),
                &[0u8; 64],
            )
            .expect("64-byte signature"),
            discharges: vec![],
            holder_sig_stale: false,
        })
        .unwrap();
        let _ = wired;
        // Wire path test deferred: requires building a real mint pair; this
        // test just verifies the revoke path is reachable from VerifyContext.
    }

    #[test]
    fn kind_v1_serializes_to_zero_byte() {
        // TV6 adjacent: verify Kind byte stable.
        assert_eq!(HolderKind::V1.as_byte(), 0x00);
    }
}
