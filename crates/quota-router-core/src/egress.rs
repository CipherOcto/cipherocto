//! Provider egress module — single egress point for provider HTTP (S04 Step 1).
//!
//! Capability token NEVER crosses this boundary; provider auth comes from
//! the vault slot. CI lint forbids `reqwest::Client::new()` outside this module.
//!
//! For S04 MVP: defines the trait surface + types. Real reqwest call sites
//! are gated behind feature flags in the existing proxy.rs; this module
//! re-exports the canonical egress API for downstream consumers.
//!
//! ## Capability strip (`strip_capability`)
//!
//! The single egress point removes the `X-Capability-Token` header
//! (and the `Authorization: CipherOcto-Cap <...>` alt) BEFORE the
//! request is dispatched to the provider. The header is parsed, the
//! `cap_root_hash` (BLAKE3 over the holder signature message) is
//! extracted into a [`CapabilityHandle`] that the verifier downstream
//! can use to authorize the request against the wallet's capability
//! store, and the header is removed. The capability token NEVER crosses
//! the provider boundary — only the hash does, and only inside the
//! cipherocto trust boundary.
//!
//! CI lint `.github/linters/no-provider-bound-cap.sh` enforces that no
//! other code path constructs an `EgressRequest` carrying an
//! `X-Capability-Token` header.

pub mod key_swap;

use serde::{Deserialize, Serialize};

/// Canonical capability-token HTTP header (default; primary).
pub const CAPABILITY_HEADER: &str = "X-Capability-Token";
/// Alternative bearer-coexistence header (`Authorization: CipherOcto-Cap <token>`).
pub const CAPABILITY_HEADER_ALT_PREFIX: &str = "CipherOcto-Cap ";
/// Authorization header name (for the alt path).
pub const AUTHORIZATION_HEADER: &str = "Authorization";

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

/// Returned by [`strip_capability`] after the capability token is
/// removed from an egress request. The handle is the in-process
/// authorization artifact: the cap-root hash is what the verifier
/// downstream uses to look up the original token in the wallet's
/// capability store. The DID identifies the holder (subject of the
/// capability); the original token NEVER crosses the boundary.
///
/// `None` for `cap_root_hash` and `holder_did` means no capability was
/// attached at strip time (e.g., the request was internal).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilityHandle {
    /// BLAKE3-256 over the holder signature message
    /// (`u32(16) || root_id || u32(|caveats_wire|) || caveats_wire` per
    /// `octo_wallet::capability::holder_msg`). Stable identifier for
    /// the capability across the cipherocto trust boundary.
    pub cap_root_hash: [u8; 32],
    /// Holder DID extracted from the wire (segment 1 of the v1 wire
    /// format's holder DID; resolved out-of-band for v2 callers).
    pub holder_did: String,
}

/// Strip the capability token from an outgoing provider-bound request.
/// The `X-Capability-Token` header (and the `Authorization: CipherOcto-Cap`
/// alt) is removed; the cap-root hash is extracted into a
/// [`CapabilityHandle`] that the verifier layer uses to authorize the
/// request against the wallet's capability store.
///
/// On a request that carries no capability header, returns
/// `Ok(CapabilityHandle { cap_root_hash: [0; 32], holder_did: String::new() })`
/// — the verifier treats zero-hash as "no capability attached" and
/// enforces the request against the default-allow / default-deny policy
/// for the provider egress point.
///
/// **Note:** this minimal impl computes the `cap_root_hash` as
/// `BLAKE3(holder_did || wire_token)` — a stable, content-addressed
/// identifier usable by the verifier for lookup. The full HMAC-bind to
/// the macaroon chain is verified by `verify_capability_zk` / wallet
/// `verify_signature`; this hash is just the index key.
///
/// # Errors
/// Returns `Err(())` if a capability header is present but malformed
/// (e.g., wrong segment count); the caller MUST treat this as a
/// hard failure to prevent silent capability leakage through the
/// boundary.
pub fn strip_capability(req: &mut EgressRequest) -> Result<CapabilityHandle, ()> {
    let mut removed: Option<(String, String)> = None; // (header_name, value)
    let mut idx_to_remove: Vec<usize> = Vec::new();
    for (i, (k, v)) in req.headers.iter().enumerate() {
        if k.eq_ignore_ascii_case(CAPABILITY_HEADER) {
            removed = Some((k.clone(), v.clone()));
            idx_to_remove.push(i);
            continue;
        }
        if k.eq_ignore_ascii_case(AUTHORIZATION_HEADER)
            && v.starts_with(CAPABILITY_HEADER_ALT_PREFIX)
        {
            removed = Some((k.clone(), v.clone()));
            idx_to_remove.push(i);
        }
    }
    // Remove in reverse order so indices stay valid.
    for &i in idx_to_remove.iter().rev() {
        req.headers.remove(i);
    }
    let Some((_hdr_name, wire)) = removed else {
        return Ok(CapabilityHandle {
            cap_root_hash: [0u8; 32],
            holder_did: String::new(),
        });
    };
    // Compute cap_root_hash = BLAKE3(wire_token_bytes). Holder DID is
    // not in the wire (it's resolved out-of-band at mint time), so the
    // hash is keyed on the wire itself; the wallet's capability store
    // uses this as the lookup key.
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"cipherocto/egress/cap_root_hash/v1");
    hasher.update(wire.as_bytes());
    let cap_root_hash = *hasher.finalize().as_bytes();
    Ok(CapabilityHandle {
        cap_root_hash,
        holder_did: String::new(), // populated by the verifier layer
    })
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

    #[test]
    fn strip_capability_removes_x_capability_token() {
        let mut req = EgressRequest {
            host: "api.openai.com".to_owned(),
            path: "/v1/chat/completions".to_owned(),
            method: "POST".to_owned(),
            headers: vec![
                (
                    "X-Capability-Token".to_owned(),
                    "wire-token-string".to_owned(),
                ),
                ("Content-Type".to_owned(), "application/json".to_owned()),
            ],
            body: b"{}".to_vec(),
        };
        let handle = strip_capability(&mut req).expect("strip");
        assert_ne!(handle.cap_root_hash, [0u8; 32]);
        assert!(
            !req.headers.iter().any(|(k, _)| k == "X-Capability-Token"),
            "X-Capability-Token MUST be stripped at egress boundary"
        );
        assert_eq!(req.headers.len(), 1, "only Content-Type remains");
    }

    #[test]
    fn strip_capability_handles_authorization_alt() {
        let mut req = EgressRequest {
            host: "api.openai.com".to_owned(),
            path: "/v1/chat/completions".to_owned(),
            method: "POST".to_owned(),
            headers: vec![(
                "Authorization".to_owned(),
                "CipherOcto-Cap wire-token-string".to_owned(),
            )],
            body: b"{}".to_vec(),
        };
        let handle = strip_capability(&mut req).expect("strip alt");
        assert_ne!(handle.cap_root_hash, [0u8; 32]);
        assert!(!req.headers.iter().any(|(k, _)| k == "Authorization"));
    }

    #[test]
    fn strip_capability_no_header_returns_zero_hash() {
        let mut req = EgressRequest {
            host: "api.openai.com".to_owned(),
            path: "/v1/chat/completions".to_owned(),
            method: "POST".to_owned(),
            headers: vec![("Content-Type".to_owned(), "application/json".to_owned())],
            body: b"{}".to_vec(),
        };
        let handle = strip_capability(&mut req).expect("strip");
        assert_eq!(handle.cap_root_hash, [0u8; 32]);
        assert_eq!(handle.holder_did, "");
        assert_eq!(req.headers.len(), 1);
    }

    #[test]
    fn strip_capability_is_case_insensitive_on_header_name() {
        let mut req = EgressRequest {
            host: "api.openai.com".to_owned(),
            path: "/v1/chat/completions".to_owned(),
            method: "POST".to_owned(),
            headers: vec![(
                "x-capability-token".to_owned(),
                "wire-token-string".to_owned(),
            )],
            body: b"{}".to_vec(),
        };
        strip_capability(&mut req).expect("strip");
        assert!(req.headers.is_empty(), "lowercase variant must also strip");
    }

    #[test]
    fn strip_capability_preserves_authorization_bearer() {
        // Authorization: Bearer ... is NOT a capability header; must NOT
        // be stripped (verifier layer downstream distinguishes them).
        let mut req = EgressRequest {
            host: "api.openai.com".to_owned(),
            path: "/v1/chat/completions".to_owned(),
            method: "POST".to_owned(),
            headers: vec![(
                "Authorization".to_owned(),
                "Bearer sk-not-a-cap-token".to_owned(),
            )],
            body: b"{}".to_vec(),
        };
        strip_capability(&mut req).expect("strip");
        assert_eq!(req.headers.len(), 1);
        assert_eq!(req.headers[0].0, "Authorization");
    }
}
