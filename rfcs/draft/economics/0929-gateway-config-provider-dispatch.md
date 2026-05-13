# RFC-0929: GatewayConfig Provider Dispatch Mapping

## Status

Draft

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
| G4 | Preserve all deployment metadata | rpm, tpm, api_key, model_info |

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
    pub provider: String,
    /// Model name (e.g., "gpt-4o", "claude-3-opus")
    pub model: String,
    /// API key for this deployment (optional — may come from key storage)
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
/// 1. Generate deployment_id if not provided (provider_model format)
/// 2. Extract provider, model, api_key, rpm, tpm from LiteLLMParams and deployment
/// 3. Extract model_group from model_info
/// 4. Return HashMap<deployment_id, DispatchInfo>
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
/// ```
/// Result:
/// ```rust
/// DispatchInfo {
///     deployment_id: "openai-gpt4o",
///     provider: "openai",
///     model: "gpt-4o",
///     ...
/// }
/// ```
pub fn to_provider_map(config: &GatewayConfig) -> Result<HashMap<String, DispatchInfo>, ConfigError>;
```

### 3. Integration with py_bridge

The dispatch map feeds into existing `py_bridge::factory::completion()`:

```rust
// Request routing example:
let dispatch_map = to_provider_map(&config)?;
let deployment = dispatch_map.get("openai-gpt4o").unwrap();
py_bridge::factory::completion(
    &deployment.provider,
    &deployment.model,
    &messages,
    deployment.api_key.as_deref(),
)
```

### 4. Routing Strategy Support

GatewayConfig.router_settings.routing_strategy (from RFC-0927) provides strategy selection:

```rust
/// Select deployment based on routing strategy
fn select_deployment(
    candidates: &[&DispatchInfo],
    strategy: &RoutingStrategy,
    args: &RoutingStrategyArgs,
) -> &DispatchInfo {
    match strategy {
        RoutingStrategy::SimpleShuffle => shuffle_select(candidates),
        RoutingStrategy::RoundRobin => round_robin_select(candidates),
        RoutingStrategy::LeastBusy => least_busy_select(candidates),
        RoutingStrategy::LatencyBased => latency_based_select(candidates, args),
        RoutingStrategy::CostBased => cost_based_select(candidates),
        RoutingStrategy::UsageBased => usage_based_select(candidates),
        RoutingStrategy::UsageBasedV2 => usage_v2_select(candidates),
        RoutingStrategy::Weighted => weighted_select(candidates),
    }
}
```

### 5. LiteLLMParams api_base Resolution

`LiteLLMParams::resolve_api_base()` (already implemented in RFC-0927) handles api_base resolution:

```rust
// Set per-deployment API base when calling provider
if let Some(api_base) = deployment.litellm_params.resolve_api_base() {
    // Configure provider with custom api_base
}
```

## API Change

### config.rs

Add to `crates/quota-router-core/src/config.rs`:

1. `DispatchInfo` struct
2. `DispatchInfo::auto_id()` impl
3. `to_provider_map(config: &GatewayConfig) -> Result<HashMap<String, DispatchInfo>, ConfigError>`

## Performance Targets

| Metric | Target | Notes |
|--------|--------|-------|
| to_provider_map() | <1ms | For 100 deployments |
| Deployment lookup | O(1) | HashMap by deployment_id |

## Security Considerations

- API keys in DispatchInfo must not be logged
- Credentials resolved from env vars at parse time when possible
- model_group used only for routing, not access control

## Open Questions

None — this RFC fully specifies the missing integration.

## Version History

| Version | Date | Changes |
|---------|------|---------|
| 1 | 2026-05-13 | Initial draft |