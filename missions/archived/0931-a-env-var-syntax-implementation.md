# Mission: 0931-a — Env Var Syntax Implementation

## Status

Complete

## RFC

RFC-0931 (Economics): any-llm-mode Environment Variable Parity

## Dependencies

- Mission-0930-a: Provider Registry Expansion (provides get_provider_default_api_base() for Tier 4)

## Context

RFC-0931 specifies `os.environ["KEY"]` syntax for env var resolution in config. This is not yet implemented. The `resolve_api_key()` and `resolve_api_base()` methods on `LiteLLMParams` need to support this syntax.

## Acceptance Criteria

### os.environ["KEY"] Syntax

- [x] Parse `os.environ["KEY"]` and `os.environ['KEY']` syntax
- [x] Extract env var name from brackets
- [x] Resolve from `std::env::var()`
- [x] Return `None` if env var not set

### resolve_api_key()

- [x] Tier 1: Explicit non-empty non-os.environ value
- [x] Tier 2: `os.environ["KEY"]` syntax
- [x] Return `None` if both tiers fail

**Note:** `{PROVIDER}_API_KEY` env var is NOT resolved at config time. It is resolved at runtime by Mission-0938-a's `resolve_api_key()` which checks `ANY_LLM_KEY` first. This ensures correct precedence: config_key > os.environ["KEY"] > ANY_LLM_KEY > {PROVIDER}_API_KEY.

### resolve_api_base()

- [x] Tier 1: Explicit non-empty value (existing: api_base or base_url alias)
- [x] Tier 2: `os.environ["KEY"]` syntax
- [x] Tier 3: `{PROVIDER}_API_BASE` env var
- [x] Tier 4: Provider-specific default from RFC-0930 registry (requires Mission-0930-a)
- [x] Return `None` if all tiers fail

### Tests

- [x] `os.environ["MY_KEY"]` resolves correctly
- [x] `os.environ['MY_KEY']` resolves correctly
- [x] Missing env var returns None
- [x] Empty `os.environ[""]` returns None
- [x] Provider env var fallback works

## Key Files

- `crates/quota-router-core/src/config.rs` — LiteLLMParams, resolve_api_key(), resolve_api_base()

## Notes

The `extract_os_environ_key()` helper function needs to be implemented to parse the bracket syntax.

**resolve_api_key():** Does NOT exist on `LiteLLMParams`. There is a standalone `resolve_api_key()` in `proxy.rs` that takes `(&Provider, Option<&str>)`. This mission should add `resolve_api_key()` as a method on `LiteLLMParams` with the 2-tier resolution (explicit value, then os.environ syntax).

**resolve_api_base():** Already exists on `LiteLLMParams` in `config.rs` but only checks `api_base` then `base_url` (returns `Option<&str>`). This mission must extend it to 4 tiers. **Breaking change:** The 4-tier implementation returns `Option<String>` (owned) because env var lookup and provider registry lookup produce owned Strings.

**Callers that must be updated (breaking change):**
1. `to_provider_map()` in `config.rs` — calls `deployment.litellm_params.resolve_api_base()` to get api_base for DispatchInfo. Currently uses `.as_deref()` on the result; must change to handle `Option<String>`.
2. Any test calling `resolve_api_base()` directly — must update return type assertions from `Option<&str>` to `Option<String>`.
3. `proxy.rs` — does NOT call `resolve_api_base()` directly (gets api_base from DispatchInfo), so no change needed.

**Migration:** Replace `.as_deref()` call in `to_provider_map()` with direct `Option<String>` usage. The owned String can be moved into `DispatchInfo.api_base` without cloning.
