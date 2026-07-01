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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_context_all_fields() {
        let ctx = RequestContext {
            model: "gpt-4o".into(),
            preferred_provider: Some("openai".into()),
            model_group: Some("reasoning".into()),
            input_tokens: Some(1024),
            max_output_tokens: Some(2048),
            tags: Some(vec!["test".into()]),
            max_price_per_1k_tokens: Some(10),
            max_latency_ms: Some(200),
            policy_override: Some(RoutingPolicy::Cheapest),
            consumer_id: [42u8; 32],
            priority: 5,
            deadline: Some(1000),
        };
        assert_eq!(ctx.model, "gpt-4o");
        assert_eq!(ctx.preferred_provider, Some("openai".into()));
        assert_eq!(ctx.model_group, Some("reasoning".into()));
        assert_eq!(ctx.input_tokens, Some(1024));
        assert_eq!(ctx.max_output_tokens, Some(2048));
        assert!(ctx.tags.is_some());
        assert_eq!(ctx.max_price_per_1k_tokens, Some(10));
        assert_eq!(ctx.max_latency_ms, Some(200));
        assert!(ctx.policy_override.is_some());
        assert_eq!(ctx.consumer_id, [42u8; 32]);
        assert_eq!(ctx.priority, 5);
        assert_eq!(ctx.deadline, Some(1000));
    }

    #[test]
    fn routing_policy_all_variants() {
        let _c = RoutingPolicy::Cheapest;
        let _f = RoutingPolicy::Fastest;
        let _q = RoutingPolicy::Quality;
        let _b = RoutingPolicy::Balanced;
        let _l = RoutingPolicy::LocalOnly;
        let _custom = RoutingPolicy::Custom(CustomPolicy::default());
    }

    #[test]
    fn forwarding_config_defaults() {
        let cfg = ForwardingConfig::default();
        assert_eq!(cfg.max_ttl, 3);
        assert_eq!(cfg.max_concurrent_forwards, 64);
        assert_eq!(cfg.forward_timeout, Duration::from_secs(30));
        assert_eq!(cfg.max_payload_bytes, 1024 * 1024);
    }
}
