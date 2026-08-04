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
}

/// LinkageResult (RFC-0969 §Phase 1).
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum LinkageResult {
    Linked {
        subject_did: String,
        ask_id: [u8; 32],
    },
    Mismatched,
    Indeterminate,
}

/// DispatchSet: parsed headers + identity linkage.
#[derive(Clone, Debug)]
pub struct DispatchSet {
    pub bearer: Option<AuthHeader>,
    pub capability: Option<AuthHeader>,
    pub identity_linkage: LinkageResult,
}

/// Parse `Authorization` + `X-Capability-Token` headers from a request map.
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
                // Unknown scheme — flagged in `Unsupported` but not an error.
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
    let identity_linkage = if bearer.is_some() && capability.is_some() {
        LinkageResult::Indeterminate
    } else {
        LinkageResult::Indeterminate
    };
    Ok(DispatchSet {
        bearer,
        capability,
        identity_linkage,
    })
}

/// AuthError (RFC-0969 §Phase 1).
#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("identity mismatch: bearer_did=<redacted>, cap_did=<redacted>")]
    IdentityMismatch { bearer_did: String, cap_did: String },
    #[error("ask binding mismatch: bearer_ask=<redacted>, cap_ask=<redacted>")]
    AskBindingMismatch {
        bearer_ask: [u8; 32],
        cap_ask: [u8; 32],
    },
    #[error("both invalid")]
    BothInvalid,
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
        let set = parse_auth_headers(&[
            ("Authorization".into(), "Bearer abc".into()),
            ("X-Capability-Token".into(), "xyz".into()),
        ])
        .unwrap();
        assert!(set.bearer.is_some());
        assert!(set.capability.is_some());
        assert_eq!(set.identity_linkage, LinkageResult::Indeterminate);
    }

    #[test]
    fn auth_header_debug_redacts() {
        let h = AuthHeader::Bearer("secret-token".into());
        let s = format!("{:?}", h);
        assert!(s.contains("redacted"), "expected redaction: {s}");
        assert!(!s.contains("secret-token"), "leaked token: {s}");
    }
}
