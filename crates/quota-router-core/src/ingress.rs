//! Provider ingress module — single ingress point for provider responses (S04 Step 2).
//!
//! Per S04 plan: ingress module converts provider HTTP responses into the
//! internal canonical representation. Provider bodies are opaque at this
//! boundary; only structured metadata (status, model_id, usage) is extracted.

use serde::{Deserialize, Serialize};

/// Provider usage metrics (per-axis consumption at ingress time).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cached_input_tokens: u64,
}

/// Provider response metadata (post-ingress).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngressMetadata {
    pub model_id: String,
    pub provider: String,
    pub usage: ProviderUsage,
    pub cache_hit: bool,
    /// Cache key hash if response is cacheable.
    pub cache_key_hash: Option<[u8; 32]>,
    /// Unix timestamp at provider response time.
    pub timestamp_unix: u64,
}

/// Ingress error.
#[derive(Debug, thiserror::Error)]
pub enum IngressError {
    #[error("malformed provider response: {0}")]
    Malformed(String),
    #[error("missing required field: {0}")]
    MissingField(String),
}

/// Ingress trait — converts raw egress response into structured metadata.
pub trait Ingress {
    fn parse(&self, raw_body: &[u8], raw_status: u16) -> Result<IngressMetadata, IngressError>;
}

/// Stub ingress for tests: returns empty metadata.
#[allow(dead_code)]
struct StubIngress;
impl Ingress for StubIngress {
    fn parse(&self, _body: &[u8], _status: u16) -> Result<IngressMetadata, IngressError> {
        Ok(IngressMetadata {
            model_id: "unknown".to_owned(),
            provider: "unknown".to_owned(),
            usage: ProviderUsage::default(),
            cache_hit: false,
            cache_key_hash: None,
            timestamp_unix: 0,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stub_ingress_roundtrip() {
        let i = StubIngress;
        let m = i.parse(b"{}", 200).unwrap();
        assert_eq!(m.model_id, "unknown");
        assert_eq!(m.usage, ProviderUsage::default());
    }
}
