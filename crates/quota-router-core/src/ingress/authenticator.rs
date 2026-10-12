// RFC-0969 §Phase 1: dual-pipeline gateway authenticator.
//
// Relocated from `crates/octo-wallet/src/capability/gateway_authenticator.rs`
// (mission 0969-a) so the authenticator sits in the same crate as its
// primary consumer (`quota-router-core::proxy::handle_request`). The
// orchestrator substrate (DispatchSet parsing + LinkageResult enum +
// CapabilityCatalog trait) remains in `octo-wallet::capability::*` per
// the per-extension crate model — only the orchestrator moves.
//
// Pipeline:
//   1. parse_auth_headers(headers)        → DispatchSet (linkage evaluated)
//   2. bearer_verifier.verify(token)      → BearerVerification  (if present)
//   3. cap_verifier.verify(token)         → CapabilityVerification  (if present)
//   4. Check identity linkage: subject_did + ask_id equality
//      - Mismatch → AuthError::IdentityMismatch
//      - Indeterminate (one present, other absent) → AuthError::Indeterminate
//   5. Return AuthenticatedRequest with subject_did + ask_id + capabilities

use std::sync::Arc;

use octo_cap_macaroon::clock::Clock;
use octo_cap_macaroon::holder_registry::HolderRegistry;
use octo_wallet::capability::dispatch::{
    parse_auth_headers, AuthError, AuthHeader, BearerError, BearerVerification, CapError,
    CapabilityVerification, DispatchSet, LinkageResult,
};
use octo_wallet::capability::macaroon::CapabilityCatalog;

/// Bearer verifier trait (RFC-0969 §Phase 1).
///
/// Real implementations will validate Ed25519 signature + RFC-0903
/// `BearerCapsule` structure. The default impl is a deterministic stub
/// that extracts placeholder `subject_did` + `ask_id` from token bytes
/// — sufficient for `GatewayAuthenticator` plumbing tests; production
/// deployments swap in a real `BearerVerifier`.
pub trait BearerVerifier: Send + Sync {
    fn verify(&self, token: &str) -> Result<BearerVerification, BearerError>;
}

/// Capability verifier trait (RFC-0969 §Phase 1).
///
/// Real implementations will validate the macaroon HMAC chain (RFC-0957)
/// and look up `HolderRecord::holder_did` via `HolderRegistry::lookup_by_ask`.
/// Default impl is a deterministic stub.
pub trait CapabilityVerifier: Send + Sync {
    fn verify(&self, token: &str) -> Result<CapabilityVerification, CapError>;
}

/// Routing decision emitted by `authenticate()`. RFC-0969 §Phase 1 leaves
/// the routing strategy pluggable; consumers (e.g. `quota-router-core`
/// `Ingress`) read `decision.kind` to dispatch to the right pipeline.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RoutingDecision {
    /// Bearer pipeline (RFC-0903 path).
    Bearer,
    /// Capability pipeline (RFC-0957 path).
    Capability,
    /// Dual-pipeline (bearer + capability, both verified + linked).
    ///
    /// **Superseded by RFC-0969 v1.2's dispatch table.** `authenticate_v12`
    /// never produces it. It is retained because [`GatewayAuthenticator::authenticate`]
    /// still implements the pre-v1.2 semantics and removing the variant would
    /// break callers that have not migrated. Mission `0969-c` tracks the
    /// retirement.
    Dual,
    /// Both `Authorization` schemes were present — RFC-0969 §Dispatch Table
    /// (v1.2), rejected as client misconfiguration.
    ///
    /// Per the 2026-08-08 clarification a client sending both is a bug, not a
    /// routing decision. The legitimate "both" case is RFC-0959-A1 market
    /// delivery, which is server-side delivery, not HTTP request ingress.
    BothSchemesUnsupported,
    /// Neither `Authorization` header was present — RFC-0969 §Dispatch Table
    /// (v1.2).
    ///
    /// Reserved for completeness: `parse_auth_headers` returns
    /// `Err(ParseError::NoAuthHeader)` on empty input, so this arm is only
    /// reachable if a future revision relaxes that. Defined because the table
    /// specifies it, not because the current parser can produce it.
    NoAuth,
    /// Pure forwarder (RFC-0970 `PureForwarder`).
    PureForward,
}

/// `AuthenticatedRequest` is the canonical return type of `authenticate()`.
/// Carries enough information for downstream consumers to dispatch without
/// re-running the verifier pipeline.
#[derive(Clone)]
pub struct AuthenticatedRequest {
    pub subject_did: String,
    pub ask_id: [u8; 32],
    pub bearer: Option<BearerVerification>,
    pub capability: Option<CapabilityVerification>,
    pub routing_decision: RoutingDecision,
}

// Manual redacting Debug (RFC-0969 §Security): subject_did + ask_id are
// credential material; field values must NOT appear in Debug output. The
// `routing_decision` is operational metadata and is preserved.
impl std::fmt::Debug for AuthenticatedRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuthenticatedRequest")
            .field("subject_did", &"<redacted>")
            .field("ask_id", &"<redacted 32 bytes>")
            .field("bearer", &self.bearer)
            .field("capability", &self.capability)
            .field("routing_decision", &self.routing_decision)
            .finish()
    }
}

/// `GatewayAuthenticator` (RFC-0969 §Phase 1) — the dual-pipeline authenticator.
///
/// Composes:
/// - `parse_auth_headers` (this crate) for header parsing + identity linkage.
/// - `BearerVerifier` + `CapabilityVerifier` (this crate) for token verification.
/// - `HolderRegistry` (quota-router-storage) for capability holder lookup.
/// - `Clock` (quota-router-storage) for timestamp + TTL checks.
/// - `CapabilityCatalog` (this crate) for catalog extension access.
#[allow(missing_debug_implementations)]
pub struct GatewayAuthenticator {
    pub clock: Arc<dyn Clock>,
    pub holder_registry: Arc<dyn HolderRegistry>,
    pub bearer_verifier: Arc<dyn BearerVerifier>,
    pub cap_verifier: Arc<dyn CapabilityVerifier>,
    #[allow(dead_code)] // Future AC: catalog lookup for gossip/settlement.
    pub catalog: Arc<dyn CapabilityCatalog>,
}

/// Outcome of a routing-decision computation. Reserved for future consumers
/// (e.g., policy log + metrics); not yet emitted by `authenticate()` which
/// uses `Result<AuthenticatedRequest, AuthError>` directly.
#[allow(dead_code)]
#[derive(Clone, Debug)]
pub enum AuthOutcome {
    Authenticated(AuthenticatedRequest),
    Rejected(AuthError),
}

impl GatewayAuthenticator {
    /// Construct a new authenticator with all dependencies injected.
    pub fn new(
        clock: Arc<dyn Clock>,
        holder_registry: Arc<dyn HolderRegistry>,
        bearer_verifier: Arc<dyn BearerVerifier>,
        cap_verifier: Arc<dyn CapabilityVerifier>,
        catalog: Arc<dyn CapabilityCatalog>,
    ) -> Self {
        Self {
            clock,
            holder_registry,
            bearer_verifier,
            cap_verifier,
            catalog,
        }
    }

    /// Entry point. Verifies the request and either returns the
    /// authenticated form or an `AuthError`.
    ///
    /// RFC-0969 §Dispatch Table (v1.2) — the routing decision as a pure
    /// function of which `Authorization` schemes are present.
    ///
    /// This is the whole of the v1.2 dispatch: **the decision is determined by
    /// the headers alone**. No side-channel lookup, no identity linkage, no
    /// cross-scheme comparison. That is the amendment's whole point — the
    /// pre-v1.2 machinery evaluated linkage between a bearer and a capability
    /// that the table now refuses to receive together.
    ///
    /// | Headers present | Decision                |
    /// | --------------- | ----------------------- |
    /// | bearer only     | `Bearer`                |
    /// | capability only | `Capability`            |
    /// | both            | `BothSchemesUnsupported` |
    /// | neither         | `NoAuth` — reserved; `parse_auth_headers` rejects empty input first |
    ///
    /// Additive: [`GatewayAuthenticator::authenticate`] still implements the
    /// pre-v1.2 semantics and can still return `RoutingDecision::Dual`. This is
    /// the function the RFC specifies; mission `0969-c` tracks migrating
    /// callers off the old path.
    pub fn routing_decision_v12(&self, dispatch: &DispatchSet) -> RoutingDecision {
        match (dispatch.bearer.is_some(), dispatch.capability.is_some()) {
            (true, false) => RoutingDecision::Bearer,
            (false, true) => RoutingDecision::Capability,
            (true, true) => RoutingDecision::BothSchemesUnsupported,
            (false, false) => RoutingDecision::NoAuth,
        }
    }

    /// Evaluate a request under RFC-0969 §Dispatch Table (v1.2), rejecting the
    /// both-schemes-present case as client misconfiguration.
    ///
    /// Returns the same [`AuthenticatedRequest`] shape as the pre-v1.2
    /// `authenticate`, with `routing_decision` set from the table. The
    /// difference is that `Err(AuthError::BothSchemesUnsupported)` replaces the
    /// linkage outcomes `IdentityMismatch` and `AskBindingMismatch` that v1.2
    /// retired.
    ///
    /// Verification of each scheme is unchanged and still delegated to the
    /// injected verifiers — this replaces the *dispatch*, which is what the
    /// amendment rewrote.
    pub fn authenticate_v12(
        &self,
        headers: &[(String, String)],
    ) -> Result<AuthenticatedRequest, AuthError> {
        let dispatch = parse_auth_headers(headers).map_err(AuthError::from)?;
        let decision = self.routing_decision_v12(&dispatch);

        // The one refusal the table adds. It happens before any verification,
        // so a dual-scheme request never reaches a verifier — which is the
        // property that makes cross-scheme linkage unreachable rather than
        // merely unused.
        if decision == RoutingDecision::BothSchemesUnsupported {
            return Err(AuthError::BothSchemesUnsupported);
        }

        let bearer = self.verify_bearer(&dispatch)?;
        let capability = self.verify_capability(&dispatch)?;

        // Identity comes from whichever single scheme verified. Under v1.2
        // exactly one of the two is populated, so there is no ambiguity to
        // resolve and no linkage comparison to make.
        let subject_did = bearer
            .as_ref()
            .map(|b| b.subject_did.clone())
            .or_else(|| capability.as_ref().map(|c| c.holder_did.clone()))
            .ok_or(AuthError::Indeterminate)?;
        let ask_id = bearer
            .as_ref()
            .map(|b| b.ask_id)
            .or_else(|| capability.as_ref().map(|c| c.ask_id))
            .ok_or(AuthError::Indeterminate)?;

        Ok(AuthenticatedRequest {
            subject_did,
            ask_id,
            bearer,
            capability,
            routing_decision: decision,
        })
    }

    /// Identity linkage rule (RFC-0969 §Phase 1 + AC-A2):
    /// - both bearer + capability verified → assert `subject_did == holder_did`
    ///   AND `ask_id == ask_id`. Mismatch → `IdentityMismatch`.
    /// - one pipeline only → `Indeterminate` (caller decides whether to allow).
    /// - neither → `NoAuthHeader` (via `ParseError → AuthError` conversion).
    pub fn authenticate(
        &self,
        headers: &[(String, String)],
    ) -> Result<AuthenticatedRequest, AuthError> {
        let dispatch = parse_auth_headers(headers).map_err(AuthError::from)?;
        let bearer = self.verify_bearer(&dispatch)?;
        let capability = self.verify_capability(&dispatch)?;
        let linkage = evaluate_linkage(bearer.as_ref(), capability.as_ref());
        match linkage {
            LinkageResult::Mismatched => Err(AuthError::IdentityMismatch {
                bearer_did: bearer
                    .as_ref()
                    .map(|b| b.subject_did.clone())
                    .unwrap_or_default(),
                cap_did: capability
                    .as_ref()
                    .map(|c| c.holder_did.clone())
                    .unwrap_or_default(),
            }),
            // Mission 0969-a3 AC-B2.1.b: subject DIDs match but ask IDs
            // differ. Surface as `AuthError::AskBindingMismatch` so the
            // caller can distinguish "wrong ask" from "wrong identity"
            // without re-running the linkage evaluation.
            LinkageResult::AskBindingMismatch {
                bearer_ask,
                cap_ask,
            } => Err(AuthError::AskBindingMismatch {
                bearer_ask,
                cap_ask,
            }),
            LinkageResult::Linked { .. } => {
                // Both verified + linked — extract identity from bearer (canonical).
                let b = bearer.as_ref().expect("Linked implies bearer present");
                Ok(AuthenticatedRequest {
                    subject_did: b.subject_did.clone(),
                    ask_id: b.ask_id,
                    bearer,
                    capability,
                    routing_decision: RoutingDecision::Dual,
                })
            }
            LinkageResult::Indeterminate => {
                // One pipeline only — caller decides whether to allow.
                let routing_decision = match (bearer.is_some(), capability.is_some()) {
                    (true, false) => RoutingDecision::Bearer,
                    (false, true) => RoutingDecision::Capability,
                    _ => return Err(AuthError::Indeterminate),
                };
                let subject_did = bearer
                    .as_ref()
                    .map(|b| b.subject_did.clone())
                    .or_else(|| capability.as_ref().map(|c| c.holder_did.clone()))
                    .ok_or(AuthError::Indeterminate)?;
                let ask_id = bearer
                    .as_ref()
                    .map(|b| b.ask_id)
                    .or_else(|| capability.as_ref().map(|c| c.ask_id))
                    .ok_or(AuthError::Indeterminate)?;
                Ok(AuthenticatedRequest {
                    subject_did,
                    ask_id,
                    bearer,
                    capability,
                    routing_decision,
                })
            }
        }
    }

    fn verify_bearer(
        &self,
        dispatch: &octo_wallet::capability::dispatch::DispatchSet,
    ) -> Result<Option<BearerVerification>, AuthError> {
        match &dispatch.bearer {
            Some(AuthHeader::Bearer(token)) => Ok(Some(
                self.bearer_verifier
                    .verify(token)
                    .map_err(AuthError::from)?,
            )),
            _ => Ok(None),
        }
    }

    fn verify_capability(
        &self,
        dispatch: &octo_wallet::capability::dispatch::DispatchSet,
    ) -> Result<Option<CapabilityVerification>, AuthError> {
        match &dispatch.capability {
            Some(AuthHeader::CipherOctoCap(token)) => Ok(Some(
                self.cap_verifier.verify(token).map_err(AuthError::from)?,
            )),
            _ => Ok(None),
        }
    }
}

/// Identity linkage evaluation (RFC-0969 §Phase 1 + AC-A2).
///
/// **Moved to `dispatch.rs` (mission 0969-a3 AC-B2.1.b)**: the linkage
/// decision logic is the canonical home for `LinkageResult` (the enum
/// itself lives in dispatch.rs). `evaluate_linkage` now lives next to
/// the enum. This wrapper is preserved as a re-export for downstream
/// callers (gateway_authenticator + tests).
pub use octo_wallet::capability::dispatch::evaluate_linkage;

#[cfg(test)]
mod tests {
    use super::*;
    use octo_cap_macaroon::clock::{Clock, FixedClock};
    use octo_cap_macaroon::holder_kind::HolderKind;
    use octo_cap_macaroon::holder_record::HolderRecord;
    use octo_cap_macaroon::holder_registry::{HolderRegistry, RegistryError};
    use octo_wallet::capability::dispatch::{
        BearerError, BearerVerification, CapError, CapabilityVerification, LinkageResult,
        ParseError,
    };
    use octo_wallet::capability::macaroon::CapabilityCatalog;
    use std::sync::Arc;

    // Compile-time check that `ParseError → AuthError` conversion exists.
    // Catches refactors that break the `From<ParseError> for AuthError`
    // impl in `dispatch.rs` without needing a runtime test.
    #[allow(dead_code)]
    fn _parse_error_to_auth_error(err: ParseError) -> AuthError {
        err.into()
    }

    /// Test `BearerVerifier` — wraps the dispatch stub.
    #[derive(Debug)]
    struct StubBearerVerifier;

    impl BearerVerifier for StubBearerVerifier {
        fn verify(&self, token: &str) -> Result<BearerVerification, BearerError> {
            // Delegate to dispatch stub (avoids duplicating decode logic).
            Ok(octo_wallet::capability::dispatch::unverified_decode_bearer(
                token,
            ))
        }
    }

    /// Test `CapabilityVerifier` — wraps the dispatch stub.
    #[derive(Debug)]
    struct StubCapabilityVerifier;

    impl CapabilityVerifier for StubCapabilityVerifier {
        fn verify(&self, token: &str) -> Result<CapabilityVerification, CapError> {
            Ok(octo_wallet::capability::dispatch::unverified_decode_capability(token))
        }
    }

    /// Always-reject bearer verifier for negative-path tests.
    #[derive(Debug)]
    struct RejectBearerVerifier;

    impl BearerVerifier for RejectBearerVerifier {
        fn verify(&self, _token: &str) -> Result<BearerVerification, BearerError> {
            Err(BearerError::Malformed)
        }
    }

    /// Always-reject capability verifier.
    #[derive(Debug)]
    struct RejectCapabilityVerifier;

    impl CapabilityVerifier for RejectCapabilityVerifier {
        fn verify(&self, _token: &str) -> Result<CapabilityVerification, CapError> {
            Err(CapError::MacaroonInvalid)
        }
    }

    /// Minimal `HolderRegistry` impl that returns `None` for all lookups.
    /// Sufficient for tests that don't exercise the holder lookup path.
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

    /// Minimal `CapabilityCatalog` impl. We use a closure stub via a fresh
    /// wrapper struct — `CapabilityCatalog` trait default impls return
    /// `Unsupported`, sufficient for `GatewayAuthenticator::new`.
    #[derive(Debug)]
    struct StubCatalog;

    impl CapabilityCatalog for StubCatalog {
        fn lookup(&self, _id: &[u8; 32]) -> Option<octo_wallet::capability::macaroon::Macaroon> {
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

    fn authenticator_reject_bearer() -> GatewayAuthenticator {
        GatewayAuthenticator::new(
            Arc::new(FixedClock::new(1_700_000_000_000)),
            Arc::new(NoopHolderRegistry),
            Arc::new(RejectBearerVerifier),
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

    #[test]
    fn bearer_only_routes_to_bearer_pipeline() {
        let auth = authenticator();
        let r = auth
            .authenticate(&[("Authorization".into(), "Bearer token-1".into())])
            .unwrap();
        assert_eq!(r.routing_decision, RoutingDecision::Bearer);
        assert!(r.bearer.is_some());
        assert!(r.capability.is_none());
    }

    #[test]
    fn capability_only_routes_to_capability_pipeline() {
        let auth = authenticator();
        let r = auth
            .authenticate(&[("Authorization".into(), "CipherOcto-Cap token-1".into())])
            .unwrap();
        assert_eq!(r.routing_decision, RoutingDecision::Capability);
        assert!(r.bearer.is_none());
        assert!(r.capability.is_some());
    }

    #[test]
    fn dual_pipeline_with_identical_tokens_routes_to_dual() {
        let auth = authenticator();
        let r = auth
            .authenticate(&[
                ("Authorization".into(), "Bearer abc".into()),
                ("X-Capability-Token".into(), "abc".into()),
            ])
            .unwrap();
        assert_eq!(r.routing_decision, RoutingDecision::Dual);
        assert!(r.bearer.is_some());
        assert!(r.capability.is_some());
    }

    #[test]
    fn dual_pipeline_with_mismatched_tokens_returns_identity_mismatch() {
        let auth = authenticator();
        let r = auth.authenticate(&[
            ("Authorization".into(), "Bearer abc".into()),
            ("X-Capability-Token".into(), "xyz".into()),
        ]);
        assert!(matches!(r, Err(AuthError::IdentityMismatch { .. })));
    }

    /// An authenticator whose verifiers count their invocations, so "refused
    /// before verification" is measurable rather than asserted.
    fn counting_auth() -> (
        GatewayAuthenticator,
        std::sync::Arc<std::sync::atomic::AtomicUsize>,
    ) {
        use std::sync::atomic::{AtomicUsize, Ordering};

        let counter = std::sync::Arc::new(AtomicUsize::new(0));

        struct CountingBearer(std::sync::Arc<AtomicUsize>);
        impl BearerVerifier for CountingBearer {
            fn verify(&self, _t: &str) -> Result<BearerVerification, BearerError> {
                self.0.fetch_add(1, Ordering::SeqCst);
                Ok(BearerVerification {
                    subject_did: "did:octo:zSubject".to_owned(),
                    ask_id: [0x11; 32],
                })
            }
        }
        struct CountingCap(std::sync::Arc<AtomicUsize>);
        impl CapabilityVerifier for CountingCap {
            fn verify(&self, _t: &str) -> Result<CapabilityVerification, CapError> {
                self.0.fetch_add(1, Ordering::SeqCst);
                Ok(CapabilityVerification {
                    holder_did: "did:octo:zSubject".to_owned(),
                    ask_id: [0x11; 32],
                })
            }
        }

        let c = counter.clone();
        let auth = GatewayAuthenticator::new(
            Arc::new(FixedClock::new(1_700_000_000_000)),
            Arc::new(NoopHolderRegistry),
            Arc::new(CountingBearer(c.clone())),
            Arc::new(CountingCap(c)),
            Arc::new(StubCatalog),
        );
        (auth, counter)
    }

    // --- 0969-a3 AC-B2.1.b: AskBindingMismatch distinguished from Mismatched ---

    /// The 4-way table, one row at a time (RFC-0969 §Dispatch Table v1.2).
    ///
    /// Each row is built from real headers through `parse_auth_headers`, not
    /// from a hand-assembled `DispatchSet`, so the decision is measured against
    /// the same parse the authenticator performs.
    #[test]
    fn routing_decision_v12_covers_the_four_way_table() {
        let bearer_only = vec![("Authorization".to_owned(), "Bearer tok-b".to_owned())];
        let cap_only = vec![("X-Capability-Token".to_owned(), "tok-c".to_owned())];
        let both = vec![
            ("Authorization".to_owned(), "Bearer tok-b".to_owned()),
            ("X-Capability-Token".to_owned(), "tok-c".to_owned()),
        ];

        let ctx = |d: DispatchSet| authenticator().routing_decision_v12(&d);

        let b = parse_auth_headers(&bearer_only).expect("bearer-only parses");
        assert_eq!(ctx(b), RoutingDecision::Bearer);

        let c = parse_auth_headers(&cap_only).expect("capability-only parses");
        assert_eq!(ctx(c), RoutingDecision::Capability);

        let d = parse_auth_headers(&both).expect("dual parses");
        assert_eq!(ctx(d), RoutingDecision::BothSchemesUnsupported);

        // `NoAuth` is reserved: `parse_auth_headers` rejects empty input with
        // `ParseError::NoAuthHeader` before a `DispatchSet` can exist with both
        // slots empty. Asserted on the constructed set so the row stays pinned.
        assert_eq!(
            ctx(DispatchSet {
                bearer: None,
                capability: None,
            }),
            RoutingDecision::NoAuth,
            "the neither row is reserved but specified"
        );
    }

    /// The property the amendment exists to establish: a dual-scheme request is
    /// refused, and refused **before** either verifier is consulted.
    ///
    /// The counters are the point. If either verifier ran, the request would
    /// have been evaluated rather than rejected as misconfiguration, and
    /// cross-scheme linkage would be reachable again.
    #[test]
    fn authenticate_v12_refuses_dual_schemes_without_verifying() {
        let both = vec![
            ("Authorization".to_owned(), "Bearer tok-b".to_owned()),
            ("X-Capability-Token".to_owned(), "tok-c".to_owned()),
        ];

        let (auth, calls) = counting_auth();
        let err = auth
            .authenticate_v12(&both)
            .expect_err("both schemes must be refused as client misconfiguration");
        assert!(
            matches!(err, AuthError::BothSchemesUnsupported),
            "expected BothSchemesUnsupported, got {err:?}"
        );
        assert_eq!(
            calls.load(std::sync::atomic::Ordering::SeqCst),
            0,
            "a refused dual-scheme request must not reach a verifier"
        );
    }

    /// The control for the test above: a single-scheme request *is* verified.
    /// Without this, refusing everything would also pass.
    #[test]
    fn authenticate_v12_verifies_a_single_scheme_request() {
        let bearer_only = vec![("Authorization".to_owned(), "Bearer tok-b".to_owned())];

        let (auth, calls) = counting_auth();
        let out = auth
            .authenticate_v12(&bearer_only)
            .expect("a single-scheme request must authenticate");
        assert_eq!(out.routing_decision, RoutingDecision::Bearer);
        assert_eq!(
            calls.load(std::sync::atomic::Ordering::SeqCst),
            1,
            "the single-scheme path must actually verify"
        );
    }

    /// Single-scheme capability routing, so both single rows are covered.
    #[test]
    fn authenticate_v12_routes_a_capability_only_request() {
        let cap_only = vec![("X-Capability-Token".to_owned(), "tok-c".to_owned())];

        let (auth, calls) = counting_auth();
        let out = auth
            .authenticate_v12(&cap_only)
            .expect("a capability-only request must authenticate");
        assert_eq!(out.routing_decision, RoutingDecision::Capability);
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    #[test]
    fn evaluate_linkage_ask_binding_mismatch_routes_to_auth_error() {
        // Direct test of `evaluate_linkage` with same subject DID + different
        // ask IDs. The stub decoders derive both subject DID and ask_id
        // from the token bytes; this test bypasses the decoder stubs by
        // calling `evaluate_linkage` with pre-constructed verification
        // structs that decouple the two (mimicking the real substrate).
        let b = BearerVerification {
            subject_did: "did:octo:holder-1".into(),
            ask_id: [0xAA; 32],
        };
        let c = CapabilityVerification {
            holder_did: "did:octo:holder-1".into(),
            ask_id: [0xBB; 32],
        };
        let r = evaluate_linkage(Some(&b), Some(&c));
        assert!(matches!(
            r,
            LinkageResult::AskBindingMismatch {
                bearer_ask,
                cap_ask,
            } if bearer_ask == [0xAA; 32] && cap_ask == [0xBB; 32]
        ));
    }

    // --- 0969-a3 AC-B2.1.a: UnsupportedScheme routed via authenticate() ---

    #[test]
    fn unsupported_auth_scheme_returns_unsupported_scheme_error() {
        // AC-B2.1.a: previously `authenticate()` returned `NoAuthHeader`
        // for `Authorization: Basic <b64>`. Now `parse_auth_headers`
        // surfaces `ParseError::UnsupportedScheme(scheme)` which converts
        // to `AuthError::UnsupportedScheme(scheme)`.
        let auth = authenticator();
        let r = auth.authenticate(&[("Authorization".into(), "Basic dXNlcjpwYXNz".into())]);
        match r {
            Err(AuthError::UnsupportedScheme(scheme)) => assert_eq!(scheme, "Basic"),
            other => panic!("expected UnsupportedScheme(\"Basic\"), got {other:?}"),
        }
    }

    #[test]
    fn bearer_verifier_failure_surfaces_via_both_invalid() {
        let auth = authenticator_reject_bearer();
        let r = auth.authenticate(&[("Authorization".into(), "Bearer bad".into())]);
        match r {
            Err(AuthError::BothInvalid {
                bearer_err: Some(BearerError::Malformed),
                cap_err: None,
            }) => {}
            other => panic!("expected BothInvalid with bearer_err=Malformed, got {other:?}"),
        }
    }

    #[test]
    fn capability_verifier_failure_surfaces_via_both_invalid() {
        let auth = authenticator_reject_capability();
        let r = auth.authenticate(&[("Authorization".into(), "CipherOcto-Cap bad".into())]);
        match r {
            Err(AuthError::BothInvalid {
                bearer_err: None,
                cap_err: Some(CapError::MacaroonInvalid),
            }) => {}
            other => panic!("expected BothInvalid with cap_err=MacaroonInvalid, got {other:?}"),
        }
    }

    #[test]
    fn no_auth_header_returns_no_auth_header_error() {
        let auth = authenticator();
        let r = auth.authenticate(&[("Content-Type".into(), "application/json".into())]);
        assert!(matches!(r, Err(AuthError::NoAuthHeader)));
    }

    #[test]
    fn duplicate_capability_header_returns_duplicate_error() {
        let auth = authenticator();
        let r = auth.authenticate(&[
            ("X-Capability-Token".into(), "abc".into()),
            ("Authorization".into(), "CipherOcto-Cap xyz".into()),
        ]);
        assert!(matches!(r, Err(AuthError::DuplicateCapabilityHeader)));
    }

    #[test]
    fn evaluate_linkage_linked_when_match() {
        let b = BearerVerification {
            subject_did: "did:octo:abc".into(),
            ask_id: [0xAA; 32],
        };
        let c = CapabilityVerification {
            holder_did: "did:octo:abc".into(),
            ask_id: [0xAA; 32],
        };
        let r = evaluate_linkage(Some(&b), Some(&c));
        assert!(matches!(r, LinkageResult::Linked { .. }));
    }

    #[test]
    fn evaluate_linkage_mismatched_when_different() {
        let b = BearerVerification {
            subject_did: "did:octo:abc".into(),
            ask_id: [0xAA; 32],
        };
        let c = CapabilityVerification {
            holder_did: "did:octo:xyz".into(),
            ask_id: [0xBB; 32],
        };
        assert_eq!(
            evaluate_linkage(Some(&b), Some(&c)),
            LinkageResult::Mismatched
        );
    }

    #[test]
    fn evaluate_linkage_indeterminate_when_one_absent() {
        let b = BearerVerification {
            subject_did: "did:octo:abc".into(),
            ask_id: [0xAA; 32],
        };
        assert_eq!(
            evaluate_linkage(Some(&b), None),
            LinkageResult::Indeterminate
        );
        assert_eq!(evaluate_linkage(None, None), LinkageResult::Indeterminate);
    }

    #[test]
    fn authenticated_request_has_redacted_debug() {
        let auth = authenticator();
        let r = auth
            .authenticate(&[("Authorization".into(), "Bearer token-1".into())])
            .unwrap();
        let s = format!("{r:?}");
        // subject_did + ask_id appear redacted in AuthenticatedRequest Debug
        // (delegates to BearerVerification Debug which redacts).
        assert!(s.contains("redacted"), "expected redaction: {s}");
        assert!(!s.contains("token-1"), "leaked token: {s}");
    }

    #[test]
    fn routing_decision_variants_are_distinct() {
        // Sanity check on the 4 routing outcomes.
        assert_ne!(RoutingDecision::Bearer, RoutingDecision::Capability);
        assert_ne!(RoutingDecision::Dual, RoutingDecision::PureForward);
    }

    #[test]
    fn authenticate_call_path_compiles_with_real_holder_registry() {
        // Compile-time check that `HolderRegistry` is wired in correctly.
        let _auth = authenticator();
    }

    // Verify the brace-balance invariant for `authenticate()`. The lint script
    // at `.github/linters/braces-balanced.sh authenticate` is the canonical
    // CI check; this in-source test is a structural smoke test that skips
    // braces inside string literals + line/block comments to avoid false
    // positives (e.g., `}` inside doc comments).
    #[test]
    fn authenticate_function_braces_balanced() {
        let src = include_str!("authenticator.rs");
        let auth_start = src
            .find("pub fn authenticate(")
            .expect("authenticate exists");
        let auth_body = &src[auth_start..];
        let (opens, closes) = count_braces_outside_strings_and_comments(auth_body);
        assert_eq!(
            opens, closes,
            "authenticate() braces unbalanced: {{ = {opens}, }} = {closes}"
        );
    }

    /// Count `{` and `}` in `s`, skipping braces inside string literals
    /// (`"..."`), line comments (`// ...`), or block comments (`/* ... */`).
    /// Cheap state machine — sufficient for the smoke test.
    fn count_braces_outside_strings_and_comments(s: &str) -> (usize, usize) {
        let mut opens = 0;
        let mut closes = 0;
        let bytes = s.as_bytes();
        let mut i = 0;
        let mut in_string = false;
        let mut in_line_comment = false;
        let mut in_block_comment = false;
        while i < bytes.len() {
            let c = bytes[i];
            if in_line_comment {
                if c == b'\n' {
                    in_line_comment = false;
                }
                i += 1;
                continue;
            }
            if in_block_comment {
                if c == b'*' && i + 1 < bytes.len() && bytes[i + 1] == b'/' {
                    in_block_comment = false;
                    i += 2;
                    continue;
                }
                i += 1;
                continue;
            }
            if in_string {
                if c == b'\\' && i + 1 < bytes.len() {
                    i += 2; // skip escape
                    continue;
                }
                if c == b'"' {
                    in_string = false;
                }
                i += 1;
                continue;
            }
            if c == b'/' && i + 1 < bytes.len() && bytes[i + 1] == b'/' {
                in_line_comment = true;
                i += 2;
                continue;
            }
            if c == b'/' && i + 1 < bytes.len() && bytes[i + 1] == b'*' {
                in_block_comment = true;
                i += 2;
                continue;
            }
            if c == b'"' {
                in_string = true;
                i += 1;
                continue;
            }
            if c == b'{' {
                opens += 1;
            } else if c == b'}' {
                closes += 1;
            }
            i += 1;
        }
        (opens, closes)
    }
}
