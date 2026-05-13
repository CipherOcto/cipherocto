# RFC-0929: GatewayConfig Provider Dispatch Mapping

## Status

Draft v2

## Authors

- Author: @cipherocto

## Maintainers

- Maintainer: @cipherocto

## Summary

Specify how `GatewayConfig.deployments` (RFC-0928) maps to `py_bridge::factory::completion()` calls, replacing the `NotYetSpecified` stub in `to_provider_map()`. This completes the integration chain: `GatewayConfig` → dispatch map → provider calls.

## Why Needed

**Current state:**
- RFC-0928 defines `DeploymentConfig` with `litellm_params` (provider, model, api_key, etc.)
- `to_provider_map()` returns `NotYetSpecified` — no actual mapping exists
- `py_bridge::factory::completion(provider, model, messages, api_key)` exists but is not wired from config

**Impact:** The project cannot route requests from GatewayConfig — the data structures exist but the integration is missing.

## Dependencies

**Requires:**
- RFC-0917: Dual-Mode Query Router (py_bridge::factory::completion)
- RFC-0927: RouterConfig Extension for LiteLLM Compatibility (LiteLLMParams)
- RFC-0928: Deployment Configuration Schema (GatewayConfig, DeploymentConfig)

**Required by:**
- RFC-0920: Unified Python SDK (uses GatewayConfig for routing)

## Design Goals

| Goal | Target | Metric |
|------|--------|--------|
| G1 | Complete integration chain | GatewayConfig → dispatch → provider call |
| G2 | Auto-generate deployment_id | When not provided, use `{provider}_{model}` |
| G3 | Support model_group routing | Multiple deployments with same group routed together |
| G4 | Preserve all deployment metadata | rpm, tpm, api_key, model_info, api_base |

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
    /// API base URL (optional — resolved from LiteLLMParams.api_base or base_url)
    pub api_base: Option<String>,
    /// API key for this deployment (optional — may come from key storage)
    /// SECURITY: Do not log this field
    pub api_key: Option<String>,
    /// Requests per minute limit (0 = unlimited)
    pub rpm: u32,
    /// Tokens per minute limit (0 = unlimited)
    pub tpm: u64,
    /// Model group for routing (multiple deployments with same group routed together)
    pub model_group: Option<String>,
    /// Per-deployment custom metadata
    pub metadata: Option<HashMap<String, String>>,
}

impl DispatchInfo {
    /// Auto-generate deployment_id from provider and model
    /// Format: "{provider}_{model}" with underscores
    pub fn auto_id(provider: &str, model: &str) -> String {
        format!("{}_{}", provider, model)
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
/// 2. Extract from LiteLLMParams: provider, model, api_base (via resolve_api_base()), api_key
/// 3. Extract rpm, tpm from DeploymentConfig
/// 4. Extract model_group from model_info.model_group
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
///       api_base: https://api.openai.com/v1
/// ```
/// Result:
/// ```rust
/// DispatchInfo {
///     deployment_id: "openai-gpt4o",
///     provider: "openai",
///     model: "gpt-4o",
///     api_base: Some("https://api.openai.com/v1"),
///     api_key: None,
///     rpm: 1000,
///     tpm: 100000,
///     model_group: None,
///     metadata: None,
/// }
/// ```
pub fn to_provider_map(config: &GatewayConfig) -> Result<HashMap<String, DispatchInfo>, ConfigError>;
```

### 3. Integration with py_bridge

The dispatch map feeds into existing `py_bridge::factory::completion()`:

```rust
// Request routing example:
// SECURITY: api_key should come from key storage, not embedded config.
// Only use embedded api_key if key storage lookup fails.
let dispatch_map = to_provider_map(&config)?;
let deployment = dispatch_map.get("openai-gpt4o").unwrap();

// Resolve api_base if configured
if let Some(api_base) = &deployment.api_base {
    // Configure provider with custom api_base before calling completion
}

// Call py_bridge factory — provider and model are strings
py_bridge::factory::completion(
    &deployment.provider,
    &deployment.model,
    &messages,
    deployment.api_key.as_deref(),  // None = use key storage
)
```

### 4. Routing Strategy Selection

Routing strategy is defined in RFC-0927's `RouterSettings.routing_strategy`. The implementation of strategy selection (e.g., `shuffle_select`, `round_robin_select`) is deferred to the implementation layer — this RFC focuses on the config-to-dispatch mapping.

**Selection point:** When multiple deployments share a model_group, the router selects one based on `router_settings.routing_strategy`. The strategy functions are implementation-defined per RFC-0927.

### 5. API Base Resolution

`LiteLLMParams::resolve_api_base()` (from RFC-0927) handles api_base resolution:

```rust
// From LiteLLMParams (RFC-0927):
pub fn resolve_api_base(&self) -> Option<&str> {
    self.api_base.as_deref().or(self.base_url.as_deref())
}

// In to_provider_map():
let api_base = deployment.litellm_params.resolve_api_base().map(String::from);
```

## API Change

### config.rs

Add to `crates/quota-router-core/src/config.rs`:

1. `DispatchInfo` struct with `#[derive(Debug, Clone, Serialize, Deserialize)]`
2. `DispatchInfo::auto_id()` impl
3. `to_provider_map(config: &GatewayConfig) -> Result<HashMap<String, DispatchInfo>, ConfigError>`

## Performance Targets

| Metric | Target | Notes |
|--------|--------|-------|
| to_provider_map() | <1ms | For 100 deployments |
| Deployment lookup | O(1) | HashMap by deployment_id |

## Security Considerations

- **API keys**: Should come from key storage (RFC-0903) by default. Only use embedded `api_key` if key storage lookup fails.
- **Logging**: Never log api_key field. When passing to py_bridge, ensure factory does not log the api_key parameter.
- **model_group**: Used only for routing selection, not for access control.

## Open Questions

| Question | Resolution |
|----------|------------|
| How does api_base configure the provider before completion call? | Provider-specific implementation (use api_base to set custom endpoint) |
| What happens if deployment has neither embedded api_key nor key storage entry? | Return error — cannot make call without credentials |

## Test Vectors

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
    // api_base resolved from litellm_params
    let yaml = r#"
deployments:
  - model_name: gpt-4o
    litellm_params:
      provider: openai
      model: gpt-4o
      api_base: https://custom.openai.com/v1
    rpm: 1000
    tpm: 100000
"#;
    let config = parse_config(yaml).unwrap();
    let map = to_provider_map(&config).unwrap();
    let info = map.get("openai_gpt-4o").unwrap();
    assert_eq!(info.api_base, Some("https://custom.openai.com/v1".to_string()));
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
fn test_dispatch_info_auto_id() {
    assert_eq!(DispatchInfo::auto_id("openai", "gpt-4o"), "openai_gpt-4o");
    assert_eq!(DispatchInfo::auto_id("anthropic", "claude-3-opus"), "anthropic_claude-3-opus");
}
```

## Version History

| Version | Date | Changes |
|---------|------|---------|
| 2 | 2026-05-13 | Fix C1: clarify return type (DispatchInfo not LiteLLMProviderConfig); Fix M1: ProviderType vs String; Fix M2: add api_base to DispatchInfo; Fix M4: clarify model source; Fix M5: auto_id underscore format; Add test vectors; Fix Open Questions; Fix version history prepend |
| 1 | 2026-05-13 | Initial draft |