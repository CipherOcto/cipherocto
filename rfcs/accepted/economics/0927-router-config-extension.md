# RFC-0927 (Economics): RouterConfig Extension for LiteLLM Compatibility

## Status

Accepted

## Authors

- Author: @cipherocto

## Maintainers

- Maintainer: @cipherocto

## Summary

Extend RFC-0917's `RouterConfig` with LiteLLM-compatible types: `RoutingStrategy` enum, `RoutingStrategyArgs`, `LiteLLMParams` with `api_base`/`base_url` aliases, and `LatencyRoutingSettings` per RFC-0925. Also adds `stream_timeout_secs`, per-provider `api_base`/`api_version`, cloud credentials (aws_*, vertex_*). This RFC does NOT replace RFC-0917's RouterConfig — it extends it in a backward-compatible way.

## Why Needed

RFC-0917's `RouterConfig` is incomplete for LiteLLM compatibility:
- Missing `RoutingStrategy` enum for routing strategy selection
- Missing `RoutingStrategyArgs` for strategy-specific parameters
- Missing `LiteLLMParams` for provider configuration with api_base/base_url aliasing
- Missing `stream_timeout_secs` for streaming requests
- Missing per-provider `api_base` and `api_version`
- Missing cloud credentials (aws_access_key_id, aws_secret_access_key, aws_region_name)
- Missing vertex credentials (vertex_project, vertex_location, vertex_credentials)
- Missing latency routing settings (failure thresholds, cooldown duration, penalty)

**Impact:** Without these fields, LiteLLM users cannot configure providers the same way.

## Dependencies

**Requires:**
- RFC-0917: Dual-Mode Query Router (RouterConfig)

**Required by:**
- RFC-0928: Deployment Configuration Schema (uses these extensions)

## Design Goals

| Goal | Target | Metric |
|------|--------|--------|
| G1 | Backward compatible with RFC-0917 | Existing RouterConfig fields unchanged |
| G2 | LiteLLM stream_timeout support | `stream_timeout_secs` field available |
| G3 | Cloud credentials support | aws_*, vertex_* fields available |
| G4 | Latency routing settings | failure thresholds, penalty, buffer per RFC-0925 |

## Scope

### Types Defined in This RFC

This RFC defines the following types in `config.rs`:

- `RoutingStrategyArgs` — strategy-specific routing parameters
- `LiteLLMParams` — LiteLLM-compatible provider parameters with `api_base`/`base_url` aliasing
- `LatencyRoutingSettings` — latency-based routing configuration (RFC-0925)
- `RateLimitMode` — rate limit enforcement mode (Soft default, Hard blocking) per RFC-0929

### Types Referenced (Not Defined Here)

- `RoutingStrategy` enum — defined in RFC-0917's `router.rs`. Do NOT redefine here.
- `RouterConfig` base — defined in RFC-0917. This RFC extends it without modification.

### RouterConfig Extension (RFC-0917 §RouterConfig)

Add to existing `RouterConfig` (in config.rs, not as direct extension to RFC-0917's RouterConfig):

```rust
use std::collections::HashMap;

/// Latency-based routing settings (RFC-0925, RFC-0926)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LatencyRoutingSettings {
    /// Max entries in latency rolling window per deployment (per RFC-0925)
    /// Default: 10 (litellm default)
    pub max_latency_list_size: usize,

    /// Latency buffer for best-available selection (default: 0 per RFC-0925)
    pub lowest_latency_buffer: f32,

    /// Cooldown duration in seconds after penalty applied (default: 300)
    pub cooldown_duration_secs: u32,

    /// Penalty latency in µs for timeouts (default: 1_000_000_000 = 1000s per RFC-0926)
    pub timeout_penalty_us: u64,

    /// Failure threshold percent to enter cooldown (default: 0.5 = 50%)
    pub failure_threshold_percent: f32,

    /// Minimum requests before failure rate is checked (default: 5)
    pub failure_threshold_min_requests: u32,
}

impl Default for LatencyRoutingSettings {
    fn default() -> Self {
        Self {
            max_latency_list_size: 10,
            lowest_latency_buffer: 0.0,
            cooldown_duration_secs: 300,
            timeout_penalty_us: 1_000_000_000,
            failure_threshold_percent: 0.5,
            failure_threshold_min_requests: 5,
        }
    }
}

/// Routing strategy (LiteLLM compatible)
/// Maps to LiteLLM's routing_strategy values
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RoutingStrategy {
    /// Default - randomly distributes requests based on rpm/tpm weights
    /// LiteLLM: "simple-shuffle" (default)
    SimpleShuffle,

    /// Route to deployment in strict sequential order (atomic lock-free)
    /// LiteLLM: "round-robin"
    RoundRobin,

    /// Route to deployment with fewest active requests
    /// LiteLLM: "least-busy"
    LeastBusy,

    /// Route to fastest responding deployment (based on rolling latency)
    /// LiteLLM: "latency-based-routing"
    LatencyBased,

    /// Route to deployment with lowest cost per token
    /// LiteLLM: "cost-based-routing"
    CostBased,

    /// Route to deployment with lowest current usage (RPM/TPM)
    /// LiteLLM: "usage-based-routing-v1"
    UsageBased,

    /// Route to deployment with recency-weighted usage scoring
    /// LiteLLM: "usage-based-routing-v2"
    UsageBasedV2,

    /// Route based on explicit deployment weights
    /// LiteLLM: "weighted"
    Weighted,
}

/// Routing strategy arguments (per strategy)
/// Maps to LiteLLM's routing_strategy_args
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingStrategyArgs {
    /// Latency threshold in ms for latency-based-routing
    /// LiteLLM: routing_strategy_args.latency_threshold
    pub latency_threshold_ms: Option<u64>,

    /// Allowed consecutive failures before cooldown
    /// LiteLLM: routing_strategy_args.allowed_fails
    pub allowed_fails: Option<u32>,

    /// Cooldown time in seconds when deployment enters cooldown
    /// LiteLLM: routing_strategy_args.cooldown_time
    pub cooldown_time_secs: Option<u32>,

    /// TPM weight multiplier for simple-shuffle
    pub tpm_weight: Option<f64>,

    /// RPM weight multiplier for simple-shuffle
    pub rpm_weight: Option<f64>,
}

impl Default for RoutingStrategyArgs {
    fn default() -> Self {
        Self {
            latency_threshold_ms: None,
            allowed_fails: Some(5),
            cooldown_time_secs: Some(30),
            tpm_weight: None,
            rpm_weight: None,
        }
    }
}

/// LiteLLM-compatible provider parameters
/// Mirrors LiteLLM's GenericLiteLLMParams
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiteLLMParams {
    /// Provider name (e.g., "openai", "anthropic", "azure")
    pub provider: String,

    /// Model identifier (e.g., "gpt-4o", "claude-3-opus")
    pub model: String,

    /// API key (optional, can use key storage instead)
    pub api_key: Option<String>,

    /// Base URL for API (optional, provider-specific default if not set)
    /// LiteLLM also accepts this as "api_base"
    pub api_base: Option<String>,

    /// Base URL alias (LiteLLM compatibility)
    /// Resolves to api_base if not set
    pub base_url: Option<String>,

    /// API version (provider-specific, e.g., "2024-01-01" for Azure)
    pub api_version: Option<String>,

    /// Request timeout in seconds
    pub timeout: Option<f64>,

    /// Streaming timeout in seconds (time-to-first-token budget)
    /// Named stream_timeout_secs per RFC-0927 for consistency
    /// Must be > 0. If not set, defaults to RouterConfigExt.stream_timeout_secs or 60s.
    pub stream_timeout_secs: Option<f64>,

    /// Maximum retries per request
    pub max_retries: Option<u32>,

    /// AWS access key for Bedrock
    pub aws_access_key_id: Option<String>,

    /// AWS secret access key for Bedrock
    pub aws_secret_access_key: Option<String>,

    /// AWS region for Bedrock (e.g., "us-east-1")
    pub aws_region_name: Option<String>,

    /// Vertex AI project for Google AI
    pub vertex_project: Option<String>,

    /// Vertex AI location for Google AI
    pub vertex_location: Option<String>,

    /// Vertex AI credentials (path to service account JSON)
    pub vertex_credentials: Option<String>,

    /// OpenAI organization ID
    pub organization: Option<String>,

    /// Custom headers (e.g., "x-api-key: ...")
    pub extra_headers: Option<HashMap<String, String>>,

    /// Model group alias for routing (LiteLLM: model_group_alias)
    /// Multiple deployments with same model_group_alias are grouped for routing
    /// Defaults to model if not specified
    pub model_group_alias: Option<String>,

    /// Parameters to drop from forwarded request (LiteLLM: drop_params)
    /// Validated: router drops only params present in the request. Dropping a required
    /// param results in an error before forwarding, not silent failure.
    pub drop_params: Option<Vec<String>>,

    /// Model to fall back to on context window error
    /// LiteLLM: context_window_fallback_model
    pub context_window_fallback_model: Option<String>,
}

impl LiteLLMParams {
    /// Resolve api_base from api_base or base_url (alias)
    pub fn resolve_api_base(&self) -> Option<&str> {
        self.api_base.as_deref().or(self.base_url.as_deref())
    }
}

/// Provider configuration for LiteLLM-compatible deployments
/// This is a separate convenience struct, NOT an extension to RFC-0917's ProviderConfig.
/// RFC-0917's ProviderConfig has: timeout, retry_policy, retry_overrides.
/// This struct adds LiteLLM-compatible fields (provider_type, api_base, credentials, etc.).
/// The mapping between this struct and RFC-0917's is deferred to the implementation layer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiteLLMProviderConfig {
    /// Provider type for dispatch
    pub provider_type: ProviderType,

    /// Model identifier (provider/model split)
    pub model: ModelIdentifier,

    /// API base URL for this deployment
    pub api_base: Option<String>,

    /// API version (provider-specific)
    pub api_version: Option<String>,

    /// Credentials for this deployment
    pub credentials: Option<Credentials>,

    /// Rate limit for this deployment
    pub rate_limit: Option<RateLimitConfig>,

    /// Custom metadata
    pub metadata: Option<HashMap<String, String>>,
}

/// Credentials for a provider deployment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Credentials {
    pub api_key: Option<String>,
    pub aws_access_key_id: Option<String>,
    pub aws_secret_access_key: Option<String>,
    pub aws_region_name: Option<String>,
    pub vertex_project: Option<String>,
    pub vertex_location: Option<String>,
    pub vertex_credentials: Option<String>,
}

/// Provider type for dispatch
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ProviderType {
    HttpProvider(HttpProviderType),
    SdkProvider(SdkProviderType),
}

/// HTTP provider types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HttpProviderType {
    NativeHttp,
    Azure,
    // Other HTTP-based providers as needed
}

/// SDK provider types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SdkProviderType {
    PyBridge,
    // Other SDK-based providers as needed
}

/// Model identifier (provider/model split)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelIdentifier {
    pub provider: String,
    pub model: String,
}

/// Rate limit configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitConfig {
    pub requests_per_minute: Option<u32>,
    pub tokens_per_minute: Option<u64>,
}

/// Extended router configuration for LiteLLM-compatible routing
/// This is a convenience wrapper that combines RFC-0917 RouterConfig fields
/// with LiteLLM-specific extensions. NOT an actual extension to RFC-0917's RouterConfig.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouterConfigExt {
    /// Routing strategy for deployment selection
    /// Default: SimpleShuffle (LiteLLM: "simple-shuffle")
    pub routing_strategy: RoutingStrategy,

    /// Routing strategy arguments (strategy-specific parameters)
    /// Maps to LiteLLM's routing_strategy_args
    pub routing_strategy_args: RoutingStrategyArgs,

    /// Provider configurations keyed by deployment_id
    pub providers: HashMap<String, LiteLLMProviderConfig>,

    /// Storage configuration
    pub storage: StorageConfig,

    /// Enterprise configuration
    pub enterprise: EnterpriseConfig,

    /// Streaming timeout in seconds (time-to-first-token budget)
    /// Default: 60s
    pub stream_timeout_secs: Option<f64>,

    /// Per-provider API base URLs (provider_name -> url)
    pub api_bases: HashMap<String, String>,

    /// Per-provider API versions (provider_name -> version)
    pub api_versions: HashMap<String, String>,

    /// AWS credentials for Bedrock (global fallback)
    pub aws_access_key_id: Option<String>,
    pub aws_secret_access_key: Option<String>,
    pub aws_region_name: Option<String>,

    /// Vertex AI credentials (global fallback)
    pub vertex_project: Option<String>,
    pub vertex_location: Option<String>,
    pub vertex_credentials: Option<String>,

    /// Latency routing settings
    pub latency_settings: Option<LatencyRoutingSettings>,
}

impl Default for RouterConfigExt {
    fn default() -> Self {
        Self {
            routing_strategy: RoutingStrategy::SimpleShuffle,
            routing_strategy_args: RoutingStrategyArgs::default(),
            providers: HashMap::new(),
            storage: StorageConfig::default(),
            enterprise: EnterpriseConfig::default(),
            stream_timeout_secs: Some(60.0),
            api_bases: HashMap::new(),
            api_versions: HashMap::new(),
            aws_access_key_id: None,
            aws_secret_access_key: None,
            aws_region_name: None,
            vertex_project: None,
            vertex_location: None,
            vertex_credentials: None,
            latency_settings: None,
        }
    }
}
```

### Design Note: Backward Compatibility

This RFC extends RFC-0917's RouterConfig WITHOUT breaking changes:
- Existing fields remain unchanged
- New fields are all `Option<T>` or have defaults
- RFC-0917 implementations continue to work

## Feature Gates

Per RFC-0917 §Feature-Gated Provider Initialization:

| Feature | Provider Type | Uses |
|---------|--------------|------|
| `litellm-mode` | `HttpProvider` (native_http) | `api_bases`, `api_versions`, `stream_timeout_secs` |
| `any-llm-mode` | `SdkProvider` (py_bridge) | AWS/Vertex credentials for Python SDKs |
| `full` | Both available | All fields |

## Performance Targets

| Metric | Target | Notes |
|--------|--------|-------|
| Config validation | <5ms | Per deployment |

## Security Considerations

- API keys and credentials must NOT be logged
- Credentials resolved from env vars at parse time

## Open Questions

**Q: Should this extend RFC-0917 directly or use a separate struct?**

**A:** Use `RouterConfigExt` as a wrapper. RFC-0917's `RouterConfig` remains the source of truth for core routing. `RouterConfigExt` adds LiteLLM compatibility layer.

## Version History

| Version | Date | Changes |
|---------|------|---------|
| 5 | 2026-05-13 | Adversarial review R8: Added RoundRobin, UsageBasedV2, Weighted to RoutingStrategy; Added LiteLLMProviderConfig, Credentials, ProviderType, HttpProviderType, SdkProviderType, ModelIdentifier, RateLimitConfig structs; Added impl Default for RouterConfigExt; Documented stream_timeout_secs range and drop_params semantics |
| 4 | 2026-05-13 | Adversarial review R7: Added `impl Default for LatencyRoutingSettings` |