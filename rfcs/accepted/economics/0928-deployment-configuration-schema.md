# RFC-0928 (Economics): Deployment Configuration Schema

## Status

Accepted

## Authors

- Author: @cipherocto

## Maintainers

- Maintainer: @cipherocto

## Summary

Define deployment configuration structures for `quota-router-core` that provide LiteLLM-compatible and any-llm-compatible deployment definitions. This RFC defines the data structures and YAML format but does NOT modify RFC-0917's RouterConfig — it provides a compatible layer that maps to RFC-0917's `providers: HashMap<String, ProviderConfig>`.

## Why Needed

**Current state:**
- RFC-0917 defines `providers: HashMap<String, ProviderConfig>` but lacks structured per-deployment settings
- litellm uses `model_list` with `litellm_params` per deployment
- any-llm uses `providers` dict + `pricing` per model

**Impact:** Users cannot configure deployments using litellm's or any-llm's native YAML format.

## Dependencies

**Requires:**
- RFC-0917: Dual-Mode Query Router (base provider model)
- RFC-0927: RouterConfig Extension for LiteLLM Compatibility (stream_timeout, credentials)

**Required by:**
- RFC-0920: Unified Python SDK (Python SDK uses this config)

**Implementation Note: Dependency Ordering**
RFC-0928 depends on RFC-0927 (which defines field types like RoutingStrategy, RoutingStrategyArgs, LiteLLMParams). Implementation order:
1. RFC-0927 first (defines field types)
2. RFC-0928 second (uses field types from RFC-0927)

## Design Goals

| Goal | Target | Metric |
|------|--------|--------|
| G1 | LiteLLM config compatibility | model_list format supported |
| G2 | any-llm config compatibility | GatewayConfig format supported |
| G3 | Core does heavy lifting | Config parsing/resolution in Rust core |
| G4 | Dual-mode provider support | litellm-mode and any-llm-mode both supported |

## Scope

### Data Structures

**Note:** `LiteLLMParams`, `RoutingStrategy`, and `RoutingStrategyArgs` are defined in RFC-0927 and imported here by reference. This RFC builds on those types to define deployment-level configuration.

#### DeploymentConfig

Per-deployment configuration:

```rust
/// Per-deployment configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeploymentConfig {
    /// Unique deployment identifier
    /// LiteLLM: "id" (serde alias for drop-in compatibility)
    /// If None, auto-generated from model_name: "{provider}_{model}"
    /// Example: None → "openai_gpt-4o"
    #[serde(alias = "id")]
    pub deployment_id: Option<String>,

    /// Model name for client (e.g., "gpt-4o")
    pub model_name: String,

    /// Litellm-compatible params (imported from RFC-0927)
    pub litellm_params: LiteLLMParams,

    /// Requests per minute limit (0 = unlimited)
    /// LiteLLM: "requests_per_minute" (serde alias for drop-in compatibility)
    #[serde(alias = "requests_per_minute")]
    pub rpm: u32,

    /// Tokens per minute limit (0 = unlimited)
    /// LiteLLM: "tokens_per_minute" (serde alias for drop-in compatibility)
    #[serde(alias = "tokens_per_minute")]
    pub tpm: u64,

    /// Model info (tier, base_model, team_id)
    pub model_info: Option<ModelInfo>,

    /// Custom metadata tags (propagate to observability)
    /// LiteLLM: metadata field on deployments
    pub metadata: Option<HashMap<String, String>>,
}
```

#### RouterSettings

Global router configuration (LiteLLM: router_settings):

```rust
/// Global router settings
/// Maps to LiteLLM's router_settings
/// Note: cooldown_time_secs and allowed_fails are in RoutingStrategyArgs, not here
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouterSettings {
    /// Routing strategy for deployment selection
    pub routing_strategy: RoutingStrategy,  // From RFC-0927

    /// Routing strategy arguments (strategy-specific parameters)
    pub routing_strategy_args: RoutingStrategyArgs,  // From RFC-0927

    /// Number of retries on failure
    pub num_retries: u32,

    /// Request timeout in seconds
    pub timeout_secs: f64,

    /// Fallback models (model -> [fallback models])
    pub fallbacks: Option<HashMap<String, Vec<String>>>,

    /// Redis host for distributed caching/rate limiting
    /// LiteLLM: router_settings.redis_host
    pub redis_host: Option<String>,

    /// Redis port
    /// LiteLLM: router_settings.redis_port
    pub redis_port: Option<u16>,

    /// Redis password
    /// LiteLLM: router_settings.redis_password
    pub redis_password: Option<String>,

    /// Streaming timeout in seconds (time-to-first-token budget)
    /// LiteLLM: router_settings.stream_timeout
    pub stream_timeout_secs: Option<f64>,
}

impl Default for RouterSettings {
    fn default() -> Self {
        Self {
            routing_strategy: RoutingStrategy::SimpleShuffle,
            routing_strategy_args: RoutingStrategyArgs::default(),
            num_retries: 3,
            timeout_secs: 60.0,
            fallbacks: None,
            redis_host: None,
            redis_port: None,
            redis_password: None,
            stream_timeout_secs: None,
        }
    }
}

/// LiteLLMSettings

Global LiteLLM settings (LiteLLM: litellm_settings):

```rust
/// Global LiteLLM settings
/// Maps to LiteLLM's litellm_settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiteLLMSettings {
    /// Enable verbose logging
    pub set_verbose: bool,

    /// Parameters to drop from all forwarded requests
    pub drop_params: Option<Vec<String>>,

    /// Enable response caching
    pub cache: bool,

    /// Cache TTL in seconds
    pub cache_ttl_secs: Option<u64>,

    /// Default API base (fallback)
    pub api_base: Option<String>,

    /// Maximum parallel requests
    pub max_parallel_requests: Option<u32>,

    /// Use Google Vertex AI (sets GOOGLE_APPLICATION_CREDENTIALS env var)
    /// LiteLLM: litellm_settings.set_google_vertex_ai
    pub set_google_vertex_ai: Option<bool>,
}

impl Default for LiteLLMSettings {
    fn default() -> Self {
        Self {
            set_verbose: false,
            drop_params: None,
            cache: false,
            cache_ttl_secs: None,
            api_base: None,
            max_parallel_requests: None,
            set_google_vertex_ai: None,
        }
    }
}

/// ModelInfo

Per-model metadata:

```rust
/// Per-model metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    /// Model tier (e.g., "base", "premium", "enterprise")
    pub tier: Option<String>,

    /// Base model for variants (e.g., "gpt-4" for "gpt-4-turbo")
    pub base_model: Option<String>,

    /// Team/owner identifier
    pub team_id: Option<String>,

    /// Model group for routing (LiteLLM: group field)
    /// Multiple deployments with same group are routed together
    /// LiteLLM: "group" (serde alias for drop-in compatibility)
    #[serde(alias = "group")]
    pub model_group: Option<String>,

    /// Supports streaming
    /// NOTE: Router validates streaming requests against provider capability.
    /// If client requests streaming but provider does not support it,
    /// router returns error with supported modes. This field is informational.
    pub supports_streaming: Option<bool>,

    /// Supports embeddings (LiteLLM: embeddings flag)
    /// LiteLLM: "embeddings" (serde alias for drop-in compatibility)
    #[serde(alias = "embeddings")]
    pub supports_embeddings: Option<bool>,
}
```

#### PricingConfig

Per-model pricing for cost tracking (from any-llm):

```rust
/// Pricing configuration per model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PricingConfig {
    /// Input price per million tokens (USD)
    /// LiteLLM: input_cost_per_token / input_cost_per_second
    pub input_price_per_million: Option<f64>,

    /// Input cost per second (alternative to per-million)
    /// LiteLLM: input_cost_per_second (serde alias for drop-in compatibility)
    #[serde(alias = "input_cost_per_second")]
    pub input_cost_per_second: Option<f64>,

    /// Output price per million tokens (USD)
    pub output_price_per_million: Option<f64>,

    /// Output cost per second (alternative to per-million)
    /// LiteLLM: output_cost_per_second (serde alias for drop-in compatibility)
    #[serde(alias = "output_cost_per_second")]
    pub output_cost_per_second: Option<f64>,
}
```

#### GatewayConfig

Top-level gateway configuration matching any-llm's GatewayConfig:

```rust
/// Top-level gateway configuration
/// Matches any-llm's GatewayConfig pattern
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewayConfig {
    /// Database URL for persistence
    pub database_url: Option<String>,

    /// Server host (default: "0.0.0.0")
    pub host: Option<String>,

    /// Server port (default: 8000)
    pub port: Option<u16>,

    /// Master key for admin access
    pub master_key: Option<String>,

    /// Global rate limit (requests per minute per user)
    pub rate_limit_rpm: Option<u32>,

    /// CORS allowed origins (default: empty = no CORS)
    pub cors_allow_origins: Option<Vec<String>>,

    /// Per-model pricing (model_key -> PricingConfig)
    /// Optional — can be None for LiteLLM-only configs where pricing isn't needed
    pub pricing: Option<HashMap<String, PricingConfig>>,

    /// Enable Prometheus metrics endpoint
    pub enable_metrics: bool,

    /// Bootstrap initial API key on startup (generates a new key if true)
    pub bootstrap_api_key: bool,

    /// Auto-migrate database on startup
    pub auto_migrate: bool,

    /// Router deployments
    /// NOTE: "model_list" is accepted as alias for "deployments" for LiteLLM compatibility
    /// Both keys may be present; "deployments" takes precedence if both are set
    pub deployments: Vec<DeploymentConfig>,

    /// Router deployments alias (LiteLLM compatibility)
    /// If "deployments" is empty but "model_list" has entries, use model_list
    #[serde(rename = "model_list")]
    pub model_list_alias: Option<Vec<DeploymentConfig>>,

    /// Global router settings (LiteLLM: router_settings)
    pub router_settings: Option<RouterSettings>,

    /// Global LiteLLM settings (LiteLLM: litellm_settings)
    pub litellm_settings: Option<LiteLLMSettings>,

    /// Provider configurations (any-llm compatibility)
    /// provider_name -> ProviderConfig (any-llm pattern)
    /// If not specified, provider is inferred from litellm_params.provider
    pub providers: Option<HashMap<String, AnyLlmProviderConfig>>,
}

/// Provider-level configuration (any-llm pattern)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnyLlmProviderConfig {
    pub api_key: Option<String>,
    pub api_base: Option<String>,
}

impl GatewayConfig {
    /// Get deployments, supporting both "deployments" and "model_list" keys
    /// LiteLLM uses "model_list", we prefer "deployments"
    pub fn get_deployments(&self) -> &[DeploymentConfig] {
        if !self.deployments.is_empty() {
            &self.deployments
        } else if let Some(ref ml) = self.model_list_alias {
            ml
        } else {
            &[]
        }
    }
}
```

### YAML Configuration Formats

#### LiteLLM-Style (proxy_config.yaml)

Compatible with litellm's model_list format. Server settings (host/port) are optional —
defaults are used if not specified (host: "0.0.0.0", port: 8000).

```yaml
# Note: LiteLLM uses model_list, we support both model_list and deployments
# Server settings are optional — defaults apply if not specified
# host: "0.0.0.0"   (default if omitted)
# port: 8000         (default if omitted)
model_list:
  - deployment_id: "gpt-4o-deploy"  # auto-generated from model_name if omitted
    model_name: gpt-4o
    litellm_params:
      provider: openai
      model: gpt-4o
      api_key: os.environ/OPENAI_API_KEY
      timeout: 60
      stream_timeout_secs: 30
    rpm: 1000
    tpm: 100000
    model_info:
      tier: premium
    metadata:
      team: "ai-engineering"
      env: "production"

  - deployment_id: "gpt-4o-azure-deploy"
    model_name: gpt-4o-azure
    litellm_params:
      provider: azure
      model: gpt-4o
      api_base: https://my-azure.openai.ai/
      api_version: "2024-01-01"
      timeout: 60
    rpm: 500
    tpm: 50000

  - deployment_id: "claude-3-opus-deploy"
    model_name: claude-3-opus
    litellm_params:
      provider: anthropic
      model: claude-3-opus-20240229
      api_key: os.environ/ANTHROPIC_API_KEY
      aws_access_key_id: os.environ/AWS_ACCESS_KEY_ID
      aws_secret_access_key: os.environ/AWS_SECRET_ACCESS_KEY
      aws_region_name: us-east-1
      model_group_alias: "claude-3-opus"  # Group with other claude deployments
    rpm: 500
    tpm: 50000

# Global router settings (LiteLLM: router_settings)
router_settings:
  routing_strategy: latency-based
  routing_strategy_args:
    latency_threshold_ms: 100
    allowed_fails: 5
    cooldown_time_secs: 30
  num_retries: 3
  timeout_secs: 60

# Global LiteLLM settings (LiteLLM: litellm_settings)
litellm_settings:
  set_verbose: false
  cache: true
  cache_ttl_secs: 3600
  drop_params: ["frequency_penalty"]
```

#### Any-LLM-Style (config.yml)

Compatible with any-llm's GatewayConfig format:

```yaml
# Server settings
host: "0.0.0.0"
port: 8000
master_key: sk-1234

# Database
database_url: "postgresql://user:pass@host/gateway"

# Rate limiting
rate_limit_rpm: 60

# Observability
enable_metrics: true
auto_migrate: true

# Model pricing (USD per million tokens)
pricing:
  openai/gpt-4o:
    input_price_per_million: 5.00
    output_price_per_million: 15.00
  anthropic/claude-3-opus:
    input_price_per_million: 15.00
    output_price_per_million: 75.00

# Deployments
deployments:
  - deployment_id: "deploy-1"
    model_name: gpt-4o
    litellm_params:
      provider: openai
      model: gpt-4o
      api_key: os.environ/OPENAI_API_KEY
    rpm: 1000
    tpm: 100000
    model_info:
      tier: premium

  - deployment_id: "deploy-2"
    model_name: claude-3-opus
    litellm_params:
      provider: anthropic
      model: claude-3-opus-20240229
      api_key: os.environ/ANTHROPIC_API_KEY
      aws_access_key_id: os.environ/AWS_ACCESS_KEY_ID
      aws_secret_access_key: os.environ/AWS_SECRET_ACCESS_KEY
      aws_region_name: us-east-1
    rpm: 500
    tpm: 50000
```

### Implementation Note: Mapping to RFC-0917

This RFC defines structures that map to RFC-0917's provider model:

```
LiteLLMParams + DeploymentConfig → RFC-0917 ProviderConfig
GatewayConfig → RFC-0917 RouterConfig (with extensions per RFC-0927)
```

The actual integration with RFC-0917's `providers: HashMap<String, ProviderConfig>` is handled by the implementation layer, not in this RFC.

**Implementation Note: Explicit Mapping is Illustrative**

The pseudo-code examples below show the *conceptual* mapping direction. They reference
hypothetical types (ProviderType, HttpProviderType, SdkProviderType, ModelIdentifier)
that do NOT exist in RFC-0917's accepted text. The implementation layer must define
appropriate types or use existing ones from the Rust core.

Do NOT treat the pseudo-code below as authoritative specification — it is a sketch only.

### Core Implementation (Rust core)

**File:** `crates/quota-router-core/src/config.rs`

```rust
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

/// Parse YAML config file into GatewayConfig
pub fn parse_config(yaml: &str) -> Result<GatewayConfig, ConfigError> {
    serde_yaml::from_str(yaml).map_err(ConfigError::from)
}

/// Load config from file path
pub fn load_config(path: &Path) -> Result<GatewayConfig, ConfigError> {
    let content = std::fs::read_to_string(path)?;
    parse_config(&content)
}

/// Convert GatewayConfig to RFC-0917 provider format
///
/// Implementation maps each DeploymentConfig to an RFC-0917 LiteLLMProviderConfig.
/// The mapping logic is implementation-defined.
/// Returns NotYetSpecified error until the mapping is implemented.
pub fn to_provider_map(config: &GatewayConfig) -> Result<HashMap<String, LiteLLMProviderConfig>, ConfigError> {
    Err(ConfigError::NotYetSpecified("to_provider_map not yet implemented"))
}
```

### Error Handling

| Error | Code | Recovery |
|-------|------|----------|
| Invalid YAML format | `CONFIG_ERROR` | Return error, don't start router |
| Missing required field | `CONFIG_MISSING_FIELD` | Return error with field name |
| Invalid rate limit | `CONFIG_INVALID_RATE_LIMIT` | Return error with value |
| Feature not yet specified | `CONFIG_NOT_YET_SPECIFIED` | Return error with feature name |

## Performance Targets

| Metric | Target | Notes |
|--------|--------|-------|
| Config parsing | <10ms | For 100 deployments |
| Provider init | <100ms | Per provider |

## Security Considerations

- API keys must NOT be logged
- Credentials resolved from env vars at parse time
- Sensitive fields stored in KeyStorage, not config

## Open Questions

**Q: Should config support hot-reload?**

**A:** No — initial implementation is static (load at init). Hot-reload via SIGHUP is future work.

## Version History

| Version | Date | Changes |
|---------|------|---------|
| 6 | 2026-05-13 | Adversarial review R9 (drop-in compat): Added serde aliases for id/deployment_id, requests_per_minute/rpm, tokens_per_minute/tpm, group/model_group, embeddings/supports_embeddings, input/output_cost_per_second; Extended PricingConfig with optional per-second pricing; Added set_google_vertex_ai to LiteLLMSettings; Added redis_* and stream_timeout_secs to RouterSettings; Added providers/AnyLlmProviderConfig for any-llm compat |
| 5 | 2026-05-13 | Adversarial review R8: Made pricing optional; Fixed to_provider_map to return Result with NotYetSpecified error; Documented supports_streaming validation; Clarified bootstrap_api_key semantics; Added server config defaults note to LiteLLM YAML |
| 1 | 2026-05-13 | Initial draft — deployment config schema independent of RFC-0917; YAML examples updated with deployment_id; note on auto-generation for LiteLLM compatibility |