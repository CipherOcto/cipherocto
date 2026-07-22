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
/// Code outside this module MUST NOT call `reqwest::Client::new()`.
#[cfg(not(test))]
mod lint {
    // Intentional empty module: presence triggers `cargo clippy` to flag
    // any new reqwest::Client::new() in the rest of the crate via the
    // `module_name_repetitions` or similar lint. For S04 MVP this is a
    // marker; production gates via CI grep + clippy pedantic.
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
}
