// RFC-0969 §Phase 1: dual-pipeline gateway header parser.
//
// Parses `Authorization: Bearer <token>` + `Authorization: CipherOcto-Cap <token>`
// + `X-Capability-Token: <token>` from a request. Returns `DispatchSet` with
// identity linkage validation.

/// AuthHeader (RFC-0969 §Phase 1).
#[derive(Clone, PartialEq, Eq)]
pub enum AuthHeader {
    Bearer(String),
    CipherOctoCap(String),
    None,
    Unsupported(String),
}

impl std::fmt::Debug for AuthHeader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Bearer(_) => f.debug_tuple("Bearer").field(&"<redacted>").finish(),
            Self::CipherOctoCap(_) => f.debug_tuple("CipherOctoCap").field(&"<redacted>").finish(),
            Self::None => f.write_str("None"),
            Self::Unsupported(s) => f.debug_tuple("Unsupported").field(&s).finish(),
        }
    }
}

/// ParseError (RFC-0969 §Phase 1).
#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("duplicate CipherOcto-Cap header")]
    DuplicateCapabilityHeader,
    #[error("no Authorization header found")]
    NoAuthHeader,
    /// Authorization header carries an unrecognized scheme (e.g., `Basic`,
    /// `Digest`, `Negotiate`). The scheme name is preserved as operational
    /// metadata (not credential material). Mission 0969-a3 AC-B2.1.a:
    /// surfaced via `From<ParseError> for AuthError` as
    /// `AuthError::UnsupportedScheme`.
    #[error("unsupported auth scheme: {0}")]
    UnsupportedScheme(String),
}

/// LinkageResult (RFC-0969 §Phase 1).
///
/// Mission 0969-a3 AC-B2.1.b adds `AskBindingMismatch` so the
/// `authenticate()` path can distinguish a same-identity / different-ask
/// mismatch from a full identity mismatch (different subject DID +
/// different ask ID).
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum LinkageResult {
    Linked {
        subject_did: String,
        ask_id: [u8; 32],
    },
    Mismatched,
    /// Subject DIDs match but ask IDs differ. Mission 0969-a3 AC-B2.1.b:
    /// surfaced via `authenticate()` as `AuthError::AskBindingMismatch`.
    AskBindingMismatch {
        bearer_ask: [u8; 32],
        cap_ask: [u8; 32],
    },
    Indeterminate,
}

/// `BearerVerification` (RFC-0969 §Phase 1) — decoded bearer token content
/// from a successful `verify_bearer_token` call. Holds the canonical
/// `subject_did` (issuer-asserted holder identity, RFC-0009 DID form) and
/// `ask_id` (the ask this bearer was issued against).
#[derive(Clone, PartialEq, Eq)]
pub struct BearerVerification {
    pub subject_did: String,
    pub ask_id: [u8; 32],
}

impl std::fmt::Debug for BearerVerification {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BearerVerification")
            .field("subject_did", &"<redacted>")
            .field("ask_id", &"<redacted 32 bytes>")
            .finish()
    }
}

/// `CapabilityVerification` (RFC-0969 §Phase 1) — decoded capability token
/// content from a successful `verify_capability_token` call. Holds the
/// `holder_did` (RFC-0957-A1 `HolderRecord::holder_did`) and `ask_id` (the
/// ask this capability was issued against).
#[derive(Clone, PartialEq, Eq)]
pub struct CapabilityVerification {
    pub holder_did: String,
    pub ask_id: [u8; 32],
}

impl std::fmt::Debug for CapabilityVerification {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CapabilityVerification")
            .field("holder_did", &"<redacted>")
            .field("ask_id", &"<redacted 32 bytes>")
            .finish()
    }
}

/// `BearerError` (RFC-0969 §Phase 1) — bearer verification failure modes.
#[derive(thiserror::Error, Clone, PartialEq, Eq)]
pub enum BearerError {
    #[error("malformed bearer token")]
    Malformed,
    #[error("invalid bearer signature")]
    InvalidSignature,
    #[error("bearer expired: expired_at_unix={expired_at_unix}")]
    Expired { expired_at_unix: u64 },
}

impl std::fmt::Debug for BearerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Malformed => f.write_str("Malformed"),
            Self::InvalidSignature => f.write_str("InvalidSignature"),
            Self::Expired { expired_at_unix } => f
                .debug_struct("Expired")
                .field("expired_at_unix", expired_at_unix)
                .finish(),
        }
    }
}

/// `CapError` (RFC-0969 §Phase 1) — capability verification failure modes.
#[derive(thiserror::Error, Clone, PartialEq, Eq)]
pub enum CapError {
    #[error("macaroon invalid")]
    MacaroonInvalid,
    #[error("caveat violation: caveat_kind={caveat_kind}")]
    CaveatViolation { caveat_kind: String },
    #[error("capability expired: expired_at_unix={expired_at_unix}")]
    Expired { expired_at_unix: u64 },
}

impl std::fmt::Debug for CapError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MacaroonInvalid => f.write_str("MacaroonInvalid"),
            Self::CaveatViolation { .. } => f
                .debug_struct("CaveatViolation")
                .field("caveat_kind", &"<redacted>")
                .finish(),
            Self::Expired { expired_at_unix } => f
                .debug_struct("Expired")
                .field("expired_at_unix", expired_at_unix)
                .finish(),
        }
    }
}

/// Stub bearer decode (RFC-0969 §Phase 1). Real decode requires RFC-0903
/// bearer substrate + Ed25519 signature verification. This stub extracts a
/// deterministic placeholder `subject_did` + `ask_id` from the token string
/// bytes — sufficient to populate `BearerVerification` for `parse_auth_headers`
/// linkage evaluation. Future AC-B1 lands real signature verification.
pub fn unverified_decode_bearer(token: &str) -> BearerVerification {
    let mut ask_id = [0u8; 32];
    let bytes = token.as_bytes();
    let n = bytes.len().min(32);
    ask_id[..n].copy_from_slice(&bytes[..n]);
    let subject_did = format!("did:octo:{}", &token[..token.len().min(8)]);
    BearerVerification {
        subject_did,
        ask_id,
    }
}

/// Stub capability decode (RFC-0969 §Phase 1). Real decode requires RFC-0957
/// capability substrate + `HolderRegistry::lookup(cap_root_hash)`. Stub extracts
/// deterministic placeholder `holder_did` + `ask_id` from token bytes. The
/// `holder_did` follows the same `did:octo:<prefix>` format as `subject_did`
/// because real substrate derives both from the same canonical identity (the
/// holder's Ed25519 pubkey + multibase encoding per RFC-0009).
pub fn unverified_decode_capability(token: &str) -> CapabilityVerification {
    let mut ask_id = [0u8; 32];
    let bytes = token.as_bytes();
    let n = bytes.len().min(32);
    ask_id[..n].copy_from_slice(&bytes[..n]);
    let holder_did = format!("did:octo:{}", &token[..token.len().min(8)]);
    CapabilityVerification { holder_did, ask_id }
}

/// DispatchSet: parsed headers + identity linkage.
#[derive(Clone, Debug)]
pub struct DispatchSet {
    pub bearer: Option<AuthHeader>,
    pub capability: Option<AuthHeader>,
    pub identity_linkage: LinkageResult,
}

/// Parse `Authorization` + `X-Capability-Token` headers from a request map.
///
/// Identity linkage evaluation (RFC-0969 §Phase 1 + AC-A2):
/// - Both bearer + capability present → decode stubs run → compare
///   `subject_did == holder_did` AND `ask_id == ask_id`.
///   - Both match → `Linked { subject_did, ask_id }`.
///   - Subject match + ask mismatch → `AskBindingMismatch { bearer_ask, cap_ask }`
///     (mission 0969-a3 AC-B2.1.b).
///   - Subject mismatch → `Mismatched`.
/// - One present, other absent → `Indeterminate` (cannot evaluate linkage).
/// - Neither present → `ParseError::NoAuthHeader` (upstream of `AuthError::NoAuthHeader`
///   via `From<ParseError> for AuthError`).
/// - Authorization header with unrecognized scheme (e.g., `Basic <b64>`) →
///   `ParseError::UnsupportedScheme(scheme)` (mission 0969-a3 AC-B2.1.a;
///   upstream of `AuthError::UnsupportedScheme`).
pub fn parse_auth_headers(headers: &[(String, String)]) -> Result<DispatchSet, ParseError> {
    let mut bearer = None;
    let mut capability = None;
    let mut cap_count = 0;
    for (name, value) in headers {
        let lower = name.to_lowercase();
        if lower == "authorization" {
            if let Some(rest) = value.strip_prefix("Bearer ") {
                bearer = Some(AuthHeader::Bearer(rest.to_string()));
            } else if let Some(rest) = value.strip_prefix("CipherOcto-Cap ") {
                capability = Some(AuthHeader::CipherOctoCap(rest.to_string()));
                cap_count += 1;
            } else {
                // Unknown scheme — mission 0969-a3 AC-B2.1.a: surface the
                // scheme name so `authenticate()` can return
                // `AuthError::UnsupportedScheme(scheme)`. The previous
                // silent-discard policy (pre-AC-B2.1) dropped the scheme.
                let scheme = value.split_whitespace().next().unwrap_or("").to_owned();
                return Err(ParseError::UnsupportedScheme(scheme));
            }
        } else if lower == "x-capability-token" {
            capability = Some(AuthHeader::CipherOctoCap(value.clone()));
            cap_count += 1;
        }
    }
    if cap_count > 1 {
        return Err(ParseError::DuplicateCapabilityHeader);
    }
    if bearer.is_none() && capability.is_none() {
        return Err(ParseError::NoAuthHeader);
    }
    // Identity linkage evaluation (AC-A2 + AC-B2.1.b): dual-pipeline case only.
    // Stub decode functions extract placeholder `subject_did` / `holder_did`
    // from token bytes; real signature verification lands in AC-B1.
    let identity_linkage = match (&bearer, &capability) {
        (Some(AuthHeader::Bearer(b)), Some(AuthHeader::CipherOctoCap(c))) => {
            let bv = unverified_decode_bearer(b);
            let cv = unverified_decode_capability(c);
            if bv.subject_did == cv.holder_did && bv.ask_id == cv.ask_id {
                LinkageResult::Linked {
                    subject_did: bv.subject_did,
                    ask_id: bv.ask_id,
                }
            } else if bv.subject_did == cv.holder_did {
                // Subject matches but ask differs — mission 0969-a3 AC-B2.1.b.
                LinkageResult::AskBindingMismatch {
                    bearer_ask: bv.ask_id,
                    cap_ask: cv.ask_id,
                }
            } else {
                LinkageResult::Mismatched
            }
        }
        _ => LinkageResult::Indeterminate,
    };
    Ok(DispatchSet {
        bearer,
        capability,
        identity_linkage,
    })
}

/// Identity linkage evaluation (RFC-0969 §Phase 1 + AC-A2 + mission 0969-a3
/// AC-B2.1.b).
///
/// Pure function: takes optional `BearerVerification` + `CapabilityVerification`
/// and returns:
/// - `Linked { subject_did, ask_id }` if both present AND both
///   `subject_did == holder_did` AND `ask_id == ask_id`.
/// - `AskBindingMismatch { bearer_ask, cap_ask }` (mission 0969-a3 AC-B2.1.b)
///   if both present AND `subject_did == holder_did` BUT `ask_id != ask_id`.
/// - `Mismatched` if both present AND `subject_did != holder_did`.
/// - `Indeterminate` if one or both absent.
pub fn evaluate_linkage(
    bearer: Option<&BearerVerification>,
    capability: Option<&CapabilityVerification>,
) -> LinkageResult {
    match (bearer, capability) {
        (Some(b), Some(c)) => {
            if b.subject_did == c.holder_did && b.ask_id == c.ask_id {
                LinkageResult::Linked {
                    subject_did: b.subject_did.clone(),
                    ask_id: b.ask_id,
                }
            } else if b.subject_did == c.holder_did {
                // Mission 0969-a3 AC-B2.1.b: same subject, different ask.
                LinkageResult::AskBindingMismatch {
                    bearer_ask: b.ask_id,
                    cap_ask: c.ask_id,
                }
            } else {
                LinkageResult::Mismatched
            }
        }
        _ => LinkageResult::Indeterminate,
    }
}

/// AuthError (RFC-0969 §Phase 1).
#[derive(thiserror::Error, Clone, PartialEq)]
pub enum AuthError {
    #[error("identity mismatch: bearer_did=<redacted>, cap_did=<redacted>")]
    IdentityMismatch { bearer_did: String, cap_did: String },
    #[error("ask binding mismatch: bearer_ask=<redacted>, cap_ask=<redacted>")]
    AskBindingMismatch {
        bearer_ask: [u8; 32],
        cap_ask: [u8; 32],
    },
    #[error("both invalid: bearer_err=<redacted>, cap_err=<redacted>")]
    BothInvalid {
        bearer_err: Option<BearerError>,
        cap_err: Option<CapError>,
    },
    #[error("routing latency exceeded: {actual_ms}ms > {threshold_ms}ms")]
    RoutingLatencyExceeded { threshold_ms: u64, actual_ms: u64 },
    #[error("duplicate capability header")]
    DuplicateCapabilityHeader,
    #[error("no auth header")]
    NoAuthHeader,
    #[error("unsupported scheme: {0}")]
    UnsupportedScheme(String),
    #[error("indeterminate")]
    Indeterminate,
}

// Manual redacting Debug (RFC-0969 §Security): credential material (DIDs,
// ask IDs) must be redacted from `Debug` output. The `#[derive(Debug)]`
// would leak `bearer_did: String` + `cap_did: String` + `bearer_ask` + `cap_ask`
// field values; the manual impl below substitutes `<redacted>` for credential
// fields and preserves operational metadata (e.g. `RoutingLatencyExceeded`
// threshold + actual ms).
impl std::fmt::Debug for AuthError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::IdentityMismatch { .. } => f
                .debug_struct("IdentityMismatch")
                .field("bearer_did", &"<redacted>")
                .field("cap_did", &"<redacted>")
                .finish(),
            Self::AskBindingMismatch { .. } => f
                .debug_struct("AskBindingMismatch")
                .field("bearer_ask", &"<redacted>")
                .field("cap_ask", &"<redacted>")
                .finish(),
            Self::BothInvalid { .. } => f
                .debug_struct("BothInvalid")
                .field("bearer_err", &"<redacted>")
                .field("cap_err", &"<redacted>")
                .finish(),
            Self::RoutingLatencyExceeded {
                threshold_ms,
                actual_ms,
            } => f
                .debug_struct("RoutingLatencyExceeded")
                .field("threshold_ms", threshold_ms)
                .field("actual_ms", actual_ms)
                .finish(),
            Self::DuplicateCapabilityHeader => f.write_str("DuplicateCapabilityHeader"),
            Self::NoAuthHeader => f.write_str("NoAuthHeader"),
            Self::UnsupportedScheme(scheme) => {
                f.debug_tuple("UnsupportedScheme").field(&scheme).finish()
            }
            Self::Indeterminate => f.write_str("Indeterminate"),
        }
    }
}

/// Convert a `ParseError` to the equivalent `AuthError` variant so that
/// `authenticate()` (or any consumer of `parse_auth_headers`) can surface
/// the failure via the unified error type without leaking parse-stage
/// internals.
impl From<ParseError> for AuthError {
    fn from(err: ParseError) -> Self {
        match err {
            ParseError::DuplicateCapabilityHeader => AuthError::DuplicateCapabilityHeader,
            ParseError::NoAuthHeader => AuthError::NoAuthHeader,
            // Mission 0969-a3 AC-B2.1.a: route `UnsupportedScheme(scheme)`
            // through `AuthError::UnsupportedScheme(scheme)` so consumers
            // see a typed scheme-name (operational metadata) rather than a
            // silent NoAuthHeader fallback.
            ParseError::UnsupportedScheme(scheme) => AuthError::UnsupportedScheme(scheme),
        }
    }
}

/// Convert a single-path `BearerError` to `AuthError::BothInvalid`. Used when
/// only the bearer leg was attempted (e.g. capability-only request fails on
/// bearer verification before reaching capability).
impl From<BearerError> for AuthError {
    fn from(err: BearerError) -> Self {
        AuthError::BothInvalid {
            bearer_err: Some(err),
            cap_err: None,
        }
    }
}

/// Convert a single-path `CapError` to `AuthError::BothInvalid`. Used when
/// only the capability leg was attempted.
impl From<CapError> for AuthError {
    fn from(err: CapError) -> Self {
        AuthError::BothInvalid {
            bearer_err: None,
            cap_err: Some(err),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_bearer_only() {
        let set = parse_auth_headers(&[("Authorization".into(), "Bearer abc".into())]).unwrap();
        assert!(matches!(set.bearer, Some(AuthHeader::Bearer(_))));
        assert!(set.capability.is_none());
    }

    #[test]
    fn parse_capability_only() {
        let set =
            parse_auth_headers(&[("Authorization".into(), "CipherOcto-Cap xyz".into())]).unwrap();
        assert!(set.bearer.is_none());
        assert!(matches!(set.capability, Some(AuthHeader::CipherOctoCap(_))));
    }

    #[test]
    fn parse_x_capability_token_header() {
        let set = parse_auth_headers(&[("X-Capability-Token".into(), "xyz".into())]).unwrap();
        assert!(matches!(set.capability, Some(AuthHeader::CipherOctoCap(_))));
    }

    #[test]
    fn parse_duplicate_capability_header() {
        let r = parse_auth_headers(&[
            ("X-Capability-Token".into(), "xyz".into()),
            ("Authorization".into(), "CipherOcto-Cap abc".into()),
        ]);
        assert!(matches!(r, Err(ParseError::DuplicateCapabilityHeader)));
    }

    #[test]
    fn parse_no_auth_header() {
        let r = parse_auth_headers(&[("Content-Type".into(), "application/json".into())]);
        assert!(matches!(r, Err(ParseError::NoAuthHeader)));
    }

    #[test]
    fn parse_both_headers_present() {
        // Identical token bytes in both header paths → decode stubs produce
        // identical (subject_did, holder_did, ask_id) → linkage is Linked.
        // Pre-0969-a2 AC-A2 implementation: linkage was stubbed Indeterminate.
        let set = parse_auth_headers(&[
            ("Authorization".into(), "Bearer abc".into()),
            ("X-Capability-Token".into(), "abc".into()),
        ])
        .unwrap();
        assert!(set.bearer.is_some());
        assert!(set.capability.is_some());
        assert!(matches!(set.identity_linkage, LinkageResult::Linked { .. }));
    }

    #[test]
    fn auth_header_debug_redacts() {
        let h = AuthHeader::Bearer("secret-token".into());
        let s = format!("{h:?}");
        assert!(s.contains("redacted"), "expected redaction: {s}");
        assert!(!s.contains("secret-token"), "leaked token: {s}");
    }

    #[test]
    fn auth_error_debug_redacts_identity_mismatch() {
        let err = AuthError::IdentityMismatch {
            bearer_did: "did:octo:b1".into(),
            cap_did: "did:octo:c1".into(),
        };
        let s = format!("{err:?}");
        assert!(s.contains("IdentityMismatch"), "expected variant: {s}");
        assert!(s.contains("redacted"), "expected redaction: {s}");
        assert!(!s.contains("did:octo:b1"), "leaked bearer_did: {s}");
        assert!(!s.contains("did:octo:c1"), "leaked cap_did: {s}");
    }

    #[test]
    fn auth_error_debug_redacts_ask_binding_mismatch() {
        let err = AuthError::AskBindingMismatch {
            bearer_ask: [0xAA; 32],
            cap_ask: [0xBB; 32],
        };
        let s = format!("{err:?}");
        assert!(s.contains("AskBindingMismatch"), "expected variant: {s}");
        assert!(s.contains("redacted"), "expected redaction: {s}");
        // hex of all-AA / all-BB must not appear
        assert!(!s.contains("aaaaaaaa"), "leaked bearer_ask: {s}");
        assert!(!s.contains("bbbbbbbb"), "leaked cap_ask: {s}");
    }

    #[test]
    fn auth_error_debug_preserves_routing_latency_metadata() {
        let err = AuthError::RoutingLatencyExceeded {
            threshold_ms: 100,
            actual_ms: 250,
        };
        let s = format!("{err:?}");
        assert!(
            s.contains("RoutingLatencyExceeded"),
            "expected variant: {s}"
        );
        assert!(s.contains("100"), "threshold_ms preserved: {s}");
        assert!(s.contains("250"), "actual_ms preserved: {s}");
    }

    #[test]
    fn auth_error_debug_unit_variants_are_stable() {
        // Unit variants (no fields) render as their variant name — useful
        // for grep tests in CI (RFC-0969 §Security).
        for (err, expected) in [
            (
                AuthError::DuplicateCapabilityHeader,
                "DuplicateCapabilityHeader",
            ),
            (AuthError::NoAuthHeader, "NoAuthHeader"),
            (AuthError::Indeterminate, "Indeterminate"),
        ] {
            assert_eq!(format!("{err:?}"), expected);
        }
    }

    #[test]
    fn auth_error_debug_unsupported_scheme_shows_scheme() {
        // The scheme name itself is operational metadata (not credential
        // material) and is preserved for forensics.
        let err = AuthError::UnsupportedScheme("Basic".into());
        let s = format!("{err:?}");
        assert!(s.contains("UnsupportedScheme"), "expected variant: {s}");
        assert!(s.contains("Basic"), "scheme name preserved: {s}");
    }

    #[test]
    fn parse_error_converts_to_auth_error() {
        // DuplicateCapabilityHeader
        let p = ParseError::DuplicateCapabilityHeader;
        let a: AuthError = p.into();
        assert!(matches!(a, AuthError::DuplicateCapabilityHeader));

        // NoAuthHeader
        let p = ParseError::NoAuthHeader;
        let a: AuthError = p.into();
        assert!(matches!(a, AuthError::NoAuthHeader));
    }

    // --- AC-A1: BearerVerification + CapabilityVerification substrate ---

    #[test]
    fn bearer_verification_debug_redacts_subject_did() {
        let v = BearerVerification {
            subject_did: "did:octo:b1".into(),
            ask_id: [0xAA; 32],
        };
        let s = format!("{v:?}");
        assert!(s.contains("BearerVerification"));
        assert!(s.contains("redacted"));
        assert!(!s.contains("did:octo:b1"));
        assert!(!s.contains("aaaaaaaa"));
    }

    #[test]
    fn capability_verification_debug_redacts_holder_did() {
        let v = CapabilityVerification {
            holder_did: "did:octo:c1".into(),
            ask_id: [0xBB; 32],
        };
        let s = format!("{v:?}");
        assert!(s.contains("CapabilityVerification"));
        assert!(s.contains("redacted"));
        assert!(!s.contains("did:octo:c1"));
        assert!(!s.contains("bbbbbbbb"));
    }

    #[test]
    fn unverified_decode_bearer_is_deterministic() {
        let a = unverified_decode_bearer("token-1");
        let b = unverified_decode_bearer("token-1");
        assert_eq!(a, b);
        assert_eq!(a.subject_did, b.subject_did);
        assert_eq!(a.ask_id, b.ask_id);
    }

    #[test]
    fn unverified_decode_capability_is_deterministic() {
        let a = unverified_decode_capability("cap-1");
        let b = unverified_decode_capability("cap-1");
        assert_eq!(a, b);
        assert_eq!(a.holder_did, b.holder_did);
        assert_eq!(a.ask_id, b.ask_id);
    }

    // --- AC-A2: identity linkage evaluation logic ---

    #[test]
    fn linkage_matched_when_tokens_identical() {
        // Two identical token strings → decode stubs produce identical
        // (subject_did, holder_did, ask_id) → Linked.
        let set = parse_auth_headers(&[
            ("Authorization".into(), "Bearer abc123".into()),
            ("X-Capability-Token".into(), "abc123".into()),
        ])
        .unwrap();
        assert!(matches!(set.identity_linkage, LinkageResult::Linked { .. }));
    }

    #[test]
    fn linkage_mismatched_when_tokens_differ() {
        let set = parse_auth_headers(&[
            ("Authorization".into(), "Bearer abc123".into()),
            ("X-Capability-Token".into(), "xyz789".into()),
        ])
        .unwrap();
        assert_eq!(set.identity_linkage, LinkageResult::Mismatched);
    }

    #[test]
    fn linkage_indeterminate_when_only_one_present() {
        // Bearer only
        let set = parse_auth_headers(&[("Authorization".into(), "Bearer abc".into())]).unwrap();
        assert_eq!(set.identity_linkage, LinkageResult::Indeterminate);
        // Capability only
        let set =
            parse_auth_headers(&[("Authorization".into(), "CipherOcto-Cap abc".into())]).unwrap();
        assert_eq!(set.identity_linkage, LinkageResult::Indeterminate);
    }

    // --- 0969-a3 AC-B2.1.b: AskBindingMismatch distinguished from Mismatched ---

    #[test]
    fn evaluate_linkage_ask_binding_mismatch_when_subject_match_ask_differ() {
        // Construct pre-decoded BearerVerification + CapabilityVerification
        // directly: same `subject_did` / `holder_did`, different `ask_id`.
        // The stub decoders derive both `subject_did` AND `ask_id` from
        // token bytes (so they cannot decouple subject from ask); the
        // real substrate (AC-B1) decouples them via Ed25519 pubkey +
        // canonical ask. Direct `evaluate_linkage` exercise proves the
        // 4-arm decision logic.
        let b = BearerVerification {
            subject_did: "did:octo:holder-1".into(),
            ask_id: [0xAA; 32],
        };
        let c = CapabilityVerification {
            holder_did: "did:octo:holder-1".into(),
            ask_id: [0xBB; 32],
        };
        let r = evaluate_linkage(Some(&b), Some(&c));
        assert!(
            matches!(r, LinkageResult::AskBindingMismatch { .. }),
            "expected AskBindingMismatch, got {r:?}"
        );
        if let LinkageResult::AskBindingMismatch {
            bearer_ask,
            cap_ask,
        } = r
        {
            assert_eq!(bearer_ask, [0xAA; 32]);
            assert_eq!(cap_ask, [0xBB; 32]);
            assert_ne!(bearer_ask, cap_ask);
        }
    }

    #[test]
    fn evaluate_linkage_mismatched_when_subject_differ() {
        // Different subject DID AND different ask ID → full Mismatched
        // (not AskBindingMismatch).
        let b = BearerVerification {
            subject_did: "did:octo:bearer".into(),
            ask_id: [0xAA; 32],
        };
        let c = CapabilityVerification {
            holder_did: "did:octo:cap".into(),
            ask_id: [0xBB; 32],
        };
        assert_eq!(
            evaluate_linkage(Some(&b), Some(&c)),
            LinkageResult::Mismatched
        );
    }

    #[test]
    fn parse_error_unsupported_scheme_carries_scheme_name() {
        // Mission 0969-a3 AC-B2.1.a: Authorization header with
        // unrecognized scheme returns `ParseError::UnsupportedScheme`.
        let r = parse_auth_headers(&[("Authorization".into(), "Basic dXNlcjpwYXNz".into())]);
        match r {
            Err(ParseError::UnsupportedScheme(scheme)) => assert_eq!(scheme, "Basic"),
            other => panic!("expected UnsupportedScheme(\"Basic\"), got {other:?}"),
        }
    }

    #[test]
    fn parse_error_unsupported_scheme_converts_to_auth_error() {
        // AC-B2.1.a: `From<ParseError> for AuthError` routes UnsupportedScheme.
        let p = ParseError::UnsupportedScheme("Digest".into());
        let a: AuthError = p.into();
        assert!(matches!(a, AuthError::UnsupportedScheme(s) if s == "Digest"));
    }

    // --- AC-A5: BothInvalid carries BearerError / CapError ---

    #[test]
    fn bearer_error_converts_to_auth_error_both_invalid() {
        let b = BearerError::Malformed;
        let a: AuthError = b.into();
        match a {
            AuthError::BothInvalid {
                bearer_err: Some(BearerError::Malformed),
                cap_err: None,
            } => {}
            other => {
                panic!("expected BothInvalid{{bearer: Some(Malformed), cap: None}}, got {other:?}")
            }
        }
    }

    #[test]
    fn cap_error_converts_to_auth_error_both_invalid() {
        let c = CapError::MacaroonInvalid;
        let a: AuthError = c.into();
        match a {
            AuthError::BothInvalid {
                bearer_err: None,
                cap_err: Some(CapError::MacaroonInvalid),
            } => {}
            other => panic!(
                "expected BothInvalid{{bearer: None, cap: Some(MacaroonInvalid)}}, got {other:?}"
            ),
        }
    }

    #[test]
    fn both_invalid_constructed_with_both_errs_redacts() {
        // AuthError::BothInvalid Debug fully redacts the inner bearer_err +
        // cap_err (both treated as credential material at the AuthError level).
        // Operational metadata lives on the inner variants themselves
        // (BearerError::Expired.expired_at_unix, CapError::Expired.expired_at_unix);
        // each has its own Debug impl that preserves that metadata.
        let err = AuthError::BothInvalid {
            bearer_err: Some(BearerError::Expired {
                expired_at_unix: 1_700_000_000,
            }),
            cap_err: Some(CapError::CaveatViolation {
                caveat_kind: "ip-allowlist".into(),
            }),
        };
        let s = format!("{err:?}");
        assert!(s.contains("BothInvalid"));
        assert!(s.contains("redacted"));
        // Caveat kind is credential-adjacent; fully redacted at AuthError level.
        assert!(!s.contains("ip-allowlist"));
    }

    #[test]
    fn bearer_error_debug_preserves_expired_metadata() {
        let err = BearerError::Expired {
            expired_at_unix: 1_700_000_000,
        };
        let s = format!("{err:?}");
        assert!(s.contains("Expired"));
        assert!(s.contains("1700000000"));
    }

    #[test]
    fn cap_error_debug_redacts_caveat_kind() {
        let err = CapError::CaveatViolation {
            caveat_kind: "ip-allowlist".into(),
        };
        let s = format!("{err:?}");
        assert!(s.contains("CaveatViolation"));
        assert!(s.contains("redacted"));
        assert!(!s.contains("ip-allowlist"));
    }
}
