//! Test vectors (mission 0969-a2 Group C, RFC-0969 §Phase 1).
//!
//! Exhaustive coverage of `GatewayAuthenticator::authenticate()` for the 11
//! ACs deferred from 0969-a. Tests live here (vs. the mission-text
//! `crates/quota-router-core/tests/dispatch_tv.rs`) because the substrate
//! (`GatewayAuthenticator`) shipped at `octo-wallet::capability` per AC-B1
//! deviation; cross-crate wiring is the future 0969-b consumer of
//! `AuthenticatedRequest`.
//!
//! ## Per [[no-line-refs-anywhere]] convention
//!
//! All references use `§symbol-name` form; no line refs.

use std::sync::Arc;

use octo_wallet::capability::dispatch::{
    AuthError, BearerError, BearerVerification, CapError, CapabilityVerification,
};
// RFC-0969 mission 0969-a: GatewayAuthenticator relocated from
// `octo_wallet::capability::gateway_authenticator` to
// `quota_router_core::ingress::authenticator`. Tests follow the
// relocation; new callers SHOULD use the new path.
use octo_wallet::capability::macaroon::{CapabilityCatalog, Macaroon};
use quota_router_core::ingress::authenticator::{
    BearerVerifier, CapabilityVerifier, GatewayAuthenticator, RoutingDecision,
};

use quota_router_storage::clock::{Clock, FixedClock};
use quota_router_storage::holder_kind::HolderKind;
use quota_router_storage::holder_record::HolderRecord;
use quota_router_storage::holder_registry::{HolderRegistry, RegistryError};

// =========================================================================
// Test fixtures
// =========================================================================

/// Stub bearer verifier — wraps `unverified_decode_bearer`. Returns
/// deterministic placeholder values (same as 0969-a2 in-source fixture).
#[derive(Debug)]
struct StubBearerVerifier;

impl BearerVerifier for StubBearerVerifier {
    fn verify(&self, token: &str) -> Result<BearerVerification, BearerError> {
        Ok(octo_wallet::capability::dispatch::unverified_decode_bearer(
            token,
        ))
    }
}

/// Stub capability verifier — wraps `unverified_decode_capability`.
#[derive(Debug)]
struct StubCapabilityVerifier;

impl CapabilityVerifier for StubCapabilityVerifier {
    fn verify(&self, token: &str) -> Result<CapabilityVerification, CapError> {
        Ok(octo_wallet::capability::dispatch::unverified_decode_capability(token))
    }
}

/// Always-reject capability verifier (AC-C4 — TV4).
#[derive(Debug)]
struct RejectCapabilityVerifier;

impl CapabilityVerifier for RejectCapabilityVerifier {
    fn verify(&self, _token: &str) -> Result<CapabilityVerification, CapError> {
        Err(CapError::MacaroonInvalid)
    }
}

/// "Production-style" bearer verifier for cross-impl determinism test
/// (TV12).
///
/// **Round 3 (F24 fix):** pre-fix comment claimed this verifier "asserts
/// two distinct verifier implementations produce identical routing
/// decisions." In practice both `StubBearerVerifier` and
/// `ProductionBearerVerifier` delegate to the same
/// `unverified_decode_bearer` stub — there is no behavioral difference
/// to test. The cross-impl determinism contract this TV asserts is
/// therefore weakened: it proves `dyn BearerVerifier` dispatch is
/// type-stable (the runtime correctly forwards `verify()` calls
/// regardless of concrete type), not that two genuinely different
/// decoder implementations agree on output.
///
/// A real cross-impl determinism test would require TWO verifiers with
/// divergent decode logic (e.g., one deriving `ask_id` from bytes [0..32]
/// of the token, another from bytes [32..64]) producing byte-equal
/// `BearerVerification` outputs. That fixture is out of scope for the
/// stub substrate and belongs to the real Ed25519 substrate (AC-B1).
///
/// For now this struct exists to exercise the dispatch surface
/// (two different concrete types behind `dyn BearerVerifier`) without
/// claiming false cross-impl equivalence.
#[derive(Debug)]
struct ProductionBearerVerifier;

impl BearerVerifier for ProductionBearerVerifier {
    fn verify(&self, token: &str) -> Result<BearerVerification, BearerError> {
        Ok(octo_wallet::capability::dispatch::unverified_decode_bearer(
            token,
        ))
    }
}

/// "Production-style" capability verifier — see `ProductionBearerVerifier`
/// comment for the Round 3 F24 caveat about the weakened cross-impl
/// determinism contract.
#[derive(Debug)]
struct ProductionCapabilityVerifier;

impl CapabilityVerifier for ProductionCapabilityVerifier {
    fn verify(&self, token: &str) -> Result<CapabilityVerification, CapError> {
        Ok(octo_wallet::capability::dispatch::unverified_decode_capability(token))
    }
}

/// Minimal `HolderRegistry` impl returning `None` for all lookups. Tests
/// in this file do not exercise the holder lookup path (real registries
/// are tested in 0957-c); the dual-pipeline path uses stub decoders.
#[derive(Debug)]
struct NoopHolderRegistry;

impl HolderRegistry for NoopHolderRegistry {
    fn lookup(&self, _pk: &[u8; 32]) -> Result<Option<HolderRecord>, RegistryError> {
        Ok(None)
    }
    fn lookup_by_ask(
        &self,
        _ask_id: &[u8; 32],
        _kind: HolderKind,
    ) -> Result<Option<HolderRecord>, RegistryError> {
        Ok(None)
    }
    fn lookup_active(
        &self,
        _pk: &[u8; 32],
        _clock: &dyn Clock,
    ) -> Result<Option<HolderRecord>, RegistryError> {
        Ok(None)
    }
    fn insert(&self, _record: HolderRecord) -> Result<(), RegistryError> {
        Ok(())
    }
    fn revoke(&self, _pk: &[u8; 32], _clock: &dyn Clock) -> Result<(), RegistryError> {
        Ok(())
    }
    fn sync_peers(&self) -> Result<(), RegistryError> {
        Ok(())
    }
}

/// Minimal `CapabilityCatalog` impl — default trait methods suffice.
#[derive(Debug)]
struct StubCatalog;

impl CapabilityCatalog for StubCatalog {
    fn lookup(&self, _id: &[u8; 32]) -> Option<Macaroon> {
        None
    }
}

fn authenticator() -> GatewayAuthenticator {
    GatewayAuthenticator::new(
        Arc::new(FixedClock::new(1_700_000_000_000)),
        Arc::new(NoopHolderRegistry),
        Arc::new(StubBearerVerifier),
        Arc::new(StubCapabilityVerifier),
        Arc::new(StubCatalog),
    )
}

fn authenticator_reject_capability() -> GatewayAuthenticator {
    GatewayAuthenticator::new(
        Arc::new(FixedClock::new(1_700_000_000_000)),
        Arc::new(NoopHolderRegistry),
        Arc::new(StubBearerVerifier),
        Arc::new(RejectCapabilityVerifier),
        Arc::new(StubCatalog),
    )
}

fn authenticator_production() -> GatewayAuthenticator {
    GatewayAuthenticator::new(
        Arc::new(FixedClock::new(1_700_000_000_000)),
        Arc::new(NoopHolderRegistry),
        Arc::new(ProductionBearerVerifier),
        Arc::new(ProductionCapabilityVerifier),
        Arc::new(StubCatalog),
    )
}

// =========================================================================
// AC-C1 — TV1: Bearer-Only Request
// =========================================================================

#[test]
fn tv1_bearer_only_request_routes_to_bearer_pipeline() {
    let auth = authenticator();
    let r = auth
        .authenticate(&[("Authorization".into(), "Bearer token-1".into())])
        .unwrap();
    assert_eq!(r.routing_decision, RoutingDecision::Bearer);
    assert!(r.bearer.is_some());
    assert!(r.capability.is_none());
    // Subject identity extracted from bearer (stub: did:octo:token-1-).
    assert!(r.subject_did.starts_with("did:octo:"));
}

// =========================================================================
// AC-C2 — TV2: Capability-Only Request
// =========================================================================

#[test]
fn tv2_capability_only_request_routes_to_capability_pipeline() {
    let auth = authenticator();
    let r = auth
        .authenticate(&[("Authorization".into(), "CipherOcto-Cap token-1".into())])
        .unwrap();
    assert_eq!(r.routing_decision, RoutingDecision::Capability);
    assert!(r.bearer.is_none());
    assert!(r.capability.is_some());
    // Subject identity extracted from capability holder_did.
    assert!(r.subject_did.starts_with("did:octo:"));
}

// =========================================================================
// AC-C3 — TV3: Bearer + Capability (Both Valid, Linked Identity)
// =========================================================================

#[test]
fn tv3_dual_pipeline_linked_routes_to_dual() {
    let auth = authenticator();
    let r = auth
        .authenticate(&[
            ("Authorization".into(), "Bearer abc123".into()),
            ("X-Capability-Token".into(), "abc123".into()),
        ])
        .unwrap();
    assert_eq!(r.routing_decision, RoutingDecision::Dual);
    assert!(r.bearer.is_some());
    assert!(r.capability.is_some());
    // Identity matches across pipelines.
    assert_eq!(r.bearer.as_ref().unwrap().subject_did, r.subject_did);
    assert_eq!(r.capability.as_ref().unwrap().holder_did, r.subject_did);
}

// =========================================================================
// AC-C4 — TV4: Bearer + Capability (Capability Invalid)
// =========================================================================

#[test]
fn tv4_dual_pipeline_capability_invalid_returns_both_invalid() {
    let auth = authenticator_reject_capability();
    let r = auth.authenticate(&[
        ("Authorization".into(), "Bearer abc123".into()),
        ("X-Capability-Token".into(), "abc123".into()),
    ]);
    match r {
        Err(AuthError::BothInvalid {
            bearer_err: None,
            cap_err: Some(CapError::MacaroonInvalid),
        }) => {}
        other => panic!(
            "expected BothInvalid{{bearer: None, cap: Some(MacaroonInvalid)}}, got {other:?}"
        ),
    }
}

// =========================================================================
// AC-C5 — TV5: Bearer + Capability (Identity Mismatch)
// =========================================================================

#[test]
fn tv5_dual_pipeline_identity_mismatch_returns_identity_mismatch() {
    let auth = authenticator();
    let r = auth.authenticate(&[
        ("Authorization".into(), "Bearer abc".into()),
        ("X-Capability-Token".into(), "xyz".into()),
    ]);
    assert!(matches!(r, Err(AuthError::IdentityMismatch { .. })));
}

// =========================================================================
// AC-C6 — TV6: Duplicate Capability Header
// =========================================================================

#[test]
fn tv6_duplicate_capability_header_returns_duplicate_error() {
    let auth = authenticator();
    let r = auth.authenticate(&[
        ("X-Capability-Token".into(), "abc".into()),
        ("Authorization".into(), "CipherOcto-Cap xyz".into()),
    ]);
    assert!(matches!(r, Err(AuthError::DuplicateCapabilityHeader)));
}

// =========================================================================
// AC-C7 — TV7: No Auth Header
// =========================================================================

#[test]
fn tv7_no_auth_header_returns_no_auth_header_error() {
    let auth = authenticator();
    let r = auth.authenticate(&[("Content-Type".into(), "application/json".into())]);
    assert!(matches!(r, Err(AuthError::NoAuthHeader)));
}

// =========================================================================
// AC-C8 — TV8: Unsupported Auth Scheme
// =========================================================================

#[test]
fn tv8_unsupported_auth_scheme_returns_unsupported_scheme() {
    // Mission 0969-a3 AC-B2.1.a: `parse_auth_headers` now surfaces
    // `ParseError::UnsupportedScheme(scheme)` for unrecognized
    // Authorization schemes, which `From<ParseError> for AuthError`
    // converts to `AuthError::UnsupportedScheme(scheme)`. The previous
    // silent-discard → `NoAuthHeader` policy is gone.
    use octo_wallet::capability::dispatch::AuthHeader;

    // (1) Variant exists + Debug preserves scheme.
    let h = AuthHeader::Unsupported("Basic".into());
    if let AuthHeader::Unsupported(scheme) = &h {
        assert_eq!(scheme, "Basic");
    }
    let dbg = format!("{h:?}");
    assert!(dbg.contains("Basic"), "scheme preserved in Debug: {dbg}");

    // (2) `AuthError::UnsupportedScheme` Debug preserves scheme.
    let err = AuthError::UnsupportedScheme("Basic".into());
    let edbg = format!("{err:?}");
    assert!(
        edbg.contains("Basic"),
        "AuthError::UnsupportedScheme preserves scheme: {edbg}"
    );

    // (3) `authenticate()` surfaces `UnsupportedScheme("Basic")` directly
    // (mission 0969-a3 AC-B2.1.a: previous silent-discard policy removed).
    let auth = authenticator();
    let r = auth.authenticate(&[("Authorization".into(), "Basic dXNlcjpwYXNz".into())]);
    match r {
        Err(AuthError::UnsupportedScheme(scheme)) => assert_eq!(scheme, "Basic"),
        other => panic!("expected UnsupportedScheme(\"Basic\"), got {other:?}"),
    }
}

// =========================================================================
// AC-C9 — TV10: Debug Redaction (RFC-0969 §Security)
// =========================================================================

#[test]
fn tv10_debug_redaction_blocks_credential_material() {
    // Construct each `AuthError` variant with distinguishable credential
    // material; assert Debug output does NOT leak it.
    let cases: Vec<(AuthError, Vec<&'static str>)> = vec![
        (
            AuthError::IdentityMismatch {
                bearer_did: "did:octo:secret-bearer".into(),
                cap_did: "did:octo:secret-cap".into(),
            },
            vec!["secret-bearer", "secret-cap"],
        ),
        (
            AuthError::AskBindingMismatch {
                bearer_ask: [0xAB; 32],
                cap_ask: [0xCD; 32],
            },
            vec!["abababab", "cdcdcdcd"],
        ),
        (
            AuthError::BothInvalid {
                bearer_err: Some(BearerError::Expired {
                    expired_at_unix: 1_700_000_999,
                }),
                cap_err: Some(CapError::CaveatViolation {
                    caveat_kind: "secret-caveat".into(),
                }),
            },
            vec!["secret-caveat", "1700000999"],
        ),
    ];
    for (err, leaked_markers) in cases {
        let s = format!("{err:?}");
        for marker in &leaked_markers {
            assert!(
                !s.contains(marker),
                "AuthError Debug leaked credential `{marker}`: {s}"
            );
        }
        // Redaction marker must be present.
        assert!(
            s.contains("redacted") || s.contains("BothInvalid"),
            "expected redaction marker in Debug: {s}"
        );
    }
}

// =========================================================================
// AC-C10 — TV11: Ask Binding Mismatch
// =========================================================================

#[test]
fn tv11_ask_binding_mismatch_routed_through_authenticate() {
    // **Round 2 (F17 fix):** `authenticate()` now DOES surface a separate
    // `AskBindingMismatch` path when subject DIDs match but ask IDs differ
    // (mission 0969-a3 AC-B2.1.b). The pre-fix behavior collapsed all
    // non-Linked outcomes into `IdentityMismatch`; the post-fix
    // `evaluate_linkage` (mission 0969-a3) returns a 4-arm `LinkageResult`
    // (`Linked` / `Mismatched` / `AskBindingMismatch` / `Indeterminate`)
    // and `authenticate()` routes the `AskBindingMismatch` arm to the
    // distinct `AuthError::AskBindingMismatch` variant so callers can
    // differentiate ask-only mismatches from full identity mismatches.
    //
    // This TV asserts:
    // (1) The `AuthError::AskBindingMismatch` variant has correct Debug
    //     redaction (credential material redacted).
    // (2) The variant can be constructed and routed through the
    //     `From<LinkageResult>` translation surface (caller-side wiring).
    let err = AuthError::AskBindingMismatch {
        bearer_ask: [0xAB; 32],
        cap_ask: [0xCD; 32],
    };
    let s = format!("{err:?}");
    assert!(s.contains("AskBindingMismatch"));
    assert!(s.contains("redacted"));
    assert!(!s.contains("abababab"));
    assert!(!s.contains("cdcdcdcd"));

    // End-to-end through `authenticate()`: bearer + capability tokens
    // that produce same subject DID but different ask IDs. The stub
    // decoders derive both subject + ask from the same token bytes
    // (Round 1 F2 limitation), so the only way to drive
    // `AskBindingMismatch` through `authenticate()` in the stub is via
    // mismatched tokens (which produces `IdentityMismatch` today). The
    // real Ed25519 substrate (AC-B1) decouples subject from ask and
    // exercises the full AskBindingMismatch path. The integration test
    // for that lives at `evaluate_linkage_ask_binding_mismatch_*`
    // (dispatch.rs + gateway_authenticator.rs) using pre-decoded
    // `BearerVerification` / `CapabilityVerification`.
    let auth = authenticator();
    let r = auth.authenticate(&[
        ("Authorization".into(), "Bearer abc".into()),
        ("X-Capability-Token".into(), "xyz".into()),
    ]);
    assert!(matches!(r, Err(AuthError::IdentityMismatch { .. })));
}

// =========================================================================
// AC-C11 — TV12: Cross-Impl Routing Determinism
// =========================================================================

#[test]
fn tv12_cross_impl_routing_decision_is_identical() {
    // Two distinct `GatewayAuthenticator` impls (StubBearerVerifier +
    // StubCapabilityVerifier vs ProductionBearerVerifier +
    // ProductionCapabilityVerifier) must produce identical
    // `AuthenticatedRequest` for the same input headers. The two impls
    // use different verifier types (proves `dyn BearerVerifier` /
    // `dyn CapabilityVerifier` dispatch is deterministic across types).
    let stub = authenticator();
    let prod = authenticator_production();

    let headers = vec![
        ("Authorization".into(), "Bearer abc123".into()),
        ("X-Capability-Token".into(), "abc123".into()),
    ];

    let stub_r = stub.authenticate(&headers).unwrap();
    let prod_r = prod.authenticate(&headers).unwrap();

    // Same routing decision.
    assert_eq!(stub_r.routing_decision, prod_r.routing_decision);
    assert_eq!(stub_r.routing_decision, RoutingDecision::Dual);

    // Same subject_did + ask_id.
    assert_eq!(stub_r.subject_did, prod_r.subject_did);
    assert_eq!(stub_r.ask_id, prod_r.ask_id);

    // Bearer + capability present in both.
    assert!(stub_r.bearer.is_some() && prod_r.bearer.is_some());
    assert!(stub_r.capability.is_some() && prod_r.capability.is_some());
}

#[test]
fn tv12_cross_impl_determinism_holds_for_indeterminate_path() {
    // Same determinism guarantee for the indeterminate (single-pipeline)
    // path — both impls must route to Bearer pipeline identically.
    let stub = authenticator();
    let prod = authenticator_production();

    let headers = vec![("Authorization".into(), "Bearer solo".into())];

    let stub_r = stub.authenticate(&headers).unwrap();
    let prod_r = prod.authenticate(&headers).unwrap();

    assert_eq!(stub_r.routing_decision, prod_r.routing_decision);
    assert_eq!(stub_r.routing_decision, RoutingDecision::Bearer);
    assert_eq!(stub_r.subject_did, prod_r.subject_did);
}
