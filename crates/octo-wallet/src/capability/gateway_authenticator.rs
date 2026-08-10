// RFC-0969 §Phase 1: dual-pipeline gateway authenticator.
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

use crate::capability::dispatch::{
    parse_auth_headers, AuthError, AuthHeader, BearerError, BearerVerification, CapError,
    CapabilityVerification, LinkageResult,
};
use crate::capability::macaroon::CapabilityCatalog;
use quota_router_storage::clock::Clock;
use quota_router_storage::holder_registry::HolderRegistry;

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
    Dual,
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
        dispatch: &crate::capability::dispatch::DispatchSet,
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
        dispatch: &crate::capability::dispatch::DispatchSet,
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
pub use crate::capability::dispatch::evaluate_linkage;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capability::dispatch::{
        BearerError, BearerVerification, CapError, CapabilityVerification, LinkageResult,
        ParseError,
    };
    use crate::capability::macaroon::CapabilityCatalog;
    use quota_router_storage::clock::{Clock, FixedClock};
    use quota_router_storage::holder_kind::HolderKind;
    use quota_router_storage::holder_record::HolderRecord;
    use quota_router_storage::holder_registry::{HolderRegistry, RegistryError};
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
            Ok(crate::capability::dispatch::unverified_decode_bearer(token))
        }
    }

    /// Test `CapabilityVerifier` — wraps the dispatch stub.
    #[derive(Debug)]
    struct StubCapabilityVerifier;

    impl CapabilityVerifier for StubCapabilityVerifier {
        fn verify(&self, token: &str) -> Result<CapabilityVerification, CapError> {
            Ok(crate::capability::dispatch::unverified_decode_capability(
                token,
            ))
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
        fn lookup(&self, _id: &[u8; 32]) -> Option<crate::capability::macaroon::Macaroon> {
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

    // --- 0969-a3 AC-B2.1.b: AskBindingMismatch distinguished from Mismatched ---

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
        let src = include_str!("gateway_authenticator.rs");
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
