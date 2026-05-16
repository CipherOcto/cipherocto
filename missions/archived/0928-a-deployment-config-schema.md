# Mission: 0928-a — Deployment Configuration Schema Implementation

## Status

Claimed

## RFC

RFC-0928 (Economics): Deployment Configuration Schema

## Dependencies

- RFC-0917: Dual-Mode Query Router (base provider model)
- RFC-0927: RouterConfig Extension for LiteLLM Compatibility (types defined here)
- **Note:** RoutingStrategy enum already exists in `router.rs` — do NOT redefine

## Context

Mission-0927-a (RFC-0927 RouterConfig Extension) was marked complete in task list, but the types it was supposed to define (`RoutingStrategyArgs`, `LiteLLMParams`) were **not present in codebase**. This mission includes those types AND the RFC-0928 types.

**Already exists:**
- `RoutingStrategy` enum in `router.rs` (correctly implemented)

**Implemented (from RFC-0927):**
- `RoutingStrategyArgs`
- `LiteLLMParams`
- `LatencyRoutingSettings`
- `RateLimitMode` (from RFC-0929)

**Implemented (from RFC-0928):**
- `DeploymentConfig`
- `RouterSettings`
- `LiteLLMSettings`
- `ModelInfo`
- `PricingConfig`
- `GatewayConfig`
- `AnyLlmProviderConfig`
- `parse_config()` / `load_config()`
- `to_provider_map()` → returns `Result<HashMap<String, DispatchInfo>, ConfigError>`

## Acceptance Criteria

### RFC-0927 Types

- [x] `RoutingStrategyArgs` struct with all fields per RFC-0927 §RoutingStrategyArgs
- [x] `LiteLLMParams` struct with all fields per RFC-0927 §LiteLLMParams, including `api_base`, `base_url` aliases, `model_group_alias`
- [x] `LiteLLMParams::resolve_api_base()` method (returns api_base or base_url)
- [x] `LatencyRoutingSettings` struct per RFC-0927 §LatencyRoutingSettings
- [x] `RateLimitMode` enum (Soft default, Hard) — required by RFC-0929

### RFC-0928 Types

- [x] `DeploymentConfig` with serde aliases: `#[serde(alias = "id")]`, `#[serde(alias = "requests_per_minute")]`, `#[serde(alias = "tokens_per_minute")]`
- [x] `RouterSettings` with redis_host, redis_port, redis_password, stream_timeout_secs, rate_limit_mode — with impl Default
- [x] `LiteLLMSettings` with set_google_vertex_ai — with #[derive(Default)]
- [x] `ModelInfo` with model_group (alias: "group"), supports_embeddings (alias: "embeddings")
- [x] `PricingConfig` with optional per-million and per-second pricing fields
- [x] `GatewayConfig` with Optional<pricing>, providers/AnyLlmProviderConfig, deployments, router_settings, litellm_settings
- [x] `AnyLlmProviderConfig` struct
- [x] `GatewayConfig::get_deployments()` method — returns deployments if non-empty, else model_list_alias, else empty slice
- [x] `parse_config(yaml: &str) -> Result<GatewayConfig, ConfigError>` ✓ (serde_yaml added, fully implemented)
- [x] `load_config(path: &Path) -> Result<GatewayConfig, ConfigError>`
- [x] `to_provider_map() -> Result<HashMap<String, DispatchInfo>, ConfigError>`

### RFC-0929 DispatchInfo

- [x] `DispatchInfo` struct with fields: deployment_id, provider, model, api_key, api_base, rpm, tpm, model_group, metadata, max_retries
- [x] `DispatchInfo::auto_id(provider, model) -> String` with "{provider}_{model}" format
- [x] All types derive(Debug, Clone, Serialize, Deserialize)

### Testing

- [x] Unit tests for DispatchInfo::auto_id, GatewayConfig::get_deployments, to_provider_map
- [x] cargo clippy -D warnings passes
- [x] cargo test --lib passes

## Implementation Notes

**RoutingStrategy imported from router.rs:** The enum already exists with correct serde rename_all = "kebab-case". Do not duplicate.

**RouterSettings.rate_limit_mode:** Non-optional RateLimitMode with Default = Soft (per RFC-0929 §API Change).