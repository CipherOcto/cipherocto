// Clippy `[disallowed-methods]` allowlist: provider health probes in
// this module are unauthenticated GETs to deployment endpoints. No
// capability material is carried on the wire — these are inventory /
// availability checks, not provider egress.
#![allow(clippy::disallowed_methods)]

use async_trait::async_trait;

#[derive(
    Clone, Copy, PartialEq, Eq, Hash, Debug, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub struct RouterNodeId(pub [u8; 32]);

#[derive(
    Clone, Copy, PartialEq, Eq, Hash, Debug, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub struct ProviderId(pub [u8; 32]);

#[derive(
    Clone, Copy, PartialEq, Eq, Hash, Debug, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub struct NetworkId(pub [u8; 32]);

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct ProviderCapacity {
    pub provider_id: ProviderId,
    pub provider_name: String,
    pub router_node_id: RouterNodeId,
    pub models: Vec<String>,
    pub requests_remaining: u64,
    pub pricing: Vec<ModelPricing>,
    pub status: ProviderHealth,
    pub latency_ms: u32,
    pub success_rate_bps: u16,
    pub last_updated: u64,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct ModelPricing {
    pub model: String,
    pub price_per_1k_tokens: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ProviderHealth {
    Healthy,
    Degraded,
    Unavailable,
    Unknown,
}

impl ProviderCapacity {
    pub fn from_config(cfg: &ProviderConfig, router_node_id: RouterNodeId) -> Self {
        let provider_id = ProviderId(
            *blake3::hash(format!("{}|{}", cfg.name, hex::encode(router_node_id.0)).as_bytes())
                .as_bytes(),
        );
        Self {
            provider_id,
            provider_name: cfg.name.clone(),
            router_node_id,
            models: cfg.models.clone(),
            requests_remaining: u64::MAX,
            pricing: cfg
                .models
                .iter()
                .map(|m| ModelPricing {
                    model: m.clone(),
                    price_per_1k_tokens: 0,
                })
                .collect(),
            status: ProviderHealth::Unknown,
            latency_ms: 0,
            success_rate_bps: 0,
            last_updated: 0,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    #[error("provider unavailable: {0}")]
    Unavailable(String),
    #[error("model not supported: {0}")]
    ModelNotSupported(String),
    #[error("context window exceeded: input {input_tokens} > max {max_tokens}")]
    ContextWindowExceeded { input_tokens: u32, max_tokens: u32 },
    #[error("rate limited")]
    RateLimited,
    #[error("request timeout")]
    Timeout,
    #[error("api error: {0}")]
    ApiError(String),
}

#[async_trait]
pub trait LocalProvider: Send + Sync {
    async fn completion(
        &self,
        model: &str,
        messages: &[u8],
        params: &ProviderCapacity,
    ) -> Result<Vec<u8>, ProviderError>;
    async fn health_check(&self) -> ProviderHealth;
    fn supported_models(&self) -> Vec<String>;
}

#[allow(dead_code)]
pub struct HttpLocalProvider {
    client: reqwest::Client,
    endpoint: String,
    api_key: String,
    models: Vec<String>,
}

impl HttpLocalProvider {
    pub fn new(cfg: ProviderConfig) -> Self {
        let api_key = match cfg.auth {
            ProviderAuth::ApiKey(k) => k,
            ProviderAuth::OAuth(k) => k,
            ProviderAuth::Local => String::new(),
        };
        Self {
            client: reqwest::Client::new(),
            endpoint: cfg.endpoint,
            api_key,
            models: cfg.models,
        }
    }
}

pub struct PyO3LocalProvider {
    models: Vec<String>,
}

impl PyO3LocalProvider {
    pub fn new(cfg: ProviderConfig) -> Self {
        Self { models: cfg.models }
    }
}

#[async_trait]
impl LocalProvider for HttpLocalProvider {
    async fn completion(
        &self,
        _model: &str,
        _messages: &[u8],
        _params: &ProviderCapacity,
    ) -> Result<Vec<u8>, ProviderError> {
        // Placeholder — real impl calls reqwest to the provider endpoint.
        Ok(b"{}".to_vec())
    }
    async fn health_check(&self) -> ProviderHealth {
        ProviderHealth::Unknown
    }
    fn supported_models(&self) -> Vec<String> {
        self.models.clone()
    }
}

#[async_trait]
impl LocalProvider for PyO3LocalProvider {
    async fn completion(
        &self,
        _model: &str,
        _messages: &[u8],
        _params: &ProviderCapacity,
    ) -> Result<Vec<u8>, ProviderError> {
        Ok(b"{}".to_vec())
    }
    async fn health_check(&self) -> ProviderHealth {
        ProviderHealth::Unknown
    }
    fn supported_models(&self) -> Vec<String> {
        self.models.clone()
    }
}

/// A deterministic local provider for docker tests and CLI
/// `--mock-provider` mode. Returns a fixed JSON response without
/// calling any real API. Used by T-CLI1 and Layer 4 docker tests.
pub struct MockLocalProvider {
    models: Vec<String>,
    response: Vec<u8>,
}

impl MockLocalProvider {
    pub fn new(models: Vec<String>) -> Self {
        Self {
            models,
            response: br#"{"mock":true}"#.to_vec(),
        }
    }

    pub fn with_response(models: Vec<String>, response: Vec<u8>) -> Self {
        Self { models, response }
    }
}

#[async_trait]
impl LocalProvider for MockLocalProvider {
    async fn completion(
        &self,
        _model: &str,
        _messages: &[u8],
        _params: &ProviderCapacity,
    ) -> Result<Vec<u8>, ProviderError> {
        Ok(self.response.clone())
    }
    async fn health_check(&self) -> ProviderHealth {
        ProviderHealth::Healthy
    }
    fn supported_models(&self) -> Vec<String> {
        self.models.clone()
    }
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct ProviderConfig {
    pub name: String,
    pub endpoint: String,
    pub auth: ProviderAuth,
    pub models: Vec<String>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub enum ProviderAuth {
    ApiKey(String),
    OAuth(String),
    Local,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct PeerConfig {
    pub node_id: RouterNodeId,
    pub endpoint: std::net::SocketAddr,
    pub trust_level: PeerTrust,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum PeerTrust {
    Trusted,
    Verified,
    Untrusted,
}

pub struct LocalProviderSender;

#[async_trait]
impl octo_transport::sender::NetworkSender for LocalProviderSender {
    async fn send(
        &self,
        _payload: &[u8],
        _ctx: &octo_transport::sender::SendContext,
    ) -> Result<(), octo_transport::sender::TransportError> {
        Ok(())
    }
    fn name(&self) -> &str {
        "local-provider-placeholder"
    }
    fn is_healthy(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_capacity_from_config() {
        let cfg = ProviderConfig {
            name: "openai".into(),
            endpoint: "https://api.openai.com".into(),
            auth: ProviderAuth::ApiKey("key".into()),
            models: vec!["gpt-4o".into(), "gpt-3.5-turbo".into()],
        };
        let node_id = RouterNodeId([1u8; 32]);
        let cap = ProviderCapacity::from_config(&cfg, node_id);
        assert_eq!(cap.provider_name, "openai");
        assert_eq!(
            cap.models,
            vec![String::from("gpt-4o"), String::from("gpt-3.5-turbo")]
        );
        assert_eq!(cap.requests_remaining, u64::MAX);
        assert_eq!(cap.status, ProviderHealth::Unknown);
        assert_eq!(cap.pricing.len(), 2);
        // Provider ID is deterministic
        let cap2 = ProviderCapacity::from_config(&cfg, node_id);
        assert_eq!(cap.provider_id, cap2.provider_id);
    }

    #[test]
    fn http_provider_new_api_key() {
        let cfg = ProviderConfig {
            name: "openai".into(),
            endpoint: "https://api.openai.com".into(),
            auth: ProviderAuth::ApiKey("sk-test".into()),
            models: vec!["gpt-4o".into()],
        };
        let p = HttpLocalProvider::new(cfg);
        assert_eq!(p.api_key, "sk-test");
        assert_eq!(p.models, vec![String::from("gpt-4o")]);
    }

    #[test]
    fn http_provider_new_oauth() {
        let cfg = ProviderConfig {
            name: "openai".into(),
            endpoint: "https://api.openai.com".into(),
            auth: ProviderAuth::OAuth("token".into()),
            models: vec![],
        };
        let p = HttpLocalProvider::new(cfg);
        assert_eq!(p.api_key, "token");
    }

    #[test]
    fn http_provider_new_local() {
        let cfg = ProviderConfig {
            name: "ollama".into(),
            endpoint: "http://localhost:11434".into(),
            auth: ProviderAuth::Local,
            models: vec!["llama3".into()],
        };
        let p = HttpLocalProvider::new(cfg);
        assert_eq!(p.api_key, "");
    }

    #[test]
    fn provider_health_all_variants() {
        let variants = [
            ProviderHealth::Healthy,
            ProviderHealth::Degraded,
            ProviderHealth::Unavailable,
            ProviderHealth::Unknown,
        ];
        for v in &variants {
            let encoded = bincode::serialize(v).unwrap();
            let decoded: ProviderHealth = bincode::deserialize(&encoded).unwrap();
            assert_eq!(*v, decoded);
        }
    }

    #[tokio::test]
    async fn mock_provider_returns_default_response() {
        let p = MockLocalProvider::new(vec!["gpt-4o".into()]);
        let params = ProviderCapacity {
            provider_id: ProviderId([1u8; 32]),
            provider_name: "test".into(),
            router_node_id: RouterNodeId([0u8; 32]),
            models: vec![],
            requests_remaining: 100,
            pricing: vec![],
            status: ProviderHealth::Healthy,
            latency_ms: 0,
            success_rate_bps: 0,
            last_updated: 0,
        };
        let result = p.completion("gpt-4o", b"test", &params).await.unwrap();
        assert_eq!(result, br#"{"mock":true}"#);
        assert_eq!(p.health_check().await, ProviderHealth::Healthy);
        assert_eq!(p.supported_models(), vec!["gpt-4o".to_string()]);
    }

    #[tokio::test]
    async fn mock_provider_with_response() {
        let p =
            MockLocalProvider::with_response(vec!["gpt-4o".into()], b"custom response".to_vec());
        let params = ProviderCapacity {
            provider_id: ProviderId([1u8; 32]),
            provider_name: "test".into(),
            router_node_id: RouterNodeId([0u8; 32]),
            models: vec![],
            requests_remaining: 100,
            pricing: vec![],
            status: ProviderHealth::Healthy,
            latency_ms: 0,
            success_rate_bps: 0,
            last_updated: 0,
        };
        let result = p.completion("gpt-4o", b"", &params).await.unwrap();
        assert_eq!(result, b"custom response");
    }

    #[tokio::test]
    async fn http_provider_health_check_returns_unknown() {
        let cfg = ProviderConfig {
            name: "openai".into(),
            endpoint: "https://api.openai.com".into(),
            auth: ProviderAuth::ApiKey("test".into()),
            models: vec!["gpt-4o".into()],
        };
        let p = HttpLocalProvider::new(cfg);
        assert_eq!(p.health_check().await, ProviderHealth::Unknown);
        assert_eq!(p.supported_models(), vec!["gpt-4o".to_string()]);
    }

    #[tokio::test]
    async fn http_provider_completion_returns_placeholder() {
        let cfg = ProviderConfig {
            name: "openai".into(),
            endpoint: "https://api.openai.com".into(),
            auth: ProviderAuth::ApiKey("test".into()),
            models: vec!["gpt-4o".into()],
        };
        let p = HttpLocalProvider::new(cfg);
        let params = ProviderCapacity {
            provider_id: ProviderId([1u8; 32]),
            provider_name: "test".into(),
            router_node_id: RouterNodeId([0u8; 32]),
            models: vec![],
            requests_remaining: 100,
            pricing: vec![],
            status: ProviderHealth::Healthy,
            latency_ms: 0,
            success_rate_bps: 0,
            last_updated: 0,
        };
        let result = p.completion("gpt-4o", b"test", &params).await.unwrap();
        assert_eq!(result, b"{}");
    }
}
