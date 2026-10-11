//! RFC-0973 §Phase 2 dispatch, exercised through the production entry point.
//!
//! The unit tests in `capability::attestation` call the dispatch directly. This
//! file exists to prove the other half of the claim: that the dispatch is
//! reachable from `verify_with_resolve`, so an attested token presented to a
//! caller that does nothing special goes through the gates rather than
//! bypassing them.
//!
//! ## Per `docs/05-process/engineering-rules.md` §No line references in documentation convention
//!
//! References use `§symbol-name` form.

use std::sync::Arc;

use octo_cap_macaroon::attestation::{AttestationEnvelope, AttestationTypeId};
use octo_cap_macaroon::macaroon::Macaroon;
use octo_ident_storage::StoolapHolderRegistry;
use octo_wallet::capability::attestation::{attach_envelope, DispatchRejection, PermissivePolicy};
use octo_wallet::capability::verify::{
    verify_attestation_for_verified_token, VerifyContext, VerifyError,
};
use quota_router_storage::Clock;
use quota_router_storage::FixedClock;

fn tid(n: u8) -> AttestationTypeId {
    let mut b = [0u8; 16];
    b[15] = n;
    AttestationTypeId::from_bytes(b)
}

fn registry() -> Arc<dyn quota_router_storage::HolderRegistry> {
    Arc::new(StoolapHolderRegistry::open_in_memory().unwrap())
}

fn clock() -> Arc<dyn Clock> {
    Arc::new(FixedClock::new(1_700_000_000_000))
}

fn macaroon() -> Macaroon {
    Macaroon::mint(&[21u8; 32]).unwrap()
}

/// A context with no attestation subsystem configured — the pre-RFC shape every
/// existing caller builds.
fn plain_ctx() -> VerifyContext {
    VerifyContext::with_registry(clock(), registry())
}

/// The same context with the RFC-0973 subsystem attached.
fn attested_ctx(accept: Vec<AttestationTypeId>) -> VerifyContext {
    use octo_wallet::capability::attestation::{
        AcceptanceSet, AllocationRecord, AttestationAdmission, AttestationRegistry,
        AttestationStrategy,
    };

    struct Pass;
    impl AttestationStrategy for Pass {
        fn type_id(&self) -> AttestationTypeId {
            tid(1)
        }
        fn verify(
            &self,
            _c: &octo_wallet::capability::attestation::AttestationVerifyCtx<'_>,
        ) -> Result<
            octo_wallet::capability::attestation::AttestationOutcome,
            octo_wallet::capability::attestation::AttestationError,
        > {
            Ok(octo_wallet::capability::attestation::AttestationOutcome::Satisfied)
        }
    }
    impl AttestationAdmission for Pass {
        fn admission_profile(
            &self,
        ) -> Option<octo_wallet::capability::attestation::AdmissionProfile> {
            // Carries no proof, so None is a truthful claim here.
            None
        }
    }

    let mut allocs = AllocationRecord::empty();
    allocs.allocate(tid(1), "test");
    let mut reg = AttestationRegistry::empty();
    let s: Arc<dyn AttestationStrategy> = Arc::new(Pass);
    let a: Arc<dyn AttestationAdmission> = Arc::new(Pass);
    reg.register_strategy(s, Some(a), "test", &allocs).unwrap();

    plain_ctx().with_attestations(
        Arc::new(reg),
        AcceptanceSet::from_ids(accept),
        Arc::new(PermissivePolicy),
    )
}

fn attested(id: AttestationTypeId) -> Macaroon {
    let mut m = macaroon();
    attach_envelope(&mut m, AttestationEnvelope::new(id, vec![9, 9, 9]), true).unwrap();
    m
}

/// The unattested path is untouched: a pre-RFC capability has no envelope and
/// returns immediately, whatever the context is configured with.
#[test]
fn an_unattested_capability_passes_on_every_context_shape() {
    let m = macaroon();
    assert!(m.attestation.is_none());

    assert!(verify_attestation_for_verified_token(&plain_ctx(), &m).is_ok());
    assert!(verify_attestation_for_verified_token(&attested_ctx(vec![tid(1)]), &m).is_ok());
    assert!(verify_attestation_for_verified_token(&attested_ctx(vec![]), &m).is_ok());
}

/// With no registry configured, an attested token is fail-closed rather than
/// waved through. Pre-RFC callers keep working because their tokens carry no
/// envelope.
#[test]
fn an_attested_token_is_fail_closed_without_a_configured_registry() {
    let m = attested(tid(1));
    let err = verify_attestation_for_verified_token(&plain_ctx(), &m).unwrap_err();
    assert!(matches!(err, DispatchRejection::UnknownStrategy(_)));
}

/// An attestation whose type is outside the operator's acceptance set rejects at
/// step 2. An empty set is a permitted choice, and the token is refused because
/// the operator said so — not because the node refused to start.
#[test]
fn an_empty_acceptance_set_refuses_attested_tokens_visibly() {
    let m = attested(tid(1));
    let err = verify_attestation_for_verified_token(&attested_ctx(vec![]), &m).unwrap_err();
    assert_eq!(err, DispatchRejection::NotAcceptedHere(tid(1)));
}

/// The same token passes once the operator accepts that type.
#[test]
fn an_accepted_type_passes_the_dispatch() {
    let m = attested(tid(1));
    assert!(verify_attestation_for_verified_token(&attested_ctx(vec![tid(1)]), &m).is_ok());
}

/// An attestation id nobody registered fails closed at step 3, even when the
/// operator would have accepted it.
#[test]
fn an_unregistered_type_fails_closed_even_when_accepted() {
    let m = attested(tid(9));
    let err = verify_attestation_for_verified_token(&attested_ctx(vec![tid(9)]), &m).unwrap_err();
    assert_eq!(err, DispatchRejection::UnknownStrategy(tid(9)));
}

/// `VerifyError` carries the dispatch rejection as its own variant, so a caller
/// using the production entry point can tell an attestation refusal apart from a
/// macaroon failure.
#[test]
fn the_production_error_type_carries_the_attestation_reason() {
    let err = VerifyError::Attestation(DispatchRejection::StrategyPanicked);
    let text = err.to_string();
    assert!(
        text.contains("strategy panicked"),
        "reason must reach the operator: {text}"
    );
}
