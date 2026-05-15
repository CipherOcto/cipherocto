# RFC-0930: Provider Inference from Model String

## Status

Draft

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
        return Some(provider.to_lowercase());
    }
    if let Some((provider, _)) = model.split_once(':') {
        return Some(provider.to_lowercase());
    }
    None
}
```

**Note:** Provider name is lowercased because factory.rs provider lookup is case-sensitive lowercase. Input like `OpenAI/gpt-4o` or `OPENAI/gpt-4o` both infer `openai`.

### 2. Source of Model String for Inference

**`model_name` is the source for provider inference** (not `litellm_params.model`).

The `model_name` field carries the full model identifier as submitted in requests — it may include a provider prefix. The `litellm_params.model` field is the deployment-specific model name (without provider prefix) used for API calls.

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
| azure | — | **No default.** Azure requires explicit `api_base` or `azure_resource_name` + `azure_api_base` in config. See §Azure Special Case. |
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

**Configuration options for Azure:**

Option A (explicit api_base):
```yaml
deployments:
  - model_name: azure/gpt-4o
    litellm_params:
      model: gpt-4o
      api_base: https://my-resource.openai.azure.com/v1
```

Option B (resource name + base template):
```yaml
deployments:
  - model_name: azure/gpt-4o
    litellm_params:
      model: gpt-4o
      extra:  # Azure-specific config via litellm_params.extra
        azure_resource_name: my-resource
        azure_api_base: https://{resource}.openai.azure.com/v1
```

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

        // Resolve api_base (steps 4-6) — 4-tier from RFC-0931
        let api_base = deployment.litellm_params.resolve_api_base();

        let info = DispatchInfo {
            deployment_id: deployment.deployment_id.clone()
                .unwrap_or_else(|| format!("{}_{}", provider, deployment.model_name)),
            provider: provider.clone(),
            model: deployment.litellm_params.model.clone(),
            api_key: deployment.litellm_params.resolve_api_key(),  // RFC-0931
            api_base,  // Already resolved via RFC-0931 4-tier resolution
            rpm: deployment.rpm,
            tpm: deployment.tpm,
            model_group: deployment.model_info.as_ref().and_then(|m| m.model_group.clone()),
            metadata: deployment.metadata.clone(),
        };
        map.insert(info.deployment_id.clone(), info);
    }
    Ok(map)
}
```

### 6. Feature Gate Scope

**This RFC applies to `any-llm-mode` and `full` only.**

`to_provider_map()` is conditionally compiled with `#[cfg(any(feature = "any-llm-mode", feature = "full"))]`. In litellm-mode, the dispatch path uses `HttpProviderFactory` which handles provider dispatch explicitly — no inference needed.

The feature gate means `to_provider_map()` does not exist in litellm-mode builds. Callers in shared code must be feature-gated accordingly.

## Implementation

### Files to Modify

| File | Change |
|------|--------|
| `crates/quota-router-core/src/config.rs` | Add `infer_provider()` returning `Option<String>`, add `get_provider_default_api_base()` registry, update `to_provider_map()` with integration code |
| `crates/quota-router-core/src/py_bridge/factory.rs` | Add `get_provider_default_api_base()` function |

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
    assert_eq!(infer_provider("/gpt-4o"), Some("".to_string()));  // Empty provider name
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
    // /gpt-4o has no provider prefix, returns empty string from infer_provider
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
- [ ] `to_provider_map()` populates `DispatchInfo.api_base` with resolved 4-tier value
- [ ] Azure has no default api_base — requires explicit config
- [ ] `get_provider_default_api_base()` returns correct defaults for known providers
- [ ] Feature-gated to `any-llm-mode` and `full` only
- [ ] Existing tests still pass
- [ ] Clippy clean