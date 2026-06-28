use std::time::Duration;

/// Full request context — carries all routing criteria through the mesh.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct RequestContext {
    pub model: String,
    pub preferred_provider: Option<String>,
    pub model_group: Option<String>,
    pub input_tokens: Option<u32>,
    pub max_output_tokens: Option<u32>,
    pub tags: Option<Vec<String>>,
    pub max_price_per_1k_tokens: Option<u64>,
    pub max_latency_ms: Option<u32>,
    pub policy_override: Option<RoutingPolicy>,
    pub consumer_id: [u8; 32],
    pub priority: u8,
    pub deadline: Option<u64>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub enum RoutingPolicy {
    Cheapest,
    Fastest,
    Quality,
    Balanced,
    LocalOnly,
    Custom(CustomPolicy),
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct CustomPolicy {
    pub model_overrides: Vec<ModelOverride>,
    pub blacklist: Vec<String>,
    pub max_price_per_1k_tokens: u64,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct ModelOverride {
    pub model: String,
    pub preferred_providers: Vec<String>,
    pub max_price: u64,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct ForwardingConfig {
    pub max_ttl: u8,
    pub max_concurrent_forwards: u32,
    pub forward_timeout: Duration,
    pub max_payload_bytes: usize,
}

impl Default for ForwardingConfig {
    fn default() -> Self {
        Self {
            max_ttl: 3,
            max_concurrent_forwards: 64,
            forward_timeout: Duration::from_secs(30),
            max_payload_bytes: 1024 * 1024,
        }
    }
}
