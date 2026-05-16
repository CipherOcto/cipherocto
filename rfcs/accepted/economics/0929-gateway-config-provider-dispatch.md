# RFC-0929: GatewayConfig Provider Dispatch Mapping

## Status

Accepted

## Authors

- Author: @cipherocto

## Maintainers

- Maintainer: @cipherocto

## Summary

Specify how `GatewayConfig.deployments` (RFC-0928) maps to provider calls, replacing the `NotYetSpecified` stub in `to_provider_map()`. This completes the integration chain: `GatewayConfig` → dispatch map → provider calls.

**Two dispatch paths:**
- **litellm-mode**: `GatewayConfig` → `DispatchInfo` → `HttpProviderFactory::create()` → `HttpProvider.completion()` (reqwest-based, native HTTP)
- **any-llm-mode**: `GatewayConfig` → `DispatchInfo` → `py_bridge::factory::completion()` (PyO3, calls Python SDKs)

Per-deployment `api_base` is stored in `DispatchInfo` and resolved at call time. LiteLLM parity requires per-deployment api_base support — LiteLLM stores `api_base` in `litellm_params` per deployment and passes it to provider clients.

## Why Needed

**Current state:**
- RFC-0928 defines `DeploymentConfig` with `litellm_params` (provider, model, api_key, etc.)
- `to_provider_map()` fails with `Err(ConfigError::NotYetSpecified(...))` — no actual mapping exists
- `py_bridge::factory::completion(provider, model, messages, api_key)` exists but is not wired from config

**Impact:** The project cannot route requests from GatewayConfig — the data structures exist but the integration is missing.

## Dependencies

**Requires:**
- RFC-0917: Dual-Mode Query Router (py_bridge::factory::completion)
- RFC-0927: RouterConfig Extension for LiteLLM Compatibility (LiteLLMParams)
- RFC-0928: Deployment Configuration Schema (GatewayConfig, DeploymentConfig)

**Required by:**
- (none — RFC-0920 is a thin binding layer that delegates to RFC-0917/Rust core; this RFC provides the internal dispatch mapping that makes RFC-0928's to_provider_map() functional, but no Accepted RFC currently consumes it as a dependency)

## Design Goals

| Goal | Target | Metric |
|------|--------|--------|
| G1 | Complete integration chain | GatewayConfig → dispatch → provider call |
| G2 | Auto-generate deployment_id | When not provided, use `{provider}_{model}` |
| G3 | Support model_group routing | Multiple deployments with same group routed together |
| G4 | Preserve deployment metadata | rpm, tpm, api_key, model_group, and custom metadata tags |

## Scope

### 1. DispatchInfo Struct

```rust
/// Dispatch information for a deployment
/// Maps GatewayConfig deployment to provider call parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DispatchInfo {
    /// Unique deployment identifier
    pub deployment_id: String,
    /// Provider name (e.g., "openai", "anthropic")
    /// Note: py_bridge::factory::completion takes String, not ProviderType enum
    pub provider: String,
    /// Model name (from LiteLLMParams.model)
    pub model: String,
    /// API key for this deployment (optional — may come from key storage at call time)
    /// SECURITY: Do not log this field
    pub api_key: Option<String>,
    /// API base URL for this deployment (optional)
    /// Used for custom endpoints (proxies, Azure, etc.)
    /// Sources: litellm_params.api_base (per-deployment in LiteLLM model_list)
    /// Resolution: first non-None wins (litellm_params.api_base checked via or_else)
    /// Flow:
    ///   - any-llm-mode: DispatchInfo → py_bridge::factory::completion(api_base) → Provider.with_api_base() (TARGET API — requires factory signature change, see §Implementation Requirements)
    ///   - litellm-mode: stored in Provider.endpoint but NOT forwarded to HttpProviderFactory — provider uses hardcoded default. See §Implementation Requirements.
    /// SECURITY: Never log this field
    pub api_base: Option<String>,
    /// Requests per minute limit (0 = unlimited)
    /// Router may enforce this limit before dispatch if rpm > 0.
    /// If not enforced by router, provider-side enforcement applies.
    /// Informational for observability when not enforced.
    pub rpm: u32,
    /// Tokens per minute limit (0 = unlimited)
    /// Router may enforce this limit before dispatch if tpm > 0.
    /// If not enforced by router, provider-side enforcement applies.
    /// Informational for observability when not enforced.
    pub tpm: u64,
    /// Model group for routing (multiple deployments with same group routed together)
    /// Sources: model_info.model_group (RFC-0928) OR litellm_params.model_group_alias (LiteLLM compat)
    /// LiteLLM uses model_group_alias in litellm_params; RFC-0928 uses group in model_info.
    /// Resolution: first non-None wins (model_info.model_group checked first via or_else).
    /// Note: Actual LiteLLM precedence is unverified — this implements "first non-None" semantics.
    pub model_group: Option<String>,
    /// Per-deployment custom metadata
    /// Used for: observability labels, deployment tags, custom routing hints
    /// NOT used for access control or request routing decisions
    /// Note: HashMap must be in scope (import std::collections::HashMap)
    /// DEPENDENCY: RFC-0928's DeploymentConfig.metadata must also be Option<HashMap<String, String>>
    /// for the to_provider_map() clone to compile. If RFC-0928 uses a different type (e.g.,
    /// Option<serde_json::Value>), a conversion must be added.
    pub metadata: Option<HashMap<String, String>>,
    /// Maximum retries per request for this deployment
    /// Falls back to RouterSettings.num_retries if litellm_params.max_retries is None AND router_settings is provided
    pub max_retries: Option<u32>,
}

impl DispatchInfo {
    /// Auto-generate deployment_id from provider and model
    /// Format: "{provider}_{model}" with underscores (e.g., "openai_gpt-4o")
    /// Note: LiteLLM internally uses "provider/model" or "provider:model" formats.
    /// This underscore format is quota-router specific for HashMap keys.
    /// Returns Err if provider or model is empty.
    pub fn auto_id(provider: &str, model: &str) -> Result<String, ConfigError> {
        if provider.is_empty() || model.is_empty() {
            return Err(ConfigError::MissingProvider(
                format!("auto_id requires non-empty provider and model, got provider='{}' model='{}'", provider, model)
            ));
        }
        Ok(format!("{}_{}", provider, model))
    }
}
```

### 2. to_provider_map() Function

```rust
/// Convert GatewayConfig to dispatch map
///
/// # Algorithm
/// For each deployment in GatewayConfig.get_deployments():
/// 1. Generate deployment_id if not provided (auto_id: "{provider}_{model}" format)
/// 2. Extract from LiteLLMParams: provider, model, api_key, api_base
/// 3. Extract rpm, tpm from DeploymentConfig
/// 4. Extract model_group from model_info.model_group or litellm_params.model_group_alias
/// 5. Return HashMap<deployment_id, DispatchInfo>
///
/// # Note on Return Type
/// RFC-0928 originally specified HashMap<String, LiteLLMProviderConfig>, but:
/// - LiteLLMProviderConfig has ProviderType enum which is incompatible with
///   py_bridge::factory::completion() that takes String provider
/// - DispatchInfo uses String provider matching py_bridge API
/// - This is a correction of RFC-0928's return type spec
///
/// # Example
/// YAML:
/// ```yaml
/// deployments:
///   - deployment_id: "openai-gpt4o"
///     model_name: gpt-4o
///     litellm_params:
///       provider: openai
///       model: gpt-4o
///       api_base: "https://api.openai.com/v1"  # per-deployment custom endpoint
/// ```
/// Result:
/// ```rust
/// DispatchInfo {
///     deployment_id: "openai-gpt4o",
///     provider: "openai",
///     model: "gpt-4o",
///     api_key: None,
///     api_base: Some("https://api.openai.com/v1"),
///     rpm: 1000,
///     tpm: 100000,
///     model_group: None,
///     metadata: None,
/// }
/// ```
pub fn to_provider_map(config: &GatewayConfig) -> Result<HashMap<String, DispatchInfo>, ConfigError> {
    // Full implementation in §API Change
}

/// Integration with py_bridge

The dispatch map feeds into provider calls. **Two mode-specific paths:**

### any-llm-mode (PyO3)

```rust
// SECURITY: api_key should come from key storage, not embedded config.
let dispatch_map = to_provider_map(&config)?;
let deployment = dispatch_map.get("openai-gpt4o").ok_or(ConfigError::DeploymentNotFound)?;

let resolved_api_key = key_storage.get(&deployment.deployment_id)
    .map(String::from)
    .or_else(|| deployment.api_key.clone());

// Current API (4 args — api_base stored in DispatchInfo but NOT yet passed):
// api_base support requires adding parameter to factory signature (see §Implementation Requirements)
py_bridge::factory::completion(
    &deployment.provider,
    &deployment.model,
    &messages,
    resolved_api_key.as_deref(),
    // deployment.api_base.as_deref(),  // TODO: REQUIRES factory signature change
)
```

### litellm-mode (reqwest)

```rust
// SECURITY: api_key should come from key storage, not embedded config.
let dispatch_map = to_provider_map(&config)?;
let deployment = dispatch_map.get("azure-gpt4o").ok_or(ConfigError::DeploymentNotFound)?;

let resolved_api_key = key_storage.get(&deployment.deployment_id)
    .map(String::from)
    .or_else(|| deployment.api_key.clone());

// api_base NOT passed — HttpProviderFactory.create() only accepts name
// Provider.endpoint contains api_base but is NOT forwarded
// GAP: provider uses hardcoded self.api_base, not per-deployment value
let http_provider = HttpProviderFactory::create(&deployment.provider)
    .ok_or(ConfigError::ProviderNotFound)?;

let request = HttpCompletionRequest {
    model: deployment.model.clone(),
    messages: messages.clone(),
    stream: None,  // Option<bool> — None defaults to false (non-streaming)
    // Other fields (temperature, timeout, etc.) per HttpCompletionRequest definition (RFC-0917)
};

http_provider.completion(&request, &resolved_api_key).await
```

**Note on litellm-mode gap:** `HttpProviderFactory::create(name)` only accepts provider name. `Provider.endpoint` (which contains api_base from DispatchInfo) is available in the proxy but is NOT forwarded to the factory. The factory registration uses `|| Box::new(OpenAIProvider::new())` which hardcodes the default api_base. Per-deployment api_base requires implementation changes — see §Implementation Requirements.

**Note on api_base resolution:** RFC-0931 specifies `resolve_api_base()` which resolves api_base through 4 tiers (explicit → os.environ → env var → provider default from RFC-0930 registry). The `api_base` stored in DispatchInfo should be the RFC-0931-resolved value, not the raw LiteLLMParams.api_base. See RFC-0931 §6 for the resolution implementation.

**Mode-specific api_base handling:**

| Mode | Provider Call | api_base Resolution | Implementation Status |
|------|---------------|---------------------|----------------------|
| **litellm-mode** (reqwest) | `HttpProviderFactory::create(name)` → `provider.completion(request, api_key)` | api_base stored in Provider but NOT passed to factory; hardcoded in provider via `OpenAIProvider::new()`. URL constructed at call time via `format!("{}/chat/completions", self.api_base)` | **Gap: requires impl change** |
| **any-llm-mode** (PyO3) | `py_bridge::factory::completion(provider, model, messages, api_key, api_base)` | api_base passed to factory, forwarded to `Provider.with_api_base()` | **Requires factory signature change** (TARGET API — see §Implementation Requirements) |

**LiteLLM comparison (reference):**
LiteLLM passes api_base from `litellm_params` through router → `litellm.acompletion(**kwargs)` → `get_llm_provider()` → `OpenAIChatCompletion._get_openai_client(base_url=api_base)`. The api_base becomes the `base_url` on the SDK client's construction.

**Factory signature update (any-llm-mode only):**
```rust
// TARGET API — requires py_bridge::factory::completion signature change (see §Implementation Requirements)
pub fn completion(
    provider: &str,
    model: &str,
    messages: &[Message],
    api_key: Option<&str>,
    api_base: Option<&str>,  // per-deployment api_base
) -> Result<ChatCompletion, PyBridgeError>
```

Each provider's `with_api_base()` method applies the api_base (implementation per provider — see py_bridge/providers/):
```rust
pub fn with_api_base(mut self, api_base: String) -> Self {
    self.api_base = Some(api_base);
    self
}
```

### 4. Routing Strategy Selection

Routing strategy is defined in RFC-0927's `RouterSettings.routing_strategy`. LiteLLM configs use string values (e.g., `"latency-based-routing"`) which must be parsed to the `RoutingStrategy` enum:

```rust
// NOTE: from_litellm_str() is deprecated — use the existing FromStr impl instead.
// impl std::str::FromStr for RoutingStrategy already handles both underscore
// and hyphen variants via s.replace("-", "_").
//
// Usage: s.parse::<RoutingStrategy>().unwrap_or_default()
```

The implementation of strategy selection (e.g., `shuffle_select`, `round_robin_select`) is deferred to the implementation layer (see `crates/quota-router-core/src/routing/`) — this RFC focuses on the config-to-dispatch mapping.

**Selection point:** When multiple deployments share a model_group, the router selects one based on `router_settings.routing_strategy`. The strategy functions are implementation-defined per RFC-0927.

**model_group matching algorithm:**
1. Filter all deployments in dispatch_map to those where `model_group` case-insensitively matches `requested_group`
2. If no deployments match, return `Err(ConfigError::NoDeploymentFound { model_group: requested_group })`
3. Apply routing_strategy to the filtered candidate set (not all deployments) to select one deployment
4. Route request to the selected deployment

`requested_group` is the model_group requested by the caller (e.g., passed as a routing parameter, extracted from request metadata, or derived from the incoming model string).

**Note:** DispatchInfo is mode-agnostic. It carries the data needed for routing decisions regardless of which mode (litellm-mode, any-llm-mode, full) the router operates in. Mode-specific behavior (how to invoke the provider, which SDK to use) is handled at the call site by the router, not by DispatchInfo.

**Mode-specific dispatch:**
- **litellm-mode**: Router applies model_group filtering and routing_strategy to select a deployment
- **any-llm-mode**: No router. Provider is explicit from model string (e.g., "openai:gpt-4o"). routing_strategy is ignored. Dispatch uses direct deployment lookup.
- **full**: Both modes supported via mode discriminator

### 5. Key Storage Resolution

API key resolution at call time follows LiteLLM's priority order (with any-llm extensions):

```rust
// Key storage lookup supersedes embedded api_key
// This happens at dispatch time, NOT in to_provider_map()
//
// Priority order (matches LiteLLM with any-llm extensions):
// 1. key_storage for deployment_id (RFC-0903) — deployment-scoped lookup
// 2. Embedded api_key from LiteLLMParams — per-deployment fallback
//    NOTE: This is the RFC-0931-resolved value stored in DispatchInfo.api_key,
//    NOT the raw LiteLLMParams.api_key. RFC-0931 resolves os.environ["KEY"]
//    syntax and {PROVIDER}_API_KEY env vars at config load time (to_provider_map).
// 3. provider_key_storage — provider-scoped lookup (e.g., any-llm set_api_key("openai", key))
// 4. Environment variable (provider-specific, e.g., OPENAI_API_KEY)
let resolved_api_key = key_storage.get(&deployment.deployment_id)
    .map(String::from)
    .or_else(|| deployment.api_key.clone())  // RFC-0931-resolved value
    .or_else(|| provider_key_storage.get(&deployment.provider).cloned())
    .or_else(|| std::env::var(format!("{}_API_KEY", deployment.provider.to_uppercase())).ok());
```

The key storage (RFC-0903) is consulted at dispatch time. If an entry exists for the deployment_id, that key is used. Otherwise, the api_key from DispatchInfo is used — this is the RFC-0931-resolved value (os.environ["KEY"] syntax and {PROVIDER}_API_KEY env vars resolved at config load time). If neither exists, the any-llm `set_api_key()` persistent storage is checked. Finally, the provider-specific environment variable is checked (e.g., `OPENAI_API_KEY` for openai, `ANTHROPIC_API_KEY` for anthropic).

## API Change

### config.rs

Add to `crates/quota-router-core/src/config.rs`:

1. `DispatchInfo` struct with `#[derive(Debug, Clone, Serialize, Deserialize)]`
2. `DispatchInfo::auto_id()` impl
3. `to_provider_map()` implementation

**RouterSettings (RFC-0928) — add RateLimitMode:**

```rust
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

pub struct RouterSettings {
    // ... existing fields (from RFC-0928) ...
    // Note: routing_strategy_args inherited from RFC-0928 — contains cooldown_time_secs, allowed_fails, etc.

    /// Rate limit enforcement mode
    /// Default: Soft (matches LiteLLM default behavior)
    /// Note: Non-optional — RateLimitMode implements Default = Soft
    pub rate_limit_mode: RateLimitMode,
}
```

```rust
use std::collections::HashMap;

/// Convert GatewayConfig to dispatch map
///
/// # Algorithm
/// For each deployment in GatewayConfig.get_deployments():
/// 1. Generate deployment_id if not provided (auto_id: "{provider}_{model}" format)
/// 2. Extract from LiteLLMParams: provider, model, api_key, api_base
/// 3. Extract rpm, tpm from DeploymentConfig
/// 4. Extract model_group from model_info.model_group or litellm_params.model_group_alias
/// 5. Return HashMap<deployment_id, DispatchInfo>
/// 6. max_retries: falls back to RouterSettings.num_retries if litellm_params.max_retries is None AND router_settings is provided
///
/// # Errors
/// Returns ConfigError if GatewayConfig is malformed or deployment has invalid data.
pub fn to_provider_map(config: &GatewayConfig) -> Result<HashMap<String, DispatchInfo>, ConfigError> {
    let mut map = HashMap::new();
    for deployment in config.get_deployments() {
        let id = deployment.deployment_id.clone()
            .unwrap_or_else(|| DispatchInfo::auto_id(
                &deployment.litellm_params.provider,
                &deployment.litellm_params.model
            ));
        let info = DispatchInfo {
            deployment_id: id.clone(),
            provider: deployment.litellm_params.provider.clone(),
            model: deployment.litellm_params.model.clone(),
            api_key: deployment.litellm_params.api_key.clone(),
            api_base: deployment.litellm_params.api_base.clone(),  // per-deployment api_base (RFC-0927 LiteLLMParams.api_base verified)
            rpm: deployment.rpm,
            tpm: deployment.tpm,
            model_group: deployment.model_info.as_ref()
                .and_then(|m| m.model_group.clone())
                .or_else(|| deployment.litellm_params.model_group_alias.clone()),
            metadata: deployment.metadata.clone(),
            max_retries: deployment.litellm_params.max_retries
                .or_else(|| config.router_settings.as_ref().map(|s| s.num_retries)),
        };
        map.insert(id, info);
    }
    Ok(map)
}
```

## Performance Targets

| Metric | Target | Notes |
|--------|--------|-------|
| to_provider_map() | <1ms | For 100 deployments |
| Deployment lookup | O(1) | HashMap by deployment_id |

## Security Considerations

- **API keys**: Key storage (RFC-0903) is consulted at dispatch time. If key storage has an entry for `deployment_id`, that key is used. Otherwise, the embedded `api_key` from LiteLLMParams is used as fallback. This supersedes the embedded key when both are present.
- **Logging**: Never log api_key field.
- **api_key in completion call**: py_bridge implementation must ensure api_key is not logged. Config layer passes api_key through but cannot enforce py_bridge behavior — py_bridge owns that responsibility.
- **api_base**: Per-deployment, stored in DispatchInfo.api_base. any-llm-mode: passed via `py_bridge::factory::completion(api_base)` → `Provider.with_api_base()` (TARGET API — requires factory signature change). litellm-mode: stored in `Provider.endpoint` but NOT forwarded to `HttpProviderFactory::create(name)` — api_base is dropped; provider uses hardcoded default. See §Implementation Requirements for litellm-mode gap. **Never log api_base field.**
- **model_group**: Used only for routing selection, not for access control.
- **rpm/tpm**: Router may enforce these limits before dispatch if rpm > 0 or tpm > 0. If not enforced by router, provider-side enforcement applies. These fields are informational for observability when not enforced.

## Implementation Requirements (litellm-mode Gap)

The following changes are required to support per-deployment api_base in litellm-mode (reqwest path):

1. `HttpProviderFactory::create(name: &str, api_base: Option<&str>)` — accept optional api_base parameter
2. Pass api_base via `HttpCompletionRequest` or as separate parameter to `provider.completion()`
3. Provider's `completion()` method uses the passed api_base instead of hardcoded `self.api_base`

**Status:** These are implementation tasks, not design decisions. The RFC specifies the target behavior; implementation is deferred to the engineering team.

## Known Gaps

1. **litellm-mode api_base gap:** `HttpProviderFactory::create()` doesn't accept per-deployment api_base — see §Implementation Requirements
2. **py_bridge factory signature change:** `completion()` needs `api_base` parameter — see §Implementation Requirements
3. **RFC-0930/0931 integration:** This RFC's §API Change shows raw LiteLLMParams values; RFC-0930/0931 supersede with resolved values (infer_provider, resolve_api_key, resolve_api_base)

## Test Vectors

**Note:** `parse_config()` in these test vectors is illustrative pseudocode — actual implementation depends on the serialization format chosen for GatewayConfig. The test structure shows what to verify, not how to parse.

```rust
#[test]
fn test_to_provider_map_explicit_id() {
    // Explicit deployment_id is preserved
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
    assert!(map.contains_key("openai-gpt4o"));  // Explicit preserved
}

#[test]
fn test_to_provider_map_api_key() {
    // api_key preserved from litellm_params
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
fn test_to_provider_map_auto_id() {
    // Auto-generated deployment_id: "{provider}_{model}"
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
    assert!(map.contains_key("openai_gpt-4o"));  // Auto-generated with underscore
}

#[test]
fn test_to_provider_map_model_group() {
    // model_group from model_info
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
fn test_to_provider_map_api_base() {
    // api_base preserved from litellm_params (per-deployment custom endpoint)
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
    assert_eq!(info.api_base, Some("https://openai-gpt-4-test.openai.azure.com/".to_string()));
    assert_eq!(info.provider, "azure");
}

#[test]
fn test_to_provider_map_model_group_case_insensitive() {
    // model_group matching is case-insensitive
    // This test verifies to_provider_map() stores the value as-is (case preserved).
    // Case-insensitive matching happens at routing layer, not in to_provider_map().
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
    // Value is stored as-is (case preserved)
    assert_eq!(info.model_group, Some("GPT-4-FAMILY".to_string()));
}

#[test]
fn test_to_provider_map_empty() {
    // Empty deployments returns empty map
    let yaml = "deployments: []";
    let config = parse_config(yaml).unwrap();
    let map = to_provider_map(&config).unwrap();
    assert!(map.is_empty());
}

#[test]
fn test_dispatch_info_auto_id_empty_errors() {
    // auto_id returns Err on empty provider or model
    assert!(DispatchInfo::auto_id("", "gpt-4o").is_err());
    assert!(DispatchInfo::auto_id("openai", "").is_err());
    assert!(DispatchInfo::auto_id("", "").is_err());
}

#[test]
fn test_dispatch_info_auto_id() {
    assert_eq!(DispatchInfo::auto_id("openai", "gpt-4o").unwrap(), "openai_gpt-4o");
    assert_eq!(DispatchInfo::auto_id("anthropic", "claude-3-opus").unwrap(), "anthropic_claude-3-opus");
}

#[test]
fn test_to_provider_map_max_retries_fallback() {
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
    assert_eq!(info.max_retries, Some(5));  // Falls back to RouterSettings.num_retries
}

#[test]
fn test_to_provider_map_max_retries_no_router_settings() {
    // When router_settings is None, max_retries stays None (no default applies)
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
    assert_eq!(info.max_retries, None);  // No router_settings, no default
}

#[test]
fn test_to_provider_map_max_retries_litellm_takes_precedence() {
    // When both litellm_params.max_retries and router_settings.num_retries are set,
    // litellm_params.max_retries takes precedence (no fallback)
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
    assert_eq!(info.max_retries, Some(3));  // litellm_params.max_retries takes precedence
}

#[test]
fn test_to_provider_map_model_group_precedence() {
    // When both model_info.model_group and litellm_params.model_group_alias are set,
    // model_info.model_group takes precedence (first non-None wins)
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
    assert_eq!(info.model_group, Some("group-primary".to_string()));  // model_info takes precedence
}

#[test]
fn test_to_provider_map_api_base_with_model_info() {
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
```

## Version History

| Version | Date | Changes |
|---------|------|---------|
| Post-Accept R1 | 2026-05-15 | Adversarial review R1 fixes (cross-RFC consistency): C3 — key resolution §5 clarified that deployment.api_key is RFC-0931-resolved value, not raw LiteLLMParams; M3 — metadata field annotated with RFC-0928 type dependency; M5 — api_base resolution cross-referenced to RFC-0931; m1 — from_litellm_str() deprecated in favor of existing FromStr impl; m4 — auto_id() returns Result<String, ConfigError> instead of panicking; m6 — "Open Questions" renamed to "Known Gaps" with 3 documented gaps; test vectors updated for auto_id Result return |
| Accept | 2026-05-14 | Accepted after 12-round adversarial review — zero critical/major/minor issues; all round fixes verified; comprehensive test coverage (14 tests) |
| 17 | 2026-05-13 | Round 5 review fixes: C1 — version header corrected from v15 to v16
| 16 | 2026-05-13 | Round 4 review fixes: C1 — DispatchInfo api_base doc comment updated (litellm-mode NOT supported, any-llm-mode supported — matches gap analysis); C2 — any-llm-mode integration example corrected to show current 4-arg factory signature (api_base requires factory signature change); m1 — version history corrected to reflect both Security Considerations AND DispatchInfo comment fixes; m2 — any_llm_key_storage replaced with provider_key_storage, comment updated to reference RFC-0903 |
| 15 | 2026-05-13 | Round 3 review fixes: C1 — fixed Security Considerations api_base claim (now matches gap analysis: litellm-mode NOT supported, any-llm-mode supported); C2 — separated integration examples by mode (any-llm-mode and litellm-mode shown separately); C3 — RateLimitMode now uses `#[derive(Default)]` on enum with `#[default]` attribute, field changed from `Option<RateLimitMode>` to non-optional `RateLimitMode`; M1 — added §Implementation Requirements section for litellm-mode gap; M2 — api_base added to "never log" list in Security Considerations; m1 — "Open Questions" expanded to "Implementation Requirements (litellm-mode Gap)" with 3 required changes; m2 — added parse_config() pseudocode note to test vectors |
| 14 | 2026-05-13 | api_base per-deployment spec — added to DispatchInfo, to_provider_map(), factory signature; mode-specific handling (litellm-mode reqwest, any-llm-mode PyO3); removed v1 Limitation + Future versions hint (per deferred-vs-unspecified memory rule); **detailed gap analysis**: litellm-mode has implementation gap (HttpProviderFactory.create doesn't pass api_base, Provider.endpoint not forwarded) |
| 12 | 2026-05-13 | External adversarial review fixes: C1 — rpm/tpm now "router may enforce"; C2 — added any-llm mode-specific dispatch (explicit provider, no routing_strategy); C3 — clarified api_base must be set via env/provider init; M4 — added RoutingStrategy::from_litellm_str(); M5 — added env var fallback to key resolution (3-level hierarchy); M6 — model_group checks both model_info.model_group and litellm_params.model_group_alias; M7 — added max_retries to DispatchInfo; L8 — documented auto_id format vs LiteLLM; L9 — documented ConfigError::NoDeploymentFound for empty candidates; L10 — documented feature gate behavior in mode-specific dispatch |
| 9 | 2026-05-13 | Eighth review fixes: M1 — defined requested_group source in model_group matching algorithm; L1 — added test_to_provider_map_model_group_case_insensitive for case-insensitivity coverage |
| 8 | 2026-05-13 | Seventh review fixes: M1 — clarified G4 metric to "rpm, tpm, api_key, model_group, and custom metadata tags"; L1 — removed misleading "second call" comment from panic test |
| 7 | 2026-05-13 | Sixth review fixes: C1 — added HashMap import note to metadata field; H1 — corrected to_provider_map return to Err(ConfigError::NotYetSpecified(...)); M1 — model_group matching is case-insensitive; M2 — replaced panic catch_unwind with #[should_panic]; L1 — removed resolved api_base open question; L2 — clarified v6 changes |
| 6 | 2026-05-13 | Fifth review fixes: C1/H1/H2 — removed stale api_base references from algorithm steps in §2 and §API Change; version history corrected |
| 5 | 2026-05-13 | Fourth review fixes: C1 — removed api_base from DispatchInfo (cannot reach completion()); H1 — added rpm/tpm "informational only" note; H2 — added key storage supersession mechanism with code; M1 — removed api_base field and test; M2 — clarified routing_strategy applies to filtered candidate set; M3 — consistent key storage supersession in completion call; L1 — auto_id panics on empty provider/model; L2 — removed deprecated api_base references |
| 4 | 2026-05-13 | Third review fixes: C1 — corrected py_bridge signature to 4 args (provider, model, messages, api_key); C2 — added std::collections::HashMap import note to DispatchInfo; H1 — clarified key storage lookup happens at call time; H2 — added mode-agnostic note; M1 — added model_group matching algorithm; M2 — resolved api_base open question (provider config, not completion param); M3 — added api_key test vector |
| 3 | 2026-05-13 | Fix C2: clarify api_base propagation path to completion(); Fix M1: document metadata usage; Fix M3: add full to_provider_map() code block; Fix L1: replace unwrap() with ok_or error handling; Fix L2: clarify py_bridge owns api_key logging enforcement; Fix L3: remove RFC-0920 from Required by (thin binding delegates to RFC-0917) |
| 2 | 2026-05-13 | Fix C1: clarify return type (DispatchInfo not LiteLLMProviderConfig); Fix M1: ProviderType vs String; Fix M2: add api_base to DispatchInfo; Fix M4: clarify model source; Fix M5: auto_id underscore format; Add test vectors; Fix Open Questions; Fix version history prepend |
| 1 | 2026-05-13 | Initial draft |