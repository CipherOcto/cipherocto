//! Provider ingress module — single ingress point for provider responses (S04 Step 2).
//!
//! Per RFC-0957 + RFC-0959 v1.0 §Algorithms: ingress module converts
//! provider HTTP responses into the internal canonical representation.
//! Provider bodies are opaque at this boundary; only structured metadata
//! (status, model_id, usage, cache_hit) is extracted.
//!
//! Per mission 0957-b R1 carryover (C-4 + M-4): the canonical `Ingress`
//! trait provides a real body-parser via `OpenAiIngress` (default
//! JSON shape used by OpenAI-compatible providers). Per-provider custom
//! ingress impls can be added without changing the canonical surface.
//!
//! Mission 0969-a (RFC-0969): `authenticator` submodule provides the
//! dual-pipeline `GatewayAuthenticator` orchestrator (relocated from
//! `octo-wallet::capability::gateway_authenticator`). The provider-response
//! ingress below remains the canonical egress-side ingestion point.

pub mod authenticator;

use serde::{Deserialize, Serialize};

/// Provider usage metrics (per-axis consumption at ingress time).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cached_input_tokens: u64,
}

/// Provider response metadata (post-ingress).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
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
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum IngressError {
    #[error("malformed provider response: {0}")]
    Malformed(String),
    #[error("missing required field: {0}")]
    MissingField(String),
    #[error("provider error status {0}: {1}")]
    ProviderError(u16, String),
}

/// Ingress trait — converts raw egress response into structured metadata.
pub trait Ingress {
    fn parse(&self, raw_body: &[u8], raw_status: u16) -> Result<IngressMetadata, IngressError>;
}

/// OpenAI-compatible ingress implementation (default JSON shape).
///
/// Parses provider responses with this shape:
///
/// ```json
/// {
///   "id": "chatcmpl-…",
///   "model": "gpt-4",
///   "usage": {
///     "prompt_tokens": 100,
///     "completion_tokens": 50,
///     "cached_prompt_tokens": 0
///   }
/// }
/// ```
///
/// Used by all OpenAI-compatible providers (Anthropic via OpenAI
/// adapter, Google via OpenAI adapter, etc.). Non-OpenAI-compatible
/// providers may supply a custom `Ingress` impl without changing the
/// canonical surface.
#[derive(Debug, Clone, Copy, Default)]
pub struct OpenAiIngress;

#[derive(Default, Deserialize)]
struct OpenAiUsage {
    #[serde(default)]
    prompt_tokens: u64,
    #[serde(default)]
    completion_tokens: u64,
    #[serde(default)]
    cached_prompt_tokens: u64,
}

#[derive(Deserialize)]
struct OpenAiResponse {
    #[serde(default)]
    model: String,
    #[serde(default)]
    usage: OpenAiUsage,
    #[serde(default)]
    cached: bool,
}

impl Ingress for OpenAiIngress {
    fn parse(&self, raw_body: &[u8], raw_status: u16) -> Result<IngressMetadata, IngressError> {
        // Provider-error fast-path: 4xx / 5xx with non-empty body is an
        // upstream error, NOT a parseable response. Surface as a typed
        // error so the orchestrator can attach discharge / cache-poison
        // classification without re-parsing.
        if raw_status >= 400 {
            let body_str = String::from_utf8_lossy(raw_body).to_string();
            return Err(IngressError::ProviderError(raw_status, body_str));
        }
        let parsed: OpenAiResponse = serde_json::from_slice(raw_body).map_err(|e| {
            IngressError::Malformed(format!("provider response JSON parse failed: {e}"))
        })?;
        let usage = ProviderUsage {
            input_tokens: parsed.usage.prompt_tokens,
            output_tokens: parsed.usage.completion_tokens,
            cached_input_tokens: parsed.usage.cached_prompt_tokens,
        };
        Ok(IngressMetadata {
            model_id: parsed.model,
            provider: "openai-compatible".to_owned(),
            usage,
            cache_hit: parsed.cached,
            cache_key_hash: None,
            timestamp_unix: 0,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn openai_ingress_parses_happy_path() {
        let body = br#"{"id":"chatcmpl-1","model":"gpt-4","usage":{"prompt_tokens":100,"completion_tokens":50}}"#;
        let m = OpenAiIngress.parse(body, 200).unwrap();
        assert_eq!(m.model_id, "gpt-4");
        assert_eq!(m.usage.input_tokens, 100);
        assert_eq!(m.usage.output_tokens, 50);
        assert_eq!(m.usage.cached_input_tokens, 0);
        assert!(!m.cache_hit);
    }

    #[test]
    fn openai_ingress_parses_with_cached_tokens() {
        let body = br#"{"model":"gpt-4","usage":{"prompt_tokens":200,"completion_tokens":80,"cached_prompt_tokens":120}}"#;
        let m = OpenAiIngress.parse(body, 200).unwrap();
        assert_eq!(m.usage.input_tokens, 200);
        assert_eq!(m.usage.output_tokens, 80);
        assert_eq!(m.usage.cached_input_tokens, 120);
    }

    #[test]
    fn openai_ingress_returns_provider_error_on_500() {
        let body = br#"{"error":{"type":"server_error","message":"oops"}}"#;
        let err = OpenAiIngress.parse(body, 500).unwrap_err();
        assert!(matches!(err, IngressError::ProviderError(500, _)));
    }

    #[test]
    fn openai_ingress_returns_malformed_on_garbage() {
        let err = OpenAiIngress.parse(b"\x00\x01not-json", 200).unwrap_err();
        assert!(matches!(err, IngressError::Malformed(_)));
    }

    #[test]
    fn openai_ingress_preserves_negative_axes_as_zero() {
        // Bad data: missing usage fields → defaults to zero. Tests the
        // #[serde(default)] annotations on OpenAiUsage fields.
        let body = br#"{"model":"gpt-4"}"#;
        let m = OpenAiIngress.parse(body, 200).unwrap();
        assert_eq!(m.usage.input_tokens, 0);
        assert_eq!(m.usage.output_tokens, 0);
        assert_eq!(m.usage.cached_input_tokens, 0);
    }
}
