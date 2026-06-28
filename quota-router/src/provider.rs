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
