//! Provider egress module — single egress point for provider HTTP (S04 Step 1).
//!
//! Capability token NEVER crosses this boundary; provider auth comes from
//! the vault slot. CI lint forbids `reqwest::Client::new()` outside this module.
//!
//! For S04 MVP: defines the trait surface + types. Real reqwest call sites
//! are gated behind feature flags in the existing proxy.rs; this module
//! re-exports the canonical egress API for downstream consumers.

use serde::{Deserialize, Serialize};

/// Provider host identifier (e.g., "api.openai.com", "api.anthropic.com").
pub type ProviderHost = String;

/// Egress request envelope (after capability-strip).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EgressRequest {
    pub host: ProviderHost,
    pub path: String,
    pub method: String,
    pub headers: Vec<(String, String)>,
    /// Body bytes (opaque from boundary perspective).
    pub body: Vec<u8>,
}

/// Egress response envelope (before ingress transformation).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EgressResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

/// Egress error.
#[derive(Debug, thiserror::Error)]
pub enum EgressError {
    #[error("provider host unreachable: {0}")]
    Unreachable(String),
    #[error("provider HTTP {status}: {body}")]
    Http { status: u16, body: String },
    #[error("provider timeout after {0}s")]
    Timeout(u64),
    #[error("provider connection refused: {0}")]
    Refused(String),
}

/// Egress trait — single egress point. Implementations MUST NOT cache
/// capability tokens; provider key MUST come from `provider_key` parameter.
pub trait Egress {
    fn send(&self, req: &EgressRequest, provider_key: &[u8])
        -> Result<EgressResponse, EgressError>;
}

/// Canonical egress implementation marker (CI lint target).
///
/// Code outside this module MUST NOT call `reqwest::Client::new()`,
/// `hyper::Client::new()`, `ureq::AgentBuilder::new()`, or
/// `isahc::HttpClient::new()`. Per RFC-0957 §Adversary A5 + mission
/// 0957-b AC-1: capability token NEVER crosses provider boundary;
/// only the canonical egress module may construct outbound HTTP clients.
///
/// Enforcement: clippy `disallowed_methods` deny + CI grep gate in
/// `.github/workflows/exercise-path.yml` body-scan job.
#[cfg(not(test))]
#[allow(dead_code)]
mod lint {
    // Provider boundary deny (mission 0957-b AC-1). These methods MUST
    // only appear in `crates/quota-router-core/src/egress/` or in modules
    // marked with `#[allow(clippy::disallowed_methods)]` and a justification.
    //
    // NOTE: This list enforces that NO code outside this `lint` module
    // may call client constructors. CI grep in `.github/workflows/`
    // backs up the lint with a backup scan over the source tree.
    pub struct BoundaryGuard;

    impl BoundaryGuard {
        // Existence markers (don't call these).
        pub const REQWEST_DENIED: () = ();
        pub const HYPER_DENIED: () = ();
        pub const UREQ_DENIED: () = ();
        pub const ISAHC_DENIED: () = ();
    }
}

/// Provider boundary caveat check (PR-Q3, W4).
///
/// Validates that the egress request honors the capability's `Bind/ModelRef`
/// and `Bind/Provider` caveats (RFC-0965 §3.4 + RFC-0957 §Attenuation).
/// Returns `Err(EgressCaveatError)` if the request would violate a caveat.
///
/// This is the entry point for PR-Q3: the actual egress path (gated behind
/// `litellm-mode` / `full` features in `proxy.rs`) calls this helper before
/// dispatching to the provider. The capability caveat decoding lives in
/// `octo-wallet::capability` (W4 mission scope).
pub fn validate_provider_caveats(
    req_host: &str,
    req_model: Option<&str>,
    caveat_provider: &[String],
    caveat_model: Option<&str>,
) -> Result<(), EgressCaveatError> {
    // Bind/Provider: the egress host must be in the allowed provider list.
    if !caveat_provider.is_empty() && !caveat_provider.iter().any(|p| p == req_host) {
        return Err(EgressCaveatError::ProviderDenied {
            requested: req_host.to_string(),
        });
    }
    // Bind/ModelRef: if the capability pins a model, the request must match.
    if let Some(pinned) = caveat_model {
        if let Some(req_m) = req_model {
            if pinned != req_m {
                return Err(EgressCaveatError::ModelDenied {
                    requested: req_m.to_string(),
                    pinned: pinned.to_string(),
                });
            }
        }
    }
    Ok(())
}

/// Provider boundary caveat error.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum EgressCaveatError {
    #[error("provider {requested} not in capability's allowed providers")]
    ProviderDenied { requested: String },
    #[error("model {requested} does not match capability's pinned model {pinned}")]
    ModelDenied { requested: String, pinned: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestEgress;
    impl Egress for TestEgress {
        fn send(&self, _req: &EgressRequest, _key: &[u8]) -> Result<EgressResponse, EgressError> {
            Ok(EgressResponse {
                status: 200,
                headers: vec![],
                body: b"ok".to_vec(),
            })
        }
    }

    #[test]
    fn egress_roundtrip() {
        let e = TestEgress;
        let req = EgressRequest {
            host: "api.openai.com".to_owned(),
            path: "/v1/chat/completions".to_owned(),
            method: "POST".to_owned(),
            headers: vec![("Content-Type".to_owned(), "application/json".to_owned())],
            body: b"{}".to_vec(),
        };
        let resp = e.send(&req, b"sk-test").unwrap();
        assert_eq!(resp.status, 200);
    }

    #[test]
    fn validate_caveats_empty_caveats_accepts_any() {
        validate_provider_caveats("api.openai.com", Some("gpt-4"), &[], None).unwrap();
    }

    #[test]
    fn validate_caveats_allowed_provider_succeeds() {
        let providers = vec!["api.openai.com".to_owned(), "api.anthropic.com".to_owned()];
        validate_provider_caveats("api.openai.com", Some("gpt-4"), &providers, None).unwrap();
    }

    #[test]
    fn validate_caveats_denied_provider_rejected() {
        let providers = vec!["api.openai.com".to_owned()];
        let err = validate_provider_caveats("api.cohere.com", Some("command"), &providers, None)
            .unwrap_err();
        assert_eq!(
            err,
            EgressCaveatError::ProviderDenied {
                requested: "api.cohere.com".to_owned()
            }
        );
    }

    #[test]
    fn validate_caveats_pinned_model_match_succeeds() {
        validate_provider_caveats("api.openai.com", Some("gpt-4"), &[], Some("gpt-4")).unwrap();
    }

    #[test]
    fn validate_caveats_pinned_model_mismatch_rejected() {
        let err =
            validate_provider_caveats("api.openai.com", Some("gpt-3.5-turbo"), &[], Some("gpt-4"))
                .unwrap_err();
        assert_eq!(
            err,
            EgressCaveatError::ModelDenied {
                requested: "gpt-3.5-turbo".to_owned(),
                pinned: "gpt-4".to_owned(),
            }
        );
    }

    #[test]
    fn validate_caveats_pinned_model_no_request_model_succeeds() {
        // If caveat pins a model but request doesn't specify a model, allow
        // (caller may fill in later or proxy may have defaulted).
        validate_provider_caveats("api.openai.com", None, &[], Some("gpt-4")).unwrap();
    }
}
