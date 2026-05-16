use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use thiserror::Error;

pub use crate::providers::Provider;
pub use crate::router::RoutingStrategy;

// ============================================================================
// Error Types
// ============================================================================

#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("Failed to get config directory")]
    NoConfigDir,
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("YAML error: {0}")]
    Yaml(#[from] serde_yaml::Error),
    #[error("Feature not yet specified: {0}")]
    NotYetSpecified(String),
    #[error("Provider not specified for model: {0}")]
    MissingProvider(String),
}

// ============================================================================
// RFC-0927 Types (RouterConfig Extension for LiteLLM Compatibility)
// ============================================================================

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

/// Routing strategy arguments (per strategy)
/// Maps to LiteLLM's routing_strategy_args
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingStrategyArgs {
    /// Latency threshold in ms for latency-based-routing
    pub latency_threshold_ms: Option<u64>,
    /// Allowed consecutive failures before cooldown
    pub allowed_fails: Option<u32>,
    /// Cooldown time in seconds when deployment enters cooldown
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
    #[serde(alias = "api_base")]
    pub api_base: Option<String>,
    /// Base URL alias (LiteLLM compatibility)
    #[serde(alias = "base_url")]
    pub base_url: Option<String>,
    /// API version (provider-specific, e.g., "2024-01-01" for Azure)
    pub api_version: Option<String>,
    /// Request timeout in seconds
    pub timeout: Option<f64>,
    /// Streaming timeout in seconds (time-to-first-token budget)
    #[serde(alias = "stream_timeout_secs")]
    pub stream_timeout_secs: Option<f64>,
    /// Maximum retries per request
    #[serde(alias = "max_retries")]
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
    /// Custom headers
    pub extra_headers: Option<HashMap<String, String>>,
    /// Model group alias for routing (LiteLLM: model_group_alias)
    #[serde(alias = "model_group_alias")]
    pub model_group_alias: Option<String>,
    /// Parameters to drop from forwarded request
    pub drop_params: Option<Vec<String>>,
    /// Model to fall back to on context window error
    pub context_window_fallback_model: Option<String>,
}

/// Extract env var name from `os.environ["KEY"]` or `os.environ['KEY']` syntax.
/// Returns None if the input doesn't match the pattern.
pub fn extract_os_environ_key(value: &str) -> Option<&str> {
    let value = value.trim();
    // Match os.environ["KEY"] or os.environ['KEY']
    let inner = value.strip_prefix("os.environ[")?.strip_suffix(']')?;
    // Remove quotes (single or double)
    let key = inner
        .strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .or_else(|| inner.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')))?;
    if key.is_empty() {
        return None;
    }
    Some(key)
}

impl LiteLLMParams {
    /// Resolve api_base with 4-tier precedence (RFC-0931):
    /// 1. Explicit non-empty value (api_base or base_url alias)
    /// 2. os.environ["KEY"] syntax
    /// 3. {PROVIDER}_API_BASE env var
    /// 4. Provider-specific default from RFC-0930 registry
    pub fn resolve_api_base(&self) -> Option<String> {
        // Tier 1: Explicit non-empty value
        if let Some(base) = self.api_base.as_deref().or(self.base_url.as_deref()) {
            if !base.is_empty() {
                // Check if it's os.environ syntax
                if let Some(key) = extract_os_environ_key(base) {
                    if let Ok(val) = std::env::var(key) {
                        if !val.is_empty() {
                            return Some(val);
                        }
                    }
                } else {
                    return Some(base.to_string());
                }
            }
        }

        // Tier 3: {PROVIDER}_API_BASE env var
        if !self.provider.is_empty() {
            let env_key = format!("{}_API_BASE", self.provider.to_uppercase());
            if let Ok(val) = std::env::var(&env_key) {
                if !val.is_empty() {
                    return Some(val);
                }
            }
        }

        // Tier 4: Provider-specific default from RFC-0930 registry
        if !self.provider.is_empty() {
            get_provider_default_api_base(&self.provider)
        } else {
            None
        }
    }

    /// Resolve api_key with 2-tier precedence (RFC-0931).
    ///
    /// Tiers: (1) explicit non-empty value, (2) os.environ["KEY"] syntax.
    ///
    /// PROVIDER_API_KEY env var is resolved at runtime (RFC-0938), not config time.
    pub fn resolve_api_key(&self) -> Option<String> {
        if let Some(key) = self.api_key.as_deref() {
            if !key.is_empty() {
                if let Some(env_key) = extract_os_environ_key(key) {
                    if let Ok(val) = std::env::var(env_key) {
                        if !val.is_empty() {
                            return Some(val);
                        }
                    }
                } else {
                    return Some(key.to_string());
                }
            }
        }
        None
    }
}

/// Returns default api_base for provider, or None if no default.
/// Per RFC-0930 Section 3.1.
pub fn get_provider_default_api_base(provider: &str) -> Option<String> {
    match provider {
        "openai" => Some("https://api.openai.com/v1".to_string()),
        "anthropic" => Some("https://api.anthropic.com".to_string()),
        "mistral" => Some("https://api.mistral.ai/v1".to_string()),
        "gemini" => Some("https://generativelanguage.googleapis.com".to_string()),
        "cohere" => Some("https://api.cohere.ai".to_string()),
        "voyage" => Some("https://api.voyageai.com/v1".to_string()),
        // azure: no default — requires explicit api_base
        // All other providers: no known default
        _ => None,
    }
}

/// Infer provider name from model string.
/// Supports "provider/model" and "provider:model" formats.
/// Per RFC-0930 Section 1.
pub fn infer_provider(model: &str) -> Option<String> {
    if let Some((provider, _)) = model.split_once('/') {
        let provider = provider.to_lowercase();
        if provider.is_empty() {
            return None;
        }
        return Some(provider);
    }
    if let Some((provider, _)) = model.split_once(':') {
        let provider = provider.to_lowercase();
        if provider.is_empty() {
            return None;
        }
        return Some(provider);
    }
    None
}

/// Rate limit enforcement mode (matches LiteLLM's enforce_model_rate_limits)
/// Default: Soft (RPM/TPM used for routing decisions only)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub enum RateLimitMode {
    /// RPM/TPM used for routing decisions only (default, matches LiteLLM)
    #[default]
    Soft,
    /// Hard blocking when limit exceeded
    Hard,
}

// ============================================================================
// RFC-0928 Types (Deployment Configuration Schema)
// ============================================================================

/// Per-deployment configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeploymentConfig {
    /// Unique deployment identifier
    /// LiteLLM: "id" (serde alias for drop-in compatibility)
    /// If None, auto-generated from model_name: "{provider}_{model}"
    #[serde(alias = "id")]
    pub deployment_id: Option<String>,
    /// Model name for client (e.g., "gpt-4o")
    pub model_name: String,
    /// Litellm-compatible params
    pub litellm_params: LiteLLMParams,
    /// Requests per minute limit (0 = unlimited)
    #[serde(alias = "requests_per_minute", default)]
    pub rpm: u32,
    /// Tokens per minute limit (0 = unlimited)
    #[serde(alias = "tokens_per_minute", default)]
    pub tpm: u64,
    /// Model info (tier, base_model, team_id)
    pub model_info: Option<ModelInfo>,
    /// Custom metadata tags
    pub metadata: Option<HashMap<String, String>>,
}

/// Global router settings (LiteLLM: router_settings)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouterSettings {
    /// Routing strategy for deployment selection
    #[serde(default)]
    pub routing_strategy: RoutingStrategy,
    /// Routing strategy arguments
    #[serde(default)]
    pub routing_strategy_args: RoutingStrategyArgs,
    /// Number of retries on failure
    #[serde(default)]
    pub num_retries: u32,
    /// Request timeout in seconds
    #[serde(default)]
    pub timeout_secs: f64,
    /// Fallback models
    pub fallbacks: Option<HashMap<String, Vec<String>>>,
    /// Redis host for distributed caching/rate limiting
    pub redis_host: Option<String>,
    /// Redis port
    pub redis_port: Option<u16>,
    /// Redis password
    pub redis_password: Option<String>,
    /// Streaming timeout in seconds
    pub stream_timeout_secs: Option<f64>,
    /// Rate limit enforcement mode (RFC-0929)
    #[serde(default)]
    pub rate_limit_mode: RateLimitMode,
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
            rate_limit_mode: RateLimitMode::Soft,
        }
    }
}

/// Global LiteLLM settings (LiteLLM: litellm_settings)
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
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
    /// Use Google Vertex AI
    pub set_google_vertex_ai: Option<bool>,
}

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
    #[serde(alias = "group")]
    pub model_group: Option<String>,
    /// Supports streaming
    pub supports_streaming: Option<bool>,
    /// Supports embeddings
    #[serde(alias = "embeddings")]
    pub supports_embeddings: Option<bool>,
}

/// Pricing configuration per model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PricingConfig {
    /// Input price per million tokens (USD)
    pub input_price_per_million: Option<f64>,
    /// Input cost per second
    #[serde(alias = "input_cost_per_second")]
    pub input_cost_per_second: Option<f64>,
    /// Output price per million tokens (USD)
    pub output_price_per_million: Option<f64>,
    /// Output cost per second
    #[serde(alias = "output_cost_per_second")]
    pub output_cost_per_second: Option<f64>,
}

/// Provider-level configuration (any-llm pattern)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnyLlmProviderConfig {
    pub api_key: Option<String>,
    pub api_base: Option<String>,
}

/// Top-level gateway configuration
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
    /// Global rate limit (RPM per user)
    pub rate_limit_rpm: Option<u32>,
    /// CORS allowed origins
    pub cors_allow_origins: Option<Vec<String>>,
    /// Per-model pricing
    pub pricing: Option<HashMap<String, PricingConfig>>,
    /// Enable Prometheus metrics endpoint
    #[serde(default)]
    pub enable_metrics: bool,
    /// Bootstrap initial API key on startup
    #[serde(default)]
    pub bootstrap_api_key: bool,
    /// Auto-migrate database on startup
    #[serde(default)]
    pub auto_migrate: bool,
    /// Router deployments (primary key)
    pub deployments: Vec<DeploymentConfig>,
    /// Router deployments alias (LiteLLM compatibility — used when YAML uses model_list)
    /// This field is populated ONLY when model_list key is used in YAML (not deployments)
    #[serde(rename = "model_list")]
    pub model_list_alias: Option<Vec<DeploymentConfig>>,
    /// Global router settings (LiteLLM: router_settings)
    pub router_settings: Option<RouterSettings>,
    /// Global LiteLLM settings (LiteLLM: litellm_settings)
    pub litellm_settings: Option<LiteLLMSettings>,
    /// Provider configurations (any-llm compatibility)
    pub providers: Option<HashMap<String, AnyLlmProviderConfig>>,
}

impl GatewayConfig {
    /// Get deployments, supporting both "deployments" and "model_list" keys
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

// ============================================================================
// RFC-0929 Types (GatewayConfig Provider Dispatch Mapping)
// ============================================================================

/// Dispatch information for a deployment
/// Maps GatewayConfig deployment to provider call parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DispatchInfo {
    /// Unique deployment identifier
    pub deployment_id: String,
    /// Provider name (e.g., "openai", "anthropic")
    pub provider: String,
    /// Model name
    pub model: String,
    /// API key (optional — may come from key storage at call time)
    pub api_key: Option<String>,
    /// API base URL for this deployment (optional)
    pub api_base: Option<String>,
    /// Requests per minute limit
    pub rpm: u32,
    /// Tokens per minute limit
    pub tpm: u64,
    /// Model group for routing
    pub model_group: Option<String>,
    /// Per-deployment custom metadata
    pub metadata: Option<HashMap<String, String>>,
    /// Maximum retries per request
    pub max_retries: Option<u32>,
}

impl DispatchInfo {
    /// Auto-generate deployment_id from provider and model
    /// Format: "{provider}_{model}" with underscores
    pub fn auto_id(provider: &str, model: &str) -> Result<String, ConfigError> {
        if provider.is_empty() {
            return Err(ConfigError::MissingProvider(
                "auto_id requires non-empty provider".to_string(),
            ));
        }
        if model.is_empty() {
            return Err(ConfigError::NotYetSpecified(
                "auto_id requires non-empty model".to_string(),
            ));
        }
        Ok(format!("{}_{}", provider, model))
    }
}

/// Convert GatewayConfig to dispatch map
pub fn to_provider_map(
    config: &GatewayConfig,
) -> Result<HashMap<String, DispatchInfo>, ConfigError> {
    let mut map = HashMap::new();
    for deployment in config.get_deployments() {
        // Resolve provider: explicit > inferred from model_name > error
        let provider = if !deployment.litellm_params.provider.is_empty() {
            deployment.litellm_params.provider.clone()
        } else if let Some(inferred) = infer_provider(&deployment.model_name) {
            inferred
        } else {
            return Err(ConfigError::MissingProvider(deployment.model_name.clone()));
        };

        let id = match deployment.deployment_id.clone() {
            Some(id) => id,
            None => DispatchInfo::auto_id(&provider, &deployment.litellm_params.model)?,
        };

        // Resolve api_base: 4-tier precedence (RFC-0931)
        // Tier 1: explicit value, Tier 2: os.environ, Tier 3: {PROVIDER}_API_BASE, Tier 4: registry
        let api_base = deployment.litellm_params.resolve_api_base();

        let info = DispatchInfo {
            deployment_id: id.clone(),
            provider,
            model: deployment.litellm_params.model.clone(),
            api_key: deployment.litellm_params.api_key.clone(),
            api_base,
            rpm: deployment.rpm,
            tpm: deployment.tpm,
            model_group: deployment
                .model_info
                .as_ref()
                .and_then(|m| m.model_group.clone())
                .or_else(|| deployment.litellm_params.model_group_alias.clone()),
            metadata: deployment.metadata.clone(),
            max_retries: deployment
                .litellm_params
                .max_retries
                .or_else(|| config.router_settings.as_ref().map(|s| s.num_retries)),
        };
        map.insert(id, info);
    }
    Ok(map)
}

/// Parse config from YAML string into GatewayConfig
pub fn parse_config(yaml: &str) -> Result<GatewayConfig, ConfigError> {
    serde_yaml::from_str(yaml).map_err(ConfigError::from)
}

/// Load config from file path
pub fn load_config(path: &std::path::Path) -> Result<GatewayConfig, ConfigError> {
    let content = std::fs::read_to_string(path)?;
    // For now, try JSON (legacy Config format) — YAML support needs serde_yaml
    serde_json::from_str(&content).map_err(ConfigError::from)
}

// ============================================================================
// Legacy Config Types (kept for backward compatibility)
// ============================================================================

/// WAL Pub/Sub configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WalPubSubConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_poll_interval")]
    pub poll_interval_ms: u64,
    pub wal_path: Option<String>,
}

fn default_true() -> bool {
    true
}

fn default_poll_interval() -> u64 {
    50
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub balance: u64,
    pub providers: Vec<Provider>,
    pub proxy_port: u16,
    pub db_path: PathBuf,
    #[serde(default)]
    pub wal_pubsub: WalPubSubConfig,
}

impl Config {
    pub fn load() -> Result<Self, ConfigError> {
        let config_path = Self::config_path()?;
        if config_path.exists() {
            let content = std::fs::read_to_string(&config_path)?;
            Ok(serde_json::from_str(&content)?)
        } else {
            Ok(Config {
                balance: 100,
                providers: vec![],
                proxy_port: 8080,
                db_path: Self::default_db_path(),
                wal_pubsub: WalPubSubConfig {
                    enabled: true,
                    poll_interval_ms: 50,
                    wal_path: None,
                },
            })
        }
    }

    pub fn save(&self) -> Result<(), ConfigError> {
        let config_path = Self::config_path()?;
        if let Some(parent) = config_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(&config_path, content)?;
        Ok(())
    }

    fn config_path() -> Result<PathBuf, ConfigError> {
        let proj_dirs = ProjectDirs::from("com", "cipherocto", "quota-router")
            .ok_or(ConfigError::NoConfigDir)?;
        Ok(proj_dirs.config_dir().join("config.json"))
    }

    fn default_db_path() -> PathBuf {
        let proj_dirs = ProjectDirs::from("com", "cipherocto", "quota-router")
            .expect("Failed to get project directories");
        proj_dirs.data_dir().join("quota-router.db")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dispatch_info_auto_id() {
        assert_eq!(
            DispatchInfo::auto_id("openai", "gpt-4o").unwrap(),
            "openai_gpt-4o"
        );
        assert_eq!(
            DispatchInfo::auto_id("anthropic", "claude-3-opus").unwrap(),
            "anthropic_claude-3-opus"
        );
    }

    #[test]
    fn test_dispatch_info_auto_id_empty_provider() {
        let result = DispatchInfo::auto_id("", "gpt-4o");
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().to_string(),
            "Provider not specified for model: auto_id requires non-empty provider"
        );
    }

    #[test]
    fn test_dispatch_info_auto_id_empty_model() {
        let result = DispatchInfo::auto_id("openai", "");
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().to_string(),
            "Feature not yet specified: auto_id requires non-empty model"
        );
    }

    #[test]
    fn test_gateway_config_get_deployments() {
        // This test requires serde_yaml — skip if not available
        // For now, verify the struct can be constructed directly
        let deployments = vec![DeploymentConfig {
            deployment_id: Some("test-deploy".to_string()),
            model_name: "gpt-4o".to_string(),
            litellm_params: LiteLLMParams {
                provider: "openai".to_string(),
                model: "gpt-4o".to_string(),
                api_key: None,
                api_base: None,
                base_url: None,
                api_version: None,
                timeout: None,
                stream_timeout_secs: None,
                max_retries: None,
                aws_access_key_id: None,
                aws_secret_access_key: None,
                aws_region_name: None,
                vertex_project: None,
                vertex_location: None,
                vertex_credentials: None,
                organization: None,
                extra_headers: None,
                model_group_alias: None,
                drop_params: None,
                context_window_fallback_model: None,
            },
            rpm: 1000,
            tpm: 100000,
            model_info: None,
            metadata: None,
        }];
        let config = GatewayConfig {
            database_url: None,
            host: None,
            port: None,
            master_key: None,
            rate_limit_rpm: None,
            cors_allow_origins: None,
            pricing: None,
            enable_metrics: false,
            bootstrap_api_key: false,
            auto_migrate: false,
            deployments: deployments.clone(),
            model_list_alias: None,
            router_settings: None,
            litellm_settings: None,
            providers: None,
        };
        let result = config.get_deployments();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].deployment_id.as_deref(), Some("test-deploy"));
    }

    #[test]
    fn test_gateway_config_model_list_alias() {
        // This test requires serde_yaml — skip if not available
        let deployments = vec![DeploymentConfig {
            deployment_id: Some("model-list-deploy".to_string()),
            model_name: "gpt-4o".to_string(),
            litellm_params: LiteLLMParams {
                provider: "openai".to_string(),
                model: "gpt-4o".to_string(),
                api_key: None,
                api_base: None,
                base_url: None,
                api_version: None,
                timeout: None,
                stream_timeout_secs: None,
                max_retries: None,
                aws_access_key_id: None,
                aws_secret_access_key: None,
                aws_region_name: None,
                vertex_project: None,
                vertex_location: None,
                vertex_credentials: None,
                organization: None,
                extra_headers: None,
                model_group_alias: None,
                drop_params: None,
                context_window_fallback_model: None,
            },
            rpm: 1000,
            tpm: 100000,
            model_info: None,
            metadata: None,
        }];
        let config = GatewayConfig {
            database_url: None,
            host: None,
            port: None,
            master_key: None,
            rate_limit_rpm: None,
            cors_allow_origins: None,
            pricing: None,
            enable_metrics: false,
            bootstrap_api_key: false,
            auto_migrate: false,
            deployments: vec![],
            model_list_alias: Some(deployments.clone()),
            router_settings: None,
            litellm_settings: None,
            providers: None,
        };
        let result = config.get_deployments();
        assert_eq!(result.len(), 1);
        assert_eq!(
            result[0].deployment_id.as_deref(),
            Some("model-list-deploy")
        );
    }

    #[test]
    fn test_to_provider_map() {
        let deployments = vec![DeploymentConfig {
            deployment_id: Some("openai-gpt4o".to_string()),
            model_name: "gpt-4o".to_string(),
            litellm_params: LiteLLMParams {
                provider: "openai".to_string(),
                model: "gpt-4o".to_string(),
                api_key: None,
                api_base: None,
                base_url: None,
                api_version: None,
                timeout: None,
                stream_timeout_secs: None,
                max_retries: None,
                aws_access_key_id: None,
                aws_secret_access_key: None,
                aws_region_name: None,
                vertex_project: None,
                vertex_location: None,
                vertex_credentials: None,
                organization: None,
                extra_headers: None,
                model_group_alias: None,
                drop_params: None,
                context_window_fallback_model: None,
            },
            rpm: 1000,
            tpm: 100000,
            model_info: None,
            metadata: None,
        }];
        let config = GatewayConfig {
            database_url: None,
            host: None,
            port: None,
            master_key: None,
            rate_limit_rpm: None,
            cors_allow_origins: None,
            pricing: None,
            enable_metrics: false,
            bootstrap_api_key: false,
            auto_migrate: false,
            deployments,
            model_list_alias: None,
            router_settings: None,
            litellm_settings: None,
            providers: None,
        };
        let map = to_provider_map(&config).unwrap();
        assert!(map.contains_key("openai-gpt4o"));
        let info = map.get("openai-gpt4o").unwrap();
        assert_eq!(info.provider, "openai");
        assert_eq!(info.model, "gpt-4o");
        assert_eq!(info.rpm, 1000);
    }

    #[test]
    fn test_to_provider_map_auto_id() {
        let deployments = vec![DeploymentConfig {
            deployment_id: None,
            model_name: "gpt-4o".to_string(),
            litellm_params: LiteLLMParams {
                provider: "openai".to_string(),
                model: "gpt-4o".to_string(),
                api_key: None,
                api_base: None,
                base_url: None,
                api_version: None,
                timeout: None,
                stream_timeout_secs: None,
                max_retries: None,
                aws_access_key_id: None,
                aws_secret_access_key: None,
                aws_region_name: None,
                vertex_project: None,
                vertex_location: None,
                vertex_credentials: None,
                organization: None,
                extra_headers: None,
                model_group_alias: None,
                drop_params: None,
                context_window_fallback_model: None,
            },
            rpm: 500,
            tpm: 50000,
            model_info: None,
            metadata: None,
        }];
        let config = GatewayConfig {
            database_url: None,
            host: None,
            port: None,
            master_key: None,
            rate_limit_rpm: None,
            cors_allow_origins: None,
            pricing: None,
            enable_metrics: false,
            bootstrap_api_key: false,
            auto_migrate: false,
            deployments,
            model_list_alias: None,
            router_settings: None,
            litellm_settings: None,
            providers: None,
        };
        let map = to_provider_map(&config).unwrap();
        assert!(map.contains_key("openai_gpt-4o"));
    }

    #[test]
    fn test_to_provider_map_max_retries_fallback() {
        let deployments = vec![DeploymentConfig {
            deployment_id: None,
            model_name: "gpt-4o".to_string(),
            litellm_params: LiteLLMParams {
                provider: "openai".to_string(),
                model: "gpt-4o".to_string(),
                api_key: None,
                api_base: None,
                base_url: None,
                api_version: None,
                timeout: None,
                stream_timeout_secs: None,
                max_retries: None,
                aws_access_key_id: None,
                aws_secret_access_key: None,
                aws_region_name: None,
                vertex_project: None,
                vertex_location: None,
                vertex_credentials: None,
                organization: None,
                extra_headers: None,
                model_group_alias: None,
                drop_params: None,
                context_window_fallback_model: None,
            },
            rpm: 1000,
            tpm: 100000,
            model_info: None,
            metadata: None,
        }];
        let router_settings = RouterSettings {
            routing_strategy: RoutingStrategy::SimpleShuffle,
            routing_strategy_args: RoutingStrategyArgs::default(),
            num_retries: 5,
            timeout_secs: 60.0,
            fallbacks: None,
            redis_host: None,
            redis_port: None,
            redis_password: None,
            stream_timeout_secs: None,
            rate_limit_mode: RateLimitMode::Soft,
        };
        let config = GatewayConfig {
            database_url: None,
            host: None,
            port: None,
            master_key: None,
            rate_limit_rpm: None,
            cors_allow_origins: None,
            pricing: None,
            enable_metrics: false,
            bootstrap_api_key: false,
            auto_migrate: false,
            deployments,
            model_list_alias: None,
            router_settings: Some(router_settings),
            litellm_settings: None,
            providers: None,
        };
        let map = to_provider_map(&config).unwrap();
        let info = map.get("openai_gpt-4o").unwrap();
        assert_eq!(info.max_retries, Some(5));
    }

    #[test]
    fn test_litellm_params_resolve_api_base() {
        let params = LiteLLMParams {
            provider: "openai".to_string(),
            model: "gpt-4o".to_string(),
            api_key: None,
            api_base: Some("https://api.openai.com/v1".to_string()),
            base_url: None,
            api_version: None,
            timeout: None,
            stream_timeout_secs: None,
            max_retries: None,
            aws_access_key_id: None,
            aws_secret_access_key: None,
            aws_region_name: None,
            vertex_project: None,
            vertex_location: None,
            vertex_credentials: None,
            organization: None,
            extra_headers: None,
            model_group_alias: None,
            drop_params: None,
            context_window_fallback_model: None,
        };
        assert_eq!(
            params.resolve_api_base(),
            Some("https://api.openai.com/v1".to_string())
        );
    }

    #[test]
    fn test_wal_pubsub_config_defaults() {
        let config = WalPubSubConfig {
            enabled: true,
            poll_interval_ms: 50,
            wal_path: None,
        };
        assert!(config.enabled);
        assert_eq!(config.poll_interval_ms, 50);
    }

    // ========================================================================
    // YAML-based test vectors (RFC-0929 §Test Vectors)
    // These use parse_config(yaml) to verify GatewayConfig YAML parsing
    // ========================================================================

    #[test]
    fn test_to_provider_map_explicit_id_yaml() {
        // RFC-0929 test_to_provider_map_explicit_id — explicit deployment_id preserved
        let yaml = r#"
deployments:
  - deployment_id: "openai-gpt4o"
    model_name: gpt-4o
    litellm_params:
      provider: openai
      model: gpt-4o
    rpm: 1000
    tpm: 100000
"#;
        let config = parse_config(yaml).unwrap();
        let map = to_provider_map(&config).unwrap();
        assert!(map.contains_key("openai-gpt4o"));
    }

    #[test]
    fn test_to_provider_map_api_key_yaml() {
        // RFC-0929 test_to_provider_map_api_key — api_key preserved from litellm_params
        let yaml = r#"
deployments:
  - deployment_id: "openai-gpt4o"
    model_name: gpt-4o
    litellm_params:
      provider: openai
      model: gpt-4o
      api_key: sk-test-key-123
    rpm: 1000
    tpm: 100000
"#;
        let config = parse_config(yaml).unwrap();
        let map = to_provider_map(&config).unwrap();
        let info = map.get("openai-gpt4o").unwrap();
        assert_eq!(info.api_key, Some("sk-test-key-123".to_string()));
    }

    #[test]
    fn test_to_provider_map_auto_id_yaml() {
        // RFC-0929 test_to_provider_map_auto_id — auto-generated deployment_id
        let yaml = r#"
deployments:
  - model_name: gpt-4o
    litellm_params:
      provider: openai
      model: gpt-4o
    rpm: 500
    tpm: 50000
"#;
        let config = parse_config(yaml).unwrap();
        let map = to_provider_map(&config).unwrap();
        assert!(map.contains_key("openai_gpt-4o"));
    }

    #[test]
    fn test_to_provider_map_auto_id_uses_litellm_model() {
        // Verify auto_id uses litellm_params.model, not model_name
        // model_name is the client-facing alias; litellm_params.model is the provider model ID
        let yaml = r#"
deployments:
  - model_name: my-custom-gpt4o
    litellm_params:
      provider: openai
      model: gpt-4o
    rpm: 500
    tpm: 50000
"#;
        let config = parse_config(yaml).unwrap();
        let map = to_provider_map(&config).unwrap();
        // auto_id should use litellm_params.model ("gpt-4o"), not model_name ("my-custom-gpt4o")
        assert!(map.contains_key("openai_gpt-4o"));
        assert!(!map.contains_key("openai_my-custom-gpt4o"));
    }

    #[test]
    fn test_to_provider_map_model_group_yaml() {
        // RFC-0929 test_to_provider_map_model_group — model_group from model_info
        let yaml = r#"
deployments:
  - model_name: gpt-4o
    litellm_params:
      provider: openai
      model: gpt-4o
    model_info:
      group: "gpt-4-family"
    rpm: 1000
    tpm: 100000
"#;
        let config = parse_config(yaml).unwrap();
        let map = to_provider_map(&config).unwrap();
        let info = map.get("openai_gpt-4o").unwrap();
        assert_eq!(info.model_group, Some("gpt-4-family".to_string()));
    }

    #[test]
    fn test_to_provider_map_api_base_yaml() {
        // RFC-0929 test_to_provider_map_api_base — api_base from litellm_params
        let yaml = r#"
deployments:
  - deployment_id: "azure-gpt4o"
    model_name: gpt-4o
    litellm_params:
      provider: azure
      model: azure/gpt-4-turbo
      api_base: "https://openai-gpt-4-test.openai.azure.com/"
    rpm: 1000
    tpm: 100000
"#;
        let config = parse_config(yaml).unwrap();
        let map = to_provider_map(&config).unwrap();
        let info = map.get("azure-gpt4o").unwrap();
        assert_eq!(
            info.api_base,
            Some("https://openai-gpt-4-test.openai.azure.com/".to_string())
        );
        assert_eq!(info.provider, "azure");
    }

    #[test]
    fn test_to_provider_map_model_group_case_insensitive_yaml() {
        // RFC-0929 test_to_provider_map_model_group_case_insensitive
        // Value stored as-is (case preserved); matching is case-insensitive at routing layer
        let yaml = r#"
deployments:
  - model_name: gpt-4o
    litellm_params:
      provider: openai
      model: gpt-4o
    model_info:
      group: "GPT-4-FAMILY"
    rpm: 1000
    tpm: 100000
"#;
        let config = parse_config(yaml).unwrap();
        let map = to_provider_map(&config).unwrap();
        let info = map.get("openai_gpt-4o").unwrap();
        assert_eq!(info.model_group, Some("GPT-4-FAMILY".to_string()));
    }

    #[test]
    fn test_to_provider_map_empty_yaml() {
        // RFC-0929 test_to_provider_map_empty — empty deployments returns empty map
        let yaml = "deployments: []";
        let config = parse_config(yaml).unwrap();
        let map = to_provider_map(&config).unwrap();
        assert!(map.is_empty());
    }

    #[test]
    fn test_to_provider_map_max_retries_fallback_yaml() {
        // RFC-0929 test_to_provider_map_max_retries_fallback
        // max_retries falls back to RouterSettings.num_retries when litellm_params.max_retries is None
        let yaml = r#"
router_settings:
  num_retries: 5
deployments:
  - model_name: gpt-4o
    litellm_params:
      provider: openai
      model: gpt-4o
    rpm: 1000
    tpm: 100000
"#;
        let config = parse_config(yaml).unwrap();
        let map = to_provider_map(&config).unwrap();
        let info = map.get("openai_gpt-4o").unwrap();
        assert_eq!(info.max_retries, Some(5));
    }

    #[test]
    fn test_to_provider_map_max_retries_no_router_settings_yaml() {
        // RFC-0929 test_to_provider_map_max_retries_no_router_settings
        // When router_settings is None, max_retries stays None
        let yaml = r#"
deployments:
  - model_name: gpt-4o
    litellm_params:
      provider: openai
      model: gpt-4o
"#;
        let config = parse_config(yaml).unwrap();
        let map = to_provider_map(&config).unwrap();
        let info = map.get("openai_gpt-4o").unwrap();
        assert_eq!(info.max_retries, None);
    }

    #[test]
    fn test_to_provider_map_max_retries_litellm_takes_precedence_yaml() {
        // RFC-0929 test_to_provider_map_max_retries_litellm_takes_precedence
        // When both litellm_params.max_retries and router_settings.num_retries are set,
        // litellm_params.max_retries takes precedence
        let yaml = r#"
router_settings:
  num_retries: 5
deployments:
  - model_name: gpt-4o
    litellm_params:
      provider: openai
      model: gpt-4o
      max_retries: 3
"#;
        let config = parse_config(yaml).unwrap();
        let map = to_provider_map(&config).unwrap();
        let info = map.get("openai_gpt-4o").unwrap();
        assert_eq!(info.max_retries, Some(3));
    }

    #[test]
    fn test_to_provider_map_model_group_precedence_yaml() {
        // RFC-0929 test_to_provider_map_model_group_precedence
        // model_info.model_group takes precedence over litellm_params.model_group_alias
        let yaml = r#"
deployments:
  - model_name: gpt-4o
    litellm_params:
      provider: openai
      model: gpt-4o
      model_group_alias: "alias-fallback"
    model_info:
      group: "group-primary"
    rpm: 1000
    tpm: 100000
"#;
        let config = parse_config(yaml).unwrap();
        let map = to_provider_map(&config).unwrap();
        let info = map.get("openai_gpt-4o").unwrap();
        assert_eq!(info.model_group, Some("group-primary".to_string()));
    }

    #[test]
    fn test_to_provider_map_api_base_with_model_info_yaml() {
        // RFC-0929 test_to_provider_map_api_base_with_model_info
        // api_base correctly extracted when both litellm_params and model_info are present
        let yaml = r#"
deployments:
  - deployment_id: "azure-gpt4o"
    model_name: gpt-4o
    litellm_params:
      provider: azure
      model: azure/gpt-4-turbo
      api_base: "https://custom.azure.com/"
    model_info:
      group: "azure-gpt4"
    rpm: 1000
    tpm: 100000
"#;
        let config = parse_config(yaml).unwrap();
        let map = to_provider_map(&config).unwrap();
        let info = map.get("azure-gpt4o").unwrap();
        assert_eq!(info.api_base, Some("https://custom.azure.com/".to_string()));
        assert_eq!(info.model_group, Some("azure-gpt4".to_string()));
    }

    // ========================================================================
    // litellm-mode api_base forwarding test (RFC-0929 Mission 0929-b)
    // ========================================================================

    #[test]
    fn test_litellm_mode_api_base_forwarded() {
        // Verify that api_base from DispatchInfo can be forwarded via HttpCompletionRequest
        // This tests the dispatch path: DispatchInfo.api_base -> HttpCompletionRequest.api_base

        // Create a GatewayConfig with a custom api_base deployment
        let yaml = r#"
deployments:
  - deployment_id: "azure-custom"
    model_name: gpt-4o
    litellm_params:
      provider: azure
      model: azure/gpt-4-turbo
      api_base: "https://custom.azure.com/"
    rpm: 1000
    tpm: 100000
"#;
        let config = parse_config(yaml).unwrap();
        let map = to_provider_map(&config).unwrap();

        // Verify api_base is in DispatchInfo
        let dispatch_info = map.get("azure-custom").unwrap();
        assert_eq!(
            dispatch_info.api_base,
            Some("https://custom.azure.com/".to_string())
        );

        // Verify we can create an HttpCompletionRequest with api_base
        use crate::native_http::HttpCompletionRequest;

        let request = HttpCompletionRequest {
            model: dispatch_info.model.clone(),
            messages: vec![],
            stream: None,
            temperature: None,
            max_tokens: None,
            top_p: None,
            stop: None,
            n: None,
            presence_penalty: None,
            frequency_penalty: None,
            user: None,
            api_base: dispatch_info.api_base.clone(),
        };

        // Verify api_base is forwarded
        assert_eq!(
            request.api_base,
            Some("https://custom.azure.com/".to_string())
        );

        // Verify base_url resolution uses request.api_base when provided
        let base_url = request
            .api_base
            .as_deref()
            .unwrap_or("https://api.openai.com/v1");
        assert_eq!(base_url, "https://custom.azure.com/");
        // The actual provider override happens inside completion() - verified by integration test
    }

    // ========================================================================
    // RFC-0930 Tests: Provider Registry & Inference
    // ========================================================================

    #[test]
    fn test_get_provider_default_api_base_known_providers() {
        assert_eq!(
            get_provider_default_api_base("openai"),
            Some("https://api.openai.com/v1".to_string())
        );
        assert_eq!(
            get_provider_default_api_base("anthropic"),
            Some("https://api.anthropic.com".to_string())
        );
        assert_eq!(
            get_provider_default_api_base("mistral"),
            Some("https://api.mistral.ai/v1".to_string())
        );
        assert_eq!(
            get_provider_default_api_base("gemini"),
            Some("https://generativelanguage.googleapis.com".to_string())
        );
        assert_eq!(
            get_provider_default_api_base("cohere"),
            Some("https://api.cohere.ai".to_string())
        );
        assert_eq!(
            get_provider_default_api_base("voyage"),
            Some("https://api.voyageai.com/v1".to_string())
        );
    }

    #[test]
    fn test_get_provider_default_api_base_azure_returns_none() {
        assert_eq!(get_provider_default_api_base("azure"), None);
    }

    #[test]
    fn test_get_provider_default_api_base_unknown_returns_none() {
        assert_eq!(get_provider_default_api_base("unknown_provider"), None);
        assert_eq!(get_provider_default_api_base(""), None);
    }

    #[test]
    fn test_infer_provider_slash_format() {
        assert_eq!(infer_provider("openai/gpt-4"), Some("openai".to_string()));
        assert_eq!(
            infer_provider("anthropic/claude-3"),
            Some("anthropic".to_string())
        );
        assert_eq!(
            infer_provider("mistral/mistral-large"),
            Some("mistral".to_string())
        );
    }

    #[test]
    fn test_infer_provider_colon_format() {
        assert_eq!(infer_provider("openai:gpt-4"), Some("openai".to_string()));
        assert_eq!(
            infer_provider("anthropic:claude-3"),
            Some("anthropic".to_string())
        );
    }

    #[test]
    fn test_infer_provider_no_prefix() {
        assert_eq!(infer_provider("gpt-4"), None);
        assert_eq!(infer_provider("claude-3"), None);
    }

    #[test]
    fn test_infer_provider_empty_prefix() {
        assert_eq!(infer_provider("/gpt-4"), None);
        assert_eq!(infer_provider(":gpt-4"), None);
    }

    #[test]
    fn test_infer_provider_case_insensitive() {
        assert_eq!(infer_provider("OpenAI/gpt-4"), Some("openai".to_string()));
        assert_eq!(
            infer_provider("ANTHROPIC/claude-3"),
            Some("anthropic".to_string())
        );
    }

    #[test]
    fn test_missing_provider_error_from_to_provider_map() {
        // Create a config with empty provider and no model prefix
        let yaml = r#"
deployments:
  - model_name: gpt-4o
    litellm_params:
      model: gpt-4o
      provider: ""
"#;
        let config = parse_config(yaml).unwrap();
        let result = to_provider_map(&config);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ConfigError::MissingProvider(_)
        ));
    }

    // ========================================================================
    // RFC-0931 Tests: Env Var Syntax
    // ========================================================================

    #[test]
    fn test_extract_os_environ_key_double_quotes() {
        assert_eq!(
            extract_os_environ_key("os.environ[\"MY_KEY\"]"),
            Some("MY_KEY")
        );
    }

    #[test]
    fn test_extract_os_environ_key_single_quotes() {
        assert_eq!(
            extract_os_environ_key("os.environ['MY_KEY']"),
            Some("MY_KEY")
        );
    }

    #[test]
    fn test_extract_os_environ_key_no_match() {
        assert_eq!(extract_os_environ_key("plain_value"), None);
        assert_eq!(extract_os_environ_key("os.environ[]"), None);
        assert_eq!(extract_os_environ_key("os.environ[\"\"]"), None);
    }

    #[test]
    fn test_resolve_api_base_tier1_explicit() {
        let params = LiteLLMParams {
            provider: "openai".to_string(),
            model: "gpt-4o".to_string(),
            api_key: None,
            api_base: Some("https://custom.api.com/v1".to_string()),
            base_url: None,
            api_version: None,
            timeout: None,
            stream_timeout_secs: None,
            max_retries: None,
            aws_access_key_id: None,
            aws_secret_access_key: None,
            aws_region_name: None,
            vertex_project: None,
            vertex_location: None,
            vertex_credentials: None,
            organization: None,
            extra_headers: None,
            model_group_alias: None,
            drop_params: None,
            context_window_fallback_model: None,
        };
        assert_eq!(
            params.resolve_api_base(),
            Some("https://custom.api.com/v1".to_string())
        );
    }

    #[test]
    fn test_resolve_api_base_tier3_env_var() {
        std::env::set_var("TESTPROVIDER_API_BASE", "https://env.api.com");
        let params = LiteLLMParams {
            provider: "testprovider".to_string(),
            model: "test-model".to_string(),
            api_key: None,
            api_base: None,
            base_url: None,
            api_version: None,
            timeout: None,
            stream_timeout_secs: None,
            max_retries: None,
            aws_access_key_id: None,
            aws_secret_access_key: None,
            aws_region_name: None,
            vertex_project: None,
            vertex_location: None,
            vertex_credentials: None,
            organization: None,
            extra_headers: None,
            model_group_alias: None,
            drop_params: None,
            context_window_fallback_model: None,
        };
        assert_eq!(
            params.resolve_api_base(),
            Some("https://env.api.com".to_string())
        );
        std::env::remove_var("TESTPROVIDER_API_BASE");
    }

    #[test]
    fn test_resolve_api_base_tier4_registry() {
        let params = LiteLLMParams {
            provider: "openai".to_string(),
            model: "gpt-4o".to_string(),
            api_key: None,
            api_base: None,
            base_url: None,
            api_version: None,
            timeout: None,
            stream_timeout_secs: None,
            max_retries: None,
            aws_access_key_id: None,
            aws_secret_access_key: None,
            aws_region_name: None,
            vertex_project: None,
            vertex_location: None,
            vertex_credentials: None,
            organization: None,
            extra_headers: None,
            model_group_alias: None,
            drop_params: None,
            context_window_fallback_model: None,
        };
        assert_eq!(
            params.resolve_api_base(),
            Some("https://api.openai.com/v1".to_string())
        );
    }

    #[test]
    fn test_resolve_api_key_explicit() {
        let params = LiteLLMParams {
            provider: "openai".to_string(),
            model: "gpt-4o".to_string(),
            api_key: Some("sk-test123".to_string()),
            api_base: None,
            base_url: None,
            api_version: None,
            timeout: None,
            stream_timeout_secs: None,
            max_retries: None,
            aws_access_key_id: None,
            aws_secret_access_key: None,
            aws_region_name: None,
            vertex_project: None,
            vertex_location: None,
            vertex_credentials: None,
            organization: None,
            extra_headers: None,
            model_group_alias: None,
            drop_params: None,
            context_window_fallback_model: None,
        };
        assert_eq!(params.resolve_api_key(), Some("sk-test123".to_string()));
    }

    #[test]
    fn test_resolve_api_key_os_environ() {
        std::env::set_var("TEST_MY_API_KEY", "sk-from-env");
        let params = LiteLLMParams {
            provider: "openai".to_string(),
            model: "gpt-4o".to_string(),
            api_key: Some("os.environ[\"TEST_MY_API_KEY\"]".to_string()),
            api_base: None,
            base_url: None,
            api_version: None,
            timeout: None,
            stream_timeout_secs: None,
            max_retries: None,
            aws_access_key_id: None,
            aws_secret_access_key: None,
            aws_region_name: None,
            vertex_project: None,
            vertex_location: None,
            vertex_credentials: None,
            organization: None,
            extra_headers: None,
            model_group_alias: None,
            drop_params: None,
            context_window_fallback_model: None,
        };
        assert_eq!(params.resolve_api_key(), Some("sk-from-env".to_string()));
        std::env::remove_var("TEST_MY_API_KEY");
    }

    #[test]
    fn test_resolve_api_key_none() {
        let params = LiteLLMParams {
            provider: "openai".to_string(),
            model: "gpt-4o".to_string(),
            api_key: None,
            api_base: None,
            base_url: None,
            api_version: None,
            timeout: None,
            stream_timeout_secs: None,
            max_retries: None,
            aws_access_key_id: None,
            aws_secret_access_key: None,
            aws_region_name: None,
            vertex_project: None,
            vertex_location: None,
            vertex_credentials: None,
            organization: None,
            extra_headers: None,
            model_group_alias: None,
            drop_params: None,
            context_window_fallback_model: None,
        };
        assert_eq!(params.resolve_api_key(), None);
    }
}
