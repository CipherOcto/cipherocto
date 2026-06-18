# RFC-0930: Provider Inference from Model String

## Status

Accepted

## Summary

Specify how `to_provider_map()` infers provider from model string prefix (e.g., `openai/gpt-4o` → provider=`openai`) when `litellm_params.provider` is omitted. This enables LiteLLM-compatible drop-in where model string carries provider info.

## Problem Statement

LiteLLM allows omitting explicit `provider` field — it infers provider from model string prefix:
- `openai/gpt-4o` → provider=openai
- `azure/gpt-4o` → provider=azure (with azure-specific api_base)
- `anthropic/claude-3-opus` → provider=anthropic

Current CipherOcto implementation requires explicit `litellm_params.provider` field. The `ParsedModel::parse()` function already extracts provider from model string, but `to_provider_map()` doesn't use this inference.

## Specification

### 1. Provider Inference Logic

When `LiteLLMParams.provider` is empty or missing, infer provider from model string:

```rust
/// Infer provider from model string prefix
/// Returns lowercase provider name, or None if no recognized prefix
pub fn infer_provider(model: &str) -> Option<String> {
    // Patterns: "provider/model" or "provider:model"
    // Provider name is lowercased to match factory.rs match arms
    if let Some((provider, _)) = model.split_once('/') {
        let provider = provider.to_lowercase();
        if provider.is_empty() {
            return None;  // Leading slash with no provider prefix (e.g., "/gpt-4o")
        }
        return Some(provider);
    }
    if let Some((provider, _)) = model.split_once(':') {
        let provider = provider.to_lowercase();
        if provider.is_empty() {
            return None;  // Leading colon with no provider prefix
        }
        return Some(provider);
    }
    None
}
```

**Note:** Provider name is lowercased because factory.rs provider lookup is case-sensitive lowercase. Input like `OpenAI/gpt-4o` or `OPENAI/gpt-4o` both infer `openai`.

### 2. Source of Model String for Inference

**`model_name` is the source for provider inference** (not `litellm_params.model`).

The `model_name` field carries the full model identifier as submitted in requests — it may include a provider prefix. The `litellm_params.model` field is the deployment-specific model name (without provider prefix) used for API calls.

**Requirement:** `model_name` MUST be present on `DeploymentConfig` for provider inference to work. If `model_name` is absent and `litellm_params.provider` is also empty, `to_provider_map()` returns `ConfigError::MissingProvider`. There is no fallback to `litellm_params.model` for inference — `litellm_params.model` is used for API calls and auto_id, not for provider inference.

Example YAML:
```yaml
model_name: openai/gpt-4o    # ← used for inference (may have prefix)
litellm_params:
  model: gpt-4o              # ← API model name (no prefix)
```

When `model_name` has a provider prefix, `infer_provider(model_name)` extracts it. When `model_name` lacks a prefix and `litellm_params.provider` is also empty, `to_provider_map()` returns `MissingProvider`.

### 3. API Base Defaults and Registry

#### 3.1 Per-Provider Default api_base Values

When `api_base` is not explicitly set, use provider-specific default:

| Provider | Default api_base | Notes |
|----------|------------------|-------|
| openai | `https://api.openai.com/v1` | |
| anthropic | `https://api.anthropic.com` | |
| mistral | `https://api.mistral.ai/v1` | |
| gemini | `https://generativelanguage.googleapis.com` | |
| cohere | `https://api.cohere.ai` | |
| voyage | `https://api.voyageai.com/v1` | |
| azure | — | **No default.** Azure requires explicit `api_base` in config. See §Azure Special Case. |
| *other* | None | No default available — returns `None`. Caller must handle. |

**`None` return ambiguity:** `get_provider_default_api_base()` returns `None` for both "unknown provider" and "known provider with no default". Callers must treat `None` uniformly: "no default available".

#### 3.2 API Base Registry Function

```rust
/// Returns default api_base for provider, or None if no default
pub fn get_provider_default_api_base(provider: &str) -> Option<String> {
    match provider {
        "openai" => Some("https://api.openai.com/v1".to_string()),
        "anthropic" => Some("https://api.anthropic.com".to_string()),
        "mistral" => Some("https://api.mistral.ai/v1".to_string()),
        "gemini" => Some("https://generativelanguage.googleapis.com".to_string()),
        "cohere" => Some("https://api.cohere.ai".to_string()),
        "voyage" => Some("https://api.voyageai.com/v1".to_string()),
        // azure: No default — requires explicit config
        _ => None,
    }
}
```

This function is called by RFC-0931's `resolve_api_base()` as tier 4 when no explicit value or env var is found.

### 4. Azure Special Case

Azure's api_base follows the template: `https://{resource}.openai.azure.com/v1`

Unlike other providers, Azure cannot infer a default because:
- The `resource` name is deployment-specific
- It cannot be derived from model string

**Azure requires explicit `api_base` in config:**
```yaml
deployments:
  - model_name: azure/gpt-4o
    litellm_params:
      model: gpt-4o
      api_base: https://my-resource.openai.azure.com/v1
```

`get_provider_default_api_base("azure")` returns `None`. If `api_base` is not set, the deployment will have no api_base and the caller must handle the absent value.

### 5. Model String Parsing Integration

Update `to_provider_map()` to:

1. If `litellm_params.provider` is set → use it
2. If `litellm_params.provider` is empty → try `infer_provider(model_name)`
3. If `model_name` has no prefix and provider is empty → **return `ConfigError::MissingProvider`** — this deployment cannot be dispatched without explicit provider
4. If `litellm_params.api_base` is set → use it
5. If `api_base` is empty → consult provider-default api_base registry
6. If no default exists for provider → use `None` (caller handles absent api_base)

#### to_provider_map Integration

`to_provider_map()` populates `DispatchInfo` with resolved values:

```rust
pub fn to_provider_map(config: &GatewayConfig) -> Result<HashMap<String, DispatchInfo>, ConfigError> {
    let mut map = HashMap::new();
    for deployment in config.get_deployments() {
        // Resolve provider (steps 1-3)
        let provider = deployment.litellm_params.provider.clone()
            .filter(|p| !p.is_empty())
            .or_else(|| infer_provider(&deployment.model_name))
            .ok_or_else(|| ConfigError::MissingProvider(deployment.model_name.clone()))?;

        // Set inferred provider on litellm_params so resolve_api_base() can use it
        // for tiers 3 ({PROVIDER}_API_BASE) and 4 (RFC-0930 registry)
        let mut params = deployment.litellm_params.clone();
        if params.provider.is_empty() {
            params.provider = provider.clone();
        }

        // Resolve api_base (steps 4-6) — 4-tier from RFC-0931
        let api_base = params.resolve_api_base();

        let info = DispatchInfo {
            deployment_id: match deployment.deployment_id.clone() {
                Some(id) => id,
                None => DispatchInfo::auto_id(&provider, &deployment.litellm_params.model)?,
            },
            provider: provider.clone(),
            model: deployment.litellm_params.model.clone(),
            api_key: deployment.litellm_params.resolve_api_key(),  // RFC-0931
            api_base,  // Already resolved via RFC-0931 4-tier resolution
            rpm: deployment.rpm,
            tpm: deployment.tpm,
            model_group: deployment.model_info.as_ref()
                .and_then(|m| m.model_group.clone())
                .or_else(|| deployment.litellm_params.model_group_alias.clone()),  // RFC-0929 fallback
            max_retries: deployment.litellm_params.max_retries
                .or_else(|| config.router_settings.as_ref().map(|s| s.num_retries)),  // RFC-0929 fallback
            metadata: deployment.metadata.clone(),
        };
        map.insert(info.deployment_id.clone(), info);
    }
    Ok(map)
}
```

### 6. Feature Gate Scope

**This RFC applies to all modes.**

`to_provider_map()` is used by both litellm-mode and any-llm-mode (per RFC-0929 §4). The function is NOT feature-gated — it must be available in all builds. The `infer_provider()` helper and `get_provider_default_api_base()` registry are also available in all modes.

Note: `infer_provider()` returns `None` when provider is explicit (from config), so it has no effect when provider is already set. This is safe for litellm-mode where providers are typically explicit.

## Implementation

### Files to Modify

| File | Change |
|------|--------|
| `crates/quota-router-core/src/config.rs` | Add `infer_provider()` returning `Option<String>`, add `get_provider_default_api_base()` function and registry, add `ConfigError::MissingProvider` variant, update `to_provider_map()` with integration code |

### Tests

```rust
#[test]
fn test_infer_provider_from_model_with_slash() {
    assert_eq!(infer_provider("openai/gpt-4o"), Some("openai".to_string()));
    assert_eq!(infer_provider("Azure/gpt-4o"), Some("azure".to_string()));  // Case normalized
    assert_eq!(infer_provider("OPENAI/gpt-4o"), Some("openai".to_string()));
}

#[test]
fn test_infer_provider_from_model_with_colon() {
    assert_eq!(infer_provider("openai:gpt-4o"), Some("openai".to_string()));
    assert_eq!(infer_provider("Anthropic:claude-3-opus"), Some("anthropic".to_string()));
}

#[test]
fn test_infer_provider_unknown() {
    assert_eq!(infer_provider("gpt-4o"), None);
}

#[test]
fn test_infer_provider_empty_provider() {
    assert_eq!(infer_provider("/gpt-4o"), None);  // No provider prefix (leading slash only)
}

#[test]
fn test_to_provider_map_infers_provider_when_missing() {
    let yaml = r#"
deployments:
  - model_name: openai/gpt-4o
    litellm_params:
      model: gpt-4o
"#;
    let config = parse_config(yaml).unwrap();
    let map = to_provider_map(&config).unwrap();
    let info = map.get("openai_gpt-4o").unwrap();
    assert_eq!(info.provider, "openai");
}

#[test]
fn test_to_provider_map_missing_provider_and_prefix_errors() {
    let yaml = r#"
deployments:
  - model_name: gpt-4o
    litellm_params:
      model: gpt-4o
"#;
    let config = parse_config(yaml).unwrap();
    let result = to_provider_map(&config);
    assert!(matches!(result, Err(ConfigError::MissingProvider(_))));
}

#[test]
fn test_to_provider_map_empty_provider_from_split_errors() {
    // /gpt-4o has no provider prefix, infer_provider returns None
    let yaml = r#"
deployments:
  - model_name: /gpt-4o
    litellm_params:
      model: gpt-4o
"#;
    let config = parse_config(yaml).unwrap();
    let result = to_provider_map(&config);
    assert!(matches!(result, Err(ConfigError::MissingProvider(_))));
}

#[test]
fn test_azure_no_default_api_base() {
    assert_eq!(get_provider_default_api_base("azure"), None);
}

#[test]
fn test_provider_default_api_base_returns_correct_values() {
    assert_eq!(get_provider_default_api_base("openai"), Some("https://api.openai.com/v1".to_string()));
    assert_eq!(get_provider_default_api_base("anthropic"), Some("https://api.anthropic.com".to_string()));
    assert_eq!(get_provider_default_api_base("unknown"), None);
}
```

## Acceptance Criteria

- [ ] `infer_provider()` returns lowercase provider name
- [ ] `infer_provider()` normalizes `OpenAI/gpt-4o` → `openai`
- [ ] `to_provider_map()` uses `model_name` (not `litellm_params.model`) for inference
- [ ] `to_provider_map()` returns `MissingProvider` error when model has no prefix and provider is empty
- [ ] `to_provider_map()` populates `DispatchInfo.api_base` with resolved 4-tier value (RFC-0931 resolve_api_base)
- [ ] Azure has no default api_base — requires explicit config
- [ ] `get_provider_default_api_base()` returns correct defaults for known providers
- [ ] `infer_provider()` returns `None` for empty provider (leading slash/colon)
- [ ] `DispatchInfo.auto_id()` uses `litellm_params.model` (not `model_name`) for consistent IDs
- [ ] Available in all modes (NOT feature-gated)
- [ ] Existing tests still pass
- [ ] Clippy clean

## Dependencies

**Requires:**
- RFC-0929: GatewayConfig Provider Dispatch Mapping (defines DispatchInfo and to_provider_map)
- RFC-0927: RouterConfig Extension for LiteLLM Compatibility (LiteLLMParams struct)
- RFC-0928: Deployment Configuration Schema (GatewayConfig, DeploymentConfig)

**Required by:**
- RFC-0931: any-llm-mode Environment Variable Parity (uses registry for api_base defaults)
- RFC-0938: YAML Interpolation & Universal Key (completes runtime key resolution — {PROVIDER}_API_KEY and ANY_LLM_KEY resolved at dispatch time by RFC-0938, not by this RFC)

## Version History

| Version | Date | Changes |
|---------|------|---------|
| 2 | 2026-05-15 | Adversarial review R1 fixes: C2 — infer_provider returns None for empty provider (leading slash/colon); M1 — auto_id standardized to use litellm_params.model via DispatchInfo::auto_id(); M4 — Azure Option B removed, Azure requires explicit api_base; m2 — model_name requirement documented (must be present for inference, no fallback); m5 — test comments updated |
| 1 | 2026-05-14 | Initial draft |