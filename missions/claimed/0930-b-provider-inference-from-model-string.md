# Mission: 0930-b — Provider Inference from Model String

## Status

Complete (implemented as part of Mission 0930-a)

## RFC

RFC-0930 (Economics): Provider Inference from Model String

## Dependencies

- Mission-0930-a: Provider Registry Expansion (provides `get_provider_default_api_base()`)
- Mission-0928-a: Deployment Config Schema (provides `DeploymentConfig`, `LiteLLMParams`)

## Context

RFC-0930 specifies how `to_provider_map()` infers provider from model string prefix when `litellm_params.provider` is omitted. This enables LiteLLM-compatible drop-in where model string carries provider info (e.g., `openai/gpt-4o` → provider=openai).

Currently, `to_provider_map()` requires explicit `litellm_params.provider` field. The `ParsedModel::parse()` function already extracts provider from model string, but `to_provider_map()` doesn't use this inference.

## Acceptance Criteria

### infer_provider() Function

- [x] Implement `infer_provider(model: &str) -> Option<String>` in `config.rs` (line 170)
- [x] Split on `/` separator: `openai/gpt-4o` → `Some("openai")`
- [x] Split on `:` separator: `openai:gpt-4o` → `Some("openai")`
- [x] Lowercase provider name (factory.rs lookup is case-sensitive)
- [x] Return `None` for empty provider (e.g., `/gpt-4o` or `:gpt-4o`)
- [x] Return `None` for bare model names (e.g., `gpt-4o`)

### to_provider_map() Integration

- [x] Update `to_provider_map()` to use `infer_provider()` when `litellm_params.provider` is empty
- [x] Resolution order:
  1. If `litellm_params.provider` is set → use it
  2. If `litellm_params.provider` is empty → try `infer_provider(model_name)`
  3. If `model_name` has no prefix and provider is empty → return `ConfigError::MissingProvider`
- [x] Set inferred provider on params before calling `resolve_api_base()` (for tier 3-4 resolution)
- [x] Use `deployment.litellm_params.model` (not `model_name`) for `auto_id()` to avoid double prefix

### ConfigError::MissingProvider (owned by Mission-0930-a)

- [x] Reference `ConfigError::MissingProvider(String)` added by Mission-0930-a
- [x] Return from `to_provider_map()` when provider cannot be determined
- [x] Error message includes the model_name that failed inference

> **Ownership:** `ConfigError::MissingProvider` is defined by Mission-0930-a. This mission uses it but does NOT add it.

### Tests

- [x] `infer_provider("openai/gpt-4o")` → `Some("openai")` (test_infer_provider_slash_format)
- [x] `infer_provider("azure:gpt-4o")` → `Some("azure")` (test_infer_provider_colon_format)
- [x] `infer_provider("OpenAI/gpt-4o")` → `Some("openai")` (test_infer_provider_case_insensitive)
- [x] `infer_provider("/gpt-4o")` → `None` (test_infer_provider_empty_prefix)
- [x] `infer_provider("gpt-4o")` → `None` (test_infer_provider_no_prefix)
- [x] `to_provider_map()` with empty provider + prefixed model_name → success (test_to_provider_map_inferred_provider)
- [x] `to_provider_map()` with empty provider + bare model_name → `MissingProvider` error (test_missing_provider_error_from_to_provider_map)
- [x] Inferred provider used for api_base tier 3-4 resolution (to_provider_map uses get_provider_default_api_base)

## Key Files

- `crates/quota-router-core/src/config.rs` — `infer_provider()`, `to_provider_map()`, `ConfigError`
- `crates/quota-router-core/src/py_bridge/factory.rs` — provider list reference

## Notes

This mission is separate from Mission-0930-a (registry expansion) because:
- Mission-0930-a creates `get_provider_default_api_base()` and adds `MissingProvider` variant
- This mission implements `infer_provider()` and wires it into `to_provider_map()`

Both missions are needed for RFC-0930 full implementation. Mission-0930-a should be completed first (provides `get_provider_default_api_base()` needed by RFC-0931's tier 4).

**model_name vs litellm_params.model:** `model_name` is used for inference (may have prefix), `litellm_params.model` is used for API calls and auto_id (no prefix). Example:
```yaml
model_name: openai/gpt-4o    # ← used for inference
litellm_params:
  model: gpt-4o              # ← used for API calls and auto_id
```
